//! The router: the tracks and vias of the nets of a placed board.
//!
//! ```text
//! Board (placed by netsmith-placer) --route--> Board (with tracks and vias)
//! ```
//!
//! [`route`] and [`route_with`] draw the tracks and vias on the two outer layers (ADR 0031, the
//! header of `route.rs`), then vias in parallel where one is too small for its current, the
//! stitching vias of the pours, then the pads that a pour of their net joins to the plane lose
//! their own vias (`pour_pads.rs`), and an arc at each bend that has room for one (the header
//! of `arcs.rs`). The meanders that match the lengths of a diff pair ([`meander`]) and the teardrops
//! ([`teardrop`]) are steps of it too; they were in `netsmith-pcb` until issue #608.
//!
//! [`carry`] keeps the router's copper of the nets a change of placement left alone and routes
//! only the rest (partial routing, issue #825).
//!
//! [`walk`] is not a step of routing: the query the editor's track tool asks while a track is drawn
//! (issue #726), whether a segment is free and the way round the copper in the way, with the
//! design rules check's own spaces.
//!
//! The board model and the decoupling pairs ([`netsmith_pcb::Quality`]) are in `netsmith-pcb`,
//! the placer in `netsmith-placer`. This crate depends on `netsmith-pcb`, never the reverse
//! (issue #518), and not on the placer (issue #607): only its tests place their boards with
//! it.

pub mod arcs;
pub mod carry;
mod decouple;
pub mod meander;
pub mod parallel;
mod plan;
mod pour_pads;
pub mod route;
pub mod shove;
mod stitch;
pub mod teardrop;
pub mod walk;

#[cfg(test)]
mod testlib;

// The placer, for the unit tests' boards (`crate::board_from_design`).
#[cfg(test)]
use netsmith_placer::board_from_design;

pub use route::{
    CoarsePlan, Costs, Crowding, LineClasses, LineReport, RouteOptions, Routing, Snapshot,
    SnapshotStage, Stage, route, route_with,
};
pub use stitch::stitch;
