//! The time a charger takes to charge the storage on its output, and the current it takes from
//! the source in front of it while it does (issue #1264), checked after the solve.
//!
//! A part that stands for `std::parts::regulator::Charger` charges at its `.charge_current`: the
//! current its programming pin sets (the datasheet's equation over the resistor there, so with
//! the resistor's tolerance) or a limit the part states (`.charge_current &= 500mA`). It is
//! taken as a constant-current source, at the least of that range, up to its float voltage.
//!
//! - **Charge time.** When the connection on the charger's `.out` states `.charge_time_limit`
//!   (the cell's terminals or the charger's output, which `~` joins), the storage on that net is
//!   charged in the worst case and the time held against the most of the limit. The storage is
//!   each `std::sources::Cell` whose terminals are on the net, by its `.capacity` (charged from
//!   empty: the cell's voltage over its state of charge is not modelled), and each part with a
//!   `bias` across the rail's two nets (a supercapacitor, a capacitor), by the most of its
//!   `effective_capacitance` (`total_effective_capacitance` for a bank) charged from the start
//!   to the end of the charge. The charge starts at the least of the rail's window (its
//!   `.voltage_floor`, or the least of `.voltage`) and ends at the top of `.voltage`, or at the
//!   least of the charger's `.float_voltage` when that is lower: the voltage the charger is sure
//!   to reach. The storage's `.leakage_current` (its most, where a part has one) is taken from
//!   the charge current for the whole charge. So the time is
//!   `(capacity + C * (end - start)) / (min(charge_current) - leakage)`, and a charge current at
//!   or below the leakage never ends the charge.
//! - **The source's budget.** While the charger charges, it takes the most of `.vin.current`
//!   when the part states it, and otherwise the most of its charge current (a linear charger
//!   takes it all; its own quiescent current is not added), and the other loads on the net keep
//!   drawing. The sum is held against `.vin.max_peak_current`, the most the source behind the
//!   rail gives (a regulator states it for its output, and `~` shares it). The other loads are
//!   counted as for a burst (`burst::other_loads`): each load of the balance the rail is a term
//!   of at its most, and the loads beyond a module's port; a term with no value (the source in
//!   front, whose current waits on the charger's) counts nothing.
//!
//! A limit or a source's maximum that is not stated is not checked, and nothing is said (ADR
//! 0074). A value the check needs that has none after the solve is listed as not evaluated.
//!
//! Not modelled: the constant-voltage tail of a charge, which makes a real charge of a cell
//! longer than its capacity over the current; the charger's preconditioning, its thermal
//! regulation and its own current (a part states `.vin.current` for it); and a load on the
//! charged rail during the charge.

use netsmith_check::{Diagnostic, Domain, FieldId, InstId, InstKind, Member, Model};
use netsmith_core::Cause;
use netsmith_quantity::Dimension;

use crate::charge::{known, pos_net, rail, range, show, stands_for, statement_of};
use crate::levels::NotEvaluated;

/// The faults of the chargers' charge time and budget, and the checks that could not be made.
pub(crate) fn check(model: &Model) -> (Vec<Diagnostic>, Vec<NotEvaluated>) {
    let mut faults = Vec::new();
    let mut notes = Vec::new();
    for (charger, inst) in model.instances() {
        if !stands_for(inst, "Charger") {
            continue;
        }
        charge_time(model, charger, &mut faults, &mut notes);
        budget(model, charger, &mut faults, &mut notes);
    }
    (faults, notes)
}

/// What a charge must fill, found on the net of the charger's output.
struct Storage {
    /// The cells' capacity, in coulombs, and their paths.
    capacity: f64,
    cells: Vec<String>,
    /// The most capacitance across the rail, and the parts that give it.
    capacitance: f64,
    capacitors: Vec<String>,
    /// The most leakage of the parts across the rail.
    leakage: f64,
    involves: Vec<InstId>,
    unknown: Vec<String>,
}

fn storage(model: &Model, out: InstId) -> Option<Storage> {
    let (_, pos, neg) = model.rail_of(out)?;
    let nets = model.nets();
    let net_of = |pin: InstId| nets.iter().position(|n| n.pins.contains(&pin));
    let (out_net, neg_net) = (net_of(pos)?, net_of(neg)?);
    let on = |pin: InstId, net: usize| nets[net].pins.contains(&pin);
    let mut s = Storage {
        capacity: 0.0,
        cells: Vec::new(),
        capacitance: 0.0,
        capacitors: Vec::new(),
        leakage: 0.0,
        involves: Vec::new(),
        unknown: Vec::new(),
    };
    for (part, inst) in model.instances() {
        if stands_for(inst, "Cell") {
            let Some(terminals) = rail(model, part, "terminals") else {
                continue;
            };
            if pos_net(model, terminals) != Some(out_net) {
                continue;
            }
            let path = model.path_of(part);
            s.involves.push(part);
            match known(model, part, "capacity") {
                Some(q) if q.dim() == Dimension::CHARGE => s.capacity += q.hi().max(0.0),
                _ => s.unknown.push(format!("`{path}.capacity`")),
            }
            s.cells.push(path);
            continue;
        }
        let Some((_, a, b)) = model.bias_of(part) else {
            continue;
        };
        if !((on(a, out_net) && on(b, neg_net)) || (on(a, neg_net) && on(b, out_net))) {
            continue;
        }
        let path = model.path_of(part);
        s.involves.push(part);
        let c = known(model, part, "total_effective_capacitance")
            .or_else(|| known(model, part, "effective_capacitance"));
        match c {
            Some(c) if c.dim() == Dimension::CAPACITANCE => s.capacitance += c.hi().max(0.0),
            _ => s.unknown.push(format!("`{path}.effective_capacitance`")),
        }
        // A part with no leakage field (a ceramic capacitor) leaks nothing the check counts.
        if let Some(Member::Field(f)) = model.member(part, "leakage_current") {
            match &model.field(f).domain {
                Domain::Quantity(q) if q.hi().is_finite() => s.leakage += q.hi().max(0.0),
                _ => s.unknown.push(format!("`{path}.leakage_current`")),
            }
        }
        s.capacitors.push(path);
    }
    Some(s)
}

fn charge_time(
    model: &Model,
    charger: InstId,
    faults: &mut Vec<Diagnostic>,
    notes: &mut Vec<NotEvaluated>,
) {
    let Some(out) = rail(model, charger, "out") else {
        return;
    };
    let Some(limit) = most(model, out, "charge_time_limit") else {
        return;
    };
    let c = model.path_of(charger);
    let Some(store) = storage(model, out) else {
        return;
    };
    let mut unknown = store.unknown.clone();
    let current = known(model, charger, "charge_current");
    let float = known(model, charger, "float_voltage");
    let voltage = known(model, out, "voltage");
    if current.is_none() {
        unknown.push(format!("`{c}.charge_current`"));
    }
    if float.is_none() {
        unknown.push(format!("`{c}.float_voltage`"));
    }
    if voltage.is_none() {
        unknown.push(format!("`{c}.out.voltage`"));
    }
    let mut involves = vec![charger];
    involves.extend(store.involves.iter().copied());
    if store.cells.is_empty() && store.capacitors.is_empty() {
        notes.push(NotEvaluated {
            at: charger,
            involves,
            message: format!(
                "the charge time of `{c}` is not judged against `{c}.out.charge_time_limit`: no cell and no capacitor is on the net of `{c}.out`, so there is nothing to charge; join the cell's terminals to it with `~`, or place the storage capacitor across it"
            ),
            cause: Cause::NotStated,
        });
        return;
    }
    let (Some(current), Some(float), Some(voltage)) = (current, float, voltage) else {
        unknown.dedup();
        notes.push(not_judged(&c, involves, &unknown));
        return;
    };
    if !unknown.is_empty() {
        unknown.dedup();
        notes.push(not_judged(&c, involves, &unknown));
        return;
    }
    let start = known(model, out, "voltage_floor")
        .map(|f| f.lo())
        .unwrap_or(voltage.lo());
    let end = voltage.hi().min(float.lo());
    let swing = (end - start).max(0.0);
    let charge = store.capacity + store.capacitance * swing;
    let net = current.lo() - store.leakage;

    let mut what = Vec::new();
    for cell in &store.cells {
        what.push(format!("the cell `{cell}`"));
    }
    if !store.capacitors.is_empty() {
        what.push(format!(
            "{} across the rail ({}) from {} to {}",
            show(Dimension::CAPACITANCE, store.capacitance),
            store
                .capacitors
                .iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", "),
            show(Dimension::VOLTAGE, start),
            show(Dimension::VOLTAGE, end),
        ));
    }
    let what = what.join(" and ");
    let (source, span) = statement_of(model, out, "charge_time_limit");
    if net <= 0.0 {
        faults.push(Diagnostic {
            source,
            span,
            message: format!(
                "`{c}` never finishes charging {what}: it charges at as little as {} (`{c}.charge_current`, {}), and the storage leaks up to {}; raise the charge current its programming pin sets, or use storage that leaks less",
                show(Dimension::CURRENT, current.lo()),
                range(Dimension::CURRENT, current),
                show(Dimension::CURRENT, store.leakage),
            ),
        });
        return;
    }
    let time = charge / net;
    if time <= limit * (1.0 + 1e-9) {
        return;
    }
    let leak = if store.leakage > 0.0 {
        format!(
            " less the {} the storage leaks",
            show(Dimension::CURRENT, store.leakage)
        )
    } else {
        String::new()
    };
    let need = charge / limit + store.leakage;
    faults.push(Diagnostic {
        source,
        span,
        message: format!(
            "`{c}` takes up to {}{} to charge {what}: {} at as little as {} (`{c}.charge_current`, {}){leak}, longer than the {} of `{c}.out.charge_time_limit`; a charge in that time needs at least {} of charge current, or less storage",
            show(Dimension::TIME, time),
            hours(time),
            show(Dimension::CHARGE, charge),
            show(Dimension::CURRENT, current.lo()),
            range(Dimension::CURRENT, current),
            show(Dimension::TIME, limit),
            show(Dimension::CURRENT, need),
        ),
    });
}

fn not_judged(c: &str, involves: Vec<InstId>, unknown: &[String]) -> NotEvaluated {
    NotEvaluated {
        at: involves[0],
        involves,
        message: format!(
            "the charge time of `{c}` is not judged against `{c}.out.charge_time_limit`: {} {} no value; state {}",
            unknown.join(", "),
            if unknown.len() == 1 { "has" } else { "have" },
            if unknown.len() == 1 { "it" } else { "them" },
        ),
        cause: Cause::NotStated,
    }
}

fn budget(
    model: &Model,
    charger: InstId,
    faults: &mut Vec<Diagnostic>,
    notes: &mut Vec<NotEvaluated>,
) {
    let Some(vin) = rail(model, charger, "vin") else {
        return;
    };
    let Some(limit) = known(model, vin, "max_peak_current") else {
        return;
    };
    let c = model.path_of(charger);
    let Some(current) = known(model, charger, "charge_current") else {
        notes.push(NotEvaluated {
            at: charger,
            involves: vec![charger],
            message: format!(
                "what `{c}` takes from its supply while it charges is not judged against `{c}.vin.max_peak_current`: `{c}.charge_current` has no value; state it"
            ),
            cause: Cause::NotStated,
        });
        return;
    };
    let Some(Member::Field(drawn)) = model.member(vin, "current") else {
        return;
    };
    // A part that states what it takes from `.vin` (a switching charger takes less than it
    // gives) is read by that; one that does not takes its charge current.
    let stated = known(model, vin, "current");
    let ours = stated.map_or(current.hi(), |q| q.hi());
    let why = match stated {
        Some(q) => format!("`{c}.vin.current`, {}", range(Dimension::CURRENT, q)),
        None => format!(
            "`{c}.charge_current`, {}",
            range(Dimension::CURRENT, current)
        ),
    };
    let Some(others) = other_loads(model, drawn) else {
        notes.push(NotEvaluated {
            at: charger,
            involves: vec![charger],
            message: format!(
                "what `{c}` takes from its supply while it charges is not judged against `{c}.vin.max_peak_current`: `{c}.vin` is a term of no balance of currents"
            ),
            cause: Cause::NotStated,
        });
        return;
    };
    let demand = ours + others;
    if demand <= limit.lo() * (1.0 + 1e-9) {
        return;
    }
    let (source, span) = statement_of(model, charger, "charge_current");
    faults.push(Diagnostic {
        source,
        span,
        message: format!(
            "while `{c}` charges it takes up to {} from `{c}.vin` ({why}) and the other loads on the net draw up to {}, so the source gives up to {}, above the {} it can give (`{c}.vin.max_peak_current`); lower the charge current, or supply the charger from a source that gives more",
            show(Dimension::CURRENT, ours),
            show(Dimension::CURRENT, others),
            show(Dimension::CURRENT, demand),
            show(Dimension::CURRENT, limit.lo()),
        ),
    });
}

/// A time of an hour or more in hours too, to two places, for the message.
fn hours(seconds: f64) -> String {
    if seconds >= 3600.0 {
        format!(" ({:.2} h)", seconds / 3600.0)
    } else {
        String::new()
    }
}

/// The most of a member's value, when it has a finite one: a limit written `..10.8ks` has no
/// least.
fn most(model: &Model, inst: InstId, name: &str) -> Option<f64> {
    let Some(Member::Field(f)) = model.member(inst, name) else {
        return None;
    };
    match &model.field(f).domain {
        Domain::Quantity(q) if q.hi().is_finite() => Some(q.hi()),
        _ => None,
    }
}

/// The most the loads other than `current` draw on its net: as `burst::other_loads`, except
/// that a term with no value counts nothing. The charger's own current is most often unstated,
/// so the source in front of it has none either, and a load that states none is the balance's
/// to report. `None` when the rail is a term of no balance.
fn other_loads(model: &Model, current: FieldId) -> Option<f64> {
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
            beyond += crate::burst::other_loads(model, d.field, 1).unwrap_or(0.0);
            continue;
        }
        if let Domain::Quantity(q) = &model.field(d.field).domain
            && q.lo() >= 0.0
            && q.hi() > 0.0
            && q.hi().is_finite()
        {
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
