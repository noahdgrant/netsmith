//! `netsmith layout` (issue #782): the layout file beside a source (`<stem>.pcb.toml`), shown,
//! written one entry at a time and checked against the board, so that a script or an agent
//! that moves a few parts need not write the TOML itself. The file's format and frame are in the
//! language guide's "The layout file"; the work is `netsmith_fabricate::layout_edit`.
//! `layout summary` and `layout compare` (issue #773) place the board as `board3d` does and print
//! what decides whether the placement is good, `netsmith_pcb::Summary` and
//! `netsmith_pcb::PlacementComparison`. `layout regenerate` (issue #825) places one module again
//! with the rest held, `netsmith_fabricate::regenerate`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args as ClapArgs, Subcommand, ValueEnum};
use netsmith_fabricate::layout_edit as lib;
use netsmith_pcb::LayoutFace;

use super::common;
use super::routes::{Target, parse_point};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "The layout file `<stem>.pcb.toml` sits beside the source and holds the solution: where each part is (`at`, `turn`, `side`, `lock`), keyed by the part's path in the design (`stage.output_cap`; the capacitors of a bank are `bank[0]`, `bank[1]`). The requirements on where a part may be are in the board's physical file, never here.\n\nThe frame is the file's: `--at X,Y` is the footprint's origin in mm from the outline's top left corner, x to the right and y down; a turn is in degrees, counterclockwise on screen, any angle from 0 up to 360; a part on the bottom face is the top face's footprint mirrored left to right (about its own y axis), then turned as it is seen from the top. The guide's \"The layout file\" has an example of a part on each face.\n\n`show`, `set` and `check` check and solve the board as `build` does and place nothing; `summary` and `compare` place it as `board3d` does and write nothing. The exit code is 0 when it did what it was asked, 1 when it refused (a part the board does not have, a turn that is not allowed, a file with an error `check` reports), and 2 when it could not run (a usage mistake included)."
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum Command {
    /// Print the entry of each part (place, turn, face, lock, source) as the placer reads it.
    #[command(
        after_long_help = "Without a PATH every part of the board is listed, with the line of its entry in the file; a part with no entry is said to be placed by the placer. A PATH is a part's path, or a module's, which lists the parts inside it. A path the board does not have is refused with the nearest names. `--format json` prints one JSON document on stdout: the `board`, the `file`, whether it `exists`, and each part's `path`, `refdes` and `entry` (`at`, `turn`, `side`, `lock`, `source`, `line`; left out when the part has none)."
    )]
    Show(ShowArgs),
    /// Write one part's entry, keeping the file's other tables.
    #[command(
        after_long_help = "Writes the entry of the part at PATH to the layout file (made if it does not exist), reading and writing the file as the editor does: every other table is kept, and so is a comment in the file (a comment goes with its entry and stays above it or after it). A part with no entry needs `--at`; for a part that has one, a flag left out keeps what the entry has. A new entry is written with `source = \"hand\"`, and an entry that exists keeps its `source`. Refused, with the file as it was: a PATH the board has no part at (the nearest names are listed), a turn that is not from 0 up to 360, and a part with no entry and no `--at`. A locked entry that breaks a requirement of the physical file is written and said as a warning; `layout check` says it again."
    )]
    Set(SetArgs),
    /// Say which parts have no entry, which entries name no part, and which locked entries break a requirement.
    #[command(
        after_long_help = "One line for each finding, with the line of the entry in the file: an entry that names no part of the board (with the nearest names), an entry with a turn that is not from 0 up to 360 or a position that is not a number, and a locked entry that breaks a requirement of the physical file (the same words `fab` uses; the footprints are loaded and nothing is placed). A part with no entry is a note: the placer places it. The exit code is 1 when there is an error, and a note does not make it so. `--format json` prints one JSON document on stdout: the `board`, the `file`, `ok` and the `findings`, each with a `severity` (`error` or `note`), a `kind` (`no_entry`, `no_part`, `bad_turn`, `bad_place`, `requirement`, `unmeasured`), the part's `path`, the `line` and the `message`."
    )]
    Check(CheckArgs),
    /// Place the board and sum up the placement: wire, decoupling, modules, area per face, nets, free space.
    #[command(
        after_long_help = "The board is placed as `board3d` places it (the layout file and the physical file read, nothing written) and summed up in numbers the placer and the checks use, in the layout file's frame (mm from the outline's top left corner, y down): the totals (`hpwl`, the half perimeter wire length; `ratsnest`; `crossings`; `decoupling_mean` and `decoupling_max`); for each face the area the courtyards take against what the outline gives inside the edge wall (`usable`, `courtyards`, `fill`, `free`) and its largest free rectangles; for each module instance of two parts or more its box, the area its courtyards take, its `spread` (the placer's cohesion term: the half perimeter of the box round its parts' courtyard centres) and its `intruders` (parts of other modules whose courtyard centre is in that box); each decoupling capacitor's distance to the pins it serves; for each net its half perimeter, its crossings with other nets and its longest ratsnest line; and the placement check's findings. `--near PATH` adds the parts within `--within` mm (courtyard to courtyard, on either face) of a part, nearest first. `--format json` prints one JSON document on one line; the language guide's \"The layout file\" has its fields. The exit code is 0 when the summary is printed, also with findings, and 1 when the board does not build or place."
    )]
    Summary(SummaryArgs),
    /// Place the board with two layout files and say what moved and which findings appeared or went.
    #[command(
        after_long_help = "The board is placed once with the layout file BEFORE and once with AFTER (with none, the layout file beside the source), each as `board3d` places it; a part with no entry in a file is placed by the placer, so it may move between the two. Printed: each part that moved, turned or changed face, with its two places and how far its origin moved; the placement check's findings that appeared and those that went; and the totals of each (`hpwl`, `ratsnest`, `crossings`, `decoupling_mean`, `decoupling_max`). `--format json` prints one JSON document on one line: `board`, `moved` (`path`, `refdes`, `from`, `to`, each `x`, `y`, `turn`, `side`, and `distance`), `appeared`, `gone`, `before` and `after`. The exit code is 0 when it is printed, 1 when a board does not build or place, and 2 when a layout file is not there."
    )]
    Compare(CompareArgs),
    /// Place one module again with every other part held, and say what moved and what it changes; `--write` keeps it.
    #[command(
        after_long_help = "The board is placed as `fab` places it (the layout file and the physical file read). The parts of the module at PATH (every part whose path is PATH or starts with `PATH.` or `PATH[`) are placed again, inside the box round them grown by `--margin` mm on each side and cut to the outline; every other part, and every part of the module the layout file locks or a requirement pins, stays where it is. The same board, module, seed and margin always give the same arrangement, and another `--seed` another one.\n\nPrinted: each part placed again with its place before and after (mm from the outline's top left corner, y down, the turn and the face), the nets with a pad on one of them (`fab` and the editor route their copper again and keep the rest), the records of the routes file that no longer match their pads (dropped as any record whose pads moved is, the locked shapes kept), the placement check's findings that appeared and those that went, and the totals before and after. Without `--write` nothing is written. With it, each part placed again gets an entry in the layout file, unlocked, with `source = \"regen\"`, and the rest of the file (its comments too) is kept. `--format json` prints one JSON document on stdout: `board`, `module`, `seed`, `margin`, `region`, `parts` (`path`, `refdes`, `before`, `was`, `after`), `held`, `nets`, `routes_dropped`, `comparison` (as `layout compare` has it) and `written`. The exit code is 0 when the proposal is printed (and written with `--write`), 1 when it is refused (a module the board has no part in, every part of it held, parts that do not fit the region), and 2 when it could not run."
    )]
    Regenerate(RegenerateArgs),
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct RegenerateArgs {
    /// The module, by its path in the design (`stage`, `channels[1]`).
    #[arg(value_name = "PATH")]
    pub module: String,
    #[command(flatten)]
    pub target: Target,
    /// The seed of the arrangement: the same seed gives the same one, another seed another.
    #[arg(long, value_name = "N", default_value_t = 0)]
    pub seed: u64,
    /// How far round the module's parts they may go, in mm.
    #[arg(long, value_name = "MM", default_value_t = netsmith_fabricate::regenerate::DEFAULT_MARGIN)]
    pub margin: f64,
    /// Write the new places to the layout file (without it, nothing is written).
    #[arg(long)]
    pub write: bool,
    #[command(flatten)]
    pub placing: common::Placing,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct SummaryArgs {
    #[command(flatten)]
    pub target: Target,
    /// A part, by path or designator, whose neighbours to list; may be given again.
    #[arg(long = "near", value_name = "PATH")]
    pub near: Vec<String>,
    /// How far from a `--near` part another may be and be listed, courtyard to courtyard, in mm.
    #[arg(long, value_name = "MM", default_value_t = 2.0)]
    pub within: f64,
    #[command(flatten)]
    pub placing: common::Placing,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct CompareArgs {
    /// The layout file of the first placement.
    #[arg(value_name = "BEFORE.PCB.TOML")]
    pub before: PathBuf,
    /// The layout file of the second; with none, the one beside the source.
    #[arg(value_name = "AFTER.PCB.TOML")]
    pub after: Option<PathBuf>,
    #[command(flatten)]
    pub target: Target,
    #[command(flatten)]
    pub placing: common::Placing,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct ShowArgs {
    /// The parts to show, by path, or the modules whose parts to show; with none, every part.
    #[arg(value_name = "PATH")]
    pub paths: Vec<String>,
    #[command(flatten)]
    pub target: Target,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

/// The face of the board a part is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Side {
    Top,
    Bottom,
}

#[derive(Debug, PartialEq, ClapArgs)]
#[command(group = clap::ArgGroup::new("change").required(true).multiple(true).args(["at", "turn", "side", "lock", "unlock"]))]
pub struct SetArgs {
    /// The part, by its path in the design (`stage.output_cap`, `decoupling[2]`).
    #[arg(value_name = "PATH")]
    pub path: String,
    #[command(flatten)]
    pub target: Target,
    /// Where the footprint's origin goes, `X,Y` in mm from the outline's top left corner, y down.
    #[arg(long, value_name = "X,Y", value_parser = parse_point, allow_hyphen_values = true)]
    pub at: Option<[f64; 2]>,
    /// The turn in degrees, counterclockwise on screen: any angle from 0 up to 360 (45, 90).
    #[arg(long, value_name = "DEGREES", value_parser = parse_turn, allow_hyphen_values = true)]
    pub turn: Option<f64>,
    /// The face: `top` or `bottom`.
    #[arg(long, value_enum)]
    pub side: Option<Side>,
    /// Lock the entry: the placer leaves the part where it is.
    #[arg(long, conflicts_with = "unlock")]
    pub lock: bool,
    /// Unlock the entry: the placer may move the part.
    #[arg(long)]
    pub unlock: bool,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct CheckArgs {
    #[command(flatten)]
    pub target: Target,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

fn parse_turn(text: &str) -> Result<f64, String> {
    let turn: f64 = text
        .trim()
        .parse()
        .map_err(|_| format!("`{text}` is not a number"))?;
    match lib::turn_error(turn) {
        Some(why) => Err(why),
        None => Ok(turn),
    }
}

pub fn run(args: Args) -> ExitCode {
    match args.command {
        Command::Show(a) => show(&a),
        Command::Set(a) => set(&a),
        Command::Check(a) => check(&a),
        Command::Summary(a) => summary(&a),
        Command::Compare(a) => compare(&a),
        Command::Regenerate(a) => regenerate(&a),
    }
}

/// `layout regenerate`: one module placed again, printed and written when asked.
fn regenerate(args: &RegenerateArgs) -> ExitCode {
    use netsmith_fabricate::regenerate as lib;
    if !args.margin.is_finite() || args.margin < 0.0 {
        eprintln!("error: --margin must be a distance of 0 mm or more");
        return ExitCode::from(2);
    }
    let request = lib::Request {
        module: args.module.clone(),
        seed: args.seed,
        margin: args.margin,
    };
    let made = common::with_placed_board(
        args.target.file.as_deref(),
        args.target.board.as_deref(),
        Some(&args.placing),
        |placed| {
            placed
                .with_built(|built| {
                    lib::propose(
                        &built.design,
                        &placed.layout,
                        placed.libraries,
                        placed.catalogs.constraints_warn(),
                        placed.seeds,
                        &request,
                    )
                    .map(|(proposal, layout)| (proposal, layout, placed.layout.clone()))
                })
                .and_then(|done| done.map_err(|e| format!("error: {e}\n")))
        },
    );
    let (proposal, layout, file) = match made {
        Ok(made) => made,
        Err(code) => return code,
    };
    if args.write
        && let Err(e) = lib::accept(&file, &layout)
    {
        eprintln!("error: {e}");
        return ExitCode::from(2);
    }
    match args.format {
        common::Format::Json => {
            let mut value = serde_json::to_value(&proposal).expect("plain data");
            value["written"] = serde_json::Value::Bool(args.write);
            println!("{value}");
        }
        common::Format::Text => {
            print!("{}", proposal.text());
            if args.write {
                println!("wrote {}", file.display());
            } else {
                println!("not written: run again with --write to keep it");
            }
        }
    }
    ExitCode::SUCCESS
}

/// `layout summary`: the board placed and summed up.
fn summary(args: &SummaryArgs) -> ExitCode {
    let made = common::with_placed_board(
        args.target.file.as_deref(),
        args.target.board.as_deref(),
        Some(&args.placing),
        |placed| {
            placed
                .with_board(|_, board| {
                    netsmith_pcb::Summary::of(board).neighbours(board, &args.near, args.within)
                })
                .and_then(|done| done.map_err(|e| format!("error: {e}\n")))
        },
    );
    match made {
        Ok(summary) => {
            match args.format {
                common::Format::Json => {
                    println!("{}", serde_json::to_string(&summary).expect("plain data"))
                }
                common::Format::Text => print!("{}", summary.text()),
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

/// `layout compare`: the board placed with two layout files, and what changed between them.
fn compare(args: &CompareArgs) -> ExitCode {
    for file in std::iter::once(&args.before).chain(&args.after) {
        if !file.is_file() {
            eprintln!("error: {}: no such layout file", file.display());
            return ExitCode::from(2);
        }
    }
    let made = common::with_placed_board(
        args.target.file.as_deref(),
        args.target.board.as_deref(),
        Some(&args.placing),
        |placed| {
            let before = placed.with_board_laid_out(&args.before, |_, board| board.clone())?;
            let after = args.after.as_deref().unwrap_or(&placed.layout);
            placed.with_board_laid_out(after, |_, board| {
                netsmith_pcb::PlacementComparison::between(&before, board)
            })
        },
    );
    match made {
        Ok(difference) => {
            match args.format {
                common::Format::Json => println!(
                    "{}",
                    serde_json::to_string(&difference).expect("plain data")
                ),
                common::Format::Text => print!("{}", difference.text()),
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

/// `then` of the board picked, checked and solved but not placed, with the layout file's path
/// and the libraries. A refusal of `then` is printed and exits 1; a board that does not build is
/// printed by [`common::with_placed_board`].
fn with_design<T>(
    target: &Target,
    then: impl Fn(
        &netsmith_core::Design,
        &std::path::Path,
        &netsmith_pipeline::Libraries,
    ) -> Result<T, String>,
) -> Result<T, ExitCode> {
    let done = common::with_placed_board(
        target.file.as_deref(),
        target.board.as_deref(),
        None,
        |placed| placed.with_built(|built| then(&built.design, &placed.layout, placed.libraries)),
    )?;
    done.map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    })
}

fn show(args: &ShowArgs) -> ExitCode {
    let shown = with_design(&args.target, |design, file, _| {
        lib::show(design, file, &args.paths)
    });
    match shown {
        Ok(shown) => {
            match args.format {
                common::Format::Json => println!("{}", lib::json(&shown)),
                common::Format::Text => print!("{}", shown.text()),
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

fn set(args: &SetArgs) -> ExitCode {
    let change = lib::Change {
        at: args.at,
        turn: args.turn,
        side: args.side.map(|s| match s {
            Side::Top => LayoutFace::Top,
            Side::Bottom => LayoutFace::Bottom,
        }),
        lock: if args.lock {
            Some(true)
        } else if args.unlock {
            Some(false)
        } else {
            None
        },
    };
    let written = with_design(&args.target, |design, file, libraries| {
        let written = lib::set(design, file, &args.path, &change)?;
        // A locked entry that breaks a requirement is written, and said.
        let warnings = if written.entry.lock {
            lib::check(design, file, libraries)
                .map(|checked| {
                    checked
                        .findings
                        .into_iter()
                        .filter(|f| {
                            f.kind == "requirement" && f.path.as_deref() == Some(&args.path)
                        })
                        .map(|f| f.message)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok((written, warnings))
    });
    match written {
        Ok((written, warnings)) => {
            match args.format {
                common::Format::Json => println!("{}", lib::json(&written)),
                common::Format::Text => println!("{}", written.text()),
            }
            for warning in warnings {
                eprintln!("warning: {warning}");
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

fn check(args: &CheckArgs) -> ExitCode {
    let checked = with_design(&args.target, |design, file, libraries| {
        lib::check(design, file, libraries)
    });
    match checked {
        Ok(checked) => {
            match args.format {
                common::Format::Json => println!("{}", lib::json(&checked)),
                common::Format::Text => print!("{}", checked.text()),
            }
            if checked.ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(code) => code,
    }
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;

    use super::*;
    use crate::commands::Command as Top;
    use crate::commands::common::testing::{kind, parse};

    #[test]
    fn layout_takes_its_subcommands() {
        assert!(matches!(
            parse(&["layout", "show"]),
            Ok(Top::Layout(Args {
                command: Command::Show(ShowArgs { ref paths, .. })
            })) if paths.is_empty()
        ));
        assert!(matches!(
            parse(&["layout", "show", "a.b", "c", "--format", "json", "--board", "X"]),
            Ok(Top::Layout(Args {
                command: Command::Show(ShowArgs { ref paths, format: common::Format::Json, .. })
            })) if paths == &["a.b", "c"]
        ));
        assert!(matches!(
            parse(&[
                "layout", "set", "u1", "--at", "1.5,-2", "--turn", "90", "--side", "bottom",
                "--lock"
            ]),
            Ok(Top::Layout(Args {
                command: Command::Set(SetArgs {
                    at: Some([1.5, -2.0]),
                    turn: Some(90.0),
                    side: Some(Side::Bottom),
                    lock: true,
                    unlock: false,
                    ..
                })
            }))
        ));
        assert!(matches!(
            parse(&["layout", "check"]),
            Ok(Top::Layout(Args {
                command: Command::Check(_)
            }))
        ));
    }

    #[test]
    fn regenerate_takes_its_arguments() {
        assert!(matches!(
            parse(&["layout", "regenerate", "stage", "--seed", "4", "--margin", "1.5", "--write"]),
            Ok(Top::Layout(Args {
                command: Command::Regenerate(RegenerateArgs { ref module, seed: 4, margin, write: true, .. })
            })) if module == "stage" && margin == 1.5
        ));
        assert!(matches!(
            parse(&["layout", "regenerate", "stage"]),
            Ok(Top::Layout(Args {
                command: Command::Regenerate(RegenerateArgs { seed: 0, margin, write: false, .. })
            })) if margin == 3.0
        ));
    }

    #[test]
    fn summary_and_compare_take_their_arguments() {
        assert!(matches!(
            parse(&["layout", "summary", "--near", "a.b", "--near", "C1", "--within", "3.5", "--format", "json"]),
            Ok(Top::Layout(Args {
                command: Command::Summary(SummaryArgs { ref near, within, format: common::Format::Json, .. })
            })) if near == &["a.b", "C1"] && within == 3.5
        ));
        assert!(matches!(
            parse(&["layout", "summary"]),
            Ok(Top::Layout(Args {
                command: Command::Summary(SummaryArgs { ref near, within, .. })
            })) if near.is_empty() && within == 2.0
        ));
        assert!(matches!(
            parse(&["layout", "compare", "old.pcb.toml"]),
            Ok(Top::Layout(Args {
                command: Command::Compare(CompareArgs { after: None, .. })
            }))
        ));
        assert!(matches!(
            parse(&[
                "layout",
                "compare",
                "a.pcb.toml",
                "b.pcb.toml",
                "--place-seeds",
                "2"
            ]),
            Ok(Top::Layout(Args {
                command: Command::Compare(CompareArgs { after: Some(_), .. })
            }))
        ));
        assert_eq!(
            kind(&["layout", "compare"]),
            ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn set_needs_a_change_and_refuses_a_wrong_turn() {
        assert_eq!(
            kind(&["layout", "set", "u1"]),
            ErrorKind::MissingRequiredArgument
        );
        assert_eq!(kind(&["layout", "set"]), ErrorKind::MissingRequiredArgument);
        for turn in ["45", "360", "-90", "ninety"] {
            assert_eq!(
                kind(&["layout", "set", "u1", "--turn", turn]),
                ErrorKind::ValueValidation,
                "{turn}"
            );
        }
        assert_eq!(
            kind(&["layout", "set", "u1", "--lock", "--unlock"]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(
            kind(&["layout", "set", "u1", "--side", "left"]),
            ErrorKind::InvalidValue
        );
        assert_eq!(
            kind(&["layout", "set", "u1", "--at", "1"]),
            ErrorKind::ValueValidation
        );
    }
}
