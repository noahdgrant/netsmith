//! A case model cut by a plane parallel to the board (`netsmith import case model`, ADR 0109 "Left
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

use super::{CaseSolid, PlacedCase, Triangle, nm};

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

/// A face whose normal points down by less than this (the sine of its tilt from upright) is
/// part of a wall, not an underside: a wall's draft of a few degrees limits nothing.
const UPRIGHT: f64 = 0.17;

/// The least area, seen from above, that a sloped face has to be an underside (mm²): the
/// triangles of a vertical curved wall are slivers whose normal points where rounding says.
const SLOPE_AREA: f64 = 0.01;

/// A height limit the case puts over part of a board's pocket: nothing taller than `height`
/// (mm from the board's face) stands inside the box `lo`..`hi`.
#[derive(Debug, Clone, PartialEq)]
pub struct Ceiling {
    /// The height of the underside, from the board's face.
    pub height: f64,
    /// The box's corner nearest the origin, in the scene's frame.
    pub lo: [f64; 2],
    /// The opposite corner.
    pub hi: [f64; 2],
    /// The solid whose underside it is.
    pub solid: String,
    /// Whether the underside is not flat: the limit is the height of a step of a slope, and
    /// the underside is no lower than it over the box.
    pub sloped: bool,
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

/// A height a limit may be at, and the plane that shows the material over it. A step of a
/// slope is read of the solid that has the sloped face only.
struct Level {
    limit: f64,
    plane: f64,
    sloped: Option<String>,
}

impl PlacedCase {
    /// The case seen from the other face of the board: every height `z` is `-thickness - z`
    /// (the board's bottom face is the top face of the result, and what is below it is above),
    /// and each triangle keeps facing out.
    pub fn mirrored(&self, thickness: f64) -> PlacedCase {
        let solids = self
            .solids
            .iter()
            .map(|s| {
                let flip = |p: [f64; 3]| [p[0], p[1], nm(-thickness - p[2])];
                let triangles = s
                    .triangles
                    .iter()
                    .map(|t| [flip(t[0]), flip(t[2]), flip(t[1])]);
                CaseSolid::new(s.name.clone(), triangles.collect(), s.closed)
                    .with_runs(s.runs.clone())
            })
            .collect();
        PlacedCase {
            solids,
            ..self.clone()
        }
    }

    /// The levels at which an underside may be, above the board's top face: the heights of the
    /// flat faces (exact), and for each stretch of heights that a sloped face (a dome, a
    /// chamfer, a slope) spans, a step of `step` mm at a time, each limited to the height of its
    /// lower end (a point first under material at the plane just below the next step is no
    /// lower than that).
    fn levels(&self, step: f64) -> Vec<Level> {
        let mut heights: Vec<f64> = Vec::new();
        let mut sloped: Vec<(&str, Vec<(f64, f64)>)> = Vec::new();
        for solid in &self.solids {
            let mut spans: Vec<(f64, f64)> = Vec::new();
            for t in &solid.triangles {
                let z = t[0][2];
                if t.iter().all(|v| (v[2] - z).abs() < 1e-6) {
                    if z > FLAT {
                        heights.push(z);
                    }
                    continue;
                }
                let (u, v) = (
                    [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]],
                    [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]],
                );
                let n = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                if len < 1e-12 || n[2] / len > -UPRIGHT || -n[2] / 2.0 < SLOPE_AREA {
                    continue;
                }
                let lo = t.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
                let hi = t.iter().map(|p| p[2]).fold(f64::NEG_INFINITY, f64::max);
                if hi > FLAT {
                    spans.push((lo.max(FLAT), hi));
                }
            }
            sloped.push((&solid.name, spans));
        }
        heights.sort_by(f64::total_cmp);
        let mut levels: Vec<Level> = Vec::new();
        let mut last = f64::NEG_INFINITY;
        for z in heights {
            if z - last >= FLAT {
                levels.push(Level {
                    limit: z,
                    plane: z + ABOVE,
                    sloped: None,
                });
                last = z;
            }
        }
        // The stretches of height the sloped faces of a solid span, joined where they meet.
        for (name, mut spans) in sloped {
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut stretches: Vec<(f64, f64)> = Vec::new();
            for s in spans {
                match stretches.last_mut() {
                    Some(last) if s.0 <= last.1 => last.1 = last.1.max(s.1),
                    _ => stretches.push(s),
                }
            }
            for (lo, hi) in stretches {
                let mut limit = lo;
                while limit < hi - ABOVE {
                    levels.push(Level {
                        limit,
                        plane: (limit + step - ABOVE).min(hi - ABOVE),
                        sloped: Some(name.to_string()),
                    });
                    limit += step;
                }
            }
        }
        levels.sort_by(|a, b| a.limit.total_cmp(&b.limit));
        levels
    }

    /// The height limits the case puts over the pocket `pocket` (the board's outline, from
    /// [`PlacedCase::section`] at the board's plane): where a part would meet the case if it
    /// stood tall enough, from the lowest underside up.
    ///
    /// The plane just above a flat face above the board's top face, and the planes a step
    /// (`step` mm) apart through the heights a sloped face spans, cut the material there: each
    /// curve of material with a part of the pocket under it that nothing stands in from the
    /// board's top face (a post through the board is a cutout, not a limit) is a limit of the
    /// level's height over the region the curve bounds (holes left out). A region is covered
    /// by rectangles taken outward to 0.05 mm: one for a rectangle, one for each part of an L
    /// or a notched shape, a slab of at most `step` mm for a curve, so a round underside is
    /// limited a little more than it covers, not over its box. A limit whose rectangle lies in
    /// one of the same height or a lower one is left out. A slope is limited by its steps: over a step the underside is
    /// no lower than the limit and up to `step` mm higher, and a feature thinner than a step
    /// is not seen.
    pub fn ceilings(&self, pocket: &[[f64; 2]], step: f64) -> Ceilings {
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
        let down = |v: f64, least: f64| nm(((v / GRID + 1e-6).floor() * GRID).max(least));
        let up = |v: f64, most: f64| nm(((v / GRID - 1e-6).ceil() * GRID).min(most));
        for level in self.levels(step) {
            let height = level.limit;
            let rings = match self.section(level.plane) {
                Ok(r) => r,
                Err(e) => {
                    skipped.push(e);
                    continue;
                }
            };
            let only = level.sloped.as_deref();
            for ring in rings
                .iter()
                .filter(|r| r.depth % 2 == 0 && only.is_none_or(|n| r.solid == n))
            {
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
                let mut region: Vec<&[[f64; 2]]> = vec![&ring.points];
                region.extend(holes.iter().map(|h| h.points.as_slice()));
                let rects: Vec<cover::Rect> = cover::rectangles(&region, lo, hi, step)
                    .into_iter()
                    .map(|(a, b)| {
                        (
                            [down(a[0], plo[0]), down(a[1], plo[1])],
                            [up(b[0], phi[0]), up(b[1], phi[1])],
                        )
                    })
                    .collect();
                for (rlo, rhi) in cover::join(rects) {
                    let lower = zones.iter().any(|z| {
                        z.height <= height
                            && z.lo[0] <= rlo[0] + 1e-3
                            && z.lo[1] <= rlo[1] + 1e-3
                            && z.hi[0] >= rhi[0] - 1e-3
                            && z.hi[1] >= rhi[1] - 1e-3
                    });
                    if !lower {
                        zones.push(Ceiling {
                            height,
                            lo: rlo,
                            hi: rhi,
                            solid: ring.solid.clone(),
                            sloped: level.sloped.is_some(),
                        });
                    }
                }
            }
        }
        Ceilings { zones, skipped }
    }

    /// The height limits the case puts under the board, for the parts on its bottom face: the
    /// same as [`PlacedCase::ceilings`] of the case seen from below, so each height is from the
    /// board's bottom face (`thickness` mm below its top face) down to the top of the floor, a
    /// rib or a boss standing under the pocket. A post that stands from there up to the board is
    /// not a limit.
    pub fn ceilings_below(&self, pocket: &[[f64; 2]], thickness: f64, step: f64) -> Ceilings {
        let mut found = self.mirrored(thickness).ceilings(pocket, step);
        for e in &mut found.skipped {
            let SectionError::Open { z, .. } = e;
            *z = nm(-thickness - *z);
        }
        found
    }
}

mod cover;
#[cfg(test)]
mod tests;
