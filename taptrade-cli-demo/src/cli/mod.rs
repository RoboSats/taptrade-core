use clap::{command, Arg, Command, ArgMatches};

use crate::coordinator;

pub struct cli_settings {
	pub coordinator: bool,
	pub maker: bool,
	pub taker: bool,
	pub c_endpoint: String,
	pub electrum_ep: String,
}

trait ArgMatchesParser {

}

pub fn parse_cli_args() -> ArgMatches {
	command!()
	.about("RoboSats taproot onchain trade pipeline CLI demonstrator. Don't use with real funds.")
	.subcommand(
		Command::new("coordinator")
	)
	.subcommand(
		Command::new("trader")
		.arg(
			Arg::new("taker")
				.short('t')
				.long("taker")
				.help("Run program as taker")
				.num_args(0)
				.conflicts_with("maker")
		)
		.arg (
			Arg::new("maker")
				.short('m')
				.long("maker")
				.num_args(0)
				.help("Run program as maker")
				.conflicts_with("taker")
		)
		.arg(
			Arg::new("endpoint")
					.short('p')
					.long("endpoint")
					.required(true)
					.help("Communication endpoint of the coordinator to connect to")
		)
	)
	.get_matches()
}
