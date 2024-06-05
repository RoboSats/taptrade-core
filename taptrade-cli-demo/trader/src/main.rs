#![allow(unused_variables, unused_imports, dead_code)]
mod cli;
mod communication;
mod trading;

use core::panic;

use cli::{parse_cli_args, CliSettings};
use anyhow::Result;
use communication::fetch_offer;

fn main() -> Result<()> {
    let mode = parse_cli_args();
    dbg!(mode);
    
    if let CliSettings::Maker(maker_config) = mode {
        trading::maker::run_maker(maker_config)?;
    } else if CliSettings::Taker(taker_data) = mode {
        trading::taker::run_taker(taker_data)?;
    } else {
        panic!("Wrong mode selected!")
    }
    
    
    Ok(())
}
