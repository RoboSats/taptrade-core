mod communication;
mod coordinator;
mod sqlite_db;
mod wallet;

use anyhow::{anyhow, Result};
use bdk::{database::MemoryDatabase, Wallet};
use communication::{api::*, api_server};
use dotenv::dotenv;
use sqlite_db::SqliteDB;
use std::{env, sync::Arc};
use wallet::CoordinatorWallet;

// populate .env with values before starting
#[tokio::main]
async fn main() -> Result<()> {
	dotenv().ok();
	// Initialize the database pool
	let coordinator_db = SqliteDB::init().await?;
	let wallet = CoordinatorWallet::init().await?;

	api_server(coordinator_db).await?;
	Ok(())
}
