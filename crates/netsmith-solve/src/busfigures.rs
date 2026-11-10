//! The parts of an I2C bus that state no figure for the pull-up (ADR 0102).
//!
//! The checker derives two things from the `I2c` interfaces that share a net with an
//! `I2cPullups` (`Checker::derive_bus_figures`): the bus capacitance, the sum of the pins and the
//! board's own estimate of the traces, and a bound on the resistance from each part's sink
//! current. A part that states no figure drops its own relation, silently (ADR 0074), so the
//! note here names it: a check that was asked for and not made is never a pass.
//!
//! - The capacitance is asked for when a bound that reads it or the rise time waits for it. The
//!   note says which part states no `.capacitance`, whether the board states no estimate for the
//!   traces, and whether an end of the bus is a peripheral, which has no interface to state it.
//! - The sink current is asked for by the parts: the note is made when at least one part states
//!   its `.sink_current` and another does not, since the bound was made for the one and not the
//!   other. A bus where no part states one has no bound to be missed (as before), and a board
//!   that states the figure itself is judged on that.

use netsmith_check::{Domain, FieldId, InstId, InstKind, Model, bus_beside};

use crate::levels::NotEvaluated;

/// Whether a field has a value (a statement, a pick or a worked-out value gave it).
fn has_value(model: &Model, f: Option<FieldId>) -> bool {
    f.is_some_and(|f| model.field(f).domain != Domain::Unset)
}

/// The paths of `parts` for a message, quoted.
fn names(model: &Model, parts: &[InstId]) -> String {
    let quoted: Vec<String> = parts
        .iter()
        .map(|&p| format!("`{}`", model.path_of(p)))
        .collect();
    match quoted.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// The notes of the pull-ups of a board. `deferred` is the relations that were not evaluated.
pub(crate) fn notes(model: &Model, deferred: &[usize]) -> Vec<NotEvaluated> {
    let mut out = Vec::new();
    let pullups: Vec<InstId> = model
        .instances()
        .filter(|(_, i)| i.kind == InstKind::Module && i.type_name == "I2cPullups")
        .map(|(id, _)| id)
        .collect();
    for pullup in pullups {
        let Some(netsmith_check::Member::Inst(bus)) = model.member(pullup, "bus") else {
            continue;
        };
        let pins: Vec<InstId> = ["sda", "scl"]
            .iter()
            .filter_map(|n| match model.member(bus, n) {
                Some(netsmith_check::Member::Inst(p)) => Some(p),
                _ => None,
            })
            .collect();
        let ends = bus_beside(model, "I2c", &pins);
        let path = model.path_of(pullup);
        let owners = |ends: &[InstId]| -> Vec<InstId> {
            ends.iter()
                .map(|&e| model.owner_of(e).unwrap_or(e))
                .collect()
        };
        let mut note = |message: String, involves: Vec<InstId>| {
            out.push(NotEvaluated {
                at: pullup,
                involves,
                message,
                cause: netsmith_core::Cause::NotStated,
            });
        };

        // The capacitance, when a check that reads it waits.
        let total = model.field_in(pullup, "bus_capacitance");
        let rise = model.field_in(pullup, "rise_time");
        let waits = deferred.iter().any(|&i| {
            let r = &model.relations()[i];
            let mut read = Vec::new();
            r.lhs.fields(&mut read);
            r.rhs.fields(&mut read);
            read.iter().any(|&f| Some(f) == total || Some(f) == rise)
        });
        if waits && !has_value(model, total) {
            let missing: Vec<InstId> = ends
                .interfaces
                .iter()
                .copied()
                .filter(|&i| !has_value(model, model.field_in(i, "capacitance")))
                .collect();
            let mut why: Vec<String> = Vec::new();
            if ends.interfaces.is_empty() && ends.peripherals.is_empty() {
                why.push("no `I2c` interface of a part is on the bus".to_string());
            }
            if !missing.is_empty() {
                why.push(format!(
                    "{} states no `.capacitance`",
                    names(model, &missing)
                ));
            }
            for (part, name) in &ends.peripherals {
                why.push(format!(
                    "`{}` is on the bus as peripheral `{name}`, which has no `.capacitance` to state",
                    model.path_of(*part)
                ));
            }
            if !has_value(model, model.field_in(pullup, "trace_capacitance")) {
                why.push(format!(
                    "the board states no estimate of the traces, `.{path}.trace_capacitance &= <range>`"
                ));
            }
            if !why.is_empty() {
                let mut involves = owners(&missing);
                involves.extend(ends.peripherals.iter().map(|(p, _)| *p));
                note(
                    format!(
                        "not evaluated: the capacitance of the bus of `{path}` is not derived, so the bounds on it and on its rise time are not checked: {}",
                        why.join("; ")
                    ),
                    involves,
                );
            }
        }

        // The sink current: some part states a figure and another does not.
        let sinks: Vec<(InstId, bool)> = ends
            .interfaces
            .iter()
            .map(|&i| (i, has_value(model, model.field_in(i, "sink_current"))))
            .collect();
        let missing: Vec<InstId> = sinks.iter().filter(|s| !s.1).map(|s| s.0).collect();
        let by_hand = has_value(model, model.field_in(pullup, "sink_current"))
            || has_value(model, model.field_in(pullup, "low_level"));
        if !by_hand && !missing.is_empty() && missing.len() < sinks.len() {
            note(
                format!(
                    "not evaluated: the least pull-up of the bus of `{path}` from the sink current of {}: it states no `.sink_current`, so the bound is made for the other parts of the bus only",
                    names(model, &missing)
                ),
                owners(&missing),
            );
        }
    }
    out
}
