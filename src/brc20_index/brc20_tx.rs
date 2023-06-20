use bitcoin::{Address, OutPoint, Txid};
use bitcoincore_rpc::bitcoincore_rpc_json::{GetRawTransactionResult, GetRawTransactionResultVin};
use serde::Serialize;
use std::{collections::HashMap, fmt, fs::File, io::Write};

use super::Brc20Inscription;

/// Brc20Tx represents a transaction containing a BRC20 inscription
/// the owner is the address that owns the BRC20 inscribed satoshi
/// which is represented by the first satoshi of vout[0]
#[derive(Debug, Clone, Serialize)]
pub struct Brc20Tx {
    tx_id: Txid,
    vout: u32,
    blocktime: u64,
    blockheight: u32,
    owner: Address,
    inputs: Vec<GetRawTransactionResultVin>,
}

impl Brc20Tx {
    pub fn new(
        raw_tx_result: &GetRawTransactionResult,
        owner: Address,
        blockheight: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let tx_id = raw_tx_result.txid;
        let vout = raw_tx_result.vout[0].n;

        // Get the blocktime from the raw transaction result
        let blocktime = raw_tx_result
            .blocktime
            .ok_or_else(|| "Blocktime not found in raw transaction result")?;

        // Create the Brc20Tx instance
        let brc20_tx = Brc20Tx {
            tx_id,
            vout,
            blocktime: blocktime as u64,
            blockheight,
            owner,
            inputs: raw_tx_result.vin.clone(),
        };

        Ok(brc20_tx)
    }

    // Getters
    pub fn get_outpoint(&self) -> OutPoint {
        OutPoint {
            txid: self.tx_id,
            vout: self.vout,
        }
    }

    pub fn get_txid(&self) -> &Txid {
        &self.tx_id
    }

    pub fn get_owner(&self) -> &Address {
        &self.owner
    }

    pub fn get_blocktime(&self) -> u64 {
        self.blocktime
    }

    pub fn get_blockheight(&self) -> u32 {
        self.blockheight
    }

    pub fn get_inputs(&self) -> &Vec<GetRawTransactionResultVin> {
        &self.inputs
    }
}

impl fmt::Display for Brc20Tx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Transaction ID: {}", self.tx_id)?;
        writeln!(f, "Vout: {}", self.vout)?;
        writeln!(f, "Blocktime: {}", self.blocktime)?;
        writeln!(f, "Owner: {}", self.owner)?;
        writeln!(f, "Inputs: {:?}", self.inputs)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InvalidBrc20Tx {
    pub tx_id: Txid,
    pub inscription: Brc20Inscription,
    pub reason: String,
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

    pub fn dump_to_file(&self, path: &str) -> std::io::Result<()> {
        let mut file = File::create(path)?;

        // Convert the invalid transactions map to JSON
        let json = serde_json::to_string_pretty(&self.map)?;

        // Write to the file
        file.write_all(json.as_bytes())?;

        Ok(())
    }

    pub fn add_invalid_tx(&mut self, invalid_tx: InvalidBrc20Tx) {
        let tx_id = invalid_tx.tx_id;
        self.map.insert(tx_id, invalid_tx);
    }
}
