mod communication;
mod coordinator;
mod database;
mod wallet;

use anyhow::{anyhow, Result};
use bdk::sled;
use communication::{api::*, api_server, communication_utils::*, handler_errors::*};
use coordinator::{
	bond_monitoring::*, coordinator_utils::*,
	tx_confirmation_monitoring::update_transaction_confirmations, *,
};
use database::CoordinatorDB;
use dotenvy::dotenv;
use log::{debug, error, info, trace, warn};
use rand::Rng;
use std::{g
	env,
	sync::Arc,
	time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;
use validator::{Validate, ValidationError};
use wallet::{escrow_psbt::*, *};

pub struct Coordinator {
	pub coordinator_db: Arc<CoordinatorDB>,
	pub coordinator_wallet: Arc<CoordinatorWallet<sled::Tree>>,
}

// populate .env with values before starting
#[tokio::main]
async fn main() -> Result<()> {
	env_logger::builder()
		.filter_module("coordinator", log::LevelFilter::Debug)
		.filter_level(log::LevelFilter::Info)
		.init();
	dotenv().ok();
	debug!("Starting coordinator");

	// Initialize the database pool
	let coordinator = Arc::new(Coordinator {
		coordinator_db: Arc::new(CoordinatorDB::init().await?),
		coordinator_wallet: Arc::new(init_coordinator_wallet().await?),
	});

	// begin monitoring bonds
	let coordinator_ref = Arc::clone(&coordinator);
	tokio::spawn(async move {
		loop {
			if let Err(e) = monitor_bonds(coordinator_ref.clone()).await {
				error!("Error in monitor_bonds: {:?}", e);
				// Optionally add a delay before retrying
				tokio::time::sleep(std::time::Duration::from_secs(5)).await;
			}
		}
	});

	// begin monitoring escrow transactions confirmations
	let coordinator_ref = Arc::clone(&coordinator);
	tokio::spawn(async move { update_transaction_confirmations(coordinator_ref).await });

	// Start the API server
	api_server(coordinator).await?;
	Ok(())
}
