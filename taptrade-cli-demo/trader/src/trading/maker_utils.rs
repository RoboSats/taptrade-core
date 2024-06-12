use crate::cli::TraderSettings;
use crate::communication::api::OfferCreationResponse;
use crate::wallet::{
	bond::Bond,
	musig2::{MuSigData, MusigNonce},
	TradingWallet,
};
use anyhow::Result;
use bdk::database::MemoryDatabase;

pub struct ActiveOffer {}

impl ActiveOffer {
	pub fn create(
		trading_wallet: &TradingWallet,
		maker_config: &TraderSettings,
	) -> Result<ActiveOffer> {
		let trading_wallet = &trading_wallet.wallet;

		// let offer_conditions = OfferCreationResponse::fetch(maker_config)?;
		let offer_conditions = OfferCreationResponse {
			// hardcoded for testing, locking_address is owned by .env xprv
			locking_amount_sat: 90000,
			bond_address: "tb1pfdvgfzwp8vhmelpv8w9kezz7nsmxw68jz6yehgze6mzx0t6r9t2qv9ynmm"
				.to_string(),
		};
		let bond = Bond::assemble(trading_wallet, &offer_conditions, maker_config)?;
		let payout_pubkey = trading_wallet.get_address(bdk::wallet::AddressIndex::LastUnused)?;
		let musig_data = MuSigData::create(&maker_config.wallet_xprv, trading_wallet.secp_ctx())?;

		Ok(ActiveOffer {})
	}
}
