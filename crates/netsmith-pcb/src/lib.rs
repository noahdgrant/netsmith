//! The board model: footprints from a tool-independent `Design`, the board before and after
//! placing, what the placer is told and answers ([`placement`]), the requirements and their
//! checks, a ratsnest, and export to KiCad and SVG. The placer and the router are in
//! `netsmith-place-route`, and the 3D scene and its glTF, STEP and PNG writers in `netsmith-3d`;
//! both depend on this crate.
//!
//! ```text
//! Design --Board::unplaced--> Unplaced --(the placer)--> Board --(the router)--> Board (with tracks and vias)
//!                                  |--kicad_pcb / kicad_pro / svg::render
//!                                  |--Ratsnest::of
//! ```
//!
//! # What a fabrication step reads
//!
//! Everything it needs is public data on [`Board`], in mm, with the origin at the sheet's
//! top left and y growing downward, as KiCad has it:
//!
//! - [`Board::parts`], one [`Placed`] per part: `refdes`, `kind`, `value`, `library_id`
//!   (the KiCad footprint, such as `Resistor_SMD:R_0402_1005Metric`), `at` (the footprint
//!   origin), `rotation` (degrees, counterclockwise on screen), `side` ([`Side`]), and
//!   [`Placed::mount`], which says [`Mount::Smd`] or [`Mount::ThroughHole`]. The pads are
//!   `placed.footprint.pads`, and [`Placed::pad_centre`] puts one on the board.
//! - [`Board::nets`]: each net's name and [`PadRef`]s (reference and pad number).
//! - [`Board::outline`]: the board edge, a rectangle.
//! - [`Board::tracks`] and [`Board::vias`]: the routed copper, on the two outer layers.
//! - [`Board::planes`]: the inner layers of a four-layer board, each poured on one net.
//! - [`Board::rules`]: the [`Rules`] the board is made to.
//!
//! A footprint's origin is where KiCad puts it, which is not always the centre of the part
//! as a pick-and-place file wants it; use [`Footprint::courtyard`] for the centre.

pub mod board;
pub mod body3d;
pub mod constraints;
pub mod copper;
pub mod explain;
pub mod export;
pub mod footprint;
pub mod generate;
pub mod geometry;
pub mod layout;
pub mod meander;
pub mod origin;
pub mod overhang;
pub mod placement;
pub mod polygon;
pub mod protrusion;
pub mod ratsnest;
pub mod rules;
pub mod shape;
pub mod stackup;
pub mod standard_height;
pub mod svg;
pub mod teardrop;
pub mod validate;

pub use board::{
    Board, BoardCutout, BoardError, BoardKeepout, BoardNet, BoardPour, CopperLayer, Hang,
    HeightUnchecked, HoleNeighbour, KeepoutArea, PadRef, Placed, PlacementProblem, Plane,
    ReferenceSide, Side, StandardPass, Track, Trim, Unplaced, Via,
};
pub use explain::{Fault, Precheck, RequirementReport};
pub use export::{PATH_PROPERTY, kicad_pcb, kicad_pro};
pub use footprint::{
    Drill, Footprint, FootprintError, FootprintLibrary, Graphic, Layer, Mount, Pad, PadKind,
    PadShape, Shape,
};
pub use geometry::{Pt, Rect};
pub use layout::{BoardLayout, Layout, LayoutError, apply_layout, place_options};
pub use origin::{ORIGINS_HEAD, PartOrigin, origins_text};
pub use overhang::{Edge, Edges, Overhang};
pub use placement::{
    DoesNotFit, Ended, Fixed, HeightZone, ORIGIN, OutsideAt, OverlapAt, Oversize, PartState,
    PlaceOptions,
};
pub use ratsnest::{Ratline, Ratsnest};
pub use rules::Rules;
pub use validate::{Subject, validate_pinout};
