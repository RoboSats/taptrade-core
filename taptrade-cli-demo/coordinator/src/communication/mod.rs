mod api;

use reqwest::StatusCode;
use axum::{routing::post, Json, Router, response::{IntoResponse, Response}, };
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::TcpListener;
// use super::*;
// use api::{BondRequirementResponse, BondSubmissionRequest, OrderActivatedResponse, OrderRequest};
// use axum::{
// 	http::StatusCode, response::IntoResponse, response::Response, routing::post, Extension, Json,
// 	Router,
// };
// use sqlx::sqlite::SqliteLockingMode;
use crate::verify_bond::verify_psbt;

// Handler function to process the received data
async fn receive_order(
	Extension(state): Extension<Arc<Coordinator>>,
	Json(order): Json<OrderRequest>,
) -> Result<Json<BondRequirementResponse>, AppError> {
	// Connecting to SQLite database
	let db_pool = state.db_pool.clone();
	let mut conn = db_pool.acquire().await.unwrap();

	// sqlx::query!(
	// 	"INSERT INTO orders (field1, field2) VALUES (?, ?)",
	// 	order.field1,
	// 	order.field2
	// )

	// insert offer into sql database
	// generate locking address for bond

	println!("Coordinator received new offer: {:?}", order);
	Ok(Json(BondRequirementResponse {
		bond_address: bond_address,
		locking_amount_sat: order.amount_satoshi * order.bond_ratio as u64 / 100,
	}))
}

// async fn submit_maker_bond(
// 	Json(payload): Json<BondSubmissionRequest>,
// ) -> Result<Json<OrderActivatedResponse>, AppError> {
// 	// Process the payload
// 	// For now, we'll just return a dummy success response
// 	let response = OrderActivatedResponse {
// 		bond_locked_until_timestamp: 0 as u128,
// 		order_id_hex: "Bond submitted successfully".to_string(),
// 	};

// 	// Create the JSON response
// 	Json(response)
// }

pub async fn api_server(coordinator: Coordinator) -> Result<()> {
	let app = Router::new()
		.route("/create-offer", post(receive_order))
		// .route("/submit-maker-bond", post(submit_maker_bond));
		.layer(Extension(coordinator));
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
