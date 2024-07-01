use anyhow::Context;

use super::*;
use sqlx::{sqlite::SqlitePoolOptions, Pool, Row, Sqlite};
use std::env;

#[derive(Clone, Debug)]
pub struct CoordinatorDB {
	pub db_pool: Arc<Pool<Sqlite>>,
}

// db structure of offers awaiting bond submission in table maker_requests
pub struct AwaitingBondOffer {
	pub robohash_hex: String,
	pub is_buy_order: bool,
	pub amount_satoshi: u64,
	pub bond_ratio: u8,
	pub offer_duration_ts: u64,
	pub bond_address: String,
	pub bond_amount_sat: u64,
}

// is our implementation resistant against sql injections?
impl CoordinatorDB {
	// will either create a new db or load existing one. Will create according tables in new db
	pub async fn init() -> Result<Self> {
		dbg!(env::var("DATABASE_PATH")?);
		let db_path =
			env::var("DATABASE_PATH").context("Parsing DATABASE_PATH from .env failed")?;

		// Add the `?mode=rwc` parameter to create the database if it doesn't exist
		let connection_string = format!("sqlite:{}?mode=rwc", db_path);

		let db_pool = SqlitePoolOptions::new()
			.connect(&connection_string)
			.await
			.map_err(|e| anyhow!("Failed to connect to SQLite database: {}", e))?;

		// Create the trades table if it doesn't exist
		sqlx::query(
			// robohash is hash as bytes
			"CREATE TABLE IF NOT EXISTS maker_requests (
					robohash BLOB PRIMARY KEY,
					is_buy_order INTEGER,
					amount_sat INTEGER NOT NULL,
					bond_ratio INTEGER NOT NULL,
					offer_duration_ts INTEGER NOT NULL,
					bond_address TEXT NOT NULL,
					bond_amount_sat INTEGER NOT NULL
				)",
		)
		.execute(&db_pool)
		.await?;
		sqlx::query(
			// robohash is hash as bytes
			"CREATE TABLE IF NOT EXISTS active_maker_offers (
				offer_id TEXT PRIMARY KEY,
				robohash BLOB,
				is_buy_order INTEGER,
				amount_sat INTEGER NOT NULL,
				bond_ratio INTEGER NOT NULL,
				offer_duration_ts INTEGER NOT NULL,
				bond_address TEXT NOT NULL,
				bond_amount_sat INTEGER NOT NULL,
				bond_tx_hex TEXT NOT NULL,
				payout_address TEXT NOT NULL,
				musig_pub_nonce_hex TEXT NOT NULL,
				musig_pubkey_hex TEXT NOT NULL
			)",
		)
		.execute(&db_pool)
		.await?;
		dbg!("Database initialized");
		let shared_db_pool = Arc::new(db_pool);
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

	pub async fn fetch_maker_request(&self, robohash: &String) -> Result<BondRequirementResponse> {
		let maker_request = sqlx::query(
			"SELECT bond_address, bond_amount_sat FROM maker_requests WHERE robohash = ?",
		)
		.bind(hex::decode(robohash)?)
		.fetch_one(&*self.db_pool)
		.await?;

		Ok(BondRequirementResponse {
			bond_address: maker_request.try_get("bond_address")?,
			locking_amount_sat: maker_request.try_get::<i64, _>("bond_amount_sat")? as u64,
		})
	}

	pub async fn fetch_and_delete_offer_from_bond_table(
		&self,
		robohash_hex: &str,
	) -> Result<AwaitingBondOffer> {
		let fetched_values = sqlx::query_as::<_, (String, bool, i64, u8, i64, String, i64)> (
			"SELECT robohash, is_buy_order, amount_sat, bond_ratio, offer_duration_ts, bond_address, bond_amount_sat FROM maker_requests WHERE <unique_identifier_column> = ?",
		)
		.bind(hex::decode(robohash_hex)?)
		.fetch_one(&*self.db_pool)
		.await?;

		// Delete the database entry.
		sqlx::query("DELETE FROM maker_requests WHERE <unique_identifier_column> = ?")
			.bind(hex::decode(robohash_hex)?)
			.execute(&*self.db_pool)
			.await?;

		Ok(AwaitingBondOffer {
			robohash_hex: hex::encode(fetched_values.0),
			is_buy_order: fetched_values.1,
			amount_satoshi: fetched_values.2 as u64,
			bond_ratio: fetched_values.3,
			offer_duration_ts: fetched_values.4 as u64,
			bond_address: fetched_values.5,
			bond_amount_sat: fetched_values.6 as u64,
		})
	}

	pub async fn move_offer_to_active(
		&self,
		data: &BondSubmissionRequest,
		offer_id: &String,
	) -> Result<u64> {
		let remaining_offer_information = self
			.fetch_and_delete_offer_from_bond_table(&data.robohash_hex)
			.await?;

		sqlx::query(
			"INSERT OR REPLACE INTO active_maker_offers (offer_id, robohash, is_buy_order, amount_sat,
					bond_ratio, offer_duration_ts, bond_address, bond_amount_sat, bond_tx_hex, payout_address, musig_pub_nonce_hex, musig_pubkey_hex)
					VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
		)
		.bind(offer_id)
		.bind(hex::decode(&data.robohash_hex)?)
		.bind(remaining_offer_information.is_buy_order)
		.bind(remaining_offer_information.amount_satoshi as i64)
		.bind(remaining_offer_information.bond_ratio)
		.bind(remaining_offer_information.offer_duration_ts as i64)
		.bind(remaining_offer_information.bond_address.clone())
		.bind(remaining_offer_information.bond_amount_sat as i64)
		.bind(data.signed_bond_hex.clone())
		.bind(data.payout_address.clone())
		.bind(data.musig_pub_nonce_hex.clone())
		.bind(data.musig_pubkey_hex.clone())
		.execute(&*self.db_pool)
		.await?;

		Ok(remaining_offer_information.offer_duration_ts)
	}

	pub async fn fetch_suitable_offers(
		&self,
		requested_offer: &OffersRequest,
	) -> Result<Option<Vec<PublicOffer>>> {
		let fetched_offers = sqlx::query_as::<_, (String, i64)> (
            "SELECT offer_id, amount_sat FROM maker_requests WHERE is_buy_order = ? AND amount_sat BETWEEN ? AND ?",
        )
        .bind(requested_offer.buy_offers)
        .bind(requested_offer.amount_min_sat as i64)
        .bind(requested_offer.amount_max_sat as i64)
        .fetch_all(&*self.db_pool)
        .await?;

		let available_offers: Vec<PublicOffer> = fetched_offers
			.into_iter()
			.map(|(offer_id_hex, amount_sat)| PublicOffer {
				offer_id_hex,
				amount_sat: amount_sat as u64,
			})
			.collect();
		if available_offers.is_empty() {
			return Ok(None);
		}
		Ok(Some(available_offers))
	}
}

#[cfg(test)]
mod tests {
	use anyhow::Ok;

	use super::*;
	async fn create_coordinator() -> Result<database::CoordinatorDB, anyhow::Error> {
		// Set up the in-memory database
		env::set_var("DATABASE_PATH", ":memory:");

		// Initialize the database
		let database = CoordinatorDB::init().await?;
		Ok(database)
	}
	#[tokio::test]
	async fn test_init() -> Result<()> {
		let database = create_coordinator().await?;
		// Verify the table creation
		let table_exists = sqlx::query(
			"SELECT name FROM sqlite_master WHERE type='table' AND name='maker_requests'",
		)
		.fetch_optional(&*database.db_pool)
		.await?
		.is_some();
		assert!(table_exists, "The maker_requests table should exist.");
		Ok(())
	}

	#[tokio::test]
	async fn test_insert_new_maker_request() -> Result<()> {
		let database = create_coordinator().await?;

		// Create a sample order request and bond requirement response
		let order_request = OrderRequest {
			robohash_hex: "a3f1f1f0e2f3f4f5".to_string(),
			is_buy_order: true,
			amount_satoshi: 1000,
			bond_ratio: 50,
			offer_duration_ts: 1234567890,
		};

		let bond_requirement_response = BondRequirementResponse {
			bond_address: "1BitcoinAddress".to_string(),
			locking_amount_sat: 500,
		};

		// Insert the new maker request
		database
			.insert_new_maker_request(&order_request, &bond_requirement_response)
			.await?;

		// Verify the insertion
		let row = sqlx::query("SELECT * FROM maker_requests WHERE robohash = ?")
			.bind(hex::decode(&order_request.robohash_hex)?)
			.fetch_one(&*database.db_pool)
			.await?;

		assert!(row.get::<bool, _>("is_buy_order"));
		assert_eq!(row.get::<i64, _>("amount_sat"), 1000);
		assert_eq!(row.get::<i64, _>("bond_ratio"), 50);
		assert_eq!(row.get::<i64, _>("offer_duration_ts"), 1234567890);
		assert_eq!(row.get::<String, _>("bond_address"), "1BitcoinAddress");
		assert_eq!(row.get::<i64, _>("bond_amount_sat"), 500);

		Ok(())
	}
}
