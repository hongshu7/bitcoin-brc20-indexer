use super::{
    brc20_ticker::Brc20Ticker,
    invalid_brc20::{InvalidBrc20Tx, InvalidBrc20TxMap},
    utils::convert_to_float,
    Brc20Index, Brc20Inscription,
};
use bitcoin::Address;
use bitcoincore_rpc::bitcoincore_rpc_json::GetRawTransactionResult;
use log::{error, info};
use serde::Serialize;
use std::{collections::HashMap, fmt};

#[derive(Debug, Clone, Serialize)]
pub struct Brc20Mint {
    pub amt: f64,
    pub block_height: u32,
    pub tx_height: u32,
    pub to: Address,
    pub tx: GetRawTransactionResult,
    pub inscription: Brc20Inscription,
    pub is_valid: bool,
}

impl Brc20Mint {
    pub fn new(
        tx: &GetRawTransactionResult,
        inscription: Brc20Inscription,
        block_height: u32,
        tx_height: u32,
        to: Address,
    ) -> Self {
        Brc20Mint {
            amt: 0.0,
            block_height,
            tx_height,
            to,
            tx: tx.clone(),
            inscription,
            is_valid: false,
        }
    }

    pub fn get_amount(&self) -> f64 {
        self.amt
    }

    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    pub fn get_mint(&self) -> &Brc20Inscription {
        &self.inscription
    }

    pub fn validate_mint<'a>(
        mut self,
        ticker_map: &'a mut HashMap<String, Brc20Ticker>,
        invalid_tx_map: &'a mut InvalidBrc20TxMap,
    ) -> Brc20Mint {
        let mut is_valid = true;
        let mut reason = String::new();

        if let Some(ticker) = ticker_map.get(&self.inscription.tick) {
            let limit = ticker.get_limit();
            let max_supply = ticker.get_max_supply();
            let total_minted = ticker.get_total_supply();
            let amount = match self.inscription.amt.as_ref().map(String::as_str) {
                Some(amt_str) => convert_to_float(amt_str, ticker.get_decimals()),
                None => Ok(0.0), // Set a default value if the amount is not present
            };

            match amount {
                Ok(amount) => {
                    // Check if the amount is greater than the limit
                    if amount > limit {
                        is_valid = false;
                        reason = "Mint amount exceeds limit".to_string();
                        error!("Mint amount exceeds limit");
                    // Check if total minted is already greater than or equal to max supply
                    } else if total_minted >= max_supply {
                        is_valid = false;
                        reason = "Total minted is already at or exceeds max supply".to_string();
                        error!("Total minted is already at max supply");
                    // Check if the total minted amount + requested mint amount exceeds the max supply
                    } else if total_minted + amount > max_supply {
                        // Adjust the mint amount to mint the remaining tokens
                        let remaining_amount = max_supply - total_minted;
                        self.amt = remaining_amount;
                    } else {
                        self.amt = amount;
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
            error!("Ticker symbol does not exist");
        }

        if !is_valid {
            let invalid_tx = InvalidBrc20Tx::new(self.tx.txid, self.inscription.clone(), reason);
            invalid_tx_map.add_invalid_tx(invalid_tx);
        } else {
            // Set is_valid to true when the transaction is valid
            is_valid = true;
            let ticker = ticker_map.get_mut(&self.inscription.tick).unwrap();
            ticker.add_mint(self.clone());
        }

        self.is_valid = is_valid;
        self
    }
}

impl fmt::Display for Brc20Mint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Brc20 TransactionId: {}", self.tx.txid)?;
        writeln!(f, "Mint: {:#?}", self.inscription)?;
        writeln!(f, "Amount: {}", self.amt)?;
        writeln!(f, "Is Valid: {}", self.is_valid)?;
        Ok(())
    }
}

pub fn handle_mint_operation(
    block_height: u32,
    tx_height: u32,
    owner: Address,
    inscription: Brc20Inscription,
    raw_tx: &GetRawTransactionResult,
    brc20_index: &mut Brc20Index,
) -> Result<(), Box<dyn std::error::Error>> {
    let validated_mint_tx = Brc20Mint::new(&raw_tx, inscription, block_height, tx_height, owner)
        .validate_mint(&mut brc20_index.tickers, &mut brc20_index.invalid_tx_map);

    // Check if the mint operation is valid.
    if validated_mint_tx.is_valid() {
        info!("Mint: {:?}", validated_mint_tx.get_mint());
        info!("TO Address: {:?}", validated_mint_tx.to);
    }
    Ok(())
}
