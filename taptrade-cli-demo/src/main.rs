mod trader;
mod coordinator;
mod cli;

use cli::parse_cli_args;

fn main() {
    let cli_args = parse_cli_args();
    dbg!(cli_args);
}
