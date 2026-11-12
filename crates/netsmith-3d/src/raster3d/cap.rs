//! The caps of a section: the face where the plane cuts a closed mesh, as triangles lying in
//! the plane.
//!
//! The plane meets each triangle that has corners on both sides (a corner exactly on the plane
//! counts as on the upper side, so a face lying in the plane is never half cut) in one segment.
//! The segments of a closed mesh join into closed loops; a loop inside an odd number of others is
//! a hole of the smallest loop round it, and every outer loop with its holes is triangulated
//! ([`crate::mesh::triangulate`]). A mesh whose segments do not join up (it is not closed after
//! all) has no cap, and the drawing leaves it open.
//!
//! The same input gives the same triangles: the crossing of an edge is worked out from its
//! lower end (so the two triangles on an edge agree to the bit), joins are matched at a
//! micrometre, and every walk is in index order.

use std::collections::BTreeMap;

use crate::mesh::{Mesh, triangulate};

/// Segments' ends closer than this (mm) are one corner.
const JOIN: f64 = 1e-6;

type P2 = [f64; 2];

fn less(a: [f64; 3], b: [f64; 3]) -> bool {
    for k in 0..3 {
        match a[k].total_cmp(&b[k]) {
            std::cmp::Ordering::Less => return true,
            std::cmp::Ordering::Greater => return false,
            std::cmp::Ordering::Equal => {}
        }
    }
    false
}

/// Where the edge `a`-`b` meets the plane `p[k] = at`, as its coordinates `(k + 1, k + 2)`.
fn crossing(a: [f64; 3], b: [f64; 3], k: usize, at: f64) -> P2 {
    let (a, b) = if less(b, a) { (b, a) } else { (a, b) };
    let (da, db) = (a[k] - at, b[k] - at);
    let t = da / (da - db);
    let (u, v) = ((k + 1) % 3, (k + 2) % 3);
    [a[u] + (b[u] - a[u]) * t, a[v] + (b[v] - a[v]) * t]
}

fn key(p: P2) -> (i64, i64) {
    ((p[0] / JOIN).round() as i64, (p[1] / JOIN).round() as i64)
}

fn area(r: &[P2]) -> f64 {
    let mut a = 0.0;
    for i in 0..r.len() {
        let (p, q) = (r[i], r[(i + 1) % r.len()]);
        a += p[0] * q[1] - q[0] * p[1];
    }
    0.5 * a.abs()
}

/// Whether `p` is inside the ring (even-odd).
fn inside(r: &[P2], p: P2) -> bool {
    let mut c = false;
    for i in 0..r.len() {
        let (a, b) = (r[i], r[(i + 1) % r.len()]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < a[0] + (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1])
        {
            c = !c;
        }
    }
    c
}

/// The closed loops where the plane `p[k] = at` cuts `mesh`, or `None` when they do not join up.
fn loops(mesh: &Mesh, k: usize, at: f64) -> Option<Vec<Vec<P2>>> {
    let mut segments: Vec<(P2, P2)> = Vec::new();
    for t in mesh.indices.as_chunks::<3>().0 {
        let ix = [t[0] as usize, t[1] as usize, t[2] as usize];
        if ix.iter().any(|&i| i >= mesh.positions.len()) {
            continue;
        }
        let p = ix.map(|i| mesh.positions[i]);
        let up = p.map(|q| q[k] - at >= 0.0);
        if up[0] == up[1] && up[1] == up[2] {
            continue;
        }
        // The corner on its own side, and the other two.
        let odd = (0..3)
            .find(|&i| up[i] != up[(i + 1) % 3] && up[i] != up[(i + 2) % 3])
            .unwrap_or(0);
        let (b, c) = ((odd + 1) % 3, (odd + 2) % 3);
        let (s, e) = (crossing(p[odd], p[b], k, at), crossing(p[odd], p[c], k, at));
        if key(s) != key(e) {
            segments.push((s, e));
        }
    }
    let mut ends: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    let mut points: BTreeMap<(i64, i64), P2> = BTreeMap::new();
    for (i, &(s, e)) in segments.iter().enumerate() {
        for q in [s, e] {
            ends.entry(key(q)).or_default().push(i);
            points.entry(key(q)).or_insert(q);
        }
    }
    if ends.values().any(|v| v.len() != 2) {
        return None;
    }
    let mut used = vec![false; segments.len()];
    let mut out = Vec::new();
    for first in 0..segments.len() {
        if used[first] {
            continue;
        }
        used[first] = true;
        let start = key(segments[first].0);
        let mut at_key = key(segments[first].1);
        let mut ring = vec![points[&start]];
        while at_key != start {
            ring.push(points[&at_key]);
            let next = ends[&at_key].iter().copied().find(|&i| !used[i])?;
            used[next] = true;
            let (s, e) = segments[next];
            at_key = if key(s) == at_key { key(e) } else { key(s) };
        }
        out.push(ring);
    }
    Some(out)
}

/// The triangles of the cut face of `mesh` with the plane `p[k] = at`, in the scene's frame, or
/// `None` when the section does not close up.
pub(super) fn cap(mesh: &Mesh, k: usize, at: f64) -> Option<Vec<[[f64; 3]; 3]>> {
    let rings: Vec<Vec<P2>> = loops(mesh, k, at)?
        .into_iter()
        .filter(|r| r.len() >= 3 && area(r) > 1e-12)
        .collect();
    let areas: Vec<f64> = rings.iter().map(|r| area(r)).collect();
    // For each ring the rings that contain it.
    let parents: Vec<Vec<usize>> = (0..rings.len())
        .map(|i| {
            (0..rings.len())
                .filter(|&j| j != i && areas[j] > areas[i] && inside(&rings[j], rings[i][0]))
                .collect()
        })
        .collect();
    let mut tris = Vec::new();
    for (i, ring) in rings.iter().enumerate() {
        if parents[i].len() % 2 == 1 {
            continue;
        }
        // The holes: the rings whose nearest container is this one.
        let holes: Vec<Vec<P2>> = (0..rings.len())
            .filter(|&j| parents[j].len() == parents[i].len() + 1 && parents[j].contains(&i))
            .map(|j| rings[j].clone())
            .collect();
        let mut points: Vec<P2> = ring.clone();
        for h in &holes {
            points.extend_from_slice(h);
        }
        let at3 = |q: P2| {
            let mut p = [0.0; 3];
            p[k] = at;
            p[(k + 1) % 3] = q[0];
            p[(k + 2) % 3] = q[1];
            p
        };
        for t in triangulate(ring, &holes) {
            tris.push(t.map(|i| at3(points[i])));
        }
    }
    Some(tris)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{Material, Mesh, Polygon, circle, rounded_rect};

    fn area3(tris: &[[[f64; 3]; 3]]) -> f64 {
        tris.iter()
            .map(|t| {
                let u = [0, 1, 2].map(|k| t[1][k] - t[0][k]);
                let v = [0, 1, 2].map(|k| t[2][k] - t[0][k]);
                let c = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                0.5 * (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt()
            })
            .sum()
    }

    #[test]
    fn a_box_is_capped_with_its_section_and_a_plane_in_a_face_is_not_half_cut() {
        let b = Mesh::axis_box("b", Material::Body, [0.0, 0.0, 0.0], [4.0, 3.0, 2.0]);
        let c = cap(&b, 0, 1.5).unwrap();
        assert!((area3(&c) - 6.0).abs() < 1e-9);
        assert!(c.iter().all(|t| t.iter().all(|p| p[0] == 1.5)));
        // On the face x = 4: the whole face (the plane's upper side takes the faces in it).
        let on = cap(&b, 0, 4.0).unwrap();
        assert!((area3(&on) - 6.0).abs() < 1e-9);
        // Beyond the box: nothing.
        assert!(cap(&b, 0, 5.0).unwrap().is_empty());
    }

    #[test]
    fn a_plate_with_a_hole_has_a_cap_with_a_gap_and_a_slab_cut_flat_has_a_ring() {
        let plate = Polygon::new(rounded_rect([0.0, 0.0], [20.0, 10.0], 0.0, 0)).with_hole(circle(
            [10.0, 5.0],
            2.0,
            32,
        ));
        let m = Mesh::prism("p", Material::Substrate, &plate, -1.0, 0.0).unwrap();
        // Across the hole: two pieces, 20 - 4 mm wide in all, 1 mm thick.
        let c = cap(&m, 1, 5.0).unwrap();
        let hole_width = 2.0 * 2.0 * crate::mesh::trig::sin_cos_turns(0.25).0;
        assert!(
            (area3(&c) - (20.0 - hole_width)).abs() < 0.05,
            "{}",
            area3(&c)
        );
        // A horizontal cut: the plate with its hole.
        let flat = cap(&m, 2, -0.5).unwrap();
        assert!((area3(&flat) - plate.area()).abs() < 1e-6);
    }

    #[test]
    fn an_open_surface_has_no_cap() {
        let sq = Polygon::new(rounded_rect([0.0, 0.0], [4.0, 4.0], 0.0, 0));
        let wall = Mesh::wall("w", Material::Copper, &sq.outer, 0.0, 1.0).unwrap();
        assert!(cap(&wall, 0, 2.0).is_none());
    }
}
