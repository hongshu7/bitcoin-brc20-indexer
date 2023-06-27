use super::{consts, invalid_brc20::InvalidBrc20Tx, mongo::MongoClient, Brc20Inscription};
use crate::brc20_index::{user_balance::UserBalanceEntryType, ToDocument};
use bitcoin::Address;
use bitcoincore_rpc::bitcoincore_rpc_json::GetRawTransactionResult;
use log::{error, info};
use mongodb::bson::{doc, Bson, DateTime, Document};
use serde::Serialize;
use std::{collections::HashMap, fmt};

// create active transfer struct
#[derive(Serialize)]
pub struct Brc20ActiveTransfer {
    pub tx_id: String,
    pub vout: i64,
    pub block_height: i64,
}

impl Brc20ActiveTransfer {
    pub fn new(tx_id: String, vout: i64, block_height: i64) -> Self {
        Brc20ActiveTransfer {
            tx_id,
            vout,
            block_height,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Brc20Transfer {
    pub amt: f64,
    pub block_height: u32,
    pub tx_height: u32,
    pub tx: GetRawTransactionResult,
    pub inscription: Brc20Inscription,
    pub send_tx: Option<GetRawTransactionResult>,
    pub from: Address,
    pub to: Option<Address>,
    pub is_valid: bool,
}

impl Brc20Transfer {
    pub fn new(
        inscription_tx: GetRawTransactionResult,
        inscription: Brc20Inscription,
        block_height: u32,
        tx_height: u32,
        from: Address,
    ) -> Self {
        let amt = inscription
            .amt
            .as_ref()
            .map(|amt_str| amt_str.parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);

        Brc20Transfer {
            amt,
            block_height,
            tx_height,
            tx: inscription_tx,
            send_tx: None,
            inscription,
            from,
            to: None,
            is_valid: false,
        }
    }

    // getters and setters
    pub fn get_transfer_script(&self) -> &Brc20Inscription {
        &self.inscription
    }

    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    pub async fn validate_inscribe_transfer(
        &mut self,
        mongo_client: &MongoClient,
        active_transfers: &mut Option<HashMap<(String, i64), Brc20ActiveTransfer>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let from = &self.from;
        let ticker_symbol = &self.inscription.tick.to_lowercase();

        // get ticker doc from mongo
        let ticker_doc_from_mongo = mongo_client
            .get_document_by_field(consts::COLLECTION_TICKERS, "tick", ticker_symbol)
            .await?;

        if ticker_doc_from_mongo.is_none() {
            // Ticker not found, create invalid transaction
            let reason = "Ticker not found";
            error!("INVALID Transfer Inscribe: {}", reason);

            self.insert_invalid_tx(reason, mongo_client).await?;

            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                reason,
            )));
        }

        // get the user balance from mongo
        let filter = doc! {
          "address": from.to_string(),
          "tick": self.inscription.tick.to_lowercase(),
        };

        let user_balance_from = mongo_client
            .get_user_balance_document(consts::COLLECTION_USER_BALANCES, filter.clone())
            .await?;

        if let Some(user_balance) = user_balance_from {
            let available_balance = mongo_client
                .get_double(&user_balance, "available_balance")
                .unwrap_or_default();

            // get transfer amount
            let transfer_amount = self
                .inscription
                .amt
                .as_ref()
                .and_then(|amt_str| amt_str.parse::<f64>().ok())
                .unwrap_or(0.0);

            // check if user has enough balance to transfer
            if available_balance >= transfer_amount {
                println!("VALID: Transfer inscription added. From: {:#?}", from);

                // if valid, add transfer inscription to user balance
                self.is_valid = true;

                // insert user balance entry
                mongo_client
                    .insert_user_balance_entry(
                        &self.from.to_string(),
                        transfer_amount,
                        &self.inscription.tick.to_lowercase(),
                        self.block_height.into(),
                        UserBalanceEntryType::Inscription,
                    )
                    .await?;

                // Update the user balance document in MongoDB
                mongo_client
                    .update_transfer_inscriber_user_balance_document(
                        &from.to_string(),
                        transfer_amount,
                        ticker_symbol,
                        user_balance,
                    )
                    .await?;

                // Create new active transfer when inscription is valid
                let active_transfer =
                    Brc20ActiveTransfer::new(self.tx.txid.to_string(), 0, self.block_height.into());

                // If active_transfers is None, create a new HashMap and assign it to active_transfers
                if active_transfers.is_none() {
                    *active_transfers = Some(HashMap::new());
                }

                // We know active_transfers is Some at this point, so we can unwrap it
                active_transfers
                    .as_mut()
                    .unwrap()
                    .insert((self.tx.txid.to_string(), 0), active_transfer);
            } else {
                // if invalid, add invalid tx and return
                let reason = "Transfer amount exceeds available balance";
                error!("INVALID: {}", reason);

                self.insert_invalid_tx(reason, mongo_client).await?;
            }
        } else {
            // User balance not found, create invalid transaction
            let reason = "User balance not found";
            error!("INVALID: {}", reason);

            self.insert_invalid_tx(reason, mongo_client).await?;
        }

        Ok(())
    }

    pub async fn insert_invalid_tx(
        &self,
        reason: &str,
        mongo_client: &MongoClient,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let invalid_tx = InvalidBrc20Tx::new(
            self.tx.txid,
            self.inscription.clone(),
            reason.to_string(),
            self.block_height,
        );

        // Insert the invalid transaction into MongoDB
        mongo_client
            .insert_document(consts::COLLECTION_INVALIDS, invalid_tx.to_document())
            .await?;

        Ok(())
    }
}

impl fmt::Display for Brc20Transfer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Inscription TransactionId: {}", self.tx.txid)?;
        writeln!(f, "Transfer Transaction: {:?}", self.send_tx)?;
        writeln!(f, "Transfer Script: {:#?}", self.inscription)?;
        writeln!(f, "Block Height: {}", self.block_height)?;
        writeln!(f, "Transaction Height: {}", self.tx_height)?;
        writeln!(f, "From: {:?}", self.from)?;
        writeln!(f, "Amount: {}", self.amt)?;
        writeln!(f, "Receiver: {:?}", self.to)?;
        writeln!(f, "Is Valid: {}", self.is_valid)?;
        Ok(())
    }
}

pub async fn handle_transfer_operation(
    mongo_client: &MongoClient,
    block_height: u32,
    tx_height: u32,
    inscription: Brc20Inscription,
    raw_tx: GetRawTransactionResult,
    sender: Address,
    active_transfers: &mut Option<HashMap<(String, i64), Brc20ActiveTransfer>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a new transfer transaction
    let mut validated_transfer_tx =
        Brc20Transfer::new(raw_tx, inscription, block_height, tx_height, sender);

    // Handle the transfer inscription
    let _ = validated_transfer_tx
        .validate_inscribe_transfer(mongo_client, active_transfers)
        .await?;

    let from_address = validated_transfer_tx.from.clone();

    if validated_transfer_tx.is_valid() {
        info!(
            "Transfer: {:?}",
            validated_transfer_tx.get_transfer_script()
        );
        info!("From Address: {:?}", &from_address);

        // Add the valid transfer to the mongo database
        mongo_client
            .insert_document(
                consts::COLLECTION_TRANSFERS,
                validated_transfer_tx.to_document(),
            )
            .await?;
    }

    Ok(())
}

impl ToDocument for Brc20Transfer {
    fn to_document(&self) -> Document {
        doc! {
            "amt": self.amt,
            "block_height": self.block_height,
            "tx_height": self.tx_height,
            "tx": self.tx.to_document(), // Convert GetRawTransactionResult to document
            "inscription": self.inscription.to_document(),
            "send_tx": self.send_tx.clone().map(|tx| tx.to_document()), // Convert Option<GetRawTransactionResult> to document
            "from": self.from.to_string(),
            "to": self.to.clone().map(|addr| addr.to_string()), // Convert Option<Address> to string
            "is_valid": self.is_valid,
            "created_at": Bson::DateTime(DateTime::now())
        }
    }
}

impl ToDocument for Brc20ActiveTransfer {
    fn to_document(&self) -> Document {
        doc! {
            "txid": self.tx_id.to_string(),
            "vout": self.vout,
            "block_height": self.block_height,
            "created_at": Bson::DateTime(DateTime::now())
        }
    }
}

impl Brc20ActiveTransfer {
    pub fn from_document(document: Document) -> Result<Self, String> {
        let tx_id = document
            .get_str("tx_id")
            .map_err(|_| "Invalid txid".to_string())?
            .to_string();

        let vout = document
            .get_i64("vout")
            .map_err(|_| "Invalid vout".to_string())?;

        let block_height = document
            .get_i64("block_height")
            .map_err(|_| "Invalid block_height".to_string())?;

        Ok(Self {
            tx_id,
            vout,
            block_height,
        })
    }
}
