//! Polygons, their tessellation, and the scanline fill that turns them into pixel runs.

use std::f64::consts::{PI, TAU};

/// A point, in millimetres unless a function says pixels.
pub(crate) type Pt = (f64, f64);

/// How the inside of a set of polygons is decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rule {
    /// Inside where the winding number is not zero: overlapping polygons of one
    /// orientation merge into their union.
    NonZero,
    /// Inside where an odd number of edges lie to the left: nested polygons are holes.
    EvenOdd,
}

/// One filled shape, optionally erasing (`on == false`) what earlier shapes of the same
/// [`Item`] drew.
#[derive(Clone, Debug)]
pub(crate) struct Prim {
    pub polys: Vec<Vec<Pt>>,
    pub rule: Rule,
    pub on: bool,
}

/// Something the file drew: shapes composed among themselves first, then added to the
/// image (`dark`) or erased from it.
#[derive(Clone, Debug)]
pub(crate) struct Item {
    pub dark: bool,
    pub prims: Vec<Prim>,
}

/// Fineness of the polygons that stand for round shapes: no vertex of a circle or arc
/// is more than a quarter of a pixel from the true curve.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tess {
    pub ppm: f64,
}

/// The largest sagitta, in pixels, allowed when a curve is cut into chords.
const MAX_ERROR_PX: f64 = 0.25;

impl Tess {
    /// The largest angle one chord of a circle of `r_mm` may span.
    fn step(self, r_mm: f64) -> f64 {
        let r = r_mm.abs() * self.ppm;
        if r <= MAX_ERROR_PX {
            return PI / 2.0;
        }
        (2.0 * (1.0 - MAX_ERROR_PX / r).acos()).clamp(TAU / 4096.0, PI)
    }

    /// Number of vertices of a full circle: a multiple of 4, so the extreme points on the
    /// axes are vertices and a bounding box is exact.
    pub(super) fn circle_n(self, r_mm: f64) -> usize {
        let n = (TAU / self.step(r_mm)).ceil() as usize;
        (n.div_ceil(4) * 4).clamp(8, 4096)
    }

    /// Number of chords for an arc of `sweep` radians and radius `r_mm`.
    pub(super) fn arc_n(self, r_mm: f64, sweep: f64) -> usize {
        let a = sweep.abs();
        let by_error = (a / self.step(r_mm)).ceil();
        let by_quadrant = (a / (PI / 2.0)).ceil();
        (by_error.max(by_quadrant) as usize).clamp(1, 100_000)
    }
}

/// Rotates `p` by `deg` degrees counter-clockwise about the origin.
pub(crate) fn rotate(p: Pt, deg: f64) -> Pt {
    if deg == 0.0 {
        return p;
    }
    let (s, c) = deg.to_radians().sin_cos();
    (p.0 * c - p.1 * s, p.0 * s + p.1 * c)
}

/// A counter-clockwise regular `n`-gon with a vertex at angle `rot` (radians).
pub(crate) fn ngon(c: Pt, r: f64, n: usize, rot: f64) -> Vec<Pt> {
    (0..n)
        .map(|k| {
            let a = rot + TAU * k as f64 / n as f64;
            (c.0 + r * a.cos(), c.1 + r * a.sin())
        })
        .collect()
}

/// A counter-clockwise polygon for the disc of radius `r` about `c`, with `n` vertices.
/// The vertices lie slightly outside the circle (by about a third of the sagitta), so that
/// the polygon has the circle's area; a plain inscribed polygon would be 2 or 3 percent
/// too small at the sizes of pads.
pub(crate) fn disc(c: Pt, r: f64, n: usize) -> Vec<Pt> {
    let half = TAU / n as f64 / 2.0;
    let k = (half / (half.sin() * half.cos())).sqrt();
    ngon(c, r * k, n, 0.0)
}

/// A counter-clockwise polygon for the set of points within `r` of the segment `p0..p1`,
/// with `n` vertices on a full circle.
pub(crate) fn capsule(p0: Pt, p1: Pt, r: f64, n: usize) -> Vec<Pt> {
    let theta = (p1.1 - p0.1).atan2(p1.0 - p0.0);
    let half = (n / 2).max(2);
    let mut out = Vec::with_capacity(2 * half + 2);
    for (centre, from) in [(p1, theta - PI / 2.0), (p0, theta + PI / 2.0)] {
        for k in 0..=half {
            let a = from + PI * k as f64 / half as f64;
            out.push((centre.0 + r * a.cos(), centre.1 + r * a.sin()));
        }
    }
    out
}

/// The convex hull, counter-clockwise (Andrew's monotone chain).
pub(crate) fn convex_hull(mut pts: Vec<Pt>) -> Vec<Pt> {
    pts.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: Pt, a: Pt, b: Pt| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let mut hull: Vec<Pt> = Vec::with_capacity(pts.len() * 2);
    for &p in &pts {
        while hull.len() >= 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
            hull.pop();
        }
        hull.push(p);
    }
    let lower = hull.len() + 1;
    for &p in pts.iter().rev().skip(1) {
        while hull.len() >= lower && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
            hull.pop();
        }
        hull.push(p);
    }
    hull.pop();
    hull
}

/// Maps millimetres (Y up) to pixel coordinates (Y down): `px = x * scale + ox`,
/// `py = -y * scale + oy`. The centre of pixel (col, row) is at (col + 0.5, row + 0.5).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Map {
    pub scale: f64,
    pub ox: f64,
    pub oy: f64,
}

impl Map {
    pub(super) fn apply(self, p: Pt) -> Pt {
        (p.0 * self.scale + self.ox, -p.1 * self.scale + self.oy)
    }
}

struct Edge {
    r0: usize,
    r1: usize,
    x: f64,
    dx: f64,
    dir: i32,
}

/// Fills `polys` (in millimetres, through `map`) on a `width` x `height` grid and calls
/// `span(row, col_start, col_end)` for every run of pixels whose centre is inside, with
/// `col_end` exclusive. A pixel whose centre is exactly on an edge belongs to the shape
/// on the edge's right and below it (half-open rule), so adjacent shapes never overlap
/// and never leave a gap.
pub(crate) fn fill(
    polys: &[Vec<Pt>],
    map: Map,
    rule: Rule,
    width: usize,
    height: usize,
    mut span: impl FnMut(usize, usize, usize),
) {
    let h = height as f64;
    let mut edges: Vec<Edge> = Vec::new();
    for poly in polys {
        let n = poly.len();
        if n < 2 || poly.iter().any(|p| !p.0.is_finite() || !p.1.is_finite()) {
            continue;
        }
        for k in 0..n {
            let a = map.apply(poly[k]);
            let b = map.apply(poly[(k + 1) % n]);
            if a.1 == b.1 {
                continue;
            }
            let (top, bottom, dir) = if a.1 < b.1 { (a, b, 1) } else { (b, a, -1) };
            let r0 = (top.1 - 0.5).ceil().max(0.0);
            let r1 = (bottom.1 - 0.5).ceil().min(h);
            if r0 >= r1 {
                continue;
            }
            let dx = (bottom.0 - top.0) / (bottom.1 - top.1);
            edges.push(Edge {
                r0: r0 as usize,
                r1: r1 as usize,
                x: top.0 + (r0 + 0.5 - top.1) * dx,
                dx,
                dir,
            });
        }
    }
    edges.sort_by_key(|e| e.r0);
    let w = width as f64;
    let mut active: Vec<usize> = Vec::new();
    let mut crossings: Vec<(f64, i32)> = Vec::new();
    let mut next = 0;
    let mut row = 0;
    while row < height && (next < edges.len() || !active.is_empty()) {
        if active.is_empty() {
            row = row.max(edges[next].r0);
        }
        while next < edges.len() && edges[next].r0 <= row {
            active.push(next);
            next += 1;
        }
        active.retain(|&i| edges[i].r1 > row);
        crossings.clear();
        crossings.extend(active.iter().map(|&i| {
            (
                edges[i].x + (row - edges[i].r0) as f64 * edges[i].dx,
                edges[i].dir,
            )
        }));
        crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut emit = |xa: f64, xb: f64| {
            let c0 = (xa - 0.5).ceil().clamp(0.0, w) as usize;
            let c1 = (xb - 0.5).ceil().clamp(0.0, w) as usize;
            if c0 < c1 {
                span(row, c0, c1);
            }
        };
        match rule {
            Rule::NonZero => {
                let (mut wind, mut start) = (0, 0.0);
                for &(x, dir) in &crossings {
                    let before = wind;
                    wind += dir;
                    if before == 0 && wind != 0 {
                        start = x;
                    } else if before != 0 && wind == 0 {
                        emit(start, x);
                    }
                }
            }
            Rule::EvenOdd => {
                for [a, b] in crossings.as_chunks::<2>().0 {
                    emit(a.0, b.0);
                }
            }
        }
        row += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(polys: &[Vec<Pt>], rule: Rule, w: usize, h: usize) -> usize {
        let map = Map {
            scale: 1.0,
            ox: 0.0,
            oy: h as f64,
        };
        let mut n = 0;
        fill(polys, map, rule, w, h, |_, a, b| n += b - a);
        n
    }

    #[test]
    fn square_and_nested_square() {
        let outer = vec![(1.0, 1.0), (9.0, 1.0), (9.0, 9.0), (1.0, 9.0)];
        let inner = vec![(3.0, 3.0), (7.0, 3.0), (7.0, 7.0), (3.0, 7.0)];
        assert_eq!(
            area(std::slice::from_ref(&outer), Rule::NonZero, 10, 10),
            64
        );
        let both = [outer, inner];
        assert_eq!(area(&both, Rule::EvenOdd, 10, 10), 64 - 16);
        // Same orientation: the nonzero rule fills the inner square too.
        assert_eq!(area(&both, Rule::NonZero, 10, 10), 64);
    }

    #[test]
    fn hull_is_counter_clockwise_and_drops_inner_points() {
        let h = convex_hull(vec![
            (0.0, 0.0),
            (2.0, 0.0),
            (1.0, 1.0),
            (2.0, 2.0),
            (0.0, 2.0),
        ]);
        assert_eq!(h.len(), 4);
        let a: f64 = (0..4)
            .map(|k| h[k].0 * h[(k + 1) % 4].1 - h[(k + 1) % 4].0 * h[k].1)
            .sum();
        assert!(a > 0.0);
    }

    #[test]
    fn circle_vertex_count_is_a_multiple_of_four() {
        let t = Tess { ppm: 20.0 };
        for r in [0.01, 0.1, 1.0, 50.0, 1e6] {
            let n = t.circle_n(r);
            assert_eq!(n % 4, 0);
            assert!((8..=4096).contains(&n));
        }
        assert!(t.arc_n(5.0, 0.0) >= 1);
    }
}
