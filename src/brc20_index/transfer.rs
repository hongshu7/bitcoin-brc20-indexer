use super::{
    consts, invalid_brc20::InvalidBrc20Tx, mongo::MongoClient, Brc20Index, Brc20Inscription,
};
use crate::brc20_index::ToDocument;
use bitcoin::{Address, OutPoint};
use bitcoincore_rpc::bitcoincore_rpc_json::GetRawTransactionResult;
use log::{error, info};
use mongodb::bson::{doc, Document};
use serde::Serialize;
use std::fmt;

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

    // set transfer tx
    pub fn set_transfer_tx(mut self, transfer_tx: GetRawTransactionResult) -> Self {
        self.send_tx = Some(transfer_tx);
        self
    }

    // get OutPoint
    pub fn get_inscription_outpoint(&self) -> OutPoint {
        OutPoint {
            txid: self.tx.txid.clone(),
            vout: 0,
        }
    }

    pub fn get_amount(&self) -> f64 {
        self.amt
    }

    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    pub async fn handle_inscribe_transfer(
        &mut self,
        index: &mut Brc20Index,
        mongo_client: &MongoClient,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let from = &self.from;
        let ticker_symbol = &self.inscription.tick;

        let ticker = match index.get_ticker_mut(ticker_symbol) {
            Some(ticker) => ticker,
            None => {
                let reason = "Ticker not found".to_string();
                error!("INVALID: {}", reason);
                index.invalid_tx_map.add_invalid_tx(InvalidBrc20Tx::new(
                    self.tx.txid,
                    self.inscription.clone(),
                    reason.clone(),
                ));
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    reason,
                )));
            }
        };

        let user_balance = match ticker.get_user_balance_mut(&from) {
            Some(balance) => balance,
            None => {
                let reason = "User balance not found".to_string();
                error!("INVALID: {}", reason);
                index.invalid_tx_map.add_invalid_tx(InvalidBrc20Tx::new(
                    self.tx.txid,
                    self.inscription.clone(),
                    reason.clone(),
                ));
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    reason,
                )));
            }
        };

        let transfer_amount = self
            .inscription
            .amt
            .as_ref()
            .map(|amt_str| amt_str.parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);

        // print available balance
        println!(
            "Available balance: {}",
            user_balance.get_available_balance()
        );

        if user_balance.get_available_balance() >= transfer_amount {
            self.is_valid = true;
            println!("VALID: Transfer inscription added. From: {:#?}", from);

            // Increase the transferable balance of the sender
            user_balance.add_transfer_inscription(self.clone());

            mongo_client
                .update_transfer_inscriber_user_balance_document(
                    &from.to_string(),
                    transfer_amount,
                    ticker_symbol,
                )
                .await?;
        } else {
            let reason = "Transfer amount exceeds available balance".to_string();
            error!("INVALID: {}", reason);
            let invalid_tx = InvalidBrc20Tx::new(self.tx.txid, self.inscription.clone(), reason);
            index.invalid_tx_map.add_invalid_tx(invalid_tx.clone());

            // Insert the invalid deploy transaction into MongoDB
            mongo_client
                .insert_document(consts::COLLECTION_INVALIDS, invalid_tx.to_document())
                .await?;
        }

        Ok(())
    }

    pub fn set_receiver(mut self, receiver: Address) -> Self {
        self.to = Some(receiver);
        self
    }
}

impl fmt::Display for Brc20Transfer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Inscription TransactionId: {}", self.tx.txid)?;
        writeln!(f, "Transfer Transaction: {:?}", self.send_tx)?;
        writeln!(f, "Transfer Script: {:#?}", self.inscription)?;
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
    brc20_index: &mut Brc20Index,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut validated_transfer_tx =
        Brc20Transfer::new(raw_tx, inscription, block_height, tx_height, sender);
    let _ = validated_transfer_tx
        .handle_inscribe_transfer(brc20_index, mongo_client)
        .await?;

    let from_address = validated_transfer_tx.from.clone();

    brc20_index.update_active_transfer_inscription(
        validated_transfer_tx.get_inscription_outpoint(),
        validated_transfer_tx.get_transfer_script().tick.clone(),
    );

    if validated_transfer_tx.is_valid() {
        info!(
            "Transfer: {:?}",
            validated_transfer_tx.get_transfer_script()
        );
        info!("From Address: {:?}", &from_address);
    }

    // Add the transfer transaction to the mongo database
    mongo_client
        .insert_document(
            consts::COLLECTION_TRANSFERS,
            validated_transfer_tx.to_document(),
        )
        .await?;

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
            "to": self.to.clone().map(|addr| addr.to_string()), // Convert Option<Address> to string
            "is_valid": self.is_valid,
        }
    }
}
