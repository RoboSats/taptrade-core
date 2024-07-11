mod utils;
// pub mod verify_tx;

use super::*;
use anyhow::Context;
use bdk::{
	bitcoin::{self, bip32::ExtendedPrivKey, consensus::encode::deserialize, Transaction},
	bitcoincore_rpc::{Client, RawTx, RpcApi},
	blockchain::{rpc::Auth, Blockchain, ConfigurableBlockchain, RpcBlockchain, RpcConfig},
	sled::{self, Tree},
	template::Bip86,
	wallet::verify::*,
	KeychainKind, SyncOptions, Wallet,
};
use std::str::FromStr;
use std::{fmt, ops::Deref};
use utils::*;
// use verify_tx::*;

#[derive(Clone)]
pub struct CoordinatorWallet<D: bdk::database::BatchDatabase> {
	pub wallet: Arc<Mutex<Wallet<D>>>,
	pub backend: Arc<RpcBlockchain>,
	pub json_rpc_client: Arc<bdk::bitcoincore_rpc::Client>,
}

#[derive(PartialEq, Debug, Clone)]
pub struct BondRequirements {
	pub bond_address: String,
	pub locking_amount_sat: u64,
	pub min_input_sum_sat: u64,
}

pub fn init_coordinator_wallet() -> Result<CoordinatorWallet<sled::Tree>> {
	let wallet_xprv = ExtendedPrivKey::from_str(
		&env::var("WALLET_XPRV").context("loading WALLET_XPRV from .env failed")?,
	)?;
	let rpc_config = RpcConfig {
		url: env::var("BITCOIN_RPC_ADDRESS_PORT")?.to_string(),
		auth: Auth::Cookie {
			file: env::var("BITCOIN_RPC_COOKIE_FILE_PATH")?.into(),
		},
		network: bdk::bitcoin::Network::Testnet,
		wallet_name: env::var("BITCOIN_RPC_WALLET_NAME")?,
		sync_params: None,
	};
	let json_rpc_client = Client::new(&rpc_config.url, rpc_config.auth.clone().into())?;
	let backend = RpcBlockchain::from_config(&rpc_config)?;
	// let backend = EsploraBlockchain::new(&env::var("ESPLORA_BACKEND")?, 1000);
	let sled_db = sled::open(env::var("BDK_DB_PATH")?)?.open_tree("default_wallet")?;
	let wallet = Wallet::new(
		Bip86(wallet_xprv, KeychainKind::External),
		Some(Bip86(wallet_xprv, KeychainKind::Internal)),
		bitcoin::Network::Testnet,
		sled_db,
	)?;

	wallet
		.sync(&backend, SyncOptions::default())
		.context("Connection to electrum server failed.")?; // we could also use Esplora to make this async
	dbg!(wallet.get_balance()?);
	Ok(CoordinatorWallet {
		wallet: Arc::new(Mutex::new(wallet)),
		backend: Arc::new(backend),
		json_rpc_client: Arc::new(json_rpc_client),
	})
}

impl<D: bdk::database::BatchDatabase> CoordinatorWallet<D> {
	pub async fn get_new_address(&self) -> Result<String> {
		let wallet = self.wallet.lock().await;
		let address = wallet.get_address(bdk::wallet::AddressIndex::New)?;
		Ok(address.address.to_string())
	}

	// validate bond (check amounts, valid inputs, correct addresses, valid signature, feerate)
	// also check if inputs are confirmed already
	// bdk::blockchain::compact_filters::Mempool::iter_txs() -> Vec(Tx) to check if contained in mempool
	// blockchain::get_tx to get input
	pub async fn validate_bonds(
		&self,
		bonds: &Vec<MonitoringBond>,
	) -> Result<Vec<(MonitoringBond, anyhow::Error)>> {
		let mut invalid_bonds: Vec<(MonitoringBond, anyhow::Error)> = Vec::new();
		let blockchain = &*self.backend;

		{
			let wallet = self.wallet.lock().await;
			for bond in bonds {
				let input_sum: u64;

				let tx: Transaction = deserialize(&hex::decode(&bond.bond_tx_hex)?)?;
				debug!("Validating bond in validate_bonds()");
				// we need to test this with signed and invalid/unsigned transactions
				// checks signatures and inputs
				if let Err(e) = verify_tx(&tx, &*wallet.database(), blockchain) {
					invalid_bonds.push((bond.clone(), anyhow!(e)));
					continue;
				}

				// check if the tx has the correct input amounts (have to be >= trading amount)
				input_sum = match tx.input_sum(blockchain, &*wallet.database()) {
					Ok(amount) => {
						if amount < bond.requirements.min_input_sum_sat {
							invalid_bonds.push((
								bond.clone(),
								anyhow!("Bond input sum too small: {}", amount),
							));
							continue;
						}
						amount
					}
					Err(e) => {
						return Err(anyhow!(e));
					}
				};
				// check if bond output to us is big enough
				match tx.bond_output_sum(&bond.requirements.bond_address) {
					Ok(amount) => {
						if amount < bond.requirements.locking_amount_sat {
							invalid_bonds.push((
								bond.clone(),
								anyhow!("Bond output sum too small: {}", amount),
							));
							continue;
						}
						amount
					}
					Err(e) => {
						return Err(anyhow!(e));
					}
				};
				if ((input_sum - tx.all_output_sum()) / tx.vsize() as u64) < 200 {
					invalid_bonds.push((
						bond.clone(),
						anyhow!(
							"Bond fee rate too low: {}",
							(input_sum - tx.all_output_sum()) / tx.vsize() as u64
						),
					));
					continue;
				}
			}
		}

		let raw_bonds: Vec<String> = bonds
			.iter()
			.map(|bond| bond.bond_tx_hex.clone().raw_hex()) // Assuming `raw_hex()` returns a String or &str
			.collect();
		let test_mempool_accept_res = self
			.json_rpc_client
			.deref()
			.test_mempool_accept(&raw_bonds)?;

		for res in test_mempool_accept_res {
			if !res.allowed {
				let invalid_bond =
					Self::search_monitoring_bond_by_txid(&bonds, &res.txid.to_string())?;
				invalid_bonds.push((
					invalid_bond,
					anyhow!(
						"Bond not accepted by testmempoolaccept: {:?}",
						res.reject_reason
							.unwrap_or("rejected by testmempoolaccept".to_string())
					),
				));
			}
		}

		debug!("validate_bond_tx_hex(): Bond validation done.");
		Ok(invalid_bonds)
	}

	pub fn publish_bond_tx_hex(&self, bond: &str) -> Result<()> {
		warn!("publish_bond_tx_hex(): publishing cheating bond tx!");
		let blockchain = &*self.backend;
		let tx: Transaction = deserialize(&hex::decode(bond)?)?;

		blockchain.broadcast(&tx)?;
		Ok(())
	}

	fn search_monitoring_bond_by_txid(
		monitoring_bonds: &Vec<MonitoringBond>,
		txid: &str,
	) -> Result<MonitoringBond> {
		for bond in monitoring_bonds {
			let bond_tx: Transaction = deserialize(&hex::decode(&bond.bond_tx_hex)?)?;
			if bond_tx.txid().to_string() == txid {
				return Ok(bond.clone());
			}
		}
		Err(anyhow!("Bond not found in monitoring bonds"))
	}
}

impl fmt::Debug for CoordinatorWallet<Tree> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("CoordinatorWallet")
			.field("wallet", &self.wallet)
			// Since ElectrumBlockchain doesn't implement Debug, we can't automatically derive it.
			// Instead, we can print a placeholder or simply omit it from the debug output.
			.field("backend", &"ElectrumBlockchain (Debug not implemented)")
			.finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use bdk::bitcoin::Network;
	use bdk::database::MemoryDatabase;
	use bdk::{blockchain::RpcBlockchain, Wallet};

	async fn new_test_wallet(wallet_xprv: &str) -> CoordinatorWallet<MemoryDatabase> {
		let rpc_config = RpcConfig {
			url: env::var("BITCOIN_RPC_ADDRESS_PORT")?.to_string(),
			auth: Auth::Cookie {
				file: env::var("BITCOIN_RPC_COOKIE_FILE_PATH")?.into(),
			},
			network: bdk::bitcoin::Network::Testnet,
			wallet_name: env::var("BITCOIN_RPC_WALLET_NAME")?,
			sync_params: None,
		};
		let json_rpc_client = Client::new(&rpc_config.url, rpc_config.auth.clone().into())?;
		let backend = RpcBlockchain::from_config(&rpc_config)?;

		let wallet_xprv = ExtendedPrivKey::from_str(wallet_xprv).unwrap();
		let wallet = Wallet::new(
			Bip86(wallet_xprv, KeychainKind::External),
			Some(Bip86(wallet_xprv, KeychainKind::Internal)),
			Network::Testnet,
			MemoryDatabase::new(),
		)
		.unwrap();
		wallet.sync(&backend, SyncOptions::default()).unwrap();

		CoordinatorWallet::<MemoryDatabase> {
			wallet: Arc::new(Mutex::new(wallet)),
			backend: Arc::new(backend),
			json_rpc_client: Arc::new(json_rpc_client),
		}
	}

	#[tokio::test]
	async fn test_transaction_without_signature() {
		let test_wallet = new_test_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").await;
		let bond_without_signature = "02000000010127a9d96655011fca55dc2667f30b98655e46da98d0f84df676b53d7fb380140000000000fdffffff02998d0000000000002251207dd0d1650cdc22537709e35620f3b5cc3249b305bda1209ba4e5e01bc3ad2d8c50c3000000000000225120a12e5d145a4a3ab43f6cc1188435e74f253eace72bd986f1aaf780fd0c6532364f860000";
		let requirements = BondRequirements {
			min_input_sum_sat: 51000,
			locking_amount_sat: 50000,
			bond_address: "tb1p5yh969z6fgatg0mvcyvggd08fujnat8890vcdud277q06rr9xgmqwfdkcx"
				.to_string(),
		};

		let result = test_wallet
			.validate_bond_tx_hex(&bond_without_signature, &requirements)
			.await;
		assert!(result.is_err());
	}

	#[tokio::test]
	async fn test_transaction_with_invalid_signature() {
		let test_wallet = new_test_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").await;
		// assembled bond tx but with the signature of a different bond = invalid
		let bond_with_invalid_signature = "020000000001010127a9d96655011fca55dc2667f30b98655e46da98d0f84df676b53d7fb3801400000000001900000002aa900000000000002251207dd0d1650cdc22537709e35620f3b5cc3249b305bda1209ba4e5e01bc3ad2d8c50c3000000000000225120a12e5d145a4a3ab43f6cc1188435e74f253eace72bd986f1aaf780fd0c65323601401fddcc681a1d0324c8fdeabbc08a3b06c26741872363c0ddfc82f15b6abe43d37815bcdc2ce1fb2f70cac426f7fb269d322ac6a621886208d0c625335bba670800000000";
		let requirements = BondRequirements {
			min_input_sum_sat: 51000,
			locking_amount_sat: 50000,
			bond_address: "tb1p5yh969z6fgatg0mvcyvggd08fujnat8890vcdud277q06rr9xgmqwfdkcx"
				.to_string(),
		};

		let result = test_wallet
			.validate_bond_tx_hex(&bond_with_invalid_signature, &requirements)
			.await;
		assert!(result.is_err());
	}

	#[tokio::test]
	async fn test_bond_with_spent_input() {
		let test_wallet = new_test_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").await;
		let bond_with_spent_input = "02000000000101f7d992795b0b43227ea83e296a7c2a91771ede3ef54f1eb5664393c79b9399080100000000fdffffff0250c3000000000000225120a12e5d145a4a3ab43f6cc1188435e74f253eace72bd986f1aaf780fd0c653236abc6010000000000225120b83c64b440203fb74a0c672cd829f387b957129835dd3b5c4e33fc71a146b3ae0140afdafbae5b76217f469790b211d7fbda427e5b4379c4603e9ae08c9ef5aaae30bfecfc16e5f636c737bea8e0e27974854d1cd0d094ed737aadfc679a974074574f860000";
		let requirements = BondRequirements {
			min_input_sum_sat: 51000,
			locking_amount_sat: 50000,
			bond_address: "tb1p5yh969z6fgatg0mvcyvggd08fujnat8890vcdud277q06rr9xgmqwfdkcx"
				.to_string(),
		};

		let result = test_wallet
			.validate_bond_tx_hex(&bond_with_spent_input, &requirements)
			.await;
		assert!(result.is_err());
	}

	#[tokio::test]
	async fn test_valid_bond_tx() {
		let test_wallet = new_test_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").await;
		let bond = "020000000001010127a9d96655011fca55dc2667f30b98655e46da98d0f84df676b53d7fb380140000000000010000000250c3000000000000225120a12e5d145a4a3ab43f6cc1188435e74f253eace72bd986f1aaf780fd0c653236aa900000000000002251207dd0d1650cdc22537709e35620f3b5cc3249b305bda1209ba4e5e01bc3ad2d8c014010e19c8b915624bd4aa0ba4d094d26ca031a6f2d8f23fe51372c7ea50e05f3caf81c7e139f6fed3e9ffd20c03d79f78542acb3d8aed664898f1c4b2909c2188c00000000";
		let requirements = BondRequirements {
			min_input_sum_sat: 100000,
			locking_amount_sat: 50000,
			bond_address: "tb1p5yh969z6fgatg0mvcyvggd08fujnat8890vcdud277q06rr9xgmqwfdkcx"
				.to_string(),
		};

		let result = test_wallet.validate_bond_tx_hex(&bond, &requirements).await;
		assert!(result.is_ok());
	}

	#[tokio::test]
	async fn test_invalid_bond_tx_low_input_sum() {
		let test_wallet = new_test_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").await;
		let bond = "020000000001010127a9d96655011fca55dc2667f30b98655e46da98d0f84df676b53d7fb380140000000000010000000250c3000000000000225120a12e5d145a4a3ab43f6cc1188435e74f253eace72bd986f1aaf780fd0c653236aa900000000000002251207dd0d1650cdc22537709e35620f3b5cc3249b305bda1209ba4e5e01bc3ad2d8c014010e19c8b915624bd4aa0ba4d094d26ca031a6f2d8f23fe51372c7ea50e05f3caf81c7e139f6fed3e9ffd20c03d79f78542acb3d8aed664898f1c4b2909c2188c00000000";
		let requirements = BondRequirements {
			min_input_sum_sat: 2000000, // Set higher than the actual input sum
			locking_amount_sat: 50000,
			bond_address: "tb1p5yh969z6fgatg0mvcyvggd08fujnat8890vcdud277q06rr9xgmqwfdkcx"
				.to_string(),
		};

		let result = test_wallet.validate_bond_tx_hex(&bond, &requirements).await;
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("Bond input sum too small"));
	}

	#[tokio::test]
	async fn test_invalid_bond_tx_low_output_sum() {
		let test_wallet = new_test_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").await;
		let bond = "020000000001010127a9d96655011fca55dc2667f30b98655e46da98d0f84df676b53d7fb380140000000000010000000250c3000000000000225120a12e5d145a4a3ab43f6cc1188435e74f253eace72bd986f1aaf780fd0c653236aa900000000000002251207dd0d1650cdc22537709e35620f3b5cc3249b305bda1209ba4e5e01bc3ad2d8c014010e19c8b915624bd4aa0ba4d094d26ca031a6f2d8f23fe51372c7ea50e05f3caf81c7e139f6fed3e9ffd20c03d79f78542acb3d8aed664898f1c4b2909c2188c00000000";
		let requirements = BondRequirements {
			min_input_sum_sat: 100000,
			locking_amount_sat: 1000000, // Set higher than the actual output sum
			bond_address: "tb1p5yh969z6fgatg0mvcyvggd08fujnat8890vcdud277q06rr9xgmqwfdkcx"
				.to_string(),
		};

		let result = test_wallet.validate_bond_tx_hex(&bond, &requirements).await;
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("Bond output sum too small"));
	}

	#[tokio::test]
	async fn test_invalid_bond_tx_low_fee_rate() {
		let test_wallet = new_test_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").await;
		let bond = "020000000001010127a9d96655011fca55dc2667f30b98655e46da98d0f84df676b53d7fb380140000000000fdffffff0259b00000000000002251207dd0d1650cdc22537709e35620f3b5cc3249b305bda1209ba4e5e01bc3ad2d8c50c3000000000000225120a12e5d145a4a3ab43f6cc1188435e74f253eace72bd986f1aaf780fd0c6532360140bee11f7f644cf09d5031683203bbe61109090b1e4be4626e13de7a889d6e5d2f154233a2bfaf9cb983f31ccf01b1be5db2cd37bb0cb9a395e2632bc50105b4583f860000";
		let requirements = BondRequirements {
			min_input_sum_sat: 100000,
			locking_amount_sat: 50000,
			bond_address: "tb1p5yh969z6fgatg0mvcyvggd08fujnat8890vcdud277q06rr9xgmqwfdkcx"
				.to_string(),
		};

		let result = test_wallet.validate_bond_tx_hex(&bond, &requirements).await;
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("Bond fee rate too low"));
	}
}
