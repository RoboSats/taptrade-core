pub mod escrow_psbt;
pub mod wallet_utils;
// pub mod verify_tx;
#[cfg(test)]
mod wallet_tests;

use self::escrow_psbt::*;
use super::*;
use anyhow::Context;
use bdk::{
	bitcoin::{
		self,
		address::Payload,
		bip32::ExtendedPrivKey,
		consensus::encode::deserialize,
		key::{secp256k1, XOnlyPublicKey},
		taproot::TapLeaf,
		Address,
		Network::Regtest,
		Transaction,
	},
	bitcoincore_rpc::{Client, RawTx, RpcApi},
	blockchain::{rpc::Auth, Blockchain, ConfigurableBlockchain, RpcBlockchain, RpcConfig},
	database::MemoryDatabase,
	sled::{self, Tree},
	template::Bip86,
	wallet::verify::*,
	KeychainKind, SyncOptions, Wallet,
};
use coordinator::mempool_monitoring::MempoolHandler;
use std::{collections::HashMap, str::FromStr};
use std::{fmt, ops::Deref};
// use verify_tx::*;

#[derive(Clone)]
pub struct CoordinatorWallet<D: bdk::database::BatchDatabase> {
	pub wallet: Arc<Mutex<Wallet<D>>>,
	pub backend: Arc<RpcBlockchain>,
	pub json_rpc_client: Arc<bdk::bitcoincore_rpc::Client>,
	pub mempool: Arc<MempoolHandler>,
	pub coordinator_feerate: f64,
}

#[derive(Debug)]
pub struct EscrowPsbt {
	pub escrow_psbt_hex: String,
	pub escrow_output_descriptor: String,
	pub escrow_tx_fee_address: String,
	pub coordinator_xonly_escrow_pk: String,
	pub escrow_amount_maker_sat: u64,
	pub escrow_amount_taker_sat: u64,
	pub escrow_fee_sat_per_participant: u64,
}

#[derive(PartialEq, Debug, Clone)]
pub struct BondRequirements {
	pub bond_address: String,
	pub locking_amount_sat: u64,
	pub min_input_sum_sat: u64,
}

pub async fn init_coordinator_wallet() -> Result<CoordinatorWallet<sled::Tree>> {
	let wallet_xprv = ExtendedPrivKey::from_str(
		&env::var("WALLET_XPRV").context("loading WALLET_XPRV from .env failed")?,
	)?;
	let secp_context = secp256k1::Secp256k1::new();
	let rpc_config = RpcConfig {
		url: env::var("BITCOIN_RPC_ADDRESS_PORT")?.to_string(),
		auth: Auth::UserPass {
			username: env::var("BITCOIN_RPC_USER")?,
			password: env::var("BITCOIN_RPC_PASSWORD")?,
		},
		network: Regtest,
		// wallet_name: env::var("BITCOIN_RPC_WALLET_NAME")?,
		wallet_name: bdk::wallet::wallet_name_from_descriptor(
			Bip86(wallet_xprv, KeychainKind::External),
			Some(Bip86(wallet_xprv, KeychainKind::Internal)),
			Regtest,
			&secp_context,
		)?,
		sync_params: None,
	};
	let json_rpc_client = Arc::new(Client::new(
		&rpc_config.url,
		rpc_config.auth.clone().into(),
	)?);
	let json_rpc_client_clone = Arc::clone(&json_rpc_client);
	let mempool = MempoolHandler::new(json_rpc_client_clone).await;
	let backend = RpcBlockchain::from_config(&rpc_config)?;
	// let backend = EsploraBlockchain::new(&env::var("ESPLORA_BACKEND")?, 1000);
	let sled_db = sled::open(env::var("BDK_DB_PATH")?)?.open_tree("default_wallet")?;
	let wallet = Wallet::new(
		Bip86(wallet_xprv, KeychainKind::External),
		Some(Bip86(wallet_xprv, KeychainKind::Internal)),
		bitcoin::Network::Regtest,
		sled_db,
	)?;

	wallet
		.sync(&backend, SyncOptions::default())
		.context("Connection to blockchain server failed.")?; // we could also use Esplora to make this async
	dbg!(wallet.get_balance()?);
	Ok(CoordinatorWallet {
		wallet: Arc::new(Mutex::new(wallet)),
		backend: Arc::new(backend),
		json_rpc_client,
		mempool: Arc::new(mempool),
		coordinator_feerate: env::var("COORDINATOR_FEERATE")?.parse::<f64>()?,
	})
}

impl<D: bdk::database::BatchDatabase> CoordinatorWallet<D> {
	pub async fn get_new_address(&self) -> Result<String> {
		let wallet = self.wallet.lock().await;
		let address = wallet.get_address(bdk::wallet::AddressIndex::New)?;
		Ok(address.address.to_string())
	}

	pub async fn validate_bond_tx_hex(
		&self,
		bond_tx_hex: &str,
		requirements: &BondRequirements,
	) -> Result<()> {
		debug!("Validating bond in validate_bond_tx_hex()");
		let dummy_monitoring_bond = MonitoringBond {
			bond_tx_hex: bond_tx_hex.to_string(),
			trade_id_hex: "0".to_string(),
			robot: vec![0],
			requirements: requirements.clone(),
			table: Table::Memory,
		};
		let invalid_bond = self
			.validate_bonds(Arc::new(vec![dummy_monitoring_bond]))
			.await?;
		if !invalid_bond.is_empty() {
			let (_, error) = invalid_bond.values().next().unwrap();
			return Err(anyhow!(error.to_string()));
		}
		Ok(())
	}

	// validate bond (check amounts, valid inputs, correct addresses, valid signature, feerate)
	// also check if inputs are confirmed already
	// bdk::blockchain::compact_filters::Mempool::iter_txs() -> Vec(Tx) to check if contained in mempool
	// blockchain::get_tx to get input
	pub async fn validate_bonds(
		&self,
		bonds: Arc<Vec<MonitoringBond>>,
	) -> Result<HashMap<Vec<u8>, (MonitoringBond, anyhow::Error)>> {
		let mut invalid_bonds: HashMap<Vec<u8>, (MonitoringBond, anyhow::Error)> = HashMap::new();
		let blockchain = &*self.backend;
		{
			let wallet = self.wallet.lock().await;
			for bond in bonds.as_ref().iter() {
				let tx: Transaction = deserialize(&hex::decode(&bond.bond_tx_hex)?)?;
				debug!("Validating bond in validate_bonds()");
				// we need to test this with signed and invalid/unsigned transactions
				// checks signatures and inputs
				if let Err(e) = verify_tx(&tx, &*wallet.database(), blockchain) {
					invalid_bonds.insert(bond.id()?, (bond.clone(), anyhow!(e)));
					continue;
				}

				// check if the tx has the correct input amounts (have to be >= trading amount)
				let input_sum: u64 = match tx.input_sum(blockchain, &*wallet.database()) {
					Ok(amount) => {
						if amount < bond.requirements.min_input_sum_sat {
							invalid_bonds.insert(
								bond.id()?,
								(
									bond.clone(),
									anyhow!("Bond input sum too small: {}", amount),
								),
							);
							continue;
						}
						amount
					}
					Err(e) => {
						return Err(anyhow!(e));
					}
				};
				// check if bond output to us is big enough
				match tx.bond_output_sum(&bond.requirements.bond_address) {
					Ok(amount) => {
						if amount < bond.requirements.locking_amount_sat {
							invalid_bonds.insert(
								bond.id()?,
								(
									bond.clone(),
									anyhow!("Bond output sum too small: {}", amount),
								),
							);
							continue;
						}
						amount
					}
					Err(e) => {
						return Err(anyhow!(e));
					}
				};
				if ((input_sum - tx.all_output_sum()) / tx.vsize() as u64) < 200 {
					invalid_bonds.insert(
						bond.id()?,
						(
							bond.clone(),
							anyhow!(
								"Bond fee rate too low: {}",
								(input_sum - tx.all_output_sum()) / tx.vsize() as u64
							),
						),
					);
					continue;
				}
			}
		}

		// now test all bonds with bitcoin core rpc testmempoolaccept
		let json_rpc_client = self.json_rpc_client.clone();
		let bonds_clone = Arc::clone(&bonds);
		let mempool_accept_future = tokio::task::spawn_blocking(move || {
			test_mempool_accept_bonds(json_rpc_client, bonds_clone)
		});
		let invalid_bonds_testmempoolaccept = mempool_accept_future.await??;
		invalid_bonds.extend(invalid_bonds_testmempoolaccept.into_iter());

		let mempool_bonds = self.mempool.lookup_mempool_inputs(&bonds).await?;
		invalid_bonds.extend(mempool_bonds.into_iter());
		debug!("validate_bond_tx_hex(): Bond validation done.");
		Ok(invalid_bonds)
	}

	pub fn publish_bond_tx_hex(&self, bond: &str) -> Result<()> {
		warn!("publish_bond_tx_hex(): publishing cheating bond tx!");
		let blockchain = &*self.backend;
		let tx: Transaction = deserialize(&hex::decode(bond)?)?;

		blockchain.broadcast(&tx)?;
		Ok(())
	}

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
			let temp_wallet = Wallet::new(
				&escrow_output_descriptor,
				None,
				bitcoin::Network::Regtest,
				MemoryDatabase::new(),
			)?;
			let escrow_address = temp_wallet
				.get_address(bdk::wallet::AddressIndex::New)?
				.address;

			// using absolute fee for now, in production we should come up with a way to determine the tx weight
			// upfront and substract the fee from the change outputs
			let tx_fee_abs = 10000;

			let change_amount_maker = maker_psbt_input_data.input_sum()?
				- (escrow_amount_maker_sat + escrow_fee_sat_per_participant + tx_fee_abs / 2);
			let change_amount_taker = taker_psbt_input_data.input_sum()?
				- (escrow_amount_taker_sat + escrow_fee_sat_per_participant + tx_fee_abs / 2);

			let amount_escrow = escrow_amount_maker_sat + escrow_amount_taker_sat;
			let mut builder = temp_wallet.build_tx();
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
				builder.add_foreign_utxo(input.utxo, input.psbt_input.clone(), 264);
			}
			for input in taker_psbt_input_data.escrow_input_utxos.iter() {
				builder.add_foreign_utxo(input.utxo, input.psbt_input.clone(), 264);
			}
			builder.finish()?
		};

		Ok(EscrowPsbt {
			escrow_psbt_hex: escrow_psbt.to_string(),
			escrow_output_descriptor,
			escrow_tx_fee_address: escrow_coordinator_fee_address.to_string(),
			coordinator_xonly_escrow_pk: coordinator_escrow_pk.to_string(),
			escrow_amount_maker_sat,
			escrow_amount_taker_sat,
			escrow_fee_sat_per_participant,
		})
	}

	pub async fn get_coordinator_taproot_pk(&self) -> Result<XOnlyPublicKey> {
		let wallet = self.wallet.lock().await;
		let address = wallet.get_address(bdk::wallet::AddressIndex::New)?;
		let pubkey = if let Payload::WitnessProgram(witness_program) = &address.payload {
			witness_program.program().as_bytes()
		} else {
			return Err(anyhow!("Getting taproot pubkey failed"));
		};
		Ok(XOnlyPublicKey::from_slice(pubkey)?)
	}
}

fn search_monitoring_bond_by_txid(
	monitoring_bonds: &Vec<MonitoringBond>,
	txid: &str,
) -> Result<MonitoringBond> {
	for bond in monitoring_bonds {
		let bond_tx: Transaction = deserialize(&hex::decode(&bond.bond_tx_hex)?)?;
		if bond_tx.txid().to_string() == txid {
			return Ok(bond.clone());
		}
	}
	Err(anyhow!("Bond not found in monitoring bonds"))
}

fn test_mempool_accept_bonds(
	json_rpc_client: Arc<Client>,
	bonds: Arc<Vec<MonitoringBond>>,
) -> Result<HashMap<Vec<u8>, (MonitoringBond, anyhow::Error)>> {
	let mut invalid_bonds: HashMap<Vec<u8>, (MonitoringBond, anyhow::Error)> = HashMap::new();

	let raw_bonds: Vec<Vec<String>> = bonds
		.iter()
		.map(|bond| bond.bond_tx_hex.clone().raw_hex())
		.collect::<Vec<String>>()
		.chunks(25) // Assuming `raw_hex()` returns a String or &str
		.map(|chunk| chunk.to_vec())
		.collect();

	let mut test_mempool_accept_res = Vec::new();
	for raw_bonds_subvec in raw_bonds {
		test_mempool_accept_res.extend(
			json_rpc_client
				.deref()
				.test_mempool_accept(&raw_bonds_subvec)?,
		);
	}

	for res in test_mempool_accept_res {
		if !res.allowed {
			let invalid_bond: MonitoringBond =
				search_monitoring_bond_by_txid(&bonds, &res.txid.to_string())?;
			invalid_bonds.insert(
				invalid_bond.id()?,
				(
					invalid_bond,
					anyhow!(
						"Bond not accepted by testmempoolaccept: {:?}",
						res.reject_reason
							.unwrap_or("rejected by testmempoolaccept".to_string())
					),
				),
			);
		};
	}
	Ok(invalid_bonds)
}

impl fmt::Debug for CoordinatorWallet<Tree> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("CoordinatorWallet")
			.field("wallet", &self.wallet)
			// Since ElectrumBlockchain doesn't implement Debug, we can't automatically derive it.
			// Instead, we can print a placeholder or simply omit it from the debug output.
			.field("backend", &"ElectrumBlockchain (Debug not implemented)")
			.finish()
	}
}
