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
		musig2_utils::{MuSigData, MusigNonce},
		TradingWallet,
	},
};
use bdk::{
	bitcoin::{amount::serde::as_btc::deserialize, psbt::PartiallySignedTransaction},
	database::MemoryDatabase,
	wallet::AddressInfo,
};
use communication::api::PayoutSignatureRequest;
use reqwest::header::ACCEPT_LANGUAGE;
use std::{str::FromStr, thread, time::Duration};

/// the main maker flow function
pub fn run_maker(maker_config: &TraderSettings) -> Result<()> {
	// load a bdk wallet from passed xprv
	let wallet = TradingWallet::load_wallet(maker_config)?; // initialize the wallet with xprv

	// create an offer with the coordinator, offer is an offer that is in the coordinator orderbook (bond submitted, awaiting taker)
	let offer = ActiveOffer::create(&wallet, maker_config)?;
	info!("Maker offer created: {:#?}", &offer);

	// waits until taker accepts offer (polling), then gets the escrow psbt in return to sign the inputs
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

	// wait for confirmation of the escrow locking transaction (polling)
	offer.wait_on_trade_ready_confirmation(maker_config)?;
	if offer.fiat_confirmation_cli_input(maker_config)? {
		// this represents the "confirm payment" / "confirm fiat recieved" button
		TradeObligationsSatisfied::submit(&offer.offer_id_hex, maker_config)?;
		info!("Waiting for other party to confirm the trade.");
		let (payout_keyspend_psbt, agg_pub_nonce, agg_pubk_ctx) =
			IsOfferReadyRequest::poll_payout(maker_config, &offer)?;
		debug!("Payout PSBT received: {}", &payout_keyspend_psbt);
		let signature = wallet
			.validate_payout_psbt(&payout_keyspend_psbt)?
			.create_keyspend_payout_signature(
				payout_keyspend_psbt,
				agg_pubk_ctx,
				agg_pub_nonce,
				offer.used_musig_config,
			)?;
		// submit signed payout psbt back to coordinator
		PayoutSignatureRequest::send(maker_config, &signature, &offer.offer_id_hex)?;
	// now the coordinator will broadcast the payout transaction and the trade should be finished
	} else {
		warn!("Trader unsatisfied. Initiating escrow mode.");
		TradeObligationsUnsatisfied::request_escrow(&offer.offer_id_hex, maker_config)?;
		let escrow_payout_script_psbt = IsOfferReadyRequest::poll_payout(maker_config, &offer)?;
	}
	Ok(())
}

/// taker main trade function
pub fn run_taker(taker_config: &TraderSettings) -> Result<()> {
	let wallet = TradingWallet::load_wallet(taker_config)?;

	// fetches public offers of the coordinator (Orderbook)
	let mut available_offers = PublicOffers::fetch(taker_config)?;

	// polls until offers are available
	while available_offers.offers.is_none() {
		debug!("No offers available, fetching again in 2 sec.");
		thread::sleep(Duration::from_secs(2));
		available_offers = PublicOffers::fetch(taker_config)?;
	}
	// ask for taker cli input to select a suitable offer
	let selected_offer: &PublicOffer = available_offers.ask_user_to_select()?;

	// take selected offer and wait for maker to sign his input to the ecrow transaction
	let accepted_offer = ActiveOffer::take(&wallet, taker_config, selected_offer)?;

	// polls the coordinator until the escrow locking transaction has been confirmed, this will take time until the maker signed his input
	// and the transaction got confirmed onchain
	accepted_offer.wait_on_trade_ready_confirmation(taker_config)?;

	// ask the taker if he is satisfied or wants to go into escrow with cli input
	if accepted_offer.fiat_confirmation_cli_input(taker_config)? {
		// this represents the "confirm payment" / "confirm fiat recieved" button
		TradeObligationsSatisfied::submit(&accepted_offer.offer_id_hex, taker_config)?;
		debug!("Waiting for other party to confirm the trade.");
		// pull for other parties confirmation, then receive the transaction to create MuSig partial signature for (keyspend) to payout address
		let (payout_keyspend_psbt, agg_pub_nonce, agg_pubk_ctx) =
			IsOfferReadyRequest::poll_payout(taker_config, &accepted_offer)?;

		debug!("Received payout psbt: {}", &payout_keyspend_psbt);
		let signature = wallet
			.validate_payout_psbt(&payout_keyspend_psbt)?
			.create_keyspend_payout_signature(
				payout_keyspend_psbt,
				agg_pubk_ctx,
				agg_pub_nonce,
				accepted_offer.used_musig_config,
			)?;

		// submit partial signature back to coordinator
		PayoutSignatureRequest::send(taker_config, &signature, &accepted_offer.offer_id_hex)?;
	// now the coordinator will broadcast the payout transaction and the trade should be finished
	// here we need to handle if the other party is not cooperating
	} else {
		error!("Trader unsatisfied. Initiating escrow mode.");
		panic!("Escrow to be implemented!");
	}
	Ok(())
}
