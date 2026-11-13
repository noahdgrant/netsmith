//! Triangle meshes: the pieces a 3D picture of a board is made of (ADR 0105).
//!
//! A [`Mesh`] is one piece of the scene (the board's substrate, one part's body, a net's copper
//! on one layer, a height limit) with a [`Material`] that says how it is drawn and an `id` that
//! names what it is (a part's path, a net, a limit), so a picture can light up or pick it. The
//! scene's builder makes them, [`crate::raster3d`] draws them, and a glTF writer can write them
//! as they are: positions, optional per-vertex normals and `u32` indices are what a glTF
//! primitive holds.
//!
//! # The frame
//!
//! Positions are in mm, in the scene's frame: right-handed, x to the right and y away from the
//! viewer when the board is seen from above, z up from the board's top face (so the top copper
//! is near z = 0 and the substrate below it). Triangles turn counterclockwise seen from the side
//! their face looks at (the outside of a closed mesh).
//!
//! [`of_scene`] turns a board's [`crate::scene3d::Scene`] into its meshes: the single source of
//! triangles for the rasterizer and the `.glb` writer, with stable ids (see its module).
//!
//! # Builders
//!
//! [`Mesh::axis_box`], [`Mesh::prism`] (a [`Polygon`] with holes, extruded: the board with its
//! cutouts and drilled holes, a body from its outline), [`Mesh::flat`] (a polygon at one height:
//! mask, silkscreen, an unknown part's outline), [`Mesh::cylinder`] and [`Mesh::cylinder_wall`]
//! (holes, vias, pins), [`Mesh::wall`] (a plated hole's barrel, round or a slot),
//! [`Mesh::spheroid`] (a solder ball). [`Mesh::transform`] moves, turns and mirrors a mesh built
//! in a footprint's frame into the scene's. [`circle`] and [`rounded_rect`] make outlines; a
//! circle with `segments` corners stands for a round one. Every builder is deterministic: the same arguments give the
//! same bits on every machine ([`trig`] computes the circles' points without the platform's
//! maths library).

pub mod boolean;
pub(crate) mod package;
mod scene;
mod section;
mod triangulate;
pub mod trig;

pub use scene::{
    BodyPieces, LayerPieces, MaskPieces, NO_NET, NetPieces, Pieces, SILK_LIFT, SilkPieces,
    UNKNOWN_LIFT, hole_segments, of_scene, pieces, slot,
};
pub use section::{PLATING, of_scene_section};
pub use triangulate::{triangulate, triangulate_keeping};

use std::fmt;

/// How a mesh is drawn: its colour comes from a palette by this, and the limits are see-through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Material {
    /// The board's laminate, seen at its edges and in its holes.
    Substrate,
    /// The solder mask on the top face.
    MaskTop,
    /// The solder mask on the bottom face.
    MaskBottom,
    /// Copper: pads, tracks, pours, via rings and barrels.
    Copper,
    /// A part's body of known height.
    Body,
    /// A part whose height is not known: drawn flat on the board (ADR 0105), never as a box.
    BodyUnknown,
    /// Metal of a part: leads, shields, a connector's shell.
    Metal,
    /// Silkscreen and other marks printed on the board (the scene's silkscreen is drawn in it).
    Mark,
    /// A height limit's volume, see-through.
    Limit,
    /// A height limit that a part breaks, see-through.
    LimitViolated,
    /// The finish of copper the mask leaves bare (ENIG, gold): pads and other openings on the
    /// outer layers.
    Finish,
    /// The solder mask where copper is under it, either face: lighter than over bare laminate,
    /// as tracks and pours show through a real board's mask.
    MaskOverCopper,
    /// The board's prepreg, and the resin between the copper of an inner layer: drawn only in
    /// a section ([`of_scene_section`]), where it is the second laminate tone next to the
    /// core's ([`Material::Substrate`]).
    Prepreg,
    /// A case model's solids (ADR 0109), see-through, so the board shows inside its case.
    Case,
}

impl Material {
    /// Every material, in order. A new one goes at the end, so a file's material index of the
    /// older ones does not move.
    pub const ALL: [Material; 14] = [
        Material::Substrate,
        Material::MaskTop,
        Material::MaskBottom,
        Material::Copper,
        Material::Body,
        Material::BodyUnknown,
        Material::Metal,
        Material::Mark,
        Material::Limit,
        Material::LimitViolated,
        Material::Finish,
        Material::MaskOverCopper,
        Material::Prepreg,
        Material::Case,
    ];

    /// The material's name, as a file (a glTF material, a command's option) writes it.
    pub fn name(self) -> &'static str {
        match self {
            Material::Substrate => "substrate",
            Material::MaskTop => "mask-top",
            Material::MaskBottom => "mask-bottom",
            Material::Copper => "copper",
            Material::Body => "body",
            Material::BodyUnknown => "body-unknown",
            Material::Metal => "metal",
            Material::Mark => "mark",
            Material::Limit => "limit",
            Material::LimitViolated => "limit-violated",
            Material::Finish => "finish",
            Material::MaskOverCopper => "mask-over-copper",
            Material::Prepreg => "prepreg",
            Material::Case => "case",
        }
    }

    /// The material of a [`Material::name`].
    pub fn from_name(name: &str) -> Option<Material> {
        Material::ALL.into_iter().find(|m| m.name() == name)
    }

    /// Whether the material is see-through (the limits, a case), drawn after everything else.
    pub fn is_translucent(self) -> bool {
        matches!(
            self,
            Material::Limit | Material::LimitViolated | Material::Case
        )
    }
}

impl fmt::Display for Material {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// An axis of the scene's frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// 0, 1 or 2: the index of the axis in a position.
    pub fn index(self) -> usize {
        match self {
            Axis::X => 0,
            Axis::Y => 1,
            Axis::Z => 2,
        }
    }
}

/// An axis-aligned box in the scene's frame, in mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Aabb {
    /// The box of `points`, or `None` for none.
    pub fn of(points: &[[f64; 3]]) -> Option<Aabb> {
        let mut it = points.iter();
        let first = *it.next()?;
        let mut b = Aabb {
            min: first,
            max: first,
        };
        for p in it {
            b.grow(*p);
        }
        Some(b)
    }

    /// The box grown to hold `p`.
    pub fn grow(&mut self, p: [f64; 3]) {
        for (k, &c) in p.iter().enumerate() {
            self.min[k] = self.min[k].min(c);
            self.max[k] = self.max[k].max(c);
        }
    }

    /// The smallest box holding both.
    pub fn union(self, other: Aabb) -> Aabb {
        let mut b = self;
        b.grow(other.min);
        b.grow(other.max);
        b
    }

    pub fn centre(&self) -> [f64; 3] {
        [0, 1, 2].map(|k| 0.5 * (self.min[k] + self.max[k]))
    }

    pub fn size(&self) -> [f64; 3] {
        [0, 1, 2].map(|k| self.max[k] - self.min[k])
    }

    /// The eight corners.
    pub fn corners(&self) -> [[f64; 3]; 8] {
        let (a, b) = (self.min, self.max);
        [
            [a[0], a[1], a[2]],
            [b[0], a[1], a[2]],
            [a[0], b[1], a[2]],
            [b[0], b[1], a[2]],
            [a[0], a[1], b[2]],
            [b[0], a[1], b[2]],
            [a[0], b[1], b[2]],
            [b[0], b[1], b[2]],
        ]
    }
}

/// A polygon in the xy plane: an outer ring and holes inside it, each ring a list of corners
/// that turns either way and need not repeat its first corner. It must be simple: no ring
/// crosses itself or another.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Polygon {
    pub outer: Vec<[f64; 2]>,
    pub holes: Vec<Vec<[f64; 2]>>,
}

impl Polygon {
    pub fn new(outer: Vec<[f64; 2]>) -> Polygon {
        Polygon {
            outer,
            holes: Vec::new(),
        }
    }

    pub fn with_hole(mut self, hole: Vec<[f64; 2]>) -> Polygon {
        self.holes.push(hole);
        self
    }

    /// The area: the outer ring's less the holes'.
    pub fn area(&self) -> f64 {
        ring_area(&self.outer).abs() - self.holes.iter().map(|h| ring_area(h).abs()).sum::<f64>()
    }
}

/// The signed area of a ring: positive when it turns counterclockwise.
pub(crate) fn ring_area(r: &[[f64; 2]]) -> f64 {
    let mut a = 0.0;
    for k in 0..r.len() {
        let (p, q) = (r[k], r[(k + 1) % r.len()]);
        a += p[0] * q[1] - q[0] * p[1];
    }
    0.5 * a
}

/// Why a mesh could not be built.
#[derive(Debug, Clone, PartialEq)]
pub enum MeshError {
    /// A ring has fewer than three distinct corners, or no area.
    Degenerate { ring: usize, corners: usize },
    /// A coordinate is not finite.
    NotFinite,
    /// The bottom is not below the top.
    Height { bottom: f64, top: f64 },
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeshError::Degenerate { ring: 0, corners } => {
                write!(f, "the outline has no area ({corners} corners)")
            }
            MeshError::Degenerate { ring, corners } => {
                write!(f, "hole {ring} has no area ({corners} corners)")
            }
            MeshError::NotFinite => f.write_str("a coordinate is not a finite number"),
            MeshError::Height { bottom, top } => {
                write!(
                    f,
                    "the bottom ({bottom} mm) is not below the top ({top} mm)"
                )
            }
        }
    }
}

impl std::error::Error for MeshError {}

/// One piece of a scene: triangles of one material, named by `id`.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    /// What the piece is, for picking and highlighting: a part's path, a net's name, a limit's
    /// name. Several meshes may share one (a part's body and its leads).
    pub id: String,
    pub material: Material,
    /// The corners, in mm in the scene's frame.
    pub positions: Vec<[f64; 3]>,
    /// One unit normal per position, for smooth shading; `None` shades each triangle flat.
    pub normals: Option<Vec<[f64; 3]>>,
    /// Three indices into `positions` per triangle, counterclockwise seen from its front.
    pub indices: Vec<u32>,
    /// Whether the mesh is the closed surface of a solid, so a triangle seen from behind is
    /// hidden by another and need not be drawn. An open mesh (a flat outline) is drawn from
    /// both sides.
    pub closed: bool,
    /// A colour of its own, sRGB, in place of its material's: a face of a part's STEP model
    /// that the file colours (ADR 0108). `None` for every other mesh, drawn in the material's.
    pub colour: Option<[u8; 3]>,
}

impl Mesh {
    /// A mesh with no triangles.
    pub fn new(id: impl Into<String>, material: Material) -> Mesh {
        Mesh {
            id: id.into(),
            material,
            positions: Vec::new(),
            normals: Some(Vec::new()),
            indices: Vec::new(),
            closed: true,
            colour: None,
        }
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// The box of the positions, `None` for an empty mesh.
    pub fn bounds(&self) -> Option<Aabb> {
        Aabb::of(&self.positions)
    }

    /// Moves every position by `d`.
    pub fn translate(&mut self, d: [f64; 3]) {
        for p in &mut self.positions {
            for k in 0..3 {
                p[k] += d[k];
            }
        }
    }

    /// Adds the triangles of `other` (its id and material are not looked at). The normals stay
    /// only when both have them; the result is closed only when both are.
    pub fn append(&mut self, other: &Mesh) {
        let base = self.positions.len() as u32;
        if self.positions.is_empty() {
            self.normals = other.normals.clone();
        } else {
            match (&mut self.normals, &other.normals) {
                (Some(a), Some(b)) => a.extend_from_slice(b),
                _ => self.normals = None,
            }
        }
        self.positions.extend_from_slice(&other.positions);
        self.indices.extend(other.indices.iter().map(|i| i + base));
        self.closed = self.closed && other.closed;
    }

    /// Maps every position `p` to `linear * p + offset` (`linear` by rows, invertible) and every
    /// normal by the inverse transpose, so that a mesh built in one frame lands in another. A
    /// map that mirrors (its determinant is negative) also turns every triangle over, so that
    /// it still faces out.
    pub fn transform(&mut self, linear: [[f64; 3]; 3], offset: [f64; 3]) {
        let apply = |m: &[[f64; 3]; 3], v: [f64; 3]| {
            [0, 1, 2].map(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2])
        };
        let l = linear;
        // The cofactors: the determinant times the inverse transpose.
        let cof = [
            [
                l[1][1] * l[2][2] - l[1][2] * l[2][1],
                l[1][2] * l[2][0] - l[1][0] * l[2][2],
                l[1][0] * l[2][1] - l[1][1] * l[2][0],
            ],
            [
                l[0][2] * l[2][1] - l[0][1] * l[2][2],
                l[0][0] * l[2][2] - l[0][2] * l[2][0],
                l[0][1] * l[2][0] - l[0][0] * l[2][1],
            ],
            [
                l[0][1] * l[1][2] - l[0][2] * l[1][1],
                l[0][2] * l[1][0] - l[0][0] * l[1][2],
                l[0][0] * l[1][1] - l[0][1] * l[1][0],
            ],
        ];
        let det = l[0][0] * cof[0][0] + l[0][1] * cof[0][1] + l[0][2] * cof[0][2];
        for p in &mut self.positions {
            let q = apply(&l, *p);
            *p = [0, 1, 2].map(|k| q[k] + offset[k]);
        }
        if let Some(ns) = &mut self.normals {
            for n in ns {
                let q = apply(&cof, *n).map(|c| c * det.signum());
                *n = normalise(q);
            }
        }
        if det < 0.0 {
            for t in self.indices.as_chunks_mut::<3>().0 {
                t.swap(1, 2);
            }
        }
    }

    /// An upright spheroid of `radius` in plan about `centre`, filling `z0` up to `z1`: a ball,
    /// or a ball squashed or stretched along z. `(segments, rings)`: the corners round (at least
    /// 3) and the bands from pole to pole (at least 2). Shaded smooth, closed.
    pub fn spheroid(
        id: impl Into<String>,
        material: Material,
        centre: [f64; 2],
        radius: f64,
        z0: f64,
        z1: f64,
        (segments, rings): (usize, usize),
    ) -> Result<Mesh, MeshError> {
        if ![centre[0], centre[1], radius, z0, z1]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err(MeshError::NotFinite);
        }
        if z0 >= z1 {
            return Err(MeshError::Height {
                bottom: z0,
                top: z1,
            });
        }
        if radius <= 0.0 {
            return Err(MeshError::Degenerate {
                ring: 0,
                corners: 0,
            });
        }
        let (n, rings) = (segments.max(3), rings.max(2));
        let (c, zc) = (0.5 * (z1 - z0), 0.5 * (z0 + z1));
        let mut m = Mesh::new(id, material);
        let top = m.vertex([centre[0], centre[1], z1], [0.0, 0.0, 1.0]);
        // The rings between the poles, from the top down, each `n` corners counterclockwise.
        let mut first = Vec::with_capacity(rings - 1);
        for k in 1..rings {
            let (st, ct) = trig::sin_cos_turns(k as f64 / (2 * rings) as f64);
            first.push(m.positions.len() as u32);
            for j in 0..n {
                let (sp, cp) = trig::sin_cos_turns(j as f64 / n as f64);
                let (ux, uy) = (st * cp, st * sp);
                m.vertex(
                    [
                        centre[0] + radius * ux,
                        centre[1] + radius * uy,
                        zc + c * ct,
                    ],
                    normalise([ux / radius, uy / radius, ct / c]),
                );
            }
        }
        let bottom = m.vertex([centre[0], centre[1], z0], [0.0, 0.0, -1.0]);
        let at = |ring: usize, j: usize| first[ring] + (j % n) as u32;
        for j in 0..n {
            m.indices.extend_from_slice(&[top, at(0, j), at(0, j + 1)]);
            for r in 0..rings - 2 {
                let (a, b, cc, d) = (at(r, j), at(r, j + 1), at(r + 1, j + 1), at(r + 1, j));
                m.indices.extend_from_slice(&[a, d, cc, a, cc, b]);
            }
            let last = rings - 2;
            m.indices
                .extend_from_slice(&[at(last, j), bottom, at(last, j + 1)]);
        }
        Ok(m)
    }

    fn vertex(&mut self, p: [f64; 3], n: [f64; 3]) -> u32 {
        let i = self.positions.len() as u32;
        self.positions.push(p);
        if let Some(ns) = &mut self.normals {
            ns.push(n);
        }
        i
    }

    /// The box from `min` to `max`, six faces of two triangles each, closed.
    pub fn axis_box(
        id: impl Into<String>,
        material: Material,
        min: [f64; 3],
        max: [f64; 3],
    ) -> Mesh {
        let mut m = Mesh::new(id, material);
        let (a, b) = (min, max);
        // Each face: its normal and its corners counterclockwise seen from outside.
        let faces: [([f64; 3], [[f64; 3]; 4]); 6] = [
            (
                [0.0, 0.0, 1.0],
                [
                    [a[0], a[1], b[2]],
                    [b[0], a[1], b[2]],
                    [b[0], b[1], b[2]],
                    [a[0], b[1], b[2]],
                ],
            ),
            (
                [0.0, 0.0, -1.0],
                [
                    [a[0], a[1], a[2]],
                    [a[0], b[1], a[2]],
                    [b[0], b[1], a[2]],
                    [b[0], a[1], a[2]],
                ],
            ),
            (
                [1.0, 0.0, 0.0],
                [
                    [b[0], a[1], a[2]],
                    [b[0], b[1], a[2]],
                    [b[0], b[1], b[2]],
                    [b[0], a[1], b[2]],
                ],
            ),
            (
                [-1.0, 0.0, 0.0],
                [
                    [a[0], a[1], a[2]],
                    [a[0], a[1], b[2]],
                    [a[0], b[1], b[2]],
                    [a[0], b[1], a[2]],
                ],
            ),
            (
                [0.0, 1.0, 0.0],
                [
                    [a[0], b[1], a[2]],
                    [a[0], b[1], b[2]],
                    [b[0], b[1], b[2]],
                    [b[0], b[1], a[2]],
                ],
            ),
            (
                [0.0, -1.0, 0.0],
                [
                    [a[0], a[1], a[2]],
                    [b[0], a[1], a[2]],
                    [b[0], a[1], b[2]],
                    [a[0], a[1], b[2]],
                ],
            ),
        ];
        for (n, q) in faces {
            let v = q.map(|p| m.vertex(p, n));
            m.indices
                .extend_from_slice(&[v[0], v[1], v[2], v[0], v[2], v[3]]);
        }
        m
    }

    /// `polygon` filled at height `z`, facing up (+z) or down: a closed mesh it is not, so it is
    /// drawn from both sides.
    pub fn flat(
        id: impl Into<String>,
        material: Material,
        polygon: &Polygon,
        z: f64,
        up: bool,
    ) -> Result<Mesh, MeshError> {
        let rings = clean_rings(polygon)?;
        if !z.is_finite() {
            return Err(MeshError::NotFinite);
        }
        let mut m = Mesh::new(id, material);
        m.closed = false;
        m.cap(&rings, z, up);
        Ok(m)
    }

    fn cap(&mut self, rings: &[Vec<[f64; 2]>], z: f64, up: bool) {
        let n = if up {
            [0.0, 0.0, 1.0]
        } else {
            [0.0, 0.0, -1.0]
        };
        let base = self.positions.len() as u32;
        for r in rings {
            for p in r {
                self.vertex([p[0], p[1], z], n);
            }
        }
        for [a, b, c] in triangulate(&rings[0], &rings[1..]) {
            let (a, b, c) = (base + a as u32, base + b as u32, base + c as u32);
            if up {
                self.indices.extend_from_slice(&[a, b, c]);
            } else {
                self.indices.extend_from_slice(&[a, c, b]);
            }
        }
    }

    /// `polygon` extruded from `z0` up to `z1`: the top, the bottom and the walls round the
    /// outline and inside each hole, closed. A wall corner whose two faces meet at less than
    /// [`SMOOTH_ANGLE_DEG`] (a circle's, an arc's) gets one normal for both, so a round wall
    /// shades round.
    pub fn prism(
        id: impl Into<String>,
        material: Material,
        polygon: &Polygon,
        z0: f64,
        z1: f64,
    ) -> Result<Mesh, MeshError> {
        let rings = clean_rings(polygon)?;
        if !z0.is_finite() || !z1.is_finite() {
            return Err(MeshError::NotFinite);
        }
        if z0 >= z1 {
            return Err(MeshError::Height {
                bottom: z0,
                top: z1,
            });
        }
        let mut m = Mesh::new(id, material);
        m.cap(&rings, z1, true);
        m.cap(&rings, z0, false);
        for r in &rings {
            m.add_wall(r, z0, z1);
        }
        Ok(m)
    }

    /// The walls of a ring that turns counterclockwise round material (an outline) or clockwise
    /// (a hole), facing away from the material.
    fn add_wall(&mut self, ring: &[[f64; 2]], z0: f64, z1: f64) {
        let n = ring.len();
        let edge_normal = |i: usize| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l = (dx * dx + dy * dy).sqrt();
            [dy / l, -dx / l, 0.0]
        };
        let normals: Vec<[f64; 3]> = (0..n).map(edge_normal).collect();
        let cos_smooth = trig::sin_cos(trig::radians(SMOOTH_ANGLE_DEG)).1;
        // The normal at the start of edge i, after edge i - 1.
        let corner = |i: usize, own: [f64; 3]| {
            let before = normals[(i + n - 1) % n];
            let after = normals[i];
            if before[0] * after[0] + before[1] * after[1] >= cos_smooth {
                normalise([before[0] + after[0], before[1] + after[1], 0.0])
            } else {
                own
            }
        };
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            let e = normals[i];
            let na = corner(i, e);
            let nb = corner((i + 1) % n, e);
            let v0 = self.vertex([a[0], a[1], z0], na);
            let v1 = self.vertex([b[0], b[1], z0], nb);
            let v2 = self.vertex([b[0], b[1], z1], nb);
            let v3 = self.vertex([a[0], a[1], z1], na);
            self.indices.extend_from_slice(&[v0, v1, v2, v0, v2, v3]);
        }
    }

    /// A round post of `radius` about `centre`, from `z0` up to `z1`, as a prism of a
    /// [`circle`] of `segments` corners (at least 3), closed.
    pub fn cylinder(
        id: impl Into<String>,
        material: Material,
        centre: [f64; 2],
        radius: f64,
        z0: f64,
        z1: f64,
        segments: usize,
    ) -> Result<Mesh, MeshError> {
        Mesh::prism(
            id,
            material,
            &Polygon::new(circle(centre, radius, segments)),
            z0,
            z1,
        )
    }

    /// The wall of a round hole of `radius` about `centre`, from `z0` up to `z1`, facing out from
    /// the axis (a plated barrel; seen from inside the hole it is lit as its other side), with no
    /// ends: an open mesh.
    pub fn cylinder_wall(
        id: impl Into<String>,
        material: Material,
        centre: [f64; 2],
        radius: f64,
        z0: f64,
        z1: f64,
        segments: usize,
    ) -> Result<Mesh, MeshError> {
        Mesh::wall(id, material, &circle(centre, radius, segments), z0, z1)
    }

    /// The wall round a hole of the shape `ring` (a circle, a slot), from `z0` up to `z1`,
    /// facing away from the hole's middle, with no ends: an open mesh.
    pub fn wall(
        id: impl Into<String>,
        material: Material,
        ring: &[[f64; 2]],
        z0: f64,
        z1: f64,
    ) -> Result<Mesh, MeshError> {
        let rings = clean_rings(&Polygon::new(ring.to_vec()))?;
        if !z0.is_finite() || !z1.is_finite() {
            return Err(MeshError::NotFinite);
        }
        if z0 >= z1 {
            return Err(MeshError::Height {
                bottom: z0,
                top: z1,
            });
        }
        let mut m = Mesh::new(id, material);
        m.closed = false;
        m.add_wall(&rings[0], z0, z1);
        Ok(m)
    }
}

/// Two faces of a wall that meet at less than this many degrees are shaded as one round
/// surface; at more, as a crease.
pub const SMOOTH_ANGLE_DEG: f64 = 35.0;

fn normalise(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l == 0.0 { v } else { v.map(|c| c / l) }
}

/// The rings of `polygon` without repeated corners, the outer one counterclockwise and the
/// holes clockwise.
fn clean_rings(polygon: &Polygon) -> Result<Vec<Vec<[f64; 2]>>, MeshError> {
    let mut rings = Vec::with_capacity(1 + polygon.holes.len());
    for (k, ring) in std::iter::once(&polygon.outer)
        .chain(&polygon.holes)
        .enumerate()
    {
        if ring.iter().any(|p| !p[0].is_finite() || !p[1].is_finite()) {
            return Err(MeshError::NotFinite);
        }
        let mut r: Vec<[f64; 2]> = Vec::with_capacity(ring.len());
        for &p in ring {
            if r.last() != Some(&p) {
                r.push(p);
            }
        }
        while r.len() > 1 && r.first() == r.last() {
            r.pop();
        }
        let a = ring_area(&r);
        if r.len() < 3 || a == 0.0 {
            return Err(MeshError::Degenerate {
                ring: k,
                corners: r.len(),
            });
        }
        if (a > 0.0) != (k == 0) {
            r.reverse();
        }
        rings.push(r);
    }
    Ok(rings)
}

/// A circle of `radius` about `centre` as `segments` corners (at least 3), counterclockwise,
/// the first on the +x side.
pub fn circle(centre: [f64; 2], radius: f64, segments: usize) -> Vec<[f64; 2]> {
    let n = segments.max(3);
    (0..n)
        .map(|k| {
            let (s, c) = trig::sin_cos_turns(k as f64 / n as f64);
            [centre[0] + radius * c, centre[1] + radius * s]
        })
        .collect()
}

/// The rectangle from `min` to `max` with corners rounded to `radius` (clamped to half the
/// shorter side), each corner `segments` straight pieces, counterclockwise. A radius of 0 gives
/// the four corners.
pub fn rounded_rect(min: [f64; 2], max: [f64; 2], radius: f64, segments: usize) -> Vec<[f64; 2]> {
    let r = radius
        .min(0.5 * (max[0] - min[0]))
        .min(0.5 * (max[1] - min[1]))
        .max(0.0);
    if r == 0.0 || segments == 0 {
        return vec![min, [max[0], min[1]], max, [min[0], max[1]]];
    }
    let centres = [
        [max[0] - r, min[1] + r],
        [max[0] - r, max[1] - r],
        [min[0] + r, max[1] - r],
        [min[0] + r, min[1] + r],
    ];
    let mut out = Vec::with_capacity(4 * (segments + 1));
    for (q, c) in centres.iter().enumerate() {
        // The corner from (q - 1) to q quarter turns, from -90 degrees for the first.
        for k in 0..=segments {
            let turns = (q as f64 - 1.0 + k as f64 / segments as f64) / 4.0;
            let (s, co) = trig::sin_cos_turns(turns);
            out.push([c[0] + r * co, c[1] + r * s]);
        }
    }
    // Where two arcs meet on a side shorter than the diameter, drop the repeat.
    out.dedup();
    if out.first() == out.last() {
        out.pop();
    }
    out
}

/// The box of every mesh, `None` when they hold no position.
pub fn bounds_of(meshes: &[Mesh]) -> Option<Aabb> {
    meshes.iter().filter_map(Mesh::bounds).reduce(Aabb::union)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface_area(m: &Mesh) -> f64 {
        m.indices
            .chunks(3)
            .map(|t| {
                let [a, b, c] = [0, 1, 2].map(|k| m.positions[t[k] as usize]);
                let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let x = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                0.5 * (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt()
            })
            .sum()
    }

    /// The volume of a closed mesh, by the divergence theorem: positive when it faces out.
    fn volume(m: &Mesh) -> f64 {
        m.indices
            .chunks(3)
            .map(|t| {
                let [a, b, c] = [0, 1, 2].map(|k| m.positions[t[k] as usize]);
                (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]))
                    / 6.0
            })
            .sum()
    }

    #[test]
    fn a_box_faces_out_and_has_its_volume() {
        let b = Mesh::axis_box("R1", Material::Body, [0.0, 0.0, 0.0], [2.0, 3.0, 4.0]);
        assert_eq!(b.triangle_count(), 12);
        assert!((volume(&b) - 24.0).abs() < 1e-12);
        assert!((surface_area(&b) - 52.0).abs() < 1e-12);
        assert_eq!(
            b.bounds(),
            Some(Aabb {
                min: [0.0, 0.0, 0.0],
                max: [2.0, 3.0, 4.0]
            })
        );
    }

    #[test]
    fn a_board_with_a_cutout_and_holes_has_its_area_and_volume() {
        let outline = rounded_rect([0.0, 0.0], [50.0, 30.0], 2.0, 6);
        let cutout = vec![[20.0, 10.0], [30.0, 10.0], [30.0, 20.0], [20.0, 20.0]];
        let poly = Polygon::new(outline)
            .with_hole(cutout)
            .with_hole(circle([5.0, 5.0], 1.5, 24))
            .with_hole(circle([45.0, 25.0], 1.5, 24));
        let area = poly.area();
        assert!(area < 50.0 * 30.0 - 100.0 && area > 1300.0);
        let board = Mesh::prism("board", Material::Substrate, &poly, -1.6, 0.0).unwrap();
        assert!(
            (volume(&board) - area * 1.6).abs() < 1e-9,
            "{}",
            volume(&board)
        );
        let top = Mesh::flat("mask", Material::MaskTop, &poly, 0.0, true).unwrap();
        assert!((surface_area(&top) - area).abs() < 1e-9);
        assert!(!top.closed);
        // The same polygon with its rings turned the other way gives the same solid.
        let mut rev = poly.clone();
        rev.outer.reverse();
        for h in &mut rev.holes {
            h.reverse();
        }
        let again = Mesh::prism("board", Material::Substrate, &rev, -1.6, 0.0).unwrap();
        assert!((volume(&again) - area * 1.6).abs() < 1e-9);
    }

    #[test]
    fn a_cylinder_approaches_a_round_post_and_shades_round() {
        let c = Mesh::cylinder("via", Material::Copper, [1.0, 2.0], 0.5, 0.0, 1.0, 64).unwrap();
        let want = std::f64::consts::PI * 0.25;
        assert!((volume(&c) - want).abs() / want < 0.002);
        // Each wall normal points out from the axis, the same at both ends of a corner.
        let ns = c.normals.as_ref().unwrap();
        for (p, n) in c.positions.iter().zip(ns) {
            if n[2] == 0.0 {
                let r = [(p[0] - 1.0) / 0.5, (p[1] - 2.0) / 0.5];
                assert!((r[0] - n[0]).abs() < 1e-12 && (r[1] - n[1]).abs() < 1e-12);
            }
        }
        let w =
            Mesh::cylinder_wall("via", Material::Copper, [0.0, 0.0], 0.5, 0.0, 1.0, 16).unwrap();
        assert_eq!(w.triangle_count(), 32);
        assert!(!w.closed);
    }

    #[test]
    fn a_box_corner_is_a_crease() {
        let m = Mesh::prism(
            "b",
            Material::Body,
            &Polygon::new(rounded_rect([0.0, 0.0], [1.0, 1.0], 0.0, 0)),
            0.0,
            1.0,
        )
        .unwrap();
        for n in m.normals.as_ref().unwrap() {
            assert_eq!(n.iter().filter(|c| c.abs() == 1.0).count(), 1, "{n:?}");
        }
    }

    #[test]
    fn bad_input_is_an_error() {
        let line = Polygon::new(vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]]);
        assert_eq!(
            Mesh::prism("x", Material::Body, &line, 0.0, 1.0),
            Err(MeshError::Degenerate {
                ring: 0,
                corners: 3
            })
        );
        let sq = Polygon::new(rounded_rect([0.0, 0.0], [1.0, 1.0], 0.0, 0));
        assert_eq!(
            Mesh::prism("x", Material::Body, &sq, 1.0, 1.0),
            Err(MeshError::Height {
                bottom: 1.0,
                top: 1.0
            })
        );
        assert_eq!(
            Mesh::flat("x", Material::Body, &sq, f64::NAN, true),
            Err(MeshError::NotFinite)
        );
        assert_eq!(
            MeshError::Degenerate {
                ring: 2,
                corners: 1
            }
            .to_string(),
            "hole 2 has no area (1 corners)"
        );
    }

    #[test]
    fn a_spheroid_faces_out_and_has_its_volume() {
        let s = Mesh::spheroid("U1", Material::Metal, [1.0, 2.0], 0.3, 0.0, 0.4, (48, 24)).unwrap();
        let want = 4.0 / 3.0 * std::f64::consts::PI * 0.3 * 0.3 * 0.2;
        assert!((volume(&s) - want).abs() / want < 0.01, "{}", volume(&s));
        let b = s.bounds().unwrap();
        assert_eq!((b.min[2], b.max[2]), (0.0, 0.4));
        assert!((b.max[0] - 1.3).abs() < 1e-12 && (b.min[1] - 1.7).abs() < 0.01);
        // Every normal points away from the middle.
        for (p, n) in s.positions.iter().zip(s.normals.as_ref().unwrap()) {
            let d = [p[0] - 1.0, p[1] - 2.0, p[2] - 0.2];
            assert!(d[0] * n[0] + d[1] * n[1] + d[2] * n[2] > 0.0);
        }
        assert!(Mesh::spheroid("x", Material::Metal, [0.0; 2], 0.0, 0.0, 1.0, (8, 4)).is_err());
        assert!(Mesh::spheroid("x", Material::Metal, [0.0; 2], 1.0, 1.0, 1.0, (8, 4)).is_err());
    }

    #[test]
    fn a_mirroring_transform_keeps_a_mesh_facing_out() {
        let mut b = Mesh::axis_box("R1", Material::Body, [0.0; 3], [2.0, 3.0, 4.0]);
        // Turned a quarter and turned over (mirrored in z): still the same volume, facing out.
        b.transform(
            [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]],
            [10.0, 0.0, -1.6],
        );
        assert!((volume(&b) - 24.0).abs() < 1e-12, "{}", volume(&b));
        let bb = b.bounds().unwrap();
        assert_eq!(bb.min, [7.0, 0.0, -5.6]);
        assert_eq!(bb.max, [10.0, 2.0, -1.6]);
        // The top face's normal now looks down.
        let ns = b.normals.as_ref().unwrap();
        assert_eq!(ns[0], [0.0, 0.0, -1.0]);
    }

    #[test]
    fn materials_round_trip_by_name() {
        for m in Material::ALL {
            assert_eq!(Material::from_name(m.name()), Some(m));
        }
        assert!(Material::Limit.is_translucent() && !Material::Body.is_translucent());
    }

    #[test]
    fn append_offsets_indices_and_keeps_normals() {
        let mut a = Mesh::axis_box("a", Material::Body, [0.0; 3], [1.0; 3]);
        let b = Mesh::axis_box("b", Material::Body, [2.0; 3], [3.0; 3]);
        a.append(&b);
        assert_eq!(a.triangle_count(), 24);
        assert_eq!(a.normals.as_ref().unwrap().len(), a.positions.len());
        assert_eq!(
            *a.indices.iter().max().unwrap() as usize,
            a.positions.len() - 1
        );
        assert_eq!(bounds_of(&[a]).unwrap().max, [3.0; 3]);
    }
}
