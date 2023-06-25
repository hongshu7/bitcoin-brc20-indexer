use self::{
    brc20_ticker::Brc20Ticker,
    deploy::handle_deploy_operation,
    invalid_brc20::InvalidBrc20TxMap,
    mint::handle_mint_operation,
    mongo::MongoClient,
    transfer::{handle_transfer_operation, Brc20Transfer},
    utils::{extract_and_process_witness_data, get_owner_of_vout, get_witness_data_from_raw_tx},
};
use bitcoin::{Address, OutPoint};
use bitcoincore_rpc::bitcoincore_rpc_json::{
    GetRawTransactionResult, GetRawTransactionResultVin, GetRawTransactionResultVout,
    GetRawTransactionResultVoutScriptPubKey,
};
use bitcoincore_rpc::{self, Client, RpcApi};
use consulrs::{
    client::{ConsulClient, ConsulClientSettingsBuilder},
    kv,
};
use log::{error, info};
use mongodb::bson::{doc, Document};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, env, thread::sleep, time::Duration};

mod brc20_ticker;
pub mod consts;
mod deploy;
mod invalid_brc20;
mod mint;
mod mongo;
mod transfer;
pub mod user_balance;
mod utils;

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

// Brc20Index is a struct that represents
// all Brc 20 Tickers and invalid Brc 20 Txs.
#[derive(Debug)]
pub struct Brc20Index {
    // The BRC-20 tickers.
    pub tickers: HashMap<String, Brc20Ticker>,
    // The invalid BRC-20 transactions.
    pub invalid_tx_map: InvalidBrc20TxMap,
    // The active BRC-20 transfer inscriptions.
    pub active_transfer_inscriptions: HashMap<OutPoint, String>,
}

impl Brc20Index {
    pub fn new() -> Self {
        Brc20Index {
            tickers: HashMap::new(),
            invalid_tx_map: InvalidBrc20TxMap::new(),
            active_transfer_inscriptions: HashMap::new(),
        }
    }

    pub async fn check_for_transfer_send(
        &mut self,
        mongo_client: &MongoClient,
        rpc: &Client,
        raw_tx_info: &GetRawTransactionResult,
    ) -> Result<(), anyhow::Error> {
        let transaction = raw_tx_info.transaction()?;

        for (input_index, input) in transaction.input.iter().enumerate() {
            let outpoint = input.previous_output;
            let ticker = match self.get_active_transfer_inscription_ticker(&outpoint) {
                Some(ticker) => ticker,
                None => {
                    log::debug!(
                        "No active transfer inscription for outpoint: {:?}",
                        outpoint
                    );
                    continue;
                }
            };

            let brc20_ticker = match self.get_ticker_mut(&ticker) {
                Some(brc20_ticker) => brc20_ticker,
                None => {
                    log::error!("Inscription found but ticker {} not found", ticker);
                    continue;
                }
            };

            if !brc20_ticker.has_active_transfer_inscription(&outpoint) {
                log::error!(
                    "No user balance with active transfer inscription for outpoint: {:?}",
                    outpoint
                );
                continue;
            }

            let brc20_transfer_tx =
                match brc20_ticker.get_and_remove_active_transfer_inscription(&outpoint) {
                    Some(brc20_transfer_tx) => brc20_transfer_tx,
                    None => {
                        log::debug!(
                            "Active transfer inscription not found for outpoint: {:?}",
                            outpoint
                        );
                        continue;
                    }
                };

            // If the input is the first input, the proper vout is 0.
            // For other inputs, calculate the proper vout by
            // finding the vout that is greater than the sum of all input values
            // of the inputs leading up to but not including the current one.
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
            let amount = brc20_transfer_tx.get_amount();
            let from = brc20_transfer_tx.from.clone();

            let ticker_symbol = &brc20_ticker.get_ticker().clone();

            // // Fetch existing balances for sender and receiver
            // let sender_balance_doc = mongo_client
            //     .get_user_balance_document(
            //         consts::COLLECTION_USER_BALANCES,
            //         doc! {
            //             "ticker": ticker_symbol,
            //             "address": from.to_string(),
            //         },
            //     )
            //     .await?;

            // let receiver_balance_doc = mongo_client
            //     .get_user_balance_document(
            //         consts::COLLECTION_USER_BALANCES,
            //         doc! {
            //             "ticker": ticker_symbol,
            //             "address": receiver_address.to_string(),
            //         },
            //     )
            //     .await?;

            // update transfer struct with receiver address
            let brc20_transfer_tx = brc20_transfer_tx
                .set_receiver(receiver_address.clone())
                .set_transfer_tx(raw_tx_info.clone());

            // Update user balances
            brc20_ticker
                .update_transfer_receives(receiver_address.clone(), brc20_transfer_tx.clone());
            brc20_ticker.update_transfer_sends(from.clone(), brc20_transfer_tx.clone());

            self.remove_active_transfer_balance(&outpoint);

            let send_tx = match brc20_transfer_tx.send_tx.clone() {
                Some(send_tx) => send_tx,
                None => {
                    log::error!("No send tx found for transfer tx: {:?}", brc20_transfer_tx);
                    continue;
                }
            };

            //-------------MONGODB-------------------//
            // Update the transfer document in MongoDB
            update_transfer_document(
                mongo_client,
                &brc20_transfer_tx,
                &receiver_address,
                &send_tx,
            )
            .await?;

            // Update user available and transferable balance for the sender in MongoDB
            mongo_client
                .update_sender_user_balance_document(&from.to_string(), amount, &ticker_symbol)
                .await?;

            // Update user overall balance for the receiver in MongoDB
            mongo_client
                .update_receiver_balance_document(
                    &receiver_address.to_string(),
                    amount,
                    &ticker_symbol,
                )
                .await?;
        }

        Ok(())
    }

    //Method to remove a ticker for a given outpoint
    pub fn remove_active_transfer_balance(&mut self, outpoint: &OutPoint) {
        self.active_transfer_inscriptions.remove(outpoint);
    }

    //method to return a ticker String for a given outpoint
    pub fn get_active_transfer_inscription_ticker(&self, outpoint: &OutPoint) -> Option<String> {
        self.active_transfer_inscriptions.get(outpoint).cloned()
    }

    // Method to add a ticker for a given txid and vout
    pub fn update_active_transfer_inscription(&mut self, outpoint: OutPoint, ticker: String) {
        self.active_transfer_inscriptions.insert(outpoint, ticker);
    }

    //get a mutable ticker struct for a given ticker string
    pub fn get_ticker_mut(&mut self, ticker: &str) -> Option<&mut Brc20Ticker> {
        self.tickers.get_mut(ticker)
    }
}

pub async fn index_brc20(
    rpc: &Client,
    start_block_height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    //TODO: This is connecting to Consul to get the MONGO_HOST, this should be refactorered to get ALL
    // the variables we need from consul in one place in the codebase and set CONSTANT variables for these.
    let consul_host = env::var("CONSUL_HOST").unwrap();
    let client = ConsulClient::new(
        ConsulClientSettingsBuilder::default()
            .address(consul_host)
            .build()
            .unwrap(),
    )
    .unwrap();
    let mut res = kv::read(&client, "omnisat-api", None).await.unwrap();
    let mykey: String = res
        .response
        .pop()
        .unwrap()
        .value
        .unwrap()
        .try_into()
        .unwrap();
    let json_value: Value = serde_json::from_str(&mykey).unwrap();

    let mongo_host = json_value
        .get("mongo_rc")
        .unwrap_or_else(|| panic!("MONGO_RC IS NOT SET"));

    let mongo_connection_str = format!(
        "mongodb://{}:27017,{}:27017,{}:27017/omnisat?replicaSet=rs0",
        mongo_host[0].as_str().unwrap(),
        mongo_host[1].as_str().unwrap(), 
        mongo_host[2].as_str().unwrap(),
    );
    // let mongo_connection_str = "mongodb://localhost:27017"; // This uses localhost as mongo host

    // Instantiate a new `Brc20Index` struct.
    let mut brc20_index = Brc20Index::new();

    // Get the mongo database name from environment variable
    let db_name = env::var("MONGO_DB_NAME").unwrap();

    // This will return a future that you can await directly
    let client = MongoClient::new(&mongo_connection_str, &db_name).await?;

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

                        let mut tx_height = 0u32;
                        for transaction in block.txdata {
                            let txid = transaction.txid();
                            // Get Raw Transaction
                            let raw_tx = rpc.get_raw_transaction_info(&txid, None)?;

                            // Get witness data from raw transaction
                            let witness_data = get_witness_data_from_raw_tx(&raw_tx)?;
                            for witness in witness_data {
                                if let Some(inscription) = extract_and_process_witness_data(witness)
                                {
                                    // print pretty json
                                    let pretty_json =
                                        serde_json::to_string(&inscription).unwrap_or_default();
                                    info!("Raw Brc-20 data: {}", pretty_json);

                                    // get owner address, inscription is first satoshi of first output
                                    let owner = get_owner_of_vout(&raw_tx, 0)?;

                                    match &inscription.op[..] {
                                        "deploy" => {
                                            handle_deploy_operation(
                                                &client,
                                                inscription,
                                                raw_tx.clone(),
                                                owner.clone(),
                                                current_block_height,
                                                tx_height,
                                                &mut brc20_index,
                                            )
                                            .await?
                                        }
                                        "mint" => {
                                            handle_mint_operation(
                                                &client,
                                                current_block_height,
                                                tx_height,
                                                owner,
                                                inscription,
                                                &raw_tx,
                                                &mut brc20_index,
                                            )
                                            .await?
                                        }
                                        "transfer" => {
                                            handle_transfer_operation(
                                                &client,
                                                current_block_height,
                                                tx_height,
                                                inscription,
                                                raw_tx.clone(),
                                                owner.clone(),
                                                &mut brc20_index,
                                            )
                                            .await?
                                        }
                                        _ => {
                                            // Unexpected operation
                                            error!("Unexpected operation: {}", inscription.op);
                                        }
                                    }
                                } else {
                                    // No inscription found
                                    // check if the tx is sending a transfer inscription
                                    brc20_index
                                        .check_for_transfer_send(&client, &rpc, &raw_tx)
                                        .await?;
                                }
                            }
                            // Increment the tx height
                            tx_height += 1;
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
            } //
              // TODO: save to MongoDB this block height and timestamp
        }
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
    brc20_transfer_tx: &Brc20Transfer,
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
            &brc20_transfer_tx.tx.txid.to_string(),
            update_doc,
        )
        .await?;
    Ok(())
}
