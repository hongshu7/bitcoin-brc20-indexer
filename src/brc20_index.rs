extern crate serde_json;

use bitcoin::{Address, Network, OutPoint};
use bitcoincore_rpc::bitcoincore_rpc_json::GetRawTransactionResult;
use bitcoincore_rpc::{self, Client, RpcApi};
use log::{error, info};
use mongodb::bson::{doc, Document};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration;

use crate::brc20_index::brc20_tx::Brc20Tx;
use crate::brc20_index::deploy::Brc20DeployTx;
use crate::brc20_index::mint::Brc20MintTx;
use crate::brc20_index::transfer::Brc20TransferTx;

use self::brc20_ticker::Brc20Ticker;
use self::brc20_tx::InvalidBrc20TxMap;

mod brc20_ticker;
mod brc20_tx;
mod deploy;
mod mint;
mod transfer;
mod user_balance;
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
            "tick": &self.tick,
            "amt": &self.amt,
            "max": &self.max,
            "lim": &self.lim,
            "dec": &self.dec,
        }
    }
}

// Brc20Index is a struct that represents the
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

    pub fn process_active_transfer(
        &mut self,
        rpc: &Client,
        raw_tx_info: &GetRawTransactionResult,
    ) -> Result<(), anyhow::Error> {
        let transaction = raw_tx_info.transaction()?;

        for (input_index, input) in transaction.input.iter().enumerate() {
            let outpoint = input.previous_output;
            let ticker = match self.get_active_transfer_inscription_ticker(&outpoint) {
                Some(ticker) => ticker,
                None => {
                    // log::error!(
                    //     "No active transfer inscription for outpoint: {:?}",
                    //     outpoint
                    // );
                    continue;
                }
            };

            let brc20_ticker = match self.get_ticker_mut(&ticker) {
                Some(brc20_ticker) => brc20_ticker,
                None => {
                    log::error!("Ticker {} not found", ticker);
                    continue;
                }
            };

            if !brc20_ticker.has_active_transfer_inscription(&outpoint) {
                // log::error!(
                //     "No user balance with active transfer inscription for outpoint: {:?}",
                //     outpoint
                // );
                continue;
            }

            let brc20_transfer_tx =
                match brc20_ticker.get_and_remove_active_transfer_inscription(&outpoint) {
                    Some(brc20_transfer_tx) => brc20_transfer_tx,
                    None => {
                        // log::error!(
                        //     "Active transfer inscription not found for outpoint: {:?}",
                        //     outpoint
                        // );
                        continue;
                    }
                };

            let proper_vout = if input_index == 0 {
                0
            } else {
                // get values of all inputs only when necessary
                let input_values =
                    utils::transaction_inputs_to_values(rpc, &transaction.input[0..input_index])?;

                let input_value_sum: u64 = input_values.iter().sum();
                let proper_vout = transaction
                    .output
                    .iter()
                    .scan(0, |acc, output| {
                        *acc += output.value;
                        Some(*acc)
                    })
                    .position(|value| value >= input_value_sum)
                    .unwrap_or_else(|| transaction.output.len() - 1);

                proper_vout
            };

            let receiver_address = get_owner_of_vout(&raw_tx_info, proper_vout)?;

            // create transfer transaction
            let brc20_transfer_tx = brc20_transfer_tx
                .set_receiver(receiver_address.clone())
                .set_transfer_tx(raw_tx_info.clone());

            // Update user balances
            brc20_ticker.update_transfer_receives(receiver_address, brc20_transfer_tx.clone());
            brc20_ticker.update_transfer_sends(
                brc20_transfer_tx
                    .get_inscription_brc20_tx()
                    .get_owner()
                    .clone(),
                brc20_transfer_tx,
            );

            self.remove_active_transfer_balance(&outpoint);
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

pub fn index_brc20(
    rpc: &Client,
    start_block_height: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Instantiate a new `Brc20Index` struct.
    let mut brc20_index = Brc20Index::new();

    let mut current_block_height = start_block_height;
    loop {
        match rpc.get_block_hash(current_block_height) {
            Ok(current_block_hash) => {
                match rpc.get_block(&current_block_hash) {
                    Ok(block) => {
                        info!(
                            "Fetched block: {:?}, height: {:?}",
                            current_block_hash, current_block_height
                        );
                        let length = block.txdata.len();
                        info!("Number of transactions: {:?}", length);

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
                                    // let pretty_json =
                                    //     serde_json::to_string(&inscription).unwrap_or_default();
                                    // info!("Brc-20 data: {}", pretty_json);

                                    // get owner address, inscription is first satoshi of first output
                                    let owner = get_owner_of_vout_0(&raw_tx)?;

                                    // create brc20_tx
                                    let brc20_tx =
                                        Brc20Tx::new(&raw_tx, owner, current_block_height as u32)?;

                                    match &inscription.op[..] {
                                        "deploy" => {
                                            let validated_deploy_tx =
                                                Brc20DeployTx::new(brc20_tx, inscription)
                                                    .validate_deploy_script(
                                                        &mut brc20_index.invalid_tx_map,
                                                        &brc20_index.tickers,
                                                    );

                                            if validated_deploy_tx.is_valid() {
                                                println!("=========================");
                                                println!(
                                                    "Deploy: {:?}",
                                                    validated_deploy_tx.get_deploy_script()
                                                );
                                                println!("=========================");

                                                // Instantiate a new `Brc20Ticker` struct and update the hashmap with the deploy information.
                                                let ticker = Brc20Ticker::new(validated_deploy_tx);
                                                brc20_index
                                                    .tickers
                                                    .insert(ticker.get_ticker(), ticker);
                                            }
                                        }
                                        "mint" => {
                                            // Validate and instantiate a new `Brc20MintTx` struct.
                                            let validated_mint_tx =
                                                Brc20MintTx::new(&brc20_tx, inscription)
                                                    .validate_mint(
                                                        &brc20_tx,
                                                        &mut brc20_index.tickers,
                                                        &mut brc20_index.invalid_tx_map,
                                                    );

                                            // Check if the mint operation is valid.
                                            if validated_mint_tx.is_valid() {
                                                println!("=========================");
                                                println!(
                                                    "Mint: {:?}",
                                                    validated_mint_tx.get_mint()
                                                );
                                                println!(
                                                    "Owner Address: {:?}",
                                                    validated_mint_tx.get_brc20_tx().get_owner()
                                                );
                                                println!("=========================");
                                            }
                                        }
                                        "transfer" => {
                                            // Instantiate a new `BrcTransferTx` struct.
                                            let mut brc20_transfer_tx =
                                                Brc20TransferTx::new(brc20_tx, inscription);

                                            // Call handle_inscribe_transfer_amount
                                            brc20_transfer_tx
                                                .handle_inscribe_transfer_amount(&mut brc20_index);

                                            // add to active transfer inscriptions for index
                                            brc20_index.update_active_transfer_inscription(
                                                brc20_transfer_tx.get_inscription_outpoint(),
                                                brc20_transfer_tx
                                                    .get_transfer_script()
                                                    .tick
                                                    .clone(),
                                            );

                                            // Check if the transfer is valid.
                                            if brc20_transfer_tx.is_valid() {
                                                println!("=========================");
                                                println!(
                                                    "Transfer: {:?}",
                                                    brc20_transfer_tx.get_transfer_script()
                                                );
                                                println!(
                                                    "Owner Address: {:?}",
                                                    brc20_transfer_tx
                                                        .get_inscription_brc20_tx()
                                                        .get_owner()
                                                );
                                                println!("=========================");
                                            }
                                        }
                                        _ => {
                                            // Unexpected operation
                                            error!("Unexpected operation: {}", inscription.op);
                                        }
                                    }
                                } else {
                                    // No inscription found
                                    // check if the tx is sending a transfer inscription
                                    brc20_index.process_active_transfer(&rpc, &raw_tx)?;
                                }
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

        // stop after reaching a certain block height
        if current_block_height > 800000 {
            break;
        }
    }

    if let Err(e) = write_tickers_to_file(&brc20_index.tickers, "brc20_indexer_output.json") {
        eprintln!("Failed to write to file: {}", e);
    }

    Ok(())
}

fn get_witness_data_from_raw_tx(
    raw_tx_info: &GetRawTransactionResult,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let transaction = raw_tx_info.transaction()?;

    let mut witness_data_strings: Vec<String> = Vec::new();

    // Get the first transaction input
    if let Some(input) = transaction.input.first() {
        // Iterate through each witness of the input
        for witness in &input.witness {
            let witness_string = String::from_utf8_lossy(witness).into_owned();
            witness_data_strings.push(witness_string);
        }
    }

    Ok(witness_data_strings)
}

// extracts only inscriptions that read "brc-20", many will be invalid
fn extract_and_process_witness_data(witness_data: String) -> Option<Brc20Inscription> {
    // Check for the correct MIME type and find its end
    let mime_end_index = if witness_data.contains("text/plain") {
        witness_data.find("text/plain").unwrap() + "text/plain".len()
    } else if witness_data.contains("application/json") {
        witness_data.find("application/json").unwrap() + "application/json".len()
    } else {
        return None;
    };

    // Start searching for the JSON data only after the MIME type
    if let Some(json_start) = witness_data[mime_end_index..].find('{') {
        let json_start = mime_end_index + json_start; // Adjust json_start to be relative to the original string
        if let Some(json_end) = witness_data[json_start..].rfind('}') {
            // Extract the JSON string
            let json_data = &witness_data[json_start..json_start + json_end + 1];

            // Try to parse the JSON data
            match serde_json::from_str::<Brc20Inscription>(json_data) {
                Ok(parsed_data) => {
                    // Only return the parsed data if it contains the expected fields
                    if parsed_data.p == "brc-20" {
                        // // Convert the data to JSON string with null values represented as "null"
                        // let json_string = serde_json::to_string(&parsed_data).unwrap_or_default();
                        // println!("{}", json_string);

                        return Some(parsed_data);
                    }
                }
                Err(e) => {
                    // error!("JSON parsing failed: {:?}", e);
                }
            }
        }
    }

    None
}

pub fn get_owner_of_vout_0(
    raw_tx_info: &GetRawTransactionResult,
) -> Result<Address, anyhow::Error> {
    if raw_tx_info.vout.is_empty() {
        return Err(anyhow::anyhow!("Transaction has no outputs"));
    }

    // Get the controlling address of the first output
    let script_pubkey = &raw_tx_info.vout[0].script_pub_key;
    let script = match script_pubkey.script() {
        Ok(script) => script,
        Err(e) => return Err(anyhow::anyhow!("Failed to get script: {:?}", e)),
    };
    let this_address = Address::from_script(&script, Network::Bitcoin).map_err(|e| {
        error!("Couldn't derive address from scriptPubKey: {:?}", e);
        anyhow::anyhow!("Couldn't derive address from scriptPubKey: {:?}", e)
    })?;

    Ok(this_address)
}

pub fn get_owner_of_vout(
    raw_tx_info: &GetRawTransactionResult,
    vout_index: usize,
) -> Result<Address, anyhow::Error> {
    if raw_tx_info.vout.is_empty() {
        return Err(anyhow::anyhow!("Transaction has no outputs"));
    }

    if raw_tx_info.vout.len() <= vout_index {
        return Err(anyhow::anyhow!(
            "Transaction doesn't have vout at given index"
        ));
    }

    // Get the controlling address of the first output
    let script_pubkey = &raw_tx_info.vout[vout_index].script_pub_key;
    let script = match script_pubkey.script() {
        Ok(script) => script,
        Err(e) => return Err(anyhow::anyhow!("Failed to get script: {:?}", e)),
    };
    let this_address = Address::from_script(&script, Network::Bitcoin).map_err(|e| {
        error!("Couldn't derive address from scriptPubKey: {:?}", e);
        anyhow::anyhow!("Couldn't derive address from scriptPubKey: {:?}", e)
    })?;

    Ok(this_address)
}

use std::fs::File;
use std::io::Write;

pub fn write_tickers_to_file(
    tickers: &HashMap<String, Brc20Ticker>,
    filename: &str,
) -> std::io::Result<()> {
    let file = File::create(filename)?;

    for ticker in tickers.values() {
        let serialized_ticker = serde_json::to_writer_pretty(file, tickers)?;
        // Serialize ticker to a JSON string.
        writeln!(file, "{:#?}", serialized_ticker)?; // Write the string to the file.
    }

    Ok(())
}
