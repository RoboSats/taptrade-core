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
use bitcoin::Address;
use std::collections::BTreeMap;
use std::env;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use log::debug;
use bdk::bitcoin::{PublicKey, ScriptBuf};
use log::info;
// use bdk::miniscript::DummyKey;
use bdk::miniscript::Tap;



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
	debug!("descriptor is {:}", descriptor);

	Ok(descriptor)
}

#[derive(Clone)]
pub struct CoordinatorWallet<D: bdk::database::BatchDatabase> {
	pub wallet: Arc<Mutex<Wallet<D>>>,
	pub backend: Arc<ElectrumBlockchain>,
}

pub fn init_coordinator_wallet(wallet_xprv: &str) -> Result<CoordinatorWallet<sled::Tree>, Box<dyn std::error::Error>> {
	let wallet_xprv = ExtendedPrivKey::from_str(wallet_xprv)?;
	let backend = ElectrumBlockchain::from(Client::new(
		&env::var("ELECTRUM_BACKEND")
			.context("Parsing ELECTRUM_BACKEND from .env failed, is it set?")?,
	)?);
	// let backend = EsploraBlockchain::new(&env::var("ESPLORA_BACKEND")?, 1000);
	let sled_db = sled::open(env::var("BDK_DB_PATH")?)?.open_tree("default_wallet")?;
	let wallet = Wallet::new(
		Bip86(wallet_xprv, KeychainKind::External),
		Some(Bip86(wallet_xprv, KeychainKind::Internal)),
		bdk::bitcoin::Network::Testnet,
		sled_db,
	)?;

	wallet
		.sync(&backend, SyncOptions::default())
		.context("Connection to electrum server failed.")?; // we could also use Esplora to make this async
	dbg!(wallet.get_balance()?);
	Ok(CoordinatorWallet {
		wallet: Arc::new(Mutex::new(wallet)),
		backend: Arc::new(backend),
	})
}
/// the provided keys, and `create_psbt`, which creates a PSBT from the descriptor
/// Figure out how to put UTXO's
// pub async fn create_psbt(descriptor: Descriptor<String>)-> Result<(PartiallySignedTransaction), Box<dyn std::error::Error>> {
pub async fn create_psbt(descriptor: Descriptor<String>){
    let coordinator_wallet= init_coordinator_wallet("xprv9xom13daMHDPvuivoBnceYvuhPHS6EHZZcND9nN3qvnRw8xM8Jrr24KHnARuReaX1G7PAyYxvkqTRdfhjC9MvQFPbQCXfJwiDiEfbFNSWd4");

	// Step 3: Deposit funds
	// Use some testnet faucet, such as https://bitcoinfaucet.uo1.net/send.php
	// https://coinfaucet.eu/en/btc-testnet4/

	// // Step 4: Print balance
	// let blockchain = EsploraBlockchain::new("https://blockstream.info/testnet/api", 20);
	// wallet.sync(&blockchain, SyncOptions::default())?;
	// info!("{:#?}", wallet.get_balance()?);

	// let maker_utxos = vec![/* UTXO details here */];
	// let taker_utxos = vec![/* UTXO details here */];

	// // Recipient address (where funds will be sent)
    //  let faucet_address = Address::from_str("tb1ql7w62elx9ucw4pj5lgw4l028hmuw80sndtntxt")?;

    // // let address = get_address_from_str("tb1ql7w62elx9ucw4pj5lgw4l028hmuw80sndtntxt")?;
    // let address_str = "tb1qqw8ledhkhezru0rwj7acpker8srtcs28sng0d6";
    
    // let mut tx_builder = wallet.build_tx();
    // tx_builder
    //     .add_utxos(&maker_utxos)?
    //     .add_utxos(&taker_utxos)?
    //     .drain_wallet()
	// 	.drain_to(ScriptBuf::from(faucet_address.script_pubkey().to_owned()))
    //     .fee_rate(FeeRate::from_sat_per_vb(3.0))
    //     .policy_path(BTreeMap::new(), KeychainKind::External);

    // let (psbt, tx_details) = tx_builder.finish()?;
    // debug!("PSBT: {:?}", psbt);
    // Ok(psbt)
	
}

// /// The `taker_unresponsive` function handles the case when the taker is unresponsive and
// /// the coordinator needs to sign the PSBT using an alternative path.
// // TODO: Figure out how to use UTXO's
// fn taker_unresponsive(
// 	psbt: PartiallySignedTransaction,
// 	wallet: Wallet<MemoryDatabase>,
// 	maker_utxos: Vec<UTXO>,
// 	taker_utxos: Vec<UTXO>,
// 	recipient_address: Address<NetworkChecked>,
// ) -> Result<(), Box<dyn std::error::Error>> {
// 	// Maker signs the PSBT
// 	let maker_signed_psbt = wallet.sign(&mut psbt.clone(), SignOptions::default())?;
// 	debug!("Maker signed PSBT: {:?}", maker_signed_psbt);

// 	// If taker is unresponsive, coordinator signs using alternative path
// 	let taker_responsive = false; // Assume taker is unresponsive
// 	if !taker_responsive {
// 		let mut path = BTreeMap::new();
// 		path.insert(
// 			wallet.policies(KeychainKind::External)?.unwrap().id,
// 			vec![1],
// 		); // Path for coordinator and maker

// 		let mut coordinator_tx_builder = wallet.build_tx();
// 		coordinator_tx_builder
// 			.add_utxos(&maker_utxos)?
// 			.add_utxos(&taker_utxos)?
// 			.drain_wallet()
// 			.drain_to(recipient_address.script_pubkey())
// 			.fee_rate(FeeRate::from_sat_per_vb(3.0))
// 			.policy_path(path, KeychainKind::External);

// 		let (coordinator_psbt, _details) = coordinator_tx_builder.finish()?;
// 		let coordinator_signed_psbt = wallet.sign(&mut coordinator_psbt, SignOptions::default())?;
// 		debug!("Coordinator signed PSBT: {:?}", coordinator_signed_psbt);
// 	}
// 	Ok(())
// }
// async fn load_wallet()
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

    async fn create_descriptor() -> Result<Descriptor<String>, Box<dyn std::error::Error>>{
    let coordinator_pub = "0209d4277f677aeaeeb6d3da1d66ba0dfabf296bf1609c505ad1f4cf50a870d082";
    let coordinator_xpub = "xpub6C3kuZk67kPgw2evdJ72ckEARaqjwtx62KZY4t4YR6AsqJrsFSnDNm5sh9FkfdHLcXNWgcwAZs2prhNj23xG5Ui1pwyW1mtcGfEtBQdmima";
    let maker_pub = "02fa55532a5ddc036db99412d050d11bf5ce4c78b9816adc3974a3c23e2a876dfe";
    let taker_pub = "0219e6db0b79f8e7ee9c5fa4e77ac77e942ec3248c1a2e94c8d5ea230b13d849f0";

    let result = create_script(&coordinator_pub, maker_pub, taker_pub).await;
    match result {
        Ok(descriptor) => {
            println!("{}", descriptor);
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
    
    async fn create_wallet(){
         // let wallet_xprv = ExtendedPrivKey::from_str(coordinator_xprv,
		// )?;
		// let backend = ElectrumBlockchain::from(bdk::electrum_client::Client::new(
		// 	&env::var("ELECTRUM_BACKEND")
		// 		.context("Parsing ELECTRUM_BACKEND from .env failed, is it set?")?,
		// )?);
		// // let backend = EsploraBlockchain::new(&env::var("ESPLORA_BACKEND")?, 1000);
		// let sled_db = sled::open(env::var("BDK_DB_PATH")?)?.open_tree("default_wallet")?;
		// let wallet = Wallet::new(
		// 	Bip86(wallet_xprv, KeychainKind::External),
		// 	Some(Bip86(wallet_xprv, KeychainKind::Internal)),
		// 	bdk::bitcoin::Network::Testnet,
		// 	sled_db,
		// )?;

		// wallet
		// 	.sync(&backend, SyncOptions::default())
		// 	.context("Connection to electrum server failed.")?; // we could also use Esplora to make this async
		// dbg!(wallet.get_balance()?);

        // let coordinator_wallet = Wallet::new(
		// 	Bip86(coordinator_xprv, KeychainKind::External),
		// 	Some(Bip86(coordinator_xprv, KeychainKind::Internal)),
		// 	bdk::bitcoin::Network::Testnet,
		// 	MemoryDatabase::default(), // non-permanent storage
		// )?;

		// coordinator_wallet.sync(&backend, SyncOptions::default())?;
		// dbg!("Balance: {} SAT", wallet.get_balance()?);

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

    // #[tokio::test]
    // async fn test_create_script() {
    //     let coordinator_key = "03b2f6e8abf3624f8e9b93f7b2567b158c15b0f20ab368f9fcb2d9251d6a788d09";
    //     let maker_key = "020202020202020202020202020202020202020202020202020202020202020202";
    //     let taker_key = "03833be68fb7559c0e62ffdbb6d46cc44a58c19c6ba82e51144b583cff0519c791";

    //     let result = create_script(coordinator_key, maker_key, taker_key).await;
    //     assert!(result.is_ok());
    // }

    // #[tokio::test]
    // async fn test_create_psbt() {
    //     let descriptor_str = "tr(youshouldputyourdescriptorhere)";
    //     let descriptor = Descriptor::from_str(descriptor_str).unwrap();

    //     let result = create_psbt(descriptor).await;
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
