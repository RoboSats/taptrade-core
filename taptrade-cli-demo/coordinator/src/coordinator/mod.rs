pub mod bond_monitoring;
pub mod coordinator_utils;
pub mod create_taproot;
pub mod mempool_monitoring;
pub mod tx_confirmation_monitoring;

use self::coordinator_utils::*;
use super::*;

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

	let bond_requirements = database
		.fetch_bond_requirements(&payload.robohash_hex)
		.await
		.map_err(|_| BondError::BondNotFound)?;

	wallet
		.validate_bond_tx_hex(&payload.signed_bond_hex, &bond_requirements)
		.await
		.map_err(|e| BondError::InvalidBond(e.to_string()))?;
	debug!("\nBond validation successful");
	let offer_id_hex: String = generate_random_order_id(16); // 16 bytes random offer id, maybe a different system makes more sense later on? (uuid or increasing counter...)
														 // create address for taker bond
	let new_taker_bond_address = wallet
		.get_new_address()
		.await
		.map_err(|e| BondError::CoordinatorError(e.to_string()))?;

	let bond_locked_until_timestamp = database
		.move_offer_to_active(payload, &offer_id_hex, new_taker_bond_address)
		.await
		.map_err(|e| BondError::CoordinatorError(e.to_string()))?;

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

	let offers = database
		.fetch_suitable_offers(request)
		.await
		.map_err(|e| FetchOffersError::Database(e.to_string()))?;

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
		.await
		.map_err(|_| BondError::BondNotFound)?;

	wallet
		.validate_bond_tx_hex(&payload.trade_data.signed_bond_hex, &bond_requirements)
		.await
		.map_err(|e| BondError::InvalidBond(e.to_string()))?;

	debug!("\nTaker bond validation successful");

	let escrow_output_data = wallet
		.create_escrow_psbt(database, &payload)
		.await
		.map_err(|e| BondError::CoordinatorError(e.to_string()))?;
	debug!(
		"\nEscrow PSBT creation successful: {:?}",
		escrow_output_data
	);

	database
		.add_taker_info_and_move_table(payload, &escrow_output_data)
		.await
		.map_err(|e| BondError::CoordinatorError(e.to_string()))?;

	trace!("Taker information added to database and moved table successfully");
	Ok(OfferTakenResponse {
		escrow_psbt_hex: escrow_output_data.escrow_psbt_hex,
		escrow_output_descriptor: escrow_output_data.escrow_output_descriptor,
		escrow_amount_maker_sat: escrow_output_data.escrow_amount_maker_sat,
		escrow_amount_taker_sat: escrow_output_data.escrow_amount_taker_sat,
		escrow_fee_sat_per_participant: escrow_output_data.escrow_fee_sat_per_participant,
	})
}

pub async fn get_offer_status_maker(
	payload: &OfferTakenRequest,
	coordinator: Arc<Coordinator>,
) -> Result<OfferTakenResponse, FetchOffersError> {
	let database = &coordinator.coordinator_db;

	let EscrowPsbt {
		escrow_output_descriptor,
		escrow_amount_maker_sat,
		escrow_amount_taker_sat,
		escrow_fee_sat_per_participant,
		escrow_psbt_hex,
		..
	} = match database
		.fetch_escrow_output_information(&payload.offer_id_hex)
		.await
	{
		Ok(Some(escrow_psbt_data)) => escrow_psbt_data,
		Ok(None) => {
			return Err(FetchOffersError::NoOffersAvailable);
		}
		Err(e) => {
			return Err(FetchOffersError::Database(e.to_string()));
		}
	};
	Ok(OfferTakenResponse {
		escrow_psbt_hex,
		escrow_output_descriptor,
		escrow_amount_maker_sat,
		escrow_amount_taker_sat,
		escrow_fee_sat_per_participant,
	})
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
		Ok(false) => return Err(FetchEscrowConfirmationError::NotFound),
		Ok(true) => (),
		Err(e) => return Err(FetchEscrowConfirmationError::Database(e.to_string())),
	}

	database
		.fetch_escrow_tx_confirmation_status(&payload.offer_id_hex)
		.await
		.map_err(|e| FetchEscrowConfirmationError::Database(e.to_string()))
}

pub async fn handle_signed_escrow_psbt(
	payload: &PsbtSubmissionRequest,
	coordinator: Arc<Coordinator>,
) -> Result<(), RequestError> {
	let database = &coordinator.coordinator_db;
	let wallet = &coordinator.coordinator_wallet;

	match database
		.is_valid_robohash_in_table(&payload.robohash_hex, &payload.offer_id_hex)
		.await
	{
		Ok(false) => return Err(RequestError::NotFound),
		Ok(true) => (),
		Err(e) => return Err(RequestError::Database(e.to_string())),
	};

	wallet
		.validate_escrow_init_psbt(&payload.signed_psbt_hex)
		.await
		.map_err(|e| RequestError::PsbtInvalid(e.to_string()))?;

	match database.insert_signed_escrow_psbt(payload).await {
		Ok(false) => return Err(RequestError::PsbtAlreadySubmitted),
		Ok(true) => (),
		Err(e) => return Err(RequestError::Database(e.to_string())),
	};

	// check if both signed parts are there, if so, combine and broadcast
	let (maker_psbt, taker_psbt) = match database
		.fetch_both_signed_escrow_psbts(&payload.offer_id_hex)
		.await
	{
		Ok(Some((maker_psbt, taker_psbt))) => (maker_psbt, taker_psbt),
		Ok(None) => return Ok(()),
		Err(e) => return Err(RequestError::Database(e.to_string())),
	};

	wallet
		.combine_and_broadcast_escrow_psbt(&maker_psbt, &taker_psbt)
		.await
		.map_err(|e| RequestError::PsbtInvalid(e.to_string()))?;

	Ok(())
}

pub async fn handle_obligation_confirmation(
	payload: &OfferTakenRequest,
	coordinator: Arc<Coordinator>,
) -> Result<(), RequestError> {
	let database = &coordinator.coordinator_db;

	check_offer_and_confirmation(&payload.offer_id_hex, &payload.robohash_hex, database).await?;
	database
		.set_trader_happy_field(&payload.offer_id_hex, &payload.robohash_hex, true)
		.await
		.map_err(|e| RequestError::Database(e.to_string()))?;
	Ok(())
}

pub async fn initiate_escrow(
	payload: &TradeObligationsUnsatisfied,
	coordinator: Arc<Coordinator>,
) -> Result<(), RequestError> {
	let database = &coordinator.coordinator_db;

	check_offer_and_confirmation(&payload.offer_id_hex, &payload.robohash_hex, database).await?;
	database
		.set_trader_happy_field(&payload.offer_id_hex, &payload.robohash_hex, false)
		.await
		.map_err(|e| RequestError::Database(e.to_string()))?;

	Ok(())
}

pub async fn handle_final_payout(
	payload: &OfferTakenRequest,
	coordinator: Arc<Coordinator>,
) -> Result<PayoutProcessingResult, RequestError> {
	let database = &coordinator.coordinator_db;

	let trader_happiness = database
		.fetch_trader_happiness(&payload.offer_id_hex)
		.await
		.map_err(|e| RequestError::Database(e.to_string()))?;

	if trader_happiness.maker_happy.is_some_and(|x| x)
		&& trader_happiness.taker_happy.is_some_and(|x| x)
	{
		let escrow_payout_data = database
			.fetch_payout_data(&payload.offer_id_hex)
			.await
			.map_err(|e| RequestError::Database(e.to_string()))?;

		let payout_keyspend_psbt_hex = coordinator
			.coordinator_wallet
			.assemble_keyspend_payout_psbt(&escrow_payout_data)
			.await
			.map_err(|e| RequestError::CoordinatorError(e.to_string()))?;

		database
			.insert_keyspend_payout_psbt(&payload.offer_id_hex, &payout_keyspend_psbt_hex)
			.await
			.map_err(|e| RequestError::Database(e.to_string()))?;

		return Ok(PayoutProcessingResult::ReadyPSBT(PayoutResponse {
			payout_psbt_hex: payout_keyspend_psbt_hex,
			agg_musig_nonce_hex: escrow_payout_data.agg_musig_nonce.to_string(),
			agg_musig_pubkey_ctx_hex: escrow_payout_data.aggregated_musig_pubkey_ctx_hex,
		}));
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
		Err(e) => return Err(RequestError::Database(e.to_string())),
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
			Ok(PayoutProcessingResult::LostEscrow)
		}
	} else {
		// this will be returned if the coordinator hasn't decided yet
		Ok(PayoutProcessingResult::DecidingEscrow)
	}
}

pub async fn handle_payout_signature(
	payload: &PayoutSignatureRequest,
	coordinator: Arc<Coordinator>,
) -> Result<(), RequestError> {
	let database = &coordinator.coordinator_db;
	let wallet = &coordinator.coordinator_wallet;

	check_offer_and_confirmation(&payload.offer_id_hex, &payload.robohash_hex, database).await?;

	let (maker_partial_sig_hex, taker_partial_sig_hex, payout_psbt_hex) = match database
		.insert_partial_sig_and_fetch_if_both(
			&payload.partial_sig_hex,
			&payload.offer_id_hex,
			&payload.robohash_hex,
		)
		.await
	{
		Ok(Some((maker_partial_sig, taker_partial_sig, payout_transaction_psbt_hex))) => (
			maker_partial_sig,
			taker_partial_sig,
			bdk::bitcoin::psbt::PartiallySignedTransaction::deserialize(
				&hex::decode(payout_transaction_psbt_hex)
					.map_err(|e| RequestError::CoordinatorError(e.to_string()))?,
			),
		),
		Ok(None) => return Ok(()),
		Err(e) => return Err(RequestError::Database(e.to_string())),
	};

	warn!("Use musig2 validate partial sig to validate sigs before using to blame users providing wrong sigs");

	let aggregated_signature = wallet::payout_tx::aggregate_partial_signatures(
		&maker_partial_sig_hex,
		&taker_partial_sig_hex,
	)
	.map_err(|e| RequestError::CoordinatorError(e.to_string()))?;

	Ok(())
}
