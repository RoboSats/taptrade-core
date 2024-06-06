mod bond;

use bdk::{Wallet, KeychainKind, SyncOptions, bitcoin, template::{Bip86, DescriptorTemplate}};
use bdk::database::MemoryDatabase;
use bdk::blockchain::ElectrumBlockchain;
use bdk::bitcoin::{Network, secp256k1::rand::{self, RngCore}, bip32::ExtendedPrivKey};
use bdk::electrum_client::Client;
use anyhow::Result;
use crate::cli::TraderSettings;

// https://github.com/bitcoindevkit/book-of-bdk

#[derive(Debug)]
pub struct WalletDescriptors {
	pub descriptor: String,
	pub change_descriptor: Option<String>
}

pub fn generate_descriptor_wallet() -> Result<WalletDescriptors> {
	let mut seed: [u8; 32] = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);  // verify this is secure randomness!

    let network: Network = Network::Testnet;
    let xprv: ExtendedPrivKey = ExtendedPrivKey::new_master(network, &seed)?;
    let (descriptor, key_map, _) = Bip86(xprv, KeychainKind::External).build(network).unwrap();
    let (change_descriptor, change_key_map, _) = Bip86(xprv, KeychainKind::Internal).build(network)?;
	let descriptors = WalletDescriptors {
		descriptor: descriptor.to_string_with_secret(&key_map),
		change_descriptor: Some(change_descriptor.to_string_with_secret(&change_key_map)),
	};
	dbg!("Generated wallet descriptors: ", &descriptors);
	Ok(descriptors)
}

pub fn load_wallet(trader_config: &TraderSettings) -> Result<()> {
	// let client = Client::new(&trader_config.electrum_endpoint)?;


	// let blockchain = ElectrumBlockchain::from(client);
    // let wallet = Wallet::new(
	// 	trader_config.funded_wallet_descriptor.as_str(),
	// 	None,
    //     bitcoin::Network::Testnet,
    //     MemoryDatabase::default(),  // non-permanent storage
    // )?;
	// panic!("yeet");
    // wallet.sync(&blockchain, SyncOptions::default())?;

    // println!("Descriptor balance: {} SAT", wallet.get_balance()?);
	// panic!("yeet");
    Ok(())
}
