mod trader;
mod coordinator;
mod cli;

use cli::parse_cli_args;

fn main() {
    let cli_args = parse_cli_args();

}

// use clap to parse mode (taker, maker or coordinator), communication endpoint (URL or PID or something else), electrum server
// https://www.youtube.com/watch?v=Ot3qCA3Iv_8
