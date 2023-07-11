use super::{
    consts, invalid_brc20::InvalidBrc20Tx, mongo::MongoClient, user_balance::UserBalanceEntry,
    Brc20Inscription,
};
use crate::brc20_index::{
    user_balance::UserBalanceEntryType, utils::update_sender_or_inscriber_user_balance_document,
    ToDocument,
};
use bitcoin::Address;
use bitcoincore_rpc::bitcoincore_rpc_json::GetRawTransactionResult;
use log::{error, info};
use mongodb::bson::{doc, Bson, DateTime, Document};
use serde::Serialize;
use std::collections::HashMap;

// create active transfer struct
#[derive(Debug, Clone, Serialize)]
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
    pub send_block_height: Option<u32>,
    pub send_tx_height: Option<u32>,
    pub from: Address,
    pub to: Option<Address>,
    pub is_valid: bool,
}

impl Brc20Transfer {
    pub fn new(
        inscription_tx: &GetRawTransactionResult,
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
            tx: inscription_tx.clone(),
            send_tx: None,
            send_block_height: None,
            send_tx_height: None,
            inscription,
            from,
            to: None,
            is_valid: false,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    pub async fn validate_inscribe_transfer(
        &mut self,
        mongo_client: &MongoClient,
        active_transfers: &mut Option<HashMap<(String, i64), Brc20ActiveTransfer>>,
        user_balances: &mut HashMap<(String, String), Document>,
        user_balances_to_insert: &mut HashMap<(String, String), Document>,
        invalid_brc20_docs: &mut Vec<Document>,
    ) -> Result<UserBalanceEntry, Box<dyn std::error::Error>> {
        let ticker_symbol = &self.inscription.tick.to_lowercase();
        let mut user_balance_entry = UserBalanceEntry::default();
        let from = &self.from.to_string();

        // Get the ticker document from MongoDB
        let ticker_doc_from_mongo = mongo_client
            .get_document_by_field(consts::COLLECTION_TICKERS, "tick", ticker_symbol)
            .await?;

        if ticker_doc_from_mongo.is_none() {
            // Ticker not found, create invalid transaction
            let reason = "Ticker not found";
            error!("INVALID Transfer Inscribe: {}", reason);

            self.insert_invalid_tx(reason, invalid_brc20_docs).await?;

            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                reason,
            )));
        }

        // Get the user balance document from the hashmap
        let user_balance_from = user_balances.get_mut(&(from.clone(), ticker_symbol.clone()));

        let user_balance = match user_balance_from {
            Some(user_balance) => user_balance,
            None => {
                // User balance not found in the hashmap, check the 'user_balances_to_insert' hashmap
                let user_balance =
                    user_balances_to_insert.get_mut(&(from.clone(), ticker_symbol.clone()));

                match user_balance {
                    Some(user_balance) => user_balance,
                    None => {
                        // User balance not found in either hashmap, load it from the database
                        let user_balance_doc = mongo_client
                            .load_user_balance_with_retry(&(from.clone(), ticker_symbol.clone()))
                            .await
                            .map_err(|e| anyhow::anyhow!("{}", e))?;

                        if let Some(doc) = user_balance_doc {
                            user_balances
                                .insert((from.clone(), ticker_symbol.clone()), doc.clone());
                            user_balances
                                .get_mut(&(from.clone(), ticker_symbol.clone()))
                                .unwrap()
                        } else {
                            // User balance not found in the database either
                            let reason = "User balance not found";
                            error!("INVALID: {}", reason);

                            self.insert_invalid_tx(reason, invalid_brc20_docs).await?;

                            return Err(Box::new(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                reason,
                            )));
                        }
                    }
                }
            }
        };

        info!(
            "user_balance from validate_inscribe_transfer: {:?}",
            user_balance
        );

        let available_balance = mongo_client
            .get_double(&user_balance, "available_balance")
            .unwrap_or_default();

        // Get transfer amount
        let transfer_amount = self
            .inscription
            .amt
            .as_ref()
            .and_then(|amt_str| amt_str.parse::<f64>().ok())
            .unwrap_or(0.0);

        // Check if the user has enough balance to transfer
        if available_balance >= transfer_amount {
            info!("VALID: Transfer inscription added. From: {:?}", self.from);
            self.is_valid = true;

            // Insert user balance entry
            user_balance_entry = mongo_client
                .insert_user_balance_entry(
                    &self.from.to_string(),
                    transfer_amount,
                    ticker_symbol,
                    self.block_height.into(),
                    UserBalanceEntryType::Inscription,
                )
                .await?;

            // Update the user balance document
            update_sender_or_inscriber_user_balance_document(user_balance, &user_balance_entry)?;

            // Create a new active transfer when the inscription is valid
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
            // If invalid, add invalid tx and return
            let reason = "Transfer amount exceeds available balance";
            error!("INVALID: {}", reason);

            self.insert_invalid_tx(reason, invalid_brc20_docs).await?;
        }

        Ok(user_balance_entry)
    }

    pub async fn insert_invalid_tx(
        &self,
        reason: &str,
        invalid_brc20_docs: &mut Vec<Document>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let invalid_tx = InvalidBrc20Tx::new(
            self.tx.txid,
            self.inscription.clone(),
            reason.to_string(),
            self.block_height,
        );

        invalid_brc20_docs.push(invalid_tx.to_document());

        Ok(())
    }
}

pub async fn handle_transfer_operation(
    mongo_client: &MongoClient,
    block_height: u32,
    tx_height: u32,
    inscription: Brc20Inscription,
    raw_tx: &GetRawTransactionResult,
    sender: Address,
    active_transfers: &mut Option<HashMap<(String, i64), Brc20ActiveTransfer>>,
    user_balances: &mut HashMap<(String, String), Document>,
    user_balances_to_insert: &mut HashMap<(String, String), Document>,
    invalid_brc20_docs: &mut Vec<Document>,
) -> Result<(Brc20Transfer, UserBalanceEntry), Box<dyn std::error::Error>> {
    // Create a new transfer transaction
    let mut validated_transfer_tx =
        Brc20Transfer::new(raw_tx, inscription, block_height, tx_height, sender);

    // Handle the transfer inscription
    let user_balance_entry = validated_transfer_tx
        .validate_inscribe_transfer(
            mongo_client,
            active_transfers,
            user_balances,
            user_balances_to_insert,
            invalid_brc20_docs,
        )
        .await?;

    Ok((validated_transfer_tx, user_balance_entry))
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
            "send_block_height": self.send_block_height,
            "send_tx_height": self.send_tx_height,
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
