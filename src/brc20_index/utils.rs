use super::{
    brc20_ticker::Brc20Ticker,
    consts,
    mongo::MongoClient,
    user_balance::{UserBalance, UserBalanceEntry, UserBalanceEntryType},
    Brc20Inscription, ToDocument,
};
use bitcoin::{Address, Network, TxIn};
use bitcoincore_rpc::{bitcoincore_rpc_json::GetRawTransactionResult, Client, RpcApi};
use log::{debug, error, info};
use mongodb::bson::{Bson, Document};
use serde::Serialize;
use std::collections::HashMap;

pub fn get_witness_data_from_raw_tx(
    raw_tx_info: &GetRawTransactionResult,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let transaction = raw_tx_info.transaction()?;

    let mut witness_data_strings: Vec<String> = Vec::new();

    // Get the first transaction input
    if let Some(input) = transaction.input.first() {
        // Iterate through each witness of the input
        for witness in &input.witness {
            let witness_string = String::from_utf8_lossy(witness).into_owned();
            witness_data_strings.push(witness_string);
        }
    }

    Ok(witness_data_strings)
}

// extracts only inscriptions that read "brc-20", many will be invalid
pub fn extract_and_process_witness_data(witness_data: String) -> Option<Brc20Inscription> {
    // Check for the correct MIME type and find its end
    let mime_end_index = if witness_data.contains("text/plain") {
        witness_data.find("text/plain").unwrap() + "text/plain".len()
    } else if witness_data.contains("application/json") {
        witness_data.find("application/json").unwrap() + "application/json".len()
    } else {
        return None;
    };

    // Start searching for the JSON data only after the MIME type
    if let Some(json_start) = witness_data[mime_end_index..].find('{') {
        let json_start = mime_end_index + json_start; // Adjust json_start to be relative to the original string
        if let Some(json_end) = witness_data[json_start..].rfind('}') {
            // Extract the JSON string
            let json_data = &witness_data[json_start..json_start + json_end + 1];

            // Try to parse the JSON data
            match serde_json::from_str::<Brc20Inscription>(json_data) {
                Ok(parsed_data) => {
                    // Only return the parsed data if it contains brc-20
                    if parsed_data.p == "brc-20" {
                        return Some(parsed_data);
                    }
                }
                Err(e) => {
                    debug!("JSON parsing failed: {:?}", e);
                }
            }
        }
    }

    None
}

pub fn get_owner_of_vout(
    raw_tx_info: &GetRawTransactionResult,
    vout_index: usize,
) -> Result<Address, anyhow::Error> {
    if raw_tx_info.vout.is_empty() {
        return Err(anyhow::anyhow!("Transaction has no outputs"));
    }

    if raw_tx_info.vout.len() <= vout_index {
        return Err(anyhow::anyhow!(
            "Transaction doesn't have vout at given index"
        ));
    }

    // Get the controlling address of vout[vout_index]
    let script_pubkey = &raw_tx_info.vout[vout_index].script_pub_key;
    let script = match script_pubkey.script() {
        Ok(script) => script,
        Err(e) => return Err(anyhow::anyhow!("Failed to get script: {:?}", e)),
    };
    let this_address = Address::from_script(&script, Network::Bitcoin).map_err(|e| {
        error!("Couldn't derive address from scriptPubKey: {:?}", e);
        anyhow::anyhow!("Couldn't derive address from scriptPubKey: {:?}", e)
    })?;

    Ok(this_address)
}

pub fn convert_to_float(number_string: &str, decimals: u8) -> Result<f64, &'static str> {
    let parts: Vec<&str> = number_string.split('.').collect();
    match parts.len() {
        1 => {
            // No decimal point in the string
            let result = number_string.parse::<f64>();
            match result {
                Ok(value) => Ok(value),
                Err(_) => Err("Malformed inscription"),
            }
        }
        2 => {
            // There is a decimal point in the string
            if parts[1].len() > decimals as usize {
                error!("There are too many digits to the right of the decimal");
                return Err("There are too many digits to the right of the decimal");
            } else {
                let result = number_string.parse::<f64>();
                match result {
                    Ok(value) => Ok(value),
                    Err(_) => Err("Malformed inscription"),
                }
            }
        }
        _ => Err("Malformed inscription"), // More than one decimal point
    }
}

pub fn transaction_inputs_to_values(client: &Client, inputs: &[TxIn]) -> anyhow::Result<Vec<u64>> {
    let mut values: Vec<u64> = vec![];

    for input in inputs {
        let prev_output = input.previous_output;
        debug!(
            "Input from transaction: {:?}, index: {:?}",
            prev_output.txid, prev_output.vout
        );

        let prev_tx_info = client.get_raw_transaction_info(&prev_output.txid, None)?;
        let prev_tx = prev_tx_info.transaction()?;
        let output = &prev_tx.output[usize::try_from(prev_output.vout).unwrap()];

        // Add the value of the output to the list
        values.push(output.value);
    }

    if values.is_empty() {
        return Err(anyhow::anyhow!("Couldn't derive any values from inputs"));
    } else {
        Ok(values)
    }
}

pub async fn update_receiver_balance_document(
    mongo_client: &MongoClient,
    user_balance_docs: &mut HashMap<(String, String), Document>,
    user_balance_entry: &UserBalanceEntry,
) -> Result<(), anyhow::Error> {
    // Create the key from the address and ticker
    let key = (
        user_balance_entry.address.to_string(),
        user_balance_entry.tick.clone(),
    );

    // Check if the user balance document exists in the in-memory hashmap
    if let Some(user_balance) = user_balance_docs.get_mut(&key) {
        // Update the existing user balance document
        update_receiver(user_balance, user_balance_entry)?;
    } else {
        // Load the user balance document with retry
        let user_balance_doc = mongo_client
            .load_user_balance_with_retry(&key)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        if let Some(user_balance) = user_balance_doc {
            // Update the existing user balance document
            update_receiver(
                user_balance_docs
                    .entry(key.clone())
                    .or_insert_with(|| user_balance.clone()),
                user_balance_entry,
            )?;
        } else {
            // Create a new user balance document
            let new_user_balance = UserBalance {
                address: user_balance_entry.address.to_string(),
                tick: user_balance_entry.tick.clone(),
                overall_balance: user_balance_entry.amt,
                available_balance: user_balance_entry.amt,
                transferable_balance: 0.0,
                block_height: user_balance_entry.block_height,
            };

            // Convert the UserBalance to a Document
            let new_user_balance_doc = new_user_balance.to_document();

            // Insert the new user balance document into the hashmap
            user_balance_docs.insert(key, new_user_balance_doc);
        }
    }

    Ok(())
}

fn update_receiver(
    user_balance: &mut Document,
    user_balance_entry: &UserBalanceEntry,
) -> Result<(), anyhow::Error> {
    // Get the overall and available balance values from the document
    let overall_balance = user_balance
        .get_f64(consts::OVERALL_BALANCE)
        .unwrap_or_default();
    let available_balance = user_balance
        .get_f64(consts::AVAILABLE_BALANCE)
        .unwrap_or_default();

    // Update the values
    let updated_overall_balance = overall_balance + user_balance_entry.amt;
    let updated_available_balance = available_balance + user_balance_entry.amt;

    // Update the document
    user_balance.insert(
        consts::OVERALL_BALANCE.to_string(),
        Bson::Double(updated_overall_balance),
    );
    user_balance.insert(
        consts::AVAILABLE_BALANCE.to_string(),
        Bson::Double(updated_available_balance),
    );
    // Update the block height
    user_balance.insert(
        consts::KEY_BLOCK_HEIGHT.to_string(),
        Bson::Int64(user_balance_entry.block_height as i64),
    );

    Ok(())
}

pub fn update_sender_or_inscriber_user_balance_document(
    user_balance: &mut Document,
    user_balance_entry: &UserBalanceEntry,
) -> Result<(), anyhow::Error> {
    // Get the available balance, transferable balance, and overall balance values
    let available_balance = user_balance
        .get_f64(consts::AVAILABLE_BALANCE)
        .unwrap_or_default();
    let transferable_balance = user_balance
        .get_f64(consts::TRANSFERABLE_BALANCE)
        .unwrap_or_default();
    let overall_balance = user_balance
        .get_f64(consts::OVERALL_BALANCE)
        .unwrap_or_default();

    // Update the values based on the entry type
    info!(
        "User balance entry type: {:?}",
        user_balance_entry.entry_type
    );
    match user_balance_entry.entry_type {
        UserBalanceEntryType::Send => {
            let updated_transferable_balance = transferable_balance - user_balance_entry.amt;
            let updated_overall_balance = overall_balance - user_balance_entry.amt;

            user_balance.insert(
                consts::TRANSFERABLE_BALANCE.to_string(),
                Bson::Double(updated_transferable_balance),
            );
            user_balance.insert(
                consts::OVERALL_BALANCE.to_string(),
                Bson::Double(updated_overall_balance),
            );
        }
        UserBalanceEntryType::Inscription => {
            let updated_available_balance = available_balance - user_balance_entry.amt;
            let updated_transferable_balance = transferable_balance + user_balance_entry.amt;

            user_balance.insert(
                consts::AVAILABLE_BALANCE.to_string(),
                Bson::Double(updated_available_balance),
            );
            user_balance.insert(
                consts::TRANSFERABLE_BALANCE.to_string(),
                Bson::Double(updated_transferable_balance),
            );
        }
        _ => {
            // Other entry types are not applicable for this function
            return Err(anyhow::anyhow!("Invalid entry type"));
        }
    }

    // Update the block height
    user_balance.insert(
        consts::KEY_BLOCK_HEIGHT.to_string(),
        Bson::Int64(user_balance_entry.block_height as i64),
    );

    Ok(())
}

pub async fn update_sender_user_balance_document(
    mongo_client: &MongoClient,
    user_balances: &mut HashMap<(String, String), Document>,
    user_balance_entry: &UserBalanceEntry,
) -> Result<(), anyhow::Error> {
    // Create the key from the address and ticker
    let key = (
        user_balance_entry.address.to_string(),
        user_balance_entry.tick.clone(),
    );

    // Check if the user balance document exists in the in-memory hashmap
    if let Some(user_balance) = user_balances.get_mut(&key) {
        // Update the existing user balance document
        update_sender_or_inscriber_user_balance_document(user_balance, user_balance_entry)?;
    } else {
        // Load the user balance document with retry
        let user_balance_doc = mongo_client
            .load_user_balance_with_retry(&key)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        if let Some(user_balance) = user_balance_doc {
            // Update the existing user balance document
            update_sender_or_inscriber_user_balance_document(
                user_balances
                    .entry(key.clone())
                    .or_insert_with(|| user_balance.clone()),
                user_balance_entry,
            )?;
        } else {
            // User balance document not found in the in-memory hashmap or database
            return Err(anyhow::anyhow!("User balance document not found"));
        }
    }

    Ok(())
}

//this is for logging to file
#[derive(Serialize)]
struct BalanceInfo {
    overall_balance: f64,
    available_balance: f64,
    transferable_balance: f64,
}

#[derive(Serialize)]
struct TickerWithBalances {
    ticker: Brc20Ticker,
    balances: HashMap<String, BalanceInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_to_float_no_decimal() {
        let result = convert_to_float("1000", 2);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1000.0);
    }

    #[test]
    fn test_convert_to_float_with_decimal() {
        let result = convert_to_float("1234.56", 2);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1234.56);
    }

    #[test]
    fn test_convert_to_float_too_many_decimals() {
        let result = convert_to_float("1234.567", 2);
        assert!(result.is_err());
    }

    #[test]
    fn test_convert_to_float_not_a_number() {
        let result = convert_to_float("abcd", 2);
        assert!(result.is_err());
    }

    #[test]
    fn test_convert_to_float_multiple_decimal_points() {
        let result = convert_to_float("1.2.3", 2);
        assert!(result.is_err());
    }
}
