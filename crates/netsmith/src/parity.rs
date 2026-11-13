//! The parity checker of the RP2350A Minimal port (`examples/rp2350_minimal/`): it compares a
//! `.kicad_pcb` written by `netsmith fab` with Raspberry Pi's board, as the reference table
//! (`reference/placements.toml`) states it, and says in which ways the two are interchangeable.
//!
//! What must match (`must_match = true` in the table): the outline and the mounting holes, and
//! the connectors, buttons and header pins: each one's place, rotation, layer, and every pad's
//! number, centre, size, shape, angle, drill, copper layers and net. Everything else (the MCU,
//! flash, crystal, regulator, decoupling, the routing) is measured and reported with the verdict
//! `info`, never `fail`. What each verdict compared is printed at the head of the report ([`WHAT_IS_COMPARED`]).
//!
//! **Pads** are judged one by one: the pads of one number (a connector's shell has a surface pad
//! and a plated slot of the same number) are paired by the nearest centre in the footprint's own
//! frame, and each pair is compared in that frame: a pad turned by 90 degrees has its width and
//! height swapped, so the same copper written two ways is the same, and a turned pad is not an
//! equal one. A part of nothing but non-plated pads is a mounting hole (judged as a hole), and so
//! is a plated `MountingHole*` footprint of KiCad's library, whose drilled pad is the hole; any
//! other part with a non-plated pad (a peg) is still a part, and the peg is a pad of it.
//!
//! **Holes** are paired by the nearest centre and judged by place, drill, whether the hole is
//! plated, the pad's size and its net (by name; a made-up name is no net). A ring of vias around
//! a plated hole is not compared.
//!
//! **The origin.** `fab` writes a board with an origin of its own, so no figure is compared as
//! the file states it. Each board is read from the **upper-left corner of its outline's bounding
//! box**: the box's smallest x and smallest y, in KiCad's coordinates, where y grows downward (so
//! that corner is the upper left as the board editor shows it). It is a corner of the box in both
//! files and needs no flip of an axis. A board moved as a whole passes; one whose parts moved
//! against its edge does not. The corner of the two boxes, as the files state them, is reported
//! as an `info` line.
//!
//! **Where to put a part.** For each part that must match, the report says where its footprint's
//! origin is on the board, how far that is from its body's centre (the box `.place.x` and
//! `.place.y` measure, as the fab check finds it), and the two lines that put the origin on the
//! reference's ([`OriginLine`], #457). A pad difference says what the board has and what the
//! reference has (`type (the board's smd, the reference's connect)`).
//!
//! **Parts** are matched by reference through a map ([`DesignatorMap`]): the original's reference
//! to the reference of the part in the board `fab` wrote, made from the port's `parts.toml` and
//! the member path that `fab` writes on each footprint ([`DesignatorMap::from_parts`], #456). A
//! part of the table with no entry is `unmapped`: a failure when the table says it must match.
//!
//! **Nets** are compared pad by pad, by name (a leading `/` and the case are ignored) where both
//! names are plain, and through the connectivity otherwise: the pads of the mapped parts that
//! share the net of a pad must be the same in both boards. A pad whose names differ, or of which
//! one name is made up (`Net-(...)`, `unconnected-...` or none), passes only when the
//! connectivity vouches for it (another mapped pad is on its net, in both boards), and says so
//! in a warning; with no other mapped pad on the net nothing vouches and it fails. Map U1 and
//! the other parts, even those that need not match, so that the connectivity has pads to see.
//! A pad with no net is not a made-up name: where the reference's pad is on a net with other
//! pads (mapped or not) it does not pass against a pad that is on no net or alone on its own
//! (and the other way round); two pads with no other pad on their nets pass.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::reference_table::{Board, Hole, Outline, Pad, Part, natural_key, read_board};

/// The tolerances, in millimetres and degrees. The owner's list (2026-11-10): outline and
/// mounting holes 0.05, positions of parts 0.1, rotation exact, pad numbers equal, nets equal.
/// Where it was silent: the corner radius and the hole's drill follow the outline and holes
/// (0.05); a pad's centre in its footprint follows the positions (0.1); a pad's size and drill
/// 0.05; the rotation's 0.001 degree is only the rounding of the file.
#[derive(Debug, Clone, Copy)]
pub struct Tolerances {
    /// The outline's width and height, and its corner radius.
    pub outline: f64,
    /// A mounting hole's place.
    pub hole: f64,
    /// A mounting hole's drill.
    pub hole_drill: f64,
    /// A part's place.
    pub position: f64,
    /// A pad's centre, in its footprint's own frame.
    pub pad_position: f64,
    /// A pad's width, height and drill.
    pub pad_size: f64,
    /// A part's rotation, in degrees (modulo 360).
    pub rotation: f64,
}

impl Default for Tolerances {
    fn default() -> Self {
        Tolerances {
            outline: 0.05,
            hole: 0.05,
            hole_drill: 0.05,
            position: 0.1,
            pad_position: 0.1,
            pad_size: 0.05,
            rotation: 0.001,
        }
    }
}

/// Slack for the rounding of figures a file writes with a few decimals.
const EPS: f64 = 1e-6;

/// What a verdict covers, printed at the head of the report (one line each).
pub const WHAT_IS_COMPARED: [&str; 5] = [
    "outline: the bounding box, the corner radius, the number of Edge.Cuts lines and arcs and \
     each arc's radius",
    "hole: place, drill, plated or not, the pad's size, the pad's net",
    "part: position, rotation, layer",
    "pad: number, centre, size and angle in the footprint's frame, shape, the extent of a \
     custom pad's polygon, drill (both sides of a slot), type, copper layers",
    "net: each pad's own net, by its name or, where a name is made up, by the pads on it",
];

/// What no verdict covers: the table does not hold it.
pub const NOT_COMPARED: &str = "not compared: a rounded rectangle's corner ratio, a custom pad's \
     anchor shape and the shape of its polygon inside the extent, a pad's mask and paste layers, \
     the silkscreen, the courtyard, text and the routing";

/// Raspberry Pi's references mapped to those of the board `fab` wrote.
#[derive(Debug, Clone, Default)]
pub struct DesignatorMap(BTreeMap<String, String>);

impl DesignatorMap {
    /// A map as TOML: one line per part, `J1 = "J1"` (the original's reference, then the
    /// reference of the part in the `.kicad_pcb`). The tests write one by hand; a port's map is
    /// [`DesignatorMap::from_parts`].
    pub fn parse(text: &str) -> Result<DesignatorMap, String> {
        let table: toml::Table = text.parse().map_err(|e| format!("designators: {e}"))?;
        let mut map = BTreeMap::new();
        for (key, value) in table {
            let Some(to) = value.as_str() else {
                return Err(format!("designators: {key} must be a text (a reference)"));
            };
            map.insert(key, to.to_string());
        }
        Ok(DesignatorMap(map))
    }

    /// The original's parts paired with the board `fab` wrote through the port's `parts.toml`
    /// (#456): each footprint of the board holds the member path of its part (`netsmith_path`),
    /// and `parts` gives the original's designator of that path. A footprint whose path
    /// `parts` does not name is left out (a part the original does not have); a part of `parts`
    /// that the board does not have is unmapped, which the check reports. A board with no
    /// `netsmith_path` on any footprint was written before the property existed, and is refused.
    pub fn from_parts(parts: &crate::port::PortParts, pcb: &str) -> Result<DesignatorMap, String> {
        let root = netsmith_kicad::sexp::Sexp::parse(pcb).map_err(|e| format!("the board: {e}"))?;
        let mut map = BTreeMap::new();
        let mut with_path = 0;
        for footprint in root.find_all("footprint") {
            let property = |name: &str| {
                footprint
                    .find_all("property")
                    .find(|p| p.args().first().and_then(|a| a.text()) == Some(name))
                    .and_then(|p| p.args().get(1).and_then(|a| a.text()).map(str::to_string))
            };
            let (Some(designator), Some(path)) =
                (property("Reference"), property(netsmith_pcb::PATH_PROPERTY))
            else {
                continue;
            };
            with_path += 1;
            if let Some(original) = parts.reference_of(&path) {
                map.insert(original.to_string(), designator);
            }
        }
        if with_path == 0 {
            return Err(format!(
                "the board's footprints have no `{}`: it was written by a netsmith that did not write the member path, so make it again with `netsmith fab`",
                netsmith_pcb::PATH_PROPERTY
            ));
        }
        Ok(DesignatorMap(map))
    }

    /// Every part of `board` mapped to the reference it has.
    pub fn identity(board: &Board) -> DesignatorMap {
        DesignatorMap(
            board
                .parts
                .iter()
                .map(|p| (p.reference.clone(), p.reference.clone()))
                .collect(),
        )
    }

    pub fn get(&self, reference: &str) -> Option<&str> {
        self.0.get(reference).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    Fail,
    /// Outside the tolerance, on something that need not match.
    Info,
}

impl Verdict {
    fn word(self) -> &'static str {
        match self {
            Verdict::Ok => "ok",
            Verdict::Fail => "FAIL",
            Verdict::Info => "info",
        }
    }
}

/// One measured thing: an offset is the board's figure minus the reference's.
#[derive(Debug, Clone)]
pub struct Item {
    /// What it says, e.g. `part J2 pad 5` or `hole MH1`; items are unique by `id` and `field`.
    pub id: String,
    /// The measured quantity: `position`, `rotation`, `layer`, `pad`, `net`, `size`, ...
    pub field: &'static str,
    /// Raspberry Pi's reference of the part (`outline` for the outline, `hole MH1`'s `MH1`).
    pub subject: String,
    pub must_match: bool,
    pub verdict: Verdict,
    pub dx: Option<f64>,
    pub dy: Option<f64>,
    /// Degrees, in (-180, 180].
    pub drotation: Option<f64>,
    /// Width and height differences.
    pub dsize: Option<(f64, f64)>,
    /// A single length difference: a radius or a drill.
    pub dvalue: Option<f64>,
    /// What the measured figures are held to (mm, or degrees for a rotation; 0 for an equality).
    pub tolerance: f64,
    pub note: String,
}

impl Item {
    fn new(id: String, field: &'static str, subject: &str, must_match: bool) -> Item {
        Item {
            id,
            field,
            subject: subject.to_string(),
            must_match,
            verdict: Verdict::Ok,
            dx: None,
            dy: None,
            drotation: None,
            dsize: None,
            dvalue: None,
            tolerance: 0.0,
            note: String::new(),
        }
    }

    /// Set the verdict of a check: a must-match item that is wrong fails, another one is `info`.
    fn judge(mut self, good: bool) -> Item {
        self.verdict = match (good, self.must_match) {
            (true, _) => Verdict::Ok,
            (false, true) => Verdict::Fail,
            (false, false) => Verdict::Info,
        };
        self
    }

    fn at(mut self, dx: f64, dy: f64) -> Item {
        self.dx = Some(dx);
        self.dy = Some(dy);
        self
    }

    fn within(mut self, tolerance: f64) -> Item {
        self.tolerance = tolerance;
        self
    }

    fn noting(mut self, note: impl Into<String>) -> Item {
        self.note = note.into();
        self
    }
}

#[derive(Debug, Clone)]
pub struct ParityReport {
    pub items: Vec<Item>,
    /// Name differences that the connectivity excused, and the like: never a failure.
    pub warnings: Vec<String>,
    pub reference: Board,
    pub actual: Board,
    map: DesignatorMap,
    /// For each part that must match, where its footprint origin is on the board and the
    /// `.place.x` and `.place.y` that put it where the reference's is (#457). Only
    /// [`check_text`], which has the board file, finds them.
    pub origins: Vec<OriginLine>,
}

/// A part that must match: where its footprint origin is on the board, how far that is from the
/// centre of its body (what `.place.x` and `.place.y` measure, as `fab` measures it), and the two
/// lines that put the origin where the reference's is (#457).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OriginLine {
    /// The original's reference of the part.
    pub reference: String,
    /// The part on the board, from the outline's corner.
    pub board: netsmith_pcb::PartOrigin,
    /// The reference's origin, from its outline's corner.
    pub reference_origin: [f64; 2],
    /// The `.place.x` and `.place.y` that put the origin at `reference_origin`: `None` when the
    /// part is on another face or turned otherwise than the reference's, which changes the
    /// offset.
    pub place: Option<[f64; 2]>,
    /// The reference's face and turn, for the line that says to match them first.
    pub reference_side: &'static str,
    pub reference_rotation: f64,
}

impl OriginLine {
    /// `part J2 (`usb`, J4 on the board) top, turned 90: origin (97.405, 12), body centre
    /// (96.9, 12), dx 0.505 dy 0; the reference's origin is (97.405, 12): write ...`.
    pub fn line(&self) -> String {
        use netsmith_pcb::origin::mm;
        let b = &self.board;
        let path = if b.path.is_empty() { "<path>" } else { &b.path };
        let mut s = format!(
            "part {} (`{}`, {} on the board) {}, turned {}: origin ({}, {}), body centre ({}, {}), dx {} dy {}; the reference's origin is ({}, {})",
            self.reference,
            path,
            b.refdes,
            b.side,
            mm(b.rotation),
            mm(b.origin[0]),
            mm(b.origin[1]),
            mm(b.body_centre[0]),
            mm(b.body_centre[1]),
            mm(b.offset[0]),
            mm(b.offset[1]),
            mm(self.reference_origin[0]),
            mm(self.reference_origin[1]),
        );
        match self.place {
            Some([x, y]) => {
                let _ = write!(
                    s,
                    ": `.{path}.place.x &= {}mm` and `.{path}.place.y &= {}mm` put it there",
                    mm(x),
                    mm(y)
                );
            }
            None => {
                let _ = write!(
                    s,
                    ", on the {} turned {}: put the part on that face and turned so, and run fab and parity again for its offset there",
                    self.reference_side,
                    mm(self.reference_rotation)
                );
            }
        }
        s
    }
}

/// The [`OriginLine`] of each part of `reference` that must match and that `map` pairs with a
/// part of the board file `pcb`. Empty when netsmith's reader of board files cannot read `pcb`.
fn origin_lines(
    reference: &Board,
    actual: &Board,
    pcb: &str,
    map: &DesignatorMap,
) -> Vec<OriginLine> {
    let Ok(art) = netsmith_fab::artwork::Artwork::parse(pcb) else {
        return Vec::new();
    };
    let paths: BTreeMap<String, String> = netsmith_kicad::sexp::Sexp::parse(pcb)
        .map(|root| {
            root.find_all("footprint")
                .filter_map(|f| {
                    let property = |name: &str| {
                        f.find_all("property")
                            .find(|p| p.args().first().and_then(|a| a.text()) == Some(name))
                            .and_then(|p| {
                                p.args().get(1).and_then(|a| a.text()).map(str::to_string)
                            })
                    };
                    Some((
                        property("Reference")?,
                        property(netsmith_pcb::PATH_PROPERTY)?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let (rc, ac) = (corner(reference), corner(actual));
    let tidy = |v: f64| (v * 1e4).round() / 1e4;
    let side = |layer: &str| {
        if layer.starts_with('B') {
            "bottom"
        } else {
            "top"
        }
    };
    let mut lines = Vec::new();
    for part in reference.parts.iter().filter(|p| p.must_match) {
        let Some(found) = map
            .get(&part.reference)
            .and_then(|to| actual.parts.iter().find(|p| p.reference == to))
        else {
            continue;
        };
        let back = found.layer.starts_with('B');
        let Some((body, _)) = netsmith_fab::drc::constraints::body_of(&art, &found.reference, back)
        else {
            continue;
        };
        let origin = [tidy(found.x - ac.0), tidy(found.y - ac.1)];
        let centre = [
            tidy((body.min.x + body.max.x) / 2.0 - ac.0),
            tidy((body.min.y + body.max.y) / 2.0 - ac.1),
        ];
        let offset = [tidy(origin[0] - centre[0]), tidy(origin[1] - centre[1])];
        let reference_origin = [tidy(part.x - rc.0), tidy(part.y - rc.1)];
        let same = found.layer == part.layer
            && angle_diff(found.rotation, part.rotation).abs() <= Tolerances::default().rotation;
        lines.push(OriginLine {
            reference: part.reference.clone(),
            board: netsmith_pcb::PartOrigin {
                refdes: found.reference.clone(),
                path: paths.get(&found.reference).cloned().unwrap_or_default(),
                side: side(&found.layer),
                rotation: tidy(place_rotation(found.rotation, back)),
                origin,
                body_centre: centre,
                offset,
            },
            reference_origin,
            place: same.then(|| {
                [
                    tidy(reference_origin[0] - offset[0]),
                    tidy(reference_origin[1] - offset[1]),
                ]
            }),
            reference_side: side(&part.layer),
            reference_rotation: tidy(place_rotation(part.rotation, part.layer.starts_with('B'))),
        });
    }
    lines
}

/// A footprint's orientation in a board file as `.place.rotation` says it, 0 to 360: a half
/// turn less on the back, where KiCad's flip mirrors in y and netsmith's in x (#553).
fn place_rotation(file: f64, back: bool) -> f64 {
    let half = if back { 180.0 } else { 0.0 };
    (file - half).rem_euclid(360.0)
}

impl ParityReport {
    /// `(ok, failed)` of the must-match items.
    pub fn must_match_counts(&self) -> (usize, usize) {
        let count = |v| {
            self.items
                .iter()
                .filter(|i| i.must_match && i.verdict == v)
                .count()
        };
        (count(Verdict::Ok), count(Verdict::Fail))
    }

    /// No must-match item failed.
    pub fn passed(&self) -> bool {
        self.must_match_counts().1 == 0
    }

    pub fn failures(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(|i| i.verdict == Verdict::Fail)
    }

    /// The report as one JSON document, for `netsmith parity --format json` (#455): whether it
    /// passed, the counts of the must-match items, each item with its figures, and the warnings.
    pub fn json(&self) -> String {
        let items: Vec<serde_json::Value> = self
            .items
            .iter()
            .map(|i| {
                serde_json::json!({
                    "id": i.id,
                    "field": i.field,
                    "subject": i.subject,
                    "must_match": i.must_match,
                    "verdict": match i.verdict {
                        Verdict::Ok => "ok",
                        Verdict::Fail => "fail",
                        Verdict::Info => "info",
                    },
                    "dx": i.dx,
                    "dy": i.dy,
                    "drotation": i.drotation,
                    "dsize": i.dsize.map(|(w, h)| [w, h]),
                    "dvalue": i.dvalue,
                    "tolerance": i.tolerance,
                    "note": i.note,
                })
            })
            .collect();
        let (ok, failed) = self.must_match_counts();
        let doc = serde_json::json!({
            "command": "parity",
            "passed": self.passed(),
            "must_match": { "ok": ok, "failed": failed },
            "items": items,
            "origins": self.origins,
            "warnings": self.warnings,
        });
        let mut text = serde_json::to_string_pretty(&doc).expect("plain data");
        text.push('\n');
        text
    }

    /// One line per item (outline, holes, parts by reference, then what the board has extra),
    /// the warnings, and the summary line `must-match: N ok, M failed`.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let (rx, ry) = corner(&self.reference);
        let (ax, ay) = corner(&self.actual);
        let _ = writeln!(
            out,
            "parity: every figure is read from the upper-left corner of the outline's bounding \
             box (smallest x and y of Edge.Cuts; y grows downward), because fab gives a board an \
             origin of its own: reference ({}, {}), board ({}, {})",
            num(rx),
            num(ry),
            num(ax),
            num(ay)
        );
        for line in WHAT_IS_COMPARED {
            let _ = writeln!(out, "compared  {line}");
        }
        let _ = writeln!(out, "{NOT_COMPARED}");
        let width = self.items.iter().map(|i| i.id.len()).max().unwrap_or(0);
        for item in &self.items {
            let _ = write!(
                out,
                "{:<4}  {}  {:<width$}",
                item.verdict.word(),
                if item.must_match { "must" } else { "    " },
                item.id,
            );
            let mut figures = Vec::new();
            if let (Some(dx), Some(dy)) = (item.dx, item.dy) {
                figures.push(format!("dx={} dy={}", signed(dx), signed(dy)));
            }
            if let Some(r) = item.drotation {
                figures.push(format!("drot={}", signed(r)));
            }
            if let Some((w, h)) = item.dsize {
                figures.push(format!("dsize={} x {}", signed(w), signed(h)));
            }
            if let Some(v) = item.dvalue {
                figures.push(format!("d={}", signed(v)));
            }
            if !figures.is_empty() {
                let _ = write!(out, "  {}", figures.join(" "));
            }
            if item.tolerance > 0.0 {
                let _ = write!(out, "  (tol {})", num(item.tolerance));
            }
            if !item.note.is_empty() {
                let _ = write!(out, "  {}", item.note);
            }
            out.push('\n');
        }
        for o in &self.origins {
            let _ = writeln!(out, "origin  {}", o.line());
        }
        for w in &self.warnings {
            let _ = writeln!(out, "warning: {w}");
        }
        let (ok, failed) = self.must_match_counts();
        let info = self
            .items
            .iter()
            .filter(|i| i.verdict == Verdict::Info && i.field != "origin")
            .count();
        let _ = writeln!(
            out,
            "not required: {info} items outside their tolerance, on what need not match"
        );
        let _ = writeln!(out, "must-match: {ok} ok, {failed} failed");
        out
    }

    /// The overlay: both boards at one scale, from their upper-left corners. Raspberry Pi's
    /// outline, holes and pads are blue, the board's are orange; each must-match part, hole and
    /// the outline carries a label with its offset, and a failing one is red.
    pub fn overlay_svg(&self) -> String {
        overlay(self)
    }
}

/// The upper-left corner of the outline's bounding box.
fn corner(board: &Board) -> (f64, f64) {
    (board.outline.min_x, board.outline.min_y)
}

fn num(n: f64) -> String {
    let mut s = format!("{n:.3}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" { "0".to_string() } else { s }
}

fn signed(n: f64) -> String {
    let n = if n.abs() < 5e-4 { 0.0 } else { n };
    format!("{n:+.3}")
}

/// The difference of two angles in (-180, 180].
fn angle_diff(actual: f64, reference: f64) -> f64 {
    let d = (actual - reference).rem_euclid(360.0);
    if d > 180.0 { d - 360.0 } else { d }
}

fn dist(dx: f64, dy: f64) -> f64 {
    dx.hypot(dy)
}

/// Read the board `fab` wrote and compare it. `table` is the text of `placements.toml`, `pcb`
/// the text of the `.kicad_pcb`.
pub fn check_text(
    table: &str,
    pcb: &str,
    map: &DesignatorMap,
    tolerances: &Tolerances,
) -> Result<ParityReport, String> {
    let reference = Board::from_table(table)?;
    let actual = read_board(pcb, "the board")?;
    let mut report = check(&reference, &actual, map, tolerances);
    report.origins = origin_lines(&reference, &actual, pcb, map);
    Ok(report)
}

/// Compare `actual` (the board `fab` wrote) with `reference` (Raspberry Pi's).
pub fn check(
    reference: &Board,
    actual: &Board,
    map: &DesignatorMap,
    tol: &Tolerances,
) -> ParityReport {
    let mut items = Vec::new();
    let mut warnings = Vec::new();
    outline_items(reference, actual, tol, &mut items);
    hole_items(reference, actual, tol, &mut items);
    part_items(reference, actual, map, tol, &mut items, &mut warnings);
    let rank = |i: &Item| -> u8 {
        if i.subject == "outline" {
            0
        } else if i.id.starts_with("hole ") {
            1
        } else if i.id.starts_with("part ") {
            2
        } else {
            3
        }
    };
    // Stable: the fields of one part stay in the order they were measured.
    items.sort_by_cached_key(|i| (rank(i), natural_key(&i.subject)));
    ParityReport {
        items,
        warnings,
        reference: reference.clone(),
        actual: actual.clone(),
        map: map.clone(),
        origins: Vec::new(),
    }
}

fn radius_of(o: &Outline) -> Option<f64> {
    if o.radii.is_empty() {
        Some(0.0)
    } else {
        o.corner_radius()
    }
}

fn outline_items(r: &Board, a: &Board, tol: &Tolerances, items: &mut Vec<Item>) {
    let size = |o: &Outline| (o.max_x - o.min_x, o.max_y - o.min_y);
    let (rw, rh) = size(&r.outline);
    let (aw, ah) = size(&a.outline);
    let (dw, dh) = (aw - rw, ah - rh);
    let mut item = Item::new("outline size".into(), "size", "outline", true).within(tol.outline);
    item.dsize = Some((dw, dh));
    items.push(
        item.noting(format!(
            "{} x {} against {} x {}",
            num(aw),
            num(ah),
            num(rw),
            num(rh)
        ))
        .judge(dw.abs() <= tol.outline + EPS && dh.abs() <= tol.outline + EPS),
    );

    let mut item =
        Item::new("outline corner radius".into(), "radius", "outline", true).within(tol.outline);
    match (radius_of(&r.outline), radius_of(&a.outline)) {
        (Some(rr), Some(ar)) => {
            item.dvalue = Some(ar - rr);
            items.push(
                item.noting(format!("{} against {}", num(ar), num(rr)))
                    .judge((ar - rr).abs() <= tol.outline + EPS),
            );
        }
        _ => items.push(
            item.noting("the arcs of a board differ in radius, so there is no one corner radius")
                .judge(false),
        ),
    }

    // The elements: a corner cut by a line instead of an arc, or a cut-out inside the board,
    // leaves the box and the radius as they were and changes the count of lines or arcs.
    let sorted = |o: &Outline| {
        let mut radii = o.radii.clone();
        radii.sort_by(f64::total_cmp);
        radii
    };
    let (rr, ar) = (sorted(&r.outline), sorted(&a.outline));
    let radii_equal = rr.len() == ar.len()
        && rr
            .iter()
            .zip(&ar)
            .all(|(p, q)| (p - q).abs() <= tol.outline + EPS);
    let list = |radii: &[f64]| radii.iter().map(|r| num(*r)).collect::<Vec<_>>().join(" ");
    let item =
        Item::new("outline elements".into(), "elements", "outline", true).within(tol.outline);
    items.push(
        item.noting(format!(
            "{} lines, {} arcs (radii {}) against {} lines, {} arcs (radii {})",
            a.outline.lines,
            ar.len(),
            list(&ar),
            r.outline.lines,
            rr.len(),
            list(&rr)
        ))
        .judge(a.outline.lines == r.outline.lines && radii_equal),
    );

    let (rx, ry) = corner(r);
    let (ax, ay) = corner(a);
    let mut item =
        Item::new("outline origin".into(), "origin", "outline", false).at(ax - rx, ay - ry);
    item.verdict = Verdict::Info;
    items.push(item.noting(
        "the corners as the files state them: a board moved as a whole is not a difference",
    ));
}

/// The id of each hole's item, in the order of `holes`: `hole <ref>` (`hole extra <ref>` with
/// `extra`), and `hole <ref>#1`, `hole <ref>#2`, ... when one footprint has several holes (a
/// connector with two pegs), numbered by the holes' places (top to bottom, then left to right),
/// so that the ids are unique and do not depend on the order of the file's pads.
fn hole_ids(holes: &[Hole], extra: bool) -> Vec<String> {
    let prefix = if extra { "hole extra" } else { "hole" };
    holes
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let mut same: Vec<usize> = (0..holes.len())
                .filter(|&j| holes[j].reference == h.reference)
                .collect();
            if same.len() == 1 {
                return format!("{prefix} {}", h.reference);
            }
            same.sort_by(|&p, &q| {
                holes[p]
                    .y
                    .total_cmp(&holes[q].y)
                    .then(holes[p].x.total_cmp(&holes[q].x))
            });
            let n = same.iter().position(|&j| j == i).unwrap_or(0) + 1;
            format!("{prefix} {}#{n}", h.reference)
        })
        .collect()
}

/// The board's hole that each hole of the reference is paired with (the nearest centre of those
/// not taken yet, from the outline's corner of each), or `None` when the board has none left.
fn pair_holes(r: &Board, a: &Board) -> Vec<Option<usize>> {
    let (rc, ac) = (corner(r), corner(a));
    let rel = |h: &Hole, c: (f64, f64)| (h.x - c.0, h.y - c.1);
    let mut used = vec![false; a.holes.len()];
    r.holes
        .iter()
        .map(|hole| {
            let (hx, hy) = rel(hole, rc);
            let nearest = a
                .holes
                .iter()
                .enumerate()
                .filter(|(i, _)| !used[*i])
                .map(|(i, h)| {
                    let (x, y) = rel(h, ac);
                    (i, dist(x - hx, y - hy))
                })
                .min_by(|p, q| p.1.total_cmp(&q.1))
                .map(|(i, _)| i);
            if let Some(i) = nearest {
                used[i] = true;
            }
            nearest
        })
        .collect()
}

fn hole_items(r: &Board, a: &Board, tol: &Tolerances, items: &mut Vec<Item>) {
    let (rc, ac) = (corner(r), corner(a));
    let rel = |h: &Hole, c: (f64, f64)| (h.x - c.0, h.y - c.1);
    let ids = hole_ids(&r.holes, false);
    let pairs = pair_holes(r, a);
    let mut used = vec![false; a.holes.len()];
    for ((hole, id), nearest) in r.holes.iter().zip(ids).zip(pairs) {
        let (hx, hy) = rel(hole, rc);
        let item = Item::new(id, "position", &hole.reference, true).within(tol.hole);
        let Some(i) = nearest else {
            items.push(
                item.noting("the board has no hole to match this one")
                    .judge(false),
            );
            continue;
        };
        used[i] = true;
        let other = &a.holes[i];
        let (x, y) = rel(other, ac);
        let (dx, dy) = (x - hx, y - hy);
        let dd = other.drill - hole.drill;
        let dsize = (other.size.0 - hole.size.0, other.size.1 - hole.size.1);
        let mut item = item.at(dx, dy);
        item.dvalue = Some(dd);
        item.dsize = Some(dsize);
        let drill_wrong = dd.abs() > tol.hole_drill + EPS;
        let size_wrong = dsize.0.abs() > tol.pad_size + EPS || dsize.1.abs() > tol.pad_size + EPS;
        let plating_wrong = other.plated != hole.plated;
        // A made-up name is no net (a mounting hole's pad is on a net of its own or on none).
        let net_of = |h: &Hole| h.net.as_deref().and_then(plain);
        let net_wrong = net_of(hole) != net_of(other);
        let named = |h: &Hole| h.net.as_deref().unwrap_or("(none)").to_string();
        let mut note = format!("drill {} against {}", num(other.drill), num(hole.drill));
        if drill_wrong {
            let _ = write!(note, " (tol {})", num(tol.hole_drill));
        }
        let _ = write!(
            note,
            ", pad {} x {} against {} x {}",
            num(other.size.0),
            num(other.size.1),
            num(hole.size.0),
            num(hole.size.1)
        );
        if size_wrong {
            let _ = write!(note, " (tol {})", num(tol.pad_size));
        }
        let plating = |h: &Hole| if h.plated { "plated" } else { "non-plated" };
        if plating_wrong {
            let _ = write!(note, "; {} against {}", plating(other), plating(hole));
        }
        if net_wrong {
            let _ = write!(note, "; net {} against {}", named(other), named(hole));
        }
        let good = dist(dx, dy) <= tol.hole + EPS
            && !drill_wrong
            && !size_wrong
            && !plating_wrong
            && !net_wrong;
        items.push(item.noting(note).judge(good));
    }
    let extra_ids = hole_ids(&a.holes, true);
    for (h, id) in a
        .holes
        .iter()
        .zip(extra_ids)
        .enumerate()
        .filter(|(i, _)| !used[*i])
        .map(|(_, p)| p)
    {
        let (x, y) = rel(h, ac);
        let mut item = Item::new(id, "position", &format!("extra {}", h.reference), false);
        item.verdict = Verdict::Info;
        items.push(item.noting(format!(
            "a hole of the board at ({}, {}), drill {}, that the reference does not have",
            num(x),
            num(y),
            num(h.drill)
        )));
    }
}

fn part_items(
    r: &Board,
    a: &Board,
    map: &DesignatorMap,
    tol: &Tolerances,
    items: &mut Vec<Item>,
    warnings: &mut Vec<String>,
) {
    let (rc, ac) = (corner(r), corner(a));
    // A mounting hole is a part of nothing but non-plated pads, or a plated hole of KiCad's
    // `MountingHole_*_Pad*` family (its drilled pad, and perhaps a ring of vias that need not
    // be copied), that the hole check judges by place and drill. A connector with a peg is a
    // part, and its peg is one of its pads.
    let hole_refs: BTreeSet<&str> = r
        .parts
        .iter()
        .filter(|p| {
            !p.pads.is_empty()
                && r.holes.iter().any(|h| {
                    h.reference == p.reference
                        && (h.plated || p.pads.iter().all(|pad| pad.kind == "np_thru_hole"))
                })
        })
        .map(|p| p.reference.as_str())
        .collect();
    let by_ref: BTreeMap<&str, &Part> = a.parts.iter().map(|p| (p.reference.as_str(), p)).collect();
    // The reference of Raspberry Pi's part for each part of the board that is mapped to.
    let mut pairs: Vec<(&Part, &Part)> = Vec::new();
    for part in r
        .parts
        .iter()
        .filter(|p| !hole_refs.contains(p.reference.as_str()))
    {
        let id = format!("part {}", part.reference);
        let must = part.must_match;
        let Some(to) = map.get(&part.reference) else {
            items.push(
                Item::new(format!("{id} unmapped"), "mapping", &part.reference, must)
                    .noting("the part has no entry in the designator map")
                    .judge(false),
            );
            continue;
        };
        let Some(found) = by_ref.get(to).copied() else {
            items.push(
                Item::new(format!("{id} missing"), "mapping", &part.reference, must)
                    .noting(format!("the map says {to}, which the board does not have"))
                    .judge(false),
            );
            continue;
        };
        pairs.push((part, found));

        let (px, py) = (part.x - rc.0, part.y - rc.1);
        let (qx, qy) = (found.x - ac.0, found.y - ac.1);
        let (dx, dy) = (qx - px, qy - py);
        items.push(
            Item::new(format!("{id} position"), "position", &part.reference, must)
                .at(dx, dy)
                .within(tol.position)
                .judge(dist(dx, dy) <= tol.position + EPS),
        );
        let drot = angle_diff(found.rotation, part.rotation);
        let mut item = Item::new(format!("{id} rotation"), "rotation", &part.reference, must)
            .within(tol.rotation);
        item.drotation = Some(drot);
        items.push(
            item.noting(format!(
                "{} against {}",
                num(found.rotation),
                num(part.rotation)
            ))
            .judge(drot.abs() <= tol.rotation),
        );
        items.push(
            Item::new(format!("{id} layer"), "layer", &part.reference, must)
                .noting(format!("{} against {}", found.layer, part.layer))
                .judge(found.layer == part.layer),
        );
        pad_items(part, found, tol, items);
    }
    net_items(&pairs, (&net_sizes(r), &net_sizes(a)), items, warnings);
}

/// What two paired pads differ by.
struct PadMeasure {
    dx: f64,
    dy: f64,
    dsize: Option<(f64, f64)>,
    ddrill: Option<f64>,
    good: bool,
    why: String,
}

/// A pad's name for an item: its number, with `(k of n)` when the part has several of it.
fn pad_label(number: &str, k: usize, n: usize) -> String {
    let base = if number.is_empty() {
        "(no number)".to_string()
    } else {
        number.to_string()
    };
    if n > 1 {
        format!("{base} ({} of {n})", k + 1)
    } else {
        base
    }
}

/// The pads of two parts, paired.
struct Paired<'a> {
    /// `(label, index of the reference's pad in its part, reference's pad, board's pad)`.
    pairs: Vec<(String, usize, &'a Pad, &'a Pad)>,
    /// `(label, true when it is the board that has the pad, the reference having none)`.
    lone: Vec<(String, bool)>,
}

/// The pads of two parts paired by number and, among equal numbers, by the nearest centre in
/// the footprint's own frame (so the pads of one number are told apart by where they are, not by
/// the order a file lists them in).
fn pair_pads<'a>(r: &'a Part, a: &'a Part) -> Paired<'a> {
    let mut numbers: Vec<&str> = r
        .pads
        .iter()
        .chain(&a.pads)
        .map(|p| p.number.as_str())
        .collect();
    numbers.sort_by_key(|n| (natural_key(n), n.to_string()));
    numbers.dedup();
    let mut paired = Paired {
        pairs: Vec::new(),
        lone: Vec::new(),
    };
    for number in numbers {
        let rp: Vec<(usize, &Pad)> = r
            .pads
            .iter()
            .enumerate()
            .filter(|(_, p)| p.number == number)
            .collect();
        let mut ap: Vec<&Pad> = a.pads.iter().filter(|p| p.number == number).collect();
        let n = rp.len().max(ap.len());
        for (k, (index, pad)) in rp.iter().enumerate() {
            let label = pad_label(number, k, n);
            let nearest = ap
                .iter()
                .enumerate()
                .map(|(i, q)| (i, dist(q.x - pad.x, q.y - pad.y)))
                .min_by(|p, q| p.1.total_cmp(&q.1));
            match nearest {
                Some((i, _)) => {
                    let q = ap.remove(i);
                    paired.pairs.push((label, *index, pad, q));
                }
                None => paired.lone.push((label, false)),
            }
        }
        for (k, _) in ap.iter().enumerate() {
            paired.lone.push((pad_label(number, rp.len() + k, n), true));
        }
    }
    paired
}

/// A pad as it is in its footprint's own frame. A pad's own angle includes the footprint's
/// rotation, so the angle in the footprint is the difference. A pad turned by a quarter has its
/// width and height (and a slot's sides) swapped; one turned by a half is as it was, unless it is
/// a custom pad, whose polygon is drawn and keeps its angle.
struct PadFrame {
    size: Option<(f64, f64)>,
    slot: Option<(f64, f64)>,
    extent: Option<[f64; 4]>,
    /// In degrees: 0 to 360 for a custom pad, 0 to 90 for a rectangle or an oval, 0 for a circle.
    angle: f64,
}

fn pad_frame(part: &Part, pad: &Pad, tol: &Tolerances) -> PadFrame {
    let turn = (pad.angle.unwrap_or(0.0) - part.rotation).rem_euclid(360.0);
    if pad.shape == "custom" {
        let turn = if turn > 360.0 - tol.rotation {
            0.0
        } else {
            turn
        };
        return PadFrame {
            size: pad.size,
            slot: pad.drill_slot,
            extent: pad.extent,
            angle: turn,
        };
    }
    let mut t = turn.rem_euclid(180.0);
    let mut swap = false;
    if t >= 180.0 - tol.rotation {
        t = 0.0;
    } else if t >= 90.0 - tol.rotation {
        t -= 90.0;
        swap = true;
    }
    if t.abs() <= tol.rotation || pad.shape == "circle" {
        t = 0.0;
    }
    let turned = |(w, h): (f64, f64)| if swap { (h, w) } else { (w, h) };
    PadFrame {
        size: pad.size.map(turned),
        slot: pad.drill_slot.map(turned),
        extent: None,
        angle: t,
    }
}

/// A pad's copper layers as a set.
fn faces(pad: &Pad) -> BTreeSet<&str> {
    pad.layers.iter().map(String::as_str).collect()
}

/// What two paired pads of the parts `rp` and `ap` differ by.
fn judge_pair(rp: &Part, pad: &Pad, ap: &Part, q: &Pad, tol: &Tolerances) -> PadMeasure {
    let (dx, dy) = (q.x - pad.x, q.y - pad.y);
    let (fr, fa) = (pad_frame(rp, pad, tol), pad_frame(ap, q, tol));
    let dsize = match (fr.size, fa.size) {
        (Some((w, h)), Some((qw, qh))) => Some((qw - w, qh - h)),
        _ => None,
    };
    // A round hole is its diameter twice; a slot is its two sides.
    let hole = |p: &Pad, f: &PadFrame| p.drill.map(|d| (d, d)).or(f.slot);
    let ddrill = match (hole(pad, &fr), hole(q, &fa)) {
        (Some((w, h)), Some((qw, qh))) => {
            let (dw, dh) = (qw - w, qh - h);
            Some(if dw.abs() >= dh.abs() { dw } else { dh })
        }
        _ => None,
    };
    let drill_wrong = match (hole(pad, &fr), hole(q, &fa)) {
        (Some((w, h)), Some((qw, qh))) => {
            (qw - w).abs() > tol.pad_size + EPS || (qh - h).abs() > tol.pad_size + EPS
        }
        (None, None) => false,
        _ => true,
    } || pad.drill.is_some() != q.drill.is_some()
        || pad.drill_slot.is_some() != q.drill_slot.is_some();
    let mut why = Vec::new();
    if dist(dx, dy) > tol.pad_position + EPS {
        why.push("centre".to_string());
    }
    if dsize.is_some_and(|(w, h)| w.abs() > tol.pad_size + EPS || h.abs() > tol.pad_size + EPS)
        || fr.size.is_some() != fa.size.is_some()
    {
        let wh = |s: Option<(f64, f64)>| {
            s.map_or("none".to_string(), |(w, h)| {
                format!("{} x {}", num(w), num(h))
            })
        };
        why.push(format!(
            "size (the board's {}, the reference's {})",
            wh(fa.size),
            wh(fr.size)
        ));
    }
    if pad.shape != q.shape {
        why.push(format!(
            "shape (the board's {}, the reference's {})",
            q.shape, pad.shape
        ));
    }
    if angle_diff(fa.angle, fr.angle).abs() > tol.rotation {
        why.push(format!(
            "angle (the board's {}, the reference's {})",
            num(fa.angle),
            num(fr.angle)
        ));
    }
    let extent_wrong = match (fr.extent, fa.extent) {
        (Some(e), Some(f)) => e
            .iter()
            .zip(&f)
            .any(|(p, q)| (p - q).abs() > tol.pad_size + EPS),
        (None, None) => false,
        _ => true,
    };
    if extent_wrong {
        why.push("extent of its polygon".to_string());
    }
    if drill_wrong {
        let drill = |h: Option<(f64, f64)>| match h {
            None => "none".to_string(),
            Some((w, h)) if (w - h).abs() < EPS => num(w),
            Some((w, h)) => format!("{} x {}", num(w), num(h)),
        };
        why.push(format!(
            "drill (the board's {}, the reference's {})",
            drill(hole(q, &fa)),
            drill(hole(pad, &fr))
        ));
    }
    if pad.kind != q.kind {
        why.push(format!(
            "type (the board's {}, the reference's {})",
            q.kind, pad.kind
        ));
    }
    // The copper faces, as sets: a pad on the wrong face is the wrong copper (#537).
    if faces(pad) != faces(q) {
        let names = |p: &Pad| {
            if p.layers.is_empty() {
                "none".to_string()
            } else {
                faces(p).into_iter().collect::<Vec<_>>().join(" ")
            }
        };
        why.push(format!(
            "layers (the board's {}, the reference's {})",
            names(q),
            names(pad)
        ));
    }
    let good = why.is_empty();
    PadMeasure {
        dx,
        dy,
        dsize,
        ddrill,
        good,
        why: why.join(", "),
    }
}

/// The pads of two parts: one item for each pair, or one for the whole part when it need not
/// match.
fn pad_items(r: &Part, a: &Part, tol: &Tolerances, items: &mut Vec<Item>) {
    let must = r.must_match;
    let id = format!("part {}", r.reference);
    let paired = pair_pads(r, a);
    let lone: Vec<Item> = paired
        .lone
        .iter()
        .map(|(label, board_has_it)| {
            Item::new(format!("{id} pad {label}"), "pad", &r.reference, must)
                .noting(if *board_has_it {
                    "the board's part has a pad that the reference does not have"
                } else {
                    "the board's part has no such pad"
                })
                .judge(false)
        })
        .collect();

    if must {
        for (label, _, pad, q) in &paired.pairs {
            let PadMeasure {
                dx,
                dy,
                dsize,
                ddrill,
                good,
                why,
            } = judge_pair(r, pad, a, q, tol);
            let mut item = Item::new(format!("{id} pad {label}"), "pad", &r.reference, true)
                .at(dx, dy)
                .within(tol.pad_position);
            item.dsize = dsize;
            item.dvalue = ddrill;
            if !good {
                item.note = format!("differs in {why}");
            }
            items.push(item.judge(good));
        }
        items.extend(lone);
    } else {
        // One line for all the pads of a part that need not match: the worst one.
        let mut bad = Vec::new();
        let mut worst: Option<(f64, f64, &str)> = None;
        for (label, _, pad, q) in &paired.pairs {
            let PadMeasure {
                dx, dy, good, why, ..
            } = judge_pair(r, pad, a, q, tol);
            if !good {
                bad.push(format!("{label} in {why}"));
            }
            if worst.is_none_or(|w| dist(dx, dy) > dist(w.0, w.1)) {
                worst = Some((dx, dy, label));
            }
        }
        let missing = lone.len();
        let mut item =
            Item::new(format!("{id} pads"), "pad", &r.reference, false).within(tol.pad_position);
        if let Some((dx, dy, label)) = worst {
            item = item.at(dx, dy);
            item.note = format!(
                "{} pads, the largest offset on pad {label}",
                paired.pairs.len()
            );
        }
        if !bad.is_empty() || missing > 0 {
            item.note = format!(
                "{} pads differ{}{}; {}",
                bad.len(),
                if bad.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", bad.join("; "))
                },
                if missing > 0 {
                    format!(" and {missing} have no partner")
                } else {
                    String::new()
                },
                item.note
            );
        }
        items.push(item.judge(bad.is_empty() && missing == 0));
    }
}

/// A name that says something: KiCad's `Net-(...)` and `unconnected-(...)` are made up.
fn plain(name: &str) -> Option<String> {
    let n = name.trim();
    if n.is_empty() || n.starts_with("Net-(") || n.starts_with("unconnected-") {
        return None;
    }
    Some(n.strip_prefix('/').unwrap_or(n).to_lowercase())
}

/// A pad of the reference: `(Raspberry Pi's reference of the part, the index of the pad in the
/// part's pads)`. The board's pad that is paired with it has the same key.
type Key = (String, usize);

/// The pads of each net, and each pad's net, by key.
#[derive(Default)]
struct Nets {
    by_net: BTreeMap<String, BTreeSet<Key>>,
    by_pad: BTreeMap<Key, String>,
}

impl Nets {
    fn add(&mut self, key: Key, pad: &Pad) {
        // A non-plated pad is on no net.
        if pad.kind == "np_thru_hole" {
            return;
        }
        if let Some(net) = &pad.net {
            self.by_net
                .entry(net.clone())
                .or_default()
                .insert(key.clone());
            self.by_pad.insert(key, net.clone());
        }
    }

    fn name(&self, key: &Key) -> Option<&str> {
        self.by_pad.get(key).map(String::as_str)
    }

    /// The pads that share a net with `key`, other than `key` itself.
    fn class(&self, key: &Key) -> BTreeSet<Key> {
        let mut set = self
            .by_pad
            .get(key)
            .map(|net| self.by_net[net].clone())
            .unwrap_or_default();
        set.remove(key);
        set
    }
}

/// How many pads are on each net of a board, every part counted, mapped or not.
fn net_sizes(board: &Board) -> BTreeMap<String, usize> {
    let mut sizes = BTreeMap::new();
    for pad in board
        .parts
        .iter()
        .flat_map(|p| &p.pads)
        .filter(|p| p.kind != "np_thru_hole")
    {
        if let Some(net) = pad.net.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            *sizes.entry(net.to_string()).or_default() += 1;
        }
    }
    sizes
}

/// A pad that nothing is connected to: it names no net, or its net has no other pad on the board.
fn alone(sizes: &BTreeMap<String, usize>, net: Option<&str>) -> bool {
    net.map(str::trim)
        .filter(|n| !n.is_empty())
        .is_none_or(|n| sizes.get(n).copied().unwrap_or(0) <= 1)
}

fn net_items(
    pairs: &[(&Part, &Part)],
    (their_sizes, our_sizes): (&BTreeMap<String, usize>, &BTreeMap<String, usize>),
    items: &mut Vec<Item>,
    warnings: &mut Vec<String>,
) {
    let paired: Vec<Paired> = pairs.iter().map(|(r, a)| pair_pads(r, a)).collect();
    let (mut theirs, mut ours) = (Nets::default(), Nets::default());
    for ((r, _), p) in pairs.iter().zip(&paired) {
        // Every pad of the reference counts, also one with no partner: the board lacks it.
        for (index, pad) in r.pads.iter().enumerate() {
            theirs.add((r.reference.clone(), index), pad);
        }
        for (_, index, _, q) in &p.pairs {
            ours.add((r.reference.clone(), *index), q);
        }
    }
    let show = |set: &BTreeSet<Key>| -> String {
        if set.is_empty() {
            "no other mapped pad".to_string()
        } else {
            set.iter()
                .map(|(r, n)| {
                    let number = pairs
                        .iter()
                        .find(|(p, _)| p.reference == *r)
                        .map_or("?", |(p, _)| p.pads[*n].number.as_str());
                    format!("{r}.{number}")
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
    };

    for ((part, _), p) in pairs.iter().zip(&paired).filter(|((r, _), _)| r.must_match) {
        for (label, index, pad, q) in &p.pairs {
            // A peg has no net on either side: nothing to compare.
            if pad.kind == "np_thru_hole"
                && q.kind == "np_thru_hole"
                && pad.net.is_none()
                && q.net.is_none()
            {
                continue;
            }
            let key: Key = (part.reference.clone(), *index);
            let (rn, an) = (theirs.name(&key), ours.name(&key));
            let (rc, ac) = (theirs.class(&key), ours.class(&key));
            let id = format!("part {} pad {label} net", part.reference);
            let item = Item::new(id, "net", &part.reference, true);
            let named = |n: Option<&str>| n.unwrap_or("(none)").to_string();
            let same_class = rc == ac;
            let (good, note) = match (rn.and_then(plain), an.and_then(plain)) {
                (Some(r), Some(a)) if r == a => (true, named(rn)),
                // Neither name says anything. A pad on no net is not a made-up name: where one
                // side has other pads on the pad's net (mapped or not) and the other has none,
                // they are not the same pad.
                (None, None) if alone(their_sizes, rn) != alone(our_sizes, an) => (
                    false,
                    format!(
                        "{} against {}: {}",
                        named(an),
                        named(rn),
                        if alone(their_sizes, rn) {
                            "the board's pad is on a net with other pads, and the reference's is \
                             on none or alone on its own"
                        } else {
                            "the reference connects the pad to other pads, and the board's is on \
                             no net or alone on its own"
                        }
                    ),
                ),
                (Some(_), Some(_)) if same_class && !rc.is_empty() => {
                    warnings.push(format!(
                        "part {} pad {label}: the net is {} on the board and {} in the reference; \
                         the same pads are on it ({}), so it passes",
                        part.reference,
                        named(an),
                        named(rn),
                        show(&rc)
                    ));
                    (
                        true,
                        format!("{} against {}, same pads", named(an), named(rn)),
                    )
                }
                (Some(_), Some(_)) if same_class => (
                    false,
                    format!(
                        "{} against {}, and no other mapped pad is on the net to vouch for it \
                         (map the part that the net goes to)",
                        named(an),
                        named(rn)
                    ),
                ),
                (Some(_), Some(_)) => (
                    false,
                    format!(
                        "{} against {}: the board has {}, the reference {}",
                        named(an),
                        named(rn),
                        show(&ac),
                        show(&rc)
                    ),
                ),
                (r, a) if same_class && r.is_some() != a.is_some() && rc.is_empty() => (
                    false,
                    format!(
                        "{} against {}: one is a made-up name, and no other mapped pad is on the \
                         net to vouch for it (nothing vouches for it: map the part that the net \
                         goes to)",
                        named(an),
                        named(rn)
                    ),
                ),
                (r, a) if same_class => {
                    if r.is_some() != a.is_some() {
                        warnings.push(format!(
                            "part {} pad {label}: the net is {} on the board and {} in the \
                             reference; one is a made-up name, so the pads on it decide",
                            part.reference,
                            named(an),
                            named(rn)
                        ));
                    }
                    (true, format!("same pads ({})", show(&rc)))
                }
                _ => (
                    false,
                    format!(
                        "{} against {}: the board has {}, the reference {}",
                        named(an),
                        named(rn),
                        show(&ac),
                        show(&rc)
                    ),
                ),
            };
            items.push(item.noting(note).judge(good));
        }
    }
}

// ---- the overlay -------------------------------------------------------------------------

const BLUE: &str = "#2b6cb0";
const ORANGE: &str = "#dd6b20";
const RED: &str = "#e53e3e";

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn overlay(rep: &ParityReport) -> String {
    let (r, a) = (&rep.reference, &rep.actual);
    let (rc, ac) = (corner(r), corner(a));
    let width = (r.outline.max_x - r.outline.min_x).max(a.outline.max_x - a.outline.min_x);
    let height = (r.outline.max_y - r.outline.min_y).max(a.outline.max_y - a.outline.min_y);
    let margin = 6.0;
    let legend = 10.0;
    let (vw, vh) = (width + 2.0 * margin, height + 2.0 * margin + legend);
    let scale = 10.0;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" \
         font-family=\"sans-serif\" font-size=\"1.1\">",
        num(vw * scale),
        num(vh * scale),
        num(-margin),
        num(-margin),
        num(vw),
        num(vh)
    );
    let _ = writeln!(
        s,
        "<title>Parity overlay: the original board (blue) and netsmith's (orange)</title>"
    );
    let _ = writeln!(
        s,
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>",
        num(-margin),
        num(-margin),
        num(vw),
        num(vh)
    );

    let failing: BTreeSet<&str> = rep.failures().map(|i| i.subject.as_str()).collect();
    // Holes are in no designator map: a board's hole is red when the item of the reference's
    // hole that it is paired with fails.
    let failing_ids: BTreeSet<&str> = rep.failures().map(|i| i.id.as_str()).collect();
    let failing_holes: BTreeSet<usize> = hole_ids(&r.holes, false)
        .iter()
        .zip(pair_holes(r, a))
        .filter(|(id, _)| failing_ids.contains(id.as_str()))
        .filter_map(|(_, i)| i)
        .collect();
    for (board, c, color, tag) in [(r, rc, BLUE, "reference"), (a, ac, ORANGE, "board")] {
        let _ = writeln!(
            s,
            "<g class=\"{tag}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"0.12\">"
        );
        let o = &board.outline;
        let rx = radius_of(o).unwrap_or(0.0);
        let red = failing.contains("outline") && tag == "board";
        let _ = writeln!(
            s,
            "<rect class=\"outline{}\" x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" rx=\"{}\"{}/>",
            if red { " fail" } else { "" },
            num(o.max_x - o.min_x),
            num(o.max_y - o.min_y),
            num(rx),
            if red {
                format!(" stroke=\"{RED}\" stroke-width=\"0.25\"")
            } else {
                String::new()
            }
        );
        for (i, h) in board.holes.iter().enumerate() {
            let red = tag == "board" && failing_holes.contains(&i);
            let _ = writeln!(
                s,
                "<circle class=\"hole{}\" cx=\"{}\" cy=\"{}\" r=\"{}\"{}/>",
                if red { " fail" } else { "" },
                num(h.x - c.0),
                num(h.y - c.1),
                num(h.drill / 2.0),
                if red {
                    format!(" stroke=\"{RED}\" stroke-width=\"0.2\"")
                } else {
                    String::new()
                }
            );
        }
        for part in &board.parts {
            let red = tag == "board" && part_failing(rep, part);
            let _ = writeln!(
                s,
                "<g class=\"part{}\" data-ref=\"{}\"{}>",
                if red { " fail" } else { "" },
                xml(&part.reference),
                if red {
                    format!(" stroke=\"{RED}\" stroke-width=\"0.2\"")
                } else {
                    String::new()
                }
            );
            for pad in &part.pads {
                let _ = writeln!(s, "{}", pad_svg(part, pad, c));
            }
            let _ = writeln!(s, "</g>");
        }
        let _ = writeln!(s, "</g>");
    }

    // Labels: the outline, the holes and every must-match part, with their offsets.
    let _ = writeln!(s, "<g class=\"labels\" stroke=\"none\">");
    for item in rep.items.iter().filter(|i| i.must_match) {
        let red = item.verdict == Verdict::Fail;
        // One label for each subject: its position, or what failed when that is the line.
        let place = label_place(rep, &item.subject);
        let Some((x, y)) = place else { continue };
        if !(item.field == "position" || item.field == "size" || red) {
            continue;
        }
        if !red && item.field == "size" && item.subject != "outline" {
            continue;
        }
        let text = match (item.dx, item.dy, item.dsize) {
            (Some(dx), Some(dy), _) => format!(
                "{} {}: dx {} dy {}",
                item.subject,
                item.field,
                signed(dx),
                signed(dy)
            ),
            (_, _, Some((w, h))) => {
                format!("{} size: dw {} dh {}", item.subject, signed(w), signed(h))
            }
            _ => format!("{} {}: {}", item.subject, item.field, item.note),
        };
        let _ = writeln!(
            s,
            "<text class=\"{}\" x=\"{}\" y=\"{}\" fill=\"{}\">{}</text>",
            if red { "label fail" } else { "label" },
            num(x),
            num(y),
            if red { RED } else { "#2d3748" },
            xml(&text)
        );
    }
    let _ = writeln!(s, "</g>");
    let ly = height + margin - 1.0;
    let _ = writeln!(
        s,
        "<g class=\"legend\" stroke=\"none\"><text x=\"0\" y=\"{}\" fill=\"{BLUE}\">blue: the original board</text>\
         <text x=\"16\" y=\"{}\" fill=\"{ORANGE}\">orange: netsmith's board</text>\
         <text x=\"34\" y=\"{}\" fill=\"{RED}\">red: a must-match item that fails</text>\
         <text x=\"0\" y=\"{}\" fill=\"#4a5568\">both from the upper-left corner of the outline's box, 1 unit = 1 mm</text>\
         <text x=\"0\" y=\"{}\" fill=\"#4a5568\">a part is judged on position, rotation and layer, each pad on number, centre, \
         size, shape, drill, copper layers and net, the outline on its box, radius and elements</text></g>",
        num(ly),
        num(ly),
        num(ly),
        num(ly + 2.0),
        num(ly + 4.0)
    );
    let _ = writeln!(s, "</svg>");
    s
}

/// Does the board's part (a part of the board file, by its own reference) have a failing item?
fn part_failing(rep: &ParityReport, part: &Part) -> bool {
    let subjects: Vec<&str> = rep
        .map
        .0
        .iter()
        .filter(|(_, to)| **to == part.reference)
        .map(|(from, _)| from.as_str())
        .collect();
    rep.failures()
        .any(|i| subjects.contains(&i.subject.as_str()))
}

/// Where a subject's label goes, in the reference's frame: the part's place, the hole's place or
/// the top of the outline.
fn label_place(rep: &ParityReport, subject: &str) -> Option<(f64, f64)> {
    let r = &rep.reference;
    let (cx, cy) = corner(r);
    if subject == "outline" {
        return Some((0.0, -2.0));
    }
    if let Some(h) = r.holes.iter().find(|h| h.reference == subject) {
        return Some((h.x - cx + 2.0, h.y - cy));
    }
    if let Some(p) = r.parts.iter().find(|p| p.reference == subject) {
        return Some((p.x - cx, p.y - cy));
    }
    // A hole or part of the board that the reference does not have.
    Some((0.0, -4.0))
}

/// A pad as an SVG shape in the board's frame (from its board's corner `c`).
fn pad_svg(part: &Part, pad: &Pad, c: (f64, f64)) -> String {
    let (x, y) = part.board_point(pad.x, pad.y);
    let (x, y) = (x - c.0, y - c.1);
    let angle = pad.angle.unwrap_or(0.0);
    let Some((w, h)) = pad.size else {
        return format!("<circle cx=\"{}\" cy=\"{}\" r=\"0.1\"/>", num(x), num(y));
    };
    if pad.shape == "circle" {
        return format!(
            "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"/>",
            num(x),
            num(y),
            num(w / 2.0),
            num(h / 2.0)
        );
    }
    let round = if pad.shape == "oval" || pad.shape == "roundrect" {
        w.min(h) / 2.0
    } else {
        0.0
    };
    // KiCad turns counter-clockwise on the screen, SVG clockwise.
    format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" transform=\"rotate({} {} {})\"/>",
        num(x - w / 2.0),
        num(y - h / 2.0),
        num(w),
        num(h),
        num(round),
        num(-angle),
        num(x),
        num(y)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hole(reference: &str, x: f64, y: f64) -> Hole {
        Hole {
            reference: reference.to_string(),
            x,
            y,
            drill: 1.0,
            size: (1.0, 1.0),
            net: None,
            plated: false,
        }
    }

    fn board(holes: Vec<Hole>) -> Board {
        Board {
            outline: Outline {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 50.0,
                max_y: 30.0,
                lines: 4,
                radii: Vec::new(),
            },
            holes,
            parts: Vec::new(),
        }
    }

    fn report(reference: &Board, actual: &Board) -> ParityReport {
        let map = DesignatorMap::identity(reference);
        check(reference, actual, &map, &Tolerances::default())
    }

    fn red_circles(svg: &str) -> usize {
        svg.lines()
            .filter(|l| l.starts_with("<circle class=\"hole fail\"") && l.contains(RED))
            .count()
    }

    /// A hole of the board that fails (here: 1 mm off its place) is drawn red, by the style a
    /// failing part has; a hole that passes is not. Holes are in no designator map.
    #[test]
    fn a_failing_hole_is_red_in_the_overlay() {
        let reference = board(vec![hole("H1", 5.0, 5.0), hole("H2", 45.0, 5.0)]);
        let passing = board(vec![hole("H1", 5.0, 5.0), hole("H2", 45.0, 5.0)]);
        let svg = report(&reference, &passing).overlay_svg();
        assert_eq!(red_circles(&svg), 0, "{svg}");
        assert!(!svg.contains("class=\"hole fail\""), "{svg}");

        let moved = board(vec![hole("H1", 5.0, 5.0), hole("H2", 45.0, 6.0)]);
        let rep = report(&reference, &moved);
        assert_eq!(
            rep.failures().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            ["hole H2"]
        );
        let svg = rep.overlay_svg();
        assert_eq!(red_circles(&svg), 1, "{svg}");
        // The red one is the board's H2 (at y 6), and none of the reference's.
        assert!(
            svg.contains("<circle class=\"hole fail\" cx=\"45\" cy=\"6\""),
            "{svg}"
        );
    }

    /// A drill that is wrong also reddens the hole, whichever the failing field.
    #[test]
    fn a_hole_with_a_wrong_drill_is_red() {
        let reference = board(vec![hole("H1", 5.0, 5.0)]);
        let mut wrong = hole("H1", 5.0, 5.0);
        wrong.drill = 1.5;
        let svg = report(&reference, &board(vec![wrong])).overlay_svg();
        assert_eq!(red_circles(&svg), 1, "{svg}");
    }

    /// A footprint with one hole keeps the id `hole <ref>`; one with several has an id of its
    /// own for each, numbered by place (not by the order of the pads), and the items are unique.
    #[test]
    fn a_footprints_holes_have_unique_ids() {
        // Pads in the order bottom, top; the numbers go by place: top first.
        let reference = board(vec![
            hole("J1", 10.0, 20.0),
            hole("J1", 10.0, 10.0),
            hole("H1", 40.0, 5.0),
        ]);
        let ids = hole_ids(&reference.holes, false);
        assert_eq!(ids, ["hole J1#2", "hole J1#1", "hole H1"]);

        let mut actual = reference.clone();
        actual.holes[1].y = 11.0; // J1's top peg is 1 mm off
        let rep = report(&reference, &actual);
        let ids: Vec<&str> = rep.items.iter().map(|i| i.id.as_str()).collect();
        for id in ["hole J1#1", "hole J1#2", "hole H1"] {
            assert_eq!(ids.iter().filter(|i| **i == id).count(), 1, "{ids:?}");
        }
        assert_eq!(
            rep.failures().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            ["hole J1#1"]
        );
        // Only that peg is red.
        assert_eq!(red_circles(&rep.overlay_svg()), 1);
    }

    /// The holes of the board that the reference lacks are told apart the same way.
    #[test]
    fn extra_holes_of_one_footprint_have_unique_ids() {
        let reference = board(Vec::new());
        let actual = board(vec![hole("J2", 10.0, 10.0), hole("J2", 20.0, 10.0)]);
        let rep = report(&reference, &actual);
        let ids: Vec<&str> = rep
            .items
            .iter()
            .map(|i| i.id.as_str())
            .filter(|id| id.starts_with("hole"))
            .collect();
        assert_eq!(ids, ["hole extra J2#1", "hole extra J2#2"]);
    }

    #[test]
    fn a_ports_parts_are_paired_by_the_member_path_of_each_footprint() {
        let parts = crate::port::PortParts::parse(
            "[parts]\nmcu = \"U7\"\n\"stage.r1\" = \"R3\"\n",
            "parts.toml",
        )
        .unwrap();
        let footprint = |reference: &str, path: Option<&str>| {
            let path = path.map_or(String::new(), |p| {
                format!("(property \"netsmith_path\" \"{p}\" (hide yes))")
            });
            format!("(footprint \"X\" (property \"Reference\" \"{reference}\") {path})")
        };
        let pcb = format!(
            "(kicad_pcb {} {} {})",
            footprint("U1", Some("mcu")),
            footprint("R1", Some("stage.r1")),
            footprint("H1", Some("hole1")),
        );
        let map = DesignatorMap::from_parts(&parts, &pcb).unwrap();
        assert_eq!(map.get("U7"), Some("U1"));
        assert_eq!(map.get("R3"), Some("R1"));
        // A footprint whose path `parts.toml` does not name is no part of the original.
        assert_eq!(map.len(), 2);
        // A part the board does not have is left unmapped, for the check to report.
        let pcb = format!("(kicad_pcb {})", footprint("U1", Some("mcu")));
        let map = DesignatorMap::from_parts(&parts, &pcb).unwrap();
        assert_eq!((map.get("U7"), map.get("R3")), (Some("U1"), None));
        // A board with no member path on any footprint was not written by this `fab`.
        let pcb = format!(
            "(kicad_pcb {} {})",
            footprint("U1", None),
            footprint("R1", None)
        );
        let refused = DesignatorMap::from_parts(&parts, &pcb).unwrap_err();
        assert!(
            refused.contains("no `netsmith_path`") && refused.contains("netsmith fab"),
            "{refused}"
        );
    }
}
