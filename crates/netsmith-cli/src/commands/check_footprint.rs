//! `netsmith board check-footprint` (issue #1684): one footprint's pads against the rules of a
//! board's process, with nothing placed or routed. A verb of the board, beside `board drc` and
//! `board rules`, as the process and the part's nets are a board's; named as `routes check-via`
//! is, a check of one object. The check is `netsmith_fabricate::footprint_check`.

use std::path::Path;
use std::process::ExitCode;

use clap::Args as ClapArgs;

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(after_long_help = LONG_HELP)]
pub struct Args {
    /// A part of the board (its designator, `U1`, or its path), or a `.kicad_mod` file.
    #[arg(value_name = "PART|FILE.KICAD_MOD")]
    pub footprint: String,
    /// The source file; with none, the project's boards (see netsmith.toml).
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The board, by name, when the source has more than one.
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

pub const LONG_HELP: &str = "The footprint is judged by the design rules check `board fab` runs, on a board of the footprint alone: the board's process, its rules and the process's figures (the space between surface-mount pads, `.min_pad_to_pad`; the mask web between pad openings, `.min_mask_bridge`; the clearance, the holes and the copper round them, the silkscreen to a pad), in an outline far from its copper, with no other part, copper, pour, plane, keep-out or waiver. The board is built, not placed or routed. The findings kept are those of the footprint's own items, less those of connections and of the outline.\n\nA part of the board (by designator or path) is its footprint with the nets the design gives its pads, so two pads of one net are not held apart. A `.kicad_mod` file is judged on the process of the board the source and `--board` pick, each pad number on a net of its own (`pad-1`, ...), so every two pads of different numbers are held apart; pads that share a number are one net, and a pad with no number has none. The process gives no space between paste openings, so none is measured; a rule of the check about paste is listed as any other.\n\nThe text is a line for each finding, errors first, the largest shortfall first: the rule, the pads, the space required and measured and the shortfall in mm (`error [pad_to_pad] pads 1 and 2: needs 0.1500 mm, has 0.1000 mm, short by 0.0500 mm at (0.000, 0.000) on F.Cu`), the place in mm from the footprint's origin, y down. With `--format json`: `board`, `process`, `part` (none for a file), `footprint`, `nets` (`part` or `pad`), `pads`, `legal`, `errors` and `warnings` (each with `severity`, `rule`, `pads`, `items`, `required`, `measured`, `shortfall`, `at`, `layer` and `message`). The exit code is 0 when no rule is broken, 1 when one is or the board has no such part, and 2 when it could not run.";

pub fn run(args: &Args) -> ExitCode {
    use netsmith_fabricate::footprint_check as check;
    let file = Path::new(&args.footprint);
    let footprint = if file.extension().is_some_and(|e| e == "kicad_mod") {
        let text = match std::fs::read_to_string(file) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("error: {}: {e}", args.footprint);
                return ExitCode::from(2);
            }
        };
        let name = file
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        match netsmith_pcb::Footprint::parse(&name, &text) {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!("error: {}: {e}", args.footprint);
                return ExitCode::from(2);
            }
        }
    } else {
        None
    };
    let done = common::with_placed_board(
        args.file.as_deref(),
        args.board.as_deref(),
        None,
        |placed| {
            placed
                .with_built(|built| {
                    let board = check::board_of(&built.design, placed.libraries)?;
                    match &footprint {
                        Some(f) => check::of_footprint(&board, f.clone(), &args.footprint),
                        None => check::of_part(&board, &args.footprint),
                    }
                })?
                .map_err(|e| format!("error: {e}\n"))
        },
    );
    let checked = match done {
        Ok(c) => c,
        Err(code) => return code,
    };
    match args.format {
        common::Format::Json => match serde_json::to_value(&checked) {
            Ok(v) => common::print_json(&v),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        },
        common::Format::Text => print!("{}", checked.text()),
    }
    if checked.legal {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
