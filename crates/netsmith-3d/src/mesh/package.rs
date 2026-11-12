//! A part's _Package shape_ as meshes: each solid of [`crate::body3d`] triangulated in the
//! footprint's frame, then put where the part is by its [`Placement`].
//!
//! The solids of a package touch and sit in one another (a mark in the top of the body, a pin
//! through a header's plastic). Where another solid's top is level with the top of a body box and
//! over it, the two faces would lie in one plane and the picture would flicker between them; so
//! the body is drawn up to the bottom of the mark, or at most [`TOP_GAP`] below the other's top.
//! The part's highest point is still its height. This is drawing only: the height limits, the
//! overlaps and every report measure the scene's body box, never these meshes.

use crate::body3d::{Axis, Material as Made, Primitive, Solid};
use crate::mesh::{Material, Mesh, Polygon, circle};
use crate::scene3d::PlacedShape;

/// Drawing only: how far below a level mark or pin a body's top is drawn at most, in mm.
pub const TOP_GAP: f64 = 0.02;

/// The corners round a pin 1 dot or a ball.
pub(crate) const ROUND_SEGMENTS: usize = 16;
/// The bands of a ball from pole to pole.
pub(crate) const BALL_RINGS: usize = 6;

/// How close two heights are to count as level, in mm.
const LEVEL: f64 = 1e-9;

/// The meshes of `shape`, id `id`, in the scene's frame, in the order of its solids.
pub(super) fn meshes(id: &str, shape: &PlacedShape, out: &mut Vec<Mesh>) {
    let place = &shape.place;
    let offset = [place.origin.x, place.origin.y, place.z_face];
    for (k, s) in shape.solids.iter().enumerate() {
        let primitive = settled(&shape.solids, k);
        let Some(mut m) = local(id, material(s.material), &primitive) else {
            continue;
        };
        m.transform(place.linear(), offset);
        out.push(m);
    }
}

/// The mesh material of a package's solid: a ball is metal.
fn material(m: Made) -> Material {
    match m {
        Made::Body => Material::Body,
        Made::Metal | Made::Ball => Material::Metal,
        Made::Mark => Material::Mark,
    }
}

/// Solid `k` as drawn: a body box lowered under whatever is level with its top and over it.
pub(crate) fn settled(solids: &[Solid], k: usize) -> Primitive {
    let s = &solids[k];
    let Primitive::Box { min, max } = s.primitive else {
        return s.primitive.clone();
    };
    if s.material != Made::Body {
        return s.primitive.clone();
    }
    let mut top = max[2];
    for (j, o) in solids.iter().enumerate() {
        let b = o.bounds();
        let over = b.min[0] < max[0] - LEVEL
            && min[0] < b.max[0] - LEVEL
            && b.min[1] < max[1] - LEVEL
            && min[1] < b.max[1] - LEVEL;
        if j != k && over && (b.max[2] - max[2]).abs() < LEVEL {
            top = top.min(b.min[2].max(max[2] - TOP_GAP));
        }
    }
    // Never lower than half the body.
    let top = top.max(min[2] + 0.5 * (max[2] - min[2]));
    Primitive::Box {
        min,
        max: [max[0], max[1], top],
    }
}

/// A primitive triangulated in the footprint's frame, `None` where it has no volume.
fn local(id: &str, material: Material, p: &Primitive) -> Option<Mesh> {
    match p {
        Primitive::Box { min, max } => (0..3)
            .all(|k| min[k] < max[k])
            .then(|| Mesh::axis_box(id, material, *min, *max)),
        Primitive::Prism {
            axis,
            profile,
            from,
            to,
        } => {
            let (lo, hi) = (from.min(*to), from.max(*to));
            let mut m = Mesh::prism(id, material, &Polygon::new(profile.clone()), lo, hi).ok()?;
            // The prism is built across z; its profile's (a, b) and its length go to the axes.
            let linear = match axis {
                Axis::Z => return Some(m),
                // (a, b, along) is (x, z, y).
                Axis::Y => [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]],
                // (a, b, along) is (y, z, x).
                Axis::X => [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            };
            m.transform(linear, [0.0; 3]);
            Some(m)
        }
        Primitive::Cylinder {
            centre,
            radius,
            z0,
            z1,
        } => Mesh::prism(
            id,
            material,
            &Polygon::new(circle([centre.x, centre.y], *radius, ROUND_SEGMENTS)),
            *z0,
            *z1,
        )
        .ok(),
        Primitive::Spheroid {
            centre,
            radius,
            z0,
            z1,
        } => Mesh::spheroid(
            id,
            material,
            [centre.x, centre.y],
            *radius,
            *z0,
            *z1,
            (ROUND_SEGMENTS, BALL_RINGS),
        )
        .ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Pt;

    #[test]
    fn a_body_is_drawn_under_a_level_mark_and_a_lead_across_x_keeps_its_box() {
        let body = Solid {
            material: Made::Body,
            primitive: Primitive::Box {
                min: [-1.0, -0.5, 0.0],
                max: [1.0, 0.5, 1.0],
            },
        };
        let dot = Solid {
            material: Made::Mark,
            primitive: Primitive::Cylinder {
                centre: Pt::new(-0.5, 0.0),
                radius: 0.1,
                z0: 0.98,
                z1: 1.0,
            },
        };
        let solids = [body.clone(), dot];
        assert_eq!(
            settled(&solids, 0),
            Primitive::Box {
                min: [-1.0, -0.5, 0.0],
                max: [1.0, 0.5, 0.98]
            }
        );
        assert_eq!(settled(&solids[..1], 0), body.primitive);
        // A lead's profile across x: (y, z) from x = 0.2 to 0.4.
        let lead = Primitive::Prism {
            axis: Axis::X,
            profile: vec![[1.0, 0.0], [1.5, 0.0], [1.5, 0.1], [1.0, 0.1]],
            from: 0.2,
            to: 0.4,
        };
        let m = local("U1", Material::Metal, &lead).unwrap();
        let b = m.bounds().unwrap();
        assert_eq!((b.min, b.max), ([0.2, 1.0, 0.0], [0.4, 1.5, 0.1]));
    }
}
