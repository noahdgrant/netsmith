//! A board edge that is not a rectangle with four equal rounded corners (issue #57): an L, a
//! polygon, a rectangle whose corners differ, or any closed run of lines and arcs a case's
//! drawing has.
//!
//! [`Polygon`] is the shape: its pieces as the board edge layer draws them, clockwise on
//! screen, and the same pieces as a closed run of short straight segments for what measures
//! distances to it. [`BoardEdge`] is the board's edge, whichever shape it is.
//!
//! **Who uses what.** The placement check and the pads' edge distance measure the polygon
//! itself ([`Polygon::depth`], [`Polygon::box_depth`]); `Edge.Cuts` (and so the Gerbers, which
//! are plotted from it) and the editor's sheet draw its pieces; the placer keeps courtyards
//! out of the part of the bounding box that is outside the polygon as boxes
//! ([`Polygon::outside_boxes`]), as it keeps them out of cutouts. The router closes the cells
//! of its grid outside the polygon ([`Polygon::contains`]) and keeps its copper from the
//! polygon's straight pieces ([`Polygon::segments`]) (issue #495), and the design rules check,
//! which reads `Edge.Cuts`, holds the copper's distance from the real edge.

use crate::geometry::{Pt, Rect};
use crate::shape::{Edge, RoundRect};

/// How far a straight chord may lie from the arc it stands for, in mm.
const SAGITTA: f64 = 0.005;

/// Points closer than this are one point, in mm.
const SAME: f64 = 1e-6;

/// The least height of a box of [`Polygon::outside_boxes`], in mm: a slanted edge or an arc
/// is kept out of in steps of about this, on the safe side.
const BAND: f64 = 0.25;

/// A band of [`Polygon::outside_boxes`]: its top, its bottom, and the x ranges inside the
/// shape all the way down it.
type Band = (f64, f64, Vec<(f64, f64)>);

/// A closed board edge of lines and arcs, clockwise on screen (y down), in the board's
/// coordinates.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Polygon {
    edges: Vec<Edge>,
    /// The edges as short straight pieces, closed: each ends where the next begins.
    segments: Vec<(Pt, Pt)>,
    bounds: Rect,
}

/// What is wrong with a run of edges that is not a board edge.
#[derive(Debug, Clone, PartialEq)]
pub enum PolygonError {
    /// Fewer than three corners, or no area.
    Degenerate,
    /// An edge does not begin where the one before it ends.
    Open { after: usize, gap: f64 },
    /// An arc whose three points are on one line.
    FlatArc { edge: usize },
    /// The edge crosses or touches itself.
    Crossing { at: Pt },
}

impl std::fmt::Display for PolygonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolygonError::Degenerate => {
                write!(f, "the outline has fewer than three corners or no area")
            }
            PolygonError::Open { after, gap } => write!(
                f,
                "the outline does not close: edge {} ends {gap:.4} mm from where edge {} begins",
                after + 1,
                after + 2
            ),
            PolygonError::FlatArc { edge } => write!(
                f,
                "edge {} of the outline is an arc whose three points are on one line",
                edge + 1
            ),
            PolygonError::Crossing { at } => write!(
                f,
                "the outline crosses or touches itself near ({:.3}, {:.3}) mm",
                at.x, at.y
            ),
        }
    }
}

impl std::error::Error for PolygonError {}

/// The circle through three points: its centre and radius, or `None` if they are on a line.
fn circle(a: Pt, b: Pt, c: Pt) -> Option<(Pt, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let (a2, b2, c2) = (
        a.x * a.x + a.y * a.y,
        b.x * b.x + b.y * b.y,
        c.x * c.x + c.y * c.y,
    );
    let centre = Pt::new(
        (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d,
        (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d,
    );
    Some((centre, centre.distance(a)))
}

/// An arc as its circle, its start angle and the signed angle it sweeps (radians, y down, so a
/// positive sweep is clockwise on screen).
struct Sweep {
    centre: Pt,
    radius: f64,
    from: f64,
    by: f64,
}

impl Sweep {
    fn of(start: Pt, mid: Pt, end: Pt) -> Option<Sweep> {
        let (centre, radius) = circle(start, mid, end)?;
        let angle = |p: Pt| (p.y - centre.y).atan2(p.x - centre.x);
        let tau = std::f64::consts::TAU;
        let (a0, am, a1) = (angle(start), angle(mid), angle(end));
        // The sweep that passes the middle point: one way round or the other.
        let forward = (a1 - a0).rem_euclid(tau);
        let to_mid = (am - a0).rem_euclid(tau);
        let by = if to_mid <= forward {
            forward
        } else {
            forward - tau
        };
        Some(Sweep {
            centre,
            radius,
            from: a0,
            by,
        })
    }

    fn at(&self, t: f64) -> Pt {
        let a = self.from + self.by * t;
        Pt::new(
            self.centre.x + self.radius * a.cos(),
            self.centre.y + self.radius * a.sin(),
        )
    }

    /// Whether the arc passes the angle `a`.
    fn passes(&self, a: f64) -> bool {
        let tau = std::f64::consts::TAU;
        if self.by >= 0.0 {
            (a - self.from).rem_euclid(tau) <= self.by
        } else {
            (self.from - a).rem_euclid(tau) <= -self.by
        }
    }
}

/// The distance from a point to a segment.
fn to_segment(p: Pt, a: Pt, b: Pt) -> f64 {
    let d = b - a;
    let len2 = d.x * d.x + d.y * d.y;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
    };
    p.distance(Pt::new(a.x + d.x * t, a.y + d.y * t))
}

fn cross(o: Pt, a: Pt, b: Pt) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Whether two segments meet (touching counts).
fn segments_meet(a: (Pt, Pt), b: (Pt, Pt)) -> bool {
    let (d1, d2) = (cross(b.0, b.1, a.0), cross(b.0, b.1, a.1));
    let (d3, d4) = (cross(a.0, a.1, b.0), cross(a.0, a.1, b.1));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    // Touching: an end on the other segment.
    to_segment(a.0, b.0, b.1) < SAME
        || to_segment(a.1, b.0, b.1) < SAME
        || to_segment(b.0, a.0, a.1) < SAME
        || to_segment(b.1, a.0, a.1) < SAME
}

/// Whether a segment passes through the inside of a box (its edge does not count).
fn crosses_box(s: (Pt, Pt), r: &Rect) -> bool {
    let inner = Rect::new(
        Pt::new(r.min.x + SAME, r.min.y + SAME),
        Pt::new(r.max.x - SAME, r.max.y - SAME),
    );
    if inner.min.x >= inner.max.x || inner.min.y >= inner.max.y {
        return false;
    }
    let inside =
        |p: Pt| p.x > inner.min.x && p.x < inner.max.x && p.y > inner.min.y && p.y < inner.max.y;
    if inside(s.0) || inside(s.1) {
        return true;
    }
    let c = inner.corners();
    (0..4).any(|k| {
        let e = (c[k], c[(k + 1) % 4]);
        let (d1, d2) = (cross(e.0, e.1, s.0), cross(e.0, e.1, s.1));
        let (d3, d4) = (cross(s.0, s.1, e.0), cross(s.0, s.1, e.1));
        ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
            && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    })
}

/// The distance between a segment and a box it does not cross.
fn segment_to_box(s: (Pt, Pt), r: &Rect) -> f64 {
    let to_box = |p: Pt| {
        let dx = (r.min.x - p.x).max(p.x - r.max.x).max(0.0);
        let dy = (r.min.y - p.y).max(p.y - r.max.y).max(0.0);
        dx.hypot(dy)
    };
    let corners = r.corners();
    let from_corners = corners
        .iter()
        .map(|&c| to_segment(c, s.0, s.1))
        .fold(f64::INFINITY, f64::min);
    to_box(s.0).min(to_box(s.1)).min(from_corners)
}

/// Twice the signed area of a closed run of segments: positive when it runs clockwise on
/// screen (y down).
fn twice_area(segments: &[(Pt, Pt)]) -> f64 {
    segments
        .iter()
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f64>()
}

/// An edge run the other way.
fn reversed(e: Edge) -> Edge {
    match e {
        Edge::Line(a, b) => Edge::Line(b, a),
        Edge::Arc { start, mid, end } => Edge::Arc {
            start: end,
            mid,
            end: start,
        },
    }
}

fn ends(e: &Edge) -> (Pt, Pt) {
    match *e {
        Edge::Line(a, b) => (a, b),
        Edge::Arc { start, end, .. } => (start, end),
    }
}

impl Polygon {
    /// A board edge from its pieces in order, either way round: each piece begins where the
    /// one before it ends, and the last ends where the first begins. It is turned clockwise on
    /// screen if it runs the other way. A run that does not close, has no area, or crosses or
    /// touches itself is refused.
    pub fn new(edges: Vec<Edge>) -> Result<Polygon, PolygonError> {
        let edges: Vec<Edge> = edges
            .into_iter()
            .filter(|e| {
                let (a, b) = ends(e);
                matches!(e, Edge::Arc { .. }) || a.distance(b) > SAME
            })
            .collect();
        if edges.is_empty() {
            return Err(PolygonError::Degenerate);
        }
        for k in 0..edges.len() {
            let end = ends(&edges[k]).1;
            let next = ends(&edges[(k + 1) % edges.len()]).0;
            let gap = end.distance(next);
            if gap > 1e-4 {
                return Err(PolygonError::Open { after: k, gap });
            }
        }
        let mut segments = Vec::new();
        for (k, e) in edges.iter().enumerate() {
            match *e {
                Edge::Line(a, b) => segments.push((a, b)),
                Edge::Arc { start, mid, end } => {
                    let sweep =
                        Sweep::of(start, mid, end).ok_or(PolygonError::FlatArc { edge: k })?;
                    let step = 2.0 * (2.0 * sweep.radius * SAGITTA).sqrt();
                    let pieces = ((sweep.by.abs() * sweep.radius / step).ceil() as usize).max(2);
                    let mut from = start;
                    for i in 1..=pieces {
                        let to = if i == pieces {
                            end
                        } else {
                            sweep.at(i as f64 / pieces as f64)
                        };
                        segments.push((from, to));
                        from = to;
                    }
                }
            }
        }
        // Each piece ends exactly where the next begins.
        for k in 0..segments.len() {
            let next = segments[(k + 1) % segments.len()].0;
            segments[k].1 = next;
        }
        if segments.len() < 3 {
            return Err(PolygonError::Degenerate);
        }
        let n = segments.len();
        for i in 0..n {
            for j in i + 1..n {
                // Neighbours share an end, which is not a crossing.
                if j == i + 1 || (i == 0 && j == n - 1) {
                    continue;
                }
                if segments_meet(segments[i], segments[j]) {
                    return Err(PolygonError::Crossing { at: segments[i].0 });
                }
            }
        }
        if twice_area(&segments).abs() < 1e-9 {
            return Err(PolygonError::Degenerate);
        }
        let (edges, segments) = if twice_area(&segments) < 0.0 {
            (
                edges.into_iter().rev().map(reversed).collect(),
                segments.into_iter().rev().map(|(a, b)| (b, a)).collect(),
            )
        } else {
            (edges, segments)
        };
        let bounds = Self::bounds_of(&edges);
        Ok(Polygon {
            edges,
            segments,
            bounds,
        })
    }

    /// A board edge of straight lines through `points`, in order.
    pub fn from_points(points: &[Pt]) -> Result<Polygon, PolygonError> {
        let n = points.len();
        Self::new(
            (0..n)
                .map(|k| Edge::Line(points[k], points[(k + 1) % n]))
                .collect(),
        )
    }

    /// The exact box round the pieces, arcs included.
    fn bounds_of(edges: &[Edge]) -> Rect {
        let mut points = Vec::new();
        for e in edges {
            match *e {
                Edge::Line(a, b) => points.extend([a, b]),
                Edge::Arc { start, mid, end } => {
                    points.extend([start, end]);
                    if let Some(s) = Sweep::of(start, mid, end) {
                        for k in 0..4 {
                            let a = std::f64::consts::FRAC_PI_2 * f64::from(k);
                            if s.passes(a) {
                                // Rounded to a nanometre, so that the top of a circle drawn
                                // to y = 10 is 10 and not a hair over.
                                let r = |v: f64| (v * 1e6).round() / 1e6;
                                points.push(Pt::new(
                                    r(s.centre.x + s.radius * a.cos()),
                                    r(s.centre.y + s.radius * a.sin()),
                                ));
                            }
                        }
                    }
                }
            }
        }
        Rect::around(points).expect("a polygon has points")
    }

    /// The pieces, clockwise on screen, as the board edge layer draws them.
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// The edge as a closed run of short straight pieces, `(from, to)`, none further than
    /// 5 micrometres from the arc it stands for.
    pub fn segments(&self) -> &[(Pt, Pt)] {
        &self.segments
    }

    /// The box that holds it.
    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    /// The same shape moved by `by`.
    pub fn translated(&self, by: Pt) -> Polygon {
        let m = |p: Pt| p + by;
        Polygon {
            edges: self
                .edges
                .iter()
                .map(|e| match *e {
                    Edge::Line(a, b) => Edge::Line(m(a), m(b)),
                    Edge::Arc { start, mid, end } => Edge::Arc {
                        start: m(start),
                        mid: m(mid),
                        end: m(end),
                    },
                })
                .collect(),
            segments: self.segments.iter().map(|&(a, b)| (m(a), m(b))).collect(),
            bounds: self.bounds.translated(by),
        }
    }

    /// The corners where the edge turns, in order: the ends of its pieces.
    pub fn corners(&self) -> Vec<Pt> {
        self.edges.iter().map(|e| ends(e).0).collect()
    }

    /// Whether a point is strictly inside, by the crossings of a ray to the right.
    fn inside(&self, p: Pt) -> bool {
        let mut inside = false;
        for &(a, b) in &self.segments {
            if (a.y > p.y) != (b.y > p.y) {
                let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
                if x > p.x {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// The distance from a point to the edge.
    fn to_edge(&self, p: Pt) -> f64 {
        self.segments
            .iter()
            .map(|&(a, b)| to_segment(p, a, b))
            .fold(f64::INFINITY, f64::min)
    }

    /// Whether a point is inside the shape or on its edge.
    pub fn contains(&self, p: Pt) -> bool {
        self.to_edge(p) <= 1e-6 || self.inside(p)
    }

    /// How far a point inside the shape is from its edge: negative if it is outside.
    pub fn depth(&self, p: Pt) -> f64 {
        let d = self.to_edge(p);
        if self.inside(p) { d } else { -d }
    }

    /// How far a box is from the edge, all of it inside: negative if any of it is outside.
    /// In a shape that is not convex the nearest point may be a corner of the edge that
    /// points into the box's side, so every piece is measured and not only the box's corners.
    pub fn box_depth(&self, r: &Rect) -> f64 {
        let corners = r.corners();
        let worst = corners
            .iter()
            .map(|&c| self.depth(c))
            .fold(f64::INFINITY, f64::min);
        if worst < 0.0 {
            return worst;
        }
        // A corner of the edge inside the box: the box reaches past it by as much as the
        // corner is inside the box.
        let poke = self
            .segments
            .iter()
            .filter(|&&s| crosses_box(s, r))
            .map(|&(a, _)| {
                let into = (a.x - r.min.x)
                    .min(r.max.x - a.x)
                    .min(a.y - r.min.y)
                    .min(r.max.y - a.y);
                into.max(SAME)
            })
            .fold(0.0f64, f64::max);
        if poke > 0.0 {
            return -poke;
        }
        self.segments
            .iter()
            .map(|&s| segment_to_box(s, r))
            .fold(f64::INFINITY, f64::min)
    }

    /// The part of the bounding box that is outside the shape, as boxes: what the placer keeps
    /// courtyards out of. They hold all of the outside. Where an edge is straight along x or y
    /// they meet it exactly; along a slanted edge or an arc they step in bands of about
    /// [`BAND`], on the inside of it, so the placer may leave a sliver of board by such an
    /// edge unused and never puts a courtyard past it.
    pub fn outside_boxes(&self) -> Vec<Rect> {
        let b = self.bounds;
        // The heights where something changes: the ends of the pieces.
        let mut ys: Vec<f64> = self.segments.iter().map(|s| s.0.y).collect();
        ys.extend([b.min.y, b.max.y]);
        ys.sort_by(f64::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < SAME);
        // Strips taller than a band are cut, so that a long slanted edge is followed.
        let mut fine = Vec::new();
        for w in ys.windows(2) {
            let (y0, y1) = (w[0], w[1]);
            let n = ((y1 - y0) / BAND).ceil().max(1.0) as usize;
            for k in 0..n {
                fine.push((
                    y0 + (y1 - y0) * k as f64 / n as f64,
                    y0 + (y1 - y0) * (k + 1) as f64 / n as f64,
                ));
            }
        }
        // Where the whole of a strip is inside: between pairs of crossings at its middle,
        // narrowed to where each crossing piece is furthest in over the strip.
        let inside_of = |(y0, y1): (f64, f64)| -> Vec<(f64, f64)> {
            let ym = (y0 + y1) / 2.0;
            let mut crossings: Vec<(f64, f64, f64)> = Vec::new();
            for &(a, c) in &self.segments {
                if (a.y > ym) != (c.y > ym) {
                    let x_at = |y: f64| a.x + (y - a.y) * (c.x - a.x) / (c.y - a.y);
                    let (xa, xb) = (x_at(y0), x_at(y1));
                    crossings.push((x_at(ym), xa.min(xb), xa.max(xb)));
                }
            }
            crossings.sort_by(|p, q| p.0.total_cmp(&q.0));
            crossings
                .chunks(2)
                .filter(|pair| pair.len() == 2)
                .map(|pair| (pair[0].2, pair[1].1))
                .filter(|(lo, hi)| hi > lo)
                .collect()
        };
        let intersect = |a: &[(f64, f64)], b: &[(f64, f64)]| -> Vec<(f64, f64)> {
            let mut out = Vec::new();
            for &(a0, a1) in a {
                for &(b0, b1) in b {
                    let (lo, hi) = (a0.max(b0), a1.min(b1));
                    if hi > lo + SAME {
                        out.push((lo, hi));
                    }
                }
            }
            out
        };
        // Bands: strips joined until they are a band tall, the inside of a band being what is
        // inside all of its strips.
        let mut bands: Vec<Band> = Vec::new();
        let mut k = 0;
        while k < fine.len() {
            let (y0, mut y1) = fine[k];
            let mut inside = inside_of(fine[k]);
            k += 1;
            while y1 - y0 < BAND - SAME && k < fine.len() {
                inside = intersect(&inside, &inside_of(fine[k]));
                y1 = fine[k].1;
                k += 1;
            }
            bands.push((y0, y1, inside));
        }
        // The outside of each band, then boxes of one x range above one another joined.
        let mut boxes: Vec<Rect> = Vec::new();
        for (y0, y1, inside) in bands {
            let mut x = b.min.x;
            let mut gaps = Vec::new();
            for &(lo, hi) in &inside {
                if lo > x + SAME {
                    gaps.push((x, lo));
                }
                x = x.max(hi);
            }
            if b.max.x > x + SAME {
                gaps.push((x, b.max.x));
            }
            for (x0, x1) in gaps {
                let joined = boxes.iter_mut().rev().find(|r| {
                    (r.max.y - y0).abs() < SAME
                        && (r.min.x - x0).abs() < SAME
                        && (r.max.x - x1).abs() < SAME
                });
                match joined {
                    Some(r) => r.max.y = y1,
                    None => boxes.push(Rect::new(Pt::new(x0, y0), Pt::new(x1, y1))),
                }
            }
        }
        boxes
    }
}

/// The polygon a design's outline states, in the board's frame (its origin the top left of
/// the polygon's box), or `None` for a rounded rectangle.
pub(crate) fn of_outline(
    outline: &netsmith_core::Outline,
) -> Result<Option<Polygon>, PolygonError> {
    let Some(vertices) = &outline.polygon else {
        return Ok(None);
    };
    let n = vertices.len();
    let pt = |(x, y): (f64, f64)| Pt::new(x, y);
    let edges = (0..n)
        .map(|k| {
            let (a, b) = (pt(vertices[k].at), pt(vertices[(k + 1) % n].at));
            match vertices[k].through {
                Some(mid) => Edge::Arc {
                    start: a,
                    mid: pt(mid),
                    end: b,
                },
                None => Edge::Line(a, b),
            }
        })
        .collect();
    Polygon::new(edges).map(Some)
}

/// The board edge: the rounded rectangle every board had, or a polygon.
#[derive(Debug, Clone, PartialEq)]
pub enum BoardEdge {
    Round(RoundRect),
    Polygon(Polygon),
}

impl BoardEdge {
    pub fn contains(&self, p: Pt) -> bool {
        match self {
            BoardEdge::Round(s) => s.contains(p),
            BoardEdge::Polygon(s) => s.contains(p),
        }
    }

    /// How far a point inside is from the edge: negative outside.
    pub fn depth(&self, p: Pt) -> f64 {
        match self {
            BoardEdge::Round(s) => s.depth(p),
            BoardEdge::Polygon(s) => s.depth(p),
        }
    }

    /// How far a box is from the edge: negative if any of it is outside.
    pub fn box_depth(&self, r: &Rect) -> f64 {
        match self {
            BoardEdge::Round(s) => {
                let o = &s.rect;
                let straight = (r.min.x - o.min.x)
                    .min(r.min.y - o.min.y)
                    .min(o.max.x - r.max.x)
                    .min(o.max.y - r.max.y);
                if s.radius == 0.0 {
                    return straight;
                }
                // A box inside a convex shape is nearest the edge at one of its corners.
                r.corners()
                    .iter()
                    .map(|&c| s.depth(c))
                    .fold(straight, f64::min)
            }
            BoardEdge::Polygon(s) => s.box_depth(r),
        }
    }

    /// The pieces as the board edge layer draws them.
    pub fn edges(&self) -> Vec<Edge> {
        match self {
            BoardEdge::Round(s) => s.edges(),
            BoardEdge::Polygon(s) => s.edges().to_vec(),
        }
    }

    /// The edge as a closed run of short straight pieces.
    pub fn segments(&self) -> Vec<(Pt, Pt)> {
        match self {
            BoardEdge::Round(s) => s.segments(),
            BoardEdge::Polygon(s) => s.segments().to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Pt {
        Pt::new(x, y)
    }

    /// An L: 30 x 20 with the top right 10 x 10 taken away.
    fn ell() -> Polygon {
        Polygon::from_points(&[
            p(0.0, 0.0),
            p(20.0, 0.0),
            p(20.0, 10.0),
            p(30.0, 10.0),
            p(30.0, 20.0),
            p(0.0, 20.0),
        ])
        .unwrap()
    }

    #[test]
    fn an_l_holds_its_arm_and_not_its_notch() {
        let l = ell();
        assert_eq!(l.bounds(), Rect::new(p(0.0, 0.0), p(30.0, 20.0)));
        assert!(l.contains(p(5.0, 5.0)));
        assert!(l.contains(p(25.0, 15.0)));
        assert!(!l.contains(p(25.0, 5.0)), "the notch");
        assert!(l.contains(p(20.0, 5.0)), "on the edge");
        assert!((l.depth(p(25.0, 5.0)) + 5.0).abs() < 1e-9);
        assert!((l.depth(p(18.0, 12.0)) - 2.0f64.hypot(2.0)).abs() < 1e-9);
    }

    #[test]
    fn a_counterclockwise_run_is_turned_round() {
        let l = Polygon::from_points(&[p(0.0, 0.0), p(0.0, 10.0), p(10.0, 10.0), p(10.0, 0.0)])
            .unwrap();
        assert!(twice_area(l.segments()) > 0.0);
        assert_eq!(l.corners()[1], p(10.0, 0.0));
    }

    #[test]
    fn a_box_is_measured_against_the_notch() {
        let l = ell();
        let r = Rect::new(p(18.0, 11.0), p(22.0, 12.0));
        assert!(l.box_depth(&r) > 0.0, "below the notch");
        let r = Rect::new(p(15.0, 8.0), p(19.0, 9.0));
        assert!(
            (l.box_depth(&r) - 1.0).abs() < 1e-9,
            "1 mm from the notch's side"
        );
        let r = Rect::new(p(15.0, 5.0), p(25.0, 6.0));
        assert!(l.box_depth(&r) < 0.0, "into the notch");
    }

    #[test]
    fn a_box_round_a_reflex_corner_is_not_inside() {
        // An arrow-head: the corner (10, 6) points up into a box whose corners are inside.
        let s = Polygon::from_points(&[
            p(0.0, 0.0),
            p(20.0, 0.0),
            p(20.0, 10.0),
            p(10.0, 6.0),
            p(0.0, 10.0),
        ])
        .unwrap();
        let r = Rect::new(p(2.0, 5.5), p(18.0, 6.5));
        assert!(r.corners().iter().all(|&c| s.contains(c)));
        let d = s.box_depth(&r);
        assert!(
            (d + 0.5).abs() < 1e-9,
            "the corner is 0.5 mm into the box: {d}"
        );
        assert!(s.box_depth(&Rect::new(p(2.0, 4.0), p(18.0, 5.0))) > 0.0);
    }

    #[test]
    fn a_crossing_or_open_run_is_refused() {
        let bow = Polygon::from_points(&[p(0.0, 0.0), p(10.0, 10.0), p(10.0, 0.0), p(0.0, 10.0)]);
        assert!(matches!(bow, Err(PolygonError::Crossing { .. })), "{bow:?}");
        let open = Polygon::new(vec![
            Edge::Line(p(0.0, 0.0), p(10.0, 0.0)),
            Edge::Line(p(10.0, 0.0), p(10.0, 10.0)),
            Edge::Line(p(10.0, 10.0), p(0.0, 9.0)),
        ]);
        assert!(
            matches!(open, Err(PolygonError::Open { after: 2, .. })),
            "{open:?}"
        );
        assert_eq!(
            Polygon::from_points(&[p(0.0, 0.0), p(5.0, 0.0), p(10.0, 0.0)]),
            Err(PolygonError::Degenerate)
        );
    }

    /// A 20 x 10 rectangle with a radius of 4 at the top left only.
    fn one_round_corner() -> Polygon {
        let s = 4.0 * std::f64::consts::FRAC_1_SQRT_2;
        Polygon::new(vec![
            Edge::Arc {
                start: p(0.0, 4.0),
                mid: p(4.0 - s, 4.0 - s),
                end: p(4.0, 0.0),
            },
            Edge::Line(p(4.0, 0.0), p(20.0, 0.0)),
            Edge::Line(p(20.0, 0.0), p(20.0, 10.0)),
            Edge::Line(p(20.0, 10.0), p(0.0, 10.0)),
            Edge::Line(p(0.0, 10.0), p(0.0, 4.0)),
        ])
        .unwrap()
    }

    #[test]
    fn unequal_corners_cut_only_the_rounded_one() {
        let s = one_round_corner();
        assert!(!s.contains(p(0.3, 0.3)));
        assert!(s.contains(p(19.9, 0.1)), "the square corner");
        let d = s.depth(p(1.0, 1.0));
        assert!((d - (4.0 - 3.0f64.hypot(3.0))).abs() < 0.006, "{d}");
        assert_eq!(s.bounds(), Rect::new(p(0.0, 0.0), p(20.0, 10.0)));
    }

    #[test]
    fn an_arc_run_either_way_is_drawn_through_its_middle() {
        // A half disc bulging up from the line y = 10.
        let half = Polygon::new(vec![
            Edge::Arc {
                start: p(0.0, 10.0),
                mid: p(10.0, 0.0),
                end: p(20.0, 10.0),
            },
            Edge::Line(p(20.0, 10.0), p(0.0, 10.0)),
        ])
        .unwrap();
        assert_eq!(half.bounds(), Rect::new(p(0.0, 0.0), p(20.0, 10.0)));
        assert!(half.contains(p(10.0, 1.0)));
        assert!(!half.contains(p(1.0, 1.0)));
        for (a, b) in half.segments() {
            let m = p((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
            assert!(half.depth(m).abs() < 0.0055);
        }
    }

    #[test]
    fn the_outside_boxes_are_the_notch_of_an_l_exactly() {
        let boxes = ell().outside_boxes();
        assert_eq!(boxes, vec![Rect::new(p(20.0, 0.0), p(30.0, 10.0))]);
    }

    #[test]
    fn the_outside_boxes_hold_all_the_outside_and_little_of_the_shape() {
        let shapes = [
            one_round_corner(),
            Polygon::from_points(&[
                p(0.0, 0.0),
                p(20.0, 0.0),
                p(20.0, 10.0),
                p(10.0, 6.0),
                p(0.0, 10.0),
            ])
            .unwrap(),
            Polygon::from_points(&[p(0.0, 5.0), p(10.0, 0.0), p(20.0, 5.0), p(10.0, 10.0)])
                .unwrap(),
        ];
        for s in &shapes {
            let boxes = s.outside_boxes();
            assert!(!boxes.is_empty());
            let b = s.bounds();
            // Every point of the bounding box that no box holds is in the shape.
            for i in 0..=200 {
                for j in 0..=100 {
                    let q = p(
                        b.min.x + b.width() * f64::from(i) / 200.0,
                        b.min.y + b.height() * f64::from(j) / 100.0,
                    );
                    let held = boxes.iter().any(|r| {
                        q.x >= r.min.x - 1e-9
                            && q.x <= r.max.x + 1e-9
                            && q.y >= r.min.y - 1e-9
                            && q.y <= r.max.y + 1e-9
                    });
                    assert!(
                        held || s.contains(q),
                        "{q:?} is outside and no box holds it"
                    );
                }
            }
            // And they take little of the shape: their area is not much more than the
            // outside's.
            let outside = b.width() * b.height() - twice_area(s.segments()) / 2.0;
            let covered: f64 = boxes.iter().map(|r| r.width() * r.height()).sum();
            assert!(covered >= outside - 1e-6, "{covered} {outside}");
            assert!(
                covered - outside < 0.05 * b.width() * b.height(),
                "{covered} {outside}"
            );
        }
    }

    #[test]
    fn a_board_edge_measures_a_box_as_its_shape_does() {
        let round = BoardEdge::Round(RoundRect::new(Rect::new(p(0.0, 0.0), p(20.0, 10.0)), 0.0));
        assert!((round.box_depth(&Rect::new(p(1.0, 2.0), p(3.0, 4.0))) - 1.0).abs() < 1e-9);
        let l = BoardEdge::Polygon(ell());
        assert!(l.box_depth(&Rect::new(p(21.0, 1.0), p(22.0, 2.0))) < 0.0);
        assert_eq!(l.edges().len(), 6);
    }
}
