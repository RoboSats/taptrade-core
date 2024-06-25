use super::*;

#[derive(Debug)]
pub struct ActiveOffer {
	pub offer_id_hex: String,
	pub used_musig_config: MuSigData,
	pub used_bond: PartiallySignedTransaction,
	pub expected_payout_address: AddressInfo,
	pub escrow_psbt: Option<PartiallySignedTransaction>,
}

impl ActiveOffer {
	// polls till the other party signed the trade transaction and it got confirmed.
	// once the coordinator signals OfferReady the fiat exchange can begin
	pub fn wait_on_trade_ready_confirmation(
		&self,
		trader_config: &TraderSettings,
	) -> Result<&Self> {
		IsOfferReadyRequest::poll(trader_config, &self)?;
		Ok(&self)
	}

	pub fn fiat_confirmation_cli_input(&self, trade_settings: &TraderSettings) -> Result<bool> {
		// let user confirm in CLI that the fiat payment has been sent/received
		println!("The escrow is now locked and the fiat exchange can begin safely.");
		if trade_settings.trade_type.is_buy_order() {
			println!("Please confirm that the fiat payment has been sent or go into mediation in case of problems. (y/M)");
		} else {
			println!("Please confirm that the fiat payment has been received or go into mediation in case of problems. (y/M)");
		}
		loop {
			let mut input = String::new();
			std::io::stdin().read_line(&mut input)?;
			if input.trim().to_lowercase() == "y" {
				return Ok(true);
			} else if input.trim() == "M" {
				return Ok(false);
			}
		}
	}
}
