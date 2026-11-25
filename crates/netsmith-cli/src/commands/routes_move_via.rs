//! `netsmith routes move-via` (issue #1644): a via of the routes file moved, with every layer's
//! tracks of its net that end in it laid again to its new place in legs at multiples of 45
//! degrees. A verb of `routes`, as `join-pad` is; the move is `netsmith_fabricate::move_via`.

use std::process::ExitCode;

use clap::Args as ClapArgs;

use super::common;
use super::routes::Target;

#[derive(Debug, PartialEq, ClapArgs)]
pub struct Args {
    #[command(flatten)]
    pub target: Target,
    /// The net, by its name in the design.
    #[arg(long, value_name = "NET")]
    pub net: String,
    /// The via of the net, numbered from 1 as `routes show` lists them.
    #[arg(long, value_name = "N")]
    pub via: usize,
    /// Its new place, mm from the outline's top left corner, y down, as the routes file has it.
    #[arg(long, value_name = "X,Y", value_parser = super::routes::parse_point)]
    pub to: [f64; 2],
    /// Say what it would change, and write nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
    #[command(flatten)]
    pub framing: crate::frame::Framing,
}

pub const LONG_HELP: &str = "The via is selected as `routes delete` and `routes lock` select it, `--net NET --via N`, and moved to `--to X,Y` (the routes file's frame, the outline's; `--frame` changes only what is printed). Every straight track of the net's record whose end lies in the via, on a layer the via is on, follows it: that end moves with the via, and the track is laid again from its other end in one leg at a multiple of 45 degrees, or in two with a 45 degree bend between them (the router's corner, mitered: no arc is drawn), the order of the legs that bends least where it leaves its other end (from the one other track of the net there, else from its old way). A new bend of more than 45 degrees there is refused, unless the track bent as much there before, so a move makes no acute bend. The first leg keeps the track's number; a second is added to the record, unlocked, with the track's width.\n\nA locked via, a locked track that would follow and an arc that ends in the via are refused before anything is tried: unlock them first (`netsmith routes unlock --net NET --via N`, `--track N`), or delete the arc. The move is refused, and nothing written, when the design rules check finds an error with it that the board did not have (a clearance to another net's copper, a short: the findings are named), or when the net has more connections left to route after it than before (the one lost is named). `--dry-run` says what it would change and writes nothing. With `--format json`: `board`, `net`, `via`, `from`, `to`, `tracks` (each with `track`, `layer`, `width`, `way_before` and `way_after`, its ways from its other end to the via, and `bend`, degrees), `refused`, `findings` (`rule`, `message`, `at`, `layer`, `shapes`), `said`, `dry_run`, `written` and `file`. The exit code is 0 when the via was moved (or would be, with `--dry-run`), 1 when it was refused, and 2 when it could not run.";

pub fn run(args: &Args) -> ExitCode {
    use netsmith_fabricate::routes as lib;
    let done = common::with_placed_board(
        args.target.file.as_deref(),
        args.target.board.as_deref(),
        None,
        |placed| {
            let (board, layout) =
                placed.with_board(|built, board| (board.clone(), placed.layout_of(built).file))?;
            let file = lib::beside_layout(&layout);
            let routes = lib::load(&file).map_err(|e| format!("error: {e}\n"))?;
            let moved = match netsmith_fabricate::move_via::move_via(
                &board, &routes, &args.net, args.via, args.to,
            ) {
                Ok(m) => m,
                Err(lib::Refusal::Refused(e)) => return Ok(Err(e)),
                Err(lib::Refusal::Failed(e)) => return Err(format!("error: {e}\n")),
            };
            let written = match (&moved.routes, args.dry_run) {
                (Some(r), false) => {
                    lib::save(&file, r).map_err(|e| format!("error: {e}\n"))?;
                    true
                }
                _ => false,
            };
            Ok(Ok((moved, file, written)))
        },
    );
    let (moved, file, written) = match done {
        Ok(Ok(done)) => done,
        Ok(Err(e)) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
        Err(code) => return code,
    };
    match args.format {
        common::Format::Json => {
            let mut doc = serde_json::to_value(&moved).expect("plain data");
            doc["dry_run"] = args.dry_run.into();
            doc["written"] = written.into();
            doc["file"] = file.display().to_string().into();
            if let Err(e) = args.framing.print(
                &doc,
                crate::frame::Frame::Outline,
                Some(crate::frame::design_corner()),
                true,
            ) {
                return crate::frame::refused(&e);
            }
        }
        common::Format::Text => {
            let (by, says) = args.framing.text_move();
            println!("places in {says}");
            print!("{}", moved.text_moved(by));
            if let Some(said) = &moved.said {
                println!("{said}");
            }
            if written {
                println!("wrote {}", file.display());
            } else if args.dry_run && moved.routes.is_some() {
                println!("dry run: {} is not written", file.display());
            }
        }
    }
    match &moved.refused {
        Some(e) => {
            if args.format == common::Format::Text {
                eprintln!("error: {}", args.framing.routes_drc(e));
            }
            ExitCode::FAILURE
        }
        None => ExitCode::SUCCESS,
    }
}
