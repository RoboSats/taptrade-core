use super::maker_utils::*;
use super::*;

#[derive(Debug)]
pub struct ActiveOffer {
	pub offer_id_hex: String,
	pub used_musig_config: MuSigData,
	pub used_bond: PartiallySignedTransaction,
	pub expected_payout_address: AddressInfo,
	pub escrow_psbt: Option<PartiallySignedTransaction>,
}
