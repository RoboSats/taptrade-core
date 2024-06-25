use anyhow::Context;

use super::*;
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};

#[derive(Clone, Debug)]
pub struct CoordinatorDB {
	pub db_pool: Arc<Pool<Sqlite>>,
}

// is our implementation secure against sql injections?
impl CoordinatorDB {
	// will either create a new db or load existing one. Will create according tables in new db
	pub async fn init() -> Result<Self> {
		let db_pool = SqlitePoolOptions::new()
			.connect(
				&("sqlite:".to_string()
					+ &env::var("DATABASE_PATH")
						.context("Parsing DATABASE_PATH from .env failed")?),
			)
			.await
			.map_err(|e| anyhow!("Failed to connect to SQLite database: {}", e))?;

		let shared_db_pool = Arc::new(db_pool);
		// Create the trades table if it doesn't exist
		sqlx::query(
			// robohash is binary hash
			"CREATE TABLE IF NOT EXISTS maker_requests (
					robohash BLOB PRIMARY KEY,
					is_buy_order INTEGER,
					amount_sat INTEGER NOT NULL,
					bond_ratio INTEGER NOT NULL,
					offer_duration_ts INTEGER NOT NULL,
					bond_address TEXT NOT NULL,
					bond_amount_sat INTEGER NOT NULL,
				)",
		)
		.execute(&*shared_db_pool)
		.await?;

		Ok(Self {
			db_pool: shared_db_pool,
		})
	}

	pub async fn insert_new_maker_request(
		&self,
		order: &OrderRequest,
		bond_requirements: &BondRequirementResponse,
	) -> Result<()> {
		let bool_to_sql_int = |flag: bool| if flag { Some(1) } else { None };

		sqlx::query(
			"INSERT OR REPLACE INTO maker_requests (robohash, is_buy_order, amount_sat,
					bond_ratio, offer_duration_ts, bond_address, bond_amount_sat)
					VALUES (?, ?, ?, ?, ?, ?, ?)",
		)
		.bind(hex::decode(&order.robohash_hex)?)
		.bind(bool_to_sql_int(order.is_buy_order))
		.bind(order.amount_satoshi as i64)
		.bind(order.bond_ratio)
		.bind(order.offer_duration_ts as i64)
		.bind(bond_requirements.bond_address.clone())
		.bind(bond_requirements.locking_amount_sat as i64)
		.execute(&*self.db_pool)
		.await?;

		Ok(())
	}
}
