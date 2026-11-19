//! A load that draws in bursts (issue #1279), checked after the rails are solved.
//!
//! A `Rail` that states `.burst` (how long one burst lasts) is a load whose `.current` is its
//! average and whose `.peak_current` is what it draws during a burst: a radio that transmits
//! 1.2 A for a few milliseconds and averages 50 mA. The balance, a regulator's dissipation and
//! every other check of the steady state read the average; the peak is what the source must
//! give. `Rail`'s own `.peak_current <= .max_peak_current` holds the peak alone against the
//! source (and steers the pick of a regulator by it); this check adds what that one cannot see:
//!
//! - **The converter's limit during the burst.** While the burst lasts the other loads on the
//!   net keep drawing, so the source gives the peak and their steady current at once. The
//!   others are the loads of the balance the rail is a term of, each at the most it draws, and
//!   a module's port stands for the loads beyond it (the balance of the module's parent, up to
//!   the board); a group of loads that take turns (`shared`) counts its worst members. The sum
//!   is held against the rail's `.max_peak_current`.
//! - **The bulk capacitance.** What the source cannot give comes from the capacitors across the
//!   rail (a part with a `bias` between the rail's `pos` and `neg` nets, its
//!   `total_effective_capacitance` for a bank and its `effective_capacitance` otherwise, at the
//!   least of each). They give the excess for the burst, so the rail falls by
//!   `excess * burst / C` from the least of its steady voltage, and it must stay at or above
//!   the least voltage the parts on the net need: the highest number a `.voltage >= ...` of a
//!   rail on the net states (a constant, or a field whose value is known). A net whose parts
//!   state no least voltage cannot lend the burst anything, and the burst is refused as the
//!   source's limit; the message says what to state.
//!
//! A peak that is already above the source's limit alone is left to `Rail`'s own relation,
//! which says so. A rail that states no `.burst`, no `.peak_current` or whose source states no
//! `.max_peak_current` is not looked at, and nothing is said (ADR 0074). A burst whose other
//! loads have no value yet is listed as not evaluated.
//!
//! Not built: the burst reflected through a converter to its input, where the input's
//! capacitance and a limited source in front hold it (issue #1340); the corners of a capacitor's
//! life and temperature (issue #1256), which the capacitance here does not read; the ESR of the
//! capacitors, which adds a step to the fall.

use netsmith_check::{Diagnostic, Domain, FieldId, InstId, InstKind, Member, Model, RExpr};
use netsmith_core::Cause;
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::{Dimension, Quantity};

use crate::levels::NotEvaluated;

/// The faults of the loads that burst, and the checks that could not be made.
pub(crate) fn check(model: &Model) -> (Vec<Diagnostic>, Vec<NotEvaluated>) {
    let mut faults = Vec::new();
    let mut notes = Vec::new();
    for (rail, inst) in model.instances() {
        if inst.kind != InstKind::Interface || inst.type_name != "Rail" {
            continue;
        }
        let Some(burst) = known(model, rail, "burst") else {
            continue;
        };
        let (Some(peak), Some(limit)) = (
            known(model, rail, "peak_current"),
            known(model, rail, "max_peak_current"),
        ) else {
            continue;
        };
        let Some(Member::Field(current)) = model.member(rail, "current") else {
            continue;
        };
        let path = model.path_of(rail);
        // A peak above the limit alone is `Rail`'s own relation's to say.
        if peak.hi() > limit.lo() * (1.0 + 1e-9) {
            continue;
        }
        let Some(others) = other_loads(model, current, 0) else {
            notes.push(NotEvaluated {
                at: rail,
                involves: vec![rail],
                message: format!(
                    "the burst of `{path}` is not judged against its source's limit: the currents of the other loads on its net are not known, so what they draw during the burst cannot be added to `{path}.peak_current`; state them"
                ),
                cause: Cause::NotStated,
            });
            continue;
        };
        let demand = peak.hi() + others;
        let excess = demand - limit.lo();
        if excess <= 1e-9 * demand.abs().max(1e-12) {
            continue;
        }
        let (source, span) = statement_of(model, rail, "burst");
        let head = format!(
            "during a burst of `{path}` (`{path}.peak_current` {} for {}) the other loads on its net draw up to {}, so the source gives up to {}, above `{path}.max_peak_current` ({})",
            show(Dimension::CURRENT, peak.hi()),
            show(Dimension::TIME, burst.hi()),
            show(Dimension::CURRENT, others),
            show(Dimension::CURRENT, demand),
            show(Dimension::CURRENT, limit.lo()),
        );
        let Some((voltage, pos_net, neg_net)) = rail_nets(model, rail) else {
            continue;
        };
        let floor = least_voltage(model, pos_net, neg_net);
        let caps = capacitance_across(model, pos_net, neg_net);
        let message = match (floor, voltage) {
            (Some((floor, by)), Some(v)) => {
                let fall = if caps > 0.0 {
                    excess * burst.hi() / caps
                } else {
                    f64::INFINITY
                };
                if v.lo() - fall >= floor - 1e-9 * floor.abs() {
                    continue;
                }
                let held = if caps > 0.0 {
                    format!(
                        "the {} across the rail give the other {} for {}, and the rail falls by {} from {}",
                        show(Dimension::CAPACITANCE, caps),
                        show(Dimension::CURRENT, excess),
                        show(Dimension::TIME, burst.hi()),
                        show(Dimension::VOLTAGE, fall),
                        show(Dimension::VOLTAGE, v.lo()),
                    )
                } else {
                    format!(
                        "no capacitor across the rail gives the other {}",
                        show(Dimension::CURRENT, excess)
                    )
                };
                let need = excess * burst.hi() / (v.lo() - floor).max(0.0);
                let room = if v.lo() > floor {
                    format!(
                        ": holding the burst takes {} of effective capacitance across the rail",
                        show(Dimension::CAPACITANCE, need)
                    )
                } else {
                    String::new()
                };
                format!(
                    "{head}; {held}, below the {} that `{by}` needs{room}",
                    show(Dimension::VOLTAGE, floor),
                )
            }
            _ => format!(
                "{head}, and no part on the net states the least voltage it works at, so the capacitance across the rail cannot hold the rest: state it on the load's rail (`.{}.voltage >= ...`), or give the source a higher limit",
                path.rsplit('.').next().unwrap_or(&path)
            ),
        };
        faults.push(Diagnostic {
            source,
            span,
            message,
        });
    }
    (faults, notes)
}

/// The value of a member of an instance, when it is a quantity that has one.
fn known(model: &Model, inst: InstId, name: &str) -> Option<Quantity> {
    let Some(Member::Field(f)) = model.member(inst, name) else {
        return None;
    };
    match &model.field(f).domain {
        Domain::Quantity(q) => Some(*q),
        _ => None,
    }
}

/// Where a member was stated, or the instance when it was not.
fn statement_of(
    model: &Model,
    inst: InstId,
    name: &str,
) -> (netsmith_check::SourceId, netsmith_lang::Span) {
    if let Some(Member::Field(f)) = model.member(inst, name)
        && let Some(&at) = model.field(f).narrowed_at.last()
    {
        return at;
    }
    model.instance(inst).at()
}

/// The most the loads other than `current` draw on its net while it bursts: the terms of the
/// balance it is a term of that only draw (a term that may supply is a source), each at its most (a group of loads at its worst), and for
/// a port of the balance's module the loads beyond it in the parent's balance. `None` when a
/// current has no value or the rail is a term of no balance.
pub(crate) fn other_loads(model: &Model, current: FieldId, depth: usize) -> Option<f64> {
    if depth > 32 {
        return None;
    }
    let balance = model.balances().iter().find(|b| {
        !b.returns
            && model.instance(b.scope).kind != InstKind::System
            && b.draws.iter().any(|d| d.field == current && !d.flipped)
    })?;
    let mut loads = Vec::new();
    let mut beyond = 0.0;
    for d in &balance.draws {
        if d.field == current {
            continue;
        }
        if d.flipped {
            // The module's own port: what is beyond it draws too.
            beyond += other_loads(model, d.field, depth + 1).unwrap_or(0.0);
            continue;
        }
        let Domain::Quantity(q) = &model.field(d.field).domain else {
            return None;
        };
        // A load draws and never supplies; a term that may supply is a source (or the port of
        // a module seen from outside), and gives nothing to the sum.
        if q.lo() >= 0.0 && q.hi() > 0.0 {
            loads.push((*d, *q));
        }
    }
    let sum = if loads.is_empty() {
        0.0
    } else {
        model.sum_draws(&loads)?.hi().max(0.0)
    };
    Some(sum + beyond)
}

/// The rail's voltage, and the nets of its `pos` and `neg` (their places in `Model::nets`).
fn rail_nets(model: &Model, rail: InstId) -> Option<(Option<Quantity>, usize, usize)> {
    let (voltage, pos, neg) = model.rail_of(rail)?;
    let net_of = |pin: InstId| model.nets().iter().position(|n| n.pins.contains(&pin));
    let v = match &model.field(voltage).domain {
        Domain::Quantity(q) => Some(*q),
        _ => None,
    };
    Some((v, net_of(pos)?, net_of(neg)?))
}

/// The rails whose `pos` is on `pos_net` and `neg` on `neg_net`.
fn rails_across(model: &Model, pos_net: usize, neg_net: usize) -> Vec<(InstId, FieldId)> {
    let nets = model.nets();
    model
        .instances()
        .filter_map(|(id, _)| {
            let (v, pos, neg) = model.rail_of(id)?;
            (nets[pos_net].pins.contains(&pos) && nets[neg_net].pins.contains(&neg))
                .then_some((id, v))
        })
        .collect()
}

/// The highest least voltage a rail across the two nets states (`.voltage >= 3V`, or a field
/// whose value is known), and the rail that states it.
fn least_voltage(model: &Model, pos_net: usize, neg_net: usize) -> Option<(f64, String)> {
    let rails = rails_across(model, pos_net, neg_net);
    let value = |e: &RExpr| -> Option<f64> {
        match e {
            RExpr::Const(q) => Some(q.hi()),
            RExpr::Field(f) => match &model.field(*f).domain {
                Domain::Quantity(q) => Some(q.hi()),
                _ => None,
            },
            _ => None,
        }
    };
    let mut best: Option<(f64, String)> = None;
    for r in model.relations() {
        let (field, bound) = match (&r.lhs, r.op, &r.rhs) {
            (RExpr::Field(f), CmpOp::Ge, e) | (e, CmpOp::Le, RExpr::Field(f)) => (*f, e),
            _ => continue,
        };
        let Some(&(rail, _)) = rails.iter().find(|(_, v)| *v == field) else {
            continue;
        };
        let Some(floor) = value(bound) else { continue };
        if best.as_ref().is_none_or(|b| floor > b.0) {
            best = Some((floor, model.path_of(rail)));
        }
    }
    best
}

/// The effective capacitance across the two nets: each part with a `bias` whose terminals are on
/// them, a bank by its total, at the least of each. A capacitor whose capacitance has no value
/// counts nothing.
fn capacitance_across(model: &Model, pos_net: usize, neg_net: usize) -> f64 {
    let nets = model.nets();
    let on = |pin: InstId, net: usize| nets[net].pins.contains(&pin);
    let mut total = 0.0;
    for (part, _) in model.instances() {
        let Some((_, a, b)) = model.bias_of(part) else {
            continue;
        };
        if !((on(a, pos_net) && on(b, neg_net)) || (on(a, neg_net) && on(b, pos_net))) {
            continue;
        }
        let c = known(model, part, "total_effective_capacitance")
            .or_else(|| known(model, part, "effective_capacitance"));
        if let Some(c) = c
            && c.dim() == Dimension::CAPACITANCE
        {
            total += c.lo().max(0.0);
        }
    }
    total
}

fn show(dim: Dimension, value: f64) -> String {
    match Quantity::point(dim, value) {
        Ok(q) => q.to_string(),
        Err(_) => format!("{value}"),
    }
}
