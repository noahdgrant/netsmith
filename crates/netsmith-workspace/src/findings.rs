//! What is wrong with a placed board, and where (ADR 0030, E3): netsmith's own checks of the
//! placement and of the connections the router left, and KiCad's design rules check, each
//! finding with a point and a box on the sheet so the page can zoom to it.

use std::{
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

use netsmith_editor::{
    Say,
    api::{FindingSource, KicadState, Location, PcbBox, PcbFinding, Severity},
};
use netsmith_fab::{KicadCli, KicadCliExe};
use netsmith_geom::{
    Artwork, Constraints, DrcFinding, DrcRule, DrcSeverity, constraint_outcome_findings, drc_check,
};
use netsmith_pcb::{Board, PlacementProblem, Pt, Ratsnest, Rect, RoutesNote, kicad_pcb, kicad_pro};

/// What KiCad's check found on a board, or why it could not be run.
#[derive(Debug, Clone, PartialEq)]
pub struct KicadResult {
    pub state: KicadState,
    pub note: String,
    pub findings: Vec<PcbFinding>,
}

/// The first line of a source file that `text` names as `file.nsm:12` (a requirement's
/// source is written so: `led.physical.nsm:12`, several joined by commas, and a sentence may
/// quote one in brackets), as a point at the start of that line. The file is as the checker
/// names it, which is how the page names it.
pub fn stated_at(text: &str) -> Option<Location> {
    text.split(|c: char| !(c.is_alphanumeric() || "._/-:".contains(c)))
        .find_map(|word| {
            let (file, line) = word.rsplit_once(':')?;
            let line: u32 = line.parse().ok().filter(|l| *l > 0)?;
            file.ends_with(".nsm").then(|| Location {
                file: file.to_string(),
                line,
                column: 1,
                end_line: line,
                end_column: 1,
            })
        })
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
        keepout: None,
        at: None,
        case_solid: None,
        point3d: None,
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

/// The fit of the board in its case models (ADR 0109, the scene's `case`), each finding with the
/// part's courtyard (or the point) on the sheet, the solid the 3D view lights and the point it
/// flies to. A collision is an error; the rest are warnings, as the 3D view has them.
pub fn case_findings(board: &Board, case: &netsmith_3d::CaseScene) -> Vec<PcbFinding> {
    use netsmith_3d::{CaseSubject, CaseWeight};
    let mut ids = Ids(Vec::new());
    let height = board.outline.height();
    case.findings
        .iter()
        .map(|f| {
            let refdes = match &f.subject {
                CaseSubject::Part { refdes, .. } | CaseSubject::Pins { refdes, .. } => {
                    Some(refdes.clone())
                }
                _ => None,
            };
            // The scene's frame is y up from the outline's bottom-left corner; the sheet's is
            // y down from its top-left.
            let at = Pt::new(
                board.outline.min.x + f.at[0],
                board.outline.min.y + height - f.at[1],
            );
            let area = refdes
                .as_deref()
                .and_then(|r| board.part(r))
                .map(|p| p.courtyard())
                .unwrap_or_else(|| Rect::new(at, at));
            let subject = refdes
                .clone()
                .map(|r| match f.subject {
                    CaseSubject::Pins { .. } => format!("{r}:pins"),
                    _ => r,
                })
                .or_else(|| (!f.solid.is_empty()).then(|| f.solid.clone()))
                .unwrap_or_else(|| f.case.clone());
            let id = ids.id(format!("case:{}:{subject}", f.kind.word()));
            let mut finding = make(
                id,
                FindingSource::Placement,
                if f.weight() == CaseWeight::Error {
                    Severity::Error
                } else {
                    Severity::Warning
                },
                f.kind.word(),
                f.to_string(),
                area,
                refdes.into_iter().collect(),
            );
            finding.case_solid = if !f.solid.is_empty() {
                Some(format!("case:{}:{}", f.case, f.solid))
            } else if f.subject == CaseSubject::Model {
                Some(format!("case:{}", f.case))
            } else {
                None
            };
            if f.subject != CaseSubject::Model {
                finding.point3d = Some(f.at);
                finding.x = at.x;
                finding.y = at.y;
            }
            finding
        })
        .collect()
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
    // A part under a height limit that states no height: a warning that names it, never a
    // pass (ADR 0045, "Height limits").
    let unchecked: Vec<PcbFinding> = board
        .heights_unchecked()
        .into_iter()
        .map(|u| {
            let area = courtyard(&u.refdes)
                .zip(keepout(&u.keepout))
                .map(|(a, b)| a.union(&b))
                .unwrap_or(board.outline);
            let id = ids.id(format!("placement:height_unchecked:{}", u.refdes));
            let mut finding = make(
                id,
                FindingSource::Placement,
                Severity::Warning,
                "height_unchecked",
                u.to_string(),
                area,
                vec![u.refdes.clone()],
            );
            finding.keepout = Some(u.keepout.clone());
            finding
        })
        .collect();
    board
        .check_placement()
        .into_iter()
        .map(|problem| {
            let message = problem.to_string();
            let (rule, subjects, area): (&str, Vec<&str>, Option<Rect>) = match &problem {
                PlacementProblem::Overlap { a, b, .. } => {
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
                PlacementProblem::OverPins { holder, cover, .. } => {
                    let (rt, rc) = (courtyard(holder), courtyard(cover));
                    let shared = rt.zip(rc).map(|(x, y)| {
                        Rect::new(
                            Pt::new(x.min.x.max(y.min.x), x.min.y.max(y.min.y)),
                            Pt::new(x.max.x.min(y.max.x), x.max.y.min(y.max.y)),
                        )
                    });
                    ("over_pins", vec![holder, cover], shared)
                }
                PlacementProblem::OutsideOutline { refdes, .. } => {
                    ("outside_outline", vec![refdes], courtyard(refdes))
                }
                PlacementProblem::PadNearEdge { refdes, .. } => {
                    ("pad_near_edge", vec![refdes], courtyard(refdes))
                }
                PlacementProblem::InCutout {
                    refdes, cutout: c, ..
                } => (
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
                PlacementProblem::InKeepout {
                    refdes, keepout: k, ..
                } => (
                    "in_keepout",
                    vec![refdes],
                    courtyard(refdes).zip(keepout(k)).map(|(a, b)| a.union(&b)),
                ),
                PlacementProblem::PadInKeepout {
                    refdes, keepout: k, ..
                } => (
                    "pad_in_keepout",
                    vec![refdes],
                    courtyard(refdes).zip(keepout(k)).map(|(a, b)| a.union(&b)),
                ),
                PlacementProblem::TooTall {
                    refdes, keepout: k, ..
                } => (
                    "too_tall",
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
            // The lines of the physical file that the problem quotes: where the pins' protrusion
            // is stated, or the requirement that allows an overhang.
            let at = match &problem {
                PlacementProblem::OverPins { source, .. } => stated_at(source),
                PlacementProblem::OutsideOutline { by, .. } => stated_at(by),
                _ => None,
            };
            let mut finding = make(
                id,
                FindingSource::Placement,
                Severity::Error,
                rule,
                message,
                area,
                parts,
            );
            finding.at = at;
            // The keepout a part is in, or too tall for: the page lights it when this is picked.
            finding.keepout = match &problem {
                PlacementProblem::InKeepout { keepout, .. }
                | PlacementProblem::PadInKeepout { keepout, .. }
                | PlacementProblem::TooTall { keepout, .. } => Some(keepout.clone()),
                _ => None,
            };
            finding
        })
        .chain(unchecked)
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

/// What of the routes file `file` was not used as written (epic #599), as warnings: the warning
/// `fab` prints, at the pads of the net (or the whole board, for a net it does not have).
pub fn routes_findings(board: &Board, file: &str, notes: &[RoutesNote]) -> Vec<PcbFinding> {
    let mut ids = Ids(Vec::new());
    notes
        .iter()
        .map(|note| {
            let (net, rule) = match note {
                RoutesNote::Moved { net, .. } => (net, "routes_pads_moved"),
                RoutesNote::NotANet { net, .. } => (net, "routes_no_net"),
            };
            let pads: Vec<(String, Pt)> = board
                .nets
                .iter()
                .filter(|n| &n.name == net)
                .flat_map(|n| &n.pads)
                .filter_map(|pad| {
                    let part = board.parts.iter().find(|p| p.refdes == pad.refdes)?;
                    let at = part
                        .footprint
                        .pads_numbered(&pad.number)
                        .next()
                        .map(|p| part.pad_centre(p))?;
                    Some((part.refdes.clone(), at))
                })
                .collect();
            let area = Rect::around(pads.iter().map(|(_, at)| *at)).unwrap_or(board.outline);
            let mut parts: Vec<String> = pads.into_iter().map(|(r, _)| r).collect();
            parts.dedup();
            make(
                ids.id(format!("routes:{rule}:{net}")),
                FindingSource::Routes,
                Severity::Warning,
                rule,
                format!("{file}: {note}"),
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
    let extras = netsmith_fabricate::fab::drc_extras(board);
    let report = match drc_check(&kicad_pcb(board), Some(&kicad_pro(board)), &extras) {
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
        .filter(|f| {
            !matches!(
                f.rule,
                DrcRule::UnconnectedItems | DrcRule::CourtyardsOverlap
            )
        })
        .map(|f| from_drc(f, board, &mut ids))
        .collect()
}

/// A finding of netsmith's design rules check as the page shows it.
fn from_drc(f: &DrcFinding, board: &Board, ids: &mut Ids) -> PcbFinding {
    let severity = match f.severity {
        DrcSeverity::Error => Severity::Error,
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

/// What the requirements prove before any placing (ADR 0045 section 14), as
/// `constraint` findings: the lines that cannot both hold, a courtyard past an edge nobody
/// stated and a part bigger than the outline are errors, the suspicious are warnings. The board
/// is not placed, so a finding has no place of its own; it is at the sheet's origin, with the
/// parts it is about named.
pub fn precheck_findings(pre: &netsmith_pcb::Precheck) -> Vec<PcbFinding> {
    let at = Rect::new(netsmith_pcb::ORIGIN, netsmith_pcb::ORIGIN);
    let mut ids = Ids(Vec::new());
    let mut found = Vec::new();
    let mut add = |severity, base: String, message: String, parts: Vec<String>| {
        let id = ids.id(format!("constraint:precheck:{base}"));
        // The precheck's findings are sentences with no structure behind them: the line is
        // read out of the sentence, which quotes it.
        let stated = stated_at(&message);
        let mut finding = make(
            id,
            FindingSource::Placement,
            severity,
            "constraint",
            message,
            at,
            parts,
        );
        finding.at = stated;
        found.push(finding);
    };
    for (n, conflict) in pre.conflicts.iter().enumerate() {
        add(
            Severity::Error,
            format!("conflict:{n}"),
            format!("these lines cannot both hold: {conflict}"),
            Vec::new(),
        );
    }
    for fault in &pre.faults {
        add(
            Severity::Error,
            format!("part:{}", fault.refdes),
            fault.text.clone(),
            vec![fault.refdes.clone()],
        );
    }
    for (n, warning) in pre.warnings.iter().enumerate() {
        add(
            Severity::Warning,
            format!("warning:{n}"),
            warning.clone(),
            Vec::new(),
        );
    }
    found
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
        .zip(constraint_outcome_findings(&outcomes, constraints.severity))
        .map(|(outcome, f)| {
            let mut finding = from_drc(&f, board, &mut ids);
            // The message already names the part and the numbers; the generic listing of
            // the items after it (`: Footprint J2`) says the same again.
            finding.message = f.message.clone();
            // Named by the requirement, so that it keeps its name as the part is dragged.
            finding.id = ids.id(format!("constraint:{}", outcome.id));
            finding.source = FindingSource::Placement;
            finding.at = stated_at(&outcome.source);
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
        let note = match netsmith_fabricate::fab::kicad_switched_off(
            &netsmith_kicad::Search::machine(),
        ) {
            Some(off) => format!("KiCad is switched off: {off} (only netsmith's own checks ran)"),
            None => {
                "KiCad not found: set KICAD_CLI or install KiCad (only netsmith's own checks ran)"
                    .to_string()
            }
        };
        return unavailable(note);
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
        let libraries = netsmith_pipeline::Libraries::sample();
        let built = netsmith_pipeline::build_with(
            &path,
            &std::fs::read_to_string(&path).unwrap(),
            &netsmith_check::Loader::std(),
            &Catalogs::sample(),
            &libraries,
        )
        .unwrap();
        netsmith_placer::board_from_design(
            &built[0].design,
            &libraries.mapping,
            &libraries.footprints,
        )
        .unwrap()
    }

    #[test]
    fn a_requirement_on_where_a_part_goes_is_a_finding_with_its_numbers_and_the_requirements_name()
    {
        use netsmith_geom::{Constraint, ConstraintMeasure as Measure};
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
            relations: Vec::new(),
            severity,
        };
        let found = constraint_findings(&board, &constraints(DrcSeverity::Error));
        assert_eq!(found.len(), 1, "{found:#?}");
        let f = &found[0];
        assert_eq!(f.id, "constraint:supply.place.left");
        assert_eq!(
            (f.rule.as_str(), f.severity),
            ("constraint", Severity::Error)
        );
        assert_eq!(f.parts, std::slice::from_ref(&part));
        // The line that states it is data, not something to read out of the message.
        assert_eq!(
            f.at,
            Some(Location {
                file: "led.nsm".into(),
                line: 9,
                column: 1,
                end_line: 9,
                end_column: 1
            })
        );
        assert!(
            f.message
                .contains("constraint supply.place.left (led.nsm:9) is violated")
                && f.message.contains("it allows 500 to 501 mm")
                && f.message.ends_with("mm over)"),
            "{}",
            f.message
        );
        // The project can make it a warning, and a met requirement is no finding.
        let warned = constraint_findings(&board, &constraints(DrcSeverity::Warning));
        assert_eq!(warned[0].severity, Severity::Warning);
        let mut met = constraints(DrcSeverity::Error);
        met.list[0].lo = f64::NEG_INFINITY;
        met.list[0].hi = f64::INFINITY;
        assert_eq!(constraint_findings(&board, &met), []);
    }

    #[test]
    fn an_unmet_relation_between_two_parts_is_a_finding_that_names_both() {
        use netsmith_geom::{
            ConstraintMeasure as Measure, ConstraintRelation as Relation,
            ConstraintRelationEnd as RelationEnd,
        };
        let board = placed("led");
        let end = |i: usize| RelationEnd {
            part: board.parts[i].refdes.clone(),
            path: board.parts[i].path.clone(),
            measure: Measure::X,
        };
        let constraints = Constraints {
            list: Vec::new(),
            sides: Vec::new(),
            relations: vec![Relation {
                id: "r1.place.x - d1.place.x".into(),
                source: "led.physical.nsm:12".into(),
                a: end(0),
                b: end(1),
                lo: 500.0,
                hi: 501.0,
            }],
            severity: DrcSeverity::Error,
        };
        let found = constraint_findings(&board, &constraints);
        assert_eq!(found.len(), 1, "{found:#?}");
        let f = &found[0];
        assert_eq!(f.id, "constraint:r1.place.x - d1.place.x");
        assert_eq!(
            (f.rule.as_str(), f.severity),
            ("constraint", Severity::Error)
        );
        assert_eq!(
            f.at.as_ref().map(|at| (at.file.as_str(), at.line)),
            Some(("led.physical.nsm", 12))
        );
        assert_eq!(
            f.parts,
            [board.parts[1].refdes.clone(), board.parts[0].refdes.clone()]
        );
        assert!(
            f.message
                .contains("constraint r1.place.x - d1.place.x (led.physical.nsm:12) is violated")
                && f.message.contains("it allows 500 to 501 mm")
                && f.message.ends_with("mm over)"),
            "{}",
            f.message
        );
    }

    #[test]
    fn the_line_a_requirement_is_stated_on_is_found_in_its_source_or_a_sentence_that_quotes_it() {
        let at = |text: &str| stated_at(text).map(|l| (l.file, l.line));
        assert_eq!(at("led.nsm:9"), Some(("led.nsm".into(), 9)));
        // Several lines: the first.
        assert_eq!(
            at("src/a.physical.nsm:3, src/b.nsm:40"),
            Some(("src/a.physical.nsm".into(), 3))
        );
        // A sentence that quotes one, in brackets and in backticks.
        assert_eq!(
            at(
                "the design says it may hang over only the bottom edge (`j1.place.right (a.nsm:3)` allows at least 36 mm)"
            ),
            Some(("a.nsm".into(), 3))
        );
        assert_eq!(
            at("conflict: src/w.physical.nsm:16 `.rf.place.right >= 59mm`"),
            Some(("src/w.physical.nsm".into(), 16))
        );
        // Not a source line: no file of ours, no line, a line 0.
        for text in [
            "",
            "no line here",
            "led.kicad_pcb:9",
            "led.nsm:",
            "led.nsm:0",
            "12:30",
        ] {
            assert_eq!(at(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_clean_placement_has_no_finding_and_an_overlap_is_found_where_the_courtyards_meet() {
        let mut board = placed("led");
        assert_eq!(placement_findings(&board), []);
        let (a, b) = (board.parts[0].refdes.clone(), board.parts[1].refdes.clone());
        let named =
            |i: usize| netsmith_pcb::board_member(&board.parts[i].path, &board.parts[i].refdes);
        let (name_a, name_b) = (named(0), named(1));
        board.parts[1].at = board.parts[0].at;
        let found = placement_findings(&board);
        let overlap = found
            .iter()
            .find(|f| f.rule == "overlap")
            .expect("an overlap");
        assert_eq!(overlap.id, format!("placement:overlap:{a}+{b}"));
        assert_eq!(overlap.parts, [a.clone(), b.clone()]);
        assert!(
            overlap.message.contains(&format!(
                "the courtyards of {name_a} and {name_b} overlap by "
            )),
            "{}",
            overlap.message
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
        use netsmith_pcb::{CopperLayer, Track};
        use netsmith_router::route;
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
            mid: None,
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
