use anyhow::Context;

use super::*;
use sqlx::{sqlite::SqlitePoolOptions, Pool, Row, Sqlite};
use std::env;

#[derive(Clone, Debug)]
pub struct CoordinatorDB {
	pub db_pool: Arc<Pool<Sqlite>>,
}

// db structure of offers awaiting bond submission in table maker_requests
#[derive(PartialEq, Debug)]
struct AwaitingBondOffer {
	robohash_hex: String,
	is_buy_order: bool,
	amount_satoshi: u64,
	bond_ratio: u8,
	offer_duration_ts: u64,
	bond_address: String,
	bond_amount_sat: u64,
}

struct AwaitinigTakerOffer {
	offer_id: String,
	robohash_maker: Vec<u8>,
	is_buy_order: bool,
	amount_sat: i64,
	bond_ratio: i32,
	offer_duration_ts: i64,
	bond_address_maker: String,
	bond_amount_sat: i64,
	bond_tx_hex_maker: String,
	payout_address_maker: String,
	musig_pub_nonce_hex_maker: String,
	musig_pubkey_hex_maker: String,
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
				musig_pubkey_hex TEXT NOT NULL,
				taker_bond_address TEXT
			)",
		)
		.execute(&db_pool)
		.await?;

		sqlx::query(
			"CREATE TABLE IF NOT EXISTS taken_offers (
				offer_id TEXT PRIMARY KEY,
				robohash_maker BLOB,
				robohash_taker BLOB,
				is_buy_order INTEGER,
				amount_sat INTEGER NOT NULL,
				bond_ratio INTEGER NOT NULL,
				offer_duration_ts INTEGER NOT NULL,
				bond_address_maker TEXT NOT NULL,
				bond_address_taker TEXT NOT NULL,
				bond_amount_sat INTEGER NOT NULL,
				bond_tx_hex_maker TEXT NOT NULL,
				bond_tx_hex_taker TEXT NOT NULL,
				payout_address_maker TEXT NOT NULL,
				payout_address_taker TEXT NOT NULL,
				musig_pub_nonce_hex_maker TEXT NOT NULL,
				musig_pubkey_hex_maker TEXT NOT NULL,
				musig_pub_nonce_hex_taker TEXT NOT NULL,
				musig_pubkey_hex_taker TEXT NOT NULL,
				escrow_psbt_hex_maker TEXT,
				escrow_psbt_hex_taker TEXT
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
		let fetched_values = sqlx::query_as::<_, (Vec<u8>, bool, i64, u8, i64, String, i64)> (
			"SELECT robohash, is_buy_order, amount_sat, bond_ratio, offer_duration_ts, bond_address, bond_amount_sat FROM maker_requests WHERE robohash = ?",
		)
		.bind(hex::decode(robohash_hex)?)
		.fetch_one(&*self.db_pool)
		.await?;

		// Delete the database entry.
		sqlx::query("DELETE FROM maker_requests WHERE robohash = ?")
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
		taker_bond_address: String,
	) -> Result<u64> {
		let remaining_offer_information = self
			.fetch_and_delete_offer_from_bond_table(&data.robohash_hex)
			.await?;

		sqlx::query(
			"INSERT OR REPLACE INTO active_maker_offers (offer_id, robohash, is_buy_order, amount_sat,
					bond_ratio, offer_duration_ts, bond_address, bond_amount_sat, bond_tx_hex, payout_address, musig_pub_nonce_hex, musig_pubkey_hex, taker_bond_address)
					VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
		.bind(taker_bond_address)
		.execute(&*self.db_pool)
		.await?;

		Ok(remaining_offer_information.offer_duration_ts)
	}

	pub async fn fetch_suitable_offers(
		&self,
		requested_offer: &OffersRequest,
	) -> Result<Option<Vec<PublicOffer>>> {
		let fetched_offers = sqlx::query_as::<_, (String, i64, i64, String)> (
            "SELECT offer_id, amount_sat, bond_amount_sat, taker_bond_address FROM active_maker_offers WHERE is_buy_order = ? AND amount_sat BETWEEN ? AND ?",
        )
        .bind(requested_offer.buy_offers)
        .bind(requested_offer.amount_min_sat as i64)
        .bind(requested_offer.amount_max_sat as i64)
        .fetch_all(&*self.db_pool)
        .await?;

		let available_offers: Vec<PublicOffer> = fetched_offers
			.into_iter()
			.map(
				|(offer_id_hex, amount_sat, bond_amount_sat, bond_address_taker)| PublicOffer {
					offer_id_hex,
					amount_sat: amount_sat as u64,
					required_bond_amount_sat: bond_amount_sat as u64,
					bond_locking_address: bond_address_taker,
				},
			)
			.collect();
		if available_offers.is_empty() {
			println!("empty");
			return Ok(None);
		}
		Ok(Some(available_offers))
	}

	pub async fn fetch_taker_bond_requirements(
		&self,
		offer_id_hex: &String,
	) -> Result<BondRequirementResponse> {
		let taker_bond_requirements = sqlx::query(
			"SELECT taker_bond_address, bond_amount_sat FROM active_maker_offers WHERE offer_id = ?",
		)
		.bind(offer_id_hex)
		.fetch_one(&*self.db_pool)
		.await?;

		Ok(BondRequirementResponse {
			bond_address: taker_bond_requirements.try_get("taker_bond_address")?,
			locking_amount_sat: taker_bond_requirements.try_get::<i64, _>("bond_amount_sat")?
				as u64,
		})
	}

	pub async fn fetch_and_delete_offer_from_public_offers_table(
		&self,
		offer_id_hex: &str,
	) -> Result<AwaitinigTakerOffer> {
		let fetched_values = sqlx::query_as::<_, (Vec<u8>, bool, i64, i32, i64, String, i64, String, String, String, String)> (
			"SELECT robohash, is_buy_order, amount_sat, bond_ratio, offer_duration_ts, bond_address, bond_amount_sat, bond_tx_hex, payout_address,
			musig_pub_nonce_hex, musig_pubkey_hex FROM active_maker_offers WHERE <unique_identifier_column> = ?",
		)
		.bind(offer_id_hex)
		.fetch_one(&*self.db_pool)
		.await?;

		// Delete the database entry.
		sqlx::query("DELETE FROM active_maker_offers WHERE <unique_identifier_column> = ?")
			.bind(offer_id_hex)
			.execute(&*self.db_pool)
			.await?;

		Ok(AwaitinigTakerOffer {
			offer_id: offer_id_hex.to_string(),
			robohash_maker: fetched_values.0,
			is_buy_order: fetched_values.1,
			amount_sat: fetched_values.2,
			bond_ratio: fetched_values.3,
			offer_duration_ts: fetched_values.4,
			bond_address_maker: fetched_values.5,
			bond_amount_sat: fetched_values.6,
			bond_tx_hex_maker: fetched_values.7,
			payout_address_maker: fetched_values.8,
			musig_pub_nonce_hex_maker: fetched_values.9,
			musig_pubkey_hex_maker: fetched_values.10,
		})
	}

	pub async fn add_taker_info_and_move_table(
		&self,
		trade_and_taker_info: &OfferPsbtRequest,
		trade_contract_psbt_maker: &String,
		trade_contract_psbt_taker: &String,
	) -> Result<()> {
		let public_offer = self
			.fetch_and_delete_offer_from_public_offers_table(
				&trade_and_taker_info.offer.offer_id_hex,
			)
			.await?;

		sqlx::query(
				"INSERT OR REPLACE INTO taken_offers (offer_id, robohash_maker, robohash_taker, is_buy_order, amount_sat,
						bond_ratio, offer_duration_ts, bond_address_maker, bond_address_taker, bond_amount_sat, bond_tx_hex_maker,
						bond_tx_hex_taker, payout_address_maker, payout_address_taker, musig_pub_nonce_hex_maker, musig_pubkey_hex_maker
						musig_pub_nonce_hex_taker, musig_pubkey_hex_taker, escrow_psbt_hex_maker, escrow_psbt_hex_taker)
						VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
			)
			.bind(public_offer.offer_id)
			.bind(public_offer.robohash_maker)
			.bind(hex::decode(&trade_and_taker_info.trade_data.robohash_hex)?)
			.bind(public_offer.is_buy_order)
			.bind(public_offer.amount_sat)
			.bind(public_offer.bond_ratio)
			.bind(public_offer.offer_duration_ts)
			.bind(public_offer.bond_address_maker)
			.bind(trade_and_taker_info.offer.bond_locking_address.clone())
			.bind(public_offer.bond_amount_sat)
			.bind(public_offer.bond_tx_hex_maker)
			.bind(trade_and_taker_info.trade_data.signed_bond_hex.clone())
			.bind(public_offer.payout_address_maker)
			.bind(trade_and_taker_info.trade_data.payout_address.clone())
			.bind(public_offer.musig_pub_nonce_hex_maker)
			.bind(public_offer.musig_pubkey_hex_maker)
			.bind(trade_and_taker_info.trade_data.musig_pub_nonce_hex.clone())
			.bind(trade_and_taker_info.trade_data.musig_pubkey_hex.clone())
			.bind(trade_contract_psbt_maker.clone())
			.bind(trade_contract_psbt_taker.clone())
			.execute(&*self.db_pool)
			.await?;

		Ok(())
	}

	pub async fn fetch_taken_offer_maker(
		&self,
		offer_id_hex: &String,
		robohash_hex_maker: &String,
	) -> Result<Option<String>> {
		let offer = sqlx::query(
			"SELECT escrow_psbt_hex_maker, robohash_maker FROM taken_offers WHERE offer_id = ?",
		)
		.bind(offer_id_hex)
		.fetch_optional(&*self.db_pool)
		.await?;
		let offer = match offer {
			Some(offer) => offer,
			None => return Ok(None),
		};
		match offer.try_get::<Vec<u8>, _>("robohash_maker") {
			Ok(robohash) => {
				if hex::encode(robohash) == *robohash_hex_maker {
					Ok(offer.try_get("escrow_psbt_hex_maker")?)
				} else {
					Ok(None)
				}
			}
			Err(_) => Ok(None),
		}
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
	
	#[tokio::test]
    async fn test_fetch_maker_request() -> Result<()> {
		let database = create_coordinator().await?;

		// Create a sample order request and insert it into the database
		let robohash_hex = "a3f1f1f0e2f3f4f5";
		let order_request = (
			hex::decode(robohash_hex).unwrap(),
			true, // is_buy_order
			1000, // amount_satoshi
			50, // bond_ratio
			1234567890, // offer_duration_ts
			"1BitcoinAddress".to_string(), // bond_address
			500, // bond_amount_sat
		);

		sqlx::query(
			"INSERT INTO maker_requests (robohash, is_buy_order, amount_sat, bond_ratio, offer_duration_ts, bond_address, bond_amount_sat)
			VALUES (?, ?, ?, ?, ?, ?, ?)",
		)
		.bind(order_request.0.clone())
		.bind(order_request.1)
		.bind(order_request.2)
		.bind(order_request.3)
		.bind(order_request.4)
		.bind(order_request.5.clone())
		.bind(order_request.6)
		.execute(&*database.db_pool)
		.await?;

		// Fetch and delete the order request
		let fetched_offer = database.fetch_maker_request(&robohash_hex.to_string()).await?;

        // Verify the result
        let expected = BondRequirementResponse {
            bond_address: "1BitcoinAddress".to_string(),
            locking_amount_sat: 500_u64,
        };
        assert_eq!(fetched_offer, expected);

        Ok(())
    }
	
	#[tokio::test]
	async fn test_fetch_and_delete_offer_from_bond_table() -> Result<()> {
    // Set up the in-memory database
		let database = create_coordinator().await?;

		// Create a sample order request and insert it into the database
		let robohash_hex = "a3f1f1f0e2f3f4f5";
		let order_request = (
			hex::decode(robohash_hex).unwrap(),
			true, // is_buy_order
			1000, // amount_satoshi
			50, // bond_ratio
			1234567890, // offer_duration_ts
			"1BitcoinAddress".to_string(), // bond_address
			500, // bond_amount_sat
		);

		sqlx::query(
			"INSERT INTO maker_requests (robohash, is_buy_order, amount_sat, bond_ratio, offer_duration_ts, bond_address, bond_amount_sat)
			VALUES (?, ?, ?, ?, ?, ?, ?)",
		)
		.bind(order_request.0.clone())
		.bind(order_request.1)
		.bind(order_request.2)
		.bind(order_request.3)
		.bind(order_request.4)
		.bind(order_request.5.clone())
		.bind(order_request.6)
		.execute(&*database.db_pool)
		.await?;

		// Fetch and delete the order request
		let fetched_offer = database.fetch_and_delete_offer_from_bond_table(robohash_hex).await?;

		// Verify the fetched offer
		let expected_offer = AwaitingBondOffer {
			robohash_hex: robohash_hex.to_string(),
			is_buy_order: order_request.1,
			amount_satoshi: order_request.2 as u64,
			bond_ratio: order_request.3,
			offer_duration_ts: order_request.4 as u64,
			bond_address: order_request.5,
			bond_amount_sat: order_request.6 as u64,
		};
		assert_eq!(fetched_offer, expected_offer);

		// Verify the record is deleted
		let result = sqlx::query("SELECT * FROM maker_requests WHERE robohash = ?")
			.bind(hex::decode(robohash_hex)?)
			.fetch_optional(&*database.db_pool)
			.await?;
		assert!(result.is_none());

		Ok(())
	}

	#[tokio::test]
    async fn test_move_offer_to_active() -> Result<()> {
        // Create a temporary SQLite database
        let database = create_coordinator().await?;

        
        // Insert a test entry into maker_requests
        let robohash_hex = "a3f1f1f0e2f3f4f5";
        let order_request = (
            hex::decode(robohash_hex).unwrap(),
            true, // is_buy_order
            1000, // amount_satoshi
            50, // bond_ratio
            1234567890, // offer_duration_ts
            "1BitcoinAddress".to_string(), // bond_address
            500, // bond_amount_sat
        );

        sqlx::query(
            "INSERT INTO maker_requests (robohash, is_buy_order, amount_sat, bond_ratio, offer_duration_ts, bond_address, bond_amount_sat)
            VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(order_request.0.clone())
        .bind(order_request.1)
        .bind(order_request.2)
        .bind(order_request.3)
        .bind(order_request.4)
        .bind(order_request.5.clone())
        .bind(order_request.6)
        .execute(&*database.db_pool)
        .await?;

        // Create a sample BondSubmissionRequest
        let bond_submission_request = BondSubmissionRequest {
            robohash_hex: robohash_hex.to_string(),
            signed_bond_hex: "signedBondHex".to_string(),
            payout_address: "1PayoutAddress".to_string(),
            musig_pub_nonce_hex: "musigPubNonceHex".to_string(),
            musig_pubkey_hex: "musigPubkeyHex".to_string(),
        };

        // Call the move_offer_to_active function
        let offer_id = "sample_offer_id".to_string();
        let taker_bond_address = "1TakerBondAddress".to_string();
        let result = database
            .move_offer_to_active(&bond_submission_request, &offer_id, taker_bond_address)
            .await?;

        // Verify the result
        assert_eq!(result, 1234567890); // Verify that the offer_duration_ts is correct

        // Verify that the entry was moved to active_maker_offers
        let active_offer = sqlx::query_as::<_, (String, Vec<u8>, bool, i64, i64, i64, String, i64, String, String, String, String)> (
            "SELECT offer_id, robohash, is_buy_order, amount_sat, bond_ratio, offer_duration_ts, bond_address, bond_amount_sat, bond_tx_hex, payout_address, musig_pub_nonce_hex, musig_pubkey_hex 
             FROM active_maker_offers WHERE offer_id = ?",
        )
        .bind(offer_id)
        .fetch_one(&*database.db_pool)
        .await?;

        assert_eq!(active_offer.0, "sample_offer_id".to_string());
        assert_eq!(hex::encode(active_offer.1), robohash_hex);
        assert!(active_offer.2);
        assert_eq!(active_offer.3, 1000);
        assert_eq!(active_offer.4, 50);
        assert_eq!(active_offer.5, 1234567890);
        assert_eq!(active_offer.6, "1BitcoinAddress".to_string());
        assert_eq!(active_offer.7, 500);
        assert_eq!(active_offer.8, "signedBondHex".to_string());
        assert_eq!(active_offer.9, "1PayoutAddress".to_string());
        assert_eq!(active_offer.10, "musigPubNonceHex".to_string());
        assert_eq!(active_offer.11, "musigPubkeyHex".to_string());

        Ok(())
    }

	#[tokio::test]
    async fn test_fetch_suitable_offers() -> Result<()> {
        let database = create_coordinator().await?;
        // Insert test entries into active_maker_offers
        let offers = vec![
            (
                "offer_id_1",
                true, // is_buy_order
                15000, // amount_sat
                100, // bond_ratio
                1234567890, // offer_duration_ts
                "1BondAddress".to_string(), // bond_address
                50, // bond_amount_sat
                "signedBondHex".to_string(),
                "1PayoutAddress".to_string(),
                "musigPubNonceHex".to_string(),
                "musigPubkeyHex".to_string(),
				"1TakerBondAddress".to_string(),
            ),
            (
                "offer_id_2",
                true, // is_buy_order
                1500, // amount_sat
                200, // bond_ratio
                1234567891, // offer_duration_ts
                "2BondAddress".to_string(), // bond_address
                100, // bond_amount_sat
                "signedBondHex2".to_string(),
                "2PayoutAddress".to_string(),
                "musigPubNonceHex2".to_string(),
                "musigPubkeyHex2".to_string(),
				"2TakerBondAddress".to_string(),
            ),
        ];

        for offer in offers {
            sqlx::query(
                "INSERT INTO active_maker_offers (offer_id, robohash, is_buy_order, amount_sat, bond_ratio, offer_duration_ts, bond_address, bond_amount_sat, bond_tx_hex, payout_address, musig_pub_nonce_hex, musig_pubkey_hex, taker_bond_address)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(offer.0)
            .bind(hex::decode("a3f1f1f0e2f3f4f5").unwrap()) // Example robohash
            .bind(offer.1)
            .bind(offer.2)
            .bind(offer.3)
            .bind(offer.4)
            .bind(offer.5.clone())
            .bind(offer.6)
            .bind(offer.7.clone())
            .bind(offer.8.clone())
            .bind(offer.9.clone())
            .bind(offer.10.clone())
            .bind(offer.11.clone())
            .execute(&*database.db_pool)
            .await?;
        }


        // Create a sample OffersRequest
        let offers_request = OffersRequest {
            buy_offers: true,
            amount_min_sat: 1000,
            amount_max_sat: 2000,
        };

        // Call the fetch_suitable_offers function
        let result = database.fetch_suitable_offers(&offers_request).await?;

		println!("{:?}", result);
        // Verify the result
        assert!(result.is_some());
        let available_offers = result.unwrap();
        assert_eq!(available_offers.len(), 1);
        let offer = &available_offers[0];
        assert_eq!(offer.offer_id_hex, "offer_id_2");
        assert_eq!(offer.amount_sat, 1500);
        assert_eq!(offer.required_bond_amount_sat, 100);
        assert_eq!(offer.bond_locking_address, "2TakerBondAddress");

        Ok(())
    }

	#[tokio::test]
    async fn test_fetch_taker_bond_requirements() -> Result<()> {
        let database = create_coordinator().await?;

        // Insert a test entry into active_maker_offers
        let offer_id_hex = "offer_id_1";
        let taker_bond_address = "1TakerBondAddress";
        let bond_amount_sat = 100;

        sqlx::query(
            "INSERT INTO active_maker_offers (offer_id, robohash, is_buy_order, amount_sat, bond_ratio, offer_duration_ts, bond_address, bond_amount_sat, bond_tx_hex, payout_address, musig_pub_nonce_hex, musig_pubkey_hex, taker_bond_address)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(offer_id_hex)
        .bind(hex::decode("a3f1f1f0e2f3f4f5").unwrap()) // Example robohash
        .bind(true) // is_buy_order
        .bind(1500) // amount_sat
        .bind(50) // bond_ratio
        .bind(1234567890) // offer_duration_ts
        .bind("1BondAddress")
        .bind(bond_amount_sat)
        .bind("signedBondHex")
        .bind("1PayoutAddress")
        .bind("musigPubNonceHex")
        .bind("musigPubkeyHex")
        .bind(taker_bond_address)
        .execute(&*database.db_pool)
        .await?;

        // Call the fetch_taker_bond_requirements function
        let result = database.fetch_taker_bond_requirements(&offer_id_hex.to_string()).await?;

        // Verify the result
        assert_eq!(result.bond_address, taker_bond_address);
        assert_eq!(result.locking_amount_sat, bond_amount_sat as u64);

        Ok(())
    }

	 #[tokio::test]
    async fn test_fetch_and_delete_offer_from_public_offers_table() -> Result<()> {
        let database = create_coordinator().await?;

        // Insert a test entry into active_maker_offers
        let offer_id_hex = "offer_id_1";
        let robohash = hex::decode("a3f1f1f0e2f3f4f5").unwrap(); // Example robohash
        let is_buy_order = true;
        let amount_sat = 1000;
        let bond_ratio = 50;
        let offer_duration_ts = 1234567890;
        let bond_address = "1BondAddress".to_string();
        let bond_amount_sat = 500;
        let bond_tx_hex = "signedBondHex".to_string();
        let payout_address = "1PayoutAddress".to_string();
        let musig_pub_nonce_hex = "musigPubNonceHex".to_string();
        let musig_pubkey_hex = "musigPubkeyHex".to_string();

        sqlx::query(
            "INSERT INTO active_maker_offers (offer_id, robohash, is_buy_order, amount_sat, bond_ratio, offer_duration_ts, bond_address, bond_amount_sat, bond_tx_hex, payout_address, musig_pub_nonce_hex, musig_pubkey_hex)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(offer_id_hex)
        .bind(robohash.clone())
        .bind(is_buy_order)
        .bind(amount_sat)
        .bind(bond_ratio)
        .bind(offer_duration_ts)
        .bind(bond_address.clone())
        .bind(bond_amount_sat)
        .bind(bond_tx_hex.clone())
        .bind(payout_address.clone())
        .bind(musig_pub_nonce_hex.clone())
        .bind(musig_pubkey_hex.clone())
        .execute(&*database.db_pool)
        .await?;

        // Call the fetch_and_delete_offer_from_public_offers_table function
        let result = database.fetch_and_delete_offer_from_public_offers_table(offer_id_hex).await?;

        // Verify the result
        assert_eq!(result.offer_id, offer_id_hex);
        assert_eq!(result.robohash_maker, robohash);
        assert_eq!(result.is_buy_order, is_buy_order);
        assert_eq!(result.amount_sat, amount_sat);
        assert_eq!(result.bond_ratio, bond_ratio);
        assert_eq!(result.offer_duration_ts, offer_duration_ts);
        assert_eq!(result.bond_address_maker, bond_address);
        assert_eq!(result.bond_amount_sat, bond_amount_sat);
        assert_eq!(result.bond_tx_hex_maker, bond_tx_hex);
        assert_eq!(result.payout_address_maker, payout_address);
        assert_eq!(result.musig_pub_nonce_hex_maker, musig_pub_nonce_hex);
        assert_eq!(result.musig_pubkey_hex_maker, musig_pubkey_hex);

        // Verify the deletion
        let remaining_offers = sqlx::query("SELECT COUNT(*) FROM active_maker_offers WHERE offer_id = ?")
            .bind(offer_id_hex)
            .fetch_one(&*database.db_pool)
            .await?;

        let remaining_offers_count: i64 = remaining_offers.try_get(0)?;
        assert_eq!(remaining_offers_count, 0);

        Ok(())
    }

}
