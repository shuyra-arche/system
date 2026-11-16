use clap::Parser;

mod options;
mod runner;

use options::{Cli, Scenario};

fn main() {
    let result = match Cli::parse().scenario {
        Scenario::Runtime(args) => runner::runtime(args),
        Scenario::Entry(args) => runner::entry(args),
        Scenario::Loader(args) => runner::loader(args),
        Scenario::Application(args) => runner::application(args, false),
        Scenario::Handoff(args) => runner::application(args, true),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
