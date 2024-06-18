pub mod maker_utils;
pub mod taker_utils;
pub mod utils;

use self::utils::ActiveOffer;
use crate::{
	cli::TraderSettings,
	communication::api::{
		BondRequirementResponse, BondSubmissionRequest, OfferTakenRequest, OfferTakenResponse,
		PublicOffer, PublicOffers,
	},
	wallet::{
		bond::Bond,
		musig2::{MuSigData, MusigNonce},
		TradingWallet,
	},
};
use anyhow::Result;
use bdk::{
	bitcoin::{amount::serde::as_btc::deserialize, psbt::PartiallySignedTransaction},
	database::MemoryDatabase,
	wallet::AddressInfo,
};
use std::{thread, time::Duration};

pub fn run_maker(maker_config: &TraderSettings) -> Result<()> {
	let wallet = TradingWallet::load_wallet(maker_config)?; // initialize the wallet with xprv

	let offer = ActiveOffer::create(&wallet, maker_config)?;
	dbg!(&offer);
	let trade_psbt = offer.wait_until_taken(maker_config)?;

	Ok(())
}

pub fn run_taker(taker_config: &TraderSettings) -> Result<()> {
	let wallet = TradingWallet::load_wallet(taker_config)?;
	let mut available_offers = PublicOffers::fetch(taker_config)?;

	while let None = available_offers.offers {
		println!("No offers available, trying again in 10 sec.");
		thread::sleep(Duration::from_secs(10));
		available_offers = PublicOffers::fetch(taker_config)?;
	}
	let selected_offer: &PublicOffer = available_offers.ask_user_to_select()?;
	let accepted_offer =
		ActiveOffer::take(&wallet, taker_config, selected_offer)?.wait_on_maker()?;

	Ok(())
}
