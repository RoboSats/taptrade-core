use super::*;

// Receiving this struct as input to the server
#[derive(Deserialize, Serialize, Debug)]
pub struct OrderRequest {
	pub robohash_hex: String,   // identifier of the trader
	pub amount_satoshi: u64,    // amount in satoshi to buy or sell
	pub is_buy_order: bool,     // true if buy, false if sell
	pub bond_ratio: u8,         // [2, 50]% of trading amount
	pub offer_duration_ts: u64, // unix timestamp how long the offer should stay available
}

// Define a struct representing your response data
#[derive(Serialize)]
pub struct BondRequirementResponse {
	pub bond_address: String,
	pub locking_amount_sat: u64, // min amount of the bond output in sat
}

// maker step 2
// (submission of signed bond and other data neccessary to coordinate the trade)
#[derive(Deserialize, Serialize, Debug)]
pub struct BondSubmissionRequest {
	pub robohash_hex: String,
	pub signed_bond_hex: String, // signed bond transaction, hex encoded
	pub payout_address: String,  // does this make sense here?
	pub musig_pub_nonce_hex: String,
	pub musig_pubkey_hex: String,
}

// Response after step2 if offer creation was successful and the offer is now online in the orderbook
#[derive(Serialize)]
pub struct OrderActivatedResponse {
	pub order_id_hex: String,
	pub bond_locked_until_timestamp: u128, // unix timestamp. Do not touch bond till then unless offer gets taken.
}
