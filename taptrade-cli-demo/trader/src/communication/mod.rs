pub mod api;
pub mod taker_requests;

use super::*;
use crate::{
	cli::{OfferType, TraderSettings},
	trading::utils::ActiveOffer,
	wallet::{bond::Bond, musig2_utils::MuSigData},
};
use anyhow::{anyhow, Result};
use api::*;
use bdk::bitcoin::{consensus::encode::serialize_hex, key::XOnlyPublicKey};
use bdk::{
	bitcoin::{consensus::Encodable, psbt::PartiallySignedTransaction},
	wallet::AddressInfo,
};
use musig2::{AggNonce, KeyAggContext};
use serde::{Deserialize, Serialize};
use std::{f32::consts::E, str::FromStr, thread::sleep, time::Duration};

impl BondRequirementResponse {
	fn _format_order_request(trader_setup: &TraderSettings) -> OrderRequest {
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
		trace!("Fetching bond requirements from coordinator. (create-offer)");
		let client = reqwest::blocking::Client::new();
		let endpoint = format!("{}{}", trader_setup.coordinator_endpoint, "/create-offer");
		let res = match client
			.post(endpoint)
			.json(&Self::_format_order_request(trader_setup))
			.send()
		{
			Ok(res) => res,
			Err(e) => return Err(anyhow!("Error calling /create-offer: {}", e)),
		};
		let status_code = res.status();
		debug!("/create-offer Response status code: {}", status_code);
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
	/// sending the signed maker bond and additional trade data to the coordinator
	pub fn send_maker(&self, trader_setup: &TraderSettings) -> Result<OrderActivatedResponse> {
		let client = reqwest::blocking::Client::new();
		let res = client
			.post(format!(
				"{}{}",
				trader_setup.coordinator_endpoint, "/submit-maker-bond"
			))
			.json(self)
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
		trace!("Polling offer status from coordinator.");
		let request = OfferTakenRequest {
			robohash_hex: trader_setup.robosats_robohash_hex.clone(),
			offer_id_hex: offer.offer_id_hex.clone(),
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
			} else if res.status() != 202 {
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
	) -> Result<(PartiallySignedTransaction, AggNonce, KeyAggContext)> {
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
				debug!("Payout psbt received. Signing...");
				break;
			} else if res.status() == 202 {
				// still waiting, retry
				continue;
			} else if res.status() == 102 {
				// other party initiated escrow
				debug!("Other party initiated escrow. Waiting for coordinator to finalize.");
				continue;
			} else if res.status() != 410 {
				return Err(anyhow!(
					"We lost the escrow, your bond is gone: {}",
					res.status()
				));
			} else {
				// unintended response
				return Err(anyhow!(
					"Requesting final payout when waiting on other party failed: {}",
					res.status()
				));
			}
		}
		let payout_response: PayoutResponse = res.json()?;
		let final_psbt = PartiallySignedTransaction::deserialize(&hex::decode(
			&payout_response.payout_psbt_hex,
		)?)?;
		let agg_nonce = AggNonce::from_str(&payout_response.agg_musig_nonce_hex)
			.map_err(|e| anyhow!("Error parsing agg nonce: {}", e))?;
		let agg_pubk_ctx = KeyAggContext::from_hex(&payout_response.agg_musig_pubkey_ctx_hex)
			.map_err(|e| anyhow!("Error parsing agg pubkey ctx: {}", e))?;
		Ok((final_psbt, agg_nonce, agg_pubk_ctx))
	}
}

impl TradeObligationsUnsatisfied {
	pub fn request_escrow(offer_id_hex: &String, trader_config: &TraderSettings) -> Result<()> {
		let request = TradeObligationsUnsatisfied {
			robohash_hex: trader_config.robosats_robohash_hex.clone(),
			offer_id_hex: offer_id_hex.clone(),
		};

		let client = reqwest::blocking::Client::new();
		let res = client
			.post(format!(
				"{}{}",
				trader_config.coordinator_endpoint, "/request-escrow"
			))
			.json(&request)
			.send()?;
		if res.status() != 200 {
			return Err(anyhow!(
				"Submitting trade obligations unsatisfied failed. Status: {}",
				res.status()
			));
		}
		Ok(())
	}
}

impl PayoutSignatureRequest {
	pub fn send(trader_config: &TraderSettings, signature: &str, offer_id_hex: &str) -> Result<()> {
		let request = Self {
			robohash_hex: trader_config.robosats_robohash_hex.clone(),
			offer_id_hex: offer_id_hex.to_string(),
			partial_sig_hex: signature.to_string(),
		};

		let client = reqwest::blocking::Client::new();
		let res = client
			.post(format!(
				"{}{}",
				trader_config.coordinator_endpoint, "/submit-payout-signature"
			))
			.json(&request)
			.send()?;
		if res.status() != 200 && res.status() != 202 {
			return Err(anyhow!(
				"Submitting trade obligations unsatisfied failed. Status: {}",
				res.status()
			));
		}
		Ok(())
	}
}
