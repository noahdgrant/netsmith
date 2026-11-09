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
//!
//! Notes for a maintainer, from ADR 0076 as it was before its cut (`git show 20f8f0c:docs/adr/`):
//!
//! - The pass is deterministic. Elements are taken in the order the model lists them, the
//!   combinations in a fixed mixed-radix order, nodes are numbered in the order they are first
//!   named, so no result depends on a hash order. It uses addition, subtraction,
//!   multiplication, division and comparison only (IEEE 754 defines them to the bit), with no
//!   transcendental function and no fused multiply-add, so a design gives the same numbers on
//!   any target.
//! - An island over `MAX_COMBINATIONS` (65536), or over `MAX_WORK` (2^28), is left alone with a
//!   note that says to cut it at a node a rail holds. Cutting at a held node costs the tie
//!   between a current and the node's voltage, so a result is wider and never narrower.
//! - The corners are the circuit's range only for a quantity that is monotone in each value. A
//!   power need not be: `p(r1)` of R1 from 500 to 1500 ohm over a fixed R2 of 1000 ohm peaks
//!   inside the box (25 mW at R1 = R2, where the corners give 22.2 mW and 24 mW), and for a
//!   resistor of 1 % the error is second order, about `(tolerance / 2)^2`. `net::check_interior`
//!   draws points inside the ranges and counts those outside the hull (a fixed seed; tests
//!   only). A stated margin of that size for any element with a range wider than 5 %, and a
//!   sound enclosure by interval Newton or affine arithmetic, are not built: the owner has not
//!   decided (ADR 0076, left open).
//! - Not built: a cache of an island's result keyed by its netlist and boundary ranges, so an
//!   edit solves only the island it is in; and sensitivity corners (the sign of a quantity's
//!   derivative with respect to each value, one extra solve per value, says which end of each
//!   range gives its extreme: `n + 2` solves a quantity for a monotone one, past 2^n).
//! - The cost budget when this was written: at most 5 % of the solver's time, because the editor
//!   re-solves on every edit (a debug build of the crate is optimised, `[profile.dev.package.
//!   netsmith-solve]` in the root `Cargo.toml`).

pub(crate) mod ceiling;
mod extract;
pub(crate) mod net;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use netsmith_check::{Domain, FieldId, InstId, Model, RExpr};
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::Dimension;

pub(crate) use extract::{Left, Modelled, Planned, RailNode, Reg, Supply, extract};
pub(crate) use net::{Collapse, Error, MAX_COMBINATIONS, Range, Report, Which};

use net::{Draw, Kind};

/// One island and what the analysis of it gave.
pub(crate) struct Outcome {
    pub planned: Planned,
    pub result: Result<Report, Error>,
}

/// What `analyze` made of a model: the islands it solved, and what it left alone, each with the
/// note that says so (a check that cannot run says so).
pub(crate) struct Analysis {
    pub outcomes: Vec<Outcome>,
    pub left: Vec<Left>,
}

/// Why the load of a regulator is not a range.
#[derive(Clone, Copy)]
enum Why {
    /// A pin on the output net is not modelled, or an island on it was not solved, and the
    /// output's own `.current` states no range.
    Unknown,
    /// The islands on the output together take current in: this much at the most.
    Pushed(f64),
}

/// The load of a regulator, and why it has none when it has none.
type Load = (Option<Range>, Option<Why>);

/// The current a regulator's output supplies, from the field the design or the balances gave it
/// (a negative `current` is the part supplying), when the loads on the output net are not all
/// modelled.
///
/// With `enclosure` (the start before the picks, ADR 0076 amendment 6) a field with no usable
/// range still gives one when a relation limits what the output supplies (`-.vout.current <=
/// .max_current`): the regulator supplies, so the load is between nothing and that limit. It is
/// an enclosure of every load a pick could make, wide and sound; the pass after the picks has
/// the loads themselves.
fn stated_load(model: &Model, reg: &Reg, enclosure: bool) -> Option<Range> {
    let field = model.field(reg.out_current?);
    if let Domain::Quantity(q) = &field.domain
        && q.dim() == Dimension::CURRENT
        && q.lo().is_finite()
        && q.hi().is_finite()
        && q.hi() <= 1e-12
    {
        return Some(Range::new((-q.hi()).max(0.0), -q.lo()));
    }
    if enclosure {
        return limit_of(model, reg.out_current?).map(|max| Range::new(0.0, max));
    }
    None
}

/// The most that a relation lets a rail's current supply: the least value of `x` over the
/// relations `-.current <= x` that have one.
pub(crate) fn limit_of(model: &Model, current: FieldId) -> Option<f64> {
    let mut limit: Option<f64> = None;
    for r in model.relations() {
        let (larger, smaller) = match r.op {
            CmpOp::Ge => (&r.lhs, &r.rhs),
            CmpOp::Le => (&r.rhs, &r.lhs),
            _ => continue,
        };
        let RExpr::Neg(inner) = smaller else { continue };
        if !matches!(&**inner, RExpr::Field(f) if *f == current) {
            continue;
        }
        if let Ok(crate::eval::Val::Q(q)) = crate::eval::eval(model, larger)
            && q.dim() == Dimension::CURRENT
            && q.hi().is_finite()
            && q.hi() >= 0.0
        {
            limit = Some(limit.map_or(q.hi(), |m| m.min(q.hi())));
        }
    }
    limit
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
    enclosure: bool,
    reg: &Reg,
    id: usize,
    islands: &[Planned],
    states: &[State],
    unsolved_output: bool,
) -> Option<Load> {
    let stated = || {
        let load = stated_load(model, reg, enclosure);
        Some((load, load.is_none().then_some(Why::Unknown)))
    };
    if !reg.out_complete || unsolved_output {
        return stated();
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
            _ => return stated(),
        }
    }
    // An output that takes current in (a load that pushes it back) is not a regulator's.
    if sum.lo < -1e-12 || !sum.lo.is_finite() || !sum.hi.is_finite() {
        return Some((None, Some(Why::Pushed(-sum.lo))));
    }
    Some((Some(Range::new(sum.lo.max(0.0), sum.hi.max(0.0))), None))
}

/// Extract and solve every island of a model, in the order of the power tree. With `enclosure`
/// (the start before the picks) a regulator whose load is not known is given the range its own
/// relations enclose it in.
pub(crate) fn analyze(
    model: &Model,
    enclosure: bool,
    enclosed: &HashMap<InstId, Range>,
    hulled: &HashSet<InstId>,
) -> Analysis {
    let extract::Extracted {
        mut islands,
        regs,
        unsolved_outputs,
        mut left,
    } = extract(model, enclosed, hulled, &HashSet::new());
    let mut states: Vec<State> = islands.iter().map(|_| State::Pending).collect();
    let mut loads: Vec<Option<Option<Range>>> = vec![None; regs.len()];
    let mut whys: Vec<Option<Why>> = vec![None; regs.len()];
    loop {
        let mut progress = false;
        for (r, reg) in regs.iter().enumerate() {
            if loads[r].is_none() {
                let found = load_of(
                    model,
                    enclosure,
                    reg,
                    r,
                    &islands,
                    &states,
                    unsolved_outputs.contains(&reg.out_net),
                );
                if let Some((load, why)) = found {
                    loads[r] = Some(load);
                    whys[r] = why;
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
            if let Some(s) = sinks.iter().find(|s| loads[s.reg] == Some(None)) {
                states[i] = State::Skipped;
                left.push(skipped_note(&islands[i], &regs[s.reg], whys[s.reg]));
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
                        // What the output supplies and the quiescent current: both are
                        // drawn at once, so the ends add.
                        Draw::Linear { .. } => Draw::Linear {
                            i: reg
                                .quiescent
                                .map_or(load, |q| Range::new(load.lo + q.lo, load.hi + q.hi)),
                        },
                        // What the output asks of the input, and the quiescent current on top
                        // of it (the ends add, as for a linear regulator).
                        Draw::Switching { eff, .. } => Draw::Switching {
                            p: Range::new(reg.vout.lo * load.lo, reg.vout.hi * load.hi),
                            eff,
                            iq: reg.quiescent.unwrap_or(Range::point(0.0)),
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
    // An island still waiting when nothing more could be done is a cycle of regulators, each
    // waiting for the load of the next.
    for (planned, state) in islands.iter().zip(&states) {
        if matches!(state, State::Pending)
            && let Some(m) = planned.parts.first()
        {
            left.push(Left {
                at: m.part,
                involves: planned.parts.iter().map(|m| m.part).collect(),
                cause: netsmith_core::Cause::Circular,
                message: format!(
                    "the DC analysis left the network of {} alone: the regulators in it wait for each other's load, and none of them has one that is known; state the current one of their outputs supplies (`.vout.current &= -200mA..0A`)",
                    names_of(planned)
                ),
            });
        }
    }
    let outcomes = islands
        .into_iter()
        .zip(states)
        .filter_map(|(planned, state)| match state {
            State::Solved(result) => Some(Outcome { planned, result }),
            _ => None,
        })
        .collect();
    Analysis { outcomes, left }
}

/// The elements of an island, as a list for a message.
pub(crate) fn names_of(planned: &Planned) -> String {
    let names: Vec<String> = planned
        .island
        .elements
        .iter()
        .map(|e| format!("`{}`", e.name))
        .collect();
    match names.len() {
        0 => "the island".to_string(),
        1..=4 => names.join(", "),
        n => format!("{} and {} more", names[..3].join(", "), n - 3),
    }
}

/// The note for an island that was not solved because the load of a regulator in it has no range.
fn skipped_note(planned: &Planned, reg: &Reg, why: Option<Why>) -> Left {
    let cause = netsmith_core::Cause::Supply;
    let message = match why {
        Some(Why::Pushed(i)) => format!(
            "the DC analysis left the network of {} alone: the loads on the output of `{}` take {} in, which no regulator supplies; a part or a resistor on that net is joined to a voltage above the output's (check what drives it), or state the current the output supplies (`{}.current &= -200mA..0A`)",
            names_of(planned),
            reg.name,
            netsmith_quantity::Quantity::point(Dimension::CURRENT, i)
                .map_or_else(|_| format!("{i} A"), |q| q.to_string()),
            reg.name,
        ),
        _ => format!(
            "the DC analysis left the network of {} alone: what the output of `{}` supplies is not known (a pin on its net is not modelled, or the network behind it was not solved) and `{}.current` states no range; state it (`{}.current &= -200mA..0A`, a supply's current is negative) or give the pin a part model",
            names_of(planned),
            reg.name,
            reg.name,
            reg.name,
        ),
    };
    Left {
        at: planned.parts.first().map_or(reg.part, |m| m.part),
        involves: planned.parts.iter().map(|m| m.part).collect(),
        cause,
        message,
    }
}

/// The islands that have one of `parts`, solved on their own and cut at the rails in `cuts` (the
/// pick-time analysis, ADR 0076 amendment 10): an island that is not a resistive one (a regulator
/// in it, or a node a regulator holds) is left out, since what it would need is the island behind
/// the regulator, and that is the pass after the picks'.
pub(crate) fn analyze_around(
    model: &Model,
    parts: &[InstId],
    cuts: &HashSet<InstId>,
) -> Vec<Outcome> {
    let extract::Extracted { islands, .. } = extract(model, &HashMap::new(), &HashSet::new(), cuts);
    islands
        .into_iter()
        .filter(|p| {
            p.sinks.is_empty()
                && p.outputs.is_empty()
                && p.parts.iter().any(|m| parts.contains(&m.part))
        })
        .map(|planned| {
            netsmith_core::job::checkpoint();
            let result = planned.island.analyze(MAX_COMBINATIONS);
            Outcome { planned, result }
        })
        .collect()
}
