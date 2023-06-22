use super::*;
use mongodb::bson::oid::ObjectId;
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

    // pub async fn update_index(
    //   &self,
    //   collection_name: &str,
    //   id: &str,
    //   index_height: i64,
    // ) -> Result<(), mongodb::error::Error> {
    //   let db = self.client.database(&self.db_name);
    //   let collection = db.collection::<bson::Document>(collection_name);
    //   let filter = doc! { "_id": ObjectId::from_str(id).unwrap() };
    //   let update_options = UpdateOptions::builder().upsert(false).build();

    //   let update_doc = doc! {
    //     "$set": {
    //       "index_height": Bson::Int64(index_height),
    //     }
    //   };

    //   collection
    //     .update_one(filter, update_doc, update_options)
    //     .await?;

    //   Ok(())
    // }

    //   pub async fn get_all_documents(
    //     &self,
    //     collection_name: &str,
    //   ) -> Result<Vec<Document>> {
    //     let db = self.client.database(&self.db_name);
    //     let collection = db.collection::<bson::Document>(collection_name);

    //     let cursor = collection.find(None, None).await?;
    //     let documents = cursor.collect();

    //     Ok(documents)
    //   }
}
