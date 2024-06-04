use reqwest;
use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct OfferConditions {
	pub locking_address: String,
}

pub fn fetch_offer(coordinator_ep: &String) -> Result<OfferConditions> {
	let res = reqwest::blocking::get(format!("{}{}", coordinator_ep, "/create-offer"))?;
	let offer_conditions: OfferConditions = res.json()?;
	Ok(offer_conditions)
}
