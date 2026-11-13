//! netsmith's own STEP reader (ADR 0108): a model file a project names for a part, read into
//! the scene's triangles.
//!
//! [`read`] takes the file's bytes through three stages, each its own module ([`read_model`]
//! stops after the second):
//!
//! - [`part21`]: the text as instances (ISO 10303-21): the header, every numbered instance,
//!   simple or complex, with its parameters typed.
//! - [`model`]: the boundary representation the instances hold: the solids the shape
//!   representations hold, placed through the assemblies that nest them (`MAPPED_ITEM`s and
//!   `ITEM_DEFINED_TRANSFORMATION`s); their shells, faces, loops and edges; the planes,
//!   cylinders, cones, spheres, tori and B-spline surfaces (rational too) under the faces; the
//!   lines, circles, ellipses, polylines and B-spline curves under the edges; each
//!   representation's unit (mm, m, inch: anything a metre or a conversion of one) and the
//!   colour the file styles a face, a shell or a solid with.
//! - [`tessellate`]: each face as triangles within [`CHORD_TOLERANCE`], with the scene's
//!   triangulator and trig ([`crate::mesh`]).
//!
//! # The contract
//!
//! - **Millimetres, the file's own frame.** The model is where the file puts it; placing it on
//!   a part is the caller's.
//! - **All or nothing.** A file that is not STEP, an entity the reader does not know inside a
//!   solid, a reference to an instance that is not there, or a face it cannot cut into
//!   triangles is a [`StepError`] naming the entity and its line. A model is never half read.
//! - **Closed shells stay closed.** Each edge is cut into pieces once and both its faces use
//!   those points, so the triangles of a closed shell meet along every edge.
//! - **Deterministic.** The same bytes give the same triangles, to the bit, on every machine:
//!   the instances are read in id order, the tolerance is fixed, the trig is the scene's own
//!   and every position is rounded to a nanometre, as the scene's are.

pub(crate) mod geometry;
pub(crate) mod model;
pub(crate) mod part21;
pub(crate) mod tessellate;

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod mesh_tests;
#[cfg(test)]
mod tests;

use std::fmt;

pub(crate) use model::{Model, Rgb};
pub(crate) use part21::{Exchange, ParseError};
pub(crate) use tessellate::CHORD_TOLERANCE;

use crate::mesh::{Material, Mesh};

/// Why a model could not be read.
#[derive(Debug, Clone, PartialEq)]
pub enum StepError {
    /// The text is not Part 21.
    Parse(ParseError),
    /// `#by` (on `line`) refers to `#id`, which the file does not have.
    Missing { id: u64, by: u64, line: u32 },
    /// A solid is made of an entity the reader does not read.
    Unsupported { id: u64, line: u32, entity: String },
    /// An entity's parameters are not what ISO 10303 says they are.
    Malformed {
        id: u64,
        line: u32,
        entity: String,
        message: String,
    },
    /// A face could not be cut into triangles.
    Face { id: u64, line: u32, message: String },
    /// The file holds no solid.
    NoSolid,
}

impl fmt::Display for StepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StepError::Parse(e) => e.fmt(f),
            StepError::Missing { id, by, line } => {
                write!(
                    f,
                    "line {line}: #{by} refers to #{id}, which is not in the file"
                )
            }
            StepError::Unsupported { id, line, entity } => write!(
                f,
                "line {line}: #{id} is a {entity}, which netsmith's STEP reader does not read"
            ),
            StepError::Malformed {
                id,
                line,
                entity,
                message,
            } => write!(f, "line {line}: #{id} ({entity}): {message}"),
            StepError::Face { id, line, message } => {
                write!(f, "line {line}: the face #{id} cannot be drawn: {message}")
            }
            StepError::NoSolid => f.write_str("the file holds no solid"),
        }
    }
}

impl std::error::Error for StepError {}

impl From<ParseError> for StepError {
    fn from(e: ParseError) -> StepError {
        StepError::Parse(e)
    }
}

/// A piece of a model drawn in one colour: the faces of one placed solid that the file styles
/// alike, or leaves unstyled (`None`).
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub colour: Option<Rgb>,
    /// In mm, in the file's frame; its id is the solid's name, or `#id` when it has none. The
    /// material is [`Material::Body`]; it is closed when its shell is.
    pub mesh: Mesh,
}

/// A model as triangles.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StepMesh {
    pub pieces: Vec<Piece>,
}

impl StepMesh {
    pub fn triangle_count(&self) -> usize {
        self.pieces.iter().map(|p| p.mesh.triangle_count()).sum()
    }

    /// The box round every piece.
    pub fn bounds(&self) -> Option<crate::mesh::Aabb> {
        let meshes: Vec<Mesh> = self.pieces.iter().map(|p| p.mesh.clone()).collect();
        crate::mesh::bounds_of(&meshes)
    }
}

/// A model file read for drawing and for copying: its instances (the STEP export copies the
/// solids' entities from them, ADR 0108), its model and its triangles.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadModel {
    pub exchange: Exchange,
    pub model: Model,
    pub mesh: StepMesh,
}

/// Reads a STEP file's bytes, keeping what each stage made.
pub fn load(bytes: &[u8]) -> Result<ReadModel, StepError> {
    let exchange = part21::parse(bytes)?;
    let model = model::model(&exchange)?;
    let mesh = mesh(&model)?;
    Ok(ReadModel {
        exchange,
        model,
        mesh,
    })
}

/// Reads a STEP file's bytes into its model: the solids, where they are, and their faces.
pub(crate) fn read_model(bytes: &[u8]) -> Result<Model, StepError> {
    let ex = part21::parse(bytes)?;
    model::model(&ex)
}

/// Reads a STEP file's bytes into triangles.
pub fn read(bytes: &[u8]) -> Result<StepMesh, StepError> {
    mesh(&read_model(bytes)?)
}

/// A length to a nanometre, with no negative zero.
fn nanometre(x: f64) -> f64 {
    let r = (x * 1e6).round() / 1e6;
    if r == 0.0 { 0.0 } else { r }
}

/// The triangles of a model read.
pub(crate) fn mesh(model: &Model) -> Result<StepMesh, StepError> {
    // Each edge's points and each face's triangles once, at the tolerance of the first solid
    // that uses it.
    let mut edges: Vec<Vec<geometry::P3>> = vec![Vec::new(); model.edges.len()];
    let mut faces: Vec<Option<(bool, tessellate::FaceMesh)>> = vec![None; model.faces.len()];
    let mut out = StepMesh::default();
    for placed in &model.placed {
        let solid = &model.solids[placed.solid];
        let tol = CHORD_TOLERANCE / solid.mm;
        let m = placed.place.m;
        let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
        let name = if solid.name.is_empty() {
            format!("#{}", solid.id)
        } else {
            solid.name.clone()
        };
        let mut pieces: Vec<Piece> = Vec::new();
        for shell in &solid.shells {
            for &fi in &shell.faces {
                let face = &model.faces[fi];
                for (lp, _) in &face.bounds {
                    if let model::Loop::Edges(uses) = lp {
                        for u in uses {
                            if edges[u.edge].is_empty() {
                                let e = &model.edges[u.edge];
                                edges[u.edge] =
                                    tessellate::edge_points(e, tol).map_err(|message| {
                                        StepError::Malformed {
                                            id: e.id,
                                            line: e.line,
                                            entity: "EDGE_CURVE".into(),
                                            message,
                                        }
                                    })?;
                            }
                        }
                    }
                }
                let cached = matches!(&faces[fi], Some((r, _)) if *r == shell.reversed);
                if !cached {
                    let fm =
                        tessellate::face(face, &edges, shell.reversed, tol).map_err(|message| {
                            StepError::Face {
                                id: face.id,
                                line: face.line,
                                message,
                            }
                        })?;
                    faces[fi] = Some((shell.reversed, fm));
                }
                let Some((_, fm)) = &faces[fi] else { continue };
                let colour = face.colour.or(shell.colour).or(solid.colour);
                let k = match pieces.iter().position(|p| p.colour == colour) {
                    Some(k) => k,
                    None => {
                        let mut mesh = Mesh::new(name.clone(), Material::Body);
                        mesh.closed = shell.closed;
                        pieces.push(Piece { colour, mesh });
                        pieces.len() - 1
                    }
                };
                let mesh = &mut pieces[k].mesh;
                mesh.closed &= shell.closed;
                let base = mesh.positions.len() as u32;
                for (p, n) in fm.positions.iter().zip(&fm.normals) {
                    let q = placed.place.apply(p.map(|c| c * solid.mm));
                    mesh.positions.push(q.map(nanometre));
                    let nn = [0, 1, 2].map(|r| m[r][0] * n[0] + m[r][1] * n[1] + m[r][2] * n[2]);
                    if let Some(ns) = &mut mesh.normals {
                        ns.push(geometry::unit(nn).unwrap_or(nn));
                    }
                }
                for t in &fm.indices {
                    let t = if det < 0.0 { [t[0], t[2], t[1]] } else { *t };
                    let t = t.map(|i| i + base);
                    // A sliver whose corners meet once rounded is left out: its sides cancel.
                    let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
                    if a != b && b != c && c != a {
                        mesh.indices.extend(t);
                    }
                }
            }
        }
        out.pieces.extend(pieces);
    }
    Ok(out)
}
