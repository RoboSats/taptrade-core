use std::ops::Add;

/// construction of the transaction spending the escrow output after a successfull trade as keyspend transaction
use super::*;
use bdk::bitcoin::psbt::PartiallySignedTransaction;
use bdk::bitcoin::OutPoint;

fn get_tx_fees_abs_sat(blockchain_backend: &RpcBlockchain) -> Result<(u64, u64)> {
	let feerate = blockchain_backend.estimate_fee(6)?;
	let keyspend_payout_tx_size_vb = 140; // ~, always 1 input, 2 outputs

	let tx_fee_abs = feerate.fee_vb(keyspend_payout_tx_size_vb);

	Ok((tx_fee_abs, tx_fee_abs / 2))
}

impl<D: bdk::database::BatchDatabase> CoordinatorWallet<D> {
	pub async fn assemble_keyspend_payout_psbt(
		&self,
		escrow_output: OutPoint,
		payout_addresses: HashMap<Address, u64>,
	) -> Result<PartiallySignedTransaction> {
		let (payout_psbt, _) = {
			let wallet = self.wallet.lock().await;
			let mut builder = wallet.build_tx();
			let (tx_fee_abs, tx_fee_abs_sat_per_user) = get_tx_fees_abs_sat(&self.backend)?;

			builder.add_utxo(escrow_output)?;
			for (address, amount) in payout_addresses {
				builder.add_recipient(address.script_pubkey(), amount - tx_fee_abs_sat_per_user);
			}
			builder.fee_absolute(tx_fee_abs);

			builder.finish()?
		};
		Ok(payout_psbt)
	}
}
