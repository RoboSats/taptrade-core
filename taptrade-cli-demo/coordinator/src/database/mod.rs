#[cfg(test)]
mod db_tests;

use anyhow::Context;
use futures_util::StreamExt;

use super::*;
use bdk::bitcoin::address::Address;
use sqlx::{sqlite::SqlitePoolOptions, Pool, Row, Sqlite};
use std::env;
use std::str::FromStr;

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
	taproot_pubkey_hex_maker: String,
	musig_pub_nonce_hex_maker: String,
	musig_pubkey_hex_maker: String,
}

pub struct TraderHappiness {
	pub maker_happy: Option<bool>,
	pub taker_happy: Option<bool>,
	pub escrow_ongoing: bool,
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
		debug!("coordinator db path: {}", env::var("DATABASE_PATH")?);
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
				change_address_maker TEXT NOT NULL,
				escrow_inputs_hex_maker_csv TEXT NOT NULL,
				taproot_pubkey_hex_maker TEXT NOT NULL,
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
				taproot_xonly_pubkey_hex_maker TEXT NOT NULL,
				payout_address_taker TEXT NOT NULL,
				taproot_xonly_pubkey_hex_taker TEXT NOT NULL,
				musig_pub_nonce_hex_maker TEXT NOT NULL,
				musig_pubkey_compressed_hex_maker TEXT NOT NULL,
				musig_pub_nonce_hex_taker TEXT NOT NULL,
				musig_pubkey_compressed_hex_taker TEXT NOT NULL,
				escrow_psbt_hex TEXT,
				escrow_psbt_txid TEXT,
				escrow_psbt_is_confirmed INTEGER,
				maker_happy INTEGER,
				taker_happy INTEGER,
				escrow_ongoing INTEGER NOT NULL,
				escrow_winner_robohash TEXT,
				escrow_taproot_pk_coordinator TEXT,
				escrow_amount_maker_sat INTEGER,
				escrow_amount_taker_sat INTEGER,
				escrow_fee_per_participant INTEGER,
				escrow_output_descriptor TEXT
			)", // escrow_psbt_is_confirmed will be set 1 once the escrow psbt is confirmed onchain
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
		order: &OfferRequest,
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
		offer_id: &str,
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
					bond_ratio, offer_duration_ts, bond_address, bond_amount_sat, bond_tx_hex, payout_address, taproot_pubkey_hex_maker, musig_pub_nonce_hex, musig_pubkey_hex, taker_bond_address,
					change_address_maker, escrow_inputs_hex_maker_csv)
					VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
		.bind(data.taproot_pubkey_hex.clone())
		.bind(data.musig_pub_nonce_hex.clone())
		.bind(data.musig_pubkey_hex.clone())
		.bind(taker_bond_address)
		.bind(data.client_change_address.clone())
		.bind(data.bdk_psbt_inputs_hex_csv.clone())
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
		offer_id_hex: &str,
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
		let fetched_values = sqlx::query_as::<_, (Vec<u8>, i32, i64, i32, i64, String, i64, String, String, String, String, String)> (
			"SELECT robohash, is_buy_order, amount_sat, bond_ratio, offer_duration_ts, bond_address, bond_amount_sat, bond_tx_hex, payout_address, taproot_pubkey_hex_maker,
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
			taproot_pubkey_hex_maker: fetched_values.9,
			musig_pub_nonce_hex_maker: fetched_values.10,
			musig_pubkey_hex_maker: fetched_values.11,
		})
	}

	pub async fn add_taker_info_and_move_table(
		&self,
		trade_and_taker_info: &OfferPsbtRequest,
		escrow_tx_data: &EscrowPsbt,
	) -> Result<()> {
		let public_offer = self
			.fetch_and_delete_offer_from_public_offers_table(
				&trade_and_taker_info.offer.offer_id_hex,
			)
			.await?;

		sqlx::query(
				"INSERT OR REPLACE INTO taken_offers (offer_id, robohash_maker, robohash_taker, is_buy_order, amount_sat,
						bond_ratio, offer_duration_ts, bond_address_maker, bond_address_taker, bond_amount_sat, bond_tx_hex_maker,
						bond_tx_hex_taker, payout_address_maker, payout_address_taker, taproot_pubkey_hex_maker, taproot_pubkey_hex_taker, musig_pub_nonce_hex_maker, musig_pubkey_hex_maker,
						musig_pub_nonce_hex_taker, musig_pubkey_hex_taker, escrow_psbt_hex, escrow_output_descriptor, escrow_tx_fee_address, escrow_psbt_is_confirmed, escrow_ongoing,
						escrow_taproot_pk_coordinator, escrow_amount_maker_sat, escrow_amount_taker_sat, escrow_fee_per_participant)
						VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
			.bind(public_offer.taproot_pubkey_hex_maker)
			.bind(trade_and_taker_info.trade_data.taproot_pubkey_hex.clone())
			.bind(public_offer.musig_pub_nonce_hex_maker)
			.bind(public_offer.musig_pubkey_hex_maker)
			.bind(trade_and_taker_info.trade_data.musig_pub_nonce_hex.clone())
			.bind(trade_and_taker_info.trade_data.musig_pubkey_hex.clone())
			.bind(&escrow_tx_data.escrow_output_descriptor)
			.bind(&escrow_tx_data.escrow_psbt_hex)
			.bind(&escrow_tx_data.escrow_tx_fee_address)
			.bind(0)
			.bind(0)
			.bind(&escrow_tx_data.coordinator_xonly_escrow_pk)
			.bind(escrow_tx_data.escrow_amount_maker_sat as i64)
			.bind(escrow_tx_data.escrow_amount_taker_sat as i64)
			.bind(escrow_tx_data.escrow_fee_sat_per_participant as i64)
			.execute(&*self.db_pool)
			.await?;

		Ok(())
	}

	pub async fn fetch_escrow_output_information(
		&self,
		offer_id_hex: &str,
	) -> Result<Option<EscrowPsbt>> {
		let offer = sqlx::query(
			"SELECT escrow_output_descriptor, escrow_tx_fee_address, escrow_amount_maker_sat, 
			escrow_amount_taker_sat, escrow_fee_per_participant, escrow_taproot_pk_coordinator 
			FROM taken_offers WHERE offer_id = ?",
		)
		.bind(offer_id_hex)
		.fetch_optional(&*self.db_pool)
		.await?;
		let offer = match offer {
			Some(offer) => offer,
			None => return Ok(None),
		};
		let escrow_output_descriptor = offer.try_get::<String, _>("escrow_output_descriptor")?;
		let escrow_tx_fee_address = offer.try_get::<String, _>("escrow_tx_fee_address")?;
		let escrow_amount_maker_sat = offer.try_get::<i64, _>("escrow_amount_maker_sat")? as u64;
		let escrow_amount_taker_sat = offer.try_get::<i64, _>("escrow_amount_taker_sat")? as u64;
		let escrow_fee_sat_per_participant =
			offer.try_get::<i64, _>("escrow_fee_per_participant")? as u64;
		let coordinator_xonly_escrow_pk =
			offer.try_get::<String, _>("escrow_taproot_pk_coordinator")?;
		let escrow_psbt_hex = offer.try_get::<String, _>("escrow_psbt_hex")?;

		Ok(Some(EscrowPsbt {
			escrow_psbt_hex,
			escrow_output_descriptor,
			escrow_tx_fee_address,
			coordinator_xonly_escrow_pk,
			escrow_amount_maker_sat,
			escrow_amount_taker_sat,
			escrow_fee_sat_per_participant,
		}))
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

	pub async fn fetch_unconfirmed_bond_txids(&self) -> Result<Vec<String>> {
		let mut txids = Vec::new();
		let mut rows = sqlx::query(
			"SELECT escrow_psbt_txid FROM taken_offers WHERE escrow_psbt_is_confirmed = 0",
		)
		.fetch(&*self.db_pool);
		while let Some(row) = rows.next().await {
			let row = row?;
			let txid: String = row.get("escrow_psbt_txid");
			txids.push(txid);
		}
		Ok(txids)
	}

	pub async fn confirm_bond_txids(&self, confirmed_txids: Vec<String>) -> Result<()> {
		for txid in confirmed_txids {
			sqlx::query(
				"UPDATE taken_offers SET escrow_psbt_is_confirmed = 1 WHERE escrow_psbt_txid = ?",
			)
			.bind(txid)
			.execute(&*self.db_pool)
			.await?;
		}
		Ok(())
	}

	pub async fn get_txid_confirmation_status(&self, txid: &String) -> Result<bool> {
		let status = sqlx::query(
			"SELECT escrow_psbt_is_confirmed FROM taken_offers WHERE escrow_psbt_txid = ?",
		)
		.bind(txid)
		.fetch_one(&*self.db_pool)
		.await?;
		Ok(status.get::<i64, _>("escrow_psbt_is_confirmed") == 1)
	}

	pub async fn is_valid_robohash_in_table(
		&self,
		robohash_hex: &str,
		offer_id: &str,
	) -> Result<bool> {
		let robohash = hex::decode(robohash_hex)?;
		let robohash = sqlx::query(
			"SELECT 1 FROM taken_offers WHERE (robohash_maker = ? OR robohash_taker = ?) AND offer_id = ?",
		)
		.bind(&robohash)
		.bind(&robohash)
		.bind(offer_id)
		.fetch_optional(&*self.db_pool)
		.await?;
		Ok(robohash.is_some())
	}

	pub async fn fetch_escrow_tx_confirmation_status(&self, offer_id: &str) -> Result<bool> {
		let status =
			sqlx::query("SELECT escrow_psbt_is_confirmed FROM taken_offers WHERE offer_id = ?")
				.bind(offer_id)
				.fetch_one(&*self.db_pool)
				.await?;
		Ok(status.get::<i64, _>("escrow_psbt_is_confirmed") == 1)
	}

	pub async fn set_trader_happy_field(
		&self,
		offer_id: &str,
		robohash: &str,
		is_happy: bool,
	) -> Result<()> {
		let robohash_bytes = hex::decode(robohash)?;

		// First, check if the robohash matches the maker or taker
		let row = sqlx::query(
			"SELECT robohash_maker, robohash_taker FROM taken_offers WHERE offer_id = ?",
		)
		.bind(offer_id)
		.fetch_one(&*self.db_pool)
		.await?;

		let is_maker = row.get::<Vec<u8>, _>("robohash_maker") == robohash_bytes;
		let is_taker = row.get::<Vec<u8>, _>("robohash_taker") == robohash_bytes;

		if !is_maker && !is_taker {
			return Err(anyhow::anyhow!("Robohash does not match maker or taker"));
		}

		let query = if is_maker {
			"UPDATE taken_offers SET maker_happy = ? WHERE offer_id = ?"
		} else {
			"UPDATE taken_offers SET taker_happy = ? WHERE offer_id = ?"
		};

		sqlx::query(query)
			.bind(bool_to_sql_int(is_happy))
			.bind(offer_id)
			.execute(&*self.db_pool)
			.await?;

		if !is_happy {
			sqlx::query("UPDATE taken_offers SET escrow_ongoing = 1 WHERE offer_id = ?")
				.bind(offer_id)
				.execute(&*self.db_pool)
				.await?;
		}

		Ok(())
	}

	pub async fn fetch_trader_happiness(&self, offer_id: &str) -> Result<TraderHappiness> {
		let row = sqlx::query(
			"SELECT maker_happy, taker_happy, escrow_ongoing FROM taken_offers WHERE offer_id = ?",
		)
		.bind(offer_id)
		.fetch_one(&*self.db_pool)
		.await?;

		let maker_happy: Option<i64> = row.try_get::<Option<i64>, _>("maker_happy")?;
		let taker_happy: Option<i64> = row.try_get::<Option<i64>, _>("taker_happy")?;
		let escrow_ongoing: i64 = row.try_get::<i64, _>("escrow_ongoing")?;

		Ok(TraderHappiness {
			maker_happy: maker_happy.map(|v| v != 0),
			taker_happy: taker_happy.map(|v| v != 0),
			escrow_ongoing: escrow_ongoing != 0,
		})
	}

	pub async fn fetch_escrow_result(&self, offer_id: &str) -> Result<Option<String>> {
		let row = sqlx::query("SELECT escrow_winner_robohash FROM taken_offers WHERE offer_id = ?")
			.bind(offer_id)
			.fetch_one(&*self.db_pool)
			.await?;

		let winner_robohash: Option<String> =
			row.try_get::<Option<String>, _>("escrow_winner_robohash")?;

		Ok(winner_robohash)
	}

	// pub async fn fetch_escrow_tx_payout_data(
	// 	&self,
	// 	offer_id: &str,
	// ) -> Result<EscrowPsbtConstructionData> {
	// 	let row = sqlx::query("SELECT taproot_xonly_pubkey_hex_maker, taproot_xonly_pubkey_hex_taker, musig_pubkey_compressed_hex_maker, musig_pubkey_compressed_hex_taker FROM taken_offers WHERE offer_id = ?")
	// 		.bind(offer_id)
	// 		.fetch_one(&*self.db_pool)
	// 		.await?;

	// 	let taproot_xonly_pubkey_hex_maker: String = row.get("taproot_xonly_pubkey_hex_maker");
	// 	let taproot_xonly_pubkey_hex_taker: String = row.get("taproot_xonly_pubkey_hex_taker");
	// 	let musig_pubkey_compressed_hex_maker: String =
	// 		row.get("musig_pubkey_compressed_hex_maker");
	// 	let musig_pubkey_compressed_hex_taker: String =
	// 		row.get("musig_pubkey_compressed_hex_taker");

	// 	Ok(EscrowPsbtConstructionData {
	// 		taproot_xonly_pubkey_hex_maker,
	// 		taproot_xonly_pubkey_hex_taker,
	// 		musig_pubkey_compressed_hex_maker,
	// 		musig_pubkey_compressed_hex_taker,
	// 	})
	// }

	pub async fn get_escrow_tx_amounts(
		&self,
		trade_id: &str,
		coordinator_feerate: f64,
	) -> Result<(u64, u64, u64)> {
		let row = sqlx::query(
			"SELECT amount_sat, is_buy_order, bond_amount_sat FROM active_maker_offers WHERE offer_id = ?",
		).bind(trade_id).fetch_one(&*self.db_pool).await?;

		let amount_sat: u64 = row.get("amount_sat");
		let is_buy_order: bool = 1 == row.get::<i64, _>("is_buy_order");
		let bond_amount_sat: u64 = row.get("bond_amount_sat");

		let escrow_fee_per_participant: u64 = (amount_sat as f64 * coordinator_feerate) as u64;

		let (escrow_amount_maker_sat, escrow_amount_taker_sat) = if is_buy_order {
			(amount_sat + bond_amount_sat, bond_amount_sat)
		} else {
			(bond_amount_sat, amount_sat + bond_amount_sat)
		};

		Ok((
			escrow_amount_maker_sat,
			escrow_amount_taker_sat,
			escrow_fee_per_participant,
		))
	}

	pub async fn fetch_maker_escrow_psbt_data(
		&self,
		trade_id: &str,
	) -> Result<EscrowPsbtConstructionData> {
		let row = sqlx::query(
			"SELECT escrow_inputs_hex_maker_csv, change_address_maker, taproot_pubkey_hex_maker, musig_pubkey_hex FROM active_maker_offers WHERE offer_id = ?",
		)
		.bind(trade_id)
		.fetch_one(&*self.db_pool)
		.await?;

		let deserialized_inputs = csv_hex_to_bdk_input(row.get("escrow_inputs_hex_maker_csv"))?;
		let change_address: String = row.get("change_address_maker");

		Ok(EscrowPsbtConstructionData {
			escrow_input_utxos: deserialized_inputs,
			change_address: Address::from_str(&change_address)?.assume_checked(),
			taproot_xonly_pubkey_hex: row.get("taproot_pubkey_hex_maker"),
			musig_pubkey_compressed_hex: row.get("musig_pubkey_hex"),
		})
	}
}
