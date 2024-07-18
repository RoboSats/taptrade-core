use std::str::FromStr;

use bdk::{bitcoin::Txid, bitcoincore_rpc::RpcApi};

use super::*;

fn get_confirmations(
	unconfirmed_txids: Vec<String>,
	coordinator: Arc<Coordinator>,
) -> Result<Vec<String>> {
	let mut now_confirmed_txs = Vec::new();
	for txid in unconfirmed_txids {
		let txid_struct = Txid::from_str(&txid)?;
		let tx_info = coordinator
			.coordinator_wallet
			.json_rpc_client
			.as_ref()
			.get_raw_transaction_info(&txid_struct, None)?;
		if let Some(confirmations) = tx_info.confirmations {
			debug!(
				"Transaction {} in now confirmed with {} confirmations",
				&txid, confirmations
			);
			if confirmations > 3 {
				now_confirmed_txs.push(txid);
			}
		}
	}
	Ok(now_confirmed_txs)
}

pub async fn update_transaction_confirmations(coordinator: Arc<Coordinator>) {
	loop {
		tokio::time::sleep(std::time::Duration::from_secs(30)).await;
		trace!("Checking for transaction confirmations");
		let unconfirmed_transactions = match coordinator
			.coordinator_db
			.fetch_unconfirmed_bond_txids()
			.await
		{
			Ok(txids) => txids,
			Err(e) => {
				error!("Error fetching unconfirmed bond txids from db: {:?}", e);
				tokio::time::sleep(std::time::Duration::from_secs(60)).await;
				continue;
			}
		};
		if unconfirmed_transactions.is_empty() {
			continue;
		}
		let coordinator_clone = Arc::clone(&coordinator);
		let newly_confirmed_txids = match tokio::task::spawn_blocking(move || {
			get_confirmations(unconfirmed_transactions, coordinator_clone)
		})
		.await
		{
			Ok(result) => match result {
				Ok(txids) => txids,
				Err(e) => {
					error!("Error getting confirmations: {:?}", e);
					Vec::new() // or handle the error as appropriate
				}
			},
			Err(e) => {
				error!("Getting tx confirmations spawn_blocking panicked: {:?}", e);
				Vec::new() // or handle the error as appropriate
			}
		};
		if !newly_confirmed_txids.is_empty() {
			if let Err(e) = coordinator
				.coordinator_db
				.confirm_bond_txids(newly_confirmed_txids)
				.await
			{
				error!("Error updating bond confirmations in db: {:?}", e);
			}
		}
	}
}
