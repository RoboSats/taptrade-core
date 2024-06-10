pub mod bond;

use bdk::{bitcoin, keys::DescriptorPublicKey, miniscript::Descriptor, template::{Bip86, DescriptorTemplate}, KeychainKind, SyncOptions, Wallet};
use bdk::database::MemoryDatabase;
use bdk::blockchain::ElectrumBlockchain;
use bdk::bitcoin::{Network, secp256k1::rand::{self, RngCore}, bip32::ExtendedPrivKey};
use bdk::electrum_client::Client;
use anyhow::Result;
use std::str::FromStr;
use crate::cli::TraderSettings;

// https://github.com/bitcoindevkit/book-of-bdk

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
		let mut seed: [u8; 32] = [0u8; 32];
		rand::thread_rng().fill_bytes(&mut seed);  // verify this is secure randomness!
		xprv = ExtendedPrivKey::new_master(network, &seed)?;
		dbg!("Generated xprv: ", xprv.to_string());
	}

	Ok(xprv)
}

pub fn load_wallet(trader_config: &TraderSettings) -> Result<Wallet<MemoryDatabase>> {
	let client = Client::new(&trader_config.electrum_endpoint)?;
	let blockchain = ElectrumBlockchain::from(client);

	let wallet = Wallet::new(
		Bip86(trader_config.wallet_xprv.clone(), KeychainKind::External),
		Some(Bip86(trader_config.wallet_xprv.clone(), KeychainKind::Internal)),
        bitcoin::Network::Testnet,
        MemoryDatabase::default(),  // non-permanent storage
    )?;

    wallet.sync(&blockchain, SyncOptions::default())?;
    println!("Descriptor balance: {} SAT", wallet.get_balance()?);
    Ok(wallet)
}
