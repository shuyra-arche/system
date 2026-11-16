use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

mod runner;

#[derive(Parser)]
#[command(about = "Run native System scenarios against a qualified RV64 Kernel")]
struct Cli {
    #[command(subcommand)]
    scenario: Scenario,
}

#[derive(Subcommand)]
enum Scenario {
    Runtime(Common),
    Entry(Entry),
    Loader(Loader),
    Application(Application),
    Handoff(Application),
}

#[derive(Clone, Args)]
struct Common {
    #[arg(long)]
    kernel: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, value_parser = ["debug", "release"], default_value = "release")]
    profile: String,
    #[arg(long, num_args = 1.., value_parser = clap::value_parser!(u8).range(1..=4))]
    harts: Vec<u8>,
    #[arg(long)]
    kernel_sha256: Option<String>,
    #[arg(long)]
    system_revision: Option<String>,
}

#[derive(Args)]
struct Entry {
    #[command(flatten)]
    common: Common,
    #[arg(long, conflicts_with = "stall")]
    abort: bool,
    #[arg(long)]
    stall: bool,
}

#[derive(Args)]
struct Loader {
    #[command(flatten)]
    common: Common,
    #[arg(long)]
    toolchain_receipt: Option<PathBuf>,
    #[arg(long, value_parser = ["std-waits", "toolchain-std"], default_value = "std-waits")]
    toolchain_fixture: String,
}

#[derive(Args)]
struct Application {
    #[command(flatten)]
    common: Common,
    #[arg(long, value_parser = ["debug", "release"], default_value = "release")]
    kernel_profile: String,
    #[arg(long, num_args = 1..)]
    scenarios: Vec<String>,
}

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
