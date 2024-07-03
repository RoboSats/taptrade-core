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

pub async fn monitor_bonds(
	coordinator_db: &CoordinatorDB,
	coordinator_wallet: &CoordinatorWallet,
) -> Result<()> {
	loop {
		// fetch all bonds
		let bonds = coordinator_db.fetch_all_bonds().await?;

		// verify all bonds and initiate punishment if necessary
		for bond in bonds {
			if let Err(e) = coordinator_wallet
				.validate_bond_tx_hex(&bond.1.bond_tx_hex, &bond.1.requirements)
				.await
			{
				// punish the violator (publish bond, remove offer from db/orderbook)
				panic!("Implement bond violation punishment logic: {:?}", e);
			}
		}

		// sleep for a while
		tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
	}
}
