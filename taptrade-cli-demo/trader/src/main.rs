#![allow(unused_variables, unused_imports, dead_code)]
mod cli;
mod communication;
mod trading;
mod wallet;

use anyhow::{anyhow, Result};
use cli::CliSettings;
use core::panic;

fn start_trade_pipeline(cli_input: &CliSettings) -> Result<()> {
	match cli_input {
		CliSettings::Maker(maker_config) => trading::run_maker(maker_config),
		CliSettings::Taker(taker_config) => trading::run_taker(taker_config),
		_ => Err(anyhow!(
			"Wrong trading mode selected, not implemented: {:?}",
			cli_input
		)),
	}
}

fn main() -> Result<()> {
	// env_logger::builder().filter_level(log::LevelFilter::Debug).init(); // enable to show extended BDK debug messages

	let mode = CliSettings::parse_cli_args()?;
	dbg!("CLI input :", &mode);
	start_trade_pipeline(&mode)?;

	Ok(())
}
