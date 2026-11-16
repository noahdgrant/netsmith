//! A case model cut by a plane parallel to the board (`netsmith case import`, ADR 0109 "Left
//! open"): the closed curves the plane draws on each solid, and how deep each lies inside the
//! others.
//!
//! The curves are found from the triangles alone. Each triangle the plane crosses gives one
//! segment between the points where two of its edges meet the plane; a point is computed from
//! the edge's two ends in a fixed order, so the two triangles that share an edge compute the
//! same bits and the segments join exactly. A vertex on the plane counts as below it. A solid
//! whose section is not closed (an open shell, or a shell that touches itself at the plane) is
//! an error that says where, as nothing can be said of the board from it.

use std::collections::BTreeMap;
use std::fmt;

use super::{PlacedCase, Triangle, nm};

/// One closed curve of a section, in the scene's frame (x, y in mm; the plane's height is the
/// section's).
#[derive(Debug, Clone, PartialEq)]
pub struct Ring {
    /// The solid whose surface the curve is on.
    pub solid: String,
    /// The corners, counterclockwise, the last joined to the first. A curve of a model made of
    /// arcs has one corner for each chord of the reader's tolerance.
    pub points: Vec<[f64; 2]>,
    /// How many other curves of the section enclose this one: 0 for the outside of a wall, 1
    /// for a pocket in it (the room a board stands in), 2 for a post standing in the pocket, 3
    /// for a hole in that post.
    pub depth: usize,
}

/// Why a case model has no section at a height.
#[derive(Debug, Clone, PartialEq)]
pub enum SectionError {
    /// The plane meets the surface of `solid` in a curve that does not close, near `at`.
    Open { solid: String, z: f64, at: [f64; 2] },
}

impl fmt::Display for SectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SectionError::Open { solid, z, at } => write!(
                f,
                "the solid `{solid}` is not closed at z = {z} mm: its surface meets that plane in a curve with a loose end near ({:.3}, {:.3}) (an open shell, or one that touches itself)",
                at[0], at[1]
            ),
        }
    }
}

impl std::error::Error for SectionError {}

type Key = (u64, u64);

fn key(p: [f64; 2]) -> Key {
    (p[0].to_bits(), p[1].to_bits())
}

/// Where the edge `p` to `q` meets the plane `z`, for a pair with `p` not above and `q` above or
/// the other way round. The ends are taken lowest first, so the point is the same bits whichever
/// triangle asks.
fn crossing(p: [f64; 3], q: [f64; 3], z: f64) -> [f64; 2] {
    let (a, b) = if p[2] <= q[2] { (p, q) } else { (q, p) };
    let t = (z - a[2]) / (b[2] - a[2]);
    [nm(a[0] + t * (b[0] - a[0])), nm(a[1] + t * (b[1] - a[1]))]
}

/// The segments the plane `z` cuts from `triangles`.
fn segments(triangles: &[Triangle], z: f64) -> Vec<([f64; 2], [f64; 2])> {
    let mut out = Vec::new();
    for t in triangles {
        let above = t.map(|v| v[2] > z);
        if above[0] == above[1] && above[1] == above[2] {
            continue;
        }
        let mut ends = Vec::with_capacity(2);
        for k in 0..3 {
            let l = (k + 1) % 3;
            if above[k] != above[l] {
                ends.push(crossing(t[k], t[l], z));
            }
        }
        if let [a, b] = ends[..]
            && a != b
        {
            out.push((a, b));
        }
    }
    out
}

/// The closed curves of `segments`, each counterclockwise.
fn chain(
    segs: &[([f64; 2], [f64; 2])],
    solid: &str,
    z: f64,
) -> Result<Vec<Vec<[f64; 2]>>, SectionError> {
    let mut at: BTreeMap<Key, Vec<usize>> = BTreeMap::new();
    for (i, (a, b)) in segs.iter().enumerate() {
        at.entry(key(*a)).or_default().push(i);
        at.entry(key(*b)).or_default().push(i);
    }
    for (k, v) in &at {
        if v.len() != 2 {
            let (a, b) = segs[v[0]];
            let p = if key(a) == *k { a } else { b };
            return Err(SectionError::Open {
                solid: solid.to_string(),
                z,
                at: p,
            });
        }
    }
    let mut used = vec![false; segs.len()];
    let mut rings = Vec::new();
    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        let mut points = vec![segs[start].0];
        let mut cur = start;
        let mut to = segs[start].1;
        used[start] = true;
        while key(to) != key(points[0]) {
            points.push(to);
            let next = at[&key(to)]
                .iter()
                .copied()
                .find(|&i| i != cur)
                .expect("every point has two segments");
            used[next] = true;
            let (a, b) = segs[next];
            to = if key(a) == key(to) { b } else { a };
            cur = next;
        }
        if area(&points) < 0.0 {
            points.reverse();
        }
        rings.push(points);
    }
    Ok(rings)
}

/// Half the signed area: positive when the points turn counterclockwise.
pub(super) fn area(p: &[[f64; 2]]) -> f64 {
    let n = p.len();
    (0..n)
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

/// Whether `p` is inside the curve `ring` (the even-odd rule; curves of one section never touch).
fn inside(p: [f64; 2], ring: &[[f64; 2]]) -> bool {
    let n = ring.len();
    let mut crossings = 0;
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if x > p[0] {
                crossings += 1;
            }
        }
    }
    crossings % 2 == 1
}

impl PlacedCase {
    /// The closed curves the plane at height `z` (mm, in the scene's frame) draws on this case
    /// model's solids, each with its depth among them, in a fixed order: by solid, then as the
    /// triangles meet the plane.
    pub fn section(&self, z: f64) -> Result<Vec<Ring>, SectionError> {
        let mut rings: Vec<Ring> = Vec::new();
        for solid in &self.solids {
            let segs = segments(&solid.triangles, z);
            for points in chain(&segs, &solid.name, z)? {
                rings.push(Ring {
                    solid: solid.name.clone(),
                    points,
                    depth: 0,
                });
            }
        }
        let depths: Vec<usize> = rings
            .iter()
            .enumerate()
            .map(|(i, r)| {
                rings
                    .iter()
                    .enumerate()
                    .filter(|(j, o)| *j != i && inside(r.points[0], &o.points))
                    .count()
            })
            .collect();
        for (r, d) in rings.iter_mut().zip(depths) {
            r.depth = d;
        }
        Ok(rings)
    }
}

#[cfg(test)]
mod tests;
