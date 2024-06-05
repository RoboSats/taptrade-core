use reqwest;
use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::cli::{TraderSettings, OfferType};

#[derive(Debug, Deserialize)]
pub struct OfferCreationResponse {
	pub locking_address: String,
}

#[derive(Serialize)]
struct OrderRequest {
	robohash_base91: String,
	amount_satoshi: u32,
	order_type: String, // buy or sell
	bond_ratio: u8 // [x > 2, 50]
}

impl OfferCreationResponse {
	fn _format_request(trader_setup: &TraderSettings) -> OrderRequest {
		let amount: u32;
		let trade_type = match &trader_setup.trade_type {
            OfferType::Buy(val) => {
                amount = *val;
                "buy"
            },
            OfferType::Sell(val) => {
                amount = *val;
                "sell"
            }
        };

        OrderRequest {
            robohash_base91: trader_setup.robosats_robohash_base91.clone(),
            amount_satoshi: amount,
            order_type: trade_type.to_string(),
            bond_ratio: trader_setup.bond_ratio,
        }
	}

	pub fn fetch(trader_setup: &TraderSettings) -> Result<OfferCreationResponse> {
		let client = reqwest::blocking::Client::new();
		let res = client.post(format!("{}{}", trader_setup.coordinator_endpoint, "/create-offer"))
										.json(&Self::_format_request(trader_setup))
										.send()?
										.json::<OfferCreationResponse>()?;
		Ok(res)
	}
}

