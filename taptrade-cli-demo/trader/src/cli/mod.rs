use crate::wallet::get_wallet_xprv;
use anyhow::{anyhow, Result};
use bdk::bitcoin::bip32::ExtendedPrivKey;
use sha2::{Digest, Sha256};
use std::{
	env,
	io::{self, Write},
	time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug)]
pub struct Coordinator;

#[derive(Debug)]
pub enum OfferType {
	Buy(u64),
	Sell(u64),
}

#[derive(Debug)]
pub struct TraderSettings {
	pub electrum_endpoint: String,
	pub coordinator_endpoint: String,
	pub robosats_robohash_hex: String,
	pub trade_type: OfferType,
	pub payout_address: String,
	pub bond_ratio: u8,
	pub wallet_xprv: ExtendedPrivKey,
	pub duration_unix_ts: u64, // until when the order should stay available
}

#[derive(Debug)]
pub enum CliSettings {
	Coordinator(Coordinator),
	Taker(TraderSettings),
	Maker(TraderSettings),
}

fn hash256(input: &String) -> [u8; 32] {
	let mut hasher = Sha256::new();
	hasher.update(input.as_bytes());
	hasher.finalize().into()
}

impl OfferType {
	pub fn value(&self) -> u64 {
		match self {
			OfferType::Buy(value) => *value,
			OfferType::Sell(value) => *value,
		}
	}
	pub fn is_buy_order(&self) -> bool {
		match self {
			OfferType::Buy(_) => true,
			OfferType::Sell(_) => false,
		}
	}
}

impl CliSettings {
	fn get_user_input(prompt: &str) -> String {
		let mut buffer = String::new();
		print!("{}", prompt);
		io::stdout().flush().unwrap();
		io::stdin()
			.read_line(&mut buffer)
			.expect("Failed to read line!");
		buffer.trim().to_string()
	}

	fn get_trade_type(trade_type: Option<String>) -> OfferType {
		let trade_type = match trade_type {
			Some(value) => value,
			None => Self::get_user_input("Do you want to buy or sell satoshis: "),
		};
		match trade_type.as_str() {
			"buy" => OfferType::Buy(
				Self::get_user_input("How many satoshi do you want to buy: ")
					.parse()
					.unwrap(),
			),
			"sell" => OfferType::Sell(
				Self::get_user_input("How many satoshi do you want to sell: ")
					.parse()
					.unwrap(),
			),
			_ => panic!("Wrong offer type, you can only buy or sell"),
		}
	}

	// parses the hours input string and returns the unix timestamp + the trade duration in seconds
	fn hours_to_ts(hours: &str) -> Result<u64> {
		let duration: u64 = hours.parse()?;
		Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() + duration * 3600)
	}

	fn get_trader_settings() -> Result<TraderSettings> {
		let electrum_endpoint = Self::get_user_input("Enter electrum endpoint: ");
		let coordinator_endpoint = Self::get_user_input("Enter coordinator endpoint: ");
		let robosats_robohash_hex = hex::encode(hash256(&Self::get_user_input(
			"Enter your robosats robot key: ", // just for testing purposes, to be improved to the real robohash spec
		)));
		let trade_type: OfferType = Self::get_trade_type(None);
		let payout_address = Self::get_user_input(
			"Enter a payout address for refunded bonds or your trade payout: ",
		); // bdk can be used for validation
		let bond_ratio: u8 = Self::get_user_input("Enter bond ration in [2, 50]%: ").parse()?;
		let wallet_xprv = Self::check_xprv_input(Some(Self::get_user_input(
			"Enter funded testnet wallet xprv or leave empty to generate: ",
		)))?;
		let duration_unix_ts: u64 = Self::hours_to_ts(&Self::get_user_input(
			"How many hours should the offer stay online: ",
		))?;
		Ok(TraderSettings {
			electrum_endpoint,
			coordinator_endpoint,
			robosats_robohash_hex,
			trade_type,
			payout_address,
			bond_ratio,
			wallet_xprv,
			duration_unix_ts,
		})
	}

	fn check_xprv_input(cli_input: Option<String>) -> Result<ExtendedPrivKey> {
		if let Some(user_input) = cli_input {
			if !(user_input.is_empty()) {
				return get_wallet_xprv(Some(user_input));
			}
		};
		get_wallet_xprv(None)
	}

	fn load_from_env(filename: &str) -> Result<TraderSettings> {
		dotenv::from_filename(filename)?;
		Ok(TraderSettings {
			electrum_endpoint: env::var("ELECTRUM_ENDPOINT")?,
			coordinator_endpoint: env::var("COORDINATOR_ENDPOINT")?,
			robosats_robohash_hex: env::var("ROBOHASH_HEX")?,
			trade_type: Self::get_trade_type(Some(env::var("TRADE_TYPE")?)),
			payout_address: env::var("PAYOUT_ADDRESS")?,
			bond_ratio: env::var("BOND_RATIO")?.parse()?,
			wallet_xprv: Self::check_xprv_input(Some(env::var("XPRV")?))?,
			duration_unix_ts: Self::hours_to_ts(&env::var("OFFER_DURATION_HOURS")?)?,
		})
	}

	fn parse_trader_settings(maybe_filename: &str) -> Result<TraderSettings> {
		match Self::get_user_input("Load from .env (y/N): ").trim() {
			"y" => Self::load_from_env(maybe_filename),
			"N" => Self::get_trader_settings(),
			_ => Err(anyhow!("Not a valid input!")),
		}
	}

	pub fn parse_cli_args() -> Result<Self> {
		let mode = Self::get_user_input("Enter mode, 'taker' or 'maker': ");

		match mode.to_lowercase().as_str() {
			"maker" => Ok(Self::Maker(Self::parse_trader_settings("maker.env")?)),
			"taker" => Ok(Self::Taker(Self::parse_trader_settings("taker.env")?)),
			_ => Err(anyhow!("Either select maker or taker!")),
		}
	}
}

// old cli parser using clap

// use clap::{command, Arg, Command, ArgMatches};

// trait ArgMatchesParser {
// 	fn parse_into_enum(&self) -> CliSettings;
// }

// impl ArgMatchesParser for ArgMatches {
// 	fn parse_into_enum(&self) -> CliSettings {
// 		if let Some(_mode) = self.subcommand_matches("coordinator") {
// 			CliSettings::Coordinator(Coordinator { })
// 		} else if let Some(mode) = self.subcommand_matches("trader") {
// 			let trader_settings = TraderSettings {
// 				coordinator_endpoint: mode.get_one::<String>("coordinator-ep")
// 										.expect("Coordinator endpoint not provided!").clone(),
// 				electrum_endpoint: mode.get_one::<String>("electrum-ep")
// 										.expect("Electrum endpoint not provided").clone()
// 			};
// 			if mode.contains_id("maker") {
// 				CliSettings::Maker( trader_settings )
// 			} else if mode.contains_id("taker") {
// 				CliSettings::Taker( trader_settings )
// 			} else {
// 				panic!("Wrong arguments for Trader mode!")
// 			}
// 		} else {
// 			panic!("Select either coordinator or trader mode!")
// 		}
// 	}
// }

// pub fn parse_cli_args() -> CliSettings {
// 	command!()
// 	.about("RoboSats taproot onchain trade pipeline CLI demonstrator. Don't use with real funds.")
// 	.subcommand(
// 		Command::new("coordinator")
// 				.about("Run in coordinator mode.")
// 	)
// 	.subcommand(
// 	Command::new("trader")
// 		.about("Two available trader modes: Maker and Taker. Select one and provide Coordinator and Electum endpoint")
// 		.arg(
// 			Arg::new("taker")
// 				.short('t')
// 				.long("taker")
// 				.help("Run program as taker")
// 				.num_args(0)
// 				.conflicts_with("maker")
// 		)
// 		.arg (
// 			Arg::new("maker")
// 				.short('m')
// 				.long("maker")
// 				.num_args(0)
// 				.help("Run program as maker")
// 				.conflicts_with("taker")
// 		)
// 		.arg(
// 			Arg::new("coordinator-ep")
// 					.short('p')
// 					.long("endpoint")
// 					.required(true)
// 					.help("Communication endpoint of the coordinator to connect to")
// 		)
// 		.arg(
// 			Arg::new("electrum-ep")
// 					.short('e')
// 					.long("electrum")
// 					.required(true)
// 					.help("URL of the electrum endpoint")
// 		)
// 	)
// 	.arg_required_else_help(true)
// 	.get_matches()
// 	.parse_into_enum()
// }
