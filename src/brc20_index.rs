use crate::brc20_index::{
    user_balance::update_user_balances,
    utils::{update_receiver_balance_document, update_sender_user_balance_document},
};

use self::{
    deploy::handle_deploy_operation,
    mint::handle_mint_operation,
    mongo::MongoClient,
    transfer::{handle_transfer_operation, Brc20ActiveTransfer},
    user_balance::UserBalanceEntryType,
    utils::{extract_and_process_witness_data, get_owner_of_vout, get_witness_data_from_raw_tx},
};
use bitcoincore_rpc::bitcoincore_rpc_json::{
    GetRawTransactionResult, GetRawTransactionResultVin, GetRawTransactionResultVout,
    GetRawTransactionResultVoutScriptPubKey,
};
use bitcoincore_rpc::{self, Client, RpcApi};
use log::{debug, error, info, warn};
use mongodb::{
    bson::{doc, Document},
    options::UpdateOptions,
};
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

                        let start = Instant::now();
                        let mut active_transfers_opt =
                            mongo_client.load_active_transfers_with_retry().await?;

                        // If active_transfers_opt is None, initialize it with a new HashMap
                        if active_transfers_opt.is_none() {
                            active_transfers_opt = Some(HashMap::new());
                        }
                        warn!("Active Transfers loaded: {:?}", start.elapsed());

                        // Vectors for mongo bulk writes
                        let mut mint_documents = Vec::new();
                        let mut transfer_documents = Vec::new();
                        let mut deploy_documents = Vec::new();
                        let mut invalid_brc20_documents = Vec::new();
                        let mut user_balance_entry_documents = Vec::new();
                        let mut tickers: HashMap<String, Document> = HashMap::new();
                        let mut user_balance_docs: HashMap<(String, String), Document> =
                            HashMap::new();
                        let mut user_balance_docs_to_insert: HashMap<(String, String), Document> =
                            HashMap::new();

                        // time to process the block
                        let process_block_start_time = Instant::now();

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

                                                        // Update user balance docs
                                                        match update_receiver_balance_document(
                                                            mongo_client,
                                                            &mut user_balance_docs,
                                                            &mut user_balance_docs_to_insert,
                                                            &user_balance_entry,
                                                        )
                                                        .await
                                                        {
                                                            Ok(_) => {}
                                                            Err(e) => {
                                                                error!(
                                                                    "Error updating user balance docs: {:?}",
                                                                    e
                                                                );
                                                            }
                                                        }
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
                                                &mut user_balance_docs_to_insert,
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
                                        &mut user_balance_docs_to_insert,
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

                        // time to process the block
                        warn!(
                            "Transactions Processed: {} in {:?}",
                            tx_height,
                            process_block_start_time.elapsed()
                        );

                        // write the updated and new user balance documents back to MongoDB
                        if !user_balance_docs.is_empty() {
                            let start = Instant::now();
                            let start_len = user_balance_docs.len();
                            // This removes all UserBalance with 0 in all the balance fields.
                            user_balance_docs.retain(|_, user_balance_doc| {
                                let overall_balance = user_balance_doc
                                    .get_f64("overall_balance")
                                    .unwrap_or_default();
                                let available_balance = user_balance_doc
                                    .get_f64("available_balance")
                                    .unwrap_or_default();
                                let transferable_balance = user_balance_doc
                                    .get_f64("transferable_balance")
                                    .unwrap_or_default();

                                overall_balance != 0.0
                                    || available_balance != 0.0
                                    || transferable_balance != 0.0
                            });

                            let len = user_balance_docs.len();

                            warn!(
                                "Zeroed User Balances removed: {} in {:?}",
                                start_len - len,
                                start.elapsed()
                            );

                            info!("Inserting User Balances...");
                            // write user balance documents to mongodb
                            match update_user_balances(
                                mongo_client,
                                user_balance_docs,
                                user_balance_docs_to_insert,
                            )
                            .await
                            {
                                Ok(_) => {}
                                Err(e) => {
                                    error!("Failed to update user balance documents: {:?}", e);
                                }
                            }
                        }

                        insert_documents_to_mongo_after_each_block(
                            mongo_client,
                            mint_documents,
                            transfer_documents,
                            deploy_documents,
                            invalid_brc20_documents,
                            user_balance_entry_documents,
                        )
                        .await?;

                        // Bulk update tickers in mongodb
                        if !tickers.is_empty() {
                            // convert tickers hashmap to vec<Document>
                            let tickers: Vec<Document> =
                                tickers.into_iter().map(|(_, ticker)| ticker).collect();

                            debug!("tickers main loop: {:?}", tickers);

                            let start = Instant::now();
                            for ticker in &tickers {
                                let filter_doc = doc! {
                                    "tick": ticker.get_str("tick").unwrap_or_default(),
                                };

                                let update_doc = doc! {
                                    "$set": ticker,
                                };

                                mongo_client
                                    .update_one_with_retries(
                                        consts::COLLECTION_TICKERS,
                                        filter_doc,
                                        update_doc,
                                        None,
                                    )
                                    .await?;
                            }

                            warn!(
                                "Tickers updated after block: {} in {:?}",
                                tickers.len(),
                                start.elapsed()
                            );
                        }

                        // drop mongodb collection right before inserting active transfers
                        mongo_client
                            .drop_collection(consts::COLLECTION_BRC20_ACTIVE_TRANSFERS)
                            .await?;

                        // store active transfer collection, if any
                        if let Some(active_transfers) = active_transfers_opt {
                            let length = active_transfers.len();
                            if !active_transfers.is_empty() {
                                let start = Instant::now();
                                mongo_client
                                    .insert_active_transfers_to_mongodb(active_transfers)
                                    .await?;

                                info!(
                                    "Active Transfers inserted to MongoDB after block: {} in {:?}",
                                    length,
                                    start.elapsed()
                                );
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
                        error!("Failed to fetch block: {:?}, retrying...", e);
                        sleep(Duration::from_secs(60));
                    }
                }
            }
            Err(e) => {
                error!("Failed to fetch block hash for height: {:?}, retrying", e);
                sleep(Duration::from_secs(60));
            }
        }
    }
}

/// Checks for transfer send events in a transaction and performs the necessary updates in MongoDB.
///
/// This function checks if there are transfer send events in the given transaction and performs the following actions:
/// - Updates user balances and entries for the sender and receiver.
/// - Updates the transfer document in MongoDB, either by updating an existing document or inserting a new one.
/// - Updates user available and transferable balances for the sender in MongoDB.
/// - Updates user overall balance for the receiver in MongoDB.
///
/// # Arguments
///
/// * `mongo_client` - The MongoDB client for performing database operations.
/// * `rpc` - The RPC client for interacting with the blockchain.
/// * `raw_tx_info` - The raw transaction information.
/// * `block_height` - The block height of the transaction.
/// * `tx_height` - The transaction height.
/// * `active_transfers` - A hashmap containing active transfers.
/// * `transfer_documents` - A vector of transfer documents.
/// * `user_balance_entry_documents` - A vector of user balance entry documents.
/// * `user_balances` - A hashmap containing user balances.
///
/// # Returns
///
/// This function returns `Ok(())` if the operation is successful, or an error if any error occurs during the process.
pub async fn check_for_transfer_send(
    mongo_client: &MongoClient,
    rpc: &Client,
    raw_tx_info: &GetRawTransactionResult,
    block_height: u64,
    tx_height: i64,
    active_transfers: &mut HashMap<(String, i64), Brc20ActiveTransfer>,
    transfer_documents: &mut Vec<Document>,
    user_balance_entry_documents: &mut Vec<Document>,
    user_balances: &mut HashMap<(String, String), Document>,
    user_balances_to_insert: &mut HashMap<(String, String), Document>,
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
        info!("Transfer Send Found: {:?}", key);
        // Check if transfer exists in the transfer_documents vector in memory
        let index = transfer_documents.iter().position(|doc| {
            if let Ok(tx) = doc.get_document("tx") {
                if let Ok(txid) = tx.get_str("txid") {
                    return txid == txid;
                }
            }
            false
        });

        let transfer_doc = if let Some(index) = index {
            // Document found in the vector, remove it from the vector
            transfer_documents.remove(index)
        } else {
            info!("Checking in MongoDB: {:?}", key);
            // Document not found in the vector, fetch it from MongoDB
            let filter_doc = doc! {"tx.txid": txid.clone()};
            match mongo_client
                .get_document_by_filter(consts::COLLECTION_TRANSFERS, filter_doc)
                .await?
            {
                Some(doc) => doc,
                None => {
                    error!(
                        "Transfer inscription not found for txid: {}, vout: {}",
                        txid, vout
                    );
                    continue;
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
            let total_output_value: u64 =
                transaction.output.iter().map(|output| output.value).sum();

            // If the sum of input values (up to the current input index) is greater than the total output value,
            // assume that the sender is the receiver.
            if input_value_sum >= total_output_value {
                std::usize::MAX // use MAX as a sentinel value
            } else {
                // Calculate the index of the output (vout) which is the recipient of the
                // inscribed satoshi by finding the first output whose value is greater than
                // the sum of all preceding input values. This is based on the ordinal theory that satoshis are processed in order.
                transaction
                    .output
                    .iter()
                    .scan(0, |acc, output| {
                        *acc += output.value;
                        Some(*acc)
                    })
                    .position(|value| value > input_value_sum)
                    .unwrap_or(transaction.output.len() - 1)
            }
        } else {
            0
        };

        let receiver_address = if proper_vout == std::usize::MAX {
            error!("Transfer sent as Miner Fee. Balance sent back to sender.");
            from.clone() // If sentinel value is present, use sender's address as receiver's address
        } else {
            get_owner_of_vout(&raw_tx_info, proper_vout)?.to_string()
        };

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
                &receiver_address,
                amount,
                &tick,
                block_height,
                UserBalanceEntryType::Receive,
            )
            .await?;

        user_balance_entry_documents.push(user_entry_to.to_document());

        //-------------MONGODB-------------------//
        // Pass the transfer document to the update_transfer_document function
        update_transfer_document(
            mongo_client,
            transfer_doc,
            &txid,
            &receiver_address,
            block_height.try_into().unwrap(),
            tx_height,
            &raw_tx_info,
        )
        .await?;

        // Update user available and transferable balance for the sender in MongoDB
        update_sender_user_balance_document(
            mongo_client,
            user_balances,
            user_balances_to_insert,
            &user_entry_from,
        )
        .await?;

        // Update user overall balance for the receiver in MongoDB
        update_receiver_balance_document(
            mongo_client,
            user_balances,
            user_balances_to_insert,
            &user_entry_to,
        )
        .await?;

        info!(
            "Transfer inscription found for txid: {}, vout: {}",
            txid, vout
        );
        info!("Amount transferred: {}, to: {}", amount, receiver_address);
    }

    Ok(())
}

/// Inserts different types of documents to MongoDB in their respective collections.
///
/// # Arguments
///
/// * `mongo_client` - A reference to the MongoDB client used to interact with the database.
/// * `mint_documents` - A vector of mint documents to be inserted into MongoDB.
/// * `transfer_documents` - A vector of transfer documents to be inserted into MongoDB.
/// * `deploy_documents` - A vector of deploy documents to be inserted into MongoDB.
/// * `invalid_brc20_documents` - A vector of invalid BRC20 documents to be inserted into MongoDB.
/// * `user_balance_entry_documents` - A vector of user balance entry documents to be inserted into MongoDB.
///
/// # Errors
///
/// This function will return an error if the insertion of any type of documents fails.
pub async fn insert_documents_to_mongo_after_each_block(
    mongo_client: &MongoClient,
    mint_documents: Vec<Document>,
    transfer_documents: Vec<Document>,
    deploy_documents: Vec<Document>,
    invalid_brc20_documents: Vec<Document>,
    user_balance_entry_documents: Vec<Document>,
) -> Result<(), Box<dyn std::error::Error>> {
    // If there are mint documents, insert them into the mints collection
    if !mint_documents.is_empty() {
        let start = Instant::now();
        mongo_client
            .insert_many_with_retries(consts::COLLECTION_MINTS, &mint_documents)
            .await?;
        warn!(
            "Mints inserted after block: {} in {:?}",
            mint_documents.len(),
            start.elapsed()
        );
    }

    // If there are transfer documents, insert them into the transfers collection
    if !transfer_documents.is_empty() {
        let start = Instant::now();
        mongo_client
            .insert_many_with_retries(consts::COLLECTION_TRANSFERS, &transfer_documents)
            .await?;
        warn!(
            "Transfers inserted after block: {} in {:?}",
            transfer_documents.len(),
            start.elapsed()
        );
    }

    // If there are deploy documents, insert them into the deploys collection
    if !deploy_documents.is_empty() {
        let start = Instant::now();
        mongo_client
            .insert_many_with_retries(consts::COLLECTION_DEPLOYS, &deploy_documents)
            .await?;
        warn!(
            "Deploys inserted after block: {} in {:?}",
            deploy_documents.len(),
            start.elapsed()
        );
    }

    // If there are invalid BRC20 documents, insert them into the invalids collection
    if !invalid_brc20_documents.is_empty() {
        let start = Instant::now();
        mongo_client
            .insert_many_with_retries(consts::COLLECTION_INVALIDS, &invalid_brc20_documents)
            .await?;
        warn!(
            "Invalids inserted after block: {} in {:?}",
            invalid_brc20_documents.len(),
            start.elapsed()
        );
    }

    // If there are user balance entry documents, insert them into the user balance entry collection
    if !user_balance_entry_documents.is_empty() {
        let start = Instant::now();
        mongo_client
            .insert_many_with_retries(
                consts::COLLECTION_USER_BALANCE_ENTRY,
                &user_balance_entry_documents,
            )
            .await?;
        warn!(
            "User Balance Entries inserted after block: {} in {:?}",
            user_balance_entry_documents.len(),
            start.elapsed()
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
    transfer_doc: Document,
    tx_id: &str,
    receiver_address: &str,
    send_block_height: i64,
    send_tx_height: i64,
    send_tx: &GetRawTransactionResult,
) -> Result<(), anyhow::Error> {
    // Update the fields of the document
    let updated_doc = {
        let mut updated_doc = transfer_doc;
        updated_doc.insert("to", receiver_address);
        updated_doc.insert("send_tx", send_tx.to_document());
        updated_doc.insert("send_block_height", send_block_height);
        updated_doc.insert("send_tx_height", send_tx_height);
        updated_doc
    };

    // Update or insert the document in MongoDB
    // We can save to MongoDB without worrying about needing to
    // delete in case of restart, they will just be overwritten by the new ones
    // and will not affect any balances that need to be recalculated
    let filter = doc! { "tx.txid": tx_id };
    let update_doc = doc! { "$set": updated_doc };
    let options = UpdateOptions::builder().upsert(true).build();
    mongo_client
        .update_one_with_retries(
            consts::COLLECTION_TRANSFERS,
            filter,
            update_doc,
            Some(options),
        )
        .await?;

    Ok(())
}
