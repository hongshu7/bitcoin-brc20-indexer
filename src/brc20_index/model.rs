use mongodb::bson::{Document, DateTime, self, Bson};

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
    pub inscription: Inscription,
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

pub struct Invalid {
    // todo: SAM
}

/// Mongo collections
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

pub struct InscriptionDB {
    pub id: bson::oid,
    pub p: str,
    pub op: str,
    pub tick: str,
    pub amt: Option<str>,
    pub max: Option<str>,
    pub lim: Option<str>,
    pub dec: Option<str>,
    pub created_at: DateTime,
}

pub struct TransactionDB {
    pub id: bson::oid,
    pub tx: GetRawTransactionResult,
    pub created_at: DateTime,
}

pub struct Balance {
    // todo: SAM
}
