use netsmith_fab::{KicadCli, KicadCliExe};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_tools::compare::{Request, Verdict, compare_files};
use netsmith_tools::findings::Format;

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`compare` checks a design against another by connectivity (ADR 0017), for example a port against the KiCad design it came from. Each of `<A>` and `<B>` is a `.kicad_sch`, a `.kicad_pcb`, a KiCad netlist (the XML of `kicad-cli sch export netlist --format kicadxml`) or a `.nsm` file, which is built as `build` does. Net names and, by default, designators may differ between the two; components are matched by value, footprint (without its library) and how their pins are wired. A pin number is identity, except on a two-pad part without polarity (a footprint of KiCad's `R_`, `C_`, `L_` or `Fuse_` families, both pads on nets): its pads may be the other way round, and the match lists it as swapped (`swapped` in the JSON). A polarised capacitor (`CP_`), a diode, an LED and any other footprint keep their pin numbers. It prints `match` with the designators and net names that correspond, or each difference: a component with no counterpart, a pair that differs in value or footprint, a net whose pins differ (each pin in its own side's designators), a net with no counterpart.\n\nPower ports and flags (`#PWR`, `#FLG`) are not parts. A pin joined to nothing is on no net, so it differs only from a pin that is joined to something. Reading a `.kicad_sch` needs KiCad's command-line tool, found as for `fab --cross-check`.\n\nThe exit code is 0 for a match, 1 for a difference and 2 for a mistake or a comparison that could not be decided (many interchangeable parts and a search past its bound, which is 20 refinements per component of the two designs and at least 20000; the answer says how many ran and which components and nets were still alike)."
)]
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
    ///
    /// The rest are matched by wiring. Blank lines and `#` comments are ignored.
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
    ///
    /// `json` holds the `verdict` and the pairs of a match or each difference.
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

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{kind, parse};
    use crate::commands::{Command, common, compare};
    use clap::error::ErrorKind;

    #[test]
    fn compare_takes_two_files_and_map_or_by_refdes() {
        assert_eq!(
            parse(&[
                "compare",
                "--board",
                "Led",
                "a.xml",
                "--map",
                "m.txt",
                "b.xml",
                "--ignore-unconnected",
                "--loose-values"
            ]),
            Ok(Command::Compare(compare::Args {
                a: "a.xml".into(),
                b: "b.xml".into(),
                board: Some("Led".into()),
                map: Some("m.txt".into()),
                by_refdes: false,
                ignore_unconnected: true,
                loose_values: true,
                format: common::Format::Text,
            }))
        );
        assert_eq!(
            kind(&["compare", "a.nsm"]),
            ErrorKind::MissingRequiredArgument
        );
        assert_eq!(
            kind(&["compare", "a", "b", "c"]),
            ErrorKind::UnknownArgument
        );
        assert_eq!(
            kind(&["compare", "a", "b", "--map", "m", "--by-refdes"]),
            ErrorKind::ArgumentConflict
        );
        assert!(parse(&["compare", "a", "b", "--map"]).is_err());
        assert!(parse(&["build", "a.nsm", "--loose-values"]).is_err());
    }
}
