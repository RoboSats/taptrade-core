use anyhow::Context;

use super::{api::*, *};

impl PublicOffers {
	// fetch a list of all publicly available offers on the coordinator fitting the requested range and type
	pub fn fetch(taker_config: &TraderSettings) -> Result<PublicOffers> {
		let amount = taker_config.trade_type.value();
		let request = OffersRequest {
			buy_offers: taker_config.trade_type.is_buy_order(),
			amount_min_sat: (amount as f64 * 0.9).round() as u64, // range can be made variable in production
			amount_max_sat: (amount as f64 * 1.1).round() as u64,
		};

		let client = reqwest::blocking::Client::new();
		let res = client
			.post(format!(
				"{}{}",
				taker_config.coordinator_endpoint, "/fetch-available-offers"
			))
			.json(&request)
			.send()?
			.json::<PublicOffers>()?;
		Ok(res)
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

impl PublicOffer {
	pub fn request_bond(&self, taker_config: &TraderSettings) -> Result<BondRequirementResponse> {
		let client = reqwest::blocking::Client::new();
		let res = client
			.post(format!(
				"{}{}",
				taker_config.coordinator_endpoint, "/request-taker-bond"
			))
			.json(self)
			.send()?
			.json::<BondRequirementResponse>()?;
		Ok(res)
	}
}

impl OfferPsbtRequest {
	pub fn taker_request(
		offer: &PublicOffer,
		trade_data: BondSubmissionRequest,
		taker_config: &TraderSettings,
	) -> Result<PartiallySignedTransaction> {
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
			.send()?
			.json::<OfferTakenResponse>()?;

		let psbt_bytes = hex::decode(res.trade_psbt_hex_to_sign)?;
		let psbt = PartiallySignedTransaction::deserialize(&psbt_bytes)?;
		Ok(psbt)
	}
}

impl PsbtSubmissionRequest {
	pub fn submit_taker_psbt(
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
				taker_config.coordinator_endpoint, "/submit-taker-psbt"
			))
			.json(&request)
			.send()?;
		if res.status() != 200 {
			return Err(anyhow!(
				"Submitting taker psbt failed. Status: {}",
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
					taker_config.coordinator_endpoint, "/poll-offer-status-taker"
				))
				.json(&request)
				.send()?;
			if res.status() == 200 {
				return Ok(());
			} else if res.status() != 201 {
				return Err(anyhow!(
					"Submitting taker psbt failed. Status: {}",
					res.status()
				));
			}
			// Sleep for 10 sec and poll again
			sleep(Duration::from_secs(10));
		}
	}
}
