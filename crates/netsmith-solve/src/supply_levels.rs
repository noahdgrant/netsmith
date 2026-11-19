//! Input levels stated in rows by the supply (issues #1305 and #1307), read once the rails are
//! solved.
//!
//! A datasheet gives an input's thresholds in rows by its supply: `0.65 x VCCI` from 1.1 V to
//! 1.95 V and a fixed 2.0 V from 3.0 V to 3.6 V, or a Schmitt input's guaranteed thresholds at
//! 1.65 V and at 2.3 V. A part states them as the table is printed, `.vih &= 0.65 * .vcc.voltage
//! at 1.1V..1.95V, 2.0V at 3.0V..3.6V` (the owner's decision on #1305: steps over the supply, as
//! the steps over temperature of #1257), and the checker records the rows
//! (`netsmith_check::SupplySteps`) without narrowing the level. Here the rows are read at the
//! voltage of the level's reference rail (the `.reference` of its `Logic` or `Levels`), and the
//! level is written as the range its rows take over that supply:
//!
//! - Each row the rail's range reaches gives its value over the part of the range it holds at. A
//!   row's value may name the supply's voltage: every field it names that is the voltage of a rail
//!   whose `pos` is on the net of the reference rail's `pos` (the one supply, as ADR 0082 reads
//!   it) is held to that part of the range, so `0.65 * .vcc.voltage` of the row up to 1.95 V is at
//!   most 1.27 V on a rail of 1.8 V..2.5 V.
//! - A rail whose range spans two rows is judged against the worse (the owner's decision): the
//!   level is the hull of the rows it reaches, so the levels check holds a driver's high against
//!   the highest `vih` and its low against the lowest `vil`.
//! - Between two rows the datasheet guarantees nothing, and the level is linear in the supply
//!   only when the part says so (issue #1307, the owner's decision), with `interpolated` after the
//!   last row: from the value of a row at its highest supply to the value of the next at its
//!   lowest. Without it, and below the first row or above the last, a supply the range reaches is
//!   a supply no row gives the level at, and the level is left with no value: the levels check
//!   says it is not evaluated, and why.
//!
//! The level written is intersected with what else states it; an empty intersection is a fault
//! at the rows' statement. The rows are read before the DC analysis (which reads which receivers
//! state levels) and again before the levels check, for a reference rail that the analysis gave
//! a voltage. Not taken: rows in a catalog cell (no catalog column is an input level), and
//! output levels by the supply (a datasheet gives them at a load, `at 2mA`, ADR 0076).

use std::collections::{HashMap, HashSet};

use netsmith_check::{Diagnostic, Domain, FieldId, InstId, Member, Model, RExpr, SupplySteps};
use netsmith_quantity::{Dimension, Quantity};

use crate::eval::{Val, eval_with};

/// What a pass over the rows found: the faults, and for each level its rows could not give,
/// why (for the levels check's note).
#[derive(Debug, Default)]
pub(crate) struct Read {
    pub faults: Vec<Diagnostic>,
    pub unread: HashMap<FieldId, String>,
}

/// Write each level stated in rows by the supply that is not written yet (`done`), from its
/// reference rail's voltage.
pub(crate) fn write(model: &mut Model, done: &mut HashSet<FieldId>) -> Read {
    let mut read = Read::default();
    let steps: Vec<SupplySteps> = model.supply_steps().to_vec();
    if steps.is_empty() {
        return read;
    }
    let net_of = nets_by_pin(model);
    for s in &steps {
        if done.contains(&s.field) {
            continue;
        }
        match level_of(model, &net_of, s) {
            Ok(q) => {
                done.insert(s.field);
                let path = model.path_of_field(s.field);
                let value = match &model.field(s.field).domain {
                    Domain::Quantity(d) => match d.intersect(&q) {
                        Ok(Some(both)) => both,
                        _ => {
                            read.faults.push(Diagnostic {
                                source: s.source,
                                span: s.span,
                                message: format!(
                                    "`{path}` is {q} by its rows at the supply its reference rail has, outside the {d} it is also stated to be: a level has one value, so keep the rows or the other statement"
                                ),
                            });
                            continue;
                        }
                    },
                    _ => q,
                };
                model.set_domain(s.field, Domain::Quantity(value));
            }
            Err(why) => {
                read.unread.insert(s.field, why);
            }
        }
    }
    read
}

/// The net of each pin, by its place in `Model::nets`.
fn nets_by_pin(model: &Model) -> HashMap<InstId, usize> {
    let mut out = HashMap::new();
    for (i, net) in model.nets().iter().enumerate() {
        for &t in &net.terminals {
            out.insert(t, i);
        }
    }
    out
}

/// The range a level's rows give it over its reference rail's voltage, or why they give none.
fn level_of(
    model: &Model,
    net_of: &HashMap<InstId, usize>,
    s: &SupplySteps,
) -> Result<Quantity, String> {
    let owner = model.field(s.field).owner;
    let path = model.path_of_field(s.field);
    let rail = match model.member(owner, "reference") {
        Some(Member::Inst(r)) => r,
        _ => {
            return Err(format!(
                "`{path}` has no reference rail to read its rows at"
            ));
        }
    };
    let Some((voltage, pos, _)) = model.rail_of(rail) else {
        return Err(format!(
            "`{path}` has no reference rail to read its rows at"
        ));
    };
    let rail_path = model.path_of_field(voltage);
    let (rlo, rhi) = match &model.field(voltage).domain {
        Domain::Quantity(q) if q.lo().is_finite() && q.hi().is_finite() => (q.lo(), q.hi()),
        _ => {
            return Err(format!(
                "`{path}` is stated in rows by the supply, and its supply `{rail_path}` has no voltage"
            ));
        }
    };
    // The fields of the rows that are the supply's voltage: the reference rail's, and that of
    // every rail whose `pos` is on its net (`.vcc.voltage` of a part whose `.a.reference ~ .vcc`).
    let supply_net = net_of.get(&pos).copied();
    let mut named = Vec::new();
    for r in &s.rows {
        r.value.fields(&mut named);
    }
    let mut supply: Vec<FieldId> = vec![voltage];
    for f in named {
        if supply.contains(&f) {
            continue;
        }
        if let Some((v, p, _)) = model.rail_of(model.field(f).owner)
            && v == f
            && supply_net.is_some()
            && net_of.get(&p).copied() == supply_net
        {
            supply.push(f);
        }
    }
    let at = |e: &RExpr, lo: f64, hi: f64| -> Result<(f64, f64), String> {
        let q = Quantity::interval(Dimension::VOLTAGE, lo, hi).map_err(|e| e.to_string())?;
        let values: Vec<(FieldId, Quantity)> = supply.iter().map(|&f| (f, q)).collect();
        match eval_with(model, e, &values) {
            Ok(Val::Q(v)) => Ok((v.lo(), v.hi())),
            Ok(Val::T(_)) => Err(format!("a row of `{path}` is not a voltage")),
            Err(err) => Err(format!("a row of `{path}` cannot be read: {err}")),
        }
    };
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut take = |(a, b): (f64, f64)| {
        lo = lo.min(a);
        hi = hi.max(b);
    };
    let mut uncovered: Vec<(f64, f64, bool)> = Vec::new();
    let first = &s.rows[0];
    let last = &s.rows[s.rows.len() - 1];
    if rlo < first.lo {
        uncovered.push((rlo, rhi.min(first.lo), false));
    }
    if rhi > last.hi {
        uncovered.push((rlo.max(last.hi), rhi, false));
    }
    for (i, r) in s.rows.iter().enumerate() {
        let (a, b) = (rlo.max(r.lo), rhi.min(r.hi));
        if a <= b {
            take(at(&r.value, a, b)?);
        }
        let Some(next) = s.rows.get(i + 1) else {
            continue;
        };
        // The gap between this row and the next (open at both ends, which the rows hold),
        // where the rail's range reaches into it.
        if r.hi >= next.lo || rhi <= r.hi || rlo >= next.lo {
            continue;
        }
        let (g0, g1) = (rlo.max(r.hi), rhi.min(next.lo));
        if !s.interpolated {
            uncovered.push((g0, g1, true));
            continue;
        }
        let from = at(&r.value, r.hi, r.hi)?;
        let to = at(&next.value, next.lo, next.lo)?;
        let line = |v0: f64, v1: f64, x: f64| v0 + (v1 - v0) * (x - r.hi) / (next.lo - r.hi);
        for x in [g0, g1] {
            take((line(from.0, to.0, x), line(from.1, to.1, x)));
        }
    }
    if !uncovered.is_empty() {
        let pieces: Vec<String> = uncovered
            .iter()
            .map(|&(a, b, _)| range_text(a, b))
            .collect();
        let between = uncovered.iter().any(|&(_, _, between)| between);
        let hint = if between {
            " (between two rows the datasheet guarantees nothing; a part whose datasheet says the level is linear there states `interpolated` after the last row)"
        } else {
            ""
        };
        return Err(format!(
            "`{path}` is stated in rows by the supply, and no row gives it at {} of the {} of `{rail_path}`{hint}",
            pieces.join(" and "),
            range_text(rlo, rhi),
        ));
    }
    Quantity::interval(Dimension::VOLTAGE, lo, hi).map_err(|e| e.to_string())
}

/// A range of volts for a message, or one voltage.
fn range_text(lo: f64, hi: f64) -> String {
    let point = |v| Quantity::point(Dimension::VOLTAGE, v);
    if lo == hi {
        return point(lo).map_or_else(|_| format!("{lo} V"), |q| q.to_string());
    }
    match (point(lo), point(hi)) {
        (Ok(a), Ok(b)) => {
            Quantity::range(&a, &b).map_or_else(|_| format!("{lo} V..{hi} V"), |q| q.to_string())
        }
        _ => format!("{lo} V..{hi} V"),
    }
}
