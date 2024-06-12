use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct OfferCreationResponse {
	pub bond_address: String,
	pub locking_amount: u64, // validate
}

#[derive(Serialize)]
pub struct OrderRequest {
	pub robohash_base91: String,
	pub amount_satoshi: u64,
	pub order_type: String, // buy or sell
	pub bond_ratio: u8,     // [2, 50]
}

#[derive(Serialize)]
pub struct BondSubmissionRequest {
	pub robohash_base91: String,
	pub signed_bond_base91: String,
	pub payout_address: String, // does this make sense here?
	pub musig_pub_nonce_base91: String,
	pub musig_pubkey_base91: String,
}
