use super::*;

#[derive(Debug)]
pub struct ActiveOffer {
	pub offer_id_hex: String,
	pub used_musig_config: MuSigData,
	pub used_bond: PartiallySignedTransaction,
	pub expected_payout_address: AddressInfo,
	pub escrow_psbt: Option<PartiallySignedTransaction>,
}

impl ActiveOffer {
	// polls till the other party signed the trade transaction and it got confirmed.
	// once the coordinator signals OfferReady the fiat exchange can begin
	pub fn wait_on_trade_ready_confirmation(self, trader_config: &TraderSettings) -> Result<Self> {
		IsOfferReadyRequest::poll(trader_config, &self)?;
		Ok(self)
	}
}
