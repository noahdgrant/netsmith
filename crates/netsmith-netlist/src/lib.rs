//! Comparing two netlists by connectivity, for checking that a design ported to netsmith
//! is the design it was ported from (ADR 0017).
//!
//! A [`Netlist`] is read from a KiCad netlist (the XML `kicad-cli sch export netlist`
//! writes), from a `.kicad_pcb`, or built from parts and nets, and written as that XML
//! ([`Netlist::to_kicad_xml`]). [`compare`] decides whether
//! two of them are the same circuit: components are matched by their value and footprint
//! and by how their pins are wired, not by net name or designator, and the answer is
//! either a [`Correspondence`] or a list of [`Difference`]s. [`report`] renders either as
//! text. A pin number is identity except on a two-pad part without polarity, whose pads may
//! be swapped (#67, a change to ADR 0017's rule; the `graph` module says which parts).

mod graph;
mod model;
mod parse;
mod report;
mod value;
mod write;

pub use graph::{
    Ambiguous, Correspondence, Difference, Identity, Options, Outcome, Pin, Undecided, compare,
};
pub use model::{Component, Net, Netlist, NetlistError};
pub use report::report;
pub use value::normalize_value;

#[cfg(test)]
mod tests;
