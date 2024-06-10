pub mod bond;
pub mod wallet_utils;
pub mod musig2;

use bdk::{bitcoin, keys::DescriptorPublicKey, miniscript::Descriptor, template::{Bip86, DescriptorTemplate}, KeychainKind, SyncOptions, Wallet};
use bdk::database::MemoryDatabase;
use bdk::blockchain::ElectrumBlockchain;
use bdk::bitcoin::{Network, bip32::ExtendedPrivKey};
use bdk::electrum_client::Client;
use anyhow::Result;
use wallet_utils::get_seed;
use std::str::FromStr;
use crate::cli::TraderSettings;


pub struct WalletDescriptors {
	pub descriptor: Bip86<ExtendedPrivKey>,
	pub change_descriptor: Option<Bip86<ExtendedPrivKey>>
}

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

pub fn load_wallet(trader_config: &TraderSettings, blockchain: &ElectrumBlockchain) -> Result<Wallet<MemoryDatabase>> {
	let wallet = Wallet::new(
		Bip86(trader_config.wallet_xprv.clone(), KeychainKind::External),
		Some(Bip86(trader_config.wallet_xprv.clone(), KeychainKind::Internal)),
        bitcoin::Network::Testnet,
        MemoryDatabase::default(),  // non-permanent storage
    )?;

    wallet.sync(blockchain, SyncOptions::default())?;
    println!("Descriptor balance: {} SAT", wallet.get_balance()?);
    Ok(wallet)
}
