mod cli;
mod communication;

use cli::parse_cli_args;

fn main() {
    let mode = parse_cli_args();
    dbg!(mode);
}
