use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct OfferCreationResponse {
    pub locking_address: String,
    pub locking_amount: u32,  // validate
}

#[derive(Serialize)]
pub struct OrderRequest {
    pub robohash_base91: String,
    pub amount_satoshi: u64,
    pub order_type: String, // buy or sell
    pub bond_ratio: u8 // [2, 50]
}
