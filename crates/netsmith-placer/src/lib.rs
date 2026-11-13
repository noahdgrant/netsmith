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
//! ([`netsmith_pcb::placement`]), and the measure of a placement ([`netsmith_pcb::quality`])
//! are in `netsmith-pcb`; the router is `netsmith-router`. This crate depends on
//! `netsmith-pcb`, never the reverse (issue #518), and not on the router (issue #607): a change
//! to the router runs none of the placer's tests.

mod anneal;
mod board;
mod fit;
mod place;

#[cfg(test)]
mod testlib;

// The measure of a placement and the decoupling pairs, which the router uses as well: in
// `netsmith-pcb`, below both, so that the router does not depend on the placer.
use netsmith_pcb::quality;

pub use board::{board_from_design, board_from_design_placed, precheck};
/// The board model this crate places. A caller can name the very types the functions here take
/// through it; `netsmith-pcb`'s own tests, which compile another copy of that crate, read a
/// placed board back into theirs through it (`board::tests::placed` there).
pub use netsmith_pcb;
pub use netsmith_pcb::quality::{DecouplingDistance, Quality, crossings};
pub use place::{PlaceError, place_subset};
