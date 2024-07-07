/// This module contains functions related to creating and broadcasting Taproot transactions.
/// It includes functions to combine and broadcast the partially signed transactions (PSBTs)
/// from multiple participants, create a Taproot script descriptor, create a PSBT from the
/// descriptor, and handle the case when the taker is unresponsive.
use bdk::bitcoin::address::NetworkUnchecked;
use bdk::bitcoin::psbt::PartiallySignedTransaction;
use bdk::bitcoin::secp256k1::Secp256k1;
use bdk::blockchain::EsploraBlockchain;
use bdk::database::MemoryDatabase;
use bdk::descriptor::Descriptor;
use bdk::miniscript::descriptor::TapTree;
use bdk::miniscript::policy::Concrete;
use bdk::miniscript::psbt::PsbtExt;
use bdk::wallet::AddressIndex;
use bdk::SignOptions;
use bdk::{FeeRate, KeychainKind, SyncOptions, Wallet};
use bitcoin::address::NetworkChecked;
use bitcoin::Address;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;

/// The main function in this module is `combine_and_broadcast`, which combines the PSBTs
/// from the maker and taker, finalizes the transaction, and broadcasts it on the blockchain.
pub async fn combine_and_broadcast() -> Result<(), Box<dyn std::error::Error>> {
	let mut base_psbt =
		PartiallySignedTransaction::from_str("TODO: insert the psbt created in step 3 here")?;
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
	// let maker_key = "020202020202020202020202020202020202020202020202020202020202020202";
	// let taker_key = "03833be68fb7559c0e62ffdbb6d46cc44a58c19c6ba82e51144b583cff0519c791";
	// let coordinator_key = "03b2f6e8abf3624f8e9b93f7b2567b158c15b0f20ab368f9fcb2d9251d6a788d09";

	// Define policies based on the scripts provided
	let script_a = format!(
		"and(and(after(escrow_timer),pk({})),pk({}))",
		maker_key, coordinator_key
	);
	let script_b = format!(
		"and_v(v:pk({}),and_v(v:pk({}),pk({})))",
		maker_key, taker_key, coordinator_key
	);
	let script_c = format!("and(pk({}),pk({}))", maker_key, coordinator_key);
	let script_d = format!("and(pk({}),pk({}))", taker_key, coordinator_key);
	let script_e = format!("and(pk({}),after(very_long_timelock))", maker_key);
	let script_f = format!(
		"and_v(and_v(v:pk({}),v:pk({})),after(2048))",
		maker_key, taker_key
	);

	// Compile the policies
	let compiled_a = Concrete::<String>::from_str(&script_a)?.compile()?;
	let compiled_b = Concrete::<String>::from_str(&script_b)?.compile()?;
	let compiled_c = Concrete::<String>::from_str(&script_c)?.compile()?;
	let compiled_d = Concrete::<String>::from_str(&script_d)?.compile()?;
	let compiled_e = Concrete::<String>::from_str(&script_e)?.compile()?;
	let compiled_f = Concrete::<String>::from_str(&script_f)?.compile()?;

	// Create TapTree leaves
	let tap_leaf_a = TapTree::Leaf(Arc::new(compiled_a));
	let tap_leaf_b = TapTree::Leaf(Arc::new(compiled_b));
	let tap_leaf_c = TapTree::Leaf(Arc::new(compiled_c));
	let tap_leaf_d = TapTree::Leaf(Arc::new(compiled_d));
	let tap_leaf_e = TapTree::Leaf(Arc::new(compiled_e));
	let tap_leaf_f = TapTree::Leaf(Arc::new(compiled_f));

	// Create the TapTree (example combining leaves, adjust as necessary)
	let tap_tree = TapTree::Tree(Arc::new(tap_leaf_a), Arc::new(tap_leaf_b));

	// Define a dummy internal key (replace with an actual key)
	let dummy_internal_key =
		"020202020202020202020202020202020202020202020202020202020202020202".to_string();

	// Create the descriptor
	let descriptor = Descriptor::new_tr(dummy_internal_key, Some(tap_tree))?;
	println!("{}", descriptor);

	Ok(descriptor)
}

/// the provided keys, and `create_psbt`, which creates a PSBT from the descriptor
/// Figure out how to put UTXO's
pub async fn create_psbt(
	descriptor: Descriptor<String>,
) -> Result<(PartiallySignedTransaction), Box<dyn std::error::Error>> {
	// Step 1: Create a BDK wallet
	let wallet = Wallet::new(
		// TODO: insert your descriptor here
		"tr(youshouldputyourdescriptorhere)",
		None,
		bdk::bitcoin::Network::Testnet,
		MemoryDatabase::new(),
	)?;

	// Step 2: Print the first address
	println!(
		"Deposit funds here: {:?}",
		wallet.get_address(AddressIndex::New)?
	);

	// Step 3: Deposit funds
	// Use some testnet faucet, such as https://bitcoinfaucet.uo1.net/send.php
	// https://coinfaucet.eu/en/btc-testnet4/

	// Step 4: Print balance
	let blockchain = EsploraBlockchain::new("https://blockstream.info/testnet/api", 20);
	wallet.sync(&blockchain, SyncOptions::default())?;
	println!("{:#?}", wallet.get_balance()?);

	let maker_utxos = vec![/* UTXO details here */];
	let taker_utxos = vec![/* UTXO details here */];

	//TODO: Change type to NetworkChecked
	// Recipient address (where funds will be sent)
	let recipient_address = Address::from_str("tb1ql7w62elx9ucw4pj5lgw4l028hmuw80sndtntxt")?;

	// Build the PSBT
	let mut tx_builder = wallet.build_tx();
	tx_builder
		.add_utxos(&maker_utxos)?
		.add_utxos(&taker_utxos)?
		.drain_wallet()
		.drain_to(recipient_address.script_pubkey())
		.fee_rate(FeeRate::from_sat_per_vb(3.0))
		.policy_path(BTreeMap::new(), KeychainKind::External);

	let (psbt, tx_details) = tx_builder.finish()?;
	println!("PSBT: {:?}", psbt);
	Ok(psbt)
}

/// The `taker_unresponsive` function handles the case when the taker is unresponsive and
/// the coordinator needs to sign the PSBT using an alternative path.
// TODO: Figure out how to use UTXO's
fn taker_unresponsive(
	psbt: PartiallySignedTransaction,
	wallet: Wallet<MemoryDatabase>,
	maker_utxos: Vec<UTXO>,
	taker_utxos: Vec<UTXO>,
	recipient_address: Address<NetworkChecked>,
) -> Result<(), Box<dyn std::error::Error>> {
	// Maker signs the PSBT
	let maker_signed_psbt = wallet.sign(&mut psbt.clone(), SignOptions::default())?;
	println!("Maker signed PSBT: {:?}", maker_signed_psbt);

	// If taker is unresponsive, coordinator signs using alternative path
	let taker_responsive = false; // Assume taker is unresponsive
	if !taker_responsive {
		let mut path = BTreeMap::new();
		path.insert(
			wallet.policies(KeychainKind::External)?.unwrap().id,
			vec![1],
		); // Path for coordinator and maker

		let mut coordinator_tx_builder = wallet.build_tx();
		coordinator_tx_builder
			.add_utxos(&maker_utxos)?
			.add_utxos(&taker_utxos)?
			.drain_wallet()
			.drain_to(recipient_address.script_pubkey())
			.fee_rate(FeeRate::from_sat_per_vb(3.0))
			.policy_path(path, KeychainKind::External);

		let (coordinator_psbt, _details) = coordinator_tx_builder.finish()?;
		let coordinator_signed_psbt = wallet.sign(&mut coordinator_psbt, SignOptions::default())?;
		println!("Coordinator signed PSBT: {:?}", coordinator_signed_psbt);
	}
	Ok(())
}
