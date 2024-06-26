mod communication;
mod coordinator;
mod database;
mod wallet;

use anyhow::{anyhow, Result};
use communication::{api::*, api_server};
use database::CoordinatorDB;
use dotenv::dotenv;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, sync::Arc};
use tokio::sync::Mutex;
use wallet::CoordinatorWallet;

// populate .env with values before starting
#[tokio::main]
async fn main() -> Result<()> {
	dotenv().ok();

	// Initialize the database pool
	let coordinator_db = CoordinatorDB::init().await?;
	let wallet = CoordinatorWallet::init()?;

	api_server(coordinator_db, wallet).await?;
	Ok(())
}
