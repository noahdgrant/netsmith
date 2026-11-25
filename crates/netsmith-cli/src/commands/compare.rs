use std::path::PathBuf;
use std::process::ExitCode;

use std::sync::Arc;

use clap::Args as ClapArgs;
use netsmith_core::job::Cancel;
use netsmith_editor::{Backend, Observer};
use netsmith_tools::compare::{Request, Verdict, compare_files};
use netsmith_tools::findings::Format;
use netsmith_workspace::{Workspace, render_compare};

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`compare` checks a design against another by connectivity (ADR 0017), for example a port against the KiCad design it came from. Each of `<A>` and `<B>` is a `.kicad_sch`, a `.kicad_pcb`, a KiCad netlist (the XML of `kicad-cli sch export netlist --format kicadxml`) or a `.nsm` file, which is built as `build` does, held to the pick lock beside it (`<file>.lock.toml`, read and never written). Net names and, by default, designators may differ between the two; components are matched by value, footprint (without its library) and how their pins are wired. A pin number is identity, except on a two-pad part without polarity (a footprint of KiCad's `R_`, `C_`, `L_` or `Fuse_` families, both pads on nets): its pads may be the other way round, and the match lists it as swapped (`swapped` in the JSON). A polarised capacitor (`CP_`), a diode, an LED and any other footprint keep their pin numbers. It prints `match` with the designators and net names that correspond, or each difference: a component with no counterpart, a pair that differs in value or footprint, a net whose pins differ (each pin in its own side's designators), a net with no counterpart.\n\nPower ports and flags (`#PWR`, `#FLG`) are not parts. A pin joined to nothing is on no net, so it differs only from a pin that is joined to something. A `.kicad_sch` is read with the sheets it places, from their files, with no KiCad (a schematic with a bus is refused: compare the netlist `kicad-cli` exports from it).\n\nThe exit code is 0 for a match, 1 for a difference and 2 for a mistake or a comparison that could not be decided (many interchangeable parts and a search past its bound, which is 20 refinements per component of the two designs and at least 20000; the answer says how many ran and which components and nets were still alike).\n\nWith `--ref` there are no `<A>` and `<B>`: the project in the current folder is compared with itself at a git ref, as the editor's compare view does (see `--ref`)."
)]
pub struct Args {
    /// The first design.
    #[arg(
        value_name = "A",
        required_unless_present = "git_ref",
        conflicts_with = "git_ref"
    )]
    pub a: Option<String>,
    /// The second design: `compare` takes two files.
    #[arg(
        value_name = "B",
        required_unless_present = "git_ref",
        conflicts_with = "git_ref"
    )]
    pub b: Option<String>,
    /// Compare the project with its state at this git ref (branch, tag, commit), not two designs.
    ///
    /// The project is the one found from the current folder.
    ///
    /// Says what changed in the circuit since the ref, as the editor's compare view does (ADR
    /// 0059): parts, values, nets, rails, modules, systems, the PCB, the bill of materials and
    /// problems, not which lines. The ref is read as the project is, with all its files, its
    /// catalogs and its dependencies' lock, and the other side is the files on disk, uncommitted
    /// changes included (`--ref HEAD` is the diff of the work tree), or `--against-ref`. Exit 0
    /// when the circuit is the same, 1 when it differs, 2 for a mistake (not in a git work
    /// tree, a ref that does not exist, a project that does not build at the ref).
    #[arg(
        long = "ref",
        value_name = "GIT REF",
        conflicts_with_all = ["map", "by_refdes", "ignore_unconnected", "loose_values"]
    )]
    pub git_ref: Option<String>,
    /// With `--ref`, compare with this second git ref instead of the files on disk.
    ///
    /// The other side is read out of git as `--ref` is, so a branch can be compared with
    /// another with nothing checked out.
    #[arg(long, value_name = "GIT REF", requires = "git_ref")]
    pub against_ref: Option<String>,
    /// The board of a `.nsm` file that has several (`Name`, or `System/member`); with `--ref`,
    /// the one board to compare.
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
    /// `json` holds the `verdict` and the pairs of a match or each difference; with `--ref` it is
    /// the document `GET /api/compare` of the editor answers (`changed`, `summary`, `boards`,
    /// `systems`, `problems`).
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

/// Compares the two designs by connectivity.
pub fn run(args: Args) -> ExitCode {
    if let Some(rev) = &args.git_ref {
        return run_ref(
            rev,
            args.against_ref.as_deref(),
            args.board.as_deref(),
            args.format.into(),
        );
    }
    // clap holds both designs to be present without `--ref`.
    let (Some(a), Some(b)) = (&args.a, &args.b) else {
        eprintln!("error: `compare` takes two designs, or `--ref`");
        return ExitCode::from(2);
    };
    // A design that is a source is built, so its project's release is looked at (issue #1657);
    // a file that cannot be read is the comparison's to say.
    for source in [a, b].into_iter().filter(|p| p.ends_with(".nsm")) {
        let _ = super::common::resolve(Some(source));
    }
    run_compare(
        &Request {
            a,
            b,
            board: args.board.as_deref(),
            map: args.map.as_deref(),
            by_refdes: args.by_refdes,
            ignore_unconnected: args.ignore_unconnected,
            loose_values: args.loose_values,
        },
        args.format.into(),
    )
}

/// `--ref`: the project at the ref against the files on disk or `--against-ref`, with the code of
/// the editor's `GET /api/compare` (issue #1097). Exit 0 when the circuit is the same, 1 when
/// it differs, 2 when the project cannot be compared (no git, no such ref, a ref that does not
/// build).
fn run_ref(rev: &str, against: Option<&str>, board: Option<&str>, format: Format) -> ExitCode {
    let target = match super::common::resolve(None) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {}", e.trim_end());
            return ExitCode::from(2);
        }
    };
    // Not analysed here: the head is read when the comparison needs it, and not at all with
    // `--against-ref`.
    let workspace = match Workspace::open_deferred(target) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("error: {}", e.trim_end());
            return ExitCode::from(2);
        }
    };
    let observe: Observer = Arc::new(|_| {});
    let compared = workspace.compare_against(rev, against, board, &observe, &Cancel::new());
    match compared {
        Ok(compare) => {
            let broken = compare.error.is_some();
            if format == Format::Json {
                match serde_json::to_string_pretty(&compare) {
                    Ok(json) => println!("{json}"),
                    Err(e) => {
                        eprintln!("error: {e}");
                        return ExitCode::from(2);
                    }
                }
            } else if broken {
                eprint!("{}", render_compare(&compare));
            } else {
                print!("{}", render_compare(&compare));
            }
            if broken {
                ExitCode::from(2)
            } else if compare.changed {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("error: {}", e.message.trim_end());
            ExitCode::from(2)
        }
    }
}

/// Exit 0 when the designs match, 1 when they differ, 2 when they could not be compared or
/// the comparison could not be decided.
fn run_compare(request: &Request, format: Format) -> ExitCode {
    match compare_files(request) {
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
    use crate::commands::common::testing::Leaf;
    use crate::commands::common::testing::{kind, parse};
    use crate::commands::{common, compare};
    use clap::error::ErrorKind;

    #[test]
    fn compare_takes_two_files_and_map_or_by_refdes() {
        assert_eq!(
            parse(&[
                "board",
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
            Ok(Leaf::Compare(compare::Args {
                a: Some("a.xml".into()),
                b: Some("b.xml".into()),
                git_ref: None,
                against_ref: None,
                board: Some("Led".into()),
                map: Some("m.txt".into()),
                by_refdes: false,
                ignore_unconnected: true,
                loose_values: true,
                format: common::Format::Text,
            }))
        );
        assert_eq!(
            kind(&["board", "compare", "a.nsm"]),
            ErrorKind::MissingRequiredArgument
        );
        assert_eq!(
            kind(&["board", "compare", "a", "b", "c"]),
            ErrorKind::UnknownArgument
        );
        assert_eq!(
            kind(&["board", "compare", "a", "b", "--map", "m", "--by-refdes"]),
            ErrorKind::ArgumentConflict
        );
        assert!(parse(&["board", "compare", "a", "b", "--map"]).is_err());
        assert!(parse(&["board", "build", "a.nsm", "--loose-values"]).is_err());
    }

    #[test]
    fn compare_takes_a_git_ref_in_place_of_two_designs() {
        assert_eq!(
            parse(&[
                "board",
                "compare",
                "--ref",
                "main",
                "--against-ref",
                "v1",
                "--board",
                "Led",
                "--format",
                "json"
            ]),
            Ok(Leaf::Compare(compare::Args {
                a: None,
                b: None,
                git_ref: Some("main".into()),
                against_ref: Some("v1".into()),
                board: Some("Led".into()),
                map: None,
                by_refdes: false,
                ignore_unconnected: false,
                loose_values: false,
                format: common::Format::Json,
            }))
        );
        // The ref takes the place of both designs, and `--against-ref` needs a ref.
        assert_eq!(
            kind(&["board", "compare", "--ref", "main", "a.nsm", "b.nsm"]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(
            kind(&["board", "compare", "--against-ref", "main"]),
            ErrorKind::MissingRequiredArgument
        );
        assert_eq!(
            kind(&["board", "compare", "--ref", "main", "--by-refdes"]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(
            kind(&["board", "compare"]),
            ErrorKind::MissingRequiredArgument
        );
    }
}
