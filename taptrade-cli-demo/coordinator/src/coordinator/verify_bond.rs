use bdk::bitcoin::consensus::encode::deserialize;
use bdk::bitcoin::psbt::PartiallySignedTransaction;
use bdk::bitcoin::Transaction;
use bdk::database::MemoryDatabase;
use bdk::SignOptions;
use bdk::Wallet;

use crate::communication::api::BondSubmissionRequest;
use crate::communication::api::OrderActivatedResponse;

use anyhow::{anyhow, Result};
use hex;

pub fn verify_and_respond(
	bond_submission: BondSubmissionRequest,
	wallet: &Wallet<MemoryDatabase>,
) -> Result<OrderActivatedResponse> {
	// Deserialize the signed bond hex
	let tx: Transaction = deserialize(hex::decode(bond_submission.signed_bond_hex)?.as_slice())?;

	// Verify the transaction (this example assumes you've implemented your own verification logic)
	let is_valid = verify_psbt(&tx, &wallet, &bond_submission)?;
	if !is_valid {
		return Err(anyhow!("Invalid PSBT"));
	}

	// Create the response (you may need additional logic to generate order_id_hex and timestamp)
	let response = OrderActivatedResponse {
		order_id_hex: generate_order_id(&tx)?, // Assuming you have a function to generate this
		bond_locked_until_timestamp: calculate_bond_lock_time()?, // Assuming you have a function for this
	};

	Ok(response)
}

pub fn verify_psbt(
	tx: &Transaction,
	wallet: &Wallet<MemoryDatabase>,
	bond_submission: &BondSubmissionRequest,
) -> Result<bool> {
	// Example verification logic
	// Check if the payout address matches
	// let payout_address = bond_submission.payout_address.parse();
	// let output = tx.output.iter().find(|output| outputvc
	//     .script_pubkey == payout_address.script_pubkey());
	// if output.is_none() {
	//     return Ok(false);
	// }

	// Check if the transaction is signed correctly
	let mut psbt = PartiallySignedTransaction::from_unsigned_tx(tx.clone())?;
	let finalized = wallet.sign(&mut psbt, SignOptions::default())?;
	if !finalized {
		return Ok(false);
	}

	// Validate MuSig data (assuming you have methods for this)
	let musig_data_valid = validate_musig_data(
		&bond_submission.musig_pubkey_hex,
		&bond_submission.musig_pub_nonce_hex,
	)?;
	if !musig_data_valid {
		return Ok(false);
	}

	Ok(true)
}

fn generate_order_id(tx: &Transaction) -> Result<String> {
	// Example logic to generate an order ID from the transaction
	Ok(tx.txid().to_string())
}

fn calculate_bond_lock_time() -> Result<u128> {
	// Example logic to calculate the bond lock time
	// This might depend on the current block height or a specific timestamp
	Ok(12345678901234567890) // Placeholder value
}

fn validate_musig_data(pubkey_hex: &str, nonce_hex: &str) -> Result<bool> {
	// Example logic to validate MuSig data
	// This might involve parsing the hex strings and ensuring they match expected values
	Ok(true) // Placeholder validation
}
