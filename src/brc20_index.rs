use self::{
    deploy::handle_deploy_operation,
    mint::handle_mint_operation,
    mongo::MongoClient,
    transfer::{handle_transfer_operation, Brc20ActiveTransfer},
    user_balance::UserBalanceEntryType,
    utils::{extract_and_process_witness_data, get_owner_of_vout, get_witness_data_from_raw_tx},
};
use bitcoin::Address;
use bitcoincore_rpc::bitcoincore_rpc_json::{
    GetRawTransactionResult, GetRawTransactionResultVin, GetRawTransactionResultVout,
    GetRawTransactionResultVoutScriptPubKey,
};
use bitcoincore_rpc::{self, Client, RpcApi};
use log::{error, info};
use mongodb::bson::{doc, Document};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, thread::sleep, time::Duration};

mod brc20_ticker;
pub mod consts;
mod deploy;
mod invalid_brc20;
mod mint;
pub mod mongo;
mod transfer;
mod user_balance;
mod utils;

pub async fn index_brc20(
    rpc: &Client,
    mongo_client: &MongoClient,
    start_block_height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut current_block_height = start_block_height;

    loop {
        match rpc.get_block_hash(current_block_height.into()) {
            Ok(current_block_hash) => {
                match rpc.get_block(&current_block_hash) {
                    Ok(block) => {
                        let length = block.txdata.len();
                        info!(
                            "Fetched block: {:?}, Transactions: {:?}, Block: {:?}",
                            current_block_hash, length, current_block_height
                        );

                        let mut active_transfers_opt = mongo_client.load_active_transfers().await?;

                        // If active_transfers_opt is None, initialize it with a new HashMap
                        if active_transfers_opt.is_none() {
                            active_transfers_opt = Some(HashMap::new());
                        }

                        let mut tx_height = 0u32;
                        for transaction in block.txdata {
                            let txid = transaction.txid();
                            // Get Raw Transaction Info
                            let raw_tx = match rpc.get_raw_transaction_info(&txid, None) {
                                Ok(tx) => tx,
                                Err(e) => {
                                    error!("Failed to get raw transaction info: {:?}", e);
                                    continue; // This will skip the current iteration of the loop
                                }
                            };

                            // Get witness data from raw transaction
                            let witness_data = match get_witness_data_from_raw_tx(&raw_tx) {
                                Ok(data) => data,
                                Err(e) => {
                                    error!("Failed to get witness data: {:?}", e);
                                    continue;
                                }
                            };

                            let mut inscription_found = false;
                            for witness in witness_data {
                                if let Some(inscription) = extract_and_process_witness_data(witness)
                                {
                                    // log raw brc20 data
                                    let pretty_json =
                                        serde_json::to_string(&inscription).unwrap_or_default();
                                    info!("Raw Brc-20 data: {}", pretty_json);

                                    // get owner address, inscription is first satoshi of first output
                                    let owner = match get_owner_of_vout(&raw_tx, 0) {
                                        Ok(owner) => owner,
                                        Err(e) => {
                                            error!("Failed to get owner: {:?}", e);
                                            continue;
                                        }
                                    };

                                    match &inscription.op[..] {
                                        "deploy" => {
                                            match handle_deploy_operation(
                                                mongo_client,
                                                inscription,
                                                &raw_tx,
                                                owner,
                                                current_block_height,
                                                tx_height,
                                            )
                                            .await
                                            {
                                                Ok(found) => inscription_found = found,
                                                Err(e) => {
                                                    error!(
                                                        "Error handling deploy operation: {:?}",
                                                        e
                                                    );
                                                }
                                            };
                                        }
                                        "mint" => {
                                            match handle_mint_operation(
                                                mongo_client,
                                                current_block_height,
                                                tx_height,
                                                owner,
                                                inscription,
                                                &raw_tx,
                                            )
                                            .await
                                            {
                                                Ok(found) => inscription_found = found,
                                                Err(e) => {
                                                    error!(
                                                        "Error handling mint operation: {:?}",
                                                        e
                                                    );
                                                }
                                            };
                                        }
                                        "transfer" => {
                                            match handle_transfer_operation(
                                                mongo_client,
                                                current_block_height,
                                                tx_height,
                                                inscription,
                                                &raw_tx,
                                                owner,
                                                &mut active_transfers_opt,
                                            )
                                            .await
                                            {
                                                Ok(found) => inscription_found = found,
                                                Err(e) => {
                                                    error!(
                                                        "Error handling transfer inscription: {:?}",
                                                        e
                                                    );
                                                }
                                            };
                                        }
                                        _ => {
                                            // Unexpected operation
                                            error!("Unexpected operation: {}", inscription.op);
                                        }
                                    }
                                }
                            }

                            // if no inscription found, check for transfer send
                            if !inscription_found {
                                if active_transfers_opt.is_none() {
                                    active_transfers_opt = Some(HashMap::new());
                                }
                                if let Some(ref mut active_transfers) = &mut active_transfers_opt {
                                    match check_for_transfer_send(
                                        mongo_client,
                                        &rpc,
                                        &raw_tx,
                                        current_block_height.into(),
                                        active_transfers,
                                    )
                                    .await
                                    {
                                        Ok(_) => (),
                                        Err(e) => {
                                            error!("Error checking for transfer send: {:?}", e);
                                        }
                                    };
                                }
                            }

                            // Increment the tx height
                            tx_height += 1;
                        }

                        // drop mongodb collection right before inserting active transfers
                        mongo_client
                            .drop_collection(consts::COLLECTION_BRC20_ACTIVE_TRANSFERS)
                            .await?;

                        // store active transfer collection
                        if let Some(active_transfers) = active_transfers_opt {
                            // if there are any active transfers, store them
                            if !active_transfers.is_empty() {
                                mongo_client
                                    .insert_active_transfers_to_mongodb(active_transfers)
                                    .await?;
                            }
                        }

                        // After successfully processing the block, store the current_block_height
                        match mongo_client
                            .store_completed_block(current_block_height.into())
                            .await
                        {
                            Ok(_) => (),
                            Err(e) => {
                                error!("Failed to store last processed block height: {:?}", e);
                            }
                        }

                        // Increment the block height
                        current_block_height += 1;
                    }
                    Err(e) => {
                        error!("Failed to fetch block: {:?}", e);
                        sleep(Duration::from_secs(60));
                    }
                }
            }
            Err(e) => {
                error!("Failed to fetch block hash for height: {:?}", e);
                sleep(Duration::from_secs(60));
            }
        }
    }
}

pub async fn check_for_transfer_send(
    mongo_client: &MongoClient,
    rpc: &Client,
    raw_tx_info: &GetRawTransactionResult,
    block_height: u64,
    active_transfers: &mut HashMap<(String, i64), Brc20ActiveTransfer>,
) -> Result<(), anyhow::Error> {
    let transaction = raw_tx_info.transaction()?;

    for (input_index, input) in transaction.input.iter().enumerate() {
        let txid = input.previous_output.txid.to_string();
        let vout = input.previous_output.vout as i64;
        let key = (txid.clone(), vout);

        // Check if active transfer exists in the HashMap
        if active_transfers.contains_key(&key) {
            active_transfers.remove(&key);
        } else {
            continue;
        }

        // get mongo doc for transfers collection that matches the txid and vout
        let filter_doc = doc! {"tx.txid": txid.to_string() };
        let transfer_doc = match mongo_client
            .get_document_by_filter(consts::COLLECTION_TRANSFERS, filter_doc)
            .await?
        {
            Some(doc) => doc,
            None => {
                log::error!(
                    "Transfer inscription not found for txid: {}, vout: {}",
                    txid,
                    vout
                );
                continue;
            }
        };

        let mut tick = String::new();
        if let Some(inscription) = transfer_doc.get_document("inscription").ok() {
            if let Some(tck) = inscription.get_str("tick").ok() {
                tick = tck.to_string();
            } else {
                error!("Failed to get 'tick' field from 'inscription'");
            }
        } else {
            error!("Failed to get 'inscription' document");
        }

        let from = mongo_client.get_string(&transfer_doc, "from")?;
        let amount = match mongo_client.get_f64(&transfer_doc, "amt") {
            Some(amt) => amt,
            None => 0.0,
        };

        let proper_vout = if input_index > 0 {
            // if not in first input, get values of all inputs only up to this input
            let input_values =
                utils::transaction_inputs_to_values(rpc, &transaction.input[0..input_index])?;

            // then get the sum these input values
            let input_value_sum: u64 = input_values.iter().sum();

            // Calculate the index of the output (vout) which is the recipient of the
            // current input by finding the first output whose value is greater than
            // or equal to the sum of all preceding input values. This is based on the
            // assumption that inputs and outputs are processed in order and each input's
            // value goes to the next output that it fully covers.
            transaction
                .output
                .iter()
                .scan(0, |acc, output| {
                    *acc += output.value;
                    Some(*acc)
                })
                .position(|value| value >= input_value_sum)
                .unwrap_or(transaction.output.len() - 1)
        } else {
            0
        };

        let receiver_address = get_owner_of_vout(&raw_tx_info, proper_vout)?;

        // Update user overall balance and available for the from address(sender) in MongoDB
        mongo_client
            .insert_user_balance_entry(
                &from,
                amount,
                &tick,
                block_height,
                UserBalanceEntryType::Send,
            )
            .await?;

        // Update user overall balance and available for the to address(receiver) in MongoDB
        mongo_client
            .insert_user_balance_entry(
                &receiver_address.to_string(),
                amount,
                &tick,
                block_height,
                UserBalanceEntryType::Receive,
            )
            .await?;

        //-------------MONGODB-------------------//
        // Update the transfer document in MongoDB
        update_transfer_document(mongo_client, txid, &receiver_address, &raw_tx_info).await?;

        // Update user available and transferable balance for the sender in MongoDB
        mongo_client
            .update_sender_user_balance_document(&from, amount, &tick)
            .await?;

        // Update user overall balance for the receiver in MongoDB
        mongo_client
            .update_receiver_balance_document(&receiver_address.to_string(), amount, &tick)
            .await?;
    }

    Ok(())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Brc20Inscription {
    pub p: String,
    pub op: String,
    pub tick: String,
    pub amt: Option<String>,
    pub max: Option<String>,
    pub lim: Option<String>,
    pub dec: Option<String>,
}

trait ToDocument {
    fn to_document(&self) -> Document;
}

impl ToDocument for Brc20Inscription {
    fn to_document(&self) -> Document {
        doc! {
            "p": &self.p,
            "op": &self.op,
            "tick": &self.tick.to_lowercase(),
            "amt": &self.amt,
            "max": &self.max,
            "lim": &self.lim,
            "dec": &self.dec,
        }
    }
}

//implement Display for Brc20Inscription
impl std::fmt::Display for Brc20Inscription {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "p: {}, op: {}, tick: {}, amt: {:?}, max: {:?}, lim: {:?}, dec: {:?}",
            self.p, self.op, self.tick, self.amt, self.max, self.lim, self.dec
        )
    }
}

impl ToDocument for GetRawTransactionResult {
    fn to_document(&self) -> Document {
        doc! {
            "hex": hex::encode(&self.hex),
            "txid": self.txid.to_string(),
            "hash": self.hash.to_string(),
            "size": self.size as i64, // Convert to i64
            "vsize": self.vsize as i64, // Convert to i64
            "version": self.version,
            "locktime": self.locktime,
            "vin": self.vin.iter().map(|vin| vin.to_document()).collect::<Vec<Document>>(),
            "vout": self.vout.iter().map(|vout| vout.to_document()).collect::<Vec<Document>>(),
            "blockhash": self.blockhash.map(|blockhash| blockhash.to_string()),
            "confirmations": self.confirmations,
            "time": self.time.map(|time| time as i64), // Convert to i64
            "blocktime": self.blocktime.map(|blocktime| blocktime as i64), // Convert to i64
        }
    }
}

impl ToDocument for GetRawTransactionResultVin {
    fn to_document(&self) -> Document {
        doc! {
            "sequence": self.sequence as i64,
            "coinbase": self.coinbase.as_ref().map(|c| hex::encode(c)),
            "txid": self.txid.map(|txid| txid.to_string()),
            "vout": self.vout.map(|vout| vout as i64),
            "script_sig": self.script_sig.as_ref().map(|script_sig| {
                doc! {
                    "asm": &script_sig.asm,
                    "hex": hex::encode(&script_sig.hex),
                }
            }),
            "txinwitness": self.txinwitness.as_ref().map(|witness| {
                witness.iter().map(|w| hex::encode(w)).collect::<Vec<_>>()
            }),
        }
    }
}

impl ToDocument for GetRawTransactionResultVoutScriptPubKey {
    fn to_document(&self) -> Document {
        doc! {
            "asm": &self.asm,
            "hex": hex::encode(&self.hex),
            "req_sigs": self.req_sigs.map(|req_sigs| req_sigs as i64),
            "type": self.type_.as_ref().map(|type_| format!("{:?}", type_)),
            "addresses": self.addresses.iter().map(|address| address.clone().assume_checked().to_string()).collect::<Vec<_>>(),
            "address": self.address.clone().map(|address| address.assume_checked().to_string()),
        }
    }
}

impl ToDocument for GetRawTransactionResultVout {
    fn to_document(&self) -> Document {
        doc! {
            "value": self.value.to_btc(),
            "n": self.n as i64,
            "script_pub_key": self.script_pub_key.to_document(),
        }
    }
}

// This function will update the transfer document in MongoDB with receiver and send_tx
pub async fn update_transfer_document(
    mongo_client: &MongoClient,
    tx_id: String,
    receiver_address: &Address,
    send_tx: &GetRawTransactionResult,
) -> Result<(), anyhow::Error> {
    let update_doc = doc! {
        "$set": {
            "to": receiver_address.to_string(),
            "send_tx": send_tx.to_document(),
        }
    };

    // Update the document in MongoDB
    mongo_client
        .update_document_by_field(
            consts::COLLECTION_TRANSFERS,
            "tx.txid",
            &tx_id.to_string(),
            update_doc,
        )
        .await?;
    Ok(())
}
