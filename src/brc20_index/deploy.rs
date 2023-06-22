use super::invalid_brc20::{InvalidBrc20Tx, InvalidBrc20TxMap};
use super::Brc20Index;
use super::{brc20_ticker::Brc20Ticker, utils::convert_to_float, Brc20Inscription};
use bitcoin::Address;
use bitcoincore_rpc::bitcoincore_rpc_json::GetRawTransactionResult;
use log::{error, info};
use serde::Serialize;
use std::{collections::HashMap, fmt};

#[derive(Debug, Clone, Serialize)]
pub struct Brc20Deploy {
    pub max: f64,
    pub lim: f64,
    pub dec: u8,
    pub block_height: u32,
    pub tx_height: u32,
    pub owner: Address,
    pub tx: GetRawTransactionResult,
    pub inscription: Brc20Inscription,
    pub is_valid: bool,
}

impl Brc20Deploy {
    pub fn new(
        tx: GetRawTransactionResult,
        inscription: Brc20Inscription,
        block_height: u32,
        tx_height: u32,
        owner: Address,
    ) -> Self {
        // populate with default values
        Brc20Deploy {
            max: 0.0,
            lim: 0.0,
            dec: 18,
            block_height,
            tx_height,
            owner,
            tx,
            inscription,
            is_valid: false,
        }
    }

    // getters and setters
    pub fn get_max_supply(&self) -> f64 {
        self.max
    }

    pub fn get_limit(&self) -> f64 {
        self.lim
    }

    pub fn get_decimals(&self) -> u8 {
        self.dec
    }

    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    pub fn set_valid(mut self, is_valid: bool) -> Self {
        self.is_valid = is_valid;
        self
    }

    pub fn get_deploy_script(&self) -> &Brc20Inscription {
        &self.inscription
    }

    pub fn get_raw_tx(&self) -> &GetRawTransactionResult {
        &self.tx
    }

    pub fn validate_deploy_script(
        mut self,
        invalid_tx_map: &mut InvalidBrc20TxMap,
        ticker_map: &HashMap<String, Brc20Ticker>,
    ) -> Self {
        let ticker_symbol = self.inscription.tick.to_lowercase();

        let mut reasons = vec![];

        match self.validate_ticker_symbol(&ticker_symbol, ticker_map) {
            Ok(_) => {}
            Err(reason) => {
                error!("INVALID: {}", reason);
                reasons.push(reason)
            }
        }

        match self.validate_decimals_field() {
            Ok(_) => {}
            Err(reason) => {
                error!("INVALID: {}", reason);
                reasons.push(reason)
            }
        }

        match self.validate_max_field() {
            Ok(max) => {
                self.max = max;
            }
            Err(reason) => {
                error!("INVALID: {}", reason);
                reasons.push(reason)
            }
        }

        match self.validate_limit_field(self.max) {
            Ok(limit) => {
                self.lim = limit;
            }
            Err(reason) => {
                error!("INVALID: {}", reason);
                reasons.push(reason)
            }
        }

        let valid_deploy_tx = self.set_valid(reasons.is_empty());

        if !valid_deploy_tx.is_valid() {
            let reason = reasons.join("; ");
            let invalid_tx = InvalidBrc20Tx::new(
                valid_deploy_tx.tx.txid,
                valid_deploy_tx.inscription.clone(),
                reason,
            );
            invalid_tx_map.add_invalid_tx(invalid_tx);
        }

        valid_deploy_tx
    }

    fn validate_ticker_symbol(
        &self,
        ticker_symbol: &String,
        ticker_map: &HashMap<String, Brc20Ticker>,
    ) -> Result<(), String> {
        if ticker_map.contains_key(ticker_symbol) {
            Err("Ticker symbol already exists".to_string())
        } else if self.inscription.tick.chars().count() != 4 {
            Err("Ticker symbol must be 4 characters long".to_string())
        } else {
            Ok(())
        }
    }

    fn validate_decimals_field(&mut self) -> Result<(), String> {
        if let Some(decimals) = &self.inscription.dec {
            let parsed_decimals = match decimals.parse::<u8>() {
                Ok(value) => value,
                Err(_) => {
                    return Err("Decimals field must be a valid unsigned integer".to_string());
                }
            };

            if parsed_decimals > 18 {
                return Err("Decimals must be 18 or less".to_string());
            }

            self.dec = parsed_decimals;
        }

        Ok(())
    }

    fn validate_max_field(&self) -> Result<f64, String> {
        match &self.inscription.max {
            Some(max_str) => match convert_to_float(max_str, self.dec) {
                Ok(max) => {
                    if max > 0.0 && decimal_places(max) <= self.dec.into() {
                        Ok(max)
                    } else {
                        Err("Max supply must be greater than 0 and the number of decimal places must not exceed the decimal value.".to_string())
                    }
                }
                Err(_) => Err("Max field must be a valid number.".to_string()),
            },
            None => {
                error!("Max field is missing.");
                Err("Max field is missing.".to_string())
            }
        }
    }

    fn validate_limit_field(&self, max: f64) -> Result<f64, String> {
        match &self.inscription.lim {
            Some(lim_str) => match convert_to_float(lim_str, self.dec) {
                Ok(limit) => {
                    if limit <= max && decimal_places(limit) <= self.dec.into() {
                        Ok(limit)
                    } else {
                        Err("Limit must be less than or equal to max supply and the number of decimal places must not exceed the decimal value.".to_string())
                    }
                }
                Err(_) => Err("Limit field must be a valid number.".to_string()),
            },
            None => Ok(max),
        }
    }
}

pub fn handle_deploy_operation(
    inscription: Brc20Inscription,
    raw_tx: GetRawTransactionResult,
    owner: Address,
    block_height: u32,
    tx_height: u32,
    brc20_index: &mut Brc20Index,
) -> Result<(), Box<dyn std::error::Error>> {
    let validated_deploy_tx = Brc20Deploy::new(raw_tx, inscription, block_height, tx_height, owner)
        .validate_deploy_script(&mut brc20_index.invalid_tx_map, &brc20_index.tickers);

    if validated_deploy_tx.is_valid() {
        info!("Deploy: {:?}", validated_deploy_tx.get_deploy_script());

        // Instantiate a new `Brc20Ticker` struct and update the hashmap with the deploy information.
        let ticker = Brc20Ticker::new(validated_deploy_tx);
        brc20_index.tickers.insert(ticker.get_ticker(), ticker);
    }
    Ok(())
}

// A helper function to find out the decimal places of the given float
fn decimal_places(num: f64) -> u32 {
    let s = num.to_string();
    if let Some(pos) = s.find('.') {
        s[pos + 1..].len() as u32
    } else {
        0
    }
}

impl fmt::Display for Brc20Deploy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Deploy TransactionId:")?;
        writeln!(f, "{}", self.get_raw_tx().txid)?;
        writeln!(f, "Deploy Script: {:#?}", self.inscription)?;
        writeln!(f, "Is Valid: {}", self.is_valid)?;

        // Additional information based on the fields of Brc20DeployTx
        writeln!(f, "Max Supply: {}", self.max)?;
        writeln!(f, "Limit: {}", self.lim)?;
        writeln!(f, "Decimals: {}", self.dec)?;

        Ok(())
    }
}
