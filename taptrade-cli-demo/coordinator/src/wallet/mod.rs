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
pub struct CoordinatorWallet {
	pub wallet: Arc<Mutex<Wallet<Tree>>>,
	pub backend: Arc<ElectrumBlockchain>,
}

#[derive(PartialEq, Debug)]
pub struct BondRequirements {
	pub bond_address: String,
	pub locking_amount_sat: u64,
	pub min_input_sum_sat: u64,
}

impl CoordinatorWallet {
	pub fn init() -> Result<Self> {
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

	pub async fn get_new_address(&self) -> Result<String> {
		let wallet = self.wallet.lock().await;
		let address = wallet.get_address(bdk::wallet::AddressIndex::New)?;
		Ok(address.address.to_string())
	}

	// validate bond (check amounts, valid inputs, correct addresses, valid signature, feerate)
	pub async fn validate_bond_tx_hex(
		&self,
		bond: &String,
		requirements: &BondRequirements,
	) -> Result<()> {
		let input_sum: u64;
		let blockchain = &*self.backend;
		let tx: Transaction = deserialize(&hex::decode(bond)?)?;
		{
			let wallet = self.wallet.lock().await;

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
		Ok(())
	}

	pub fn publish_bond_tx_hex(&self, bond: &str) -> Result<()> {
		let blockchain = &*self.backend;
		let tx: Transaction = deserialize(&hex::decode(bond)?)?;

		blockchain.broadcast(&tx)?;
		Ok(())
	}
}

impl fmt::Debug for CoordinatorWallet {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("CoordinatorWallet")
			.field("wallet", &self.wallet)
			// Since ElectrumBlockchain doesn't implement Debug, we can't automatically derive it.
			// Instead, we can print a placeholder or simply omit it from the debug output.
			.field("backend", &"ElectrumBlockchain (Debug not implemented)")
			.finish()
	}
}
