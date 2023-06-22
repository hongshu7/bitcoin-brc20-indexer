extern crate bitcoincore_rpc;
extern crate serde_json;

use bitcoincore_rpc::{Auth, Client};
use brc20_index::index_brc20;
use dotenv::dotenv;
use log::info;
use std::env;

mod brc20_index;
mod mongo;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger and load env variables
    dotenv().ok();
    env_logger::init();

    // Retrieve the RPC url, user and password from environment variables
    let rpc_url = env::var("RPC_URL").unwrap();
    let rpc_user = env::var("RPC_USER").unwrap();
    let rpc_password = env::var("RPC_PASSWORD").unwrap();

    // Connect to Bitcoin Core RPC server
    let rpc = Client::new(&rpc_url, Auth::UserPass(rpc_user, rpc_password))?;
    info!("Connected to Bitcoin Core");

    // block height to start indexing from
    // TODO: get this from the database
    let start_block_height = 795300; // 779832 is starting block height for BRC20

    // LFG!
    index_brc20(&rpc, start_block_height).await?;

    Ok(())
}
