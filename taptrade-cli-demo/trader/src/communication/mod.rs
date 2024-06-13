pub mod api;

use crate::{
	cli::{OfferType, TraderSettings},
	wallet::{bond::Bond, musig2::MuSigData},
};
use anyhow::Result;
use api::{BondSubmissionRequest, OfferCreationResponse, OrderActivatedResponse, OrderRequest};
use bdk::bitcoin::consensus::encode::serialize_hex;
use bdk::{
	bitcoin::{consensus::Encodable, psbt::PartiallySignedTransaction},
	wallet::AddressInfo,
};
use serde::{Deserialize, Serialize};

impl OfferCreationResponse {
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

	pub fn fetch(trader_setup: &TraderSettings) -> Result<OfferCreationResponse> {
		let client = reqwest::blocking::Client::new();
		let res = client
			.post(format!(
				"{}{}",
				trader_setup.coordinator_endpoint, "/create-offer"
			))
			.json(&Self::_format_request(trader_setup))
			.send()?
			.json::<OfferCreationResponse>()?;
		Ok(res)
	}
}

impl BondSubmissionRequest {
	pub fn send(
		robohash_hex: &String,
		bond: &PartiallySignedTransaction,
		musig_data: &mut MuSigData,
		payout_address: &AddressInfo,
		trader_setup: &TraderSettings,
	) -> Result<OrderActivatedResponse> {
		let signed_bond_hex = serialize_hex(&bond.to_owned().extract_tx());
		let musig_pub_nonce_hex = hex::encode(musig_data.nonce.get_pub_for_sharing()?.serialize());
		let musig_pubkey_hex = hex::encode(musig_data.public_key.0.serialize());
		let request = BondSubmissionRequest {
			robohash_hex: robohash_hex.clone(),
			signed_bond_hex,
			payout_address: payout_address.address.to_string(),
			musig_pub_nonce_hex,
			musig_pubkey_hex,
		};

		let client = reqwest::blocking::Client::new();
		let res = client
			.post(format!(
				"{}{}",
				trader_setup.coordinator_endpoint, "/submit-maker-bond"
			))
			.json(&request)
			.send()?
			.json::<OrderActivatedResponse>()?;
		Ok(res)
	}
}
