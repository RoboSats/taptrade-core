use std::str::FromStr;

use anyhow::Context;
use bdk::{
	bitcoin::{key::XOnlyPublicKey, Address},
	miniscript::Descriptor,
};
use musig2::BinaryEncoding;

use super::*;

#[derive(Debug)]
pub enum PayoutProcessingResult {
	ReadyPSBT(PayoutResponse),
	NotReady,
	LostEscrow,
	DecidingEscrow,
}

#[derive(Debug)]
pub struct PayoutData {
	pub escrow_output_descriptor: Descriptor<XOnlyPublicKey>,
	pub payout_address_maker: Address,
	pub payout_address_taker: Address,
	pub payout_amount_maker: u64,
	pub payout_amount_taker: u64,
	pub agg_musig_nonce: MusigAggNonce,
	pub aggregated_musig_pubkey_ctx_hex: String,
}

impl PayoutData {
	pub fn new_from_strings(
		escrow_output_descriptor: &str,
		payout_address_maker: &str,
		payout_address_taker: &str,
		payout_amount_maker: u64,
		payout_amount_taker: u64,
		musig_pub_nonce_hex_maker: &str,
		musig_pub_nonce_hex_taker: &str,
		musig_pk_hex_maker: &str,
		musig_pk_hex_taker: &str,
	) -> Result<Self> {
		let musig_pub_nonce_maker = match MusigPubNonce::from_hex(musig_pub_nonce_hex_maker) {
			Ok(musig_pub_nonce_maker) => musig_pub_nonce_maker,
			Err(e) => {
				return Err(anyhow!(
					"Error decoding maker musig pub nonce: {}",
					e.to_string()
				))
			}
		};
		let musig_pub_nonce_taker = match MusigPubNonce::from_hex(musig_pub_nonce_hex_taker) {
			Ok(musig_pub_nonce_taker) => musig_pub_nonce_taker,
			Err(e) => {
				return Err(anyhow!(
					"Error decoding taker musig pub nonce: {}",
					e.to_string()
				))
			}
		};

		let aggregated_musig_pubkey_ctx_hex = hex::encode(
			aggregate_musig_pubkeys(musig_pk_hex_maker, musig_pk_hex_taker)?.to_bytes(),
		);

		let agg_musig_nonce: MusigAggNonce =
			musig2::AggNonce::sum([musig_pub_nonce_maker, musig_pub_nonce_taker]);

		Ok(Self {
			escrow_output_descriptor: Descriptor::from_str(escrow_output_descriptor)?,
			payout_address_maker: Address::from_str(payout_address_maker)?
				.require_network(bdk::bitcoin::Network::Regtest)
				.context("Maker payout address wrong network")?,
			payout_address_taker: Address::from_str(payout_address_taker)?
				.require_network(bdk::bitcoin::Network::Regtest)
				.context("Taker payout address wrong Network")?,
			payout_amount_maker,
			payout_amount_taker,
			agg_musig_nonce,
			aggregated_musig_pubkey_ctx_hex,
		})
	}
}

pub fn generate_random_order_id(len: usize) -> String {
	// Generate `len` random bytes
	let bytes: Vec<u8> = rand::thread_rng()
		.sample_iter(&rand::distributions::Standard)
		.take(len)
		.collect();

	// Convert bytes to hex string
	hex::encode(bytes)
}

pub async fn check_offer_and_confirmation(
	offer_id_hex: &str,
	robohash_hex: &str,
	database: &CoordinatorDB,
) -> Result<(), RequestError> {
	// sanity check if offer is in table
	match database
		.is_valid_robohash_in_table(robohash_hex, offer_id_hex)
		.await
	{
		Ok(false) => return Err(RequestError::NotFound),
		Ok(true) => (),
		Err(e) => return Err(RequestError::Database(e.to_string())),
	};

	// sanity check if the escrow tx is confirmed
	match database
		.fetch_escrow_tx_confirmation_status(offer_id_hex)
		.await
	{
		Ok(false) => Err(RequestError::NotConfirmed),
		Ok(true) => Ok(()),
		Err(e) => Err(RequestError::Database(e.to_string())),
	}
}
