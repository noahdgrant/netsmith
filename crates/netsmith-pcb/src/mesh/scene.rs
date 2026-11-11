//! The meshes of a board's [`Scene`]: the one source of triangles that the rasterizer
//! ([`crate::raster3d`]) and the `.glb` writer both draw, so a picture and a file show the same
//! geometry with the same ids.
//!
//! In order:
//!
//! 1. The board, id `board`: the substrate (material `substrate`) from the bottom mask up to
//!    the top mask, and each mask layer of the stackup as a thin solid of its own (`mask-top`,
//!    `mask-bottom`), all three with the rounded outline, the cutouts and every drilled hole
//!    (round, or a slot) as real holes. Without mask layers the substrate is the whole thickness.
//! 2. The plated barrels (`copper`, open walls through the board): a via's id is its net, a
//!    pad's the part's path.
//! 3. Per part, by path, id the part's path: a body of known height as its turned body box
//!    from the face out to its far end (`body`; a bottom part hangs down); a body of unknown
//!    height as its outline flat at [`UNKNOWN_LIFT`] mm off its face (`body-unknown`), never a
//!    box (ADR 0105); the pins below the far face where their protrusion is known (`metal`).
//! 4. Per height limit, by name, id `limit:<name>`: a volume over each face it limits, `limit`,
//!    or `limit-violated` where a part on that face is too tall for it.
//!
//! The scene's lists are already in a fixed order, and so are these meshes: the same scene gives
//! the same meshes, bit for bit. A piece whose shape has no area (a zero-size box) is left out.

use crate::board::Side;
use crate::mesh::{Material, Mesh, Polygon, circle, rounded_rect};
use crate::scene3d::{Body, HoleOwner, Pins, Plan, PlanBox, Scene};
use crate::stackup::LayerKind;

/// How far off its face the flat outline of a part of unknown height is drawn, in mm.
pub const UNKNOWN_LIFT: f64 = 0.02;

/// The corners a round hole of `diameter` mm is drawn with.
pub fn hole_segments(diameter: f64) -> usize {
    if diameter <= 1.0 {
        16
    } else if diameter <= 3.0 {
        24
    } else {
        32
    }
}

/// The ring of a slot of `diameter` with its axis from `a` to `b` (a circle when they meet),
/// counterclockwise.
pub fn slot(a: [f64; 2], b: [f64; 2], diameter: f64, segments: usize) -> Vec<[f64; 2]> {
    let r = 0.5 * diameter;
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 {
        return circle(a, r, segments);
    }
    let (ux, uy) = (dx / len, dy / len);
    let half = segments.max(4) / 2;
    let mut out = Vec::with_capacity(2 * (half + 1));
    // A half circle round `b` from its right side (seen along the axis), then one round `a`.
    for (c, start) in [(b, 0.0), (a, 0.5)] {
        for k in 0..=half {
            let (s, co) = super::trig::sin_cos_turns(start - 0.25 + 0.5 * k as f64 / half as f64);
            // In the frame of the axis: co along it, s across it (to the left).
            out.push([c[0] + r * (co * ux - s * uy), c[1] + r * (co * uy + s * ux)]);
        }
    }
    out
}

fn xy(p: Plan) -> [f64; 2] {
    [p.x, p.y]
}

fn box_ring(b: &PlanBox) -> Vec<[f64; 2]> {
    rounded_rect(xy(b.min), xy(b.max), 0.0, 0)
}

/// The meshes of `scene`, in the order of the module's header.
pub fn of_scene(scene: &Scene) -> Vec<Mesh> {
    let mut out = Vec::new();
    let solid = &scene.board;
    let mut outline = Polygon::new(rounded_rect(
        [0.0, 0.0],
        [solid.width, solid.height],
        solid.corner_radius,
        8,
    ));
    for c in &solid.cutouts {
        outline
            .holes
            .push(rounded_rect(xy(c.at.min), xy(c.at.max), c.radius, 6));
    }
    for h in &solid.holes {
        outline.holes.push(slot(
            xy(h.from),
            xy(h.to),
            h.diameter,
            hole_segments(h.diameter),
        ));
    }
    let bottom = -solid.thickness;
    let mask = |top: bool| {
        solid
            .layers
            .iter()
            .find(|l| l.kind == LayerKind::Mask && (l.z_top == 0.0) == top && l.z_bottom != l.z_top)
    };
    let (mask_top, mask_bottom) = (mask(true), mask(false));
    let core_top = mask_top.map_or(0.0, |l| l.z_bottom);
    let core_bottom = mask_bottom.map_or(bottom, |l| l.z_top);
    out.extend(
        Mesh::prism(
            "board",
            Material::Substrate,
            &outline,
            core_bottom,
            core_top,
        )
        .ok(),
    );
    if let Some(l) = mask_top {
        out.extend(Mesh::prism("board", Material::MaskTop, &outline, l.z_bottom, l.z_top).ok());
    }
    if let Some(l) = mask_bottom {
        out.extend(Mesh::prism("board", Material::MaskBottom, &outline, l.z_bottom, l.z_top).ok());
    }
    for h in solid.holes.iter().filter(|h| h.plated) {
        let id = match &h.owner {
            HoleOwner::Pad { path, .. } => path.clone(),
            HoleOwner::Via { net } => net.clone(),
        };
        let ring = slot(xy(h.from), xy(h.to), h.diameter, hole_segments(h.diameter));
        out.extend(Mesh::wall(id, Material::Copper, &ring, bottom, 0.0).ok());
    }
    for b in &scene.bodies {
        body(b, &mut out);
    }
    for limit in &scene.limits {
        for face in &limit.faces {
            let broken = scene
                .findings
                .too_tall
                .iter()
                .any(|t| t.keepout == limit.name && t.side == face.side);
            let material = if broken {
                Material::LimitViolated
            } else {
                Material::Limit
            };
            out.extend(
                Mesh::prism(
                    format!("limit:{}", limit.name),
                    material,
                    &Polygon::new(box_ring(&limit.at)),
                    face.z_bottom,
                    face.z_top,
                )
                .ok(),
            );
        }
    }
    out
}

fn body(b: &Body, out: &mut Vec<Mesh>) {
    let ring = Polygon::new(b.corners.iter().map(|&p| xy(p)).collect());
    let down = b.side == Side::Back;
    match b.z_far {
        Some(far) => {
            let (z0, z1) = if far < b.z_face {
                (far, b.z_face)
            } else {
                (b.z_face, far)
            };
            out.extend(Mesh::prism(b.path.clone(), Material::Body, &ring, z0, z1).ok());
        }
        None => {
            let z = if down {
                b.z_face - UNKNOWN_LIFT
            } else {
                b.z_face + UNKNOWN_LIFT
            };
            out.extend(Mesh::flat(b.path.clone(), Material::BodyUnknown, &ring, z, !down).ok());
        }
    }
    if let Pins::Known {
        at,
        z_top,
        z_bottom,
        ..
    } = &b.pins
    {
        let (z0, z1) = (z_top.min(*z_bottom), z_top.max(*z_bottom));
        out.extend(
            Mesh::prism(
                b.path.clone(),
                Material::Metal,
                &Polygon::new(box_ring(at)),
                z0,
                z1,
            )
            .ok(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_is_a_stadium_with_its_area() {
        let s = slot([0.0, 0.0], [4.0, 0.0], 2.0, 64);
        let p = Polygon::new(s.clone());
        let want = 4.0 * 2.0 + std::f64::consts::PI;
        assert!((p.area() - want).abs() < 0.01, "{}", p.area());
        assert!(
            s.iter()
                .all(|q| q[0] >= -1.0 - 1e-12 && q[0] <= 5.0 + 1e-12)
        );
        assert_eq!(slot([1.0, 1.0], [1.0, 1.0], 1.0, 16).len(), 16);
    }
}
