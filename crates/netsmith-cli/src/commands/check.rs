use std::process::ExitCode;

use clap::Args as ClapArgs;

use super::build::{BuildRun, run_build};
use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "The exit code is 0 when every board builds, 1 when one does not, and 2 when the command cannot run (a file that cannot be read, no boards)."
)]
#[command(args_override_self = true)]
pub struct Args {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// Make a guideline finding an error for this run.
    #[arg(long)]
    pub strict: bool,
    #[command(flatten)]
    pub output: common::Output,
}

/// `build` that writes nothing, not even the pin file: a request without a pin is given one in
/// memory, as the editor does.
pub fn run(args: Args) -> ExitCode {
    run_build(&BuildRun {
        file: args.file.as_deref(),
        out: None,
        strict: args.strict,
        locked: false,
        format: args.output.format.into(),
        notes: args.output.notes.into(),
        write: false,
    })
}
