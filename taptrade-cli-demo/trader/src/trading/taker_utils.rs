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

		// now we submit the signed bond transaction to the coordinator and receive the escrow PSBT we have to sign
		// in exchange
		let bond_submission_request = BondSubmissionRequest::prepare_bond_request(
			&bond,
			&payout_address,
			&mut musig_data,
			taker_config,
			&trading_wallet.taproot_pubkey,
		)?;
		let escrow_contract_requirements =
			OfferPsbtRequest::taker_request(offer, bond_submission_request, taker_config)?;

		// now we have to verify, sign and submit the escrow psbt again
		let escrow_contract_psbt =
			trading_wallet.get_escrow_psbt(escrow_contract_requirements, taker_config)?;

		// submit signed escrow psbt back to coordinator
		PsbtSubmissionRequest::submit_escrow_psbt(
			&escrow_contract_psbt,
			offer.offer_id_hex.clone(),
			taker_config,
		)?;

		Ok(ActiveOffer {
			offer_id_hex: offer.offer_id_hex.clone(),
			used_musig_config: musig_data,
			used_bond: bond,
			expected_payout_address: payout_address,
			escrow_psbt: Some(escrow_contract_psbt),
		})
	}
}
