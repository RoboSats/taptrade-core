pub mod bond_monitoring;
pub mod coordinator_utils;
pub mod escrow_cli;
pub mod mempool_monitoring;
pub mod tx_confirmation_monitoring;
// pub mod create_taproot;

use super::*;

/// Accepts the request to create a new offer, inserts it in the database and
/// returns the required bond information to the maker.
pub async fn process_order(
	coordinator: Arc<Coordinator>,
	offer: &OfferRequest,
) -> Result<BondRequirementResponse, AppError> {
	let wallet = &coordinator.coordinator_wallet;
	let database = &coordinator.coordinator_db;

	let bond_amount = (offer.bond_ratio as u64 * offer.amount_satoshi) / 100;
	let coordinator_fee = ((coordinator.coordinator_wallet.coordinator_feerate
		* offer.amount_satoshi as f64)
		/ 100.0) as u64;
	let absolute_tx_fee = 5000;
	// 5000 is a buffer
	let escrow_locking_input_amount_without_trade_sum =
		bond_amount + coordinator_fee + absolute_tx_fee + 5000;
	trace!(
		"Offer amount: {}, Locking amount: {}",
		offer.amount_satoshi,
		bond_amount
	);
	let bond_requirements = BondRequirementResponse {
		bond_address: wallet.get_new_address().await?,
		locking_amount_sat: bond_amount,
		escrow_locking_input_amount_without_trade_sum,
	};

	database
		.insert_new_maker_request(offer, &bond_requirements)
		.await?;

	debug!("Coordinator received new offer: {:?}", offer);
	Ok(bond_requirements)
}

/// Accepts the signed bond transaction passed by the maker, validates it and inserts it in the database for further monitoring.
/// Moves the offer from the pending table to the active_maker_offers table ("the Orderbook").
pub async fn handle_maker_bond(
	payload: &BondSubmissionRequest,
	coordinator: Arc<Coordinator>,
) -> Result<OfferActivatedResponse, BondError> {
	let wallet = &coordinator.coordinator_wallet;
	let database = &coordinator.coordinator_db;

	// get the according bond requirements from the database to validate against them
	let bond_requirements = database
		.fetch_bond_requirements(&payload.robohash_hex)
		.await
		.map_err(|_| BondError::BondNotFound)?;

	// validate the signed bond transaction
	wallet
		.validate_bond_tx_hex(&payload.signed_bond_hex, &bond_requirements)
		.await
		.map_err(|e| BondError::InvalidBond(e.to_string()))?;
	debug!("\nBond validation successful");
	// generates a random offer id to be able to identify the offer
	let offer_id_hex: String = generate_random_order_id(16); // 16 bytes random offer id, maybe a different system makes more sense later on? (uuid or increasing counter...)
														 // create address for taker bond

	// get new address for the taker bond to which the taker has to lock its bond when accepting this offer
	let new_taker_bond_address = wallet
		.get_new_address()
		.await
		.map_err(|e| BondError::CoordinatorError(e.to_string()))?;

	// move the offer from the pending table to the active_maker_offers table in the database, returns the unix timestamp until the
	// bond is being monitored (the inputs shouldn't be touched by the trader except for the following escrow transaction)
	let bond_locked_until_timestamp = database
		.move_offer_to_active(payload, &offer_id_hex, new_taker_bond_address)
		.await
		.map_err(|e| BondError::CoordinatorError(e.to_string()))?;

	Ok(OfferActivatedResponse {
		bond_locked_until_timestamp,
		offer_id_hex,
	})
}

/// fetches all offers from the database that are suitable for the trade requested by the taker
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

/// Accepts the request of the taker to take an offer, validates the taker bond tx that is passed with the request,
/// creates the escrow locking transaction and moves all information to the taken offers db table. Returns the
/// information necessary for the taker to sign its input to the escrow locking psbt
pub async fn handle_taker_bond(
	payload: &OfferPsbtRequest,
	coordinator: Arc<Coordinator>,
) -> Result<OfferTakenResponse, BondError> {
	let wallet = &coordinator.coordinator_wallet;
	let database = &coordinator.coordinator_db;

	// fetch the bond requirements for the taker bond from the database
	let bond_requirements = database
		.fetch_taker_bond_requirements(&payload.offer.offer_id_hex)
		.await
		.map_err(|_| BondError::BondNotFound)?;

	// validate the signed taker bond transaction against the requirements
	wallet
		.validate_bond_tx_hex(&payload.trade_data.signed_bond_hex, &bond_requirements)
		.await
		.map_err(|e| BondError::InvalidBond(e.to_string()))?;

	debug!("\nTaker bond validation successful");

	// create the escrow locking transaction
	let escrow_output_data = wallet
		.create_escrow_psbt(database, payload)
		.await
		.map_err(|e| BondError::CoordinatorError(e.to_string()))?;
	debug!(
		"\nEscrow PSBT creation successful: {:?}",
		escrow_output_data
	);

	// add the taker information to the database and move the offer to the taken_offers table
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

/// gets called by the polling endpoint the maker polls when waiting for an offer taker,
/// looks in the database if escrow output information is available for the offer id
/// which means the offer has been taken, returns the escrow locking tx information if
/// the offer has been taken so the maker can sign its input to it.
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

/// gets polled by both traders so they can see if the exchange can safely begin.
/// checks the database for the confirmation flag of the escrow transaction which is
/// set by the concurrent confirmation monitoring task
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

/// handles the returned signed escrow locking psbt of both traders, if both are present in the db
/// it combines them and broadcasts the escrow transaction to the network
/// otherwise the tx will just get stored in the db.
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

/// sets the trader happy flag in the database which both traders have to either set true or false to continue with payout or escrow procedure
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

/// if a trader requests escrow this function sets the trader happy flag to false in the db. Then a CLI for the coordinator should be opened
/// to decide which trader is correct
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

/// if both traders are happy this function will assemble the final keyspend payout transaction and return it to the traders
/// for them to be able to create the partial signatures
pub async fn handle_final_payout(
	payload: &OfferTakenRequest,
	coordinator: Arc<Coordinator>,
) -> Result<PayoutProcessingResult, RequestError> {
	let database = &coordinator.coordinator_db;

	let trader_happiness = database
		.fetch_trader_happiness(&payload.offer_id_hex)
		.await
		.map_err(|e| RequestError::Database(e.to_string()))?;

	// both traders are happy, keyspend payout can begin
	if trader_happiness.maker_happy.is_some_and(|x| x)
		&& trader_happiness.taker_happy.is_some_and(|x| x)
	{
		let escrow_payout_data = database
			.fetch_payout_data(&payload.offer_id_hex)
			.await
			.map_err(|e| RequestError::Database(e.to_string()))?;

		let payout_keyspend_psbt_hex = if let Some(payout_psbt_hex) = database
			.fetch_keyspend_payout_psbt(&payload.offer_id_hex)
			.await
			.map_err(|e| RequestError::Database(e.to_string()))?
		{
			payout_psbt_hex
		} else {
			if !database
				.toggle_processing(&payload.offer_id_hex)
				.await
				.map_err(|e| RequestError::Database(e.to_string()))?
			{
				return Ok(PayoutProcessingResult::NotReady);
			}
			let payout_keyspend_psbt_hex = coordinator
				.coordinator_wallet
				.assemble_keyspend_payout_psbt(&escrow_payout_data)
				.await
				.map_err(|e| RequestError::CoordinatorError(e.to_string()))?;

			database
				.insert_keyspend_payout_psbt(&payload.offer_id_hex, &payout_keyspend_psbt_hex)
				.await
				.map_err(|e| RequestError::Database(e.to_string()))?;
			database
				.toggle_processing(&payload.offer_id_hex)
				.await
				.map_err(|e| RequestError::Database(e.to_string()))?;
			payout_keyspend_psbt_hex
		};
		return Ok(PayoutProcessingResult::ReadyPSBT(PayoutResponse {
			payout_psbt_hex: payout_keyspend_psbt_hex,
			agg_musig_nonce_hex: escrow_payout_data.agg_musig_nonce.to_string(),
			agg_musig_pubkey_ctx_hex: escrow_payout_data.aggregated_musig_pubkey_ctx_hex,
		}));
	// at least one trader has not yet submitted the satisfaction request, or a escrow is already ongoing
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
		trace!("Escrow winner not yet chosen");
		Ok(PayoutProcessingResult::DecidingEscrow)
	}
}

/// handles the returned partial signatures for the keyspend payout, if both are available it aggregates them,
/// inserts the signature in the payout tx and broadcasts it
pub async fn handle_payout_signature(
	payload: &PayoutSignatureRequest,
	coordinator: Arc<Coordinator>,
) -> Result<bool, RequestError> {
	let database = &coordinator.coordinator_db;
	check_offer_and_confirmation(&payload.offer_id_hex, &payload.robohash_hex, database).await?;

	database
		.insert_partial_sig(
			&payload.partial_sig_hex,
			&payload.offer_id_hex,
			&payload.robohash_hex,
		)
		.await
		.map_err(|e| RequestError::Database(e.to_string()))?;

	let keyspend_information = match database
		.fetch_keyspend_payout_information(&payload.offer_id_hex)
		.await
		.map_err(|e| RequestError::Database(e.to_string()))?
	{
		Some(context) => context,
		None => return Ok(false),
	};

	debug!("Keyspend info: {:?}", keyspend_information);
	trace!(
		"Keyspend agg sig : {} \n Agg pubk: {}",
		keyspend_information.agg_sig.to_string(),
		keyspend_information.agg_keyspend_pk.to_string()
	);
	warn!("Use musig2 validate partial sig to validate sigs before using to blame users providing wrong sigs");
	coordinator
		.coordinator_wallet
		.broadcast_keyspend_tx(&keyspend_information)
		.await
		.map_err(|e| RequestError::CoordinatorError(e.to_string()))?;
	database
		.delete_complete_offer(&payload.offer_id_hex)
		.await
		.map_err(|e| {
			RequestError::Database(format!(
				"Failed to delete complete offer from taken_offers: {}",
				e
			))
		})?;
	Ok(true)
}
