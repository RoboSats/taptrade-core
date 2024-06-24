mod api;

use super::*;
use api::{BondRequirementResponse, BondSubmissionRequest, OrderActivatedResponse, OrderRequest};
use axum::{routing::post, Json, Router};

use std::net::SocketAddr;
use tokio::net::TcpListener;

// Handler function to process the received data
async fn receive_order(Json(order): Json<OrderRequest>) -> Json<BondRequirementResponse> {
	// Print the received data to the console
	println!("Received order: {:?}", order);

	// Access individual fields
	// let robohash = &order.robohash_hex;
	let amount = order.amount_satoshi;
	// let order_type = &order.is_buy_order;
	let bond_ratio = order.bond_ratio;
	// let offer_duration= order.offer_duration_ts;

	// Create a response struct
	let response = BondRequirementResponse {
		bond_address: "Order received successfully".to_string(),
		// Add any other fields you want to include in your response
		locking_amount_sat: (amount * bond_ratio as u64 / 100),
	};

	// Return the response as JSON
	Json(response)
}

async fn submit_maker_bond(
	Json(payload): Json<BondSubmissionRequest>,
) -> Json<OrderActivatedResponse> {
	// Process the payload
	// For now, we'll just return a dummy success response
	let response = OrderActivatedResponse {
		bond_locked_until_timestamp: 0 as u128,
		order_id_hex: "Bond submitted successfully".to_string(),
	};

	// Create the JSON response
	Json(response)
}

pub async fn api_server() -> Result<()> {
	let app = Router::new()
		.route("/create-offer", post(receive_order))
		.route("/submit-maker-bond", post(submit_maker_bond));
	// add other routes here

	// Run the server on localhost:9999
	let addr = SocketAddr::from(([127, 0, 0, 1], 9999));
	println!("Listening on {}", addr);

	let tcp = TcpListener::bind(&addr).await.unwrap();
	axum::serve(tcp, app).await?;
	Ok(())
}
