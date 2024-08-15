use super::*;

// Receiving this struct as input to the server
#[derive(Deserialize, Serialize, Debug, Validate)]
pub struct OfferRequest {
	pub robohash_hex: String, // identifier of the trader
	#[validate(range(min = 10000, max = 20000000))]
	pub amount_satoshi: u64, // amount in satoshi to buy or sell
	pub is_buy_order: bool,   // true if buy, false if sell
	#[validate(range(min = 2, max = 50))]
	pub bond_ratio: u8, // [2, 50]% of trading amount
	#[validate(custom(function = "validate_timestamp"))]
	pub offer_duration_ts: u64, // unix timestamp how long the offer should stay available
}

// Define a struct representing your response data
#[derive(Serialize, PartialEq, Debug, Validate)]
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
	pub taproot_pubkey_hex: String,
	pub musig_pub_nonce_hex: String,
	pub musig_pubkey_hex: String,
	pub bdk_psbt_inputs_hex_csv: String,
	pub client_change_address: String,
}

// Response after step2 if offer creation was successful and the offer is now online in the orderbook
#[derive(Serialize)]
pub struct OfferActivatedResponse {
	pub offer_id_hex: String,
	pub bond_locked_until_timestamp: u64, // unix timestamp. Do not touch bond till then unless offer gets taken.
}

#[derive(Deserialize, Serialize, Debug)]
pub struct OffersRequest {
	pub buy_offers: bool, // true if looking for buy offers, false if looking for sell offers
	pub amount_min_sat: u64,
	pub amount_max_sat: u64,
}

// Offer information of each offer returned by the previous response
#[derive(Deserialize, Serialize, Debug)]
pub struct PublicOffer {
	pub amount_sat: u64,
	pub offer_id_hex: String,
	pub required_bond_amount_sat: u64,
	pub bond_locking_address: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct PublicOffers {
	pub offers: Option<Vec<PublicOffer>>, // don't include offers var in return json if no offers are available
}

#[derive(Serialize, Debug, Deserialize)]
pub struct OfferTakenResponse {
	pub escrow_psbt_hex: String,
	pub escrow_output_descriptor: String,
	pub escrow_amount_maker_sat: u64,
	pub escrow_amount_taker_sat: u64,
	pub escrow_fee_sat_per_participant: u64,
}

// request to receive the escrow psbt to sign for the specified offer to take it
#[derive(Debug, Serialize, Deserialize)]
pub struct OfferPsbtRequest {
	pub offer: PublicOffer,
	pub trade_data: BondSubmissionRequest,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OfferTakenRequest {
	pub robohash_hex: String,
	pub offer_id_hex: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PsbtSubmissionRequest {
	pub signed_psbt_hex: String,
	pub offer_id_hex: String,
	pub robohash_hex: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PayoutResponse {
	pub payout_psbt_hex: String,
	pub agg_musig_nonce_hex: String,
	pub agg_musig_pubkey_ctx_hex: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TradeObligationsUnsatisfied {
	pub robohash_hex: String,
	pub offer_id_hex: String,
}

#[derive(Debug, Deserialize)]
pub struct PayoutSignatureRequest {
	pub partial_sig_hex: String,
	pub offer_id_hex: String,
	pub robohash_hex: String,
}
