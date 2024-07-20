pub mod api;
mod utils;

use self::api::*;
use self::utils::*;
use super::*;
use crate::wallet::*;
use axum::{
	http::StatusCode,
	response::{IntoResponse, Response},
	routing::{get, post},
	Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::TcpListener;

//
// Axum handler functions
//
/// Handler function to process the received data
async fn receive_order(
	Extension(coordinator): Extension<Arc<Coordinator>>,
	Json(offer): Json<OfferRequest>,
) -> Result<Response, AppError> {
	if let Err(_) = offer.validate() {
		return Ok(StatusCode::BAD_REQUEST.into_response());
	} else {
		let bond_requirements = process_order(coordinator, &offer).await?;
		return Ok(Json(bond_requirements).into_response());
	}
}

/// receives the maker bond, verifies it and moves to offer to the active table (orderbook)
async fn submit_maker_bond(
	Extension(coordinator): Extension<Arc<Coordinator>>,
	Json(payload): Json<BondSubmissionRequest>,
) -> Result<Response, AppError> {
	debug!("\n\nReceived maker bond: {:?}", payload);

	match handle_maker_bond(&payload, coordinator).await {
		Ok(offer_activated_response) => Ok(Json(offer_activated_response).into_response()),
		Err(BondError::BondNotFound) => {
			info!("Bond requirements not found in database");
			return Ok(StatusCode::NOT_FOUND.into_response());
		}
		Err(BondError::InvalidBond(e)) => {
			warn!("Invalid bond submission: {e}");
			return Ok(StatusCode::NOT_ACCEPTABLE.into_response());
		}
		Err(BondError::CoordinatorError(e)) => {
			error!("Coordinator error on bond submission: {e}");
			return Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response());
		}
	}
}

/// returns available offers from the active table (orderbook)
async fn fetch_available_offers(
	Extension(coordinator): Extension<Arc<Coordinator>>,
	Json(payload): Json<OffersRequest>,
) -> Result<Response, AppError> {
	debug!("\n\nReceived offer request: {:?}", payload);

	match get_public_offers(&payload, coordinator).await {
		Ok(offers) => Ok(Json(offers).into_response()),
		Err(FetchOffersError::NoOffersAvailable) => Ok(StatusCode::NO_CONTENT.into_response()),
		Err(FetchOffersError::DatabaseError(e)) => {
			error!("Database error fetching offers: {e}");
			Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
		}
	}
}

/// receives the taker bond for a given offer, verifies it, creates escrow transaction psbt
/// and moves the offer to the taken table. Will return the trade contract psbt for the taker to sign.
async fn submit_taker_bond(
	Extension(coordinator): Extension<Arc<Coordinator>>,
	Json(payload): Json<OfferPsbtRequest>,
) -> Result<Response, AppError> {
	debug!("\n\nReceived taker bond: {:?}", payload);

	match handle_taker_bond(&payload, coordinator).await {
		Ok(offer_taken_response) => Ok(Json(offer_taken_response).into_response()),
		Err(BondError::BondNotFound) => {
			info!("Bond requirements not found in database");
			return Ok(StatusCode::NOT_FOUND.into_response());
		}
		Err(BondError::InvalidBond(e)) => {
			warn!("Invalid bond submission: {e}");
			return Ok(StatusCode::NOT_ACCEPTABLE.into_response());
		}
		Err(BondError::CoordinatorError(e)) => {
			error!("Coordinator error on bond submission: {e}");
			return Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response());
		}
	}
}

/// gets polled by the maker and returns the escrow psbt in case the offer has been taken
async fn request_offer_status_maker(
	Extension(coordinator): Extension<Arc<Coordinator>>,
	Json(payload): Json<OfferTakenRequest>,
) -> Result<Response, AppError> {
	debug!("\n\nReceived offer status request: {:?}", payload);

	match get_offer_status_maker(&payload, coordinator).await {
		Ok(offer_taken_response) => Ok(Json(offer_taken_response).into_response()),
		Err(FetchOffersError::NoOffersAvailable) => Ok(StatusCode::NO_CONTENT.into_response()),
		Err(FetchOffersError::DatabaseError(e)) => {
			error!("Database error fetching offers: {e}");
			Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
		}
	}
}

/// receives the signed escrow psbt and verifies it
/// Supposed to be the endpoint that both maker & taker will send their part of the PSBT to (with signatures), the
/// coordinator then has to check if their signatures are valid and everything else is according to the agreed upon contract.
/// Once the coordinator has received both partitial signed PSBTs he can assemble them together to a transaction and publish it to the bitcoin network.
async fn submit_escrow_psbt(
	Extension(coordinator): Extension<Arc<Coordinator>>,
	Json(payload): Json<PsbtSubmissionRequest>,
) -> Result<Response, AppError> {
	panic!("implement")

	// check if psbt is correct, valid and signed
	// publish psbt if it is correct
	// return 200 if everything is correct
	// return 400 if something is wrong
}

/// Will get polled by the traders once they submitted their PSBT part. The coorinator will return status code 200 once he received both PSBTs and they got mined,
/// then the traders will know it is secure to begin with the fiat exchange and can continue with the trade (exchange information in the chat and transfer fiat).
/// In theory this polling mechanism could also be replaced by the traders scanning the blockchain themself so they could also see once the tx is confirmed.
async fn poll_escrow_confirmation(
	Extension(coordinator): Extension<Arc<Coordinator>>,
	Json(payload): Json<OfferTakenRequest>,
) -> Result<Response, AppError> {
	match fetch_escrow_confirmation_status(&payload, coordinator).await {
		Ok(true) => Ok(StatusCode::OK.into_response()),
		Ok(false) => Ok(StatusCode::ACCEPTED.into_response()),
		Err(FetchEscrowConfirmationError::NotFoundError) => {
			info!("Escrow confirmation check transaction not found");
			Ok(StatusCode::NOT_FOUND.into_response())
		}
		Err(FetchEscrowConfirmationError::DatabaseError(e)) => {
			error!("Database error fetching escrow confirmation: {e}");
			Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
		}
	}
}

async fn submit_obligation_confirmation(
	Extension(database): Extension<Arc<CoordinatorDB>>,
	Json(payload): Json<OfferTakenRequest>,
) -> Result<Response, AppError> {
	// sanity check if offer is in table and if the escrow tx is confirmed
	if !database
		.is_valid_robohash_in_table(&payload.robohash_hex, &payload.offer_id_hex)
		.await? || !database
		.fetch_escrow_tx_confirmation_status(&payload.offer_id_hex)
		.await?
	{
		return Ok(StatusCode::NOT_FOUND.into_response());
	}
	database
		.set_trader_happy_field(&payload.offer_id_hex, &payload.robohash_hex, true)
		.await?;
	Ok(StatusCode::OK.into_response())
}

// or

// gets called if one of the traders wants to initiate escrow (e.g. claiming they didn't receive the fiat)
// before timeout ends
async fn request_escrow(
	Extension(database): Extension<Arc<CoordinatorDB>>,
	Json(payload): Json<TradeObligationsUnsatisfied>,
) -> Result<Response, AppError> {
	if !database
		.is_valid_robohash_in_table(&payload.robohash_hex, &payload.offer_id_hex)
		.await? || !database
		.fetch_escrow_tx_confirmation_status(&payload.offer_id_hex)
		.await?
	{
		return Ok(StatusCode::NOT_FOUND.into_response());
	}
	database
		.set_trader_happy_field(&payload.offer_id_hex, &payload.robohash_hex, false)
		.await?;

	Ok(StatusCode::OK.into_response())
}

/// Is supposed to get polled by the traders once they clicked on "i sent the fiat" or "i received the fiat".
/// If both agree then the payout logic (tbd) will be called (assembly of a payout transaction out of the escrow contract to their payout addresses).
/// If one of them is not happy and initiating escrow (e.g. claiming they didn't receive the fiat) then this
/// endpoint can return 201 and the escrow mediation logic will get executed (tbd).
async fn poll_final_payout(
	Extension(database): Extension<Arc<CoordinatorDB>>,
	Extension(wallet): Extension<Arc<CoordinatorWallet<sled::Tree>>>,
	Json(payload): Json<OfferTakenRequest>,
) -> Result<Response, AppError> {
	if !database
		.is_valid_robohash_in_table(&payload.robohash_hex, &payload.offer_id_hex)
		.await? || !database
		.fetch_escrow_tx_confirmation_status(&payload.offer_id_hex)
		.await?
	{
		return Ok(StatusCode::NOT_FOUND.into_response());
	}

	let trader_happiness = database
		.fetch_trader_happiness(&payload.offer_id_hex)
		.await?;
	if trader_happiness.maker_happy.is_some_and(|x| x == true)
		&& trader_happiness.taker_happy.is_some_and(|x| x == true)
	{
		panic!("Implement wallet.assemble_keyspend_payout_psbt()");
	// let payout_keyspend_psbt_hex = wallet
	// 	.assemble_keyspend_payout_psbt(&payload.offer_id_hex, &payload.robohash_hex)
	// 	.await
	// 	.context("Error assembling payout PSBT")?;
	// return Ok(String::from(payout_keyspend_psbt_hex).into_response());
	} else if (trader_happiness.maker_happy.is_none() || trader_happiness.taker_happy.is_none())
		&& !trader_happiness.escrow_ongoing
	{
		return Ok(StatusCode::ACCEPTED.into_response());
	}
	// if one of them is not happy
	// open escrow cli on coordinator to decide who will win (chat/dispute is out of scope for this demo)
	// once decided who will win assemble the correct payout psbt and return it to the according trader
	// the other trader gets a error code/ end of trade code
	// escrow winner has to be set true with a cli input of the coordinator. This could be an api
	// endpoint for the admin UI frontend in the future
	if let Some(escrow_winner) = database.fetch_escrow_result(&payload.offer_id_hex).await? {
		if escrow_winner == payload.robohash_hex {
			panic!("Implement wallet.assemble_script_payout_psbt()");
		// let script_payout_psbt_hex = wallet
		// 	.assemble_script_payout_psbt(&payload.offer_id_hex, &payload.robohash_hex, is_maker_bool)
		// 	.await
		// 	.context("Error assembling payout PSBT")?;
		// return Ok(String::from(payout_keyspend_psbt_hex).into_response());
		} else {
			return Ok(StatusCode::GONE.into_response()); // this will be returned to the losing trader
		}
	} else {
		return Ok(StatusCode::PROCESSING.into_response()); // this will be returned if the coordinator hasn't decided yet
	}
}

async fn test_api() -> &'static str {
	"Hello, World!"
}

pub async fn api_server(coordinator: Arc<Coordinator>) -> Result<()> {
	let app = Router::new()
		.route("/test", get(test_api))
		.route("/create-offer", post(receive_order))
		.route("/submit-maker-bond", post(submit_maker_bond))
		.route("/fetch-available-offers", post(fetch_available_offers))
		.route("/submit-taker-bond", post(submit_taker_bond))
		.route("/request-offer-status", post(request_offer_status_maker))
		.route("/submit-escrow-psbt", post(submit_escrow_psbt))
		.route("/poll-escrow-confirmation", post(poll_escrow_confirmation))
		.route(
			"/submit-obligation-confirmation",
			post(submit_obligation_confirmation),
		)
		.route("/request-escrow", post(request_escrow))
		.route("/poll-final-payout", post(poll_final_payout))
		.layer(Extension(coordinator));
	// add other routes here

	let port: u16 = env::var("PORT")
		.unwrap_or_else(|_| "9999".to_string())
		.parse()?;
	info!("Listening on {}", port);
	let addr = SocketAddr::from(([127, 0, 0, 1], port));
	let tcp = TcpListener::bind(&addr).await.unwrap();
	axum::serve(tcp, app).await?;

	Ok(())
}

// ANYHOW ERROR HANDLING
// --------------
// Make our own error that wraps `anyhow::Error`.
#[derive(Debug)]
pub struct AppError(anyhow::Error);

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for AppError {
	fn into_response(self) -> Response {
		(
			StatusCode::INTERNAL_SERVER_ERROR,
			format!("Something went wrong: {}", self.0),
		)
			.into_response()
	}
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
// `Result<_, AppError>`. That way you don't need to do that manually.
impl<E> From<E> for AppError
where
	E: Into<anyhow::Error>,
{
	fn from(err: E) -> Self {
		Self(err.into())
	}
}
