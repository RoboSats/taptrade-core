mod communication;
mod coordinator;

use anyhow::{anyhow, Error, Result};
use bdk::database::MemoryDatabase;
use bdk::Wallet;
use communication::api_server;
use dotenv::dotenv;
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::env;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Coordinator {
	pub db_pool: Arc<Pool<Sqlite>>,
	pub wallet: Arc<Wallet<MemoryDatabase>>, // using sqlite for Wallet?
}

// populate .env with values before starting
#[tokio::main]
async fn main() -> Result<()> {
	dotenv().ok();
	// Initialize the database pool
	let db_pool = SqlitePoolOptions::new()
		.connect("sqlite:./db/trades.db")
		.await
		.unwrap();
	let shared_db_pool: Arc<sqlx::Pool<sqlx::Sqlite>> = Arc::new(db_pool);

	// let coordinator = Coordinator {
	// 	db_pool: shared_db_pool,
	// 	wallet: // impl wallet
	// };

	// api_server(coordinator).await?;
	Ok(())
}
