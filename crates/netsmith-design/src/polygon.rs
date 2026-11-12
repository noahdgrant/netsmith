//! A `polygon` outline as the design states it (issue #57): the corners of the statement, with
//! each `round R` turned into the two ends of a tangent arc, as the board edge is drawn.
//!
//! The rules, each with its message: a fillet is tangent to the edges on both sides of its
//! corner, straight or arcs (the owner's example on issue #57 rounds the corner after an
//! `arc through`); it needs a real corner (not one edge running on into the next), and room on
//! both edges (the fillets at the two ends of an edge must not meet); the box of the edge
//! has its top left at the origin of the board's frame, as a rectangular outline's has; the edge
//! does not cross itself (a crossing found here names the two edges; the board stage measures
//! arcs exactly as well). `.width`, `.height` and `.corner_radius` beside a polygon are refused
//! by the caller, since the polygon says all three.

use netsmith_check::PolygonCorner;
use netsmith_core::OutlineVertex;

type P = (f64, f64);

fn sub(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}

fn len(a: P) -> f64 {
    a.0.hypot(a.1)
}

fn unit(a: P) -> P {
    let l = len(a);
    (a.0 / l, a.1 / l)
}

fn at(p: P, d: P, k: f64) -> P {
    (p.0 + d.0 * k, p.1 + d.1 * k)
}

/// A number as a message prints it: at most four decimals, no trailing zeros.
fn num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn corner_text(p: P) -> String {
    format!("({}mm, {}mm)", num(p.0), num(p.1))
}

/// The circle through three points, or `None` if they are on a line.
fn circle(a: P, b: P, c: P) -> Option<(P, f64)> {
    let d = 2.0 * (a.0 * (b.1 - c.1) + b.0 * (c.1 - a.1) + c.0 * (a.1 - b.1));
    if d.abs() < 1e-12 {
        return None;
    }
    let (a2, b2, c2) = (
        a.0 * a.0 + a.1 * a.1,
        b.0 * b.0 + b.1 * b.1,
        c.0 * c.0 + c.1 * c.1,
    );
    let centre = (
        (a2 * (b.1 - c.1) + b2 * (c.1 - a.1) + c2 * (a.1 - b.1)) / d,
        (a2 * (c.0 - b.0) + b2 * (a.0 - c.0) + c2 * (b.0 - a.0)) / d,
    );
    Some((centre, len(sub(a, centre))))
}

/// An arc from `a` through `m` to `b` as its circle, start angle and signed sweep (radians).
fn sweep(a: P, m: P, b: P) -> Option<(P, f64, f64, f64)> {
    let (c, r) = circle(a, m, b)?;
    let tau = std::f64::consts::TAU;
    let angle = |p: P| (p.1 - c.1).atan2(p.0 - c.0);
    let (a0, am, a1) = (angle(a), angle(m), angle(b));
    let forward = (a1 - a0).rem_euclid(tau);
    let by = if (am - a0).rem_euclid(tau) <= forward {
        forward
    } else {
        forward - tau
    };
    Some((c, r, a0, by))
}

/// The points of an arc from `a` through `m` to `b`, a degree apart, for the crossing test.
fn arc_points(a: P, m: P, b: P) -> Vec<P> {
    let Some((c, r, a0, by)) = sweep(a, m, b) else {
        return vec![a, m, b];
    };
    let n = ((by.abs().to_degrees()).ceil() as usize).max(2);
    (0..=n)
        .map(|k| {
            if k == n {
                return b;
            }
            let t = a0 + by * k as f64 / n as f64;
            (c.0 + r * t.cos(), c.1 + r * t.sin())
        })
        .collect()
}

/// The least and greatest corners of the edge's box, exact for arcs.
fn bounds(vertices: &[OutlineVertex]) -> (P, P) {
    let m = vertices.len();
    let mut lo = (f64::INFINITY, f64::INFINITY);
    let mut hi = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut take = |p: P| {
        lo = (lo.0.min(p.0), lo.1.min(p.1));
        hi = (hi.0.max(p.0), hi.1.max(p.1));
    };
    for (k, v) in vertices.iter().enumerate() {
        take(v.at);
        let b = vertices[(k + 1) % m].at;
        let Some(mid) = v.through else { continue };
        let Some((c, r, a0, by)) = sweep(v.at, mid, b) else {
            continue;
        };
        let tau = std::f64::consts::TAU;
        for q in 0..4 {
            let e = std::f64::consts::FRAC_PI_2 * f64::from(q);
            let passes = if by >= 0.0 {
                (e - a0).rem_euclid(tau) <= by
            } else {
                (a0 - e).rem_euclid(tau) <= -by
            };
            if passes {
                take((c.0 + r * e.cos(), c.1 + r * e.sin()));
            }
        }
    }
    let r = |v: f64| (v * 1e6).round() / 1e6;
    ((r(lo.0), r(lo.1)), (r(hi.0), r(hi.1)))
}

fn cross(o: P, a: P, b: P) -> f64 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

fn proper_cross(a: (P, P), b: (P, P)) -> bool {
    let (d1, d2) = (cross(b.0, b.1, a.0), cross(b.0, b.1, a.1));
    let (d3, d4) = (cross(a.0, a.1, b.0), cross(a.0, a.1, b.1));
    ((d1 > 1e-12 && d2 < -1e-12) || (d1 < -1e-12 && d2 > 1e-12))
        && ((d3 > 1e-12 && d4 < -1e-12) || (d3 < -1e-12 && d4 > 1e-12))
}

/// One edge of the polygon, as a line or as an arc of a circle, with a parameter `t` from 0 at
/// its start to 1 at its end.
#[derive(Clone, Copy)]
enum Geom {
    Line {
        a: P,
        b: P,
    },
    /// The circle, the angle at the start and the signed sweep (radians, y down).
    Arc {
        c: P,
        r: f64,
        a0: f64,
        by: f64,
    },
}

impl Geom {
    fn of(a: P, through: Option<P>, b: P) -> Geom {
        match through.and_then(|m| sweep(a, m, b)) {
            Some((c, r, a0, by)) => Geom::Arc { c, r, a0, by },
            None => Geom::Line { a, b },
        }
    }

    fn point(&self, t: f64) -> P {
        match *self {
            Geom::Line { a, b } => at(a, sub(b, a), t),
            Geom::Arc { c, r, a0, by } => {
                let q = a0 + by * t;
                (c.0 + r * q.cos(), c.1 + r * q.sin())
            }
        }
    }

    /// The direction of travel at `t`.
    fn heading(&self, t: f64) -> P {
        match *self {
            Geom::Line { a, b } => unit(sub(b, a)),
            Geom::Arc { a0, by, .. } => {
                let q = a0 + by * t;
                let s = by.signum();
                (-q.sin() * s, q.cos() * s)
            }
        }
    }

    /// The parameter of the point of the edge's line or circle nearest `p`, not clamped.
    fn foot(&self, p: P) -> f64 {
        match *self {
            Geom::Line { a, b } => {
                let d = sub(b, a);
                let l2 = d.0 * d.0 + d.1 * d.1;
                ((p.0 - a.0) * d.0 + (p.1 - a.1) * d.1) / l2
            }
            Geom::Arc { c, a0, by, .. } => {
                let tau = std::f64::consts::TAU;
                let q = (p.1 - c.1).atan2(p.0 - c.0);
                // The angle from the start in the sweep's direction, then the other way.
                let ahead = if by >= 0.0 {
                    (q - a0).rem_euclid(tau)
                } else {
                    (a0 - q).rem_euclid(tau)
                };
                let t = ahead / by.abs();
                // An angle just behind the start is a small negative t, not a large one.
                if t > 1.0 && (tau - ahead) / by.abs() < t - 1.0 {
                    -(tau - ahead) / by.abs()
                } else {
                    t
                }
            }
        }
    }

    /// The curves at distance `d` from the edge's line or circle (both sides).
    fn offsets(&self, d: f64) -> Vec<Curve> {
        match *self {
            Geom::Line { a, b } => {
                let u = unit(sub(b, a));
                let n = (-u.1, u.0);
                vec![Curve::Line(at(a, n, d), u), Curve::Line(at(a, n, -d), u)]
            }
            Geom::Arc { c, r, .. } => {
                let mut out = vec![Curve::Circle(c, r + d)];
                if r - d > 1e-9 {
                    out.push(Curve::Circle(c, r - d));
                }
                out
            }
        }
    }
}

/// A line through a point along a unit direction, or a circle.
#[derive(Clone, Copy)]
enum Curve {
    Line(P, P),
    Circle(P, f64),
}

/// Where two curves meet.
fn meet(a: Curve, b: Curve) -> Vec<P> {
    match (a, b) {
        (Curve::Line(p, u), Curve::Line(q, v)) => {
            let det = u.0 * (-v.1) - u.1 * (-v.0);
            if det.abs() < 1e-12 {
                return Vec::new();
            }
            let w = sub(q, p);
            let t = (w.0 * (-v.1) - w.1 * (-v.0)) / det;
            vec![at(p, u, t)]
        }
        (Curve::Line(p, u), Curve::Circle(c, r)) | (Curve::Circle(c, r), Curve::Line(p, u)) => {
            let w = sub(p, c);
            let bq = w.0 * u.0 + w.1 * u.1;
            let cq = w.0 * w.0 + w.1 * w.1 - r * r;
            let disc = bq * bq - cq;
            if disc < 0.0 {
                return Vec::new();
            }
            let s = disc.sqrt();
            vec![at(p, u, -bq - s), at(p, u, -bq + s)]
        }
        (Curve::Circle(c1, r1), Curve::Circle(c2, r2)) => {
            let d = len(sub(c2, c1));
            if d < 1e-12 || d > r1 + r2 || d < (r1 - r2).abs() {
                return Vec::new();
            }
            let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
            let h = (r1 * r1 - a * a).max(0.0).sqrt();
            let u = unit(sub(c2, c1));
            let m = at(c1, u, a);
            vec![at(m, (-u.1, u.0), h), at(m, (u.1, -u.0), h)]
        }
    }
}

/// A fillet of radius `r` at the corner between `into` (which ends there) and `out` (which
/// begins there): the parameters on the two edges where it meets them, and its middle.
fn fillet(into: &Geom, out: &Geom, corner: P, r: f64) -> Option<(f64, f64, P)> {
    let u = (-into.heading(1.0).0, -into.heading(1.0).1);
    let v = out.heading(0.0);
    let cos = (u.0 * v.0 + u.1 * v.1).clamp(-1.0, 1.0);
    let half = cos.acos() / 2.0;
    if half.sin() < 1e-6 || (std::f64::consts::FRAC_PI_2 - half).abs() < 1e-6 {
        return None;
    }
    let guess = at(corner, unit((u.0 + v.0, u.1 + v.1)), r / half.sin());
    let mut best: Option<(f64, P)> = None;
    for a in into.offsets(r) {
        for b in out.offsets(r) {
            for p in meet(a, b) {
                let d = len(sub(p, guess));
                if best.is_none_or(|(e, _)| d < e) {
                    best = Some((d, p));
                }
            }
        }
    }
    let (_, centre) = best?;
    let (t1, t2) = (into.foot(centre), out.foot(centre));
    let (p1, p2) = (into.point(t1), out.point(t2));
    let mid = unit((p1.0 + p2.0 - 2.0 * centre.0, p1.1 + p2.1 - 2.0 * centre.1));
    Some((t1, t2, at(centre, mid, r)))
}

/// The corners of the board edge for `corners`, or every reason it is not one (each message
/// starts with `path`, the outline's).
pub(crate) fn vertices(
    path: &str,
    corners: &[PolygonCorner],
) -> Result<Vec<OutlineVertex>, Vec<String>> {
    let n = corners.len();
    let mut errors = Vec::new();
    let geoms: Vec<Geom> = (0..n)
        .map(|k| Geom::of(corners[k].at, corners[k].through, corners[(k + 1) % n].at))
        .collect();
    // Where each edge begins and ends after the fillets, and each fillet's middle.
    let mut starts = vec![0.0; n];
    let mut ends = vec![1.0; n];
    let mut middles: Vec<Option<P>> = vec![None; n];
    for (k, c) in corners.iter().enumerate() {
        let Some(r) = c.round else { continue };
        let before = (k + n - 1) % n;
        match fillet(&geoms[before], &geoms[k], c.at, r) {
            Some((t1, t2, mid)) => {
                ends[before] = t1;
                starts[k] = t2;
                middles[k] = Some(mid);
            }
            None => errors.push(format!(
                "`{path}`: the corner {} with `round {}mm` has its edges running on into each other, so there is no corner to round",
                corner_text(c.at),
                num(r)
            )),
        }
    }
    for k in 0..n {
        let j = (k + 1) % n;
        if starts[k] < ends[k] - 1e-9 && starts[k] >= -1e-9 && ends[k] <= 1.0 + 1e-9 {
            continue;
        }
        let (a, b) = (corner_text(corners[k].at), corner_text(corners[j].at));
        let rounds: Vec<String> = [k, j]
            .into_iter()
            .filter(|&i| middles[i].is_some())
            .map(|i| {
                let taken = match geoms[k] {
                    Geom::Line { a, b } => {
                        let t = if i == k { starts[k] } else { 1.0 - ends[k] };
                        format!(" takes {} mm of it", num(t * len(sub(b, a))))
                    }
                    Geom::Arc { .. } => String::new(),
                };
                format!(
                    "`round {}mm` at {}{taken}",
                    num(corners[i].round.unwrap_or_default()),
                    corner_text(corners[i].at)
                )
            })
            .collect();
        let what = match geoms[k] {
            Geom::Line { a, b } => format!(
                "the edge from {a_} to {b_} is {} mm long",
                num(len(sub(b, a))),
                a_ = corner_text(a),
                b_ = corner_text(b)
            ),
            Geom::Arc { .. } => format!("the arc from {a} to {b}"),
        };
        errors.push(format!(
            "`{path}`: {what}, too short for its fillets: {}",
            rounds.join(" and ")
        ));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut out = Vec::new();
    for k in 0..n {
        let start = geoms[k].point(starts[k]);
        if let Some(mid) = middles[k] {
            let before = (k + n - 1) % n;
            out.push(OutlineVertex {
                at: geoms[before].point(ends[before]),
                through: Some(mid),
            });
        }
        let through = match geoms[k] {
            Geom::Arc { .. } => Some(geoms[k].point((starts[k] + ends[k]) / 2.0)),
            Geom::Line { .. } => None,
        };
        out.push(OutlineVertex {
            at: if middles[k].is_some() {
                start
            } else {
                corners[k].at
            },
            through,
        });
    }
    // The edge as short pieces: for the box and for crossings.
    let m = out.len();
    let mut pieces: Vec<(P, P, usize)> = Vec::new();
    for (k, v) in out.iter().enumerate() {
        let b = out[(k + 1) % m].at;
        let points = match v.through {
            Some(mid) => arc_points(v.at, mid, b),
            None => vec![v.at, b],
        };
        pieces.extend(points.windows(2).map(|w| (w[0], w[1], k)));
    }
    let (lo, _) = bounds(&out);
    if lo.0.abs() > 1e-6 || lo.1.abs() > 1e-6 {
        errors.push(format!(
            "`{path}`: the polygon's leftmost point is at x = {}mm and its topmost at y = {}mm; the board's frame has its origin at the outline's top left, so move the corners to start at 0mm",
            num(lo.0),
            num(lo.1)
        ));
    }
    let count = pieces.len();
    'outer: for i in 0..count {
        for j in i + 1..count {
            let (a, b) = (pieces[i], pieces[j]);
            if a.2 == b.2 {
                continue;
            }
            if proper_cross((a.0, a.1), (b.0, b.1)) {
                errors.push(format!(
                    "`{path}`: the polygon's edge from {} crosses its edge from {}; a board edge goes round once",
                    corner_text(out[a.2].at),
                    corner_text(out[b.2].at)
                ));
                break 'outer;
            }
        }
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

/// The box of the edge: its width and height, the extremes of arcs included.
pub(crate) fn size(vertices: &[OutlineVertex]) -> (f64, f64) {
    let (lo, hi) = bounds(vertices);
    let r = |v: f64| (v * 1e6).round() / 1e6;
    (r(hi.0 - lo.0), r(hi.1 - lo.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(x: f64, y: f64) -> PolygonCorner {
        PolygonCorner {
            at: (x, y),
            round: None,
            through: None,
        }
    }

    fn near(a: P, b: P) -> bool {
        len(sub(a, b)) < 1e-6
    }

    /// The owner's example on issue #57: an L with a fillet at the top right, a bulge on the
    /// bottom edge and a fillet after it at the bottom left.
    fn example() -> Vec<PolygonCorner> {
        vec![
            c(0.0, 0.0),
            PolygonCorner {
                round: Some(2.0),
                ..c(40.0, 0.0)
            },
            c(40.0, 15.0),
            c(25.0, 15.0),
            PolygonCorner {
                through: Some((12.0, 34.0)),
                ..c(25.0, 30.0)
            },
            PolygonCorner {
                round: Some(3.0),
                ..c(0.0, 30.0)
            },
        ]
    }

    #[test]
    fn a_square_corner_is_cut_by_a_quarter_circle_tangent_to_both_edges() {
        let v = vertices("outline", &example()).unwrap();
        // The corner (40, 0) is two vertices: the fillet from (38, 0) to (40, 2).
        assert!(near(v[1].at, (38.0, 0.0)), "{:?}", v[1]);
        let mid = v[1].through.unwrap();
        assert!((len(sub(mid, (38.0, 2.0))) - 2.0).abs() < 1e-9, "{mid:?}");
        assert!(near(v[2].at, (40.0, 2.0)) && v[2].through.is_none());
    }

    #[test]
    fn a_fillet_after_an_arc_is_tangent_to_the_arc() {
        let v = vertices("outline", &example()).unwrap();
        // The arc from (25, 30) through (12, 34) is cut short where the fillet at (0, 30)
        // begins; the fillet ends on the left edge, x = 0.
        let n = v.len();
        let (arc, fillet, after) = (&v[n - 3], &v[n - 2], &v[n - 1]);
        assert!(near(arc.at, (25.0, 30.0)));
        let ((cx, cy), r) = circle((25.0, 30.0), (12.0, 34.0), (0.0, 30.0)).unwrap();
        let on_circle = |p: P| (len(sub(p, (cx, cy))) - r).abs() < 1e-6;
        assert!(on_circle(arc.through.unwrap()) && on_circle(fillet.at));
        assert!(after.at.0.abs() < 1e-9 && after.at.1 < 30.0, "{after:?}");
        // The fillet's middle is 3 mm from a centre 3 mm from both: the left edge and the arc.
        let f = circle(fillet.at, fillet.through.unwrap(), after.at).unwrap();
        assert!((f.1 - 3.0).abs() < 1e-6, "{f:?}");
        assert!((f.0.0 - 3.0).abs() < 1e-6, "3 mm in from x = 0: {f:?}");
        assert!(
            (len(sub(f.0, (cx, cy))) - (r - 3.0)).abs() < 1e-6,
            "inside the arc"
        );
    }

    #[test]
    fn the_box_takes_the_bulge_of_an_arc() {
        let v = vertices("outline", &example()).unwrap();
        let (w, h) = size(&v);
        assert_eq!(w, 40.0);
        // The arc through (12, 34) from (25, 30) to (0, 30) reaches below 34.
        let ((cx, cy), r) = circle((25.0, 30.0), (12.0, 34.0), (0.0, 30.0)).unwrap();
        assert!((h - (cy + r)).abs() < 1e-6 && h > 34.0, "{h} {}", cy + r);
        let _ = cx;
    }

    #[test]
    fn a_polygon_away_from_the_origin_or_crossing_itself_is_refused() {
        let moved: Vec<PolygonCorner> = [(5.0, 2.0), (20.0, 2.0), (20.0, 12.0)]
            .map(|(x, y)| c(x, y))
            .into();
        assert_eq!(
            vertices("outline", &moved).unwrap_err(),
            [
                "`outline`: the polygon's leftmost point is at x = 5mm and its topmost at y = 2mm; the board's frame has its origin at the outline's top left, so move the corners to start at 0mm"
            ]
        );
        let bow: Vec<PolygonCorner> = [(0.0, 0.0), (10.0, 10.0), (10.0, 0.0), (0.0, 10.0)]
            .map(|(x, y)| c(x, y))
            .into();
        assert_eq!(
            vertices("outline", &bow).unwrap_err(),
            [
                "`outline`: the polygon's edge from (0mm, 0mm) crosses its edge from (10mm, 0mm); a board edge goes round once"
            ]
        );
    }

    #[test]
    fn a_round_where_the_edge_runs_straight_on_is_refused() {
        let flat = vec![
            c(0.0, 0.0),
            PolygonCorner {
                round: Some(1.0),
                ..c(10.0, 0.0)
            },
            c(20.0, 0.0),
            c(20.0, 10.0),
        ];
        assert_eq!(
            vertices("outline", &flat).unwrap_err(),
            [
                "`outline`: the corner (10mm, 0mm) with `round 1mm` has its edges running on into each other, so there is no corner to round"
            ]
        );
    }
}
