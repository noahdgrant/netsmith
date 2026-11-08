//! A bound on the current of a resistive part, by Ohm's law over the voltages at its two ends.
//!
//! The solver leaves the current of a part open when nothing states it and the net it is on has
//! two or more currents it does not know (ADR 0026), so the part's drop and power checks wait.
//! But the current is limited when the voltage at each end of the part lies in a known range:
//! `|I| <= max |Va - Vb| / R`, and the power is at most `max (Va - Vb)^2 / R`, with `R` taken at
//! its least. The bound is only ever used to *prove* a check passes (ADR 0026, amendment "the Ohm
//! bound, pass-only"): a check that it does not prove stays as it was, not evaluated. It is not
//! written into the model, so no value, pick or track changes with it.
//!
//! The range of a net is read here with more care than the levels check reads it, because a
//! range that is too narrow would clear a note that should stand. A net's voltage is within the
//! hull of:
//!
//! - a rail on it (its stated voltage, and `0 V` for the ground the range is measured from).
//!   A net with a rail's pin is that rail's, whatever else is on it;
//! - a `Logic` line that drives it: its ground to its reference rail's top;
//! - the nets on the far ends of the resistors on it (a series resistor, a pull-up, a divider):
//!   the voltage of a node in a network of resistors is between those of its neighbours;
//! - the ground, when a part's input or supply pin is on it (its current returns there).
//!
//! A net with anything else on it that could set its voltage has no range: a pin that drives it
//! and is not a `Logic` line, a passive pin of a part that conducts or whose model does not say
//! it passes nothing, a pin with no kind, a rail with no voltage, a rail whose ground is not the
//! ground the range is measured from. Then there is no bound.

use std::collections::HashMap;

use netsmith_check::{Domain, InstId, InstKind, Member, Model, PinKind};
use netsmith_quantity::Dimension;

use crate::levels::{Run, reader};

/// What Ohm's law allows a part, over every state of the circuit around it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Bound {
    pub part: InstId,
    /// The largest `|Va - Vb|` across the part, in volts.
    pub drop: f64,
    /// The largest `|I|` through it, in amperes: `drop` over its least resistance.
    pub current: f64,
    /// That current through its greatest resistance: a drop that `drop == current * resistance`
    /// does not refuse, wider than `drop` by the resistor's tolerance (and a rounding's worth,
    /// so that the product of the two ranges is inside it).
    pub drop_through: f64,
}

/// The bound of every part that is a resistor (a `bridge` and a `resistance` with a value above
/// zero) whose voltage has a range at both ends. A part whose current has a value is left out:
/// there is nothing to bound.
pub(crate) fn bounds(model: &Model) -> Vec<Bound> {
    let read = reader(model);
    let net_of: HashMap<InstId, usize> = model
        .nets()
        .iter()
        .enumerate()
        .flat_map(|(i, n)| n.pins.iter().map(move |&p| (p, i)))
        .collect();
    // The nets that a rail's ground is on: the references a range may be measured from.
    let mut grounds: Vec<usize> = model
        .instances()
        .filter_map(|(id, _)| model.rail_of(id))
        .filter_map(|(_, _, neg)| net_of.get(&neg).copied())
        .collect();
    grounds.sort_unstable();
    grounds.dedup();
    let ranges = Ranges {
        model,
        read: &read,
        net_of: &net_of,
    };
    let mut out = Vec::new();
    for (part, inst) in model.instances() {
        netsmith_core::job::checkpoint();
        let Some((a, b)) = inst.bridge else { continue };
        let Some(current) = model.through_of(part) else {
            continue;
        };
        if model.field(current).domain != Domain::Unset {
            continue;
        }
        let Some((_, resistance)) = read.value(part, "resistance") else {
            continue;
        };
        if resistance.dim() != Dimension::RESISTANCE
            || !(resistance.lo().is_finite() && resistance.lo() > 0.0)
        {
            continue;
        }
        let (Some(&na), Some(&nb)) = (net_of.get(&a), net_of.get(&b)) else {
            continue;
        };
        // The first ground that both ends can be measured from; the difference between two
        // ends does not depend on which, once both are measured from the same one.
        let Some((x, y)) = grounds
            .iter()
            .find_map(|&g| Some((ranges.of(na, g)?, ranges.of(nb, g)?)))
        else {
            continue;
        };
        let drop = (x.1 - y.0).max(y.1 - x.0);
        if !drop.is_finite() || drop < 0.0 {
            continue;
        }
        out.push(Bound {
            part,
            drop,
            current: drop / resistance.lo(),
            drop_through: drop / resistance.lo() * resistance.hi() * (1.0 + 1e-9),
        });
    }
    out
}

struct Ranges<'a> {
    model: &'a Model,
    read: &'a Run<'a>,
    net_of: &'a HashMap<InstId, usize>,
}

impl Ranges<'_> {
    /// The lowest and highest voltage the net can have, measured from net `ground`, or `None`
    /// when something on it, or on a net it reaches through resistors, could set it and is not
    /// known.
    fn of(&self, start: usize, ground: usize) -> Option<(f64, f64)> {
        let model = self.model;
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        let mut add = |l: f64, h: f64| {
            lo = lo.min(l);
            hi = hi.max(h);
        };
        let mut seen = vec![start];
        let mut at = 0;
        while at < seen.len() {
            netsmith_core::job::checkpoint();
            let index = seen[at];
            at += 1;
            let net = &model.nets()[index];
            // A rail on the net sets it, and what else is on it is the rail's to carry.
            let mut railed = index == ground;
            if railed {
                add(0.0, 0.0);
            }
            for &pin in &net.pins {
                let Some(parent) = model.instance(pin).parent else {
                    continue;
                };
                let Some((voltage, pos, neg)) = model.rail_of(parent) else {
                    continue;
                };
                if pin == pos {
                    railed = true;
                    if self.net_of.get(&neg) != Some(&ground) {
                        return None;
                    }
                    let Domain::Quantity(q) = model.field(voltage).domain else {
                        return None;
                    };
                    add(q.lo(), q.hi());
                } else if pin == neg {
                    railed = true;
                    if index != ground {
                        return None;
                    }
                }
            }
            if railed {
                continue;
            }
            let mut returns = false;
            for &t in &net.terminals {
                if let Some(far) = self.read.resistor_far_end(t) {
                    let far = *self.net_of.get(&far)?;
                    if !seen.contains(&far) {
                        seen.push(far);
                    }
                    continue;
                }
                match model.pin_kind(t)? {
                    PinKind::Output | PinKind::TriState | PinKind::Bidirectional => {
                        let (l, h) = match self.read.logic_of(t) {
                            Some(logic) => self.logic_swing(logic, ground)?,
                            None => self.part_swing(model.owner_of(t)?, ground)?,
                        };
                        add(l, h);
                    }
                    PinKind::Input | PinKind::Power => returns = true,
                    PinKind::NoConnect => {}
                    PinKind::Passive => {
                        // A part that passes nothing at DC (a capacitor, a test point) does
                        // not set the voltage, unless it is a connector, where something
                        // outside the design may; any other passive pin might.
                        let part = model.owner_of(t)?;
                        if model.instance(part).kind != InstKind::Part
                            || !model.draws_nothing(part)
                            || model.member(part, "mating").is_some()
                        {
                            return None;
                        }
                    }
                }
            }
            if returns {
                add(0.0, 0.0);
            }
        }
        (lo <= hi).then_some((lo, hi))
    }

    /// What a `Logic` line can drive: from its ground to the top of its reference rail.
    fn logic_swing(&self, logic: InstId, ground: usize) -> Option<(f64, f64)> {
        let g = self.read.ground_pin(logic)?;
        if self.net_of.get(&g) != Some(&ground) {
            return None;
        }
        let Some(Member::Inst(reference)) = self.model.member(logic, "reference") else {
            return None;
        };
        let (_, q) = self.read.value(reference, "voltage")?;
        Some((q.lo().min(0.0), q.hi().max(0.0)))
    }

    /// What a pin that drives, and is not the line of a `Logic`, can drive: no more than the
    /// supplies of its own part, from the ground to the top of its highest rail. A part with no
    /// rail of its own, or one that is not on this ground or has no voltage, says nothing.
    fn part_swing(&self, part: InstId, ground: usize) -> Option<(f64, f64)> {
        let model = self.model;
        let (mut lo, mut hi) = (0.0_f64, f64::NEG_INFINITY);
        for (_, member) in &model.instance(part).children {
            let Member::Inst(rail) = *member else {
                continue;
            };
            let Some((voltage, _, neg)) = model.rail_of(rail) else {
                continue;
            };
            if self.net_of.get(&neg) != Some(&ground) {
                return None;
            }
            let Domain::Quantity(q) = model.field(voltage).domain else {
                return None;
            };
            lo = lo.min(q.lo());
            hi = hi.max(q.hi());
        }
        (hi >= lo).then_some((lo, hi))
    }
}
