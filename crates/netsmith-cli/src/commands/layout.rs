//! `netsmith layout` (issue #782): the layout file beside a source (`<stem>.pcb.toml`), shown,
//! written one entry at a time and checked against the board, so that a script or an agent
//! that moves a few parts need not write the TOML itself. The file's format and frame are in the
//! language guide's "The layout file"; the work is `netsmith_fabricate::layout_edit`.

use std::process::ExitCode;

use clap::{Args as ClapArgs, Subcommand, ValueEnum};
use netsmith_fabricate::layout_edit as lib;
use netsmith_pcb::LayoutFace;

use super::common;
use super::routes::{Target, parse_point};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "The layout file `<stem>.pcb.toml` sits beside the source and holds the solution: where each part is (`at`, `turn`, `side`, `lock`), keyed by the part's path in the design (`stage.output_cap`; the capacitors of a bank are `bank[0]`, `bank[1]`). The requirements on where a part may be are in the board's physical file, never here.\n\nThe frame is the file's: `--at X,Y` is the footprint's origin in mm from the outline's top left corner, x to the right and y down; a turn is in degrees, counterclockwise on screen, and a part is turned by 0, 90, 180 or 270; a part on the bottom face is the top face's footprint mirrored left to right (about its own y axis), then turned as it is seen from the top. The guide's \"The layout file\" has an example of a part on each face.\n\nEach command checks and solves the board as `build` does and places nothing. The exit code is 0 when it did what it was asked, 1 when it refused (a part the board does not have, a turn that is not allowed, a file with an error `check` reports), and 2 when it could not run (a usage mistake included)."
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
        after_long_help = "Writes the entry of the part at PATH to the layout file (made if it does not exist), reading and writing the file as the editor does: every other table is kept, and a comment in the file is not. A part with no entry needs `--at`; for a part that has one, a flag left out keeps what the entry has. A new entry is written with `source = \"hand\"`, and an entry that exists keeps its `source`. Refused, with the file as it was: a PATH the board has no part at (the nearest names are listed), a turn that is not 0, 90, 180 or 270, and a part with no entry and no `--at`. A locked entry that breaks a requirement of the physical file is written and said as a warning; `layout check` says it again."
    )]
    Set(SetArgs),
    /// Say which parts have no entry, which entries name no part, and which locked entries break a requirement.
    #[command(
        after_long_help = "One line for each finding, with the line of the entry in the file: an entry that names no part of the board (with the nearest names), an entry with a turn that is not 0, 90, 180 or 270 or a position that is not a number, and a locked entry that breaks a requirement of the physical file (the same words `fab` uses; the footprints are loaded and nothing is placed). A part with no entry is a note: the placer places it. The exit code is 1 when there is an error, and a note does not make it so. `--format json` prints one JSON document on stdout: the `board`, the `file`, `ok` and the `findings`, each with a `severity` (`error` or `note`), a `kind` (`no_entry`, `no_part`, `bad_turn`, `bad_place`, `requirement`, `unmeasured`), the part's `path`, the `line` and the `message`."
    )]
    Check(CheckArgs),
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
    /// The turn in degrees, counterclockwise on screen: 0, 90, 180 or 270.
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
    let done =
        common::with_placed_board(target.file.as_deref(), target.board.as_deref(), |placed| {
            placed.with_built(|built| then(&built.design, &placed.layout, placed.libraries))
        })?;
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
