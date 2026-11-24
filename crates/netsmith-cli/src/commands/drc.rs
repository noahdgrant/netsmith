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
    after_long_help = "`board drc` runs netsmith's own design rules check (ADR 0037), the one `board fab` runs and the editor's PCB view shows, on a board as it stands: with no `--kicad-pcb`, the board as `fab` has it before the router, the placement (the layout file read, the placer run) with the copper of the routes file beside the source on it. Nothing is routed and nothing is written: a connection the routes file does not make is a finding (`unconnected_items`), as it is for `fab` when the router leaves it, so a board with no routes file has one for each connection. The design's requirements on where parts go are measured too (as errors, or as warnings with `constraints = \"warn\"` in netsmith.toml), and the pick lock holds the parts to its rows as it does for `fab`. For the board of a source with more than one, `--board` says which.\n\nWith `--kicad-pcb`, the check is of that KiCad board, with the `.kicad_pro` beside it when there is one (a board KiCad wrote, `layout export`'s or one that came back to `layout import` or `routes import`), and the rules netsmith wrote into the `.kicad_dru` beside it (`fab` writes one: the process's space between surface-mount pads, from holes to tracks and between pad holes, the least non-plated hole and slot, the copper round a plated hole and the space from a silkscreen text to a pad, as KiCad custom rules that KiCad's check reads too). The board says nothing else of the process it was made for: none of `fab`'s other extras (the mask web and via rules, the fiducials, the stackup) and no requirement of a design.\n\nThe text is each finding, errors first, as `fab` lists them (`error [clearance] ... at (x, y) on F.Cu: Pad 1 of C1; Track of GND`), a finding whose items are all one part's ending `(within U1's own footprint)`, which the board cannot change; then each finding the layout file waives (`waived [pad_to_pad] ..., by `reg.ic.pad_to_pad`: the reason (source: the document)`: a waiver under the part's path and the rule's name, with a reason, a source and the pads or the place, covers a finding of the part's own footprint, which then fails nothing, and a waiver that covers none is an error, `unmatched_waiver`); and a last line that says how many. Before that line, notes list the nets the copper made long (`routed long:`, each net routed more than twice the straight length between its pads and 2 mm over it, longest ratio first, with its length, the straight length, the ratio and its vias) and the nets with the most vias (`most vias:`), of the nets no zone pours, as `fab-notes.txt` has them, and the totals of the quality of the routed copper (`routing quality:`, the own-net crossings, duplicate ways, unneeded tracks and vias, short tracks and vias side by side that `board nets` lists for each net, with the worst three nets), when there is a defect, and each power net whose routed tracks drop more than 1 % of its rail's voltage (`track drop guideline:`, the message `board fab` warns with, issue #1540, then the pieces of its path indented, the one that drops most first, as `board nets` lists them, issue #1610; `board nets` lists every power net's drop); they do not fail the check. With `--format json` it is one document, as `board fab --format json` has it: `ok`, `boards` (with `outline_at`, the outline's top left corner in the frame of the places, which a place less it is in mm from the outline, as the layout file and the routes file have them, and `routes`, the same lists as data: `long` and `most_vias`, each net with `net`, `length`, `straight`, `ratio` and `vias`, left out when both are empty), `quality` (as `board nets --format json` has it, left out when there is no defect), `track_drops` (the nets over the track-drop budget, as `board nets --format json` has them, left out when there are none) and `findings`, each with `severity`, the `code` `drc`, the `rule`, the `message` with the things it is about, the `place` (`x`, `y` in mm in the frame of the board file, y down, and the `layer`), the `nets` and the `parts` it names, the `footprint`, the part whose own footprint it is within, and, for `track_current` and `via_current`, the numbers (`current` in A and `current_from`, the loads it comes from; `width` and `needed_width` in mm, or `vias` and `needed_vias`; the sizing's `copper` in mm, `rise` in K and `tolerance`); a waived finding has the `severity` `waived`, the `waiver` (its key in the layout file), the `reason` and the `source`. The `rule` is one of KiCad's names for the rules KiCad checks too: `clearance`, `shorting_items`, `tracks_crossing`, `unconnected_items`, `hole_clearance`, `hole_to_hole`, `drill_out_of_range`, `annular_width`, `track_width`, `via_diameter`, `copper_edge_clearance`, `courtyards_overlap`, `malformed_courtyard`, `silk_over_copper`, `silk_overlap`, `silk_edge_clearance`, `text_height`, `text_thickness`, `solder_mask_bridge` (the mask between the openings of pads of two nets, two pads with no net among them, narrower than the process's web), `items_not_allowed`, `track_dangling`, `via_dangling` (a via, with the vias its copper touches, joined to other copper of its net on fewer than two layers), `invalid_outline`, `starved_thermal`, `isolated_copper`, `copper_sliver`; or netsmith's own, which KiCad's project file cannot hold: `unjoinable_thermal` (a pad the fill of its pour joins by no thermal spoke and no track, and no other fill of its net joins, with the reason), `pad_island_cut` (a piece of a pour's fill with pads of its net that a track, pad or via of another net, a hole, a rule area or a pour of a higher priority cut off from the rest of the fill, with no via, plated hole, track or other fill of the net to join them: the pads, what cut it off and the piece; a warning, an error with `guidelines = \"error\"`), `pair_skew` (a diff pair whose two nets' lengths differ by more than the pair's skew limit: the pair, both lengths, the skew and the limit; a warning, an error with `guidelines = \"error\"`; `pair_skews` in `--format json` still lists every pair), `hole_to_edge`, `via_in_pad`, `constraint`, `hole_to_track`, `pad_to_pad`, `mask_to_track`, `pad_hole_to_hole`, `slot_width`, `npth_drill`, `plated_ring` (the six from `hole_to_track` on are in the `.kicad_dru` too), `track_current`, `track_impedance`, `via_current`, `track_angle`, `fiducial_edge`, `fiducial_mask`, `fiducial_keepout`, `fiducial_count`, `fiducial_size`, `fiducial_angle`, `finish_fine_pitch`, `assembly_size`, `v_cut_edge` (copper of a board under the assembly size, which is ordered as a panel, nearer its outline than the board house keeps copper from a V-cut edge, `.process.min_v_cut_edge_clearance`, with its source as `source`; a note), `test_point_pad`, `test_point_pitch`, `test_point_body`, `test_point_edge`, `acid_trap`, `stackup`, `via_span`, `via_aspect_ratio`, `unmatched_waiver`; and two notes, which fail nothing whatever the strictness: `near_rule_area` (a part, track, via or pour within the process's `.rule_area_reach`, 3 mm by default, of a rule area that forbids copper or parts, a module's antenna keep-out: under it on a layer it is not on, inside it on a layer where it does not forbid that kind, or beside it; `note [near_rule_area] ...` in the text, counted in the last line, and the `severity` `note` in JSON) and `switch_node_copper` (a via or a track of another net within the process's `.switch_node_keepout`, 1 mm by default, of a switching regulator's switch node, a part that provides `BuckIc`, or under its body, on a layer with no plane between; `board nets --power-stage` lists each regulator's stage; none for a `--kicad-pcb`, whose file names no regulator). The guide's chapter on the commands has each beside KiCad's check (_The design rules check against KiCad's_). The exit code is 0 when the check finds no error (warnings and waived findings do not fail it), 1 when it finds one or the board does not build or place, and 2 when it could not run."
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

/// What the check was made of: the report, the board's name and where its outline's corner is.
struct Made {
    board: String,
    checked: netsmith_fab::Checked,
    outline_at: Option<Place>,
    /// What of the routes file was not used as written, as `fab` says it.
    notes: Vec<String>,
    /// The quality of the routed copper (issue #1481), of a design's board.
    quality: Option<netsmith_fabricate::net_quality::Quality>,
    /// The nets over the budget of the track-drop guideline (issue #1540).
    drops: Vec<netsmith_fabricate::track_drop::NetDrop>,
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
            quality: None,
            drops: Vec::new(),
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
            let corner = made.outline_at.as_ref().map(|p| (p.x, p.y));
            let mut report = Report::new("drc");
            report.ok = clean;
            report.boards.push(BoardOut {
                board: made.board.clone(),
                dir: None,
                parts: Vec::new(),
                outline_at: made.outline_at,
                routes: made.checked.routes.clone(),
                pair_skews: made.checked.skewed.clone(),
                group_spreads: made.checked.spread.clone(),
                long_paths: made.checked.long_paths.clone(),
                quality: made.quality.clone().filter(|q| q.totals.total() > 0),
                track_drops: made.drops.clone(),
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
            if let Err(e) =
                args.framing
                    .print(&report, crate::frame::Frame::BoardFile, corner, true)
            {
                return crate::frame::refused(&e);
            }
        }
        common::Format::Text => {
            for note in &made.notes {
                eprintln!("{}: warning: {note}", made.board);
            }
            // The places of the text are in the frame asked for (`--frame`).
            let corner = made.outline_at.as_ref().map(|p| (p.x, p.y));
            let (_, dx, dy) = match args.framing.moved(crate::frame::Frame::BoardFile, corner) {
                Ok(moved) => moved,
                Err(e) => return crate::frame::refused(&e),
            };
            print!(
                "{}",
                made.checked.report.moved(netsmith_geom::Pt::new(dx, dy))
            );
            // The nets routed long and the nets with the most vias (issue #1194): notes, which
            // do not fail the check.
            for line in made.checked.routes.lines() {
                match line.strip_prefix("  ") {
                    Some(net) => println!("  {net}"),
                    None => println!("{}: note: {line}", made.board),
                }
            }
            // The groups of nets whose spread is over their tolerance (issue #1234): notes. A diff
            // pair whose skew is over its limit is a `pair_skew` finding above (issue #1587).
            for group in &made.checked.spread {
                println!("{}: note: {}", made.board, group.over_line());
            }
            // The track paths longer than their length budget (issue #1275) or losing more
            // than their loss budget (issue #1405): notes too.
            for path in &made.checked.long_paths {
                for line in path.over_lines() {
                    println!("{}: note: {line}", made.board);
                }
            }
            // The totals of the quality of the routed copper (issue #1481): a note too.
            if let Some(note) = made.quality.as_ref().and_then(|q| q.note()) {
                println!("{}: note: {note}", made.board);
            }
            for d in &made.drops {
                let Some(drop) = d.finding() else { continue };
                println!(
                    "{}: note: {} guideline: {drop}",
                    made.board,
                    netsmith_fabricate::track_drop::RULE
                );
                // The path's pieces, the one that drops most first (issue #1610).
                for line in d.path_lines((dx, dy)) {
                    println!("  {line}");
                }
            }
            let counts = made.checked.report.counts();
            let warnings = made.checked.report.warnings().count();
            // The findings of a part's own footprint the layout file waives (issue #1196).
            let waived = match made.checked.report.waived.len() {
                0 => String::new(),
                n => format!(", {n} waived"),
            };
            // The notes of what stands near a rule area (issue #1591), which fail nothing.
            let waived = match made.checked.report.notes().count() {
                0 => waived,
                n => format!("{waived}, {n} note(s)"),
            };
            if clean {
                println!(
                    "{}: design rules check: passed ({warnings} warning(s){waived})",
                    made.board
                );
            } else {
                println!(
                    "{}: design rules check: {} error(s) ({} violation(s), {} unconnected, {} footprint error(s)), {warnings} warning(s){waived}",
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
                        routes::apply_beside(&mut board, Some(&placed.layout_of(built).file))
                            .map_err(&fail)?;
                    let constraints = netsmith_pipeline::constraints::of_design(
                        &built.design,
                        placed.catalogs.constraints_warn(),
                    );
                    let checked =
                        drc::of_board(&board, constraints, placed.catalogs.strict_guidelines())
                            .map_err(&fail)?;
                    let quality = netsmith_fabricate::net_quality::Quality::of_board(&board);
                    // The track-drop guideline of `fab`, on the copper of the routes file
                    // (issue #1540).
                    let drops =
                        netsmith_fabricate::track_drop::drops_of(&board, &built.trace.model)
                            .into_iter()
                            .filter(|d| d.over)
                            .collect();
                    Ok(Made {
                        board: built.label(),
                        checked,
                        outline_at: Some(Place {
                            x: board.outline.min.x,
                            y: board.outline.min.y,
                            layer: None,
                        }),
                        notes,
                        quality: Some(quality),
                        drops,
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
    fn the_help_names_every_rule_a_finding_can_have() {
        let command = <super::Args as clap::Args>::augment_args(clap::Command::new("drc"));
        let help = command
            .get_after_long_help()
            .expect("drc has a long help")
            .to_string();
        for rule in netsmith_geom::DrcRule::ALL {
            assert!(
                help.contains(&format!("`{}`", rule.id())),
                "the help of `board drc` does not name the rule `{}`",
                rule.id()
            );
        }
    }

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
