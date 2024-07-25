use std::collections::btree_map::Range;
use std::time::Duration;

use super::*;
use bdk::bitcoin::Network;
use bdk::database::MemoryDatabase;
use bdk::keys::GeneratableKey;
use bdk::{blockchain::RpcBlockchain, Wallet};
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
		network: Regtest,
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
	tokio::time::sleep(Duration::from_secs(16)).await; // fetch the mempool
	CoordinatorWallet::<MemoryDatabase> {
		wallet: Arc::new(Mutex::new(wallet)),
		backend: Arc::new(backend),
		json_rpc_client: Arc::clone(&json_rpc_client),
		mempool: Arc::new(MempoolHandler::new(json_rpc_client).await),
	}
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
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Bond fee rate too low"));
}

#[test]
fn test_build_escrow_transaction_output_descriptor() {
	// let seed: [u8; 32] = [
	// 	0x1b, 0x2d, 0x3d, 0x4d, 0x5d, 0x6d, 0x7d, 0x8d, 0x9d, 0xad, 0xbd, 0xcd, 0xdd, 0xed, 0xfd,
	// 	0x0d, 0x1d, 0x2d, 0x3d, 0x4d, 0x5d, 0x6d, 0x8d, 0x8d, 0x9d, 0xbd, 0xbd, 0xcd, 0xdd, 0xed,
	// 	0xfd, 0x0d,
	// ];
	// let xprv = ExtendedPrivKey::new_master(Network::Testnet, &seed).unwrap();
	// let pubkey = xprv
	// 	.to_keypair(&secp256k1::Secp256k1::new())
	// 	.x_only_public_key()
	// 	.0
	// 	.to_string();
	// dbg!(&pubkey);
	let escrow_data = EscrowPsbtConstructionData {
		taproot_pubkey_hex_maker:
			"b709f64da734e04e35b129a65a7fae361cad8a9458d1abc4f0b45b7661a42fca".to_string(),
		taproot_pubkey_hex_taker:
			"4987f3de20a9b1fa6f76c6758934953a8d615e415f1a656f0f6563694b53107d".to_string(),
		musig_pubkey_hex_maker: "b943789a0c9a16e27d7a9d27077eacd0fc664f01ff795f64e0f5fd257b4019e9"
			.to_string(),
		musig_pubkey_hex_taker: "a2e78a076b9ecfec8a8eb71ad5f0f29592d1559424f35ac25a0130d1a2880733"
			.to_string(),
	};
	let coordinator_pk = XOnlyPublicKey::from_str(
		"d8e204cdaebec4c5a637311072c865858dc4f142b3848b8e6dde4143476535b5",
	)
	.unwrap();
	let result = build_escrow_transaction_output_descriptor(&escrow_data, &coordinator_pk);
	dbg!(&result);
	assert!(result.is_ok());
}
