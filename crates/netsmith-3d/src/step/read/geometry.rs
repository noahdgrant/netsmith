//! The curves and surfaces of a model, in the file's length unit: where a parameter puts a point,
//! the derivatives a surface's normal and a search need, and the parameter of a point already on
//! a curve or surface. Every function is plain `f64` arithmetic in a fixed order and the scene's
//! own trig ([`crate::mesh::trig`]), so the same model gives the same bits on every machine.

use crate::mesh::trig::{atan2, sin_cos};

pub type P3 = [f64; 3];

pub fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: P3, s: f64) -> P3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn norm(a: P3) -> f64 {
    dot(a, a).sqrt()
}

pub fn dist(a: P3, b: P3) -> f64 {
    norm(sub(a, b))
}

/// `a` scaled to length 1, `None` when it has no length.
pub fn unit(a: P3) -> Option<P3> {
    let l = norm(a);
    (l > 1e-300 && l.is_finite()).then(|| scale(a, 1.0 / l))
}

/// A right-handed frame: `AXIS2_PLACEMENT_3D`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub o: P3,
    pub x: P3,
    pub y: P3,
    pub z: P3,
}

impl Frame {
    pub const IDENTITY: Frame = Frame {
        o: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
        z: [0.0, 0.0, 1.0],
    };

    /// The frame at `o` with its z along `axis` and its x along `reference` made square to it,
    /// as ISO 10303-42 builds an `AXIS2_PLACEMENT_3D`. `None` when the directions are zero or
    /// parallel.
    pub fn new(o: P3, axis: Option<P3>, reference: Option<P3>) -> Option<Frame> {
        let z = unit(axis.unwrap_or([0.0, 0.0, 1.0]))?;
        let r = match reference {
            Some(r) => r,
            // The standard's default: x unless z is along it.
            None if z[0].abs() < 0.9 => [1.0, 0.0, 0.0],
            None => [0.0, 1.0, 0.0],
        };
        let x = unit(sub(r, scale(z, dot(r, z))))?;
        Some(Frame {
            o,
            x,
            y: cross(z, x),
            z,
        })
    }

    /// The point at local `(a, b, c)`.
    pub fn at(&self, a: f64, b: f64, c: f64) -> P3 {
        [0, 1, 2].map(|i| self.o[i] + a * self.x[i] + b * self.y[i] + c * self.z[i])
    }

    /// A direction given in local coordinates.
    pub fn dir(&self, a: f64, b: f64, c: f64) -> P3 {
        [0, 1, 2].map(|i| a * self.x[i] + b * self.y[i] + c * self.z[i])
    }

    /// The local coordinates of `p`.
    pub fn local(&self, p: P3) -> P3 {
        let d = sub(p, self.o);
        [dot(d, self.x), dot(d, self.y), dot(d, self.z)]
    }
}

/// A B-spline: its degree, the knots with each written as often as its multiplicity, and,
/// for a rational one, a weight for each control point.
#[derive(Debug, Clone, PartialEq)]
pub struct Knots {
    pub degree: usize,
    pub knots: Vec<f64>,
}

impl Knots {
    /// The range the parameter runs over.
    pub fn domain(&self) -> (f64, f64) {
        let n = self.knots.len() - self.degree - 1;
        (self.knots[self.degree], self.knots[n])
    }

    /// How many control points it needs.
    pub fn count(&self) -> usize {
        self.knots.len() - self.degree - 1
    }

    /// The span `[knots[s], knots[s+1])` that holds `t` (the last one for the end).
    fn span(&self, t: f64) -> usize {
        let p = self.degree;
        let n = self.count();
        if t >= self.knots[n] {
            // The last span with length.
            let mut s = n - 1;
            while s > p && self.knots[s] >= self.knots[n] {
                s -= 1;
            }
            return s;
        }
        if t <= self.knots[p] {
            let mut s = p;
            while s + 1 < n && self.knots[s + 1] <= self.knots[p] {
                s += 1;
            }
            return s;
        }
        // The last s with knots[s] <= t.
        let (mut lo, mut hi) = (p, n);
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.knots[mid] <= t {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// The span of `t` and the `degree + 1` basis functions that are not zero there with their
    /// first derivatives (The NURBS Book, A2.2, the derivatives from the functions of one degree
    /// lower). It works in arrays on the stack: a surface is evaluated millions of times.
    pub fn basis(&self, t: f64) -> Basis {
        let p = self.degree;
        let s = self.span(t);
        let k = &self.knots;
        let mut out = Basis {
            span: s,
            value: [0.0; MAX_ORDER],
            deriv: [0.0; MAX_ORDER],
        };
        let n = &mut out.value;
        let mut left = [0.0; MAX_ORDER];
        let mut right = [0.0; MAX_ORDER];
        // The functions of degree p - 1, and the knot differences of the last step.
        let mut lower = [0.0; MAX_ORDER];
        let mut den = [0.0; MAX_ORDER];
        n[0] = 1.0;
        for j in 1..=p {
            if j == p {
                lower[..p].copy_from_slice(&n[..p]);
            }
            left[j] = t - k[s + 1 - j];
            right[j] = k[s + j] - t;
            let mut saved = 0.0;
            for r in 0..j {
                let d = right[r + 1] + left[j - r];
                if j == p {
                    den[r] = d;
                }
                let temp = if d == 0.0 { 0.0 } else { n[r] / d };
                n[r] = saved + right[r + 1] * temp;
                saved = left[j - r] * temp;
            }
            n[j] = saved;
        }
        if p > 0 {
            for r in 0..=p {
                let mut v = 0.0;
                if r >= 1 && den[r - 1] != 0.0 {
                    v += lower[r - 1] / den[r - 1];
                }
                if r < p && den[r] != 0.0 {
                    v -= lower[r] / den[r];
                }
                out.deriv[r] = v * p as f64;
            }
        }
        out
    }
}

/// The most control points a B-spline's piece depends on: a degree up to 25, as the reader
/// takes them.
pub const MAX_ORDER: usize = 26;

/// The basis functions of a B-spline at a parameter: `value[j]` and `deriv[j]` for the control
/// point `span - degree + j`.
#[derive(Debug, Clone, Copy)]
pub struct Basis {
    pub span: usize,
    pub value: [f64; MAX_ORDER],
    pub deriv: [f64; MAX_ORDER],
}

/// A B-spline curve, rational when it has weights.
#[derive(Debug, Clone, PartialEq)]
pub struct BSplineCurve {
    pub knots: Knots,
    pub points: Vec<P3>,
    pub weights: Option<Vec<f64>>,
}

impl BSplineCurve {
    /// The point and the derivative at `t`.
    pub fn eval(&self, t: f64) -> (P3, P3) {
        let Basis {
            span: s,
            value: n,
            deriv: dn,
        } = self.knots.basis(t);
        let p = self.knots.degree;
        let (mut a, mut da, mut w, mut dw) = ([0.0; 3], [0.0; 3], 0.0, 0.0);
        for j in 0..=p {
            let i = s - p + j;
            let wi = self.weights.as_ref().map_or(1.0, |ws| ws[i]);
            let q = self.points[i];
            for c in 0..3 {
                a[c] += n[j] * wi * q[c];
                da[c] += dn[j] * wi * q[c];
            }
            w += n[j] * wi;
            dw += dn[j] * wi;
        }
        let point = scale(a, 1.0 / w);
        let deriv = scale(sub(da, scale(point, dw)), 1.0 / w);
        (point, deriv)
    }
}

/// A B-spline surface, rational when it has weights: `points[i][j]` with `i` along u.
#[derive(Debug, Clone, PartialEq)]
pub struct BSplineSurface {
    pub u: Knots,
    pub v: Knots,
    pub points: Vec<Vec<P3>>,
    pub weights: Option<Vec<Vec<f64>>>,
}

#[cfg(test)]
thread_local! {
    /// How many times this thread has evaluated a B-spline surface: the tests bound the work a
    /// face takes by it, which a clock on a busy machine cannot.
    pub static EVALS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

impl BSplineSurface {
    /// The point and the derivatives along u and v at `(u, v)`.
    pub fn eval(&self, u: f64, v: f64) -> (P3, P3, P3) {
        #[cfg(test)]
        EVALS.with(|n| n.set(n.get() + 1));
        let Basis {
            span: su,
            value: nu,
            deriv: dnu,
        } = self.u.basis(u);
        let Basis {
            span: sv,
            value: nv,
            deriv: dnv,
        } = self.v.basis(v);
        let (p, q) = (self.u.degree, self.v.degree);
        let (mut a, mut au, mut av) = ([0.0; 3], [0.0; 3], [0.0; 3]);
        let (mut w, mut wu, mut wv) = (0.0, 0.0, 0.0);
        for k in 0..=p {
            let i = su - p + k;
            for l in 0..=q {
                let j = sv - q + l;
                let wij = self.weights.as_ref().map_or(1.0, |ws| ws[i][j]);
                let pt = self.points[i][j];
                let (b, bu, bv) = (
                    nu[k] * nv[l] * wij,
                    dnu[k] * nv[l] * wij,
                    nu[k] * dnv[l] * wij,
                );
                for c in 0..3 {
                    a[c] += b * pt[c];
                    au[c] += bu * pt[c];
                    av[c] += bv * pt[c];
                }
                w += b;
                wu += bu;
                wv += bv;
            }
        }
        let s = scale(a, 1.0 / w);
        let su_ = scale(sub(au, scale(s, wu)), 1.0 / w);
        let sv_ = scale(sub(av, scale(s, wv)), 1.0 / w);
        (s, su_, sv_)
    }
}

/// A curve an edge runs along.
#[derive(Debug, Clone, PartialEq)]
pub enum Curve {
    /// Through `p` along the unit `d`, the parameter a length.
    Line {
        p: P3,
        d: P3,
    },
    /// Round the frame's z, the parameter an angle from its x.
    Circle {
        f: Frame,
        r: f64,
    },
    /// `a` along the frame's x and `b` along its y.
    Ellipse {
        f: Frame,
        a: f64,
        b: f64,
    },
    BSpline(BSplineCurve),
    /// Straight segments through the points, the parameter the segment's index and the
    /// fraction along it.
    Polyline(Vec<P3>),
}

impl Curve {
    pub fn point(&self, t: f64) -> P3 {
        match self {
            Curve::Line { p, d } => add(*p, scale(*d, t)),
            Curve::Circle { f, r } => {
                let (s, c) = sin_cos(t);
                f.at(r * c, r * s, 0.0)
            }
            Curve::Ellipse { f, a, b } => {
                let (s, c) = sin_cos(t);
                f.at(a * c, b * s, 0.0)
            }
            Curve::BSpline(b) => b.eval(t).0,
            Curve::Polyline(ps) => {
                let n = ps.len() - 1;
                let k = (t.floor().max(0.0) as usize).min(n.saturating_sub(1));
                let f = t - k as f64;
                let (a, b) = (ps[k], ps[(k + 1).min(n)]);
                add(a, scale(sub(b, a), f))
            }
        }
    }

    /// Whether the parameter goes round: an angle.
    pub fn period(&self) -> Option<f64> {
        match self {
            Curve::Circle { .. } | Curve::Ellipse { .. } => Some(std::f64::consts::TAU),
            _ => None,
        }
    }

    /// The parameter of `p`, a point on the curve (or the nearest point's, for a point off it).
    pub fn param(&self, p: P3) -> f64 {
        match self {
            Curve::Line { p: o, d } => dot(sub(p, *o), *d),
            Curve::Circle { f, .. } => {
                let l = f.local(p);
                atan2(l[1], l[0])
            }
            Curve::Ellipse { f, a, b } => {
                let l = f.local(p);
                atan2(l[1] / b, l[0] / a)
            }
            Curve::BSpline(b) => bspline_param(b, p),
            Curve::Polyline(ps) => {
                let mut best = (f64::INFINITY, 0.0);
                for k in 0..ps.len() - 1 {
                    let (a, b) = (ps[k], ps[k + 1]);
                    let ab = sub(b, a);
                    let l2 = dot(ab, ab);
                    let f = if l2 > 0.0 {
                        (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let d = dist(p, add(a, scale(ab, f)));
                    if d < best.0 {
                        best = (d, k as f64 + f);
                    }
                }
                best.1
            }
        }
    }
}

/// The parameter of the point of `b` nearest `p`: the nearest of samples, then Newton's steps.
fn bspline_param(b: &BSplineCurve, p: P3) -> f64 {
    let (t0, t1) = b.knots.domain();
    // The ends first: an edge's vertices are most often there.
    let (e0, e1) = (b.eval(t0).0, b.eval(t1).0);
    let n = 16 * b.points.len().max(2);
    let mut best = (dist(e0, p), t0);
    if dist(e1, p) < best.0 {
        best = (dist(e1, p), t1);
    }
    for k in 1..n {
        let t = t0 + (t1 - t0) * k as f64 / n as f64;
        let d = dist(b.eval(t).0, p);
        if d < best.0 {
            best = (d, t);
        }
    }
    let mut t = best.1;
    for _ in 0..20 {
        let (q, d) = b.eval(t);
        let e = sub(q, p);
        let dd = dot(d, d);
        if dd <= 0.0 {
            break;
        }
        let step = dot(e, d) / dd;
        let next = (t - step).clamp(t0, t1);
        if (next - t).abs() <= 1e-14 * (1.0 + t.abs()) {
            t = next;
            break;
        }
        t = next;
    }
    t
}

/// How near a pole (or an apex, or a B-spline's collapsed edge), as a fraction of the surface's
/// size, a point is taken to be on it: a file's vertex at a pole is often a few nanometres off.
const POLE: f64 = 1e-6;

/// A surface a face lies on. The parameters `(u, v)` and the normal `Su x Sv` are as
/// ISO 10303-42 states them, so a face's `same_sense` says whether its outside is that normal.
#[derive(Debug, Clone, PartialEq)]
pub enum Surface {
    /// `(u, v)` along the frame's x and y.
    Plane(Frame),
    /// u round the frame's z, v along it.
    Cylinder {
        f: Frame,
        r: f64,
    },
    /// Radius `r + v * tan` at `v` along the frame's z, u round it.
    Cone {
        f: Frame,
        r: f64,
        tan: f64,
    },
    /// u the longitude round z, v the latitude from -pi/2 to pi/2.
    Sphere {
        f: Frame,
        r: f64,
    },
    /// u round z, v round the tube.
    Torus {
        f: Frame,
        major: f64,
        minor: f64,
    },
    BSpline(BSplineSurface),
}

/// A surface's point and its derivatives along u and v.
pub type Eval = (P3, P3, P3);

impl Surface {
    pub fn eval(&self, u: f64, v: f64) -> Eval {
        match self {
            Surface::Plane(f) => (f.at(u, v, 0.0), f.x, f.y),
            Surface::Cylinder { f, r } => {
                let (s, c) = sin_cos(u);
                (f.at(r * c, r * s, v), f.dir(-r * s, r * c, 0.0), f.z)
            }
            Surface::Cone { f, r, tan } => {
                let (s, c) = sin_cos(u);
                let rad = r + v * tan;
                (
                    f.at(rad * c, rad * s, v),
                    f.dir(-rad * s, rad * c, 0.0),
                    f.dir(tan * c, tan * s, 1.0),
                )
            }
            Surface::Sphere { f, r } => {
                let (s, c) = sin_cos(u);
                let (sv, cv) = sin_cos(v);
                (
                    f.at(r * cv * c, r * cv * s, r * sv),
                    f.dir(-r * cv * s, r * cv * c, 0.0),
                    f.dir(-r * sv * c, -r * sv * s, r * cv),
                )
            }
            Surface::Torus { f, major, minor } => {
                let (s, c) = sin_cos(u);
                let (sv, cv) = sin_cos(v);
                let rad = major + minor * cv;
                (
                    f.at(rad * c, rad * s, minor * sv),
                    f.dir(-rad * s, rad * c, 0.0),
                    f.dir(-minor * sv * c, -minor * sv * s, minor * cv),
                )
            }
            Surface::BSpline(b) => b.eval(u, v),
        }
    }

    pub fn point(&self, u: f64, v: f64) -> P3 {
        self.eval(u, v).0
    }

    /// The periods of u and v, for the surfaces whose parameters go round.
    pub fn periods(&self) -> [Option<f64>; 2] {
        let tau = Some(std::f64::consts::TAU);
        match self {
            Surface::Plane(_) | Surface::BSpline(_) => [None, None],
            Surface::Cylinder { .. } | Surface::Cone { .. } | Surface::Sphere { .. } => [tau, None],
            Surface::Torus { .. } => [tau, tau],
        }
    }

    /// The parameters `(u, v)` of `p`, a point on the surface, with an angle in `(-pi, pi]`;
    /// `hint` is a nearby point's, which a B-spline's search starts from. The second value
    /// names the parameter (0 for u, 1 for v) that is undefined there: at a sphere's pole or a
    /// cone's apex every u is the same point.
    pub fn param(&self, p: P3, hint: Option<[f64; 2]>) -> ([f64; 2], Option<usize>) {
        match self {
            Surface::Plane(f) => {
                let l = f.local(p);
                ([l[0], l[1]], None)
            }
            Surface::Cylinder { f, .. } => {
                let l = f.local(p);
                ([atan2(l[1], l[0]), l[2]], None)
            }
            Surface::Cone { f, r, tan } => {
                let l = f.local(p);
                let rad = r + l[2] * tan;
                let scale = r.abs().max(l[2].abs()).max(1e-9);
                (
                    [atan2(l[1], l[0]), l[2]],
                    (rad.abs() <= POLE * scale).then_some(0),
                )
            }
            Surface::Sphere { f, r } => {
                let l = f.local(p);
                let h = l[0].hypot(l[1]);
                (
                    [atan2(l[1], l[0]), atan2(l[2], h)],
                    (h <= POLE * r).then_some(0),
                )
            }
            Surface::Torus { f, major, .. } => {
                let l = f.local(p);
                let h = l[0].hypot(l[1]);
                (
                    [atan2(l[1], l[0]), atan2(l[2], h - major)],
                    (h <= 1e-12 * major.abs()).then_some(0),
                )
            }
            Surface::BSpline(b) => bspline_surface_param(b, p, hint),
        }
    }
}

/// The parameters of the point of `b` nearest `p`: from `hint` when it is near, else from the
/// nearest of a grid of samples, then by Newton's steps on the distance.
fn bspline_surface_param(
    b: &BSplineSurface,
    p: P3,
    hint: Option<[f64; 2]>,
) -> ([f64; 2], Option<usize>) {
    Inverse::new(b).param(p, hint)
}

/// The search for the parameters of points on one B-spline surface: its size, and its grid of
/// samples, made the first time a point needs it and kept for the face's other points.
pub struct Inverse<'a> {
    b: &'a BSplineSurface,
    size: f64,
    seeds: std::cell::OnceCell<Vec<([f64; 2], P3)>>,
}

impl<'a> Inverse<'a> {
    pub fn new(b: &'a BSplineSurface) -> Inverse<'a> {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for row in &b.points {
            for q in row {
                for c in 0..3 {
                    lo[c] = lo[c].min(q[c]);
                    hi[c] = hi[c].max(q[c]);
                }
            }
        }
        Inverse {
            b,
            size: dist(lo, hi).max(1e-9),
            seeds: std::cell::OnceCell::new(),
        }
    }

    /// The samples: each way, eight a control point over the knot spans with length, and at
    /// least `2 (degree + 1)` in each span, so every piece of the surface has some whatever its
    /// knots' spacing.
    fn seeds(&self) -> &[([f64; 2], P3)] {
        self.seeds.get_or_init(|| {
            let steps = |k: &Knots| -> Vec<f64> {
                let (t0, t1) = k.domain();
                let mut out = vec![t0];
                let spans = k.knots.windows(2).filter(|w| w[1] > w[0]).count().max(1);
                let n = (2 * (k.degree + 1)).max((8 * k.count()).div_ceil(spans));
                for w in k.knots.windows(2) {
                    let (a, z) = (w[0].max(t0), w[1].min(t1));
                    if z <= a {
                        continue;
                    }
                    for j in 1..=n {
                        out.push(if j == n {
                            z
                        } else {
                            a + (z - a) * j as f64 / n as f64
                        });
                    }
                }
                out
            };
            let (us, vs) = (steps(&self.b.u), steps(&self.b.v));
            let mut out = Vec::with_capacity(us.len() * vs.len());
            for &u in &us {
                for &v in &vs {
                    out.push(([u, v], self.b.point_at([u, v])));
                }
            }
            out
        })
    }

    /// Newton's steps on the distance from `uv`, kept in the domain: the nearest of the
    /// parameters reached (a step held at the domain's edge can lead away) and its distance
    /// from `p`.
    fn newton(&self, p: P3, mut uv: [f64; 2]) -> ([f64; 2], f64) {
        let b = self.b;
        let (u0, u1) = b.u.domain();
        let (v0, v1) = b.v.domain();
        let mut best = (uv, f64::INFINITY);
        for _ in 0..30 {
            let (q, su, sv) = b.eval(uv[0], uv[1]);
            let e = sub(q, p);
            let d = norm(e);
            if d < best.1 {
                best = (uv, d);
            }
            // The normal equations of the linearised distance.
            let (a, bb, c) = (dot(su, su), dot(su, sv), dot(sv, sv));
            let (r0, r1) = (dot(e, su), dot(e, sv));
            let det = a * c - bb * bb;
            if det.abs() <= 1e-300 {
                break;
            }
            let du = (c * r0 - bb * r1) / det;
            let dv = (a * r1 - bb * r0) / det;
            let next = [(uv[0] - du).clamp(u0, u1), (uv[1] - dv).clamp(v0, v1)];
            let moved = (next[0] - uv[0]).abs() + (next[1] - uv[1]).abs();
            uv = next;
            if moved <= 1e-14 * (1.0 + uv[0].abs() + uv[1].abs()) {
                break;
            }
        }
        let d = dist(b.point_at(uv), p);
        if d <= best.1 { (uv, d) } else { best }
    }

    /// The parameters of `p` and the one undefined there, as [`Surface::param`] gives them.
    pub fn param(&self, p: P3, hint: Option<[f64; 2]>) -> ([f64; 2], Option<usize>) {
        let b = self.b;
        let (u0, u1) = b.u.domain();
        let (v0, v1) = b.v.domain();
        let close = 1e-7 * self.size;
        let from_hint = hint.map(|h| self.newton(p, h));
        let (uv, _) = match from_hint {
            Some(r) if r.1 <= close => r,
            _ => {
                // The nearest sample, then Newton's steps. A point a little off the surface (an
                // edge's, within the file's own tolerance) misses `close` from the hint too, and
                // on a surface closed on itself its seam has two parameters as near: the hint's
                // is kept when it is about as near, so a loop runs on without jumping the seam.
                let mut seed = ([u0, v0], f64::INFINITY);
                for &(uv, q) in self.seeds() {
                    let d = dist(q, p);
                    if d < seed.1 {
                        seed = (uv, d);
                    }
                }
                let r = self.newton(p, seed.0);
                match from_hint {
                    Some(h) if h.1 <= 2.0 * r.1 + close => h,
                    _ => r,
                }
            }
        };
        // On the domain's edge when within a hair of it, as a B-spline's own edges are.
        let mut uv = uv;
        for (x, (a, z)) in uv.iter_mut().zip([(u0, u1), (v0, v1)]) {
            let hair = 1e-10 * (z - a);
            if (*x - a).abs() <= hair {
                *x = a;
            } else if (*x - z).abs() <= hair {
                *x = z;
            }
        }
        let (_, su, sv) = b.eval(uv[0], uv[1]);
        let undefined = if norm(su) <= POLE * self.size {
            Some(0)
        } else if norm(sv) <= POLE * self.size {
            Some(1)
        } else {
            None
        };
        (uv, undefined)
    }
}

impl BSplineSurface {
    fn point_at(&self, uv: [f64; 2]) -> P3 {
        self.eval(uv[0], uv[1]).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rational_quadratic_is_an_exact_quarter_circle() {
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let b = BSplineCurve {
            knots: Knots {
                degree: 2,
                knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            },
            points: vec![[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            weights: Some(vec![1.0, w, 1.0]),
        };
        for k in 0..=10 {
            let p = b.eval(k as f64 / 10.0).0;
            assert!((norm(p) - 1.0).abs() < 1e-15, "{p:?}");
        }
        let p = Curve::BSpline(b.clone()).point(0.3);
        assert!((Curve::BSpline(b).param(p) - 0.3).abs() < 1e-12);
    }

    #[test]
    fn surfaces_give_back_the_parameters_of_their_points() {
        let f = Frame::new(
            [1.0, 2.0, 3.0],
            Some([0.0, 1.0, 1.0]),
            Some([1.0, 0.0, 0.0]),
        )
        .unwrap();
        let surfaces = [
            Surface::Plane(f),
            Surface::Cylinder { f, r: 2.0 },
            Surface::Cone {
                f,
                r: 2.0,
                tan: 0.5,
            },
            Surface::Sphere { f, r: 2.0 },
            Surface::Torus {
                f,
                major: 3.0,
                minor: 1.0,
            },
        ];
        for s in &surfaces {
            for (u, v) in [(0.3, 0.2), (-2.5, 1.0), (3.0, -0.4)] {
                let p = s.point(u, v);
                let ([pu, pv], _) = s.param(p, None);
                assert!(dist(s.point(pu, pv), p) < 1e-12, "{s:?} {u} {v}");
                // The normal Su x Sv of the elementary surfaces points away from the axis.
                let (_, su, sv) = s.eval(u, v);
                let n = cross(su, sv);
                if let Surface::Sphere { f, .. } = s {
                    assert!(dot(n, sub(p, f.o)) > 0.0);
                }
            }
        }
    }

    #[test]
    fn a_bicubic_surface_gives_back_its_parameters() {
        let knots = Knots {
            degree: 3,
            knots: vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0],
        };
        let points: Vec<Vec<P3>> = (0..5)
            .map(|i| {
                (0..5)
                    .map(|j| {
                        let (x, y) = (i as f64, j as f64);
                        [x, y, 0.3 * (x * y).sin()]
                    })
                    .collect()
            })
            .collect();
        let b = BSplineSurface {
            u: knots.clone(),
            v: knots,
            points,
            weights: None,
        };
        let s = Surface::BSpline(b);
        for (u, v) in [(0.1, 0.9), (0.5, 0.5), (0.77, 0.03), (1.0, 0.0)] {
            let ([pu, pv], _) = s.param(s.point(u, v), None);
            assert!(
                (pu - u).abs() < 1e-9 && (pv - v).abs() < 1e-9,
                "{u} {v}: {pu} {pv}"
            );
        }
    }
}
