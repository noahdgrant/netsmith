//! The router: the tracks and vias of the nets of a placed board.
//!
//! ```text
//! Board (placed by netsmith-placer) --route--> Board (with tracks and vias)
//! ```
//!
//! [`route`] and [`route_with`] draw the tracks and vias on the two outer layers (ADR 0031, the
//! header of `route.rs`), then vias in parallel where one is too small for its current, the
//! stitching vias of the pours, and an arc at each bend that has room for one (the header of
//! `arcs.rs`). The meanders that match the lengths of a diff pair ([`meander`]) and the teardrops
//! ([`teardrop`]) are steps of it too; they were in `netsmith-pcb` until issue #608.
//!
//! The board model and the decoupling pairs ([`netsmith_pcb::quality`]) are in `netsmith-pcb`,
//! the placer in `netsmith-placer`. This crate depends on `netsmith-pcb`, never the reverse
//! (issue #518), and not on the placer (issue #607): only its tests place their boards with
//! it.

pub mod arcs;
mod decouple;
pub mod meander;
pub mod parallel;
mod plan;
pub mod route;
mod stitch;
pub mod teardrop;

#[cfg(test)]
mod testlib;

// The decoupling pairs the router routes first, which the placer derives as well: in
// `netsmith-pcb`, below both.
use netsmith_pcb::quality;

// The placer, for the unit tests' boards (`crate::board_from_design`).
#[cfg(test)]
use netsmith_placer::board_from_design;

pub use route::{
    CoarsePlan, Costs, Crowding, LineClasses, LineReport, RouteOptions, Routing, Snapshot,
    SnapshotStage, Stage, route, route_with,
};
pub use stitch::stitch;
