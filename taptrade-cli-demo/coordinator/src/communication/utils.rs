use super::*;

pub fn validate_timestamp(offer_duration_ts: u64) -> Result<(), ValidationError> {
	// Get the current time
	let now = SystemTime::now();
	// Convert the current time to a UNIX timestamp
	let unix_timestamp = now
		.duration_since(UNIX_EPOCH)
		.expect("Time went backwards")
		.as_secs();
	if offer_duration_ts < unix_timestamp + 10800 {
		return Err(ValidationError::new("Offer duration too short"));
	}
	if offer_duration_ts > unix_timestamp + 604800 {
		return Err(ValidationError::new("Offer duration too long"));
	}
	Ok(())
}
