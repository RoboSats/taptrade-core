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

// ANYHOW ERROR HANDLING
// --------------
// Make our own error that wraps `anyhow::Error`.
#[derive(Debug)]
pub struct AppError(anyhow::Error);

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for AppError {
	fn into_response(self) -> Response {
		(
			StatusCode::INTERNAL_SERVER_ERROR,
			format!("Something went wrong: {}", self.0),
		)
			.into_response()
	}
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
// `Result<_, AppError>`. That way you don't need to do that manually.
impl<E> From<E> for AppError
where
	E: Into<anyhow::Error>,
{
	fn from(err: E) -> Self {
		Self(err.into())
	}
}
