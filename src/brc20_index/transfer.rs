use std::{collections::HashMap, fmt};

use bitcoin::{Address, OutPoint};
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
    transfer_tx: Option<Brc20Tx>,
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

    pub fn get_transfer_brc20_tx(&self) -> Option<Brc20Tx> {
        self.clone().transfer_tx
    }

    // get OutPoint
    pub fn get_inscription_outpoint(&self) -> OutPoint {
        self.inscription_tx.get_outpoint()
    }

    //   pub fn set_transfer_brc20_tx(mut self, transfer_tx: Brc20Tx) -> Self {
    //     self.transfer_tx = Some(transfer_tx);
    //     self
    //   }

    //   pub fn get_receiver(&self) -> Option<&Address> {
    //     &self.receiver
    //   }

    pub fn get_amount(&self) -> f64 {
        self.amount
    }

    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    /// Handles the processing of an inscribed transfer amount in a BRC20 transaction.
    ///
    /// Takes in mutable references to a ticker map and an invalid transaction map.
    /// The ticker map is a `HashMap` that maps a ticker symbol string to a BRC20Ticker object.
    /// The invalid transaction map is a map that associates invalid BRC20 transactions with a reason for their invalidity.
    ///
    /// Checks if the ticker symbol exists in the ticker map. If it does, it proceeds to get the transfer amount
    /// and then checks if the user balance exists in the BRC20Ticker object associated with the ticker symbol.
    ///
    /// If the user balance is found, it compares the available balance with the transfer amount.
    /// If the available balance is greater than or equal to the transfer amount, the transaction is valid,
    /// prints a validation message, and adds the transaction to the list of transfer inscriptions in the user balance.
    ///
    /// If the available balance is less than the transfer amount, it considers the transaction as invalid
    /// and sets the reason for invalidity as "Transfer amount exceeds available balance".
    ///
    /// If the user balance is not found, it sets the reason for invalidity as "User balance not found".
    ///
    /// If the ticker symbol is not found in the ticker map, it sets the reason for invalidity as "Ticker not found".
    ///
    /// If invalid, it creates a new `InvalidBrc20Tx` object,
    /// associates it with the reason for invalidity, and adds it to the invalid transaction map.
    ///
    /// The function returns a clone of the transfer transaction
    pub fn handle_inscribe_transfer_amount(
        &mut self,
        ticker_map: &mut HashMap<String, Brc20Ticker>,
        invalid_tx_map: &mut InvalidBrc20TxMap,
    ) {
        let owner = self.inscription_tx.get_owner();
        let ticker_symbol = &self.transfer_script.tick;

        if let Some(ticker) = ticker_map.get_mut(ticker_symbol) {
            if let Some(mut user_balance) = ticker.get_user_balance(&owner) {
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

    /// Sets the transfer transaction.
    ///
    /// # Arguments
    ///
    /// * `transfer_tx` - An optional `Brc20Tx` representing the transfer (second) transaction.
    pub fn set_transfer_tx(mut self, transfer_tx: Brc20Tx) -> Self {
        self.transfer_tx = Some(transfer_tx);
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
