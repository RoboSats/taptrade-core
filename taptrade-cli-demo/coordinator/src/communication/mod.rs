pub mod api;
pub mod communication_utils;
pub mod handler_errors;

use self::communication_utils::*;
use super::*;
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
	if offer.validate().is_err() {
		Ok(StatusCode::BAD_REQUEST.into_response())
	} else {
		let bond_requirements = process_order(coordinator, &offer).await?;
		Ok(Json(bond_requirements).into_response())
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
			Ok(StatusCode::NOT_FOUND.into_response())
		}
		Err(BondError::InvalidBond(e)) => {
			warn!("Invalid bond submission: {e}");
			Ok(StatusCode::NOT_ACCEPTABLE.into_response())
		}
		Err(BondError::CoordinatorError(e)) => {
			error!("Coordinator error on bond submission: {e}");
			Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
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
		Err(FetchOffersError::Database(e)) => {
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
			Ok(StatusCode::NOT_FOUND.into_response())
		}
		Err(BondError::InvalidBond(e)) => {
			warn!("Invalid bond submission: {e}");
			Ok(StatusCode::NOT_ACCEPTABLE.into_response())
		}
		Err(BondError::CoordinatorError(e)) => {
			error!("Coordinator error on bond submission: {e}");
			Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
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
		Err(FetchOffersError::Database(e)) => {
			error!("Database error fetching offer status maker: {e}");
			Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
		}
	}
}

/// receives the signed escrow psbt and verifies it
/// Supposed to be the endpoint that both maker & taker will send their part of the PSBT to (with signatures), the
/// coordinator then has to check if their signatures are valid and everything else is according to the agreed upon contract.
/// Once the coordinator has received both partial signed PSBTs he can assemble them together to a transaction and publish it to the bitcoin network.
async fn submit_escrow_psbt(
	Extension(coordinator): Extension<Arc<Coordinator>>,
	Json(payload): Json<PsbtSubmissionRequest>,
) -> Result<Response, AppError> {
	debug!("\n\nReceived signed escrow psbt: {:?}", payload);
	match handle_signed_escrow_psbt(&payload, coordinator).await {
		Ok(()) => Ok(StatusCode::OK.into_response()),
		Err(RequestError::PsbtAlreadySubmitted) => {
			Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
		}
		Err(RequestError::PsbtInvalid(e)) => {
			warn!("Invalid PSBT: {e}");
			Ok(StatusCode::NOT_ACCEPTABLE.into_response())
		}
		_ => Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response()),
		// Err(RequestError::NotFound) => {
		// 	info!("Offer for escrow psbt not found");
		// 	Ok(StatusCode::NOT_FOUND.into_response())
		// }
		// Err(RequestError::NotConfirmed) => {
		// 	info!("Offer for escrow psbt not confirmed");
		// 	Ok(StatusCode::NOT_ACCEPTABLE.into_response())
		// }
		// Err(RequestError::Database(e)) => {
		// 	error!("Database error fetching escrow psbt: {e}");
		// 	Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
		// }
	}
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
		Err(FetchEscrowConfirmationError::NotFound) => {
			info!("Escrow confirmation check transaction not found");
			Ok(StatusCode::NOT_FOUND.into_response())
		}
		Err(FetchEscrowConfirmationError::Database(e)) => {
			error!("Database error fetching escrow confirmation: {e}");
			Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
		}
	}
}

async fn submit_obligation_confirmation(
	Extension(coordinator): Extension<Arc<Coordinator>>,
	Json(payload): Json<OfferTakenRequest>,
) -> Result<Response, AppError> {
	match handle_obligation_confirmation(&payload, coordinator).await {
		Ok(_) => Ok(StatusCode::OK.into_response()),
		Err(RequestError::NotFound) => {
			info!("Offer for obligation confirmation not found");
			Ok(StatusCode::NOT_FOUND.into_response())
		}
		Err(RequestError::NotConfirmed) => {
			info!("Offer for obligation confirmation not confirmed");
			Ok(StatusCode::NOT_ACCEPTABLE.into_response())
		}
		Err(RequestError::Database(e)) => {
			error!("Database error fetching obligation confirmation: {e}");
			Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
		}
		_ => {
			error!("Unknown error handling obligation confirmation");
			Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
		}
	}
}

// or

// gets called if one of the traders wants to initiate escrow (e.g. claiming they didn't receive the fiat)
// before timeout ends
async fn request_escrow(
	Extension(coordinator): Extension<Arc<Coordinator>>,
	Json(payload): Json<TradeObligationsUnsatisfied>,
) -> Result<Response, AppError> {
	match initiate_escrow(&payload, coordinator).await {
		Ok(_) => Ok(StatusCode::OK.into_response()),
		Err(RequestError::NotConfirmed) => {
			info!("Offer tx for escrow initiation not confirmed");
			Ok(StatusCode::NOT_ACCEPTABLE.into_response())
		}
		Err(RequestError::NotFound) => {
			info!("Offer for escrow initiation not found");
			Ok(StatusCode::NOT_FOUND.into_response())
		}
		Err(RequestError::Database(e)) => {
			error!("Database error fetching obligation confirmation: {e}");
			Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
		}
		_ => {
			error!("Unknown error handling request_escrow()");
			Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
		}
	}
}

/// Is supposed to get polled by the traders once they clicked on "i sent the fiat" or "i received the fiat".
/// If both agree then the payout logic (tbd) will be called (assembly of a payout transaction out of the escrow contract to their payout addresses).
/// If one of them is not happy and initiating escrow (e.g. claiming they didn't receive the fiat) then this
/// endpoint can return 201 and the escrow mediation logic will get executed (tbd).
async fn poll_final_payout(
	Extension(coordinator): Extension<Arc<Coordinator>>,
	Json(payload): Json<OfferTakenRequest>,
) -> Result<Response, AppError> {
	match handle_final_payout(&payload, coordinator).await {
		Ok(PayoutProcessingResult::NotReady) => Ok(StatusCode::ACCEPTED.into_response()),
		Ok(PayoutProcessingResult::LostEscrow) => Ok(StatusCode::GONE.into_response()),
		Ok(PayoutProcessingResult::ReadyPSBT(psbt_and_nonce)) => {
			Ok(Json(psbt_and_nonce).into_response())
		}
		Ok(PayoutProcessingResult::DecidingEscrow) => Ok(StatusCode::PROCESSING.into_response()),
		Err(RequestError::NotConfirmed) => {
			info!("Offer tx for final payout not confirmed");
			Ok(StatusCode::NOT_ACCEPTABLE.into_response())
		}
		Err(RequestError::NotFound) => {
			info!("Offer for final payout not found");
			Ok(StatusCode::NOT_FOUND.into_response())
		}
		Err(RequestError::Database(e)) => {
			error!("Database error fetching final payout: {e}");
			Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
		}
		_ => {
			error!("Unknown error handling poll_final_payout()");
			Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
		}
	}
}

async fn submit_payout_signature(
	Extension(coordinator): Extension<Arc<Coordinator>>,
	Json(payload): Json<PayoutSignatureRequest>,
) -> Result<Response, AppError> {
	match handle_payout_signature(&payload, coordinator).await {
		// received both sigs, published final tx
		Ok(true) => Ok(StatusCode::OK.into_response()),

		// this was the first signature
		Ok(false) => Ok(StatusCode::ACCEPTED.into_response()),

		// 	Err(RequestError::NotConfirmed) => {
		// 		info!("Offer tx for final payout not confirmed");
		// 		Ok(StatusCode::NOT_ACCEPTABLE.into_response())
		// 	}
		// 	Err(RequestError::NotFound) => {
		// 		info!("Offer for final payout not found");
		// 		Ok(StatusCode::NOT_FOUND.into_response())
		// 	}
		// 	Err(RequestError::Database(e)) => {
		// 		error!("Database error fetching final payout: {e}");
		// 		Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
		// 	}
		_ => {
			error!("Unknown error handling submit_payout_signature()");
			Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
		}
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
		.route("/submit-payout-signature", post(submit_payout_signature))
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
