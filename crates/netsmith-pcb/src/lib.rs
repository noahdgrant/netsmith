//! The board: footprints from a tool-independent `Design`, placement, a ratsnest, routing,
//! and export to KiCad and SVG.
//!
//! ```text
//! Design --Board::from_design--> Board --route--> Board (with tracks and vias)
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

mod anneal;
pub mod board;
pub mod constraints;
mod explain;
pub mod export;
mod fit;
pub mod footprint;
pub mod generate;
pub mod geometry;
pub mod layout;
pub mod overhang;
mod place;
pub mod quality;
pub mod ratsnest;
pub mod route;
pub mod rules;
pub mod shape;
mod stitch;
pub mod svg;
pub mod validate;

pub use anneal::{Ended, OutsideAt, OverlapAt};
pub use board::{
    Board, BoardCutout, BoardError, BoardKeepout, BoardNet, BoardPour, CopperLayer, Hang,
    HeightUnchecked, HoleNeighbour, KeepoutArea, PadRef, Placed, PlacementProblem, Plane,
    ReferenceSide, Side, Track, Via,
};
pub use explain::RequirementReport;
pub use export::{kicad_pcb, kicad_pro};
pub use footprint::{
    Drill, Footprint, FootprintError, FootprintLibrary, Graphic, Layer, Mount, Pad, PadKind,
    PadShape, Shape,
};
pub use geometry::{Pt, Rect};
pub use layout::{BoardLayout, Layout, LayoutError, apply_layout, place_options};
pub use overhang::{Edge, Edges, Overhang};
pub use place::{
    DoesNotFit, Fixed, HeightZone, Oversize, PartState, PlaceError, PlaceOptions, place_subset,
};
pub use quality::{DecouplingDistance, Quality, crossings};
pub use ratsnest::{Ratline, Ratsnest};
pub use route::{RouteOptions, Routing, route, route_with};
pub use rules::Rules;
pub use stitch::stitch;
pub use validate::{Subject, validate_pinout};
