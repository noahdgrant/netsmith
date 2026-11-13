//! The meshes of a scene for a _section_: the same pieces as [`of_scene`](super::of_scene), with
//! the board drawn as its stackup, so that the cut face the rasterizer fills ([`crate::raster3d`],
//! caps) shows each layer in its own colour at its true thickness.
//!
//! What differs from the whole board's meshes:
//!
//! - **The board is a slab per stackup layer** (id `board` on each), at the scene's own heights
//!   (`board.layers`): the mask (`mask-top`, `mask-bottom`), the core (`substrate`) and the
//!   prepreg (`prepreg`). Where the stackup has a copper layer the slab is the insulator that
//!   fills between its copper: the mask for the two outer layers, the prepreg for an inner one.
//! - **The copper of each layer is where the plane crosses it**, as flat pieces lying in the
//!   plane (id `copper:<layer>`, material `copper`): the pads, tracks, pours and via rings the
//!   scene has, less the drilled holes. They are worked out in one dimension (the line where
//!   the plane meets the layer), so overlapping copper needs no union. A plane across the
//!   layer's thickness (`z` within a copper layer) draws that layer's polygons themselves.
//! - **A plated barrel has a wall** ([`PLATING`] thick, a closed ring) instead of a bare surface,
//!   so the cut shows it as two strips of copper either side of the hole.
//!
//! Bodies, pins and limits are as in [`of_scene`](super::of_scene). The mesh order is the one
//! that makes the painter's rule of the caps right: the board's slabs, the copper over them, the
//! barrels, then the rest.

use super::scene::{board_outline, hole_segments, slot};
use super::{Axis, Material, Mesh, Polygon, pieces};
use crate::scene3d::{CopperLayer, Hole, HoleOwner, Plan, PlanArea, Scene};
use netsmith_pcb::stackup::LayerKind;

/// The thickness of the copper plating on the wall of a drilled hole, in mm. A picture's figure:
/// the fabricator's own varies (IPC-6012 asks at least 20 µm on average for class 2), and the
/// scene holds none.
pub const PLATING: f64 = 0.025;

/// The meshes of `scene` for a section at right angles to `axis` at `at` mm, in the order of the
/// module's header.
pub fn of_scene_section(scene: &Scene, axis: Axis, at: f64) -> Vec<Mesh> {
    let p = pieces(scene);
    let solid = &scene.board;
    let outline = board_outline(solid);
    let mut out = slabs(scene, &outline);
    if let Ok(copper) = &scene.copper {
        for layer in &copper.layers {
            out.extend(copper_cap(scene, layer, &copper.vias, axis, at));
        }
    }
    out.extend(barrels(scene));
    for b in p.bodies {
        out.extend(b.body);
        out.extend(b.pins);
    }
    out.extend(p.limits.into_iter().flatten().flatten());
    // The case is cut with the rest: the rasterizer keeps the far side of the plane.
    out.extend(p.cases.into_iter().flatten());
    out
}

/// A slab per layer of the stackup (see the module's header).
fn slabs(scene: &Scene, outline: &Polygon) -> Vec<Mesh> {
    let solid = &scene.board;
    let mut out = Vec::new();
    let last_copper = solid
        .layers
        .iter()
        .rposition(|l| l.kind == LayerKind::Copper);
    let first_copper = solid
        .layers
        .iter()
        .position(|l| l.kind == LayerKind::Copper);
    for (i, l) in solid.layers.iter().enumerate() {
        let material = match l.kind {
            LayerKind::Mask if l.z_top == 0.0 => Material::MaskTop,
            LayerKind::Mask => Material::MaskBottom,
            LayerKind::Core => Material::Substrate,
            LayerKind::Prepreg => Material::Prepreg,
            LayerKind::Copper if Some(i) == first_copper => Material::MaskTop,
            LayerKind::Copper if Some(i) == last_copper => Material::MaskBottom,
            LayerKind::Copper => Material::Prepreg,
        };
        out.extend(Mesh::prism("board", material, outline, l.z_bottom, l.z_top).ok());
    }
    if out.is_empty() {
        out.extend(Mesh::prism("board", Material::Substrate, outline, -solid.thickness, 0.0).ok());
    }
    out
}

/// The z range a drilled hole's plating covers: from the top of the front copper to the bottom
/// of the back copper, else the board's whole thickness.
fn barrel_range(scene: &Scene) -> (f64, f64) {
    let solid = &scene.board;
    let copper: Vec<_> = solid
        .layers
        .iter()
        .filter(|l| l.kind == LayerKind::Copper)
        .collect();
    match (copper.first(), copper.last()) {
        (Some(f), Some(l)) => (l.z_bottom, f.z_top),
        _ => (-solid.thickness, 0.0),
    }
}

fn xy(p: Plan) -> [f64; 2] {
    [p.x, p.y]
}

fn hole_ring(h: &Hole, grow: f64) -> Vec<[f64; 2]> {
    let d = h.diameter + 2.0 * grow;
    slot(xy(h.from), xy(h.to), d, hole_segments(h.diameter))
}

/// The plated holes as rings of copper [`PLATING`] thick.
fn barrels(scene: &Scene) -> Vec<Mesh> {
    let (z0, z1) = barrel_range(scene);
    scene
        .board
        .holes
        .iter()
        .filter(|h| h.plated)
        .filter_map(|h| {
            let id = match &h.owner {
                HoleOwner::Pad { path, .. } => path.clone(),
                HoleOwner::Via { net } => net.clone(),
            };
            let ring = Polygon::new(hole_ring(h, PLATING)).with_hole(hole_ring(h, 0.0));
            Mesh::prism(id, Material::Copper, &ring, z0, z1).ok()
        })
        .collect()
}

/// Where the line `coord[k] = c` meets the rings, as the positions of the crossings along the
/// other axis `t`, sorted: a ring edge counts when it has one end at or below `c` and the other
/// above (so a corner on the line counts once). Taken two by two they are the pieces of the line
/// inside the rings, with the even-odd rule (a hole is a ring too).
fn crossings<'a>(rings: impl Iterator<Item = &'a [[f64; 2]]>, k: usize, c: f64) -> Vec<f64> {
    let mut out = Vec::new();
    for r in rings {
        for i in 0..r.len() {
            let (a, b) = (r[i], r[(i + 1) % r.len()]);
            if (a[k] <= c) != (b[k] <= c) {
                let s = (c - a[k]) / (b[k] - a[k]);
                out.push(a[1 - k] + (b[1 - k] - a[1 - k]) * s);
            }
        }
    }
    out.sort_by(f64::total_cmp);
    out
}

fn pairs(xs: &[f64]) -> Vec<(f64, f64)> {
    xs.as_chunks::<2>().0.iter().map(|c| (c[0], c[1])).collect()
}

/// The sorted, merged union of `spans`.
fn union(mut spans: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    spans.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for s in spans {
        match out.last_mut() {
            Some(last) if s.0 <= last.1 => last.1 = last.1.max(s.1),
            _ => out.push(s),
        }
    }
    out
}

/// `spans` (merged) less `cuts` (merged).
fn minus(spans: &[(f64, f64)], cuts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for &(mut a, b) in spans {
        for &(c0, c1) in cuts {
            if c1 <= a || c0 >= b {
                continue;
            }
            if c0 > a {
                out.push((a, c0));
            }
            a = a.max(c1);
        }
        if a < b {
            out.push((a, b));
        }
    }
    out
}

fn area_rings(a: &PlanArea) -> Vec<Vec<[f64; 2]>> {
    std::iter::once(&a.outer)
        .chain(a.holes.iter())
        .map(|r| r.iter().map(|p| xy(*p)).collect())
        .collect()
}

/// The copper of `layer` where the plane cuts it.
fn copper_cap(
    scene: &Scene,
    layer: &CopperLayer,
    vias: &[crate::scene3d::Via],
    axis: Axis,
    at: f64,
) -> Option<Mesh> {
    if layer.z_top <= layer.z_bottom {
        return None;
    }
    let id = format!("copper:{}", layer.name);
    let areas: Vec<&PlanArea> = layer
        .pads
        .iter()
        .flat_map(|p| p.areas.iter())
        .chain(layer.tracks.iter().map(|t| &t.area))
        .chain(layer.pours.iter().map(|p| &p.area))
        .chain(layer.drawings.iter())
        .collect();
    if axis == Axis::Z {
        // The plane is across the layer: its polygons, where it is within the layer.
        if at < layer.z_bottom || at > layer.z_top {
            return None;
        }
        let mut m = Mesh::new(id.clone(), Material::Copper);
        for a in &areas {
            let mut poly = Polygon::new(a.outer.iter().map(|p| xy(*p)).collect());
            poly.holes = a
                .holes
                .iter()
                .map(|h| h.iter().map(|p| xy(*p)).collect())
                .collect();
            if let Ok(f) = Mesh::flat(id.clone(), Material::Copper, &poly, at, true) {
                m.append(&f);
            }
        }
        m.closed = false;
        return (m.triangle_count() > 0).then_some(m);
    }
    // x and y: the line along the other horizontal axis `t`, and z up.
    let k = axis.index();
    let mut spans = Vec::new();
    for a in &areas {
        spans.extend(pairs(&crossings(
            area_rings(a).iter().map(Vec::as_slice),
            k,
            at,
        )));
    }
    let r = |d: f64| 0.5 * d;
    for v in vias {
        let p = [v.at.x, v.at.y];
        let dc = (at - p[k]).abs();
        if dc < r(v.diameter) {
            let h = (r(v.diameter) * r(v.diameter) - dc * dc).sqrt();
            spans.push((p[1 - k] - h, p[1 - k] + h));
        }
    }
    let mut drills = Vec::new();
    for h in &scene.board.holes {
        drills.extend(pairs(&crossings(
            std::iter::once(hole_ring(h, 0.0).as_slice()),
            k,
            at,
        )));
    }
    let spans = minus(&union(spans), &union(drills));
    let mut m = Mesh::new(id, Material::Copper);
    // Open, with no normals: each piece is lit by its own face, turned to the camera.
    m.closed = false;
    m.normals = None;
    for (t0, t1) in spans {
        if t1 - t0 <= 1e-9 {
            continue;
        }
        let at3 = |t: f64, z: f64| {
            let mut p = [0.0; 3];
            p[k] = at;
            p[1 - k] = t;
            p[2] = z;
            p
        };
        let base = m.positions.len() as u32;
        for (t, z) in [
            (t0, layer.z_bottom),
            (t1, layer.z_bottom),
            (t1, layer.z_top),
            (t0, layer.z_top),
        ] {
            m.positions.push(at3(t, z));
        }
        m.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (m.triangle_count() > 0).then_some(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_merge_and_subtract() {
        let u = union(vec![(3.0, 4.0), (0.0, 2.0), (1.0, 2.5), (4.0, 5.0)]);
        assert_eq!(u, vec![(0.0, 2.5), (3.0, 5.0)]);
        let m = minus(&u, &[(1.0, 1.5), (2.0, 3.5), (4.5, 9.0)]);
        assert_eq!(m, vec![(0.0, 1.0), (1.5, 2.0), (3.5, 4.5)]);
    }

    #[test]
    fn a_line_through_a_square_with_a_hole_crosses_four_times() {
        let outer = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let hole = vec![[4.0, 4.0], [4.0, 6.0], [6.0, 6.0], [6.0, 4.0]];
        let x = crossings([outer.as_slice(), hole.as_slice()].into_iter(), 0, 5.0);
        assert_eq!(x, vec![0.0, 4.0, 6.0, 10.0]);
        assert_eq!(pairs(&x), vec![(0.0, 4.0), (6.0, 10.0)]);
        // A line along an edge counts as inside: the two corner edges cross it once each.
        let edge = crossings([outer.as_slice()].into_iter(), 0, 0.0);
        assert_eq!(edge, vec![0.0, 10.0]);
    }
}
