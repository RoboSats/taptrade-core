mod communication;
mod coordinator;
mod database;
mod wallet;

use anyhow::{anyhow, Result};
use communication::{api::*, api_server};
use coordinator::monitoring::monitor_bonds;
use coordinator::monitoring::*;
use database::CoordinatorDB;
use dotenv::dotenv;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, sync::Arc};
use tokio::sync::Mutex;
use wallet::*;

pub struct Coordinator {
	pub coordinator_db: CoordinatorDB,
	pub coordinator_wallet: CoordinatorWallet,
}

// populate .env with values before starting
#[tokio::main]
async fn main() -> Result<()> {
	dotenv().ok();

	// Initialize the database pool
	let coordinator = Coordinator {
		coordinator_db: CoordinatorDB::init().await?,
		coordinator_wallet: CoordinatorWallet::init()?,
	};

	// begin monitoring bonds
	monitor_bonds(&coordinator).await?;

	// Start the API server
	api_server(coordinator).await?;
	Ok(())
}
