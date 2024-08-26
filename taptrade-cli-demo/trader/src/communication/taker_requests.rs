use anyhow::Context;
use reqwest::StatusCode;

use super::{api::*, *};

impl PublicOffers {
	// fetch a list of all publicly available offers on the coordinator fitting the requested range and type
	pub fn fetch(taker_config: &TraderSettings) -> Result<PublicOffers> {
		let amount = taker_config.trade_type.value();
		let request = OffersRequest {
			buy_offers: !taker_config.trade_type.is_buy_order(),
			amount_min_sat: (amount as f64 * 0.9).round() as u64, // range can be made variable in production
			amount_max_sat: (amount as f64 * 1.1).round() as u64,
		};
		debug!("Taker requesting offers: {:#?}", request);
		let client = reqwest::blocking::Client::new();
		let res = client
			.post(format!(
				"{}{}",
				taker_config.coordinator_endpoint, "/fetch-available-offers"
			))
			.json(&request)
			.send();
		let res = match res {
			Ok(res) => res,
			Err(e) => {
				return Err(anyhow!("Error fetching offers: {:#?}", e));
			}
		};
		if res.status() == 204 {
			Ok(PublicOffers { offers: None })
		} else {
			match res.json::<PublicOffers>() {
				Ok(offers) => {
					debug!("Received offers: {:#?}", offers);
					Ok(offers)
				}
				Err(e) => Err(anyhow!(
					"Error unpacking fetching offers response: {:#?}",
					e
				)),
			}
		}
	}

	// ask the user to select a offer to take on the CLI
	pub fn ask_user_to_select(&self) -> Result<&PublicOffer> {
		for (index, offer) in self.offers.as_ref().unwrap().iter().enumerate() {
			println!(
				"Offer Index: {} | Amount: {} | ID: {}",
				index, offer.amount_sat, offer.offer_id_hex
			);
		}

		println!("Enter index of the offer you want to accept: ");
		let mut input = String::new();
		std::io::stdin().read_line(&mut input)?;
		let index: usize = input.trim().parse().context("Wrong index entered")?;

		Ok(&self.offers.as_ref().unwrap()[index])
	}
}

impl OfferPsbtRequest {
	pub fn taker_request(
		offer: &PublicOffer,
		trade_data: BondSubmissionRequest,
		taker_config: &TraderSettings,
	) -> Result<OfferTakenResponse> {
		let request = OfferPsbtRequest {
			offer: offer.clone(),
			trade_data,
		};

		let client = reqwest::blocking::Client::new();
		let res = client
			.post(format!(
				"{}{}",
				taker_config.coordinator_endpoint, "/submit-taker-bond"
			))
			.json(&request)
			.send()?;
		match res.status() {
			StatusCode::OK => {
				debug!("Taker bond accepted");
				Ok(res.json::<OfferTakenResponse>()?)
			}
			_ => {
				debug!(
					"Taker bond submission failed: status code: {}",
					res.status()
				);
				Err(anyhow!("Taker bond rejected"))
			}
		}
	}
}
