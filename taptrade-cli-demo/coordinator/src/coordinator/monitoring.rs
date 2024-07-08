// we create an async function that loops trough the sqlite db table active_maker_offers and
// continoously verifies the bond inputs (mempool and chain), maybe with some caching in a hashmap to
// prevent querying the db all the time.
// Also needs to implement punishment logic in case a fraud is detected.
use super::*;

pub enum Table {
	Orderbook,
	ActiveTrades,
}

pub struct MonitoringBond {
	pub bond_tx_hex: String,
	pub trade_id_hex: String,
	pub requirements: BondRequirements,
	pub table: Table,
}

// the current implementation only publishes the bond and removes the offer from the db
// in a more advanced implementation we could increase the transaction fee (cpfp) and
// continue monitoring the bond transaction until a confirmation happens for maximum pain
// in case the trader is actively malicious and did not just accidentally invalidate the bond
// we could directly forward bond sats to the other parties payout address in case it is a taken trade
async fn punish_trader(
	coordinator: &Coordinator,
	robohash: Vec<u8>,
	bond: MonitoringBond,
) -> Result<()> {
	// publish bond
	coordinator
		.coordinator_wallet
		.publish_bond_tx_hex(&bond.bond_tx_hex)?; // can be made async with esplora backend if we figure out the compilation error of bdk

	// remove offer from db/orderbook
	Ok(())
}

pub async fn monitor_bonds(coordinator: Arc<Coordinator>) -> Result<()> {
	let coordinator_db = Arc::clone(&coordinator.coordinator_db);
	let coordinator_wallet = Arc::clone(&coordinator.coordinator_wallet);

	loop {
		// fetch all bonds
		let bonds = coordinator_db.fetch_all_bonds().await?;

		// verify all bonds and initiate punishment if necessary
		for bond in bonds {
			if let Err(e) = coordinator_wallet
				.validate_bond_tx_hex(&bond.1.bond_tx_hex, &bond.1.requirements)
				.await
			{
				match env::var("PUNISHMENT_ENABLED")
					.unwrap_or_else(|_| "0".to_string())
					.as_str()
				{
					"1" => {
						dbg!("Punishing trader for bond violation: {:?}", e);
						punish_trader(&coordinator, bond.0, bond.1).await?;
					}
					"0" => {
						dbg!("Punishment disabled, ignoring bond violation: {:?}", e);
						continue;
					}
					_ => Err(anyhow!("Invalid PUNISHMENT_ENABLED env var"))?,
				}
			}
		}

		// sleep for a while
		tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
	}
}
