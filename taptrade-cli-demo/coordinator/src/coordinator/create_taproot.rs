use anyhow::Context;
/// This module contains functions related to creating and broadcasting Taproot transactions.
/// It includes functions to combine and broadcast the partially signed transactions (PSBTs)
/// from multiple participants, create a Taproot script descriptor, create a PSBT from the
/// descriptor, and handle the case when the taker is unresponsive.
use bdk::bitcoin::address::NetworkUnchecked;
use bdk::bitcoin::bip32::{ChildNumber, DerivationPath, ExtendedPrivKey};
use bdk::bitcoin::psbt::PartiallySignedTransaction;
use bdk::bitcoin::secp256k1::Secp256k1;
use bdk::bitcoin::{base64, PrivateKey};
use bdk::blockchain::{ElectrumBlockchain, EsploraBlockchain};
use bdk::database::MemoryDatabase;
use bdk::descriptor::Descriptor;
use bdk::electrum_client::Client;
use bdk::miniscript::descriptor::TapTree;
use bdk::miniscript::policy::Concrete;
use bdk::miniscript::psbt::PsbtExt;
use bdk::template::Bip86;
use bdk::wallet::signer::{SignerContext, SignerOrdering, SignerWrapper};
use bdk::wallet::AddressIndex;
use bdk::{sled, SignOptions};
use bdk::{FeeRate, KeychainKind, SyncOptions, Wallet};
use log::debug;
use log::info;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
// use bdk::miniscript::DummyKey;
use bdk::miniscript::Tap;
// use crate::coordinator::create_taproot::Network;
use bdk::bitcoin::consensus::deserialize;
use bdk::bitcoin::network::constants::Network;
use bdk::bitcoin::Address;
use serde_json::to_string_pretty;

/// Other functions include `create_script`, which creates a Taproot script descriptor from
async fn create_script(
	coordinator_key: &str,
	maker_key: &str,
	taker_key: &str,
) -> Result<(bdk::descriptor::Descriptor<std::string::String>), Box<dyn std::error::Error>> {
	// Define policies based on the scripts provided
	let script_a = format!(
		"and(and(after(escrow_timer),{}),{})",
		maker_key, coordinator_key
	);
	let script_b = format!(
		"and_v(v:{},and_v(v:{},{}))",
		maker_key, taker_key, coordinator_key
	);
	let script_c: String = format!("and(pk({}),pk({}))", maker_key, coordinator_key);
	let script_d = format!("and(pk({}),pk({}))", taker_key, coordinator_key);
	let script_e = format!("and({},after(very_long_timelock))", maker_key);
	let script_f = format!("and_v(and_v(v:{},v:{}),after(2048))", maker_key, taker_key);
	// Compile the policies
	// let compiled_a = Concrete::<String>::from_str(&script_a)?.compile::<Tap>()?;
	// let compiled_b = Concrete::<String>::from_str(&script_b)?.compile()?;
	let compiled_c = Concrete::<String>::from_str(&script_c)?.compile::<Tap>()?;
	let compiled_d = Concrete::<String>::from_str(&script_d)?.compile::<Tap>()?;
	// let compiled_e = Concrete::<String>::from_str(&script_e)?.compile()?;
	// let compiled_f = Concrete::<String>::from_str(&script_f)?.compile()?;

	// Create TapTree leaves
	// let tap_leaf_a = TapTree::Leaf(Arc::new(compiled_a));
	// let tap_leaf_b = TapTree::Leaf(Arc::new(compiled_b));
	let tap_leaf_c = TapTree::Leaf(Arc::new(compiled_c));
	let tap_leaf_d = TapTree::Leaf(Arc::new(compiled_d));
	// let tap_leaf_e = TapTree::Leaf(Arc::new(compiled_e));
	// let tap_leaf_f = TapTree::Leaf(Arc::new(compiled_f));

	// Create the TapTree (example combining leaves, adjust as necessary), will be used for Script Path Spending (Alternative Spending Paths) in the descriptor
	let tap_tree = TapTree::Tree(Arc::new(tap_leaf_c), Arc::new(tap_leaf_d));

	// An internal key, that defines the way to spend the transaction directly, using Key Path Spending
	let dummy_internal_key = coordinator_key.to_string();

	// Create the descriptor
	let descriptor = Descriptor::new_tr(dummy_internal_key, Some(tap_tree))?;
	// debug!("descriptor is {:}", descriptor);

	Ok(descriptor)
}

#[derive(Clone)]
pub struct CoordinatorWallet<D: bdk::database::BatchDatabase> {
	pub wallet: Arc<Mutex<Wallet<D>>>,
	pub backend: Arc<ElectrumBlockchain>,
}

/// the provided keys, and `create_psbt`, which creates a PSBT from the descriptor
/// Figure out how to put UTXO's
// pub async fn fund_psbt(descriptor: Descriptor<String>)-> Result<(PartiallySignedTransaction), Box<dyn std::error::Error>> {
pub async fn fund_psbt(descriptor: Descriptor<String>) -> Result<(), Box<dyn std::error::Error>> {
	// println!("Hello create_psbt");
	// let coordinator_wallet= init_coordinator_wallet("xprv9xom13daMHDPvuivoBnceYvuhPHS6EHZZcND9nN3qvnRw8xM8Jrr24KHnARuReaX1G7PAyYxvkqTRdfhjC9MvQFPbQCXfJwiDiEfbFNSWd4");
	let wallet_result = Wallet::new(
        "tr(tpubDDD58oLVWb7GbbvS4q8DW1wnTgJD1ZNS1p9wgeb1G3zDqTEohjGWbeDcmJNc2S1TJ1FW6hLYG1odBJwp1BhvzGQLhHKjWSHm6E6NzvvLjiA,{and_v(v:pk(tpubDDk7P4EjBobe6VbQnW15wCtEm4sfDHiS4Yg3tCS9LQ55x8duoTvELtRJCc4rjdsDeq6FMTW8Sjdy34PC43dY93E2wEn2TytrM8D6FhLtqg2),pk(tpubDDD58oLVWb7GbbvS4q8DW1wnTgJD1ZNS1p9wgeb1G3zDqTEohjGWbeDcmJNc2S1TJ1FW6hLYG1odBJwp1BhvzGQLhHKjWSHm6E6NzvvLjiA)),and_v(v:pk(tpubDDpD87xvUwFdswsQTXsjkBPWXThYZABDsGP9JbzDatobQzBn2pvPrP4B7MBLTHmTdpx1zuTM4TjB5X31WrLvexsvV4ayGStKF4mxa4gct6y),pk(tpubDDD58oLVWb7GbbvS4q8DW1wnTgJD1ZNS1p9wgeb1G3zDqTEohjGWbeDcmJNc2S1TJ1FW6hLYG1odBJwp1BhvzGQLhHKjWSHm6E6NzvvLjiA))})#mpnyppnp",
        None,
        bdk::bitcoin::Network::Testnet,
        MemoryDatabase::new()
    );

	match wallet_result {
		Ok(wallet) => {
			let electrum_url = "ssl://electrum.blockstream.info:60002";
			let client = Client::new(electrum_url)?;
			let blockchain = ElectrumBlockchain::from(client);

			// Sync the wallet with the blockchain
			wallet.sync(&blockchain, Default::default())?;

			let new_address = wallet.get_address(AddressIndex::New).unwrap();
			println!("New wallet receiving address: {}", new_address);
			// New wallet receiving address: tb1pwqvyjf2sl4znw4w8naajgl4utaxezkr06gynvzjkuesplw28qk4q4a9hl7
			// Fetch and print the wallet balance
			match wallet.get_balance() {
				Ok(balance) => {
					println!("Wallet balance: {}", balance);
				}
				Err(e) => {
					println!("Error fetching wallet balance: {:?}", e);
				}
			}
		}
		Err(e) => {
			println!("Error creating wallet: {:?}", e);
		}
	}
	// // Step 3: Deposit funds
	// // Use some testnet faucet, such as https://bitcoinfaucet.uo1.net/send.php
	// // https://coinfaucet.eu/en/btc-testnet4/
	Ok(())
}

fn taker_unresponsive_psbt(
	taker_address: &str,
) -> Result<PartiallySignedTransaction, Box<dyn std::error::Error>> {
	// If taker is unresponsive, coordinator signs using alternative path
	let taker_responsive = false; // Assume taker is unresponsive
	if !taker_responsive {
		let wallet_result = Wallet::new(
			"tr(tpubDDD58oLVWb7GbbvS4q8DW1wnTgJD1ZNS1p9wgeb1G3zDqTEohjGWbeDcmJNc2S1TJ1FW6hLYG1odBJwp1BhvzGQLhHKjWSHm6E6NzvvLjiA,{and_v(v:pk(tpubDDk7P4EjBobe6VbQnW15wCtEm4sfDHiS4Yg3tCS9LQ55x8duoTvELtRJCc4rjdsDeq6FMTW8Sjdy34PC43dY93E2wEn2TytrM8D6FhLtqg2),pk(tpubDDD58oLVWb7GbbvS4q8DW1wnTgJD1ZNS1p9wgeb1G3zDqTEohjGWbeDcmJNc2S1TJ1FW6hLYG1odBJwp1BhvzGQLhHKjWSHm6E6NzvvLjiA)),and_v(v:pk(tpubDDpD87xvUwFdswsQTXsjkBPWXThYZABDsGP9JbzDatobQzBn2pvPrP4B7MBLTHmTdpx1zuTM4TjB5X31WrLvexsvV4ayGStKF4mxa4gct6y),pk(tpubDDD58oLVWb7GbbvS4q8DW1wnTgJD1ZNS1p9wgeb1G3zDqTEohjGWbeDcmJNc2S1TJ1FW6hLYG1odBJwp1BhvzGQLhHKjWSHm6E6NzvvLjiA))})#mpnyppnp",
            None,
            bdk::bitcoin::Network::Testnet,
            MemoryDatabase::new()
        );

		match wallet_result {
			Ok(wallet) => {
				let electrum_url = "ssl://electrum.blockstream.info:60002";
				let client = Client::new(electrum_url)?;
				let blockchain = ElectrumBlockchain::from(client);

				// Sync the wallet with the blockchain
				wallet.sync(&blockchain, Default::default())?;
				// Recipient address (where funds will be sent)
				match wallet.get_balance() {
					Ok(balance) => {
						println!("Wallet balance: {}", balance);
					}
					Err(e) => {
						println!("Error fetching wallet balance: {:?}", e);
					}
				}
				// let unchecked_address = Address::from_str("tb1ql7w62elx9ucw4pj5lgw4l028hmuw80sndtntxt")?;
				let unchecked_address = Address::from_str(taker_address).map_err(|e| {
					println!("Error parsing address: {:?}", e);
					e
				})?;

				// Ensure the address is valid for the correct network (testnet in this case)
				let address = unchecked_address
					.require_network(Network::Testnet)
					.map_err(|e| {
						println!("Error validating network: {:?}", e);
						e
					})?;

				// We need to specify with which policy funds will be spent. Our current wallet contains 3
				// policies: the key path spend, and two leaves in the script path spend.
				let wallet_policy = wallet.policies(KeychainKind::External)?.unwrap();
				let mut path = BTreeMap::new();
				// We need to use the first leaf of the script path spend, hence the second policy
				// If you're not sure what's happening here, no worries, this is bit tricky :)
				// You can learn more here: https://docs.rs/bdk/latest/bdk/wallet/tx_builder/struct.TxBuilder.html#method.policy_path
				path.insert(wallet_policy.id, vec![1]);

				let mut tx_builder = wallet.build_tx();
				tx_builder
					.drain_wallet()
					// .add_recipient((address.script_pubkey()), 10_000)
					.drain_to(address.script_pubkey())
					.fee_rate(FeeRate::from_sat_per_vb(3.0))
					.policy_path(path, KeychainKind::External);

				let (psbt, _tx_details) = tx_builder.finish()?;
				// debug!("PSBT: {:?}", psbt);
				println!("psbt is main {:?}", psbt);
				let json = to_string_pretty(&psbt).unwrap();
				// println!("psbt is {}", json);

				Ok(psbt)
			}
			Err(e) => {
				println!("Error creating wallet: {:?}", e);
				Err(Box::new(e)) // Convert bdk::Error to Box<dyn std::error::Error>
			}
		}
	} else {
		Err("Taker is responsive, no need to sign using alternative path".into())
	}
}

pub fn init_wallet(
	wallet_xprv: &str,
) -> Result<CoordinatorWallet<sled::Tree>, Box<dyn std::error::Error>> {
	println!("Hello init_coordinator_wallet");
	let wallet_xprv = ExtendedPrivKey::from_str(wallet_xprv)?;
	//  let backend = ElectrumBlockchain::from(Client::new(
	// 	&env::var("ELECTRUM_BACKEND")
	// 		.context("Parsing ELECTRUM_BACKEND from .env failed, is it set?")?,
	// )?);
	// println!("ELECTRUM_BACKEND: {:?}", backend);
	let electrum_backend = "ssl://mempool.space:40002";
	let client = match Client::new(&electrum_backend) {
		Ok(c) => {
			println!("Electrum client created");
			c
		}
		Err(e) => {
			println!("Failed to create Electrum client: {}", e);
			return Err(e.into());
		}
	};

	let backend = ElectrumBlockchain::from(client);
	println!("Electrum blockchain backend created");

	// let backend = EsploraBlockchain::new(&env::var("ESPLORA_BACKEND")?, 1000);
	let sled_db = sled::open("./dbs/bdk-wallet")?.open_tree("default_wallet")?;
	println!("HELLO???");
	// let wallet = Wallet::new(
	// 	Bip86(wallet_xprv, KeychainKind::External),
	// 	Some(Bip86(wallet_xprv, KeychainKind::Internal)),
	// 	bdk::bitcoin::Network::Testnet,
	// 	sled_db,
	// )?;
	let wallet_result = Wallet::new(
		Bip86(wallet_xprv, KeychainKind::External),
		Some(Bip86(wallet_xprv, KeychainKind::Internal)),
		bdk::bitcoin::Network::Testnet,
		sled_db,
	);

	match wallet_result {
		Ok(wallet) => {
			println!("Wallet created successfully");
			wallet
				.sync(&backend, SyncOptions::default())
				.context("Connection to electrum server failed.")?; // we could also use Esplora to make this async
			dbg!(wallet.get_balance()?);
			println! {"{:?}", wallet.get_balance()};
			Ok(CoordinatorWallet {
				wallet: Arc::new(Mutex::new(wallet)),
				backend: Arc::new(backend),
			})
		}
		Err(e) => {
			println!("Failed to create wallet: {}", e);
			Err(e.into())
		}
	}
}

/// The `taker_unresponsive` function handles the case when the taker is unresponsive and
/// the coordinator needs to sign the PSBT using an alternative path.
fn taker_unresponsive(
	mut psbt: PartiallySignedTransaction,
) -> Result<(), Box<dyn std::error::Error>> {
	println!("here");
	// If taker is unresponsive, coordinator signs using alternative path
	let taker_responsive = false; // Assume taker is unresponsive
	if !taker_responsive {
		let wallet_result = Wallet::new(
			"tr(tpubDDD58oLVWb7GbbvS4q8DW1wnTgJD1ZNS1p9wgeb1G3zDqTEohjGWbeDcmJNc2S1TJ1FW6hLYG1odBJwp1BhvzGQLhHKjWSHm6E6NzvvLjiA,{and_v(v:pk(tpubDDk7P4EjBobe6VbQnW15wCtEm4sfDHiS4Yg3tCS9LQ55x8duoTvELtRJCc4rjdsDeq6FMTW8Sjdy34PC43dY93E2wEn2TytrM8D6FhLtqg2),pk(tpubDDD58oLVWb7GbbvS4q8DW1wnTgJD1ZNS1p9wgeb1G3zDqTEohjGWbeDcmJNc2S1TJ1FW6hLYG1odBJwp1BhvzGQLhHKjWSHm6E6NzvvLjiA)),and_v(v:pk(tpubDDpD87xvUwFdswsQTXsjkBPWXThYZABDsGP9JbzDatobQzBn2pvPrP4B7MBLTHmTdpx1zuTM4TjB5X31WrLvexsvV4ayGStKF4mxa4gct6y),pk(tpubDDD58oLVWb7GbbvS4q8DW1wnTgJD1ZNS1p9wgeb1G3zDqTEohjGWbeDcmJNc2S1TJ1FW6hLYG1odBJwp1BhvzGQLhHKjWSHm6E6NzvvLjiA))})#mpnyppnp",
			None,
            bdk::bitcoin::Network::Testnet,
            MemoryDatabase::new()
        );

		match wallet_result {
			Ok(mut wallet) => {
				let electrum_url = "ssl://electrum.blockstream.info:60002";
				let client = Client::new(electrum_url)?;
				let blockchain = ElectrumBlockchain::from(client);

				// Sync the wallet with the blockchain
				wallet.sync(&blockchain, Default::default())?;
				// Recipient address (where funds will be sent)
				match wallet.get_balance() {
					Ok(balance) => {
						println!("Wallet balance: {}", balance);
					}
					Err(e) => {
						println!("Error fetching wallet balance: {:?}", e);
					}
				}
				println!("here");

				// // Step 2: Add the BDK signer
				// let mut private_key_str = String::new();
				// File::open("key.txt")?.read_to_string(&mut private_key_str)?;
				// println!("{}", private_key_str);
				//  taker xprv
				let xprv_str = "tprv8inPcmXmKnA9ZA9TURpvvva3i7JDsKEmcXAcAkccVPJtur4p8dgFCLsyYoGe6oNiKoKLyRvJTAUXHUbHC2i7LHFg6dFCLfvrfqa5k1ajTGQ";
				let secp = Secp256k1::new();

				// Parse the xprv key
				let xprv = ExtendedPrivKey::from_str(xprv_str)?;

				// Derive the private key (you can adjust the derivation path as needed)
				let derivation_path = DerivationPath::from(vec![ChildNumber::from(0)]);
				let derived_priv_key = xprv.derive_priv(&secp, &derivation_path)?;

				// Convert the derived key to a PrivateKey
				let private_key = PrivateKey::new(derived_priv_key.private_key, Network::Bitcoin);

				// Ensure the private key is correctly derived
				println!("Private key: {:?}", private_key);

				// let private_key = PrivateKey::from_str(&private)?;
				let signer = SignerWrapper::new(
					private_key,
					SignerContext::Tap {
						is_internal_key: false,
					},
				);

				println!("here");

				wallet.add_signer(
					KeychainKind::External,
					SignerOrdering(0), 
					Arc::new(signer)
				);

				// // Step 3: Sign the transaction
				// let mut psbt = PartiallySignedTransaction::from_str("TODO: paste the PSBT obtained in step 3 here")?;
				let finalized = wallet.sign(&mut psbt, SignOptions::default());
				println!("taker unresponsive psbt is: {}", psbt);
			}
			Err(e) => {
				println!("Error creating wallet: {:?}", e);
			}
		}
	}
	Ok(())
}

/// The main function in this module is `combine_and_broadcast`, which combines the PSBTs
/// from the maker and taker, finalizes the transaction, and broadcasts it on the blockchain.
pub async fn combine_and_broadcast() -> Result<(), Box<dyn std::error::Error>> {
	let mut base_psbt = PartiallySignedTransaction::from_str(
		"TODO: insert the psbt created by the coordinator here",
	)?;
	let signed_psbts = vec![
         // TODO: Paste each participant's PSBT here
         "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",
    ];

	for psbt in signed_psbts {
		let psbt = PartiallySignedTransaction::from_str(psbt)?;
		base_psbt.combine(psbt)?;
	}

	let secp = Secp256k1::new();
	let psbt = base_psbt.finalize(&secp).unwrap();
	let finalized_tx = psbt.extract_tx();
	dbg!(finalized_tx.txid());

	let blockchain = EsploraBlockchain::new("https://blockstream.info/testnet/api", 20);
	dbg!(blockchain.broadcast(&finalized_tx));
	Ok(())
}

fn coordinator_sign(
	mut psbt: PartiallySignedTransaction,
) -> Result<(), Box<dyn std::error::Error>> {
	println!("here in coordinator_sign");

	let wallet_result = Wallet::new(
		"tr(tpubDDD58oLVWb7GbbvS4q8DW1wnTgJD1ZNS1p9wgeb1G3zDqTEohjGWbeDcmJNc2S1TJ1FW6hLYG1odBJwp1BhvzGQLhHKjWSHm6E6NzvvLjiA,{and_v(v:pk(tpubDDk7P4EjBobe6VbQnW15wCtEm4sfDHiS4Yg3tCS9LQ55x8duoTvELtRJCc4rjdsDeq6FMTW8Sjdy34PC43dY93E2wEn2TytrM8D6FhLtqg2),pk(tpubDDD58oLVWb7GbbvS4q8DW1wnTgJD1ZNS1p9wgeb1G3zDqTEohjGWbeDcmJNc2S1TJ1FW6hLYG1odBJwp1BhvzGQLhHKjWSHm6E6NzvvLjiA)),and_v(v:pk(tpubDDpD87xvUwFdswsQTXsjkBPWXThYZABDsGP9JbzDatobQzBn2pvPrP4B7MBLTHmTdpx1zuTM4TjB5X31WrLvexsvV4ayGStKF4mxa4gct6y),pk(tpubDDD58oLVWb7GbbvS4q8DW1wnTgJD1ZNS1p9wgeb1G3zDqTEohjGWbeDcmJNc2S1TJ1FW6hLYG1odBJwp1BhvzGQLhHKjWSHm6E6NzvvLjiA))})#mpnyppnp",
        None,
        bdk::bitcoin::Network::Testnet,
        MemoryDatabase::new()
    );

	match wallet_result {
		Ok(mut wallet) => {
			let electrum_url = "ssl://electrum.blockstream.info:60002";
			let client = Client::new(electrum_url)?;
			let blockchain = ElectrumBlockchain::from(client);

			// Sync the wallet with the blockchain
			wallet.sync(&blockchain, Default::default())?;
			// Recipient address (where funds will be sent)
			match wallet.get_balance() {
				Ok(balance) => {
					println!("Wallet balance: {}", balance);
				}
				Err(e) => {
					println!("Error fetching wallet balance: {:?}", e);
				}
			}
			// // Step 2: Add the BDK signer
			// let mut private_key_str = String::new();
			// File::open("key.txt")?.read_to_string(&mut private_key_str)?;
			// println!("{}", private_key_str);
			let xprv_str = "tprv8iseuSeeGfkpgpV1kK7HBMXFPUp8j8Repu7pGqp6S1fn58snqXUYsNjwVXxgFVN2wt8mtdHcmyjTQKD4F34k3ATozjm8QA66xLUBstpJVKH";
			let secp = Secp256k1::new();

			// Parse the xprv key
			let xprv = ExtendedPrivKey::from_str(xprv_str)?;
			println!("Parsed xprv: {:?}", xprv);
			println!("Base58 xprv: {:?}", xprv.to_string());

			// Derive the private key (you can adjust the derivation path as needed)
			let derivation_path = DerivationPath::from(vec![ChildNumber::from(0)]);
			let derived_priv_key = xprv.derive_priv(&secp, &derivation_path)?;
			println!("Derived private key: {:?}", derived_priv_key);
			println!(
				"Base58 derived_priv_key: {:?}",
				derived_priv_key.to_string()
			);

			// Convert the derived key to a PrivateKey
			let private_key = PrivateKey::new(derived_priv_key.private_key, Network::Bitcoin);

			// let private_key = PrivateKey::from_str(&private)?;
			let signer = SignerWrapper::new(
				private_key,
				SignerContext::Tap {
					is_internal_key: false,
				},
			);

			wallet.add_signer(
				KeychainKind::External, 
				SignerOrdering(0), 
				Arc::new(signer)
			);
			// Print the PSBT before signing
			println!("PSBT before signing: {:?}", psbt);
			for (i, input) in (0_u32..).zip(psbt.inputs.iter()) {
				println!("Input: {:?}", input);
				let input_script = input.witness_utxo.as_ref().unwrap().script_pubkey.clone();
				let address = wallet.get_address(bdk::wallet::AddressIndex::Peek(i))?;
				if input_script != address.script_pubkey() {
					println!("Input {} does not correspond to the wallet", i);
				}
			}

			// // Step 3: Sign the transaction
			// let mut psbt = PartiallySignedTransaction::from_str("TODO: paste the PSBT obtained in step 3 here")?;
			let finalized = wallet.sign(&mut psbt, SignOptions::default());
			match finalized {
				Ok(_) => {
					println!("Successfully signed PSBT.");
					println!("Final PSBT: {:?}", psbt);
				},
				Err(e) => {
					println!("Error signing PSBT: {:?}", e);
				}
			}
			
		}
		Err(e) => {
			println!("Error creating wallet: {:?}", e);
		}
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use crate::coordinator;

use super::*;
	use anyhow::{Context, Error};
	// use bdk::blockchain::ElectrumBlockchain;
	// use bdk::sled;
	// use bdk::template::Bip86;
	// use bdk::{bitcoin::bip32::ExtendedPrivKey, descriptor};
	// use bitcoin::consensus::deserialize;
	// use std::env;

	#[tokio::test]
	async fn test_fund_psbt() -> Result<(), Error> {
		let result = create_descriptor().await;
		match &result {
			Ok(descriptor) => {
				println!("descriptor is: {}", descriptor);
				let _ = fund_psbt(descriptor.clone()).await;
				let psbt = taker_unresponsive_psbt("tb1qqw8ledhkhezru0rwj7acpker8srtcs28sng0d6");
				match &psbt {
					Ok(psbt) => {
						println!("psbt is {:?}", psbt.clone());
						let _ = taker_unresponsive(psbt.clone());
						let _ = coordinator_sign(psbt.clone());
					}
					Err(e) => {
						println!("Error: {}", e)
					}
				}
			}
			Err(e) => {
				println!("Error: {}", e)
			}
		}
		assert!(result.is_ok());
		Ok(())
	}

	async fn create_descriptor() -> Result<Descriptor<String>, Box<dyn std::error::Error>> {
		let _coordinator_pub = "03f294ab32537a49f7dc07b49249b4add8cad82cd1f7b7b4a9db6fb0947dc7b755";
		let _maker_pub = "0349ba2df1a252ec57a53d329edc076045ec621334471b19dd54a91054451e2fce";
		let _taker_pub = "023bb7fc7bf8482fa7a509b24af286a83ce3316036ef97408cdc575d18df387166";

		let _coordinator_xpub= "xpub6FC4XcsChmV3JVKxBmnnNqqzdPEQu57Vra8JCooFWNiHALTrPhT3uRgyReYQuCUTutYNB7X9wSsTuLgYQvSbXFwEfJAefM3msGexe4V8rRz";
		let _coordinator_xprv= "xprvA2Ci87LJsPvk61FV5kFn1huG5MPvVcPeVMChQRPdx3BJHY8hrA8oMdNVaMo2F7yiaSbztXfrcd9ewTfK7pioE7CDU6YpjoN43EimS76xhqB";
		let _coordinator_tpub= "tpubDDD58oLVWb7GbbvS4q8DW1wnTgJD1ZNS1p9wgeb1G3zDqTEohjGWbeDcmJNc2S1TJ1FW6hLYG1odBJwp1BhvzGQLhHKjWSHm6E6NzvvLjiA";
		let _coordinator_tprv= "tprv8iseuSeeGfkpgpV1kK7HBMXFPUp8j8Repu7pGqp6S1fn58snqXUYsNjwVXxgFVN2wt8mtdHcmyjTQKD4F34k3ATozjm8QA66xLUBstpJVKH";

		let _taker_xpub= "xpub6F9EN4dLoTNy5SS3y47TmcY8Y1aDizAkEukEwPEL23NiybeBodokP8gkCWeqvVjaAdn5F56nRtsbg9Jv59NawfgwoD1yypW9WezwZL6YnLn";
		let _taker_xprv= "xprvA29sxZ6Sy5pfrxMas2aTQUbPyyjjKXStsgpe8zpiThqk6oK3G6VVqLNGMCLsYJ817KvxU6wZmttcJ98QgvAgEbbSZdnaVABxjWHXHM29jnU";
		let _taker_tpub= "tpubDDpD87xvUwFdswsQTXsjkBPWXThYZABDsGP9JbzDatobQzBn2pvPrP4B7MBLTHmTdpx1zuTM4TjB5X31WrLvexsvV4ayGStKF4mxa4gct6y";
		let _taker_tprv= "tprv8ippjtQnNMekTmb7XbRxa8DPJ79wZ3UuDEjm1RFAwgLDtQ48FTqFM5jiGNWXYfWKUmTjUCZKwFUQkzg9p8Wd3es36Gzt9Wv1ec2wj2nNUCD";

		let _maker_xpub= "xpub6F6oEwkKkstNApzPutWS8Qtnx1iW3FvceCB66ibmZkMQ13esgoekEPq1UuH3wmGdhtQrCGeP7HxkbpuK9WDWWCKTMow321MsH3fhJjUMnAk";
		let _maker_xprv= "xprvA27SqSDRvWL4xLuvoryRmGx4Pyt1doCmGyFVJLCA1QpR8FKj9GLVgbWXdd6z6RzPxMnZyLJYHotipd3Y4pNAXDz5Zz2tgKCokjpfJGnvFBU";
		let _maker_tpub= "tpubDDk7P4EjBobe6VbQnW15wCtEm4sfDHiS4Yg3tCS9LQ55x8duoTvELtRJCc4rjdsDeq6FMTW8Sjdy34PC43dY93E2wEn2TytrM8D6FhLtqg2";
		let _maker_tprv= "tprv8inPcmXmKnA9ZA9TURpvvva3i7JDsKEmcXAcAkccVPJtur4p8dgFCLsyYoGe6oNiKoKLyRvJTAUXHUbHC2i7LHFg6dFCLfvrfqa5k1ajTGQ";

		let result = create_script(_coordinator_tpub, _maker_tpub, _taker_tpub).await;
		match result {
			Ok(descriptor) => {
				// println!("{}", descriptor);
				Ok(descriptor)
			}
			Err(e) => {
				println!("Error: {}", e);
				Err(e)
			}
		}
	}
	// https://github.com/danielabrozzoni/multisigs_and_carrots/tree/master
	#[tokio::test]
	async fn test_create_script() -> Result<(), Error> {
		// Taking public key using https://iancoleman.io/bip39/ that generates addresses and respective public key by the seed phrase of wallet (Using sparrow wallet)

		let result = create_descriptor().await;
		match &result {
			Ok(descriptor) => {
				println!("{}", descriptor);
			}
			Err(e) => println!("Error: {}", e),
		}
		assert!(result.is_ok());
		Ok(())
		// tr(0209d4277f677aeaeeb6d3da1d66ba0dfabf296bf1609c505ad1f4cf50a870d082,{and_v(v:pk(02fa55532a5ddc036db99412d050d11bf5ce4c78b9816adc3974a3c23e2a876dfe),pk(0209d4277f677aeaeeb6d3da1d66ba0dfabf296bf1609c505ad1f4cf50a870d082)),and_v(v:pk(0219e6db0b79f8e7ee9c5fa4e77ac77e942ec3248c1a2e94c8d5ea230b13d849f0),pk(0209d4277f677aeaeeb6d3da1d66ba0dfabf296bf1609c505ad1f4cf50a870d082))})#0du8cgum
	}

	// #[tokio::test]
	// async fn test_combine_and_broadcast() {
	//     // Create a base PSBT
	//     let base_psbt_hex = "020000000001010000000000000000000000000000000000000000000000000000000000000000ffffffff040000000000000000ffffffff0100f2052a01000000160014e8e7a7e7e7e7e7e7e7e7e7e7e7e7e7e7e7e7e7e00000000";
	//     let base_psbt_bytes = Vec::<u8>::from_hex(base_psbt_hex).unwrap();
	//     let base_psbt: PartiallySignedTransaction = deserialize(&base_psbt_bytes).unwrap();

	//     // Create signed PSBTs
	//     let maker_psbt_hex = "020000000001010000000000000000000000000000000000000000000000000000000000000000ffffffff040000000000000000ffffffff0100f2052a01000000160014e8e7a7e7e7e7e7e7e7e7e7e7e7e7e7e7e7e7e7e00000000";
	//     let maker_psbt_bytes = Vec::<u8>::from_hex(maker_psbt_hex).unwrap();
	//     let maker_psbt: PartiallySignedTransaction = deserialize(&maker_psbt_bytes).unwrap();

	//     let taker_psbt_hex = "020000000001010000000000000000000000000000000000000000000000000000000000000000ffffffff040000000000000000ffffffff0100f2052a01000000160014e8e7a7e7e7e7e7e7e7e7e7e7e7e7e7e7e7e7e7e00000000";
	//     let taker_psbt_bytes = Vec::<u8>::from_hex(taker_psbt_hex).unwrap();
	//     let taker_psbt: PartiallySignedTransaction = deserialize(&taker_psbt_bytes).unwrap();

	//     // Combine the PSBTs
	//     let mut combined_psbt = base_psbt.clone();
	//     combined_psbt.combine(maker_psbt).unwrap();
	//     combined_psbt.combine(taker_psbt).unwrap();

	//     // Finalize the transaction
	//     let secp = Secp256k1::new();
	//     let finalized_psbt = combined_psbt.finalize(&secp).unwrap();
	//     let finalized_tx = finalized_psbt.extract_tx();

	//     // Broadcast the transaction
	//     let blockchain = EsploraBlockchain::new("https://blockstream.info/testnet/api", 20);
	//     let result = blockchain.broadcast(&finalized_tx).await;
	//     assert!(result.is_ok());
	// }
}
