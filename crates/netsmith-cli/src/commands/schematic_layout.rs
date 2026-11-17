//! `netsmith schematic` (issue #1093): the schematic layout file beside a source
//! (`<stem>.sch.toml`, ADR 0061 and 0139), shown, written one entry at a time, checked against the
//! board and used to arrange the sheets of a big board, so that a script or an agent that keeps a
//! drawing legible need not write the TOML itself. It is `layout`'s counterpart for the drawing,
//! where `layout` is for the placement of the PCB; the work is `netsmith_workspace::schematic_file`,
//! which draws with the code the editor draws with.

use std::process::ExitCode;

use clap::{Args as ClapArgs, Subcommand};
use netsmith_workspace::schematic_file as lib;

use super::common;
use super::routes::{Target, parse_point};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "The schematic layout file `<stem>.sch.toml` sits beside the source (`src/main.nsm` has `src/main.sch.toml`) and holds how the schematic is arranged, so that the drawing stays as arranged across edits: where a part's block is moved to (`dx`, `dy`, or a `pin` from the sheet's reference part), a turn and a mirror, how far a note's frame is moved, and where a sheet's box goes on the page of a big board drawn by module (`at`). The editor writes it when a part or a card is dragged; this is the command line's way. The layout file `netsmith layout` writes is the PCB's, and the picture is drawn by the editor.\n\nA part is named by its path in the design (`stage.output_cap`), a note by its path and `#doc` or `#relation` (`stage.output_cap#doc`), a sheet by its module instance (`stage`) or, for the parts the board declares itself, the board's name. A length is in mm on the schematic's 1.27 mm grid, x to the right and y down, as in the file. A part's `--by` is from where the drawing puts it; `--pin` is from the sheet's reference part; a turn replaces the automatic one.\n\n`show`, `set`, `check` and `arrange` check and solve the board as `board build` does, place nothing, need no KiCad and draw the schematic with the file as the editor draws it. The exit code is 0 when it did what it was asked, 1 when it refused (a name the board does not have, a file with a fault, a `check` that finds something), and 2 when it could not run (a usage mistake included)."
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum Command {
    /// Print the board's sheets and each entry of the file, with whether the drawing uses it.
    #[command(
        after_long_help = "Prints the board's sheets (a module instance of the board, or the parts it declares itself) with their parts, and where the drawing puts the box of each when the board is drawn by module (more than 16 parts), then each entry of the file with its line: a sheet's place (`at`), a part's move (`dx`, `dy`) or pin, turn and mirror, a note's move. An entry that matches nothing in the design, or that the drawing could not honour, is marked ignored, with why. With names, only those sheets and parts (and their entries) are listed; a name the board has neither a sheet nor a part by is refused with the nearest names. `--format json` prints one JSON document on stdout: the `board`, the `file`, whether it `exists`, `by_module`, the `sheets` (`name`, `own`, `parts`, `placed`, `frame`) and the `entries` (`sheet`, `kind`, `path`, `line`, `dx`, `dy`, `pin`, `at`, `rotate`, `mirror`, `used`, `message`)."
    )]
    Show(ShowArgs),
    /// Write one entry (a part's move, pin, turn or mirror, a note's move, a sheet's place), keeping the file's other tables.
    #[command(
        after_long_help = "Writes the entry at PATH to the schematic layout file (made if it does not exist), reading and writing the file as the editor does: every other table is kept, and so is a comment in the file (a comment goes with its entry and stays above it or after it). A part takes `--by DX,DY` (moved by that much from where the drawing puts its block) or `--pin X,Y` (at that place from the sheet's reference part), `--rotate` (0, 90, 180 or 270 degrees counterclockwise, replacing the automatic turn) and `--mirror` or `--no-mirror` (replacing the automatic mirror); a flag left out keeps what the entry has, and `--by` and `--pin` replace one another. A note takes `--by`. A sheet takes `--at X,Y`, the top-left corner of its box on the page of a board drawn by module. `--clear` removes the entry of a part or note, or a sheet's `at`. Refused, with the file as it was: a PATH the board has no part (or, for `--at` and `--clear`, sheet) by (the nearest names are listed), a length that is not on the 1.27 mm grid (the nearest two are said), a turn that is not 0, 90, 180 or 270, flags that do not go together, and a file with a fault in its text. An entry the drawing then cannot honour (its block would lie on another part, or its box on another sheet's) is written and said as a warning; `check` says it again."
    )]
    Set(SetArgs),
    /// Say which entries name no sheet or part of the board, which the drawing could not honour, and which lines of the file are wrong.
    #[command(
        after_long_help = "One line for each finding, with the line of the entry in the file: an entry for a sheet, part or note the board does not have (or one that is on another sheet, with where it belongs), a table for a board the source does not have, an entry the drawing could not honour (a block that would lie on another part or stand too far from the drawing, a box that would lie on another sheet's, a `at` of a board drawn as one sheet), and a fault in the file's text, which makes the drawing ignore the whole file. These are the notes and warnings the editor lists for the file (stage `layout`). The exit code is 1 when there is any finding. `--format json` prints one JSON document on stdout: the `board`, the `file`, `exists`, `ok` and the `findings`, each with a `kind` (`invalid`, `stale_sheet`, `stale_entry`, `wrong_sheet`, `unknown_board`, `unusable`), the `line`, the `entry` (the table as the file names it) and the `message`."
    )]
    Check(CheckArgs),
    /// Say where the drawing puts each sheet's box on the page of a big board; `--write` keeps it as `at` entries.
    #[command(
        after_long_help = "A board of more than 16 parts is drawn by module, a box for each sheet, and the boxes are set out by how the modules connect (the editor's overview, issue #798); a box the file places (`at`) stays where it is, and the others are set out round it (ADR 0139). Printed: each sheet's box, its top-left corner on the page, on the grid, and its size, and whether the file places it or the drawing sets it out. With `--write` each sheet that can be placed gets an `at` entry with the place it has, so the arrangement stays when the circuit changes (the file's other entries and comments are kept; a sheet drawn as several boxes has no one place and is left out, with why). `--reset` drops the file's `at` entries first, so the arrangement is the one by connections alone (with `--write` it is then written). Refused: a board drawn as one sheet, which has no boxes to set out. To move one sheet, `set SHEET --at X,Y`. `--format json` prints one JSON document on stdout: the `board`, the `file`, `reset`, the `sheets` (`name`, `own`, `at`, `width`, `height`, `source` (`file` or `arranged`), `skipped`) and whether it was `written`."
    )]
    Arrange(ArrangeArgs),
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct ShowArgs {
    /// Sheets (a module instance) and parts (by path) to list; with none, all.
    #[arg(value_name = "PATH")]
    pub paths: Vec<String>,
    #[command(flatten)]
    pub target: Target,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct SetArgs {
    /// The part by its path in the design (`stage.output_cap`), a note by its path and `#doc` or `#relation`, or a sheet by its name (`--at`, `--clear`).
    #[arg(value_name = "PATH")]
    pub path: String,
    #[command(flatten)]
    pub target: Target,
    /// Move a part's block, or a note, by `DX,DY` mm from where the drawing puts it (a multiple of 1.27).
    #[arg(long, value_name = "DX,DY", value_parser = parse_point, allow_hyphen_values = true, conflicts_with = "pin")]
    pub by: Option<[f64; 2]>,
    /// Pin a part's block at `X,Y` mm from the sheet's reference part (a multiple of 1.27).
    #[arg(long, value_name = "X,Y", value_parser = parse_point, allow_hyphen_values = true)]
    pub pin: Option<[f64; 2]>,
    /// Turn a part to 0, 90, 180 or 270 degrees counterclockwise, replacing the automatic turn.
    #[arg(long, value_name = "DEGREES", allow_hyphen_values = true)]
    pub rotate: Option<i32>,
    /// Mirror a part left to right before its turn, replacing the automatic mirror.
    #[arg(long, conflicts_with = "no_mirror")]
    pub mirror: bool,
    /// Do not mirror a part, replacing the automatic mirror.
    #[arg(long)]
    pub no_mirror: bool,
    /// Place a sheet's box with its top-left corner at `X,Y` mm on the page (a multiple of 1.27).
    #[arg(long, value_name = "X,Y", value_parser = parse_point, allow_hyphen_values = true)]
    pub at: Option<[f64; 2]>,
    /// Remove the entry of a part or a note, or a sheet's `at`.
    #[arg(long)]
    pub clear: bool,
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

#[derive(Debug, PartialEq, ClapArgs)]
pub struct ArrangeArgs {
    #[command(flatten)]
    pub target: Target,
    /// Drop the file's `at` entries first: the arrangement by connections alone.
    #[arg(long)]
    pub reset: bool,
    /// Keep the arrangement: write an `at` entry for each sheet that can be placed.
    #[arg(long)]
    pub write: bool,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

pub fn run(args: Args) -> ExitCode {
    match args.command {
        Command::Show(a) => show(&a),
        Command::Set(a) => set(&a),
        Command::Check(a) => check(&a),
        Command::Arrange(a) => arrange(&a),
    }
}

/// `then` of the board picked, checked and solved but not placed, with the schematic layout file's
/// path and name, the libraries and the names of all the boards of the source. A refusal of
/// `then` is printed and exits 1; a board that does not build is printed by
/// [`common::with_placed_board`].
fn with_board<T>(
    target: &Target,
    then: impl Fn(
        &netsmith_pipeline::Built,
        &netsmith_pipeline::Libraries,
        &std::path::Path,
        &str,
        &[String],
    ) -> Result<T, String>,
) -> Result<T, ExitCode> {
    let done = common::with_placed_board(
        target.file.as_deref(),
        target.board.as_deref(),
        None,
        |placed| {
            let (file, name) = lib::file_of(placed.source, placed.name);
            placed.with_built_in_source(|built, boards| {
                then(built, placed.libraries, &file, &name, boards)
            })
        },
    )?;
    done.map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    })
}

fn show(args: &ShowArgs) -> ExitCode {
    let shown = with_board(&args.target, |built, libraries, file, name, _| {
        lib::show(built, libraries, file, name, &args.paths)
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
        by: args.by.map(|[x, y]| (x, y)),
        pin: args.pin.map(|[x, y]| (x, y)),
        rotate: args.rotate,
        mirror: if args.mirror {
            Some(true)
        } else if args.no_mirror {
            Some(false)
        } else {
            None
        },
        at: args.at.map(|[x, y]| (x, y)),
        clear: args.clear,
    };
    let written = with_board(&args.target, |built, libraries, file, name, _| {
        lib::set(built, libraries, file, name, &args.path, &change)
    });
    match written {
        Ok(written) => {
            match args.format {
                common::Format::Json => println!("{}", lib::json(&written)),
                common::Format::Text => println!("{}", written.text()),
            }
            for warning in &written.warnings {
                eprintln!("warning: {warning}");
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

fn check(args: &CheckArgs) -> ExitCode {
    let checked = with_board(&args.target, |built, libraries, file, name, boards| {
        lib::check(built, libraries, file, name, boards)
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

fn arrange(args: &ArrangeArgs) -> ExitCode {
    let arranged = with_board(&args.target, |built, libraries, file, name, _| {
        lib::arrange(built, libraries, file, name, args.reset, args.write)
    });
    match arranged {
        Ok(arranged) => {
            match args.format {
                common::Format::Json => println!("{}", lib::json(&arranged)),
                common::Format::Text => {
                    print!("{}", arranged.text());
                    if args.write {
                        println!(
                            "{}",
                            if arranged.written {
                                format!("wrote {}", arranged.file)
                            } else {
                                format!("{} is as it was", arranged.file)
                            }
                        );
                    } else {
                        println!("not written: run again with --write to keep it");
                    }
                }
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;

    use super::*;
    use crate::commands::common::testing::Leaf as Top;
    use crate::commands::common::testing::{kind, parse};

    #[test]
    fn schematic_takes_its_subcommands() {
        assert!(matches!(
            parse(&["schematic", "show", "stage", "u1", "--format", "json"]),
            Ok(Top::SchematicLayout(Args {
                command: Command::Show(ShowArgs { ref paths, format: common::Format::Json, .. })
            })) if paths == &["stage", "u1"]
        ));
        assert!(matches!(
            parse(&[
                "schematic",
                "set",
                "stage.c1",
                "--by",
                "2.54,-1.27",
                "--rotate",
                "-90",
                "--no-mirror"
            ]),
            Ok(Top::SchematicLayout(Args {
                command: Command::Set(SetArgs {
                    by: Some([2.54, -1.27]),
                    rotate: Some(-90),
                    no_mirror: true,
                    ..
                })
            }))
        ));
        assert!(matches!(
            parse(&["schematic", "arrange", "--reset", "--write"]),
            Ok(Top::SchematicLayout(Args {
                command: Command::Arrange(ArrangeArgs {
                    reset: true,
                    write: true,
                    ..
                })
            }))
        ));
    }

    #[test]
    fn set_refuses_flags_that_exclude_each_other() {
        assert_eq!(
            kind(&["schematic", "set", "u1", "--by", "0,0", "--pin", "0,0"]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(
            kind(&["schematic", "set", "u1", "--mirror", "--no-mirror"]),
            ErrorKind::ArgumentConflict
        );
    }
}
