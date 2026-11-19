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
//!   rail on the net states (a constant, a field whose value is known, or an expression of
//!   them). A net whose parts state no least voltage cannot lend the burst anything, and the
//!   burst is refused as the source's limit; the message says what to state.
//! - **The converter's input (issue #1340).** The converter that feeds the load's net draws
//!   the burst, scaled, from its input: the `dc linear_regulator` or `dc switching_regulator`
//!   whose output rail's `pos` is on the net, or reached from it through series parts (a
//!   `dc resistor` of at most 10 ohm, which a buck's inductor is, or a `dc load_switch`). The
//!   factor is read from the converter's own `dc` statement, so a project's converter is
//!   reflected as the standard library's are: a linear regulator draws what its output gives and
//!   its quiescent current; a switching regulator draws the output's power at the highest output
//!   voltage over the least input voltage and the least `.efficiency`, and its quiescent
//!   current. What the output gives is the burst and the other loads there, at most the output's
//!   `.max_peak_current` when it states one. On the input that is added to the other loads
//!   there (as on the output) and held against the input rail's `.max_peak_current`; the
//!   capacitance across the input holds the rest, as on the output, down to the highest least
//!   voltage a rail on the input states: a converter's own least input counts, its
//!   `.vin.voltage >= .ic.min_input_voltage` or `.vin.voltage >= min(.vout.voltage) + .dropout`,
//!   whatever expression of known values the bound is. The walk goes on to the converter in
//!   front of that one, up to eight converters. An input that states no limit is not judged and
//!   passes the whole of the burst on; one whose capacitance holds it passes on its limit.
//!   The current drawn is taken at the least steady input: a switching regulator draws a little
//!   more as its input falls during the burst, and that is not added.
//!
//! A peak that is already above the source's limit alone is left to `Rail`'s own relation,
//! which says so. A rail that states no `.burst` or no `.peak_current` is not looked at, nor a net whose source
//! states no `.max_peak_current`, and nothing is said (ADR 0074). A burst whose other
//! loads have no value yet is listed as not evaluated.
//!
//! Not built: the corners of a capacitor's life and temperature (issue #1256), which the
//! capacitance here does not read; the ESR of the capacitors, which adds a step to the fall.

use std::collections::HashMap;

use netsmith_check::{
    DcElement, DcKind, Diagnostic, Domain, FieldId, InstId, InstKind, Member, Model, RExpr,
};
use netsmith_core::Cause;
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::{Dimension, Quantity};

use crate::levels::NotEvaluated;

/// The faults of the loads that burst, and the checks that could not be made.
pub(crate) fn check(model: &Model) -> (Vec<Diagnostic>, Vec<NotEvaluated>) {
    let mut faults = Vec::new();
    let mut notes = Vec::new();
    let nets = net_index(model);
    for (rail, inst) in model.instances() {
        if inst.kind != InstKind::Interface || inst.type_name != "Rail" {
            continue;
        }
        let Some(burst) = known(model, rail, "burst") else {
            continue;
        };
        let Some(peak) = known(model, rail, "peak_current") else {
            continue;
        };
        let Some(Member::Field(current)) = model.member(rail, "current") else {
            continue;
        };
        let path = model.path_of(rail);
        let limit = known(model, rail, "max_peak_current");
        let others = other_loads(model, current, 0);
        let (source, span) = statement_of(model, rail, "burst");
        let Some((voltage, pos_net, neg_net)) = rail_nets(model, rail) else {
            continue;
        };
        // What the source of the load's net gives during the burst: the peak and the others.
        let mut demand = others.map(|o| peak.hi() + o);
        if let Some(limit) = limit {
            // A peak above the limit alone is `Rail`'s own relation's to say.
            if peak.hi() > limit.lo() * (1.0 + 1e-9) {
                continue;
            }
            match demand {
                None => {
                    notes.push(NotEvaluated {
                        at: rail,
                        involves: vec![rail],
                        message: format!(
                            "the burst of `{path}` is not judged against its source's limit: the currents of the other loads on its net are not known, so what they draw during the burst cannot be added to `{path}.peak_current`; state them"
                        ),
                        cause: Cause::NotStated,
                    });
                    continue;
                }
                Some(d) => {
                    let head = format!(
                        "during a burst of `{path}` (`{path}.peak_current` {} for {}) the other loads on its net draw up to {}, so the source gives up to {}, above `{path}.max_peak_current` ({})",
                        show(Dimension::CURRENT, peak.hi()),
                        show(Dimension::TIME, burst.hi()),
                        show(Dimension::CURRENT, d - peak.hi()),
                        show(Dimension::CURRENT, d),
                        show(Dimension::CURRENT, limit.lo()),
                    );
                    let short = path.rsplit('.').next().unwrap_or(&path).to_string();
                    let fix = format!("state it on the load's rail (`.{short}.voltage >= ...`)");
                    let net = Across {
                        pos: pos_net,
                        neg: neg_net,
                        voltage,
                        words: "the rail",
                    };
                    if let Some(message) = held(model, &net, head, d - limit.lo(), burst.hi(), &fix)
                    {
                        faults.push(Diagnostic {
                            source,
                            span,
                            message,
                        });
                    }
                    demand = Some(d.min(limit.lo()));
                }
            }
        }
        reflect(
            model,
            &nets,
            Burst {
                rail,
                path: &path,
                burst: burst.hi(),
                source,
                span,
            },
            (pos_net, neg_net),
            demand,
            &mut faults,
            &mut notes,
        );
    }
    (faults, notes)
}

/// The two nets a capacitance is across, the least of the voltage between them, and what the
/// message calls them.
struct Across {
    pos: usize,
    neg: usize,
    voltage: Option<Quantity>,
    words: &'static str,
}

/// The message of a burst whose source gives `excess` less than it draws, or `None` when the
/// capacitance across the nets holds it down to the least voltage a part on them needs. `fix`
/// says what to state when no part states one.
fn held(
    model: &Model,
    net: &Across,
    head: String,
    excess: f64,
    burst: f64,
    fix: &str,
) -> Option<String> {
    if excess <= 1e-9 * excess.abs().max(1e-12) {
        return None;
    }
    let floor = least_voltage(model, net.pos, net.neg);
    let caps = capacitance_across(model, net.pos, net.neg);
    let words = net.words;
    Some(match (floor, net.voltage) {
        (Some((floor, by)), Some(v)) => {
            let fall = if caps > 0.0 {
                excess * burst / caps
            } else {
                f64::INFINITY
            };
            if v.lo() - fall >= floor - 1e-9 * floor.abs() {
                return None;
            }
            let held = if caps > 0.0 {
                format!(
                    "the {} across {words} give the other {} for {}, and {words} falls by {} from {}",
                    show(Dimension::CAPACITANCE, caps),
                    show(Dimension::CURRENT, excess),
                    show(Dimension::TIME, burst),
                    show(Dimension::VOLTAGE, fall),
                    show(Dimension::VOLTAGE, v.lo()),
                )
            } else {
                format!(
                    "no capacitor across {words} gives the other {}",
                    show(Dimension::CURRENT, excess)
                )
            };
            let need = excess * burst / (v.lo() - floor).max(0.0);
            let room = if v.lo() > floor {
                format!(
                    ": holding the burst takes {} of effective capacitance across {words}",
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
            "{head}, and no part on the net states the least voltage it works at, so the capacitance across {words} cannot hold the rest: {fix}, or give the source a higher limit"
        ),
    })
}

/// The load that bursts, for the messages of its reflection.
struct Burst<'a> {
    rail: InstId,
    path: &'a str,
    burst: f64,
    source: netsmith_check::SourceId,
    span: netsmith_lang::Span,
}

/// The most links a burst is reflected through (a converter behind a converter), so that a loop
/// of converters ends.
const MAX_STAGES: usize = 8;

/// The largest resistance a series part (an inductor's winding, a ferrite bead, a sense resistor)
/// may have for the walk from a load's net to the converter that feeds it to pass through it: a
/// pull-up of a few kilohms is no supply path.
const SERIES_OHMS: f64 = 10.0;

/// The burst reflected to the input of each converter in front of the load (issue #1340): from
/// the load's net to the converter whose output feeds it, and from that converter's input on.
/// `demand` is what the load's net draws from its source during the burst, `None` when the other
/// loads there have no value.
#[allow(clippy::too_many_arguments)]
fn reflect(
    model: &Model,
    nets: &HashMap<InstId, usize>,
    load: Burst<'_>,
    mut at: (usize, usize),
    mut demand: Option<f64>,
    faults: &mut Vec<Diagnostic>,
    notes: &mut Vec<NotEvaluated>,
) {
    let path = load.path;
    let mut seen = Vec::new();
    for _ in 0..MAX_STAGES {
        let Some((part, e)) = feeder(model, nets, at) else {
            return;
        };
        if seen.contains(&part) {
            return;
        }
        seen.push(part);
        let (input, output) = (e.pins[0], e.pins[1]);
        let Some(Member::Field(in_current)) = model.member(input, "current") else {
            return;
        };
        let Some((in_voltage, in_pos, in_neg)) = rail_nets(model, input) else {
            return;
        };
        let limit = known(model, input, "max_peak_current");
        let converter = model.path_of(part);
        let unknown = |what: String, notes: &mut Vec<NotEvaluated>| {
            // Nothing is said of a stage that states no limit and has none after it to judge.
            if limit.is_some() {
                notes.push(NotEvaluated {
                    at: load.rail,
                    involves: vec![load.rail, part],
                    message: format!(
                        "the burst of `{path}` is not judged at the input of `{converter}`, which feeds it: {what}; state it"
                    ),
                    cause: Cause::NotStated,
                });
            }
        };
        let Some(out) = demand else {
            unknown(
                "the currents of the other loads on the converter's output are not known".into(),
                notes,
            );
            return;
        };
        let Some((drawn, how)) = reflected(model, e, out, input, output) else {
            unknown(
                format!(
                    "the voltages or the efficiency `{}` relates its input to its output by are not known",
                    e.kind.example()
                ),
                notes,
            );
            return;
        };
        let Some(others) = other_loads(model, in_current, 0) else {
            unknown(
                "the currents of the other loads on the converter's input are not known".into(),
                notes,
            );
            return;
        };
        let total = drawn + others;
        if let Some(limit) = limit {
            let input_path = model.path_of(input);
            let rest = if others > 0.0 {
                format!(
                    " and the other loads on the input draw up to {}",
                    show(Dimension::CURRENT, others)
                )
            } else {
                String::new()
            };
            let head = format!(
                "during a burst of `{path}` (for {}) `{converter}`, which feeds it, draws up to {} from its input ({how}){rest}, so the source in front gives up to {}, above `{input_path}.max_peak_current` ({})",
                show(Dimension::TIME, load.burst),
                show(Dimension::CURRENT, drawn),
                show(Dimension::CURRENT, total),
                show(Dimension::CURRENT, limit.lo()),
            );
            let net = Across {
                pos: in_pos,
                neg: in_neg,
                voltage: in_voltage,
                words: "the input",
            };
            let fix = "state the least voltage of the converter's input (`.voltage >= ...` on the rail in front)";
            if let Some(message) = held(model, &net, head, total - limit.lo(), load.burst, fix) {
                faults.push(Diagnostic {
                    source: load.source,
                    span: load.span,
                    message,
                });
                return;
            }
            demand = Some(total.min(limit.lo()));
        } else {
            demand = Some(total);
        }
        at = (in_pos, in_neg);
    }
}

/// What a converter draws at its input while its output gives `out`, from its own `dc`
/// statement, so that a project's converter is reflected as the standard library's are: a
/// linear regulator draws the same current and its quiescent current, and a switching regulator
/// the output's power at its highest voltage over the least of its input voltage and of its
/// efficiency, and its quiescent current. With the words that say how, for the message.
fn reflected(
    model: &Model,
    e: &DcElement,
    out: f64,
    input: InstId,
    output: InstId,
) -> Option<(f64, String)> {
    let value = |f: FieldId| match &model.field(f).domain {
        Domain::Quantity(q) => Some(*q),
        _ => None,
    };
    let voltage = |rail: InstId| value(model.rail_of(rail)?.0);
    match e.kind {
        DcKind::LinearRegulator => {
            let quiescent = e
                .values
                .first()
                .and_then(|&f| value(f))
                .map_or(0.0, |q| q.hi());
            Some((
                out + quiescent,
                format!(
                    "the {} its output gives, which a linear regulator passes on",
                    show(Dimension::CURRENT, out)
                ),
            ))
        }
        DcKind::SwitchingRegulator => {
            let efficiency = value(*e.values.first()?)?.lo();
            let vout = voltage(output)?.hi();
            let vin = voltage(input)?.lo();
            if efficiency <= 0.0 || vin <= 0.0 {
                return None;
            }
            let quiescent = e
                .values
                .get(1)
                .and_then(|&f| value(f))
                .map_or(0.0, |q| q.hi());
            Some((
                out * vout / (vin * efficiency) + quiescent,
                format!(
                    "the {} its output gives at {}, over the least input {} and the least efficiency {}",
                    show(Dimension::CURRENT, out),
                    show(Dimension::VOLTAGE, vout),
                    show(Dimension::VOLTAGE, vin),
                    efficiency,
                ),
            ))
        }
        _ => None,
    }
}

/// The net of each pin.
fn net_index(model: &Model) -> HashMap<InstId, usize> {
    model
        .nets()
        .iter()
        .enumerate()
        .flat_map(|(i, n)| n.pins.iter().map(move |&p| (p, i)))
        .collect()
}

/// The converter whose output feeds the two nets: the `dc linear_regulator` or
/// `dc switching_regulator` whose output rail's `pos` is on the positive net, or reached from it
/// through series parts (a `dc resistor` of at most `SERIES_OHMS`, a buck's inductor; a
/// `dc load_switch`, output to input), and whose `neg` is on the negative net. The nearest one.
fn feeder<'m>(
    model: &'m Model,
    nets: &HashMap<InstId, usize>,
    (pos, neg): (usize, usize),
) -> Option<(InstId, &'m DcElement)> {
    let rail_pos = |rail: InstId| nets.get(&model.rail_of(rail)?.1).copied();
    let rail_neg = |rail: InstId| nets.get(&model.rail_of(rail)?.2).copied();
    let mut links: Vec<(usize, usize)> = Vec::new();
    for (_, e) in model.dc_elements() {
        match e.kind {
            DcKind::Resistor => {
                let (Some(&a), Some(&b)) = (nets.get(&e.pins[0]), nets.get(&e.pins[1])) else {
                    continue;
                };
                let small = e
                    .values
                    .first()
                    .and_then(|&f| match &model.field(f).domain {
                        Domain::Quantity(q) => Some(q.lo() <= SERIES_OHMS),
                        _ => None,
                    })
                    .unwrap_or(false);
                if small {
                    links.push((a, b));
                    links.push((b, a));
                }
            }
            DcKind::LoadSwitch => {
                if let (Some(i), Some(o)) = (rail_pos(e.pins[0]), rail_pos(e.pins[1])) {
                    links.push((o, i));
                }
            }
            _ => {}
        }
    }
    let mut seen = vec![pos];
    let mut frontier = vec![pos];
    while !frontier.is_empty() {
        for (part, e) in model.dc_elements() {
            if !matches!(e.kind, DcKind::LinearRegulator | DcKind::SwitchingRegulator) {
                continue;
            }
            let output = e.pins[1];
            if rail_pos(output).is_some_and(|n| frontier.contains(&n))
                && rail_neg(output) == Some(neg)
            {
                return Some((part, e));
            }
        }
        let mut next = Vec::new();
        for &(a, b) in &links {
            if frontier.contains(&a) && b != neg && !seen.contains(&b) {
                seen.push(b);
                next.push(b);
            }
        }
        frontier = next;
    }
    None
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

/// The highest least voltage a rail across the two nets states (`.voltage >= 3V`, a field
/// whose value is known, or an expression of them, at its highest), and the rail that states it.
fn least_voltage(model: &Model, pos_net: usize, neg_net: usize) -> Option<(f64, String)> {
    let rails = rails_across(model, pos_net, neg_net);
    // A bound is any expression whose value is known: a constant, a field, or a converter's
    // own least input (`min(.vout.voltage) + .dropout`), at its highest.
    let value = |e: &RExpr| -> Option<f64> {
        match crate::eval::eval(model, e) {
            Ok(crate::eval::Val::Q(q)) if q.dim() == Dimension::VOLTAGE => Some(q.hi()),
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
        let mut mentions = Vec::new();
        bound.fields(&mut mentions);
        if mentions.contains(&field) {
            continue;
        }
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
