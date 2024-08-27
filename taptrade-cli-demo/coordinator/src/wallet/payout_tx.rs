use bdk::FeeRate;

/// construction of the transaction spending the escrow output after a successfull trade as keyspend transaction
use super::*;
use bitcoin;

/// get current feerate from blockchain backend and calculate absolute fees for the keyspend tx
/// depending on the feerate. Fallback to 40sat/vb if the feerate cannot be estimated (e.g. regtest backend).
fn get_tx_fees_abs_sat(blockchain_backend: &RpcBlockchain) -> Result<(u64, u64)> {
	let feerate = match blockchain_backend.estimate_fee(6) {
		Ok(feerate) => feerate,
		Err(e) => {
			error!("Failed to estimate fee: {}. Using fallback 40 sat/vb`", e);
			FeeRate::from_sat_per_vb(40.0)
		}
	};
	let keyspend_payout_tx_size_vb = 140; // ~, always 1 input, 2 outputs

	let tx_fee_abs = feerate.fee_vb(keyspend_payout_tx_size_vb);

	Ok((tx_fee_abs, tx_fee_abs / 2))
}

impl<D: bdk::database::BatchDatabase> CoordinatorWallet<D> {
	/// loads the escrow descriptor in a temp wallet and return the escrow utxo (as Input and its Outpoint)
	fn get_escrow_utxo(
		&self,
		descriptor: &Descriptor<XOnlyPublicKey>,
	) -> anyhow::Result<(Input, OutPoint)> {
		let temp_wallet = Wallet::new(
			&descriptor.to_string(),
			None,
			bdk::bitcoin::Network::Regtest,
			MemoryDatabase::new(),
		)?;
		temp_wallet.sync(&self.backend, SyncOptions::default())?;
		let available_utxos = temp_wallet.list_unspent()?;
		if available_utxos.len() != 1 {
			return Err(anyhow!(
				"Expected exactly one utxo [found: {}] for escrow output: {:?}",
				available_utxos.len(),
				available_utxos
			));
		};

		let input = temp_wallet.get_psbt_input(available_utxos[0].clone(), None, false)?;
		let outpoint = available_utxos[0].outpoint;
		Ok((input, outpoint))
	}

	/// assembles the keyspend payout transaction as PSBT (without signatures)
	pub async fn assemble_keyspend_payout_psbt(
		&self,
		payout_information: &PayoutData,
	) -> anyhow::Result<String> {
		let (escrow_utxo_psbt_input, escrow_utxo_outpoint) =
			self.get_escrow_utxo(&payout_information.escrow_output_descriptor)?;

		let (payout_psbt, _) = {
			let wallet = self.wallet.lock().await;
			let mut builder = wallet.build_tx();
			let (tx_fee_abs, tx_fee_abs_sat_per_user) = get_tx_fees_abs_sat(&self.backend)?;

			// why 264 wu?: see escrow_psbt.tx
			builder.add_foreign_utxo(escrow_utxo_outpoint, escrow_utxo_psbt_input, 264)?;

			builder.add_recipient(
				payout_information.payout_address_maker.script_pubkey(),
				payout_information.payout_amount_maker - tx_fee_abs_sat_per_user,
			);

			builder.add_recipient(
				payout_information.payout_address_taker.script_pubkey(),
				payout_information.payout_amount_taker - tx_fee_abs_sat_per_user,
			);

			builder.fee_absolute(tx_fee_abs);

			builder.finish()?
		};
		Ok(payout_psbt.serialize_hex())
	}

	/// Inserts the aggregated signature into the keyspend transaction and broadcasts it
	pub async fn broadcast_keyspend_tx(
		&self,
		keyspend_ctx: &KeyspendContext,
	) -> anyhow::Result<()> {
		// we need a bitcoin 0.32 psbt to access the taproot_hash_ty() method
		let bitcoin_032_psbt = bitcoin::Psbt::from_str(&keyspend_ctx.keyspend_psbt.to_string())?;
		debug!("Payout psbt: {}", bitcoin_032_psbt.to_string());

		// extract the unsigned transaction from the bitcoin 0.32 psbt
		let mut bitcoin_032_tx: bitcoin::Transaction = bitcoin_032_psbt.clone().extract_tx()?;

		// get a secp256k1::schnorr::Signature from the aggregated musig signature
		let secp_signature =
			bitcoin::secp256k1::schnorr::Signature::from_slice(&keyspend_ctx.agg_sig.to_bytes())?;

		let sighash_type = bitcoin_032_psbt.inputs[0].taproot_hash_ty()?;

		// assemble a rust bitcoin Signature from the secp signature and sighash type
		let rust_bitcoin_sig = bitcoin::taproot::Signature {
			signature: secp_signature,
			sighash_type,
		};

		// create a p2tr key spend witness from the rust bitcoin signature
		let witness = bitcoin::Witness::p2tr_key_spend(&rust_bitcoin_sig);

		// insert the witness into the transaction
		let escrow_input: &mut bitcoin::TxIn = &mut bitcoin_032_tx.input[0];
		escrow_input.witness = witness.clone();
		let signed_hex_tx = bitcoin::consensus::encode::serialize_hex(&bitcoin_032_tx);

		// convert the hex tx back into a bitcoin030 tx to be able to broadcast it with the bdk backend
		let bdk_bitcoin_030_tx: bdk::bitcoin::Transaction =
			deserialize(&hex::decode(signed_hex_tx.clone())?)?;

		self.backend.broadcast(&bdk_bitcoin_030_tx)?;
		debug!("Broadcasted keyspend tx: {}", signed_hex_tx);
		Ok(())
	}
}
