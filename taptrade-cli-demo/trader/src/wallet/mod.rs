pub mod bond;
pub mod musig2;
pub mod wallet_utils;

use super::*;
use crate::{
	cli::TraderSettings,
	communication::api::{BondRequirementResponse, OfferTakenResponse},
};
use anyhow::{anyhow, Result};
use bdk::{
	bitcoin::{
		self,
		bip32::ExtendedPrivKey,
		key::{KeyPair, Secp256k1, XOnlyPublicKey},
		psbt::{serialize, Input, PartiallySignedTransaction},
		Address, Network,
	},
	blockchain::ElectrumBlockchain,
	database::{Database, MemoryDatabase},
	electrum_client::Client,
	keys::DescriptorPublicKey,
	miniscript::{descriptor::Tr, Descriptor},
	template::{Bip86, DescriptorTemplate},
	wallet::{AddressIndex, AddressInfo},
	FeeRate, KeychainKind, SignOptions, SyncOptions, Wallet,
};
use bond::Bond;
use cli::OfferType;
use musig2::MuSigData;
use serde::Serialize;
use std::str::FromStr;
use wallet_utils::get_seed;

pub struct TradingWallet {
	pub wallet: Wallet<MemoryDatabase>,
	pub backend: ElectrumBlockchain,
	pub taproot_pubkey: XOnlyPublicKey,
}

pub fn get_wallet_xprv(xprv_input: Option<String>) -> Result<ExtendedPrivKey> {
	let xprv: ExtendedPrivKey;
	let network: Network = Network::Regtest;

	if let Some(xprv_i) = xprv_input {
		xprv = ExtendedPrivKey::from_str(&xprv_i)?;
	} else {
		xprv = ExtendedPrivKey::new_master(network, &get_seed())?;
		dbg!("Generated xprv: ", xprv.to_string());
	}

	Ok(xprv)
}

impl TradingWallet {
	pub fn load_wallet(trader_config: &TraderSettings) -> Result<TradingWallet> {
		let backend = ElectrumBlockchain::from(Client::new(&trader_config.electrum_endpoint)?);
		let wallet = Wallet::new(
			Bip86(trader_config.wallet_xprv, KeychainKind::External),
			Some(Bip86(trader_config.wallet_xprv, KeychainKind::Internal)),
			bitcoin::Network::Regtest,
			MemoryDatabase::default(), // non-permanent storage
		)?;
		let taproot_pubkey = trader_config
			.wallet_xprv
			.to_keypair(&Secp256k1::new())
			.x_only_public_key();

		wallet.sync(&backend, SyncOptions::default())?;
		dbg!("Balance: {} SAT", wallet.get_balance()?);
		Ok(TradingWallet {
			wallet,
			backend,
			taproot_pubkey: taproot_pubkey.0,
		})
	}

	// assemble bond and generate musig data for passed trade
	pub fn trade_onchain_assembly(
		&self,
		offer_conditions: &BondRequirementResponse,
		trader_config: &TraderSettings,
	) -> Result<(PartiallySignedTransaction, MuSigData, AddressInfo)> {
		let trading_wallet = &self.wallet;
		let bond = Bond::assemble(&self.wallet, offer_conditions, trader_config)?;
		let payout_address: AddressInfo =
			trading_wallet.get_address(bdk::wallet::AddressIndex::New)?;
		let musig_data = MuSigData::create(&trader_config.wallet_xprv, trading_wallet.secp_ctx())?;

		Ok((bond, musig_data, payout_address))
	}

	// pub fn get_escrow_psbt(
	// 	&self,
	// 	escrow_psbt_requirements: OfferTakenResponse,
	// 	trader_config: &TraderSettings,
	// ) -> Result<PartiallySignedTransaction> {
	// 	let fee_output = Address::from_str(&escrow_psbt_requirements.escrow_tx_fee_address)?
	// 		.assume_checked()
	// 		.script_pubkey();
	// 	let escrow_output = {
	// 		let temp_wallet = Wallet::new(
	// 			&escrow_psbt_requirements.escrow_output_descriptor,
	// 			None,
	// 			Network::Regtest,
	// 			MemoryDatabase::new(),
	// 		)?;
	// 		temp_wallet.get_address(AddressIndex::New)?.script_pubkey()
	// 	};
	// 	self.wallet.sync(&self.backend, SyncOptions::default())?;

	// 	let escrow_amount_sat = match trader_config.trade_type {
	// 		OfferType::Buy(_) => escrow_psbt_requirements.escrow_amount_taker_sat,
	// 		OfferType::Sell(_) => escrow_psbt_requirements.escrow_amount_maker_sat,
	// 	};
	// 	let (mut psbt, details) = {
	// 		let mut builder = self.wallet.build_tx();
	// 		builder
	// 			.add_recipient(escrow_output, escrow_amount_sat)
	// 			.add_recipient(
	// 				fee_output,
	// 				escrow_psbt_requirements.escrow_fee_sat_per_participant,
	// 			)
	// 			.fee_rate(FeeRate::from_sat_per_vb(10.0));
	// 		builder.finish()?
	// 	};
	// 	debug!("Signing escrow psbt.");
	// 	self.wallet.sign(&mut psbt, SignOptions::default())?;
	// 	Ok(psbt)
	// }

	/// returns suitable inputs (hex, csv serialized) and a change address for the assembly of the escrow psbt (coordinator side)
	pub fn get_escrow_psbt_inputs(&self, mut amount_sat: i64) -> Result<(String, String)> {
		let mut inputs: Vec<String> = Vec::new();

		self.wallet.sync(&self.backend, SyncOptions::default())?;
		let available_utxos = self.wallet.list_unspent()?;

		// could use more advanced coin selection if neccessary
		for utxo in available_utxos {
			let psbt_input = self.wallet.get_psbt_input(utxo, None, false)?;
			inputs.push(hex::encode(bincode::serialize(&psbt_input)?));
			amount_sat -= utxo.txout.value as i64;
			if amount_sat <= 0 {
				break;
			}
		}
		let serialized_inputs = inputs.join(",");

		let change_address = self
			.wallet
			.get_address(AddressIndex::New)?
			.address
			.to_string();
		Ok((serialized_inputs, change_address))
	}

	// validate that the taker psbt references the correct inputs and amounts
	// taker input should be the same as in the previous bond transaction.
	// input amount should be the bond amount when buying,
	// pub fn validate_taker_psbt(&self, psbt: &PartiallySignedTransaction) -> Result<&Self> {
	// 	error!("IMPLEMENT TAKER PSBT VALIDATION!");
	// 	// tbd once the trade psbt is implemented on coordinator side
	// 	Ok(self)
	// }

	// pub fn sign_escrow_psbt(&self, escrow_psbt: &mut PartiallySignedTransaction) -> Result<&Self> {
	// 	let finalized = self.wallet.sign(escrow_psbt, SignOptions::default())?;
	// 	if !finalized {
	// 		return Err(anyhow!("Signing of taker escrow psbt failed!"));
	// 	}
	// 	Ok(self)
	// }

	// validate amounts, escrow output
	// pub fn validate_maker_psbt(&self, psbt: &PartiallySignedTransaction) -> Result<&Self> {
	// 	error!("IMPLEMENT MAKER PSBT VALIDATION!");
	// 	// tbd once the trade psbt is implemented on coordinator side

	// 	Ok(self)
	// }
}
