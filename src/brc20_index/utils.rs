use bitcoin::{Address, Network, OutPoint, TxIn, TxOut};
use bitcoincore_rpc::{bitcoincore_rpc_json::GetRawTransactionResult, RpcApi};

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

// pub(crate) fn handle_transaction(
//   index: &Index,
//   outpoint: &OutPoint,
// ) -> Result<(), Box<dyn std::error::Error>> {
//   // Get the raw transaction info.
//   let raw_tx_info = index
//     .client
//     .get_raw_transaction_info(&outpoint.txid, None)?;

//   // Display the raw transaction info.
//   display_raw_transaction_info(&raw_tx_info);

//   // Get the transaction Inputs
//   let inputs = &raw_tx_info.transaction()?.input;

//   // Get the addresses and values of the inputs.
//   let input_addresses_values = transaction_inputs_to_addresses_values(index, inputs)?;
//   for (index, (address, value)) in input_addresses_values.iter().enumerate() {
//     println!("Input Address {}: {}, Value: {}", index + 1, address, value);
//   }

//   // display_input_info(&raw_tx_info);

//   println!("=====");
//   // Get the transaction Outputs
//   let outputs = &raw_tx_info.transaction()?.output;

//   // Get the addresses and values of the outputs.
//   let output_addresses_values = transaction_outputs_to_addresses_values(outputs)?;
//   for (index, (address, value)) in output_addresses_values.iter().enumerate() {
//     println!(
//       "Output Address {}: {}, Value: {}",
//       index + 1,
//       address,
//       value
//     );
//   }

//   Ok(())
// }

// fn transaction_inputs_to_addresses_values(
//   index: &Index,
//   inputs: &Vec<TxIn>,
// ) -> Result<Vec<(Address, u64)>, Box<dyn std::error::Error>> {
//   let mut addresses_values: Vec<(Address, u64)> = vec![];

//   for input in inputs {
//     let prev_output = input.previous_output;
//     println!(
//       "Input from transaction: {:?}, index: {:?}",
//       prev_output.txid, prev_output.vout
//     );

//     let prev_tx_info = index
//       .client
//       .get_raw_transaction_info(&prev_output.txid, None)?;

//     let prev_tx = prev_tx_info.transaction()?;

//     let output = &prev_tx.output[usize::try_from(prev_output.vout).unwrap()];
//     let script_pub_key = &output.script_pubkey;

//     let address = Address::from_script(&script_pub_key, Network::Testnet).map_err(|_| {
//       println!("Couldn't derive address from scriptPubKey");
//       "Couldn't derive address from scriptPubKey"
//     })?;

//     // Add both the address and the value of the output to the list
//     addresses_values.push((address, output.value));

//     println!("=====");
//   }

//   if addresses_values.is_empty() {
//     Err("Couldn't derive any addresses or values from scriptPubKeys".into())
//   } else {
//     Ok(addresses_values)
//   }
// }

fn transaction_outputs_to_addresses_values(
    outputs: &Vec<TxOut>,
) -> Result<Vec<(Address, u64)>, Box<dyn std::error::Error>> {
    let mut addresses_values: Vec<(Address, u64)> = vec![];

    for output in outputs {
        let script_pub_key = &output.script_pubkey;

        if let Ok(address) = Address::from_script(&script_pub_key, Network::Testnet) {
            // Add both the address and the value of the output to the list
            addresses_values.push((address, output.value));
        } else {
            println!("Couldn't derive address from scriptPubKey");
        }
    }

    if addresses_values.is_empty() {
        Err("Couldn't derive any addresses or values from scriptPubKeys".into())
    } else {
        Ok(addresses_values)
    }
}

fn display_raw_transaction_info(raw_transaction_info: &GetRawTransactionResult) {
    println!("Raw Transaction Information:");
    println!("----------------");
    println!("Txid: {:?}", raw_transaction_info.txid);
    println!("Hash: {:?}", raw_transaction_info.hash);
    println!("Size: {:?}", raw_transaction_info.size);
    println!("Vsize: {:?}", raw_transaction_info.vsize);
    println!("Version: {:?}", raw_transaction_info.version);
    println!("Locktime: {:?}", raw_transaction_info.locktime);
    println!("Blockhash: {:?}", raw_transaction_info.blockhash);
    println!("Confirmations: {:?}", raw_transaction_info.confirmations);
    println!("Time: {:?}", raw_transaction_info.time);
    println!("Blocktime: {:?}", raw_transaction_info.blocktime);
    println!();
}

fn display_input_info(raw_transaction_info: &GetRawTransactionResult) {
    println!("Inputs (Vin):");
    println!("-------------");
    for (i, vin) in raw_transaction_info.vin.iter().enumerate() {
        println!("Vin {}: {:?}", i + 1, vin);
        if let Some(txid) = &vin.txid {
            println!("  txid: {:?}", txid);
        }
        if let Some(vout) = vin.vout {
            println!("  vout: {:?}", vout);
        }
        if let Some(script_sig) = &vin.script_sig {
            println!("  script_sig: {:?}", script_sig);
        }
        if let Some(txinwitness) = &vin.txinwitness {
            println!("  txinwitness: {:?}", txinwitness);
        }
        if let Some(coinbase) = &vin.coinbase {
            println!("  coinbase: {:?}", coinbase);
        }
        println!("  sequence: {:?}", vin.sequence);
    }
    println!();
}

fn display_output_info(raw_transaction_info: &GetRawTransactionResult, vout_index: usize) {
    if let Some(vout) = raw_transaction_info.vout.get(vout_index) {
        println!("----------------------------------------------");
        println!("Vout {}", vout_index);
        println!("- Value: {:?}", vout.value);
        println!("- N: {:?}", vout.n);

        let script_pub_key = &vout.script_pub_key;
        println!("- Script Pub Key:");
        println!("    - ASM: {:?}", script_pub_key.asm);
        println!("    - Hex: {:?}", script_pub_key.hex);
        println!("    - Required Signatures: {:?}", script_pub_key.req_sigs);
        println!("    - Type: {:?}", script_pub_key.type_);
        println!("    - Addresses: {:?}", script_pub_key.addresses);
        println!("    - Address: {:?}", script_pub_key.address);

        println!();
    } else {
        println!("Invalid vout index: {}", vout_index);
    }
    println!();
}
