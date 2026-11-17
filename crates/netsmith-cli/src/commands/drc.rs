//! `netsmith board drc` (issue #1095): netsmith's own design rules check, alone. The check is
//! `netsmith_fabricate::drc`, which is the one `fab` runs (`netsmith_fab::check`).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_fabricate::{drc, routes};
use netsmith_tools::findings::{BoardOut, Finding, Place, Report, Severity, of_drc};

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`board drc` runs netsmith's own design rules check (ADR 0037), the one `board fab` runs and the editor's PCB view shows, on a board as it stands: with no `--kicad-pcb`, the board as `fab` has it before the router, the placement (the layout file read, the placer run) with the copper of the routes file beside the source on it. Nothing is routed and nothing is written: a connection the routes file does not make is a finding (`unconnected_items`), as it is for `fab` when the router leaves it, so a board with no routes file has one for each connection. The design's requirements on where parts go are measured too (as errors, or as warnings with `constraints = \"warn\"` in netsmith.toml), and the pick lock holds the parts to its rows as it does for `fab`. For the board of a source with more than one, `--board` says which.\n\nWith `--kicad-pcb`, the check is of that KiCad board, with the `.kicad_pro` beside it when there is one (a board KiCad wrote, `layout export`'s or one that came back to `layout import` or `routes import`). It says nothing of the process the board was made for, so the rules are the project file's alone: none of `fab`'s extras (the process's mask web and via rules, the fiducials, the stackup) and no requirement of a design.\n\nThe text is each finding, errors first, as `fab` lists them (`error [clearance] ... at (x, y) on F.Cu: Pad 1 of C1; Track of GND`) and a last line that says how many. With `--format json` it is one document, as `board fab --format json` has it: `ok`, `boards` (with `outline_at`, the outline's top left corner in the frame of the places, which a place less it is in mm from the outline, as the layout file and the routes file have them) and `findings`, each with `severity`, the `code` `drc`, the `rule`, the `message` with the things it is about, the `place` (`x`, `y` in mm in the frame of the board file, y down, and the `layer`), the `nets` and the `parts` it names. The exit code is 0 when the check finds no error (warnings do not fail it), 1 when it finds one or the board does not build or place, and 2 when it could not run."
)]
pub struct Args {
    /// The source file; with none, the project's boards (see netsmith.toml).
    #[arg(value_name = "FILE.NSM", conflicts_with = "kicad_pcb")]
    pub file: Option<String>,
    /// The board, by name, when the source has more than one.
    #[arg(long, value_name = "NAME", conflicts_with = "kicad_pcb")]
    pub board: Option<String>,
    /// Check this KiCad board (with the `.kicad_pro` beside it) instead of a design's board.
    #[arg(
        long,
        value_name = "BOARD.KICAD_PCB",
        conflicts_with_all = ["place_seeds", "place_budget"]
    )]
    pub kicad_pcb: Option<PathBuf>,
    #[command(flatten)]
    pub placing: common::Placing,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

/// What the check was made of: the report, the board's name and where its outline's corner is.
struct Made {
    board: String,
    checked: netsmith_fab::Checked,
    outline_at: Option<Place>,
    /// What of the routes file was not used as written, as `fab` says it.
    notes: Vec<String>,
}

pub fn run(args: Args) -> ExitCode {
    let made = match &args.kicad_pcb {
        Some(pcb) => drc::of_kicad_pcb(pcb).map(|checked| Made {
            board: pcb
                .file_stem()
                .map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
            checked,
            outline_at: None,
            notes: Vec::new(),
        }),
        None => match of_design(&args) {
            Ok(made) => Ok(made),
            Err(code) => return code,
        },
    };
    let made = match made {
        Ok(made) => made,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    let clean = made.checked.report.is_clean();
    match args.format {
        common::Format::Json => {
            let mut report = Report::new("drc");
            report.ok = clean;
            report.boards.push(BoardOut {
                board: made.board.clone(),
                dir: None,
                parts: Vec::new(),
                outline_at: made.outline_at,
            });
            report
                .findings
                .extend(made.notes.iter().map(|note| Finding {
                    board: Some(made.board.clone()),
                    ..Finding::plain(Severity::Warning, "fab", note.clone())
                }));
            report
                .findings
                .extend(of_drc(&made.checked.report, Some(&made.board)));
            print!("{}", report.to_json());
        }
        common::Format::Text => {
            for note in &made.notes {
                eprintln!("{}: warning: {note}", made.board);
            }
            print!("{}", made.checked.report);
            let counts = made.checked.report.counts();
            let warnings = made.checked.report.warnings().count();
            if clean {
                println!(
                    "{}: design rules check: passed ({warnings} warning(s))",
                    made.board
                );
            } else {
                println!(
                    "{}: design rules check: {} error(s) ({} violation(s), {} unconnected, {} footprint error(s)), {warnings} warning(s)",
                    made.board,
                    counts.total(),
                    counts.violations,
                    counts.unconnected,
                    counts.footprint_errors,
                );
            }
        }
    }
    if clean {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// The check of the design's board: placed, the routes file's copper on it, its requirements.
fn of_design(args: &Args) -> Result<Made, ExitCode> {
    common::with_stage_events(|| {
        common::with_placed_board(
            args.file.as_deref(),
            args.board.as_deref(),
            Some(&args.placing),
            |placed| {
                placed.with_board(|built, board| {
                    let fail = |e: String| format!("error: {}: {e}\n", built.label());
                    let mut board = board.clone();
                    let notes =
                        routes::apply_beside(&mut board, Some(&placed.layout)).map_err(&fail)?;
                    let constraints = netsmith_pipeline::constraints::of_design(
                        &built.design,
                        placed.catalogs.constraints_warn(),
                    );
                    let checked = drc::of_board(&board, constraints).map_err(&fail)?;
                    Ok(Made {
                        board: built.label(),
                        checked,
                        outline_at: Some(Place {
                            x: board.outline.min.x,
                            y: board.outline.min.y,
                            layer: None,
                        }),
                        notes,
                    })
                })?
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{Leaf, kind, parse};
    use clap::error::ErrorKind;

    #[test]
    fn a_design_or_a_kicad_board_is_checked_and_not_both() {
        let Ok(Leaf::Drc(a)) =
            parse(&["board", "drc", "a.nsm", "--board", "B", "--format", "json"])
        else {
            panic!("drc should parse");
        };
        assert_eq!(a.file.as_deref(), Some("a.nsm"));
        assert_eq!(a.board.as_deref(), Some("B"));
        let Ok(Leaf::Drc(a)) = parse(&["board", "drc", "--kicad-pcb", "b.kicad_pcb"]) else {
            panic!("drc should parse");
        };
        assert_eq!(
            a.kicad_pcb.as_deref(),
            Some(std::path::Path::new("b.kicad_pcb"))
        );
        for args in [
            &["board", "drc", "a.nsm", "--kicad-pcb", "b.kicad_pcb"][..],
            &["board", "drc", "--board", "B", "--kicad-pcb", "b.kicad_pcb"],
            &[
                "board",
                "drc",
                "--place-seeds",
                "2",
                "--kicad-pcb",
                "b.kicad_pcb",
            ],
        ] {
            assert_eq!(kind(args), ErrorKind::ArgumentConflict, "{args:?}");
        }
    }
}
