use clap::{command, Arg, Command, ArgMatches};

#[derive(Debug)]
pub struct Coordinator { 

}

#[derive(Debug)]
pub struct TraderSettings {
	pub electrum_endpoint: String,
	pub coordinator_endpoint: String,
}

#[derive(Debug)]
pub enum CliSettings {
	Coordinator(Coordinator),
	Taker(TraderSettings),
	Maker(TraderSettings)
}

trait ArgMatchesParser {
	fn parse_into_enum(&self) -> CliSettings;
}

impl ArgMatchesParser for ArgMatches {
	fn parse_into_enum(&self) -> CliSettings {
		if let Some(_mode) = self.subcommand_matches("coordinator") {
			CliSettings::Coordinator(Coordinator { })
		} else if let Some(_mode) = self.subcommand_matches("trader") {
			let trader_settings = TraderSettings {
				coordinator_endpoint: self.get_one::<String>("coordinator-ep")
										.expect("Coordinator endpoint not provided!").clone(),
				electrum_endpoint: self.get_one::<String>("electrum-ep")
										.expect("Electrum endpoint not provided").clone()
			};
			if self.contains_id("maker") {
				CliSettings::Maker( trader_settings )
			} else if self.contains_id("taker") {
				CliSettings::Taker( trader_settings )
			} else {
				panic!("Wrong arguments for Trader mode!")
			}
		} else {
			panic!("Select either coordinator or trader mode!")
		}
	}
}

pub fn parse_cli_args() -> CliSettings {
	command!()
	.about("RoboSats taproot onchain trade pipeline CLI demonstrator. Don't use with real funds.")
	.subcommand(
		Command::new("coordinator")
				.about("Run in coordinator mode.")
	)
	.subcommand(
	Command::new("trader")
		.about("Two available trader modes: Maker and Taker. Select one and provide Coordinator and Electum endpoint")
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
			Arg::new("coordinator-ep")
					.short('p')
					.long("endpoint")
					.required(true)
					.help("Communication endpoint of the coordinator to connect to")
		)
		.arg(
			Arg::new("electrum-ep")
					.short('e')
					.long("electrum")
					.required(true)
					.help("URL of the electrum endpoint")
		)
	)
	.arg_required_else_help(true)
	.get_matches()
	.parse_into_enum()
}
