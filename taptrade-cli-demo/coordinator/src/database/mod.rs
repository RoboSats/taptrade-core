#[cfg(test)]
mod db_tests;

use escrow_cli::EscrowCase;

use super::*;

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
	escrow_locking_input_amount_without_trade_sum: u64,
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
		// this table contains requests of makers awaiting submission of bond
		sqlx::query(
			// robohash is hash as bytes
			"CREATE TABLE IF NOT EXISTS maker_requests (
					robohash BLOB PRIMARY KEY,
					is_buy_order INTEGER,
					amount_sat INTEGER NOT NULL,
					bond_ratio INTEGER NOT NULL,
					offer_duration_ts INTEGER NOT NULL,
					bond_address TEXT NOT NULL,
					bond_amount_sat INTEGER NOT NULL,
					escrow_locking_input_amount_without_trade_sum INTEGER NOT NULL
				)",
		)
		.execute(&db_pool)
		.await?;

		// this table contains offers that are active in the orderbook awaiting a taker
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
				escrow_locking_input_amount_without_trade_sum INTEGER,
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

		// this table contains offers that are taken and are in the trade process
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
				musig_partial_sig_hex_maker TEXT,
				musig_partial_sig_hex_taker TEXT,
				escrow_psbt_hex TEXT NOT NULL,
				escrow_psbt_txid TEXT NOT NULL,
				signed_escrow_psbt_hex_maker TEXT,
				signed_escrow_psbt_hex_taker TEXT,
				escrow_psbt_is_confirmed INTEGER,
				maker_happy INTEGER,
				taker_happy INTEGER,
				escrow_ongoing INTEGER NOT NULL,
				escrow_winner_robohash TEXT,
				escrow_taproot_pk_coordinator TEXT,
				escrow_amount_maker_sat INTEGER,
				escrow_amount_taker_sat INTEGER,
				escrow_fee_per_participant INTEGER,
				escrow_output_descriptor TEXT,
				payout_transaction_psbt_hex TEXT,
				processing INTEGER NOT NULL
			)", // escrow_psbt_is_confirmed will be set 1 once the escrow psbt is confirmed onchain
		)
		.execute(&db_pool)
		.await?;
		debug!("Database initialized");
		let shared_db_pool = Arc::new(db_pool);
		Ok(Self {
			db_pool: shared_db_pool,
		})
	}

	/// insert a new maker request to create an offer in the table
	pub async fn insert_new_maker_request(
		&self,
		order: &OfferRequest,
		bond_requirements: &BondRequirementResponse,
	) -> Result<()> {
		sqlx::query(
			"INSERT OR REPLACE INTO maker_requests (robohash, is_buy_order, amount_sat,
					bond_ratio, offer_duration_ts, bond_address, bond_amount_sat, escrow_locking_input_amount_without_trade_sum)
					VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
		)
		.bind(hex::decode(&order.robohash_hex)?)
		.bind(bool_to_sql_int(order.is_buy_order))
		.bind(order.amount_satoshi as i64)
		.bind(order.bond_ratio)
		.bind(order.offer_duration_ts as i64)
		.bind(bond_requirements.bond_address.clone())
		.bind(bond_requirements.locking_amount_sat as i64)
		.bind(bond_requirements.escrow_locking_input_amount_without_trade_sum as i64)
		.execute(&*self.db_pool)
		.await?;

		Ok(())
	}

	/// fetch the bond requirements for a maker request
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

	/// deletes the maker offer from the pending table and returns it
	async fn fetch_and_delete_offer_from_bond_table(
		&self,
		robohash_hex: &str,
	) -> Result<AwaitingBondOffer> {
		let fetched_values = sqlx::query_as::<_, (Vec<u8>, bool, i64, u8, i64, String, i64)> (
			"SELECT robohash, is_buy_order, amount_sat, bond_ratio, offer_duration_ts, bond_address, bond_amount_sat, escrow_locking_input_amount_without_trade_sum FROM maker_requests WHERE robohash = ?",
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
			escrow_locking_input_amount_without_trade_sum: fetched_values.6 as u64,
		};
		debug!(
			"Deleted offer from maker_requests table. Fetched offer: {:#?}",
			awaiting_bond_offer
		);
		Ok(awaiting_bond_offer)
	}

	/// on reciept of a valid bond this will fetch the offer from the pending table and insert it into the active trades table (orderbook)
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
					change_address_maker, escrow_inputs_hex_maker_csv, escrow_locking_input_amount_without_trade_sum)
					VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
		.bind(remaining_offer_information.escrow_locking_input_amount_without_trade_sum as i64)
		.execute(&*self.db_pool)
		.await?;

		debug!("\nDATABASE: moved offer to active trades\n");
		Ok(remaining_offer_information.offer_duration_ts)
	}

	/// called by the taker, returns offers that match the taker's request
	pub async fn fetch_suitable_offers(
		&self,
		requested_offer: &OffersRequest,
	) -> Result<Option<Vec<PublicOffer>>> {
		debug!(
			"Fetching suitable offers from db. Specification: {:#?}",
			requested_offer
		);
		let fetched_offers = sqlx::query_as::<_, (String, i64, i64, String, i64)> (
            "SELECT offer_id, amount_sat, bond_amount_sat, taker_bond_address, escrow_locking_input_amount_without_trade_sum FROM active_maker_offers WHERE is_buy_order = ? AND amount_sat BETWEEN ? AND ?",
        )
        .bind(requested_offer.buy_offers)
        .bind(requested_offer.amount_min_sat as i64)
        .bind(requested_offer.amount_max_sat as i64)
        .fetch_all(&*self.db_pool)
        .await?;

		let available_offers: Vec<PublicOffer> = fetched_offers
			.into_iter()
			.map(
				|(offer_id_hex, amount_sat, bond_amount_sat, bond_address_taker, min_inputs)| {
					PublicOffer {
						offer_id_hex,
						amount_sat: amount_sat as u64,
						bond_requirements: BondRequirementResponse {
							bond_address: bond_address_taker,
							locking_amount_sat: bond_amount_sat as u64,
							escrow_locking_input_amount_without_trade_sum: min_inputs as u64,
						},
					}
				},
			)
			.collect();
		if available_offers.is_empty() {
			debug!("No available offers in db...");
			return Ok(None);
		}
		Ok(Some(available_offers))
	}

	/// fetches the bond requirements for the taker
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

	/// used to fetch and delete the offer from the orderbook (active_maker_offers) table
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

	/// once the taker submitted his bond the offer is moved to the taken_offers table (removed from the orderbook)
	pub async fn add_taker_info_and_move_table(
		&self,
		trade_and_taker_info: &OfferPsbtRequest,
		escrow_tx_data: &EscrowPsbt,
	) -> Result<()> {
		// this fetches the offer and deletes it from the orderbook
		let public_offer = self
			.fetch_and_delete_offer_from_public_offers_table(
				&trade_and_taker_info.offer.offer_id_hex,
			)
			.await?;

		// insert the offer into the taken_offers table
		sqlx::query(
				"INSERT OR REPLACE INTO taken_offers (offer_id, robohash_maker, robohash_taker, is_buy_order, amount_sat,
						bond_ratio, offer_duration_ts, bond_address_maker, bond_address_taker, bond_amount_sat, bond_tx_hex_maker,
						bond_tx_hex_taker, payout_address_maker, payout_address_taker, taproot_xonly_pubkey_hex_maker, taproot_xonly_pubkey_hex_taker, musig_pub_nonce_hex_maker, musig_pubkey_compressed_hex_maker,
						musig_pub_nonce_hex_taker, musig_pubkey_compressed_hex_taker, escrow_psbt_hex, escrow_psbt_txid, escrow_output_descriptor, escrow_psbt_is_confirmed, escrow_ongoing,
						escrow_taproot_pk_coordinator, escrow_amount_maker_sat, escrow_amount_taker_sat, escrow_fee_per_participant, processing)
						VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
			)
			.bind(public_offer.offer_id)
			.bind(public_offer.robohash_maker)
			.bind(hex::decode(&trade_and_taker_info.trade_data.robohash_hex)?)
			.bind(bool_to_sql_int(public_offer.is_buy_order))
			.bind(public_offer.amount_sat)
			.bind(public_offer.bond_ratio)
			.bind(public_offer.offer_duration_ts)
			.bind(public_offer.bond_address_maker)
			.bind(trade_and_taker_info.offer.bond_requirements.bond_address.clone())
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
			.bind(&escrow_tx_data.escrow_psbt_hex)
			.bind(&escrow_tx_data.escrow_tx_txid)
			.bind(&escrow_tx_data.escrow_output_descriptor)
			.bind(0)
			.bind(0)
			.bind(&escrow_tx_data.coordinator_xonly_escrow_pk)
			.bind(escrow_tx_data.escrow_amount_maker_sat as i64)
			.bind(escrow_tx_data.escrow_amount_taker_sat as i64)
			.bind(escrow_tx_data.escrow_fee_sat_per_participant as i64)
			.bind(0)
			.execute(&*self.db_pool)
			.await?;

		Ok(())
	}

	/// fetches the escrow psbt from the db for the given offer
	pub async fn fetch_escrow_output_information(
		&self,
		offer_id_hex: &str,
	) -> Result<Option<EscrowPsbt>> {
		let offer = sqlx::query(
			"SELECT escrow_output_descriptor, escrow_amount_maker_sat,
			escrow_amount_taker_sat, escrow_fee_per_participant, escrow_taproot_pk_coordinator, escrow_psbt_hex, escrow_psbt_txid
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
		let escrow_amount_maker_sat = offer.try_get::<i64, _>("escrow_amount_maker_sat")? as u64;
		let escrow_amount_taker_sat = offer.try_get::<i64, _>("escrow_amount_taker_sat")? as u64;
		let escrow_fee_sat_per_participant =
			offer.try_get::<i64, _>("escrow_fee_per_participant")? as u64;
		let coordinator_xonly_escrow_pk =
			offer.try_get::<String, _>("escrow_taproot_pk_coordinator")?;
		let escrow_psbt_hex = offer.try_get::<String, _>("escrow_psbt_hex")?;
		let escrow_tx_txid = offer.try_get::<String, _>("escrow_psbt_txid")?;

		Ok(Some(EscrowPsbt {
			escrow_tx_txid,
			escrow_psbt_hex,
			escrow_output_descriptor,
			coordinator_xonly_escrow_pk,
			escrow_amount_maker_sat,
			escrow_amount_taker_sat,
			escrow_fee_sat_per_participant,
		}))
	}

	/// returns a hashmap of RoboHash, MonitoringBond for the monitoring loop
	/// in case this gets a bottleneck (db too large for heap) we can implement in place checking
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
		Ok(bonds)
	}

	/// removes an offer from the orderbook (active_maker_offers) table, gets called when a bond violation is detected
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
		Ok(())
	}

	/// fetches all txids of escrow transactions that have the flag escrow_psbt_is_confirmed set to 0
	/// used to check if theses txids are confirmed onchain
	pub async fn fetch_unconfirmed_escrow_txids(&self) -> Result<Vec<String>> {
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

	/// sets all passed escrow txids to confirmed
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

	/// used to check if txid is set to confirmed in the db
	pub async fn get_txid_confirmation_status(&self, txid: &String) -> Result<bool> {
		let status = sqlx::query(
			"SELECT escrow_psbt_is_confirmed FROM taken_offers WHERE escrow_psbt_txid = ?",
		)
		.bind(txid)
		.fetch_one(&*self.db_pool)
		.await?;
		Ok(status.get::<i64, _>("escrow_psbt_is_confirmed") == 1)
	}

	/// used to verify that a robohash/user id is actually part of a trade and contained in the table
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

	/// used to check if a user id / robohash is the maker or taker (true if maker, false if taker)
	async fn is_maker_in_taken_offers(&self, offer_id: &str, robohash_hex: &str) -> Result<bool> {
		let robohash_bytes = hex::decode(robohash_hex)?;

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
		Ok(is_maker)
	}

	/// insert a returned, signed escrow psbt into the db, if it was already existent return false, else return true if inserted
	pub async fn insert_signed_escrow_psbt(
		&self,
		signed_escow_psbt_data: &PsbtSubmissionRequest,
	) -> Result<bool> {
		// first check if the escrow psbt has already been submitted
		let is_maker = self
			.is_maker_in_taken_offers(
				&signed_escow_psbt_data.offer_id_hex,
				&signed_escow_psbt_data.robohash_hex,
			)
			.await?;

		let is_already_there = match is_maker {
			true => {
				let status = sqlx::query(
					"SELECT signed_escrow_psbt_hex_maker FROM taken_offers WHERE offer_id = ?",
				)
				.bind(&signed_escow_psbt_data.offer_id_hex)
				.fetch_one(&*self.db_pool)
				.await?;
				status
					.get::<Option<String>, _>("signed_escrow_psbt_hex_maker")
					.is_some()
			}
			false => {
				let status = sqlx::query(
					"SELECT signed_escrow_psbt_hex_taker FROM taken_offers WHERE offer_id = ?",
				)
				.bind(&signed_escow_psbt_data.offer_id_hex)
				.fetch_one(&*self.db_pool)
				.await?;
				status
					.get::<Option<String>, _>("signed_escrow_psbt_hex_taker")
					.is_some()
			}
		};

		if is_already_there {
			Ok(false)
		} else {
			let query = if is_maker {
				"UPDATE taken_offers SET signed_escrow_psbt_hex_maker = ? WHERE offer_id = ?"
			} else {
				"UPDATE taken_offers SET signed_escrow_psbt_hex_taker = ? WHERE offer_id = ?"
			};

			sqlx::query(query)
				.bind(&signed_escow_psbt_data.signed_psbt_hex)
				.bind(&signed_escow_psbt_data.offer_id_hex)
				.execute(&*self.db_pool)
				.await?;
			Ok(true)
		}
	}

	/// used to fetch both signed escrow locking psbts from the db
	pub async fn fetch_both_signed_escrow_psbts(
		&self,
		offer_id_hex: &str,
	) -> Result<Option<(String, String)>> {
		let row = sqlx::query(
			"SELECT signed_escrow_psbt_hex_maker, signed_escrow_psbt_hex_taker FROM taken_offers WHERE offer_id = ?",
		)
		.bind(offer_id_hex)
		.fetch_one(&*self.db_pool)
		.await?;

		let maker_psbt: Option<String> = row.try_get("signed_escrow_psbt_hex_maker")?;
		let taker_psbt: Option<String> = row.try_get("signed_escrow_psbt_hex_taker")?;

		Ok(match (maker_psbt, taker_psbt) {
			(Some(maker), Some(taker)) => Some((maker, taker)),
			_ => None,
		})
	}

	/// used to check if the escrow locking transaction has been confirmed onchain
	pub async fn fetch_escrow_tx_confirmation_status(&self, offer_id: &str) -> Result<bool> {
		let status =
			sqlx::query("SELECT escrow_psbt_is_confirmed FROM taken_offers WHERE offer_id = ?")
				.bind(offer_id)
				.fetch_one(&*self.db_pool)
				.await?;
		Ok(status.get::<i64, _>("escrow_psbt_is_confirmed") == 1)
	}

	/// used to set that a trader is satisfied with the trade (true)
	pub async fn set_trader_happy_field(
		&self,
		offer_id: &str,
		robohash: &str,
		is_happy: bool,
	) -> Result<()> {
		let is_maker = self.is_maker_in_taken_offers(offer_id, robohash).await?;

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

	/// checked by the payout handler on request to determine if the trade is ready for payout and
	/// if escrow is required
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

	/// this will be checked by the payout handler on request, the escrow winner will be set trough CLI input
	pub async fn fetch_escrow_result(&self, offer_id: &str) -> Result<Option<String>> {
		let row = sqlx::query("SELECT escrow_winner_robohash FROM taken_offers WHERE offer_id = ?")
			.bind(offer_id)
			.fetch_one(&*self.db_pool)
			.await?;

		let winner_robohash: Option<String> =
			row.try_get::<Option<String>, _>("escrow_winner_robohash")?;
		trace!(
			"Escrow winner robohash fetched from db: {:?}",
			winner_robohash,
		);
		Ok(winner_robohash)
	}

	/// fetch the amounts the traders have to contribute to the escrow locking transaction
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

		let escrow_fee_per_participant: u64 =
			(amount_sat as f64 * (coordinator_feerate / 100.0)) as u64;

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

	/// fetch the data required to construct the escrow psbt for the maker
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

	/// fetch the data required to construct the musig keyspend payout transaction to be signed by the traders on payout initialization
	pub async fn fetch_payout_data(&self, trade_id: &str) -> Result<PayoutData> {
		let row = sqlx::query(
			"SELECT escrow_output_descriptor, payout_address_maker,
			payout_address_taker, musig_pub_nonce_hex_maker, musig_pub_nonce_hex_taker,
			escrow_amount_maker_sat, escrow_amount_taker_sat, musig_pubkey_compressed_hex_maker,
			musig_pubkey_compressed_hex_taker
			FROM taken_offers WHERE offer_id = ?",
		)
		.bind(trade_id)
		.fetch_one(&*self.db_pool)
		.await
		.context("SQL query to fetch escrow_ouput_descriptor failed.")?;

		let escrow_output_descriptor = row.try_get("escrow_output_descriptor")?;
		let payout_address_maker = row.try_get("payout_address_maker")?;
		let payout_address_taker = row.try_get("payout_address_taker")?;
		let musig_pub_nonce_hex_maker: &str = row.try_get("musig_pub_nonce_hex_maker")?;
		let musig_pub_nonce_hex_taker: &str = row.try_get("musig_pub_nonce_hex_taker")?;
		let payout_amount_maker: u64 = row.try_get::<i64, _>("escrow_amount_maker_sat")? as u64;
		let payout_amount_taker: u64 = row.try_get::<i64, _>("escrow_amount_taker_sat")? as u64;
		let musig_pubkey_hex_maker: &str = row.try_get("musig_pubkey_compressed_hex_maker")?;
		let musig_pubkey_hex_taker: &str = row.try_get("musig_pubkey_compressed_hex_taker")?;

		PayoutData::new_from_strings(
			escrow_output_descriptor,
			payout_address_maker,
			payout_address_taker,
			payout_amount_maker,
			payout_amount_taker,
			musig_pub_nonce_hex_maker,
			musig_pub_nonce_hex_taker,
			musig_pubkey_hex_maker,
			musig_pubkey_hex_taker,
		)
	}

	/// insert the keyspend payout transaction into the db
	pub async fn insert_keyspend_payout_psbt(
		&self,
		offer_id_hex: &str,
		payout_psbt_hex: &str,
	) -> Result<()> {
		sqlx::query("UPDATE taken_offers SET payout_transaction_psbt_hex = ? WHERE offer_id = ?")
			.bind(payout_psbt_hex)
			.bind(offer_id_hex)
			.execute(&*self.db_pool)
			.await?;
		Ok(())
	}

	/// insert a partial signature submitted by the trader into the db
	pub async fn insert_partial_sig(
		&self,
		partial_sig_hex: &str,
		offer_id_hex: &str,
		robohash_hex: &str,
	) -> Result<()> {
		// first check if the escrow psbt has already been submitted
		let is_maker = self
			.is_maker_in_taken_offers(offer_id_hex, robohash_hex)
			.await?;

		let is_already_there = match is_maker {
			true => {
				let status = sqlx::query(
					"SELECT musig_partial_sig_hex_maker FROM taken_offers WHERE offer_id = ?",
				)
				.bind(offer_id_hex)
				.fetch_one(&*self.db_pool)
				.await?;
				status
					.get::<Option<String>, _>("musig_partial_sig_hex_maker")
					.is_some()
			}
			false => {
				let status = sqlx::query(
					"SELECT musig_partial_sig_hex_taker FROM taken_offers WHERE offer_id = ?",
				)
				.bind(offer_id_hex)
				.fetch_one(&*self.db_pool)
				.await?;
				status
					.get::<Option<String>, _>("musig_partial_sig_hex_taker")
					.is_some()
			}
		};

		warn!("we can use musig2::verify_partial to detect users submitting invalid partial signatures");
		if is_already_there {
			return Err(anyhow!("Partial sig already submitted"));
		} else {
			let query = if is_maker {
				"UPDATE taken_offers SET musig_partial_sig_hex_maker = ? WHERE offer_id = ?"
			} else {
				"UPDATE taken_offers SET musig_partial_sig_hex_taker = ? WHERE offer_id = ?"
			};
			sqlx::query(query)
				.bind(partial_sig_hex)
				.bind(offer_id_hex)
				.execute(&*self.db_pool)
				.await?;
		}
		Ok(())
	}

	/// fetches all data required to execute the keyspend payout (including the signatures)
	pub async fn fetch_keyspend_payout_information(
		&self,
		offer_id_hex: &str,
	) -> Result<Option<KeyspendContext>> {
		let row = sqlx::query(
			"SELECT musig_partial_sig_hex_maker, musig_partial_sig_hex_taker,
			musig_pubkey_compressed_hex_maker, musig_pubkey_compressed_hex_taker, musig_pub_nonce_hex_maker, musig_pub_nonce_hex_taker,
			payout_transaction_psbt_hex, escrow_output_descriptor FROM taken_offers WHERE offer_id = ?",
		).bind(offer_id_hex).fetch_one(&*self.db_pool).await?;

		let maker_sig: Option<String> = row.try_get("musig_partial_sig_hex_maker")?;
		let taker_sig: Option<String> = row.try_get("musig_partial_sig_hex_taker")?;

		let maker_pubkey: String = row.try_get("musig_pubkey_compressed_hex_maker")?;
		let taker_pubkey: String = row.try_get("musig_pubkey_compressed_hex_taker")?;

		let maker_nonce: String = row.try_get("musig_pub_nonce_hex_maker")?;
		let taker_nonce: String = row.try_get("musig_pub_nonce_hex_taker")?;

		let keyspend_psbt: String = row.try_get("payout_transaction_psbt_hex")?;
		let descriptor: String = row.try_get("escrow_output_descriptor")?;

		if let (Some(maker), Some(taker)) = (maker_sig, taker_sig) {
			Ok(Some(KeyspendContext::from_hex_str(
				&maker,
				&taker,
				&maker_nonce,
				&taker_nonce,
				&maker_pubkey,
				&taker_pubkey,
				&keyspend_psbt,
				&descriptor,
			)?))
		} else {
			Ok(None)
		}
	}

	/// fetches the keyspend payout psbt from the db
	pub async fn fetch_keyspend_payout_psbt(&self, offer_id_hex: &str) -> Result<Option<String>> {
		let row =
			sqlx::query("SELECT payout_transaction_psbt_hex FROM taken_offers WHERE offer_id = ?")
				.bind(offer_id_hex)
				.fetch_one(&*self.db_pool)
				.await?;

		let payout_psbt: Option<String> = row.try_get("payout_transaction_psbt_hex")?;
		Ok(payout_psbt)
	}

	/// used to as db lock to prevent race conditions when the payout is being handled
	pub async fn toggle_processing(&self, offer_id: &str) -> Result<bool> {
		let result = sqlx::query(
			r#"
        UPDATE taken_offers
        SET processing = CASE
            WHEN processing = 0 THEN 1
            ELSE 0
        END
        WHERE offer_id = ?
        RETURNING processing
        "#,
		)
		.bind(offer_id)
		.fetch_one(&*self.db_pool)
		.await?;

		trace!("Toggled processing status for offer {}", offer_id);
		Ok(result.get::<i64, _>(0) == 1)
	}

	/// deletes a finished offer from the database 🎉
	pub async fn delete_complete_offer(&self, offer_id: &str) -> Result<()> {
		sqlx::query("DELETE FROM taken_offers WHERE offer_id = ?")
			.bind(offer_id)
			.execute(&*self.db_pool)
			.await?;
		Ok(())
	}

	/// fetch entries with escrow awaiting flag to request cli input
	pub async fn get_open_escrows(&self) -> Result<Vec<EscrowCase>> {
		let escrows = sqlx::query(
			"SELECT offer_id, robohash_maker, robohash_taker
			FROM taken_offers WHERE escrow_ongoing = 1",
		)
		.fetch_all(&*self.db_pool)
		.await?;

		let mut escrow_cases = Vec::new();
		for escrow in escrows {
			escrow_cases.push(EscrowCase {
				offer_id: escrow.get("offer_id"),
				maker_id: hex::encode(escrow.get::<Vec<u8>, _>("robohash_maker")),
				taker_id: hex::encode(escrow.get::<Vec<u8>, _>("robohash_taker")),
			});
		}
		Ok(escrow_cases)
	}

	// set the winning robohash in the db
	pub async fn resolve_escrow(&self, offer_id: &str, winner_robohash: &str) -> Result<()> {
		sqlx::query("UPDATE taken_offers SET escrow_ongoing = 0, escrow_winner_robohash = ? WHERE offer_id = ?")
			.bind(winner_robohash)
			.bind(offer_id)
			.execute(&*self.db_pool)
			.await?;
		Ok(())
	}
}
