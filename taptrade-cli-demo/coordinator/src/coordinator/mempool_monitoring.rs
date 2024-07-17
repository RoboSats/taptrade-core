// Obsolete trough usage of gettxspendingprevout, unfortunately bitcoincore_rpc does not support it yet
// doing upstream PR with gettxspendingprevout would make it possible to get rid of this internal mempool state

use super::*;
use anyhow::Ok;
use bdk::bitcoin::consensus::encode::deserialize;
use bdk::bitcoin::{OutPoint, Transaction};
use bdk::bitcoin::{TxIn, Txid};
use bdk::bitcoincore_rpc::{Client, RpcApi};
use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use std::sync::RwLock;

struct Mempool {
	transactions: Arc<RwLock<HashMap<Txid, Vec<TxIn>>>>,
	utxo_set: Arc<RwLock<HashSet<OutPoint>>>,
	json_rpc_client: Arc<Client>,
}

impl Mempool {
	fn new(json_rpc_client: Arc<Client>) -> Self {
		Self {
			transactions: Arc::new(RwLock::new(HashMap::new())),
			utxo_set: Arc::new(RwLock::new(HashSet::new())),
			json_rpc_client,
		}
	}
}

fn run_mempool(mempool: Arc<Mempool>) {
	loop {
		// sleep for a while
		std::thread::sleep(std::time::Duration::from_secs(15));
		trace!("Fetching mempool");
		let mempool_txs = match mempool.json_rpc_client.deref().get_raw_mempool() {
			std::result::Result::Ok(mempool_txs) => mempool_txs,
			Err(e) => {
				error!("Error fetching mempool: {}", e);
				continue;
			}
		};
		let mut mempool_state = mempool
			.transactions
			.write()
			.expect("Error locking mempool write mutex");
		for txid in &mempool_txs {
			if mempool_state.contains_key(txid) {
				continue;
			} else {
				let tx = match mempool
					.json_rpc_client
					.deref()
					.get_raw_transaction(&txid, None)
				{
					std::result::Result::Ok(tx) => tx,
					Err(e) => {
						error!(
							"Error fetching transaction {} from mempool: {}",
							txid.to_string(),
							e
						);
						continue;
					}
				};
				let mut inputs = Vec::new();
				for input in tx.input {
					inputs.push(input);
				}
				mempool_state.insert(*txid, inputs);
			}
		}
		mempool_state.retain(|txid, _| mempool_txs.contains(txid));
		let mut utxo_set = mempool
			.utxo_set
			.write()
			.expect("Error locking utxo_set write mutex");
		utxo_set.clear();
		for (_, inputs) in mempool_state.iter() {
			for input in inputs {
				utxo_set.insert(input.previous_output);
			}
		}
	}
}

pub struct MempoolHandler {
	mempool: Arc<Mempool>,
}

impl MempoolHandler {
	pub async fn new(json_rpc_client: Arc<Client>) -> Self {
		let mempool = Arc::new(Mempool::new(json_rpc_client));
		let mempool_clone = Arc::clone(&mempool);
		tokio::task::spawn_blocking(move || run_mempool(mempool_clone));
		Self { mempool }
	}

	pub async fn lookup_mempool_inputs(
		&self,
		bonds: &Vec<MonitoringBond>,
	) -> Result<HashMap<Vec<u8>, (MonitoringBond, anyhow::Error)>> {
		debug!("Looking up mempool inputs for bonds");
		let mut bonds_to_punish: HashMap<Vec<u8>, (MonitoringBond, anyhow::Error)> = HashMap::new();
		let utxo_set = self
			.mempool
			.utxo_set
			.read()
			.expect("Error locking utxo_set read mutex");
		debug!("Mempool utxo_set: {:?}", utxo_set);
		for bond in bonds {
			let bond_tx: Transaction = deserialize(&hex::decode(&bond.bond_tx_hex)?)?;
			for input in bond_tx.input {
				if utxo_set.contains(&input.previous_output) {
					bonds_to_punish.insert(bond.id()?, (bond.clone(), anyhow!("Input in mempool")));
					break;
				}
			}
		}
		Ok(bonds_to_punish)
	}
}
