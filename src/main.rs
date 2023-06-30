use crate::brc20_index::{consts, mongo::MongoClient};
use bitcoincore_rpc;
use bitcoincore_rpc::{Auth, Client};
use brc20_index::index_brc20;
use consulrs::{
    client::{ConsulClient, ConsulClientSettingsBuilder},
    kv,
};
use dotenv::dotenv;
use log::{error, info};
use serde_json;
use serde_json::Value;
use std::env;

mod brc20_index;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger and load env variables
    dotenv().ok();
    env_logger::init();
    let consul_host = env::var("CONSUL_HOST").unwrap();
    let client = ConsulClient::new(
        ConsulClientSettingsBuilder::default()
            .address(consul_host)
            .build()
            .unwrap(),
    )
    .unwrap();
    let mut res = kv::read(&client, "omnisat-api", None).await.unwrap();
    let mykey: String = res
        .response
        .pop()
        .unwrap()
        .value
        .unwrap()
        .try_into()
        .unwrap();
    let json_value: Value = serde_json::from_str(&mykey).unwrap();

    let mut rpc_url = json_value
        .get("btc_rpc_host")
        .unwrap()
        .as_str()
        .unwrap_or_else(|| panic!("BTC_RPC_HOST IS NOT SET"));

    let mut rpc_user = json_value
        .get("btc_rpc_user")
        .unwrap()
        .as_str()
        .unwrap_or_else(|| panic!("BTC_RPC_USER IS NOT SET"));

    let mut rpc_password = json_value
        .get("btc_rpc_pass")
        .unwrap()
        .as_str()
        .unwrap_or_else(|| panic!("BTC_RPC_PASSWORD IS NOT SET"));

    let rpc_url_env = env::var("RPC_URL").unwrap();
    let rpc_user_env = env::var("RPC_USER").unwrap();
    let rpc_password_env = env::var("RPC_PASSWORD").unwrap();

    match env::var("ENV").unwrap().as_ref() {
        "workstation" => {
            rpc_url = rpc_url_env.as_str();
            rpc_user = rpc_user_env.as_str();
            rpc_password = rpc_password_env.as_str();
        }
        &_ => (),
    };

    // Connect to Bitcoin Core RPC server
    let rpc = Client::new(
        &rpc_url,
        Auth::UserPass(rpc_user.to_string(), rpc_password.to_string()),
    )?;
    info!("Connected to Bitcoin Core");

    //MongoDB connection
    //TODO: This is connecting to Consul to get the MONGO_HOST, this should be refactorered to get ALL
    // the variables we need from consul in one place in the codebase and set CONSTANT variables for these.

    let mongo_host = json_value
        .get("mongo_rc")
        .unwrap_or_else(|| panic!("MONGO_RC IS NOT SET"));

    let mongo_connection_str = format!(
        "mongodb://{}:27017,{}:27017,{}:27017/omnisat?replicaSet=rs0",
        mongo_host[0].as_str().unwrap(),
        mongo_host[1].as_str().unwrap(),
        mongo_host[2].as_str().unwrap(),
    );

    // Get the mongo host from environment variable if on local workstation
    let mongo_db_host = env::var("MONGO_DB_HOST");
    let mongo_connection_str = match mongo_db_host {
        Ok(host) => format!("mongodb://{}:27017", host),
        Err(_) => mongo_connection_str,
    };

    // let mongo_connection_str = "mongodb://localhost:27017"; // This uses localhost as mongo host
    // Get the mongo database name from environment variable
    let db_name = env::var("MONGO_DB_NAME").unwrap();
    let mongo_client = MongoClient::new(&mongo_connection_str, &db_name).await?;

    // get block height to start indexing from
    let mut start_block_height = consts::BRC20_STARTING_BLOCK_HEIGHT; // default starting point
    let last_completed_block = mongo_client
        .get_last_completed_block_height()
        .await
        .unwrap();
    if let Some(height) = last_completed_block {
        start_block_height = height + 1; // Start from the next block
    }
    info!(
        "Starting indexing from block height: {}",
        start_block_height
    );
    // if BRC20_STARTING_BLOCK_HEIGHT is < start_block_height, then we need to delete everything in db that is >= start_block_height
    // delete deploys, mints, transfers, inscriptions, tickers, invalids, entries
    if consts::BRC20_STARTING_BLOCK_HEIGHT < start_block_height {
        let collections = vec![
            consts::COLLECTION_DEPLOYS,
            consts::COLLECTION_MINTS,
            consts::COLLECTION_TRANSFERS,
            consts::COLLECTION_INVALIDS,
            consts::COLLECTION_TICKERS,
            consts::COLLECTION_USER_BALANCE_ENTRY,
            consts::COLLECTION_BRC20_ACTIVE_TRANSFERS,
            consts::COLLECTION_TOTAL_MINTED_AT_BLOCK_HEIGHT,
        ];

        for collection in collections {
            mongo_client
                .delete_from_collection(collection, start_block_height)
                .await?;
        }

        //delete user balance collection
        mongo_client
            .drop_collection(consts::COLLECTION_USER_BALANCES)
            .await?;

        //recalculate total_minted for each ticker
        info!("Recalculating total_minted for all tickers...");
        match mongo_client.update_ticker_totals(start_block_height).await {
            Ok(_) => info!("Recalculation complete."),
            Err(e) => info!("Error recalculating total_minted for all tickers: {:?}", e),
        };

        // rebuild userbalances
        info!("Recreating userbalances...");
        match mongo_client.rebuild_user_balances().await {
            Ok(_) => info!("Recreation complete."),
            Err(e) => info!("Error recreating userbalances: {:?}", e),
        };
    }

    // LFG!
    match index_brc20(&rpc, &mongo_client, start_block_height.try_into().unwrap()).await {
        Ok(_) => info!("Finished indexing BRC20 tokens"),
        Err(e) => error!("Error indexing BRC20 tokens: {:?}", e),
    };

    Ok(())
}
