//! A rectangle with rounded corners: the board edge and the holes cut inside it.

use crate::geometry::{Pt, Rect};

/// How far a straight chord may lie from the arc it stands for, in mm, when an arc is
/// drawn as a polyline.
const SAGITTA: f64 = 0.005;

/// A piece of an outline.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Edge {
    Line(Pt, Pt),
    /// A quarter circle, as KiCad draws an arc: the two ends and a point between them.
    Arc {
        start: Pt,
        mid: Pt,
        end: Pt,
    },
}

/// A rectangle whose four corners are quarter circles of one radius (0 for square ones).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoundRect {
    pub rect: Rect,
    pub radius: f64,
}

/// Round to a tenth of a micrometre, which is what the board files print, so that the end
/// of a line and the end of the arc beside it are the same point.
fn tidy(p: Pt) -> Pt {
    let r = |v: f64| (v * 1e4).round() / 1e4;
    Pt::new(r(p.x), r(p.y))
}

impl RoundRect {
    pub fn new(rect: Rect, radius: f64) -> Self {
        // A radius cannot be more than half the shorter side.
        let radius = radius.clamp(0.0, rect.width().min(rect.height()) / 2.0);
        Self { rect, radius }
    }

    /// The centres of the four corner circles, clockwise from the top left on screen.
    fn centres(&self) -> [Pt; 4] {
        let (r, o) = (self.radius, &self.rect);
        [
            Pt::new(o.min.x + r, o.min.y + r),
            Pt::new(o.max.x - r, o.min.y + r),
            Pt::new(o.max.x - r, o.max.y - r),
            Pt::new(o.min.x + r, o.max.y - r),
        ]
    }

    /// Whether a point is inside the shape or on its edge.
    pub fn contains(&self, p: Pt) -> bool {
        const TOLERANCE: f64 = 1e-6;
        let o = &self.rect;
        if p.x < o.min.x - TOLERANCE
            || p.x > o.max.x + TOLERANCE
            || p.y < o.min.y - TOLERANCE
            || p.y > o.max.y + TOLERANCE
        {
            return false;
        }
        self.corner_depth(p)
            .is_none_or(|(centre, _)| p.distance(centre) <= self.radius + TOLERANCE)
    }

    /// The corner circle a point is in the corner square of, if it is in one.
    fn corner_depth(&self, p: Pt) -> Option<(Pt, usize)> {
        if self.radius == 0.0 {
            return None;
        }
        self.centres().into_iter().enumerate().find_map(|(k, c)| {
            let (left, top) = (k == 0 || k == 3, k < 2);
            let in_square = (if left { p.x < c.x } else { p.x > c.x })
                && (if top { p.y < c.y } else { p.y > c.y });
            in_square.then_some((c, k))
        })
    }

    /// How far a point inside the shape is from its edge: negative if it is outside.
    pub fn depth(&self, p: Pt) -> f64 {
        let o = &self.rect;
        let straight = (p.x - o.min.x)
            .min(p.y - o.min.y)
            .min(o.max.x - p.x)
            .min(o.max.y - p.y);
        match self.corner_depth(p) {
            Some((centre, _)) => straight.min(self.radius - p.distance(centre)),
            None => straight,
        }
    }

    /// The outline as lines and arcs, clockwise on screen from the top left, ready to be
    /// written as board edge.
    pub fn edges(&self) -> Vec<Edge> {
        self.pieces().into_iter().map(|(edge, _)| edge).collect()
    }

    /// The edges, each arc with the centre of its circle.
    fn pieces(&self) -> Vec<(Edge, Pt)> {
        let o = &self.rect;
        let r = self.radius;
        if r == 0.0 {
            let c = o.corners();
            return (0..4)
                .map(|k| (Edge::Line(c[k], c[(k + 1) % 4]), Pt::default()))
                .collect();
        }
        let centres = self.centres();
        let s = std::f64::consts::FRAC_1_SQRT_2 * r;
        // Each corner: where the arc starts and ends going clockwise, and its middle.
        let corner = |k: usize| -> (Pt, Pt, Pt) {
            let c = centres[k];
            match k {
                0 => (
                    Pt::new(o.min.x, c.y),
                    Pt::new(c.x - s, c.y - s),
                    Pt::new(c.x, o.min.y),
                ),
                1 => (
                    Pt::new(c.x, o.min.y),
                    Pt::new(c.x + s, c.y - s),
                    Pt::new(o.max.x, c.y),
                ),
                2 => (
                    Pt::new(o.max.x, c.y),
                    Pt::new(c.x + s, c.y + s),
                    Pt::new(c.x, o.max.y),
                ),
                _ => (
                    Pt::new(c.x, o.max.y),
                    Pt::new(c.x - s, c.y + s),
                    Pt::new(o.min.x, c.y),
                ),
            }
        };
        let mut edges = Vec::new();
        let mut previous_end: Option<Pt> = None;
        let mut first_start = Pt::default();
        for (k, centre) in centres.iter().enumerate() {
            let (start, mid, end) = corner(k);
            let (start, mid, end) = (tidy(start), tidy(mid), tidy(end));
            match previous_end {
                Some(from) if from != start => edges.push((Edge::Line(from, start), Pt::default())),
                None => first_start = start,
                _ => {}
            }
            edges.push((Edge::Arc { start, mid, end }, *centre));
            previous_end = Some(end);
        }
        let last = previous_end.expect("four corners were drawn");
        if last != first_start {
            edges.push((Edge::Line(last, first_start), Pt::default()));
        }
        edges
    }

    /// The outline as a closed run of short straight pieces, `(from, to)`, for what needs
    /// distances to it.
    pub fn segments(&self) -> Vec<(Pt, Pt)> {
        let mut out = Vec::new();
        for (edge, c) in self.pieces() {
            match edge {
                Edge::Line(a, b) => out.push((a, b)),
                Edge::Arc { start, end, .. } => {
                    let r = self.radius;
                    // Pieces short enough that none lies further than SAGITTA from the arc.
                    let step = 2.0 * (2.0 * r * SAGITTA).sqrt();
                    let pieces = ((std::f64::consts::FRAC_PI_2 * r / step).ceil() as usize).max(1);
                    let a0 = (start.y - c.y).atan2(start.x - c.x);
                    let mut a1 = (end.y - c.y).atan2(end.x - c.x);
                    // Clockwise on screen is increasing angle with y down.
                    while a1 < a0 {
                        a1 += std::f64::consts::TAU;
                    }
                    let mut from = start;
                    for i in 1..=pieces {
                        let to = if i == pieces {
                            end
                        } else {
                            let a = a0 + (a1 - a0) * i as f64 / pieces as f64;
                            Pt::new(c.x + r * a.cos(), c.y + r * a.sin())
                        };
                        out.push((from, to));
                        from = to;
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(r: f64) -> RoundRect {
        RoundRect::new(Rect::new(Pt::new(0.0, 0.0), Pt::new(20.0, 10.0)), r)
    }

    #[test]
    fn a_square_cornered_shape_is_four_lines() {
        let e = shape(0.0).edges();
        assert_eq!(e.len(), 4);
        assert!(e.iter().all(|e| matches!(e, Edge::Line(..))));
    }

    #[test]
    fn a_rounded_shape_is_four_arcs_and_four_lines_that_meet() {
        let e = shape(2.0).edges();
        assert_eq!(e.len(), 8);
        // Each edge ends where the next begins, and the last where the first begins.
        let ends = |e: &Edge| match *e {
            Edge::Line(a, b) => (a, b),
            Edge::Arc { start, end, .. } => (start, end),
        };
        for k in 0..e.len() {
            assert_eq!(ends(&e[k]).1, ends(&e[(k + 1) % e.len()]).0, "edge {k}");
        }
    }

    #[test]
    fn the_corner_is_cut_off() {
        let s = shape(2.0);
        assert!(s.contains(Pt::new(10.0, 5.0)));
        assert!(s.contains(Pt::new(2.0, 0.0)));
        assert!(!s.contains(Pt::new(0.2, 0.2)));
        assert!(s.contains(Pt::new(0.6, 0.6)), "inside the arc");
        assert!(!s.contains(Pt::new(-0.1, 5.0)));
        assert!(shape(0.0).contains(Pt::new(0.0, 0.0)));
    }

    #[test]
    fn depth_is_measured_to_the_arc_in_a_corner() {
        let s = shape(2.0);
        assert!((s.depth(Pt::new(10.0, 5.0)) - 5.0).abs() < 1e-9);
        // The point on the diagonal, 1 mm in from the sharp corner each way, is
        // (2 - 1) * sqrt(2) from the arc's centre; the arc is 2 away from it.
        let d = s.depth(Pt::new(1.0, 1.0));
        assert!((d - (2.0 - 2f64.sqrt())).abs() < 1e-9, "{d}");
    }

    #[test]
    fn the_polyline_follows_the_arc_within_a_few_microns() {
        let s = shape(3.0);
        let segments = s.segments();
        for (a, b) in &segments {
            let mid = Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
            let depth = s.depth(mid);
            assert!(depth.abs() < 0.0055, "{a:?} {b:?} {depth}");
        }
        // It is closed.
        for k in 0..segments.len() {
            let next = segments[(k + 1) % segments.len()].0;
            assert!(segments[k].1.distance(next) < 1e-9, "piece {k}");
        }
    }

    #[test]
    fn a_radius_cannot_pass_half_the_shorter_side() {
        assert_eq!(shape(50.0).radius, 5.0);
    }
}
