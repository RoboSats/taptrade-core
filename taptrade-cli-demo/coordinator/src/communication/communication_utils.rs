use super::*;
/// Validates the timestamp of an offer duration.
///
/// This function takes an offer duration timestamp as input and validates it against the current time.
/// It checks if the offer duration is within a valid range, which is between the current time plus 3 hours
/// and the current time plus 7 days. If the offer duration is too short or too long, it returns a validation error.
///
/// # Arguments
///
/// * `offer_duration_ts` - The offer duration timestamp to validate.
///
/// # Returns
///
/// * `Result<(), ValidationError>` - An empty result if the offer duration is valid, or a validation error if it is not.
///
/// # Example
///
/// ```
/// use coordinator::communication::communication_utils::validate_timestamp;
/// use std::time::{SystemTime, UNIX_EPOCH};
///
/// // Get the current time
/// let now = SystemTime::now();
/// // Convert the current time to a UNIX timestamp
/// let unix_timestamp = now
///     .duration_since(UNIX_EPOCH)
///     .expect("Time went backwards")
///     .as_secs();
///
/// // Use a timestamp that is within the valid range (current time + 4 hours)
/// let offer_duration_ts = unix_timestamp + 4 * 3600;
/// let result = validate_timestamp(offer_duration_ts);
/// assert!(result.is_ok());
/// ```
///
/// # Errors
///
/// This function can return the following errors:
///
/// * `ValidationError` - If the offer duration is too short or too long.
///
/// # Panics
///
/// This function may panic if the system time goes backwards during the calculation of the current time.
///
/// # Safety
///
/// This function is safe to use.
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
