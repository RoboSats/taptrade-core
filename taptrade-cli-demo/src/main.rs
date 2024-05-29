mod trader;
mod coordinator;
mod cli;

use cli::parse_cli_args;

fn main() {
    let mode = parse_cli_args();
    dbg!(mode);
}

// test with cargo run -- trader --maker --endpoint "taptrade-coordinator.com:5432" --electrum "electrum-server.com:50002"
