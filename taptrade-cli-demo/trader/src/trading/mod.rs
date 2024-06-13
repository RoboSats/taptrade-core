pub mod maker_utils;

use self::maker_utils::ActiveOffer;
use crate::cli::TraderSettings;
use crate::wallet::TradingWallet;
use anyhow::Result;

pub fn run_maker(maker_config: &TraderSettings) -> Result<()> {
	let wallet = TradingWallet::load_wallet(maker_config)?; // initialize the wallet with xprv

	let offer = ActiveOffer::create(&wallet, maker_config)?;
	dbg!(&offer);
	let trade_psbt = offer.wait_until_taken(maker_config)?;

	Ok(())
}

pub fn run_taker(taker_config: &TraderSettings) -> Result<()> {
	// panic!("Taker to be implemented!");

	Ok(())
}
