pub mod api;
mod utils;

use self::api::*;
use self::utils::*;
use super::*;
use crate::wallet::*;
use anyhow::Context;
use axum::{
	http::StatusCode,
	response::{IntoResponse, Response},
	routing::{get, post},
	Extension, Json, Router,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::TcpListener;

//
// Axum handler functions
//
/// Handler function to process the received data
async fn receive_order(
	Extension(database): Extension<Arc<CoordinatorDB>>,
	Extension(wallet): Extension<Arc<CoordinatorWallet>>,
	Json(order): Json<OrderRequest>,
) -> Result<Json<BondRequirementResponse>, AppError> {
	debug!("{:#?}", &order);
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
	debug!("Coordinator received new offer: {:?}", order);
	Ok(Json(bond_requirements))
}

/// receives the maker bond, verifies it and moves to offer to the active table (orderbook)
async fn submit_maker_bond(
	Extension(database): Extension<Arc<CoordinatorDB>>,
	Extension(wallet): Extension<Arc<CoordinatorWallet>>,
	Json(payload): Json<BondSubmissionRequest>,
) -> Result<Response, AppError> {
	debug!("\n\nReceived maker bond: {:?}", payload);
	let bond_requirements = if let Ok(requirements) = database
		.fetch_bond_requirements(&payload.robohash_hex)
		.await
	{
		requirements
	} else {
		return Ok(StatusCode::NOT_FOUND.into_response());
	};

	match wallet
		.validate_bond_tx_hex(&payload.signed_bond_hex, &bond_requirements)
		.await
	{
		Ok(()) => (),
		Err(e) => {
			error!("{}", e);
			return Ok(StatusCode::NOT_ACCEPTABLE.into_response());
		}
	}
	debug!("\nBond validation successful");
	let offer_id_hex: String = generate_random_order_id(16); // 16 bytes random offer id, maybe a different system makes more sense later on? (uuid or increasing counter...)
														 // create address for taker bond
	let new_taker_bond_address = wallet.get_new_address().await.context(format!(
		"Error generating taker bond address for offer id: {}",
		offer_id_hex
	))?;
	// insert bond into sql database and move offer to different table
	let bond_locked_until_timestamp = match database
		.move_offer_to_active(&payload, &offer_id_hex, new_taker_bond_address)
		.await
	{
		Ok(timestamp) => timestamp,
		Err(e) => {
			debug!("Error in validate_bond_tx_hex: {}", e);
			return Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response());
		}
	};

	// Create the JSON response
	Ok(Json(OrderActivatedResponse {
		bond_locked_until_timestamp,
		offer_id_hex,
	})
	.into_response())
}

/// returns available offers from the active table (orderbook)
async fn fetch_available_offers(
	Extension(database): Extension<Arc<CoordinatorDB>>,
	Json(payload): Json<OffersRequest>,
) -> Result<Response, AppError> {
	let offers: Option<Vec<PublicOffer>> = database.fetch_suitable_offers(&payload).await?;
	if offers.is_none() {
		return Ok(StatusCode::NO_CONTENT.into_response());
	}
	Ok(Json(PublicOffers { offers }).into_response())
}

/// receives the taker bond for a given offer, verifies it, creates escrow transaction psbt
/// and moves the offer to the taken table. Will return the trade contract psbt for the taker to sign.
async fn submit_taker_bond(
	Extension(database): Extension<Arc<CoordinatorDB>>,
	Extension(wallet): Extension<Arc<CoordinatorWallet>>,
	Json(payload): Json<OfferPsbtRequest>,
) -> Result<Response, AppError> {
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
					warn!("{}", e);
					return Ok(StatusCode::NOT_ACCEPTABLE.into_response());
				}
			}
		}
		Err(_) => return Ok(StatusCode::NOT_FOUND.into_response()),
	}
	debug!("\nTaker bond validation successful");

	panic!("Trade contract PSBT not implemented!");
	let trade_contract_psbt_taker = "".to_string(); // implement psbt
	let trade_contract_psbt_maker = "".to_string(); // implement psbt

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
	Extension(database): Extension<Arc<CoordinatorDB>>,
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
/// Supposed to be the endpoint that both maker & taker will send their part of the PSBT to (with signatures), the
/// coordinator then has to check if their signatures are valid and everything else is according to the agreed upon contract.
/// Once the coordinator has received both partitial signed PSBTs he can assemble them together to a transaction and publish it to the bitcoin network.
async fn submit_escrow_psbt(
	Extension(database): Extension<Arc<CoordinatorDB>>,
	Extension(wallet): Extension<Arc<CoordinatorWallet>>,
	Json(payload): Json<PsbtSubmissionRequest>,
) -> Result<Response, AppError> {
	panic!("implement")
}

/// Will get polled by the traders once they submitted their PSBT part. The coorinator will return status code 200 once he received both PSBTs and they got mined,
/// then the traders will know it is secure to begin with the fiat exchange and can continue with the trade (exchange information in the chat and transfer fiat).
/// We can implement this once the PSBT is done.
/// In theory this polling mechanism could also be replaced by the traders scanning the blockchain themself so they could also see once the tx is confirmed.
/// We have to see what makes more sense later, but maybe this would be more elegant. TBD.
async fn poll_escrow_confirmation(
	Extension(database): Extension<Arc<CoordinatorDB>>,
	Extension(wallet): Extension<Arc<CoordinatorWallet>>,
	Json(payload): Json<OfferTakenRequest>,
) -> Result<Response, AppError> {
	panic!("implement")
}

async fn submit_obligation_confirmation(
	Extension(database): Extension<Arc<CoordinatorDB>>,
	Extension(wallet): Extension<Arc<CoordinatorWallet>>,
	Json(payload): Json<OfferTakenRequest>,
) -> Result<Response, AppError> {
	panic!("implement")
}

/// Is supposed to get polled by the traders once they clicked on "i sent the fiat" or "i received the fiat".
/// If both agree then the payout logic (tbd) will be called (assembly of a payout transaction out of the escrow contract to their payout addresses).
/// If one of them is not happy and initiating escrow (e.g. claiming they didn't receive the fiat) then this
/// endpoint can return 201 and the escrow mediation logic will get executed (tbd).
async fn poll_final_payout(
	Extension(database): Extension<Arc<CoordinatorDB>>,
	Extension(wallet): Extension<Arc<CoordinatorWallet>>,
	Json(payload): Json<OfferTakenRequest>,
) -> Result<Response, AppError> {
	panic!("implement")
}

async fn test_api() -> &'static str {
	"Hello, World!"
}

pub async fn api_server(coordinator: Arc<Coordinator>) -> Result<()> {
	let database = Arc::clone(&coordinator.coordinator_db);
	let wallet = Arc::clone(&coordinator.coordinator_wallet);

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
		.route("/poll-final-payout", post(poll_final_payout))
		.layer(Extension(database))
		.layer(Extension(wallet));
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
