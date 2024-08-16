use super::*;
use bdk::{
	bitcoin::{psbt::PartiallySignedTransaction},
	descriptor::{Descriptor},
	miniscript::{descriptor::TapTree, policy::Concrete, Tap, ToPublicKey},
	SignOptions,
};
use musig2::{secp256k1::PublicKey as MuSig2PubKey, KeyAggContext};


#[derive(Debug)]
pub struct EscrowPsbtConstructionData {
	pub taproot_xonly_pubkey_hex: String,
	pub escrow_input_utxos: Vec<PsbtInput>,
	pub change_address: Address,
	// pub taproot_xonly_pubkey_hex_taker: String,
	// pub taproot_pubkey_hex_coordinator: String,
	pub musig_pubkey_compressed_hex: String,
	// pub musig_pubkey_compressed_hex_taker: String,
}

impl EscrowPsbtConstructionData {
	pub fn input_sum(&self) -> Result<u64> {
		let mut input_sum = 0;
		for input in &self.escrow_input_utxos {
			if let Some(txout) = input.psbt_input.witness_utxo.as_ref() {
				input_sum += txout.value;
			}
		}
		if input_sum == 0 {
			return Err(anyhow!("Input sum of escrow creation psbt input is 0"));
		}
		Ok(input_sum)
	}
}

pub fn aggregate_musig_pubkeys(
	maker_musig_pubkey: &str,
	taker_musig_pubkey: &str,
) -> Result<KeyAggContext> {
	debug!(
		"Aggregating musig pubkeys: {} and {}",
		maker_musig_pubkey, taker_musig_pubkey
	);
	let pubkeys: [MuSig2PubKey; 2] = [
		MuSig2PubKey::from_str(maker_musig_pubkey).context("Error parsing musig pk 1")?,
		MuSig2PubKey::from_str(taker_musig_pubkey).context("Error parsing musig pk 2")?,
	];

	let key_agg_ctx = KeyAggContext::new(pubkeys).context("Error aggregating musig pubkeys")?;
	Ok(key_agg_ctx)
}

/// this function builds the escrow output with all possible spending conditions
pub fn build_escrow_transaction_output_descriptor(
	maker_escrow_data: &EscrowPsbtConstructionData,
	taker_escrow_data: &EscrowPsbtConstructionData,
	coordinator_pk: &XOnlyPublicKey,
) -> Result<Descriptor<XOnlyPublicKey>> {
	let maker_pk = maker_escrow_data.taproot_xonly_pubkey_hex.clone();
	let taker_pk = taker_escrow_data.taproot_xonly_pubkey_hex.clone();
	let coordinator_pk = hex::encode(coordinator_pk.serialize());

	let policy_a_string = format!("and(pk({}),pk({}))", maker_pk, coordinator_pk);
	let policy_b_string = format!("and(pk({}),pk({}))", taker_pk, coordinator_pk);
	let policy_c_string = format!("and(pk({}),after(12228))", maker_pk);
	let policy_d_string = format!("and(and(pk({}),pk({})),after(2048))", maker_pk, taker_pk);

	// parse the policy strings into policy objects
	let policy_a = Concrete::<XOnlyPublicKey>::from_str(&policy_a_string)
		.context("Failed to parse policy string a")?;
	let policy_b = Concrete::<XOnlyPublicKey>::from_str(&policy_b_string)
		.context("Failed to parse policy string b")?;
	let policy_c = Concrete::<XOnlyPublicKey>::from_str(&policy_c_string)
		.context("Failed to parse policy string c")?;
	let policy_d = Concrete::<XOnlyPublicKey>::from_str(&policy_d_string)
		.context("Failed to parse policy string d")?;

	// Compile the policies into Miniscript
	let miniscript_a = policy_a
		.compile::<Tap>()
		.context("Failed to compile miniscript a")?;
	let miniscript_b = policy_b
		.compile::<Tap>()
		.context("Failed to compile miniscript b")?;
	let miniscript_c = policy_c
		.compile::<Tap>()
		.context("Failed to compile miniscript c")?;
	let miniscript_d = policy_d
		.compile::<Tap>()
		.context("Failed to compile miniscript d")?;

	// Create TapTree leaves
	let tap_leaf_a = bdk::miniscript::descriptor::TapTree::Leaf(Arc::new(miniscript_a));
	let tap_leaf_b = bdk::miniscript::descriptor::TapTree::Leaf(Arc::new(miniscript_b));
	let tap_leaf_c = bdk::miniscript::descriptor::TapTree::Leaf(Arc::new(miniscript_c));
	let tap_leaf_d = bdk::miniscript::descriptor::TapTree::Leaf(Arc::new(miniscript_d));

	let tap_node_ab = TapTree::Tree(Arc::new(tap_leaf_a), Arc::new(tap_leaf_b));
	let tap_node_cd = TapTree::Tree(Arc::new(tap_leaf_d), Arc::new(tap_leaf_c));

	// Create the TapTree (example combining leaves, adjust as necessary), will be used for Script Path Spending (Alternative Spending Paths) in the descriptor
	let tap_root = TapTree::Tree(Arc::new(tap_node_ab), Arc::new(tap_node_cd));

	// An internal key, that defines the way to spend the transaction directly, using Key Path Spending
	let key_agg_ctx: KeyAggContext = aggregate_musig_pubkeys(
		&maker_escrow_data.musig_pubkey_compressed_hex,
		&taker_escrow_data.musig_pubkey_compressed_hex,
	)?;
	let internal_agg_musig_key: XOnlyPublicKey = bdk::bitcoin::PublicKey::from_slice(
		&key_agg_ctx.aggregated_pubkey::<MuSig2PubKey>().serialize(),
	)
	.context("Error converting musig pk to bitcoin pk")?
	.to_x_only_pubkey();

	// Create the descriptor
	let descriptor = Descriptor::<XOnlyPublicKey>::new_tr(internal_agg_musig_key, Some(tap_root))
		.context("Error assembling escrow output descriptor")?;
	descriptor.sanity_check()?;
	debug!("Escrow descriptor: {:#?}", descriptor.to_string());
	Ok(descriptor) // then spend to descriptor.address(Network::Regtest)
}

// pub fn assemble_escrow_psbts(
impl<D: bdk::database::BatchDatabase> CoordinatorWallet<D> {
	pub async fn create_escrow_psbt(
		&self,
		db: &Arc<CoordinatorDB>,
		taker_psbt_request: &OfferPsbtRequest,
	) -> Result<EscrowPsbt> {
		let trade_id = &taker_psbt_request.offer.offer_id_hex.clone();
		let maker_psbt_input_data = db.fetch_maker_escrow_psbt_data(trade_id).await?;
		let taker_psbt_input_data = EscrowPsbtConstructionData {
			taproot_xonly_pubkey_hex: taker_psbt_request.trade_data.taproot_pubkey_hex.clone(),
			escrow_input_utxos: csv_hex_to_bdk_input(
				&taker_psbt_request.trade_data.bdk_psbt_inputs_hex_csv,
			)?,
			change_address: Address::from_str(
				&taker_psbt_request.trade_data.client_change_address,
			)?
			.assume_checked(),
			musig_pubkey_compressed_hex: taker_psbt_request.trade_data.musig_pubkey_hex.clone(),
		};

		let coordinator_escrow_pk = self.get_coordinator_taproot_pk().await?;
		let escrow_output_descriptor = build_escrow_transaction_output_descriptor(
			&maker_psbt_input_data,
			&taker_psbt_input_data,
			&coordinator_escrow_pk,
		)?;

		let escrow_coordinator_fee_address =
			Address::from_str(&self.get_new_address().await?)?.assume_checked();

		let (escrow_amount_maker_sat, escrow_amount_taker_sat, escrow_fee_sat_per_participant) = db
			.get_escrow_tx_amounts(trade_id, self.coordinator_feerate)
			.await?;

		let (escrow_psbt, details) = {
			// maybe we can generate a address/taproot pk directly from the descriptor without a new wallet?
			// let temp_wallet = Wallet::new(
			// 	&escrow_output_descriptor,
			// 	None,
			// 	bitcoin::Network::Regtest,
			// 	MemoryDatabase::new(),
			// )?;
			let escrow_address =
				escrow_output_descriptor.address(bdk::bitcoin::Network::Regtest)?;

			debug!("Created escrow address: {escrow_address}");

			// dummy escrow address for testing the psbt signing flow
			// let escrow_address =
			// Address::from_str(self.get_new_address().await?.as_str())?.assume_checked();

			// using absolute fee for now, in production we should come up with a way to determine the tx weight
			// upfront and substract the fee from the change outputs (10k == ~30/sat vbyte)
			let tx_fee_abs = 10000;

			let change_amount_maker = maker_psbt_input_data.input_sum()?
				- (escrow_amount_maker_sat + escrow_fee_sat_per_participant + tx_fee_abs / 2);
			let change_amount_taker = taker_psbt_input_data.input_sum()?
				- (escrow_amount_taker_sat + escrow_fee_sat_per_participant + tx_fee_abs / 2);

			let amount_escrow = escrow_amount_maker_sat + escrow_amount_taker_sat;

			let wallet = self.wallet.lock().await;
			// let mut builder = temp_wallet.build_tx();
			let mut builder = wallet.build_tx();
			builder
				.manually_selected_only()
				.add_recipient(escrow_address.script_pubkey(), amount_escrow)
				.add_recipient(
					escrow_coordinator_fee_address.script_pubkey(),
					escrow_fee_sat_per_participant * 2,
				)
				.add_recipient(
					maker_psbt_input_data.change_address.script_pubkey(),
					change_amount_maker,
				)
				.add_recipient(
					taker_psbt_input_data.change_address.script_pubkey(),
					change_amount_taker,
				)
				.fee_absolute(tx_fee_abs);
			for input in maker_psbt_input_data.escrow_input_utxos.iter() {
				// satisfaction weight 66 bytes for schnorr sig + opcode + sighash for keyspend. This is a hack?
				builder.add_foreign_utxo(input.utxo, input.psbt_input.clone(), 264)?;
			}
			for input in taker_psbt_input_data.escrow_input_utxos.iter() {
				builder.add_foreign_utxo(input.utxo, input.psbt_input.clone(), 264)?;
			}
			builder.finish()?
		};

		let escrow_tx_txid: String = details.txid.to_string();

		Ok(EscrowPsbt {
			escrow_tx_txid,
			escrow_psbt_hex: escrow_psbt.to_string(),
			escrow_output_descriptor: escrow_output_descriptor.to_string(),
			coordinator_xonly_escrow_pk: coordinator_escrow_pk.to_string(),
			escrow_amount_maker_sat,
			escrow_amount_taker_sat,
			escrow_fee_sat_per_participant,
		})
	}

	pub async fn validate_escrow_init_psbt(&self, _escrow_init_psbt: &str) -> Result<()> {
		warn!("Implement escrow psbt validation. For now, returning Ok");
		Ok(())
	}

	pub async fn combine_and_broadcast_escrow_psbt(
		&self,
		signed_maker_psbt_hex: &str,
		signed_taker_psbt_hex: &str,
	) -> Result<()> {
		trace!(
			"\n\n\nCombining and broadcasting escrow psbt.
			signed maker psbt hex: {}, signed taker psbt hex: {}",
			signed_maker_psbt_hex,
			signed_taker_psbt_hex
		);
		let mut maker_psbt =
			PartiallySignedTransaction::deserialize(&hex::decode(signed_maker_psbt_hex)?)?;
		let taker_psbt =
			PartiallySignedTransaction::deserialize(&hex::decode(signed_taker_psbt_hex)?)?;

		maker_psbt.combine(taker_psbt)?;
		debug!("Combined escrow psbt: {:#?}", maker_psbt);

		let wallet = self.wallet.lock().await;
		match wallet.finalize_psbt(&mut maker_psbt, SignOptions::default()) {
			Ok(true) => {
				let tx = maker_psbt.extract_tx();
				self.backend.broadcast(&tx)?;
				info!("Escrow transaction broadcasted: {}", tx.txid());
				Ok(())
			}
			Ok(false) => Err(anyhow!("Failed to finalize escrow psbt")),
			Err(e) => Err(anyhow!("Error finalizing escrow psbt: {}", e)),
		}
	}
}
