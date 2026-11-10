//! The parity checker of the RP2350A Minimal port (`examples/rp2350_minimal/`): it compares a
//! `.kicad_pcb` written by `netsmith fab` with Raspberry Pi's board, as the reference table
//! (`reference/placements.toml`) states it, and says in which ways the two are interchangeable.
//!
//! What must match (`must_match = true` in the table): the outline and the mounting holes, and
//! the connectors, buttons and header pins: each one's place, rotation, layer, and every pad's
//! number, centre, size, drill and net. Everything else (the MCU, flash, crystal, regulator,
//! decoupling, the routing) is measured and reported with the verdict `info`, never `fail`.
//!
//! **The origin.** `fab` writes a board with an origin of its own, so no figure is compared as
//! the file states it. Each board is read from the **upper-left corner of its outline's bounding
//! box**: the box's smallest x and smallest y, in KiCad's coordinates, where y grows downward (so
//! that corner is the upper left as the board editor shows it). It is a corner of the box in both
//! files and needs no flip of an axis. A board moved as a whole passes; one whose parts moved
//! against its edge does not. The corner of the two boxes, as the files state them, is reported
//! as an `info` line.
//!
//! **Parts** are matched by reference through a map ([`DesignatorMap`], `designators.toml`):
//! Raspberry Pi's reference to the reference of the part in the board `fab` wrote. A part of the
//! table with no entry is `unmapped`: a failure when the table says it must match.
//!
//! **Nets** are compared by name (a leading `/` and the case are ignored) where both names are
//! plain, and through the connectivity otherwise: the pads of the mapped parts that share the
//! net of a pad must be the same in both boards. A pad whose names differ passes only when the
//! connectivity vouches for it (another mapped pad is on its net, in both boards), and says so
//! in a warning. Map U1 and the other parts, even those that need not match, so that the
//! connectivity has pads to see.

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

/// Raspberry Pi's references mapped to those of the board `fab` wrote.
#[derive(Debug, Clone, Default)]
pub struct DesignatorMap(BTreeMap<String, String>);

impl DesignatorMap {
    /// `designators.toml`: one line per part, `J1 = "J1"` (Raspberry Pi's reference, then the
    /// reference of the part in the `.kicad_pcb`).
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
    Ok(check(&reference, &actual, map, tolerances))
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

    let (rx, ry) = corner(r);
    let (ax, ay) = corner(a);
    let mut item =
        Item::new("outline origin".into(), "origin", "outline", false).at(ax - rx, ay - ry);
    item.verdict = Verdict::Info;
    items.push(item.noting(
        "the corners as the files state them: a board moved as a whole is not a difference",
    ));
}

fn hole_items(r: &Board, a: &Board, tol: &Tolerances, items: &mut Vec<Item>) {
    let (rc, ac) = (corner(r), corner(a));
    let rel = |h: &Hole, c: (f64, f64)| (h.x - c.0, h.y - c.1);
    let mut used = vec![false; a.holes.len()];
    for hole in &r.holes {
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
            .min_by(|p, q| p.1.total_cmp(&q.1));
        let id = format!("hole {}", hole.reference);
        let item = Item::new(id, "position", &hole.reference, true).within(tol.hole);
        let Some((i, _)) = nearest else {
            items.push(
                item.noting("the board has no hole to match this one")
                    .judge(false),
            );
            continue;
        };
        used[i] = true;
        let (x, y) = rel(&a.holes[i], ac);
        let (dx, dy) = (x - hx, y - hy);
        let dd = a.holes[i].drill - hole.drill;
        let mut item = item.at(dx, dy);
        item.dvalue = Some(dd);
        let good = dist(dx, dy) <= tol.hole + EPS && dd.abs() <= tol.hole_drill + EPS;
        items.push(
            item.noting(format!(
                "drill {} against {}{}",
                num(a.holes[i].drill),
                num(hole.drill),
                if dd.abs() > tol.hole_drill + EPS {
                    format!(" (tol {})", num(tol.hole_drill))
                } else {
                    String::new()
                }
            ))
            .judge(good),
        );
    }
    for (_, h) in a.holes.iter().enumerate().filter(|(i, _)| !used[*i]) {
        let (x, y) = rel(h, ac);
        let mut item = Item::new(
            format!("hole extra {}", h.reference),
            "position",
            &format!("extra {}", h.reference),
            false,
        );
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
    let hole_refs: BTreeSet<&str> = r.holes.iter().map(|h| h.reference.as_str()).collect();
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
    net_items(&pairs, items, warnings);
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

/// The pads of two parts paired by number and, among equal numbers, by the nearest centre.
fn pad_items(r: &Part, a: &Part, tol: &Tolerances, items: &mut Vec<Item>) {
    let must = r.must_match;
    let id = format!("part {}", r.reference);
    let mut numbers: Vec<&str> = r
        .pads
        .iter()
        .chain(&a.pads)
        .map(|p| p.number.as_str())
        .collect();
    numbers.sort_by_key(|n| (natural_key(n), n.to_string()));
    numbers.dedup();
    let mut measured: Vec<(String, &Pad, &Pad)> = Vec::new();
    let mut lone: Vec<Item> = Vec::new();
    for number in numbers {
        let rp: Vec<&Pad> = r.pads.iter().filter(|p| p.number == number).collect();
        let mut ap: Vec<&Pad> = a.pads.iter().filter(|p| p.number == number).collect();
        let n = rp.len().max(ap.len());
        for (k, pad) in rp.iter().enumerate() {
            let label = pad_label(number, k, n);
            let nearest = ap
                .iter()
                .enumerate()
                .map(|(i, q)| (i, dist(q.x - pad.x, q.y - pad.y)))
                .min_by(|p, q| p.1.total_cmp(&q.1));
            match nearest {
                Some((i, _)) => {
                    let q = ap.remove(i);
                    measured.push((label, pad, q));
                }
                None => lone.push(
                    Item::new(format!("{id} pad {label}"), "pad", &r.reference, must)
                        .noting("the board's part has no such pad")
                        .judge(false),
                ),
            }
        }
        for (k, _) in ap.iter().enumerate() {
            let label = pad_label(number, rp.len() + k, n);
            lone.push(
                Item::new(format!("{id} pad {label}"), "pad", &r.reference, must)
                    .noting("the board's part has a pad that the reference does not have")
                    .judge(false),
            );
        }
    }

    let judge_pair = |pad: &Pad, q: &Pad| -> PadMeasure {
        let (dx, dy) = (q.x - pad.x, q.y - pad.y);
        let dsize = match (pad.size, q.size) {
            (Some((w, h)), Some((qw, qh))) => Some((qw - w, qh - h)),
            _ => None,
        };
        let drill_of = |p: &Pad| p.drill.or_else(|| p.drill_slot.map(|(w, h)| w.min(h)));
        let ddrill = match (drill_of(pad), drill_of(q)) {
            (Some(d), Some(e)) => Some(e - d),
            _ => None,
        };
        let mut why = Vec::new();
        if dist(dx, dy) > tol.pad_position + EPS {
            why.push("centre".to_string());
        }
        if dsize.is_some_and(|(w, h)| w.abs() > tol.pad_size + EPS || h.abs() > tol.pad_size + EPS)
            || pad.size.is_some() != q.size.is_some()
        {
            why.push("size".to_string());
        }
        if ddrill.is_some_and(|d| d.abs() > tol.pad_size + EPS)
            || pad.drill.is_some() != q.drill.is_some()
            || pad.drill_slot.is_some() != q.drill_slot.is_some()
        {
            why.push("drill".to_string());
        }
        if pad.kind != q.kind {
            why.push(format!("type ({} against {})", q.kind, pad.kind));
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
    };

    if must {
        for (label, pad, q) in &measured {
            let PadMeasure {
                dx,
                dy,
                dsize,
                ddrill,
                good,
                why,
            } = judge_pair(pad, q);
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
        for (label, pad, q) in &measured {
            let PadMeasure { dx, dy, good, .. } = judge_pair(pad, q);
            if !good {
                bad.push(label.as_str());
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
            item.note = format!("{} pads, the largest offset on pad {label}", measured.len());
        }
        if !bad.is_empty() || missing > 0 {
            item.note = format!(
                "{} pads differ in centre, size or drill{}; {}",
                bad.len(),
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

type Key = (String, String);

/// The pads of each net, as `(reference of Raspberry Pi's part, pad number)`.
fn nets_of<'a>(
    parts: impl Iterator<Item = (&'a str, &'a Part)>,
) -> (BTreeMap<String, BTreeSet<Key>>, BTreeMap<Key, Vec<String>>) {
    let mut by_net: BTreeMap<String, BTreeSet<Key>> = BTreeMap::new();
    let mut by_pad: BTreeMap<Key, Vec<String>> = BTreeMap::new();
    for (reference, part) in parts {
        for pad in part.pads.iter().filter(|p| p.kind != "np_thru_hole") {
            let key = (reference.to_string(), pad.number.clone());
            let entry = by_pad.entry(key.clone()).or_default();
            if let Some(net) = &pad.net {
                by_net.entry(net.clone()).or_default().insert(key);
                entry.push(net.clone());
            }
        }
    }
    (by_net, by_pad)
}

fn net_items(pairs: &[(&Part, &Part)], items: &mut Vec<Item>, warnings: &mut Vec<String>) {
    let (ref_by_net, ref_by_pad) = nets_of(pairs.iter().map(|(r, _)| (r.reference.as_str(), *r)));
    let (act_by_net, act_by_pad) = nets_of(pairs.iter().map(|(r, a)| (r.reference.as_str(), *a)));
    // The pads that share a net with `key`, other than `key` itself.
    let class = |by_net: &BTreeMap<String, BTreeSet<Key>>,
                 by_pad: &BTreeMap<Key, Vec<String>>,
                 key: &Key|
     -> BTreeSet<Key> {
        let mut set = BTreeSet::new();
        for net in by_pad.get(key).into_iter().flatten() {
            set.extend(by_net[net].iter().cloned());
        }
        set.remove(key);
        set
    };
    let show = |set: &BTreeSet<Key>| -> String {
        if set.is_empty() {
            "no other mapped pad".to_string()
        } else {
            set.iter()
                .map(|(r, n)| format!("{r}.{n}"))
                .collect::<Vec<_>>()
                .join(" ")
        }
    };
    let net_name = |by_pad: &BTreeMap<Key, Vec<String>>, key: &Key| -> Option<String> {
        by_pad.get(key).and_then(|v| v.first()).cloned()
    };

    for (part, _) in pairs.iter().filter(|(p, _)| p.must_match) {
        let mut seen = BTreeSet::new();
        for pad in part.pads.iter().filter(|p| p.kind != "np_thru_hole") {
            let key: Key = (part.reference.clone(), pad.number.clone());
            if !seen.insert(key.clone()) || !act_by_pad.contains_key(&key) {
                continue;
            }
            let (rn, an) = (net_name(&ref_by_pad, &key), net_name(&act_by_pad, &key));
            let (rc, ac) = (
                class(&ref_by_net, &ref_by_pad, &key),
                class(&act_by_net, &act_by_pad, &key),
            );
            let id = format!("part {} pad {} net", part.reference, pad.number);
            let item = Item::new(id, "net", &part.reference, true);
            let named = |n: &Option<String>| n.clone().unwrap_or_else(|| "(none)".to_string());
            let same_class = rc == ac;
            let (good, note) = match (rn.as_deref().and_then(plain), an.as_deref().and_then(plain))
            {
                (Some(r), Some(a)) if r == a => (true, named(&rn)),
                (Some(_), Some(_)) if same_class && !rc.is_empty() => {
                    warnings.push(format!(
                        "part {} pad {}: the net is {} on the board and {} in the reference; \
                         the same pads are on it ({}), so it passes",
                        part.reference,
                        pad.number,
                        named(&an),
                        named(&rn),
                        show(&rc)
                    ));
                    (
                        true,
                        format!("{} against {}, same pads", named(&an), named(&rn)),
                    )
                }
                (Some(_), Some(_)) if same_class => (
                    false,
                    format!(
                        "{} against {}, and no other mapped pad is on the net to vouch for it \
                         (map the part that the net goes to)",
                        named(&an),
                        named(&rn)
                    ),
                ),
                (Some(_), Some(_)) => (
                    false,
                    format!(
                        "{} against {}: the board has {}, the reference {}",
                        named(&an),
                        named(&rn),
                        show(&ac),
                        show(&rc)
                    ),
                ),
                (r, a) if same_class => {
                    if r.is_some() != a.is_some() {
                        warnings.push(format!(
                            "part {} pad {}: the net is {} on the board and {} in the reference; \
                             one is a made-up name, so the pads on it decide",
                            part.reference,
                            pad.number,
                            named(&an),
                            named(&rn)
                        ));
                    }
                    (true, format!("same pads ({})", show(&rc)))
                }
                _ => (
                    false,
                    format!(
                        "{} against {}: the board has {}, the reference {}",
                        named(&an),
                        named(&rn),
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
    let legend = 8.0;
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
        "<title>Parity overlay: Raspberry Pi's board (blue) and netsmith's (orange)</title>"
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
        for h in &board.holes {
            let _ = writeln!(
                s,
                "<circle class=\"hole\" cx=\"{}\" cy=\"{}\" r=\"{}\"/>",
                num(h.x - c.0),
                num(h.y - c.1),
                num(h.drill / 2.0)
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
        "<g class=\"legend\" stroke=\"none\"><text x=\"0\" y=\"{}\" fill=\"{BLUE}\">blue: Raspberry Pi's board</text>\
         <text x=\"16\" y=\"{}\" fill=\"{ORANGE}\">orange: netsmith's board</text>\
         <text x=\"34\" y=\"{}\" fill=\"{RED}\">red: a must-match item that fails</text>\
         <text x=\"0\" y=\"{}\" fill=\"#4a5568\">both from the upper-left corner of the outline's box, 1 unit = 1 mm</text></g>",
        num(ly),
        num(ly),
        num(ly),
        num(ly + 2.0)
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
