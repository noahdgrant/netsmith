//! `netsmith board fill` (issue #1187): how the fill of a board's pours, planes and zones joins
//! each pad of its net (solid, a thermal relief with the spokes that reach the fill, or not at
//! all), the pads of other nets it keeps clear of, the starved thermal reliefs and the copper
//! islands, kept and removed. The board is placed and has the copper `view pcb` draws on it
//! (`netsmith_fabricate::pcb_picture::put_copper`, with no router); the report and its text are
//! `netsmith_pcb::FillReport`'s, which the editor's PCB summary tab shows too.
//!
//! A sibling of `board copper` and not a flag of it: `board copper` reads the source alone, with
//! no placing and no fill, and is cheap; this one places the board and fills every pour as `fab`
//! does, and takes the placer's flags.

use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_fabricate::pcb_picture;

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`board fill` fills a board's pours, planes and the zones of its routes file as `fab` fills them for the Gerbers and as `board drc` judges them, and says what the fill does with the pads, so that no KiCad refill is needed to learn which pads hang on one thermal spoke. The board is placed (the layout file read, the placer run for the rest) and has the copper `view pcb` draws on it: the routes file beside the source, the router's copper the editor keeps (`.netsmith/routes/`) for every net whose pads have not moved since it was routed, and the teardrops the router draws on the finished board. Nothing is routed and nothing is written.\n\nFor each fill (each pour, plane or zone of a net on each copper layer, from the top) it lists each pad of its net whose centre is in its outline and how the fill joins it: `solid`, `thermal` with how many of its spokes reach the fill (`2 of 4 spokes`; a pad narrower than the fill's minimum width has none) and the tracks of its net that run from it onto the fill, or `none` (the pad connection `none`). A thermal relief that one or more spokes but fewer than two spokes and tracks join to the fill is starved: KiCad's `starved_thermal`, counted as KiCad counts it, the warning `board drc` gives for the same pad. One that no spoke and no track joins is unjoinable, with why the fill can make none of its spokes (`unjoinable: the end of each spoke is off the fill: the clearance of pad C1.1 [VCC] (0.3 mm); ...`): `board drc`'s `unjoinable_thermal`; its own via or track joins it. One that another fill of its net joins (a zone of the net of a higher priority over it, or a fill on another layer of a pad with a hole) is not unjoinable but `joined by another fill of its net`, as `board drc` judges it. Then the pads of other nets in the outline, which the fill keeps clear of, and the copper islands (pieces of the fill that touch no copper of its net): those the zone's island setting keeps and those the fill took out, each with a corner and its area. The counts are of pads joined by two or more spokes, by one, by none, solid and none, and of the starved and the unjoinable ones, for each fill and for the board. The places are in mm in the frame of the board file, y down, as `board drc` gives them; the outline's top left corner is printed, and a place less it is in the frame of the layout and routes files.\n\nWith `--format json` one JSON document is printed on stdout: `board`, `outline_at` (`x`, `y`), `fills` (each with `layer`, `layer_name`, `net`, `source` (`plane`, `pour` or `zone`), `held`, `area`, `pads` (`part`, `pad`, `at`, `join` (`solid`, `thermal` or `none`), `spokes`, `joined_spokes`, `tracks`, `joined`, `starved`, `unjoinable` with the reason when it is, and `joined_elsewhere`, true when another fill of its net joins a relief this one joins by nothing), `counts`, `cleared` (`part`, `pad`, `net`, `at`), `islands` and `removed_islands` (`at`, `area`)), `counts` (`no_spokes`, `one_spoke`, `two_or_more_spokes`, `solid`, `none`, `starved`, `unjoinable`), `starved` (`layer`, `net`, `part`, `pad`, `at`, `spokes`, `joined_spokes`, `tracks`) and `unjoinable` (`layer`, `net`, `part`, `pad`, `at`, `spokes`, `why`). The exit code is 0 when it is printed (starved pads too: `board drc` is the check), 1 when the board does not build or place, or cannot be filled, and 2 when the command cannot run."
)]
pub struct Args {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The board, by name, when the source has more than one.
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    #[command(flatten)]
    pub placing: common::Placing,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

pub fn run(args: Args) -> ExitCode {
    let made = common::with_stage_events(|| {
        common::with_placed_board(
            args.file.as_deref(),
            args.board.as_deref(),
            Some(&args.placing),
            |placed| {
                placed.with_board(|built, board| {
                    let label = built.label();
                    let mut board = board.clone();
                    let say = |line: String| eprintln!("{label}: {line}");
                    pcb_picture::put_copper(&mut board, &placed.layout, None, &say)
                        .map_err(|e| format!("error: {label}: {e}\n"))?;
                    let mut report = netsmith_pcb::FillReport::of(&board)
                        .map_err(|e| format!("error: {label}: {e}\n"))?;
                    report.board = label;
                    Ok(report)
                })?
            },
        )
    });
    match made {
        Ok(report) => {
            match args.format {
                common::Format::Json => {
                    println!("{}", serde_json::to_string(&report).expect("plain data"))
                }
                common::Format::Text => print!("{}", report.text()),
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::common;
    use crate::commands::common::testing::{Leaf, parse};

    #[test]
    fn a_file_a_board_the_placer_and_a_format() {
        let Ok(Leaf::Fill(a)) = parse(&[
            "board",
            "fill",
            "a.nsm",
            "--board",
            "B",
            "--place-seeds",
            "2",
            "--format",
            "json",
        ]) else {
            panic!("fill should parse");
        };
        assert_eq!(a.file.as_deref(), Some("a.nsm"));
        assert_eq!(a.board.as_deref(), Some("B"));
        assert_eq!(a.placing.place_seeds, Some(2));
        assert_eq!(a.format, common::Format::Json);
        let Ok(Leaf::Fill(a)) = parse(&["board", "fill"]) else {
            panic!("fill should parse");
        };
        assert_eq!(
            (a.file, a.board, a.format),
            (None, None, common::Format::Text)
        );
    }
}
