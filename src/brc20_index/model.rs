use bitcoincore_rpc::bitcoincore_rpc_json::GetRawTransactionResult;
use mongodb::bson::{self, Bson, DateTime, Document};

pub struct BRC20Ticker {
    pub tick: str,
    pub limit: f64,
    pub max_supply: f64,
    pub decimals: u8,
    pub deploy: Deploy,
    pub mints: Vec<Mint>,
    pub transfers: Vec<Transfer>,
    pub balances: HashMap<Address, UserBalance>,
}

pub struct BRC20Deploy {
    pub max: f64,
    pub lim: f64,
    pub dec: u8,
    pub block_height: u32,
    pub tx_height: u32,
    pub tx: GetRawTransactionResult,
    pub inscription: Brc20Inscription,
    pub is_valid: bool,
}

pub struct BRC20Mint {
    pub amt: f64,
    pub block_height: u32,
    pub tx_height: u32,
    pub block_time: u64,
    pub tx: GetRawTransactionResult,
    pub inscription: Inscription,
    pub is_valid: bool,
}

pub struct BRC20Transfer {
    pub amt: f64,
    pub block_height: u32,
    pub tx_height: u32,
    pub tx: GetRawTransactionResult,
    pub inscription: Inscription,
    pub send_tx: Option<GetRawTransactionResult>,
    pub receiver: Option<Address>,
    pub is_valid: bool,
}

pub struct BRC20Invalid {
    // todo: SAM
}

pub struct BRC20Inscription {
    pub p: str,
    pub op: str,
    pub tick: str,
    pub amt: Option<str>,
    pub max: Option<str>,
    pub lim: Option<str>,
    pub dec: Option<str>,
}

pub struct Balance {
    // todo: SAM
}

/// Mongo collections
///
///

const COLLECTION_TICKERS: str = "brc20_tickers";
const COLLECTION_DEPLOYS: str = "brc20_deploys";
const COLLECTION_MINTS: str = "brc20_mints";
const COLLECTION_TRANSFERS: str = "brc20_transfers";
const COLLECTION_INVALIDS: str = "brc20_invalids";
// const COLLECTION_INSCRIPTIONS: str = "inscriptions";
const COLLECTION_TRANSACTIONS: str = "transactions";

pub struct TickerDB {
    pub id: bson::oid,
    pub tick: str,
    pub limit: f64,
    pub max_supply: f64,
    pub decimals: u8,
    pub deploy: bson::oid,
    pub mints: Vec<bson::oid>,
    pub transfers: Vec<bson::oid>,
    pub invalids: Vex<bson::oid>,
    pub created_at: DateTime,
}

impl bson::ToDocument for TickerDB {
    fn to_document(&self) -> Document {
        doc! {
            "_id": self.id.clone(),
            "tick": self.tick.clone(),
            "limit": self.limit,
            "max_supply": self.max_supply,
            "decimals": self.decimals,
            "deploy": self.deploy.clone(),
            "mints": self.mints.clone(),
            "transfers": self.transfers.clone(),
            "invalids": self.invalids.clone(),
            "created_at": self.created_at.clone(),
        }
    }
}

pub struct DeployDB {
    pub id: bson::oid,
    pub ticker_id: bson::iod,
    pub max: f64,
    pub lim: f64,
    pub dec: u8,
    pub block_height: u32,
    pub tx_height: u32,
    pub tx: GetRawTransactionResult,
    pub inscription: Inscription,
    pub is_valid: bool,
    pub created_at: DateTime,
}

impl bson::ToDocument for DeployDB {
    fn to_document(&self) -> Document {
        doc! {
            "_id": self.id.clone(),
            "ticker_id": self.ticker_id.clone(),
            "max": self.max,
            "lim": self.lim,
            "dec": self.dec,
            "block_height": self.block_height,
            "tx_height": self.tx_height,
            "tx": self.tx.to_document(), // Convert GetRawTransactionResult to document
            "inscription": self.inscription.to_document(),
            "is_valid": self.is_valid,
            "created_at": self.created_at.clone(),
        }
    }
}

pub struct MintDB {
    pub id: bson::oid,
    pub ticker_id: bson::iod,
    pub amt: f64,
    pub block_height: u32,
    pub tx_height: u32,
    pub block_time: u64,
    pub tx: GetRawTransactionResult,
    pub inscription: Inscription,
    pub is_valid: bool,
    pub created_at: DateTime,
}

impl bson::ToDocument for MintDB {
    fn to_document(&self) -> Document {
        doc! {
            "_id": self.id.clone(),
            "ticker_id": self.ticker_id.clone(),
            "amt": self.amt,
            "block_height": self.block_height,
            "tx_height": self.tx_height,
            "block_time": self.block_time,
            "tx": self.tx.to_document(), // Convert GetRawTransactionResult to document
            "inscription": self.inscription.to_document(),
            "is_valid": self.is_valid,
            "created_at": self.created_at.clone(),
        }
    }
}

pub struct TransferDB {
    pub id: bson::oid,
    pub ticker_id: bson::iod,
    pub amt: f64,
    pub block_height: u32,
    pub tx_height: u32,
    pub tx: GetRawTransactionResult,
    pub inscription: Inscription,
    pub send_tx: Option<GetRawTransactionResult>,
    pub receiver: Option<Address>,
    pub is_valid: bool,
    pub created_at: DateTime,
}

impl bson::ToDocument for TransferDB {
    fn to_document(&self) -> Document {
        doc! {
            "_id": self.id.clone(),
            "ticker_id": self.ticker_id.clone(),
            "amt": self.amt,
            "block_height": self.block_height,
            "tx_height": self.tx_height,
            "tx": self.tx.to_document(), // Convert GetRawTransactionResult to document
            "inscription": self.inscription.to_document(),
            "send_tx": self.send_tx.clone().map(|tx| tx.to_document()), // Convert Option<GetRawTransactionResult> to document
            "receiver": self.receiver.clone(),
            "is_valid": self.is_valid,
            "created_at": self.created_at.clone(),
        }
    }
}

pub struct BRC20Invalid {
    pub id: bson::oid,
    pub ticker_id: bson::iod,
    pub created_at: DateTime,
}

pub struct InscriptionDB {
    pub p: str,
    pub op: str,
    pub tick: str,
    pub amt: Option<str>,
    pub max: Option<str>,
    pub lim: Option<str>,
    pub dec: Option<str>,
}

impl bson::ToDocument for InscriptionDB {
    fn to_document(&self) -> Document {
        doc! {
            "p": self.p.clone(),
            "op": self.op.clone(),
            "tick": self.tick.clone(),
            "amt": self.amt.clone(),
            "max": self.max.clone(),
            "lim": self.lim.clone(),
            "dec": self.dec.clone(),
        }
    }
}

pub struct TransactionDB {
    pub id: bson::oid,
    pub tx: GetRawTransactionResult,
    pub created_at: DateTime,
}

impl bson::ToDocument for TransactionDB {
    fn to_document(&self) -> Document {
        doc! {
            "_id": self.id.clone(),
            "tx": self.tx.to_document(), // Convert GetRawTransactionResult to document
            "created_at": self.created_at.clone(),
        }
    }
}

impl bson::ToDocument for GetRawTransactionResult {
    fn to_document(&self) -> Document {
        doc! {
            "in_active_chain": self.in_active_chain.clone(),
            "hex": self.hex.clone(),
            "txid": self.txid.to_string(),
            "hash": self.hash.to_string(),
            "size": self.size,
            "vsize": self.vsize,
            "version": self.version,
            "locktime": self.locktime,
            "vin": self.vin.iter().map(|vin| vin.to_document()).collect::<Vec<Document>>(),
            "vout": self.vout.iter().map(|vout| vout.to_document()).collect::<Vec<Document>>(),
            "blockhash": self.blockhash.clone().map(|blockhash| blockhash.to_string()),
            "confirmations": self.confirmations.clone(),
            "time": self.time.clone(),
            "blocktime": self.blocktime.clone(),
        }
    }
}

pub struct Balance {
    // todo: SAM
}
