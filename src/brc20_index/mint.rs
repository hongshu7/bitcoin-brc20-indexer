use super::{
    brc20_ticker::Brc20Ticker,
    consts,
    invalid_brc20::{InvalidBrc20Tx, InvalidBrc20TxMap},
    mongo::MongoClient,
    utils::convert_to_float,
    Brc20Index, Brc20Inscription, ToDocument,
};
use bitcoin::Address;
use bitcoincore_rpc::bitcoincore_rpc_json::GetRawTransactionResult;
use log::{error, info};
use mongodb::bson::{doc, Bson, Document};
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
            "amt": self.amt,
            "block_height": self.block_height,
            "tx_height": self.tx_height,
            "to": self.to.to_string(),
            "tx": self.tx.to_document(),
            "inscription": self.inscription.to_document(),
            "is_valid": self.is_valid,
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
        let mut is_valid = false;
        let mut reason = String::new();

        if let Some(ticker) = ticker_map.get(&self.inscription.tick.to_lowercase()) {
            //TODO: get these values from db
            let limit = ticker.get_limit();
            let max_supply = ticker.get_max_supply();
            let total_minted = ticker.get_total_supply();
            let amount = match self.inscription.amt.as_ref().map(String::as_str) {
                Some(amt_str) => convert_to_float(amt_str, ticker.get_decimals()),
                None => Ok(0.0),
            };

            match amount {
                Ok(amount) => {
                    // Check if the amount is greater than the limit
                    if amount > limit {
                        reason = "Mint amount exceeds limit".to_string();
                    // Check if total minted is already greater than or equal to max supply
                    } else if total_minted >= max_supply {
                        reason = "Total minted is already at or exceeds max supply".to_string();
                    // Check if the total minted amount + requested mint amount exceeds the max supply
                    } else if total_minted + amount > max_supply {
                        is_valid = true;
                        // Adjust the mint amount to mint the remaining tokens
                        let remaining_amount = max_supply - total_minted;
                        self.amt = remaining_amount;
                    } else {
                        is_valid = true;
                        self.amt = amount;
                    }
                }
                Err(e) => {
                    reason = e.to_string();
                }
            }
        } else {
            reason = "Ticker symbol does not exist".to_string();
        }

        if !is_valid {
            // Set is_valid to false when the transaction is invalid
            error!("INVALID: {}", reason);

            // Add the invalid mint transaction to the invalid transaction map
            let invalid_tx = InvalidBrc20Tx::new(self.tx.txid, self.inscription.clone(), reason);
            invalid_tx_map.add_invalid_tx(invalid_tx.clone());

            // Insert the invalid mint inscription into MongoDB
            mongo_client
                .insert_document(consts::COLLECTION_INVALIDS, invalid_tx.to_document())
                .await?;
        } else {
            // Set is_valid to true when the transaction is valid
            println!("VALID: Mint inscription added: {:#?}", self.inscription);
            self.is_valid = is_valid;

            //get ticker for this mint
            let ticker = ticker_map
                .get_mut(&self.inscription.tick.to_lowercase())
                .unwrap();

            ticker
                .add_mint_to_ticker(self.clone(), mongo_client)
                .await?;
        }
        // Return the updated mint transaction, which might be valid or invalid
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

        let amount = validated_mint_tx.get_amount();

        //---------------MONGO DOB-----------------//
        // Add the mint transaction to the mongo database
        mongo_client
            .insert_document(consts::COLLECTION_MINTS, validated_mint_tx.to_document())
            .await?;

        // retrieve ticker struct from mongodb
        let ticker_doc_from_mongo = mongo_client
            .get_document_by_field(
                consts::COLLECTION_TICKERS,
                "tick",
                &validated_mint_tx.inscription.tick.to_lowercase(),
            )
            .await?;

        if let Some(mut ticker_doc) = ticker_doc_from_mongo {
            // get ticker from ticker map
            let ticker_from_map = brc20_index
                .tickers
                .get(&validated_mint_tx.inscription.tick.to_lowercase())
                .unwrap();

            if let Some(total_minted) = ticker_doc.get("total_minted") {
                if let Bson::Double(val) = total_minted {
                    ticker_doc.insert("total_minted", Bson::Double(val + amount));
                }
            }

            let update_doc = doc! {
                "$set": {
                    "total_minted": ticker_doc.get("total_minted").unwrap_or_else(|| &Bson::Double(0.0)),
                }
            };

            // update ticker struct in mongodb
            mongo_client
                .update_document_by_field(
                    consts::COLLECTION_TICKERS,
                    "total_minted",
                    &ticker_from_map.get_total_supply().to_string(),
                    update_doc,
                )
                .await?;
        } else {
            error!("Ticker not found in MongoDB");
        }
    }

    Ok(())
}
