use anyhow::Context;
/// This module contains functions related to creating and broadcasting Taproot transactions.
/// It includes functions to combine and broadcast the partially signed transactions (PSBTs)
/// from multiple participants, create a Taproot script descriptor, create a PSBT from the
/// descriptor, and handle the case when the taker is unresponsive.
use bdk::bitcoin::address::NetworkUnchecked;
use bdk::bitcoin::bip32::ExtendedPrivKey;
use bdk::bitcoin::psbt::PartiallySignedTransaction;
use bdk::bitcoin::secp256k1::Secp256k1;
use bdk::blockchain::{ElectrumBlockchain, EsploraBlockchain};
use bdk::database::MemoryDatabase;
use bdk::descriptor::Descriptor;
use bdk::electrum_client::Client;
use bdk::miniscript::descriptor::TapTree;
use bdk::miniscript::policy::Concrete;
use bdk::miniscript::psbt::PsbtExt;
use bdk::template::Bip86;
use bdk::wallet::AddressIndex;
use bdk::{sled, SignOptions};
use bdk::{FeeRate, KeychainKind, SyncOptions, Wallet};
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use log::debug;
use log::info;
// use bdk::miniscript::DummyKey;
use bdk::miniscript::Tap;
// use crate::coordinator::create_taproot::Network;
use bdk::bitcoin::network::constants::Network;
use bdk::bitcoin::Address;
use serde_json::to_string_pretty;


/// The main function in this module is `combine_and_broadcast`, which combines the PSBTs
/// from the maker and taker, finalizes the transaction, and broadcasts it on the blockchain.
pub async fn combine_and_broadcast() -> Result<(), Box<dyn std::error::Error>> {
    let mut base_psbt = PartiallySignedTransaction::from_str("TODO: insert the psbt created by the coordinator here")?;
    let signed_psbts = vec![
         // TODO: Paste each participant's PSBT here
         "makers_psbt",
         "takers_psbt",
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
	let script_f = format!(
		"and_v(and_v(v:{},v:{}),after(2048))",
		maker_key, taker_key
	);
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

pub fn init_coordinator_wallet(wallet_xprv: &str) -> Result<CoordinatorWallet<sled::Tree>, Box<dyn std::error::Error>> {
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
        },
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
            println!{"{:?}", wallet.get_balance()};
            return Ok(CoordinatorWallet {
                wallet: Arc::new(Mutex::new(wallet)),
                backend: Arc::new(backend),
            });
        },
        Err(e) => {
            println!("Failed to create wallet: {}", e);
            return Err(e.into());
        }
    }
	
}
/// the provided keys, and `create_psbt`, which creates a PSBT from the descriptor
/// Figure out how to put UTXO's
// pub async fn fund_psbt(descriptor: Descriptor<String>)-> Result<(PartiallySignedTransaction), Box<dyn std::error::Error>> {
pub async fn fund_psbt(descriptor: Descriptor<String>)-> Result<(), Box<dyn std::error::Error>>{
    // println!("Hello create_psbt");
    // let coordinator_wallet= init_coordinator_wallet("xprv9xom13daMHDPvuivoBnceYvuhPHS6EHZZcND9nN3qvnRw8xM8Jrr24KHnARuReaX1G7PAyYxvkqTRdfhjC9MvQFPbQCXfJwiDiEfbFNSWd4");
    let wallet_result = Wallet::new(
        "tr(0209d4277f677aeaeeb6d3da1d66ba0dfabf296bf1609c505ad1f4cf50a870d082,{and_v(v:pk(02fa55532a5ddc036db99412d050d11bf5ce4c78b9816adc3974a3c23e2a876dfe),pk(0209d4277f677aeaeeb6d3da1d66ba0dfabf296bf1609c505ad1f4cf50a870d082)),and_v(v:pk(0219e6db0b79f8e7ee9c5fa4e77ac77e942ec3248c1a2e94c8d5ea230b13d849f0),pk(0209d4277f677aeaeeb6d3da1d66ba0dfabf296bf1609c505ad1f4cf50a870d082))})#0du8cgum",
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
) -> Result<(), Box<dyn std::error::Error>> {
	// If taker is unresponsive, coordinator signs using alternative path
	let taker_responsive = false; // Assume taker is unresponsive
	if !taker_responsive {
        let wallet_result = Wallet::new(
            "tr(0209d4277f677aeaeeb6d3da1d66ba0dfabf296bf1609c505ad1f4cf50a870d082,{and_v(v:pk(02fa55532a5ddc036db99412d050d11bf5ce4c78b9816adc3974a3c23e2a876dfe),pk(0209d4277f677aeaeeb6d3da1d66ba0dfabf296bf1609c505ad1f4cf50a870d082)),and_v(v:pk(0219e6db0b79f8e7ee9c5fa4e77ac77e942ec3248c1a2e94c8d5ea230b13d849f0),pk(0209d4277f677aeaeeb6d3da1d66ba0dfabf296bf1609c505ad1f4cf50a870d082))})#0du8cgum",
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
                let address = unchecked_address.require_network(Network::Testnet).map_err(|e| {
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

                
                let mut tx_builder= wallet.build_tx();
                tx_builder
                    .drain_wallet()
                    // .add_recipient((address.script_pubkey()), 10_000)
                    .drain_to(address.script_pubkey())
                    .fee_rate(FeeRate::from_sat_per_vb(3.0))
                    .policy_path(path, KeychainKind::External);

                let (psbt, tx_details) = tx_builder.finish()?;
                // debug!("PSBT: {:?}", psbt);
                // println!("psbt is {:?}", psbt);
                let json = to_string_pretty(&psbt).unwrap();
                println!("psbt is {}", json);

                // Ok(psbt)
                }
            Err(e) => {
                println!("Error creating wallet: {:?}", e);
            }
        }
        }
        Ok(())
    }
/// The `taker_unresponsive` function handles the case when the taker is unresponsive and
/// the coordinator needs to sign the PSBT using an alternative path.
// TODO: Figure out how to use UTXO's
// fn taker_unresponsive(
// 	psbt: PartiallySignedTransaction,
// 	wallet: Wallet<MemoryDatabase>,
// 	// maker_utxos: Vec<UTXO>,
// 	// taker_utxos: Vec<UTXO>,
// 	recipient_address: Address,
// ) -> Result<(), Box<dyn std::error::Error>> {
// 	// If taker is unresponsive, coordinator signs using alternative path
// 	let taker_responsive = false; // Assume taker is unresponsive
// 	if !taker_responsive {
//         let wallet_result = Wallet::new(
//             "tr(0209d4277f677aeaeeb6d3da1d66ba0dfabf296bf1609c505ad1f4cf50a870d082,{and_v(v:pk(02fa55532a5ddc036db99412d050d11bf5ce4c78b9816adc3974a3c23e2a876dfe),pk(0209d4277f677aeaeeb6d3da1d66ba0dfabf296bf1609c505ad1f4cf50a870d082)),and_v(v:pk(0219e6db0b79f8e7ee9c5fa4e77ac77e942ec3248c1a2e94c8d5ea230b13d849f0),pk(0209d4277f677aeaeeb6d3da1d66ba0dfabf296bf1609c505ad1f4cf50a870d082))})#0du8cgum",
//             None,
//             bdk::bitcoin::Network::Testnet,
//             MemoryDatabase::new()
//         );

//         match wallet_result {
//             Ok(wallet) => {
                
//                 }
//             Err(e) => {
//                 println!("Error creating wallet: {:?}", e);
//             }
//         }
// 		 // // Step 2: Add the BDK signer
//         let mut private_key_str = String::new();
//         File::open("key.txt")?.read_to_string(&mut private_key_str)?;
//         println!("{}", private_key_str);
//         let private_key = PrivateKey::from_str(&private_key_str)?;
//         let signer = SignerWrapper::new(private_key, SignerContext::Tap { is_internal_key: false });

//         wallet.add_signer(
//             KeychainKind::External,
//             SignerOrdering(0),
//             Arc::new(signer)
//         );

//         // // Step 3: Sign the transaction
//         let mut psbt = PartiallySignedTransaction::from_str("TODO: paste the PSBT obtained in step 3 here")?;
//         let finalized = wallet.sign(&mut psbt, SignOptions::default());
//         println!("{}", psbt);
// 	}
// 	Ok(())
// }
#[cfg(test)]
mod tests {
    use super::*;
    use bdk::{bitcoin::bip32::ExtendedPrivKey, descriptor};
    use bitcoin::consensus::deserialize;
    use bdk::blockchain::ElectrumBlockchain;
    use bdk::template::Bip86;
    use std::env;
    use anyhow::{Context, Error};
    use bdk::sled;

    #[tokio::test]
    async fn test_fund_psbt()-> Result<(), Error> {
        let result = create_descriptor().await;
        match &result{
            Ok(descriptor) => {
                // println!("{}", descriptor);
                let _ = fund_psbt(descriptor.clone()).await;
                let _ = taker_unresponsive_psbt("tb1qqw8ledhkhezru0rwj7acpker8srtcs28sng0d6");

            },
            Err(e) => println!("Error: {}", e),
        }
        assert!(result.is_ok());
        Ok(())
    }

    async fn create_descriptor() -> Result<Descriptor<String>, Box<dyn std::error::Error>>{
    let coordinator_pub = "0209d4277f677aeaeeb6d3da1d66ba0dfabf296bf1609c505ad1f4cf50a870d082";
    let coordinator_xpub = "xpub6C3kuZk67kPgw2evdJ72ckEARaqjwtx62KZY4t4YR6AsqJrsFSnDNm5sh9FkfdHLcXNWgcwAZs2prhNj23xG5Ui1pwyW1mtcGfEtBQdmima";
    let maker_pub = "02fa55532a5ddc036db99412d050d11bf5ce4c78b9816adc3974a3c23e2a876dfe";
    let taker_pub = "0219e6db0b79f8e7ee9c5fa4e77ac77e942ec3248c1a2e94c8d5ea230b13d849f0";

    let result = create_script(&coordinator_pub, maker_pub, taker_pub).await;
    match result {
        Ok(descriptor) => {
            // println!("{}", descriptor);
            Ok(descriptor)
        },
        Err(e) => {
            println!("Error: {}", e);
            Err(e)
        },
    }
}
// https://github.com/danielabrozzoni/multisigs_and_carrots/tree/master
    #[tokio::test]
    async fn test_create_script()-> Result<(), Error>{
        // Taking public key using https://iancoleman.io/bip39/ that generates addresses and respective public key by the seed phrase of wallet (Using sparrow wallet)
        
        let result = create_descriptor().await;
        match &result{
            Ok(descriptor) => {
                println!("{}", descriptor);
            },
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

    

    // #[test]
    // fn test_taker_unresponsive() {
    //     let psbt_hex = "020000000001010000000000000000000000000000000000000000000000000000000000000000ffffffff040000000000000000ffffffff0100f2052a01000000160014e8e7a7e7e7e7e7e7e7e7e7e7e7e7e7e7e7e7e7e00000000";
    //     let psbt_bytes = Vec::<u8>::from_hex(psbt_hex).unwrap();
    //     let psbt: BdkPsbt = deserialize(&psbt_bytes).unwrap();

    //     let wallet = Wallet::new(
    //         "tr(youshouldputyourdescriptorhere)",
    //         None,
    //         bdk::bitcoin::Network::Testnet,
    //         MemoryDatabase::new(),
    //     )
    //     .unwrap();

    //     let maker_utxos = vec![];
    //     let taker_utxos = vec![];

    //     let recipient_address = Address::from_str("tb1ql7w62elx9ucw4pj5lgw4l028hmuw80sndtntxt").unwrap();

    //     let result = taker_unresponsive(psbt, wallet, maker_utxos, taker_utxos, recipient_address);
    //     assert!(result.is_ok());
    // }
}
