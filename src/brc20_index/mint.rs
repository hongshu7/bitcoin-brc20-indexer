use std::collections::HashMap;

use crate::brc20_index::user_balance::UserBalanceEntryType;

use super::{
    consts, invalid_brc20::InvalidBrc20Tx, mongo::MongoClient, user_balance::UserBalanceEntry,
    utils::convert_to_float, Brc20Inscription, ToDocument,
};
use bitcoin::Address;
use bitcoincore_rpc::bitcoincore_rpc_json::GetRawTransactionResult;
use log::{error, info};
use mongodb::bson::{doc, Bson, DateTime, Document};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Brc20Mint {
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
            "created_at": Bson::DateTime(DateTime::now())
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

    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    pub fn get_mint(&self) -> &Brc20Inscription {
        &self.inscription
    }

    pub async fn validate_mint<'a>(
        mut self,
        ticker_doc_opt: Option<&Document>,
        invalid_brc20_docs: &mut Vec<Document>,
    ) -> Result<Brc20Mint, Box<dyn std::error::Error>> {
        let mut reason = String::new();

        if let Some(ticker_doc) = ticker_doc_opt {
            // get values from ticker doc
            let limit = ticker_doc
                .get("limit")
                .and_then(Bson::as_f64)
                .unwrap_or_default();
            let max_supply = ticker_doc
                .get("max_supply")
                .and_then(Bson::as_f64)
                .unwrap_or_default();
            let total_minted = ticker_doc
                .get("total_minted")
                .and_then(Bson::as_f64)
                .unwrap_or_default();
            let decimals = ticker_doc
                .get("decimals")
                .and_then(Bson::as_i32)
                .unwrap_or_default();

            // get amount from inscription
            let amount = match self.inscription.amt.as_ref().map(String::as_str) {
                Some(amt_str) => convert_to_float(amt_str, decimals.try_into().unwrap()),
                None => Ok(0.0),
            };

            // validate mint amount against ticker limit and max supply
            match amount {
                Ok(amount) => {
                    // Check if the amount is greater than the limit
                    if amount > limit {
                        reason = "Mint amount exceeds limit".to_string();
                    // Check if total minted is already greater than or equal to max supply
                    } else if total_minted >= max_supply {
                        reason = "Total minted is already at max supply".to_string();
                    // Check if the total minted amount + requested mint amount exceeds the max supply
                    } else if total_minted + amount > max_supply {
                        self.is_valid = true;
                        // Adjust the mint amount to mint remaining tokens
                        let remaining_amount = max_supply - total_minted;
                        self.amt = remaining_amount;
                    } else {
                        self.is_valid = true;
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
        // handle invalid mint transaction
        if !self.is_valid {
            error!("INVALID: {}", reason);

            // Insert the invalid mint inscription
            let invalid_tx = InvalidBrc20Tx::new(
                self.tx.txid,
                self.inscription.clone(),
                reason,
                self.block_height,
            );

            invalid_brc20_docs.push(invalid_tx.to_document());
        }

        Ok(self)
    }
}

// This function will try to get a ticker's document from the hashmap
// If the ticker is not in the hashmap, it will fetch the document from MongoDB and store it in the hashmap
async fn get_ticker<'a>(
    tickers: &'a mut HashMap<String, Document>,
    ticker_symbol: &String,
    mongo_client: &MongoClient,
) -> Option<&'a Document> {
    // Check if the hashmap contains the ticker
    if tickers.contains_key(ticker_symbol) {
        tickers.get(ticker_symbol)
    } else {
        // If not, fetch the ticker from MongoDB and store it in the hashmap
        match mongo_client
            .get_document_by_field(consts::COLLECTION_TICKERS, "tick", ticker_symbol)
            .await
        {
            Ok(Some(ticker_doc)) => {
                tickers.insert(ticker_symbol.clone(), ticker_doc.clone());
                tickers.get(ticker_symbol)
            }
            Ok(None) => None,
            Err(_) => None,
        }
    }
}

// This function will update the total minted tokens for a given ticker in MongoDB and the in-memory hashmap
async fn update_ticker_total_minted(
    ticker_symbol: &String,
    mint_amount: f64,
    tickers: &mut HashMap<String, Document>,
    mongo_client: &MongoClient,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check if the hashmap contains the ticker
    if let Some(ticker_doc) = get_ticker(tickers, ticker_symbol, mongo_client).await {
        // Update the total minted amount in the hashmap
        let new_total_minted = ticker_doc
            .get("total_minted")
            .and_then(Bson::as_f64)
            .unwrap_or(0.0)
            + mint_amount;

        // Create a new document with the updated total_minted
        let mut updated_ticker_doc = ticker_doc.clone();
        updated_ticker_doc.insert("total_minted", Bson::Double(new_total_minted));

        // Replace the old ticker_doc in the hashmap with the updated one
        tickers.insert(ticker_symbol.clone(), updated_ticker_doc);

        // Update the total minted amount in MongoDB
        //TODO: write to mongo at the end of the block
        // mongo_client
        //     .update_brc20_ticker_total_minted(ticker_symbol, mint_amount)
        //     .await?;
    }

    Ok(())
}

pub async fn pre_validate_mint(
    mongo_client: &MongoClient,
    block_height: u32,
    tx_height: u32,
    owner: Address,
    inscription: Brc20Inscription,
    raw_tx: &GetRawTransactionResult,
    tickers: &mut HashMap<String, Document>,
    invalid_brc20_docs: &mut Vec<Document>,
) -> Result<Brc20Mint, Box<dyn std::error::Error>> {
    // Try to get the ticker from the hashmap if not, then mongodb
    let ticker_doc_opt = get_ticker(tickers, &inscription.tick.to_lowercase(), mongo_client).await;

    // Create a new Brc20Mint instance
    let new_mint = Brc20Mint::new(&raw_tx, inscription, block_height, tx_height, owner);
    new_mint
        .validate_mint(ticker_doc_opt, invalid_brc20_docs)
        .await
}

pub async fn update_balances_and_ticker(
    mongo_client: &MongoClient,
    validated_mint_tx: &Brc20Mint,
    tickers: &mut HashMap<String, Document>,
) -> Result<UserBalanceEntry, Box<dyn std::error::Error>> {
    if validated_mint_tx.is_valid() {
        // Update user overall balance and available for the receiver in MongoDB
        mongo_client
            .update_receiver_balance_document(
                &validated_mint_tx.to.to_string(),
                validated_mint_tx.amt,
                &validated_mint_tx.inscription.tick.to_lowercase(),
            )
            .await?;

        // Update total minted tokens for this ticker in MongoDB and in-memory hashmap
        update_ticker_total_minted(
            &validated_mint_tx.inscription.tick.to_lowercase(),
            validated_mint_tx.amt,
            tickers,
            mongo_client,
        )
        .await?;
    }

    // Insert user balance entry
    Ok(mongo_client
        .insert_user_balance_entry(
            &validated_mint_tx.to.to_string(),
            validated_mint_tx.amt,
            &validated_mint_tx.inscription.tick.to_lowercase(),
            validated_mint_tx.block_height.into(),
            UserBalanceEntryType::Receive,
        )
        .await?)
}

pub async fn handle_mint_operation(
    mongo_client: &MongoClient,
    block_height: u32,
    tx_height: u32,
    owner: Address,
    inscription: Brc20Inscription,
    raw_tx: &GetRawTransactionResult,
    tickers: &mut HashMap<String, Document>,
    invalid_brc20_docs: &mut Vec<Document>,
) -> Result<(Brc20Mint, UserBalanceEntry), Box<dyn std::error::Error>> {
    // Note: pre_validate_mint now also takes a reference to the tickers hashmap
    let validated_mint_tx = pre_validate_mint(
        mongo_client,
        block_height,
        tx_height,
        owner,
        inscription,
        raw_tx,
        tickers,
        invalid_brc20_docs,
    )
    .await?;

    let mut user_balance_entry = UserBalanceEntry::default();

    // Check if the mint operation is valid.
    if validated_mint_tx.is_valid() {
        info!(
            "VALID: Mint inscription added: {}",
            validated_mint_tx.get_mint()
        );
        info!("TO Address: {:?}", validated_mint_tx.to);

        // update_balances_and_ticker now also takes a reference to the tickers hashmap
        user_balance_entry =
            update_balances_and_ticker(mongo_client, &validated_mint_tx, tickers).await?;
    }

    Ok((validated_mint_tx, user_balance_entry))
}
