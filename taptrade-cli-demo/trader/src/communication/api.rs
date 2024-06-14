use super::*;

// maker step 1
// requesting to create an offer on the orderbook (POST request)
#[derive(Serialize)]
pub struct OrderRequest {
	pub robohash_hex: String,   // identifier of the trader
	pub amount_satoshi: u64,    // amount in satoshi to buy or sell
	pub is_buy_order: bool,     // true if buy, false if sell
	pub bond_ratio: u8,         // [2, 50]% of trading amount
	pub offer_duration_ts: u64, // unix timestamp how long the offer should stay available
}

// coordinator answer to maker step 1
// direct Json answer to step 1 (same request)
#[derive(Debug, Deserialize)]
pub struct OfferCreationResponse {
	pub bond_address: String, // address the bond ha/workspaces/taptrade-core/taptrade-cli-demo/trader/src/communications to be locked to
	pub locking_amount_sat: u64, // min amount of the bond output in sat
}

// maker step 2
// (submission of signed bond and other data neccessary to coordinate the trade)
#[derive(Serialize)]
pub struct BondSubmissionRequest {
	pub robohash_hex: String,
	pub signed_bond_hex: String, // signed bond transaction, hex encoded
	pub payout_address: String,  // does this make sense here?
	pub musig_pub_nonce_hex: String,
	pub musig_pubkey_hex: String,
}

// Response after step2 if offer creation was successful and the offer is now online in the orderbook
#[derive(Debug, Deserialize)]
pub struct OrderActivatedResponse {
	pub order_id_hex: String,
	pub bond_locked_until_timestamp: u128, // unix timestamp. Do not touch bond till then unless offer gets taken.
}

#[derive(Debug, Serialize)]
pub struct OfferTakenRequest {
	pub robohash_hex: String,
	pub order_id_hex: String,
}

#[derive(Debug, Deserialize)]
pub struct OfferTakenResponse {
	pub trade_psbt_hex_to_sign: String,
}

// Taker structures

#[derive(Debug, Serialize)]
pub struct OffersRequest {
	pub buy_offers: bool, // true if looking for buy offers, false if looking for sell offers
	pub amount_min_sat: u64,
	pub amount_max_sat: u64,
}

#[derive(Debug, Deserialize)]
pub struct PublicOffer {
	pub amount_sat: u64,
	pub offer_id_hex: String,
}

#[derive(Debug, Deserialize)]
pub struct PublicOffers {
	pub offers: Option<Vec<PublicOffer>>, // don't include offers var in return json if no offers are available
}
