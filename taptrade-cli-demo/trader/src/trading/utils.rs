use super::maker_utils::*;
use super::*;

#[derive(Debug)]
pub struct ActiveOffer {
	pub order_id_hex: String,
	pub bond_locked_until_timestamp: u128,
	pub used_musig_config: MuSigData,
	pub used_bond: PartiallySignedTransaction,
	pub expected_payout_address: AddressInfo,
}

impl ActiveOffer {
	pub fn onchain_assembly(
		trading_wallet: &TradingWallet,
		offer_conditions: &BondRequirementResponse,
		trader_config: &TraderSettings,
	) -> Result<(PartiallySignedTransaction, MuSigData, AddressInfo)> {
		let trading_wallet = &trading_wallet.wallet;
		let bond = Bond::assemble(trading_wallet, &offer_conditions, trader_config)?;
		let payout_address: AddressInfo =
			trading_wallet.get_address(bdk::wallet::AddressIndex::LastUnused)?;
		let mut musig_data =
			MuSigData::create(&trader_config.wallet_xprv, trading_wallet.secp_ctx())?;

		Ok((bond, musig_data, payout_address))
	}
}
