use super::{invalid_brc20::InvalidBrc20Tx, Brc20Index, Brc20Inscription};
use bitcoin::{Address, OutPoint};
use bitcoincore_rpc::bitcoincore_rpc_json::GetRawTransactionResult;
use log::info;
use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Serialize)]
pub struct Brc20Transfer {
    pub amt: f64,
    pub block_height: u32,
    pub tx_height: u32,
    pub tx: GetRawTransactionResult,
    pub inscription: Brc20Inscription,
    pub send_tx: Option<GetRawTransactionResult>,
    pub from: Address,
    pub to: Option<Address>,
    pub is_valid: bool,
}

impl Brc20Transfer {
    pub fn new(
        inscription_tx: GetRawTransactionResult,
        inscription: Brc20Inscription,
        block_height: u32,
        tx_height: u32,
        from: Address,
    ) -> Self {
        let amt = inscription
            .amt
            .as_ref()
            .map(|amt_str| amt_str.parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);

        Brc20Transfer {
            amt,
            block_height,
            tx_height,
            tx: inscription_tx,
            send_tx: None,
            inscription,
            from,
            to: None,
            is_valid: false,
        }
    }

    // getters and setters
    pub fn get_transfer_script(&self) -> &Brc20Inscription {
        &self.inscription
    }

    // set transfer tx
    pub fn set_transfer_tx(mut self, transfer_tx: GetRawTransactionResult) -> Self {
        self.send_tx = Some(transfer_tx);
        self
    }

    // get OutPoint
    pub fn get_inscription_outpoint(&self) -> OutPoint {
        OutPoint {
            txid: self.tx.txid.clone(),
            vout: 0,
        }
    }

    pub fn get_amount(&self) -> f64 {
        self.amt
    }

    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    pub fn handle_inscribe_transfer_amount(&mut self, index: &mut Brc20Index) {
        let owner = &self.from;
        let ticker_symbol = &self.inscription.tick;

        if let Some(ticker) = index.get_ticker_mut(ticker_symbol) {
            if let Some(user_balance) = ticker.get_user_balance_mut(&owner) {
                let transfer_amount = self
                    .inscription
                    .amt
                    .as_ref()
                    .map(|amt_str| amt_str.parse::<f64>().unwrap_or(0.0))
                    .unwrap_or(0.0);

                let available_balance = user_balance.get_available_balance();

                if available_balance >= transfer_amount {
                    self.is_valid = true;
                    println!("VALID: Transfer inscription added. Owner: {:#?}", owner);

                    // Increase the transferable balance of the sender
                    user_balance.add_transfer_inscription(self.clone());
                } else {
                    let reason = "Transfer amount exceeds available balance".to_string();
                    let invalid_tx =
                        InvalidBrc20Tx::new(self.tx.txid, self.inscription.clone(), reason);
                    index.invalid_tx_map.add_invalid_tx(invalid_tx);
                }
            } else {
                let reason = "User balance not found".to_string();
                let invalid_tx =
                    InvalidBrc20Tx::new(self.tx.txid, self.inscription.clone(), reason);
                index.invalid_tx_map.add_invalid_tx(invalid_tx);
            }
        } else {
            let reason = "Ticker not found".to_string();
            let invalid_tx = InvalidBrc20Tx::new(self.tx.txid, self.inscription.clone(), reason);
            index.invalid_tx_map.add_invalid_tx(invalid_tx);
        }
    }

    pub fn set_receiver(mut self, receiver: Address) -> Self {
        self.to = Some(receiver);
        self
    }
}

impl fmt::Display for Brc20Transfer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Inscription TransactionId: {}", self.tx.txid)?;
        writeln!(f, "Transfer Transaction: {:?}", self.send_tx)?;
        writeln!(f, "Transfer Script: {:#?}", self.inscription)?;
        writeln!(f, "Amount: {}", self.amt)?;
        writeln!(f, "Receiver: {:?}", self.to)?;
        writeln!(f, "Is Valid: {}", self.is_valid)?;
        Ok(())
    }
}

pub fn handle_transfer_operation(
    block_height: u32,
    tx_height: u32,
    inscription: Brc20Inscription,
    raw_tx: GetRawTransactionResult,
    sender: Address,
    brc20_index: &mut Brc20Index,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut brc20_transfer_tx =
        Brc20Transfer::new(raw_tx, inscription, block_height, tx_height, sender);

    brc20_transfer_tx.handle_inscribe_transfer_amount(brc20_index);

    brc20_index.update_active_transfer_inscription(
        brc20_transfer_tx.get_inscription_outpoint(),
        brc20_transfer_tx.get_transfer_script().tick.clone(),
    );

    if brc20_transfer_tx.is_valid() {
        info!("Transfer: {:?}", brc20_transfer_tx.get_transfer_script());
        info!("Sender Address: {:?}", brc20_transfer_tx.from);
    }
    Ok(())
}
