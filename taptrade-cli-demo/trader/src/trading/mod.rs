// use maker_utils;
// use taker_utils;
// use utils;
use anyhow::Result;
use crate::cli::TraderSettings;
use crate::communication::fetch_offer;

pub fn run_maker(maker_config: &TraderSettings) -> Result<()> {
    let offer_conditions = fetch_offer(&maker_config.coordinator_endpoint)?;
    // maker_utils::maker(offer_conditions, maker_config)
}