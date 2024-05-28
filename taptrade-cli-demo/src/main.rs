use clap::{command, Arg};
mod trader;
mod coordinator;

fn main() {
    let cli_args = command!()
                    .about("RoboSats taproot onchain trade pipeline CLI demonstrator. Don't use with real funds.")
                    .arg(
                        Arg::new("mode")
                                .short('m')
                                .long("mode")
                                .required(true)
                                .help("Mode: coordinator, maker or taker"))
                    .arg(
                        Arg::new("endpoint")
                                .short('p')
                                .long("endpoint")
                                .help("Communication endpoint of the coordinator to connect to")
                                // .conflicts_with("coordinator")
                                )  // only required for traders
                    .get_matches();

}

// use clap to parse mode (taker, maker or coordinator), communication endpoint (URL or PID or something else), electrum server
// https://www.youtube.com/watch?v=Ot3qCA3Iv_8
// clap tutorial (min 32)
