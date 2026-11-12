//! A model's faces as triangles, to a fixed chord tolerance.
//!
//! Each edge is cut into straight pieces once, and both faces it bounds use the same points, so
//! a closed shell stays closed. A face's loops are taken into its surface's parameters `(u, v)`,
//! where a loop that goes round a cylinder, a cone, a sphere or a torus is joined to another
//! (or to the pole) along a seam; the region is cut into triangles by the scene's own ear
//! clipping, keeping every point of the loops; then, on a curved surface, any triangle side
//! whose middle is more than the tolerance off the surface is halved, round after round, until
//! none is. The loops' own sides are never halved: they are already within the tolerance.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use super::geometry::{
    Curve, Eval, Inverse, P3, Surface, add, cross, dist, norm, scale, sub, unit,
};
use super::model::{Edge, Face, Loop};
use crate::mesh::triangulate_keeping;

/// The furthest a triangle may be from the surface it stands for, in mm: an edge, a side of a
/// triangle and a triangle's middle are all within it.
pub const CHORD_TOLERANCE: f64 = 0.01;

/// The widest angle a piece of a circle or an ellipse spans, so that a small round is still
/// round: twelve pieces a turn.
const MAX_ANGLE: f64 = std::f64::consts::PI / 6.0;

/// The rounds of halving, and the most triangles a face may need.
const MAX_ROUNDS: usize = 20;
const MAX_TRIANGLES: usize = 200_000;

/// The most points an edge may need.
const MAX_EDGE_POINTS: usize = 100_000;

/// The points along an edge from its start to its end, both exactly the edge's vertices, the
/// pieces between within `tol` of the curve.
pub fn edge_points(e: &Edge, tol: f64) -> Result<Vec<P3>, String> {
    let c = &e.curve;
    // Its vertices on its curve, as a file's tolerance has them.
    for (p, which) in [(e.start, "start"), (e.end, "end")] {
        let off = dist(c.point(c.param(p)), p);
        if off.is_nan() || off > 5.0 * tol {
            return Err(format!("its {which} is {off:.3e} off its curve"));
        }
    }
    let closed = dist(e.start, e.end) <= 1e-9 * (1.0 + norm(e.start));
    let mut out = vec![e.start];
    match c {
        Curve::Line { .. } => {}
        Curve::Circle { .. } | Curve::Ellipse { .. } => {
            let tau = std::f64::consts::TAU;
            let t0 = c.param(e.start);
            let mut t1 = c.param(e.end);
            if e.same_sense {
                while t1 <= t0 + if closed { 1e-12 } else { 0.0 } {
                    t1 += tau;
                }
                while t1 - t0 > tau + 1e-12 {
                    t1 -= tau;
                }
            } else {
                while t1 >= t0 - if closed { 1e-12 } else { 0.0 } {
                    t1 -= tau;
                }
                while t0 - t1 > tau + 1e-12 {
                    t1 += tau;
                }
            }
            // The radius of curvature at its tightest.
            let r = match c {
                Curve::Circle { r, .. } => *r,
                Curve::Ellipse { a, b, .. } => a.min(*b) * a.min(*b) / a.max(*b),
                _ => unreachable!(),
            };
            // A chord over an angle `s` is `r (1 - cos(s/2))` from the arc, below `r s^2 / 8`.
            let step = (8.0 * tol / r).sqrt().min(MAX_ANGLE);
            let n = ((t1 - t0).abs() / step).ceil().max(1.0);
            if n.is_nan() || n > MAX_EDGE_POINTS as f64 {
                return Err(format!("it needs more than {MAX_EDGE_POINTS} points"));
            }
            let n = n as usize;
            for k in 1..n {
                out.push(c.point(t0 + (t1 - t0) * k as f64 / n as f64));
            }
        }
        Curve::BSpline(b) => {
            let (d0, d1) = b.knots.domain();
            let (mut t0, mut t1) = (c.param(e.start), c.param(e.end));
            if closed && (t1 - t0).abs() <= 1e-12 * (1.0 + d1.abs()) {
                (t0, t1) = if e.same_sense { (d0, d1) } else { (d1, d0) };
            }
            // The knots between split the run; each piece is halved until it is within tol.
            let (lo, hi) = (t0.min(t1), t0.max(t1));
            let mut breaks = vec![t0];
            let mut inner: Vec<f64> = Vec::new();
            for &k in &b.knots.knots {
                if k > lo && k < hi && inner.last() != Some(&k) {
                    inner.push(k);
                }
            }
            if t1 < t0 {
                inner.reverse();
            }
            breaks.extend(inner);
            breaks.push(t1);
            let deg = b.knots.degree;
            for w in breaks.windows(2) {
                // Each knot span starts as `degree` pieces.
                for k in 0..deg {
                    let a = w[0] + (w[1] - w[0]) * k as f64 / deg as f64;
                    let z = w[0] + (w[1] - w[0]) * (k + 1) as f64 / deg as f64;
                    let pa = *out.last().unwrap_or(&e.start);
                    let pz = c.point(z);
                    split_curve(c, a, z, pa, pz, tol, 0, &mut out);
                    out.push(pz);
                }
            }
            out.pop();
            if out.len() > MAX_EDGE_POINTS {
                return Err(format!("it needs more than {MAX_EDGE_POINTS} points"));
            }
        }
        Curve::Polyline(ps) => {
            let (t0, t1) = (c.param(e.start), c.param(e.end));
            let (lo, hi) = (t0.min(t1), t0.max(t1));
            let mut mid: Vec<P3> = (0..ps.len())
                .filter(|&k| (k as f64) > lo && (k as f64) < hi)
                .map(|k| ps[k])
                .collect();
            if t1 < t0 {
                mid.reverse();
            }
            out.extend(mid);
        }
    }
    out.push(e.end);
    Ok(out)
}

/// Pushes the points strictly between `a` and `z` that bring the curve within `tol` of the
/// pieces.
#[allow(clippy::too_many_arguments)]
fn split_curve(
    c: &Curve,
    a: f64,
    z: f64,
    pa: P3,
    pz: P3,
    tol: f64,
    depth: usize,
    out: &mut Vec<P3>,
) {
    if depth >= 16 {
        return;
    }
    let m = 0.5 * (a + z);
    let pm = c.point(m);
    let chord = |t: f64, p: P3| {
        let f = (t - a) / (z - a);
        dist(p, add(pa, scale(sub(pz, pa), f)))
    };
    let q1 = a + 0.25 * (z - a);
    let q3 = a + 0.75 * (z - a);
    let off = chord(m, pm)
        .max(chord(q1, c.point(q1)))
        .max(chord(q3, c.point(q3)));
    if off <= tol {
        return;
    }
    split_curve(c, a, m, pa, pm, tol, depth + 1, out);
    out.push(pm);
    split_curve(c, m, z, pm, pz, tol, depth + 1, out);
}

/// A face's triangles in the file's unit: positions, unit normals and indices, each triangle
/// counterclockwise seen from outside.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FaceMesh {
    pub positions: Vec<P3>,
    pub normals: Vec<P3>,
    pub indices: Vec<[u32; 3]>,
}

/// A loop in the surface's parameters: the points and their `(u, v)`, the sides between them
/// all loop sides.
struct UvLoop {
    xyz: Vec<P3>,
    uv: Vec<[f64; 2]>,
    /// How many times it goes round in u and in v.
    winding: [i64; 2],
}

/// The points round a face's loop, each once, in the order the face runs it.
fn loop_points(lp: &Loop, forward: bool, edges: &[Vec<P3>]) -> Vec<P3> {
    let mut pts: Vec<P3> = match lp {
        Loop::Vertex(p) => vec![*p],
        Loop::Poly(ps) => ps.clone(),
        Loop::Edges(uses) => {
            let mut pts = Vec::new();
            for u in uses {
                let e = &edges[u.edge];
                if u.forward {
                    pts.extend_from_slice(&e[..e.len() - 1]);
                } else {
                    pts.extend(e[1..].iter().rev());
                }
            }
            pts
        }
    };
    if !forward {
        pts.reverse();
    }
    // Repeats (an edge of no length) go.
    pts.dedup();
    while pts.len() > 1 && pts[0] == pts[pts.len() - 1] {
        pts.pop();
    }
    pts
}

/// `x` moved by whole periods to be nearest `to`.
fn nearest(x: f64, to: f64, period: f64) -> f64 {
    x + period * ((to - x) / period).round()
}

/// A loop's points in the surface's parameters; `inv` is a B-spline surface's search, shared by
/// the face's loops.
fn map_loop(s: &Surface, inv: Option<&Inverse>, pts: &[P3]) -> Result<UvLoop, String> {
    let periods = s.periods();
    let mut uv = Vec::with_capacity(pts.len());
    let mut undefined = Vec::with_capacity(pts.len());
    let mut hint = None;
    for &p in pts {
        let (q, u) = match inv {
            Some(inv) => inv.param(p, hint),
            None => s.param(p, hint),
        };
        hint = Some(q);
        uv.push(q);
        undefined.push(u);
    }
    // Unwrap each periodic parameter along the loop, over the points where it is defined.
    let mut winding = [0i64; 2];
    for d in 0..2 {
        let Some(period) = periods[d] else { continue };
        let mut prev: Option<f64> = None;
        let mut first = None;
        for k in 0..uv.len() {
            if undefined[k] == Some(d) {
                continue;
            }
            if let Some(p) = prev {
                uv[k][d] = nearest(uv[k][d], p, period);
            }
            prev = Some(uv[k][d]);
            first.get_or_insert(uv[k][d]);
        }
        if let (Some(f), Some(l)) = (first, prev) {
            // The step from the last point back to the first is the nearest one, so the loop
            // went round as many times as the last point is periods from the first.
            winding[d] = ((l - f) / period).round() as i64;
        }
    }
    // A point where a parameter is undefined takes it from its neighbours: once from the one
    // before and once from the one after, so the loop runs along the pole between them.
    let n = uv.len();
    let mut out_xyz = Vec::with_capacity(n + 2);
    let mut out_uv = Vec::with_capacity(n + 2);
    for k in 0..n {
        let Some(d) = undefined[k] else {
            out_xyz.push(pts[k]);
            out_uv.push(uv[k]);
            continue;
        };
        let period = periods[d].unwrap_or(0.0);
        let find = |step: isize| -> Option<f64> {
            let mut j = k as isize;
            for _ in 0..n {
                j += step;
                let wrapped = j.rem_euclid(n as isize) as usize;
                if undefined[wrapped] != Some(d) {
                    // Across the loop's start the parameter is a winding away.
                    let shift = if j < 0 {
                        -(winding[d] as f64) * period
                    } else if j >= n as isize {
                        winding[d] as f64 * period
                    } else {
                        0.0
                    };
                    return Some(uv[wrapped][d] + shift);
                }
            }
            None
        };
        let (Some(before), Some(after)) = (find(-1), find(1)) else {
            return Err("every point of a loop is on the surface's pole".into());
        };
        let mut q = uv[k];
        q[d] = before;
        out_xyz.push(pts[k]);
        out_uv.push(q);
        if after != before {
            q[d] = after;
            out_xyz.push(pts[k]);
            out_uv.push(q);
        }
    }
    Ok(UvLoop {
        xyz: out_xyz,
        uv: out_uv,
        winding,
    })
}

/// Twice the signed area of a ring.
fn area(uv: &[[f64; 2]]) -> f64 {
    let n = uv.len();
    (0..n)
        .map(|k| {
            let (a, b) = (uv[k], uv[(k + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

fn mean(uv: &[[f64; 2]], d: usize) -> f64 {
    uv.iter().map(|p| p[d]).sum::<f64>() / uv.len() as f64
}

/// The triangles being made: points in the parameters and in space, and the sides that are a
/// loop's (never halved).
struct Mesher<'a> {
    s: &'a Surface,
    uv: Vec<[f64; 2]>,
    xyz: Vec<P3>,
    /// The surface's derivatives at a point the halving made, from the evaluation that placed
    /// it, for its normal.
    derivs: Vec<Option<(P3, P3)>>,
    fixed: SideMap<()>,
}

/// A triangle being refined: its corners, and for each side `k` (from corner `k` to the next)
/// whether it is known to be within the tolerance or a loop's ([`KNOWN`]) and whether it is a
/// loop's ([`FIXED`]).
#[derive(Clone, Copy)]
struct Tri {
    v: [u32; 3],
    sides: u8,
    /// Every side and the middle are within the tolerance: no round measures it again.
    done: bool,
}

const KNOWN: u8 = 0b001;
const FIXED: u8 = 0b1000;

impl Tri {
    fn new(v: [u32; 3]) -> Tri {
        Tri {
            v,
            sides: 0,
            done: false,
        }
    }

    /// A piece of `parent` whose side `to` is the parent's whole side `from`, which is within
    /// the tolerance or a loop's.
    fn piece(v: [u32; 3], parent: &Tri, from: usize, to: usize) -> Tri {
        let fixed = (parent.sides >> from) & FIXED;
        Tri {
            v,
            sides: (KNOWN << to) | (fixed << to),
            done: false,
        }
    }
}

/// The mark of a side measured within the tolerance in this round, among the halved ones.
const WITHIN: u32 = u32::MAX;

impl Mesher<'_> {
    fn push(&mut self, uv: [f64; 2], xyz: P3) -> u32 {
        self.uv.push(uv);
        self.xyz.push(xyz);
        self.derivs.push(None);
        (self.uv.len() - 1) as u32
    }

    /// Adds a ring: its points, and its sides as loop sides when `fixed`.
    fn ring(&mut self, pts: &[([f64; 2], P3, bool)]) -> Vec<u32> {
        let ids: Vec<u32> = pts.iter().map(|&(uv, xyz, _)| self.push(uv, xyz)).collect();
        let n = ids.len();
        for k in 0..n {
            if pts[k].2 {
                let (a, b) = (ids[k], ids[(k + 1) % n]);
                self.fixed.insert((a.min(b), a.max(b)), ());
            }
        }
        ids
    }

    fn mid(&self, a: u32, b: u32) -> [f64; 2] {
        let (p, q) = (self.uv[a as usize], self.uv[b as usize]);
        [0.5 * (p[0] + q[0]), 0.5 * (p[1] + q[1])]
    }

    /// Adds the surface's point at the middle of the side `a b`.
    fn push_mid(&mut self, a: u32, b: u32) -> u32 {
        let m = self.mid(a, b);
        let e = self.s.eval(m[0], m[1]);
        self.push_eval(m, e)
    }

    fn push_eval(&mut self, uv: [f64; 2], (p, su, sv): Eval) -> u32 {
        let id = self.push(uv, p);
        self.derivs[id as usize] = Some((su, sv));
        id
    }

    /// How far the middle of the side `a b` is from the surface, and the surface there.
    fn side_error(&self, a: u32, b: u32) -> (f64, [f64; 2], Eval) {
        let m = self.mid(a, b);
        let e = self.s.eval(m[0], m[1]);
        let (p, q) = (self.xyz[a as usize], self.xyz[b as usize]);
        (dist(e.0, scale(add(p, q), 0.5)), m, e)
    }

    fn centre_error(&self, t: [u32; 3]) -> f64 {
        let [a, b, c] = t.map(|i| self.uv[i as usize]);
        let m = [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0];
        let on = self.s.point(m[0], m[1]);
        let [p, q, r] = t.map(|i| self.xyz[i as usize]);
        dist(on, scale(add(add(p, q), r), 1.0 / 3.0))
    }

    /// Halves the sides that stand off the surface by more than `tol`, round after round.
    ///
    /// A side's error depends only on its ends, so each is measured once: a triangle carries
    /// which of its sides are known, and its pieces keep that for the parent's sides they keep
    /// whole. A triangle a round leaves whole is final, as every side of it and its middle are
    /// within `tol`, so each round measures only the pieces the last one cut. The halving is
    /// the same, point for point, as measuring every side of every triangle each round.
    fn refine(&mut self, out: &mut Vec<[u32; 3]>, tol: f64) -> Result<(), String> {
        let key = |a: u32, b: u32| (a.min(b), a.max(b));
        let mut tris: Vec<Tri> = out
            .iter()
            .map(|&v| {
                let mut t = Tri::new(v);
                for k in 0..3 {
                    if self.fixed.contains_key(&key(v[k], v[(k + 1) % 3])) {
                        t.sides |= (KNOWN | FIXED) << k;
                    }
                }
                t
            })
            .collect();
        for _ in 0..MAX_ROUNDS {
            // The sides measured this round: a halved one's middle, or WITHIN.
            let mut round: SideMap<u32> = SideMap::default();
            for t in tris.iter().filter(|t| !t.done) {
                for k in 0..3 {
                    if t.sides & (KNOWN << k) != 0 {
                        continue;
                    }
                    let (a, b) = (t.v[k], t.v[(k + 1) % 3]);
                    let kk = key(a, b);
                    if round.contains_key(&kk) {
                        continue;
                    }
                    let (err, m, e) = self.side_error(a, b);
                    let id = if err > tol {
                        self.push_eval(m, e)
                    } else {
                        WITHIN
                    };
                    round.insert(kk, id);
                }
            }
            let halved = |round: &SideMap<u32>, kk: &(u32, u32)| -> Option<u32> {
                round.get(kk).copied().filter(|&id| id != WITHIN)
            };
            // A triangle whose sides are all close but whose middle is not has its longest
            // free side halved.
            let mut any = round.values().any(|&id| id != WITHIN);
            for t in tris.iter().filter(|t| !t.done) {
                let sides = [0, 1, 2].map(|k| key(t.v[k], t.v[(k + 1) % 3]));
                if sides.iter().any(|s| halved(&round, s).is_some())
                    || self.centre_error(t.v) <= tol
                {
                    continue;
                }
                let longest = (0..3)
                    .filter(|&k| t.sides & (FIXED << k) == 0)
                    .map(|k| sides[k])
                    .max_by(|a, b| {
                        let la = dist(self.xyz[a.0 as usize], self.xyz[a.1 as usize]);
                        let lb = dist(self.xyz[b.0 as usize], self.xyz[b.1 as usize]);
                        la.total_cmp(&lb)
                    });
                if let Some(s) = longest {
                    let id = self.push_mid(s.0, s.1);
                    round.insert(s, id);
                    any = true;
                }
            }
            if !any {
                break;
            }
            let mut next = Vec::with_capacity(tris.len() * 2);
            for t in &tris {
                let [a, b, c] = t.v;
                let m = |p: u32, q: u32| halved(&round, &key(p, q));
                let piece = |v, from, to| Tri::piece(v, t, from, to);
                match (m(a, b), m(b, c), m(c, a)) {
                    // Left whole, it is final.
                    (None, None, None) => next.push(Tri {
                        sides: t.sides | 0b111,
                        done: true,
                        ..*t
                    }),
                    (Some(x), None, None) => {
                        next.extend([piece([a, x, c], 2, 2), piece([x, b, c], 1, 1)]);
                    }
                    (None, Some(y), None) => {
                        next.extend([piece([b, y, a], 0, 2), piece([y, c, a], 2, 1)]);
                    }
                    (None, None, Some(z)) => {
                        next.extend([piece([c, z, b], 1, 2), piece([z, a, b], 0, 1)]);
                    }
                    (Some(x), Some(y), None) => next.extend([
                        Tri::new([x, b, y]),
                        Tri::new([a, x, y]),
                        piece([a, y, c], 2, 2),
                    ]),
                    (None, Some(y), Some(z)) => next.extend([
                        Tri::new([y, c, z]),
                        Tri::new([b, y, z]),
                        piece([b, z, a], 0, 2),
                    ]),
                    (Some(x), None, Some(z)) => next.extend([
                        Tri::new([z, a, x]),
                        Tri::new([c, z, x]),
                        piece([c, x, b], 1, 2),
                    ]),
                    (Some(x), Some(y), Some(z)) => next.extend([
                        Tri::new([a, x, z]),
                        Tri::new([x, b, y]),
                        Tri::new([z, y, c]),
                        Tri::new([x, y, z]),
                    ]),
                }
            }
            tris = next;
            if tris.len() > MAX_TRIANGLES {
                return Err(format!(
                    "it needs more than {MAX_TRIANGLES} triangles to be within {CHORD_TOLERANCE} mm"
                ));
            }
        }
        *out = tris.into_iter().map(|t| t.v).collect();
        Ok(())
    }
}

/// A map keyed by a side (its two point indices, the lower first), with a cheap hash: the maps
/// are only looked in, never walked, so the hash has no effect on the triangles.
type SideMap<V> = HashMap<(u32, u32), V, BuildHasherDefault<SideHash>>;

#[derive(Default)]
struct SideHash(u64);

impl Hasher for SideHash {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(u64::from(b));
        }
    }

    fn write_u32(&mut self, x: u32) {
        self.write_u64(u64::from(x));
    }

    fn write_u64(&mut self, x: u64) {
        // FxHash's step.
        self.0 = (self.0.rotate_left(5) ^ x).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// The triangles of `face` (turned over when `reversed`, for a reversed shell), from the points
/// of the model's `edges`, within `tol` in the file's unit.
pub fn face(face: &Face, edges: &[Vec<P3>], reversed: bool, tol: f64) -> Result<FaceMesh, String> {
    let Some((mut m, mut tris, outward)) = cut(face, edges, reversed)? else {
        return Ok(FaceMesh::default());
    };
    if !matches!(face.surface, Surface::Plane(_)) {
        m.refine(&mut tris, tol)?;
    }
    Ok(finish(&m, tris, outward))
}

/// A face cut before any halving: its points, its triangles, and whether the surface's normal
/// is its outside.
type Cut<'a> = (Mesher<'a>, Vec<[u32; 3]>, bool);

/// A face's loops in its surface's parameters, cut into triangles by ear clipping before any
/// is halved, and whether the surface's normal is its outside; `None` for a face that holds no
/// area.
fn cut<'a>(face: &'a Face, edges: &[Vec<P3>], reversed: bool) -> Result<Option<Cut<'a>>, String> {
    let s = &face.surface;
    let outward = face.same_sense != reversed;
    let inv = match s {
        Surface::BSpline(b) => Some(Inverse::new(b)),
        _ => None,
    };
    let mut loops = Vec::new();
    let mut vertices = Vec::new();
    for (lp, forward) in &face.bounds {
        let pts = loop_points(lp, *forward, edges);
        if let Loop::Vertex(p) = lp {
            vertices.push(*p);
            continue;
        }
        if pts.len() < 2 {
            continue;
        }
        let mut l = map_loop(s, inv.as_ref(), &pts)?;
        // With the region on the left of every loop, seen from the surface's normal.
        if !outward {
            l.xyz.reverse();
            l.uv.reverse();
            l.winding = l.winding.map(|w| -w);
        }
        loops.push(l);
    }
    let periods = s.periods();
    let mut m = Mesher {
        s,
        uv: Vec::new(),
        xyz: Vec::new(),
        derivs: Vec::new(),
        fixed: SideMap::default(),
    };
    let ring_of = |l: &UvLoop| -> Vec<([f64; 2], P3, bool)> {
        l.uv.iter()
            .zip(&l.xyz)
            .map(|(&uv, &xyz)| (uv, xyz, true))
            .collect()
    };
    let round: Vec<usize> = (0..loops.len())
        .filter(|&k| loops[k].winding != [0, 0])
        .collect();
    let mut outer: Vec<([f64; 2], P3, bool)>;
    let mut holes: Vec<usize> = (0..loops.len()).filter(|k| !round.contains(k)).collect();
    // The direction loops go round in, if any.
    let dim = |l: &UvLoop| -> Result<usize, String> {
        match l.winding {
            [w, 0] if w.abs() == 1 => Ok(0),
            [0, w] if w.abs() == 1 => Ok(1),
            _ => Err("a loop goes round the surface more than once, or both ways".into()),
        }
    };
    match round.len() {
        0 => {
            if holes.is_empty() {
                // No loop: the whole surface, if it closes on itself.
                outer = whole(s)?;
            } else {
                let k = (0..holes.len())
                    .max_by(|&a, &b| {
                        area(&loops[holes[a]].uv)
                            .abs()
                            .total_cmp(&area(&loops[holes[b]].uv).abs())
                            .then(b.cmp(&a))
                    })
                    .unwrap_or(0);
                let o = holes.remove(k);
                outer = ring_of(&loops[o]);
            }
        }
        1 => {
            let a = &loops[round[0]];
            let d = dim(a)?;
            let pole = pole(s, d, a, vertices.first().copied())?;
            outer = seam_ring(a, d, periods[d].unwrap_or(0.0));
            let p = outer[0].0;
            let last = outer[outer.len() - 1].0;
            // Up (or down) to the pole and back along it.
            let mut at = last;
            at[1 - d] = pole.0;
            outer.push((at, pole.1, false));
            let mut at = p;
            at[1 - d] = pole.0;
            outer.push((at, pole.1, false));
        }
        2 => {
            let (a, b) = (&loops[round[0]], &loops[round[1]]);
            let d = dim(a)?;
            if dim(b)? != d {
                return Err("two loops go round the surface in different directions".into());
            }
            let period = periods[d].unwrap_or(0.0);
            outer = seam_ring(a, d, period);
            let start = outer[0].0[d];
            // The other loop backward, from the point nearest the seam.
            let mut other = seam_ring(b, d, period);
            other.pop();
            other.reverse();
            let n = other.len();
            let first = (0..n)
                .min_by(|&i, &j| {
                    let di = (nearest(other[i].0[d], start, period) - start).abs();
                    let dj = (nearest(other[j].0[d], start, period) - start).abs();
                    di.total_cmp(&dj).then(i.cmp(&j))
                })
                .unwrap_or(0);
            let mut run: Vec<([f64; 2], P3, bool)> = Vec::with_capacity(n + 1);
            for k in 0..=n {
                let (uv, xyz, _) = other[(first + k) % n];
                run.push((uv, xyz, k < n));
            }
            // Unwrapped from the far end of the seam back to the near one.
            let top = nearest(run[0].0[d], start + period, period);
            let shift0 = top - run[0].0[d];
            let mut prev = top;
            for (k, r) in run.iter_mut().enumerate() {
                if k == 0 {
                    r.0[d] += shift0;
                } else {
                    r.0[d] = nearest(r.0[d], prev, period);
                }
                prev = r.0[d];
            }
            // The side from the end of `a` across to `b` is a seam, not a loop's side.
            if let Some(last) = outer.last_mut() {
                last.2 = false;
            }
            if let Some(last) = run.last_mut() {
                last.2 = false;
            }
            outer.extend(run);
        }
        _ => return Err("more than two loops go round the surface".into()),
    }
    // The holes, moved by whole periods to lie in the outer ring.
    let outer_uv: Vec<[f64; 2]> = outer.iter().map(|p| p.0).collect();
    let mut hole_rings = Vec::new();
    for &h in &holes {
        let mut ring = ring_of(&loops[h]);
        let uv: Vec<[f64; 2]> = ring.iter().map(|p| p.0).collect();
        for (d, period) in periods.iter().enumerate() {
            if let Some(period) = *period {
                let shift = nearest(mean(&uv, d), mean(&outer_uv, d), period) - mean(&uv, d);
                for p in &mut ring {
                    p.0[d] += shift;
                }
            }
        }
        hole_rings.push(ring);
    }
    // Points meant to lie on one line of the parameters (a circle at one height of a cylinder,
    // a B-spline's own edge) come out a few units in the last place off it, and the ear
    // clipping would cut slivers from them: each parameter is rounded to a billionth of the
    // face's extent in it, so they lie on it exactly.
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in outer.iter().chain(hole_rings.iter().flatten()) {
        for d in 0..2 {
            lo[d] = lo[d].min(p.0[d]);
            hi[d] = hi[d].max(p.0[d]);
        }
    }
    let q = [0, 1].map(|d| ((hi[d] - lo[d]) * 1e-9).max(1e-300));
    let snap = |r: &mut Vec<([f64; 2], P3, bool)>| {
        for p in r.iter_mut() {
            for (x, q) in p.0.iter_mut().zip(q) {
                *x = (*x / q).round() * q;
            }
        }
    };
    snap(&mut outer);
    for r in &mut hole_rings {
        snap(r);
    }
    // A face a file bounds by one curve run there and back holds nothing.
    let ring: Vec<[f64; 2]> = outer.iter().map(|p| p.0).collect();
    let perimeter: f64 = (0..ring.len())
        .map(|k| {
            let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
            (b[0] - a[0]).hypot(b[1] - a[1])
        })
        .sum();
    if area(&ring).abs() <= 1e-9 * perimeter * perimeter {
        return Ok(None);
    }
    let outer_ids = m.ring(&outer);
    let hole_ids: Vec<Vec<u32>> = hole_rings.iter().map(|r| m.ring(r)).collect();
    let flat = |ids: &[u32], m: &Mesher| ids.iter().map(|&i| m.uv[i as usize]).collect::<Vec<_>>();
    let holes_uv: Vec<Vec<[f64; 2]>> = hole_ids.iter().map(|h| flat(h, &m)).collect();
    let all: Vec<u32> = outer_ids
        .iter()
        .chain(hole_ids.iter().flatten())
        .copied()
        .collect();
    let tris: Vec<[u32; 3]> = triangulate_keeping(&flat(&outer_ids, &m), &holes_uv)
        .into_iter()
        .map(|t| t.map(|i| all[i]))
        .collect();
    if tris.is_empty() {
        return Err("its loops hold no area".into());
    }
    Ok(Some((m, tris, outward)))
}

/// The ring of a loop that goes round in `d`: its points in the direction that goes up in `d`,
/// from its first, and that first point again a period on.
fn seam_ring(l: &UvLoop, d: usize, period: f64) -> Vec<([f64; 2], P3, bool)> {
    let mut uv = l.uv.clone();
    let mut xyz = l.xyz.clone();
    if l.winding[d] < 0 {
        uv.reverse();
        xyz.reverse();
    }
    let mut ring: Vec<([f64; 2], P3, bool)> =
        uv.iter().zip(&xyz).map(|(&u, &x)| (u, x, true)).collect();
    let (mut again, x0, _) = ring[0];
    again[d] += period;
    ring.push((again, x0, true));
    // The last side is the seam's other copy of nothing: a loop's side ends at `again`.
    if let Some(last) = ring.last_mut() {
        last.2 = false;
    }
    ring
}

/// The pole a single loop that goes round in `d` closes on: the value of the other parameter
/// there and the point. The region is on the loop's left.
fn pole(s: &Surface, d: usize, l: &UvLoop, vertex: Option<P3>) -> Result<(f64, P3), String> {
    if d != 0 {
        return Err("a single loop goes round the surface the way it has no pole".into());
    }
    let up = l.winding[0] > 0;
    let v = match s {
        Surface::Sphere { .. } => {
            if up {
                std::f64::consts::FRAC_PI_2
            } else {
                -std::f64::consts::FRAC_PI_2
            }
        }
        Surface::Cone { r, tan, .. } => {
            let apex = -r / tan;
            let side =
                l.uv.iter()
                    .all(|p| if up { p[1] <= apex } else { p[1] >= apex });
            if !side {
                return Err("a single loop goes round a cone away from its apex".into());
            }
            apex
        }
        _ => return Err("a single loop goes round a surface with no pole".into()),
    };
    // The file's own point for it when a vertex loop names it, so faces that meet there meet.
    Ok((v, vertex.unwrap_or_else(|| s.point(0.0, v))))
}

/// A surface with no loop: a whole sphere or torus.
fn whole(s: &Surface) -> Result<Vec<([f64; 2], P3, bool)>, String> {
    use std::f64::consts::{FRAC_PI_2, PI};
    let (u, v) = match s {
        Surface::Sphere { .. } => ([-PI, PI], [-FRAC_PI_2, FRAC_PI_2]),
        Surface::Torus { .. } => ([-PI, PI], [-PI, PI]),
        _ => return Err("it has no loop, and its surface does not close".into()),
    };
    // The corners, and the middles of the sides, so the first cut is not too coarse.
    let mut out = Vec::new();
    let n = 4;
    let corners = [[u[0], v[0]], [u[1], v[0]], [u[1], v[1]], [u[0], v[1]]];
    for k in 0..4 {
        let (a, b) = (corners[k], corners[(k + 1) % 4]);
        for j in 0..n {
            let f = j as f64 / n as f64;
            let p = [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f];
            out.push((p, s.point(p[0], p[1]), false));
        }
    }
    Ok(out)
}

/// The face's triangles with their normals, turned to face out; triangles with two corners at
/// one point are left out.
fn finish(m: &Mesher, tris: Vec<[u32; 3]>, outward: bool) -> FaceMesh {
    let n = m.uv.len();
    let mut normals: Vec<Option<P3>> = (0..n)
        .map(|k| {
            let [u, v] = m.uv[k];
            let (su, sv) = m.derivs[k].unwrap_or_else(|| {
                let (_, su, sv) = m.s.eval(u, v);
                (su, sv)
            });
            let c = cross(su, sv);
            let scale_ = norm(su) * norm(sv);
            if norm(c) <= 1e-12 * scale_ || scale_ == 0.0 {
                None
            } else {
                unit(if outward { c } else { scale(c, -1.0) })
            }
        })
        .collect();
    let mut kept = Vec::with_capacity(tris.len());
    for [a, b, c] in tris {
        let t = if outward { [a, b, c] } else { [a, c, b] };
        let [p, q, r] = t.map(|i| m.xyz[i as usize]);
        // Two corners at one point (a seam's two copies, a pole): no triangle. One with all
        // three on a line stays, since its sides are other triangles' too.
        if p == q || q == r || r == p {
            continue;
        }
        kept.push(t);
    }
    // A point at a pole takes the normal of the triangles round it.
    let mut sums: Vec<P3> = vec![[0.0; 3]; n];
    for t in &kept {
        let [p, q, r] = t.map(|i| m.xyz[i as usize]);
        let f = cross(sub(q, p), sub(r, p));
        for &i in t {
            if normals[i as usize].is_none() {
                sums[i as usize] = add(sums[i as usize], f);
            }
        }
    }
    for k in 0..n {
        if normals[k].is_none() {
            normals[k] = Some(unit(sums[k]).unwrap_or([0.0, 0.0, 1.0]));
        }
    }
    // Only the points a triangle uses.
    let mut remap = vec![u32::MAX; n];
    let mut out = FaceMesh::default();
    for t in &kept {
        let mut idx = [0u32; 3];
        for (j, &i) in t.iter().enumerate() {
            if remap[i as usize] == u32::MAX {
                remap[i as usize] = out.positions.len() as u32;
                out.positions.push(m.xyz[i as usize]);
                out.normals
                    .push(normals[i as usize].unwrap_or([0.0, 0.0, 1.0]));
            }
            idx[j] = remap[i as usize];
        }
        out.indices.push(idx);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::geometry::{BSplineSurface, EVALS, Knots};
    use super::super::model::{Face, Loop};
    use super::super::{fixtures, read_model};
    use super::*;

    /// The halving as it was first written: every side and every middle of every triangle
    /// measured each round, so `refine` can be held to it.
    fn refine_every_round(m: &mut Mesher, tris: &mut Vec<[u32; 3]>, tol: f64) {
        let key = |a: u32, b: u32| (a.min(b), a.max(b));
        for _ in 0..MAX_ROUNDS {
            let mut mids: HashMap<(u32, u32), u32> = HashMap::new();
            let mut checked: HashMap<(u32, u32), ()> = HashMap::new();
            for t in tris.iter() {
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    let kk = key(a, b);
                    if m.fixed.contains_key(&kk) || checked.insert(kk, ()).is_some() {
                        continue;
                    }
                    if m.side_error(a, b).0 > tol {
                        let id = m.push_mid(a, b);
                        mids.insert(kk, id);
                    }
                }
            }
            for t in tris.iter() {
                let sides = [0, 1, 2].map(|k| key(t[k], t[(k + 1) % 3]));
                if sides.iter().any(|s| mids.contains_key(s)) || m.centre_error(*t) <= tol {
                    continue;
                }
                let longest = sides
                    .iter()
                    .filter(|s| !m.fixed.contains_key(s))
                    .max_by(|a, b| {
                        let la = dist(m.xyz[a.0 as usize], m.xyz[a.1 as usize]);
                        let lb = dist(m.xyz[b.0 as usize], m.xyz[b.1 as usize]);
                        la.total_cmp(&lb)
                    })
                    .copied();
                if let Some(s) = longest {
                    let id = m.push_mid(s.0, s.1);
                    mids.insert(s, id);
                }
            }
            if mids.is_empty() {
                return;
            }
            let mut next = Vec::new();
            for &[a, b, c] in tris.iter() {
                let h = |p: u32, q: u32| mids.get(&key(p, q)).copied();
                match (h(a, b), h(b, c), h(c, a)) {
                    (None, None, None) => next.push([a, b, c]),
                    (Some(x), None, None) => next.extend([[a, x, c], [x, b, c]]),
                    (None, Some(y), None) => next.extend([[b, y, a], [y, c, a]]),
                    (None, None, Some(z)) => next.extend([[c, z, b], [z, a, b]]),
                    (Some(x), Some(y), None) => next.extend([[x, b, y], [a, x, y], [a, y, c]]),
                    (None, Some(y), Some(z)) => next.extend([[y, c, z], [b, y, z], [b, z, a]]),
                    (Some(x), None, Some(z)) => next.extend([[z, a, x], [c, z, x], [c, x, b]]),
                    (Some(x), Some(y), Some(z)) => {
                        next.extend([[a, x, z], [x, b, y], [z, y, c], [x, y, z]]);
                    }
                }
            }
            *tris = next;
        }
    }

    /// A cubic wave 10 mm long and 4 mm wide, linear across, as part models' bent leads are:
    /// 19 by 2 control points.
    fn wave() -> BSplineSurface {
        let mut knots = vec![0.0; 4];
        knots.extend((1..16).map(|k| k as f64 / 16.0));
        knots.extend([1.0; 4]);
        BSplineSurface {
            u: Knots { degree: 3, knots },
            v: Knots {
                degree: 1,
                knots: vec![0.0, 0.0, 1.0, 1.0],
            },
            points: (0..19)
                .map(|i| {
                    let (x, z) = (10.0 * i as f64 / 18.0, 0.8 * (0.9 * i as f64).sin());
                    vec![[x, 0.0, z], [x, 4.0, z]]
                })
                .collect(),
            weights: None,
        }
    }

    /// The wave bounded by its own edges, every point 0.1 µm off the surface as a file's
    /// edges are within its tolerance: too far for a point's search to stop at its
    /// neighbour's parameters.
    fn wave_face() -> Face {
        let b = wave();
        let n = 60;
        let mut pts = Vec::new();
        for (v, run) in [(0.0, 0..=n), (1.0, 0..=n)] {
            let mut us: Vec<f64> = run.map(|k| k as f64 / n as f64).collect();
            if v == 1.0 {
                us.reverse();
            }
            for u in us {
                let (p, su, sv) = b.eval(u, v);
                let normal = unit(cross(su, sv)).unwrap();
                pts.push(add(p, scale(normal, 1e-4)));
            }
        }
        Face {
            id: 1,
            line: 1,
            surface: Surface::BSpline(b),
            same_sense: true,
            bounds: vec![(Loop::Poly(pts), true)],
            colour: None,
        }
    }

    #[test]
    fn refine_halves_as_measuring_every_side_each_round_does() {
        let mut faces = vec![wave_face()];
        let mut edges = Vec::new();
        for text in [
            fixtures::patch(),
            fixtures::rational_quarter(),
            fixtures::sphere(),
            fixtures::cylinder(),
            fixtures::torus(),
            fixtures::cone(1.0, 2.0),
        ] {
            let model = read_model(text.as_bytes()).unwrap();
            let base = edges.len();
            for e in &model.edges {
                edges.push(edge_points(e, CHORD_TOLERANCE).unwrap());
            }
            for mut f in model.faces {
                for (lp, _) in &mut f.bounds {
                    if let Loop::Edges(uses) = lp {
                        for u in uses {
                            u.edge += base;
                        }
                    }
                }
                faces.push(f);
            }
        }
        let mut halved = 0;
        for f in &faces {
            let Some((mut a, mut ta, _)) = cut(f, &edges, false).unwrap() else {
                continue;
            };
            let (mut b, mut tb, _) = cut(f, &edges, false).unwrap().unwrap();
            let before = a.uv.len();
            a.refine(&mut ta, CHORD_TOLERANCE).unwrap();
            refine_every_round(&mut b, &mut tb, CHORD_TOLERANCE);
            assert_eq!(ta, tb, "face #{}", f.id);
            let bits = |m: &Mesher| -> Vec<u64> {
                m.uv.iter()
                    .flat_map(|p| p.map(f64::to_bits))
                    .chain(m.xyz.iter().flat_map(|p| p.map(f64::to_bits)))
                    .collect()
            };
            assert_eq!(bits(&a), bits(&b), "face #{}", f.id);
            halved += usize::from(a.uv.len() > before);
        }
        assert!(halved >= 8, "{halved} faces were halved");
    }

    #[test]
    fn a_b_spline_face_takes_few_evaluations_of_its_surface() {
        let f = wave_face();
        EVALS.with(|n| n.set(0));
        let m = face(&f, &[], false, CHORD_TOLERANCE).unwrap();
        let evals = EVALS.with(|n| n.get());
        let points = 2 * 61;
        let triangles = m.indices.len() as u64;
        assert!(triangles > 100, "{triangles} triangles");
        // A grid of samples made once (161 by 17), Newton's steps for each loop point, and a
        // few evaluations a triangle for the halving and the normals. A grid searched point by
        // point, as the reader first did, took over 20,000 evaluations a point.
        let bound = 161 * 17 + 70 * points + 6 * triangles;
        assert!(
            evals <= bound,
            "{evals} evaluations for {triangles} triangles, against {bound}"
        );
        // Its points on the surface, and the same bits a second time.
        let Surface::BSpline(b) = &f.surface else {
            unreachable!()
        };
        let inv = Inverse::new(b);
        for p in &m.positions {
            let (uv, _) = inv.param(*p, None);
            assert!(dist(b.eval(uv[0], uv[1]).0, *p) < 2e-4, "{p:?}");
        }
        let again = face(&f, &[], false, CHORD_TOLERANCE).unwrap();
        assert_eq!(m, again);
        let tbits = |m: &FaceMesh| -> Vec<u64> {
            m.positions
                .iter()
                .chain(&m.normals)
                .flat_map(|p| p.map(f64::to_bits))
                .collect()
        };
        assert_eq!(tbits(&m), tbits(&again));
    }
}
