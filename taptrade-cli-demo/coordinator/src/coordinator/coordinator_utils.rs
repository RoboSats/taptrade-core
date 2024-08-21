use bitcoin::secp256k1::Scalar;
use hex::ToHex;

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
		descriptor: &str,
	) -> anyhow::Result<Self> {
		let tweak = get_keyspend_tweak_scalar(descriptor)?;
		let agg_keyspend_pk: musig2::KeyAggContext =
			aggregate_musig_pubkeys_with_tweak(maker_pk, taker_pk, tweak)?;
		let agg_nonce: MusigAggNonce =
			coordinator_utils::agg_hex_musig_nonces(maker_nonce, taker_nonce)?;
		let keyspend_psbt = PartiallySignedTransaction::deserialize(&hex::decode(keyspend_psbt)?)?;

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

			let sighash_type = keyspend_psbt.inputs[0].taproot_hash_ty()?;
			// get the msg (sighash) to sign with the musig key
			let binding = sig_hash_cache
				.taproot_key_spend_signature_hash(0, &Prevouts::All(&[utxo]), sighash_type)
				.context("Failed to create keyspend sighash")?;
			binding.to_raw_hash()
		};

		let agg_sig: LiftedSignature = musig2::aggregate_partial_signatures(
			&agg_keyspend_pk,
			&agg_nonce,
			partial_signatures,
			msg,
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

fn get_keyspend_tweak_scalar(descriptor: &str) -> Result<bdk::bitcoin::secp256k1::Scalar> {
	let tr_descriptor: Descriptor<XOnlyPublicKey> =
		bdk::descriptor::Descriptor::from_str(descriptor)?;
	let spend_info = if let Descriptor::Tr(tr) = tr_descriptor {
		tr.spend_info()
	} else {
		return Err(anyhow!(
			"Descriptor {descriptor} is not a taproot descriptor"
		));
	};
	let tweak = spend_info.tap_tweak();
	debug!(
		"Internal key: {}, Tweaked (outer) key: {}, Tweak: {}",
		spend_info.internal_key(),
		spend_info.output_key(),
		tweak
	);
	Ok(tweak.to_scalar())
}

pub fn aggregate_musig_pubkeys_with_tweak(
	maker_musig_pubkey: &str,
	taker_musig_pubkey: &str,
	tweak_scalar: bdk::bitcoin::secp256k1::Scalar,
) -> Result<KeyAggContext> {
	let musig_scalar = musig2::secp256k1::Scalar::from_be_bytes(tweak_scalar.to_be_bytes())?;
	let pubkeys: [MuSig2PubKey; 2] = [
		MuSig2PubKey::from_str(maker_musig_pubkey).context("Error parsing musig pk 1")?,
		MuSig2PubKey::from_str(taker_musig_pubkey).context("Error parsing musig pk 2")?,
	];

	let key_agg_ctx = KeyAggContext::new(pubkeys)
		.context("Error aggregating musig pubkeys")?
		.with_tweak(musig_scalar, true)?;
	debug!(
		"Aggregating musig pubkeys: {} and {} to {:?}",
		maker_musig_pubkey, taker_musig_pubkey, key_agg_ctx
	);
	Ok(key_agg_ctx)
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
		let tweak = get_keyspend_tweak_scalar(escrow_output_descriptor)?;
		let aggregated_musig_pubkey_ctx_hex = hex::encode(
			aggregate_musig_pubkeys_with_tweak(musig_pk_hex_maker, musig_pk_hex_taker, tweak)?
				.to_bytes(),
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
