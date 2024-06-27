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
// Handler function to process the received data
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

async fn submit_maker_bond(
	Extension(database): Extension<CoordinatorDB>,
	Extension(wallet): Extension<CoordinatorWallet>,
	Json(payload): Json<BondSubmissionRequest>,
) -> Result<Json<OrderActivatedResponse>, AppError> {
	let bond_requirements = database.fetch_maker_request(&payload.robohash_hex).await?;
	let offer_id_hex = generate_random_order_id(16); // 16 bytes random offer id, maybe a different system makes more sense later on? (uuid or increasing counter...)

	// validate bond (check amounts, valid inputs, correct addresses, valid signature, feerate)
	wallet
		.validate_bond_tx_hex(&payload.signed_bond_hex, &bond_requirements)
		.await?;

	// insert bond into sql database and move offer to different table
	let bond_locked_until_timestamp = database
		.move_offer_to_active(&payload, &offer_id_hex)
		.await?;

	// begin monitoring bond -> async loop monitoring bonds in sql table "active_maker_offers" -> see ../coordinator/monitoring.rs
	// show trade to orderbook -> orderbook endpoint will scan sql table "active_maker_offers" and return fitting results

	// Create the JSON response
	Ok(Json(OrderActivatedResponse {
		bond_locked_until_timestamp,
		offer_id_hex,
	}))
}

pub async fn api_server(database: CoordinatorDB, wallet: CoordinatorWallet) -> Result<()> {
	let app = Router::new()
		.route("/create-offer", post(receive_order))
		.route("/submit-maker-bond", post(submit_maker_bond))
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
