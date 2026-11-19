//! A charger joined to a rechargeable cell (issue #1262), checked after the solve.
//!
//! A part that stands for `std::parts::regulator::Charger` whose `.out` rail is on the net of
//! the `.terminals` of a `std::sources::Cell` (the two rails joined with `~`, directly or through
//! a module's rail) charges that cell, and the solved values of the two are held against each
//! other:
//!
//! - **Float voltage.** The charger's `.float_voltage` must lie within the cell's, both ends:
//!   a charger that may hold the cell above the most the cell allows overcharges it, and one
//!   that may stop below the least leaves it short of the charge its capacity is rated at.
//! - **Thermistor window.** The charger's `.thermistor_window`, the cell temperatures it charges
//!   between, must lie within the cell's `.charge_temperature`, both ends. A charger that states
//!   no window (one with no thermistor input) is not judged on it (ADR 0074).
//! - **Charge current.** The most of the charger's `.charge_current` (set by its programming
//!   pin, so with the tolerance of the resistor there) must be at most the cell's
//!   `.max_charge_current`.
//!
//! A value of either side that has none after the solve is listed as not evaluated. The cell's
//! `.discharge_temperature` is not read, and the time a charge takes and what the charger takes
//! from its supply are `charging`'s (issue #1264).

use netsmith_check::{Diagnostic, Domain, InstId, InstKind, Member, Model, SourceId};
use netsmith_core::Cause;
use netsmith_lang::Span;
use netsmith_quantity::{Dimension, Quantity};

use crate::levels::NotEvaluated;

/// The faults of the chargers joined to cells, and the checks that could not be made.
pub(crate) fn check(model: &Model) -> (Vec<Diagnostic>, Vec<NotEvaluated>) {
    let mut faults = Vec::new();
    let mut notes = Vec::new();
    let cells: Vec<(InstId, usize)> = model
        .instances()
        .filter(|(_, inst)| stands_for(inst, "Cell"))
        .filter_map(|(id, _)| Some((id, pos_net(model, rail(model, id, "terminals")?)?)))
        .collect();
    for (charger, inst) in model.instances() {
        if !stands_for(inst, "Charger") {
            continue;
        }
        let Some(out_net) = rail(model, charger, "out").and_then(|r| pos_net(model, r)) else {
            continue;
        };
        for &(cell, _) in cells.iter().filter(|(_, net)| *net == out_net) {
            judge(model, charger, cell, &mut faults, &mut notes);
        }
    }
    (faults, notes)
}

/// Whether an instance is a part that stands for the standard generic `name`.
pub(crate) fn stands_for(inst: &netsmith_check::Instance, name: &str) -> bool {
    inst.kind == InstKind::Part && inst.standard.iter().any(|s| s == name)
}

fn judge(
    model: &Model,
    charger: InstId,
    cell: InstId,
    faults: &mut Vec<Diagnostic>,
    notes: &mut Vec<NotEvaluated>,
) {
    let c = model.path_of(charger);
    let b = model.path_of(cell);
    let mut unknown = Vec::new();

    // The float voltage, both ends within the cell's.
    match (
        known(model, charger, "float_voltage"),
        known(model, cell, "float_voltage"),
    ) {
        (Some(ours), Some(theirs)) => {
            if !within(ours, theirs) {
                let (source, span) = statement_of(model, charger, "float_voltage");
                faults.push(Diagnostic {
                    source,
                    span,
                    message: format!(
                        "`{c}` charges the cell `{b}` to {} (`{c}.float_voltage`), not within the {} the cell is charged to (`{b}.float_voltage`): {}; use a charger whose regulation voltage the cell allows, or a cell rated for it",
                        range(Dimension::VOLTAGE, ours),
                        range(Dimension::VOLTAGE, theirs),
                        outside(Dimension::VOLTAGE, ours, theirs),
                    ),
                });
            }
        }
        (ours, theirs) => {
            if ours.is_none() {
                unknown.push(format!("`{c}.float_voltage`"));
            }
            if theirs.is_none() {
                unknown.push(format!("`{b}.float_voltage`"));
            }
        }
    }

    // The thermistor window, within the cell's charge window. A charger that states none has no
    // thermistor input, and is not judged on it.
    if stated(model, charger, "thermistor_window") {
        match (
            known(model, charger, "thermistor_window"),
            known(model, cell, "charge_temperature"),
        ) {
            (Some(ours), Some(theirs)) => {
                if !within(ours, theirs) {
                    let (source, span) = statement_of(model, charger, "thermistor_window");
                    faults.push(Diagnostic {
                        source,
                        span,
                        message: format!(
                            "`{c}` charges the cell `{b}` between {} (`{c}.thermistor_window`), not within the {} the cell may be charged in (`{b}.charge_temperature`): {}; choose the thermistor and its resistors so the charger stops outside the cell's window",
                            range(Dimension::TEMPERATURE, ours),
                            range(Dimension::TEMPERATURE, theirs),
                            outside(Dimension::TEMPERATURE, ours, theirs),
                        ),
                    });
                }
            }
            (ours, theirs) => {
                if ours.is_none() {
                    unknown.push(format!("`{c}.thermistor_window`"));
                }
                if theirs.is_none() {
                    unknown.push(format!("`{b}.charge_temperature`"));
                }
            }
        }
    }

    // The charge current, at its most, within the cell's limit.
    match (
        known(model, charger, "charge_current"),
        known(model, cell, "max_charge_current"),
    ) {
        (Some(ours), Some(limit)) => {
            if ours.hi() > limit.hi() * (1.0 + 1e-9) {
                let (source, span) = statement_of(model, charger, "charge_current");
                faults.push(Diagnostic {
                    source,
                    span,
                    message: format!(
                        "`{c}` charges the cell `{b}` at up to {} (`{c}.charge_current`, {}), above the {} the cell allows (`{b}.max_charge_current`); lower the charge current its programming pin sets, or use a cell rated for it",
                        show(Dimension::CURRENT, ours.hi()),
                        range(Dimension::CURRENT, ours),
                        show(Dimension::CURRENT, limit.hi()),
                    ),
                });
            }
        }
        (ours, limit) => {
            if ours.is_none() {
                unknown.push(format!("`{c}.charge_current`"));
            }
            if limit.is_none() {
                unknown.push(format!("`{b}.max_charge_current`"));
            }
        }
    }

    if !unknown.is_empty() {
        notes.push(NotEvaluated {
            at: charger,
            involves: vec![charger, cell],
            message: format!(
                "the charger `{c}` is not judged against the cell `{b}` it charges in full: {} {} no value; state {}",
                unknown.join(", "),
                if unknown.len() == 1 { "has" } else { "have" },
                if unknown.len() == 1 { "it" } else { "them" },
            ),
            cause: Cause::NotStated,
        });
    }
}

/// The rail member `name` of an instance.
pub(crate) fn rail(model: &Model, inst: InstId, name: &str) -> Option<InstId> {
    match model.member(inst, name) {
        Some(Member::Inst(r)) => Some(r),
        _ => None,
    }
}

/// The net (its place in `Model::nets`) of a rail's `pos`.
pub(crate) fn pos_net(model: &Model, rail: InstId) -> Option<usize> {
    let (_, pos, _) = model.rail_of(rail)?;
    model.nets().iter().position(|n| n.pins.contains(&pos))
}

/// The value of a member of an instance, when it is a quantity that has one.
pub(crate) fn known(model: &Model, inst: InstId, name: &str) -> Option<Quantity> {
    let Some(Member::Field(f)) = model.member(inst, name) else {
        return None;
    };
    match &model.field(f).domain {
        Domain::Quantity(q) if q.lo().is_finite() && q.hi().is_finite() => Some(*q),
        _ => None,
    }
}

/// Whether an optional member has a value or a statement: one with neither was left open.
fn stated(model: &Model, inst: InstId, name: &str) -> bool {
    let Some(Member::Field(f)) = model.member(inst, name) else {
        return false;
    };
    let field = model.field(f);
    !field.narrowed_at.is_empty() || matches!(field.domain, Domain::Quantity(_))
}

/// Where a member was stated, or the instance when it was not.
pub(crate) fn statement_of(model: &Model, inst: InstId, name: &str) -> (SourceId, Span) {
    if let Some(Member::Field(f)) = model.member(inst, name)
        && let Some(&at) = model.field(f).narrowed_at.last()
    {
        return at;
    }
    model.instance(inst).at()
}

/// Whether `inner` lies within `outer`, both ends, to a part in a billion.
fn within(inner: Quantity, outer: Quantity) -> bool {
    let slack = 1e-9 * outer.lo().abs().max(outer.hi().abs()).max(1e-12);
    inner.lo() >= outer.lo() - slack && inner.hi() <= outer.hi() + slack
}

/// What of `inner` lies outside `outer`: the ends past each of its ends.
fn outside(dim: Dimension, inner: Quantity, outer: Quantity) -> String {
    let mut parts = Vec::new();
    if inner.hi() > outer.hi() {
        parts.push(format!(
            "{} is above {}",
            show(dim, inner.hi()),
            show(dim, outer.hi())
        ));
    }
    if inner.lo() < outer.lo() {
        parts.push(format!(
            "{} is below {}",
            show(dim, inner.lo()),
            show(dim, outer.lo())
        ));
    }
    parts.join(" and ")
}

pub(crate) fn range(dim: Dimension, q: Quantity) -> String {
    if (q.hi() - q.lo()).abs() <= 1e-12 * q.hi().abs().max(1e-12) {
        show(dim, q.lo())
    } else {
        format!("{} to {}", show(dim, q.lo()), show(dim, q.hi()))
    }
}

pub(crate) fn show(dim: Dimension, value: f64) -> String {
    if dim == Dimension::TEMPERATURE {
        let c = value - netsmith_quantity::CELSIUS_ZERO_K;
        let c = (c * 1e6).round() / 1e6;
        return format!("{c}degC");
    }
    match Quantity::point(dim, value) {
        Ok(q) => q.to_string(),
        Err(_) => format!("{value}"),
    }
}
