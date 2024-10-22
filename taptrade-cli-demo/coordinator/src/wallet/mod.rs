pub mod escrow_psbt;
pub mod payout_tx;
pub mod wallet_utils;
// pub mod verify_tx;
#[cfg(test)]
mod wallet_tests;

pub use self::escrow_psbt::*;
use super::*;
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
	pub escrow_tx_txid: String,
	pub escrow_output_descriptor: String,
	pub coordinator_xonly_escrow_pk: String,
	pub escrow_amount_maker_sat: u64,
	pub escrow_amount_taker_sat: u64,
	pub escrow_fee_sat_per_participant: u64,
}

/// struct to hold the necessary data to construct the bond transaction
#[derive(PartialEq, Debug, Clone)]
pub struct BondRequirements {
	pub bond_address: String,
	pub locking_amount_sat: u64,
	pub min_input_sum_sat: u64,
}

/// sets up the coordinator bdk wallet from the env variables
pub async fn init_coordinator_wallet() -> Result<CoordinatorWallet<MemoryDatabase>> {
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
		network: Network::Regtest,
		// derives wallet name from xprv/wallet
		wallet_name: bdk::wallet::wallet_name_from_descriptor(
			Bip86(wallet_xprv, KeychainKind::External),
			Some(Bip86(wallet_xprv, KeychainKind::Internal)),
			Network::Regtest,
			&secp_context,
		)?,
		sync_params: None,
	};
	let json_rpc_client = Arc::new(Client::new(
		&rpc_config.url,
		rpc_config.auth.clone().into(),
	)?);
	let json_rpc_client_clone = Arc::clone(&json_rpc_client);
	// start new mempool instance
	let mempool = MempoolHandler::new(json_rpc_client_clone).await;
	let backend = RpcBlockchain::from_config(&rpc_config)?;
	let wallet = Wallet::new(
		Bip86(wallet_xprv, KeychainKind::External),
		Some(Bip86(wallet_xprv, KeychainKind::Internal)),
		Network::Regtest,
		MemoryDatabase::new(),
	)?;

	wallet
		.sync(&backend, SyncOptions::default())
		.context("Connection to blockchain server failed.")?; // we could also use Esplora to make this async
	info!("{}", wallet.get_balance()?);
	Ok(CoordinatorWallet {
		wallet: Arc::new(Mutex::new(wallet)),
		backend: Arc::new(backend),
		json_rpc_client,
		mempool: Arc::new(mempool),
		coordinator_feerate: env::var("COORDINATOR_FEERATE")?.parse::<f64>()?,
	})
}

impl<D: bdk::database::BatchDatabase> CoordinatorWallet<D> {
	/// shutdown function to end the mempool task
	pub async fn shutdown(&self) {
		debug!("Shutting down wallet");
		self.mempool.shutdown().await;
	}

	/// get a new address of the coordinator wallet
	pub async fn get_new_address(&self) -> Result<String> {
		let wallet = self.wallet.lock().await;
		let address = wallet.get_address(bdk::wallet::AddressIndex::New)?;
		Ok(address.address.to_string())
	}

	/// used to validate submitted bond transactions using the same logic as in the continoous monitoring
	/// puts the bond in a dummy MonitoringBond struct use the existing logic
	pub async fn validate_bond_tx_hex(
		&self,
		bond_tx_hex: &str,
		requirements: &BondRequirements,
	) -> Result<()> {
		debug!("Validating bond in validate_bond_tx_hex(): {}", bond_tx_hex);
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
				trace!("Deserializing bond in validate_bonds()");
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

				// check if the fee rate is high enough
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

		// now test all bonds with bitcoin core rpc testmempoolaccept, this would be triggered if the bond inputs are spent in another
		// transaction on the chain (e.g. out of band mining)
		let json_rpc_client = self.json_rpc_client.clone();
		let bonds_clone = Arc::clone(&bonds);
		let mempool_accept_future = tokio::task::spawn_blocking(move || {
			test_mempool_accept_bonds(json_rpc_client, bonds_clone)
		});
		let invalid_bonds_testmempoolaccept = mempool_accept_future.await??;
		invalid_bonds.extend(invalid_bonds_testmempoolaccept.into_iter());

		// looks up inputs in the mempool, would be triggered if a transaction appears in the mempool that spends the bond inputs
		let mempool_bonds = self.mempool.lookup_mempool_inputs(&bonds).await?;
		invalid_bonds.extend(mempool_bonds.into_iter());
		debug!("validate_bond_tx_hex(): Bond validation done.");
		Ok(invalid_bonds)
	}

	/// Publishes the bond transaction to the mempool as punishment
	pub fn publish_bond_tx_hex(&self, bond: &str) -> Result<()> {
		warn!("publish_bond_tx_hex(): publishing cheating bond tx!");
		let blockchain = &*self.backend;
		let tx: Transaction = deserialize(&hex::decode(bond)?)?;

		blockchain.broadcast(&tx)?;
		Ok(())
	}

	/// derive a new address from the coordinator wallet and extract the xonly taproot pubkey for use in the
	/// trade protocol
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

/// lookup a MonitoringBond by its txid in a Vec of MonitoringBonds
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

/// tests all passed MonitoringBonds against bitcoin core rpc testmempoolaccept and returns a HashMap of invalid bonds
fn test_mempool_accept_bonds(
	json_rpc_client: Arc<Client>,
	bonds: Arc<Vec<MonitoringBond>>,
) -> Result<HashMap<Vec<u8>, (MonitoringBond, anyhow::Error)>> {
	let mut invalid_bonds: HashMap<Vec<u8>, (MonitoringBond, anyhow::Error)> = HashMap::new();

	// split bonds into chunks of 25 to avoid hitting the maxmimum allowed size of the rpc call
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
