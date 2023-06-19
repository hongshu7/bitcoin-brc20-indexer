use std::{collections::HashMap, fmt};

use bitcoin::{Address, OutPoint};
use bitcoincore_rpc::bitcoincore_rpc_json::GetRawTransactionResult;
use serde::{Deserialize, Serialize};

use super::{
    brc20_ticker::Brc20Ticker,
    brc20_tx::{Brc20Tx, InvalidBrc20Tx, InvalidBrc20TxMap},
    Brc20Inscription,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Brc20Transfer {
    pub p: String,
    pub op: String,
    pub tick: String,
    pub amt: String,
}

#[derive(Debug, Clone)]
pub struct Brc20TransferTx {
    inscription_tx: Brc20Tx,
    transfer_tx: Option<GetRawTransactionResult>,
    transfer_script: Brc20Inscription,
    amount: f64,
    receiver: Option<Address>,
    is_valid: bool,
}

impl Brc20TransferTx {
    pub fn new(inscription_tx: Brc20Tx, transfer_script: Brc20Inscription) -> Self {
        let amount = transfer_script
            .amt
            .as_ref()
            .map(|amt_str| amt_str.parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);

        Brc20TransferTx {
            inscription_tx,
            transfer_tx: None,
            transfer_script,
            amount,
            receiver: None,
            is_valid: false,
        }
    }

    // getters and setters
    pub fn get_transfer_script(&self) -> &Brc20Inscription {
        &self.transfer_script
    }

    pub fn get_inscription_brc20_tx(&self) -> &Brc20Tx {
        &self.inscription_tx
    }

    // get transfer tx
    pub fn get_transfer_tx(&self) -> Option<&GetRawTransactionResult> {
        self.transfer_tx.as_ref()
    }

    // set transfer tx
    pub fn set_transfer_tx(mut self, transfer_tx: GetRawTransactionResult) -> Self {
        self.transfer_tx = Some(transfer_tx);
        self
    }

    // get OutPoint
    pub fn get_inscription_outpoint(&self) -> OutPoint {
        self.inscription_tx.get_outpoint()
    }

    //   pub fn get_receiver(&self) -> Option<&Address> {
    //     &self.receiver
    //   }

    pub fn get_amount(&self) -> f64 {
        self.amount
    }

    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    pub fn handle_inscribe_transfer_amount(
        &mut self,
        ticker_map: &mut HashMap<String, Brc20Ticker>,
        invalid_tx_map: &mut InvalidBrc20TxMap,
    ) {
        let owner = self.inscription_tx.get_owner();
        let ticker_symbol = &self.transfer_script.tick;

        if let Some(ticker) = ticker_map.get_mut(ticker_symbol) {
            if let Some(mut user_balance) = ticker.get_user_balance_mut(&owner) {
                let transfer_amount = self
                    .transfer_script
                    .amt
                    .as_ref()
                    .map(|amt_str| amt_str.parse::<f64>().unwrap_or(0.0))
                    .unwrap_or(0.0);

                let available_balance = user_balance.get_available_balance();

                if available_balance >= transfer_amount {
                    self.is_valid = true;
                    println!("VALID: Transfer inscription added. Owner: {:#?}", owner);

                    // Increase the transferable balance of the sender
                    user_balance.add_transfer_inscription(self.clone());
                } else {
                    let reason = "Transfer amount exceeds available balance".to_string();
                    let invalid_tx = InvalidBrc20Tx::new(self.inscription_tx.clone(), reason);
                    invalid_tx_map.add_invalid_tx(invalid_tx);
                }
            } else {
                let reason = "User balance not found".to_string();
                let invalid_tx = InvalidBrc20Tx::new(self.inscription_tx.clone(), reason);
                invalid_tx_map.add_invalid_tx(invalid_tx);
            }
        } else {
            let reason = "Ticker not found".to_string();
            let invalid_tx = InvalidBrc20Tx::new(self.inscription_tx.clone(), reason);
            invalid_tx_map.add_invalid_tx(invalid_tx);
        }
    }

    /// Sets the validity of the transfer.
    ///
    /// # Arguments
    ///
    /// * `is_valid` - A bool indicating the validity of the transfer.
    pub fn set_validity(mut self, is_valid: bool) -> Self {
        self.is_valid = is_valid;
        self
    }

    /// Sets the receiver address.
    ///
    /// # Arguments
    ///
    /// * `receiver` - An optional `Address` representing the receiver address.
    pub fn set_receiver(mut self, receiver: Address) -> Self {
        self.receiver = Some(receiver);
        self
    }
}

impl fmt::Display for Brc20TransferTx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Inscription Transaction: {}", self.inscription_tx)?;
        writeln!(f, "Transfer Transaction: {:?}", self.transfer_tx)?;
        writeln!(f, "Transfer Script: {:#?}", self.transfer_script)?;
        writeln!(f, "Amount: {}", self.amount)?;
        writeln!(f, "Receiver: {:?}", self.receiver)?;
        writeln!(f, "Is Valid: {}", self.is_valid)?;
        Ok(())
    }
}
