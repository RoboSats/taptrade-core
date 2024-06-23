pub mod maker_utils;
pub mod taker_utils;
pub mod utils;

use self::utils::ActiveOffer;
use crate::{
	cli::TraderSettings,
	communication::api::{
		BondRequirementResponse, BondSubmissionRequest, IsOfferReadyRequest, OfferTakenRequest,
		OfferTakenResponse, PsbtSubmissionRequest, PublicOffer, PublicOffers,
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

	let mut escrow_contract_psbt = offer.wait_until_taken(maker_config)?;
	wallet
		.validate_maker_psbt(&escrow_contract_psbt)?
		.sign_escrow_psbt(&mut escrow_contract_psbt)?;

	// submit signed escrow psbt back to coordinator
	PsbtSubmissionRequest::submit_escrow_psbt(
		&escrow_contract_psbt,
		offer.offer_id_hex.clone(),
		maker_config,
	)?;

	// wait for confirmation
	offer.wait_on_trade_ready_confirmation(maker_config)?;

	Ok(())
}

pub fn run_taker(taker_config: &TraderSettings) -> Result<()> {
	let wallet = TradingWallet::load_wallet(taker_config)?;
	let mut available_offers = PublicOffers::fetch(taker_config)?;

	while available_offers.offers.is_none() {
		println!("No offers available, fetching again in 10 sec.");
		thread::sleep(Duration::from_secs(10));
		available_offers = PublicOffers::fetch(taker_config)?;
	}
	let selected_offer: &PublicOffer = available_offers.ask_user_to_select()?;

	// take selected offer and wait for maker to sign his input to the ecrow transaction
	let accepted_offer = ActiveOffer::take(&wallet, taker_config, selected_offer)?
		.wait_on_trade_ready_confirmation(taker_config)?
		.wait_on_fiat_confirmation_cli_input()?;

	// .wait_on_maker_confirmation(); // here we wait for the maker to confirm the reciept of the fiat. We could go into escrow here.

	Ok(())
}
