//! Cutting an opening out of the board edge: the outline with a notch (issue #90).
//!
//! The outline and the opening are loops of lines and arcs. Both are cut at every point where an
//! edge of one meets an edge of the other; the pieces of the outline that are outside the opening
//! and the pieces of the opening that are inside the outline, the latter turned round, make the
//! new edge. It is one loop when the opening takes a bite out of the board, and several when it
//! cuts the board in two (or leaves an island): that is refused, because a board has one outline.

use std::f64::consts::TAU;

use crate::chain::{ring, shoelace};
use crate::geom::{ArcGeom, P, dist, inside, segment_distance};
use crate::{Polygon, Vertex};

/// Why an opening cannot be cut out of the outline.
pub(crate) enum Refused {
    /// An edge of the opening lies along an edge of the outline.
    Along,
    /// The opening cuts the board into this many pieces.
    Pieces(usize),
    /// The opening has no edge inside the outline, or the outline no edge outside the opening.
    NoCut,
    /// The pieces of the new edge do not join up.
    Open,
}

/// One edge, with the circle it lies on if it is an arc.
#[derive(Clone, Copy)]
struct Edge {
    a: P,
    b: P,
    bulge: f64,
    arc: Option<ArcGeom>,
}

impl Edge {
    fn new(a: P, b: P, bulge: f64) -> Edge {
        Edge {
            a,
            b,
            bulge,
            arc: ArcGeom::of(a, b, bulge),
        }
    }

    /// The point `t` of the way along it, 0 to 1.
    fn at(&self, t: f64) -> P {
        match self.arc {
            Some(arc) => arc.point_at(t),
            None => (
                self.a.0 + (self.b.0 - self.a.0) * t,
                self.a.1 + (self.b.1 - self.a.1) * t,
            ),
        }
    }

    /// How far along it the point `p`, which is on its circle, is; `None` past either end.
    fn arc_t(arc: &ArcGeom, p: P) -> Option<f64> {
        let angle = (p.1 - arc.centre.1).atan2(p.0 - arc.centre.0);
        let mut d = if arc.sweep >= 0.0 {
            (angle - arc.start).rem_euclid(TAU)
        } else {
            (arc.start - angle).rem_euclid(TAU)
        };
        if d > TAU - 1e-9 {
            d = 0.0;
        }
        let t = d / arc.sweep.abs();
        (t <= 1.0 + T_EPS).then_some(t.min(1.0))
    }

    /// The piece from `t0` to `t1` (each 0 to 1).
    fn piece(&self, t0: f64, t1: f64) -> Edge {
        let a = if t0 <= 0.0 { self.a } else { self.at(t0) };
        let b = if t1 >= 1.0 { self.b } else { self.at(t1) };
        let bulge = match self.arc {
            Some(arc) => (arc.sweep * (t1 - t0) / 4.0).tan(),
            None => 0.0,
        };
        Edge::new(a, b, bulge)
    }

    fn reversed(&self) -> Edge {
        Edge::new(self.b, self.a, -self.bulge)
    }

    fn length(&self) -> f64 {
        match self.arc {
            Some(arc) => arc.length(),
            None => dist(self.a, self.b),
        }
    }
}

/// The slack in a position along an edge (a fraction of its length) for a point at its end.
const T_EPS: f64 = 1e-9;
/// Two meeting points closer than this (mm) are one.
const SAME: f64 = 1e-5;

fn edges(poly: &Polygon) -> Vec<Edge> {
    poly.edges()
        .map(|(a, b, bulge)| Edge::new(a, b, bulge))
        .collect()
}

/// Where `e` and `f` meet, as the position along each. `Err` if they lie along each other.
fn meet(e: &Edge, f: &Edge) -> Result<Vec<(f64, f64)>, Refused> {
    match (e.arc, f.arc) {
        (None, None) => {
            let (d1, d2) = (
                (e.b.0 - e.a.0, e.b.1 - e.a.1),
                (f.b.0 - f.a.0, f.b.1 - f.a.1),
            );
            let denom = d1.0 * d2.1 - d1.1 * d2.0;
            let scale = d1.0.hypot(d1.1) * d2.0.hypot(d2.1);
            let (ox, oy) = (f.a.0 - e.a.0, f.a.1 - e.a.1);
            if denom.abs() <= 1e-12 * scale {
                // Parallel: along each other when on one line, and then they overlap or not.
                let off = (ox * d1.1 - oy * d1.0).abs() / d1.0.hypot(d1.1);
                if off > SAME {
                    return Ok(Vec::new());
                }
                let along = |p: P| {
                    ((p.0 - e.a.0) * d1.0 + (p.1 - e.a.1) * d1.1) / (d1.0 * d1.0 + d1.1 * d1.1)
                };
                let (lo, hi) = (along(f.a).min(along(f.b)), along(f.a).max(along(f.b)));
                let slack = SAME / d1.0.hypot(d1.1);
                // End to end on one line shares a corner of both, and no more.
                return if hi > slack && lo < 1.0 - slack {
                    Err(Refused::Along)
                } else {
                    Ok(Vec::new())
                };
            }
            let te = (ox * d2.1 - oy * d2.0) / denom;
            let tf = (ox * d1.1 - oy * d1.0) / denom;
            let ok = |t: f64| (-T_EPS..=1.0 + T_EPS).contains(&t);
            Ok(if ok(te) && ok(tf) {
                vec![(te.clamp(0.0, 1.0), tf.clamp(0.0, 1.0))]
            } else {
                Vec::new()
            })
        }
        (Some(arc), None) | (None, Some(arc)) => {
            let (line, swap) = if e.arc.is_none() {
                (e, false)
            } else {
                (f, true)
            };
            let d = (line.b.0 - line.a.0, line.b.1 - line.a.1);
            let o = (line.a.0 - arc.centre.0, line.a.1 - arc.centre.1);
            let (qa, qb, qc) = (
                d.0 * d.0 + d.1 * d.1,
                2.0 * (d.0 * o.0 + d.1 * o.1),
                o.0 * o.0 + o.1 * o.1 - arc.radius * arc.radius,
            );
            let disc = qb * qb - 4.0 * qa * qc;
            if disc < 0.0 {
                return Ok(Vec::new());
            }
            let root = disc.sqrt();
            let mut out = Vec::new();
            for sign in [-1.0, 1.0] {
                let t = (-qb + sign * root) / (2.0 * qa);
                if !(-T_EPS..=1.0 + T_EPS).contains(&t) {
                    continue;
                }
                let p = line.at(t.clamp(0.0, 1.0));
                if let Some(s) = Edge::arc_t(&arc, p) {
                    let pair = (t.clamp(0.0, 1.0), s);
                    out.push(if swap { (pair.1, pair.0) } else { pair });
                }
                if root == 0.0 {
                    break;
                }
            }
            Ok(out)
        }
        (Some(c1), Some(c2)) => {
            let d = dist(c1.centre, c2.centre);
            if d < 1e-9 {
                // Concentric: on one circle they overlap where their angles do.
                if (c1.radius - c2.radius).abs() > SAME {
                    return Ok(Vec::new());
                }
                let overlap = (0..=16).any(|k| {
                    let p = f.at(f64::from(k) / 16.0);
                    let inside_e = Edge::arc_t(&c1, p).is_some();
                    let ends = dist(p, e.a) <= SAME || dist(p, e.b) <= SAME;
                    inside_e && !ends
                });
                return if overlap {
                    Err(Refused::Along)
                } else {
                    Ok(Vec::new())
                };
            }
            if d > c1.radius + c2.radius || d < (c1.radius - c2.radius).abs() {
                return Ok(Vec::new());
            }
            let u = (
                (c2.centre.0 - c1.centre.0) / d,
                (c2.centre.1 - c1.centre.1) / d,
            );
            let a = (c1.radius * c1.radius - c2.radius * c2.radius + d * d) / (2.0 * d);
            let h = (c1.radius * c1.radius - a * a).max(0.0).sqrt();
            let mid = (c1.centre.0 + a * u.0, c1.centre.1 + a * u.1);
            let mut out = Vec::new();
            for sign in [-1.0, 1.0] {
                let p = (mid.0 - sign * h * u.1, mid.1 + sign * h * u.0);
                if let (Some(s), Some(t)) = (Edge::arc_t(&c1, p), Edge::arc_t(&c2, p)) {
                    out.push((s, t));
                }
                if h == 0.0 {
                    break;
                }
            }
            Ok(out)
        }
    }
}

/// `edges` cut at the given positions (0 to 1 along each).
fn cut(edges: &[Edge], at: &[Vec<f64>]) -> Vec<Edge> {
    let mut out = Vec::new();
    for (e, ts) in edges.iter().zip(at) {
        let mut ts: Vec<f64> = ts
            .iter()
            .copied()
            .filter(|&t| t > T_EPS && t < 1.0 - T_EPS)
            .collect();
        ts.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        let mut kept: Vec<f64> = Vec::new();
        for t in ts {
            if kept
                .last()
                .is_none_or(|&last| dist(e.at(last), e.at(t)) > SAME)
            {
                kept.push(t);
            }
        }
        let mut from = 0.0;
        for t in kept.into_iter().chain([1.0]) {
            out.push(e.piece(from, t));
            from = t;
        }
    }
    out
}

fn counterclockwise(poly: &Polygon) -> Polygon {
    if shoelace(&poly.vertices) >= 0.0 {
        return poly.clone();
    }
    let n = poly.vertices.len();
    let mut vertices: Vec<Vertex> = poly.vertices.iter().rev().copied().collect();
    for (i, v) in vertices.iter_mut().enumerate() {
        // The vertex now at i was at n-1-i; its edge to the next is the edge from the vertex
        // before it in the old order, run the other way.
        v.bulge = -poly.vertices[(2 * n - 2 - i) % n].bulge;
    }
    Polygon { vertices }
}

/// Whether the loop `inner` lies in `outer` (its first point does; the loops do not cross).
pub(crate) fn lies_in(inner: &Polygon, outer: &Polygon) -> bool {
    let r = ring(outer);
    let v = &inner.vertices[0];
    inside((v.x, v.y), &r)
}

/// The outline `outer` with `opening` cut out of it. Both are loops of the drawing (any
/// direction); the result runs counterclockwise (y up). `tol` is the distance within which two
/// edge ends are one point.
pub(crate) fn notch(outer: &Polygon, opening: &Polygon, tol: f64) -> Result<Polygon, Refused> {
    let (outer, opening) = (counterclockwise(outer), counterclockwise(opening));
    let (oe, pe) = (edges(&outer), edges(&opening));
    let mut at_o = vec![Vec::new(); oe.len()];
    let mut at_p = vec![Vec::new(); pe.len()];
    for (i, e) in oe.iter().enumerate() {
        for (j, f) in pe.iter().enumerate() {
            for (te, tf) in meet(e, f)? {
                at_o[i].push(te);
                at_p[j].push(tf);
            }
        }
    }
    let (ring_o, ring_p) = (ring(&outer), ring(&opening));
    let near = |p: P, r: &[P]| {
        (0..r.len())
            .map(|i| segment_distance(p, p, r[i], r[(i + 1) % r.len()]))
            .fold(f64::INFINITY, f64::min)
    };
    // The pieces that stay: the outline outside the opening, and the opening inside the outline
    // run the other way round.
    let mut kept: Vec<Edge> = Vec::new();
    let mut dropped = 0;
    for piece in cut(&oe, &at_o) {
        if piece.length() <= tol {
            continue;
        }
        let mid = piece.at(0.5);
        if piece.length() > 0.01 && near(mid, &ring_p) < 1e-3 {
            return Err(Refused::Along);
        }
        if inside(mid, &ring_p) {
            dropped += 1;
        } else {
            kept.push(piece);
        }
    }
    let outer_kept = kept.len();
    for piece in cut(&pe, &at_p) {
        if piece.length() <= tol {
            continue;
        }
        let mid = piece.at(0.5);
        if piece.length() > 0.01 && near(mid, &ring_o) < 1e-3 {
            return Err(Refused::Along);
        }
        if inside(mid, &ring_o) {
            kept.push(piece.reversed());
        }
    }
    if dropped == 0 || kept.len() == outer_kept || outer_kept == 0 {
        return Err(Refused::NoCut);
    }

    // Join the pieces into loops. Their ends meet to within a few tolerances: a piece shorter
    // than the tolerance was dropped.
    let join = 5.0 * tol;
    let mut used = vec![false; kept.len()];
    let mut loops = 0;
    let mut result: Vec<Edge> = Vec::new();
    for start in 0..kept.len() {
        if used[start] {
            continue;
        }
        loops += 1;
        let first = kept[start].a;
        let mut current = start;
        loop {
            used[current] = true;
            let e = kept[current];
            if loops == 1 {
                result.push(e);
            }
            if dist(e.b, first) <= join {
                break;
            }
            let next = (0..kept.len())
                .filter(|&k| !used[k])
                .map(|k| (k, dist(kept[k].a, e.b)))
                .filter(|&(_, d)| d <= join)
                .min_by(|a, b| a.1.partial_cmp(&b.1).expect("finite"));
            match next {
                Some((k, _)) => current = k,
                None => return Err(Refused::Open),
            }
        }
    }
    if loops != 1 {
        return Err(Refused::Pieces(loops));
    }
    let result = merge_arcs(result);
    Ok(Polygon {
        vertices: result
            .iter()
            .map(|e| Vertex {
                x: e.a.0,
                y: e.a.1,
                bulge: e.bulge,
            })
            .collect(),
    })
}

/// Two arcs in a row on one circle and turning the same way are one arc: a circle drawn as two
/// halves, cut where the outline meets it, is one arc of the notch.
fn merge_arcs(mut edges: Vec<Edge>) -> Vec<Edge> {
    let same_circle = |e: &Edge, f: &Edge| match (e.arc, f.arc) {
        (Some(c), Some(d)) => {
            dist(c.centre, d.centre) <= SAME
                && (c.radius - d.radius).abs() <= SAME
                && c.sweep * d.sweep > 0.0
                && (c.sweep + d.sweep).abs() < TAU - 1e-6
        }
        _ => false,
    };
    loop {
        let n = edges.len();
        let Some(i) = (0..n).find(|&i| n > 2 && same_circle(&edges[i], &edges[(i + 1) % n])) else {
            return edges;
        };
        let j = (i + 1) % n;
        let (e, f) = (edges[i], edges[j]);
        let sweep = e.arc.expect("an arc").sweep + f.arc.expect("an arc").sweep;
        edges[i] = Edge::new(e.a, f.b, (sweep / 4.0).tan());
        edges.remove(j);
    }
}
