mod utils;
// pub mod verify_tx;

use super::*;
use anyhow::Context;
use bdk::{
	bitcoin::{self, bip32::ExtendedPrivKey, consensus::encode::deserialize, Transaction},
	blockchain::{Blockchain, ElectrumBlockchain},
	electrum_client::client::Client,
	sled::{self, Tree},
	template::Bip86,
	wallet::verify::*,
	KeychainKind, SyncOptions, Wallet,
};
use std::fmt;
use std::str::FromStr;
use utils::*;
// use verify_tx::*;

#[derive(Clone)]
pub struct CoordinatorWallet<D: bdk::database::BatchDatabase> {
	pub wallet: Arc<Mutex<Wallet<D>>>,
	pub backend: Arc<ElectrumBlockchain>,
}

#[derive(PartialEq, Debug)]
pub struct BondRequirements {
	pub bond_address: String,
	pub locking_amount_sat: u64,
	pub min_input_sum_sat: u64,
}

pub fn init_coordinator_wallet() -> Result<CoordinatorWallet<sled::Tree>> {
	let wallet_xprv = ExtendedPrivKey::from_str(
		&env::var("WALLET_XPRV").context("loading WALLET_XPRV from .env failed")?,
	)?;
	let backend = ElectrumBlockchain::from(Client::new(
		&env::var("ELECTRUM_BACKEND")
			.context("Parsing ELECTRUM_BACKEND from .env failed, is it set?")?,
	)?);
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
	pub async fn validate_bond_tx_hex(
		&self,
		bond: &String,
		requirements: &BondRequirements,
	) -> Result<()> {
		let input_sum: u64;
		let blockchain = &*self.backend;
		let tx: Transaction = deserialize(&hex::decode(bond)?)?;
		{
			debug!("Called validate_bond_tx_hex()");
			let wallet = self.wallet.lock().await;
			if let Err(e) = wallet.sync(blockchain, SyncOptions::default()) {
				error!("Error syncing wallet: {:?}", e);
				return Ok(()); // if the electrum server goes down all bonds will be considered valid. Maybe redundancy should be added.
			};
			// we need to test this with signed and invalid/unsigned transactions
			// checks signatures and inputs
			if let Err(e) = verify_tx(&tx, &*wallet.database(), blockchain) {
				return Err(anyhow!(e));
			}

			// check if the tx has the correct input amounts (have to be >= trading amount)
			input_sum = match tx.input_sum(blockchain, &*wallet.database()) {
				Ok(amount) => {
					if amount < requirements.min_input_sum_sat {
						return Err(anyhow!("Bond input sum too small"));
					}
					amount
				}
				Err(e) => {
					return Err(anyhow!(e));
				}
			};
		}
		// check if bond output to us is big enough
		let output_sum = match tx.bond_output_sum(&requirements.bond_address) {
			Ok(amount) => {
				if amount < requirements.locking_amount_sat {
					return Err(anyhow!("Bond output sum too small"));
				}
				amount
			}
			Err(e) => {
				return Err(anyhow!(e));
			}
		};

		if ((input_sum - output_sum) / tx.vsize() as u64) < 200 {
			return Err(anyhow!("Bond fee rate too low"));
		}
		debug!("validate_bond_tx_hex(): Bond validation successful.");
		Ok(())
	}

	pub fn publish_bond_tx_hex(&self, bond: &str) -> Result<()> {
		warn!("publish_bond_tx_hex(): publishing cheating bond tx!");
		let blockchain = &*self.backend;
		let tx: Transaction = deserialize(&hex::decode(bond)?)?;

		blockchain.broadcast(&tx)?;
		Ok(())
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
	use bdk::bitcoin::{Address, Network};
	use bdk::database::MemoryDatabase;
	use bdk::{blockchain::ElectrumBlockchain, Wallet};
	// use tokio::test;
	// use bitcoincore_rpc_json::GetRawTransactionResult;

	async fn new_wallet(wallet_xprv: &str) -> CoordinatorWallet<MemoryDatabase> {
		let backend = ElectrumBlockchain::from(Client::new("ssl://mempool.space:40002").unwrap());

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
		}
	}

	#[tokio::test]
	async fn test_valid_bond_tx() {
		let test_wallet = new_wallet("xprv9s21ZrQH143K2XqaJ5boFeHgrJTsMgfzrgrsFXdk3UBYtLLhUkCj2QKPmqYpC92zd6bv46Nh8QxXmjH2MwJWVLQzfC6Bv1Tbeoz28nXjeM2").await;
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
		let test_wallet = TestWallet::new().await;
		let bond = "020000000001010000000000000000000000000000000000000000000000000000000000000000ffffffff0502c5080101ffffffff0200f2052a010000001976a914d0c59903c5bac2868760e90fd521a4665aa7652088ac0000000000000000266a24aa21a9ede2f61c3f71d1defd3fa999dfa36953755c690689799962b48bebd836974e8cf90120000000000000000000000000000000000000000000000000000000000000000000000000";
		let requirements = BondRequirements {
			min_input_sum_sat: 1000000, // Set higher than the actual input sum
			locking_amount_sat: 50000,
			bond_address: Address::from_str("tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx").unwrap(),
		};

		test_wallet
			.backend
			.expect_get_tx()
			.returning(|_| Ok(Some(Transaction::default())));

		let result = test_wallet.validate_bond_tx_hex(&bond, &requirements).await;
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("Bond input sum too small"));
	}

	#[tokio::test]
	async fn test_invalid_bond_tx_low_output_sum() {
		let test_wallet = TestWallet::new().await;
		let bond = "020000000001010000000000000000000000000000000000000000000000000000000000000000ffffffff0502c5080101ffffffff0200f2052a010000001976a914d0c59903c5bac2868760e90fd521a4665aa7652088ac0000000000000000266a24aa21a9ede2f61c3f71d1defd3fa999dfa36953755c690689799962b48bebd836974e8cf90120000000000000000000000000000000000000000000000000000000000000000000000000";
		let requirements = BondRequirements {
			min_input_sum_sat: 100000,
			locking_amount_sat: 1000000, // Set higher than the actual output sum
			bond_address: Address::from_str("tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx").unwrap(),
		};

		test_wallet
			.backend
			.expect_get_tx()
			.returning(|_| Ok(Some(Transaction::default())));

		let result = test_wallet.validate_bond_tx_hex(&bond, &requirements).await;
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("Bond output sum too small"));
	}

	#[tokio::test]
	async fn test_invalid_bond_tx_low_fee_rate() {
		let test_wallet = TestWallet::new().await;
		let bond = "020000000001010000000000000000000000000000000000000000000000000000000000000000ffffffff0502c5080101ffffffff0200f2052a010000001976a914d0c59903c5bac2868760e90fd521a4665aa7652088ac0000000000000000266a24aa21a9ede2f61c3f71d1defd3fa999dfa36953755c690689799962b48bebd836974e8cf90120000000000000000000000000000000000000000000000000000000000000000000000000";
		let requirements = BondRequirements {
			min_input_sum_sat: 100000,
			locking_amount_sat: 50000,
			bond_address: Address::from_str("tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx").unwrap(),
		};

		test_wallet
			.backend
			.expect_get_tx()
			.returning(|_| Ok(Some(Transaction::default())));

		// Modify the transaction to have a very low fee
		let mut tx: Transaction = deserialize(&hex::decode(bond).unwrap()).unwrap();
		tx.output[0].value = tx
			.input_sum(
				&*test_wallet.backend,
				&*test_wallet.wallet.lock().await.database(),
			)
			.unwrap() - 1;

		let low_fee_bond = hex::encode(serialize(&tx));

		let result = test_wallet
			.validate_bond_tx_hex(&low_fee_bond, &requirements)
			.await;
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("Bond fee rate too low"));
	}
}
