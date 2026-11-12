//! `netsmith`: the command line. `netsmith --help` lists the commands.

use std::process::ExitCode;

use clap::Parser;
use netsmith_cli::commands;

#[derive(Parser)]
#[command(
    name = "netsmith",
    version,
    about = "Electronics as code.",
    after_help = "NETSMITH_TIMINGS=1 prints the time of each stage of a run to stderr."
)]
struct Cli {
    #[command(subcommand)]
    command: commands::Command,
}

fn main() -> ExitCode {
    // A usage mistake exits 2, as netsmith's "cannot run" does; help and the version exit 0.
    commands::run(Cli::parse().command)
}
