extern crate serde_json;

use bitcoincore_rpc::{self, Client, RpcApi};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::thread::sleep;
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Brc20Inscription {
    pub p: String,
    pub op: String,
    tick: String,
    amt: Option<String>,
    max: Option<String>,
    lim: Option<String>,
    dec: Option<String>,
}

impl Brc20Inscription {
    fn is_valid(&self) -> bool {
        // Put your validation logic here.
        // This example checks if "op" is either "deploy" or "mint".
        matches!(&self.op[..], "deploy" | "mint")
    }
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

fn get_witness_data_for_txid(
    rpc: &Client,
    txid: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // Convert the transaction ID string to a `bitcoin::Txid`
    let txid: Result<bitcoin::Txid, _> = txid.parse();

    if let Ok(txid) = txid {
        // Fetch the raw transaction hex
        let raw_tx_info = rpc.get_raw_transaction_info(&txid, None)?;
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
    } else {
        Err("Invalid transaction ID")?
    }
}

pub fn index_brc20(
    rpc: &Client,
    start_block_height: u64,
) -> Result<(), Box<dyn std::error::Error>> {
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
                            let witness_data = get_witness_data_for_txid(&rpc, &txid.to_string())?;
                            for witness in witness_data {
                                if let Some(data) = extract_and_process_witness_data(witness) {
                                    let pretty_json =
                                        serde_json::to_string(&data).unwrap_or_default();
                                    info!("Brc-20 data: {}", pretty_json);
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
    Ok(())
}

fn extract_and_process_witness_data(witness_data: String) -> Option<serde_json::Value> {
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
            match serde_json::from_str::<serde_json::Value>(json_data) {
                Ok(parsed_data) => {
                    // Only return the parsed data if it contains the expected fields
                    if parsed_data["p"] == "brc-20" {
                        return Some(parsed_data);
                    }
                }
                Err(_e) => {
                    // error!("JSON parsing failed: {:?}", e);
                }
            }
        }
    }

    None
}
