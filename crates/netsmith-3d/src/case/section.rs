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
//!
//! [`PlacedCase::ceilings`] reads the height limits of a board's pocket from the same
//! sections: the undersides of the case above the board's top face.

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

/// How far above the board's top face the plane is that tells what stands on the board: a post
/// that stands through the board is material there, a lid is not (mm).
const BOARD_TOP: f64 = 0.005;

/// The lowest a flat face may be above the board's top face to be an underside that limits a
/// part, and how near two heights are the same one (mm). A face at the board's top face is a
/// boss the board rests on.
const FLAT: f64 = 0.01;

/// How far above an underside the plane is that shows the material over it (mm).
const ABOVE: f64 = 0.005;

/// The box of a limit is taken outward to a multiple of this (mm): the section of a curved face
/// is of triangles up to 0.01 mm inside it, and a limit that is a little too small would let a
/// part touch the case.
const GRID: f64 = 0.05;

/// How many cells, along each side of a bounding box, are tried to tell what an area is made of.
const CELLS: usize = 24;

/// A height limit the case puts over part of a board's pocket: nothing taller than `height`
/// (mm above the board's top face) stands inside the box `lo`..`hi`.
#[derive(Debug, Clone, PartialEq)]
pub struct Ceiling {
    /// The height of the underside, from the board's top face.
    pub height: f64,
    /// The box's corner nearest the origin, in the scene's frame.
    pub lo: [f64; 2],
    /// The opposite corner.
    pub hi: [f64; 2],
    /// The solid whose underside it is.
    pub solid: String,
}

/// The limits [`PlacedCase::ceilings`] found, and the planes it could not cut.
#[derive(Debug, Clone, PartialEq)]
pub struct Ceilings {
    /// Lowest first.
    pub zones: Vec<Ceiling>,
    /// A plane at which a solid was not closed: the limits that plane would show are not read.
    pub skipped: Vec<SectionError>,
}

/// Whether the curves enclose `p` an odd number of times: it is in material.
fn material(rings: &[Ring], p: [f64; 2]) -> bool {
    rings.iter().filter(|r| inside(p, &r.points)).count() % 2 == 1
}

fn bounds(points: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in points {
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    (lo, hi)
}

impl PlacedCase {
    /// The heights of the flat faces of the model above the board's top face (z = 0), lowest
    /// first, those within [`FLAT`] of one another counted once.
    fn flat_heights(&self) -> Vec<f64> {
        let mut heights: Vec<f64> = Vec::new();
        for solid in &self.solids {
            for t in &solid.triangles {
                let z = t[0][2];
                if t.iter().all(|v| (v[2] - z).abs() < 1e-6) && z > FLAT {
                    heights.push(z);
                }
            }
        }
        heights.sort_by(f64::total_cmp);
        let mut out: Vec<f64> = Vec::new();
        for z in heights {
            if out.last().is_none_or(|&last| z - last >= FLAT) {
                out.push(z);
            }
        }
        out
    }

    /// The height limits the case puts over the pocket `pocket` (the board's outline, from
    /// [`PlacedCase::section`] at the board's plane): where a part would meet the case if it
    /// stood tall enough, from the lowest underside up.
    ///
    /// Every flat face above the board's top face is a height at which an underside may be. The
    /// plane just above it cuts the material there: each curve of material with a part of the
    /// pocket under it that nothing stands in from the board's top face (a post through the
    /// board is a cutout, not a limit) is a limit of that height over the curve's box, cut
    /// to the pocket's. A limit whose box lies in a lower one is left out. A box is the
    /// curve's bounding box taken outward to 0.05 mm, so a shape that is not a rectangle
    /// limits more than it covers, and an underside that is not flat (a dome, a slope) is not
    /// read.
    pub fn ceilings(&self, pocket: &[[f64; 2]]) -> Ceilings {
        let mut skipped = Vec::new();
        let mut zones: Vec<Ceiling> = Vec::new();
        let base = match self.section(BOARD_TOP) {
            Ok(r) => r,
            Err(e) => {
                skipped.push(e);
                return Ceilings { zones, skipped };
            }
        };
        let (plo, phi) = bounds(pocket);
        for height in self.flat_heights() {
            let rings = match self.section(height + ABOVE) {
                Ok(r) => r,
                Err(e) => {
                    skipped.push(e);
                    continue;
                }
            };
            for ring in rings.iter().filter(|r| r.depth % 2 == 0) {
                // The curves just inside it: the holes of this curve's material.
                let holes: Vec<&Ring> = rings
                    .iter()
                    .filter(|o| o.depth == ring.depth + 1 && inside(o.points[0], &ring.points))
                    .collect();
                let (rlo, rhi) = bounds(&ring.points);
                let lo = [rlo[0].max(plo[0]), rlo[1].max(plo[1])];
                let hi = [rhi[0].min(phi[0]), rhi[1].min(phi[1])];
                if lo[0] >= hi[0] || lo[1] >= hi[1] {
                    continue;
                }
                // The cells of the box that are in the pocket and in this curve's material.
                let mut under = 0;
                let mut standing = 0;
                for i in 0..CELLS {
                    for j in 0..CELLS {
                        let p = [
                            lo[0] + (hi[0] - lo[0]) * (i as f64 + 0.5) / CELLS as f64,
                            lo[1] + (hi[1] - lo[1]) * (j as f64 + 0.5) / CELLS as f64,
                        ];
                        if inside(p, pocket)
                            && inside(p, &ring.points)
                            && !holes.iter().any(|h| inside(p, &h.points))
                        {
                            under += 1;
                            if material(&base, p) {
                                standing += 1;
                            }
                        }
                    }
                }
                if under == 0 || standing == under {
                    continue;
                }
                let lower = zones.iter().any(|z| {
                    z.height < height
                        && z.lo[0] <= lo[0] + 1e-3
                        && z.lo[1] <= lo[1] + 1e-3
                        && z.hi[0] >= hi[0] - 1e-3
                        && z.hi[1] >= hi[1] - 1e-3
                });
                if !lower {
                    let down =
                        |v: f64, least: f64| nm(((v / GRID + 1e-6).floor() * GRID).max(least));
                    let up = |v: f64, most: f64| nm(((v / GRID - 1e-6).ceil() * GRID).min(most));
                    zones.push(Ceiling {
                        height,
                        lo: [down(lo[0], plo[0]), down(lo[1], plo[1])],
                        hi: [up(hi[0], phi[0]), up(hi[1], phi[1])],
                        solid: ring.solid.clone(),
                    });
                }
            }
        }
        Ceilings { zones, skipped }
    }
}

#[cfg(test)]
mod tests;
