use anyhow::{anyhow, Result};
use bdk::bitcoin::address::{NetworkChecked, NetworkUnchecked};
use bdk::bitcoin::amount::serde::as_btc::opt::deserialize;
use bdk::bitcoin::psbt::PartiallySignedTransaction;
use bdk::bitcoin::{Address, Network};
use bdk::bitcoin::{ScriptBuf, Transaction};
use bdk::{
	database::MemoryDatabase, wallet::coin_selection::BranchAndBoundCoinSelection, FeeRate,
	SignOptions, Wallet,
};
use log::debug;
use serde::de::value;
use std::str::FromStr;

use crate::communication::api::BondRequirementResponse;
use crate::wallet::TraderSettings;

pub struct Outpoint {
	pub txid_hex: String,
	pub index: u32,
}

pub struct Bond {
	// pub signed_bond_tx_hex: String,  not needed
	// pub used_outpoint: Outpoint,
}

impl Bond {
	pub fn assemble(
		wallet: &Wallet<MemoryDatabase>,
		bond_target: &BondRequirementResponse,
		trader_input: &TraderSettings,
	) -> Result<PartiallySignedTransaction> {
		debug!("Assembling bond transaction");
		// parse bond locking address as Address struct and verify network is testnet
		let address: Address =
			Address::from_str(&bond_target.bond_address)?.require_network(Network::Regtest)?;

		// build bond locking transaction. Use coin selection to add at least enough outputs
		// to have the full trading sum as change as evidence for the coordinator that the maker owns
		// enough funds to cover the trade
		let (mut psbt, details) = {
			let mut builder = wallet
				.build_tx()
				.coin_selection(BranchAndBoundCoinSelection::new(
					trader_input.trade_type.value(),
				));

			builder
				.add_recipient(address.script_pubkey(), bond_target.locking_amount_sat)
				// .do_not_spend_change() // reconsider if we need this?
				.fee_rate(FeeRate::from_sat_per_vb(201.0));

			builder.finish()?
		};
		debug!("Signing bond transaction.");
		let finalized = wallet.sign(&mut psbt, SignOptions::default())?; // deactivate to test bond validation
		if !finalized {
			return Err(anyhow!("Transaction could not be finalized"));
		};
		Ok(psbt)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::*;
	use bdk::{
		bitcoin::{self, bip32::ExtendedPrivKey, psbt::PartiallySignedTransaction, Network},
		blockchain::ElectrumBlockchain,
		database::MemoryDatabase,
		electrum_client::Client,
		keys::DescriptorPublicKey,
		miniscript::Descriptor,
		template::{Bip86, DescriptorTemplate},
		wallet::AddressInfo,
		KeychainKind, SignOptions, SyncOptions, Wallet,
	};
	use std::str::FromStr;

	fn setup_wallet(xprv_str: &str) -> Wallet<MemoryDatabase> {
		let backend = ElectrumBlockchain::from(Client::new("ssl://mempool.space:40002").unwrap());
		let wallet_xprv = ExtendedPrivKey::from_str(xprv_str).unwrap();
		let wallet = Wallet::new(
			Bip86(wallet_xprv, KeychainKind::External),
			Some(Bip86(wallet_xprv, KeychainKind::Internal)),
			Network::Regtest,
			MemoryDatabase::default(),
		)
		.unwrap();
		wallet.sync(&backend, SyncOptions::default()).unwrap();
		wallet
	}

	#[test]
	fn test_assemble_success() {
		let wallet = setup_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32");
		let bond_target = BondRequirementResponse {
			bond_address: "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx".to_string(),
			locking_amount_sat: 10000,
		};
		let trader_input = TraderSettings {
			electrum_endpoint: "ssl://mempool.space:40002".to_string(),
			coordinator_endpoint: "http://127.0.0.1:9999".to_string(),
			robosats_robohash_hex:
				"169b6049cf865eba7d01e1ad26975f1d5ff29d570297ff18d40a53c8281dff5d".to_string(),
			trade_type: OfferType::Buy(10000),
			payout_address: "tb1p37qg73t5y0l4un3q5dknzl8fgfhemghaap67wns45pzgrw2tasrq6kesxm"
				.to_string(),
			bond_ratio: 12,
			wallet_xprv: ExtendedPrivKey::from_str("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").unwrap(),
			duration_unix_ts: 1783593911, // until when the order should stay available
		};

		let result = Bond::assemble(&wallet, &bond_target, &trader_input);
		assert!(result.is_ok());
	}

	#[test]
	fn test_assemble_invalid_address() {
		let wallet = setup_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32");
		let bond_target = BondRequirementResponse {
			bond_address: "invalid_address".to_string(),
			locking_amount_sat: 10000,
		};
		let trader_input = TraderSettings {
			electrum_endpoint: "ssl://mempool.space:40002".to_string(),
			coordinator_endpoint: "http://127.0.0.1:9999".to_string(),
			robosats_robohash_hex:
				"169b6049cf865eba7d01e1ad26975f1d5ff29d570297ff18d40a53c8281dff5d".to_string(),
			trade_type: OfferType::Buy(10000),
			payout_address: "tb1p37qg73t5y0l4un3q5dknzl8fgfhemghaap67wns45pzgrw2tasrq6kesxm"
				.to_string(),
			bond_ratio: 12,
			wallet_xprv: ExtendedPrivKey::from_str("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").unwrap(),
			duration_unix_ts: 1783593911, // until when the order should stay available
		};

		let result = Bond::assemble(&wallet, &bond_target, &trader_input);
		assert!(result.is_err());
	}

	#[test]
	fn test_assemble_mainnet_address() {
		let wallet = setup_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32");
		let bond_target = BondRequirementResponse {
			bond_address: "bc1p5d7rjq7g6rdk2yhzks9smlaqtedr4dekq08ge8ztwac72sfr9rusxg3297"
				.to_string(),
			locking_amount_sat: 10000,
		};
		let trader_input = TraderSettings {
			electrum_endpoint: "ssl://mempool.space:40002".to_string(),
			coordinator_endpoint: "http://127.0.0.1:9999".to_string(),
			robosats_robohash_hex:
				"169b6049cf865eba7d01e1ad26975f1d5ff29d570297ff18d40a53c8281dff5d".to_string(),
			trade_type: OfferType::Buy(10000),
			payout_address: "tb1p37qg73t5y0l4un3q5dknzl8fgfhemghaap67wns45pzgrw2tasrq6kesxm"
				.to_string(),
			bond_ratio: 12,
			wallet_xprv: ExtendedPrivKey::from_str("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").unwrap(),
			duration_unix_ts: 1783593911, // until when the order should stay available
		};

		let result = Bond::assemble(&wallet, &bond_target, &trader_input);
		assert!(result.is_err());
	}

	#[test]
	fn test_assemble_insufficient_funds() {
		let wallet = setup_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32");
		let bond_target = BondRequirementResponse {
			bond_address: "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx".to_string(),
			locking_amount_sat: 10000000000, // Very high amount
		};
		let trader_input = TraderSettings {
			electrum_endpoint: "ssl://mempool.space:40002".to_string(),
			coordinator_endpoint: "http://127.0.0.1:9999".to_string(),
			robosats_robohash_hex:
				"169b6049cf865eba7d01e1ad26975f1d5ff29d570297ff18d40a53c8281dff5d".to_string(),
			trade_type: OfferType::Buy(10000),
			payout_address: "tb1p37qg73t5y0l4un3q5dknzl8fgfhemghaap67wns45pzgrw2tasrq6kesxm"
				.to_string(),
			bond_ratio: 12,
			wallet_xprv: ExtendedPrivKey::from_str("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").unwrap(),
			duration_unix_ts: 1783593911, // until when the order should stay available
		};

		let result = Bond::assemble(&wallet, &bond_target, &trader_input);
		assert!(result.is_err());
	}

	#[test]
	fn test_assemble_zero_amount() {
		let wallet = setup_wallet("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32");
		let bond_target = BondRequirementResponse {
			bond_address: "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx".to_string(),
			locking_amount_sat: 0,
		};
		let trader_input = TraderSettings {
			electrum_endpoint: "ssl://mempool.space:40002".to_string(),
			coordinator_endpoint: "http://127.0.0.1:9999".to_string(),
			robosats_robohash_hex:
				"169b6049cf865eba7d01e1ad26975f1d5ff29d570297ff18d40a53c8281dff5d".to_string(),
			trade_type: OfferType::Buy(10000),
			payout_address: "tb1p37qg73t5y0l4un3q5dknzl8fgfhemghaap67wns45pzgrw2tasrq6kesxm"
				.to_string(),
			bond_ratio: 12,
			wallet_xprv: ExtendedPrivKey::from_str("tprv8ZgxMBicQKsPdHuCSjhQuSZP1h6ZTeiRqREYS5guGPdtL7D1uNLpnJmb2oJep99Esq1NbNZKVJBNnD2ZhuXSK7G5eFmmcx73gsoa65e2U32").unwrap(),
			duration_unix_ts: 1783593911, // until when the order should stay available
		};

		let result = Bond::assemble(&wallet, &bond_target, &trader_input);
		assert!(result.is_err());
	}
}
