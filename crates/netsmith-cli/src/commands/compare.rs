use netsmith_fab::{KicadCli, KicadCliExe};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith::compare::{Request, Verdict, compare_files};
use netsmith::findings::Format;

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "Each design is a `.kicad_sch`, a `.kicad_pcb`, a KiCad netlist (XML) or a `.nsm` file, which is built as `build` does; parts are matched by value, footprint and wiring (ADR 0017). The exit code is 0 for a match, 1 for a difference and 2 for a mistake or a comparison that could not be decided."
)]
#[command(args_override_self = true)]
pub struct Args {
    /// The first design.
    #[arg(value_name = "A")]
    pub a: String,
    /// The second design: `compare` takes two files.
    #[arg(value_name = "B")]
    pub b: String,
    /// The board of a `.nsm` file that has several (`Name`, or `System/member`).
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// A file of `A_ref B_ref` lines that fix pairs of designators.
    #[arg(long, value_name = "FILE", conflicts_with = "by_refdes")]
    pub map: Option<PathBuf>,
    /// Match a part only with the part of the same designator.
    #[arg(long)]
    pub by_refdes: bool,
    /// Leave out nets of a single pin on both sides.
    #[arg(long)]
    pub ignore_unconnected: bool,
    /// Values that read as the same number agree (`4k7` and `4.7k`).
    #[arg(long)]
    pub loose_values: bool,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

/// Compares the two designs by connectivity.
pub fn run(args: Args) -> ExitCode {
    run_compare(
        &Request {
            a: &args.a,
            b: &args.b,
            board: args.board.as_deref(),
            map: args.map.as_deref(),
            by_refdes: args.by_refdes,
            ignore_unconnected: args.ignore_unconnected,
            loose_values: args.loose_values,
        },
        args.format.into(),
    )
}

/// Exit 0 when the designs match, 1 when they differ, 2 when they could not be compared or
/// the comparison could not be decided.
fn run_compare(request: &Request, format: Format) -> ExitCode {
    let kicad = KicadCliExe::locate();
    match compare_files(request, kicad.as_ref().map(|k| k as &dyn KicadCli)) {
        Ok(done) if format == Format::Json => {
            print!("{}", done.json);
            match done.verdict {
                Verdict::Match => ExitCode::SUCCESS,
                Verdict::Different => ExitCode::FAILURE,
                Verdict::Undecided => ExitCode::from(2),
            }
        }
        Ok(done) => match done.verdict {
            Verdict::Match => {
                print!("{}", done.text);
                ExitCode::SUCCESS
            }
            Verdict::Different => {
                print!("{}", done.text);
                ExitCode::FAILURE
            }
            Verdict::Undecided => {
                eprint!("{}", done.text);
                ExitCode::from(2)
            }
        },
        Err(e) => {
            eprintln!("error: {}", e.trim_end());
            ExitCode::from(2)
        }
    }
}
