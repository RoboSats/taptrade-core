pub mod db_tests;

use anyhow::Context;
use futures_util::StreamExt;

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

#[derive(PartialEq, Debug)]
struct AwaitingTakerOffer {
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

fn bool_to_sql_int(flag: bool) -> Option<i64> {
	if flag {
		Some(1)
	} else {
		None
	}
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

	pub async fn fetch_bond_requirements(&self, robohash: &String) -> Result<BondRequirements> {
		let maker_request = sqlx::query(
			"SELECT bond_address, bond_amount_sat, amount_sat FROM maker_requests WHERE robohash = ?",
		)
		.bind(hex::decode(robohash)?)
		.fetch_one(&*self.db_pool)
		.await?;

		Ok(BondRequirements {
			bond_address: maker_request.try_get("bond_address")?,
			locking_amount_sat: maker_request.try_get::<i64, _>("bond_amount_sat")? as u64,
			min_input_sum_sat: maker_request.try_get::<i64, _>("amount_sat")? as u64,
		})
	}

	async fn fetch_and_delete_offer_from_bond_table(
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
		let awaiting_bond_offer = AwaitingBondOffer {
			robohash_hex: hex::encode(fetched_values.0),
			is_buy_order: fetched_values.1,
			amount_satoshi: fetched_values.2 as u64,
			bond_ratio: fetched_values.3,
			offer_duration_ts: fetched_values.4 as u64,
			bond_address: fetched_values.5,
			bond_amount_sat: fetched_values.6 as u64,
		};
		debug!(
			"Deleted offer from maker_requests table. Fetched offer: {:#?}",
			awaiting_bond_offer
		);
		Ok(awaiting_bond_offer)
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

		debug!(
			"DATABASE: Moving maker offer to active trades table. Bond data: {:#?}",
			data
		);
		sqlx::query(
			"INSERT OR REPLACE INTO active_maker_offers (offer_id, robohash, is_buy_order, amount_sat,
					bond_ratio, offer_duration_ts, bond_address, bond_amount_sat, bond_tx_hex, payout_address, musig_pub_nonce_hex, musig_pubkey_hex, taker_bond_address)
					VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
		)
		.bind(offer_id)
		.bind(hex::decode(&data.robohash_hex)?)
		.bind(bool_to_sql_int(remaining_offer_information.is_buy_order))
		.bind(remaining_offer_information.amount_satoshi as i64)
		.bind(remaining_offer_information.bond_ratio as i32)
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

		debug!("\nDATABASE: moved offer to active trades\n");
		Ok(remaining_offer_information.offer_duration_ts)
	}

	pub async fn fetch_suitable_offers(
		&self,
		requested_offer: &OffersRequest,
	) -> Result<Option<Vec<PublicOffer>>> {
		debug!(
			"Fetching suitable offers from db. Specification: {:#?}",
			requested_offer
		);
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
			debug!("No available offers in db...");
			return Ok(None);
		}
		Ok(Some(available_offers))
	}

	pub async fn fetch_taker_bond_requirements(
		&self,
		offer_id_hex: &String,
	) -> Result<BondRequirements> {
		let taker_bond_requirements = sqlx::query(
			"SELECT taker_bond_address, bond_amount_sat, amount_sat FROM active_maker_offers WHERE offer_id = ?",
		)
		.bind(offer_id_hex)
		.fetch_one(&*self.db_pool)
		.await?;

		Ok(BondRequirements {
			bond_address: taker_bond_requirements.try_get("taker_bond_address")?,
			locking_amount_sat: taker_bond_requirements.try_get::<i64, _>("bond_amount_sat")?
				as u64,
			min_input_sum_sat: taker_bond_requirements.try_get::<i64, _>("amount_sat")? as u64,
		})
	}

	async fn fetch_and_delete_offer_from_public_offers_table(
		&self,
		offer_id_hex: &str,
	) -> Result<AwaitingTakerOffer> {
		let fetched_values = sqlx::query_as::<_, (Vec<u8>, i32, i64, i32, i64, String, i64, String, String, String, String)> (
			"SELECT robohash, is_buy_order, amount_sat, bond_ratio, offer_duration_ts, bond_address, bond_amount_sat, bond_tx_hex, payout_address,
			musig_pub_nonce_hex, musig_pubkey_hex FROM active_maker_offers WHERE offer_id = ?",
		)
		.bind(offer_id_hex)
		.fetch_one(&*self.db_pool)
		.await?;

		// Delete the database entry.
		sqlx::query("DELETE FROM active_maker_offers WHERE offer_id = ?")
			.bind(offer_id_hex)
			.execute(&*self.db_pool)
			.await?;

		Ok(AwaitingTakerOffer {
			offer_id: offer_id_hex.to_string(),
			robohash_maker: fetched_values.0,
			is_buy_order: fetched_values.1 != 0,
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
			.bind(bool_to_sql_int(public_offer.is_buy_order))
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

	// returns a hashmap of RoboHash, MonitoringBond for the monitoring loop
	// in case this gets a bottleneck (db too large for heap) we can implement in place checking
	pub async fn fetch_all_bonds(&self) -> Result<Vec<MonitoringBond>> {
		let mut bonds = Vec::new();
		let mut rows_orderbook = sqlx::query(
			"SELECT offer_id, robohash, bond_address, bond_amount_sat, amount_sat, bond_tx_hex FROM active_maker_offers",
		)
		.fetch(&*self.db_pool);
		while let Some(row) = rows_orderbook.next().await {
			let row = row?;

			let robohash: Vec<u8> = row.get("robohash");
			let requirements = BondRequirements {
				bond_address: row.get("bond_address"),
				locking_amount_sat: row.get::<i64, _>("bond_amount_sat") as u64,
				min_input_sum_sat: row.get::<i64, _>("amount_sat") as u64,
			};

			let bond = MonitoringBond {
				bond_tx_hex: row.get("bond_tx_hex"),
				robot: robohash,
				trade_id_hex: row.get("offer_id"),
				requirements,
				table: Table::Orderbook,
			};
			bonds.push(bond);
		}

		// we shouldn't need this as bonds will be locked onchain when trade is taken and we should
		// move to taken_offers only once everything is confirmed
		// let mut rows_taken = sqlx::query(
		// 	"SELECT offer_id, robohash_maker, robohash_taker,
		// 	bond_address_maker, bond_address_taker, bond_amount_sat, amount_sat, bond_tx_hex_maker, bond_tx_hex_taker
		// 	FROM taken_offers",
		// )
		// .fetch(&*self.db_pool);

		// while let Some(row) = rows_taken.next().await {
		// 	let row = row?;

		// 	let robohash_maker: Vec<u8> = row.get("robohash_maker");
		// 	let robohash_taker: Vec<u8> = row.get("robohash_taker");
		// 	let locking_amount_sat = row.get::<i64, _>("bond_amount_sat") as u64;
		// 	let min_input_sum_sat = row.get::<i64, _>("amount_sat") as u64;
		// 	let trade_id_hex: String = row.get("offer_id");

		// 	let requirements_maker = BondRequirements {
		// 		bond_address: row.get("bond_address_maker"),
		// 		locking_amount_sat,
		// 		min_input_sum_sat,
		// 	};

		// 	let bond_maker = MonitoringBond {
		// 		bond_tx_hex: row.get("bond_tx_hex_maker"),
		// 		robot: robohash_maker,
		// 		trade_id_hex: trade_id_hex.clone(),
		// 		requirements: requirements_maker,
		// 		table: Table::ActiveTrades,
		// 	};
		// 	bonds.push(bond_maker);

		// 	let requirements_maker = BondRequirements {
		// 		bond_address: row.get("bond_address_taker"),
		// 		locking_amount_sat,
		// 		min_input_sum_sat,
		// 	};

		// 	let bond_taker = MonitoringBond {
		// 		bond_tx_hex: row.get("bond_tx_hex_taker"),
		// 		trade_id_hex,
		// 		robot: robohash_taker,
		// 		requirements: requirements_maker,
		// 		table: Table::ActiveTrades,
		// 	};
		// 	bonds.push(bond_taker);
		// }
		Ok(bonds)
	}

	pub async fn remove_violating_bond(&self, bond: &MonitoringBond) -> Result<()> {
		if bond.table == Table::Orderbook {
			sqlx::query("DELETE FROM active_maker_offers WHERE offer_id = ?")
				.bind(&bond.trade_id_hex)
				.execute(&*self.db_pool)
				.await?;
			debug!("Removed violating bond offer from orderbook");
		} else {
			return Err(anyhow!(
				"Invalid table type when trying to remove violating bond from db"
			));
		}

		// we shouldn't need this as bonds will be locked onchain when trade is taken and we should
		// move to taken_offers only once everything is confirmed
		// } else if bond.table == Table::ActiveTrades {
		// 	sqlx::query("DELETE FROM taken_offers WHERE offer_id = ?")
		// 		.bind(bond.trade_id_hex)
		// 		.execute(&*self.db_pool)
		// 		.await?;

		// sqlx::query("DELETE FROM active_maker_offers WHERE offer_id = ?")
		// 	.bind(trade_id_hex)
		// 	.execute(&*self.db_pool)
		// 	.await?;
		Ok(())
	}
}
