use super::utils::*;
use super::*;

impl ActiveOffer {
	pub fn take(
		trading_wallet: &TradingWallet,
		taker_config: &TraderSettings,
		offer: &PublicOffer,
	) -> Result<ActiveOffer> {
		let bond_conditions: BondRequirementResponse = offer.take(taker_config)?;
		let (bond, mut musig_data, payout_address) =
			trading_wallet.trade_onchain_assembly(&bond_conditions, taker_config)?;
		// let trading_psbt =
	}

	pub fn wait_on_maker(&self) -> Result<()> {
		// tbd
		Ok(())
	}
}
