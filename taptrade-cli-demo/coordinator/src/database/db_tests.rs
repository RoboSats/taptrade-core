use super::*;
#[cfg(test)]
use anyhow::Ok;

#[allow(dead_code)]
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
	let table_exists =
		sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name='maker_requests'")
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
	let order_request = OfferRequest {
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
async fn test_fetch_bond_requirements() -> Result<()> {
	let database = create_coordinator().await?;

	// Create a sample order request and insert it into the database
	let robohash_hex = "a3f1f1f0e2f3f4f5";
	let order_request = (
		hex::decode(robohash_hex).unwrap(),
		true,                          // is_buy_order
		1000,                          // amount_satoshi
		50,                            // bond_ratio
		1234567890,                    // offer_duration_ts
		"1BitcoinAddress".to_string(), // bond_address
		500,                           // bond_amount_sat
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
	let fetched_offer = database
		.fetch_bond_requirements(&robohash_hex.to_string())
		.await?;

	// Verify the result
	let expected = BondRequirements {
		bond_address: "1BitcoinAddress".to_string(),
		locking_amount_sat: 500_u64,
		min_input_sum_sat: 1000_u64,
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
		true,                          // is_buy_order
		1000,                          // amount_satoshi
		50,                            // bond_ratio
		1234567890,                    // offer_duration_ts
		"1BitcoinAddress".to_string(), // bond_address
		500,                           // bond_amount_sat
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
	let fetched_offer = database
		.fetch_and_delete_offer_from_bond_table(robohash_hex)
		.await?;

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
		true,                          // is_buy_order
		1000,                          // amount_satoshi
		50,                            // bond_ratio
		1234567890,                    // offer_duration_ts
		"1BitcoinAddress".to_string(), // bond_address
		500,                           // bond_amount_sat
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
			true,                       // is_buy_order
			15000,                      // amount_sat
			100,                        // bond_ratio
			1234567890,                 // offer_duration_ts
			"1BondAddress".to_string(), // bond_address
			50,                         // bond_amount_sat
			"signedBondHex".to_string(),
			"1PayoutAddress".to_string(),
			"musigPubNonceHex".to_string(),
			"musigPubkeyHex".to_string(),
			"1TakerBondAddress".to_string(),
		),
		(
			"offer_id_2",
			true,                       // is_buy_order
			1500,                       // amount_sat
			200,                        // bond_ratio
			1234567891,                 // offer_duration_ts
			"2BondAddress".to_string(), // bond_address
			100,                        // bond_amount_sat
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
	let result = database
		.fetch_taker_bond_requirements(&offer_id_hex.to_string())
		.await?;

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
	let is_buy_order = bool_to_sql_int(true);
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
	let result = database
		.fetch_and_delete_offer_from_public_offers_table(offer_id_hex)
		.await?;

	// Verify the result
	assert_eq!(result.offer_id, offer_id_hex);
	assert_eq!(result.robohash_maker, robohash);
	assert_eq!(bool_to_sql_int(result.is_buy_order), is_buy_order);
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
	let remaining_offers =
		sqlx::query("SELECT COUNT(*) FROM active_maker_offers WHERE offer_id = ?")
			.bind(offer_id_hex)
			.fetch_one(&*database.db_pool)
			.await?;

	let remaining_offers_count: i64 = remaining_offers.try_get(0)?;
	assert_eq!(remaining_offers_count, 0);

	Ok(())
}
