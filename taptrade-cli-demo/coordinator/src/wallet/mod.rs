use super::*;
use anyhow::Context;
use bdk::{
	bitcoin::{self, bip32::ExtendedPrivKey},
	blockchain::ElectrumBlockchain,
	electrum_client::Client,
	template::Bip86,
	KeychainKind, SyncOptions, Wallet,
};
use std::str::FromStr;

pub struct CoordinatorWallet {
	pub wallet: Wallet<MemoryDatabase>,
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
		let wallet = Wallet::new(
			Bip86(wallet_xprv, KeychainKind::External),
			Some(Bip86(wallet_xprv, KeychainKind::Internal)),
			bitcoin::Network::Testnet,
			MemoryDatabase::default(), // non-permanent storage
		)?;

		wallet.sync(&backend, SyncOptions::default())?;
		dbg!("Balance: {} SAT", wallet.get_balance()?);
		Ok(TradingWallet { wallet, backend })
	}
}
