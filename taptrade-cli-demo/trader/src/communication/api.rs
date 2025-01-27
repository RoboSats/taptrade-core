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
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BondRequirementResponse {
	pub bond_address: String, // address the bond ha/workspaces/taptrade-core/taptrade-cli-demo/trader/src/communications to be locked to
	pub locking_amount_sat: u64, // min amount of the bond output in sat
	pub escrow_locking_input_amount_without_trade_sum: u64, // minimum required amount of input to the escrow tx
}

// maker step 2
// (submission of signed bond and other data neccessary to coordinate the trade)
#[derive(Serialize, Debug)]
pub struct BondSubmissionRequest {
	pub robohash_hex: String,
	pub signed_bond_hex: String,    // signed bond transaction, hex encoded
	pub payout_address: String,     // does this make sense here?
	pub taproot_pubkey_hex: String, // used for script path spending
	pub musig_pub_nonce_hex: String,
	pub musig_pubkey_hex: String, // used for key path spending
	pub bdk_psbt_inputs_hex_csv: String,
	pub client_change_address: String,
}

// Response after step2 if offer creation was successful and the offer is now online in the orderbook
#[derive(Debug, Deserialize)]
pub struct OrderActivatedResponse {
	pub offer_id_hex: String,
	pub bond_locked_until_timestamp: u64, // unix timestamp. Do not touch bond till then unless offer gets taken.
}

#[derive(Debug, Serialize)]
pub struct OfferTakenRequest {
	pub robohash_hex: String,
	pub offer_id_hex: String,
}

#[derive(Debug, Deserialize)]
pub struct OfferTakenResponse {
	pub escrow_psbt_hex: String,
	pub escrow_output_descriptor: String,
	pub escrow_amount_maker_sat: u64,
	pub escrow_amount_taker_sat: u64,
	pub escrow_fee_sat_per_participant: u64,
}

// Taker structures //

// request all fitting offers from the coordinator
#[derive(Debug, Serialize)]
pub struct OffersRequest {
	pub buy_offers: bool, // true if looking for buy offers, false if looking for sell offers
	pub amount_min_sat: u64,
	pub amount_max_sat: u64,
}

// response of the coordinator, containing all fitting offers to the OffersRequest request
#[derive(Debug, Deserialize)]
pub struct PublicOffers {
	pub offers: Option<Vec<PublicOffer>>, // don't include offers var in return json if no offers are available
}

// Offer information of each offer returned by the previous response
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PublicOffer {
	pub amount_sat: u64,
	pub offer_id_hex: String,
	pub bond_requirements: BondRequirementResponse,
}

// request to receive the escrow psbt to sign for the specified offer to take it
#[derive(Debug, Serialize)]
pub struct OfferPsbtRequest {
	pub offer: PublicOffer,
	pub trade_data: BondSubmissionRequest,
}

// submit signed escrow psbt back to coordinator in a Json like this
#[derive(Debug, Serialize)]
pub struct PsbtSubmissionRequest {
	pub signed_psbt_hex: String,
	pub offer_id_hex: String,
	pub robohash_hex: String,
}

// request polled to check if the maker has submitted his escrow transaction
// and the escrow transaction is confirmed once this returns 200 the chat can open
#[derive(Debug, Serialize)]
pub struct IsOfferReadyRequest {
	pub robohash_hex: String,
	pub offer_id_hex: String,
}

// request posted by both parties when the trade obligations
#[derive(Debug, Serialize)]
pub struct TradeObligationsSatisfied {
	pub robohash_hex: String,
	pub offer_id_hex: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PayoutResponse {
	pub payout_psbt_hex: String,
	pub agg_musig_nonce_hex: String,
	pub agg_musig_pubkey_ctx_hex: String,
}
#[derive(Debug, Serialize)]
pub struct TradeObligationsUnsatisfied {
	pub robohash_hex: String,
	pub offer_id_hex: String,
}

#[derive(Debug, Serialize)]
pub struct PayoutSignatureRequest {
	pub partial_sig_hex: String,
	pub offer_id_hex: String,
	pub robohash_hex: String,
}
