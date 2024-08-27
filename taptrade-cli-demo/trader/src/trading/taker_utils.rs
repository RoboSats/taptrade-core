use bdk::bitcoin::consensus::encode::serialize_hex;
use bdk::electrum_client::Request;

use crate::communication::api::{IsOfferReadyRequest, OfferPsbtRequest, PsbtSubmissionRequest};

use super::utils::*;
use super::*;

impl ActiveOffer {
	pub fn take(
		trading_wallet: &TradingWallet,
		taker_config: &TraderSettings,
		offer: &PublicOffer,
	) -> Result<ActiveOffer> {
		let bond_requirements = BondRequirementResponse {
			bond_address: offer.bond_locking_address.clone(),
			locking_amount_sat: offer.required_bond_amount_sat,
		};

		// assembly of the Bond transaction and generation of MuSig data and payout address
		let (bond, mut musig_data, payout_address) =
			trading_wallet.trade_onchain_assembly(&bond_requirements, taker_config)?;

		// get inputs and a change address necessary for the coordinator to assemble the escrow locking psbt
		let (bdk_psbt_inputs_hex_csv, client_change_address) =
			trading_wallet.get_escrow_psbt_inputs(bond_requirements.locking_amount_sat as i64)?;

		let bond_submission_request = BondSubmissionRequest {
			robohash_hex: taker_config.robosats_robohash_hex.clone(),
			signed_bond_hex: serialize_hex(&bond.clone().extract_tx()),
			payout_address: payout_address.address.to_string(),
			taproot_pubkey_hex: trading_wallet.taproot_pubkey.to_string(),
			musig_pub_nonce_hex: musig_data.nonce.get_pub_for_sharing()?.to_string(),
			musig_pubkey_hex: hex::encode(musig_data.public_key.serialize()),
			bdk_psbt_inputs_hex_csv: bdk_psbt_inputs_hex_csv.clone(),
			client_change_address: client_change_address.clone(),
		};

		// now we submit the signed bond transaction to the coordinator and receive the escrow PSBT we have to sign
		// in exchange
		let escrow_contract_requirements =
			OfferPsbtRequest::taker_request(offer, bond_submission_request, taker_config)?;

		let mut escrow_psbt =
			PartiallySignedTransaction::from_str(&escrow_contract_requirements.escrow_psbt_hex)?;

		// now we have to verify, sign and submit the escrow psbt again
		trading_wallet
			.validate_escrow_psbt(&escrow_psbt)?
			.sign_escrow_psbt(&mut escrow_psbt)?;

		// submit signed escrow psbt back to coordinator
		PsbtSubmissionRequest::submit_escrow_psbt(
			&escrow_psbt,
			offer.offer_id_hex.clone(),
			taker_config,
		)?;

		// offer is now active
		Ok(ActiveOffer {
			offer_id_hex: offer.offer_id_hex.clone(),
			used_musig_config: musig_data,
			used_bond: bond,
			expected_payout_address: payout_address,
			escrow_psbt: Some(escrow_psbt),
			psbt_inputs_hex_csv: bdk_psbt_inputs_hex_csv,
			escrow_change_address: client_change_address,
		})
	}
}
