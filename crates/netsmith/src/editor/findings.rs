//! What is wrong with a placed board, and where (ADR 0030, E3): netsmith's own checks of the
//! placement and of the connections the router left, and KiCad's design rules check, each
//! finding with a point and a box on the sheet so the page can zoom to it.

use std::{
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

use netsmith_editor::{
    Say,
    api::{FindingSource, KicadState, PcbBox, PcbFinding, Severity},
};
use netsmith_fab::{
    KicadCli, KicadCliExe,
    artwork::Artwork,
    drc::{self, Rule, constraints::Constraints},
};
use netsmith_pcb::{Board, PlacementProblem, Pt, Ratsnest, Rect, kicad_pcb, kicad_pro};

/// What KiCad's check found on a board, or why it could not be run.
#[derive(Debug, Clone, PartialEq)]
pub struct KicadResult {
    pub state: KicadState,
    pub note: String,
    pub findings: Vec<PcbFinding>,
}

/// The least a finding's box is, in mm each way, so a zoom to a point is not to nothing.
const LEAST: f64 = 3.0;

fn finding_box(r: Rect) -> PcbBox {
    let c = r.centre();
    let (w, h) = (r.width().max(LEAST), r.height().max(LEAST));
    PcbBox {
        x: c.x - w / 2.0,
        y: c.y - h / 2.0,
        width: w,
        height: h,
    }
}

fn make(
    id: String,
    source: FindingSource,
    severity: Severity,
    rule: &str,
    message: String,
    area: Rect,
    parts: Vec<String>,
) -> PcbFinding {
    let at = area.centre();
    PcbFinding {
        id,
        source,
        severity,
        rule: rule.to_string(),
        message,
        x: at.x,
        y: at.y,
        area: finding_box(area),
        parts,
    }
}

/// Names the findings of a kind apart: the same subject twice gets `#2`.
struct Ids(Vec<String>);

impl Ids {
    fn id(&mut self, base: String) -> String {
        let n = self.0.iter().filter(|b| **b == base).count();
        self.0.push(base.clone());
        if n == 0 {
            base
        } else {
            format!("{base}#{}", n + 1)
        }
    }
}

/// The problems with where the parts are, each with the box it is about.
pub fn placement_findings(board: &Board) -> Vec<PcbFinding> {
    let courtyard = |refdes: &str| board.part(refdes).map(|p| p.courtyard());
    let cutout = |name: &str| {
        board
            .cutout_shapes()
            .into_iter()
            .find(|(c, _)| c.name == name)
            .map(|(_, shape)| shape.rect)
    };
    let keepout = |name: &str| {
        board
            .keepout_areas()
            .into_iter()
            .find(|k| k.name == name)
            .map(|k| k.rect)
    };
    let mut ids = Ids(Vec::new());
    board
        .check_placement()
        .into_iter()
        .map(|problem| {
            let message = problem.to_string();
            let (rule, subjects, area): (&str, Vec<&str>, Option<Rect>) = match &problem {
                PlacementProblem::Overlap { a, b } => {
                    let (ra, rb) = (courtyard(a), courtyard(b));
                    // The area they share, or the two if the board says they touch.
                    let shared = ra.zip(rb).map(|(x, y)| {
                        Rect::new(
                            Pt::new(x.min.x.max(y.min.x), x.min.y.max(y.min.y)),
                            Pt::new(x.max.x.min(y.max.x), x.max.y.min(y.max.y)),
                        )
                    });
                    ("overlap", vec![a, b], shared)
                }
                PlacementProblem::OutsideOutline { refdes, .. } => {
                    ("outside_outline", vec![refdes], courtyard(refdes))
                }
                PlacementProblem::PadNearEdge { refdes, .. } => {
                    ("pad_near_edge", vec![refdes], courtyard(refdes))
                }
                PlacementProblem::InCutout { refdes, cutout: c } => (
                    "in_cutout",
                    vec![refdes],
                    courtyard(refdes).zip(cutout(c)).map(|(a, b)| a.union(&b)),
                ),
                PlacementProblem::PadNearCutout {
                    refdes, cutout: c, ..
                } => (
                    "pad_near_cutout",
                    vec![refdes],
                    courtyard(refdes).zip(cutout(c)).map(|(a, b)| a.union(&b)),
                ),
                PlacementProblem::InKeepout { refdes, keepout: k } => (
                    "in_keepout",
                    vec![refdes],
                    courtyard(refdes).zip(keepout(k)).map(|(a, b)| a.union(&b)),
                ),
                PlacementProblem::PadInKeepout { refdes, keepout: k } => (
                    "pad_in_keepout",
                    vec![refdes],
                    courtyard(refdes).zip(keepout(k)).map(|(a, b)| a.union(&b)),
                ),
                PlacementProblem::CutoutOutsideOutline { cutout: c } => {
                    ("cutout_outside_outline", vec![], cutout(c))
                }
                // Nowhere: the board's corner is where the page looks.
                PlacementProblem::NoPosition { .. } => (
                    "no_position",
                    vec![],
                    Some(Rect::new(board.outline.min, board.outline.min)),
                ),
                PlacementProblem::HoleTooNear { refdes, other, .. } => (
                    "hole_too_near",
                    vec![refdes, other],
                    courtyard(refdes)
                        .zip(courtyard(other))
                        .map(|(a, b)| a.union(&b)),
                ),
            };
            let area = area.unwrap_or(board.outline);
            let parts: Vec<String> = subjects.iter().map(ToString::to_string).collect();
            let id = ids.id(format!("placement:{rule}:{}", parts.join("+")));
            make(
                id,
                FindingSource::Placement,
                Severity::Error,
                rule,
                message,
                area,
                parts,
            )
        })
        .collect()
}

/// The connections still to be made on a routed board, each from pad to pad.
pub fn unrouted_findings(board: &Board) -> Vec<PcbFinding> {
    let mut ids = Ids(Vec::new());
    Ratsnest::of(board)
        .lines
        .iter()
        .map(|line| {
            let area = if line.to_plane {
                Rect::new(line.from_at, line.from_at)
            } else {
                Rect::around([line.from_at, line.to_at]).expect("two points")
            };
            let parts = if line.to_plane || line.from.refdes == line.to.refdes {
                vec![line.from.refdes.clone()]
            } else {
                vec![line.from.refdes.clone(), line.to.refdes.clone()]
            };
            let id = ids.id(format!("unrouted:{}:{}:{}", line.net, line.from, line.to));
            make(
                id,
                FindingSource::Unrouted,
                Severity::Error,
                "unrouted",
                line.to_string(),
                area,
                parts,
            )
        })
        .collect()
}

/// netsmith's own design rules check of a routed board, as findings: the board is written as
/// the `.kicad_pcb` and `.kicad_pro` that `fab` writes and checked as `fab` checks it (ADR
/// 0037). Not listed here: the connections left, which are the ratsnest's (`unrouted`), and
/// courtyards that overlap, which are the placement check's. KiCad is not involved.
pub fn drc_findings(board: &Board) -> Vec<PcbFinding> {
    let extras = drc::Extras {
        mask_web: Some(board.rules.mask_bridge),
        via_in_pad: Some(board.process.via_in_pad),
    };
    let report = match drc::check(&kicad_pcb(board), Some(&kicad_pro(board)), &extras) {
        Ok(report) => report,
        // The board file is netsmith's own, so this is a mistake in netsmith; say so.
        Err(e) => {
            return vec![make(
                "drc:failed".to_string(),
                FindingSource::Drc,
                Severity::Error,
                "drc_failed",
                format!("netsmith's design rules check could not read the board it wrote: {e}"),
                board.outline,
                Vec::new(),
            )];
        }
    };
    let mut ids = Ids(Vec::new());
    report
        .findings
        .iter()
        .filter(|f| !matches!(f.rule, Rule::UnconnectedItems | Rule::CourtyardsOverlap))
        .map(|f| from_drc(f, board, &mut ids))
        .collect()
}

/// A finding of netsmith's design rules check as the page shows it.
fn from_drc(f: &drc::Finding, board: &Board, ids: &mut Ids) -> PcbFinding {
    let severity = match f.severity {
        drc::Severity::Error => Severity::Error,
        _ => Severity::Warning,
    };
    let points: Vec<Pt> = std::iter::once(Pt::new(f.at.x, f.at.y))
        .chain(f.items.iter().map(|i| Pt::new(i.at.x, i.at.y)))
        .collect();
    let area = Rect::around(points).unwrap_or(board.outline);
    let id = ids.id(format!("drc:{}:{:.2}:{:.2}", f.rule.id(), f.at.x, f.at.y));
    let mut finding = make(
        id,
        FindingSource::Drc,
        severity,
        f.rule.id(),
        f.with_items(),
        area,
        f.parts(),
    );
    // The finding is at the place of the problem, not at the middle of its box.
    finding.x = f.at.x;
    finding.y = f.at.y;
    finding
}

/// The design's requirements on where parts go (ADR 0045), measured on the placed board as
/// `fab` measures them on its file: a finding for each one the placement does not meet, with
/// the numbers. They are listed with the placement's, since they need no copper. A board the
/// design puts no requirement on has none.
pub fn constraint_findings(board: &Board, constraints: &Constraints) -> Vec<PcbFinding> {
    let art = match Artwork::parse(&kicad_pcb(board)) {
        Ok(art) => art,
        Err(e) => {
            return vec![make(
                "constraint:failed".to_string(),
                FindingSource::Placement,
                Severity::Error,
                "constraint",
                format!(
                    "netsmith could not read the board it wrote to measure the requirements on where parts go: {e}"
                ),
                board.outline,
                Vec::new(),
            )];
        }
    };
    let outcomes = constraints.measure(&art);
    let violated = outcomes.iter().filter(|o| !o.satisfied);
    let mut ids = Ids(Vec::new());
    violated
        .zip(drc::constraints::outcome_findings(
            &outcomes,
            constraints.severity,
        ))
        .map(|(outcome, f)| {
            let mut finding = from_drc(&f, board, &mut ids);
            // The message already names the part and the numbers; the generic listing of
            // the items after it (`: Footprint J2`) says the same again.
            finding.message = f.message.clone();
            // Named by the requirement, so that it keeps its name as the part is dragged.
            finding.id = ids.id(format!("constraint:{}", outcome.id));
            finding.source = FindingSource::Placement;
            finding
        })
        .collect()
}

/// The findings in KiCad's JSON report. `board` names the parts the descriptions mention.
pub fn parse_kicad(json: &str, board: &Board, routed: bool) -> Result<Vec<PcbFinding>, String> {
    let report: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("KiCad's report is not JSON: {e}"))?;
    let refs: Vec<&str> = board.parts.iter().map(|p| p.refdes.as_str()).collect();
    let mut ids = Ids(Vec::new());
    let mut found = Vec::new();
    // Every connection of a board that is not routed is unconnected: that is the ratsnest.
    for section in ["violations", "unconnected_items"] {
        if section == "unconnected_items" && !routed {
            continue;
        }
        let Some(entries) = report.get(section).and_then(|v| v.as_array()) else {
            continue;
        };
        for entry in entries {
            let text = |v: &serde_json::Value, key: &str| {
                v.get(key)
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            let severity = match text(entry, "severity").as_str() {
                "error" => Severity::Error,
                "warning" => Severity::Warning,
                // What the person chose to ignore or exclude is not a finding.
                _ => continue,
            };
            let rule = text(entry, "type");
            let items = entry
                .get("items")
                .and_then(|v| v.as_array())
                .map(Vec::as_slice)
                .unwrap_or_default();
            let points: Vec<Pt> = items
                .iter()
                .filter_map(|i| {
                    let pos = i.get("pos")?;
                    Some(Pt::new(pos.get("x")?.as_f64()?, pos.get("y")?.as_f64()?))
                })
                .collect();
            let area = Rect::around(points).unwrap_or(board.outline);
            // The parts an item names: `Pad 1 [VIN] of C1`, `Footprint R1`.
            let mut parts: Vec<String> = Vec::new();
            for item in items {
                let described = text(item, "description");
                for word in described.split(|c: char| !c.is_alphanumeric()) {
                    if refs.contains(&word) && !parts.iter().any(|p| p == word) {
                        parts.push(word.to_string());
                    }
                }
            }
            let message = if items.is_empty() {
                text(entry, "description")
            } else {
                format!(
                    "{}: {}",
                    text(entry, "description"),
                    items
                        .iter()
                        .map(|i| text(i, "description"))
                        .collect::<Vec<_>>()
                        .join("; ")
                )
            };
            let at = area.centre();
            let id = ids.id(format!("kicad:{rule}:{:.2}:{:.2}", at.x, at.y));
            found.push(make(
                id,
                FindingSource::Kicad,
                severity,
                &rule,
                message,
                area,
                parts,
            ));
        }
    }
    Ok(found)
}

/// Run KiCad's check on the board and read what it found.
pub fn run_kicad(board: &Board, routed: bool, say: &Say) -> KicadResult {
    let unavailable = |note: String| KicadResult {
        state: KicadState::Unavailable,
        note,
        findings: Vec::new(),
    };
    let Some(cli) = KicadCliExe::locate() else {
        return unavailable(
            "kicad-cli was not found (set KICAD_CLI, or install KiCad 9): only netsmith's own checks ran"
                .to_string(),
        );
    };
    static RUNS: AtomicU32 = AtomicU32::new(0);
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "netsmith-drc-{}-{}",
        std::process::id(),
        RUNS.fetch_add(1, Ordering::Relaxed)
    ));
    let pcb = dir.join(format!("{}.kicad_pcb", board.name));
    let written = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(&pcb, kicad_pcb(board)))
        .and_then(|()| std::fs::write(pcb.with_extension("kicad_pro"), kicad_pro(board)));
    if let Err(e) = written {
        let _ = std::fs::remove_dir_all(&dir);
        return unavailable(format!("cannot write {}: {e}", pcb.display()));
    }
    say(&format!(
        "{}: running KiCad's design rules check",
        board.name
    ));
    let ran = cli.drc_json(&pcb);
    let _ = std::fs::remove_dir_all(&dir);
    match ran
        .map_err(|e| e.to_string())
        .and_then(|json| parse_kicad(&json, board, routed))
    {
        Ok(findings) => KicadResult {
            state: KicadState::Ran,
            note: format!(
                "KiCad's design rules check found {} problem(s)",
                findings.len()
            ),
            findings,
        },
        Err(e) => unavailable(format!("KiCad's check failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use netsmith_pcb::Board;
    use netsmith_solve::Catalogs;

    use super::*;

    /// An example placed as the placer has it, with the sample footprints.
    fn placed(example: &str) -> Board {
        let path = format!(
            "{}/../../examples/{example}.nsm",
            env!("CARGO_MANIFEST_DIR")
        );
        let libraries = crate::Libraries::sample();
        let built = crate::build_with(
            &path,
            &std::fs::read_to_string(&path).unwrap(),
            &netsmith_check::Loader::std(),
            &Catalogs::sample(),
            &libraries,
        )
        .unwrap();
        Board::from_design(&built[0].design, &libraries.mapping, &libraries.footprints).unwrap()
    }

    #[test]
    fn a_requirement_on_where_a_part_goes_is_a_finding_with_its_numbers_and_the_requirements_name()
    {
        use drc::constraints::{Constraint, Measure};
        let board = placed("led");
        let part = board.parts[0].refdes.clone();
        let constraints = |severity| Constraints {
            list: vec![Constraint {
                id: "supply.place.left".into(),
                source: "led.nsm:9".into(),
                parts: vec![part.clone()],
                measure: Measure::Left,
                lo: 500.0,
                hi: 501.0,
            }],
            sides: Vec::new(),
            severity,
        };
        let found = constraint_findings(&board, &constraints(drc::Severity::Error));
        assert_eq!(found.len(), 1, "{found:#?}");
        let f = &found[0];
        assert_eq!(f.id, "constraint:supply.place.left");
        assert_eq!(
            (f.rule.as_str(), f.severity),
            ("constraint", Severity::Error)
        );
        assert_eq!(f.parts, std::slice::from_ref(&part));
        assert!(
            f.message
                .contains("constraint supply.place.left (led.nsm:9) is violated")
                && f.message.contains("it allows 500 to 501 mm")
                && f.message.ends_with("mm over)"),
            "{}",
            f.message
        );
        // The project can make it a warning, and a met requirement is no finding.
        let warned = constraint_findings(&board, &constraints(drc::Severity::Warning));
        assert_eq!(warned[0].severity, Severity::Warning);
        let mut met = constraints(drc::Severity::Error);
        met.list[0].lo = f64::NEG_INFINITY;
        met.list[0].hi = f64::INFINITY;
        assert_eq!(constraint_findings(&board, &met), []);
    }

    #[test]
    fn a_clean_placement_has_no_finding_and_an_overlap_is_found_where_the_courtyards_meet() {
        let mut board = placed("led");
        assert_eq!(placement_findings(&board), []);
        let (a, b) = (board.parts[0].refdes.clone(), board.parts[1].refdes.clone());
        board.parts[1].at = board.parts[0].at;
        let found = placement_findings(&board);
        let overlap = found
            .iter()
            .find(|f| f.rule == "overlap")
            .expect("an overlap");
        assert_eq!(overlap.id, format!("placement:overlap:{a}+{b}"));
        assert_eq!(overlap.parts, [a.clone(), b.clone()]);
        assert!(
            overlap
                .message
                .contains(&format!("the courtyards of {a} and {b} overlap"))
        );
        for refdes in [&a, &b] {
            let c = board.part(refdes).unwrap().courtyard();
            assert!(c.min.x <= overlap.x && overlap.x <= c.max.x);
            assert!(c.min.y <= overlap.y && overlap.y <= c.max.y);
        }
    }

    #[test]
    fn a_part_off_the_board_is_found_at_the_part_and_each_finding_has_its_own_id() {
        let mut board = placed("led");
        board.parts[0].at.x += 500.0;
        board.parts[1].at.x += 500.0;
        let found = placement_findings(&board);
        let outside: Vec<_> = found
            .iter()
            .filter(|f| f.rule == "outside_outline")
            .collect();
        assert_eq!(outside.len(), 2);
        assert_eq!(outside[0].parts, [board.parts[0].refdes.clone()]);
        assert!(outside[0].x > 400.0, "{}", outside[0].x);
        let mut ids: Vec<_> = found.iter().map(|f| f.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), found.len());
    }

    #[test]
    fn every_connection_that_is_left_is_a_finding_between_the_pads_it_joins() {
        let board = placed("led");
        let lines = Ratsnest::of(&board).lines;
        let found = unrouted_findings(&board);
        assert_eq!(found.len(), lines.len());
        assert!(!found.is_empty());
        for (f, line) in found.iter().zip(&lines) {
            assert_eq!(
                (f.rule.as_str(), f.source),
                ("unrouted", FindingSource::Unrouted)
            );
            assert_eq!(f.message, line.to_string());
            assert!(f.parts.contains(&line.from.refdes));
            let mid = Pt::new(
                (line.from_at.x + line.to_at.x) / 2.0,
                (line.from_at.y + line.to_at.y) / 2.0,
            );
            assert!((f.x - mid.x).abs() < 1e-9 && (f.y - mid.y).abs() < 1e-9);
            assert!(f.area.width >= LEAST && f.area.height >= LEAST);
        }
    }

    /// What `kicad-cli pcb drc --format json` writes, cut to what the editor reads.
    fn report(part: &str, other: &str) -> String {
        format!(
            r#"{{
  "$schema": "https://schemas.kicad.org/drc.v1.json",
  "units": "mm",
  "violations": [
    {{
      "description": "Courtyards overlap",
      "items": [
        {{"description": "Footprint {part}", "pos": {{"x": 10.0, "y": 20.0}}, "uuid": "a"}},
        {{"description": "Footprint {other}", "pos": {{"x": 14.0, "y": 21.0}}, "uuid": "b"}}
      ],
      "severity": "error",
      "type": "courtyards_overlap"
    }},
    {{
      "description": "Silkscreen clipped by solder mask",
      "items": [{{"description": "Segment on F.Silkscreen", "pos": {{"x": 3.0, "y": 4.0}}}}],
      "severity": "warning",
      "type": "silk_over_copper"
    }},
    {{
      "description": "Something the person chose to ignore",
      "items": [],
      "severity": "ignore",
      "type": "hole_clearance"
    }}
  ],
  "unconnected_items": [
    {{
      "description": "Missing connection between items",
      "items": [
        {{"description": "Pad 1 [VIN] of {part}", "pos": {{"x": 1.0, "y": 2.0}}}},
        {{"description": "Pad 2 [VIN] of {other}", "pos": {{"x": 5.0, "y": 8.0}}}}
      ],
      "severity": "error",
      "type": "unconnected_items"
    }}
  ],
  "schematic_parity": []
}}"#
        )
    }

    #[test]
    fn kicads_report_becomes_findings_with_places_and_the_parts_they_name() {
        let board = placed("led");
        let (a, b) = (board.parts[0].refdes.clone(), board.parts[1].refdes.clone());
        let json = report(&a, &b);
        let found = parse_kicad(&json, &board, false).unwrap();
        // The ignored one is not a finding, and nothing is unconnected on a board not routed.
        assert_eq!(found.len(), 2, "{found:?}");
        let overlap = &found[0];
        assert_eq!(overlap.rule, "courtyards_overlap");
        assert_eq!(
            (overlap.source, overlap.severity),
            (FindingSource::Kicad, Severity::Error)
        );
        assert_eq!(
            overlap.message,
            format!("Courtyards overlap: Footprint {a}; Footprint {b}")
        );
        assert_eq!(overlap.parts, [a.clone(), b.clone()]);
        assert_eq!((overlap.x, overlap.y), (12.0, 20.5));
        // The box is round the two points, at least as big as a zoom needs.
        assert!(overlap.area.x <= 10.0 && overlap.area.x + overlap.area.width >= 14.0);
        assert_eq!(found[1].severity, Severity::Warning);
        assert_eq!(found[1].parts, Vec::<String>::new());
        // Routed, the connections KiCad says are missing count too.
        let routed = parse_kicad(&json, &board, true).unwrap();
        assert_eq!(routed.len(), 3);
        assert_eq!(routed[2].rule, "unconnected_items");
        assert_eq!(routed[2].parts, [a, b]);
        assert_eq!(
            parse_kicad("not json", &board, true)
                .unwrap_err()
                .split(':')
                .next(),
            Some("KiCad's report is not JSON")
        );
    }

    #[test]
    fn our_design_rules_check_of_a_routed_board_has_places_parts_and_numbers() {
        use netsmith_pcb::{CopperLayer, Track, route};
        let mut board = placed("led");
        assert!(route(&mut board).is_empty());
        let clean = drc_findings(&board);
        assert_eq!(
            clean
                .iter()
                .filter(|f| f.severity == Severity::Error)
                .count(),
            0,
            "{clean:?}"
        );
        // A track of another net laid 0.05 mm from a pad: a clearance finding at the pad.
        let part = board.parts[0].clone();
        let pad = &part.footprint.pads[0];
        let at = part.pad_bounds(pad);
        let net = board
            .nets
            .iter()
            .find(|n| n.pads.iter().all(|p| p.refdes != part.refdes))
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "OTHER".to_string());
        let y = at.min.y - 0.05 - 0.05;
        board.tracks.push(Track {
            net,
            layer: CopperLayer::Front,
            from: Pt::new(at.min.x - 0.2, y),
            to: Pt::new(at.max.x + 0.2, y),
            width: 0.1,
        });
        let found = drc_findings(&board);
        let clearance = found
            .iter()
            .find(|f| f.rule == "clearance")
            .unwrap_or_else(|| panic!("{found:?}"));
        assert_eq!(clearance.source, FindingSource::Drc);
        assert_eq!(clearance.severity, Severity::Error);
        assert!(
            clearance.message.contains("the least is"),
            "{}",
            clearance.message
        );
        assert!(
            clearance.parts.contains(&part.refdes),
            "{:?}",
            clearance.parts
        );
        assert!(
            (clearance.y - (at.min.y - 0.05)).abs() < 0.2,
            "{}",
            clearance.y
        );
        assert!(clearance.id.starts_with("drc:clearance:"));
        // The connections left and the courtyards are other findings' business.
        assert!(
            found
                .iter()
                .all(|f| f.rule != "unconnected_items" && f.rule != "courtyards_overlap")
        );
    }

    #[test]
    fn the_same_subject_twice_is_named_apart() {
        let mut ids = Ids(Vec::new());
        assert_eq!(ids.id("a".into()), "a");
        assert_eq!(ids.id("b".into()), "b");
        assert_eq!(ids.id("a".into()), "a#2");
    }

    #[test]
    fn a_finding_has_a_box_big_enough_to_zoom_to() {
        let b = finding_box(Rect::new(Pt::new(10.0, 10.0), Pt::new(10.0, 10.0)));
        assert_eq!((b.width, b.height), (LEAST, LEAST));
        assert_eq!((b.x + b.width / 2.0, b.y + b.height / 2.0), (10.0, 10.0));
        let big = finding_box(Rect::new(Pt::new(0.0, 0.0), Pt::new(20.0, 5.0)));
        assert_eq!((big.width, big.height), (20.0, 5.0));
    }
}
