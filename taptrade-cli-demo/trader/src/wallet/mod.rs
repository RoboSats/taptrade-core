pub mod bond;
pub mod musig2;
pub mod wallet_utils;

use crate::cli::TraderSettings;
use anyhow::Result;
use bdk::bitcoin::{bip32::ExtendedPrivKey, Network};
use bdk::blockchain::ElectrumBlockchain;
use bdk::database::MemoryDatabase;
use bdk::electrum_client::Client;
use bdk::{
	bitcoin,
	keys::DescriptorPublicKey,
	miniscript::Descriptor,
	template::{Bip86, DescriptorTemplate},
	KeychainKind, SyncOptions, Wallet,
};
use std::str::FromStr;
use wallet_utils::get_seed;

// pub struct WalletDescriptors {
// 	pub descriptor: Bip86<ExtendedPrivKey>,
// 	pub change_descriptor: Option<Bip86<ExtendedPrivKey>>,
// }

pub fn get_wallet_xprv(xprv_input: Option<String>) -> Result<ExtendedPrivKey> {
	let xprv: ExtendedPrivKey;
	let network: Network = Network::Testnet;

	if let Some(xprv_i) = xprv_input {
		xprv = ExtendedPrivKey::from_str(&xprv_i)?;
	} else {
		xprv = ExtendedPrivKey::new_master(network, &get_seed())?;
		dbg!("Generated xprv: ", xprv.to_string());
	}

	Ok(xprv)
}

pub fn load_wallet(
	trader_config: &TraderSettings,
	blockchain: &ElectrumBlockchain,
) -> Result<Wallet<MemoryDatabase>> {
	let wallet = Wallet::new(
		Bip86(trader_config.wallet_xprv.clone(), KeychainKind::External),
		Some(Bip86(
			trader_config.wallet_xprv.clone(),
			KeychainKind::Internal,
		)),
		bitcoin::Network::Testnet,
		MemoryDatabase::default(), // non-permanent storage
	)?;

	wallet.sync(blockchain, SyncOptions::default())?;
	println!("Balance: {} SAT", wallet.get_balance()?);
	Ok(wallet)
}
