use super::{mint::Brc20Mint, transfer::Brc20Transfer, ToDocument};
use bitcoin::OutPoint;
use mongodb::bson::{doc, Bson, DateTime, Document};
use serde::Serialize;
use std::{collections::HashMap, fmt};

#[derive(Debug, Clone, Serialize)]
pub struct UserBalance {
    pub address: String,
    pub tick: String,
    pub overall_balance: f64,
    pub available_balance: f64,
    pub transferable_balance: f64,
    active_transfer_inscriptions: HashMap<OutPoint, Brc20Transfer>,
    transfer_sends: Vec<Brc20Transfer>,
    transfer_receives: Vec<Brc20Transfer>,
    mints: Vec<Brc20Mint>,
}

impl ToDocument for UserBalance {
    fn to_document(&self) -> Document {
        doc! {
            "address": self.address.to_string(),
            "tick": self.tick.to_lowercase().clone(),
            "overall_balance": self.overall_balance,
            "available_balance": self.available_balance,
            "transferable_balance": self.transferable_balance,
            "created_at": Bson::DateTime(DateTime::now())
        }
    }
}

impl UserBalance {
    pub fn new(address: String, tick: String) -> Self {
        UserBalance {
            address,
            tick,
            overall_balance: 0.0,
            available_balance: 0.0,
            transferable_balance: 0.0,
            active_transfer_inscriptions: HashMap::new(),
            transfer_sends: Vec::new(),
            transfer_receives: Vec::new(),
            mints: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UserBalanceEntry {
    pub address: String,
    pub tick: String,
    pub block_height: u64,
    pub amt: f64,
    pub entry_type: String,
}

impl UserBalanceEntry {
    pub fn new(
        address: String,
        tick: String,
        block_height: u64,
        amount: f64,
        entry_type: UserBalanceEntryType,
    ) -> Self {
        let entry = UserBalanceEntry {
            address,
            tick,
            block_height,
            amt: amount,
            entry_type: entry_type.to_string(), // Convert enum variant to String using Display trait
        };
        entry
    }
}

impl ToDocument for UserBalanceEntry {
    fn to_document(&self) -> Document {
        doc! {
            "address": &self.address,
            "tick": &self.tick,
            "block_height": self.block_height as i64,
            "amt": self.amt,
            "entry_type": &self.entry_type,
        }
    }
}

use std::convert::From;

#[derive(Debug, Clone, Serialize)]
pub enum UserBalanceEntryType {
    Inscription,
    Send,
    Receive,
}

impl fmt::Display for UserBalanceEntryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UserBalanceEntryType::Inscription => write!(f, "inscription"),
            UserBalanceEntryType::Send => write!(f, "send"),
            UserBalanceEntryType::Receive => write!(f, "receive"),
        }
    }
}

impl From<&str> for UserBalanceEntryType {
    fn from(item: &str) -> Self {
        match item {
            "inscription" => UserBalanceEntryType::Inscription,
            "send" => UserBalanceEntryType::Send,
            "receive" => UserBalanceEntryType::Receive,
            _ => panic!("Invalid UserBalanceEntryType"), // Decide how to handle invalid input
        }
    }
}
