//! The router: tracks and vias for the nets of a placed [`Board`](netsmith_pcb::Board), on its
//! two outer layers, then vias in parallel where one is too small for its current, and the
//! stitching vias of the pours.
//!
//! ```text
//! Board (placed, netsmith_pcb) --route--> Board (with tracks and vias)
//! ```
//!
//! The board model and the placer are in `netsmith-pcb`, the 3D scene and its writers in
//! `netsmith-3d`; this crate depends on `netsmith-pcb`, never the reverse (issue #518), so that a
//! change to the router runs its own tests and its dependents', and a change to the 3D code
//! runs none of them.

pub mod parallel;
pub mod route;
mod stitch;

#[cfg(test)]
mod testlib;

pub use route::{
    Costs, Crowding, LineReport, RouteOptions, Routing, Snapshot, SnapshotStage, Stage, route,
    route_with,
};
pub use stitch::stitch;
