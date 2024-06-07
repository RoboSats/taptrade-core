use anyhow::Result;
use bdk::{database::MemoryDatabase, wallet::coin_selection::BranchAndBoundCoinSelection, Wallet};
use serde::de::value;

use crate::communication::api::OfferCreationResponse;
use crate::wallet::TraderSettings;

pub struct Outpoint {
	pub txid_hex: String,
	pub index: u32
}

pub struct Bond {
	pub signed_bond_tx_hex: String,
	pub used_outpoint: Outpoint
}

impl Bond {
	pub fn assemble(wallet: &Wallet<MemoryDatabase>,
					bond_target: &OfferCreationResponse,
					trader_input: &TraderSettings) -> Result<Bond> {
		// let send_to = wallet.get_address(New)?;

		let (psbt, details) = {
			let mut builder =  wallet.build_tx();
			builder
				.coin_selection(BranchAndBoundCoinSelection::new(trader_input.trade_type.value()))
				.add_recipient(bond_target.locking_address, bond_target.locking_amount)
				.enable_rbf()
				.do_not_spend_change()
				.fee_rate(FeeRate::from_sat_per_vb(5.0));
			// coin_select
			// manually_selected_only
			// add_unspendable
			// do_not_spend_change
			// builder.finish()?
    };

		Ok(_)
	}
}

// impl BranchAndBoundCoinSelection
// pub fn new(size_of_change: u64) -> Self
// Create new instance with target size for change output
