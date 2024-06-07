// use maker_utils;
// use taker_utils;
// use utils;

use anyhow::Result;
use crate::cli::TraderSettings;
use crate::communication::api::OfferCreationResponse;
use crate::wallet::{load_wallet, bond::Bond};



pub fn run_maker(maker_config: &TraderSettings) -> Result<()> {
    let offer_conditions = OfferCreationResponse::fetch(maker_config)?;

    let offer_conditions = OfferCreationResponse {
        
    };
    let wallet = load_wallet(maker_config)?;
    let bond = Bond::assemble(&wallet, &offer_conditions, maker_config)?;
    dbg!(bond);
    Ok(())
}
