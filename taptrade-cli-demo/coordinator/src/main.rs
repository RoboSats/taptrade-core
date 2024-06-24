mod communication;
mod coordinator;

use anyhow::{anyhow, Result};
use communication::api_server;
use dotenv::dotenv;
use serde::{Deserialize, Serialize};
use std::env;

// populate .env with values before starting
#[tokio::main]
async fn main() -> Result<()> {
	dotenv().ok();

	api_server().await?;
	Ok(())
}
