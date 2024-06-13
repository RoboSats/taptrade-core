use crate::cli::TraderSettings;
use crate::communication::api::{
	BondSubmissionRequest, OfferCreationResponse, OfferTakenRequest, OfferTakenResponse,
};
use crate::wallet::{
	bond::Bond,
	musig2::{MuSigData, MusigNonce},
	TradingWallet,
};
use anyhow::Result;
use bdk::bitcoin::amount::serde::as_btc::deserialize;
use bdk::{
	bitcoin::psbt::PartiallySignedTransaction, database::MemoryDatabase, wallet::AddressInfo,
};
use std::{thread, time::Duration};

#[derive(Debug)]
pub struct ActiveOffer {
	pub order_id_hex: String,
	pub bond_locked_until_timestamp: u128,
	pub used_musig_config: MuSigData,
	pub used_bond: PartiallySignedTransaction,
	pub expected_payout_address: AddressInfo,
}

impl ActiveOffer {
	pub fn create(
		trading_wallet: &TradingWallet,
		maker_config: &TraderSettings,
	) -> Result<ActiveOffer> {
		let trading_wallet = &trading_wallet.wallet;

		let offer_conditions = OfferCreationResponse::fetch(maker_config)?;
		// let offer_conditions = OfferCreationResponse {
		// 	// hardcoded for testing, locking_address is owned by .env xprv
		// 	locking_amount_sat: 90000,
		// 	bond_address: "tb1pfdvgfzwp8vhmelpv8w9kezz7nsmxw68jz6yehgze6mzx0t6r9t2qv9ynmm"
		// 		.to_string(),
		// };
		let bond = Bond::assemble(trading_wallet, &offer_conditions, maker_config)?;
		let payout_address = trading_wallet.get_address(bdk::wallet::AddressIndex::LastUnused)?;
		let mut musig_data =
			MuSigData::create(&maker_config.wallet_xprv, trading_wallet.secp_ctx())?;
		let submission_result = BondSubmissionRequest::send(
			&maker_config.robosats_robohash_hex,
			&bond,
			&mut musig_data,
			&payout_address,
			maker_config,
		)?;
		Ok(ActiveOffer {
			order_id_hex: submission_result.order_id_hex,
			bond_locked_until_timestamp: submission_result.bond_locked_until_timestamp,
			used_musig_config: musig_data,
			used_bond: bond,
			expected_payout_address: payout_address,
		})
	}

	// polling until offer is taken, in production a more efficient way would make sense
	// returns the PSBT of the escrow trade transaction we have to validate, sign and return
	pub fn wait_until_taken(
		self,
		trader_config: &TraderSettings,
	) -> Result<PartiallySignedTransaction> {
		loop {
			thread::sleep(Duration::from_secs(10));
			if let Some(offer_taken_response) = OfferTakenResponse::check(&self, trader_config)? {
				let psbt_bytes = hex::decode(offer_taken_response.trade_psbt_hex_to_sign)?;
				let psbt = PartiallySignedTransaction::deserialize(&psbt_bytes)?;
				return Ok(psbt);
			}
		}
	}
}
