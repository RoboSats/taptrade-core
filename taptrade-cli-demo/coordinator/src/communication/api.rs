/// This module contains the API structures used for communication in the coordinator.
///
/// The `OfferRequest` struct represents a request to create an offer. It contains the following fields:
/// - `robohash_hex`: The identifier of the trader.
/// - `amount_satoshi`: The amount in satoshi to buy or sell.
/// - `is_buy_order`: A boolean indicating whether it is a buy order or a sell order.
/// - `bond_ratio`: The percentage of the trading amount to be used as a bond.
/// - `offer_duration_ts`: The unix timestamp indicating how long the offer should stay available.
///
/// The `BondRequirementResponse` struct represents the response containing bond requirements. It has the following fields:
/// - `bond_address`: The bond address.
/// - `locking_amount_sat`: The minimum amount of the bond output in satoshi.
///
/// The `BondSubmissionRequest` struct represents a request to submit a bond. It contains the following fields:
/// - `robohash_hex`: The identifier of the trader.
/// - `signed_bond_hex`: The signed bond transaction in hex format.
/// - `payout_address`: The payout address.
/// - `taproot_pubkey_hex`: The taproot public key in hex format.
/// - `musig_pub_nonce_hex`: The musig public nonce in hex format.
/// - `musig_pubkey_hex`: The musig public key in hex format.
/// - `bdk_psbt_inputs_hex_csv`: The bdk psbt inputs in hex format.
/// - `client_change_address`: The client change address.
///
/// The `OfferActivatedResponse` struct represents the response after successfully activating an offer. It has the following fields:
/// - `offer_id_hex`: The offer ID in hex format.
/// - `bond_locked_until_timestamp`: The unix timestamp until which the bond should not be touched unless the offer gets taken.
///
/// The `OffersRequest` struct represents a request to get offers. It contains the following fields:
/// - `buy_offers`: A boolean indicating whether to look for buy offers or sell offers.
/// - `amount_min_sat`: The minimum amount in satoshi.
/// - `amount_max_sat`: The maximum amount in satoshi.
///
/// The `PublicOffer` struct represents information about a public offer. It has the following fields:
/// - `amount_sat`: The amount in satoshi.
/// - `offer_id_hex`: The offer ID in hex format.
/// - `required_bond_amount_sat`: The required bond amount in satoshi.
/// - `bond_locking_address`: The bond locking address.
///
/// The `PublicOffers` struct represents a collection of public offers. It has the following field:
/// - `offers`: An optional vector of `PublicOffer` structs. This field is not included in the return JSON if no offers are available.
///
/// The `OfferTakenResponse` struct represents the response after taking an offer. It has the following fields:
/// - `escrow_psbt_hex`: The escrow PSBT in hex format.
/// - `escrow_output_descriptor`: The escrow output descriptor.
/// - `escrow_amount_maker_sat`: The escrow amount for the maker in satoshi.
/// - `escrow_amount_taker_sat`: The escrow amount for the taker in satoshi.
/// - `escrow_fee_sat_per_participant`: The escrow fee in satoshi per participant.
///
/// The `OfferPsbtRequest` struct represents a request to receive the escrow PSBT for a specified offer. It contains the following fields:
/// - `offer`: The `PublicOffer` struct representing the offer.
/// - `trade_data`: The `BondSubmissionRequest` struct representing the trade data.
///
/// The `OfferTakenRequest` struct represents a request to take an offer. It contains the following fields:
/// - `robohash_hex`: The identifier of the trader.
/// - `offer_id_hex`: The offer ID in hex format.
///
/// The `PsbtSubmissionRequest` struct represents a request to submit a PSBT. It contains the following fields:
/// - `signed_psbt_hex`: The signed PSBT in hex format.
/// - `offer_id_hex`: The offer ID in hex format.
/// - `robohash_hex`: The identifier of the trader.
///
/// The `PayoutResponse` struct represents the response after a payout. It has the following fields:
/// - `payout_psbt_hex`: The payout PSBT in hex format.
/// - `agg_musig_nonce_hex`: The aggregated musig nonce in hex format.
/// - `agg_musig_pubkey_ctx_hex`: The aggregated musig public key context in hex format.
///
/// The `TradeObligationsUnsatisfied` struct represents unsatisfied trade obligations. It has the following fields:
/// - `robohash_hex`: The identifier of the trader.
/// - `offer_id_hex`: The offer ID in hex format.
///
/// The `PayoutSignatureRequest` struct represents a request for a payout signature. It contains the following fields:
/// - `partial_sig_hex`: The partial signature in hex format.
/// - `offer_id_hex`: The offer ID in hex format.
/// - `robohash_hex`: The identifier of the trader.
use super::*;

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

#[derive(Serialize, PartialEq, Debug, Validate, Deserialize)]
pub struct BondRequirementResponse {
	pub bond_address: String,
	pub locking_amount_sat: u64, // min amount of the bond output in sat
	pub escrow_locking_input_amount_without_trade_sum: u64,
}

// maker step 2
#[derive(Deserialize, Serialize, Debug)]
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
	pub bond_requirements: BondRequirementResponse,
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
