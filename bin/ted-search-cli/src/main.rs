use clap::Parser;
use ted_search::{create_method, SedFactory};

mod cli;

use cli::Cli;

fn main() {
    let cli = Cli::parse();

    let lower_bound_method = match cli.method {

    }
}
