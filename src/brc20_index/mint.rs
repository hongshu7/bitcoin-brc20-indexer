use crate::brc20_index::user_balance::UserBalanceEntryType;

use super::{
    consts, invalid_brc20::InvalidBrc20Tx, mongo::MongoClient, utils::convert_to_float,
    Brc20Inscription, ToDocument,
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
        mongo_client: &MongoClient,
    ) -> Result<Brc20Mint, Box<dyn std::error::Error>> {
        let mut reason = String::new();

        // get ticker doc from mongo
        let ticker_doc_from_mongo = mongo_client
            .get_document_by_field(
                consts::COLLECTION_TICKERS,
                "tick",
                &self.inscription.tick.to_lowercase(),
            )
            .await?;

        if let Some(ticker_doc) = ticker_doc_from_mongo {
            // get values from ticker doc
            let limit = mongo_client
                .get_double(&ticker_doc, "limit")
                .unwrap_or_default();
            let max_supply = mongo_client
                .get_double(&ticker_doc, "max_supply")
                .unwrap_or_default();
            let total_minted = mongo_client
                .get_double(&ticker_doc, "total_minted")
                .unwrap_or_default();
            let decimals = mongo_client
                .get_i32(&ticker_doc, "decimals")
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

            // Insert the invalid mint inscription into MongoDB
            let invalid_tx = InvalidBrc20Tx::new(
                self.tx.txid,
                self.inscription.clone(),
                reason,
                self.block_height,
            );

            mongo_client
                .insert_document(
                    consts::COLLECTION_INVALIDS,
                    invalid_tx.to_document(),
                    consts::MONGO_RETRIES,
                )
                .await?;
        }

        Ok(self)
    }
}

pub async fn validate_and_insert_mint(
    mongo_client: &MongoClient,
    block_height: u32,
    tx_height: u32,
    owner: Address,
    inscription: Brc20Inscription,
    raw_tx: &GetRawTransactionResult,
) -> Result<Brc20Mint, Box<dyn std::error::Error>> {
    let validated_mint_tx = Brc20Mint::new(&raw_tx, inscription, block_height, tx_height, owner)
        .validate_mint(mongo_client)
        .await?;

    if validated_mint_tx.is_valid() {
        // Add the mint transaction to the mongo database
        mongo_client
            .insert_document(
                consts::COLLECTION_MINTS,
                validated_mint_tx.to_document(),
                consts::MONGO_RETRIES,
            )
            .await?;
    }

    Ok(validated_mint_tx)
}

pub async fn update_balances_and_ticker(
    mongo_client: &MongoClient,
    validated_mint_tx: &Brc20Mint,
) -> Result<(), Box<dyn std::error::Error>> {
    if validated_mint_tx.is_valid() {
        // Insert user balance entry into MongoDB
        mongo_client
            .insert_user_balance_entry(
                &validated_mint_tx.to.to_string(),
                validated_mint_tx.amt,
                &validated_mint_tx.inscription.tick.to_lowercase(),
                validated_mint_tx.block_height.into(),
                UserBalanceEntryType::Receive,
            )
            .await?;

        // Update user overall balance and available for the receiver in MongoDB
        mongo_client
            .update_receiver_balance_document(
                &validated_mint_tx.to.to_string(),
                validated_mint_tx.amt,
                &validated_mint_tx.inscription.tick.to_lowercase(),
            )
            .await?;

        // Update total minted tokens for this ticker in MongoDB
        mongo_client
            .update_brc20_ticker_total_minted(
                &validated_mint_tx.inscription.tick.to_lowercase(),
                validated_mint_tx.amt,
            )
            .await?;
    }

    Ok(())
}

pub async fn handle_mint_operation(
    mongo_client: &MongoClient,
    block_height: u32,
    tx_height: u32,
    owner: Address,
    inscription: Brc20Inscription,
    raw_tx: &GetRawTransactionResult,
) -> Result<bool, Box<dyn std::error::Error>> {
    let validated_mint_tx = validate_and_insert_mint(
        mongo_client,
        block_height,
        tx_height,
        owner,
        inscription,
        raw_tx,
    )
    .await?;

    // Check if the mint operation is valid.
    if validated_mint_tx.is_valid() {
        info!(
            "VALID: Mint inscription added: {}",
            validated_mint_tx.get_mint()
        );
        info!("TO Address: {:?}", validated_mint_tx.to);

        update_balances_and_ticker(mongo_client, &validated_mint_tx).await?;

        return Ok(true);
    } else {
        return Ok(false);
    }
}
