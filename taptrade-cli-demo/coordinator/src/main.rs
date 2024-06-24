mod communication;
mod coordinator;

use anyhow::{anyhow, Error, Result};
use communication::api_server;
use dotenv::dotenv;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::env;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Coordinator {
	pub db_pool: Arc<Pool<Sqlite>>,
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

	let coordinator = Coordinator {
		db_pool: shared_db_pool,
	};

	api_server(coordinator).await?;
	Ok(())
}
