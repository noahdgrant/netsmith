//! The placer: where the parts of a board go.
//!
//! ```text
//! Design --board_from_design--> Board (placed) --(netsmith-router)--> Board (with tracks and vias)
//! ```
//!
//! [`board_from_design`], [`board_from_design_placed`] and [`place_subset`] move and turn the
//! footprints by simulated annealing (ADR 0033, the header of `anneal.rs`), keeping to the
//! requirements of the physical file (ADR 0045). [`precheck`] says, before any placing, what
//! those requirements prove. [`Quality`] measures a placement.
//!
//! The board model, the options the placer is told and its refusals
//! ([`netsmith_pcb::PlaceOptions`]), and the measure of a placement ([`netsmith_pcb::Quality`])
//! are in `netsmith-pcb`; the router is `netsmith-router`. This crate depends on
//! `netsmith-pcb`, never the reverse (issue #518), and not on the router (issue #607): a change
//! to the router runs none of the placer's tests.

mod anneal;
mod board;
mod fit;
mod place;

#[cfg(test)]
mod pcb_fixtures;
#[cfg(test)]
mod testlib;

pub use board::{board_from_design, board_from_design_placed, precheck};
pub use netsmith_pcb::{DecouplingDistance, Quality, crossings};
pub use place::{PlaceError, place_subset};
