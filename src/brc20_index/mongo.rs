use super::ToDocument;
use crate::brc20_index::consts;
use crate::brc20_index::user_balance::UserBalance;
use mongodb::bson::{doc, Bson, Document};
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
        client_options.direct_connection = Some(true);
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

    // This method will update the user balance document in MongoDB
    pub async fn update_transfer_inscriber_user_balance_document(
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
                if let Some(available_balance) = user_balance_doc.get(consts::AVAILABLE_BALANCE) {
                    if let Bson::Double(val) = available_balance {
                        user_balance_doc
                            .insert(consts::AVAILABLE_BALANCE, Bson::Double(val - amount));
                    }
                }

                if let Some(transferable_balance) =
                    user_balance_doc.get(consts::TRANSFERABLE_BALANCE)
                {
                    if let Bson::Double(val) = transferable_balance {
                        user_balance_doc
                            .insert(consts::TRANSFERABLE_BALANCE, Bson::Double(val + amount));
                    }
                }

                // create an update document using $set operator
                println!("debug #3");
                let update_doc = doc! {
                    "$set": {
                        consts::TRANSFERABLE_BALANCE: user_balance_doc.get(consts::TRANSFERABLE_BALANCE).unwrap_or_else(|| &Bson::Double(0.0)),
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
            None => {}
        }
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
        println!("Inserted document with id {:?}", document);

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

        println!("debug #5");
        // let update = doc! { "$set": update_doc };
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

        println!("debug #2");
        // let update = doc! { "$set": update_doc };
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
        let db = self.client.database("omnisat");
        let collection = db.collection::<bson::Document>(consts::COLLECTION_TICKERS);

        // Retrieve the brc20ticker document
        let filter = doc! { "tick": ticker };
        let ticker_doc = collection.find_one(filter.clone(), None).await?;

        println!("ticker_doc: {:?}", ticker_doc);

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

                // // Update the document in MongoDB
                // self.update_document_by_filter(consts::COLLECTION_TICKERS, filter, update_doc)
                //     .await?;

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

                // create an update document using $set operator

                println!("debug #4");
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
}
