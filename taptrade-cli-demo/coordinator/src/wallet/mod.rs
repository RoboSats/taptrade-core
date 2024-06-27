use super::*;
use anyhow::Context;
use bdk::{
	bitcoin::{self, bip32::ExtendedPrivKey, consensus::encode::deserialize, Transaction},
	blockchain::ElectrumBlockchain,
	electrum_client::Client,
	sled::{self, Tree},
	template::Bip86,
	wallet::verify::*,
	KeychainKind, SyncOptions, Wallet,
};
use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct CoordinatorWallet {
	pub wallet: Arc<Mutex<Wallet<Tree>>>,
	// database: Arc<Mutex<Tree>>,
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
			// database: Arc::new(Mutex::new(sled_db)),
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
		requirements: &BondRequirementResponse,
	) -> Result<bool> {
		let tx: Transaction = deserialize(&hex::decode(bond)?)?;
		let wallet = self.wallet.lock().await;

		// we need to test this with signed and invalid/unsigned transactions
		// let result = verify_tx(&tx, wallet.database(), blockchain);

		// let valid = tx.verify_tx();
		panic!("Not implemented");
		Ok(true)
	}
}