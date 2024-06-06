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

#[derive(Debug)]
pub struct WalletDescriptors {
	pub descriptor: Descriptor<DescriptorPublicKey>,
	pub change_descriptor: Option<Descriptor<DescriptorPublicKey>>
}

pub fn generate_descriptor_wallet(xprv_input: Option<String>) -> Result<WalletDescriptors> {
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

    let (descriptor, key_map, _) = Bip86(xprv, KeychainKind::External).build(network).unwrap();
    let (change_descriptor, change_key_map, _) = Bip86(xprv, KeychainKind::Internal).build(network)?;
	let descriptors = WalletDescriptors {
		descriptor,
		change_descriptor: Some(change_descriptor)
	};
	Ok(descriptors)
}

pub fn load_wallet(trader_config: &TraderSettings) -> Result<Wallet<MemoryDatabase>> {
	let client = Client::new(&trader_config.electrum_endpoint)?;
	let blockchain = ElectrumBlockchain::from(client);

	let wallet = Wallet::new(
		trader_config.funded_wallet_descriptor.descriptor.clone(),
		trader_config.funded_wallet_descriptor.change_descriptor.clone(),
        bitcoin::Network::Testnet,
        MemoryDatabase::default(),  // non-permanent storage
    )?;

    wallet.sync(&blockchain, SyncOptions::default())?;
    println!("Descriptor balance: {} SAT", wallet.get_balance()?);
    Ok(wallet)
}
