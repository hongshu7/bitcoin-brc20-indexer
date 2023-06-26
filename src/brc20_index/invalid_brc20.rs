use super::{Brc20Inscription, ToDocument};
use bitcoin::Txid;
use mongodb::bson::{doc, Bson, DateTime, Document};
use serde::Serialize;
use std::{collections::HashMap, fmt};

// InvalidBrc20Tx represents an invalid BRC20 transaction,
// storing the id of the transaction, the faulty inscription and the reason why it's invalid.
#[derive(Debug, Clone, Serialize)]
pub struct InvalidBrc20Tx {
    tx_id: Txid,                   // The unique identifier of the invalid transaction
    inscription: Brc20Inscription, // The faulty inscription of the transaction
    reason: String,                // The reason why the transaction is invalid
    block_height: u32,             // The block height of the transaction
}

impl InvalidBrc20Tx {
    pub fn new(
        tx_id: Txid,
        inscription: Brc20Inscription,
        reason: String,
        block_height: u32,
    ) -> Self {
        InvalidBrc20Tx {
            tx_id,
            inscription,
            reason,
            block_height,
        }
    }
}

impl ToDocument for InvalidBrc20Tx {
    fn to_document(&self) -> Document {
        doc! {
            "tx_id": self.tx_id.to_string(),
            "inscription": self.inscription.to_document(),
            "reason": self.reason.clone(),
            "block_height": self.block_height,
            "created_at": Bson::DateTime(DateTime::now())
        }
    }
}

impl fmt::Display for InvalidBrc20Tx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Brc20 Transaction id: {}", self.tx_id)?;
        writeln!(f, "Inscription: {}", self.inscription)?;
        writeln!(f, "Reason: {}", self.reason)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InvalidBrc20TxMap {
    map: HashMap<Txid, InvalidBrc20Tx>,
}

impl InvalidBrc20TxMap {
    pub fn new() -> Self {
        InvalidBrc20TxMap {
            map: HashMap::new(),
        }
    }

    // adds an invalid transaction to the map.
    // uses the transaction id as the key to store the invalid transaction.
    pub fn add_invalid_tx(&mut self, invalid_tx: InvalidBrc20Tx) {
        let tx_id = invalid_tx.tx_id;
        self.map.insert(tx_id, invalid_tx);
    }
}
