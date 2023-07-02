use std::collections::HashMap;
use std::env;
use std::time::Duration;

use super::transfer::Brc20ActiveTransfer;
use super::user_balance::{UserBalanceEntry, UserBalanceEntryType};
use super::ToDocument;
use crate::brc20_index::consts;
use futures_util::stream::TryStreamExt;
use futures_util::StreamExt;
use mongodb::bson::{doc, Bson, DateTime, Document};
use mongodb::options::{FindOneOptions, FindOptions, IndexOptions, UpdateOptions};
use mongodb::{bson, options::ClientOptions, Client};
use mongodb::{Cursor, IndexModel};

pub struct MongoClient {
    client: Client,
    db_name: String,
}

impl MongoClient {
    pub async fn new(
        connection_string: &str,
        db_name: &str,
        mongo_direct_connection: bool,
    ) -> Result<Self, mongodb::error::Error> {
        let mut client_options = ClientOptions::parse(connection_string).await?;
        // Uncomment when using locally
        // Get the mongo host from environment variable if on local workstation
        let mongo_db_host = env::var("MONGO_DB_HOST");
        match mongo_db_host {
            Ok(_host) => client_options.direct_connection = Some(mongo_direct_connection),
            Err(_) => (),
        };

        let client = Client::with_options(client_options)?;

        Ok(Self {
            client,
            db_name: db_name.to_string(),
        })
    }

    pub async fn insert_document(
        &self,
        collection_name: &str,
        document: bson::Document,
    ) -> anyhow::Result<()> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(collection_name);
        let retries = consts::MONGO_RETRIES;

        for attempt in 0..=retries {
            match collection.insert_one(document.clone(), None).await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    println!(
                        "Attempt {}/{} failed with error: {}. Retrying...",
                        attempt + 1,
                        retries,
                        e,
                    );
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
        Err(anyhow::anyhow!(
            "Failed to insert document after all retries"
        ))
    }

    pub async fn update_document_by_filter(
        &self,
        collection_name: &str,
        filter: Document,
        update_doc: Document,
        update_options: Option<UpdateOptions>,
    ) -> anyhow::Result<()> {
        self.update_one_with_retries(collection_name, filter, update_doc, update_options)
            .await
    }

    pub async fn update_one_with_retries(
        &self,
        collection_name: &str,
        filter: Document,
        update: Document,
        update_options: Option<UpdateOptions>,
    ) -> anyhow::Result<()> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(collection_name);
        let retries = consts::MONGO_RETRIES;

        for attempt in 0..=retries {
            match collection
                .update_one(filter.clone(), update.clone(), update_options.clone())
                .await
            {
                Ok(_) => return Ok(()),
                Err(e) => {
                    println!(
                        "Attempt {}/{} failed with error: {}. Retrying...",
                        attempt + 1,
                        retries,
                        e,
                    );
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
        Err(anyhow::anyhow!(
            "Failed to update document after all retries"
        ))
    }

    pub async fn find_one_with_retries(
        &self,
        collection_name: &str,
        filter: Document,
        options: Option<FindOneOptions>,
    ) -> anyhow::Result<Option<Document>> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(collection_name);
        let retries = consts::MONGO_RETRIES;

        for attempt in 0..=retries {
            match collection.find_one(filter.clone(), options.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    println!(
                        "Attempt {}/{} failed with error: {}. Retrying...",
                        attempt + 1,
                        retries,
                        e,
                    );
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
        Err(anyhow::anyhow!("Failed to find document after all retries"))
    }

    pub async fn find_with_retries(
        &self,
        collection_name: &str,
        filter: Option<Document>,
        options: Option<FindOptions>,
    ) -> anyhow::Result<Cursor<Document>> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(collection_name);
        let retries = consts::MONGO_RETRIES;

        for attempt in 0..=retries {
            match collection.find(filter.clone(), options.clone()).await {
                Ok(cursor) => return Ok(cursor),
                Err(e) => {
                    println!(
                        "Attempt {}/{} failed with error: {}. Retrying...",
                        attempt + 1,
                        retries,
                        e,
                    );
                    tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
                }
            }
        }
        Err(anyhow::anyhow!(
            "Failed to find documents after all retries"
        ))
    }

    pub async fn insert_many_with_retries(
        &self,
        collection_name: &str,
        documents: Vec<bson::Document>,
    ) -> Result<(), anyhow::Error> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(collection_name);
        let retries = consts::MONGO_RETRIES;

        let mut attempts = 0;
        while attempts <= retries {
            match collection.insert_many(documents.clone(), None).await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    println!(
                        "Failed to insert documents: {}. Attempt {}/{}",
                        e,
                        attempts + 1,
                        retries + 1
                    );
                    attempts += 1;
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
        Err(anyhow::Error::msg("All retry attempts failed"))
    }

    pub async fn delete_many_with_retries(
        &self,
        collection_name: &str,
        filter: Document,
    ) -> Result<(), anyhow::Error> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(collection_name);
        let retries = consts::MONGO_RETRIES;

        let mut attempts = 0;
        while attempts <= retries {
            match collection.delete_many(filter.clone(), None).await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    println!(
                        "Failed to delete documents: {}. Attempt {}/{}",
                        e,
                        attempts + 1,
                        retries + 1
                    );
                    attempts += 1;
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
        Err(anyhow::Error::msg("All retry attempts failed"))
    }

    pub async fn get_document_by_field(
        &self,
        collection_name: &str,
        field_name: &str,
        field_value: &str,
    ) -> Result<Option<Document>, anyhow::Error> {
        let filter = doc! { field_name: field_value };
        self.find_one_with_retries(collection_name, filter, None)
            .await
    }

    pub async fn get_document_by_filter(
        &self,
        collection_name: &str,
        filter: Document,
    ) -> Result<Option<Document>, anyhow::Error> {
        self.find_one_with_retries(collection_name, filter, None)
            .await
    }

    pub async fn update_document_by_field(
        &self,
        collection_name: &str,
        field_name: &str,
        field_value: &str,
        update_doc: Document,
    ) -> anyhow::Result<()> {
        let update_options = UpdateOptions::builder().upsert(false).build();
        let filter = doc! { field_name: field_value };
        self.update_one_with_retries(collection_name, filter, update_doc, Some(update_options))
            .await
    }

    pub async fn insert_user_balance_entry(
        &self,
        address: &String,
        amount: f64,
        tick: &str,
        block_height: u64,
        entry_type: UserBalanceEntryType,
    ) -> Result<(), anyhow::Error> {
        // instantiate a new user balance entry
        let user_balance_entry = UserBalanceEntry::new(
            address.to_string(),
            tick.to_string(),
            block_height,
            amount,
            entry_type,
        );

        // Insert the new document into the MongoDB collection
        self.insert_document(
            consts::COLLECTION_USER_BALANCE_ENTRY,
            user_balance_entry.to_document(),
        )
        .await?;

        Ok(())
    }

    // Method to update the balance document for a receiver in MongoDB
    pub async fn update_receiver_balance_document(
        &self,                  // reference to the MongoClient
        receiver_address: &str, // address of the receiver
        amount: f64,            // amount to add
        tick: &str,             // token symbol
    ) -> Result<(), anyhow::Error> {
        // Prepare the filter document to find the specific user's balance document
        let filter = doc! {
            "address": receiver_address,
            "tick": tick
        };

        // Prepare the update document to increment overall and available balances
        // "$inc" operator increments the value of the field by the specified amount
        let update_doc = doc! {
            "$setOnInsert": { "transferable_balance": 0.0, "created_at": Bson::DateTime(DateTime::now()) },
            "$inc": {
                consts::OVERALL_BALANCE: amount,
                consts::AVAILABLE_BALANCE: amount,
            }
        };

        // Prepare the options for the update operation
        // Upsert is set to true which will insert a new document if no document matches the filter
        let update_options = UpdateOptions::builder().upsert(true).build();

        // Perform the update operation
        // If a document matching the filter is found, it is updated
        // If no matching document is found, a new document is inserted due to the upsert option
        self.update_document_by_filter(
            consts::COLLECTION_USER_BALANCES,
            filter,
            update_doc,
            Some(update_options),
        )
        .await?;

        // Return Ok if the operation is successful
        Ok(())
    }

    // This method will update the user balance document in MongoDB
    pub async fn update_sender_user_balance_document(
        &self,
        from: &str,
        amount: f64,
        tick: &str,
    ) -> Result<(), anyhow::Error> {
        let filter = doc! {
          "address": from,
          "tick": tick
        };

        // Prepare the update document to decrement overall and transferable balances
        // "$inc" operator increments the value of the field by the specified amount
        // Here we are using negative amounts to decrease the balances
        let update_doc = doc! {
            "$inc": {
                consts::OVERALL_BALANCE: -amount,
                consts::TRANSFERABLE_BALANCE: -amount,
            }
        };

        // Update the document in MongoDB
        let update_options = UpdateOptions::builder().upsert(false).build();
        self.update_document_by_filter(
            consts::COLLECTION_USER_BALANCES,
            filter,
            update_doc,
            Some(update_options),
        )
        .await?;

        Ok(())
    }

    pub async fn update_transfer_inscriber_user_balance_document(
        &self,
        from: &str,
        amount: f64,
        tick: &str,
    ) -> Result<(), anyhow::Error> {
        // Prepare the filter document
        let filter = doc! {
            "address": from,
            "tick": tick
        };

        // Prepare the update document
        // "$inc" operator increments the value of the field by the specified amount
        // Here, it's decrementing the `consts::AVAILABLE_BALANCE` by `amount`
        // and incrementing the `consts::TRANSFERABLE_BALANCE` by `amount`
        let update_doc = doc! {
            "$inc": {
                consts::AVAILABLE_BALANCE: -amount,
                consts::TRANSFERABLE_BALANCE: amount,
            }
        };

        // Update the document in MongoDB
        let update_options = UpdateOptions::builder().upsert(false).build();
        self.update_document_by_filter(
            consts::COLLECTION_USER_BALANCES,
            filter,
            update_doc,
            Some(update_options),
        )
        .await?;

        Ok(())
    }

    pub async fn update_brc20_ticker_total_minted(
        &self,
        ticker: &str,
        amount_to_add: f64,
    ) -> anyhow::Result<()> {
        let update_options = UpdateOptions::builder().upsert(false).build();
        let filter = doc! { "tick": ticker };
        let update_doc = doc! {
            "$inc": { "total_minted": amount_to_add }
        };

        // Update the document in the collection with retries
        self.update_one_with_retries(
            consts::COLLECTION_TICKERS,
            filter,
            update_doc,
            Some(update_options),
        )
        .await?;

        Ok(())
    }

    pub async fn store_completed_block(&self, block_height: i64) -> anyhow::Result<()> {
        let document = doc! {
            consts::KEY_BLOCK_HEIGHT: block_height,
            "created_at": Bson::DateTime(DateTime::now())
        };

        // Insert into MongoDB collection
        self.insert_document(consts::COLLECTION_BLOCKS_COMPLETED, document)
            .await?;

        Ok(())
    }

    pub async fn get_last_completed_block_height(&self) -> Result<Option<i64>, anyhow::Error> {
        // Sort in descending order to get the latest block height
        let sort_doc = doc! { consts::KEY_BLOCK_HEIGHT: -1 };
        let find_options = FindOneOptions::builder().sort(sort_doc).build();

        // Find one document (the latest) with the sorted criteria
        if let Some(result) = self
            .find_one_with_retries(
                consts::COLLECTION_BLOCKS_COMPLETED,
                doc! {}, // No filter, we want any document
                Some(find_options),
            )
            .await?
        {
            if let Ok(block_height) = result.get_i64(consts::KEY_BLOCK_HEIGHT) {
                return Ok(Some(block_height));
            }
        }

        // No processed blocks found or unable to get the block_height field
        Ok(None)
    }

    pub async fn delete_from_collection(
        &self,
        collection_name: &str,
        start_block_height: i64,
    ) -> anyhow::Result<()> {
        self.delete_many_with_retries(
            collection_name,
            doc! { "block_height": { "$gte": start_block_height } },
        )
        .await?;

        Ok(())
    }

    pub async fn drop_collection(&self, collection_name: &str) -> anyhow::Result<()> {
        self.delete_many_with_retries(collection_name, doc! {})
            .await?;

        Ok(())
    }

    pub async fn rebuild_user_balances(&self) -> anyhow::Result<()> {
        let db = self.client.database(&self.db_name);

        // Fetch all user balance entries
        let user_balance_entries_coll =
            db.collection::<bson::Document>(consts::COLLECTION_USER_BALANCE_ENTRY);
        let cursor = user_balance_entries_coll.find(None, None).await?;
        let user_balance_entries: Vec<Document> = cursor.try_collect().await?;

        // Prepare a HashMap to hold user balances
        let mut user_balances: HashMap<String, HashMap<String, (f64, f64, f64)>> = HashMap::new();

        // Iterate over user balance entries
        for user_balance_entry in user_balance_entries {
            let address = user_balance_entry.get_str("address")?;
            let ticker = user_balance_entry.get_str("tick")?;
            let amount = user_balance_entry.get_f64("amt")?;
            let entry_type: UserBalanceEntryType =
                UserBalanceEntryType::from(user_balance_entry.get_str("entry_type")?);

            let user_balance = user_balances
                .entry(address.to_string())
                .or_insert_with(HashMap::new);
            let balance = user_balance
                .entry(ticker.to_string())
                .or_insert((0.0, 0.0, 0.0)); // (available_balance, transferable_balance, overall balance)

            // Adjust balances based on entry type
            match entry_type {
                UserBalanceEntryType::Receive => {
                    balance.0 += amount; // Increase the available balance
                    balance.2 += amount; // Increase the overall balance
                }
                UserBalanceEntryType::Send => {
                    balance.1 -= amount; // Decrease the transferable balance
                    balance.2 -= amount; // Decrease the overall balance
                }
                UserBalanceEntryType::Inscription => {
                    balance.0 -= amount; // Decrease the available balance
                    balance.1 += amount; // Increase the transferable balance
                }
            }
        }

        // Iterate over the constructed user balances
        for (address, ticker_balances) in user_balances {
            for (ticker, (available_balance, transferable_balance, overall_balance)) in
                ticker_balances
            {
                // Construct a new user balance document
                let new_user_balance = doc! {
                    "address": &address,
                    "tick": &ticker,
                    "available_balance": available_balance,
                    "transferable_balance": transferable_balance,
                    "overall_balance": overall_balance,
                };

                // Insert the new document into the MongoDB collection
                self.insert_document(consts::COLLECTION_USER_BALANCES, new_user_balance)
                    .await?;
            }
        }

        Ok(())
    }

    pub async fn ticker_exists(
        &self,
        collection: &str,
        filter: Document,
    ) -> Result<bool, anyhow::Error> {
        match self.find_one_with_retries(collection, filter, None).await {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub fn get_double(&self, doc: &Document, field: &str) -> Option<f64> {
        match doc.get(field) {
            Some(Bson::Double(value)) => Some(*value),
            _ => None,
        }
    }

    pub fn get_f64(&self, doc: &Document, field: &str) -> Option<f64> {
        match doc.get(field) {
            Some(Bson::Double(value)) => Some(*value),
            _ => None,
        }
    }

    pub fn get_string(
        &self,
        doc: &Document,
        key: &str,
    ) -> Result<String, mongodb::bson::document::ValueAccessError> {
        match doc.get_str(key) {
            Ok(value) => Ok(value.to_string()),
            Err(e) => Err(e),
        }
    }

    pub async fn load_active_transfers_with_retry(
        &self,
    ) -> Result<Option<HashMap<(String, i64), Brc20ActiveTransfer>>, String> {
        let retries = consts::MONGO_RETRIES;
        for attempt in 0..=retries {
            match self.load_active_transfers().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    // Error handling and backoff logic here
                    log::warn!(
                        "Attempt {}/{} failed with error: {}. Retrying...",
                        attempt + 1,
                        retries,
                        e,
                    );
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
        Err(format!(
            "Failed to load active transfers after {} attempts",
            retries
        ))
    }

    pub async fn load_active_transfers(
        &self,
    ) -> Result<Option<HashMap<(String, i64), Brc20ActiveTransfer>>, String> {
        let mut active_transfers = HashMap::new();

        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(consts::COLLECTION_BRC20_ACTIVE_TRANSFERS);

        // Check if the collection has any documents
        let doc_count = collection
            .estimated_document_count(None)
            .await
            .map_err(|e| e.to_string())?;

        // If no documents, return None
        if doc_count == 0 {
            return Ok(None);
        }

        let mut cursor = collection
            .find(None, None)
            .await
            .map_err(|e| e.to_string())?;

        while let Some(result) = cursor.next().await {
            match result {
                Ok(document) => {
                    let active_transfer = Brc20ActiveTransfer::from_document(document)?;
                    let key = (active_transfer.tx_id.clone(), active_transfer.vout);
                    active_transfers.insert(key, active_transfer);
                }
                Err(e) => return Err(e.to_string()),
            }
        }

        Ok(Some(active_transfers))
    }

    pub async fn insert_active_transfers_to_mongodb(
        &self,
        active_transfers: HashMap<(String, i64), Brc20ActiveTransfer>,
    ) -> Result<(), anyhow::Error> {
        // Convert the HashMap to a Vec<bson::Document>.
        let documents: Result<Vec<bson::Document>, _> = active_transfers
            .values()
            .map(|value| bson::to_document(value))
            .collect::<Result<Vec<_>, _>>();

        // Insert the documents into the collection with retries.
        self.insert_many_with_retries(consts::COLLECTION_BRC20_ACTIVE_TRANSFERS, documents?)
            .await?;

        Ok(())
    }

    pub async fn insert_tickers_total_minted_at_block_height(
        &self,
        block_height: i64,
    ) -> anyhow::Result<()> {
        // Get all tickers
        let cursor = self
            .find_with_retries(consts::COLLECTION_TICKERS, None, None)
            .await?;
        let tickers: Vec<Document> = cursor.try_collect().await?;

        // Prepare the tickers array for the new document
        let mut ticker_docs = Vec::new();
        for ticker in tickers {
            // Get the total minted for this ticker
            let total_minted = ticker.get_f64("total_minted").unwrap_or(0.0);

            // Build a subdocument for this ticker
            let ticker_doc = doc! {
                "tick": ticker.get_str("tick")?,
                "total_minted": total_minted,
            };
            ticker_docs.push(ticker_doc);
        }

        // Build the new document
        let new_ticker_total_minted_at_block_height = doc! {
            "block_height": block_height,
            "created_at": Bson::DateTime(DateTime::now()),
            "tickers": ticker_docs,
        };

        // Insert the new document into the blocks_completed collection
        // blocks_coll.insert_one(new_block_summary, None).await?;
        self.insert_document(
            consts::COLLECTION_TOTAL_MINTED_AT_BLOCK_HEIGHT,
            new_ticker_total_minted_at_block_height,
        )
        .await?;

        Ok(())
    }

    // Function to get ticker totals by block height
    pub async fn get_ticker_totals_by_block_height(
        &self,
        block_height: i64,
    ) -> Result<Option<Document>, anyhow::Error> {
        let db = self.client.database(&self.db_name);
        let collection =
            db.collection::<bson::Document>(consts::COLLECTION_TOTAL_MINTED_AT_BLOCK_HEIGHT);

        let filter = doc! { "block_height": block_height };

        let result = self
            .find_one_with_retries(collection.name(), filter, None)
            .await?;

        Ok(result)
    }

    // Function to update ticker totals
    pub async fn update_ticker_totals(&self, block_height: i64) -> Result<(), anyhow::Error> {
        // First, get the document for the given block height
        let ticker_totals_doc = match self.get_ticker_totals_by_block_height(block_height).await? {
            Some(doc) => doc,
            None => {
                return Err(anyhow::Error::msg(format!(
                    "No document found for block height {}",
                    block_height
                )))
            }
        };

        // Get the tickers array from the ticker totals document
        let ticker_totals = ticker_totals_doc.get_array("tickers")?;
        let update_options = UpdateOptions::builder().upsert(false).build();

        for ticker_doc in ticker_totals {
            if let Bson::Document(ticker_doc) = ticker_doc {
                let tick = ticker_doc.get_str("tick")?;
                let total_minted = ticker_doc.get_f64("total_minted")?;

                // Update the total_minted field for this ticker in the tickers collection
                let filter = doc! { "tick": tick };
                let update = doc! { "$set": { "total_minted": total_minted } };

                self.update_one_with_retries(
                    consts::COLLECTION_TICKERS,
                    filter,
                    update,
                    Some(update_options.clone()),
                )
                .await?;
            }
        }

        Ok(())
    }

    pub async fn create_indexes(&self) -> Result<(), Box<dyn std::error::Error>> {
        let db = self.client.database(&self.db_name);

        // Create an index on the 'address' and 'tick' fields for COLLECTION_USER_BALANCES
        let user_balances_collection =
            db.collection::<bson::Document>(consts::COLLECTION_USER_BALANCES);
        let user_balances_index_model = IndexModel::builder()
            .keys(doc! { "address": 1, "tick": 1 }) // 1 for ascending
            .options(IndexOptions::builder().unique(true).build())
            .build();

        // Create the index for COLLECTION_USER_BALANCES
        user_balances_collection
            .create_index(user_balances_index_model, None)
            .await?;

        // Create an index on the 'tick' field for COLLECTION_TICKERS
        let tickers_collection = db.collection::<bson::Document>(consts::COLLECTION_TICKERS);
        let tickers_index_model = IndexModel::builder()
            .keys(doc! { "tick": 1 }) // 1 for ascending
            .options(IndexOptions::builder().unique(true).build())
            .build();

        // Create the index for COLLECTION_TICKERS
        tickers_collection
            .create_index(tickers_index_model, None)
            .await?;

        Ok(())
    }
}
