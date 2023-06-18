extern crate bitcoincore_rpc;
extern crate serde_json;

use bitcoincore_rpc::{Auth, Client};
use brc20_index::index_brc20;
use dotenv::dotenv;
use log::info;
use std::env;

mod brc20_index;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set the RUST_LOG environment variable to enable info logs
    env::set_var("RUST_LOG", "info");

    // Initialize logger and load env variables
    env_logger::init();
    dotenv().ok();

    // Retrieve the RPC url, user and password from environment variables
    let rpc_url = env::var("RPC_URL").unwrap();
    let rpc_user = env::var("RPC_USER").unwrap();
    let rpc_password = env::var("RPC_PASSWORD").unwrap();

    // Connect to Bitcoin Core RPC server
    let rpc = Client::new(&rpc_url, Auth::UserPass(rpc_user, rpc_password))?;
    info!("Connected to Bitcoin Core");

    // block height to start indexing from
    let start_block_height = 779832;

    // LFG!
    index_brc20(&rpc, start_block_height)?;

    Ok(())
}
