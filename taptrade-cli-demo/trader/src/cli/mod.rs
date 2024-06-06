use std::io::{self, Write};
use anyhow::{anyhow, Result};
use sha2::{Sha256, Digest};
use std::env;

use crate::wallet::{generate_descriptor_wallet, WalletDescriptors};

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
    pub robosats_robohash_base91: String,
    pub trade_type: OfferType,
    pub payout_address: String,
    pub bond_ratio: u8,
    pub funded_wallet_descriptor: WalletDescriptors,
}

#[derive(Debug)]
pub enum CliSettings {
	Coordinator(Coordinator),
	Taker(TraderSettings),
	Maker(TraderSettings)
}

fn hash256(input: &String) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hasher.finalize().into()
}

// Robosats uses base91 encoded sha256 hash of the private robot key
fn bytes_to_base91(input: &[u8; 32]) -> String {
    let encoded_robohash: String = base91::EncodeIterator::new(input.iter().copied())
                                            .as_char_iter()
                                            .collect();
    encoded_robohash
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
            "buy" => OfferType::Buy(Self::get_user_input("How many satoshi do you want to buy: ").parse().unwrap()),
            "sell" => OfferType::Sell(Self::get_user_input("How many satoshi do you want to sell: ").parse().unwrap()),
            _ => panic!("Wrong offer type, you can only buy or sell"),
        }
    }

    fn get_trader_settings() -> Result<TraderSettings> {
        let electrum_endpoint = Self::get_user_input("Enter electrum endpoint: ");
        let coordinator_endpoint = Self::get_user_input("Enter coordinator endpoint: ");
        let robosats_robohash_base91 = bytes_to_base91(&hash256(&Self::get_user_input("Enter your robosats robot key: ")));
        let trade_type: OfferType = Self::get_trade_type(None);
        let payout_address = Self::get_user_input("Enter a payout address for refunded bonds or your trade payout: ");  // bdk can be used for validation
        let bond_ratio: u8 = Self::get_user_input("Enter bond ration in [2, 50]%: ").parse()?;
        let funded_wallet_descriptor = Self::get_wallet_descriptors(Some(Self::get_user_input("Enter funded testnet wallet xprv or leave empty to generate: ")))?;
        Ok(TraderSettings {
            electrum_endpoint,
            coordinator_endpoint,
            robosats_robohash_base91,
            trade_type,
            payout_address,
            bond_ratio,
            funded_wallet_descriptor
        })
    }

    fn get_wallet_descriptors(cli_input: Option<String>) -> Result<WalletDescriptors> {
        if let Some(user_input) = cli_input {
            if !(user_input.is_empty()) {
                return generate_descriptor_wallet(Some(user_input));
            }
        };
        generate_descriptor_wallet(None)
    }

    fn load_from_env() -> Result<TraderSettings> {
        dotenv::from_filename(".env")?;
        Ok(TraderSettings {
            electrum_endpoint: env::var("ELECTRUM_ENDPOINT")?,
            coordinator_endpoint: env::var("COORDINATOR_ENDPOINT")?,
            robosats_robohash_base91: env::var("ROBOHASH_BASE91")?,
            trade_type: Self::get_trade_type(Some(env::var("TRADE_TYPE")?)),
            payout_address: env::var("PAYOUT_ADDRESS")?,
            bond_ratio: env::var("BOND_RATIO")?.parse()?,
            funded_wallet_descriptor: Self::get_wallet_descriptors(Some(env::var("XPRV")?))?,
        })
    }

    pub fn parse_cli_args() -> Result<Self> {
        let mode = Self::get_user_input("Enter mode, 'taker' or 'maker': ");
        let trader_settings = match Self::get_user_input("Load from .env (y/N): ").trim() {
            "y" => Self::load_from_env()?,
            "N" => Self::get_trader_settings()?,
            _ => return Err(anyhow!("Not a valid input!")),
        };
        match mode.to_lowercase().as_str() {
            "maker" => Ok(Self::Maker(trader_settings)),
            "taker" => Ok(Self::Taker(trader_settings)),
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
