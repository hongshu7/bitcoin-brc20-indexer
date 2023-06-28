use std::collections::HashMap;
use std::env;

use super::transfer::Brc20ActiveTransfer;
use super::user_balance::{UserBalanceEntry, UserBalanceEntryType};
use super::ToDocument;
use crate::brc20_index::consts;
use crate::brc20_index::user_balance::UserBalance;
use futures_util::stream::TryStreamExt;
use futures_util::StreamExt;
use mongodb::bson::{doc, Bson, DateTime, Document};
use mongodb::options::UpdateOptions;
use mongodb::{bson, options::ClientOptions, Client};

pub struct MongoClient {
    client: Client,
    db_name: String,
}

impl MongoClient {
    pub async fn new(
        connection_string: &str,
        db_name: &str,
    ) -> Result<Self, mongodb::error::Error> {
        let mut client_options = ClientOptions::parse(connection_string).await?;
        // Uncomment when using locally
        // Get the mongo host from environment variable if on local workstation
        let mongo_db_host = env::var("MONGO_DB_HOST");
        match mongo_db_host {
            Ok(_host) => client_options.direct_connection = Some(true),
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
    ) -> Result<(), mongodb::error::Error> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(collection_name);

        collection
            .insert_one(document, None)
            .await
            .expect("Could not insert document");

        Ok(())
    }

    // This method will update the user balance document in MongoDB
    pub async fn update_sender_user_balance_document(
        &self,
        from: &String,
        amount: f64,
        tick: &str,
    ) -> Result<(), anyhow::Error> {
        let filter = doc! {
          "address": from,
          "tick": tick
        };
        // retrieve the user balance from mongo
        let user_balance_from = self
            .get_user_balance_document(consts::COLLECTION_USER_BALANCES, filter.clone())
            .await?;

        match user_balance_from {
            Some(mut user_balance_doc) => {
                if let Some(overall_balance) = user_balance_doc.get(consts::OVERALL_BALANCE) {
                    if let Bson::Double(val) = overall_balance {
                        user_balance_doc
                            .insert(consts::OVERALL_BALANCE, Bson::Double(val - amount));
                    }
                }

                if let Some(transferable_balance) =
                    user_balance_doc.get(consts::TRANSFERABLE_BALANCE)
                {
                    if let Bson::Double(val) = transferable_balance {
                        user_balance_doc
                            .insert(consts::TRANSFERABLE_BALANCE, Bson::Double(val - amount));
                    }
                }
                println!("from update_sender_user_balance_document");

                let update_doc = doc! {
                    "$set": {
                        consts::TRANSFERABLE_BALANCE: user_balance_doc.get(consts::TRANSFERABLE_BALANCE).unwrap_or_else(|| &Bson::Double(0.0)),
                        consts::OVERALL_BALANCE: user_balance_doc.get(consts::OVERALL_BALANCE).unwrap_or_else(|| &Bson::Double(0.0)),
                    }
                };

                // Update the document in MongoDB
                self.update_document_by_filter(
                    consts::COLLECTION_USER_BALANCES,
                    filter,
                    update_doc,
                )
                .await?;
            }
            None => {}
        }
        Ok(())
    }

    pub async fn update_transfer_inscriber_user_balance_document(
        &self,
        from: &String,
        amount: f64,
        tick: &str,
        user_balance_from: Document,
    ) -> Result<(), anyhow::Error> {
        let filter = doc! {
          "address": from,
          "tick": tick
        };

        let mut user_balance_doc = user_balance_from;

        if let Some(available_balance) = user_balance_doc.get(consts::AVAILABLE_BALANCE) {
            if let Bson::Double(val) = available_balance {
                user_balance_doc.insert(consts::AVAILABLE_BALANCE, Bson::Double(val - amount));
            }
        }

        if let Some(transferable_balance) = user_balance_doc.get(consts::TRANSFERABLE_BALANCE) {
            if let Bson::Double(val) = transferable_balance {
                user_balance_doc.insert(consts::TRANSFERABLE_BALANCE, Bson::Double(val + amount));
            }
        }

        // create an update document
        let update_doc = doc! {
            "$set": {
                consts::TRANSFERABLE_BALANCE: user_balance_doc.get(consts::TRANSFERABLE_BALANCE).unwrap_or_else(|| &Bson::Double(0.0)),
                consts::AVAILABLE_BALANCE: user_balance_doc.get(consts::AVAILABLE_BALANCE).unwrap_or_else(|| &Bson::Double(0.0)),
            }
        };

        // Update the document in MongoDB
        self.update_document_by_filter(consts::COLLECTION_USER_BALANCES, filter, update_doc)
            .await?;

        Ok(())
    }

    // This method will retrieve the user balance document from MongoDB
    pub async fn get_user_balance_document(
        &self,
        collection_name: &str,
        filter: Document,
    ) -> Result<Option<Document>, mongodb::error::Error> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(collection_name);

        let result = collection.find_one(filter, None).await?;

        Ok(result)
    }

    pub async fn insert_new_document(
        &self,
        collection_name: &str,
        document: Document,
    ) -> Result<(), mongodb::error::Error> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(collection_name);

        collection.insert_one(document.clone(), None).await?;

        Ok(())
    }

    pub async fn get_document_by_field(
        &self,
        collection_name: &str,
        field_name: &str,
        field_value: &str,
    ) -> Result<Option<Document>, mongodb::error::Error> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(collection_name);

        let filter = doc! { field_name: field_value };
        let result = collection.find_one(filter, None).await?;

        Ok(result)
    }

    //get document by filter
    pub async fn get_document_by_filter(
        &self,
        collection_name: &str,
        filter: Document,
    ) -> Result<Option<Document>, mongodb::error::Error> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(collection_name);

        let result = collection.find_one(filter, None).await?;

        Ok(result)
    }

    //update document by field
    pub async fn update_document_by_field(
        &self,
        collection_name: &str,
        field_name: &str,
        field_value: &str,
        update_doc: Document,
    ) -> Result<(), mongodb::error::Error> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(collection_name);
        let filter = doc! { field_name: field_value };
        let update_options = UpdateOptions::builder().upsert(false).build();
        collection
            .update_one(filter, update_doc, update_options)
            .await?;

        Ok(())
    }

    //update a document in MongoDB using a filter
    pub async fn update_document_by_filter(
        &self,
        collection_name: &str,
        filter: Document,
        update_doc: Document,
    ) -> Result<(), mongodb::error::Error> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(collection_name);
        let update_options = UpdateOptions::builder().upsert(false).build();
        collection
            .update_one(filter, update_doc, update_options)
            .await?;

        Ok(())
    }

    pub async fn update_brc20_ticker_total_minted(
        &self,
        ticker: &str,
        amount_to_add: f64,
    ) -> Result<(), mongodb::error::Error> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(consts::COLLECTION_TICKERS);

        // Retrieve the brc20ticker document
        let filter = doc! { "tick": ticker };
        let ticker_doc = collection.find_one(filter.clone(), None).await?;

        match ticker_doc {
            Some(mut ticker) => {
                if let Some(total_minted) = ticker.get("total_minted") {
                    if let Bson::Double(val) = total_minted {
                        ticker.insert("total_minted", Bson::Double(val + amount_to_add));
                    }
                }

                let update_doc = doc! {
                    "$set": {
                        "total_minted": ticker.get("total_minted").unwrap_or_else(|| &Bson::Double(0.0)),
                    }
                };

                // Update the document in the collection
                let update_options = UpdateOptions::builder().upsert(false).build();
                collection
                    .update_one(filter, update_doc, update_options)
                    .await?;
            }
            None => {
                println!("No ticker document found for ticker {}", ticker);
            }
        }

        Ok(())
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
        self.insert_new_document(
            consts::COLLECTION_USER_BALANCE_ENTRY,
            user_balance_entry.to_document(),
        )
        .await?;

        Ok(())
    }

    pub async fn update_receiver_balance_document(
        &self,
        receiver_address: &String,
        amount: f64,
        tick: &str,
    ) -> Result<(), anyhow::Error> {
        let filter = doc! {
          "address": receiver_address,
          "tick": tick
        };

        // retrieve the user balance for the receiver from MongoDB
        let user_balance_to = self
            .get_user_balance_document(consts::COLLECTION_USER_BALANCES, filter.clone())
            .await?;

        match user_balance_to {
            // if the user balance document exists in Mongodb, update it
            Some(mut user_balance_doc) => {
                if let Some(overall_balance) = user_balance_doc.get(consts::OVERALL_BALANCE) {
                    if let Bson::Double(val) = overall_balance {
                        user_balance_doc
                            .insert(consts::OVERALL_BALANCE, Bson::Double(val + amount));
                    }
                }

                if let Some(available_balance) = user_balance_doc.get(consts::AVAILABLE_BALANCE) {
                    if let Bson::Double(val) = available_balance {
                        user_balance_doc
                            .insert(consts::AVAILABLE_BALANCE, Bson::Double(val + amount));
                    }
                }

                // create an update document
                let update_doc = doc! {
                    "$set": {
                        consts::OVERALL_BALANCE: user_balance_doc.get(consts::OVERALL_BALANCE).unwrap_or_else(|| &Bson::Double(0.0)),
                        consts::AVAILABLE_BALANCE: user_balance_doc.get(consts::AVAILABLE_BALANCE).unwrap_or_else(|| &Bson::Double(0.0)),
                    }
                };

                // Update the document in MongoDB
                self.update_document_by_filter(
                    consts::COLLECTION_USER_BALANCES,
                    filter,
                    update_doc,
                )
                .await?;
            }
            // if the user balance document does not exist in MongoDB, create a new one
            None => {
                // Create a new UserBalance
                let mut user_balance = UserBalance::new(receiver_address.clone(), tick.to_string());
                user_balance.overall_balance = amount;
                user_balance.available_balance = amount;

                // Insert the new document into the MongoDB collection
                self.insert_new_document(
                    consts::COLLECTION_USER_BALANCES,
                    user_balance.to_document(),
                )
                .await?;
            }
        }

        Ok(())
    }

    pub async fn store_completed_block(
        &self,
        block_height: i64,
    ) -> Result<(), mongodb::error::Error> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(consts::COLLECTION_BLOCKS_COMPLETED);

        let document = doc! {
            consts::KEY_BLOCK_HEIGHT: block_height,
            "created_at": Bson::DateTime(DateTime::now())
        };

        collection.insert_one(document, None).await?;

        Ok(())
    }

    pub async fn get_last_completed_block_height(
        &self,
    ) -> Result<Option<i64>, mongodb::error::Error> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(consts::COLLECTION_BLOCKS_COMPLETED);

        // Sort in descending order to get the latest block height
        let sort_doc = doc! { consts::KEY_BLOCK_HEIGHT: -1 };

        // Find one document (the latest) with the sorted criteria
        if let Some(result) = collection
            .find_one(
                None,
                mongodb::options::FindOneOptions::builder()
                    .sort(sort_doc)
                    .build(),
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
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(collection_name);

        collection
            .delete_many(
                doc! { "block_height": { "$gte": start_block_height } },
                None,
            )
            .await?;

        Ok(())
    }

    pub async fn drop_collection(
        &self,
        collection_name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(collection_name);

        collection.delete_many(doc! {}, None).await?;

        Ok(())
    }

    pub async fn reset_tickers_total_minted(&self) -> Result<(), Box<dyn std::error::Error>> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(consts::COLLECTION_TICKERS);

        let filter = doc! {}; // matches all documents
        let update = doc! { "$set": { "total_minted": 0.0 } };

        // Apply the update to all documents
        let update_options = UpdateOptions::builder().upsert(false).build();
        collection
            .update_many(filter, update, update_options)
            .await?;

        Ok(())
    }

    pub async fn calculate_and_update_total_minted(
        &self,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = self.client.database(&self.db_name);
        // Get a handle to the brc20_tickers collection
        let tickers_coll = db.collection::<bson::Document>(consts::COLLECTION_TICKERS);

        // Get a handle to the brc20_mints collection
        let mints_coll = db.collection::<bson::Document>(consts::COLLECTION_MINTS);

        // Get all tickers
        let cursor = tickers_coll.find(None, None).await?;
        let tickers: Vec<Document> = cursor.try_collect().await?;

        for ticker in tickers {
            // Extract ticker from the document
            let tick = ticker.get_str("tick")?;

            // Query all mints associated with this ticker
            let filter = doc! { "inscription.tick": ticker.get_str("tick")? };
            let cursor = mints_coll.find(filter, None).await?;
            let mints: Vec<Document> = cursor.try_collect().await?;

            // Sum the amounts
            let total_minted: f64 = mints
                .iter()
                .filter_map(|mint| mint.get_f64("amt").ok())
                .sum();

            // Update "total_minted" for this ticker in the database
            let filter = doc! { "tick": tick };
            let update = doc! { "$set": { "total_minted": total_minted } };
            tickers_coll.update_one(filter, update, None).await?;
        }

        Ok(())
    }

    pub async fn rebuild_user_balances(&self) -> Result<(), Box<dyn std::error::Error>> {
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

        // Get a handle to the "brc20_user_balances" collection
        let user_balances_coll = db.collection::<bson::Document>("brc20_user_balances");

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

                // Insert the new user balance document into the "brc20_user_balances" collection
                user_balances_coll
                    .insert_one(new_user_balance, None)
                    .await?;
            }
        }

        Ok(())
    }

    pub async fn ticker_exists(
        &self,
        collection: &str,
        filter: Document,
    ) -> Result<bool, mongodb::error::Error> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<bson::Document>(collection);
        match collection.find_one(filter, None).await {
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

    pub fn get_i32(&self, doc: &Document, field: &str) -> Option<i32> {
        match doc.get(field) {
            Some(Bson::Int32(value)) => Some(*value),
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
    ) -> Result<(), mongodb::error::Error> {
        // Get a handle to the collection where you want to insert the documents.
        let db = self.client.database(&self.db_name);
        let collection = db.collection(consts::COLLECTION_BRC20_ACTIVE_TRANSFERS);

        // Convert the HashMap to a Vec<bson::Document>.
        let documents: Result<Vec<bson::Document>, _> = active_transfers
            .values()
            .map(|value| bson::to_document(value))
            .collect::<Result<Vec<_>, _>>();

        // Insert the documents into the collection.
        collection.insert_many(documents?, None).await?;

        Ok(())
    }
}
