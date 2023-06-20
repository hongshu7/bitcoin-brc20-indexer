use std::collections::HashMap;

use bitcoin::{Address, OutPoint};
use serde::Serialize;

use super::{
    deploy::Brc20DeployTx, mint::Brc20MintTx, transfer::Brc20TransferTx, user_balance::UserBalance,
};

#[derive(Debug, Clone, Serialize)]
pub struct Brc20Ticker {
    ticker: String,
    limit: f64,
    max_supply: f64,
    total_minted: f64,
    decimals: u8,
    deploy_tx: Brc20DeployTx,
    mints: Vec<Brc20MintTx>,
    transfers: Vec<Brc20TransferTx>,
    balances: HashMap<Address, UserBalance>,
}

impl Brc20Ticker {
    pub fn new(deploy_tx: Brc20DeployTx) -> Brc20Ticker {
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

    pub fn has_active_transfer_inscription(&self, outpoint: &OutPoint) -> bool {
        self.balances.values().any(|balance| {
            balance
                .get_active_transfer_inscriptions()
                .contains_key(outpoint)
        })
    }

    // get and remove active transfer inscription from Brc20Ticker
    pub fn get_and_remove_active_transfer_inscription(
        &mut self,
        outpoint: &OutPoint,
    ) -> Option<Brc20TransferTx> {
        self.balances
            .values_mut()
            .find_map(|balance| balance.remove_inscription(outpoint))
    }

    pub fn update_transfer_sends(&mut self, sender: Address, tx: Brc20TransferTx) {
        if let Some(user_balance) = self.balances.get_mut(&sender) {
            user_balance.add_transfer_send(tx.clone());
        } else {
            let mut new_user_balance = UserBalance::new();
            new_user_balance.add_transfer_send(tx.clone());
            self.balances.insert(sender.clone(), new_user_balance);
        }

        if let Some(user_balance) = self.balances.get(&sender) {
            log::info!(
              "Updated transfer sends for user {}: overall balance = {}, available balance = {}, transferable balance = {}",
              sender,
              user_balance.get_overall_balance(),
              user_balance.get_available_balance(),
              user_balance.get_transferable_balance()
          );
        }
    }

    pub fn update_transfer_receives(&mut self, receiver: Address, tx: Brc20TransferTx) {
        if let Some(user_balance) = self.balances.get_mut(&receiver) {
            user_balance.add_transfer_receive(tx.clone());
        } else {
            let mut new_user_balance = UserBalance::new();
            new_user_balance.add_transfer_receive(tx.clone());
            self.balances.insert(receiver.clone(), new_user_balance);
        }

        if let Some(user_balance) = self.balances.get(&receiver) {
            log::info!(
              "Updated transfer receives for user {}: overall balance = {}, available balance = {}, transferable balance = {}",
              receiver,
              user_balance.get_overall_balance(),
              user_balance.get_available_balance(),
              user_balance.get_transferable_balance()
          );
        }
    }

    pub fn add_mint(&mut self, mint: Brc20MintTx) {
        let owner = mint.get_brc20_tx().get_owner().clone();
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
        }
    }

    pub fn add_user_balance(&mut self, address: Address, balance: UserBalance) {
        self.balances.insert(address, balance);
    }

    pub fn get_user_balance(&self, address: &Address) -> Option<&UserBalance> {
        self.balances.get(address)
    }

    // A method to get a mutable reference to a user's balance
    pub fn get_user_balance_mut(&mut self, address: &Address) -> Option<&mut UserBalance> {
        self.balances.get_mut(address)
    }

    pub fn get_total_supply(&self) -> f64 {
        self.total_minted
    }

    // get total_minted from mints
    pub fn get_total_minted_from_mint_txs(&self) -> f64 {
        self.mints.iter().map(|mint| mint.get_amount()).sum()
    }

    pub fn get_mint_txs(&self) -> &[Brc20MintTx] {
        &self.mints
    }

    pub fn get_transfer_txs(&self) -> &[Brc20TransferTx] {
        &self.transfers
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

    pub fn get_deploy_tx(&self) -> &Brc20DeployTx {
        &self.deploy_tx
    }

    // pub fn get_all_holders_with_balances(&self) -> Vec<(&Address, f64)> {
    //   self
    //     .balances
    //     .iter()
    //     .map(|(address, balance)| (address, balance.get_overall_balance()))
    //     .collect()
    // }

    pub fn display_brc20_ticker(&self) {
        println!("Deploy Transaction:\n{}", self.deploy_tx);

        println!("Mints:");
        for mint in &self.mints {
            println!("{}", mint);
        }

        println!("Transfers:");
        for transfer in &self.transfers {
            println!("{}", transfer);
        }

        println!("Total Minted: {}", self.total_minted);

        println!("Balances:");
        for (address, balance) in &self.balances {
            println!("Address: {}", address);
            println!("Overall Balance: {}", balance.get_overall_balance());

            println!("Active Transfer Inscriptions:");
            for (outpoint, transfer) in balance.get_active_transfer_inscriptions() {
                println!("OutPoint: {:?}", outpoint);
                println!("{}", transfer);
            }

            println!("=========================");
        }
    }
}
