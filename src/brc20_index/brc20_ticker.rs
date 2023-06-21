use super::{
    deploy::Brc20Deploy, mint::Brc20Mint, transfer::Brc20Transfer, user_balance::UserBalance,
};
use bitcoin::{Address, OutPoint};
use serde::Serialize;
use std::collections::HashMap;

// methods implement various functionality for the Brc20Ticker, such as:
//  - checking if a user has an active transfer inscription
//  - getting and removing active transfer inscriptions
//  - updating transfer sends and receives
//  - adding mint transactions
//  - getting balances, mint transactions, transfer transactions, etc.
//  - adding user balance
//  - displaying BRC20 ticker info
#[derive(Debug, Clone, Serialize)]
pub struct Brc20Ticker {
    ticker: String,
    limit: f64,
    max_supply: f64,
    total_minted: f64,
    decimals: u8,
    deploy_tx: Brc20Deploy,
    mints: Vec<Brc20Mint>,
    transfers: Vec<Brc20Transfer>,
    balances: HashMap<Address, UserBalance>,
}

impl Brc20Ticker {
    pub fn new(deploy_tx: Brc20Deploy) -> Brc20Ticker {
        let ticker = deploy_tx.get_deploy_script().tick.clone();
        let limit = deploy_tx.get_limit();
        let max_supply = deploy_tx.get_max_supply();
        let decimals = deploy_tx.get_decimals();

        Brc20Ticker {
            ticker,
            limit,
            max_supply,
            total_minted: 0.0,
            decimals,
            deploy_tx,
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
    pub fn update_transfer_sends(&mut self, sender: Address, tx: Brc20Transfer) {
        if let Some(user_balance) = self.balances.get_mut(&sender) {
            user_balance.add_transfer_send(tx.clone());
        } else {
            let mut new_user_balance = UserBalance::new();
            new_user_balance.add_transfer_send(tx.clone());
            self.balances.insert(sender.clone(), new_user_balance);
        }

        // log to console
        if let Some(user_balance) = self.balances.get(&sender) {
            log::info!(
              "Transfer send from user {}: overall balance = {}, available balance = {}, transferable balance = {}",
              sender,
              user_balance.get_overall_balance(),
              user_balance.get_available_balance(),
              user_balance.get_transferable_balance()
          );
        }
    }

    // Updates the receiver's balance after a transfer receive operation. It either adds the transaction
    // to an existing balance or creates a new balance for the receiver if it doesn't already exist.
    pub fn update_transfer_receives(&mut self, receiver: Address, tx: Brc20Transfer) {
        if let Some(user_balance) = self.balances.get_mut(&receiver) {
            user_balance.add_transfer_receive(tx.clone());
        } else {
            let mut new_user_balance = UserBalance::new();
            new_user_balance.add_transfer_receive(tx.clone());
            self.balances.insert(receiver.clone(), new_user_balance);
        }

        // log to console
        if let Some(user_balance) = self.balances.get(&receiver) {
            log::info!(
              "Transfer received for user {}: overall balance = {}, available balance = {}, transferable balance = {}",
              receiver,
              user_balance.get_overall_balance(),
              user_balance.get_available_balance(),
              user_balance.get_transferable_balance()
          );
        }
    }

    // Adds a mint transaction to the owner's balance. If the owner's balance doesn't exist yet, it
    // creates a new one. Also updates the total minted tokens for this Brc20Ticker.
    pub fn add_mint(&mut self, mint: Brc20Mint) {
        let owner = mint.owner.clone();
        // add mint to UserBalance
        if let Some(balance) = self.balances.get_mut(&owner) {
            balance.add_mint_tx(mint.clone());
        } else {
            let mut user_balance = UserBalance::new();
            user_balance.add_mint_tx(mint.clone());
            self.balances.insert(owner.clone(), user_balance);
        }
        self.mints.push(mint);

        if let Some(user_balance) = self.get_user_balance(&owner) {
            log::info!(
              "Minted tokens for user {}: overall balance = {}, available balance = {}, transferable balance = {}",
              owner,
              user_balance.get_overall_balance(),
              user_balance.get_available_balance(),
              user_balance.get_transferable_balance()
          );
            log::info!(
                "Total minted tokens for ticker {}:  {}",
                self.ticker,
                self.get_total_supply()
            );
        }
    }

    // get balances
    pub fn get_balances(&self) -> &HashMap<Address, UserBalance> {
        &self.balances
    }

    pub fn get_user_balance(&self, address: &Address) -> Option<&UserBalance> {
        self.balances.get(address)
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
        self.ticker.to_lowercase()
    }

    pub fn get_decimals(&self) -> u8 {
        self.decimals
    }

    pub fn get_limit(&self) -> f64 {
        self.limit
    }

    pub fn get_max_supply(&self) -> f64 {
        self.max_supply
    }
}
