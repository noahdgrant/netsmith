//! A layout one sheet at a time, as straight segments and boxes: what [`crate::metrics`] and
//! [`crate::check`] both read. Nothing here is drawn.

use crate::geom::{Pt, Rect};
use crate::glyph::{P, Shape};
use crate::layout::{Layout, PlacedPart, Text};

/// Two coordinates closer than this are the same (millimetres).
pub(crate) const EPS: f64 = 1e-6;

/// How far two boxes may overlap before they count as overlapping, as in the tests of the
/// drawing.
pub(crate) const OVERLAP: f64 = 0.05;

/// Half the side of the X on a pin that connects to nothing.
pub(crate) const MARK_HALF: f64 = 0.5;

/// How far a body or a text is shrunk before a wire is said to run through it.
pub(crate) const INSET: f64 = 0.1;

/// One straight run of wire.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Seg {
    pub net: usize,
    pub a: Pt,
    pub b: Pt,
}

impl Seg {
    pub fn horizontal(&self) -> bool {
        (self.a.y - self.b.y).abs() < EPS && (self.a.x - self.b.x).abs() >= EPS
    }

    pub fn vertical(&self) -> bool {
        (self.a.x - self.b.x).abs() < EPS && (self.a.y - self.b.y).abs() >= EPS
    }

    pub fn is_axis_aligned(&self) -> bool {
        self.horizontal() || self.vertical()
    }

    pub fn length(&self) -> f64 {
        (self.a.x - self.b.x).abs() + (self.a.y - self.b.y).abs()
    }

    pub fn rect(&self) -> Rect {
        let mut r = Rect::around(self.a);
        r.include(self.b);
        r
    }

    /// Whether `p` is on the segment, ends included.
    pub fn holds(&self, p: Pt) -> bool {
        let r = self.rect();
        if self.horizontal() {
            (p.y - self.a.y).abs() < EPS && p.x >= r.min.x - EPS && p.x <= r.max.x + EPS
        } else if self.vertical() {
            (p.x - self.a.x).abs() < EPS && p.y >= r.min.y - EPS && p.y <= r.max.y + EPS
        } else {
            false
        }
    }

    /// Whether `p` is on the segment and not at either end.
    pub fn holds_inside(&self, p: Pt) -> bool {
        self.holds(p) && !same(p, self.a) && !same(p, self.b)
    }
}

pub(crate) fn same(a: Pt, b: Pt) -> bool {
    (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS
}

/// How two segments meet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Rel {
    Apart,
    /// Square across each other, the point inside both.
    Cross(Pt),
    /// They share a point, and it is an end of at least one of them (a corner, a T, two ends
    /// meeting along a line).
    Touch(Pt),
    /// They lie along one line over this length.
    Overlap {
        at: Pt,
        len: f64,
    },
}

/// How `s` and `t` meet. Segments that are not horizontal or vertical are `Apart`.
pub(crate) fn relate(s: &Seg, t: &Seg) -> Rel {
    let (rs, rt) = (s.rect(), t.rect());
    if rs.min.x > rt.max.x + EPS
        || rt.min.x > rs.max.x + EPS
        || rs.min.y > rt.max.y + EPS
        || rt.min.y > rs.max.y + EPS
    {
        return Rel::Apart;
    }
    match (s.horizontal(), s.vertical(), t.horizontal(), t.vertical()) {
        (true, _, true, _) => {
            if (s.a.y - t.a.y).abs() >= EPS {
                return Rel::Apart;
            }
            let (lo, hi) = (rs.min.x.max(rt.min.x), rs.max.x.min(rt.max.x));
            if hi - lo > EPS {
                Rel::Overlap {
                    at: Pt::new(lo, s.a.y),
                    len: hi - lo,
                }
            } else {
                Rel::Touch(Pt::new(lo, s.a.y))
            }
        }
        (_, true, _, true) => {
            if (s.a.x - t.a.x).abs() >= EPS {
                return Rel::Apart;
            }
            let (lo, hi) = (rs.min.y.max(rt.min.y), rs.max.y.min(rt.max.y));
            if hi - lo > EPS {
                Rel::Overlap {
                    at: Pt::new(s.a.x, lo),
                    len: hi - lo,
                }
            } else {
                Rel::Touch(Pt::new(s.a.x, lo))
            }
        }
        (true, _, _, true) => meet(s, t),
        (_, true, true, _) => meet(t, s),
        _ => Rel::Apart,
    }
}

/// A horizontal segment `h` and a vertical one `v`.
fn meet(h: &Seg, v: &Seg) -> Rel {
    let p = Pt::new(v.a.x, h.a.y);
    if !h.holds(p) || !v.holds(p) {
        return Rel::Apart;
    }
    if h.holds_inside(p) && v.holds_inside(p) {
        Rel::Cross(p)
    } else {
        Rel::Touch(p)
    }
}

/// Whether the open boxes `a` and `b` overlap by more than `tolerance` in both directions.
pub(crate) fn overlaps(a: &Rect, b: &Rect, tolerance: f64) -> bool {
    a.min.x < b.max.x - tolerance
        && b.min.x < a.max.x - tolerance
        && a.min.y < b.max.y - tolerance
        && b.min.y < a.max.y - tolerance
}

pub(crate) fn shrunk(r: Rect, by: f64) -> Rect {
    Rect {
        min: Pt::new(r.min.x + by, r.min.y + by),
        max: Pt::new(r.max.x - by, r.max.y - by),
    }
}

/// Whether a segment runs through the inside of `r` (shrunk by [`INSET`]).
pub(crate) fn runs_through(s: &Seg, r: &Rect) -> bool {
    let inner = shrunk(*r, INSET);
    inner.min.x < inner.max.x && inner.min.y < inner.max.y && overlaps(&inner, &s.rect(), 0.0)
}

/// The corners of a polyline with repeated points and straight-through points taken out.
pub(crate) fn simplify(points: &[Pt]) -> Vec<Pt> {
    let mut out: Vec<Pt> = Vec::with_capacity(points.len());
    for &p in points {
        if out.last().is_some_and(|&q| same(p, q)) {
            continue;
        }
        while out.len() >= 2 {
            let (a, b) = (out[out.len() - 2], out[out.len() - 1]);
            let straight = ((a.x - b.x).abs() < EPS && (b.x - p.x).abs() < EPS)
                || ((a.y - b.y).abs() < EPS && (b.y - p.y).abs() < EPS);
            if straight {
                out.pop();
            } else {
                break;
            }
        }
        out.push(p);
    }
    out
}

/// The box a symbol's body and pins take, on the sheet.
pub(crate) fn symbol_box(part: &PlacedPart) -> Rect {
    on_sheet(part, part.glyph.bounds())
}

/// What a symbol's body is drawn with, on the sheet: the lines of its outline, the inside
/// of its boxes, its circles.
pub(crate) enum Stroke {
    Line(Pt, Pt),
    Inside(Rect),
    Disc(Pt, f64),
}

/// The strokes of a part's drawn body (its shapes, not its pins or its texts).
pub(crate) fn body_strokes(part: &PlacedPart) -> Vec<Stroke> {
    let at = |p: P| {
        let q = part.orient.apply(p);
        Pt::new(q.x + part.at.x, q.y + part.at.y)
    };
    let mut out = Vec::new();
    let polyline = |pts: &[Pt], out: &mut Vec<Stroke>| {
        for w in pts.windows(2) {
            out.push(Stroke::Line(w[0], w[1]));
        }
    };
    for shape in &part.glyph.shapes {
        match shape {
            Shape::Rect { a, b, .. } => {
                let (a, b) = (at(*a), at(*b));
                let mut r = Rect::around(a);
                r.include(b);
                let corners = [
                    Pt::new(r.min.x, r.min.y),
                    Pt::new(r.max.x, r.min.y),
                    Pt::new(r.max.x, r.max.y),
                    Pt::new(r.min.x, r.max.y),
                    Pt::new(r.min.x, r.min.y),
                ];
                polyline(&corners, &mut out);
                out.push(Stroke::Inside(r));
            }
            Shape::Poly { pts, .. } => {
                let pts: Vec<Pt> = pts.iter().map(|&p| at(p)).collect();
                polyline(&pts, &mut out);
            }
            Shape::Circle { center, radius, .. } => out.push(Stroke::Disc(at(*center), *radius)),
            Shape::Arc {
                start, mid, end, ..
            } => polyline(&[at(*start), at(*mid), at(*end)], &mut out),
            Shape::Text { .. } => {}
        }
    }
    out
}

/// Whether the wire `s` runs through the body drawn with `strokes`: it crosses a line
/// of the outline (not merely ending on one), lies along one, or runs inside a box or a
/// circle.
pub(crate) fn runs_through_body(s: &Seg, strokes: &[Stroke]) -> bool {
    strokes.iter().any(|k| match k {
        Stroke::Line(p, q) => crosses_line(s, *p, *q),
        Stroke::Inside(r) => {
            let inner = shrunk(*r, INSET);
            inner.min.x < inner.max.x
                && inner.min.y < inner.max.y
                && overlaps(&inner, &s.rect(), 0.0)
        }
        Stroke::Disc(c, radius) => {
            let r = s.rect();
            let nearest = Pt::new(c.x.clamp(r.min.x, r.max.x), c.y.clamp(r.min.y, r.max.y));
            let d = ((nearest.x - c.x).powi(2) + (nearest.y - c.y).powi(2)).sqrt();
            d < radius - INSET
        }
    })
}

/// Whether the wire `s` crosses the line from `p` to `q` somewhere that is inside the
/// wire, or runs along it for more than [`INSET`].
fn crosses_line(s: &Seg, p: Pt, q: Pt) -> bool {
    let line = Seg {
        net: s.net,
        a: p,
        b: q,
    };
    if line.is_axis_aligned() {
        return match relate(s, &line) {
            Rel::Cross(_) => true,
            Rel::Overlap { len, .. } => len > INSET,
            _ => false,
        };
    }
    if (p.x - q.x).abs() < EPS && (p.y - q.y).abs() < EPS {
        return false;
    }
    // The wire is horizontal or vertical; the line is slanted.
    let (t, along) = if s.horizontal() {
        (
            (s.a.y - p.y) / (q.y - p.y),
            p.x + (s.a.y - p.y) / (q.y - p.y) * (q.x - p.x),
        )
    } else {
        (
            (s.a.x - p.x) / (q.x - p.x),
            p.y + (s.a.x - p.x) / (q.x - p.x) * (q.y - p.y),
        )
    };
    let r = s.rect();
    let (lo, hi) = if s.horizontal() {
        (r.min.x, r.max.x)
    } else {
        (r.min.y, r.max.y)
    };
    t.is_finite() && t > -EPS && t < 1.0 + EPS && along > lo + EPS && along < hi - EPS
}

fn on_sheet(part: &PlacedPart, symbol_space: Rect) -> Rect {
    let corner = |x: f64, y: f64| {
        let p = part.orient.apply((x, y));
        Pt::new(p.x + part.at.x, p.y + part.at.y)
    };
    let mut r = Rect::around(corner(symbol_space.min.x, symbol_space.min.y));
    r.include(corner(symbol_space.max.x, symbol_space.max.y));
    r.include(corner(symbol_space.min.x, symbol_space.max.y));
    r.include(corner(symbol_space.max.x, symbol_space.min.y));
    r
}

/// Everything of a layout that is on one sheet (or the whole drawing, when it has no sheets).
pub(crate) struct SheetView {
    pub name: String,
    pub frame: Rect,
    /// Indices into `layout.parts`.
    pub parts: Vec<usize>,
    /// Wires, as straight segments.
    pub segments: Vec<Seg>,
    /// The wires of the sheet, each as its corners.
    pub wires: Vec<(usize, Vec<Pt>)>,
    /// Wires that are a point, or go from a point to itself: the net and where.
    pub zero_wires: Vec<(usize, Pt)>,
    /// Indices into `layout.symbols`, `.ports`, `.junctions`, `.marks`, `.labels`.
    pub symbols: Vec<usize>,
    pub ports: Vec<usize>,
    pub junctions: Vec<usize>,
    pub marks: Vec<usize>,
    pub labels: Vec<usize>,
    /// Indices into `layout.buses`.
    pub buses: Vec<usize>,
}

/// The first number [`bus_net`] gives a bus: above any net of a design.
const BUS_NET: usize = usize::MAX / 2;

/// The `net` of the segments of the thick line of bus `i`, so that they are told from the
/// segments of the nets and grouped as one.
pub(crate) fn bus_net(i: usize) -> usize {
    BUS_NET + i
}

/// The bus whose segments carry `net`, if they are a bus's.
pub(crate) fn bus_of(net: usize) -> Option<usize> {
    net.checked_sub(BUS_NET).filter(|_| net >= BUS_NET)
}

/// The straight runs of the thick line of every bus of a sheet, as segments of [`bus_net`].
pub(crate) fn bus_segments(l: &Layout, v: &SheetView) -> Vec<Seg> {
    let mut out = Vec::new();
    for &i in &v.buses {
        for (a, b) in l.buses[i].runs() {
            out.push(Seg {
                net: bus_net(i),
                a,
                b,
            });
        }
    }
    out
}

/// The sheets of `l`, or one view of everything when it has none. Each item belongs to the
/// sheet whose frame holds it (the nearest, if none does).
pub(crate) fn views(l: &Layout) -> Vec<SheetView> {
    let empty = |name: String, frame: Rect, parts: Vec<usize>| SheetView {
        name,
        frame,
        parts,
        segments: Vec::new(),
        wires: Vec::new(),
        zero_wires: Vec::new(),
        symbols: Vec::new(),
        ports: Vec::new(),
        junctions: Vec::new(),
        marks: Vec::new(),
        labels: Vec::new(),
        buses: Vec::new(),
    };
    let mut out: Vec<SheetView> = if l.sheets.is_empty() {
        vec![empty(
            "board".into(),
            l.bounds,
            (0..l.parts.len()).collect(),
        )]
    } else {
        l.sheets
            .iter()
            .map(|s| empty(s.name.clone(), s.frame, s.parts.clone()))
            .collect()
    };
    let home = |views: &[SheetView], p: Pt| -> usize {
        let dist = |r: &Rect| {
            let dx = (r.min.x - p.x).max(p.x - r.max.x).max(0.0);
            let dy = (r.min.y - p.y).max(p.y - r.max.y).max(0.0);
            dx + dy
        };
        (0..views.len())
            .min_by(|&i, &j| dist(&views[i].frame).total_cmp(&dist(&views[j].frame)))
            .unwrap_or(0)
    };
    for w in &l.wires {
        let corners = simplify(&w.points);
        let Some(&first) = corners.first() else {
            continue;
        };
        let v = home(&out, first);
        if w.points.len() < 2 || w.points.windows(2).any(|p| same(p[0], p[1])) {
            out[v].zero_wires.push((w.net, first));
        }
        for pair in corners.windows(2) {
            out[v].segments.push(Seg {
                net: w.net,
                a: pair[0],
                b: pair[1],
            });
        }
        out[v].wires.push((w.net, corners));
    }
    for (i, s) in l.symbols.iter().enumerate() {
        let h = home(&out, s.at);
        out[h].symbols.push(i);
    }
    for (i, p) in l.ports.iter().enumerate() {
        let h = home(&out, p.at);
        out[h].ports.push(i);
    }
    for (i, j) in l.junctions.iter().enumerate() {
        let h = home(&out, j.at);
        out[h].junctions.push(i);
    }
    for (i, m) in l.marks.iter().enumerate() {
        let h = home(&out, m.at);
        out[h].marks.push(i);
    }
    for (i, t) in l.labels.iter().enumerate() {
        let h = home(&out, t.text.at);
        out[h].labels.push(i);
    }
    for (i, b) in l.buses.iter().enumerate() {
        let at = b.lines.first().and_then(|line| line.first()).copied();
        let h = home(&out, at.unwrap_or(Pt::new(0.0, 0.0)));
        out[h].buses.push(i);
    }
    out
}

/// The box of a text as it is drawn: the layout measures every letter at `CHAR_WIDTH` of the
/// text's height, which is short for capitals and long for lower case in the page's font, so
/// the check measures each letter by its class ([`crate::glyph::text_em`]).
fn written(t: &Text, size: f64) -> Rect {
    use crate::layout::Anchor;
    let mut r = t.extent_at(size);
    // The box reaches a whole text height above the line; capitals and ascenders reach 0.75.
    if t.vertical {
        r.min.x += 0.25 * size;
    } else {
        r.min.y += 0.25 * size;
    }
    let more = size * crate::glyph::text_em(&t.text)
        - t.text.chars().count() as f64 * size * crate::glyph::CHAR_WIDTH;
    if t.vertical {
        match t.anchor {
            Anchor::Start => r.min.y -= more,
            Anchor::End => r.max.y += more,
            Anchor::Middle => {
                r.min.y -= more / 2.0;
                r.max.y += more / 2.0;
            }
        }
    } else {
        match t.anchor {
            Anchor::Start => r.max.x += more,
            Anchor::End => r.min.x -= more,
            Anchor::Middle => {
                r.min.x -= more / 2.0;
                r.max.x += more / 2.0;
            }
        }
    }
    r
}

/// The box of every text of a sheet: references and values, pin numbers and names, the names on power
/// symbols and flags, and labels; and the X marks, which are as much in the way.
pub(crate) fn texts(l: &Layout, v: &SheetView) -> Vec<Rect> {
    let mut out = Vec::new();
    for &i in &v.parts {
        let p = &l.parts[i];
        for t in std::iter::once(&p.reference).chain(p.value.as_ref()) {
            out.push(written(t, crate::layout::TEXT_SIZE));
        }
        for (pin, placed) in p.glyph.pins.iter().zip(&p.pins) {
            if pin.hidden {
                continue;
            }
            let toward = p.orient.dir(pin.toward_body);
            let (number, name) = p.glyph.pin_texts(pin, placed.at, toward);
            // Digits have nothing below the line they sit on.
            if let Some(t) = number {
                let mut r = written(&t, p.glyph.pin_text);
                if !t.vertical {
                    r.max.y -= 0.3 * p.glyph.pin_text;
                }
                out.push(r);
            }
            if let Some(t) = name {
                out.push(written(&t, p.glyph.pin_text));
            }
        }
    }
    for &i in &v.symbols {
        if let Some(t) = &l.symbols[i].label {
            out.push(written(t, crate::layout::TEXT_SIZE));
        }
    }
    for &i in &v.ports {
        if let Some(t) = &l.ports[i].label {
            out.push(written(t, crate::layout::TEXT_SIZE));
        }
    }
    for &i in &v.labels {
        out.push(written(&l.labels[i].text, crate::layout::TEXT_SIZE));
    }
    for &i in &v.buses {
        let b = &l.buses[i];
        for t in [&b.label, &b.list].into_iter().chain(b.other.as_ref()) {
            out.push(written(t, crate::layout::TEXT_SIZE));
        }
        out.push(written(&b.count, crate::layout::TEXT_SIZE));
        if let Some(f) = &b.flag {
            out.push(written(&f.label, crate::layout::Port::TEXT));
        }
    }
    // An X on a pin is as much in the way of a text as a text is.
    for &i in &v.marks {
        let at = l.marks[i].at;
        out.push(Rect {
            min: Pt::new(at.x - MARK_HALF, at.y - MARK_HALF),
            max: Pt::new(at.x + MARK_HALF, at.y + MARK_HALF),
        });
    }
    out
}

/// One grid step, in millimetres: how near a wire may come back to itself.
const STEP: f64 = 1.27;

/// What is wrong with a wire or a dot, found from the wires alone.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Fault {
    /// A wire that is a point, or has two equal points in a row.
    Zero,
    /// Two wires of one net along one line over `len`.
    Redundant { len: f64 },
    /// An end that touches no pin, symbol, flag, label, X or other wire of its net.
    DeadEnd,
    /// Out, across a step or less, and back.
    UTurn,
    /// A dot where fewer than three arms of its net meet (or a second dot at one point).
    UnusedJunction { arms: usize },
    /// Three arms or more meet and there is no dot.
    MissingJunction { arms: usize },
    /// A wire runs along the line of a pin of a part (index in the design) with this pad.
    AlongPin { part: usize, pad: String },
    /// A wire runs along the outline of the body of this part (index in the design) closer
    /// than [`HUG`], without being on it.
    HugsPart { part: usize },
    /// The same for the flag of this net (index in the design).
    HugsFlag { net: usize },
}

/// A wire that runs along a body's outline or a flag's closer than this (millimetres), for
/// longer than [`HUG_LEN`], is hugging it.
pub(crate) const HUG: f64 = 0.7;
const HUG_LEN: f64 = 1.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Found {
    pub net: usize,
    pub at: Pt,
    pub fault: Fault,
}

/// Every fault of the wires and dots of a sheet, in a fixed order.
pub(crate) fn wire_faults(l: &Layout, v: &SheetView) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    let mut found = |net: usize, at: Pt, fault: Fault| out.push(Found { net, at, fault });

    // Where a wire may end besides another wire: a pin, a power symbol, a flag, a label, an X.
    let mut anchors: Vec<Pt> = Vec::new();
    for &i in &v.parts {
        anchors.extend(l.parts[i].pins.iter().map(|p| p.at));
    }
    anchors.extend(v.symbols.iter().map(|&i| l.symbols[i].at));
    anchors.extend(v.ports.iter().map(|&i| l.ports[i].at));
    anchors.extend(v.labels.iter().map(|&i| l.labels[i].at));
    // Where a member's wire meets the thick line of its bus.
    for &i in &v.buses {
        anchors.extend(l.buses[i].entries.iter().map(|e| e.at));
    }
    anchors.extend(v.marks.iter().map(|&i| l.marks[i].at));
    let anchored = |p: Pt| anchors.iter().any(|&q| same(p, q));
    let key = |p: Pt| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64);

    for &(net, at) in &v.zero_wires {
        found(net, at, Fault::Zero);
    }

    let segs: Vec<&Seg> = v.segments.iter().filter(|s| s.is_axis_aligned()).collect();
    for (i, s) in segs.iter().enumerate() {
        for t in &segs[i + 1..] {
            if s.net == t.net
                && let Rel::Overlap { at, len } = relate(s, t)
            {
                found(s.net, at, Fault::Redundant { len });
            }
        }
    }

    // Dead ends: an end that no anchor and no other wire of the net touches.
    let mut seen: std::collections::BTreeSet<(usize, (i64, i64))> = Default::default();
    for (i, s) in segs.iter().enumerate() {
        for p in [s.a, s.b] {
            let touched = anchored(p)
                || segs
                    .iter()
                    .enumerate()
                    .any(|(j, t)| j != i && t.net == s.net && t.holds(p));
            if !touched && seen.insert((s.net, key(p))) {
                found(s.net, p, Fault::DeadEnd);
            }
        }
    }

    // Dots: the arms of a net at a point, a straight wire through it two, an end one, and a
    // pin (or symbol, flag, label) there one.
    let arms = |net: usize, p: Pt| -> usize {
        // A pin, symbol, flag or label at the point is one more arm: two wires and a pin
        // meeting look like a T.
        usize::from(anchored(p))
            + segs
                .iter()
                .filter(|t| t.net == net)
                .map(|t| {
                    if same(t.a, p) || same(t.b, p) {
                        1
                    } else if t.holds_inside(p) {
                        2
                    } else {
                        0
                    }
                })
                .sum::<usize>()
    };
    let mut points: std::collections::BTreeMap<(usize, (i64, i64)), Pt> = Default::default();
    for (i, s) in segs.iter().enumerate() {
        for p in [s.a, s.b] {
            points.insert((s.net, key(p)), p);
        }
        for t in &segs[i + 1..] {
            if s.net == t.net
                && let Rel::Cross(p) = relate(s, t)
            {
                points.insert((s.net, key(p)), p);
            }
        }
    }
    let dots: Vec<(usize, Pt)> = v
        .junctions
        .iter()
        .map(|&i| (l.junctions[i].net, l.junctions[i].at))
        .collect();
    for (&(net, _), &p) in &points {
        let n = arms(net, p);
        if n >= 3 && !dots.iter().any(|&(dn, d)| dn == net && same(d, p)) {
            found(net, p, Fault::MissingJunction { arms: n });
        }
    }
    let mut dotted: std::collections::BTreeSet<(usize, (i64, i64))> = Default::default();
    for &(net, p) in &dots {
        let n = arms(net, p);
        if n < 3 || !dotted.insert((net, key(p))) {
            found(net, p, Fault::UnusedJunction { arms: n });
        }
    }

    // Hooks: out, across a step or less, and back.
    for (net, corners) in &v.wires {
        for w in corners.windows(4) {
            let (a, b, c, d) = (w[0], w[1], w[2], w[3]);
            let first = Seg { net: *net, a, b };
            let across = Seg {
                net: *net,
                a: b,
                b: c,
            };
            let last = Seg {
                net: *net,
                a: c,
                b: d,
            };
            let hook = first.is_axis_aligned()
                && across.is_axis_aligned()
                && last.is_axis_aligned()
                && first.horizontal() == last.horizontal()
                && across.length() <= STEP + EPS
                && ((first.horizontal() && (b.x - a.x) * (d.x - c.x) < 0.0)
                    || (first.vertical() && (b.y - a.y) * (d.y - c.y) < 0.0));
            if hook && !(anchored(a) && anchored(d)) {
                found(*net, b, Fault::UTurn);
            }
        }
    }

    // Along the line of a pin.
    for &i in &v.parts {
        let part = &l.parts[i];
        for (g, placed) in part.glyph.pins.iter().zip(&part.pins) {
            if g.hidden {
                continue;
            }
            let inner = placed.at.step(part.orient.dir(g.toward_body), g.length);
            let line = Seg {
                net: usize::MAX,
                a: placed.at,
                b: inner,
            };
            for s in &segs {
                if let Rel::Overlap { at, len } = relate(s, &line)
                    && len > EPS
                {
                    found(
                        s.net,
                        at,
                        Fault::AlongPin {
                            part: part.part,
                            pad: placed.id.clone(),
                        },
                    );
                }
            }
        }
    }
    // Along the outline of a body or a flag, closer than a wire should be.
    let hug = |s: &Seg, a: Pt, b: Pt| -> Option<Pt> {
        let line = Seg { net: s.net, a, b };
        if !line.is_axis_aligned() || s.horizontal() != line.horizontal() {
            return None;
        }
        let (r, q) = (s.rect(), line.rect());
        let (gap, lo, hi) = if s.horizontal() {
            (
                (s.a.y - a.y).abs(),
                r.min.x.max(q.min.x),
                r.max.x.min(q.max.x),
            )
        } else {
            (
                (s.a.x - a.x).abs(),
                r.min.y.max(q.min.y),
                r.max.y.min(q.max.y),
            )
        };
        (gap > EPS && gap < HUG && hi - lo > HUG_LEN).then(|| {
            if s.horizontal() {
                Pt::new(lo, s.a.y)
            } else {
                Pt::new(s.a.x, lo)
            }
        })
    };
    for &i in &v.parts {
        for stroke in body_strokes(&l.parts[i]) {
            if let Stroke::Line(a, b) = stroke {
                for s in &segs {
                    if let Some(at) = hug(s, a, b) {
                        found(
                            s.net,
                            at,
                            Fault::HugsPart {
                                part: l.parts[i].part,
                            },
                        );
                    }
                }
            }
        }
    }
    for &i in &v.ports {
        let r = l.ports[i].extent();
        let corners = [
            Pt::new(r.min.x, r.min.y),
            Pt::new(r.max.x, r.min.y),
            Pt::new(r.max.x, r.max.y),
            Pt::new(r.min.x, r.max.y),
        ];
        for k in 0..4 {
            for s in &segs {
                if let Some(at) = hug(s, corners[k], corners[(k + 1) % 4]) {
                    found(
                        s.net,
                        at,
                        Fault::HugsFlag {
                            net: l.ports[i].net,
                        },
                    );
                }
            }
        }
    }
    out
}
