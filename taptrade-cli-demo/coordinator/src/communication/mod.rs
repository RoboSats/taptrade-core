pub mod api;

use self::api::*;
use super::*;
use axum::{
	http::StatusCode,
	response::{IntoResponse, Response},
	routing::post,
	Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::TcpListener;
// use crate::coordinator::verify_psbt;

// Handler function to process the received data
async fn receive_order(
	Extension(database): Extension<CoordinatorDB>,
	Extension(wallet): Extension<CoordinatorWallet>,
	Json(order): Json<OrderRequest>,
) -> Result<Json<BondRequirementResponse>, AppError> {
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
	// Process the payload
	// For now, we'll just return a dummy success response
	let response = OrderActivatedResponse {
		bond_locked_until_timestamp: 0 as u128,
		order_id_hex: "Bond submitted successfully".to_string(),
	};

	// Create the JSON response
	Json(response)
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
