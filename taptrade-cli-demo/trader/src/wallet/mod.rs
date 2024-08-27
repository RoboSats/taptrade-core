pub mod bond;
pub mod musig2_utils;
pub mod wallet_utils;

use super::*;
use crate::{
	cli::TraderSettings,
	communication::api::{BondRequirementResponse, OfferTakenResponse},
};
use ::musig2::{AggNonce, KeyAggContext};
use anyhow::{anyhow, Context, Result};
use bdk::{
	bitcoin::{
		self,
		bip32::ExtendedPrivKey,
		consensus::encode::serialize_hex,
		hashes::Hash,
		key::{KeyPair, Secp256k1, XOnlyPublicKey},
		psbt::{raw, serialize, Input, PartiallySignedTransaction, Prevouts},
		sighash::{SighashCache, TapSighash, TapSighashType},
		Address, Network, TxOut,
	},
	blockchain::ElectrumBlockchain,
	database::{Database, MemoryDatabase},
	electrum_client::Client,
	keys::{DescriptorPublicKey, ValidNetworks},
	miniscript::{descriptor::Tr, Descriptor},
	template::{Bip86, DescriptorTemplate},
	wallet::{AddressIndex, AddressInfo},
	FeeRate, KeychainKind, SignOptions, SyncOptions, Wallet,
};
use bond::Bond;
use cli::OfferType;
use hex::ToHex;
use musig2::secp::MaybeScalar;
use musig2_utils::MuSigData;
use serde::Serialize;
use std::{ops::Add, str::FromStr};
use wallet_utils::get_seed;

pub struct TradingWallet {
	pub wallet: Wallet<MemoryDatabase>,
	pub backend: ElectrumBlockchain,
	pub taproot_pubkey: XOnlyPublicKey,
}

#[derive(Serialize)]
pub struct PsbtInput {
	pub psbt_input: Input,
	pub utxo: bdk::bitcoin::OutPoint,
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
		dbg!(
			"Balance: {} SAT\nnew address: {}",
			wallet.get_balance()?,
			wallet.get_address(AddressIndex::New)?.address
		);
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
		// assembles the bond according to the requirements
		let bond = Bond::assemble(&self.wallet, offer_conditions, trader_config)?;

		// get a new payout address from the trader wallet
		let payout_address: AddressInfo =
			trading_wallet.get_address(bdk::wallet::AddressIndex::New)?;

		// generate new musig nonce and keys from the wallet xprv
		let musig_data = MuSigData::create(&trader_config.wallet_xprv, trading_wallet.secp_ctx())?;

		Ok((bond, musig_data, payout_address))
	}

	/// returns suitable inputs (binary encoded using bincode, hex serialized, csv formatted) and a change address for the assembly of the escrow psbt (coordinator side)
	pub fn get_escrow_psbt_inputs(&self, mut amount_sat: i64) -> Result<(String, String)> {
		let mut inputs: Vec<String> = Vec::new();

		self.wallet.sync(&self.backend, SyncOptions::default())?;
		let available_utxos = self.wallet.list_unspent()?;

		// could use more advanced coin selection if neccessary
		for utxo in available_utxos {
			let psbt_input: Input = self.wallet.get_psbt_input(utxo.clone(), None, false)?;
			let input = PsbtInput {
				psbt_input,
				utxo: utxo.outpoint,
			};
			inputs.push(hex::encode(bincode::serialize(&input)?));
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

	/// signs the inputs of the passed psbt that are controlled by the bdk wallet of the trader
	pub fn sign_escrow_psbt(&self, escrow_psbt: &mut PartiallySignedTransaction) -> Result<&Self> {
		// we need to finalize here too to make finalizing on the coordinator side work
		let sign_options = SignOptions {
			try_finalize: true,
			..SignOptions::default()
		};
		let _ = self.wallet.sign(escrow_psbt, sign_options)?;
		Ok(self)
	}

	// validate amounts, escrow output
	pub fn validate_escrow_psbt(&self, psbt: &PartiallySignedTransaction) -> Result<&Self> {
		warn!("IMPLEMENT MAKER PSBT VALIDATION for production use!");
		// validate: change output address, amounts, fee
		// tbd
		Ok(self)
	}

	pub fn validate_payout_psbt(&self, psbt: &PartiallySignedTransaction) -> Result<&Self> {
		warn!("IMPLEMENT PAYOUT PSBT VALIDATION for production use!");
		// validate: change output address, amounts, fee
		// tbd
		Ok(self)
	}

	/// creates a partial signature to spend the keyspend path of the escrow output
	/// which will be returned to the coordinator for aggregation
	pub fn create_keyspend_payout_signature(
		&self,
		validated_payout_psbt: PartiallySignedTransaction,
		key_agg_context: KeyAggContext,
		agg_pub_nonce: AggNonce,
		local_musig_state: MuSigData,
	) -> Result<String> {
		let mut sig_hash_cache = SighashCache::new(&validated_payout_psbt.unsigned_tx);

		let utxo = validated_payout_psbt
			.iter_funding_utxos()
			.next()
			.ok_or(anyhow!("No UTXO found in payout psbt"))??
			.clone();
		let sighash_type = validated_payout_psbt.inputs[0].taproot_hash_ty()?;

		// get the msg (sighash) to sign with the musig key
		let binding = sig_hash_cache
			.taproot_key_spend_signature_hash(0, &Prevouts::All(&[utxo]), sighash_type)
			.context("Failed to create keyspend sighash")?;
		let raw_sig_hash = binding.to_raw_hash();

		// get secret nonce from trader musig state
		let secret_nonce = local_musig_state.nonce.get_sec_for_signing()?;
		// get secret key from trade musig state
		let seckey = local_musig_state.secret_key;

		// create partial signature for the taproot keyspend signature hash of the payout psbt
		let keyspend_sig: musig2::PartialSignature = musig2::sign_partial(
			&key_agg_context,
			seckey,
			secret_nonce,
			&agg_pub_nonce,
			raw_sig_hash,
		)?;

		match keyspend_sig {
			MaybeScalar::Valid(s) => Ok(s.encode_hex()),
			MaybeScalar::Zero => Err(anyhow!("keyspend sig maybe scalar is Zero")),
		}
	}
}
