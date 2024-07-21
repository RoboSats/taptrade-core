pub mod create_taproot;
pub mod mempool_monitoring;
pub mod monitoring;
pub mod tx_confirmation_monitoring;
pub mod utils;

use self::utils::*;
use super::*;

pub enum PayoutProcessingResult {
	ReadyPSBT(String),
	NotReady,
	LostEscrow,
	DecidingEscrow,
}

pub async fn process_order(
	coordinator: Arc<Coordinator>,
	offer: &OfferRequest,
) -> Result<BondRequirementResponse, AppError> {
	let wallet = &coordinator.coordinator_wallet;
	let database = &coordinator.coordinator_db;

	let bond_address = wallet.get_new_address().await?;
	let locking_amount_sat = offer.amount_satoshi * offer.bond_ratio as u64 / 100;

	let bond_requirements = BondRequirementResponse {
		bond_address,
		locking_amount_sat,
	};

	database
		.insert_new_maker_request(offer, &bond_requirements)
		.await?;

	debug!("Coordinator received new offer: {:?}", offer);
	Ok(bond_requirements)
}

pub async fn handle_maker_bond(
	payload: &BondSubmissionRequest,
	coordinator: Arc<Coordinator>,
) -> Result<OfferActivatedResponse, BondError> {
	let wallet = &coordinator.coordinator_wallet;
	let database = &coordinator.coordinator_db;

	let bond_requirements = if let Ok(requirements) = database
		.fetch_bond_requirements(&payload.robohash_hex)
		.await
	{
		requirements
	} else {
		return Err(BondError::BondNotFound);
	};

	match wallet
		.validate_bond_tx_hex(&payload.signed_bond_hex, &bond_requirements)
		.await
	{
		Ok(()) => (),
		Err(e) => {
			return Err(BondError::InvalidBond(e.to_string()));
		}
	}
	debug!("\nBond validation successful");
	let offer_id_hex: String = generate_random_order_id(16); // 16 bytes random offer id, maybe a different system makes more sense later on? (uuid or increasing counter...)
														 // create address for taker bond
	let new_taker_bond_address = match wallet.get_new_address().await {
		Ok(address) => address,
		Err(e) => {
			let error = format!(
				"Error generating taker bond address for offer id: {}. Error: {e}",
				offer_id_hex
			);
			return Err(BondError::CoordinatorError(error.to_string()));
		}
	};
	// insert bond into sql database and move offer to different table
	let bond_locked_until_timestamp = match database
		.move_offer_to_active(&payload, &offer_id_hex, new_taker_bond_address)
		.await
	{
		Ok(timestamp) => timestamp,
		Err(e) => {
			debug!(
				"Error in validate_bond_tx_hex in move_offer_to_active: {}",
				e
			);
			return Err(BondError::CoordinatorError(e.to_string()));
		}
	};
	Ok(OfferActivatedResponse {
		bond_locked_until_timestamp,
		offer_id_hex,
	})
}

pub async fn get_public_offers(
	request: &OffersRequest,
	coordinator: Arc<Coordinator>,
) -> Result<PublicOffers, FetchOffersError> {
	let database = &coordinator.coordinator_db;

	let offers = match database.fetch_suitable_offers(request).await {
		Ok(offers) => offers,
		Err(e) => {
			return Err(FetchOffersError::DatabaseError(e.to_string()));
		}
	};
	if offers.is_none() {
		return Err(FetchOffersError::NoOffersAvailable);
	}
	Ok(PublicOffers { offers })
}

pub async fn handle_taker_bond(
	payload: &OfferPsbtRequest,
	coordinator: Arc<Coordinator>,
) -> Result<OfferTakenResponse, BondError> {
	let wallet = &coordinator.coordinator_wallet;
	let database = &coordinator.coordinator_db;

	let bond_requirements = database
		.fetch_taker_bond_requirements(&payload.offer.offer_id_hex)
		.await;

	match bond_requirements {
		Ok(bond_requirements) => {
			match wallet
				.validate_bond_tx_hex(&payload.trade_data.signed_bond_hex, &bond_requirements)
				.await
			{
				Ok(()) => (),
				Err(e) => {
					return Err(BondError::InvalidBond(e.to_string()));
				}
			}
		}
		Err(_) => return Err(BondError::BondNotFound),
	}
	debug!("\nTaker bond validation successful");

	panic!("Trade contract PSBT not implemented!");
	let trade_contract_psbt_taker = "".to_string(); // implement psbt
	let trade_contract_psbt_maker = "".to_string(); // implement psbt
	let escrow_tx_txid: String = "".to_string(); // implement txid of psbt

	if let Err(e) = database
		.add_taker_info_and_move_table(
			&payload,
			&trade_contract_psbt_maker,
			&trade_contract_psbt_taker,
			escrow_tx_txid,
		)
		.await
	{
		return Err(BondError::CoordinatorError(e.to_string()));
	}

	Ok(OfferTakenResponse {
		trade_psbt_hex_to_sign: trade_contract_psbt_taker,
	})
}

pub async fn get_offer_status_maker(
	payload: &OfferTakenRequest,
	coordinator: Arc<Coordinator>,
) -> Result<OfferTakenResponse, FetchOffersError> {
	let database = &coordinator.coordinator_db;

	let offer = match database
		.fetch_taken_offer_maker(&payload.offer_id_hex, &payload.robohash_hex)
		.await
	{
		Ok(offer) => offer,
		Err(e) => {
			return Err(FetchOffersError::DatabaseError(e.to_string()));
		}
	};
	match offer {
		Some(offer) => Ok(OfferTakenResponse {
			trade_psbt_hex_to_sign: offer,
		}),
		None => Err(FetchOffersError::NoOffersAvailable),
	}
}

pub async fn fetch_escrow_confirmation_status(
	payload: &OfferTakenRequest,
	coordinator: Arc<Coordinator>,
) -> Result<bool, FetchEscrowConfirmationError> {
	let database = &coordinator.coordinator_db;

	match database
		.is_valid_robohash_in_table(&payload.robohash_hex, &payload.offer_id_hex)
		.await
	{
		Ok(false) => return Err(FetchEscrowConfirmationError::NotFoundError),
		Ok(true) => (),
		Err(e) => return Err(FetchEscrowConfirmationError::DatabaseError(e.to_string())),
	}

	if match database
		.fetch_escrow_tx_confirmation_status(&payload.offer_id_hex)
		.await
	{
		Ok(status) => status,
		Err(e) => return Err(FetchEscrowConfirmationError::DatabaseError(e.to_string())),
	} {
		// rust smh
		Ok(true)
	} else {
		Err(FetchEscrowConfirmationError::NotFoundError)
	}
}

pub async fn handle_obligation_confirmation(
	payload: &OfferTakenRequest,
	coordinator: Arc<Coordinator>,
) -> Result<(), RequestError> {
	let database = &coordinator.coordinator_db;

	if let Err(e) =
		check_offer_and_confirmation(&payload.offer_id_hex, &payload.robohash_hex, database).await
	{
		return Err(e);
	}
	if let Err(e) = database
		.set_trader_happy_field(&payload.offer_id_hex, &payload.robohash_hex, true)
		.await
	{
		return Err(RequestError::DatabaseError(e.to_string()));
	}
	Ok(())
}

pub async fn initiate_escrow(
	payload: &TradeObligationsUnsatisfied,
	coordinator: Arc<Coordinator>,
) -> Result<(), RequestError> {
	let database = &coordinator.coordinator_db;

	if let Err(e) =
		check_offer_and_confirmation(&payload.offer_id_hex, &payload.robohash_hex, database).await
	{
		return Err(e);
	}

	if let Err(e) = database
		.set_trader_happy_field(&payload.offer_id_hex, &payload.robohash_hex, false)
		.await
	{
		return Err(RequestError::DatabaseError(e.to_string()));
	}

	Ok(())
}

pub async fn handle_final_payout(
	payload: &OfferTakenRequest,
	coordinator: Arc<Coordinator>,
) -> Result<PayoutProcessingResult, RequestError> {
	let database = &coordinator.coordinator_db;

	if let Err(e) =
		check_offer_and_confirmation(&payload.offer_id_hex, &payload.robohash_hex, database).await
	{
		return Err(e);
	}

	let trader_happiness = match database.fetch_trader_happiness(&payload.offer_id_hex).await {
		Ok(happiness) => happiness,
		Err(e) => return Err(RequestError::DatabaseError(e.to_string())),
	};

	if trader_happiness.maker_happy.is_some_and(|x| x == true)
		&& trader_happiness.taker_happy.is_some_and(|x| x == true)
	{
		panic!("Implement wallet.assemble_keyspend_payout_psbt()");
	// let payout_keyspend_psbt_hex = wallet
	// 	.assemble_keyspend_payout_psbt(&payload.offer_id_hex, &payload.robohash_hex)
	// 	.await
	// 	.context("Error assembling payout PSBT")?;
	// return Ok(PayoutProcessingResult::ReadyPSBT(payout_keyspend_psbt_hex));
	} else if (trader_happiness.maker_happy.is_none() || trader_happiness.taker_happy.is_none())
		&& !trader_happiness.escrow_ongoing
	{
		return Ok(PayoutProcessingResult::NotReady);
	}
	// if one of them is not happy
	// open escrow cli on coordinator to decide who will win (chat/dispute is out of scope for this demo)
	// once decided who will win assemble the correct payout psbt and return it to the according trader
	// the other trader gets a error code/ end of trade code
	// escrow winner has to be set true with a cli input of the coordinator. This could be an api
	// endpoint for the admin UI frontend in the future
	let potential_escrow_winner = match database.fetch_escrow_result(&payload.offer_id_hex).await {
		Ok(escrow_winner) => escrow_winner,
		Err(e) => return Err(RequestError::DatabaseError(e.to_string())),
	};

	if let Some(escrow_winner) = potential_escrow_winner {
		if escrow_winner == payload.robohash_hex {
			panic!("Implement wallet.assemble_script_payout_psbt()");
		// let script_payout_psbt_hex = wallet
		// 	.assemble_script_payout_psbt(&payload.offer_id_hex, &payload.robohash_hex, is_maker_bool)
		// 	.await
		// 	.context("Error assembling payout PSBT")?;
		// return Ok(PayoutProcessingResult::ReadyPSBT(script_payout_psbt_hex));
		} else {
			// this will be returned to the losing trader
			return Ok(PayoutProcessingResult::LostEscrow);
		}
	} else {
		// this will be returned if the coordinator hasn't decided yet
		return Ok(PayoutProcessingResult::DecidingEscrow);
	}
}
