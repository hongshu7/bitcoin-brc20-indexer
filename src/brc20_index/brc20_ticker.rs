use super::{
    deploy::Brc20Deploy, mint::Brc20Mint, transfer::Brc20Transfer, user_balance::UserBalance,
    ToDocument,
};
use bitcoin::{Address, OutPoint};
use mongodb::bson::{doc, Bson, DateTime, Document};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct Brc20Ticker {
    pub tick: String,
    pub limit: f64,
    pub max_supply: f64,
    pub total_minted: f64,
    pub decimals: u8,
    pub deploy: Brc20Deploy,
    pub mints: Vec<Brc20Mint>,
    pub transfers: Vec<Brc20Transfer>,
    pub balances: HashMap<Address, UserBalance>,
}

impl ToDocument for Brc20Ticker {
    fn to_document(&self) -> Document {
        doc! {
            "tick": self.get_ticker().clone(),
            "limit": self.limit,
            "max_supply": self.max_supply,
            "decimals": self.decimals as i64,
            "total_minted": self.total_minted,
            "block_height": self.deploy.block_height,
            "created_at": Bson::DateTime(DateTime::now())
        }
    }
}

impl Brc20Ticker {
    pub fn new(deploy: Brc20Deploy) -> Brc20Ticker {
        let tick = deploy.get_deploy_script().tick.to_lowercase().clone();
        let limit = deploy.get_limit();
        let max_supply = deploy.get_max_supply();
        let decimals = deploy.get_decimals();

        Brc20Ticker {
            tick,
            limit,
            max_supply,
            total_minted: 0.0,
            decimals,
            deploy,
            mints: Vec::new(),
            transfers: Vec::new(),
            balances: HashMap::new(),
        }
    }

    // Checks whether any balance in this Brc20Ticker has an active transfer inscription
    // for the provided outpoint. Returns true if at least one balance does, false otherwise.
    pub fn has_active_transfer_inscription(&self, outpoint: &OutPoint) -> bool {
        self.balances.values().any(|balance| {
            balance
                .get_active_transfer_inscriptions()
                .contains_key(outpoint)
        })
    }

    // Searches for an active transfer inscription for the provided outpoint in all the balances.
    // If it finds it, it removes the inscription from the balance and returns the associated
    // Brc20TransferTx. If no such inscription is found, it returns None.
    pub fn get_and_remove_active_transfer_inscription(
        &mut self,
        outpoint: &OutPoint,
    ) -> Option<Brc20Transfer> {
        self.balances
            .values_mut()
            .find_map(|balance| balance.remove_inscription(outpoint))
    }

    // Updates the sender's balance after a transfer send operation. It either adds the transaction
    // to an existing balance or creates a new balance for the sender if it doesn't already exist.
    pub fn update_transfer_sends(&mut self, from: Address, tx: Brc20Transfer) {
        if let Some(user_balance) = self.balances.get_mut(&from) {
            user_balance.add_transfer_send(tx.clone());
        } else {
            let mut new_user_balance =
                UserBalance::new(from.to_string(), self.get_ticker().clone());
            new_user_balance.add_transfer_send(tx.clone());
            self.balances.insert(from.clone(), new_user_balance);
        }

        // log to console
        if let Some(user_balance) = self.balances.get(&from) {
            log::info!(
              "Transfer send from user {}: overall balance = {}, available balance = {}, transferable balance = {}",
              from,
              user_balance.get_overall_balance(),
              user_balance.get_available_balance(),
              user_balance.get_transferable_balance()
          );
        }
    }

    // Updates the receiver's balance after a transfer receive operation. It either adds the transaction
    // to an existing balance or creates a new balance for the receiver if it doesn't already exist.
    pub fn update_transfer_receives(&mut self, to: Address, tx: Brc20Transfer) {
        if let Some(user_balance) = self.balances.get_mut(&to) {
            user_balance.add_transfer_receive(tx.clone());
        } else {
            let mut new_user_balance = UserBalance::new(to.to_string(), self.get_ticker().clone());
            new_user_balance.add_transfer_receive(tx.clone());
            self.balances.insert(to.clone(), new_user_balance);
        }

        // log to console
        if let Some(user_balance) = self.balances.get(&to) {
            log::info!(
              "Transfer received for user {}: overall balance = {}, available balance = {}, transferable balance = {}",
              to,
              user_balance.get_overall_balance(),
              user_balance.get_available_balance(),
              user_balance.get_transferable_balance()
          );
        }
    }

    // A method to get a mutable reference to a user's balance
    pub fn get_user_balance_mut(&mut self, address: &Address) -> Option<&mut UserBalance> {
        self.balances.get_mut(address)
    }

    // get total_minted from mints
    pub fn get_total_supply(&self) -> f64 {
        self.mints.iter().map(|mint| mint.get_amount()).sum()
    }

    pub fn get_ticker(&self) -> String {
        self.tick.to_lowercase()
    }
}
