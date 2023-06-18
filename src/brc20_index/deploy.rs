use std::{collections::HashMap, fmt};

use super::{brc20_ticker::Brc20Ticker, utils::convert_to_float, Brc20Inscription};
use serde::{Deserialize, Serialize};

use super::brc20_tx::{Brc20Tx, InvalidBrc20Tx, InvalidBrc20TxMap};

#[derive(Debug, Clone)]
pub struct Brc20DeployTx {
    max_supply: f64,
    limit: f64,
    decimals: u8,
    brc20_tx: Brc20Tx,
    deploy_script: Brc20Inscription,
    is_valid: bool,
}

impl Brc20DeployTx {
    pub fn new(brc20_tx: Brc20Tx, deploy_script: Brc20Inscription) -> Self {
        // populate with default values
        Brc20DeployTx {
            max_supply: 0.0,
            limit: 0.0,
            decimals: 18,
            brc20_tx,
            deploy_script,
            is_valid: false,
        }
    }

    // getters and setters
    pub fn get_max_supply(&self) -> f64 {
        self.max_supply
    }

    pub fn get_limit(&self) -> f64 {
        self.limit
    }

    pub fn get_decimals(&self) -> u8 {
        self.decimals
    }

    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    pub fn set_valid(mut self, is_valid: bool) -> Self {
        self.is_valid = is_valid;
        self
    }

    pub fn get_deploy_script(&self) -> &Brc20Inscription {
        &self.deploy_script
    }

    pub fn get_brc20_tx(&self) -> &Brc20Tx {
        &self.brc20_tx
    }

    pub fn validate_deploy_script(
        mut self,
        invalid_tx_map: &mut InvalidBrc20TxMap,
        ticker_map: &HashMap<String, Brc20Ticker>,
    ) -> Self {
        let ticker_symbol = self.deploy_script.tick.to_lowercase();

        let mut reasons = vec![];

        match self.validate_ticker_symbol(&ticker_symbol, ticker_map) {
            Ok(_) => {}
            Err(reason) => reasons.push(reason),
        }

        match self.validate_decimals_field() {
            Ok(_) => {}
            Err(reason) => reasons.push(reason),
        }

        match self.validate_max_field() {
            Ok(max) => {
                self.max_supply = max;
            }
            Err(reason) => reasons.push(reason),
        }

        match self.validate_limit_field(self.max_supply) {
            Ok(limit) => {
                self.limit = limit;
            }
            Err(reason) => reasons.push(reason),
        }

        let valid_deploy_tx = self.set_valid(reasons.is_empty());

        if !valid_deploy_tx.is_valid() {
            let reason = reasons.join("; ");
            let invalid_tx = InvalidBrc20Tx::new(valid_deploy_tx.get_brc20_tx().clone(), reason);
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
        } else if self.deploy_script.tick.chars().count() != 4 {
            Err("Ticker symbol must be 4 characters long".to_string())
        } else {
            Ok(())
        }
    }

    fn validate_decimals_field(&mut self) -> Result<(), String> {
        if let Some(decimals) = &self.deploy_script.dec {
            let parsed_decimals = match decimals.parse::<u8>() {
                Ok(value) => value,
                Err(_) => return Err("Decimals field must be a valid unsigned integer".to_string()),
            };

            if parsed_decimals > 18 {
                return Err("Decimals must be 18 or less".to_string());
            }

            self.decimals = parsed_decimals;
        }

        Ok(())
    }

    fn validate_max_field(&self) -> Result<f64, String> {
        match &self.deploy_script.max {
            Some(max_str) => match convert_to_float(max_str, self.decimals) {
                Ok(max) => {
                    if max > 0.0 && decimal_places(max) <= self.decimals.into() {
                        Ok(max)
                    } else {
                        Err("Max supply must be greater than 0 and the number of decimal places must not exceed the decimal value.".to_string())
                    }
                }
                Err(_) => Err("Max field must be a valid number.".to_string()),
            },
            None => Err("Max field is missing.".to_string()),
        }
    }

    fn validate_limit_field(&self, max: f64) -> Result<f64, String> {
        match &self.deploy_script.lim {
            Some(lim_str) => match convert_to_float(lim_str, self.decimals) {
                Ok(limit) => {
                    if limit <= max && decimal_places(limit) <= self.decimals.into() {
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

// A helper function to find out the decimal places of the given float
fn decimal_places(num: f64) -> u32 {
    let s = num.to_string();
    if let Some(pos) = s.find('.') {
        s[pos + 1..].len() as u32
    } else {
        0
    }
}

impl fmt::Display for Brc20DeployTx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Deploy Transaction:")?;
        writeln!(f, "{}", self.brc20_tx)?;
        writeln!(f, "Deploy Script: {:#?}", self.deploy_script)?;
        writeln!(f, "Is Valid: {}", self.is_valid)?;

        // Additional information based on the fields of Brc20DeployTx
        writeln!(f, "Max Supply: {}", self.max_supply)?;
        writeln!(f, "Limit: {}", self.limit)?;
        writeln!(f, "Decimals: {}", self.decimals)?;

        Ok(())
    }
}
