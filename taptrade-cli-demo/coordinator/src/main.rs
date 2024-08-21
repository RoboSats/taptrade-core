mod communication;
mod coordinator;
mod database;
mod wallet;

use std::{
	collections::{HashMap, HashSet},
	env, fmt,
	net::SocketAddr,
	ops::Deref,
	str::FromStr,
	sync::{Arc, RwLock},
	time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context, Result};
use axum::{
	http::StatusCode,
	response::{IntoResponse, Response},
	routing::{get, post},
	Extension, Json, Router,
};
use bdk::{
	bitcoin::{
		address::Payload,
		bip32::ExtendedPrivKey,
		consensus::encode::deserialize,
		key::{secp256k1, XOnlyPublicKey},
		psbt::{Input, PartiallySignedTransaction, Prevouts},
		sighash::SighashCache,
		Address, Network, OutPoint, Transaction, TxIn, Txid,
	},
	bitcoincore_rpc::{
		jsonrpc::Error as JsonRpcError, Client, Error as CoreRpcError, RawTx, RpcApi,
	},
	blockchain::{rpc::Auth, Blockchain, ConfigurableBlockchain, GetTx, RpcBlockchain, RpcConfig},
	database::{Database, MemoryDatabase},
	descriptor::Descriptor,
	miniscript::{descriptor::TapTree, policy::Concrete, Tap, ToPublicKey},
	sled::Tree,
	template::Bip86,
	wallet::verify::*,
	KeychainKind, SignOptions, SyncOptions, Wallet,
};
use communication::{api::*, api_server, communication_utils::*, handler_errors::*};
use coordinator::{
	bond_monitoring::*, coordinator_utils::*, mempool_monitoring::MempoolHandler,
	tx_confirmation_monitoring::update_transaction_confirmations, *,
};
use database::CoordinatorDB;
use dotenvy::dotenv;
use futures_util::StreamExt;
use log::{debug, error, info, trace, warn};
use musig2::{
	secp256k1::PublicKey as MuSig2PubKey, AggNonce as MusigAggNonce, BinaryEncoding, KeyAggContext,
	LiftedSignature, PartialSignature, PubNonce as MusigPubNonce,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Row, Sqlite};
use tokio::{
	net::TcpListener,
	sync::{oneshot, Mutex},
};
use validator::{Validate, ValidationError};
use wallet::{escrow_psbt::*, wallet_utils::*, *};

pub struct Coordinator {
	pub coordinator_db: Arc<CoordinatorDB>,
	pub coordinator_wallet: Arc<CoordinatorWallet<MemoryDatabase>>,
}

// populate .env with values before starting
#[tokio::main]
async fn main() -> Result<()> {
	env_logger::builder()
		.filter_module("coordinator", log::LevelFilter::Trace)
		.filter_level(log::LevelFilter::Info)
		.init();
	dotenv().ok();
	debug!("Starting coordinator");

	// Initialize the database pool
	let coordinator = Arc::new(Coordinator {
		coordinator_db: Arc::new(CoordinatorDB::init().await?),
		coordinator_wallet: Arc::new(init_coordinator_wallet().await?),
	});

	// begin monitoring bonds
	let coordinator_ref = Arc::clone(&coordinator);
	tokio::spawn(async move {
		loop {
			if let Err(e) = monitor_bonds(coordinator_ref.clone()).await {
				error!("Error in monitor_bonds: {:?}", e);
				// Optionally add a delay before retrying
				tokio::time::sleep(std::time::Duration::from_secs(5)).await;
			}
		}
	});

	// begin monitoring escrow transactions confirmations
	let coordinator_ref = Arc::clone(&coordinator);
	tokio::spawn(async move { update_transaction_confirmations(coordinator_ref).await });

	// Start the API server
	api_server(coordinator).await?;
	Ok(())
}
