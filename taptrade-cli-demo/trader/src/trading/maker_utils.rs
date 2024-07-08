use super::utils::*;
use super::*;

impl ActiveOffer {
	pub fn create(
		trading_wallet: &TradingWallet,
		maker_config: &TraderSettings,
	) -> Result<ActiveOffer> {
		let offer_conditions = BondRequirementResponse::fetch(maker_config)?;
		// let offer_conditions = OfferCreationResponse {
		// 	// hardcoded for testing, locking_address is owned by .env xprv
		// 	locking_amount_sat: 90000,
		// 	bond_address: "tb1pfdvgfzwp8vhmelpv8w9kezz7nsmxw68jz6yehgze6mzx0t6r9t2qv9ynmm"
		// 		.to_string(),
		// };

		let (bond, mut musig_data, payout_address) =
			trading_wallet.trade_onchain_assembly(&offer_conditions, maker_config)?;
		let submission_result = BondSubmissionRequest::send_maker(
			&maker_config.robosats_robohash_hex,
			&bond,
			&mut musig_data,
			&payout_address,
			maker_config,
		)?;
		Ok(ActiveOffer {
			offer_id_hex: submission_result.offer_id_hex,
			used_musig_config: musig_data,
			used_bond: bond,
			expected_payout_address: payout_address,
			escrow_psbt: None,
		})
	}

	// polling until offer is taken, in production a more efficient way would make sense
	// returns the PSBT of the escrow trade transaction we have to validate, sign and return
	pub fn wait_until_taken(
		&self,
		trader_config: &TraderSettings,
	) -> Result<PartiallySignedTransaction> {
		loop {
			thread::sleep(Duration::from_secs(10));
			if let Some(offer_taken_response) = OfferTakenResponse::check(self, trader_config)? {
				let psbt_bytes = hex::decode(offer_taken_response.trade_psbt_hex_to_sign)?;
				let psbt = PartiallySignedTransaction::deserialize(&psbt_bytes)?;
				return Ok(psbt);
			}
		}
	}
}
