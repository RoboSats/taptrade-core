pub mod maker_utils;

use self::maker_utils::ActiveOffer;
use crate::{cli::TraderSettings, communication::api::PublicOffers, wallet::TradingWallet};
use anyhow::Result;
use std::{thread, time::Duration};

pub fn run_maker(maker_config: &TraderSettings) -> Result<()> {
	let wallet = TradingWallet::load_wallet(maker_config)?; // initialize the wallet with xprv

	let offer = ActiveOffer::create(&wallet, maker_config)?;
	dbg!(&offer);
	let trade_psbt = offer.wait_until_taken(maker_config)?;

	Ok(())
}

pub fn run_taker(taker_config: &TraderSettings) -> Result<()> {
	let wallet = TradingWallet::load_wallet(maker_config)?;
	let mut available_offers = PublicOffers::fetch(taker_config)?;

	while let None = available_offers.offers {
		println!("No offers available, trying again in 10 sec.");
		thread::sleep(Duration::from_secs(10));
		available_offers = PublicOffers::fetch(taker_config)?;
	}
	let selected_offer = available_offers.ask_user_to_select()?;

	Ok(())
}
