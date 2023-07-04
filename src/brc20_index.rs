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
use std::{
    collections::HashMap,
    thread::sleep,
    time::{Duration, Instant},
};

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

                        let mut active_transfers_opt =
                            mongo_client.load_active_transfers_with_retry().await?;

                        // If active_transfers_opt is None, initialize it with a new HashMap
                        if active_transfers_opt.is_none() {
                            active_transfers_opt = Some(HashMap::new());
                        }

                        // get all user balances from mongo
                        let mut user_balance_docs =
                            match mongo_client.load_user_balances_with_retry().await {
                                Ok(docs) => match docs {
                                    Some(docs) => docs,
                                    None => Vec::new(),
                                },
                                Err(e) => {
                                    error!("Failed to load user balances: {:?}", e);
                                    Vec::new()
                                }
                            };

                        // Vectors for mongo bulk writes
                        let mut mint_documents = Vec::new();
                        let mut transfer_documents = Vec::new();
                        let mut deploy_documents = Vec::new();
                        let mut invalid_brc20_documents = Vec::new();
                        let mut user_balance_entry_documents = Vec::new();
                        let mut tickers: HashMap<String, Document> = HashMap::new();

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
                                    log::debug!("Raw Brc-20 data: {}", pretty_json);

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
                                                &mut invalid_brc20_documents,
                                            )
                                            .await
                                            {
                                                Ok(deploy) => {
                                                    inscription_found = deploy.is_valid();
                                                    if inscription_found {
                                                        deploy_documents.push(deploy.to_document());
                                                    }
                                                }
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
                                                &mut tickers,
                                                &mut user_balance_docs,
                                                &mut invalid_brc20_documents,
                                            )
                                            .await
                                            {
                                                Ok((mint, user_balance_entry)) => {
                                                    inscription_found = mint.is_valid();
                                                    if inscription_found {
                                                        mint_documents.push(mint.to_document());
                                                        user_balance_entry_documents
                                                            .push(user_balance_entry.to_document());
                                                    }
                                                }
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
                                                &mut user_balance_docs,
                                                &mut invalid_brc20_documents,
                                            )
                                            .await
                                            {
                                                Ok((transfer, user_balance_entry)) => {
                                                    inscription_found = transfer.is_valid();
                                                    if inscription_found {
                                                        transfer_documents
                                                            .push(transfer.to_document());

                                                        user_balance_entry_documents
                                                            .push(user_balance_entry.to_document());
                                                    }
                                                }
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
                                        tx_height.into(),
                                        active_transfers,
                                        &mut transfer_documents,
                                        &mut user_balance_entry_documents,
                                        &mut user_balance_docs,
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

                        // Bulk write the updated and new user balance documents back to MongoDB
                        if !user_balance_docs.is_empty() {
                            let start = Instant::now(); // time the process

                            // drop mongodb collection right before inserting active transfers
                            mongo_client
                                .drop_collection(consts::COLLECTION_USER_BALANCES)
                                .await?;

                            log::warn!("User Balances Deleted after block: {:?}", start.elapsed());

                            let start = Instant::now();
                            // Bulk write user balance documents to mongodb
                            match mongo_client
                                .insert_many_with_retries(
                                    consts::COLLECTION_USER_BALANCES,
                                    user_balance_docs.clone(),
                                )
                                .await
                            {
                                Ok(_) => {
                                    info!(
                                        "User Balances inserted after block: {:?}",
                                        start.elapsed()
                                    )
                                }
                                Err(e) => {
                                    error!("Failed to update user balance documents: {:?}", e)
                                }
                            }
                        }

                        // Bulk write mints to mongodb
                        if !mint_documents.is_empty() {
                            let start = Instant::now();
                            mongo_client
                                .insert_many_with_retries(consts::COLLECTION_MINTS, mint_documents)
                                .await?;
                            log::warn!("Mints inserted after block: {:?}", start.elapsed());
                        }

                        // Bulk write transfers to mongodb
                        if !transfer_documents.is_empty() {
                            let start = Instant::now();
                            mongo_client
                                .insert_many_with_retries(
                                    consts::COLLECTION_TRANSFERS,
                                    transfer_documents,
                                )
                                .await?;

                            log::warn!("Transfers inserted after block: {:?}", start.elapsed());
                        }

                        // Bulk write deploys to mongodb
                        if !deploy_documents.is_empty() {
                            let start = Instant::now();
                            mongo_client
                                .insert_many_with_retries(
                                    consts::COLLECTION_DEPLOYS,
                                    deploy_documents,
                                )
                                .await?;

                            log::warn!("Deploys inserted after block: {:?}", start.elapsed());
                        }

                        // convert tickers hashmap to vec<Document>
                        let tickers: Vec<Document> =
                            tickers.into_iter().map(|(_, ticker)| ticker).collect();

                        // Bulk update tickers in mongodb
                        if !tickers.is_empty() {
                            let start = Instant::now();
                            for ticker in tickers {
                                let filter_doc = doc! {
                                    "tick": ticker.get_str("tick").unwrap_or_default(),
                                };

                                let update_doc = doc! {
                                    "$set": ticker,
                                };

                                mongo_client
                                    .update_many_with_retries(
                                        consts::COLLECTION_TICKERS,
                                        filter_doc,
                                        update_doc,
                                    )
                                    .await?;
                            }

                            log::warn!("Tickers updated after block: {:?}", start.elapsed());
                        }

                        // Bulk write user balance entries to mongodb
                        if !user_balance_entry_documents.is_empty() {
                            let start = Instant::now();
                            mongo_client
                                .insert_many_with_retries(
                                    consts::COLLECTION_USER_BALANCE_ENTRY,
                                    user_balance_entry_documents,
                                )
                                .await?;

                            log::warn!(
                                "User Balance Entries inserted after block: {:?}",
                                start.elapsed()
                            );
                        }

                        // Bulk write invalid brc20 documents to mongodb
                        if !invalid_brc20_documents.is_empty() {
                            let start = Instant::now();
                            mongo_client
                                .insert_many_with_retries(
                                    consts::COLLECTION_INVALIDS,
                                    invalid_brc20_documents,
                                )
                                .await?;

                            log::warn!("Invalids inserted after block: {:?}", start.elapsed());
                        }

                        let start = Instant::now();

                        // drop mongodb collection right before inserting active transfers
                        mongo_client
                            .drop_collection(consts::COLLECTION_BRC20_ACTIVE_TRANSFERS)
                            .await?;

                        log::warn!(
                            "Active Transfers Deleted after block: {:?}",
                            start.elapsed()
                        );

                        // store active transfer collection, if any
                        if let Some(active_transfers) = active_transfers_opt {
                            if !active_transfers.is_empty() {
                                let start = Instant::now();
                                mongo_client
                                    .insert_active_transfers_to_mongodb(active_transfers)
                                    .await?;

                                log::warn!(
                                    "Active Transfers inserted after block: {:?}",
                                    start.elapsed()
                                );
                            }
                        }

                        // store into a new collection a document that has all of the tickers
                        // and their total_minted at this block height
                        let start = Instant::now();
                        mongo_client
                            .insert_tickers_total_minted_and_user_balances_at_block_height(
                                current_block_height.into(),
                                &user_balance_docs,
                            )
                            .await?;

                        log::warn!(
                            "Tickers and User Balances inserted after block: {:?}",
                            start.elapsed()
                        );

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
    tx_height: i64,
    active_transfers: &mut HashMap<(String, i64), Brc20ActiveTransfer>,
    transfer_documents: &mut Vec<Document>,
    user_balance_entry_documents: &mut Vec<Document>,
    user_balance_docs: &mut Vec<Document>,
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

        // Check if transfer exists in the transfer_documents vector in memory
        let transfer_doc_opt = transfer_documents.iter().find(|doc| {
            doc.get_str("tx.txid").ok() == Some(&txid) && doc.get_i64("tx.vout").ok() == Some(vout)
        });

        let transfer_doc = match transfer_doc_opt {
            Some(transfer_doc) => transfer_doc.clone(),
            None => {
                // get mongo doc for transfers collection that matches the txid and vout
                let filter_doc = doc! {"tx.txid": txid.clone()};
                match mongo_client
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
                }
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
            // inscribed satoshi by finding the first output whose value is greater than
            // the sum of all preceding input values. This is based on the
            // ordinal theory that satoshis are processed in order
            transaction
                .output
                .iter()
                .scan(0, |acc, output| {
                    *acc += output.value;
                    Some(*acc)
                })
                .position(|value| value > input_value_sum)
                .unwrap_or(transaction.output.len() - 1)
        } else {
            0
        };

        let receiver_address = get_owner_of_vout(&raw_tx_info, proper_vout)?;

        // Update user overall balance and available for the from address(sender)
        let user_entry_from = mongo_client
            .insert_user_balance_entry(
                &from,
                amount,
                &tick,
                block_height,
                UserBalanceEntryType::Send,
            )
            .await?;

        user_balance_entry_documents.push(user_entry_from.to_document());

        // Update user overall balance and available for the to address(receiver)
        let user_entry_to = mongo_client
            .insert_user_balance_entry(
                &receiver_address.to_string(),
                amount,
                &tick,
                block_height,
                UserBalanceEntryType::Receive,
            )
            .await?;

        user_balance_entry_documents.push(user_entry_to.to_document());

        //-------------MONGODB-------------------//
        // Update the transfer document in MongoDB (check in memory first)
        update_transfer_document(
            mongo_client,
            &txid,
            vout,
            &receiver_address,
            block_height.try_into().unwrap(),
            tx_height,
            &raw_tx_info,
            transfer_documents,
        )
        .await?;

        // Update user available and transferable balance for the sender in MongoDB
        mongo_client
            .update_sender_user_balance_document(&from, amount, &tick, user_balance_docs)
            .await?;

        // Update user overall balance for the receiver in MongoDB
        mongo_client
            .update_receiver_balance_document(
                &receiver_address.to_string(),
                amount,
                &tick,
                user_balance_docs,
            )
            .await?;

        info!(
            "Transfer inscription found for txid: {}, vout: {}",
            txid, vout
        );
        info!(
            "Amount transferred: {}, to: {}",
            amount,
            receiver_address.to_string()
        );
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

pub async fn update_transfer_document(
    mongo_client: &MongoClient,
    tx_id: &str,
    vout: i64,
    receiver_address: &Address,
    send_block_height: i64,
    send_tx_height: i64,
    send_tx: &GetRawTransactionResult,
    transfer_documents: &mut Vec<Document>,
) -> Result<(), anyhow::Error> {
    // Check if the transfer exists in the transfer_documents vector
    if let Some(transfer_doc) = transfer_documents.iter_mut().find(|doc| {
        doc.get_str("tx.txid").ok() == Some(tx_id) && doc.get_i64("tx.vout").ok() == Some(vout)
    }) {
        // If it does, update it
        transfer_doc.insert("to", receiver_address.to_string());
        transfer_doc.insert("send_tx", send_tx.to_document());
    } else {
        // If it doesn't exist in the transfer_documents vector, update it in MongoDB
        let update_doc = doc! {
            "$set": {
                "to": receiver_address.to_string(),
                "send_tx": send_tx.to_document(),
                "send_block_height": send_block_height,
                "send_tx_height": send_tx_height,
            }
        };

        // Update the document in MongoDB
        mongo_client
            .update_document_by_field(consts::COLLECTION_TRANSFERS, "tx.txid", tx_id, update_doc)
            .await?;
    }

    Ok(())
}
