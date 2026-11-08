//! The DC analysis of the picked parts (ADR 0076).
//!
//! [`extract`] reads the netlist from the model and cuts it into islands, [`net`] solves one
//! island over every corner of its ranges and every state of its diodes. `solve/dc.rs` writes
//! the results back into the solver's model. This stage models the resistive islands: resistors
//! (a fuse and a ferrite bead are resistors), diodes, the board's own supplies as ideal sources,
//! and what a part draws as a current source when its `.current` has a value. Regulators, the
//! drive states of logic pins and MOSFETs as switches are later stages; the cheap bound of
//! `crate::ohm` stays as the fallback for whatever is not modelled.

mod extract;
pub(crate) mod net;
#[cfg(test)]
mod tests;

pub(crate) use extract::{Modelled, Planned, extract};
pub(crate) use net::{Error, MAX_COMBINATIONS, Report, Which};

/// One island and what the analysis of it gave.
pub(crate) struct Outcome {
    pub planned: Planned,
    pub result: Result<Report, Error>,
}

/// Extract and solve every island of a model, in a fixed order.
pub(crate) fn analyze(model: &netsmith_check::Model) -> Vec<Outcome> {
    extract(model)
        .into_iter()
        .map(|planned| {
            let result = planned.island.analyze(MAX_COMBINATIONS);
            Outcome { planned, result }
        })
        .collect()
}
