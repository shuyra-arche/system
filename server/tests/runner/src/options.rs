use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Run native System scenarios against a qualified RV64 Kernel")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) scenario: Scenario,
}

#[derive(Subcommand)]
pub(crate) enum Scenario {
    Runtime(Common),
    Entry(Entry),
    Loader(Loader),
    Application(Application),
    Handoff(Application),
}

#[derive(Clone, Args)]
pub(crate) struct Common {
    #[arg(long)]
    pub(crate) kernel: PathBuf,
    #[arg(long)]
    pub(crate) output: PathBuf,
    #[arg(long, value_parser = ["debug", "release"], default_value = "release")]
    pub(crate) profile: String,
    #[arg(long, num_args = 1.., value_parser = clap::value_parser!(u8).range(1..=4))]
    pub(crate) harts: Vec<u8>,
    #[arg(long)]
    pub(crate) kernel_sha256: Option<String>,
    #[arg(long)]
    pub(crate) system_revision: Option<String>,
}

#[derive(Args)]
pub(crate) struct Entry {
    #[command(flatten)]
    pub(crate) common: Common,
    #[arg(long, conflicts_with = "stall")]
    pub(crate) abort: bool,
    #[arg(long)]
    pub(crate) stall: bool,
}

#[derive(Args)]
pub(crate) struct Loader {
    #[command(flatten)]
    pub(crate) common: Common,
    #[arg(long)]
    pub(crate) toolchain_receipt: Option<PathBuf>,
    #[arg(long, value_parser = ["std-waits", "toolchain-std"], default_value = "std-waits")]
    pub(crate) toolchain_fixture: String,
}

#[derive(Args)]
pub(crate) struct Application {
    #[command(flatten)]
    pub(crate) common: Common,
    #[arg(long, value_parser = ["debug", "release"], default_value = "release")]
    pub(crate) kernel_profile: String,
    #[arg(long, num_args = 1..)]
    pub(crate) scenarios: Vec<String>,
}
