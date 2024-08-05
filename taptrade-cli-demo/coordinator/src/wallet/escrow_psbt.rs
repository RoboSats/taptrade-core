use super::*;
use bdk::{
	bitcoin::psbt::Input,
	descriptor::Descriptor,
	miniscript::{descriptor::TapTree, policy::Concrete, Tap},
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
) -> Result<String> {
	debug!(
		"Aggregating musig pubkeys: {} and {}",
		maker_musig_pubkey, taker_musig_pubkey
	);
	let pubkeys: [MuSig2PubKey; 2] = [
		MuSig2PubKey::from_str(maker_musig_pubkey).context("Error parsing musig pk 1")?,
		MuSig2PubKey::from_str(taker_musig_pubkey).context("Error parsing musig pk 2")?,
	];

	let key_agg_ctx = KeyAggContext::new(pubkeys).context("Error aggregating musig pubkeys")?;
	let agg_pk: MuSig2PubKey = key_agg_ctx.aggregated_pubkey();

	Ok(agg_pk.to_string())
}

pub fn build_escrow_transaction_output_descriptor(
	maker_escrow_data: &EscrowPsbtConstructionData,
	taker_escrow_data: &EscrowPsbtConstructionData,
	coordinator_pk: &XOnlyPublicKey,
) -> Result<String> {
	let maker_pk = maker_escrow_data.taproot_xonly_pubkey_hex.clone();
	let taker_pk = taker_escrow_data.taproot_xonly_pubkey_hex.clone();
	let coordinator_pk = hex::encode(coordinator_pk.serialize());

	// let script_a = format!("and(and(after({}),{}),{})", "144", maker_pk, coordinator_pk);
	// let script_b = format!(
	// 	"and_v(v:{},and_v(v:{},{}))",
	// 	maker_pk, taker_pk, coordinator_pk
	// );
	let script_c: String = format!("and(pk({}),pk({}))", maker_pk, coordinator_pk);
	let script_d = format!("and(pk({}),pk({}))", taker_pk, coordinator_pk);
	let script_e = format!("and(pk({}),after(12228))", maker_pk);
	let script_f = format!("and(and(pk({}),pk({})),after(2048))", maker_pk, taker_pk);

	// let compiled_a = Concrete::<String>::from_str(&script_a)?.compile::<Tap>()?;
	// let compiled_b = Concrete::<String>::from_str(&script_b)?.compile()?;
	let compiled_c = Concrete::<String>::from_str(&script_c)
		.context("Failed to parse script_c")?
		.compile::<Tap>()
		.context("Failed to compile script_c")?;
	let compiled_d = Concrete::<String>::from_str(&script_d)
		.context("Failed to parse script_d")?
		.compile::<Tap>()
		.context("Failed to compile script_d")?;
	let compiled_e = Concrete::<String>::from_str(&script_e)
		.context("Failed to parse script_e")?
		.compile::<Tap>()
		.context("Failed to compile script_e")?;
	let compiled_f = Concrete::<String>::from_str(&script_f)
		.context("Failed to parse script_f")?
		.compile::<Tap>()
		.context("Failed to compile script_f")?;

	// Create TapTree leaves
	// let tap_leaf_a = TapTree::Leaf(Arc::new(compiled_a));
	// let tap_leaf_b = TapTree::Leaf(Arc::new(compiled_b));
	let tap_leaf_c = TapTree::Leaf(Arc::new(compiled_c));
	let tap_leaf_d = TapTree::Leaf(Arc::new(compiled_d));
	let tap_leaf_e = TapTree::Leaf(Arc::new(compiled_e));
	let tap_leaf_f = TapTree::Leaf(Arc::new(compiled_f));

	let tap_node_cd = TapTree::Tree(Arc::new(tap_leaf_c), Arc::new(tap_leaf_d));
	let tap_node_ef = TapTree::Tree(Arc::new(tap_leaf_e), Arc::new(tap_leaf_f));

	// Create the TapTree (example combining leaves, adjust as necessary), will be used for Script Path Spending (Alternative Spending Paths) in the descriptor
	let final_tap_tree = TapTree::Tree(Arc::new(tap_node_cd), Arc::new(tap_node_ef));

	// An internal key, that defines the way to spend the transaction directly, using Key Path Spending
	let internal_agg_musig_key = aggregate_musig_pubkeys(
		&maker_escrow_data.musig_pubkey_compressed_hex,
		&taker_escrow_data.musig_pubkey_compressed_hex,
	)?;

	// Create the descriptor
	let descriptor = Descriptor::new_tr(internal_agg_musig_key, Some(final_tap_tree))
		.context("Error assembling escrow output descriptor")?;
	descriptor.sanity_check()?;

	debug!("Escrow descriptor: {}", descriptor);
	Ok(descriptor.to_string())
}

// pub fn assemble_escrow_psbts(
// coordinator: &Coordinator,
// escrow_data: &EscrowPsbtConstructionData,
// coordinator_pk: &XOnlyPublicKey,
// ) -> Result<()> {
// panic!("Implement wallet.build_escrow_psbt()");
// Ok(())
// }
