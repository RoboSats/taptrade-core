#![allow(unused_variables, unused_imports, dead_code)]
mod cli;
mod communication;
mod trading;
mod wallet;

use core::panic;

use cli::CliSettings;
use anyhow::{anyhow, Result};

fn start_trade_pipeline(cli_input: &CliSettings) -> Result<()> {
    if let CliSettings::Maker(maker_data) = cli_input {
        Ok(trading::run_maker(maker_data)?)
    } else if let CliSettings::Taker(taker_data) = cli_input {
        Err(anyhow!("not implemented!"))
        // trading::taker::run_taker(taker_data)?;
    } else {
        Err(anyhow!("Wrong mode selected!"))
    }
}

fn main() -> Result<()> {
    // env_logger::builder().filter_level(log::LevelFilter::Debug).init(); // enable to show extended BDK debug messages

    let mode = CliSettings::parse_cli_args()?;
    dbg!("CLI input :", &mode);
    start_trade_pipeline(&mode)?;

    Ok(())
}
