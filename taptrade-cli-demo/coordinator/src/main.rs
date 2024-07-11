mod communication;
mod coordinator;
mod database;
mod wallet;

use anyhow::{anyhow, Result};
use bdk::sled;
use communication::{api::*, api_server};
use coordinator::monitoring::monitor_bonds;
use coordinator::monitoring::*;
use database::CoordinatorDB;
use dotenv::dotenv;
use log::{debug, error, info, warn};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, sync::Arc};
use tokio::{sync::Mutex, task::spawn_blocking};
use wallet::*;

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
		coordinator_wallet: Arc::new(init_coordinator_wallet()?),
	});

	// begin monitoring bonds
	// spawn_blocking(monitor_bonds(Arc::clone(&coordinator)));

	// Start the API server
	api_server(coordinator).await?;
	Ok(())
}
