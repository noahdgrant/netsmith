//! Requirements on where parts go (ADR 0045), turned into what the placer can obey.
//!
//! A design states, in the `.place` of a part or a module, ranges for the centre and the
//! edges of the part's body box, for its turn and for its face (`netsmith_core::Placement`).
//! Everything is measured in the frame of the layout file: the origin is the outline's top
//! left corner, x grows to the right and y downward, in millimetres. The body is the box of
//! the footprint's fabrication layer drawing, or its courtyard if it draws none
//! ([`crate::footprint::Footprint::body`]).
//!
//! [`plan`] runs before anything is placed. For each part it works out, for each of the four
//! turns the part may take, where the footprint's origin may be (a [`Want`]); a part whose
//! requirements leave one turn and one place is pinned, which is the same as a part the layout
//! file locks (`PlaceOptions::fixed`). A requirement that no turn can meet, a window outside
//! the outline, two pinned parts whose courtyards overlap and a locked part of the layout
//! file that breaks a requirement are errors there, each naming what conflicts and the
//! numbers. The annealer then keeps every other constrained part inside its [`Want`].
//!
//! A module's `.place` is the box of all its parts. The placer enforces the half of it that
//! keeps the module in: no part of it has its left or top edge below a lower bound of the
//! module's, or its right or bottom edge above an upper bound. The rest of a module's
//! requirements is measured by the check after placement ([`netsmith_fab::drc`]).

use std::collections::BTreeMap;

use netsmith_core::{Design, PlaceField, Placement};

use crate::board::{Placed, Side};
use crate::geometry::{Pt, Rect};
use crate::overhang::{self, Edge, Edges, Overhang};
use crate::place::{Fixed, ORIGIN, PlaceOptions};

/// The grid the placer puts origins on, in mm.
pub(crate) const GRID: f64 = 0.05;

/// The tolerance with which a position meets a window, in mm: half a micrometre, as the
/// design rules check measures (ADR 0037).
pub const TOLERANCE: f64 = 5e-4;

/// Where the origin of a part's footprint may be, in sheet coordinates, for one turn. An end
/// is infinite when nothing bounds it. A range narrower than the grid is one value (its
/// middle), which the placer sets exactly instead of on the grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reach {
    pub x: (f64, f64),
    pub y: (f64, f64),
}

/// What a part's requirements leave to the placer.
#[derive(Debug, Clone, PartialEq)]
pub struct Want {
    /// For the turns 0, 90, 180 and 270 degrees: where the origin may be, or `None` if the
    /// requirements rule that turn out.
    pub reach: [Option<Reach>; 4],
    /// Whether any requirement is about where the part is along x, or along y.
    pub holds_x: bool,
    pub holds_y: bool,
    /// Whether the requirements say, for some turn, that the part hangs over an edge of the
    /// outline (see [`crate::overhang`]).
    pub overhangs: bool,
}

/// The index of a right angle in [`Want::reach`], or `None` for any other angle.
pub fn quarter(angle: f64) -> Option<usize> {
    let a = angle.rem_euclid(360.0);
    let q = (a / 90.0).round();
    ((a - q * 90.0).abs() < 1e-6).then_some((q as usize) % 4)
}

impl Want {
    /// The nearest origin the requirements allow to `at`, for a part turned `angle`; `at`
    /// itself if the turn is not allowed.
    pub fn clamp(&self, angle: f64, at: Pt) -> Pt {
        match quarter(angle).and_then(|q| self.reach[q]) {
            Some(r) => Pt::new(
                at.x.clamp(r.x.0, r.x.1.max(r.x.0)),
                at.y.clamp(r.y.0, r.y.1.max(r.y.0)),
            ),
            None => at,
        }
    }

    /// The box that holds every origin the requirements allow, if it is bounded.
    pub fn extent(&self) -> Option<Rect> {
        let mut lo = Pt::new(f64::INFINITY, f64::INFINITY);
        let mut hi = Pt::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for r in self.reach.iter().flatten() {
            lo = Pt::new(lo.x.min(r.x.0), lo.y.min(r.y.0));
            hi = Pt::new(hi.x.max(r.x.1), hi.y.max(r.y.1));
        }
        (lo.x.is_finite() && lo.y.is_finite() && hi.x.is_finite() && hi.y.is_finite())
            .then(|| Rect::new(lo, hi))
    }
}

/// What [`plan`] decided.
#[derive(Debug, Default)]
pub(crate) struct Plan {
    /// Parts whose requirements leave one place and one turn: they stay there.
    pub fixed: BTreeMap<String, Fixed>,
    /// The other constrained parts, which the annealer keeps inside their wants.
    pub wants: BTreeMap<String, Want>,
    /// The face of every constrained part, which the placer does not change.
    pub sides: BTreeMap<String, Side>,
    /// The parts whose requirements put an edge of their body at or past an edge of the
    /// outline, which makes the courtyard past it intended.
    pub overhang: BTreeMap<String, Overhang>,
    /// The relations between two parts, which the annealer keeps and the check measures.
    pub offsets: Vec<PlanOffset>,
}

/// A relation between two parts' places for the placer (ADR 0045, section 3): the measure of
/// the field `fb` of part `b`'s body minus that of the field `fa` of part `a`'s, along one
/// axis, is in `lo..=hi`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanOffset {
    /// The reference designators of the two parts, and their paths in the design.
    pub a: String,
    pub b: String,
    pub a_path: String,
    pub b_path: String,
    pub fa: PlaceField,
    pub fb: PlaceField,
    pub lo: f64,
    pub hi: f64,
    /// `radio.row_b.place.x - radio.row_a.place.x (radio.physical.nsm:12)`.
    pub label: String,
}

impl PlanOffset {
    /// Whether the relation is along x (else along y).
    pub fn axis(&self) -> usize {
        axis_of(self.fa).unwrap_or(0)
    }

    /// How far `value` (`b` minus `a`) is from the window, in mm: 0 when it is in it.
    pub fn miss(&self, value: f64) -> f64 {
        (self.lo - value).max(value - self.hi).max(0.0)
    }

    /// The relation and its window, as a person reads them.
    pub fn says(&self) -> String {
        format!(
            "`{}` allows {}",
            self.label,
            range_text(self.lo, self.hi, "mm")
        )
    }
}

/// One requirement on one part, with where it comes from.
struct Req {
    field: PlaceField,
    lo: f64,
    hi: f64,
    /// Whether a module the part is in states it for all its parts.
    inherited: bool,
    /// `usb.place.left (widget.nsm:14)`
    label: String,
}

fn range_text(lo: f64, hi: f64, unit: &str) -> String {
    let n = |v: f64| format!("{}", (v * 1e4).round() / 1e4);
    match (lo.is_finite(), hi.is_finite()) {
        (true, true) if (hi - lo).abs() < 1e-9 => format!("{} {unit}", n(lo)),
        (true, true) => format!("{} to {} {unit}", n(lo), n(hi)),
        (true, false) => format!("at least {} {unit}", n(lo)),
        (false, true) => format!("at most {} {unit}", n(hi)),
        (false, false) => "anything".to_string(),
    }
}

impl Req {
    fn says(&self) -> String {
        format!(
            "`{}` allows {}",
            self.label,
            range_text(self.lo, self.hi, self.field.unit())
        )
    }
}

fn label(path: &str, field: &str, source: &str) -> String {
    if source.is_empty() {
        format!("{path}.place.{field}")
    } else {
        format!("{path}.place.{field} ({source})")
    }
}

/// The requirements that apply to the part at `path`: its own, and the ones of the modules
/// it is in that keep a part inside (a least `left` or `top`, a most `right` or `bottom`).
fn requirements(design: &Design, path: &str) -> (Vec<Req>, Option<(String, String)>) {
    let mut reqs = Vec::new();
    let mut side = None;
    for p in &design.features.constraints {
        let own = p.path == path;
        let inside = p.module && path.starts_with(&format!("{}.", p.path));
        if !own && !inside {
            continue;
        }
        for b in &p.bounds {
            let (lo, hi) = (b.lo, b.hi);
            let text = label(&p.path, b.field.word(), &b.source);
            if own {
                reqs.push(Req {
                    field: b.field,
                    lo,
                    hi,
                    inherited: false,
                    label: text,
                });
                continue;
            }
            let (lo, hi) = match b.field {
                PlaceField::Left | PlaceField::Top => (lo, f64::INFINITY),
                PlaceField::Right | PlaceField::Bottom => (f64::NEG_INFINITY, hi),
                _ => continue,
            };
            if lo.is_finite() || hi.is_finite() {
                reqs.push(Req {
                    field: b.field,
                    lo,
                    hi,
                    inherited: true,
                    label: format!("{text}, which holds for every part of the module"),
                });
            }
        }
        if own && let Some(s) = &p.side {
            side = Some((s.clone(), label(&p.path, "side", &p.side_source)));
        }
    }
    (reqs, side)
}

/// The body of a footprint with origin at 0, turned `angle`, on the face `side`.
fn body_at(part: &Placed, angle: f64, side: Side) -> Rect {
    part.footprint.body.rotated_for(side == Side::Back, angle)
}

/// How far a body at `at` is from meeting `req`, in mm: 0 when it does, else by how much it
/// misses. The measure of the field, in the frame of the outline.
fn miss(req: &Req, body: Rect, rotation: f64) -> f64 {
    let (v, scale) = match req.field {
        PlaceField::X => (body.centre().x - ORIGIN.x, 1.0),
        PlaceField::Y => (body.centre().y - ORIGIN.y, 1.0),
        PlaceField::Left => (body.min.x - ORIGIN.x, 1.0),
        PlaceField::Right => (body.max.x - ORIGIN.x, 1.0),
        PlaceField::Top => (body.min.y - ORIGIN.y, 1.0),
        PlaceField::Bottom => (body.max.y - ORIGIN.y, 1.0),
        PlaceField::Rotation => {
            // The nearest multiple of 360 degrees that lands in the window.
            let r = rotation.rem_euclid(360.0);
            let best = [r - 360.0, r, r + 360.0]
                .into_iter()
                .map(|t| (req.lo - t).max(t - req.hi).max(0.0))
                .fold(f64::INFINITY, f64::min);
            return best;
        }
    };
    (req.lo - v * scale).max(v * scale - req.hi).max(0.0)
}

/// The measure of the field of a body, for a message.
fn measured(field: PlaceField, body: Rect) -> f64 {
    match field {
        PlaceField::X => body.centre().x - ORIGIN.x,
        PlaceField::Y => body.centre().y - ORIGIN.y,
        PlaceField::Left => body.min.x - ORIGIN.x,
        PlaceField::Right => body.max.x - ORIGIN.x,
        PlaceField::Top => body.min.y - ORIGIN.y,
        PlaceField::Bottom => body.max.y - ORIGIN.y,
        PlaceField::Rotation => 0.0,
    }
}

pub(crate) fn axis_of(field: PlaceField) -> Option<usize> {
    match field {
        PlaceField::X | PlaceField::Left | PlaceField::Right => Some(0),
        PlaceField::Y | PlaceField::Top | PlaceField::Bottom => Some(1),
        PlaceField::Rotation => None,
    }
}

/// Where along `body`'s own box the field is measured, from the origin of the footprint.
pub(crate) fn offset(field: PlaceField, body: Rect) -> f64 {
    match field {
        PlaceField::X => body.centre().x,
        PlaceField::Y => body.centre().y,
        PlaceField::Left => body.min.x,
        PlaceField::Right => body.max.x,
        PlaceField::Top => body.min.y,
        PlaceField::Bottom => body.max.y,
        PlaceField::Rotation => 0.0,
    }
}

/// The interval of origins on one axis that meets every requirement on it, in sheet
/// coordinates, or `None` if there is none. An interval narrower than the grid is its middle.
fn interval(reqs: &[Req], axis: usize, body: Rect) -> Option<(f64, f64)> {
    let origin = if axis == 0 { ORIGIN.x } else { ORIGIN.y };
    let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for r in reqs.iter().filter(|r| axis_of(r.field) == Some(axis)) {
        let off = offset(r.field, body);
        lo = lo.max(origin + r.lo - off);
        hi = hi.min(origin + r.hi - off);
    }
    if lo > hi + TOLERANCE / 2.0 {
        return None;
    }
    if lo.is_finite() && hi.is_finite() && hi - lo < GRID {
        let m = ((lo + hi) / 2.0 * 1e6).round() / 1e6;
        return Some((m, m));
    }
    Some((lo, hi))
}

/// The edges of the outline that the part's own requirements put the body's edge at or past:
/// the whole window a `left`, `right`, `top` or `bottom` requirement leaves is at or beyond
/// the outline's same edge (flush counts), so the person has asked for the body to be there.
/// The windows of several requirements on one edge are intersected. A window on `x` or `y`, a
/// bound a module states for its parts, and a side of an outline whose size is not fixed
/// state nothing.
fn stated_edges(reqs: &[Req], width: Option<f64>, height: Option<f64>) -> Edges {
    let mut edges = Edges::default();
    for (field, edge, size) in [
        (PlaceField::Left, Edge::Left, Some(0.0)),
        (PlaceField::Right, Edge::Right, width),
        (PlaceField::Top, Edge::Top, Some(0.0)),
        (PlaceField::Bottom, Edge::Bottom, height),
    ] {
        let own: Vec<&Req> = reqs
            .iter()
            .filter(|r| r.field == field && !r.inherited)
            .collect();
        let (Some(size), false) = (size, own.is_empty()) else {
            continue;
        };
        let lo = own.iter().map(|r| r.lo).fold(f64::NEG_INFINITY, f64::max);
        let hi = own.iter().map(|r| r.hi).fold(f64::INFINITY, f64::min);
        let beyond = match edge {
            Edge::Left | Edge::Top => hi.is_finite() && hi <= size + TOLERANCE,
            Edge::Right | Edge::Bottom => lo.is_finite() && lo >= size - TOLERANCE,
        };
        if beyond {
            edges.set(edge);
        }
    }
    edges
}

/// The overhang a part's requirements state on the edges `may`, with the requirements that
/// say it, or `None` when they state none.
fn stated_overhang(reqs: &[Req], may: Edges) -> Option<Overhang> {
    may.any().then(|| Overhang::Stated {
        edges: may,
        says: reqs
            .iter()
            .filter(|r| {
                !r.inherited
                    && Edge::ALL
                        .into_iter()
                        .any(|e| may.has(e) && r.field.word() == e.word())
            })
            .map(Req::says)
            .collect::<Vec<_>>()
            .join(", "),
    })
}

fn turns_allowed(reqs: &[Req]) -> [bool; 4] {
    let mut ok = [true; 4];
    for r in reqs.iter().filter(|r| r.field == PlaceField::Rotation) {
        for (q, allowed) in ok.iter_mut().enumerate() {
            let angle = q as f64 * 90.0;
            *allowed &= miss(r, Rect::new(Pt::default(), Pt::default()), angle) < 1e-6;
        }
    }
    ok
}

fn number(v: f64) -> String {
    format!("{}", (v * 1e3).round() / 1e3)
}

/// The outline's size where it is one value, else its most if that is finite.
fn outline_extent(design: &Design) -> (Option<f64>, Option<f64>) {
    let pick = |(lo, hi): (f64, f64)| {
        if hi.is_finite() {
            Some(hi)
        } else if lo > 0.0 {
            Some(lo)
        } else {
            None
        }
    };
    match &design.features.outline {
        Some(o) => (pick(o.width), pick(o.height)),
        None => (None, None),
    }
}

/// Work out what the requirements of `design` leave to the placer, or why they cannot all
/// be met. `parts` are the footprints as loaded, with the faces `options` gave them.
pub(crate) fn plan(
    design: &Design,
    parts: &[Placed],
    options: &PlaceOptions,
    clearance: f64,
) -> Result<Plan, Vec<String>> {
    let mut plan = Plan::default();
    let mut errors = Vec::new();
    let (width, height) = outline_extent(design);
    // Where each part ends up if the requirements decide it: refdes, pose, who said so.
    let mut decided: Vec<(String, Pt, f64, Side, String, Edges)> = Vec::new();

    for part in parts {
        let (reqs, side_req) = requirements(design, &part.path);
        if reqs.is_empty() && side_req.is_none() {
            continue;
        }
        let name = format!("{} (`{}`)", part.refdes, part.path);
        let says: Vec<String> = reqs.iter().map(Req::says).collect();
        let causes = says.join(", ");

        // The face: the requirement's, else what the layout or the caller said, else the top.
        let layout_side = options.sides.get(&part.refdes).copied();
        let side = match &side_req {
            Some((s, _)) if s == "bottom" => Side::Back,
            Some(_) => Side::Front,
            None => layout_side.unwrap_or(Side::Front),
        };

        // A part the layout file locks stays where it is, if that meets the requirements.
        if let Some(f) = options.fixed.get(&part.refdes) {
            let body = body_at(part, f.rotation, f.side).translated(ORIGIN + f.at);
            let mut bad: Vec<String> = Vec::new();
            for r in &reqs {
                let m = miss(r, body, f.rotation);
                if m > TOLERANCE {
                    bad.push(if r.field == PlaceField::Rotation {
                        format!("{}, and it is turned {}", r.says(), number(f.rotation))
                    } else {
                        format!(
                            "{}, and its {} is {} mm",
                            r.says(),
                            r.field.word(),
                            number(measured(r.field, body))
                        )
                    });
                }
            }
            if let Some((s, l)) = &side_req
                && (s == "bottom") != (f.side == Side::Back)
            {
                bad.push(format!(
                    "`{l}` allows \"{s}\", and it is on the {}",
                    if f.side == Side::Back {
                        "bottom"
                    } else {
                        "top"
                    }
                ));
            }
            if !bad.is_empty() {
                errors.push(format!(
                    "{name} is locked in the layout file at ({}, {}) mm, and that breaks a requirement of the design: {}; unlock it and let the placer move it, or change the requirement",
                    number(f.at.x),
                    number(f.at.y),
                    bad.join("; ")
                ));
            }
            // A lock fixes where the part is; it does not say it may hang over an edge. Only
            // the part's own requirements do (owner decision, 2026-11-03, ADR 0045 section 10).
            let may = stated_edges(&reqs, width, height);
            if let Some(stated) = stated_overhang(&reqs, may) {
                plan.overhang.insert(part.refdes.clone(), stated);
            }
            decided.push((
                part.refdes.clone(),
                ORIGIN + f.at,
                f.rotation,
                f.side,
                format!(
                    "the layout file locks it at ({}, {})",
                    number(f.at.x),
                    number(f.at.y)
                ),
                may,
            ));
            continue;
        }

        let allowed = turns_allowed(&reqs);
        if !allowed.iter().any(|&a| a) {
            let r = reqs
                .iter()
                .find(|r| r.field == PlaceField::Rotation)
                .map(Req::says)
                .unwrap_or_default();
            errors.push(format!(
                "{name} cannot be turned as required: {r}, and a part is turned by multiples of 90"
            ));
            continue;
        }
        let mut reach = [None; 4];
        let mut why: Vec<String> = Vec::new();
        for (q, ok) in allowed.iter().enumerate() {
            if !ok {
                continue;
            }
            let angle = q as f64 * 90.0;
            let body = body_at(part, angle, side);
            let (x, y) = (interval(&reqs, 0, body), interval(&reqs, 1, body));
            match (x, y) {
                (Some(x), Some(y)) => reach[q] = Some(Reach { x, y }),
                _ => {
                    let axis = if x.is_none() { "x" } else { "y" };
                    why.push(format!(
                        "turned {angle} degrees its body is {} by {} mm, and along {axis} no position meets {}",
                        number(body.width()),
                        number(body.height()),
                        reqs.iter()
                            .filter(|r| axis_of(r.field) == Some(usize::from(x.is_some())))
                            .map(Req::says)
                            .collect::<Vec<_>>()
                            .join(" and ")
                    ));
                }
            }
        }
        if reach.iter().all(Option::is_none) {
            errors.push(format!(
                "{name} cannot meet its requirements ({causes}): {}{}",
                why.join("; "),
                if part.footprint.body_is_courtyard {
                    " (the footprint draws no fabrication layer, so its courtyard is the body)"
                } else {
                    ""
                }
            ));
            continue;
        }

        // The edges the part's own requirements put its body at or past (ADR 0045, section 10).
        let may = stated_edges(&reqs, width, height);
        // A window that is wholly outside a fixed outline. A part whose requirement states an
        // overhang on an edge (PROVISIONAL, ADR 0045 section 10) may have its centre outside
        // in that direction: its body is meant to stick out, and the pads staying inside is
        // the overhang rule's. A window on `x` or `y` keeps the old rule.
        for (q, slot) in reach.iter_mut().enumerate() {
            let Some(r) = *slot else { continue };
            let body = body_at(part, q as f64 * 90.0, side);
            let c = body.centre();
            for (axis, range, size) in [(0, r.x, width), (1, r.y, height)] {
                let Some(size) = size else { continue };
                let origin = if axis == 0 { ORIGIN.x } else { ORIGIN.y };
                let centre = if axis == 0 { c.x } else { c.y };
                let (lo, hi) = (range.0 + centre - origin, range.1 + centre - origin);
                let (before, after) = if axis == 0 {
                    (Edge::Left, Edge::Right)
                } else {
                    (Edge::Top, Edge::Bottom)
                };
                let outside_before = hi < -TOLERANCE;
                let outside_after = lo > size + TOLERANCE;
                if (outside_before && !may.has(before)) || (outside_after && !may.has(after)) {
                    errors.push(format!(
                        "{name} is required to be where its body's centre is {} mm from the {} edge ({causes}), and the outline is {} mm {}",
                        range_text(lo, hi, "").trim_end(),
                        if axis == 0 { "left" } else { "top" },
                        number(size),
                        if axis == 0 { "wide" } else { "high" }
                    ));
                    *slot = None;
                }
            }
        }
        if reach.iter().all(Option::is_none) {
            continue;
        }

        plan.sides.insert(part.refdes.clone(), side);
        let live: Vec<usize> = (0..4).filter(|&q| reach[q].is_some()).collect();
        if let Some(stated) = stated_overhang(&reqs, may) {
            plan.overhang.insert(part.refdes.clone(), stated);
        }
        let pinned = live.len() == 1
            && reach[live[0]].is_some_and(|r| {
                r.x.0 == r.x.1 && r.y.0 == r.y.1 && r.x.0.is_finite() && r.y.0.is_finite()
            });
        if pinned {
            let q = live[0];
            let r = reach[q].expect("live");
            let rotation = q as f64 * 90.0;
            plan.fixed.insert(
                part.refdes.clone(),
                Fixed {
                    at: Pt::new(r.x.0, r.y.0) - ORIGIN,
                    rotation,
                    side,
                    text: crate::board::ReferenceSide::Above,
                },
            );
            decided.push((
                part.refdes.clone(),
                Pt::new(r.x.0, r.y.0),
                rotation,
                side,
                format!("its requirements ({causes}) pin it"),
                may,
            ));
        } else {
            let holds_x = reqs.iter().any(|r| axis_of(r.field) == Some(0));
            let holds_y = reqs.iter().any(|r| axis_of(r.field) == Some(1));
            let needs_search = reach.iter().flatten().any(|r| {
                r.x.0 != f64::NEG_INFINITY
                    || r.x.1 != f64::INFINITY
                    || r.y.0 != f64::NEG_INFINITY
                    || r.y.1 != f64::INFINITY
            }) || live.len() < 4;
            if needs_search {
                plan.wants.insert(
                    part.refdes.clone(),
                    Want {
                        reach,
                        holds_x,
                        holds_y,
                        overhangs: may.any(),
                    },
                );
            }
        }
    }

    // Relations between two parts (ADR 0045, section 3). The annealer keeps them; two parts
    // that are both decided, or one decided and one whose requirements leave no way to meet
    // it, are reported here, with the numbers, and not looped on.
    for o in &design.features.offsets {
        let (Some(a), Some(b)) = (
            parts.iter().find(|p| p.path == o.a.path),
            parts.iter().find(|p| p.path == o.b.path),
        ) else {
            continue;
        };
        let id = format!(
            "{}.place.{} - {}.place.{}",
            o.b.path,
            o.b.field.word(),
            o.a.path,
            o.a.field.word()
        );
        plan.offsets.push(PlanOffset {
            a: a.refdes.clone(),
            b: b.refdes.clone(),
            a_path: o.a.path.clone(),
            b_path: o.b.path.clone(),
            fa: o.a.field,
            fb: o.b.field,
            lo: o.lo,
            hi: o.hi,
            label: if o.source.is_empty() {
                id
            } else {
                format!("{id} ({})", o.source)
            },
        });
    }
    // A part the layout file locks is decided for a relation whether or not a requirement of
    // its own says anything about it.
    let mut known = decided.clone();
    for part in parts {
        if let Some(f) = options.fixed.get(&part.refdes)
            && !known.iter().any(|d| d.0 == part.refdes)
        {
            known.push((
                part.refdes.clone(),
                ORIGIN + f.at,
                f.rotation,
                f.side,
                format!(
                    "the layout file locks it at ({}, {})",
                    number(f.at.x),
                    number(f.at.y)
                ),
                Edges::default(),
            ));
        }
    }
    for o in &plan.offsets {
        if let Some(why) = offset_cannot_be_met(o, parts, &known, &plan) {
            errors.push(why);
        }
    }

    // Two parts that are decided, on the same face, whose courtyards overlap.
    for (i, a) in decided.iter().enumerate() {
        for b in &decided[i + 1..] {
            let (Some(pa), Some(pb)) = (
                parts.iter().find(|p| p.refdes == a.0),
                parts.iter().find(|p| p.refdes == b.0),
            ) else {
                continue;
            };
            // The parts as they would lie, to find the room each takes on each face (a part
            // with a hole may take only its pads on a face, `Placed::room_on`).
            let lies = |p: &Placed, at: Pt, rotation: f64, side: Side| {
                let mut q = p.clone();
                (q.at, q.rotation, q.side) = (at, rotation, side);
                q
            };
            let court = |p: &Placed, at: Pt, rotation: f64, side: Side| {
                p.footprint
                    .courtyard
                    .rotated_for(side == Side::Back, rotation)
                    .translated(at)
            };
            // A part that hangs over the edge takes only the room it has on the board.
            let on_board = |c: Rect, may: Edges| match (width, height) {
                (Some(w), Some(h)) if may.any() => {
                    overhang::clip(&c, &Rect::new(ORIGIN, Pt::new(ORIGIN.x + w, ORIGIN.y + h)))
                }
                _ => c,
            };
            let (ca, cb) = (
                on_board(court(pa, a.1, a.2, a.3), a.5),
                on_board(court(pb, b.1, b.2, b.3), b.5),
            );
            if lies(pa, a.1, a.2, a.3).rooms_overlap(ca, &lies(pb, b.1, b.2, b.3), cb, clearance) {
                let span = |c: &Rect| {
                    format!(
                        "x {} to {}, y {} to {}",
                        number(c.min.x - ORIGIN.x),
                        number(c.max.x - ORIGIN.x),
                        number(c.min.y - ORIGIN.y),
                        number(c.max.y - ORIGIN.y)
                    )
                };
                errors.push(format!(
                    "{} and {} are put on top of each other: {} ({}), {} ({}); the courtyard of {} spans {} mm and that of {} spans {} mm",
                    a.0, b.0, a.0, a.4, b.0, b.4, a.0, span(&ca), b.0, span(&cb)
                ));
            }
        }
    }

    if errors.is_empty() {
        Ok(plan)
    } else {
        Err(errors)
    }
}

/// What a field measures, as a person says it.
fn what(field: PlaceField) -> &'static str {
    match field {
        PlaceField::X | PlaceField::Y => "the centre",
        PlaceField::Left => "the left edge",
        PlaceField::Right => "the right edge",
        PlaceField::Top => "the top edge",
        PlaceField::Bottom => "the bottom edge",
        PlaceField::Rotation => "the turn",
    }
}

/// The decided part `refdes` as [`plan`] left it: where its origin is on the sheet, its turn,
/// its face, and why it is there.
type Decided = (String, Pt, f64, Side, String, Edges);

/// Why a relation between two parts cannot be met, when the requirements already decide it:
/// both parts are fixed (pinned by their requirements or locked in the layout file) and the
/// relation does not hold between them, or one is fixed and the requirements of the other
/// leave no turn and no place where it holds. `None` when it can be met, or the search may.
fn offset_cannot_be_met(
    o: &PlanOffset,
    parts: &[Placed],
    decided: &[Decided],
    plan: &Plan,
) -> Option<String> {
    let find = |refdes: &str| parts.iter().find(|p| p.refdes == refdes);
    let fixed = |refdes: &str| decided.iter().find(|d| d.0 == refdes);
    let (pa, pb) = (find(&o.a)?, find(&o.b)?);
    let axis = o.axis();
    let name = |p: &Placed| format!("{} (`{}`)", p.refdes, p.path);
    let value = |d: &Decided, p: &Placed, field: PlaceField| {
        offset(field, body_at(p, d.2, d.3).translated(d.1))
    };
    let along = |at: Pt| if axis == 0 { at.x } else { at.y };
    match (fixed(&o.a), fixed(&o.b)) {
        (Some(da), Some(db)) => {
            let (va, vb) = (value(da, pa, o.fa), value(db, pb, o.fb));
            let v = vb - va;
            let miss = o.miss(v);
            (miss > TOLERANCE).then(|| {
                format!(
                    "constraint {} is violated: {} is fixed ({}) and {} is fixed ({}); {} of the body of {} minus {} of the body of {} is {} mm along {}, and it allows {} ({} mm over)",
                    o.label,
                    name(pa),
                    da.4,
                    name(pb),
                    db.4,
                    what(o.fb),
                    name(pb),
                    what(o.fa),
                    name(pa),
                    number(v),
                    if axis == 0 { "x" } else { "y" },
                    range_text(o.lo, o.hi, "mm"),
                    number(miss)
                )
            })
        }
        (Some(d), None) | (None, Some(d)) => {
            // `d` is the fixed end; the other end `p` is searched.
            let fixed_is_a = fixed(&o.a).is_some();
            let (fixed_part, free_part) = if fixed_is_a { (pa, pb) } else { (pb, pa) };
            let (f_fixed, f_free) = if fixed_is_a {
                (o.fa, o.fb)
            } else {
                (o.fb, o.fa)
            };
            let want = plan.wants.get(&free_part.refdes)?;
            let v_fixed = value(d, fixed_part, f_fixed);
            // The values the free end's measure may take, so that the relation holds.
            let (lo, hi) = if fixed_is_a {
                (v_fixed + o.lo, v_fixed + o.hi)
            } else {
                (v_fixed - o.hi, v_fixed - o.lo)
            };
            let side = plan
                .sides
                .get(&free_part.refdes)
                .copied()
                .unwrap_or(free_part.side);
            let mut allowed: Vec<String> = Vec::new();
            for (q, reach) in want.reach.iter().enumerate() {
                let Some(r) = reach else { continue };
                let off = offset(f_free, body_at(free_part, q as f64 * 90.0, side));
                let range = if axis == 0 { r.x } else { r.y };
                // Where the measure must be for the relation to hold, against where the
                // requirements let it be, in the frame of the outline.
                let (from, to) = (lo, hi);
                let (reach_lo, reach_hi) = (range.0 + off, range.1 + off);
                if from <= reach_hi + TOLERANCE && to >= reach_lo - TOLERANCE {
                    return None;
                }
                let origin = if axis == 0 { ORIGIN.x } else { ORIGIN.y };
                allowed.push(format!(
                    "turned {} degrees {} of its body must be {} mm along {}, and its requirements allow {}",
                    number(q as f64 * 90.0),
                    what(f_free),
                    range_text(from - origin, to - origin, "").trim_end(),
                    if axis == 0 { "x" } else { "y" },
                    range_text(reach_lo - origin, reach_hi - origin, "mm")
                ));
            }
            (!allowed.is_empty()).then(|| {
                format!(
                    "constraint {} cannot be met: {} is fixed ({}) at {} mm along {}, and {} cannot be where it holds: {}",
                    o.label,
                    name(fixed_part),
                    d.4,
                    number(along(d.1) - if axis == 0 { ORIGIN.x } else { ORIGIN.y }),
                    if axis == 0 { "x" } else { "y" },
                    name(free_part),
                    allowed.join("; ")
                )
            })
        }
        (None, None) => None,
    }
}

/// Where the body's centre may be, in the frame of the outline, if the requirements on
/// `part` bound it: the intersection over all turns allowed, as boxes. Used to draw a
/// requirement. `None` for a part with no positional requirement.
pub fn centre_window(design: &Design, part: &Placed) -> Option<Rect> {
    let (reqs, side_req) = requirements(design, &part.path);
    let side = match &side_req {
        Some((s, _)) if s == "bottom" => Side::Back,
        Some(_) => Side::Front,
        None => part.side,
    };
    let q = quarter(part.rotation)
        .filter(|&q| turns_allowed(&reqs)[q])
        .or_else(|| turns_allowed(&reqs).iter().position(|&a| a))?;
    let body = body_at(part, q as f64 * 90.0, side);
    let c = body.centre();
    let x = interval(&reqs, 0, body)?;
    let y = interval(&reqs, 1, body)?;
    if reqs.iter().all(|r| axis_of(r.field).is_none()) {
        return None;
    }
    let finite = |v: f64, fallback: f64| if v.is_finite() { v } else { fallback };
    // An unbounded end is drawn to where the part is now.
    let here = part.at;
    let (x0, x1) = (finite(x.0, here.x), finite(x.1, here.x));
    let (y0, y1) = (finite(y.0, here.y), finite(y.1, here.y));
    Some(Rect::new(
        Pt::new(x0 + c.x - ORIGIN.x, y0 + c.y - ORIGIN.y),
        Pt::new(x1 + c.x - ORIGIN.x, y1 + c.y - ORIGIN.y),
    ))
}

/// The placements of a design that apply to the part at `path`, own or by module.
pub fn placements_of<'a>(design: &'a Design, path: &str) -> Vec<&'a Placement> {
    design
        .features
        .constraints
        .iter()
        .filter(|p| p.path == path || (p.module && path.starts_with(&format!("{}.", p.path))))
        .collect()
}
