pub mod bond;
pub mod musig2;
pub mod wallet_utils;

use crate::{cli::TraderSettings, communication::api::BondRequirementResponse};
use anyhow::Result;
use bdk::{
	bitcoin::{self, bip32::ExtendedPrivKey, psbt::PartiallySignedTransaction, Network},
	blockchain::ElectrumBlockchain,
	database::MemoryDatabase,
	electrum_client::Client,
	keys::DescriptorPublicKey,
	miniscript::Descriptor,
	template::{Bip86, DescriptorTemplate},
	wallet::AddressInfo,
	KeychainKind, SyncOptions, Wallet,
};
use bond::Bond;
use musig2::MuSigData;
use std::str::FromStr;
use wallet_utils::get_seed;

pub struct TradingWallet {
	pub wallet: Wallet<MemoryDatabase>,
	pub backend: ElectrumBlockchain,
}

pub fn get_wallet_xprv(xprv_input: Option<String>) -> Result<ExtendedPrivKey> {
	let xprv: ExtendedPrivKey;
	let network: Network = Network::Testnet;

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
			Bip86(trader_config.wallet_xprv.clone(), KeychainKind::External),
			Some(Bip86(
				trader_config.wallet_xprv.clone(),
				KeychainKind::Internal,
			)),
			bitcoin::Network::Testnet,
			MemoryDatabase::default(), // non-permanent storage
		)?;

		wallet.sync(&backend, SyncOptions::default())?;
		dbg!("Balance: {} SAT", wallet.get_balance()?);
		Ok(TradingWallet { wallet, backend })
	}

	// assemble bond and generate musig data for passed trade
	pub fn trade_onchain_assembly(
		&self,
		offer_conditions: &BondRequirementResponse,
		trader_config: &TraderSettings,
	) -> Result<(PartiallySignedTransaction, MuSigData, AddressInfo)> {
		let trading_wallet = &self.wallet;
		let bond = Bond::assemble(&self.wallet, &offer_conditions, trader_config)?;
		let payout_address: AddressInfo =
			trading_wallet.get_address(bdk::wallet::AddressIndex::LastUnused)?;
		let musig_data = MuSigData::create(&trader_config.wallet_xprv, trading_wallet.secp_ctx())?;

		Ok((bond, musig_data, payout_address))
	}

	// validate that the taker psbt references the correct inputs and amounts
	// taker input should be the same as in the previous bond transaction.
	// input amount should be the bond amount when buying,
	pub fn validate_taker_psbt(&self, psbt: &PartiallySignedTransaction) -> bool {
		false
	}
}
