//! Points, arcs given as a chord and a bulge, and their flattening.
//!
//! An arc is stored the way a DXF `LWPOLYLINE` stores it: from `a` to `b` with a *bulge*, the
//! tangent of a quarter of the angle the arc turns through, positive when it turns
//! counterclockwise (in a frame with y up). A bulge of 0 is a straight line, and 1 is a
//! half circle. Mirroring a drawing negates every bulge.

use std::f64::consts::{PI, TAU};

/// A point, in millimetres.
pub type P = (f64, f64);

pub(crate) fn dist(a: P, b: P) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// The circle an arc lies on, and the angles it covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArcGeom {
    pub centre: P,
    pub radius: f64,
    /// Radians, of the point `a`.
    pub start: f64,
    /// Radians turned through, positive counterclockwise (y up).
    pub sweep: f64,
}

impl ArcGeom {
    /// The arc from `a` to `b` with bulge `bulge` (not zero), or `None` if `a` and `b` are the
    /// same point.
    pub fn of(a: P, b: P, bulge: f64) -> Option<ArcGeom> {
        let d = dist(a, b);
        if d == 0.0 || bulge == 0.0 {
            return None;
        }
        let half = 2.0 * bulge.atan(); // half the angle turned through
        let radius = (d / (2.0 * half.sin())).abs();
        let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        // The centre is on the left of a to b by (d / 2) / tan(half): negative is on the right.
        let offset = (d / 2.0) / half.tan();
        let left = (-(b.1 - a.1) / d, (b.0 - a.0) / d);
        let centre = (mid.0 + left.0 * offset, mid.1 + left.1 * offset);
        Some(ArcGeom {
            centre,
            radius,
            start: (a.1 - centre.1).atan2(a.0 - centre.0),
            sweep: 2.0 * half,
        })
    }

    pub fn point_at(&self, t: f64) -> P {
        let angle = self.start + self.sweep * t;
        (
            self.centre.0 + self.radius * angle.cos(),
            self.centre.1 + self.radius * angle.sin(),
        )
    }

    pub fn length(&self) -> f64 {
        self.radius * self.sweep.abs()
    }

    /// Whether the arc passes through the angle `angle` (radians).
    fn covers(&self, angle: f64) -> bool {
        let (from, to) = if self.sweep >= 0.0 {
            (self.start, self.start + self.sweep)
        } else {
            (self.start + self.sweep, self.start)
        };
        let mut x = (angle - from).rem_euclid(TAU);
        if x > TAU - 1e-12 {
            x = 0.0;
        }
        x <= to - from + 1e-12
    }
}

/// The least and greatest x and y of the straight or curved edge from `a` to `b`.
pub(crate) fn edge_bounds(a: P, b: P, bulge: f64) -> (P, P) {
    let mut min = (a.0.min(b.0), a.1.min(b.1));
    let mut max = (a.0.max(b.0), a.1.max(b.1));
    if let Some(arc) = ArcGeom::of(a, b, bulge) {
        for k in 0..4 {
            let angle = f64::from(k) * PI / 2.0;
            if arc.covers(angle) {
                let p = (
                    arc.centre.0 + arc.radius * angle.cos(),
                    arc.centre.1 + arc.radius * angle.sin(),
                );
                min = (min.0.min(p.0), min.1.min(p.1));
                max = (max.0.max(p.0), max.1.max(p.1));
            }
        }
    }
    (min, max)
}

/// The length of the edge from `a` to `b`.
pub(crate) fn edge_length(a: P, b: P, bulge: f64) -> f64 {
    match ArcGeom::of(a, b, bulge) {
        Some(arc) => arc.length(),
        None => dist(a, b),
    }
}

/// The points of the edge from `a` to `b` as a polyline whose chords are at most `tol` from
/// the arc, `a` first and `b` last.
pub(crate) fn flatten_edge(a: P, b: P, bulge: f64, tol: f64) -> Vec<P> {
    let Some(arc) = ArcGeom::of(a, b, bulge) else {
        return vec![a, b];
    };
    let step = if tol >= arc.radius {
        PI / 2.0
    } else {
        2.0 * (1.0 - tol / arc.radius).acos()
    };
    let n = ((arc.sweep.abs() / step).ceil() as usize).clamp(2, 100_000);
    let mut out = Vec::with_capacity(n + 1);
    out.push(a);
    for i in 1..n {
        out.push(arc.point_at(i as f64 / n as f64));
    }
    out.push(b);
    out
}

fn cross(o: P, a: P, b: P) -> f64 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

fn point_segment(p: P, a: P, b: P) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    if l2 == 0.0 {
        return dist(p, a);
    }
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0);
    dist(p, (a.0 + t * dx, a.1 + t * dy))
}

/// The least distance between the segments `a1 a2` and `b1 b2`: 0 if they cross.
pub(crate) fn segment_distance(a1: P, a2: P, b1: P, b2: P) -> f64 {
    let (d1, d2) = (cross(a1, a2, b1), cross(a1, a2, b2));
    let (d3, d4) = (cross(b1, b2, a1), cross(b1, b2, a2));
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
        return 0.0;
    }
    point_segment(a1, b1, b2)
        .min(point_segment(a2, b1, b2))
        .min(point_segment(b1, a1, a2))
        .min(point_segment(b2, a1, a2))
}

/// Whether `p` is inside the closed polyline `ring` (even-odd rule).
pub(crate) fn inside(p: P, ring: &[P]) -> bool {
    let mut odd = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[j]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            odd = !odd;
        }
        j = i;
    }
    odd
}

/// A 2D affine map: `x' = a x + b y + e`, `y' = c x + d y + f`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Xf {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Xf {
    pub const IDENTITY: Xf = Xf {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn scale(sx: f64, sy: f64) -> Xf {
        Xf {
            a: sx,
            d: sy,
            ..Xf::IDENTITY
        }
    }

    pub fn translate(x: f64, y: f64) -> Xf {
        Xf {
            e: x,
            f: y,
            ..Xf::IDENTITY
        }
    }

    pub fn rotate(radians: f64) -> Xf {
        let (s, c) = radians.sin_cos();
        Xf {
            a: c,
            b: -s,
            c: s,
            d: c,
            e: 0.0,
            f: 0.0,
        }
    }

    /// The map that does `other` first and then `self`.
    pub fn after(&self, other: &Xf) -> Xf {
        Xf {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.a * other.e + self.b * other.f + self.e,
            f: self.c * other.e + self.d * other.f + self.f,
        }
    }

    pub fn apply(&self, p: P) -> P {
        (
            self.a * p.0 + self.b * p.1 + self.e,
            self.c * p.0 + self.d * p.1 + self.f,
        )
    }

    pub fn det(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    /// Whether it keeps circles circles: a rotation, a mirror and one scale.
    pub fn is_similarity(&self) -> bool {
        let (c1, c2) = (self.a.hypot(self.c), self.b.hypot(self.d));
        let tol = 1e-9 * c1.max(c2).max(1.0);
        (c1 - c2).abs() <= tol
            && (self.a * self.b + self.c * self.d).abs() <= tol * tol.max(1.0) + 1e-12
    }

    /// The factor lengths are multiplied by (for a similarity).
    pub fn length_scale(&self) -> f64 {
        self.det().abs().sqrt()
    }
}
