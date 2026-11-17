//! `netsmith`: the command line. `netsmith --help` lists the commands.

use std::process::ExitCode;

use clap::Parser;
use netsmith_cli::commands;

#[derive(Parser)]
#[command(
    name = "netsmith",
    version,
    about = "Electronics as code.",
    after_help = commands::AFTER_HELP
)]
struct Cli {
    #[command(subcommand)]
    command: commands::Command,
}

fn main() -> ExitCode {
    // A command or flag of an earlier release is an error that names the one to write (#1090).
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    if let Some(moved) = commands::moved::moved(&args) {
        eprint!("{}", moved.message());
        return ExitCode::from(2);
    }
    // A usage mistake exits 2, as netsmith's "cannot run" does; help and the version exit 0.
    commands::run(Cli::parse().command)
}
