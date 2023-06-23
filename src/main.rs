extern crate bitcoincore_rpc;
extern crate serde_json;
use bitcoincore_rpc::{Auth, Client};
use brc20_index::index_brc20;
// use consulrs::client::{ConsulClient, ConsulClientSettingsBuilder};
// use consulrs::kv;
// use serde_json::Value;
use dotenv::dotenv;
use log::info;
use std::env;

mod brc20_index;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger and load env variables
    dotenv().ok();
    env_logger::init();

    // let client = ConsulClient::new(
    //     ConsulClientSettingsBuilder::default()
    //         .address("http://localhost:8500")
    //         .build()
    //         .unwrap(),
    // )
    // .unwrap();
    // let mut res = kv::read(&client, "omnisat-api", None).await.unwrap();
    // let mykey: String = res
    //     .response
    //     .pop()
    //     .unwrap()
    //     .value
    //     .unwrap()
    //     .try_into()
    //     .unwrap();
    // let json_value: Value = serde_json::from_str(&mykey).unwrap();

    // let rpc_url = json_value
    //     .get("btc_rpc_host")
    //     .unwrap_or_else(|| panic!("BTC_RPC_HOST IS NOT SET"));

    // let rpc_user = json_value
    //     .get("btc_rpc_user")
    //     .unwrap_or_else(|| panic!("BTC_RPC_USER IS NOT SET"));

    // let rpc_password = json_value
    //     .get("btc_rpc_pass")
    //     .unwrap_or_else(|| panic!("BTC_RPC_PASSWORD IS NOT SET"));

    // println!("rpc_url: {}", rpc_url.to_string());
    // println!("rpc_user: {}", rpc_user.to_string());
    // println!("rpc_password: {}", rpc_password);

    // Retrieve the RPC url, user and password from environment variables
    let rpc_url = env::var("RPC_URL").unwrap();
    let rpc_user = env::var("RPC_USER").unwrap();
    let rpc_password = env::var("RPC_PASSWORD").unwrap();

    // Connect to Bitcoin Core RPC server
    let rpc = Client::new(
        &rpc_url.to_string(),
        Auth::UserPass(rpc_user.to_string(), rpc_password.to_string()),
    )?;
    info!("Connected to Bitcoin Core");

    // block height to start indexing from
    // TODO: get this from the database
    let start_block_height = 779832; // 779832 is starting block height for BRC20

    // LFG!
    index_brc20(&rpc, start_block_height).await?;

    Ok(())
}
