use std::{collections::HashMap, fmt};

use serde::{Deserialize, Serialize};

use super::{
    brc20_ticker::Brc20Ticker,
    brc20_tx::{Brc20Tx, InvalidBrc20Tx, InvalidBrc20TxMap},
    utils::convert_to_float,
    Brc20Inscription,
};

impl Brc20MintTx {
    /// Validates a BRC20 mint transaction.
    ///
    /// The function takes in a reference to a BRC20 transaction, a mutable reference to a ticker map,
    /// and a mutable reference to an invalid transaction map.
    /// The ticker map is a `HashMap` that maps a ticker symbol string to a BRC20Ticker object.
    /// The invalid transaction map is a map that associates invalid BRC20 transactions with a reason for their invalidity.
    ///
    /// It begins by creating a new BRC20MintTx from the provided BRC20 transaction.
    ///
    /// Then, it checks if the ticker symbol for the mint operation exists in the ticker map.
    ///
    /// If it does, it gets the mint limit, maximum supply, total minted amount, and decimal places for the ticker,
    /// and then converts the amount to be minted into a floating point number using these values.
    ///
    /// If the amount to be minted exceeds the limit, it marks the transaction as invalid and sets the reason for invalidity
    /// as "Mint amount exceeds limit".
    ///
    /// If the sum of the total minted amount and the amount to be minted exceeds the maximum supply,
    /// it adjusts the mint amount to the remaining supply, and sets a warning reason.
    ///
    /// If there's an error in conversion (e.g., the amount is not a valid number), it marks the transaction as invalid
    /// and sets the reason for invalidity as the error message.
    ///
    /// If the ticker symbol does not exist in the ticker map, it marks the transaction as invalid
    /// and sets the reason for invalidity as "Ticker symbol does not exist".
    ///
    /// If the transaction is marked as invalid (i.e., the `is_valid` flag is `false`),
    /// it creates a new `InvalidBrc20Tx` object, associates it with the reason for invalidity,
    /// and adds it to the invalid transaction map.
    ///
    /// If the transaction is valid, it retrieves the ticker object from the ticker map again,
    /// and updates the ticker with the mint operation.
    ///
    /// Finally, it returns a `Brc20MintTx` object with the original transaction, the mint details,
    /// the computed or adjusted amount, and the validity flag.
    pub fn validate_mint<'a>(
        self,
        brc20_tx: &'a Brc20Tx,
        ticker_map: &'a mut HashMap<String, Brc20Ticker>,
        invalid_tx_map: &'a mut InvalidBrc20TxMap,
    ) -> Brc20MintTx {
        let mut is_valid = true;
        let mut reason = String::new();
        // instantiate new Brc20MintTx
        let mut brc20_mint_tx: Brc20MintTx = Brc20MintTx::new(brc20_tx, self.mint);

        if let Some(ticker) = ticker_map.get(&brc20_mint_tx.mint.tick) {
            let limit = ticker.get_limit();
            let max_supply = ticker.get_max_supply();
            let total_minted = ticker.get_total_supply();
            let amount = match brc20_mint_tx.mint.amt.as_ref().map(String::as_str) {
                Some(amt_str) => convert_to_float(amt_str, ticker.get_decimals()),
                None => Ok(0.0), // Set a default value if the amount is not present
            };

            match amount {
                Ok(amount) => {
                    // Check if the amount is greater than the limit
                    if amount > limit {
                        is_valid = false;
                        reason = "Mint amount exceeds limit".to_string();
                    } else if total_minted + amount > max_supply {
                        // Check if the total minted amount + requested mint amount exceeds the max supply
                        // Adjust the mint amount to mint the remaining tokens
                        let remaining_amount = max_supply - total_minted;
                        brc20_mint_tx.amount = remaining_amount;
                    } else {
                        brc20_mint_tx.amount = amount;
                    }
                }
                Err(e) => {
                    is_valid = false;
                    reason = e.to_string();
                }
            }
        } else {
            is_valid = false;
            reason = "Ticker symbol does not exist".to_string();
        }

        if !is_valid {
            let invalid_tx = InvalidBrc20Tx::new(brc20_mint_tx.get_brc20_tx().clone(), reason);
            invalid_tx_map.add_invalid_tx(invalid_tx);
        } else {
            // update the ticker
            let ticker = ticker_map.get_mut(&brc20_mint_tx.mint.tick).unwrap();

            // Update the ticker struct with the mint operation.
            ticker.add_mint(brc20_mint_tx.clone());
        }

        Brc20MintTx {
            brc20_tx: brc20_mint_tx.brc20_tx.clone(),
            mint: brc20_mint_tx.mint,
            amount: brc20_mint_tx.amount,
            is_valid,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Brc20MintTx {
    brc20_tx: Brc20Tx,
    mint: Brc20Inscription,
    amount: f64,
    is_valid: bool,
}

impl Brc20MintTx {
    pub fn new(brc20_tx: &Brc20Tx, mint: Brc20Inscription) -> Self {
        Brc20MintTx {
            brc20_tx: brc20_tx.clone(),
            mint,
            amount: 0.0,
            is_valid: false,
        }
    }

    pub fn get_amount(&self) -> f64 {
        self.amount
    }

    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    pub fn get_mint(&self) -> &Brc20Inscription {
        &self.mint
    }

    pub fn get_brc20_tx(&self) -> &Brc20Tx {
        &self.brc20_tx
    }
}

impl fmt::Display for Brc20MintTx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Brc20 Transaction: {}", self.brc20_tx)?;
        writeln!(f, "Mint: {:#?}", self.mint)?;
        writeln!(f, "Amount: {}", self.amount)?;
        writeln!(f, "Is Valid: {}", self.is_valid)?;
        Ok(())
    }
}
