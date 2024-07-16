// we create an async function that loops trough the sqlite db table active_maker_offers and
// continoously verifies the bond inputs (mempool and chain), maybe with some caching in a hashmap to
// prevent querying the db all the time.
// Also needs to implement punishment logic in case a fraud is detected.
use super::*;
use anyhow::Context;
use mempool_monitoring::MempoolHandler;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq)]
pub enum Table {
	Orderbook,
	ActiveTrades,
	Memory,
}

#[derive(Debug, Clone)]
pub struct MonitoringBond {
	pub bond_tx_hex: String,
	pub trade_id_hex: String,
	pub robot: Vec<u8>,
	pub requirements: BondRequirements,
	pub table: Table,
}

impl MonitoringBond {
	// used a hash of bond instead of txid to prevent issues when a valid txid can't be generated
	// due to missing fields etc. (crate error)
	pub fn id(&self) -> Result<Vec<u8>> {
		Ok(sha256(&hex::decode(&self.bond_tx_hex)?))
	}

	async fn remove_from_db_tables(&self, db: Arc<CoordinatorDB>) -> Result<()> {
		// remove bond from db
		db.remove_violating_bond(self)
			.await
			.context("Error removing violating bond from db")?;
		Ok(())
	}

	// the current implementation only publishes the bond and removes the offer from the db
	// in a more advanced implementation we could increase the transaction fee (cpfp) and
	// continue monitoring the bond transaction until a confirmation happens for maximum pain
	// in case the trader is actively malicious and did not just accidentally invalidate the bond
	// we could directly forward bond sats to the other parties payout address in case it is a taken trade
	async fn punish(&self, coordinator: &Coordinator) -> Result<()> {
		// publish bond
		debug!("Publishing violating bond tx: {}", self.bond_tx_hex);
		coordinator
			.coordinator_wallet
			.publish_bond_tx_hex(&self.bond_tx_hex)?; // can be made async with esplora backend if we figure out the compilation error of bdk

		// remove offer from db/orderbook
		self.remove_from_db_tables(coordinator.coordinator_db.clone())
			.await?;
		Ok(())
	}
}

pub async fn monitor_bonds(coordinator: Arc<Coordinator>) -> Result<()> {
	let coordinator_db = Arc::clone(&coordinator.coordinator_db);
	let coordinator_wallet = Arc::clone(&coordinator.coordinator_wallet);

	loop {
		// sleep for a while
		tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
		// fetch all bonds
		let bonds = Arc::new(coordinator_db.fetch_all_bonds().await?);
		if bonds.is_empty() {
			continue;
		}
		let validation_results = coordinator_wallet
			.validate_bonds(Arc::clone(&bonds))
			.await?;
		debug!("Monitoring active bonds: {}", bonds.len());
		// verify all bonds and initiate punishment if necessary
		for (_, (bond, error)) in validation_results {
			warn!("Bond validation failed: {:?}", error);
			match env::var("PUNISHMENT_ENABLED")
				.unwrap_or_else(|_| "0".to_string())
				.as_str()
			{
				"1" => {
					dbg!("Punishing trader for bond violation: {:?}", error);
					bond.punish(&coordinator).await?;
				}
				"0" => {
					dbg!("Punishment disabled, ignoring bond violation: {:?}", error);
					continue;
				}
				_ => Err(anyhow!("Invalid PUNISHMENT_ENABLED env var"))?,
			}
		}
	}
}

fn sha256(data: &[u8]) -> Vec<u8> {
	let mut hasher = Sha256::new();
	hasher.update(data);
	let result = hasher.finalize();
	result.to_vec()
}
