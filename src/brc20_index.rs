use self::{
    brc20_ticker::Brc20Ticker,
    brc20_tx::Brc20Tx,
    brc20_tx::InvalidBrc20TxMap,
    deploy::handle_deploy_operation,
    mint::handle_mint_operation,
    transfer::handle_transfer_operation,
    utils::{
        extract_and_process_witness_data, get_owner_of_vout, get_witness_data_from_raw_tx,
        write_tickers_to_file,
    },
};
use bitcoin::OutPoint;
use bitcoincore_rpc::bitcoincore_rpc_json::GetRawTransactionResult;
use bitcoincore_rpc::{self, Client, RpcApi};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs::DirBuilder, thread::sleep, time::Duration};

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

    pub fn dump_invalid_txs_to_file(&self, path: &str) -> std::io::Result<()> {
        self.invalid_tx_map.dump_to_file(path)
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
    start_block_height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    // Instantiate a new `Brc20Index` struct.
    let mut brc20_index = Brc20Index::new();

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

                                    // create brc20_tx
                                    let brc20_tx = Brc20Tx::new(
                                        &raw_tx,
                                        owner.clone(),
                                        current_block_height as u32,
                                    )?;

                                    match &inscription.op[..] {
                                        "deploy" => handle_deploy_operation(
                                            inscription,
                                            raw_tx.clone(),
                                            owner.clone(),
                                            current_block_height,
                                            tx_height,
                                            &mut brc20_index,
                                        )?,
                                        "mint" => handle_mint_operation(
                                            current_block_height,
                                            tx_height,
                                            owner,
                                            inscription,
                                            &raw_tx,
                                            &mut brc20_index,
                                        )?,
                                        "transfer" => handle_transfer_operation(
                                            inscription,
                                            brc20_tx,
                                            &mut brc20_index,
                                        )?,
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
            }
        }

        // stop after reaching a certain block height
        if current_block_height > 795150 {
            break;
        }
    }

    //Log Tickers to file
    let result = write_tickers_to_file(&brc20_index.tickers, "tickers");
    match result {
        Ok(()) => println!("Successfully wrote tickers to files."),
        Err(e) => println!("An error occurred while writing tickers to files: {:?}", e),
    }

    // Log Invalids to file
    DirBuilder::new().recursive(true).create("invalid_txs")?;
    let result = brc20_index.dump_invalid_txs_to_file("invalid_txs/invalid_txs.json");
    match result {
        Ok(()) => println!("Successfully dumped invalid transactions to file."),
        Err(e) => println!(
            "An error occurred while dumping invalid transactions to file: {:?}",
            e
        ),
    }

    Ok(())
}
