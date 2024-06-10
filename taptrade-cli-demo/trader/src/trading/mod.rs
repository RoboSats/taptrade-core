// use maker_utils;
// use taker_utils;
// use utils;

use anyhow::Result;
use crate::cli::TraderSettings;
use crate::communication::api::OfferCreationResponse;
use crate::wallet::{load_wallet, bond::Bond};


pub fn run_maker(maker_config: &TraderSettings) -> Result<()> {
    // let offer_conditions = OfferCreationResponse::fetch(maker_config)?;
    let offer_conditions = OfferCreationResponse {
        locking_amount: 90000,
        bond_address: "tb1pfdvgfzwp8vhmelpv8w9kezz7nsmxw68jz6yehgze6mzx0t6r9t2qv9ynmm".to_string(),
    };

    let wallet = load_wallet(maker_config)?;

    let bond = Bond::assemble(&wallet, &offer_conditions, maker_config)?;
    dbg!(bond.serialize_hex());
    Ok(())
}
