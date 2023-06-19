use std::{collections::HashMap, fmt};

use super::{
    brc20_ticker::Brc20Ticker,
    brc20_tx::{Brc20Tx, InvalidBrc20Tx, InvalidBrc20TxMap},
    user_balance::UserBalance,
    utils::convert_to_float,
    Brc20Inscription,
};

impl Brc20MintTx {
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
            let ticker = ticker_map.get_mut(&brc20_mint_tx.mint.tick).unwrap();
            ticker.add_mint(brc20_mint_tx.clone());

            // Assume you have the user's address
            let user_address = brc20_mint_tx.get_brc20_tx().get_owner();
            if let Some(user_balance) = ticker.get_user_balance_mut(&user_address) {
                user_balance.add_mint_tx(brc20_mint_tx.clone());
            } else {
                let mut new_user_balance = UserBalance::new();
                new_user_balance.add_mint_tx(brc20_mint_tx.clone());
                ticker.add_user_balance(user_address.clone(), new_user_balance);
            }

            if let Some(user_balance) = ticker.get_user_balance(&user_address) {
                log::info!(
                    "Minted tokens for user {}: overall balance = {}, available balance = {}, transferable balance = {}",
                    user_address,
                    user_balance.get_overall_balance(),
                    user_balance.get_available_balance(),
                    user_balance.get_transferable_balance()
                );
            }
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
