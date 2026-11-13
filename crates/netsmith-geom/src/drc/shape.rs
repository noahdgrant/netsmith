//! Distances between the shapes a board is made of, and a grid that finds the pairs that
//! can be near enough to matter.
//!
//! Every shape is a path of points grown by a radius: a track is two points grown by half
//! its width, a round pad is one point grown by its radius, a rectangular pad is a closed
//! polygon, a rounded one a polygon grown by its corner radius. A filled shape has an inside;
//! the distance between two shapes is the least distance between their paths, less both
//! radii, and zero if they touch or one lies inside the other.

use crate::geom::{Bounds, Pt};

#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    /// The path. One point is a disc, two a capsule.
    pub pts: Vec<Pt>,
    /// Whether the last point joins the first.
    pub closed: bool,
    /// Whether the inside of a closed path belongs to the shape.
    pub filled: bool,
    /// How far the shape reaches round its path.
    pub r: f64,
    pub bounds: Bounds,
}

impl Shape {
    fn make(pts: Vec<Pt>, closed: bool, filled: bool, r: f64) -> Shape {
        let bounds = Bounds::of(pts.iter().copied())
            .unwrap_or(Bounds {
                min: Pt::default(),
                max: Pt::default(),
            })
            .expanded(r);
        Shape {
            pts,
            closed,
            filled,
            r,
            bounds,
        }
    }

    pub fn disc(c: Pt, r: f64) -> Shape {
        Shape::make(vec![c], false, false, r)
    }

    /// A segment grown by `r`: a track.
    pub fn capsule(a: Pt, b: Pt, r: f64) -> Shape {
        Shape::make(vec![a, b], false, false, r)
    }

    /// A path grown by `r`: an arc track, a drawn line.
    pub fn stroke(pts: Vec<Pt>, r: f64) -> Shape {
        Shape::make(pts, false, false, r)
    }

    /// A solid polygon grown by `r`.
    pub fn polygon(pts: Vec<Pt>, r: f64) -> Shape {
        Shape::make(pts, true, true, r)
    }

    /// A closed outline with nothing inside, grown by `r`: a drawn rectangle or circle.
    pub fn ring(pts: Vec<Pt>, r: f64) -> Shape {
        Shape::make(pts, true, false, r)
    }

    /// The path's pieces, a point as a piece of no length.
    fn edges(&self) -> impl Iterator<Item = (Pt, Pt)> + '_ {
        let n = self.pts.len();
        let count = match n {
            0 => 0,
            1 => 1,
            _ if self.closed && n > 2 => n,
            _ => n - 1,
        };
        (0..count).map(move |i| (self.pts[i], self.pts[(i + 1) % n]))
    }
}

/// How far into each other shapes are when one lies in the other or their paths cross: more
/// than any tolerance, and less than anything a board is made of.
const DEEP: f64 = 1e-3;

/// The gap between two shapes, and the points of each that are nearest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gap {
    /// Zero if they touch.
    pub d: f64,
    /// How far they run into each other; zero if they only touch or are apart.
    pub depth: f64,
    pub a: Pt,
    pub b: Pt,
}

impl Gap {
    pub fn middle(&self) -> Pt {
        Pt::new((self.a.x + self.b.x) / 2.0, (self.a.y + self.b.y) / 2.0)
    }
}

/// The nearest points of two segments and their distance.
fn segments(p: (Pt, Pt), q: (Pt, Pt)) -> (f64, Pt, Pt) {
    if let Some(x) = cross(p, q) {
        return (0.0, x, x);
    }
    let mut best = (f64::MAX, p.0, q.0);
    for (point, seg, swap) in [
        (p.0, q, false),
        (p.1, q, false),
        (q.0, p, true),
        (q.1, p, true),
    ] {
        let on = nearest_on(seg, point);
        let d = point.dist(on);
        if d < best.0 {
            best = if swap { (d, on, point) } else { (d, point, on) };
        }
    }
    best
}

/// The point of segment `s` nearest to `p`.
fn nearest_on(s: (Pt, Pt), p: Pt) -> Pt {
    let (d, ap) = (s.1 - s.0, p - s.0);
    let len2 = d.x * d.x + d.y * d.y;
    if len2 < 1e-18 {
        return s.0;
    }
    let t = ((ap.x * d.x + ap.y * d.y) / len2).clamp(0.0, 1.0);
    s.0 + d * t
}

/// Where two segments cross, if they do.
fn cross(p: (Pt, Pt), q: (Pt, Pt)) -> Option<Pt> {
    let (r, s) = (p.1 - p.0, q.1 - q.0);
    let denom = r.x * s.y - r.y * s.x;
    if denom.abs() < 1e-15 {
        return None;
    }
    let qp = q.0 - p.0;
    let t = (qp.x * s.y - qp.y * s.x) / denom;
    let u = (qp.x * r.y - qp.y * r.x) / denom;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| p.0 + r * t)
}

/// Whether `p` is inside the polygon `pts` (even-odd).
fn inside(pts: &[Pt], p: Pt) -> bool {
    pts.len() >= 3 && crate::geom::contains(pts, p)
}

/// The gap between two shapes: the least distance between their paths less their radii, and
/// zero if they touch or overlap (then the points are where they meet).
pub fn gap(a: &Shape, b: &Shape) -> Gap {
    let mut best = (f64::MAX, a.pts[0], b.pts[0]);
    // One inside the other.
    if a.filled && inside(&a.pts, b.pts[0]) {
        return Gap {
            d: 0.0,
            depth: DEEP,
            a: b.pts[0],
            b: b.pts[0],
        };
    }
    if b.filled && inside(&b.pts, a.pts[0]) {
        return Gap {
            d: 0.0,
            depth: DEEP,
            a: a.pts[0],
            b: a.pts[0],
        };
    }
    'search: for ea in a.edges() {
        for eb in b.edges() {
            let (d, pa, pb) = segments(ea, eb);
            if d < best.0 {
                best = (d, pa, pb);
                if d == 0.0 {
                    break 'search;
                }
            }
        }
    }
    let (d, pa, pb) = best;
    let reach = a.r + b.r;
    if d <= reach {
        let at = Pt::new((pa.x + pb.x) / 2.0, (pa.y + pb.y) / 2.0);
        // Paths that cross run into each other even if nothing is grown.
        let depth = if d == 0.0 { reach.max(DEEP) } else { reach - d };
        return Gap {
            d: 0.0,
            depth,
            a: at,
            b: at,
        };
    }
    // Move each point out to the shape's own edge.
    let toward = (pb - pa) * (1.0 / d);
    Gap {
        d: d - reach,
        depth: 0.0,
        a: pa + toward * a.r,
        b: pb - toward * b.r,
    }
}

/// Whether the shape reaches anything of `polygon` (a solid area): used for rule areas and
/// zones.
pub fn touches_area(s: &Shape, polygon: &[Pt]) -> bool {
    let area = Shape::polygon(polygon.to_vec(), 0.0);
    gap(s, &area).d == 0.0
}

/// Pairs `(i, j)` with `i < j` whose boxes, each grown by `margin / 2`, overlap: the pairs
/// that can be less than `margin` apart. A pair is reported once. Linear in the number of
/// boxes for a board's usual spread of sizes.
pub fn near_pairs(boxes: &[Bounds], margin: f64) -> Vec<(usize, usize)> {
    use std::collections::HashMap;
    const CELL: f64 = 2.0;
    let grown: Vec<Bounds> = boxes.iter().map(|b| b.expanded(margin / 2.0)).collect();
    let cell = |v: f64| (v / CELL).floor() as i64;
    let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, b) in grown.iter().enumerate() {
        for cx in cell(b.min.x)..=cell(b.max.x) {
            for cy in cell(b.min.y)..=cell(b.max.y) {
                cells.entry((cx, cy)).or_default().push(i);
            }
        }
    }
    let mut pairs = Vec::new();
    for (&(cx, cy), members) in &cells {
        for (k, &i) in members.iter().enumerate() {
            for &j in &members[k + 1..] {
                let (a, b) = (&grown[i], &grown[j]);
                let overlap = a.min.x <= b.max.x
                    && b.min.x <= a.max.x
                    && a.min.y <= b.max.y
                    && b.min.y <= a.max.y;
                // Report in the cell that holds the lower-left corner of the overlap.
                let corner = (cell(a.min.x.max(b.min.x)), cell(a.min.y.max(b.min.y)));
                if overlap && corner == (cx, cy) {
                    pairs.push(if i < j { (i, j) } else { (j, i) });
                }
            }
        }
    }
    pairs.sort_unstable();
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Pt {
        Pt::new(x, y)
    }

    #[test]
    fn two_tracks_side_by_side_are_a_pitch_less_both_widths_apart() {
        let a = Shape::capsule(p(0.0, 0.0), p(5.0, 0.0), 0.1);
        let b = Shape::capsule(p(0.0, 0.5), p(5.0, 0.5), 0.1);
        let g = gap(&a, &b);
        assert!((g.d - 0.3).abs() < 1e-9, "{}", g.d);
        assert!((g.middle().y - 0.25).abs() < 1e-9);
    }

    #[test]
    fn a_pad_and_a_track_that_cross_have_no_gap_and_one_inside_the_other_too() {
        let pad = Shape::polygon(
            vec![p(0.0, 0.0), p(1.0, 0.0), p(1.0, 1.0), p(0.0, 1.0)],
            0.0,
        );
        let across = Shape::capsule(p(-1.0, 0.5), p(2.0, 0.5), 0.05);
        assert_eq!(gap(&pad, &across).d, 0.0);
        let within = Shape::disc(p(0.5, 0.5), 0.1);
        assert_eq!(gap(&pad, &within).d, 0.0);
        let away = Shape::disc(p(3.0, 0.5), 0.5);
        assert!((gap(&pad, &away).d - 1.5).abs() < 1e-9);
    }

    #[test]
    fn a_ring_has_no_inside() {
        let ring = Shape::ring(
            vec![p(0.0, 0.0), p(4.0, 0.0), p(4.0, 4.0), p(0.0, 4.0)],
            0.0,
        );
        let inner = Shape::disc(p(2.0, 2.0), 0.1);
        assert!((gap(&ring, &inner).d - 1.9).abs() < 1e-9);
    }

    #[test]
    fn the_grid_finds_the_pairs_that_can_touch_once_each() {
        let b = |x: f64, y: f64| Bounds {
            min: p(x, y),
            max: p(x + 1.0, y + 1.0),
        };
        // 0 and 1 are 0.2 apart; 2 is far; 3 is a long one crossing many cells.
        let boxes = [b(0.0, 0.0), b(1.2, 0.0), b(50.0, 50.0), b(-5.0, 0.0)];
        let boxes = [
            boxes[0],
            boxes[1],
            boxes[2],
            Bounds {
                min: p(-5.0, 0.0),
                max: p(60.0, 0.5),
            },
        ];
        assert_eq!(near_pairs(&boxes, 0.5), [(0, 1), (0, 3), (1, 3)]);
        assert_eq!(near_pairs(&boxes, 0.1), [(0, 3), (1, 3)]);
    }
}
