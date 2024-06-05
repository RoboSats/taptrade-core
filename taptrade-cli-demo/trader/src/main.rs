#![allow(unused_variables, unused_imports, dead_code)]
mod cli;
mod communication;
mod trading;

use core::panic;

use cli::CliSettings;
use anyhow::Result;
use communication::fetch_offer;

fn main() -> Result<()> {
    let mode = CliSettings::parse_cli_args()?;
    dbg!("CLI input :", &mode);

    // if let CliSettings::Maker(maker_data) = &mode {
    //     trading::maker::run_maker(maker_data)?;
    // } else if let CliSettings::Taker(taker_data) = &mode {
    //     trading::taker::run_taker(taker_data)?;
    // } else {
    //     panic!("Wrong mode selected!")
    // }
    Ok(())
}
