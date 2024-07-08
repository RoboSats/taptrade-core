pub mod api;
pub mod taker_requests;

use crate::{
	cli::{OfferType, TraderSettings},
	trading::utils::ActiveOffer,
	wallet::{bond::Bond, musig2::MuSigData},
};
use anyhow::{anyhow, Result};
use api::*;
use bdk::bitcoin::consensus::encode::serialize_hex;
use bdk::{
	bitcoin::{consensus::Encodable, psbt::PartiallySignedTransaction},
	wallet::AddressInfo,
};
use serde::{Deserialize, Serialize};
use std::{str::FromStr, thread::sleep, time::Duration};

impl BondRequirementResponse {
	fn _format_request(trader_setup: &TraderSettings) -> OrderRequest {
		let amount: u64;
		let is_buy_order = match &trader_setup.trade_type {
			OfferType::Buy(val) => {
				amount = *val;
				true
			}
			OfferType::Sell(val) => {
				amount = *val;
				false
			}
		};

		OrderRequest {
			robohash_hex: trader_setup.robosats_robohash_hex.clone(),
			amount_satoshi: amount,
			is_buy_order,
			bond_ratio: trader_setup.bond_ratio,
			offer_duration_ts: trader_setup.duration_unix_ts,
		}
	}

	pub fn fetch(trader_setup: &TraderSettings) -> Result<BondRequirementResponse> {
		let client = reqwest::blocking::Client::new();
		let endpoint = format!("{}{}", trader_setup.coordinator_endpoint, "/create-offer");
		let res = client
			.post(endpoint)
			.json(&Self::_format_request(trader_setup))
			.send()?;
		let status_code = res.status();
		match res.json::<BondRequirementResponse>() {
			Ok(response) => Ok(response),
			Err(e) => Err(anyhow!(
				"Error fetching bond requirements: {}. Status code: {}",
				e,
				status_code
			)),
		}
	}
}

impl BondSubmissionRequest {
	pub fn prepare_bond_request(
		bond: &PartiallySignedTransaction,
		payout_address: &AddressInfo,
		musig_data: &mut MuSigData,
		trader_config: &TraderSettings,
	) -> Result<BondSubmissionRequest> {
		let signed_bond_hex = serialize_hex(&bond.to_owned().extract_tx());
		let musig_pub_nonce_hex = hex::encode(musig_data.nonce.get_pub_for_sharing()?.serialize());
		let musig_pubkey_hex = hex::encode(musig_data.public_key.0.serialize());

		let request = BondSubmissionRequest {
			robohash_hex: trader_config.robosats_robohash_hex.clone(),
			signed_bond_hex,
			payout_address: payout_address.address.to_string(),
			musig_pub_nonce_hex,
			musig_pubkey_hex,
		};
		Ok(request)
	}

	pub fn send_maker(
		robohash_hex: &str,
		bond: &PartiallySignedTransaction,
		musig_data: &mut MuSigData,
		payout_address: &AddressInfo,
		trader_setup: &TraderSettings,
	) -> Result<OrderActivatedResponse> {
		let request = Self::prepare_bond_request(bond, payout_address, musig_data, trader_setup)?;
		let client = reqwest::blocking::Client::new();
		let res = client
			.post(format!(
				"{}{}",
				trader_setup.coordinator_endpoint, "/submit-maker-bond"
			))
			.json(&request)
			.send();
		match res {
			Ok(res) => {
				let status_code = res.status();
				match res.json::<OrderActivatedResponse>() {
					Ok(response) => Ok(response),
					Err(e) => Err(anyhow!(
						"Error submitting maker bond: {}. Status code: {}",
						e,
						status_code
					)),
				}
			}
			Err(e) => Err(anyhow!("Error submitting maker bond: {}", e)),
		}
	}
}

impl OfferTakenResponse {
	// posts offer to coordinator to check if it has been taken, if not taken
	// returns status code 204 No Content
	pub fn check(
		offer: &ActiveOffer,
		trader_setup: &TraderSettings,
	) -> Result<Option<OfferTakenResponse>> {
		let request = OfferTakenRequest {
			robohash_hex: trader_setup.robosats_robohash_hex.clone(),
			order_id_hex: offer.offer_id_hex.clone(),
		};
		let client = reqwest::blocking::Client::new();
		let res = client
			.post(format!(
				"{}{}",
				trader_setup.coordinator_endpoint, "/request-offer-status"
			))
			.json(&request)
			.send()?;
		if res.status() == 200 {
			Ok(Some(res.json::<OfferTakenResponse>()?))
		} else if res.status() == 204 {
			Ok(None)
		} else {
			Err(anyhow!("Offer status polling returned: {}", res.status()))
		}
	}
}

impl PsbtSubmissionRequest {
	pub fn submit_escrow_psbt(
		psbt: &PartiallySignedTransaction,
		offer_id_hex: String,
		taker_config: &TraderSettings,
	) -> Result<()> {
		let request = PsbtSubmissionRequest {
			signed_psbt_hex: psbt.serialize_hex(),
			offer_id_hex,
			robohash_hex: taker_config.robosats_robohash_hex.clone(),
		};
		let client = reqwest::blocking::Client::new();
		let res = client
			.post(format!(
				"{}{}",
				taker_config.coordinator_endpoint, "/submit-escrow-psbt"
			))
			.json(&request)
			.send()?;
		if res.status() != 200 {
			return Err(anyhow!(
				"Submitting escrow psbt failed. Status: {}",
				res.status()
			));
		}
		Ok(())
	}
}

impl TradeObligationsSatisfied {
	// if the trader is satisfied he can submit this to signal the coordinator readiness to close the trade
	// if the other party also submits this the coordinator can initiate the closing transaction, otherwise
	// escrow has to be initiated
	pub fn submit(offer_id_hex: &String, trader_config: &TraderSettings) -> Result<()> {
		let request = TradeObligationsSatisfied {
			robohash_hex: trader_config.robosats_robohash_hex.clone(),
			offer_id_hex: offer_id_hex.clone(),
		};

		let client = reqwest::blocking::Client::new();
		let res = client
			.post(format!(
				"{}{}",
				trader_config.coordinator_endpoint, "/submit-obligation-confirmation"
			))
			.json(&request)
			.send()?;
		if res.status() != 200 {
			return Err(anyhow!(
				"Submitting trade obligations confirmation failed. Status: {}",
				res.status()
			));
		}
		Ok(())
	}
}

impl IsOfferReadyRequest {
	pub fn poll(taker_config: &TraderSettings, offer: &ActiveOffer) -> Result<()> {
		let request = IsOfferReadyRequest {
			robohash_hex: taker_config.robosats_robohash_hex.clone(),
			offer_id_hex: offer.offer_id_hex.clone(),
		};
		let client = reqwest::blocking::Client::new();
		loop {
			let res = client
				.post(format!(
					"{}{}",
					taker_config.coordinator_endpoint, "/poll-escrow-confirmation"
				))
				.json(&request)
				.send()?;
			if res.status() == 200 {
				return Ok(());
			} else if res.status() != 204 {
				return Err(anyhow!(
					"Requesting offer status when waiting on other party failed: {}",
					res.status()
				));
			}
			// Sleep for 10 sec and poll again
			sleep(Duration::from_secs(10));
		}
	}

	pub fn poll_payout(
		trader_config: &TraderSettings,
		offer: &ActiveOffer,
	) -> Result<Option<PartiallySignedTransaction>> {
		let request = IsOfferReadyRequest {
			robohash_hex: trader_config.robosats_robohash_hex.clone(),
			offer_id_hex: offer.offer_id_hex.clone(),
		};
		let client = reqwest::blocking::Client::new();
		let mut res: reqwest::blocking::Response;

		loop {
			// Sleep for 10 sec and poll
			sleep(Duration::from_secs(10));

			res = client
				.post(format!(
					"{}{}",
					trader_config.coordinator_endpoint, "/poll-final-payout"
				))
				.json(&request)
				.send()?;
			if res.status() == 200 {
				// good case, psbt is returned
				break;
			} else if res.status() == 204 {
				// still waiting, retry
				continue;
			} else if res.status() == 201 {
				// other party initiated escrow
				return Ok(None);
			} else {
				// unintended response
				return Err(anyhow!(
					"Requesting final payout when waiting on other party failed: {}",
					res.status()
				));
			}
		}
		let final_psbt = PartiallySignedTransaction::from_str(
			&res.json::<PayoutPsbtResponse>()?.payout_psbt_hex,
		)?;
		Ok(Some(final_psbt))
	}
}
