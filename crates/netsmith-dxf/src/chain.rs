//! Joining edges into closed loops, and finding which loop is the board and which are cutouts.

use std::collections::HashMap;

use crate::entities::Seg;
use crate::geom::{ArcGeom, P, dist, flatten_edge, inside, segment_distance};
use crate::{Error, Gap, Polygon, Vertex};

/// Edge ends closer than the tolerance are one point; the loops the edges make. Every
/// point must have exactly two ends; the ones that do not are reported with their distance to
/// the nearest other loose end.
pub(crate) fn loops(
    segs: &[Seg],
    tol: f64,
    warnings: &mut Vec<String>,
) -> Result<Vec<Polygon>, Error> {
    // Drop edges shorter than the tolerance (their ends are one point anyway), and edges
    // drawn twice (CAD files often have them): both would give a point more than two ends.
    let cell = |p: P| ((p.0 / tol).round() as i64, (p.1 / tol).round() as i64);
    let mut seen = std::collections::HashSet::new();
    let mut doubled = 0;
    let mut tiny = 0;
    let mut segs_kept: Vec<Seg> = Vec::new();
    for s in segs {
        if dist(s.a, s.b) <= tol {
            tiny += 1;
            continue;
        }
        let (ca, cb) = (cell(s.a), cell(s.b));
        let bulge = (s.bulge * 1e6).round() as i64;
        let key = if (ca, cb) <= (cb, ca) {
            (ca, cb, bulge)
        } else {
            (cb, ca, -bulge)
        };
        if seen.insert(key) {
            segs_kept.push(*s);
        } else {
            doubled += 1;
        }
    }
    if tiny > 0 {
        warnings.push(format!("{tiny} edges shorter than {tol} mm were dropped"));
    }
    if doubled > 0 {
        warnings.push(format!("{doubled} edges drawn twice were counted once"));
    }
    let segs = segs_kept;
    let n = segs.len();
    if n == 0 {
        return Ok(Vec::new());
    }
    let end = |e: usize| {
        if e.is_multiple_of(2) {
            segs[e / 2].a
        } else {
            segs[e / 2].b
        }
    };

    // Union the ends that are within the tolerance, through a grid of cells one tolerance wide.
    let mut parent: Vec<usize> = (0..2 * n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let cell = |p: P| ((p.0 / tol).floor() as i64, (p.1 / tol).floor() as i64);
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for e in 0..2 * n {
        let (cx, cy) = cell(end(e));
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(others) = grid.get(&(cx + dx, cy + dy)) {
                    for &o in others {
                        if dist(end(e), end(o)) <= tol {
                            let (a, b) = (find(&mut parent, e), find(&mut parent, o));
                            if a != b {
                                parent[a.max(b)] = a.min(b);
                            }
                        }
                    }
                }
            }
        }
        grid.entry((cx, cy)).or_default().push(e);
    }
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); 2 * n];
    for e in 0..2 * n {
        let root = find(&mut parent, e);
        members[root].push(e);
    }

    let mut loose: Vec<usize> = Vec::new();
    let mut partner = vec![usize::MAX; 2 * n];
    for group in members.iter().filter(|g| !g.is_empty()) {
        match group.len() {
            1 => loose.push(group[0]),
            2 => {
                partner[group[0]] = group[1];
                partner[group[1]] = group[0];
            }
            k => {
                let c = group
                    .iter()
                    .fold((0.0, 0.0), |s, &e| (s.0 + end(e).0, s.1 + end(e).1));
                return Err(Error::Branch {
                    at: (c.0 / k as f64, c.1 / k as f64),
                    ends: k,
                });
            }
        }
    }
    if !loose.is_empty() {
        let gaps = loose
            .iter()
            .map(|&e| {
                let nearest = loose
                    .iter()
                    .filter(|&&o| o != e)
                    .map(|&o| (end(o), dist(end(e), end(o))))
                    .min_by(|a, b| a.1.partial_cmp(&b.1).expect("finite"));
                Gap {
                    at: end(e),
                    nearest: nearest.map(|n| n.0),
                    distance: nearest.map(|n| n.1),
                }
            })
            .collect();
        return Err(Error::Gaps {
            gaps,
            tolerance: tol,
        });
    }

    // Walk each loop.
    let mut used = vec![false; n];
    let mut out = Vec::new();
    for start in 0..n {
        if used[start] {
            continue;
        }
        let mut vertices = Vec::new();
        let (mut s, mut forward) = (start, true);
        loop {
            used[s] = true;
            let (a, bulge, to) = if forward {
                (segs[s].a, segs[s].bulge, 2 * s + 1)
            } else {
                (segs[s].b, -segs[s].bulge, 2 * s)
            };
            vertices.push(Vertex {
                x: a.0,
                y: a.1,
                bulge,
            });
            let next = partner[to];
            s = next / 2;
            forward = next % 2 == 0;
            if used[s] {
                break;
            }
        }
        out.push(Polygon { vertices });
    }
    Ok(out)
}

/// A circle as a loop of two half circles.
pub(crate) fn circle_loop(c: P, r: f64) -> Polygon {
    Polygon {
        vertices: vec![
            Vertex {
                x: c.0 + r,
                y: c.1,
                bulge: 1.0,
            },
            Vertex {
                x: c.0 - r,
                y: c.1,
                bulge: 1.0,
            },
        ],
    }
}

/// The signed area of a loop of edges and arcs: positive when it runs counterclockwise in a
/// frame with y up (clockwise on a screen with y down).
pub(crate) fn shoelace(vertices: &[Vertex]) -> f64 {
    let n = vertices.len();
    let mut area = 0.0;
    for i in 0..n {
        let (a, b) = (&vertices[i], &vertices[(i + 1) % n]);
        area += (a.x * b.y - b.x * a.y) / 2.0;
        if let Some(arc) = ArcGeom::of((a.x, a.y), (b.x, b.y), a.bulge) {
            area += arc.radius * arc.radius / 2.0 * (arc.sweep - arc.sweep.sin());
        }
    }
    area
}

/// How far a chord of a flattened arc may be from it (mm), for the tests of crossing.
const FLAT: f64 = 2e-4;

fn ring(poly: &Polygon) -> Vec<P> {
    let mut points = Vec::new();
    for (a, b, bulge) in poly.edges() {
        let mut pts = flatten_edge(a, b, bulge, FLAT);
        pts.pop();
        points.extend(pts);
    }
    points
}

fn corner(poly: &Polygon) -> String {
    poly.describe()
}

/// The least distance between two rings, if it is under `limit`, with where.
fn rings_closer_than(a: &[P], b: &[P], limit: f64) -> Option<P> {
    let boxes = |r: &[P]| -> Vec<(P, P)> {
        (0..r.len())
            .map(|i| {
                let (p, q) = (r[i], r[(i + 1) % r.len()]);
                ((p.0.min(q.0), p.1.min(q.1)), (p.0.max(q.0), p.1.max(q.1)))
            })
            .collect()
    };
    let (ba, bb) = (boxes(a), boxes(b));
    for (i, (alo, ahi)) in ba.iter().enumerate() {
        for (j, (blo, bhi)) in bb.iter().enumerate() {
            if alo.0 > bhi.0 + limit
                || blo.0 > ahi.0 + limit
                || alo.1 > bhi.1 + limit
                || blo.1 > ahi.1 + limit
            {
                continue;
            }
            let (a1, a2) = (a[i], a[(i + 1) % a.len()]);
            let (b1, b2) = (b[j], b[(j + 1) % b.len()]);
            if segment_distance(a1, a2, b1, b2) < limit {
                return Some(a1);
            }
        }
    }
    None
}

/// A point where the ring meets itself, if it does.
fn self_contact(r: &[P], limit: f64) -> Option<P> {
    let m = r.len();
    let mut order: Vec<usize> = (0..m).collect();
    let lo = |i: usize| r[i].0.min(r[(i + 1) % m].0);
    let hi = |i: usize| r[i].0.max(r[(i + 1) % m].0);
    order.sort_by(|&a, &b| lo(a).partial_cmp(&lo(b)).expect("finite"));
    for (k, &i) in order.iter().enumerate() {
        for &j in &order[k + 1..] {
            if lo(j) > hi(i) + limit {
                break;
            }
            let (a, b) = (i.min(j), i.max(j));
            if b == a + 1 || (a == 0 && b == m - 1) {
                continue;
            }
            let (p, q) = (r[i], r[(i + 1) % m]);
            let (s, t) = (r[j], r[(j + 1) % m]);
            if segment_distance(p, q, s, t) < limit {
                return Some(p);
            }
        }
    }
    None
}

/// The one outer loop and the cutouts inside it.
pub(crate) fn classify(loops: Vec<Polygon>, tol: f64) -> Result<(Polygon, Vec<Polygon>), Error> {
    if loops.is_empty() {
        return Err(Error::NoOutline);
    }
    let rings: Vec<Vec<P>> = loops.iter().map(ring).collect();
    let limit = tol / 2.0;
    for (poly, r) in loops.iter().zip(&rings) {
        if let Some(at) = self_contact(r, limit) {
            return Err(Error::Crossing {
                what: format!(
                    "a loop crosses or touches itself near ({:.4}, {:.4}) (mm in the drawing, y up): {}",
                    at.0,
                    at.1,
                    corner(poly)
                ),
            });
        }
    }
    let boxes: Vec<(P, P)> = loops.iter().map(Polygon::bounds).collect();
    let overlap = |i: usize, j: usize| {
        boxes[i].0.0 <= boxes[j].1.0 + tol
            && boxes[j].0.0 <= boxes[i].1.0 + tol
            && boxes[i].0.1 <= boxes[j].1.1 + tol
            && boxes[j].0.1 <= boxes[i].1.1 + tol
    };
    // depth[i]: how many loops it is inside; parent[i]: the innermost of them.
    let k = loops.len();
    let mut within: Vec<Vec<usize>> = vec![Vec::new(); k];
    for i in 0..k {
        for j in 0..k {
            if i == j || !overlap(i, j) {
                continue;
            }
            if i < j
                && let Some(at) = rings_closer_than(&rings[i], &rings[j], limit)
            {
                return Err(Error::Crossing {
                    what: format!(
                        "two loops cross or touch near ({:.4}, {:.4}) (mm in the drawing, y up): {} and {}",
                        at.0,
                        at.1,
                        corner(&loops[i]),
                        corner(&loops[j])
                    ),
                });
            }
            if inside(rings[i][0], &rings[j]) {
                within[i].push(j);
            }
        }
    }
    let outers: Vec<usize> = (0..k).filter(|&i| within[i].is_empty()).collect();
    if outers.len() > 1 {
        return Err(Error::SeveralOuterLoops {
            loops: outers.iter().map(|&i| corner(&loops[i])).collect(),
        });
    }
    let outer = outers[0];
    let mut cutouts = Vec::new();
    for i in 0..k {
        match within[i].len() {
            0 => {}
            1 => cutouts.push(loops[i].clone()),
            _ => {
                let enclosing = within[i]
                    .iter()
                    .copied()
                    .find(|&j| j != outer)
                    .unwrap_or(outer);
                return Err(Error::Island {
                    inner: corner(&loops[i]),
                    within: corner(&loops[enclosing]),
                });
            }
        }
    }
    Ok((loops[outer].clone(), cutouts))
}
