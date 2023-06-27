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

    pub fn get_transferable_balance(&self) -> f64 {
        self.active_transfer_inscriptions
            .values()
            .map(|inscription| inscription.get_amount())
            .sum()
    }

    pub fn add_transfer_inscription(&mut self, transfer_inscription: Brc20Transfer) {
        self.active_transfer_inscriptions.insert(
            transfer_inscription.get_inscription_outpoint(),
            transfer_inscription.clone(),
        );
        self.set_balances();

        // display user overall balance
        println!("User overall balance: {}", self.get_overall_balance());
        // display user available balance
        println!("User available balance: {}", self.get_available_balance());
        // display user transferable balance
        println!(
            "User transferable balance: {}",
            self.get_transferable_balance()
        );
    }

    pub fn remove_inscription(&mut self, outpoint: &OutPoint) -> Option<Brc20Transfer> {
        self.active_transfer_inscriptions.remove(&outpoint)
    }

    // get active transfer inscriptions
    pub fn get_active_transfer_inscriptions(&self) -> &HashMap<OutPoint, Brc20Transfer> {
        &self.active_transfer_inscriptions
    }

    // get total amount of mints
    pub fn get_total_amount_from_mints(&self) -> f64 {
        self.mints.iter().map(|mint| mint.get_amount()).sum::<f64>()
    }

    // get overall balance using transfer sends, transfer receives and mints
    pub fn get_overall_balance(&self) -> f64 {
        self.get_total_amount_from_transfer_receives() - self.get_total_amount_from_transfer_sends()
            + self.get_total_amount_from_mints()
    }

    // get available balance using get_overall_balance_from_txs and active transfer inscriptions
    pub fn get_available_balance(&self) -> f64 {
        self.get_overall_balance() - self.get_transferable_balance()
    }

    // get total amount from transfer sends
    pub fn get_total_amount_from_transfer_sends(&self) -> f64 {
        self.transfer_sends
            .iter()
            .map(|transfer_send| transfer_send.get_amount())
            .sum()
    }

    // get total amount from transfer receives
    pub fn get_total_amount_from_transfer_receives(&self) -> f64 {
        self.transfer_receives
            .iter()
            .map(|transfer_receive| transfer_receive.get_amount())
            .sum()
    }

    pub fn add_transfer_send(&mut self, transfer_send: Brc20Transfer) {
        self.transfer_sends.push(transfer_send);
        self.set_balances();
    }

    pub fn add_transfer_receive(&mut self, transfer_receive: Brc20Transfer) {
        self.transfer_receives.push(transfer_receive);
        self.set_balances();
    }

    pub fn set_balances(&mut self) {
        self.overall_balance = self.get_overall_balance();
        self.available_balance = self.get_available_balance();
        self.transferable_balance = self.get_transferable_balance();
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

impl fmt::Display for UserBalance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Overall Balance: {}", self.get_overall_balance())?;
        writeln!(f, "Active Transfer Inscriptions:")?;
        for (outpoint, transfer_tx) in &self.active_transfer_inscriptions {
            writeln!(f, "OutPoint: {}\n{}", outpoint, transfer_tx)?;
        }
        Ok(())
    }
}
