//! `netsmith board fill` (issue #1187): how the fill of a board's pours, planes and zones joins
//! each pad of its net (solid, a thermal relief with the spokes that reach the fill, or not at
//! all), the pads of other nets it keeps clear of, the starved thermal reliefs and the copper
//! islands, kept and removed, and a warning for each ground pad of a switching regulator a fill
//! does not join solid (issue #1586). The board is placed and has the copper `view pcb` draws on it
//! (`netsmith_fabricate::pcb_picture::put_copper`, with no router); the report and its text are
//! `netsmith_pcb::FillReport`'s, which the editor's PCB summary tab shows too.
//!
//! With `--islands` (issue #1655) it prints the regions of each fill instead
//! (`netsmith_pcb::IslandsReport`): a flag and not a command of its own, since it fills the same
//! board the same way and asks the same question of it (what the fill joins), and not a section
//! of the default report, since it follows each net's copper across every layer, which the default
//! report does not need, and its document is of another shape.
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
    after_long_help = "`board fill` fills a board's pours, planes and the zones of its routes file as `fab` fills them for the Gerbers and as `board drc` judges them, and says what the fill does with the pads, so that no KiCad refill is needed to learn which pads hang on one thermal spoke. The board is placed (the layout file read, the placer run for the rest) and has the copper `view pcb` draws on it: the routes file beside the source, the router's copper the editor keeps (`.netsmith/routes/`) for every net whose pads have not moved since it was routed, and the teardrops the router draws on the finished board. Nothing is routed and nothing is written.\n\nFor each fill (each pour, plane or zone of a net on each copper layer, from the top) it lists each pad of its net whose centre is in its outline and how the fill joins it: `solid`, `thermal` with how many of its spokes reach the fill (`2 of 4 spokes`; a pad narrower than the fill's minimum width has none) and the tracks of its net that run from it onto the fill, or `none` (the pad connection `none`). A thermal relief that one or more spokes but fewer than two spokes and tracks join to the fill is starved: KiCad's `starved_thermal`, counted as KiCad counts it, the warning `board drc` gives for the same pad. One that no spoke and no track joins is unjoinable, with why the fill can make none of its spokes (`unjoinable: the end of each spoke is off the fill: the clearance of pad C1.1 [VCC] (0.3 mm); ...`): `board drc`'s `unjoinable_thermal`; its own via or track joins it. One that another fill of its net joins (a zone of the net of a higher priority over it, or a fill on another layer of a pad with a hole) is not unjoinable but `joined by another fill of its net`, as `board drc` judges it. Then the pads of other nets in the outline, which the fill keeps clear of, and the copper islands (pieces of the fill that touch no copper of its net): those the zone's island setting keeps and those the fill took out, each with a corner and its area. Then a warning for each ground pad of a switching regulator (a part that provides `BuckIc`, as `board nets --power-stage` finds them: the IC's pads on the net of its `vin.neg`, of whichever of its pins) that no fill of its net on its layer joins with solid copper (issue #1586): one fill that joins it solid, a patch zone over it or a zone of the routes file, is enough (issue #1666). It says how each fill that holds the pad holds it (a thermal relief however many of its spokes reach the fill, no join, or solid copper that does not reach it) and the IC's pin that has the pad, to state `.pour_connection &= PadConnection::Solid` on, with the pads that one setting reaches (issue #1667); of a pad that no fill reaches, that no setting of the pin changes it. The counts are of pads joined by two or more spokes, by one, by none, solid and none, and of the starved and the unjoinable ones, for each fill and for the board. The places are in mm in the frame of the board file, y down, as `board drc` gives them; the outline's top left corner is printed, and a place less it is in the frame of the layout and routes files.\n\nWith `--format json` one JSON document is printed on stdout: `board`, `outline_at` (`x`, `y`), `fills` (each with `layer`, `layer_name`, `net`, `source` (`plane`, `pour` or `zone`), `held`, `area`, `outline` (the fill's copper, piece by piece: each with its `outer` ring and its `holes`, rings of `x`, `y` simplified within 2 µm, issue #1293), `pads` (`part`, `pad`, `at`, `join` (`solid`, `thermal` or `none`), `spokes`, `joined_spokes`, `tracks`, `joined`, `starved`, `unjoinable` with the reason when it is, and `joined_elsewhere`, true when another fill of its net joins a relief this one joins by nothing), `counts`, `cleared` (`part`, `pad`, `net`, `at`), `islands` and `removed_islands` (`at`, `area`)), `counts` (`no_spokes`, `one_spoke`, `two_or_more_spokes`, `solid`, `none`, `starved`, `unjoinable`), `starved` (`layer`, `net`, `part`, `pad`, `at`, `spokes`, `joined_spokes`, `tracks`) and `unjoinable` (`layer`, `net`, `part`, `pad`, `at`, `spokes`, `why`) and `regulator_grounds` (`path`, the regulator IC's, `layer`, `net`, `part`, `pad`, `at`, `join`, `spokes`, `joined_spokes`, `starved` and `stated_by`, the pin that states the pad's connection when one does, `pin`, the IC's pin that has the pad, `pin_pads`, the pads its setting reaches, `fills` (`source`, `join`, `joined`, `spokes`, `joined_spokes`, `starved`), each fill that holds the pad, and `warning`, the words of the warning). With `--islands` it prints the regions of each fill instead (issue #1655): for each fill, each separate region of its copper with a corner, its box and its area, the pads and vias of its net it joins, and whether it is in the net's main region (the copper of the net, followed across every layer through its tracks, vias and pads with holes, that holds the most of its pads), or cut off from it; a fill of one region in the main one says so on one line. Of a region cut off, each track, via and pad of another net and each rule area that forbids pours that bounds it (its copper within the fill's clearance and minimum width of the region's edge), with its net, its place and its gap to the region, the ones that come as near a region of the main one on the layer first (`it parts the two`): the track that closed a pocket is among them. Its JSON has `board`, `outline_at`, `fills` (each with `layer`, `layer_name`, `net`, `source` and `regions`: `at`, `bounds` (`min`, `max`), `area`, `outline` (`outer`, `holes`), `pads` (`part`, `pad`, `at`), `vias` (`at`), `main` and `bounded_by` (`kind`: `track`, `via`, `pad` or `rule_area`, `net`, `at`, a track's `to` and `width`, a pad's `part` and `pad`, a rule area's `name`, `gap` and `parts_from_main`)) and `cut_off`, the count of the regions cut off. The exit code is 0 when it is printed (starved pads too: `board drc` is the check), 1 when the board does not build or place, or cannot be filled, and 2 when the command cannot run."
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
    /// Print each fill's regions: the pads and vias each joins, and what cuts one off.
    #[arg(long)]
    pub islands: bool,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
    #[command(flatten)]
    pub framing: crate::frame::Framing,
}

pub fn run(args: Args) -> ExitCode {
    if args.islands {
        return match report_of(&args, netsmith_pcb::IslandsReport::of) {
            Ok(r) => print(&args, r.outline_at, &r, |r, by, frame| {
                r.text_moved(by, frame)
            }),
            Err(code) => code,
        };
    }
    match report_of(&args, netsmith_pcb::FillReport::of) {
        Ok(r) => print(&args, r.outline_at, &r, |r, by, frame| {
            r.text_moved(by, frame)
        }),
        Err(code) => code,
    }
}

/// A board's report: the board placed, with the copper `view pcb` draws, given to `of`.
fn report_of<R: Labelled>(
    args: &Args,
    of: impl Fn(&netsmith_pcb::Board) -> Result<R, netsmith_pcb::CopperError>,
) -> Result<R, ExitCode> {
    common::with_stage_events(|| {
        common::with_placed_board(
            args.file.as_deref(),
            args.board.as_deref(),
            Some(&args.placing),
            |placed| {
                placed.with_board(|built, board| {
                    let label = built.label();
                    let mut board = board.clone();
                    let say = |line: String| eprintln!("{label}: {line}");
                    pcb_picture::put_copper(&mut board, &placed.layout_of(built).file, None, &say)
                        .map_err(|e| format!("error: {label}: {e}\n"))?;
                    let mut report = of(&board).map_err(|e| format!("error: {label}: {e}\n"))?;
                    report.label(label);
                    Ok(report)
                })?
            },
        )
    })
}

/// A report named for the board it is of.
trait Labelled {
    fn label(&mut self, label: String);
}

impl Labelled for netsmith_pcb::FillReport {
    fn label(&mut self, label: String) {
        self.board = label;
    }
}

impl Labelled for netsmith_pcb::IslandsReport {
    fn label(&mut self, label: String) {
        self.board = label;
    }
}

/// Prints `report` in the format and the frame asked for: its places are in the board file's
/// frame, the outline's corner at `corner`.
fn print<R: serde::Serialize>(
    args: &Args,
    corner: netsmith_pcb::Pt,
    report: &R,
    text: impl Fn(&R, netsmith_pcb::Pt, &str) -> String,
) -> ExitCode {
    let corner = (corner.x, corner.y);
    match args.format {
        common::Format::Json => {
            if let Err(e) =
                args.framing
                    .print(report, crate::frame::Frame::BoardFile, Some(corner), false)
            {
                return crate::frame::refused(&e);
            }
        }
        common::Format::Text => {
            let (frame, dx, dy) = match args
                .framing
                .moved(crate::frame::Frame::BoardFile, Some(corner))
            {
                Ok(moved) => moved,
                Err(e) => return crate::frame::refused(&e),
            };
            print!(
                "{}",
                text(report, netsmith_pcb::Pt::new(dx, dy), frame.words())
            );
        }
    }
    ExitCode::SUCCESS
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
            (a.file, a.board, a.format, a.islands),
            (None, None, common::Format::Text, false)
        );
        let Ok(Leaf::Fill(a)) = parse(&["board", "fill", "--islands"]) else {
            panic!("fill --islands should parse");
        };
        assert!(a.islands);
    }
}
