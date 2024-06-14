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
