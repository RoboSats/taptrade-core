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
		// fetching the bond requirements for the requested Offer (amount, locking address)
		let bond_conditions: BondRequirementResponse = offer.request_bond(taker_config)?;

		// assembly of the Bond transaction and generation of MuSig data and payout address
		let (bond, mut musig_data, payout_address) =
			trading_wallet.trade_onchain_assembly(&bond_conditions, taker_config)?;

		// now we submit the signed bond transaction to the coordinator and receive the escrow PSBT we have to sign
		// in exchange
		let bond_submission_request = BondSubmissionRequest::prepare_bond_request(
			&bond,
			&payout_address,
			&mut musig_data,
			taker_config,
		)?;
		let mut escrow_contract_psbt =
			OfferPsbtRequest::taker_request(offer, bond_submission_request, taker_config)?;

		// now we have to verify, sign and submit the escrow psbt again
		trading_wallet
			.validate_taker_psbt(&escrow_contract_psbt)?
			.sign_escrow_psbt(&mut escrow_contract_psbt)?;

		// submit signed escrow psbt back to coordinator
		PsbtSubmissionRequest::submit_taker_psbt(
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

	pub fn wait_on_maker(self, taker_config: &TraderSettings) -> Result<Self> {
		IsOfferReadyRequest::poll(taker_config, &self)?;
		Ok(self)
	}

	pub fn wait_on_fiat_confirmation(&self) -> Result<&Self> {
		// let user confirm in CLI that the fiat payment has been sent/receivec
		loop {
			println!("Please confirm that the fiat payment has been sent/received. (y/N)");
			let mut input = String::new();
			std::io::stdin().read_line(&mut input)?;
			if input.trim().to_lowercase() == "y" {
				break;
			}
		}
		Ok(self)
	}
}
