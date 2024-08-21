use bdk::FeeRate;

/// construction of the transaction spending the escrow output after a successfull trade as keyspend transaction
use super::*;
use bitcoin;

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

	pub async fn broadcast_keyspend_tx(
		&self,
		keyspend_ctx: &KeyspendContext,
	) -> anyhow::Result<()> {
		let bitcoin_032_psbt = bitcoin::Psbt::from_str(&keyspend_ctx.keyspend_psbt.to_string())?;
		debug!("Payout psbt: {}", bitcoin_032_psbt.to_string());
		let mut bitcoin_032_tx: bitcoin::Transaction = bitcoin_032_psbt.clone().extract_tx()?;

		let secp_signature =
			bitcoin::secp256k1::schnorr::Signature::from_slice(&keyspend_ctx.agg_sig.to_bytes())?;

		let sighash_type = bitcoin_032_psbt.inputs[0].taproot_hash_ty()?;

		let rust_bitcoin_sig = bitcoin::taproot::Signature {
			signature: secp_signature,
			sighash_type,
		};
		// let unsigned_tx_hex = bitcoin::consensus::encode::serialize_hex(&bitcoin_032_tx);

		let witness = bitcoin::Witness::p2tr_key_spend(&rust_bitcoin_sig);
		// let mut tx_clone = bitcoin_032_tx.clone();

		let escrow_input: &mut bitcoin::TxIn = &mut bitcoin_032_tx.input[0];
		escrow_input.witness = witness.clone();
		let signed_hex_tx = bitcoin::consensus::encode::serialize_hex(&bitcoin_032_tx);

		// convert the hex tx back into a bitcoin030 tx
		let bdk_bitcoin_030_tx: bdk::bitcoin::Transaction =
			deserialize(&hex::decode(signed_hex_tx.clone())?)?;

		self.backend.broadcast(&bdk_bitcoin_030_tx)?;
		debug!("Broadcasted keyspend tx: {}", signed_hex_tx);
		Ok(())
	}
}
