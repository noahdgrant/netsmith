//! The layout guidelines of the parts' datasheets on a board (`layout` blocks, ADR 0030, issues
//! #829 and #994): which parts each statement is about, found from the nets, what is measured,
//! and the finding when a number the datasheet gives is not met.
//!
//! A part's statement names its own pins. `bridging .a .b` finds every other part with two
//! pads, one on the net of each pin; `on .pin kind K` every other part with a pad on the net of
//! the pin, of the kind when one is given. Distances are pad centre to pad centre:
//!
//! - `near`: from the pads of a part found on the net of the pin after `to` (all its pads, when
//!   none is on it) to the pads of that pin, the nearest pair. At most `within`.
//! - `away`: from any pad of a part found to any pad of the pin after `from`, the nearest pair.
//!   At least `beyond`.
//! - `loop`: for each part found by `bridging .a .b`, the shortest closed path from a pad of
//!   `.a` to the part's pad on that net, across the part to its pad on the net of `.b`, to a
//!   pad of `.b`, and straight back to `.a`. At most `within`.
//! - `edge`: from the nearest pad of the pin to the board's edge (the polygon's edge, on a
//!   board with one). At most `within`.
//! - `vias`: the vias of the pin's net whose centre is on one of the pin's pads. At least
//!   `count`. Checked on the routed board.
//! - `route`: the length of every track of the pin's net, in all. At most `length`. Checked on
//!   the routed board. Its `width` is not measured here: the design adds it to the net's least
//!   width (`Features::net_widths`), which the router keeps to.
//!
//! The placer pulls each part a `near` or a `loop` finds as it pulls a decoupling capacitor,
//! pushes the parts an `away` finds out to its distance, and pulls a part with an `edge`
//! statement to the edge as it pulls a connector (`netsmith_placer`'s annealer). A `keepout` is
//! not here: the design turns it into a keepout the part carries, which the placer, the router
//! and the pour keep to. `fab` warns of each finding, unless the layout file waives the
//! guideline ([`crate::Waiver`]).

use std::collections::HashMap;

use netsmith_core::{LayoutGuideline, LayoutRule, NearSelect};

use crate::board::{Board, BoardNet, Placed};
use crate::geometry::Pt;

/// One guideline of a part, with the pads of the pins it names, as the board's part carries it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PartGuideline {
    pub rule: LayoutGuideline,
    /// The pads of each pin the rule names, in the order of [`pins_of`].
    pub pads: Vec<Vec<String>>,
}

/// The pins a rule names, in a fixed order: the selector's (both pins of `bridging`, the one of
/// `on`) and then the pin after `to` or `from`; `.a` and `.b` of a `loop`; the one pin of
/// `edge`, `vias` and `route`.
pub fn pins_of(rule: &LayoutRule) -> Vec<&str> {
    fn select(s: &NearSelect) -> Vec<&str> {
        match s {
            NearSelect::Bridging { a, b } => vec![a.as_str(), b.as_str()],
            NearSelect::On { pin, .. } => vec![pin.as_str()],
        }
    }
    match rule {
        LayoutRule::Near { select: s, to, .. } => {
            let mut out = select(s);
            out.push(to);
            out
        }
        LayoutRule::Away { select: s, from, .. } => {
            let mut out = select(s);
            out.push(from);
            out
        }
        LayoutRule::Loop { a, b, .. } => vec![a, b],
        LayoutRule::Edge { pin, .. } | LayoutRule::Vias { pin, .. } | LayoutRule::Route { pin, .. } => {
            vec![pin]
        }
    }
}

impl PartGuideline {
    /// The guidelines of the part at `part` among `rules`, with the pads of their pins from the
    /// part's pin table. A pin with no pads in the package has none, and its guideline then
    /// finds and measures nothing.
    pub fn of_part(part: &netsmith_core::Part, rules: &[LayoutGuideline]) -> Vec<Self> {
        let pads = |name: &str| -> Vec<String> {
            part.pins
                .iter()
                .filter(|p| p.name == name)
                .flat_map(|p| p.pads.iter().cloned())
                .collect()
        };
        rules
            .iter()
            .filter(|r| r.part == part.path)
            .map(|r| PartGuideline {
                rule: r.clone(),
                pads: pins_of(&r.rule).into_iter().map(pads).collect(),
            })
            .collect()
    }

    /// The pads of the pin the parts are measured to: `to` of a `near`, `from` of an `away`.
    fn target_pads(&self) -> &[String] {
        self.pads.last().map_or(&[], Vec::as_slice)
    }
}

/// A part a `near` or an `away` statement found on the board.
#[derive(Debug, Clone, PartialEq)]
pub struct NearPair {
    /// The index of the part that states the guideline, and of the guideline in its
    /// [`Placed::layout`].
    pub part: usize,
    pub rule: usize,
    /// The index of the part found.
    pub other: usize,
    /// The numbers of the pads of the part found that are measured.
    pub other_pads: Vec<String>,
    /// The numbers of the pads of the pin `to` (of a `near`) or `from` (of an `away`).
    pub to_pads: Vec<String>,
}

impl NearPair {
    /// The guideline, as the part states it.
    pub fn rule<'a>(&self, parts: &'a [Placed]) -> &'a LayoutGuideline {
        &parts[self.part].layout[self.rule].rule
    }

    /// The distance from the nearest pad of the part found to the nearest pad of the pin, centre
    /// to centre, in mm; `None` when either has no pad on the footprint.
    pub fn distance(&self, parts: &[Placed]) -> Option<f64> {
        nearest(
            &centres(&parts[self.other], &self.other_pads),
            &centres(&parts[self.part], &self.to_pads),
        )
    }
}

/// A part a `loop` statement found on the board: the pads of each side of its loop.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopPair {
    /// The index of the part that states the guideline, and of the guideline in its
    /// [`Placed::layout`].
    pub part: usize,
    pub rule: usize,
    /// The index of the part found.
    pub other: usize,
    /// The pad of the part found on the net of `.a`, and the pads of `.a`.
    pub a: (String, Vec<String>),
    /// The pad of the part found on the net of `.b`, and the pads of `.b`.
    pub b: (String, Vec<String>),
}

impl LoopPair {
    /// The guideline, as the part states it.
    pub fn rule<'a>(&self, parts: &'a [Placed]) -> &'a LayoutGuideline {
        &parts[self.part].layout[self.rule].rule
    }

    /// The length of the shortest such loop, in mm; `None` when a pad is not on the footprint.
    pub fn length(&self, parts: &[Placed]) -> Option<f64> {
        let (part, other) = (&parts[self.part], &parts[self.other]);
        let ca = centres(other, std::slice::from_ref(&self.a.0));
        let cb = centres(other, std::slice::from_ref(&self.b.0));
        let (pa, pb) = (centres(part, &self.a.1), centres(part, &self.b.1));
        let mut best: Option<f64> = None;
        for &x in &pa {
            for &y in &pb {
                for &u in &ca {
                    for &v in &cb {
                        let d = x.distance(u) + u.distance(v) + v.distance(y) + y.distance(x);
                        best = Some(best.map_or(d, |b: f64| b.min(d)));
                    }
                }
            }
        }
        best
    }
}

/// The centres on the board of the pads of `part` with these numbers.
fn centres(part: &Placed, numbers: &[String]) -> Vec<Pt> {
    numbers
        .iter()
        .flat_map(|n| part.footprint.pads_numbered(n))
        .map(|p| part.pad_centre(p))
        .collect()
}

/// The least distance between a point of `a` and a point of `b`.
fn nearest(a: &[Pt], b: &[Pt]) -> Option<f64> {
    a.iter()
        .flat_map(|p| b.iter().map(move |q| p.distance(*q)))
        .reduce(f64::min)
}

/// The net of each pad of the board's parts, by (part index, pad number).
struct Nets<'a> {
    of: HashMap<(usize, &'a str), usize>,
}

impl<'a> Nets<'a> {
    fn new(parts: &[Placed], nets: &'a [BoardNet]) -> Self {
        let index: HashMap<&str, usize> = parts
            .iter()
            .enumerate()
            .map(|(i, p)| (p.refdes.as_str(), i))
            .collect();
        let mut of = HashMap::new();
        for (n, net) in nets.iter().enumerate() {
            for pad in &net.pads {
                if let Some(&i) = index.get(pad.refdes.as_str()) {
                    of.insert((i, pad.number.as_str()), n);
                }
            }
        }
        Nets { of }
    }

    fn of(&self, part: usize, pad: &str) -> Option<usize> {
        self.of.get(&(part, pad)).copied()
    }

    /// The nets of these pads of a part, sorted, each once.
    fn of_pads(&self, part: usize, pads: &[String]) -> Vec<usize> {
        let mut out: Vec<usize> = pads.iter().filter_map(|p| self.of(part, p)).collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// Whether part `j` (not `i`) is one a selector of part `i` finds, given the nets of the
/// selector's pins (`sides`).
fn selects(
    select: &NearSelect,
    sides: &[Vec<usize>],
    other: &Placed,
    j: usize,
    nets: &Nets,
) -> bool {
    let numbers = other.footprint.pad_numbers();
    let on = |number: &str| nets.of(j, number);
    match select {
        NearSelect::Bridging { .. } => bridges(&numbers, &sides[0], &sides[1], on).is_some(),
        NearSelect::On { kind, .. } => {
            kind.is_none_or(|k| other.declared_kind == Some(k))
                && numbers
                    .iter()
                    .any(|x| on(x).is_some_and(|n| sides[0].contains(&n)))
        }
    }
}

/// The pad on side `a` and the pad on side `b` of a part with two pads, one on a net of each.
fn bridges<'n>(
    numbers: &[&'n str],
    a: &[usize],
    b: &[usize],
    on: impl Fn(&str) -> Option<usize>,
) -> Option<(&'n str, &'n str)> {
    let [x, y] = numbers else { return None };
    let (nx, ny) = (on(x)?, on(y)?);
    if a.contains(&nx) && b.contains(&ny) {
        Some((x, y))
    } else if a.contains(&ny) && b.contains(&nx) {
        Some((y, x))
    } else {
        None
    }
}

/// Every part that a `near` (or, with `away`, an `away`) statement of a part of the board
/// finds, in the order of the parts that state them, their guidelines, and the parts found.
fn pairs_of(parts: &[Placed], nets: &[BoardNet], away: bool) -> Vec<NearPair> {
    if parts.iter().all(|p| p.layout.is_empty()) {
        return Vec::new();
    }
    let index = Nets::new(parts, nets);
    let mut pairs = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        for (r, g) in part.layout.iter().enumerate() {
            let select = match (&g.rule.rule, away) {
                (LayoutRule::Near { select, .. }, false) | (LayoutRule::Away { select, .. }, true) => {
                    select
                }
                _ => continue,
            };
            let selector = &g.pads[..g.pads.len() - 1];
            let sides: Vec<Vec<usize>> = selector.iter().map(|p| index.of_pads(i, p)).collect();
            let to_nets = index.of_pads(i, g.target_pads());
            for (j, other) in parts.iter().enumerate() {
                if j == i || !selects(select, &sides, other, j, &index) {
                    continue;
                }
                let numbers = other.footprint.pad_numbers();
                let all: Vec<String> = numbers.iter().map(|x| x.to_string()).collect();
                let other_pads = if away {
                    all
                } else {
                    let on_to: Vec<String> = numbers
                        .iter()
                        .filter(|x| index.of(j, x).is_some_and(|n| to_nets.contains(&n)))
                        .map(|x| x.to_string())
                        .collect();
                    if on_to.is_empty() { all } else { on_to }
                };
                pairs.push(NearPair {
                    part: i,
                    rule: r,
                    other: j,
                    other_pads,
                    to_pads: g.target_pads().to_vec(),
                });
            }
        }
    }
    pairs
}

/// Every part that a `near` statement of a part of the board finds, in the order of the parts
/// that state them, their guidelines, and the parts found.
pub fn near_pairs(parts: &[Placed], nets: &[BoardNet]) -> Vec<NearPair> {
    pairs_of(parts, nets, false)
}

/// Every part that an `away` statement of a part of the board finds, in the same order.
pub fn away_pairs(parts: &[Placed], nets: &[BoardNet]) -> Vec<NearPair> {
    pairs_of(parts, nets, true)
}

/// Every part that a `loop` statement of a part of the board finds, in the same order.
pub fn loop_pairs(parts: &[Placed], nets: &[BoardNet]) -> Vec<LoopPair> {
    if parts.iter().all(|p| p.layout.is_empty()) {
        return Vec::new();
    }
    let index = Nets::new(parts, nets);
    let mut pairs = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        for (r, g) in part.layout.iter().enumerate() {
            if !matches!(g.rule.rule, LayoutRule::Loop { .. }) {
                continue;
            }
            let (a, b) = (index.of_pads(i, &g.pads[0]), index.of_pads(i, &g.pads[1]));
            for (j, other) in parts.iter().enumerate() {
                if j == i {
                    continue;
                }
                let numbers = other.footprint.pad_numbers();
                let Some((x, y)) = bridges(&numbers, &a, &b, |n| index.of(j, n)) else {
                    continue;
                };
                pairs.push(LoopPair {
                    part: i,
                    rule: r,
                    other: j,
                    a: (x.to_string(), g.pads[0].clone()),
                    b: (y.to_string(), g.pads[1].clone()),
                });
            }
        }
    }
    pairs
}

/// The parts with an `edge` statement, by index, each once.
pub fn edge_parts(parts: &[Placed]) -> Vec<usize> {
    parts
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.layout
                .iter()
                .any(|g| matches!(g.rule.rule, LayoutRule::Edge { .. }))
        })
        .map(|(i, _)| i)
        .collect()
}

/// A guideline of a part that the board does not meet.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutFinding {
    /// The path of the part that states the guideline, and its name: what a waiver names.
    pub part: String,
    pub name: String,
    /// The sentence: the guideline, what is measured, the source and what to do.
    pub message: String,
}

/// What every guideline of the board's parts measures, for `netsmith layout summary`: one line
/// for each part found (or for the part itself, for `edge`, `vias` and `route`), with the
/// number the datasheet gives when it gives one. A measure that needs routed copper is left
/// out of a board with none.
pub fn measures(board: &Board) -> Vec<Measure> {
    let parts = &board.parts;
    let mut out = Vec::new();
    for pair in near_pairs(parts, &board.nets) {
        let rule = pair.rule(parts);
        let LayoutRule::Near { within, .. } = rule.rule else {
            continue;
        };
        out.push(Measure {
            guideline: rule.clone(),
            other: Some(parts[pair.other].refdes.clone()),
            measured: pair.distance(parts),
            limit: within.map(Limit::AtMost),
        });
    }
    for pair in away_pairs(parts, &board.nets) {
        let rule = pair.rule(parts);
        let LayoutRule::Away { beyond, .. } = rule.rule else {
            continue;
        };
        out.push(Measure {
            guideline: rule.clone(),
            other: Some(parts[pair.other].refdes.clone()),
            measured: pair.distance(parts),
            limit: Some(Limit::AtLeast(beyond)),
        });
    }
    for pair in loop_pairs(parts, &board.nets) {
        let rule = pair.rule(parts);
        let LayoutRule::Loop { within, .. } = rule.rule else {
            continue;
        };
        out.push(Measure {
            guideline: rule.clone(),
            other: Some(parts[pair.other].refdes.clone()),
            measured: pair.length(parts),
            limit: within.map(Limit::AtMost),
        });
    }
    let routed = !board.tracks.is_empty() || !board.vias.is_empty();
    for part in parts {
        for g in &part.layout {
            let pads = &g.pads[0];
            let (measured, limit) = match &g.rule.rule {
                LayoutRule::Edge { within, .. } => {
                    (to_edge(board, part, pads), within.map(Limit::AtMost))
                }
                LayoutRule::Vias { count, .. } if routed => (
                    Some(vias_on(board, part, pads) as f64),
                    Some(Limit::AtLeast(*count as f64)),
                ),
                LayoutRule::Route {
                    length: Some(most), ..
                } if routed => (
                    net_of(board, part, pads).map(|net| track_length(board, &net)),
                    Some(Limit::AtMost(*most)),
                ),
                _ => continue,
            };
            out.push(Measure {
                guideline: g.rule.clone(),
                other: None,
                measured,
                limit,
            });
        }
    }
    out.sort_by(|a, b| {
        (&a.guideline.part, &a.guideline.name, &a.other).cmp(&(
            &b.guideline.part,
            &b.guideline.name,
            &b.other,
        ))
    });
    out
}

/// What one guideline measures on a board ([`measures`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Measure {
    pub guideline: LayoutGuideline,
    /// The reference designator of the part found, for `near`, `away` and `loop`.
    pub other: Option<String>,
    /// In mm, or a count of vias; `None` when a pad to measure is not on the footprint.
    pub measured: Option<f64>,
    /// The number the datasheet gives; `None` for "close to".
    pub limit: Option<Limit>,
}

impl Measure {
    /// Whether the measure meets the number, when there is a number and a measure.
    pub fn met(&self) -> Option<bool> {
        let (m, limit) = (self.measured?, self.limit?);
        Some(match limit {
            Limit::AtMost(x) => m <= x + 1e-6,
            Limit::AtLeast(x) => m + 1e-6 >= x,
        })
    }
}

/// The number a guideline gives.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Limit {
    AtMost(f64),
    AtLeast(f64),
}

/// The distance from the nearest of these pads of a part to the board's edge, in mm: zero for
/// a pad centre outside it.
fn to_edge(board: &Board, part: &Placed, pads: &[String]) -> Option<f64> {
    centres(part, pads)
        .into_iter()
        .map(|p| match &board.polygon {
            Some(poly) => poly.depth(p).max(0.0),
            None => {
                let o = &board.outline;
                (p.x - o.min.x)
                    .min(o.max.x - p.x)
                    .min(p.y - o.min.y)
                    .min(o.max.y - p.y)
                    .max(0.0)
            }
        })
        .reduce(f64::min)
}

/// The vias of the net of these pads whose centre is on one of them.
fn vias_on(board: &Board, part: &Placed, pads: &[String]) -> usize {
    let Some(net) = net_of(board, part, pads) else {
        return 0;
    };
    let boxes: Vec<_> = pads
        .iter()
        .flat_map(|n| part.footprint.pads_numbered(n))
        .map(|p| part.pad_bounds(p))
        .collect();
    board
        .vias
        .iter()
        .filter(|v| v.net == net && boxes.iter().any(|b| b.contains(&crate::geometry::Rect::new(v.at, v.at))))
        .count()
}

/// The net of the first of these pads of a part that is on one.
fn net_of(board: &Board, part: &Placed, pads: &[String]) -> Option<String> {
    board
        .nets
        .iter()
        .find(|n| {
            n.pads
                .iter()
                .any(|p| p.refdes == part.refdes && pads.contains(&p.number))
        })
        .map(|n| n.name.clone())
}

/// The length of every track of a net, in mm.
fn track_length(board: &Board, net: &str) -> f64 {
    board
        .tracks
        .iter()
        .filter(|t| t.net == net)
        .map(|t| t.length())
        .sum()
}

/// A sentence for each guideline with a number that the board does not meet: the guideline,
/// the measure, the source and what to do. A statement with no number, a part with no pad to
/// measure, and a measure of copper on a board with none have none.
pub fn layout_findings(board: &Board) -> Vec<LayoutFinding> {
    let parts = &board.parts;
    let mut out = Vec::new();
    for m in measures(board) {
        if m.met() != Some(false) {
            continue;
        }
        let g = &m.guideline;
        let part = parts
            .iter()
            .find(|p| p.path == g.part)
            .expect("a measure is of a part of the board");
        let other = m
            .other
            .as_ref()
            .and_then(|r| parts.iter().find(|p| &p.refdes == r));
        let measured = m.measured.expect("a finding has a measure");
        let pads = |pin: &str| -> String {
            part.layout
                .iter()
                .find(|x| x.rule.name == g.name)
                .and_then(|x| {
                    pins_of(&x.rule.rule)
                        .iter()
                        .position(|p| *p == pin)
                        .map(|k| x.pads[k].join(", "))
                })
                .unwrap_or_default()
        };
        let head = format!("`{}` ({}) asks for `{}`", part.path, part.refdes, g.name);
        let tail = format!("({}; {})", g.source, g.site);
        let message = match (&g.rule, other) {
            (LayoutRule::Near { to, within, .. }, Some(o)) => format!(
                "{head} within {} mm of its pin `.{to}` (pads {}), and {} (`{}`) is {} mm from it {tail}: move {} nearer {}, or turn it so that its pad faces the pin",
                number(within.unwrap_or_default()),
                pads(to),
                o.refdes,
                o.path,
                number(measured),
                o.refdes,
                part.refdes,
            ),
            (LayoutRule::Away { from, beyond, .. }, Some(o)) => format!(
                "{head}: no part it finds within {} mm of its pin `.{from}` (pads {}), and {} (`{}`) is {} mm from it {tail}: move {} further from {}",
                number(*beyond),
                pads(from),
                o.refdes,
                o.path,
                number(measured),
                o.refdes,
                part.refdes,
            ),
            (LayoutRule::Loop { a, b, within }, Some(o)) => format!(
                "{head}: the loop from its pin `.{a}` through {} (`{}`) to its pin `.{b}` and back at most {} mm long, and it is {} mm {tail}: move {} nearer the pins of {}, its pads facing them",
                o.refdes,
                o.path,
                number(within.unwrap_or_default()),
                number(measured),
                o.refdes,
                part.refdes,
            ),
            (LayoutRule::Edge { pin, within }, _) => format!(
                "{head}: its pin `.{pin}` (pads {}) within {} mm of the board's edge, and it is {} mm from it {tail}: move {} to the edge, the pin facing out",
                pads(pin),
                number(within.unwrap_or_default()),
                number(measured),
                part.refdes,
            ),
            (LayoutRule::Vias { pin, count }, _) => format!(
                "{head}: at least {count} vias in the pads of its pin `.{pin}` (pads {}), and they have {} {tail}: add vias of the pin's net in its pads, or a process that allows vias in pads",
                pads(pin),
                measured as usize,
            ),
            (LayoutRule::Route { pin, length, .. }, _) => format!(
                "{head}: the tracks of the net of its pin `.{pin}` at most {} mm long in all, and they are {} mm {tail}: move the parts on the net nearer {}",
                number(length.unwrap_or_default()),
                number(measured),
                part.refdes,
            ),
            _ => continue,
        };
        out.push(LayoutFinding {
            part: g.part.clone(),
            name: g.name.clone(),
            message,
        });
    }
    out
}

/// The sentences of [`layout_findings`] for a placed board's `near` guidelines alone, as
/// before the routed checks: what the placer's tests read.
pub fn near_findings(parts: &[Placed], nets: &[BoardNet]) -> Vec<String> {
    near_pairs(parts, nets)
        .iter()
        .filter_map(|pair| {
            let rule = pair.rule(parts);
            let LayoutRule::Near { to, within, .. } = &rule.rule else {
                return None;
            };
            let within = (*within)?;
            let distance = pair.distance(parts)?;
            if distance <= within + 1e-6 {
                return None;
            }
            let (part, other) = (&parts[pair.part], &parts[pair.other]);
            Some(format!(
                "`{}` ({}) asks for `{}` within {} mm of its pin `.{}` (pads {}), and {} (`{}`) is {} mm from it ({}; {}): move {} nearer {}, or turn it so that its pad faces the pin",
                part.path,
                part.refdes,
                rule.name,
                number(within),
                to,
                pair.to_pads.join(", "),
                other.refdes,
                other.path,
                number(distance),
                rule.source,
                rule.site,
                other.refdes,
                part.refdes,
            ))
        })
        .collect()
}

/// A number with at most two decimals and no trailing zeros: `3`, `2.45`.
pub(crate) fn number(x: f64) -> String {
    let text = format!("{x:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}
