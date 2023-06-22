use crate::mongo::MongoClient;

use super::{
    brc20_ticker::Brc20Ticker,
    consts,
    invalid_brc20::{InvalidBrc20Tx, InvalidBrc20TxMap},
    utils::convert_to_float,
    Brc20Index, Brc20Inscription, ToDocument,
};
use bitcoin::Address;
use bitcoincore_rpc::bitcoincore_rpc_json::GetRawTransactionResult;
use log::{error, info};
use mongodb::bson::{doc, Document};
use serde::Serialize;
use std::{collections::HashMap, fmt};

#[derive(Debug, Clone, Serialize)]
pub struct Brc20Mint {
    // pub id: Option<bson::oid>,
    pub amt: f64,
    pub block_height: u32,
    pub tx_height: u32,
    pub to: Address,
    pub tx: GetRawTransactionResult,
    pub inscription: Brc20Inscription,
    pub is_valid: bool,
}

impl ToDocument for Brc20Mint {
    fn to_document(&self) -> Document {
        doc! {
            // "_id": self.id.clone(),
            // "ticker_id": self.ticker_id.clone(),
            "amt": self.amt,
            "block_height": self.block_height,
            "tx_height": self.tx_height,
            "to": self.to.to_string(),
            "tx": self.tx.to_document(), // Convert GetRawTransactionResult to document
            "inscription": self.inscription.to_document(),
            "is_valid": self.is_valid,
            // "created_at": self.created_at.clone(),
        }
    }
}

impl Brc20Mint {
    pub fn new(
        tx: &GetRawTransactionResult,
        inscription: Brc20Inscription,
        block_height: u32,
        tx_height: u32,
        to: Address,
    ) -> Self {
        Brc20Mint {
            amt: 0.0,
            block_height,
            tx_height,
            to,
            tx: tx.clone(),
            inscription,
            is_valid: false,
        }
    }

    pub fn get_amount(&self) -> f64 {
        self.amt
    }

    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    pub fn get_mint(&self) -> &Brc20Inscription {
        &self.inscription
    }

    pub async fn validate_mint<'a>(
        mut self,
        ticker_map: &'a mut HashMap<String, Brc20Ticker>,
        invalid_tx_map: &'a mut InvalidBrc20TxMap,
        mongo_client: &MongoClient,
    ) -> Result<Brc20Mint, Box<dyn std::error::Error>> {
        let mut is_valid = true;
        let mut reason = String::new();

        if let Some(ticker) = ticker_map.get(&self.inscription.tick) {
            let limit = ticker.get_limit();
            let max_supply = ticker.get_max_supply();
            let total_minted = ticker.get_total_supply();
            let amount = match self.inscription.amt.as_ref().map(String::as_str) {
                Some(amt_str) => convert_to_float(amt_str, ticker.get_decimals()),
                None => Ok(0.0), // Set a default value if the amount is not present
            };

            match amount {
                Ok(amount) => {
                    // Check if the amount is greater than the limit
                    if amount > limit {
                        is_valid = false;
                        reason = "Mint amount exceeds limit".to_string();
                    // Check if total minted is already greater than or equal to max supply
                    } else if total_minted >= max_supply {
                        is_valid = false;
                        reason = "Total minted is already at or exceeds max supply".to_string();
                    // Check if the total minted amount + requested mint amount exceeds the max supply
                    } else if total_minted + amount > max_supply {
                        // Adjust the mint amount to mint the remaining tokens
                        let remaining_amount = max_supply - total_minted;
                        self.amt = remaining_amount;
                    } else {
                        self.amt = amount;
                    }
                }
                Err(e) => {
                    is_valid = false;
                    reason = e.to_string();
                }
            }
        } else {
            is_valid = false;
            reason = "Ticker symbol does not exist".to_string();
        }

        if !is_valid {
            error!("INVALID: {}", reason);
            let invalid_tx = InvalidBrc20Tx::new(self.tx.txid, self.inscription.clone(), reason);
            invalid_tx_map.add_invalid_tx(invalid_tx.clone());

            // Insert the invalid mint inscription into MongoDB
            mongo_client
                .insert_document(consts::COLLECTION_INVALIDS, invalid_tx.to_document())
                .await?;
        } else {
            // Set is_valid to true when the transaction is valid
            is_valid = true;
            let ticker = ticker_map.get_mut(&self.inscription.tick).unwrap();
            ticker.add_mint(self.clone());
        }

        self.is_valid = is_valid;
        Ok(self)
    }
}

impl fmt::Display for Brc20Mint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Brc20 TransactionId: {}", self.tx.txid)?;
        writeln!(f, "Mint: {:#?}", self.inscription)?;
        writeln!(f, "Amount: {}", self.amt)?;
        writeln!(f, "Is Valid: {}", self.is_valid)?;
        Ok(())
    }
}

pub async fn handle_mint_operation(
    mongo_client: &MongoClient,
    block_height: u32,
    tx_height: u32,
    owner: Address,
    inscription: Brc20Inscription,
    raw_tx: &GetRawTransactionResult,
    brc20_index: &mut Brc20Index,
) -> Result<(), Box<dyn std::error::Error>> {
    let validated_mint_tx = Brc20Mint::new(&raw_tx, inscription, block_height, tx_height, owner)
        .validate_mint(
            &mut brc20_index.tickers,
            &mut brc20_index.invalid_tx_map,
            mongo_client,
        )
        .await?;

    // Check if the mint operation is valid.
    if validated_mint_tx.is_valid() {
        info!("Mint: {:?}", validated_mint_tx.get_mint());
        info!("TO Address: {:?}", validated_mint_tx.to);
    }

    // Add the mint transaction to the mongo database
    mongo_client
        .insert_document(consts::COLLECTION_MINTS, validated_mint_tx.to_document())
        .await?;

    //TODO: update userbalance and ticker structs in mongodb

    Ok(())
}
