use bdk::bitcoin::amount::serde::as_btc::opt::serialize;
use bdk::bitcoin::consensus::encode::serialize_hex;
use bdk::bitcoin::Transaction;
use serde::Serialize;

use super::utils::*;
use super::*;

impl ActiveOffer {
	pub fn create(
		trading_wallet: &TradingWallet,
		maker_config: &TraderSettings,
	) -> Result<ActiveOffer> {
		// fetches the bond requirements necessary to assemble the bond for the requested offer
		let offer_conditions = BondRequirementResponse::fetch(maker_config)?;
		debug!("Offer conditions fetched: {:#?}", &offer_conditions);
		// assembles the bond required by the coordinator, also generates the musig data (keys, nonces) and a payout address
		// which are being submitted to the coordinator for the further trade
		let (bond, mut musig_data, payout_address) =
			trading_wallet.trade_onchain_assembly(&offer_conditions, maker_config)?;

		// get necessary data for the coordinator to assemble the escrow locking psbt (inputs owned by maker, change address)
		let (psbt_inputs_hex_csv, escrow_change_address) =
			trading_wallet.get_escrow_psbt_inputs(offer_conditions.locking_amount_sat as i64)?;

		debug!(
			"Submitting maker bond: {:#?}",
			hex::encode(bond.serialize())
		);
		let bond_submission_request = BondSubmissionRequest {
			robohash_hex: maker_config.robosats_robohash_hex.clone(),
			signed_bond_hex: serialize_hex(&bond.clone().extract_tx()),
			payout_address: payout_address.address.to_string(),
			musig_pub_nonce_hex: hex::encode(musig_data.nonce.get_pub_for_sharing()?.serialize()),
			musig_pubkey_hex: hex::encode(musig_data.public_key.serialize()),
			taproot_pubkey_hex: hex::encode(trading_wallet.taproot_pubkey.serialize()),
			bdk_psbt_inputs_hex_csv: psbt_inputs_hex_csv.clone(),
			client_change_address: escrow_change_address.clone(),
		};

		// send the bond submission request to the coordinator, returns submission result with offer id and unix timestamp of bond lock
		let submission_result = bond_submission_request.send_maker(maker_config)?;
		Ok(ActiveOffer {
			offer_id_hex: submission_result.offer_id_hex,
			used_musig_config: musig_data,
			used_bond: bond,
			expected_payout_address: payout_address,
			escrow_psbt: None,
			psbt_inputs_hex_csv,
			escrow_change_address,
		})
	}

	// polling until offer is taken, in production a more efficient way would make sense
	// returns the PSBT of the escrow trade transaction we have to validate, sign and return
	pub fn wait_until_taken(&self, trader_config: &TraderSettings) -> Result<OfferTakenResponse> {
		loop {
			thread::sleep(Duration::from_secs(2));
			if let Some(offer_taken_response) = OfferTakenResponse::check(self, trader_config)? {
				return Ok(offer_taken_response);
			}
		}
	}
}
