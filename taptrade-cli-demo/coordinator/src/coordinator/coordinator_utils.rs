use super::*;
use anyhow::Context;
use bdk::{
	bitcoin::{
		hashes::Hash,
		key::XOnlyPublicKey,
		psbt::{PartiallySignedTransaction, Prevouts},
		sighash::{SighashCache, TapSighashType},
		Address,
	},
	miniscript::Descriptor,
};
use musig2::{BinaryEncoding, LiftedSignature};
use std::str::FromStr;

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

#[derive(Debug, Clone)]
pub struct KeyspendContext {
	pub agg_sig: LiftedSignature,
	pub agg_nonce: MusigAggNonce,
	pub agg_keyspend_pk: KeyAggContext,
	pub keyspend_psbt: PartiallySignedTransaction,
}

pub fn agg_hex_musig_nonces(maker_nonce: &str, taker_nonce: &str) -> Result<MusigAggNonce> {
	let musig_pub_nonce_maker = match MusigPubNonce::from_hex(maker_nonce) {
		Ok(musig_pub_nonce_maker) => musig_pub_nonce_maker,
		Err(e) => {
			return Err(anyhow!(
				"Error decoding maker musig pub nonce: {}",
				e.to_string()
			))
		}
	};
	let musig_pub_nonce_taker = match MusigPubNonce::from_hex(taker_nonce) {
		Ok(musig_pub_nonce_taker) => musig_pub_nonce_taker,
		Err(e) => {
			return Err(anyhow!(
				"Error decoding taker musig pub nonce: {}",
				e.to_string()
			))
		}
	};

	let agg_nonce = musig2::AggNonce::sum([musig_pub_nonce_maker, musig_pub_nonce_taker]);

	Ok(agg_nonce)
}

impl KeyspendContext {
	pub fn from_hex_str(
		maker_sig: &str,
		taker_sig: &str,
		maker_nonce: &str,
		taker_nonce: &str,
		maker_pk: &str,
		taker_pk: &str,
		keyspend_psbt: &str,
	) -> anyhow::Result<Self> {
		let agg_keyspend_pk: musig2::KeyAggContext =
			wallet::aggregate_musig_pubkeys(maker_pk, taker_pk)?;
		let agg_nonce: MusigAggNonce =
			coordinator_utils::agg_hex_musig_nonces(maker_nonce, taker_nonce)?;
		let keyspend_psbt = PartiallySignedTransaction::from_str(keyspend_psbt)?;

		let partial_maker_sig = PartialSignature::from_hex(maker_sig)?;
		let partial_taker_sig = PartialSignature::from_hex(taker_sig)?;
		let partial_signatures = vec![partial_maker_sig, partial_taker_sig];

		// let msg = keyspend_psbt.
		let msg = {
			let mut sig_hash_cache = SighashCache::new(keyspend_psbt.unsigned_tx.clone());

			let utxo = keyspend_psbt
				.iter_funding_utxos()
				.next()
				.ok_or(anyhow!("No UTXO found in payout psbt"))??
				.clone();

			// get the msg (sighash) to sign with the musig key
			let binding = sig_hash_cache
				.taproot_key_spend_signature_hash(0, &Prevouts::All(&[utxo]), TapSighashType::All)
				.context("Failed to create keyspend sighash")?;
			binding.as_byte_array().to_vec()
		};

		let agg_sig: LiftedSignature = musig2::aggregate_partial_signatures(
			&agg_keyspend_pk,
			&agg_nonce,
			partial_signatures,
			msg.as_slice(),
		)
		.context("Aggregating partial signatures failed")?;

		Ok(Self {
			agg_sig,
			agg_nonce,
			agg_keyspend_pk,
			keyspend_psbt,
		})
	}
}

impl PayoutData {
	#[allow(clippy::too_many_arguments)]
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
		let aggregated_musig_pubkey_ctx_hex = hex::encode(
			aggregate_musig_pubkeys(musig_pk_hex_maker, musig_pk_hex_taker)?.to_bytes(),
		);

		let agg_musig_nonce: MusigAggNonce =
			agg_hex_musig_nonces(musig_pub_nonce_hex_maker, musig_pub_nonce_hex_taker)?;

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
