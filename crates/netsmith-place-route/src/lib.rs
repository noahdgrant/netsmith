//! The placer and the router: where the parts of a board go, then the tracks and vias of its
//! nets.
//!
//! ```text
//! Design --board_from_design--> Board (placed) --route--> Board (with tracks and vias)
//! ```
//!
//! - The placer ([`board_from_design`], [`board_from_design_placed`], [`place_subset`]) moves
//!   and turns the footprints by simulated annealing (ADR 0033, the header of `anneal.rs`),
//!   keeping to the requirements of the physical file (ADR 0045). [`precheck`] says, before
//!   any placing, what those requirements prove. [`Quality`] measures a placement.
//! - The router ([`route`], [`route_with`]) draws the tracks and vias on the two outer layers
//!   (ADR 0031, the header of `route.rs`), then vias in parallel where one is too small for its
//!   current, the stitching vias of the pours, and an arc at each bend that has room for one
//!   (the header of `arcs.rs`).
//!
//! The board model, the options the placer is told and its refusals
//! ([`netsmith_pcb::placement`]) are in `netsmith-pcb`, the 3D scene and its writers in
//! `netsmith-3d`. This crate depends on `netsmith-pcb`, never the reverse (issue #518): a change
//! to the placer or the router runs its own tests and its dependents', and a change to the 3D
//! code runs none of them.

mod anneal;
pub mod arcs;
mod board;
mod decouple;
mod fit;
pub mod parallel;
mod place;
mod plan;
pub mod quality;
pub mod route;
mod stitch;

#[cfg(test)]
mod testlib;

pub use board::{board_from_design, board_from_design_placed, precheck};
/// The board model this crate places and routes. A caller can name the very types the
/// functions here take through it; `netsmith-pcb`'s own tests, which compile another copy of
/// that crate, read a placed board back into theirs through it (`board::tests::placed` there).
pub use netsmith_pcb;
pub use place::{PlaceError, place_subset};
pub use quality::{DecouplingDistance, Quality, crossings};
pub use route::{
    CoarsePlan, Costs, Crowding, LineClasses, LineReport, RouteOptions, Routing, Snapshot,
    SnapshotStage, Stage, route, route_with,
};
pub use stitch::stitch;
