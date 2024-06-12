pub mod api;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::cli::{OfferType, TraderSettings};
use api::{OfferCreationResponse, OrderRequest};

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
