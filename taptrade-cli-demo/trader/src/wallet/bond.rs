use anyhow::{anyhow, Result};
use bdk::bitcoin::address::{NetworkChecked, NetworkUnchecked};
use bdk::bitcoin::psbt::PartiallySignedTransaction;
use bdk::bitcoin::ScriptBuf;
use bdk::bitcoin::{Address, Network};
use bdk::{
	database::MemoryDatabase, wallet::coin_selection::BranchAndBoundCoinSelection, FeeRate,
	SignOptions, Wallet,
};
use serde::de::value;
use std::str::FromStr;

use crate::communication::api::OfferCreationResponse;
use crate::wallet::TraderSettings;

pub struct Outpoint {
	pub txid_hex: String,
	pub index: u32,
}

pub struct Bond {
	pub signed_bond_tx_hex: String,
	pub used_outpoint: Outpoint,
}

impl Bond {
	pub fn assemble(
		wallet: &Wallet<MemoryDatabase>,
		bond_target: &OfferCreationResponse,
		trader_input: &TraderSettings,
	) -> Result<PartiallySignedTransaction> {
		// parse bond locking address as Address struct and verify network is testnet
		let address: Address =
			Address::from_str(&bond_target.bond_address)?.require_network(Network::Testnet)?;

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
				.do_not_spend_change()
				.fee_rate(FeeRate::from_sat_per_vb(201.0));

			builder.finish()?
		};
		let finalized = wallet.sign(&mut psbt, SignOptions::default())?;
		if !finalized {
			return Err(anyhow!("Transaction could not be finalized"));
		};
		Ok(psbt)
	}
}

// impl BranchAndBoundCoinSelection
// pub fn new(size_of_change: u64) -> Self
// Create new instance with target size for change output
