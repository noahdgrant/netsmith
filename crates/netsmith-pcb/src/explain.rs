//! Which lines of the physical file are not met, and which cannot all hold, when the placer
//! ends without an arrangement that meets them (ADR 0045, PROVISIONAL).
//!
//! [`explain`] reads the arrangement the placer ended on and the design's requirements, and
//! gives two lists:
//!
//! - **unmet**: every requirement the arrangement does not meet (a field of a part's `.place`
//!   outside its window, a face, a relation between two parts), each with the line that
//!   states it, the text of the line, the part, the measured value, the window and how far
//!   out;
//! - **conflicts**: sets of lines that cannot all hold, which is only said where it is
//!   proved from what the requirements leave: the windows of origins [`constraints::plan`]
//!   works out for each part, the keepouts, cutouts and height limits of the design, and the
//!   outline. Nothing is guessed: a conflict that cannot be proved is not claimed, and the
//!   unmet lines are listed without a cause.
//!
//! A conflict is one of: a relation whose window the windows of its two parts leave no way
//! to meet; two parts whose courtyards overlap wherever their windows put them; a part whose
//! courtyard overlaps a keepout or cutout wherever its window puts it; a part with a stated
//! face and a height that a limit on that face rules out wherever its window puts it; a
//! window that has no place inside the outline. Each is first tried on the lines alone, and
//! then with the outline and its margin, which bound a window the lines leave open (a
//! `>=` has no upper end of its own); the second form says so.

use std::collections::BTreeSet;

use netsmith_core::{Design, PlaceField};

use crate::board::{Placed, Side};
use crate::constraints::{
    self, Plan, Reach, TOLERANCE, axis_of, body_at, measured, miss, number, offset, quarter,
    range_text, requirements, turns_allowed, what,
};
use crate::geometry::{Pt, Rect};
use crate::place::{ORIGIN, PlaceOptions};
use crate::rules::Rules;

/// The least overlap, in mm, on each axis, that counts as an overlap proved for a window
/// (a courtyard that only touches does not).
const DEPTH: f64 = 1e-3;
/// How much taller than a limit a part must be to be ruled out by it: the board check's.
const HEIGHT_TOLERANCE: f64 = 0.0005;

/// What the requirements of the physical file did not get from the placer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RequirementReport {
    /// Sets of lines that cannot all hold, each as a sentence with the numbers.
    pub conflicts: Vec<String>,
    /// Every requirement not met where the placer ended, each as a sentence with the line
    /// (`file:line` and its text), the part, the measured value, the window and how far out.
    pub unmet: Vec<String>,
}

impl RequirementReport {
    /// Whether there is nothing to say.
    pub fn is_empty(&self) -> bool {
        self.conflicts.is_empty() && self.unmet.is_empty()
    }

    /// The report as lines, each starting with `indent`.
    pub fn render(&self, indent: &str) -> String {
        let mut lines: Vec<String> = Vec::new();
        if self.conflicts.is_empty() {
            lines.push(format!(
                "{} requirement(s) of the physical file are not met where the placer ended; no two lines could be proved to conflict, so no cause is named:",
                self.unmet.len()
            ));
        } else {
            lines.push("no placement meets every requirement of the physical file:".to_string());
            for c in &self.conflicts {
                lines.push(format!("  these lines cannot both hold: {c}"));
            }
            if !self.unmet.is_empty() {
                lines.push(format!(
                    "  where the placer ended, {} requirement(s) are not met:",
                    self.unmet.len()
                ));
            }
        }
        for u in &self.unmet {
            lines.push(format!("    {u}"));
        }
        lines
            .iter()
            .map(|l| format!("{indent}{l}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl std::fmt::Display for RequirementReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.render(""))
    }
}

/// A line of the physical file as a person reads it: `widget.physical.nsm:16 `.rf.place.right
/// >= 59mm``. Where the design does not know the file, the requirement's own name.
fn quote(design: &Design, source: &str, id: &str) -> String {
    if source.is_empty() {
        return format!("`{id}`");
    }
    source
        .split(", ")
        .map(|s| match design.features.line_texts.get(s) {
            Some(text) => format!("{s} `{text}`"),
            None => s.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" and ")
}

/// The face line of the part at `path`: its source and name.
fn side_source(design: &Design, path: &str) -> Option<(String, String)> {
    design
        .features
        .constraints
        .iter()
        .find(|p| p.path == path && p.side.is_some())
        .map(|p| (p.side_source.clone(), format!("{}.place.side", p.path)))
}

/// What the placer ended with, against the design's requirements. `parts` are where the
/// placer left them, `outline` the board's box (its top left corner is [`ORIGIN`]), `options`
/// what the layout file and the caller said (the locks, the faces) and `wall` the space the
/// placer keeps between a courtyard and the edge. `None` if the design states no requirement
/// or all are met.
pub(crate) fn explain(
    design: &Design,
    parts: &[Placed],
    outline: Rect,
    options: &PlaceOptions,
    rules: &Rules,
) -> Option<RequirementReport> {
    if design.features.constraints.is_empty() && design.features.offsets.is_empty() {
        return None;
    }
    let unmet = unmet(design, parts);
    let mut options = options.clone();
    options.wall = Some(rules.edge_wall());
    let conflicts = match constraints::plan(design, parts, &options, rules.clearance) {
        Ok(plan) => Context {
            design,
            parts,
            options: &options,
            plan: &plan,
            outline,
            wall: rules.edge_wall(),
        }
        .conflicts(),
        // Requirements the plan itself refuses are reported by it, before placing.
        Err(_) => Vec::new(),
    };
    let report = RequirementReport { conflicts, unmet };
    (!report.is_empty()).then_some(report)
}

/// Every requirement the arrangement does not meet.
fn unmet(design: &Design, parts: &[Placed]) -> Vec<String> {
    let mut found = Vec::new();
    for part in parts {
        let (reqs, side_req) = requirements(design, &part.path);
        let body = part.body();
        let drawing = format!(
            "{}.{}",
            if part.side == Side::Back { "B" } else { "F" },
            if part.footprint.body_is_courtyard {
                "CrtYd"
            } else {
                "Fab"
            }
        );
        let name = format!("{} (`{}`)", part.refdes, part.path);
        let drawn = format!("{} (`{}`, {drawing})", part.refdes, part.path);
        for r in &reqs {
            let m = miss(r, body, part.rotation);
            if m <= TOLERANCE {
                continue;
            }
            let line = quote(design, &r.source, &r.id);
            let window = range_text(r.lo, r.hi, r.field.unit());
            if r.field == PlaceField::Rotation {
                found.push(format!(
                    "{line}: {name} is turned {} degrees, and it allows {window} ({} degrees out)",
                    number(part.rotation.rem_euclid(360.0)),
                    number(m)
                ));
            } else {
                found.push(format!(
                    "{line}: {} of the body of {drawn} is {} mm, and it allows {window} ({} mm out)",
                    what(r.field),
                    number(measured(r.field, body)),
                    number(m)
                ));
            }
        }
        if let Some((side, _)) = &side_req
            && (side == "bottom") != (part.side == Side::Back)
        {
            let (source, id) = side_source(design, &part.path).unwrap_or_default();
            found.push(format!(
                "{}: {name} is on the {} face, and it allows \"{side}\"",
                quote(design, &source, &id),
                part.side.word()
            ));
        }
    }
    for o in &design.features.offsets {
        let (Some(a), Some(b)) = (
            parts.iter().find(|p| p.path == o.a.path),
            parts.iter().find(|p| p.path == o.b.path),
        ) else {
            continue;
        };
        let v = offset(o.b.field, b.body()) - offset(o.a.field, a.body());
        let axis = axis_of(o.a.field).unwrap_or(0);
        let m = (o.lo - v).max(v - o.hi).max(0.0);
        if m <= TOLERANCE {
            continue;
        }
        let id = format!(
            "{}.place.{} - {}.place.{}",
            o.b.path,
            o.b.field.word(),
            o.a.path,
            o.a.field.word()
        );
        found.push(format!(
            "{}: {} of the body of {} (`{}`) minus {} of the body of {} (`{}`) is {} mm along {}, and it allows {} ({} mm out)",
            quote(design, &o.source, &id),
            what(o.b.field),
            b.refdes,
            b.path,
            what(o.a.field),
            a.refdes,
            a.path,
            number(v),
            if axis == 0 { "x" } else { "y" },
            range_text(o.lo, o.hi, "mm"),
            number(m)
        ));
    }
    found
}

/// A part's courtyard and body in one turn, and where its origin may be.
#[derive(Debug, Clone, Copy)]
struct Quarter {
    /// The turn, in degrees.
    angle: f64,
    court: Rect,
    body: Rect,
    /// The range of the origin on the sheet, along x and along y; infinite where nothing
    /// bounds it.
    x: (f64, f64),
    y: (f64, f64),
}

/// What the requirements leave of the places of one part.
struct Win<'a> {
    part: &'a Placed,
    side: Side,
    quarters: Vec<Quarter>,
    /// The lines that bound its place or turn, quoted, and the layout file's lock if any.
    lines: Vec<String>,
    /// The line that states its face, quoted.
    side_line: Option<String>,
    /// Whether it may hang over an edge: its window is not bounded by the outline.
    may_hang: bool,
    /// Whether the clip by the outline left it no place at all.
    clipped_out: bool,
}

struct Context<'a> {
    design: &'a Design,
    parts: &'a [Placed],
    options: &'a PlaceOptions,
    plan: &'a Plan,
    outline: Rect,
    wall: f64,
}

/// The least overlap of two intervals `a` and `b` (each at their origin) when the origins are
/// anywhere in `ra` and `rb`, or `None` if they do not overlap for every position.
fn depth(a: (f64, f64), ra: (f64, f64), b: (f64, f64), rb: (f64, f64)) -> Option<f64> {
    if ![ra.0, ra.1, rb.0, rb.1].iter().all(|v| v.is_finite()) {
        return None;
    }
    let mut least = f64::INFINITY;
    for ta in [ra.0, ra.1] {
        for tb in [rb.0, rb.1] {
            let d = (a.1 + ta).min(b.1 + tb) - (a.0 + ta).max(b.0 + tb);
            least = least.min(d);
        }
    }
    (least > DEPTH).then_some(least)
}

/// How much two boxes overlap on each axis for every position of their origins, or `None`.
fn always_overlap(
    a: &Rect,
    (ax, ay): ((f64, f64), (f64, f64)),
    b: &Rect,
    (bx, by): ((f64, f64), (f64, f64)),
) -> Option<(f64, f64)> {
    let x = depth((a.min.x, a.max.x), ax, (b.min.x, b.max.x), bx)?;
    let y = depth((a.min.y, a.max.y), ay, (b.min.y, b.max.y), by)?;
    Some((x, y))
}

fn dedupe(lines: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in lines {
        if !out.contains(&l) {
            out.push(l);
        }
    }
    out
}

impl<'a> Context<'a> {
    /// What each part's requirements leave, for the parts that have any (or that a relation
    /// names, or the layout file locks). With `clip`, the outline and its margin bound the
    /// window of a part that may not hang over an edge.
    fn windows(&self, clip: bool) -> Vec<Win<'a>> {
        let mut wins = Vec::new();
        for part in self.parts {
            let (reqs, side_req) = requirements(self.design, &part.path);
            let locked = self.options.fixed.get(&part.refdes);
            let in_relation = self
                .design
                .features
                .offsets
                .iter()
                .any(|o| o.a.path == part.path || o.b.path == part.path);
            if reqs.is_empty() && side_req.is_none() && locked.is_none() && !in_relation {
                continue;
            }
            let pinned = locked.or_else(|| self.plan.fixed.get(&part.refdes));
            let side = match pinned {
                Some(f) => f.side,
                None => self
                    .plan
                    .sides
                    .get(&part.refdes)
                    .or_else(|| self.options.sides.get(&part.refdes))
                    .copied()
                    .unwrap_or(Side::Front),
            };
            let may_hang = self.plan.overhang.contains_key(&part.refdes)
                || self.options.overhang.contains_key(&part.refdes);
            let make = |q: usize, x: (f64, f64), y: (f64, f64)| {
                let angle = q as f64 * 90.0;
                Quarter {
                    angle,
                    court: part
                        .footprint
                        .courtyard
                        .rotated_for(side == Side::Back, angle),
                    body: body_at(part, angle, side),
                    x,
                    y,
                }
            };
            let mut quarters: Vec<Quarter> = Vec::new();
            if let Some(f) = pinned {
                if let Some(q) = quarter(f.rotation) {
                    let at = ORIGIN + f.at;
                    quarters.push(make(q, (at.x, at.x), (at.y, at.y)));
                }
            } else {
                let allowed = turns_allowed(&reqs);
                let reach: [Option<Reach>; 4] = match self.plan.wants.get(&part.refdes) {
                    Some(w) => w.reach,
                    None => std::array::from_fn(|q| {
                        allowed[q].then_some(Reach {
                            x: (f64::NEG_INFINITY, f64::INFINITY),
                            y: (f64::NEG_INFINITY, f64::INFINITY),
                        })
                    }),
                };
                for (q, r) in reach.iter().enumerate() {
                    if let Some(r) = r {
                        quarters.push(make(q, r.x, r.y));
                    }
                }
            }
            let mut clipped_out = false;
            if clip && pinned.is_none() && !may_hang && !quarters.is_empty() {
                let o = self.outline;
                let w = self.wall;
                quarters.retain_mut(|q| {
                    q.x = (
                        q.x.0.max(o.min.x + w - q.court.min.x),
                        q.x.1.min(o.max.x - w - q.court.max.x),
                    );
                    q.y = (
                        q.y.0.max(o.min.y + w - q.court.min.y),
                        q.y.1.min(o.max.y - w - q.court.max.y),
                    );
                    q.x.0 <= q.x.1 + TOLERANCE && q.y.0 <= q.y.1 + TOLERANCE
                });
                clipped_out = quarters.is_empty();
            }
            let mut lines: Vec<String> = reqs
                .iter()
                .map(|r| quote(self.design, &r.source, &r.id))
                .collect();
            if let Some(f) = locked {
                lines.push(format!(
                    "the layout file locks {} at ({}, {}) mm",
                    part.refdes,
                    number(f.at.x),
                    number(f.at.y)
                ));
            }
            let side_line = side_req.as_ref().map(|_| {
                let (source, id) = side_source(self.design, &part.path).unwrap_or_default();
                quote(self.design, &source, &id)
            });
            wins.push(Win {
                part,
                side,
                quarters,
                lines: dedupe(lines),
                side_line,
                may_hang,
                clipped_out,
            });
        }
        wins
    }

    /// The box the courtyards of a window can cover, as text: `x 52.9 to 57.1, y 2.9 to 7.1`.
    fn span(w: &Win, body: bool) -> String {
        let (mut lo, mut hi) = (Pt::new(f64::MAX, f64::MAX), Pt::new(f64::MIN, f64::MIN));
        for q in &w.quarters {
            let r = if body { q.body } else { q.court };
            lo = Pt::new(lo.x.min(r.min.x + q.x.0), lo.y.min(r.min.y + q.y.0));
            hi = Pt::new(hi.x.max(r.max.x + q.x.1), hi.y.max(r.max.y + q.y.1));
        }
        format!(
            "x {} to {}, y {} to {}",
            number(lo.x - ORIGIN.x),
            number(hi.x - ORIGIN.x),
            number(lo.y - ORIGIN.y),
            number(hi.y - ORIGIN.y)
        )
    }

    fn name(w: &Win) -> String {
        Self::name_of(w.part)
    }

    fn name_of(part: &Placed) -> String {
        format!("{} (`{}`)", part.refdes, part.path)
    }

    /// The lines of the windows, once each, joined.
    fn joined(wins: &[&Win]) -> String {
        dedupe(wins.iter().flat_map(|w| w.lines.iter().cloned())).join("; ")
    }

    fn outline_note(&self) -> String {
        format!(
            "; this holds only inside the outline of {} x {} mm, where the placer keeps every courtyard {} mm from the edge",
            number(self.outline.width()),
            number(self.outline.height()),
            number((self.wall * 100.0).round() / 100.0)
        )
    }

    fn conflicts(&self) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for clip in [false, true] {
            let wins = self.windows(clip);
            let note = if clip {
                self.outline_note()
            } else {
                String::new()
            };
            if !clip {
                self.relations_together(&mut seen, &mut found);
            }
            self.relations(&wins, &note, &mut seen, &mut found);
            self.pairs(&wins, &note, &mut seen, &mut found);
            self.areas(&wins, &note, &mut seen, &mut found);
            self.heights(&wins, &note, &mut seen, &mut found);
            if clip {
                self.fits(&wins, &mut seen, &mut found);
            }
        }
        found
    }

    /// Two relations between the same two parts, of the same fields, whose windows have no
    /// value in common: no places of the parts hold both.
    fn relations_together(&self, seen: &mut BTreeSet<String>, found: &mut Vec<String>) {
        let offsets = &self.design.features.offsets;
        let id = |o: &netsmith_core::Offset| {
            format!(
                "{}.place.{} - {}.place.{}",
                o.b.path,
                o.b.field.word(),
                o.a.path,
                o.a.field.word()
            )
        };
        for (i, p) in offsets.iter().enumerate() {
            for (k, q) in offsets.iter().enumerate().skip(i + 1) {
                let key = format!("relations:{i}:{k}");
                // `q` as it reads with the ends of `p`: swapped ends negate the window.
                let window = if (&q.a, &q.b) == (&p.a, &p.b) {
                    (q.lo, q.hi)
                } else if (&q.a, &q.b) == (&p.b, &p.a) {
                    (-q.hi, -q.lo)
                } else {
                    continue;
                };
                let gap = (p.lo - window.1).max(window.0 - p.hi);
                if seen.contains(&key) || gap.is_nan() || gap <= TOLERANCE {
                    continue;
                }
                seen.insert(key);
                let (Some(a), Some(b)) = (
                    self.parts.iter().find(|x| x.path == p.a.path),
                    self.parts.iter().find(|x| x.path == p.b.path),
                ) else {
                    continue;
                };
                found.push(format!(
                    "{}; {}: they ask for {} of the body of {} minus {} of the body of {} to be {} and {}, and no value is in both ({} mm apart)",
                    quote(self.design, &p.source, &id(p)),
                    quote(self.design, &q.source, &id(q)),
                    what(p.b.field),
                    Self::name_of(b),
                    what(p.a.field),
                    Self::name_of(a),
                    range_text(p.lo, p.hi, "mm"),
                    range_text(window.0, window.1, "mm"),
                    number(gap)
                ));
            }
        }
    }

    /// A relation whose window the windows of its two parts leave no way to meet.
    fn relations(
        &self,
        wins: &[Win],
        note: &str,
        seen: &mut BTreeSet<String>,
        found: &mut Vec<String>,
    ) {
        for (i, o) in self.design.features.offsets.iter().enumerate() {
            let key = format!("relation:{i}");
            if seen.contains(&key) {
                continue;
            }
            let (Some(a), Some(b)) = (
                wins.iter().find(|w| w.part.path == o.a.path),
                wins.iter().find(|w| w.part.path == o.b.path),
            ) else {
                continue;
            };
            if a.quarters.is_empty() || b.quarters.is_empty() {
                continue;
            }
            let axis = axis_of(o.a.field).unwrap_or(0);
            let reach = |w: &Win, field: PlaceField| {
                let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                for q in &w.quarters {
                    let t = if axis == 0 { q.x } else { q.y };
                    let off = offset(field, q.body);
                    lo = lo.min(t.0 + off);
                    hi = hi.max(t.1 + off);
                }
                (lo, hi)
            };
            let (va, vb) = (reach(a, o.a.field), reach(b, o.b.field));
            let diff = (vb.0 - va.1, vb.1 - va.0);
            let out = (o.lo - diff.1).max(diff.0 - o.hi);
            if diff.0.is_nan() || diff.1.is_nan() || out <= TOLERANCE {
                continue;
            }
            seen.insert(key);
            let id = format!(
                "{}.place.{} - {}.place.{}",
                o.b.path,
                o.b.field.word(),
                o.a.path,
                o.a.field.word()
            );
            let others = Self::joined(&[a, b]);
            found.push(format!(
                "{}{}: {} of the body of {} minus {} of the body of {} must be {} along {}, and the places the other lines leave them make it {} ({} mm out){note}",
                quote(self.design, &o.source, &id),
                if others.is_empty() {
                    String::new()
                } else {
                    format!("; {others}")
                },
                what(o.b.field),
                Self::name(b),
                what(o.a.field),
                Self::name(a),
                range_text(o.lo, o.hi, "mm"),
                if axis == 0 { "x" } else { "y" },
                range_text(diff.0, diff.1, "mm"),
                number(out)
            ));
        }
    }

    /// Whether the two windows take room in the same place on a face for every turn.
    fn takes_same_room(a: &Win, b: &Win, qa: &Quarter, qb: &Quarter) -> bool {
        [Side::Front, Side::Back].into_iter().any(|face| {
            let room = |w: &Win, q: &Quarter| {
                let mut p = w.part.clone();
                p.side = w.side;
                let r = p.room_on(face, 0.0, q.court);
                r.len() == 1 && r[0] == q.court
            };
            room(a, qa) && room(b, qb)
        })
    }

    /// Two parts whose courtyards overlap wherever their windows put them.
    fn pairs(
        &self,
        wins: &[Win],
        note: &str,
        seen: &mut BTreeSet<String>,
        found: &mut Vec<String>,
    ) {
        for (i, a) in wins.iter().enumerate() {
            for b in &wins[i + 1..] {
                let key = format!("pair:{}:{}", a.part.refdes, b.part.refdes);
                if seen.contains(&key)
                    || a.lines.is_empty()
                    || b.lines.is_empty()
                    || a.quarters.is_empty()
                    || b.quarters.is_empty()
                {
                    continue;
                }
                let mut least = (f64::INFINITY, f64::INFINITY);
                let mut all = true;
                for qa in &a.quarters {
                    for qb in &b.quarters {
                        match always_overlap(&qa.court, (qa.x, qa.y), &qb.court, (qb.x, qb.y)) {
                            Some((dx, dy)) if Self::takes_same_room(a, b, qa, qb) => {
                                least = (least.0.min(dx), least.1.min(dy));
                            }
                            _ => all = false,
                        }
                    }
                }
                if !all {
                    continue;
                }
                seen.insert(key);
                found.push(format!(
                    "{}: they leave the courtyard of {} within {} mm and that of {} within {} mm, and the two overlap by at least {} x {} mm wherever they are put{note}",
                    Self::joined(&[a, b]),
                    Self::name(a),
                    Self::span(a, false),
                    Self::name(b),
                    Self::span(b, false),
                    number(least.0),
                    number(least.1)
                ));
            }
        }
    }

    /// The keepouts and cutouts that forbid parts, named, with the box each covers.
    fn forbidden(&self) -> Vec<(String, String, Rect)> {
        let rect = |at: (f64, f64), size: (f64, f64)| {
            let min = ORIGIN + Pt::new(at.0, at.1);
            Rect::new(min, Pt::new(min.x + size.0, min.y + size.1))
        };
        let mut areas = Vec::new();
        for c in &self.design.features.cutouts {
            if let Some(at) = c.at {
                areas.push(("cutout".to_string(), c.name.clone(), rect(at, c.size)));
            }
        }
        for k in &self.design.features.keepouts {
            if let (true, None, Some(at)) = (k.parts, &k.part, k.at) {
                areas.push(("keepout".to_string(), k.name.clone(), rect(at, k.size)));
            }
        }
        areas
    }

    /// A part whose courtyard overlaps a keepout or a cutout wherever its window puts it.
    fn areas(
        &self,
        wins: &[Win],
        note: &str,
        seen: &mut BTreeSet<String>,
        found: &mut Vec<String>,
    ) {
        for w in wins {
            if w.lines.is_empty() || w.quarters.is_empty() {
                continue;
            }
            for (kind, name, area) in self.forbidden() {
                let key = format!("area:{}:{name}", w.part.refdes);
                if seen.contains(&key) {
                    continue;
                }
                let mut least = (f64::INFINITY, f64::INFINITY);
                let mut all = true;
                for q in &w.quarters {
                    match always_overlap(&q.court, (q.x, q.y), &area, ((0.0, 0.0), (0.0, 0.0))) {
                        Some((dx, dy)) => least = (least.0.min(dx), least.1.min(dy)),
                        None => all = false,
                    }
                }
                if !all {
                    continue;
                }
                seen.insert(key);
                found.push(format!(
                    "{}: they leave the courtyard of {} within {} mm, and it overlaps the {kind} `{name}` (x {} to {}, y {} to {} mm, which forbids parts) by at least {} x {} mm wherever it is put{note}",
                    Self::joined(&[w]),
                    Self::name(w),
                    Self::span(w, false),
                    number(area.min.x - ORIGIN.x),
                    number(area.max.x - ORIGIN.x),
                    number(area.min.y - ORIGIN.y),
                    number(area.max.y - ORIGIN.y),
                    number(least.0),
                    number(least.1)
                ));
            }
        }
    }

    /// A part with a stated face and a stated height that a height limit on that face rules
    /// out wherever its window puts it.
    fn heights(
        &self,
        wins: &[Win],
        note: &str,
        seen: &mut BTreeSet<String>,
        found: &mut Vec<String>,
    ) {
        for w in wins {
            let (Some(side_line), Some(height)) = (&w.side_line, w.part.height) else {
                continue;
            };
            if w.quarters.is_empty() {
                continue;
            }
            for k in &self.design.features.keepouts {
                let (Some(limit), None, Some(at)) = (k.max_height, &k.part, k.at) else {
                    continue;
                };
                let face = if w.side == Side::Back {
                    "bottom"
                } else {
                    "top"
                };
                let key = format!("height:{}:{}", w.part.refdes, k.name);
                if seen.contains(&key)
                    || !k.layers.iter().any(|l| l == face)
                    || height <= limit + HEIGHT_TOLERANCE
                {
                    continue;
                }
                let min = ORIGIN + Pt::new(at.0, at.1);
                let zone = Rect::new(min, Pt::new(min.x + k.size.0, min.y + k.size.1));
                let mut least = (f64::INFINITY, f64::INFINITY);
                let mut all = true;
                for q in &w.quarters {
                    match always_overlap(&q.body, (q.x, q.y), &zone, ((0.0, 0.0), (0.0, 0.0))) {
                        Some((dx, dy)) => least = (least.0.min(dx), least.1.min(dy)),
                        None => all = false,
                    }
                }
                if !all {
                    continue;
                }
                seen.insert(key);
                let mut lines = w.lines.clone();
                lines.push(side_line.clone());
                found.push(format!(
                    "{}: they keep {} on the {face} face with its body within {} mm, which is under the keepout `{}` (x {} to {}, y {} to {} mm) wherever it is put, and that keepout allows {} mm on the {face} face while {} stands {} mm tall{note}",
                    dedupe(lines).join("; "),
                    Self::name(w),
                    Self::span(w, true),
                    k.name,
                    number(zone.min.x - ORIGIN.x),
                    number(zone.max.x - ORIGIN.x),
                    number(zone.min.y - ORIGIN.y),
                    number(zone.max.y - ORIGIN.y),
                    number(limit),
                    w.part.refdes,
                    number(height)
                ));
            }
        }
    }

    /// A window that has no place inside the outline and its margin.
    fn fits(&self, wins: &[Win], seen: &mut BTreeSet<String>, found: &mut Vec<String>) {
        for w in wins {
            let key = format!("fit:{}", w.part.refdes);
            if !w.clipped_out || w.may_hang || w.lines.is_empty() || seen.contains(&key) {
                continue;
            }
            // What the lines leave, turn by turn, and what the outline leaves.
            let open = self
                .windows(false)
                .into_iter()
                .find(|o| o.part.refdes == w.part.refdes);
            // The turns the part fits the outline in at all: a part too big for it in every
            // turn is the outline's fault and not the lines', and the placer says so.
            let o = self.outline;
            let room = |q: &Quarter| {
                (
                    (
                        o.min.x + self.wall - q.court.min.x,
                        o.max.x - self.wall - q.court.max.x,
                    ),
                    (
                        o.min.y + self.wall - q.court.min.y,
                        o.max.y - self.wall - q.court.max.y,
                    ),
                )
            };
            let fits: Vec<&Quarter> = open
                .iter()
                .flat_map(|o| o.quarters.iter())
                .filter(|q| {
                    let (x, y) = room(q);
                    x.0 <= x.1 && y.0 <= y.1
                })
                .collect();
            if fits.is_empty() {
                continue;
            }
            let along = |lo: f64, hi: f64, origin: f64| {
                range_text(lo - origin, hi - origin, "mm").replace(" mm", "")
            };
            let mut by_text: Vec<(String, Vec<String>)> = Vec::new();
            for q in fits {
                let (x, y) = room(q);
                let text = format!(
                    "the lines leave the origin of its footprint in x {}, y {} mm and the outline leaves it in x {}, y {} mm",
                    along(q.x.0, q.x.1, ORIGIN.x),
                    along(q.y.0, q.y.1, ORIGIN.y),
                    along(x.0, x.1, ORIGIN.x),
                    along(y.0, y.1, ORIGIN.y),
                );
                match by_text.iter_mut().find(|(t, _)| *t == text) {
                    Some((_, angles)) => angles.push(number(q.angle)),
                    None => by_text.push((text, vec![number(q.angle)])),
                }
            }
            let turns: Vec<String> = by_text
                .iter()
                .map(|(text, angles)| {
                    if by_text.len() == 1 && angles.len() == 4 {
                        format!("in every turn {text}")
                    } else {
                        format!("turned {} degrees {text}", angles.join(" or "))
                    }
                })
                .collect();
            seen.insert(key);
            found.push(format!(
                "{}: no place meets them and keeps the courtyard of {} inside the outline of {} x {} mm and {} mm from its edge; {}",
                w.lines.join("; "),
                Self::name(w),
                number(o.width()),
                number(o.height()),
                number((self.wall * 100.0).round() / 100.0),
                turns.join("; ")
            ));
        }
    }
}
