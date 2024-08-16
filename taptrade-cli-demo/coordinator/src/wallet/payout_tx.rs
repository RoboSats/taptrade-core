/// construction of the transaction spending the escrow output after a successfull trade as keyspend transaction
use super::*;

fn get_tx_fees_abs_sat(blockchain_backend: &RpcBlockchain) -> Result<(u64, u64)> {
	let feerate = blockchain_backend.estimate_fee(6)?;
	let keyspend_payout_tx_size_vb = 140; // ~, always 1 input, 2 outputs

	let tx_fee_abs = feerate.fee_vb(keyspend_payout_tx_size_vb);

	Ok((tx_fee_abs, tx_fee_abs / 2))
}

// pub fn aggregate_partial_signatures(
// 	maker_sig_hex: &str,
// 	taker_sig_hex: &str,
// ) -> anyhow::Result<String> {
// 	Ok(())
// }

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
			return Err(anyhow!("Expected exactly one utxo for escrow output"));
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
}
