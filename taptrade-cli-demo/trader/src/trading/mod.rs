// use maker_utils;
// use taker_utils;
// use utils;
use anyhow::Result;
use crate::cli::TraderSettings;
use crate::communication::api::OfferCreationResponse;
use crate::wallet::load_wallet;


pub fn run_maker(maker_config: &TraderSettings) -> Result<()> {
    // let offer_conditions = OfferCreationResponse::fetch(maker_config)?;
    load_wallet(maker_config)?;
    // maker_utils::maker(offer_conditions, maker_config)

    Ok(())
}
