//! netsmith's own STEP reader (ADR 0108): a model file a project names for a part.
//!
//! [`read_model`] takes the file's bytes through two stages, each its own module:
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
//!
//! [`geometry`] evaluates the curves and surfaces with the scene's own trig
//! ([`crate::mesh::trig`]). Cutting the faces into the scene's triangles comes next.
//!
//! # The contract
//!
//! - **The file's own frame**, each solid's placement in mm and its unit in mm. Placing the
//!   model on a part is the caller's.
//! - **All or nothing.** A file that is not STEP, an entity the reader does not know inside a
//!   solid, or a reference to an instance that is not there is a [`StepError`] naming the
//!   entity and its line. A model is never half read.
//! - **Deterministic.** The instances are read in id order and the solids walked in the order
//!   the file nests them, so the same bytes give the same model.

pub mod geometry;
pub mod model;
pub mod part21;

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod tests;

use std::fmt;

pub use model::{Model, Rgb};
pub use part21::{Exchange, ParseError};

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

/// Reads a STEP file's bytes into its model: the solids, where they are, and their faces.
pub fn read_model(bytes: &[u8]) -> Result<Model, StepError> {
    let ex = part21::parse(bytes)?;
    model::model(&ex)
}
