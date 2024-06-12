use crate::cli::TraderSettings;
use crate::communication::api::OfferCreationResponse;
use crate::wallet::{
	bond::Bond,
	load_wallet,
	musig2::{MuSigData, MusigNonce},
};
use anyhow::Result;
use bdk::{
	bitcoin::block,
	blockchain::{Blockchain, ElectrumBlockchain},
	electrum_client::Client,
	wallet::AddressIndex::LastUnused,
};

pub fn run_maker(maker_config: &TraderSettings) -> Result<()> {
	let blockchain = ElectrumBlockchain::from(Client::new(&maker_config.electrum_endpoint)?);

	// let offer_conditions = OfferCreationResponse::fetch(maker_config)?;
	let offer_conditions = OfferCreationResponse {
		// hardcoded for testing, locking_address is owned by .env xprv
		locking_amount: 90000,
		bond_address: "tb1pfdvgfzwp8vhmelpv8w9kezz7nsmxw68jz6yehgze6mzx0t6r9t2qv9ynmm".to_string(),
	};

	let wallet = load_wallet(maker_config, &blockchain)?; // initialize the wallet with xprv

	let bond = Bond::assemble(&wallet, &offer_conditions, maker_config)?; // assemble the Bond transaction for offer creation
																	  // blockchain.broadcast(&bond.extract_tx())?;  // publish bond to be mined for testing
	let payout_pubkey = wallet.get_address(bdk::wallet::AddressIndex::LastUnused)?;

	let musig = MuSigData::create(&maker_config.wallet_xprv, wallet.secp_ctx())?;

	// dbg!(&bond.extract_tx().txid());
	Ok(())
}

pub fn run_taker(taker_config: &TraderSettings) -> Result<()> {
	let blockchain = ElectrumBlockchain::from(Client::new(&taker_config.electrum_endpoint)?);

	// panic!("Taker to be implemented!");

	Ok(())
}
