use super::Brc20Inscription;
use bitcoin::Txid;
use serde::Serialize;
use std::{collections::HashMap, fmt, fs::File, io::Write};

// InvalidBrc20Tx represents an invalid BRC20 transaction,
// storing the id of the transaction, the faulty inscription and the reason why it's invalid.
#[derive(Debug, Clone, Serialize)]
pub struct InvalidBrc20Tx {
    tx_id: Txid,                   // The unique identifier of the invalid transaction
    inscription: Brc20Inscription, // The faulty inscription of the transaction
    reason: String,                // The reason why the transaction is invalid
}

impl InvalidBrc20Tx {
    pub fn new(tx_id: Txid, inscription: Brc20Inscription, reason: String) -> Self {
        InvalidBrc20Tx {
            tx_id,
            inscription,
            reason,
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

    // writes the invalid transactions map to a file at the provided path.
    // It converts the map to JSON before writing.
    pub fn dump_to_file(&self, path: &str) -> std::io::Result<()> {
        let mut file = File::create(path)?;

        // Convert the invalid transactions map to JSON
        let json = serde_json::to_string_pretty(&self.map)?;

        // Write to the file
        file.write_all(json.as_bytes())?;

        Ok(())
    }

    // adds an invalid transaction to the map.
    // uses the transaction id as the key to store the invalid transaction.
    pub fn add_invalid_tx(&mut self, invalid_tx: InvalidBrc20Tx) {
        let tx_id = invalid_tx.tx_id;
        self.map.insert(tx_id, invalid_tx);
    }
}
