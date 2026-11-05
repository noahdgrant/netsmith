//! Numbers for how readable a drawing is, so a change to the layout can be judged by them and
//! not by looking: bends, crossings, wire length, how far a decoupling capacitor stands from
//! its pin, which side of a symbol a pin is on, overlaps, shape and size.
//!
//! [`measure`] is a pure, deterministic function of a [`Layout`]. It reports and never
//! judges: a lower count is better for every number named `bends`, `crossings`, `overlaps`,
//! `detour` and `ratio`, and nothing here has a threshold. The hard rules (no short, no open,
//! no wire through a symbol) are in [`crate::check`].
//!
//! One [`SheetMetrics`] is for one sheet of a board drawn by module
//! ([`crate::sheets::layout_sheets`]), or for the whole drawing when it has no sheets; `total`
//! adds the sheets up. All lengths are millimetres on the sheet. Definitions:
//!
//! - **bends**: the points where exactly two wires of one net end at right angles to each
//!   other. A T (a wire ending on the middle of another) is a junction, not a bend. The thick
//!   line of a bus (its spines and its trunk) is one net here: its bends are counted once.
//! - **crossings**: pairs of wires of different nets that cross square, each in its middle.
//!   A bus's thick line crosses as one wire; a member's entry, which ends on it, is not a
//!   contact. `collinear` counts pairs of different nets that run along one line over some length,
//!   and `contacts` pairs of different nets that touch with an end (both are shorts).
//! - **length ratio**: the wire length of the nets that have two pins or more on the sheet,
//!   over the sum of those nets' half-perimeters (the bounding box of their pins). 1 is a
//!   straight run. The `signal` ratio leaves out nets that end in a power symbol.
//! - **longest detour**: the most a single routed wire (a pin to the net's tree) is longer
//!   than the straight distance between its two ends.
//! - **satellite distance**: a satellite is a part with two pins that face opposite ways (the
//!   parts the layout stands beside a pin: a capacitor, a resistor, a diode). Its distance
//!   is, in grid steps, how far the nearest pin of another part on one of its nets is from
//!   its own pin on that net, measured across.
//! - **pair satellite**: a satellite whose two pins are on two different nets that no power symbol
//!   of the sheet is on, with a pin of one same other part on each (a crystal across `XL1` and
//!   `XL2`). `pair_satellites` counts them. **pair crossings** (`pair_crossings`, target 0)
//!   are the crossings of the wire of a pair satellite's pins with the wires of other nets
//!   (and with the thick line of a bus), counted along the wire from each of its pins to the pin
//!   of the other part on that net: what else is on the net is not counted.
//! - **rail span** (`rail_span`): the longest straight run of wire, in grid steps, of a net
//!   that ends in a power symbol of the sheet (runs that lie along one line and meet are
//!   one). A decoupling capacitor with a symbol of its own has a few steps; a rail across a
//!   whole sheet, many.
//! - **bodies** (`parts`, `units`): a part whose symbol has several units is drawn as a box for
//!   each (ADR 0050) and each box is a body here, with its own pins, satellites and side
//!   consistency; `parts` counts the boxes and `units` those that draw a unit of such a part.
//! - **satellite wire** (`satellite_wire`): the longest way along the wire of a net, in grid
//!   steps, from the pin of a satellite to the nearest pin of another part on that net, or
//!   power symbol, flag or label of it.
//! - **spine** (`spine_bends`, target 0; `detached_chain`, target 0): the layout finds, from the
//!   netlist, the pin kinds and the rails, the power path through an IC of a regulator: the
//!   chains of **series parts** (a fuse, a diode, a ferrite bead, an inductor: two-terminal parts
//!   on two nets, neither a ground, that are not across two pins of the IC) that lead from its
//!   input pins back to where the power comes from and from its switching pin out to the output
//!   (`Layout::spines`; `NETSMITH_SPINE=off` draws without a spine and still finds the chains,
//!   to compare). `spine_bends` is the number of bends along the wires of the longest chain
//!   (most parts; the input side's first), from the IC's pin through each part to the end of
//!   the chain: along each net between two parts, the bends of the shortest way from the pin of
//!   one to the pin of the next. A chain is **detached** unless it stands on the row of the
//!   IC (every pin of every part of it on one horizontal line, which is the line of a pin of
//!   the IC), or a wire of at most [`ATTACH_STEPS`] steps joins one of its parts to a pin of the
//!   IC. `detached_chain` counts the chains that are.
//! - **side consistency**: of the pins of parts that are not satellites, the share that are
//!   on the side a reader expects: an input on the left, an output on the right, a supply
//!   pin on top, a ground pin at the bottom. Pins the symbol does not say anything about
//!   are not counted.
//! - **stray wire**: a count of each way a wire serves nothing (the same rules as in
//!   [`crate::check`], counted here): `dead_ends` (an end that touches no pin, symbol, flag,
//!   label, X or wire of its net), `redundant_wires` (two wires of one net along one line),
//!   `zero_wires`, `u_turns` (a hook of a step or less), `unused_junctions` and
//!   `missing_junctions` (a dot where fewer than three arms meet, and three arms with no
//!   dot), `wires_along_pins` (a wire along a pin's line) and `wires_hugging` (a wire along
//!   the outline of a body or a flag within 0.7 mm). `stray` is their sum. `jogs` counts S-bends
//!   of a step or less; it is not part of `stray`.
//! - **symbol overlaps**: pairs of symbol boxes (a part's body and pins, a power symbol, a
//!   flag) that overlap. **Text overlaps**: pairs of texts that overlap. **Wire over text**
//!   and **wire through body**: wire segments that run through a text or a symbol's body.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::geom::{Dir, Pt, Rect};
use crate::layout::{Layout, PlacedPart, PowerKind};
use crate::sheetview::{
    EPS, Fault, OVERLAP, Rel, Seg, SheetView, body_strokes, bus_of, bus_segments, overlaps, relate,
    runs_through, runs_through_body, same, symbol_box, texts, views, wire_faults,
};

/// The schematic grid, as the layout uses it: the unit of the satellite distance.
const GRID: f64 = 1.27;

/// Numbers for one sheet. The raw sums are kept so that sheets add up; the ratios are
/// methods.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SheetMetrics {
    /// The sheet's name; `board` for a drawing with no sheets, `total` for the sum.
    pub name: String,
    /// The box of the sheet, title included, for a board drawn by module.
    pub width: f64,
    pub height: f64,

    /// Bodies drawn: each part, and each unit of a part whose symbol has several (a part of
    /// two units is two here, `units` counts them).
    pub parts: usize,
    /// Boxes that draw one unit of a part with several units (a processor's power unit and
    /// its signal unit are two).
    pub units: usize,
    /// Nets with at least one wire.
    pub wired_nets: usize,
    pub wires: usize,
    /// Straight runs of wire, after merging runs that continue in line.
    pub segments: usize,
    /// Segments that are neither horizontal nor vertical (there should be none).
    pub diagonal_segments: usize,
    pub bends: usize,
    /// The most bends in one net.
    pub max_bends_in_a_net: usize,
    /// Wire crossing a wire of another net square.
    pub crossings: usize,
    /// Wires of two nets lying along one line.
    pub collinear: usize,
    /// Wires of two nets touching with an end.
    pub contacts: usize,

    /// Wire length of the nets that have two pins on the sheet or more, and the sum of
    /// their pins' half-perimeters; the same two numbers for nets with no power symbol.
    pub wire_length: f64,
    pub half_perimeter: f64,
    pub signal_wire_length: f64,
    pub signal_half_perimeter: f64,
    pub longest_detour: f64,
    pub total_detour: f64,

    /// Satellites that stand near a pin of another part, the sum and the greatest of their
    /// distances in grid steps.
    pub satellites: usize,
    pub satellite_steps_sum: f64,
    pub satellite_steps_max: f64,

    /// Pins judged for side, and of them the pins on the expected side.
    pub pins_judged: usize,
    pub pins_on_side: usize,

    pub symbol_overlaps: usize,
    pub text_overlaps: usize,
    pub wire_over_text: usize,
    pub wire_through_body: usize,

    pub dead_ends: usize,
    pub redundant_wires: usize,
    pub zero_wires: usize,
    pub u_turns: usize,
    pub unused_junctions: usize,
    pub missing_junctions: usize,
    pub wires_along_pins: usize,
    pub wires_hugging: usize,
    /// S-bends: out, a step or less across, and on the same way again (not a stray, only a
    /// number: a part may stand a step off the pin it is wired to).
    pub jogs: usize,

    /// Flags (where a net leaves the sheet), power symbols, labels (a net's name at the end of
    /// a stub, where the net is not drawn as a wire), junction dots and X marks on pins that
    /// connect to nothing.
    pub flags: usize,
    pub power_symbols: usize,
    pub labels: usize,
    pub junctions: usize,
    pub open_pins: usize,

    /// Pair satellites, the crossings of their nets' wires with other wires, the longest straight
    /// run of wire of a net that ends in a power symbol, and the longest wire from a satellite's
    /// pin to what it serves, the last two in grid steps.
    pub pair_satellites: usize,
    pub pair_crossings: usize,
    pub rail_span: f64,
    pub satellite_wire: f64,

    /// Bends along the longest series chain of a spine, and the chains of the sheet that are
    /// not on the row of their IC nor near it.
    pub spine_bends: usize,
    pub detached_chain: usize,

    /// Interfaces drawn as one thick line, their members (the entries at each end count
    /// once), and the flags that end a bus where its members go on to another sheet (these are
    /// not in `flags`).
    pub buses: usize,
    pub bus_members: usize,
    pub bus_flags: usize,
}

impl SheetMetrics {
    /// Every way a wire serves nothing, added up.
    pub fn stray(&self) -> usize {
        self.dead_ends
            + self.redundant_wires
            + self.zero_wires
            + self.u_turns
            + self.unused_junctions
            + self.missing_junctions
            + self.wires_along_pins
            + self.wires_hugging
    }

    pub fn area(&self) -> f64 {
        self.width * self.height
    }

    /// Width over height; 0 for an empty sheet.
    pub fn aspect(&self) -> f64 {
        if self.height > 0.0 {
            self.width / self.height
        } else {
            0.0
        }
    }

    pub fn bends_per_net(&self) -> f64 {
        ratio(self.bends as f64, self.wired_nets as f64)
    }

    /// Wire length over the sum of the nets' half-perimeters; 0 when no net has two pins.
    pub fn length_ratio(&self) -> f64 {
        ratio(self.wire_length, self.half_perimeter)
    }

    /// [`length_ratio`](Self::length_ratio) for nets that do not end in a power symbol.
    pub fn signal_length_ratio(&self) -> f64 {
        ratio(self.signal_wire_length, self.signal_half_perimeter)
    }

    pub fn satellite_steps_mean(&self) -> f64 {
        ratio(self.satellite_steps_sum, self.satellites as f64)
    }

    /// The share of judged pins on the expected side, 0 to 1; 1 when none is judged.
    pub fn side_consistency(&self) -> f64 {
        if self.pins_judged == 0 {
            1.0
        } else {
            self.pins_on_side as f64 / self.pins_judged as f64
        }
    }

    /// Area per part, in square millimetres.
    pub fn area_per_part(&self) -> f64 {
        ratio(self.area(), self.parts as f64)
    }

    fn add(&mut self, o: &SheetMetrics) {
        self.parts += o.parts;
        self.units += o.units;
        self.wired_nets += o.wired_nets;
        self.wires += o.wires;
        self.segments += o.segments;
        self.diagonal_segments += o.diagonal_segments;
        self.bends += o.bends;
        self.max_bends_in_a_net = self.max_bends_in_a_net.max(o.max_bends_in_a_net);
        self.crossings += o.crossings;
        self.collinear += o.collinear;
        self.contacts += o.contacts;
        self.wire_length += o.wire_length;
        self.half_perimeter += o.half_perimeter;
        self.signal_wire_length += o.signal_wire_length;
        self.signal_half_perimeter += o.signal_half_perimeter;
        self.longest_detour = self.longest_detour.max(o.longest_detour);
        self.total_detour += o.total_detour;
        self.satellites += o.satellites;
        self.satellite_steps_sum += o.satellite_steps_sum;
        self.satellite_steps_max = self.satellite_steps_max.max(o.satellite_steps_max);
        self.pins_judged += o.pins_judged;
        self.pins_on_side += o.pins_on_side;
        self.symbol_overlaps += o.symbol_overlaps;
        self.text_overlaps += o.text_overlaps;
        self.wire_over_text += o.wire_over_text;
        self.wire_through_body += o.wire_through_body;
        self.dead_ends += o.dead_ends;
        self.redundant_wires += o.redundant_wires;
        self.zero_wires += o.zero_wires;
        self.u_turns += o.u_turns;
        self.unused_junctions += o.unused_junctions;
        self.missing_junctions += o.missing_junctions;
        self.wires_along_pins += o.wires_along_pins;
        self.wires_hugging += o.wires_hugging;
        self.jogs += o.jogs;
        self.flags += o.flags;
        self.power_symbols += o.power_symbols;
        self.labels += o.labels;
        self.junctions += o.junctions;
        self.open_pins += o.open_pins;
        self.pair_satellites += o.pair_satellites;
        self.pair_crossings += o.pair_crossings;
        self.rail_span = self.rail_span.max(o.rail_span);
        self.satellite_wire = self.satellite_wire.max(o.satellite_wire);
        self.spine_bends += o.spine_bends;
        self.detached_chain += o.detached_chain;
        self.buses += o.buses;
        self.bus_members += o.bus_members;
        self.bus_flags += o.bus_flags;
    }
}

fn ratio(a: f64, b: f64) -> f64 {
    if b > 0.0 { a / b } else { 0.0 }
}

/// The numbers of every sheet of a drawing, and their sum.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metrics {
    pub sheets: Vec<SheetMetrics>,
    /// The sum of the sheets (their greatest, for the `max` and `longest` numbers). Its size
    /// is the drawing's: the box round every sheet.
    pub total: SheetMetrics,
}

/// Measures every sheet of `layout`.
pub fn measure(layout: &Layout) -> Metrics {
    let sheets: Vec<SheetMetrics> = views(layout)
        .iter()
        .map(|v| measure_sheet(layout, v))
        .collect();
    let mut total = SheetMetrics {
        name: "total".into(),
        width: layout.bounds.width(),
        height: layout.bounds.height(),
        ..SheetMetrics::default()
    };
    for s in &sheets {
        total.add(s);
    }
    Metrics { sheets, total }
}

fn measure_sheet(l: &Layout, v: &SheetView) -> SheetMetrics {
    let mut m = SheetMetrics {
        name: v.name.clone(),
        width: v.frame.width(),
        height: v.frame.height(),
        parts: v.parts.len(),
        units: v
            .parts
            .iter()
            .filter(|&&i| l.parts[i].glyph.unit.is_some())
            .count(),
        wires: v.wires.len(),
        flags: v.ports.len(),
        power_symbols: v.symbols.len(),
        labels: v.labels.len(),
        junctions: v.junctions.len(),
        open_pins: v.marks.len(),
        buses: v.buses.len(),
        bus_members: v.buses.iter().map(|&b| l.buses[b].members.len()).sum(),
        bus_flags: v
            .buses
            .iter()
            .filter(|&&b| l.buses[b].flag.is_some())
            .count(),
        ..SheetMetrics::default()
    };
    m.diagonal_segments = v.segments.iter().filter(|s| !s.is_axis_aligned()).count();
    let segments: Vec<Seg> = v
        .segments
        .iter()
        .filter(|s| s.is_axis_aligned())
        .copied()
        .collect();
    m.segments = segments.len();

    let by_net = group_by_net(&segments);
    m.wired_nets = by_net.len();
    for segs in by_net.values() {
        let bends = bends_in(segs);
        m.bends += bends;
        m.max_bends_in_a_net = m.max_bends_in_a_net.max(bends);
    }
    meetings(&segments, &mut m);
    let bus_runs = bus_segments(l, v);
    for runs in group_by_net(&bus_runs).values() {
        let bends = bends_in(runs);
        m.bends += bends;
    }
    bus_meetings(l, &bus_runs, &segments, &mut m);
    wire_length(l, v, &by_net, &mut m);
    for (_, corners) in &v.wires {
        let (first, last) = (corners[0], corners[corners.len() - 1]);
        let routed: f64 = corners.windows(2).map(|p| manhattan(p[0], p[1])).sum();
        let detour = routed - manhattan(first, last);
        m.longest_detour = m.longest_detour.max(detour);
        m.total_detour += detour;
    }
    satellites(l, v, &mut m);
    pairs_and_rails(l, v, &segments, &bus_runs, &mut m);
    spines(l, v, &segments, &mut m);
    sides(l, v, &mut m);
    overlap_counts(l, v, &segments, &mut m);
    m.jogs = jogs(v);
    for f in wire_faults(l, v) {
        match f.fault {
            Fault::Zero => m.zero_wires += 1,
            Fault::Redundant { .. } => m.redundant_wires += 1,
            Fault::DeadEnd => m.dead_ends += 1,
            Fault::UTurn => m.u_turns += 1,
            Fault::UnusedJunction { .. } => m.unused_junctions += 1,
            Fault::MissingJunction { .. } => m.missing_junctions += 1,
            Fault::AlongPin { .. } => m.wires_along_pins += 1,
            Fault::HugsPart { .. } | Fault::HugsFlag { .. } => m.wires_hugging += 1,
        }
    }
    m
}

/// S-bends in the wires of a sheet: a corner, a short run of a step or less, and a corner
/// that turns back to the first direction.
fn jogs(v: &SheetView) -> usize {
    let mut n = 0;
    for (_, corners) in &v.wires {
        for w in corners.windows(4) {
            let (a, b, c, d) = (w[0], w[1], w[2], w[3]);
            let (first, across, last) = (
                Seg { net: 0, a, b },
                Seg { net: 0, a: b, b: c },
                Seg { net: 0, a: c, b: d },
            );
            let aligned = first.is_axis_aligned()
                && across.is_axis_aligned()
                && last.is_axis_aligned()
                && first.horizontal() == last.horizontal()
                && first.horizontal() != across.horizontal();
            let same_way = if first.horizontal() {
                (b.x - a.x) * (d.x - c.x) > 0.0
            } else {
                (b.y - a.y) * (d.y - c.y) > 0.0
            };
            if aligned && same_way && across.length() <= GRID + EPS {
                n += 1;
            }
        }
    }
    n
}

fn manhattan(a: Pt, b: Pt) -> f64 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

fn group_by_net(segments: &[Seg]) -> BTreeMap<usize, Vec<Seg>> {
    let mut by_net: BTreeMap<usize, Vec<Seg>> = BTreeMap::new();
    for s in segments {
        by_net.entry(s.net).or_default().push(*s);
    }
    by_net
}

/// Corners of one net: points where exactly two of its segments end, at right angles, and
/// no other segment of the net passes through.
fn bends_in(segs: &[Seg]) -> usize {
    let key = |p: Pt| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64);
    let mut ends: BTreeMap<(i64, i64), Vec<(usize, Pt)>> = BTreeMap::new();
    for (i, s) in segs.iter().enumerate() {
        ends.entry(key(s.a)).or_default().push((i, s.a));
        ends.entry(key(s.b)).or_default().push((i, s.b));
    }
    ends.values()
        .filter(|at| at.len() == 2 && at[0].0 != at[1].0)
        .filter(|at| {
            let (s, t) = (&segs[at[0].0], &segs[at[1].0]);
            s.horizontal() != t.horizontal()
        })
        .filter(|at| !segs.iter().any(|s| s.holds_inside(at[0].1)))
        .count()
}

/// Crossings, collinear runs and touching ends between wires of different nets.
fn meetings(segments: &[Seg], m: &mut SheetMetrics) {
    for (i, s) in segments.iter().enumerate() {
        for t in &segments[i + 1..] {
            if s.net == t.net {
                continue;
            }
            match relate(s, t) {
                Rel::Apart => {}
                Rel::Cross(_) => m.crossings += 1,
                Rel::Touch(_) => m.contacts += 1,
                Rel::Overlap { .. } => m.collinear += 1,
            }
        }
    }
}

/// The crossings, touches and collinear runs of the thick lines of buses with the wires and
/// with each other. A member's entry ends on its bus's line, which is not a contact.
fn bus_meetings(l: &Layout, runs: &[Seg], segments: &[Seg], m: &mut SheetMetrics) {
    let count = |rel: Rel, m: &mut SheetMetrics| match rel {
        Rel::Apart => {}
        Rel::Cross(_) => m.crossings += 1,
        Rel::Touch(_) => m.contacts += 1,
        Rel::Overlap { .. } => m.collinear += 1,
    };
    for (i, r) in runs.iter().enumerate() {
        let bus = bus_of(r.net).map(|b| &l.buses[b]);
        for s in segments {
            match relate(r, s) {
                Rel::Touch(p)
                    if bus.is_some_and(|b| {
                        b.entries.iter().any(|e| e.net == s.net && same(e.at, p))
                    }) => {}
                rel => count(rel, m),
            }
        }
        for t in &runs[i + 1..] {
            if t.net != r.net {
                count(relate(r, t), m);
            }
        }
    }
}

fn wire_length(
    l: &Layout,
    v: &SheetView,
    by_net: &BTreeMap<usize, Vec<Seg>>,
    m: &mut SheetMetrics,
) {
    let mut boxes: BTreeMap<usize, Rect> = BTreeMap::new();
    for &i in &v.parts {
        for pin in &l.parts[i].pins {
            if let Some(n) = pin.net {
                boxes
                    .entry(n)
                    .and_modify(|r| r.include(pin.at))
                    .or_insert_with(|| Rect::around(pin.at));
            }
        }
    }
    let powered: BTreeSet<usize> = v.symbols.iter().map(|&i| l.symbols[i].net).collect();
    for (net, r) in &boxes {
        let hp = r.width() + r.height();
        let Some(segs) = by_net.get(net) else {
            continue;
        };
        if hp < EPS {
            continue;
        }
        let len: f64 = segs.iter().map(Seg::length).sum();
        m.wire_length += len;
        m.half_perimeter += hp;
        if !powered.contains(net) {
            m.signal_wire_length += len;
            m.signal_half_perimeter += hp;
        }
    }
}

/// Whether the part is one the layout stands beside a pin: two pins facing opposite ways
/// along one line.
fn is_satellite(p: &PlacedPart) -> bool {
    let g = &p.glyph;
    if g.symbol.is_none() || g.pins.len() != 2 || g.unit.is_some() {
        return false;
    }
    let (a, b) = (&g.pins[0], &g.pins[1]);
    let opposite = a.toward_body.0 == -b.toward_body.0 && a.toward_body.1 == -b.toward_body.1;
    let along_x = a.toward_body.1 == 0.0 && a.at.1 == b.at.1;
    let along_y = a.toward_body.0 == 0.0 && a.at.0 == b.at.0;
    opposite && (along_x || along_y)
}

fn satellites(l: &Layout, v: &SheetView, m: &mut SheetMetrics) {
    // The pins of the parts that are not satellites, by net.
    let mut anchors: BTreeMap<usize, Vec<Pt>> = BTreeMap::new();
    for &i in &v.parts {
        let p = &l.parts[i];
        if is_satellite(p) {
            continue;
        }
        for (pin, placed) in p.glyph.pins.iter().zip(&p.pins) {
            if let (false, Some(n)) = (pin.hidden, placed.net) {
                anchors.entry(n).or_default().push(placed.at);
            }
        }
    }
    for &i in &v.parts {
        let p = &l.parts[i];
        if !is_satellite(p) {
            continue;
        }
        let nearest = p
            .pins
            .iter()
            .filter_map(|pin| {
                let at = anchors.get(&pin.net?)?;
                at.iter()
                    .map(|&q| manhattan(pin.at, q))
                    .min_by(f64::total_cmp)
            })
            .min_by(f64::total_cmp);
        if let Some(d) = nearest {
            let steps = d / GRID;
            m.satellites += 1;
            m.satellite_steps_sum += steps;
            m.satellite_steps_max = m.satellite_steps_max.max(steps);
        }
    }
}

/// The pair satellites of a sheet, each as its part and its two nets: satellites whose two pins
/// are on two different nets that carry no power symbol of the sheet, with a pin of one same
/// other part on each net.
fn pair_satellites(l: &Layout, v: &SheetView) -> Vec<(usize, [usize; 2])> {
    let powered: BTreeSet<usize> = v.symbols.iter().map(|&i| l.symbols[i].net).collect();
    // The nets each part that is not a satellite has a pin on.
    let mut main_nets: Vec<BTreeSet<usize>> = Vec::new();
    for &i in &v.parts {
        let p = &l.parts[i];
        let mut nets = BTreeSet::new();
        if !is_satellite(p) {
            for (pin, placed) in p.glyph.pins.iter().zip(&p.pins) {
                if let (false, Some(n)) = (pin.hidden, placed.net) {
                    nets.insert(n);
                }
            }
        }
        main_nets.push(nets);
    }
    let mut out = Vec::new();
    for &i in &v.parts {
        let p = &l.parts[i];
        if !is_satellite(p) {
            continue;
        }
        let (Some(a), Some(b)) = (p.pins[0].net, p.pins[1].net) else {
            continue;
        };
        if a == b || powered.contains(&a) || powered.contains(&b) {
            continue;
        }
        if main_nets.iter().any(|n| n.contains(&a) && n.contains(&b)) {
            out.push((i, [a, b]));
        }
    }
    out
}

/// The shortest way along the wire `segs` from `from` to any of `targets`: its length in
/// millimetres and the straight runs it is made of.
fn wire_path(segs: &[Seg], from: Pt, targets: &[Pt]) -> Option<(f64, Vec<(Pt, Pt)>)> {
    let key = |p: Pt| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64);
    let mut points: BTreeMap<(i64, i64), Pt> = BTreeMap::new();
    for s in segs {
        points.insert(key(s.a), s.a);
        points.insert(key(s.b), s.b);
    }
    points.insert(key(from), from);
    for &t in targets {
        points.insert(key(t), t);
    }
    let all: Vec<(i64, i64)> = points.keys().copied().collect();
    let index: BTreeMap<(i64, i64), usize> = all.iter().enumerate().map(|(i, &k)| (k, i)).collect();
    let mut edges: Vec<Vec<(usize, f64)>> = vec![Vec::new(); all.len()];
    for s in segs {
        // The points that lie on the segment, in order along it.
        let mut on: Vec<(f64, usize)> = all
            .iter()
            .filter(|k| s.holds(points[k]))
            .map(|k| (manhattan(s.a, points[k]), index[k]))
            .collect();
        on.sort_by(|x, y| x.0.total_cmp(&y.0));
        for w in on.windows(2) {
            let d = w[1].0 - w[0].0;
            edges[w[0].1].push((w[1].1, d));
            edges[w[1].1].push((w[0].1, d));
        }
    }
    let start = index[&key(from)];
    let goal: BTreeSet<usize> = targets.iter().map(|&t| index[&key(t)]).collect();
    let mut dist = vec![f64::INFINITY; all.len()];
    let mut before = vec![usize::MAX; all.len()];
    let mut done = vec![false; all.len()];
    dist[start] = 0.0;
    loop {
        let next = (0..all.len())
            .filter(|&i| !done[i] && dist[i].is_finite())
            .min_by(|&a, &b| dist[a].total_cmp(&dist[b]));
        let n = next?;
        if goal.contains(&n) {
            let mut runs = Vec::new();
            let mut at = n;
            while before[at] != usize::MAX {
                runs.push((points[&all[before[at]]], points[&all[at]]));
                at = before[at];
            }
            return Some((dist[n], runs));
        }
        done[n] = true;
        for &(m, d) in &edges[n] {
            if dist[n] + d < dist[m] {
                dist[m] = dist[n] + d;
                before[m] = n;
            }
        }
    }
}

/// Where the wires of a net could end: the pins of the parts that `keep` says to, and the
/// power symbols, flags and labels of the net on the sheet.
fn ends_of(l: &Layout, v: &SheetView, net: usize, keep: impl Fn(&PlacedPart) -> bool) -> Vec<Pt> {
    let mut targets: Vec<Pt> = Vec::new();
    for &j in &v.parts {
        let q = &l.parts[j];
        if !keep(q) {
            continue;
        }
        for (gp, placed) in q.glyph.pins.iter().zip(&q.pins) {
            if !gp.hidden && placed.net == Some(net) {
                targets.push(placed.at);
            }
        }
    }
    targets
}

/// The pair satellites, the crossings of their nets, the rail spans and the satellites' wires.
fn pairs_and_rails(
    l: &Layout,
    v: &SheetView,
    segments: &[Seg],
    runs: &[Seg],
    m: &mut SheetMetrics,
) {
    let pairs = pair_satellites(l, v);
    m.pair_satellites = pairs.len();
    let mut by_net: BTreeMap<usize, Vec<Seg>> = BTreeMap::new();
    for s in segments {
        by_net.entry(s.net).or_default().push(*s);
    }
    // The wire from each pin of a pair satellite to the pin of the other part on its net, and
    // where it crosses another net's wire (a crossing is counted once).
    let mut crossings: BTreeSet<(i64, i64)> = BTreeSet::new();
    for &(i, _) in &pairs {
        for pin in &l.parts[i].pins {
            let Some(net) = pin.net else { continue };
            let Some(segs) = by_net.get(&net) else {
                continue;
            };
            let targets = ends_of(l, v, net, |q| !is_satellite(q));
            let Some((_, path)) = wire_path(segs, pin.at, &targets) else {
                continue;
            };
            for (a, b) in path {
                let run = Seg { net, a, b };
                for t in segments.iter().chain(runs) {
                    if t.net == net {
                        continue;
                    }
                    if let Rel::Cross(p) = relate(&run, t) {
                        crossings
                            .insert(((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64));
                    }
                }
            }
        }
    }
    m.pair_crossings = crossings.len();

    // The longest straight run of a net that ends in a power symbol.
    let powered: BTreeSet<usize> = v.symbols.iter().map(|&i| l.symbols[i].net).collect();
    // Per net and line, the intervals along it; those that touch are one run.
    let mut lines: BTreeMap<(usize, bool, i64), Vec<(f64, f64)>> = BTreeMap::new();
    for s in segments.iter().filter(|s| powered.contains(&s.net)) {
        let (fixed, a, b) = if s.horizontal() {
            (s.a.y, s.a.x.min(s.b.x), s.a.x.max(s.b.x))
        } else {
            (s.a.x, s.a.y.min(s.b.y), s.a.y.max(s.b.y))
        };
        lines
            .entry((s.net, s.horizontal(), (fixed * 1000.0).round() as i64))
            .or_default()
            .push((a, b));
    }
    for spans in lines.values_mut() {
        spans.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut run = spans[0];
        let mut flush = |r: (f64, f64)| m.rail_span = m.rail_span.max((r.1 - r.0) / GRID);
        for &(a, b) in &spans[1..] {
            if a <= run.1 + EPS {
                run.1 = run.1.max(b);
            } else {
                flush(run);
                run = (a, b);
            }
        }
        flush(run);
    }

    // The longest wire from a satellite's pin to what it serves.
    for &i in &v.parts {
        let p = &l.parts[i];
        if !is_satellite(p) {
            continue;
        }
        for pin in &p.pins {
            let Some(net) = pin.net else { continue };
            let Some(segs) = by_net.get(&net) else {
                continue;
            };
            let mut targets = ends_of(l, v, net, |q| !is_satellite(q));
            targets.extend(
                v.symbols
                    .iter()
                    .filter(|&&k| l.symbols[k].net == net)
                    .map(|&k| l.symbols[k].at),
            );
            targets.extend(
                v.ports
                    .iter()
                    .filter(|&&k| l.ports[k].net == net)
                    .map(|&k| l.ports[k].at),
            );
            targets.extend(
                v.labels
                    .iter()
                    .filter(|&&k| l.labels[k].net == net)
                    .map(|&k| l.labels[k].at),
            );
            if targets.is_empty() {
                continue;
            }
            if let Some((d, _)) = wire_path(segs, pin.at, &targets) {
                m.satellite_wire = m.satellite_wire.max(d / GRID);
            }
        }
    }
}

/// A chain whose part is joined to a pin of its IC by a wire of at most this many steps is
/// attached to it.
pub const ATTACH_STEPS: f64 = 8.0;

/// The net the two parts have a pin on, and the pin of each on it.
fn meeting(a: &PlacedPart, b: &PlacedPart) -> Option<(usize, Pt, Pt)> {
    for pa in &a.pins {
        for pb in &b.pins {
            if let (Some(x), Some(y)) = (pa.net, pb.net)
                && x == y
            {
                return Some((x, pa.at, pb.at));
            }
        }
    }
    None
}

/// The bends of a path of runs, as `wire_path` gives them.
fn bends_on(path: &[(Pt, Pt)]) -> usize {
    let horizontal = |r: &(Pt, Pt)| (r.0.y - r.1.y).abs() < EPS;
    path.windows(2)
        .filter(|w| horizontal(&w[0]) != horizontal(&w[1]))
        .count()
}

/// `spine_bends` and `detached_chain`: the chains the layout found, read against the drawing.
fn spines(l: &Layout, v: &SheetView, segments: &[Seg], m: &mut SheetMetrics) {
    let by_net = group_by_net(segments);
    for mark in l.spines.iter().filter(|s| v.parts.contains(&s.ic)) {
        let ic = &l.parts[mark.ic];
        // The longest chain, the first of those that tie.
        let mut longest: Option<&Vec<usize>> = None;
        for c in &mark.chains {
            if longest.is_none_or(|b| c.len() > b.len()) {
                longest = Some(c);
            }
        }
        if let Some(chain) = longest {
            // From the IC's pin to the first part, then from part to part.
            let mut bends = 0;
            let first = &l.parts[chain[0]];
            let ic_net = first
                .pins
                .iter()
                .filter_map(|p| p.net)
                .find(|n| ic.pins.iter().any(|q| q.net == Some(*n)));
            if let Some(net) = ic_net
                && let Some(segs) = by_net.get(&net)
            {
                let targets: Vec<Pt> = ic
                    .pins
                    .iter()
                    .filter(|q| q.net == Some(net))
                    .map(|q| q.at)
                    .collect();
                let from = first.pins.iter().find(|p| p.net == Some(net)).map(|p| p.at);
                if let Some(from) = from
                    && let Some((_, path)) = wire_path(segs, from, &targets)
                {
                    bends += bends_on(&path);
                }
            }
            for w in chain.windows(2) {
                let Some((net, a, b)) = meeting(&l.parts[w[0]], &l.parts[w[1]]) else {
                    continue;
                };
                if let Some(segs) = by_net.get(&net)
                    && let Some((_, path)) = wire_path(segs, a, &[b])
                {
                    bends += bends_on(&path);
                }
            }
            m.spine_bends += bends;
        }
        // Which chains stand on the IC's row or are wired to it.
        let rows: Vec<f64> = ic.pins.iter().map(|q| q.at.y).collect();
        for chain in &mark.chains {
            let ys: Vec<f64> = chain
                .iter()
                .flat_map(|&p| l.parts[p].pins.iter().map(|q| q.at.y))
                .collect();
            let level = ys.windows(2).all(|w| (w[0] - w[1]).abs() < EPS)
                && ys
                    .first()
                    .is_some_and(|y| rows.iter().any(|r| (r - y).abs() < EPS));
            let wired = chain.iter().any(|&p| {
                l.parts[p].pins.iter().any(|pin| {
                    let Some(net) = pin.net else { return false };
                    let Some(segs) = by_net.get(&net) else {
                        return false;
                    };
                    let targets: Vec<Pt> = ic
                        .pins
                        .iter()
                        .filter(|q| q.net == Some(net))
                        .map(|q| q.at)
                        .collect();
                    !targets.is_empty()
                        && wire_path(segs, pin.at, &targets)
                            .is_some_and(|(d, _)| d / GRID <= ATTACH_STEPS)
                })
            });
            if !level && !wired {
                m.detached_chain += 1;
            }
        }
    }
}

fn sides(l: &Layout, v: &SheetView, m: &mut SheetMetrics) {
    let mut kind_of: BTreeMap<usize, PowerKind> = BTreeMap::new();
    for &i in &v.symbols {
        kind_of.insert(l.symbols[i].net, l.symbols[i].kind);
    }
    for &i in &v.parts {
        let p = &l.parts[i];
        if is_satellite(p) {
            continue;
        }
        for (pin, placed) in p.glyph.pins.iter().zip(&p.pins) {
            let Some(net) = placed.net else { continue };
            if pin.hidden {
                continue;
            }
            let want = if pin.kind.starts_with("power") {
                match kind_of.get(&net) {
                    Some(PowerKind::Ground) => Dir::Down,
                    Some(PowerKind::Supply) => Dir::Up,
                    None => continue,
                }
            } else if pin.flow.takes() {
                Dir::Left
            } else if pin.flow.gives() {
                Dir::Right
            } else {
                continue;
            };
            m.pins_judged += 1;
            if placed.escape == want {
                m.pins_on_side += 1;
            }
        }
    }
}

fn overlap_counts(l: &Layout, v: &SheetView, segments: &[Seg], m: &mut SheetMetrics) {
    let mut boxes: Vec<Rect> = v.parts.iter().map(|&i| symbol_box(&l.parts[i])).collect();
    boxes.extend(v.symbols.iter().map(|&i| l.symbols[i].glyph_extent()));
    boxes.extend(v.ports.iter().map(|&i| l.ports[i].extent()));
    boxes.extend(
        v.buses
            .iter()
            .filter_map(|&i| l.buses[i].flag.as_ref().map(|f| f.extent())),
    );
    m.symbol_overlaps = pairs_overlapping(boxes.iter());

    let written = texts(l, v);
    m.text_overlaps = pairs_overlapping(written.iter());
    let runs = bus_segments(l, v);
    m.wire_over_text = segments
        .iter()
        .chain(&runs)
        .map(|s| written.iter().filter(|t| runs_through(s, t)).count())
        .sum();
    let bodies: Vec<_> = v.parts.iter().map(|&i| body_strokes(&l.parts[i])).collect();
    m.wire_through_body = segments
        .iter()
        .chain(&runs)
        .map(|s| bodies.iter().filter(|b| runs_through_body(s, b)).count())
        .sum();
}

fn pairs_overlapping<'a>(boxes: impl Iterator<Item = &'a Rect>) -> usize {
    let boxes: Vec<&Rect> = boxes.collect();
    let mut n = 0;
    for (i, a) in boxes.iter().enumerate() {
        n += boxes[i + 1..]
            .iter()
            .filter(|b| overlaps(a, b, OVERLAP))
            .count();
    }
    n
}

impl Metrics {
    /// One line per sheet and a last one for the total, in columns, for reading and for
    /// comparing two runs with `diff`. The first line names the columns:
    ///
    /// `bends` and `b/net` are corners and corners per wired net; `cross`, `colin` and
    /// `touch` are the wire pairs of different nets that cross, lie along one line and
    /// touch with an end; `len/hp` is the length ratio and `sig` the same without power
    /// nets; `detour` the longest detour in mm; `sat` the satellites with the mean and the
    /// greatest distance in grid steps; `side` the share of pins on the expected side;
    /// `symOv`, `txtOv`, `wireTxt` and `wireBody` the overlap counts; `w x h` the size in
    /// mm and `asp` width over height; `flag`, `pwr`, `label`, `X` the flags, power symbols,
    /// labels (names at the end of a stub) and X marks; `bus` and `bmem` the buses and their
    /// members; `stray` the wires and dots that serve
    /// nothing (see above); `pairX` the crossings of pair satellites' wires, `rail` the
    /// longest rail run and `satW` the longest wire from a satellite to its pin, the last two in
    /// grid steps; `spineB` the bends along the longest series chain and `detach` the chains
    /// that are not on the IC's row.
    pub fn table(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{:<18} {:>5} {:>5} {:>5} {:>6} {:>5} {:>5} {:>5} {:>5} {:>6} {:>6} {:>7} {:>4} {:>5} {:>5} {:>6} {:>5} {:>5} {:>7} {:>8} {:>8} {:>5} {:>5} {:>5} {:>5} {:>3} {:>4} {:>4} {:>5} {:>5} {:>6} {:>6} {:>6} {:>6}",
            "sheet",
            "parts",
            "nets",
            "wires",
            "bends",
            "b/net",
            "cross",
            "colin",
            "touch",
            "len/hp",
            "sig",
            "detour",
            "sat",
            "satAvg",
            "satMax",
            "side",
            "symOv",
            "txtOv",
            "wireTxt",
            "wireBody",
            "w x h",
            "asp",
            "flag",
            "pwr",
            "label",
            "X",
            "bus",
            "bmem",
            "stray",
            "pairX",
            "rail",
            "satW",
            "spineB",
            "detach"
        );
        for s in self.sheets.iter().chain(std::iter::once(&self.total)) {
            let _ = writeln!(
                out,
                "{:<18} {:>5} {:>5} {:>5} {:>6} {:>5.2} {:>5} {:>5} {:>5} {:>6.2} {:>6.2} {:>7.2} {:>4} {:>5.1} {:>5.1} {:>6.2} {:>5} {:>5} {:>7} {:>8} {:>8} {:>5.2} {:>5} {:>5} {:>5} {:>3} {:>4} {:>4} {:>5} {:>5} {:>6.1} {:>6.1} {:>6} {:>6}",
                clip(&s.name, 18),
                s.parts,
                s.wired_nets,
                s.wires,
                s.bends,
                s.bends_per_net(),
                s.crossings,
                s.collinear,
                s.contacts,
                s.length_ratio(),
                s.signal_length_ratio(),
                s.longest_detour,
                s.satellites,
                s.satellite_steps_mean(),
                s.satellite_steps_max,
                s.side_consistency(),
                s.symbol_overlaps,
                s.text_overlaps,
                s.wire_over_text,
                s.wire_through_body,
                format!("{:.0}x{:.0}", s.width, s.height),
                s.aspect(),
                s.flags,
                s.power_symbols,
                s.labels,
                s.open_pins,
                s.buses,
                s.bus_members,
                s.stray(),
                s.pair_crossings,
                s.rail_span,
                s.satellite_wire,
                s.spine_bends,
                s.detached_chain,
            );
        }
        out
    }

    /// Every number of every sheet as `sheet.name = value` lines, in a fixed order.
    pub fn plain(&self) -> String {
        let mut out = String::new();
        for s in self.sheets.iter().chain(std::iter::once(&self.total)) {
            let mut put = |key: &str, value: String| {
                let _ = writeln!(out, "{}.{key} = {value}", s.name);
            };
            let n = |v: usize| v.to_string();
            let f = |v: f64| format!("{v:.3}");
            put("width", f(s.width));
            put("height", f(s.height));
            put("area", f(s.area()));
            put("aspect", f(s.aspect()));
            put("area_per_part", f(s.area_per_part()));
            put("parts", n(s.parts));
            put("units", n(s.units));
            put("wired_nets", n(s.wired_nets));
            put("wires", n(s.wires));
            put("segments", n(s.segments));
            put("diagonal_segments", n(s.diagonal_segments));
            put("bends", n(s.bends));
            put("bends_per_net", f(s.bends_per_net()));
            put("max_bends_in_a_net", n(s.max_bends_in_a_net));
            put("crossings", n(s.crossings));
            put("collinear", n(s.collinear));
            put("contacts", n(s.contacts));
            put("wire_length", f(s.wire_length));
            put("half_perimeter", f(s.half_perimeter));
            put("length_ratio", f(s.length_ratio()));
            put("signal_length_ratio", f(s.signal_length_ratio()));
            put("longest_detour", f(s.longest_detour));
            put("satellites", n(s.satellites));
            put("satellite_steps_mean", f(s.satellite_steps_mean()));
            put("satellite_steps_max", f(s.satellite_steps_max));
            put("pins_judged", n(s.pins_judged));
            put("side_consistency", f(s.side_consistency()));
            put("symbol_overlaps", n(s.symbol_overlaps));
            put("text_overlaps", n(s.text_overlaps));
            put("wire_over_text", n(s.wire_over_text));
            put("wire_through_body", n(s.wire_through_body));
            put("dead_ends", n(s.dead_ends));
            put("redundant_wires", n(s.redundant_wires));
            put("zero_wires", n(s.zero_wires));
            put("u_turns", n(s.u_turns));
            put("unused_junctions", n(s.unused_junctions));
            put("missing_junctions", n(s.missing_junctions));
            put("wires_along_pins", n(s.wires_along_pins));
            put("wires_hugging", n(s.wires_hugging));
            put("jogs", n(s.jogs));
            put("stray", n(s.stray()));
            put("flags", n(s.flags));
            put("power_symbols", n(s.power_symbols));
            put("labels", n(s.labels));
            put("junctions", n(s.junctions));
            put("open_pins", n(s.open_pins));
            put("buses", n(s.buses));
            put("bus_members", n(s.bus_members));
            put("bus_flags", n(s.bus_flags));
            put("pair_satellites", n(s.pair_satellites));
            put("pair_crossings", n(s.pair_crossings));
            put("rail_span", f(s.rail_span));
            put("satellite_wire", f(s.satellite_wire));
            put("spine_bends", n(s.spine_bends));
            put("detached_chain", n(s.detached_chain));
        }
        out
    }
}

fn clip(name: &str, width: usize) -> String {
    if name.chars().count() <= width {
        name.to_string()
    } else {
        let head: String = name.chars().take(width - 1).collect();
        format!("{head}…")
    }
}
