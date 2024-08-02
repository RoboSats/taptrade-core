pub mod maker_utils;
pub mod taker_utils;
pub mod utils;

use self::utils::ActiveOffer;
use super::*;
use crate::{
	cli::{OfferType, TraderSettings},
	communication::api::{
		BondRequirementResponse, BondSubmissionRequest, IsOfferReadyRequest, OfferTakenRequest,
		OfferTakenResponse, PsbtSubmissionRequest, PublicOffer, PublicOffers,
		TradeObligationsSatisfied, TradeObligationsUnsatisfied,
	},
	wallet::{
		bond::Bond,
		musig2::{MuSigData, MusigNonce},
		TradingWallet,
	},
};
use bdk::{
	bitcoin::{amount::serde::as_btc::deserialize, psbt::PartiallySignedTransaction},
	database::MemoryDatabase,
	wallet::AddressInfo,
};
use std::{str::FromStr, thread, time::Duration};

pub fn run_maker(maker_config: &TraderSettings) -> Result<()> {
	let wallet = TradingWallet::load_wallet(maker_config)?; // initialize the wallet with xprv

	let offer = ActiveOffer::create(&wallet, maker_config)?;
	info!("Maker offer created: {:#?}", &offer);

	let escrow_psbt_requirements = offer.wait_until_taken(maker_config)?;
	let mut escrow_psbt =
		PartiallySignedTransaction::from_str(escrow_psbt_requirements.escrow_psbt_hex.as_str())?;
	let signed_escrow_psbt = wallet
		.validate_escrow_psbt(&escrow_psbt)?
		.sign_escrow_psbt(&mut escrow_psbt)?;

	// submit signed escrow psbt back to coordinator
	PsbtSubmissionRequest::submit_escrow_psbt(
		&escrow_psbt,
		offer.offer_id_hex.clone(),
		maker_config,
	)?;

	// wait for confirmation
	offer.wait_on_trade_ready_confirmation(maker_config)?;
	if offer.fiat_confirmation_cli_input(maker_config)? {
		// this represents the "confirm payment" / "confirm fiat recieved" button
		TradeObligationsSatisfied::submit(&offer.offer_id_hex, maker_config)?;
		info!("Waiting for other party to confirm the trade.");
		let payout_keyspend_psbt = IsOfferReadyRequest::poll_payout(maker_config, &offer)?;
	} else {
		error!("Trade failed. Initiating escrow mode.");
		TradeObligationsUnsatisfied::request_escrow(&offer.offer_id_hex, maker_config)?;
		let escrow_payout_script_psbt = IsOfferReadyRequest::poll_payout(maker_config, &offer)?;
	}
	Ok(())
}

pub fn run_taker(taker_config: &TraderSettings) -> Result<()> {
	let wallet = TradingWallet::load_wallet(taker_config)?;
	let mut available_offers = PublicOffers::fetch(taker_config)?;

	while available_offers.offers.is_none() {
		debug!("No offers available, fetching again in 10 sec.");
		thread::sleep(Duration::from_secs(10));
		available_offers = PublicOffers::fetch(taker_config)?;
	}
	let selected_offer: &PublicOffer = available_offers.ask_user_to_select()?;

	// take selected offer and wait for maker to sign his input to the ecrow transaction
	let accepted_offer = ActiveOffer::take(&wallet, taker_config, selected_offer)?;
	accepted_offer.wait_on_trade_ready_confirmation(taker_config)?;

	if accepted_offer.fiat_confirmation_cli_input(taker_config)? {
		// this represents the "confirm payment" / "confirm fiat recieved" button
		TradeObligationsSatisfied::submit(&accepted_offer.offer_id_hex, taker_config)?;
		debug!("Waiting for other party to confirm the trade.");
		// pull for other parties confirmation, then receive the transaction to create MuSig signature for (keyspend) to payout address
		let payout_keyspend_psbt = IsOfferReadyRequest::poll_payout(taker_config, &accepted_offer)?;
	// here we need to handle if the other party is not cooperating
	} else {
		error!("Trade failed.");
		panic!("Escrow to be implemented!");
	}
	Ok(())
}
