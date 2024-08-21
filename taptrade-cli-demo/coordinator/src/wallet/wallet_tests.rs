use std::time::Duration;

use super::escrow_psbt::*;
use super::*;
use bdk::bitcoin::secp256k1::XOnlyPublicKey;
use bdk::miniscript::ToPublicKey;
use bdk::{
	bitcoin::{psbt::Input, Network},
	blockchain::RpcBlockchain,
	database::MemoryDatabase,
	miniscript::{policy::Concrete, Descriptor, Tap},
	Wallet,
};
use bitcoin;
use bitcoin::consensus::Decodable;

fn get_backend() -> RpcBlockchain {
	dotenv().ok();
	let test_descriptor = "tr(f00949d6dd1ce99a03f88a1a4f59117d553b0da51728bb7fd5b98fbf541337fb,{{and_v(v:pk(4987f3de20a9b1fa6f76c6758934953a8d615e415f1a656f0f6563694b53107d),pk(8f808f457423ff5e4e20a36d317ce9426f9da2fde875e74e15a04481b94bec06)),and_v(v:pk(f1f1db08126af105974cde6021096525ed390cf9b7cde5fedb17a0b16ed31151),pk(8f808f457423ff5e4e20a36d317ce9426f9da2fde875e74e15a04481b94bec06))},{and_v(v:and_v(v:pk(4987f3de20a9b1fa6f76c6758934953a8d615e415f1a656f0f6563694b53107d),pk(f1f1db08126af105974cde6021096525ed390cf9b7cde5fedb17a0b16ed31151)),after(2048)),and_v(v:pk(4987f3de20a9b1fa6f76c6758934953a8d615e415f1a656f0f6563694b53107d),after(12228))}})#0edq24m2";
	let secp_context = secp256k1::Secp256k1::new();
	let rpc_config = RpcConfig {
		url: env::var("BITCOIN_RPC_ADDRESS_PORT").unwrap().to_string(),
		auth: Auth::UserPass {
			username: env::var("BITCOIN_RPC_USER").unwrap(),
			password: env::var("BITCOIN_RPC_PASSWORD").unwrap(),
		},
		network: Network::Regtest,
		// wallet_name: env::var("BITCOIN_RPC_WALLET_NAME")?,
		wallet_name: bdk::wallet::wallet_name_from_descriptor(
			test_descriptor,
			None,
			Network::Regtest,
			&secp_context,
		)
		.unwrap(),
		sync_params: None,
	};
	RpcBlockchain::from_config(&rpc_config).unwrap()
}

async fn new_test_wallet(wallet_xprv: &str) -> CoordinatorWallet<MemoryDatabase> {
	dotenv().ok();
	let wallet_xprv = ExtendedPrivKey::from_str(wallet_xprv).unwrap();
	let secp_context = secp256k1::Secp256k1::new();
	let rpc_config = RpcConfig {
		url: env::var("BITCOIN_RPC_ADDRESS_PORT").unwrap().to_string(),
		auth: Auth::UserPass {
			username: env::var("BITCOIN_RPC_USER").unwrap(),
			password: env::var("BITCOIN_RPC_PASSWORD").unwrap(),
		},
		network: Network::Regtest,
		// wallet_name: env::var("BITCOIN_RPC_WALLET_NAME")?,
		wallet_name: bdk::wallet::wallet_name_from_descriptor(
			Bip86(wallet_xprv, KeychainKind::External),
			Some(Bip86(wallet_xprv, KeychainKind::Internal)),
			Network::Testnet,
			&secp_context,
		)
		.unwrap(),
		sync_params: None,
	};
	let json_rpc_client =
		Arc::new(Client::new(&rpc_config.url, rpc_config.auth.clone().into()).unwrap());
	let backend = RpcBlockchain::from_config(&rpc_config).unwrap();

	let wallet = Wallet::new(
		Bip86(wallet_xprv, KeychainKind::External),
		Some(Bip86(wallet_xprv, KeychainKind::Internal)),
		Network::Testnet,
		MemoryDatabase::new(),
	)
	.unwrap();
	wallet.sync(&backend, SyncOptions::default()).unwrap();
	tokio::time::sleep(Duration::from_secs(10)).await; // fetch the mempool
	CoordinatorWallet::<MemoryDatabase> {
		wallet: Arc::new(Mutex::new(wallet)),
		backend: Arc::new(backend),
		json_rpc_client: Arc::clone(&json_rpc_client),
		mempool: Arc::new(MempoolHandler::new(json_rpc_client).await),
		coordinator_feerate: env::var("COORDINATOR_FEERATE").unwrap().parse().unwrap(),
	}
}

async fn get_escrow_psbt_inputs(
	coordinator_wallet: &CoordinatorWallet<MemoryDatabase>,
	mut amount_sat: i64,
) -> Result<Vec<PsbtInput>> {
	let wallet = coordinator_wallet.wallet.lock().await;
	let mut inputs: Vec<PsbtInput> = Vec::new();

	wallet.sync(&coordinator_wallet.backend, SyncOptions::default())?;
	let available_utxos = wallet.list_unspent()?;

	// could use more advanced coin selection if neccessary
	for utxo in available_utxos {
		let psbt_input: Input = wallet.get_psbt_input(utxo.clone(), None, false)?;
		let input = PsbtInput {
			psbt_input,
			utxo: utxo.outpoint,
		};
		inputs.push(input);
		amount_sat -= utxo.txout.value as i64;
		if amount_sat <= 0 {
			break;
		}
	}
	Ok(inputs)
}

async fn get_dummy_escrow_psbt_data(
	maker_wallet: &CoordinatorWallet<MemoryDatabase>,
	taker_wallet: &CoordinatorWallet<MemoryDatabase>,
) -> (EscrowPsbtConstructionData, EscrowPsbtConstructionData) {
	let maker_inputs = get_escrow_psbt_inputs(maker_wallet, 50000).await.unwrap();
	let taker_inputs = get_escrow_psbt_inputs(taker_wallet, 50000).await.unwrap();
	let maker_escrow_data = EscrowPsbtConstructionData {
		taproot_xonly_pubkey_hex:
			"b709f64da734e04e35b129a65a7fae361cad8a9458d1abc4f0b45b7661a42fca".to_string(),
		musig_pubkey_compressed_hex:
			"02d8e204cdaebec4c5a637311072c865858dc4f142b3848b8e6dde4143476535b5".to_string(),
		change_address: Address::from_str(
			"bcrt1pmcgt8wjuxlkp2pqykatt4n6w0jw45vzgsa8em3rx9gacqwzyttjqmg0ufp",
		)
		.expect("Invalid address")
		.assume_checked(),
		escrow_input_utxos: maker_inputs,
	};
	let taker_escrow_data = EscrowPsbtConstructionData {
		taproot_xonly_pubkey_hex:
			"4987f3de20a9b1fa6f76c6758934953a8d615e415f1a656f0f6563694b53107d".to_string(),
		musig_pubkey_compressed_hex:
			"02d8e204cdaebec4c5a637311072c865858dc4f142b3848b8e6dde4143476535b5".to_string(),
		change_address: Address::from_str(
			"bcrt1p28lv60c0t64taw5pp6k5fwwd4z66t99lny9d8mmpsysm5xanzd3smyz320",
		)
		.expect("Invalid address")
		.assume_checked(),
		escrow_input_utxos: taker_inputs,
	};
	(maker_escrow_data, taker_escrow_data)
}

// the transactions are testnet4 transactions, so run a testnet4 rpc node as backend
#[tokio::test]
async fn test_transaction_without_signature() {
	let test_wallet = new_test_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").await;
	let bond_without_signature = "02000000010127a9d96655011fca55dc2667f30b98655e46da98d0f84df676b53d7fb380140000000000fdffffff02998d0000000000002251207dd0d1650cdc22537709e35620f3b5cc3249b305bda1209ba4e5e01bc3ad2d8c50c3000000000000225120a12e5d145a4a3ab43f6cc1188435e74f253eace72bd986f1aaf780fd0c6532364f860000";
	let requirements = BondRequirements {
		min_input_sum_sat: 51000,
		locking_amount_sat: 50000,
		bond_address: "tb1p5yh969z6fgatg0mvcyvggd08fujnat8890vcdud277q06rr9xgmqwfdkcx".to_string(),
	};

	let result = test_wallet
		.validate_bond_tx_hex(&bond_without_signature, &requirements)
		.await;
	assert!(result.is_err());
	test_wallet.shutdown().await;
}

#[tokio::test]
async fn test_transaction_with_invalid_signature() {
	let test_wallet = new_test_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").await;
	// assembled bond tx but with the signature of a different bond = invalid
	let bond_with_invalid_signature = "020000000001010127a9d96655011fca55dc2667f30b98655e46da98d0f84df676b53d7fb3801400000000001900000002aa900000000000002251207dd0d1650cdc22537709e35620f3b5cc3249b305bda1209ba4e5e01bc3ad2d8c50c3000000000000225120a12e5d145a4a3ab43f6cc1188435e74f253eace72bd986f1aaf780fd0c65323601401fddcc681a1d0324c8fdeabbc08a3b06c26741872363c0ddfc82f15b6abe43d37815bcdc2ce1fb2f70cac426f7fb269d322ac6a621886208d0c625335bba670800000000";
	let requirements = BondRequirements {
		min_input_sum_sat: 51000,
		locking_amount_sat: 50000,
		bond_address: "tb1p5yh969z6fgatg0mvcyvggd08fujnat8890vcdud277q06rr9xgmqwfdkcx".to_string(),
	};

	let result = test_wallet
		.validate_bond_tx_hex(&bond_with_invalid_signature, &requirements)
		.await;
	assert!(result.is_err());
	test_wallet.shutdown().await;
}

#[tokio::test]
async fn test_bond_with_spent_input() {
	let test_wallet = new_test_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").await;
	let bond_with_spent_input = "02000000000101f7d992795b0b43227ea83e296a7c2a91771ede3ef54f1eb5664393c79b9399080100000000fdffffff0250c3000000000000225120a12e5d145a4a3ab43f6cc1188435e74f253eace72bd986f1aaf780fd0c653236abc6010000000000225120b83c64b440203fb74a0c672cd829f387b957129835dd3b5c4e33fc71a146b3ae0140afdafbae5b76217f469790b211d7fbda427e5b4379c4603e9ae08c9ef5aaae30bfecfc16e5f636c737bea8e0e27974854d1cd0d094ed737aadfc679a974074574f860000";
	let requirements = BondRequirements {
		min_input_sum_sat: 51000,
		locking_amount_sat: 50000,
		bond_address: "tb1p5yh969z6fgatg0mvcyvggd08fujnat8890vcdud277q06rr9xgmqwfdkcx".to_string(),
	};

	let result = test_wallet
		.validate_bond_tx_hex(&bond_with_spent_input, &requirements)
		.await;
	assert!(result.is_err());
	test_wallet.shutdown().await;
}

#[tokio::test]
async fn test_valid_bond_tx() {
	let test_wallet = new_test_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").await;
	let bond = "020000000001010127a9d96655011fca55dc2667f30b98655e46da98d0f84df676b53d7fb380140000000000010000000250c3000000000000225120a12e5d145a4a3ab43f6cc1188435e74f253eace72bd986f1aaf780fd0c653236aa900000000000002251207dd0d1650cdc22537709e35620f3b5cc3249b305bda1209ba4e5e01bc3ad2d8c014010e19c8b915624bd4aa0ba4d094d26ca031a6f2d8f23fe51372c7ea50e05f3caf81c7e139f6fed3e9ffd20c03d79f78542acb3d8aed664898f1c4b2909c2188c00000000";
	let requirements = BondRequirements {
		min_input_sum_sat: 100000,
		locking_amount_sat: 50000,
		bond_address: "tb1p5yh969z6fgatg0mvcyvggd08fujnat8890vcdud277q06rr9xgmqwfdkcx".to_string(),
	};

	let result = test_wallet.validate_bond_tx_hex(&bond, &requirements).await;
	assert!(result.is_ok());
	test_wallet.shutdown().await;
}

#[tokio::test]
async fn test_invalid_bond_tx_low_input_sum() {
	let test_wallet = new_test_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").await;
	let bond = "020000000001010127a9d96655011fca55dc2667f30b98655e46da98d0f84df676b53d7fb380140000000000010000000250c3000000000000225120a12e5d145a4a3ab43f6cc1188435e74f253eace72bd986f1aaf780fd0c653236aa900000000000002251207dd0d1650cdc22537709e35620f3b5cc3249b305bda1209ba4e5e01bc3ad2d8c014010e19c8b915624bd4aa0ba4d094d26ca031a6f2d8f23fe51372c7ea50e05f3caf81c7e139f6fed3e9ffd20c03d79f78542acb3d8aed664898f1c4b2909c2188c00000000";
	let requirements = BondRequirements {
		min_input_sum_sat: 2000000, // Set higher than the actual input sum
		locking_amount_sat: 50000,
		bond_address: "tb1p5yh969z6fgatg0mvcyvggd08fujnat8890vcdud277q06rr9xgmqwfdkcx".to_string(),
	};

	let result = test_wallet.validate_bond_tx_hex(&bond, &requirements).await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Bond input sum too small"));
	test_wallet.shutdown().await;
}

#[tokio::test]
async fn test_invalid_bond_tx_low_output_sum() {
	let test_wallet = new_test_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").await;
	let bond = "020000000001010127a9d96655011fca55dc2667f30b98655e46da98d0f84df676b53d7fb380140000000000010000000250c3000000000000225120a12e5d145a4a3ab43f6cc1188435e74f253eace72bd986f1aaf780fd0c653236aa900000000000002251207dd0d1650cdc22537709e35620f3b5cc3249b305bda1209ba4e5e01bc3ad2d8c014010e19c8b915624bd4aa0ba4d094d26ca031a6f2d8f23fe51372c7ea50e05f3caf81c7e139f6fed3e9ffd20c03d79f78542acb3d8aed664898f1c4b2909c2188c00000000";
	let requirements = BondRequirements {
		min_input_sum_sat: 100000,
		locking_amount_sat: 1000000, // Set higher than the actual output sum
		bond_address: "tb1p5yh969z6fgatg0mvcyvggd08fujnat8890vcdud277q06rr9xgmqwfdkcx".to_string(),
	};

	let result = test_wallet.validate_bond_tx_hex(&bond, &requirements).await;
	test_wallet.shutdown().await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Bond output sum too small"));
}

#[tokio::test]
async fn test_invalid_bond_tx_low_fee_rate() {
	let test_wallet = new_test_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").await;
	let bond = "020000000001010127a9d96655011fca55dc2667f30b98655e46da98d0f84df676b53d7fb380140000000000fdffffff0259b00000000000002251207dd0d1650cdc22537709e35620f3b5cc3249b305bda1209ba4e5e01bc3ad2d8c50c3000000000000225120a12e5d145a4a3ab43f6cc1188435e74f253eace72bd986f1aaf780fd0c6532360140bee11f7f644cf09d5031683203bbe61109090b1e4be4626e13de7a889d6e5d2f154233a2bfaf9cb983f31ccf01b1be5db2cd37bb0cb9a395e2632bc50105b4583f860000";
	let requirements = BondRequirements {
		min_input_sum_sat: 100000,
		locking_amount_sat: 50000,
		bond_address: "tb1p5yh969z6fgatg0mvcyvggd08fujnat8890vcdud277q06rr9xgmqwfdkcx".to_string(),
	};

	let result = test_wallet.validate_bond_tx_hex(&bond, &requirements).await;
	test_wallet.shutdown().await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Bond fee rate too low"));
}

#[tokio::test]
async fn test_build_escrow_transaction_output_descriptor() {
	// generating pubkeys
	// let seed: [u8; 32] = [
	// 	0x1b, 0x2d, 0x3d, 0x4d, 0x5d, 0x6d, 0x7d, 0x8d, 0x9d, 0xad, 0xbd, 0xcd, 0xdd, 0xed, 0xfd,
	// 	0x0d, 0x1d, 0x2d, 0x3d, 0x4d, 0x5d, 0x6d, 0x8d, 0x8d, 0x9d, 0xbd, 0xbd, 0xcd, 0xdd, 0xed,
	// 	0xfd, 0x1d,
	// ];
	// let xprv = ExtendedPrivKey::new_master(Network::Regtest, &seed).unwrap();
	// println!("xprv: {}", xprv.to_string());
	// let pubkey = xprv
	// 	.to_keypair(&secp256k1::Secp256k1::new())
	// 	.public_key()
	// 	.to_string();
	// dbg!(&pubkey);
	let maker_escrow_data: EscrowPsbtConstructionData;
	let taker_escrow_data: EscrowPsbtConstructionData;
	{
		let maker_wallet = new_test_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").await;
		let taker_wallet = new_test_wallet("tprv8ZgxMBicQKsPdKxWZWv9zVc22ubUdFrgaUzA4BZQUpEyMxYX3dwFbNfAGsVJ94zEhUUS1z56YBARpvTEjrSz9NzHyySCL33oMXpbqoGunL4").await;

		(maker_escrow_data, taker_escrow_data) =
			get_dummy_escrow_psbt_data(&maker_wallet, &taker_wallet).await;
		maker_wallet.shutdown().await;
		taker_wallet.shutdown().await;
	}
	println!("created dummmy psbt data");
	let coordinator_pk = XOnlyPublicKey::from_str(
		"d8e204cdaebec4c5a637311072c865858dc4f142b3848b8e6dde4143476535b5",
	)
	.unwrap();
	println!("assembling output descriptor");
	let result = build_escrow_transaction_output_descriptor(
		&maker_escrow_data,
		&taker_escrow_data,
		&coordinator_pk,
	);
	dbg!(&result); // cargo test -- --nocapture to see the output
	assert!(result.is_ok());
}

#[test]
fn test_aggregate_musig_pubkeys() {
	let agg_pk_result = aggregate_musig_pubkeys(
		"02F9308A019258C31049344F85F89D5229B531C845836F99B08601F113BCE036F9",
		"03DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659",
	);
	assert!(agg_pk_result.is_ok());
}

#[test]
fn test_miniscript_compilation() {
	let maker_pk = "4987f3de20a9b1fa6f76c6758934953a8d615e415f1a656f0f6563694b53107d";
	let taker_pk = "f1f1db08126af105974cde6021096525ed390cf9b7cde5fedb17a0b16ed31151";
	let coordinator_pk = "4b588489c13b2fbcfc2c3b8b6c885e9c366768f216899ba059d6c467af432ad4";
	let internal_key = bdk::bitcoin::PublicKey::from_str(
		"03f00949d6dd1ce99a03f88a1a4f59117d553b0da51728bb7fd5b98fbf541337fb",
	)
	.unwrap()
	.to_x_only_pubkey();

	let policy_a_string = format!("and(pk({}),pk({}))", maker_pk, taker_pk);
	let policy_b_string = format!("and(pk({}),pk({}))", maker_pk, coordinator_pk);

	let policy_a = Concrete::<XOnlyPublicKey>::from_str(&policy_a_string).unwrap();
	let policy_b = Concrete::<XOnlyPublicKey>::from_str(&policy_b_string).unwrap();

	let miniscript_a = policy_a.compile::<Tap>().unwrap();
	let miniscript_b = policy_b.compile::<Tap>().unwrap();

	let tap_leaf_a = bdk::miniscript::descriptor::TapTree::Leaf(Arc::new(miniscript_a));
	let tap_leaf_b = bdk::miniscript::descriptor::TapTree::Leaf(Arc::new(miniscript_b));

	let tap_tree_root =
		bdk::miniscript::descriptor::TapTree::Tree(Arc::new(tap_leaf_a), Arc::new(tap_leaf_b));

	let descriptor =
		Descriptor::<XOnlyPublicKey>::new_tr(internal_key, Some(tap_tree_root)).unwrap();
	dbg!(descriptor.address(bdk::bitcoin::Network::Regtest).unwrap());
}

#[test]
fn test_create_escrow_spending_psbt() {
	dotenv().ok();
	let test_descriptor = "tr(f00949d6dd1ce99a03f88a1a4f59117d553b0da51728bb7fd5b98fbf541337fb,{{and_v(v:pk(4987f3de20a9b1fa6f76c6758934953a8d615e415f1a656f0f6563694b53107d),pk(8f808f457423ff5e4e20a36d317ce9426f9da2fde875e74e15a04481b94bec06)),and_v(v:pk(f1f1db08126af105974cde6021096525ed390cf9b7cde5fedb17a0b16ed31151),pk(8f808f457423ff5e4e20a36d317ce9426f9da2fde875e74e15a04481b94bec06))},{and_v(v:and_v(v:pk(4987f3de20a9b1fa6f76c6758934953a8d615e415f1a656f0f6563694b53107d),pk(f1f1db08126af105974cde6021096525ed390cf9b7cde5fedb17a0b16ed31151)),after(2048)),and_v(v:pk(4987f3de20a9b1fa6f76c6758934953a8d615e415f1a656f0f6563694b53107d),after(12228))}})#0edq24m2";

	let escrow_output_wallet = Wallet::new(
		test_descriptor,
		None,
		Network::Regtest,
		MemoryDatabase::new(),
	)
	.unwrap();
	let secp_context = secp256k1::Secp256k1::new();
	let rpc_config = RpcConfig {
		url: env::var("BITCOIN_RPC_ADDRESS_PORT").unwrap().to_string(),
		auth: Auth::UserPass {
			username: env::var("BITCOIN_RPC_USER").unwrap(),
			password: env::var("BITCOIN_RPC_PASSWORD").unwrap(),
		},
		network: Network::Regtest,
		// wallet_name: env::var("BITCOIN_RPC_WALLET_NAME")?,
		wallet_name: bdk::wallet::wallet_name_from_descriptor(
			test_descriptor,
			None,
			Network::Regtest,
			&secp_context,
		)
		.unwrap(),
		sync_params: None,
	};
	let backend = RpcBlockchain::from_config(&rpc_config).unwrap();
	escrow_output_wallet
		.sync(&backend, SyncOptions::default())
		.unwrap();

	let escrow_utxo = escrow_output_wallet.list_unspent().unwrap();
	dbg!(&escrow_utxo);
	assert!(escrow_utxo.len() > 0);
}

#[test]
#[allow(unused)]
fn test_signing_keyspend_payout() {
	let keyspend_payout = bitcoin::Psbt::from_str("cHNidP8BAIkBAAAAASot4B9PBjdDjbgmOX/vfTk/FDS30ejTBu8dx3NJbPORAgAAAAD+////AjiPAQAAAAAAIlEgoZWbDMyvOKWEsTWwmthl3se6/M00x/bk2/4ELlCyCHSYCAAAAAAAACJRIHEcvg3V6lJW5RcQRmSz4Zj5gYP8C7RKMMwuVQ26vfH1qgcAAAABASuwrQEAAAAAACJRIPvD+LXxB25lAO+cwhHVKNlBgAPuS5QN9K2kFa5nAUhbYhXB8AlJ1t0c6ZoD+IoaT1kRfVU7DaUXKLt/1bmPv1QTN/sJv25s7HlPjxujOG0+WdJm6rnzQC0/DqM/Kl9zpbzzSkzhGQ99VWndyHBjT06XjMNhkpiuLWZT3vXWNLhoyTl5RSDx8dsIEmrxBZdM3mAhCWUl7TkM+bfN5f7bF6CxbtMRUa0g3FqLzlTfAuvoX+A8TbtP5naEIhEikUXXdYYGwn/ADp6swGIVwfAJSdbdHOmaA/iKGk9ZEX1VOw2lFyi7f9W5j79UEzf7TcSMPrBzbJ9+vK8/+WWM6ViQ5io+JfpLsugeicDL/ZpTGq6PyYTAXjMMx8QHcRpmSR/S5w7BsoGyEG9PK9qsEkkgSYfz3iCpsfpvdsZ1iTSVOo1hXkFfGmVvD2VjaUtTEH2tIPHx2wgSavEFl0zeYCEJZSXtOQz5t83l/tsXoLFu0xFRrQIACLHAYhXB8AlJ1t0c6ZoD+IoaT1kRfVU7DaUXKLt/1bmPv1QTN/ub9OAVg7whO/4eOawUxxjxx5y98Nr5fqsMtr5jOjgWOFMaro/JhMBeMwzHxAdxGmZJH9LnDsGygbIQb08r2qwSJyBJh/PeIKmx+m92xnWJNJU6jWFeQV8aZW8PZWNpS1MQfa0CxC+xwGIVwfAJSdbdHOmaA/iKGk9ZEX1VOw2lFyi7f9W5j79UEzf7wK3W/OD10zUnuKlYOeZzff8M3puuqUs5FopzHBxnb+BM4RkPfVVp3chwY09Ol4zDYZKYri1mU9711jS4aMk5eUUgSYfz3iCpsfpvdsZ1iTSVOo1hXkFfGmVvD2VjaUtTEH2tINxai85U3wLr6F/gPE27T+Z2hCIRIpFF13WGBsJ/wA6erMAhFkmH894gqbH6b3bGdYk0lTqNYV5BXxplbw9lY2lLUxB9ZQMJv25s7HlPjxujOG0+WdJm6rnzQC0/DqM/Kl9zpbzzSk3EjD6wc2yffryvP/lljOlYkOYqPiX6S7LoHonAy/2am/TgFYO8ITv+HjmsFMcY8cecvfDa+X6rDLa+Yzo4FjgMTuxdIRbcWovOVN8C6+hf4DxNu0/mdoQiESKRRdd1hgbCf8AOnkUCCb9ubOx5T48bozhtPlnSZuq580AtPw6jPypfc6W880rArdb84PXTNSe4qVg55nN9/wzem66pSzkWinMcHGdv4NzV3gchFvAJSdbdHOmaA/iKGk9ZEX1VOw2lFyi7f9W5j79UEzf7BQCETUSTIRbx8dsIEmrxBZdM3mAhCWUl7TkM+bfN5f7bF6CxbtMRUUUCm/TgFYO8ITv+HjmsFMcY8cecvfDa+X6rDLa+Yzo4FjjArdb84PXTNSe4qVg55nN9/wzem66pSzkWinMcHGdv4Gyh2C0BFyDwCUnW3RzpmgP4ihpPWRF9VTsNpRcou3/VuY+/VBM3+wEYIHUdvPOwH4IODj95hMoeLCf/7D01JV+CRgY/QOS+4hTfAAAA").unwrap();

	let agg_sig = LiftedSignature::from_str("f14ce71d49cefeb7b3f5dc903dc26a84abaf5c7ccc1bf29e4c029073e07525078bd3441b369ca1e80d2d3f12c401ffa6cccc10a86c70429a53841e7b4e92ed60").unwrap();
	// let agg_pubk_ctx = KeyAggContext::from_str("").unwrap();

	let mut bitcoin_032_tx: bitcoin::Transaction = keyspend_payout.clone().extract_tx().unwrap();
	let secp_signature =
		bitcoin::secp256k1::schnorr::Signature::from_slice(&agg_sig.to_bytes()).unwrap();
	// dbg!(secp_signature);

	let sighash_type = keyspend_payout.inputs[0].taproot_hash_ty().unwrap();
	let rust_bitcoin_sig = bitcoin::taproot::Signature {
		signature: secp_signature,
		sighash_type,
	};
	let unsigned_tx_hex = bitcoin::consensus::encode::serialize_hex(&bitcoin_032_tx);

	// 1st way to sign input
	let witness = bitcoin::Witness::p2tr_key_spend(&rust_bitcoin_sig);
	let escrow_input: &mut bitcoin::TxIn = bitcoin_032_tx.input.get_mut(0).unwrap();
	escrow_input.witness = witness.clone();

	// 2nd way to sign input
	// let input = &mut keyspend_payout.inputs[0];
	// input.tap_key_sig = Some(rust_bitcoin_sig);
	// let secp_context = bitcoin::secp256k1::Secp256k1::new();
	// let finalized = keyspend_payout.finalize(&secp_context).unwrap();

	let signed_hex_tx = bitcoin::consensus::encode::serialize_hex(&bitcoin_032_tx);

	// check the tx is different from the unsigned tx (sig has been added)
	assert_ne!(signed_hex_tx, unsigned_tx_hex);

	dbg!(&signed_hex_tx);
	dbg!(&unsigned_tx_hex);
	// let hex_tx = bitcoin::consensus::encode::serialize_hex(&finalized.extract_tx().unwrap());

	// convert the hex tx back into a bitcoin030 tx
	let bdk_bitcoin_030_tx: bdk::bitcoin::Transaction =
		deserialize(&hex::decode(signed_hex_tx.clone()).unwrap()).unwrap();

	// check the bitcoin030 tx is the same as the bitcoin032 tx
	assert_eq!(
		signed_hex_tx,
		bdk::bitcoin::consensus::encode::serialize_hex(&bdk_bitcoin_030_tx)
	);

	let backend = get_backend();
	// backend.broadcast(&bdk_bitcoin_030_tx).unwrap();
	// dbg!(bdk_bitcon_030_tx);
}
