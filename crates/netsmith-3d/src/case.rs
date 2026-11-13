//! A board's _Case model_ in its scene, and the fit check of the board against it (ADR 0109).
//!
//! A case model is a STEP file the project names in the `case { }` section of the board's
//! physical file (`std::pcb::CaseModel`). netsmith's reader (ADR 0108, [`crate::step::read`])
//! turns it into triangles in the file's frame; [`place`] turns and moves them into the scene's
//! frame (`.rx`, `.ry`, `.rz` about x, then y, then z, in degrees, then the offset `.x`, `.y`,
//! `.z`), one [`CaseSolid`] per solid of the file. [`check`] measures the board against them.
//!
//! # What is measured
//!
//! What the height checks measure (ADR 0106): each part's _Body_ box (its four turned corners in
//! plan, from its face out to its height, stated, catalog or standard) and the board's own
//! solid (its outline with its rounded corners, through its thickness, less its cutouts and
//! holes), and the pins of a through-hole part that the design states stand out of the far face
//! (`.pin_protrusion`): the box of its plated pads in plan, from that face out by the stated
//! length, as the scene's `protrusions` draw them. Pins of no stated protrusion are not
//! measured (how far they stand out is unknown, not zero), and a part's drawn `.model` is not
//! read. A body of height zero (a mounting hole, a fiducial: "flat") has no volume and is not
//! measured.
//!
//! # The findings, each with its numbers, the part and the solid
//!
//! - `case_collision`: a body or the board inside a case solid by more than [`CONTACT`]
//!   (0.01 mm, the reader's chord). Touching is contact, not a collision: a board resting on a
//!   boss is fine. The depth is how far the case's surface goes in past the body's nearest
//!   face (the least move, along a face's normal, that would part them); a body wholly inside a
//!   solid has no surface in it and is found by ray parity.
//!   An error that stops `fab` for a stated or a catalog height, for stated pins and for the
//!   board.
//! - `case_clearance`: a body nearer a case surface than the case model's `.clearance`. The gap
//!   is measured to the body box's faces grown outward, so near an edge or a corner of the box
//!   it is at most the true distance (a warning a little early, never late).
//! - `case_reach`: a _Connector_ whose body stands at an edge of the board (its box within
//!   [`AT_EDGE`] of the outline's edge, or past it), with case material between the side of its
//!   box toward that edge (its face) and the outside, or that face more than `.reach` inside the
//!   wall's outer face. The face is the side of the box toward the nearest edge: a connector
//!   that mates from above (a pin header at an edge) is measured the same way.
//! - `case_boss`: a mounting hole (`MountingHole`, `PlatedHole`) whose ring (its body's box in
//!   plan) has no case solid within `.boss_gap` above the top face or below the bottom face.
//! - `case_unchecked`: a part of unknown height with case material above it (on its face, out
//!   to the case's far end), a part of standard height that collides (a part in that package
//!   may be lower, ADR 0106), and a solid whose shell is not closed, which gets no test for a
//!   body or the board lying wholly inside it (its surface is still measured): a warning, an
//!   error with strict constraints.
//!
//! # How
//!
//! Triangles, not exact solids (ADR 0109): a triangle meets a convex volume (a body box, the
//! board's outline prism, a region near a hole) when clipping it to the volume's planes leaves
//! anything. Each solid's box and each triangle's box are a prefilter, so a body far from the
//! case reads none of its triangles. A point is inside a closed solid when a ray from it crosses
//! the surface an odd number of times; a ray that grazes an edge is cast again another way.
//! Everything is in a fixed order and every reported length is rounded to a nanometre, so the
//! same scene and model give the same findings on every machine.

use std::fmt;

use netsmith_core::CaseFigures;
use netsmith_pcb::board::Side;

use crate::mesh::{Aabb, Material, Mesh, rounded_rect, trig};
use crate::scene3d::{BoardSolid, Body, HeightProvenance, HoleOwner, PartCategory, Pins, Plan};
use crate::step::read::StepMesh;

/// How far a case solid may reach into a body or the board and still be contact, in mm: the
/// reader's chord tolerance (ADR 0109).
pub const CONTACT: f64 = 0.01;

/// How near the outline's edge a connector's body box must come to be "at the edge", in mm.
pub const AT_EDGE: f64 = 1.0;

/// How far around a connector's face the wall's outer face is looked for, in mm.
pub const REACH_LOOK: f64 = 3.0;

/// How far out from a connector's face a wall is looked for, in mm.
pub const REACH_RAY: f64 = 25.0;

/// A triangle's three corners, in mm.
pub type Triangle = [[f64; 3]; 3];

/// One solid of a case model, placed in the scene's frame.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseSolid {
    /// The solid's name in the file, or `#id` when it has none.
    pub name: String,
    /// The triangles, in mm in the scene's frame, each counterclockwise seen from outside.
    pub triangles: Vec<[[f64; 3]; 3]>,
    /// Whether every shell of the solid is closed, so that a point can be inside it.
    pub closed: bool,
    pub bounds: Aabb,
    /// The box of each triangle, for the prefilter.
    boxes: Vec<Aabb>,
}

impl CaseSolid {
    /// A solid of `triangles` (in the scene's frame).
    pub fn new(name: impl Into<String>, triangles: Vec<[[f64; 3]; 3]>, closed: bool) -> CaseSolid {
        let boxes: Vec<Aabb> = triangles
            .iter()
            .map(|t| Aabb::of(t).expect("three corners"))
            .collect();
        let bounds = boxes.iter().copied().reduce(Aabb::union).unwrap_or(Aabb {
            min: [0.0; 3],
            max: [0.0; 3],
        });
        CaseSolid {
            name: name.into(),
            triangles,
            closed,
            bounds,
            boxes,
        }
    }

    /// The triangles whose boxes meet `b`.
    fn near<'a>(&'a self, b: &'a Aabb) -> impl Iterator<Item = &'a [[f64; 3]; 3]> + 'a {
        self.triangles
            .iter()
            .zip(&self.boxes)
            .filter(move |(_, t)| meets(t, b))
            .map(|(t, _)| t)
    }
}

/// A case model read and placed.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedCase {
    /// The member's name (`shell`).
    pub name: String,
    /// The file as the design states it.
    pub file: String,
    /// The SHA-256 of the file's bytes, in hex.
    pub sha256: String,
    pub figures: CaseFigures,
    /// In the order of the file.
    pub solids: Vec<CaseSolid>,
}

impl PlacedCase {
    pub fn triangle_count(&self) -> usize {
        self.solids.iter().map(|s| s.triangles.len()).sum()
    }

    /// The box round every solid, `None` for a case with no triangles.
    pub fn bounds(&self) -> Option<Aabb> {
        self.solids
            .iter()
            .filter(|s| !s.triangles.is_empty())
            .map(|s| s.bounds)
            .reduce(Aabb::union)
    }
}

impl PlacedCase {
    /// One mesh per solid, its id `case:<name>` (what `render --highlight` names), material
    /// [`Material::Case`], each triangle shaded flat; closed when the solid is.
    pub fn meshes(&self) -> Vec<Mesh> {
        self.solids
            .iter()
            .filter(|s| !s.triangles.is_empty())
            .map(|s| {
                let mut mesh = Mesh::new(format!("case:{}", self.name), Material::Case);
                let mut normals = Vec::with_capacity(3 * s.triangles.len());
                for t in &s.triangles {
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
                    let n = if len > 0.0 {
                        n.map(|c| c / len)
                    } else {
                        [0.0, 0.0, 1.0]
                    };
                    let base = mesh.positions.len() as u32;
                    mesh.positions.extend_from_slice(t);
                    normals.extend([n; 3]);
                    mesh.indices.extend([base, base + 1, base + 2]);
                }
                mesh.normals = Some(normals);
                mesh.closed = s.closed;
                mesh
            })
            .collect()
    }
}

/// What a scene holds of its case models: each model read and placed, and what the fit check
/// found, the files not read first.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CaseScene {
    pub models: Vec<PlacedCase>,
    pub findings: Vec<FitFinding>,
}

/// The map of a case model's `.rx`, `.ry`, `.rz` (degrees, about x, then y, then z) and
/// offset: the rows of the linear part and the offset.
pub fn placement(rotation: [f64; 3], offset: [f64; 3]) -> ([[f64; 3]; 3], [f64; 3]) {
    let turn = |deg: f64| trig::sin_cos(trig::radians(deg));
    let (sx, cx) = turn(rotation[0]);
    let (sy, cy) = turn(rotation[1]);
    let (sz, cz) = turn(rotation[2]);
    let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    (mul(rz, mul(ry, rx)), offset)
}

fn mul(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    [0, 1, 2].map(|r| [0, 1, 2].map(|c| (0..3).map(|k| a[r][k] * b[k][c]).sum()))
}

/// A length to a nanometre, with no negative zero.
fn nm(x: f64) -> f64 {
    let r = (x * 1e6).round() / 1e6;
    if r == 0.0 { 0.0 } else { r }
}

/// The case model `name` (the file `file`, of hash `sha256`, read as `model`) turned by
/// `rotation` and moved by `offset` into the scene's frame. The pieces of one solid (the reader
/// splits a solid by colour) are one [`CaseSolid`]; a solid is closed when all of them are.
pub fn place(
    name: &str,
    file: &str,
    sha256: &str,
    figures: CaseFigures,
    model: &StepMesh,
    rotation: [f64; 3],
    offset: [f64; 3],
) -> PlacedCase {
    let (m, d) = placement(rotation, offset);
    let apply = |p: [f64; 3]| {
        [0, 1, 2].map(|r| nm(m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + d[r]))
    };
    // A turn never mirrors, so each triangle keeps the reader's order and still faces out.
    let mut solids: Vec<(String, Vec<Triangle>, bool)> = Vec::new();
    for piece in &model.pieces {
        let mesh = &piece.mesh;
        let triangles = mesh
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| t.map(|i| apply(mesh.positions[i as usize])));
        match solids.last_mut() {
            Some((n, tris, closed)) if *n == mesh.id => {
                tris.extend(triangles);
                *closed &= mesh.closed;
            }
            _ => solids.push((mesh.id.clone(), triangles.collect(), mesh.closed)),
        }
    }
    PlacedCase {
        name: name.to_string(),
        file: file.to_string(),
        sha256: sha256.to_string(),
        figures,
        solids: solids
            .into_iter()
            .map(|(n, t, c)| CaseSolid::new(n, t, c))
            .collect(),
    }
}

/// The finding's name, as `board3d --json`, the notes and the editor write it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FitKind {
    Collision,
    Clearance,
    Reach,
    Boss,
    Unchecked,
}

impl FitKind {
    pub fn word(self) -> &'static str {
        match self {
            FitKind::Collision => "case_collision",
            FitKind::Clearance => "case_clearance",
            FitKind::Reach => "case_reach",
            FitKind::Boss => "case_boss",
            FitKind::Unchecked => "case_unchecked",
        }
    }
}

/// What a finding is about.
#[derive(Debug, Clone, PartialEq)]
pub enum Subject {
    /// A part's body, by path and designator.
    Part { path: String, refdes: String },
    /// A part's pins below the far face, which the design states stand out by `length` mm.
    Pins {
        path: String,
        refdes: String,
        length: f64,
    },
    /// The board's own solid.
    Board,
    /// A solid of the case model (`case_unchecked` of a shell that is not closed).
    Solid,
    /// The case model as a whole (a file that is not there or cannot be read).
    Model,
}

/// What a finding found, with its numbers in mm.
#[derive(Debug, Clone, PartialEq)]
pub enum Why {
    /// A body or the board in a solid: by `depth`, or wholly inside it (`None`). `height` and
    /// `provenance` are the body's (`None` for the board).
    Collision {
        depth: Option<f64>,
        height: Option<f64>,
        provenance: Option<HeightProvenance>,
    },
    /// A body nearer a surface than `least`.
    Clearance { gap: f64, least: f64 },
    /// A connector at an edge with case material `distance` out from its face.
    ReachBlocked { edge: &'static str, distance: f64 },
    /// A connector at an edge whose face is `depth` inside the wall's outer face.
    ReachDeep {
        edge: &'static str,
        depth: f64,
        most: f64,
    },
    /// A mounting hole with no solid within `gap` of its ring.
    Boss { gap: f64 },
    /// A part of unknown height with case material over it.
    UnknownHeight,
    /// A part of standard height in a solid by `depth` (or wholly inside, `None`).
    Standard {
        depth: Option<f64>,
        height: f64,
        standard: String,
    },
    /// The solid's shell is not closed.
    OpenShell,
    /// The file is not there: the message `models::locate` gives.
    Missing { message: String },
    /// The file is there and netsmith's reader cannot read it: the message.
    Unreadable { message: String },
}

/// One finding of the fit check.
#[derive(Debug, Clone, PartialEq)]
pub struct FitFinding {
    pub kind: FitKind,
    /// The case model's name, `base` (several joined with `, ` for a `case_boss`).
    pub case: String,
    /// The case solid's name; empty when the finding is about no one solid.
    pub solid: String,
    pub subject: Subject,
    pub why: Why,
    /// Where to look, in mm in the scene's frame: the deepest point, the nearest, the face.
    pub at: [f64; 3],
}

/// How a finding stops `fab` (ADR 0109).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    /// An error that stops `fab`: `--allow-drc-violations` and `--constraints warn` never waive
    /// it.
    Error,
    /// A warning, and an error when constraints are strict.
    Unchecked,
    /// A warning.
    Warning,
}

impl FitFinding {
    pub fn weight(&self) -> Weight {
        match (&self.kind, &self.why) {
            (FitKind::Collision, _) => Weight::Error,
            // A file that names a case and cannot be read is a fault of the project's input, as
            // a path outside it is: it stops `fab` (the conservative reading of ADR 0109, which
            // names only a file that is not there).
            (FitKind::Unchecked, Why::Unreadable { .. }) => Weight::Error,
            (FitKind::Unchecked, _) => Weight::Unchecked,
            _ => Weight::Warning,
        }
    }

    /// The severity for a run whose constraints are strict or not: `error`, or `warning`.
    pub fn severity(&self, strict: bool) -> &'static str {
        match self.weight() {
            Weight::Error => "error",
            Weight::Unchecked if strict => "error",
            _ => "warning",
        }
    }

    /// The part's path, if it is about a part.
    pub fn path(&self) -> Option<&str> {
        match &self.subject {
            Subject::Part { path, .. } | Subject::Pins { path, .. } => Some(path),
            _ => None,
        }
    }
}

fn mm(x: f64) -> String {
    format!("{x:.2} mm")
}

fn point(p: [f64; 3]) -> String {
    format!("({:.2}, {:.2}, {:.2})", p[0], p[1], p[2])
}

impl fmt::Display for FitFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let who = match &self.subject {
            Subject::Part { path, refdes } => format!("{refdes} (`{path}`)"),
            Subject::Pins {
                path,
                refdes,
                length,
            } => format!(
                "the pins of {refdes} (`{path}`), stated to stand {} out of the far face,",
                mm(*length)
            ),
            Subject::Board => "the board".to_string(),
            Subject::Solid | Subject::Model => String::new(),
        };
        let solid = format!(
            "the solid `{}` of the case model `{}`",
            self.solid, self.case
        );
        // Pins are many.
        let (is, lies) = match self.subject {
            Subject::Pins { .. } => ("are", "lie"),
            _ => ("is", "lies"),
        };
        match &self.why {
            Why::Collision {
                depth,
                height,
                provenance,
            } => {
                let body = match (height, provenance) {
                    (Some(h), Some(p)) => format!(", {} tall ({}),", mm(*h), p.word()),
                    _ => String::new(),
                };
                match depth {
                    Some(d) => write!(
                        f,
                        "{who}{body} {is} inside {solid} by {} at {}: the case and the board collide",
                        mm(*d),
                        point(self.at)
                    ),
                    None => write!(
                        f,
                        "{who}{body} {lies} wholly inside {solid}, at {}: the case and the board collide",
                        point(self.at)
                    ),
                }
            }
            Why::Clearance { gap, least } => write!(
                f,
                "{who} {is} {} from {solid} at {}, less than `{}.clearance`, {}",
                mm(*gap),
                point(self.at),
                self.case,
                mm(*least)
            ),
            Why::ReachBlocked { edge, distance } => write!(
                f,
                "{who}, a connector at the board's {edge} edge, has {solid} {} out from its face, between the face and the outside: a plug cannot reach it",
                mm(*distance)
            ),
            Why::ReachDeep { edge, depth, most } => write!(
                f,
                "{who}, a connector at the board's {edge} edge, stands {} inside the outer face of the wall of the case model `{}`, more than `{}.reach`, {}: a plug may not seat",
                mm(*depth),
                self.case,
                self.case,
                mm(*most)
            ),
            Why::Boss { gap } => write!(
                f,
                "the mounting hole {who} has no case solid (`{}`) within {} above or below its ring: no boss or post holds the board there",
                self.case,
                mm(*gap)
            ),
            Why::UnknownHeight => write!(
                f,
                "{who} has no height and stands under {solid}, so its fit in the case is not checked: state its `.height`, from the datasheet"
            ),
            Why::Standard {
                depth,
                height,
                standard,
            } => {
                let inside = match depth {
                    Some(d) => format!("is inside {solid} by {}", mm(*d)),
                    None => format!("lies wholly inside {solid}"),
                };
                write!(
                    f,
                    "{who}, at its package standard's greatest height of {} ({standard}), {inside} at {}; a part in that package may be lower, so its fit is not checked: state its `.height`, from the datasheet",
                    mm(*height),
                    point(self.at)
                )
            }
            Why::OpenShell => write!(
                f,
                "{solid} is not a closed shell, so no part and not the board is tested for lying wholly inside it (its surface is still measured)"
            ),
            Why::Missing { message } | Why::Unreadable { message } => f.write_str(message),
        }
    }
}

/// A plane: a point `p` is on the inside when `n . p <= d`.
#[derive(Debug, Clone, Copy)]
struct Plane {
    n: [f64; 3],
    d: f64,
    /// A side shared with another piece of the same volume (a diagonal of a polygon outline cut
    /// into triangles): it is never moved by a clip's `grow`, and a depth is not measured to it.
    inner: bool,
}

/// A convex volume: the planes that bound it.
#[derive(Debug, Clone)]
struct Convex {
    planes: Vec<Plane>,
    bounds: Aabb,
    /// The mean of the ring's corners, half-way up: inside, as the ring is convex.
    centre: [f64; 3],
}

impl Convex {
    /// The prism of the convex ring `ring` (either turn) from `z0` up to `z1`.
    fn prism(ring: &[[f64; 2]], z0: f64, z1: f64) -> Convex {
        Convex::prism_with(ring, &vec![false; ring.len()], z0, z1)
    }

    /// [`Convex::prism`], where `inner[i]` says the side from corner `i` to the next is shared
    /// with another piece.
    fn prism_with(ring: &[[f64; 2]], inner: &[bool], z0: f64, z1: f64) -> Convex {
        let area: f64 = (0..ring.len())
            .map(|i| {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum();
        let turn = if area < 0.0 { -1.0 } else { 1.0 };
        let mut planes = Vec::with_capacity(ring.len() + 2);
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
            let len = (ex * ex + ey * ey).sqrt();
            if len < 1e-12 {
                continue;
            }
            let n = [turn * ey / len, -turn * ex / len, 0.0];
            planes.push(Plane {
                n,
                d: n[0] * a[0] + n[1] * a[1],
                inner: inner[i],
            });
        }
        planes.push(Plane {
            n: [0.0, 0.0, 1.0],
            d: z1,
            inner: false,
        });
        planes.push(Plane {
            n: [0.0, 0.0, -1.0],
            d: -z0,
            inner: false,
        });
        let mut bounds = Aabb {
            min: [ring[0][0], ring[0][1], z0],
            max: [ring[0][0], ring[0][1], z1],
        };
        for p in ring {
            bounds.grow([p[0], p[1], z0]);
        }
        let k = ring.len() as f64;
        let centre = [
            ring.iter().map(|p| p[0]).sum::<f64>() / k,
            ring.iter().map(|p| p[1]).sum::<f64>() / k,
            0.5 * (z0 + z1),
        ];
        Convex {
            planes,
            bounds,
            centre,
        }
    }

    /// The box of `b` in plan from `z0` to `z1`.
    fn plan_box(min: Plan, max: Plan, z0: f64, z1: f64) -> Convex {
        Convex::prism(
            &[
                [min.x, min.y],
                [max.x, min.y],
                [max.x, max.y],
                [min.x, max.y],
            ],
            z0,
            z1,
        )
    }

    /// The prefilter's box of the volume grown by `by` (shrunk for a negative one).
    fn bounds_grown(&self, by: f64) -> Aabb {
        let by = by.max(0.0);
        Aabb {
            min: self.bounds.min.map(|c| c - by),
            max: self.bounds.max.map(|c| c + by),
        }
    }

    /// The part of the triangle `t` inside the volume with every plane moved out by `grow`
    /// (in by a negative one), or `None` when nothing is left.
    fn clip(&self, t: &[[f64; 3]; 3], grow: f64) -> Option<Vec<[f64; 3]>> {
        let mut poly: Vec<[f64; 3]> = t.to_vec();
        let mut next: Vec<[f64; 3]> = Vec::with_capacity(8);
        for p in &self.planes {
            let g = if p.inner { 0.0 } else { grow };
            let side = |q: &[f64; 3]| p.n[0] * q[0] + p.n[1] * q[1] + p.n[2] * q[2] - (p.d + g);
            next.clear();
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                let (sa, sb) = (side(&a), side(&b));
                if sa <= 0.0 {
                    next.push(a);
                }
                if (sa < 0.0 && sb > 0.0) || (sa > 0.0 && sb < 0.0) {
                    let k = sa / (sa - sb);
                    next.push([0, 1, 2].map(|c| a[c] + k * (b[c] - a[c])));
                }
            }
            std::mem::swap(&mut poly, &mut next);
            if poly.is_empty() {
                return None;
            }
        }
        Some(poly)
    }

    /// A point inside: the mean of the ring's corners, half-way up.
    fn centre(&self) -> [f64; 3] {
        self.centre
    }
}

fn meets(a: &Aabb, b: &Aabb) -> bool {
    (0..3).all(|k| a.min[k] <= b.max[k] && b.min[k] <= a.max[k])
}

/// How far `solid`'s surface reaches into `volume`, more than [`CONTACT`] in, and the point
/// that reaches farthest; `None` when no triangle reaches that far in. The depth is the least,
/// over the volume's faces, of how far the surface goes in past that face: how far the two
/// must move apart along the best of those directions (a lid 1 mm below a part's top is 1 mm
/// in, however wide it is). `skip` leaves out a piece of surface (the part of a triangle in a
/// hole of the board).
fn deepest(
    volume: &Convex,
    solid: &CaseSolid,
    skip: &dyn Fn(&[[f64; 3]]) -> bool,
) -> Option<([f64; 3], f64)> {
    let b = volume.bounds;
    // Per face of the volume: the farthest in past it, and the point.
    let mut past: Vec<Option<([f64; 3], f64)>> = vec![None; volume.planes.len()];
    for t in solid.near(&b) {
        let Some(poly) = volume.clip(t, -CONTACT) else {
            continue;
        };
        if skip(&poly) {
            continue;
        }
        for q in poly {
            for (slot, p) in past.iter_mut().zip(&volume.planes) {
                if p.inner {
                    continue;
                }
                let d = p.d - (p.n[0] * q[0] + p.n[1] * q[1] + p.n[2] * q[2]);
                if slot.is_none_or(|(_, bd)| d > bd) {
                    *slot = Some((q, d));
                }
            }
        }
    }
    past.into_iter()
        .flatten()
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// Whether `q` is inside the closed `solid`: the surface crossed an odd number of times by a
/// ray from it. A ray that passes within a nanometre of an edge is cast another way.
fn inside(solid: &CaseSolid, q: [f64; 3]) -> bool {
    if !solid.closed || !(0..3).all(|k| solid.bounds.min[k] <= q[k] && q[k] <= solid.bounds.max[k])
    {
        return false;
    }
    // Unit directions with no zero part, so no ray runs along an axis-aligned face.
    const DIRECTIONS: [[f64; 3]; 3] = [[0.48, 0.36, 0.8], [0.36, -0.8, 0.48], [-0.8, 0.48, 0.36]];
    let mut count = 0;
    for dir in DIRECTIONS {
        count = 0;
        let mut grazed = false;
        for t in &solid.triangles {
            match ray_hit(q, dir, t) {
                Hit::Through(_) => count += 1,
                Hit::Edge => {
                    grazed = true;
                    break;
                }
                Hit::Miss => {}
            }
        }
        if !grazed {
            break;
        }
    }
    count % 2 == 1
}

enum Hit {
    Miss,
    /// At this distance along the ray.
    Through(f64),
    /// Within a nanometre of the triangle's edge, or of the ray's start.
    Edge,
}

/// Möller and Trumbore's test of a ray from `o` along `dir` against the triangle `t`.
fn ray_hit(o: [f64; 3], dir: [f64; 3], t: &[[f64; 3]; 3]) -> Hit {
    const EPS: f64 = 1e-9;
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (e1, e2) = (sub(t[1], t[0]), sub(t[2], t[0]));
    let p = cross(dir, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-15 {
        return Hit::Miss;
    }
    let inv = 1.0 / det;
    let s = sub(o, t[0]);
    let u = dot(s, p) * inv;
    if !(-EPS..=1.0 + EPS).contains(&u) {
        return Hit::Miss;
    }
    let qv = cross(s, e1);
    let v = dot(dir, qv) * inv;
    if v < -EPS || u + v > 1.0 + EPS {
        return Hit::Miss;
    }
    let along = dot(e2, qv) * inv;
    if along < -EPS {
        return Hit::Miss;
    }
    if u < EPS || v < EPS || u + v > 1.0 - EPS || along < EPS {
        return Hit::Edge;
    }
    Hit::Through(along)
}

/// The distances along a ray from `o` along `dir` (out to `length`) at which it meets a
/// triangle of `solid`, grazing hits included.
fn ray_hits(solid: &CaseSolid, o: [f64; 3], dir: [f64; 3], length: f64) -> Vec<f64> {
    let end = [0, 1, 2].map(|k| o[k] + dir[k] * length);
    let mut b = Aabb { min: o, max: o };
    b.grow(end);
    let mut out = Vec::new();
    for t in solid.near(&b) {
        match ray_hit(o, dir, t) {
            Hit::Through(s) if s <= length => out.push(s),
            Hit::Edge => {
                // A graze is a hit at the distance the plane of the triangle is at.
                let n = {
                    let (a, b2, c) = (t[0], t[1], t[2]);
                    let (u, v) = (
                        [b2[0] - a[0], b2[1] - a[1], b2[2] - a[2]],
                        [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
                    );
                    [
                        u[1] * v[2] - u[2] * v[1],
                        u[2] * v[0] - u[0] * v[2],
                        u[0] * v[1] - u[1] * v[0],
                    ]
                };
                let den = n[0] * dir[0] + n[1] * dir[1] + n[2] * dir[2];
                if den.abs() > 1e-15 {
                    let s = (n[0] * (t[0][0] - o[0])
                        + n[1] * (t[0][1] - o[1])
                        + n[2] * (t[0][2] - o[2]))
                        / den;
                    if (0.0..=length).contains(&s) {
                        out.push(s);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// A body's box as a volume, from its lowest to its highest z.
fn body_volume(b: &Body, z0: f64, z1: f64) -> Convex {
    let ring: Vec<[f64; 2]> = b.corners.iter().map(|c| [c.x, c.y]).collect();
    Convex::prism(&ring, z0, z1)
}

/// The fit of the board (`board`, its parts' `bodies` and its holes) in its case models.
/// Findings are in the order: per case model, its open shells; the board; per body by path,
/// per case model, a collision (or an unknown height, a standard height in a solid), else a
/// clearance, and then its stated pins the same way; the connectors' reach; the mounting holes.
pub fn check(board: &BoardSolid, bodies: &[Body], cases: &[PlacedCase]) -> Vec<FitFinding> {
    let mut out = Vec::new();
    if cases.is_empty() {
        return out;
    }
    for case in cases {
        for s in case.solids.iter().filter(|s| !s.closed) {
            out.push(FitFinding {
                kind: FitKind::Unchecked,
                case: case.name.clone(),
                solid: s.name.clone(),
                subject: Subject::Solid,
                why: Why::OpenShell,
                at: s.bounds.centre().map(nm),
            });
        }
    }
    out.extend(board_collisions(board, cases));
    let extent = cases
        .iter()
        .filter_map(PlacedCase::bounds)
        .reduce(Aabb::union);
    for b in bodies {
        out.extend(body_fit(b, cases, extent));
        out.extend(pins_fit(b, cases));
    }
    for b in bodies
        .iter()
        .filter(|b| b.category == PartCategory::Connector)
    {
        out.extend(reach(board, b, cases));
    }
    out.extend(bosses(board, bodies, cases));
    out
}

/// Whether the plan point `p` is in a hole or a cutout of the board, grown by [`CONTACT`].
fn in_opening(board: &BoardSolid, p: [f64; 3]) -> bool {
    let cut = board.cutouts.iter().any(|c| {
        p[0] >= c.at.min.x - CONTACT
            && p[0] <= c.at.max.x + CONTACT
            && p[1] >= c.at.min.y - CONTACT
            && p[1] <= c.at.max.y + CONTACT
    });
    cut || board.holes.iter().any(|h| {
        // The distance from the hole's axis (a point, or a slot's segment).
        let (ax, ay, bx, by) = (h.from.x, h.from.y, h.to.x, h.to.y);
        let (dx, dy) = (bx - ax, by - ay);
        let len2 = dx * dx + dy * dy;
        let k = if len2 > 0.0 {
            (((p[0] - ax) * dx + (p[1] - ay) * dy) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (cx, cy) = (ax + k * dx, ay + k * dy);
        ((p[0] - cx).powi(2) + (p[1] - cy).powi(2)).sqrt() <= h.diameter / 2.0 + CONTACT
    })
}

/// The board's solid in each case solid.
fn board_collisions(board: &BoardSolid, cases: &[PlacedCase]) -> Vec<FitFinding> {
    let pieces = board_pieces(board);
    let bounds = pieces.iter().map(|p| p.bounds).reduce(Aabb::union);
    let Some(bounds) = bounds else {
        return Vec::new();
    };
    // A piece of surface wholly in one hole or cutout is in no board material.
    let skip = |poly: &[[f64; 3]]| poly.iter().all(|q| in_opening(board, *q));
    let mut out = Vec::new();
    for case in cases {
        for s in &case.solids {
            if !meets(&s.bounds, &bounds) {
                continue;
            }
            // The deepest over the board's pieces.
            let surface = pieces
                .iter()
                .filter_map(|v| deepest(v, s, &skip))
                .max_by(|a, b| a.1.total_cmp(&b.1));
            let found = match surface {
                Some((q, d)) => Some((q, Some(d))),
                None => {
                    // The board wholly inside: a point of its material inside the solid (the
                    // middle of a piece, which is inside the outline).
                    pieces
                        .iter()
                        .map(Convex::centre)
                        .find(|q| !in_opening(board, *q))
                        .filter(|q| inside(s, *q))
                        .map(|q| (q, None))
                }
            };
            if let Some((q, depth)) = found {
                out.push(FitFinding {
                    kind: FitKind::Collision,
                    case: case.name.clone(),
                    solid: s.name.clone(),
                    subject: Subject::Board,
                    why: Why::Collision {
                        depth: depth.map(nm),
                        height: None,
                        provenance: None,
                    },
                    at: q.map(nm),
                });
            }
        }
    }
    out
}

/// The board's solid as convex prisms through its thickness: the rounded rectangle as one, a
/// polygon outline (which may be concave) cut into triangles whose shared sides are inner.
fn board_pieces(board: &BoardSolid) -> Vec<Convex> {
    let (z0, z1) = (-board.thickness, 0.0);
    if board.outline.is_empty() {
        let ring = rounded_rect(
            [0.0, 0.0],
            [board.width, board.height],
            board.corner_radius,
            16,
        );
        return vec![Convex::prism(&ring, z0, z1)];
    }
    let ring: Vec<[f64; 2]> = board.outline.iter().map(|p| [p.x, p.y]).collect();
    let n = ring.len();
    // A side of a triangle is the outline's when its two corners are next to each other on it.
    let outer = |i: usize, j: usize| (i + 1) % n == j || (j + 1) % n == i;
    crate::mesh::triangulate(&ring, &[])
        .into_iter()
        .map(|t| {
            let inner: Vec<bool> = (0..3).map(|k| !outer(t[k], t[(k + 1) % 3])).collect();
            let tri: Vec<[f64; 2]> = t.iter().map(|&i| ring[i]).collect();
            Convex::prism_with(&tri, &inner, z0, z1)
        })
        .collect()
}

/// One body against every case model.
fn body_fit(b: &Body, cases: &[PlacedCase], extent: Option<Aabb>) -> Vec<FitFinding> {
    let mut out = Vec::new();
    let subject = Subject::Part {
        path: b.path.clone(),
        refdes: b.refdes.clone(),
    };
    let (lo, hi) = b.z_range();
    let (Some(z0), Some(z1)) = (lo, hi) else {
        // Unknown height: case material anywhere over its face, out to the case's far end.
        let Some(extent) = extent else { return out };
        let (z0, z1) = match b.side {
            Side::Back => (extent.min[2] - 1.0, b.z_face - CONTACT),
            _ => (b.z_face + CONTACT, extent.max[2] + 1.0),
        };
        if z1 <= z0 {
            return out;
        }
        let volume = body_volume(b, z0, z1);
        for case in cases {
            let hit = case.solids.iter().find_map(|s| {
                if !meets(&s.bounds, &volume.bounds) {
                    return None;
                }
                s.near(&volume.bounds)
                    .find_map(|t| volume.clip(t, 0.0))
                    .map(|poly| (s, poly[0]))
            });
            if let Some((s, q)) = hit {
                out.push(FitFinding {
                    kind: FitKind::Unchecked,
                    case: case.name.clone(),
                    solid: s.name.clone(),
                    subject: subject.clone(),
                    why: Why::UnknownHeight,
                    at: q.map(nm),
                });
                break;
            }
        }
        return out;
    };
    if z1 - z0 <= 1e-6 {
        return out;
    }
    let volume = body_volume(b, z0, z1);
    volume_fit(
        &volume,
        &subject,
        cases,
        &|depth| match (b.provenance, b.standard, b.height) {
            (HeightProvenance::Standard, Some(st), Some(h)) => (
                FitKind::Unchecked,
                Why::Standard {
                    depth,
                    height: h,
                    standard: st.cited(),
                },
            ),
            _ => (
                FitKind::Collision,
                Why::Collision {
                    depth,
                    height: b.height,
                    provenance: Some(b.provenance),
                },
            ),
        },
    )
}

/// A part's pins below the far face against every case model, where the design states how far
/// they stand out: a collision of stated pins is an error, as a stated height's is.
fn pins_fit(b: &Body, cases: &[PlacedCase]) -> Vec<FitFinding> {
    let Pins::Known {
        at,
        length,
        z_top,
        z_bottom,
        ..
    } = &b.pins
    else {
        return Vec::new();
    };
    if z_top - z_bottom <= 1e-6 {
        return Vec::new();
    }
    let volume = Convex::plan_box(at.min, at.max, *z_bottom, *z_top);
    let subject = Subject::Pins {
        path: b.path.clone(),
        refdes: b.refdes.clone(),
        length: *length,
    };
    volume_fit(&volume, &subject, cases, &|depth| {
        (
            FitKind::Collision,
            Why::Collision {
                depth,
                height: None,
                provenance: None,
            },
        )
    })
}

/// A volume (a body's box, a part's pins) against every case model: per case model, each solid
/// it is in (by how far, or wholly, `None`), as `collision` names it; else the nearest surface
/// nearer than the clearance.
fn volume_fit(
    volume: &Convex,
    subject: &Subject,
    cases: &[PlacedCase],
    collision: &dyn Fn(Option<f64>) -> (FitKind, Why),
) -> Vec<FitFinding> {
    let mut out = Vec::new();
    let none = |_: &[[f64; 3]]| false;
    for case in cases {
        let mut collided = false;
        for s in &case.solids {
            if !meets(&s.bounds, &volume.bounds) {
                continue;
            }
            let found = match deepest(volume, s, &none) {
                Some((q, d)) => Some((q, Some(d))),
                None => {
                    let c = volume.centre();
                    inside(s, c).then_some((c, None))
                }
            };
            let Some((q, depth)) = found else { continue };
            collided = true;
            let (kind, why) = collision(depth.map(nm));
            out.push(FitFinding {
                kind,
                case: case.name.clone(),
                solid: s.name.clone(),
                subject: subject.clone(),
                why,
                at: q.map(nm),
            });
        }
        if collided {
            continue;
        }
        if let Some((s, q, gap)) = nearest(volume, case) {
            out.push(FitFinding {
                kind: FitKind::Clearance,
                case: case.name.clone(),
                solid: s.to_string(),
                subject: subject.clone(),
                why: Why::Clearance {
                    gap: nm(gap),
                    least: case.figures.clearance,
                },
                at: q.map(nm),
            });
        }
    }
    out
}

/// The nearest surface of `case` to `volume` when it is nearer than the case's clearance: the
/// solid, a point of it and the gap, to a micrometre or better. The gap is the least distance
/// the volume's planes must move out to reach the surface.
fn nearest<'a>(volume: &Convex, case: &'a PlacedCase) -> Option<(&'a str, [f64; 3], f64)> {
    let least = case.figures.clearance;
    if least <= 0.0 {
        return None;
    }
    let grown = volume.bounds_grown(least);
    let mut candidates: Vec<(&str, &[[f64; 3]; 3])> = Vec::new();
    for s in &case.solids {
        if !meets(&s.bounds, &grown) {
            continue;
        }
        for t in s.near(&grown) {
            // Strictly nearer than the clearance: a gap of exactly the figure is enough.
            if volume.clip(t, least - 1e-7).is_some() {
                candidates.push((s.name.as_str(), t));
            }
        }
    }
    if candidates.is_empty() {
        return None;
    }
    let (mut lo, mut hi) = (0.0_f64, least);
    while hi - lo > 1e-7 {
        let mid = 0.5 * (lo + hi);
        let near: Vec<(&str, &[[f64; 3]; 3])> = candidates
            .iter()
            .copied()
            .filter(|(_, t)| volume.clip(t, mid).is_some())
            .collect();
        if near.is_empty() {
            lo = mid;
        } else {
            hi = mid;
            candidates = near;
        }
    }
    // The middle of the nearest piece of surface, where the camera is to look: beside the
    // body's face rather than off a corner of the grown box.
    let (s, t) = candidates[0];
    let q = volume
        .clip(t, hi)
        .filter(|poly| !poly.is_empty())
        .map(|poly| {
            let n = poly.len() as f64;
            [0, 1, 2].map(|k| poly.iter().map(|p| p[k]).sum::<f64>() / n)
        })
        .unwrap_or(t[0]);
    Some((s, q, (hi * 1e4).round() / 1e4))
}

/// A connector at the board's edge, against each case model: material out from its face, or
/// a face too deep inside the wall.
fn reach(board: &BoardSolid, b: &Body, cases: &[PlacedCase]) -> Vec<FitFinding> {
    let mut out = Vec::new();
    let (Some(z0), Some(z1)) = b.z_range() else {
        return out;
    };
    let bx = b.bounds;
    // The nearest edge: its name, the outward direction, the face's place along it and how
    // far the box is from the edge (negative past it).
    let edges = [
        ("left", [-1.0, 0.0], bx.min.x, bx.min.x),
        ("right", [1.0, 0.0], bx.max.x, board.width - bx.max.x),
        ("bottom", [0.0, -1.0], bx.min.y, bx.min.y),
        ("top", [0.0, 1.0], bx.max.y, board.height - bx.max.y),
    ];
    let Some(&(edge, dir, face, _)) = edges
        .iter()
        .min_by(|a, b| a.3.total_cmp(&b.3))
        .filter(|e| e.3 <= AT_EDGE)
    else {
        return out;
    };
    let dir3 = [dir[0], dir[1], 0.0];
    let along_x = dir[0] != 0.0;
    // The face's centre, and points round it, REACH_LOOK out from it on each side.
    let (u0, u1) = if along_x {
        (bx.min.y, bx.max.y)
    } else {
        (bx.min.x, bx.max.x)
    };
    let at = |u: f64, z: f64| {
        if along_x { [face, u, z] } else { [u, face, z] }
    };
    let (uc, zc) = (0.5 * (u0 + u1), 0.5 * (z0 + z1));
    let centre = at(uc, zc);
    let (ua, ub, za, zb) = (
        u0 - REACH_LOOK,
        u1 + REACH_LOOK,
        z0 - REACH_LOOK,
        z1 + REACH_LOOK,
    );
    let around = [
        at(ua, za),
        at(uc, za),
        at(ub, za),
        at(ua, zc),
        at(ub, zc),
        at(ua, zb),
        at(uc, zb),
        at(ub, zb),
    ];
    let subject = Subject::Part {
        path: b.path.clone(),
        refdes: b.refdes.clone(),
    };
    for case in cases {
        let first = case
            .solids
            .iter()
            .filter_map(|s| {
                ray_hits(s, centre, dir3, REACH_RAY)
                    .into_iter()
                    .fold(None, |m: Option<f64>, d| Some(m.map_or(d, |m| m.min(d))))
                    .map(|d| (s, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((s, d)) = first {
            out.push(FitFinding {
                kind: FitKind::Reach,
                case: case.name.clone(),
                solid: s.name.clone(),
                subject: subject.clone(),
                why: Why::ReachBlocked {
                    edge,
                    distance: nm(d),
                },
                at: [0, 1, 2].map(|k| nm(centre[k] + dir3[k] * d)),
            });
            continue;
        }
        // The wall's outer face: the farthest a ray round the face leaves case material.
        let outer = around
            .iter()
            .flat_map(|o| {
                case.solids
                    .iter()
                    .flat_map(|s| ray_hits(s, *o, dir3, REACH_RAY))
            })
            .fold(None, |m: Option<f64>, d| Some(m.map_or(d, |m| m.max(d))));
        if let Some(depth) = outer
            && depth > case.figures.reach + 1e-6
        {
            out.push(FitFinding {
                kind: FitKind::Reach,
                case: case.name.clone(),
                solid: String::new(),
                subject: subject.clone(),
                why: Why::ReachDeep {
                    edge,
                    depth: nm(depth),
                    most: case.figures.reach,
                },
                at: centre.map(nm),
            });
        }
    }
    out
}

/// The mounting holes no case solid holds: nothing within the case's `.boss_gap` of the ring
/// above the top face or below the bottom face, in any case model.
fn bosses(board: &BoardSolid, bodies: &[Body], cases: &[PlacedCase]) -> Vec<FitFinding> {
    let mut out = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for h in &board.holes {
        let HoleOwner::Pad { path, refdes, .. } = &h.owner else {
            continue;
        };
        let Some(b) = bodies
            .iter()
            .find(|b| &b.path == path && matches!(b.kind.as_str(), "MountingHole" | "PlatedHole"))
        else {
            continue;
        };
        if seen.contains(&path.as_str()) {
            continue;
        }
        seen.push(path);
        // The ring: the body's box in plan, at least the hole's.
        let r = h.diameter / 2.0;
        let min = Plan {
            x: b.bounds.min.x.min(h.from.x.min(h.to.x) - r),
            y: b.bounds.min.y.min(h.from.y.min(h.to.y) - r),
        };
        let max = Plan {
            x: b.bounds.max.x.max(h.from.x.max(h.to.x) + r),
            y: b.bounds.max.y.max(h.from.y.max(h.to.y) + r),
        };
        let held = cases.iter().any(|case| {
            let gap = case.figures.boss_gap;
            [
                Convex::plan_box(min, max, 0.0, gap),
                Convex::plan_box(min, max, -board.thickness - gap, -board.thickness),
            ]
            .iter()
            .any(|v| {
                let grown = v.bounds_grown(CONTACT);
                case.solids.iter().any(|s| {
                    meets(&s.bounds, &grown) && s.near(&grown).any(|t| v.clip(t, CONTACT).is_some())
                })
            })
        });
        if !held {
            let gap = cases.iter().map(|c| c.figures.boss_gap).fold(0.0, f64::max);
            out.push(FitFinding {
                kind: FitKind::Boss,
                case: cases
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                solid: String::new(),
                subject: Subject::Part {
                    path: path.clone(),
                    refdes: refdes.clone(),
                },
                why: Why::Boss { gap },
                at: [
                    nm(0.5 * (h.from.x + h.to.x)),
                    nm(0.5 * (h.from.y + h.to.y)),
                    nm(-board.thickness / 2.0),
                ],
            });
        }
    }
    out
}

#[cfg(test)]
mod tests;
