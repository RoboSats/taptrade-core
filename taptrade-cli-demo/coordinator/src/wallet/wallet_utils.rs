use super::*;
use bdk::{
	bitcoin::{psbt::Input, Address, Network},
	blockchain::GetTx,
	database::Database,
};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct PsbtInput {
	pub psbt_input: Input,
	pub utxo: bdk::bitcoin::OutPoint,
}

pub trait BondTx {
	fn input_sum<D: Database, B: GetTx>(&self, blockchain: &B, db: &D) -> Result<u64>;
	fn bond_output_sum(&self, bond_address: &str) -> Result<u64>;
	fn all_output_sum(&self) -> u64;
}

impl BondTx for Transaction {
	fn input_sum<D: Database, B: GetTx>(&self, blockchain: &B, db: &D) -> Result<u64> {
		let mut input_sum = 0;

		for input in self.input.iter() {
			let prev_tx = if let Some(prev_tx) = db.get_raw_tx(&input.previous_output.txid)? {
				prev_tx
			} else if let Some(prev_tx) = blockchain.get_tx(&input.previous_output.txid)? {
				prev_tx
			} else {
				return Err(anyhow!(VerifyError::MissingInputTx(
					input.previous_output.txid
				)));
			};

			let spent_output = prev_tx
				.output
				.get(input.previous_output.vout as usize)
				.ok_or(VerifyError::InvalidInput(input.previous_output))?;

			input_sum += spent_output.value;
		}
		if input_sum == 0 {
			return Err(anyhow!("Empty input sum in transaction"));
		}
		Ok(input_sum)
	}

	fn bond_output_sum(&self, bond_address: &str) -> Result<u64> {
		let bond_script = Address::from_str(bond_address)?
			.require_network(Network::Regtest)?
			.script_pubkey();

		for output in self.output.iter() {
			if output.script_pubkey == bond_script {
				return Ok(output.value);
			}
		}
		Err(anyhow!("No output to bond address in transaction"))
	}

	fn all_output_sum(&self) -> u64 {
		self.output.iter().map(|output| output.value).sum()
	}
}

pub fn csv_hex_to_bdk_input(inputs_csv_hex: &str) -> Result<Vec<PsbtInput>> {
	let mut inputs: Vec<PsbtInput> = Vec::new();
	for input_hex in inputs_csv_hex.split(',') {
		let binary = hex::decode(input_hex)?;
		let input: PsbtInput = bincode::deserialize(&binary)?;
		inputs.push(input);
	}
	if inputs.is_empty() {
		return Err(anyhow!("No inputs found in csv input value"));
	}
	Ok(inputs)
}
