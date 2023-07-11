use crate::brc20_index::{consts, mongo::MongoClient};
use bitcoincore_rpc;
use bitcoincore_rpc::{Auth, Client};
use brc20_index::index_brc20;
use consulrs::{
    client::{ConsulClient, ConsulClientSettingsBuilder},
    kv,
};
use dotenv::dotenv;
use log::{error, info, warn};
use serde_json;
use serde_json::Value;
use std::env;
use std::time::Instant;

mod brc20_index;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    env_logger::init();

    // Variables for configuration
    let rpc_url: String;
    let rpc_user: String;
    let rpc_password: String;
    let mongo_connection_str: String;
    let mut mongo_direct_connection_str: String;
    let mongo_direct_connection;

    // Check for CONSUL_HOST environment variable
    if let Ok(consul_host) = env::var("CONSUL_HOST") {
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

        rpc_url = json_value
            .get("btc_rpc_host")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        rpc_user = json_value
            .get("btc_rpc_user")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        rpc_password = json_value
            .get("btc_rpc_pass")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        mongo_direct_connection_str = json_value
            .get("mongo_direct_connection")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        // mongo_direct_connection = mongo_direct_connection_str.to_lowercase() == "true";
        // let mongo_direct_connection_str_env = env::var("MONGO_DIRECT_CONNECTION").ok();
        if let Ok(mongo_direct_connection_str_env) = env::var("MONGO_DIRECT_CONNECTION") {
            mongo_direct_connection_str = mongo_direct_connection_str_env;
        }

        mongo_direct_connection = mongo_direct_connection_str.to_lowercase() == "true";
        //MongoDB connection string
        let mongo_host_consul = json_value.get("mongo_rc").unwrap().as_array().unwrap();
        let mongo_host_env = env::var("MONGO_DB_HOST").ok();

        mongo_connection_str = if let Some(mongo_host_env) = mongo_host_env {
            format!("mongodb://{}:27017", mongo_host_env)
        } else {
            format!(
                "mongodb://{}:27017,{}:27017,{}:27017/omnisat?replicaSet=rs0",
                mongo_host_consul[0].as_str().unwrap(),
                mongo_host_consul[1].as_str().unwrap(),
                mongo_host_consul[2].as_str().unwrap(),
            )
        };
    } else {
        mongo_direct_connection_str = env::var("MONGO_DIRECT_CONNECTION").unwrap();
        mongo_direct_connection = mongo_direct_connection_str.to_lowercase() == "true";

        // Pick up environment vars from .env file
        rpc_url = env::var("RPC_URL").unwrap();
        rpc_user = env::var("RPC_USER").unwrap();
        rpc_password = env::var("RPC_PASSWORD").unwrap();

        let mongo_user = env::var("MONGO_USER").ok();
        let mongo_password = env::var("MONGO_PASSWORD").ok();
        let mongo_db_host = env::var("MONGO_DB_HOST").unwrap();

        mongo_connection_str = if let (Some(user), Some(password)) = (mongo_user, mongo_password) {
            format!("mongodb://{}:{}@{}:27017", user, password, mongo_db_host)
        } else {
            format!("mongodb://{}:27017", mongo_db_host)
        };
    }

    // Connect to Bitcoin Core RPC server
    let rpc = Client::new(&rpc_url, Auth::UserPass(rpc_user, rpc_password))?;
    info!("Connected to Bitcoin Core");

    // Get the mongo database name from environment variable
    let db_name = env::var("MONGO_DB_NAME").unwrap();
    let mongo_client =
        MongoClient::new(&mongo_connection_str, &db_name, mongo_direct_connection).await?;

    // Call create_indexes after MongoClient has been initialized
    mongo_client.create_indexes().await?;

    let start = Instant::now();
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

    warn!("Retrieved starting block height: {:?}", start.elapsed());

    // if BRC20_STARTING_BLOCK_HEIGHT is < start_block_height, then we need to delete everything in db that is >= start_block_height
    // delete deploys, mints, transfers, inscriptions, tickers, invalids, entries
    if consts::BRC20_STARTING_BLOCK_HEIGHT < start_block_height {
        info!("Deleting incomplete records...");
        let start = Instant::now();

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

        warn!("Incomplete Block Records deleted: {:?}", start.elapsed());

        info!("Resetting total_minted for selected tickers...");
        let start = Instant::now();
        match mongo_client
            .reset_tickers_total_minted(start_block_height)
            .await
        {
            Ok(updated_tickers) => {
                println!("Reset total_minted for the following tickers:");
                for ticker in &updated_tickers {
                    println!("{}", ticker);
                }
                warn!("Reset total_minted for tickers in: {:?}", start.elapsed());
                updated_tickers
            }
            Err(e) => {
                error!("Error resetting total_minted for tickers: {:?}", e);
                Vec::new() // Return an empty vector if there is an error
            }
        };

        // let start = Instant::now();
        // //recalculate total_minted for each ticker
        // info!("Recalculating total_minted for all tickers...");
        // match mongo_client
        //     .update_ticker_totals(start_block_height - 1)
        //     .await
        // {
        //     Ok(_) => {
        //         warn!("Recalculation complete: {:?}", start.elapsed())
        //     }
        //     Err(e) => error!("Error recalculating total_minted for all tickers: {:?}", e),
        // };

        info!("Deleting User Balances...");
        let start = Instant::now();
        //delete user balance collection
        let deleted_user_balances = mongo_client
            .delete_user_balances_by_block_height(start_block_height)
            .await;
        info!("Deleted User Balances: {:?}", deleted_user_balances);

        warn!("User Balances Deleted: {:?}", start.elapsed());

        // rebuild userbalances
        info!("Rebuilding User Balances...");
        let start = Instant::now();
        match deleted_user_balances {
            Ok(deleted_balances) => {
                // Call the `rebuild_deleted_user_balances` function
                let rebuilt_result = mongo_client
                    .rebuild_deleted_user_balances(start_block_height, deleted_balances)
                    .await;
                if let Err(err) = rebuilt_result {
                    println!("Failed to rebuild user balances: {:?}", err);
                }
            }
            Err(err) => {
                println!("Failed to delete user balances: {:?}", err);
            }
        }
        warn!("User Balances Rebuilt: {:?}", start.elapsed());
    }

    // LFG!
    match index_brc20(&rpc, &mongo_client, start_block_height.try_into().unwrap()).await {
        Ok(_) => info!("Finished indexing BRC20 tokens"),
        Err(e) => error!("Error indexing BRC20 tokens: {:?}", e),
    };

    Ok(())
}
