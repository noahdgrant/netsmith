//! `netsmith board rules` (issue #1513): the rules a board is checked to, as data, for a tool
//! that draws copper by them. The document is `netsmith_geom::DrcBoardRules`, read by the
//! function the design rules check reads its numbers with (`netsmith_fabricate::drc`), from
//! the board as `board drc` has it.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_fabricate::drc;

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`board rules` prints the rules netsmith's design rules check (`board drc`, the one `fab` runs) holds a board to, so that a tool that draws copper keeps the spaces the check asks. The numbers are read by the function the check reads them with, from the same board: with no `--kicad-pcb`, the board as `board drc` has it (placed, the layout file read, the placer run for the rest, the copper of the routes file beside the source on it), its project file and the process's extras; with `--kicad-pcb`, that KiCad board with the `.kicad_pro` and the `.kicad_dru` beside it (and then none of `fab`'s other extras: no width a current needs, no via rules). For the board of a source with more than one, `--board` says which. Nothing is written.\n\nThe text is a line for each kind of rule, then the net classes, the clearance between each two classes, the pads with a clearance of their own, each net's rules, the footprints whose solder mask margin is not the board's, the rule areas and the layers with their pours. With `--format json` it is one document: `board`, `outline_at` (the outline's top left corner in the frame of the board file, as `board drc` has it; a place less it is in the frame of the layout and routes files), `clearance` (`least`, the board's; `pairs`, each with `a`, `b` and the `clearance` between nets of the two classes, the larger of theirs; `pads`, each with `part`, `pad` and the `clearance` of its own, which counts where it is larger), `classes` (the default class `Default` first, each with `name`, `clearance`, `priority`, `patterns` and what it draws its copper at: `track_width`, `via_diameter`, `via_drill`, `diff_pair_gap`, `diff_pair_width`), `nets` (each with `net`, `class`, `clearance`, `width` (the width its class draws it at, the width it wants), `least_width` and `least_width_inner` (the least on an outer and an inner layer: the board's, or what its current needs, `track_current`), `impedance_widths` (each `layer` and `width` that gives its impedance target, `track_impedance`), `vias` (how many side by side it needs where it changes layer, `via_current`), `via_diameter` and `via_drill`), `widths` (`least`, and `neck_reach`, how near one of its net's pads a track may be narrower than its current asks), `mask` (`margin`, the board's; `web`, the least mask between the openings of pads of two nets; `bridges_in_footprints`; `to_track`, the least space from a track to the opening of a pad of another net, `mask_to_track`; `footprints`, each `part` with its `margin` and the `pads` whose own `margin` is not the footprint's), `holes` (`to_copper`, `hole_clearance`; `to_track` and `plated_pad_to_track`, `hole_to_track`; `hole_to_hole`; `pad_hole_to_hole`; `least_drill`, `least_npth_drill`, `least_plated_slot`, `least_npth_slot`, `least_pad_ring`), `pad_to_pad` (between surface-mount pads of two nets), `copper_to_edge` (from copper, and from a hole, to the edge), `vias` (`least_diameter`, `least_drill`, `least_ring`, `least_microvia_diameter`, `least_microvia_drill`, the `diameter` and `drill` of the default class, `in_pad`, the widest via the process fills so it may sit in a pad, `group_gap`, the most space between the copper of two vias of one net at which they stand side by side as one group (`via_current`), `hole_pad_reach` and `ring_pad_reach`, how near a pad's copper a via's hole and its ring come before `board drc` notes them (`via_hole_near_pad`, `via_ring_near_pad`), and `spans`, the blind and buried vias it makes, each `from`, `to` and `laser`), `rule_areas` (each with `name`, `part` (the footprint that carries it, null for the board's: a footprint's area does not forbid the footprint itself), `layers`, `outlines` and `holes` as lists of `[x, y]` in the frame of the board file, and the kinds it `forbids`: `tracks`, `vias`, `pads`, `copperpour`, `footprints`; what only touches its edge is not in it) and `layers` (`copper`, front to back, `thickness`, the `stackup` top to bottom, each `name`, `kind` and `thickness`, and the `pours`, each `net`, `name`, `layers`, `clearance`, `min_thickness` and `priority`). Lengths are in mm; a rule that is not checked is null. The exit code is 0 when the rules are printed, 1 when the board does not build or place, and 2 when the command could not run."
)]
pub struct Args {
    /// The source file; with none, the project's boards (see netsmith.toml).
    #[arg(value_name = "FILE.NSM", conflicts_with = "kicad_pcb")]
    pub file: Option<String>,
    /// The board, by name, when the source has more than one.
    #[arg(long, value_name = "NAME", conflicts_with = "kicad_pcb")]
    pub board: Option<String>,
    /// The rules of this KiCad board (with the `.kicad_pro` and `.kicad_dru` beside it) instead
    /// of a design's board.
    #[arg(
        long,
        value_name = "BOARD.KICAD_PCB",
        conflicts_with_all = ["place_seeds", "place_budget", "routing_objective"]
    )]
    pub kicad_pcb: Option<PathBuf>,
    #[command(flatten)]
    pub placing: common::Placing,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
    #[command(flatten)]
    pub framing: crate::frame::Framing,
}

/// The rules of one board as a document and as text, with its name and where its outline's
/// corner is.
struct Made {
    board: String,
    json: serde_json::Value,
    rules: netsmith_geom::DrcBoardRules,
    outline_at: Option<(f64, f64)>,
}

pub fn run(args: Args) -> ExitCode {
    let made = match &args.kicad_pcb {
        Some(pcb) => match drc::rules_of_kicad_pcb(pcb) {
            Ok(rules) => Made {
                board: pcb
                    .file_stem()
                    .map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
                json: serde_json::to_value(&rules).expect("plain data"),
                rules,
                outline_at: None,
            },
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        },
        None => match of_design(&args) {
            Ok(made) => made,
            Err(code) => return code,
        },
    };
    match args.format {
        common::Format::Json => {
            let mut out = made.json;
            let rest = std::mem::take(out.as_object_mut().expect("an object"));
            let mut doc = serde_json::Map::new();
            doc.insert("board".into(), made.board.into());
            doc.insert(
                "outline_at".into(),
                made.outline_at.map_or(
                    serde_json::Value::Null,
                    |(x, y)| serde_json::json!({"x": x, "y": y}),
                ),
            );
            doc.extend(rest);
            if let Err(e) = args.framing.print(
                &serde_json::Value::Object(doc),
                crate::frame::Frame::BoardFile,
                made.outline_at,
                false,
            ) {
                return crate::frame::refused(&e);
            }
        }
        common::Format::Text => {
            let (_, dx, dy) = match args
                .framing
                .moved(crate::frame::Frame::BoardFile, made.outline_at)
            {
                Ok(moved) => moved,
                Err(e) => return crate::frame::refused(&e),
            };
            println!("{}:", made.board);
            print!("{}", made.rules.text_moved(netsmith_geom::Pt::new(dx, dy)));
        }
    }
    ExitCode::SUCCESS
}

/// The rules of the design's board, placed as `board drc` places it, with the routes file's
/// copper on it.
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
                    netsmith_fabricate::routes::apply_beside(
                        &mut board,
                        Some(&placed.layout_of(built).file),
                    )
                    .map_err(&fail)?;
                    let rules = drc::rules_of_board(&board).map_err(&fail)?;
                    Ok(Made {
                        board: built.label(),
                        json: serde_json::to_value(&rules).expect("plain data"),
                        rules,
                        outline_at: Some((board.outline.min.x, board.outline.min.y)),
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
    fn a_design_or_a_kicad_board_and_not_both() {
        let Ok(Leaf::Rules(a)) = parse(&[
            "board", "rules", "a.nsm", "--board", "B", "--format", "json",
        ]) else {
            panic!("rules should parse");
        };
        assert_eq!(a.file.as_deref(), Some("a.nsm"));
        assert_eq!(a.board.as_deref(), Some("B"));
        assert_eq!(a.format, crate::commands::common::Format::Json);
        let Ok(Leaf::Rules(a)) = parse(&["board", "rules", "--kicad-pcb", "b.kicad_pcb"]) else {
            panic!("rules should parse");
        };
        assert_eq!(
            a.kicad_pcb.as_deref(),
            Some(std::path::Path::new("b.kicad_pcb"))
        );
        assert_eq!(
            kind(&["board", "rules", "a.nsm", "--kicad-pcb", "b.kicad_pcb"]),
            ErrorKind::ArgumentConflict
        );
    }
}
