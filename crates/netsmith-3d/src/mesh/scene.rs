//! The meshes of a board's [`Scene`]: the one source of triangles that the rasterizer
//! ([`crate::raster3d`]) and the `.glb` writer both draw, so a picture and a file show the same
//! geometry with the same ids.
//!
//! In order:
//!
//! 1. The board, id `board`: the substrate (material `substrate`) between the outer copper
//!    layers (from the bottom of the front copper to the top of the back copper), with the
//!    rounded outline, the cutouts and every drilled hole (round, or a slot) as real holes.
//!    Without copper layers in the stackup it runs from mask to mask, and without mask layers
//!    it is the whole thickness.
//! 2. Each face's solder mask, id `board`, where the stackup has a mask layer: the outline
//!    less the holes, the cutouts and the mask's openings (the scene's copper; none when the
//!    copper cannot be drawn). Over bare laminate it is `mask-top` or `mask-bottom` and fills
//!    from its outer side down to the substrate (the mask's own thickness and the outer
//!    copper's); over copper it is `mask-over-copper`, the mask's own thickness on the copper.
//!    It is opaque: two greens tell copper under it from laminate, as a real board shows them.
//! 3. The plated barrels (`copper`, open walls through the board): a via's id is its net, a
//!    pad's the part's path.
//! 4. The copper, per layer of the scene's copper (top face down) and per net on it (the copper
//!    with no net first, id [`NO_NET`], then by name, id the net's name): its pads, tracks,
//!    pours, planes and via rings joined into one piece, the drilled holes taken out, as a solid
//!    through the layer's thickness at its Z (`copper`). On an outer layer the part the mask
//!    leaves bare is a solid of its own, material `finish`.
//! 5. Each face's silkscreen, id `silk:top` or `silk:bottom`: the legend's ink kept to the
//!    mask (no ink off the board, in a hole or on an opening), flat at [`SILK_LIFT`] off the
//!    mask (material `mark`).
//! 6. Per part, by path, id the part's path (every mesh of a part has it): a body of known
//!    height whose footprint has a package record as its _Package shape_ (the solids of
//!    [`netsmith_pcb::body3d`], each `body`, `metal` or `mark`, a solder ball `metal`, put where the
//!    part is: turned, mirrored and hanging down for a bottom part; see `package.rs`); a part
//!    drawn from its STEP model (ADR 0108) as the model's triangles, one mesh per piece, `body`
//!    with the file's colour where it has one, whatever its height; any other
//!    body of known height as its turned body box from the face out to its far end (`body`); a
//!    body of unknown height as its outline flat at [`UNKNOWN_LIFT`] mm off its face
//!    (`body-unknown`), never a box or a shape (ADR 0105); the pins below the far face where
//!    their protrusion is known (`metal`). Only the picture is shaped: the height limits, the
//!    overlaps and every report measure the body box.
//! 7. Per height limit, by name, id `limit:<name>`: a volume over each face it limits, `limit`,
//!    or `limit-violated` where a part on that face is too tall for it.
//! 8. Per case model of the scene (ADR 0109), id `case:<name>`: one mesh per solid, its
//!    triangles as netsmith's STEP reader made them, placed in this frame, material `case`
//!    (see-through).
//!
//! **No two surfaces share a plane where both show.** From the top face down: the silkscreen at
//! `+SILK_LIFT`, the mask's outer side at 0, the copper's top at the mask's inner side, the
//! substrate's top at the copper's bottom; the bottom face mirrors it. The faces that do meet
//! (a mask's inner side on the copper, the copper's bottom on the substrate) face each other, so
//! one hides the other.
//!
//! The scene's lists are already in a fixed order, and so are these meshes: the same scene gives
//! the same meshes, bit for bit. A piece whose shape has no area (a zero-size box) is left out.

use std::collections::BTreeMap;

use crate::mesh::boolean::{difference, intersection};
use crate::mesh::{Material, Mesh, Polygon, circle, rounded_rect};
use crate::scene3d::{
    BoardSolid, Body, CopperLayer, HoleOwner, Pins, PlacedShape, Plan, PlanArea, PlanBox, Scene,
};
use netsmith_pcb::board::Side;
use netsmith_pcb::stackup::LayerKind;

/// How far off its face the flat outline of a part of unknown height is drawn, in mm.
pub(crate) const UNKNOWN_LIFT: f64 = 0.02;

/// How far off the mask the silkscreen is drawn, in mm: about the ink's thickness, and enough
/// that a viewer's depth buffer tells it from the mask.
pub const SILK_LIFT: f64 = 0.015;

/// A net (`None` for no net) and its copper on one layer.
type NetCopper = (Option<String>, Vec<Polygon>);

/// The id of the copper with no net.
pub const NO_NET: &str = "(no net)";

/// The corners a round hole of `diameter` mm is drawn with.
pub(crate) fn hole_segments(diameter: f64) -> usize {
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
pub(crate) fn slot(a: [f64; 2], b: [f64; 2], diameter: f64, segments: usize) -> Vec<[f64; 2]> {
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

/// The meshes of a scene grouped by what they belong to, each list parallel to the scene's own,
/// so a writer can name every mesh by the scene's ids (the `.glb` writer, [`crate::gltf`]).
/// [`Pieces::into_meshes`] gives them in the order of the module's header, which is
/// [`of_scene`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Pieces {
    /// The substrate (id `board`), when its outline has area.
    pub board: Vec<Mesh>,
    /// The top mask, then the bottom mask, where the stackup has them.
    pub masks: Vec<MaskPieces>,
    /// One per hole of the scene's `board.holes`: the plated barrel, `None` for a hole that is
    /// not plated (or whose wall has no area).
    pub walls: Vec<Option<Mesh>>,
    /// One per body of the scene's `bodies`.
    pub bodies: Vec<BodyPieces>,
    /// One per limit of the scene's `limits`, and in each one per face of its `faces`.
    pub limits: Vec<Vec<Option<Mesh>>>,
    /// One per layer of the scene's copper (none when it cannot be drawn).
    pub copper: Vec<LayerPieces>,
    /// The front's silkscreen, then the back's (none when the copper cannot be drawn).
    pub silk: Vec<SilkPieces>,
    /// One per case model of the scene's `case.models`, and in each one mesh per solid
    /// (`case:<name>`, see-through, ADR 0109).
    pub cases: Vec<Vec<Mesh>>,
}

/// The solder mask of one face: the mask over laminate (`mask-top` or `mask-bottom`) and over
/// copper (`mask-over-copper`), either `None` when it has no area.
#[derive(Debug, Clone, PartialEq)]
pub struct MaskPieces {
    pub side: Side,
    pub bare: Option<Mesh>,
    pub over_copper: Option<Mesh>,
}

/// The copper of one layer, net by net.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerPieces {
    /// `F.Cu`, `In1.Cu`, ..., `B.Cu`.
    pub layer: String,
    /// The copper with no net first, then by name; a net with no copper left once the holes
    /// are drilled is left out.
    pub nets: Vec<NetPieces>,
}

/// One net's copper on one layer.
#[derive(Debug, Clone, PartialEq)]
pub struct NetPieces {
    /// `None` for the copper with no net.
    pub net: Option<String>,
    /// The copper under the mask, or all of it on an inner layer (`copper`).
    pub copper: Option<Mesh>,
    /// The copper the mask leaves bare, on an outer layer (`finish`).
    pub finish: Option<Mesh>,
}

/// The silkscreen of one face, `None` when it has none.
#[derive(Debug, Clone, PartialEq)]
pub struct SilkPieces {
    pub side: Side,
    pub mesh: Option<Mesh>,
}

/// The meshes of one part.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BodyPieces {
    /// The body: its package shape (several meshes, one per solid), a box of known height, or the
    /// flat outline of an unknown one; empty when its outline has no area.
    pub body: Vec<Mesh>,
    /// The pins below the far face, where their protrusion is known.
    pub pins: Option<Mesh>,
}

impl Pieces {
    /// Every mesh, in the order of the module's header.
    pub fn into_meshes(self) -> Vec<Mesh> {
        let mut out = self.board;
        for m in self.masks {
            out.extend(m.bare);
            out.extend(m.over_copper);
        }
        out.extend(self.walls.into_iter().flatten());
        for l in self.copper {
            for n in l.nets {
                out.extend(n.copper);
                out.extend(n.finish);
            }
        }
        out.extend(self.silk.into_iter().filter_map(|s| s.mesh));
        for b in self.bodies {
            // A part's pins come right after its body.
            out.extend(b.body);
            out.extend(b.pins);
        }
        out.extend(self.limits.into_iter().flatten().flatten());
        out.extend(self.cases.into_iter().flatten());
        out
    }
}

/// The meshes of `scene`, in the order of the module's header.
pub fn of_scene(scene: &Scene) -> Vec<Mesh> {
    pieces(scene).into_meshes()
}

/// The meshes of `scene`, grouped as the scene's lists are.
pub fn pieces(scene: &Scene) -> Pieces {
    let mut out = Pieces::default();
    let solid = &scene.board;
    let outline = board_outline(solid);
    let bottom = -solid.thickness;
    let mask = |top: bool| {
        solid
            .layers
            .iter()
            .find(|l| l.kind == LayerKind::Mask && (l.z_top == 0.0) == top && l.z_bottom != l.z_top)
    };
    let (mask_top, mask_bottom) = (mask(true), mask(false));
    let coppers: Vec<_> = solid
        .layers
        .iter()
        .filter(|l| l.kind == LayerKind::Copper)
        .collect();
    let core_top = coppers
        .first()
        .map(|l| l.z_bottom)
        .or(mask_top.map(|l| l.z_bottom))
        .unwrap_or(0.0);
    let core_bottom = coppers
        .last()
        .filter(|_| coppers.len() > 1)
        .map(|l| l.z_top)
        .or(mask_bottom.map(|l| l.z_top))
        .unwrap_or(bottom);
    out.board.extend(
        Mesh::prism(
            "board",
            Material::Substrate,
            &outline,
            core_bottom,
            core_top,
        )
        .ok(),
    );

    let copper = scene.copper.as_ref().ok();
    let drills: Vec<Polygon> = solid
        .holes
        .iter()
        .map(|h| {
            Polygon::new(slot(
                xy(h.from),
                xy(h.to),
                h.diameter,
                hole_segments(h.diameter),
            ))
        })
        .collect();
    let openings = |side: Side| -> Vec<Polygon> {
        copper
            .into_iter()
            .flat_map(|c| c.mask.iter().filter(move |m| m.side == side))
            .flat_map(|m| m.areas.iter().map(polygon))
            .collect()
    };
    // Per copper layer, each net's copper joined, with the holes drilled.
    let mut nets_of: Vec<(String, Vec<NetCopper>)> = Vec::new();
    if let Some(c) = copper {
        for l in &c.layers {
            let nets = layer_nets(l, &c.vias)
                .into_iter()
                .map(|(net, polys)| (net, difference(&polys, &drills)))
                .filter(|(_, polys)| !polys.is_empty())
                .collect();
            nets_of.push((l.name.clone(), nets));
        }
    }
    let n_layers = nets_of.len();
    // Each face's mask: the outline less the openings, split where copper is under it.
    let mut keep_silk: Vec<(Side, Vec<Polygon>)> = Vec::new();
    for (side, layer, material) in [
        (Side::Front, mask_top, Material::MaskTop),
        (Side::Back, mask_bottom, Material::MaskBottom),
    ] {
        let Some(l) = layer else { continue };
        let open = openings(side);
        let region = difference(std::slice::from_ref(&outline), &open);
        let outer = match side {
            Side::Front => nets_of.first(),
            Side::Back => nets_of.last().filter(|_| n_layers > 1),
        };
        let under: Vec<Polygon> = outer
            .map(|(_, nets)| nets.iter().flat_map(|(_, p)| p.iter().cloned()).collect())
            .unwrap_or_default();
        let (bare, over) = if under.is_empty() {
            (region.clone(), Vec::new())
        } else {
            (difference(&region, &under), intersection(&region, &under))
        };
        // Over laminate the mask fills down to the substrate; over copper it is its own
        // thickness on the copper.
        let (bare_z, over_z) = match side {
            Side::Front => ((core_top, l.z_top), (l.z_bottom, l.z_top)),
            Side::Back => ((l.z_bottom, core_bottom), (l.z_bottom, l.z_top)),
        };
        out.masks.push(MaskPieces {
            side,
            bare: solid_of("board", material, &bare, bare_z.0, bare_z.1),
            over_copper: solid_of("board", Material::MaskOverCopper, &over, over_z.0, over_z.1),
        });
        keep_silk.push((side, if copper.is_some() { region } else { Vec::new() }));
    }
    for h in &solid.holes {
        if !h.plated {
            out.walls.push(None);
            continue;
        }
        let id = match &h.owner {
            HoleOwner::Pad { path, .. } => path.clone(),
            HoleOwner::Via { net } => net.clone(),
        };
        let ring = slot(xy(h.from), xy(h.to), h.diameter, hole_segments(h.diameter));
        out.walls
            .push(Mesh::wall(id, Material::Copper, &ring, bottom, 0.0).ok());
    }
    if let Some(c) = copper {
        for (k, (l, (name, nets))) in c.layers.iter().zip(nets_of).enumerate() {
            let face = if k == 0 {
                Some(Side::Front)
            } else if k + 1 == n_layers {
                Some(Side::Back)
            } else {
                None
            };
            let open = face.map(openings).unwrap_or_default();
            let nets = nets
                .into_iter()
                .map(|(net, polys)| {
                    let id = net.as_deref().unwrap_or(NO_NET).to_string();
                    let (covered, bare) = if open.is_empty() {
                        (polys, Vec::new())
                    } else {
                        (difference(&polys, &open), intersection(&polys, &open))
                    };
                    NetPieces {
                        copper: solid_of(&id, Material::Copper, &covered, l.z_bottom, l.z_top),
                        finish: solid_of(&id, Material::Finish, &bare, l.z_bottom, l.z_top),
                        net,
                    }
                })
                .collect();
            out.copper.push(LayerPieces { layer: name, nets });
        }
        for s in &c.silk {
            let region = keep_silk
                .iter()
                .find(|(side, _)| *side == s.side)
                .map(|(_, r)| r.clone())
                .unwrap_or_else(|| vec![outline.clone()]);
            let ink: Vec<Polygon> = s.areas.iter().map(polygon).collect();
            let kept = intersection(&ink, &region);
            let (id, z, up) = match s.side {
                Side::Front => ("silk:top", s.z + SILK_LIFT, true),
                Side::Back => ("silk:bottom", s.z - SILK_LIFT, false),
            };
            out.silk.push(SilkPieces {
                side: s.side,
                mesh: flat_of(id, Material::Mark, &kept, z, up),
            });
        }
    }
    out.bodies = scene.bodies.iter().map(body).collect();
    for limit in &scene.limits {
        let faces = limit
            .faces
            .iter()
            .map(|face| {
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
                Mesh::prism(
                    format!("limit:{}", limit.name),
                    material,
                    &Polygon::new(box_ring(&limit.at)),
                    face.z_bottom,
                    face.z_top,
                )
                .ok()
            })
            .collect();
        out.limits.push(faces);
    }
    out.cases = scene
        .case
        .models
        .iter()
        .map(crate::case::PlacedCase::meshes)
        .collect();
    out
}

/// The board's outline with its cutouts and every drilled hole (round, or a slot) as holes.
pub(super) fn board_outline(solid: &BoardSolid) -> Polygon {
    let mut outline = Polygon::new(if solid.outline.is_empty() {
        rounded_rect(
            [0.0, 0.0],
            [solid.width, solid.height],
            solid.corner_radius,
            8,
        )
    } else {
        solid.outline.iter().map(|&p| xy(p)).collect()
    });
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
    outline
}

fn polygon(a: &PlanArea) -> Polygon {
    Polygon {
        outer: a.outer.iter().map(|&p| xy(p)).collect(),
        holes: a
            .holes
            .iter()
            .map(|h| h.iter().map(|&p| xy(p)).collect())
            .collect(),
    }
}

/// The copper of a layer by net, the copper with no net first: pads, tracks, pours (or planes),
/// the rings of the vias, and the footprints' copper drawings (no net).
fn layer_nets(
    l: &CopperLayer,
    vias: &[crate::scene3d::Via],
) -> BTreeMap<Option<String>, Vec<Polygon>> {
    let mut nets: BTreeMap<Option<String>, Vec<Polygon>> = BTreeMap::new();
    for p in &l.pads {
        nets.entry(p.net.clone())
            .or_default()
            .extend(p.areas.iter().map(polygon));
    }
    for t in &l.tracks {
        nets.entry(t.net.clone())
            .or_default()
            .push(polygon(&t.area));
    }
    for p in &l.pours {
        nets.entry(p.net.clone())
            .or_default()
            .push(polygon(&p.area));
    }
    for v in vias {
        nets.entry(v.net.clone())
            .or_default()
            .push(Polygon::new(circle(
                xy(v.at),
                0.5 * v.diameter,
                hole_segments(v.diameter),
            )));
    }
    if !l.drawings.is_empty() {
        nets.entry(None)
            .or_default()
            .extend(l.drawings.iter().map(polygon));
    }
    nets
}

/// `polygons` extruded from `z0` to `z1` as one mesh; `None` when none has area.
fn solid_of(id: &str, material: Material, polygons: &[Polygon], z0: f64, z1: f64) -> Option<Mesh> {
    join(
        id,
        material,
        polygons
            .iter()
            .filter_map(|p| Mesh::prism(id, material, p, z0, z1).ok()),
    )
}

/// `polygons` flat at `z`, facing up or down, as one mesh; `None` when none has area.
fn flat_of(id: &str, material: Material, polygons: &[Polygon], z: f64, up: bool) -> Option<Mesh> {
    join(
        id,
        material,
        polygons
            .iter()
            .filter_map(|p| Mesh::flat(id, material, p, z, up).ok()),
    )
}

fn join(id: &str, material: Material, meshes: impl Iterator<Item = Mesh>) -> Option<Mesh> {
    let mut out: Option<Mesh> = None;
    for m in meshes {
        match &mut out {
            None => out = Some(m),
            Some(o) => o.append(&m),
        }
    }
    out.filter(|m| !m.indices.is_empty()).map(|mut m| {
        m.id = id.to_string();
        m.material = material;
        m
    })
}

fn body(b: &Body) -> BodyPieces {
    let ring = Polygon::new(b.corners.iter().map(|&p| xy(p)).collect());
    let down = b.side == Side::Back;
    let mut body = Vec::new();
    match (b.z_far, &b.shape) {
        // A part drawn from its STEP model, whatever its height (ADR 0108).
        _ if b.model.is_some() => {
            body.extend(b.model.iter().flat_map(|m| m.meshes.iter().cloned()));
        }
        (Some(_), Some(shape)) => super::package::meshes(&b.path, shape, &mut body),
        (Some(far), None) => {
            let (z0, z1) = if far < b.z_face {
                (far, b.z_face)
            } else {
                (b.z_face, far)
            };
            body.extend(Mesh::prism(b.path.clone(), Material::Body, &ring, z0, z1).ok());
        }
        (None, _) => {
            let z = if down {
                b.z_face - UNKNOWN_LIFT
            } else {
                b.z_face + UNKNOWN_LIFT
            };
            body.extend(Mesh::flat(b.path.clone(), Material::BodyUnknown, &ring, z, !down).ok());
        }
    }
    let pins = match &b.pins {
        // Each pin of a header or socket, through the board and out of its far face.
        Pins::Known { tails, .. } if !tails.is_empty() => {
            let shape = PlacedShape {
                solids: tails.clone(),
                place: b.place,
            };
            let mut meshes = Vec::new();
            super::package::meshes(&b.path, &shape, &mut meshes);
            join(&b.path, Material::Metal, meshes.into_iter())
        }
        Pins::Known {
            at,
            z_top,
            z_bottom,
            ..
        } => {
            let (z0, z1) = (z_top.min(*z_bottom), z_top.max(*z_bottom));
            Mesh::prism(
                b.path.clone(),
                Material::Metal,
                &Polygon::new(box_ring(at)),
                z0,
                z1,
            )
            .ok()
        }
        _ => None,
    };
    BodyPieces { body, pins }
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
