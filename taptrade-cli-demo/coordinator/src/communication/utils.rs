use anyhow::Context;

use super::*;

impl OrderRequest {
	pub fn sanity_check(&self) -> Result<()> {
		// Get the current time
		let now = SystemTime::now();
		// Convert the current time to a UNIX timestamp
		let unix_timestamp = now
			.duration_since(UNIX_EPOCH)
			.context("Time went backwards")?
			.as_secs();
		if self.amount_satoshi < 10000 {
			return Err(anyhow!("Amount too low"));
		}
		if self.amount_satoshi > 20000000 {
			return Err(anyhow!("Amount too high"));
		}
		if self.bond_ratio < 2 || self.bond_ratio > 50 {
			return Err(anyhow!("Bond ratio out of bounds"));
		}
		if self.offer_duration_ts < unix_timestamp + 10800 {
			return Err(anyhow!("Offer duration too short"));
		}
		if self.offer_duration_ts > unix_timestamp + 604800 {
			return Err(anyhow!("Offer duration too long"));
		}
		Ok(())
	}
}
