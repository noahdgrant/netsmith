//! The DC analysis of the picked parts (ADR 0076).
//!
//! [`extract`] reads the netlist from the model and cuts it into islands, [`net`] solves one
//! island over every corner of its ranges and every state of its diodes. `solve/dcpass.rs` writes
//! the results back into the solver's model. This stage models the resistive islands: resistors
//! (a fuse and a ferrite bead are resistors), diodes, the board's own supplies as ideal sources,
//! what a part draws as a current source when its `.current` has a value, regulators (an
//! output that holds a voltage, an input whose current the output sets), logic pins that drive
//! (high, low or high impedance through the output resistance their levels and test currents
//! give) and MOSFETs as switches (on and off, by what their gate says). The cheap bound of
//! `crate::ohm` stays as the fallback for whatever is not modelled.
//!
//! The islands are solved in the order of the power tree. A regulator's output holds its net, so
//! what is behind it is an island of its own, and the current it takes from the output is a range
//! that the island in front of the regulator treats as the regulator's load (ADR 0076, section 3):
//! the island behind is solved first, and the island in front takes its range.

mod extract;
pub(crate) mod net;
#[cfg(test)]
mod tests;

use netsmith_check::{Domain, Model};
use netsmith_quantity::Dimension;

pub(crate) use extract::{Modelled, Planned, RailNode, Reg, extract};
pub(crate) use net::{Collapse, Error, MAX_COMBINATIONS, Report, Which};

use net::{Draw, Kind, Range};

/// One island and what the analysis of it gave.
pub(crate) struct Outcome {
    pub planned: Planned,
    pub result: Result<Report, Error>,
}

/// The current a regulator's output supplies, from the field the design or the balances gave it
/// (a negative `current` is the part supplying), when the loads on the output net are not all
/// modelled.
fn stated_load(model: &Model, reg: &Reg) -> Option<Range> {
    let field = model.field(reg.out_current?);
    match &field.domain {
        Domain::Quantity(q)
            if q.dim() == Dimension::CURRENT
                && q.lo().is_finite()
                && q.hi().is_finite()
                && q.hi() <= 1e-12 =>
        {
            Some(Range::new((-q.hi()).max(0.0), -q.lo()))
        }
        _ => None,
    }
}

/// What an island is in the order of solving.
enum State {
    /// Waiting for the load of a regulator in it.
    Pending,
    /// Left alone: a regulator in it has no load that is known, or the order has a cycle.
    Skipped,
    Solved(Result<Report, Error>),
}

/// The load of a regulator: the range of the current its output supplies, summed over every
/// island that has a pin on the output net. `None` while an island of them waits.
fn load_of(
    model: &Model,
    reg: &Reg,
    id: usize,
    islands: &[Planned],
    states: &[State],
    unsolved_output: bool,
) -> Option<Option<Range>> {
    if !reg.out_complete || unsolved_output {
        return Some(stated_load(model, reg));
    }
    let touching: Vec<(usize, usize)> = islands
        .iter()
        .enumerate()
        .flat_map(|(i, p)| {
            p.outputs
                .iter()
                .filter(move |o| o.reg == id)
                .map(move |o| (i, o.held))
        })
        .collect();
    if touching
        .iter()
        .any(|(i, _)| matches!(states[*i], State::Pending))
    {
        return None;
    }
    let mut sum = Range::point(0.0);
    for (i, held) in touching {
        match &states[i] {
            State::Solved(Ok(report)) if report.consistent > 0 => {
                let r = report.range(Which::HeldDraw(held));
                sum = Range {
                    lo: sum.lo + r.lo,
                    hi: sum.hi + r.hi,
                };
            }
            _ => return Some(stated_load(model, reg)),
        }
    }
    // An output that takes current in (a load that pushes it back) is not a regulator's.
    if sum.lo < -1e-12 || !sum.lo.is_finite() || !sum.hi.is_finite() {
        return Some(None);
    }
    Some(Some(Range::new(sum.lo.max(0.0), sum.hi.max(0.0))))
}

/// Extract and solve every island of a model, in the order of the power tree.
pub(crate) fn analyze(model: &Model) -> Vec<Outcome> {
    let extract::Extracted {
        mut islands,
        regs,
        unsolved_outputs,
    } = extract(model);
    let mut states: Vec<State> = islands.iter().map(|_| State::Pending).collect();
    let mut loads: Vec<Option<Option<Range>>> = vec![None; regs.len()];
    loop {
        let mut progress = false;
        for (r, reg) in regs.iter().enumerate() {
            if loads[r].is_none() {
                let found = load_of(
                    model,
                    reg,
                    r,
                    &islands,
                    &states,
                    unsolved_outputs.contains(&reg.out_net),
                );
                if found.is_some() {
                    loads[r] = found;
                    progress = true;
                }
            }
        }
        for i in 0..islands.len() {
            if !matches!(states[i], State::Pending) {
                continue;
            }
            let sinks = islands[i].sinks.clone();
            if sinks.iter().any(|s| loads[s.reg].is_none()) {
                continue;
            }
            progress = true;
            netsmith_core::job::checkpoint();
            if sinks.iter().any(|s| loads[s.reg] == Some(None)) {
                states[i] = State::Skipped;
                continue;
            }
            for s in &sinks {
                let (Some(Some(load)), reg) = (loads[s.reg], &regs[s.reg]) else {
                    continue;
                };
                if let Kind::Regulator { draw, .. } =
                    &mut islands[i].island.elements[s.element].kind
                {
                    *draw = match *draw {
                        Draw::Linear { .. } => Draw::Linear { i: load },
                        Draw::Switching { eff, .. } => Draw::Switching {
                            p: Range::new(reg.vout.lo * load.lo, reg.vout.hi * load.hi),
                            eff,
                        },
                    };
                }
            }
            states[i] = State::Solved(islands[i].island.analyze(MAX_COMBINATIONS));
        }
        if !progress {
            break;
        }
    }
    islands
        .into_iter()
        .zip(states)
        .filter_map(|(planned, state)| match state {
            State::Solved(result) => Some(Outcome { planned, result }),
            _ => None,
        })
        .collect()
}
