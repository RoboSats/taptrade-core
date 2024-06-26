use super::*;
use anyhow::Context;
use bdk::{
	bitcoin::{self, bip32::ExtendedPrivKey},
	blockchain::ElectrumBlockchain,
	database::any::SledDbConfiguration,
	electrum_client::Client,
	sled,
	template::Bip86,
	KeychainKind, SyncOptions, Wallet,
};
use std::str::FromStr;

pub struct CoordinatorWallet {
	pub wallet: Wallet<SledDbConfiguration>,
}

impl CoordinatorWallet {
	pub async fn init() -> Result<Self> {
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

		wallet.sync(&backend, SyncOptions::default()).await?;
		dbg!("BDK Wallet loaded, Balance: {} SAT", wallet.get_balance()?);
		Ok(CoordinatorWallet { wallet })
	}
}
