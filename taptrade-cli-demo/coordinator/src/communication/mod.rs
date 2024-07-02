pub mod api;
mod utils;

use self::api::*;
use self::utils::*;
use super::*;
use axum::{
	http::StatusCode,
	response::{IntoResponse, Response},
	routing::post,
	Extension, Json, Router,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::TcpListener;
// use crate::coordinator::verify_psbt;

//
// Axum handler functions
//
/// Handler function to process the received data
async fn receive_order(
	Extension(database): Extension<CoordinatorDB>,
	Extension(wallet): Extension<CoordinatorWallet>,
	Json(order): Json<OrderRequest>,
) -> Result<Json<BondRequirementResponse>, AppError> {
	if order.sanity_check().is_err() {
		return Err(AppError(anyhow!("Invalid order request")));
	}
	let bond_requirements = BondRequirementResponse {
		bond_address: wallet.get_new_address().await?,
		locking_amount_sat: order.amount_satoshi * order.bond_ratio as u64 / 100,
	};
	// insert offer into sql database
	database
		.insert_new_maker_request(&order, &bond_requirements)
		.await?;
	println!("Coordinator received new offer: {:?}", order);
	Ok(Json(bond_requirements))
}

/// receives the maker bond, verifies it and moves to offer to the active table (orderbook)
async fn submit_maker_bond(
	Extension(database): Extension<CoordinatorDB>,
	Extension(wallet): Extension<CoordinatorWallet>,
	Json(payload): Json<BondSubmissionRequest>,
) -> Result<Response, AppError> {
	let bond_requirements = database.fetch_maker_request(&payload.robohash_hex).await?;

	// validate bond (check amounts, valid inputs, correct addresses, valid signature, feerate)
	// if !wallet
	// 	.validate_bond_tx_hex(&payload.signed_bond_hex)
	// 	.await?
	// {
	// 	return Ok(StatusCode::NOT_ACCEPTABLE.into_response());
	// }
	let offer_id_hex = generate_random_order_id(16); // 16 bytes random offer id, maybe a different system makes more sense later on? (uuid or increasing counter...)
												 // create address for taker bond
	let new_taker_bond_address = wallet.get_new_address().await?;
	// insert bond into sql database and move offer to different table
	let bond_locked_until_timestamp = database
		.move_offer_to_active(&payload, &offer_id_hex, new_taker_bond_address)
		.await?;

	// begin monitoring bond -> async loop monitoring bonds in sql table "active_maker_offers" -> see ../coordinator/monitoring.rs
	// show trade to orderbook -> orderbook endpoint will scan sql table "active_maker_offers" and return fitting results

	// Create the JSON response
	Ok(Json(OrderActivatedResponse {
		bond_locked_until_timestamp,
		offer_id_hex,
	})
	.into_response())
}

/// returns available offers from the active table (orderbook)
async fn fetch_available_offers(
	Extension(database): Extension<CoordinatorDB>,
	Json(payload): Json<OffersRequest>,
) -> Result<Json<PublicOffers>, AppError> {
	let offers: Option<Vec<PublicOffer>> = database.fetch_suitable_offers(&payload).await?;

	Ok(Json(PublicOffers { offers }))
}

/// receives the taker bond for a given offer, verifies it, creates escrow transaction psbt
/// and moves the offer to the taken table. Will return the trade contract psbt for the taker to sign.
async fn submit_taker_bond(
	Extension(database): Extension<CoordinatorDB>,
	Extension(wallet): Extension<CoordinatorWallet>,
	Json(payload): Json<OfferPsbtRequest>,
) -> Result<Response, AppError> {
	let bond_requirements = database
		.fetch_taker_bond_requirements(&payload.offer.offer_id_hex)
		.await;
	// match bond_requirements {
	// 	Ok(bond_requirements) => {
	// 		if !wallet
	// 			.validate_bond_tx_hex(&payload.trade_data.signed_bond_hex, &bond_requirements)
	// 			.await?
	// 		{
	// 			dbg!("Taker Bond validation failed");
	// 			return Ok(StatusCode::NOT_ACCEPTABLE.into_response());
	// 		}
	// 	}
	// 	Err(_) => return Ok(StatusCode::NOT_FOUND.into_response()),
	// }

	let trade_contract_psbt_taker = "".to_string(); // implement psbt
	let trade_contract_psbt_maker = "".to_string(); // implement psbt
	panic!("Trade contract PSBT not implemented!");

	database
		.add_taker_info_and_move_table(
			&payload,
			&trade_contract_psbt_maker,
			&trade_contract_psbt_taker,
		)
		.await?;
	Ok(Json(OfferTakenResponse {
		trade_psbt_hex_to_sign: trade_contract_psbt_taker,
	})
	.into_response())
}

/// gets polled by the maker and returns the escrow psbt in case the offer has been taken
async fn request_offer_status_maker(
	Extension(database): Extension<CoordinatorDB>,
	Json(payload): Json<OfferTakenRequest>,
) -> Result<Response, AppError> {
	let offer = database
		.fetch_taken_offer_maker(&payload.order_id_hex, &payload.robohash_hex)
		.await?;
	match offer {
		Some(offer) => Ok(Json(OfferTakenResponse {
			trade_psbt_hex_to_sign: offer,
		})
		.into_response()),
		None => Ok(StatusCode::NO_CONTENT.into_response()),
	}
}

/// receives the signed escrow psbt and verifies it
async fn submit_escrow_psbt(
	Extension(database): Extension<CoordinatorDB>,
	Extension(wallet): Extension<CoordinatorWallet>,
	Json(payload): Json<PsbtSubmissionRequest>,
) -> Result<Response, AppError> {
	panic!("implement")
}

async fn poll_escrow_confirmation(
	Extension(database): Extension<CoordinatorDB>,
	Extension(wallet): Extension<CoordinatorWallet>,
	Json(payload): Json<OfferTakenRequest>,
) -> Result<Response, AppError> {
	panic!("implement")
}

async fn submit_obligation_confirmation(
	Extension(database): Extension<CoordinatorDB>,
	Extension(wallet): Extension<CoordinatorWallet>,
	Json(payload): Json<OfferTakenRequest>,
) -> Result<Response, AppError> {
	panic!("implement")
}

async fn poll_final_payout(
	Extension(database): Extension<CoordinatorDB>,
	Extension(wallet): Extension<CoordinatorWallet>,
	Json(payload): Json<OfferTakenRequest>,
) -> Result<Response, AppError> {
	panic!("implement")
}

pub async fn api_server(database: CoordinatorDB, wallet: CoordinatorWallet) -> Result<()> {
	let app = Router::new()
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
		.route("/poll-final-payout", post(poll_final_payout))
		.layer(Extension(database))
		.layer(Extension(wallet));
	// add other routes here

	// Run the server on localhost:9999
	let addr = SocketAddr::from(([127, 0, 0, 1], 9999));
	let tcp = TcpListener::bind(&addr).await.unwrap();
	axum::serve(tcp, app).await?;
	println!("Listening on {}", addr);

	Ok(())
}

// ANYHOW ERROR HANDLING
// --------------
// Make our own error that wraps `anyhow::Error`.
struct AppError(anyhow::Error);

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
