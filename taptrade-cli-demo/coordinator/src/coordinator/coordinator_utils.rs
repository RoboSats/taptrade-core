use super::*;

#[derive(Debug)]
pub enum PayoutProcessingResult {
	ReadyPSBT(String),
	NotReady,
	LostEscrow,
	DecidingEscrow,
}

pub fn generate_random_order_id(len: usize) -> String {
	// Generate `len` random bytes
	let bytes: Vec<u8> = rand::thread_rng()
		.sample_iter(&rand::distributions::Standard)
		.take(len)
		.collect();

	// Convert bytes to hex string
	hex::encode(bytes)
}

pub async fn check_offer_and_confirmation(
	offer_id_hex: &str,
	robohash_hex: &str,
	database: &CoordinatorDB,
) -> Result<(), RequestError> {
	// sanity check if offer is in table
	match database
		.is_valid_robohash_in_table(robohash_hex, offer_id_hex)
		.await
	{
		Ok(false) => return Err(RequestError::NotFound),
		Ok(true) => (),
		Err(e) => return Err(RequestError::Database(e.to_string())),
	};

	// sanity check if the escrow tx is confirmed
	match database
		.fetch_escrow_tx_confirmation_status(offer_id_hex)
		.await
	{
		Ok(false) => Err(RequestError::NotConfirmed),
		Ok(true) => Ok(()),
		Err(e) => Err(RequestError::Database(e.to_string())),
	}
}
