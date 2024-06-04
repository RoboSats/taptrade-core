mod coordinator;
mod cli;
mod communication;

use cli::parse_cli_args;
use communication::webserver;

fn main() {
    webserver();
    let mode = parse_cli_args();
    dbg!(mode);
}

// test with cargo run -- trader --maker --endpoint "taptrade-coordinator.com:5432" --electrum "electrum-server.com:50002"
