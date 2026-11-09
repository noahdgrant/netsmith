//! What a bus needs from the board around it (ADR 0044).
//!
//! An I2C line is open-drain: no device drives it high, so a resistor to a supply must.
//! `std::bus::I2c` says which pins are the lines, and the checker looks for the resistor. A line
//! that goes out through a connector is followed through the mates of a system to the other
//! boards (ADR 0090).

use std::collections::HashSet;

use netsmith_quantity::Dimension;

use crate::diag::Diagnostic;
use crate::model::{FieldKind, InstId, InstKind, Member, Model, Net, PinKind};

/// The interface whose `sda` and `scl` are open-drain. The checker knows it by name, as
/// it knows `Rail` by its `pos` and `neg`.
const I2C: &str = "I2c";

/// The faults of the I2C buses of a board: a data or clock line with no pull-up.
pub(crate) fn i2c_pullups(model: &Model) -> Vec<Diagnostic> {
    let mut found: Vec<(String, Diagnostic)> = Vec::new();
    let mut judged: HashSet<usize> = HashSet::new();
    for (id, inst) in model.instances() {
        if inst.kind != InstKind::Interface || inst.type_name != I2C {
            continue;
        }
        let Some(owner) = model.owner_of(id) else {
            continue;
        };
        if model.board_of(owner).is_none() {
            continue;
        }
        for line in ["sda", "scl"] {
            let Some(Member::Inst(pin)) = model.member(id, line) else {
                continue;
            };
            let Some(index) = model.nets().iter().position(|n| n.pins.contains(&pin)) else {
                continue;
            };
            if !judged.insert(index) {
                continue;
            }
            let net = &model.nets()[index];
            if pulled_up(model, index) || is_port(model, net) {
                continue;
            }
            // A line that goes out through a connector may be pulled up on the other side. In
            // a system whose mates show where every such connector leads, the other side is
            // looked at (ADR 0090); anywhere else this board cannot see it, and says nothing.
            let mut crossed: Vec<String> = Vec::new();
            let mut nets = vec![index];
            if leaves_the_board(model, net) {
                match beyond_the_connectors(model, index) {
                    Beyond::Unknown => continue,
                    Beyond::PulledUp(seen) => {
                        judged.extend(seen);
                        continue;
                    }
                    Beyond::Open { seen, mated } => {
                        judged.extend(seen.iter().copied());
                        nets = seen;
                        crossed = mated;
                    }
                }
            }
            let mut lines: Vec<String> = nets
                .iter()
                .flat_map(|&n| model.nets()[n].pins.iter())
                .filter(|&&p| is_line(model, p))
                .map(|&p| format!("`{}`", model.path_of(p)))
                .collect();
            lines.sort();
            let (source, span) = model.instance(owner).at();
            let message = if crossed.is_empty() {
                format!(
                    "net `{}` carries the {line} line of an I2C bus ({}) and has no pull-up: no resistor joins it to the positive pin of a rail; add `std::bus::I2cPullups` to the bus, or a resistor from a rail's `pos` to the line",
                    net.name,
                    lines.join(", ")
                )
            } else {
                format!(
                    "net `{}` carries the {line} line of an I2C bus ({}) and has no pull-up, on this board or on the boards its mated connectors lead to ({}): no resistor joins it to the positive pin of a rail; add `std::bus::I2cPullups` to the bus on one of the boards, or a resistor from a rail's `pos` to the line",
                    net.name,
                    lines.join(", "),
                    crossed.join(", ")
                )
            };
            found.push((
                net.name.clone(),
                Diagnostic {
                    source,
                    span,
                    message,
                },
            ));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found.into_iter().map(|(_, d)| d).collect()
}

/// Whether a pin is the `sda` or `scl` of an `I2c` interface.
fn is_line(model: &Model, pin: InstId) -> bool {
    let inst = model.instance(pin);
    matches!(inst.name.as_str(), "sda" | "scl")
        && inst
            .parent
            .is_some_and(|p| model.instance(p).type_name == I2C)
}

/// Whether the net reaches the positive pin of a rail through resistors: a resistor from
/// the net to a rail's `pos`, or to a net that has one, and so on. The net itself being
/// on a rail is a short, not a pull-up.
fn pulled_up(model: &Model, start: usize) -> bool {
    let mut seen = vec![start];
    let mut next = vec![start];
    while let Some(index) = next.pop() {
        for &t in &model.nets()[index].terminals {
            let Some(part) = model.owner_of(t) else {
                continue;
            };
            let Some(other) = resistor_far_end(model, part, t) else {
                continue;
            };
            let Some(far) = model.nets().iter().position(|n| n.pins.contains(&other)) else {
                continue;
            };
            if seen.contains(&far) {
                continue;
            }
            if has_rail_pos(model, &model.nets()[far]) {
                return true;
            }
            seen.push(far);
            next.push(far);
        }
    }
    false
}

/// The terminal at the other end of a resistor, if `pin` is one end of a part that is one:
/// a `bridge` and a `resistance`.
fn resistor_far_end(model: &Model, part: InstId, pin: InstId) -> Option<InstId> {
    let (a, b) = model.instance(part).bridge?;
    let Some(Member::Field(r)) = model.member(part, "resistance") else {
        return None;
    };
    if model.field(r).kind != FieldKind::Quantity(Dimension::RESISTANCE) {
        return None;
    }
    match pin {
        p if p == a => Some(b),
        p if p == b => Some(a),
        _ => None,
    }
}

/// Whether the positive pin of a rail is on the net.
fn has_rail_pos(model: &Model, net: &Net) -> bool {
    net.pins.iter().any(|&p| {
        model
            .instance(p)
            .parent
            .and_then(|i| model.rail_of(i))
            .is_some_and(|(_, pos, _)| pos == p)
    })
}

/// Whether a terminal is a pin of a connector: a part of several passive pins and no bridge.
fn is_connector_pin(model: &Model, t: InstId) -> bool {
    let Some(part) = model.owner_of(t) else {
        return false;
    };
    let inst = model.instance(part);
    inst.bridge.is_none()
        && model
            .pin_facts(t)
            .is_some_and(|r| r.kind == PinKind::Passive)
        && inst.tables.iter().any(|table| table.rows.len() >= 2)
        && inst
            .tables
            .iter()
            .all(|table| table.rows.iter().all(|r| r.kind == PinKind::Passive))
}

/// Whether the line goes out through a connector: a part of several passive pins and no
/// bridge. Its other side may have the pull-up, which this board cannot see.
fn leaves_the_board(model: &Model, net: &Net) -> bool {
    net.terminals.iter().any(|&t| is_connector_pin(model, t))
}

/// What is on the far side of the connectors a line goes out through.
enum Beyond {
    /// A connector that no mate in this model accounts for: the line leaves for somewhere this
    /// check cannot see, and nothing is said.
    Unknown,
    /// A pull-up on the line's own board or on a board it is mated to; the nets looked at.
    PulledUp(Vec<usize>),
    /// Every connector is mated to another in this model and none of the boards has a pull-up.
    Open {
        /// The nets of the line on every board it reaches.
        seen: Vec<usize>,
        /// The connectors it goes out through.
        mated: Vec<String>,
    },
}

/// The pin of the other connector that a mate meets `pin` with: contact N of one connector is
/// contact N of the other (ADR 0040). A connector that is not mated, or whose mate is not read,
/// has none.
fn mated_pin(model: &Model, pin: InstId) -> Option<InstId> {
    model.mates().iter().find_map(|mate| {
        mate.contacts.iter().find_map(|c| match pin {
            p if p == c.a.pin => Some(c.b.pin),
            p if p == c.b.pin => Some(c.a.pin),
            _ => None,
        })
    })
}

/// Follow a line through the connectors it leaves its board by, as a system mates them, to
/// the nets it is on in the boards behind them, and look for a pull-up on each. The answer is
/// "unknown" as soon as one connector on the way is not mated in this model, since the line
/// then goes somewhere unseen.
fn beyond_the_connectors(model: &Model, start: usize) -> Beyond {
    let mut seen = vec![start];
    let mut next = vec![start];
    let mut mated: Vec<String> = Vec::new();
    while let Some(index) = next.pop() {
        let net = &model.nets()[index];
        if index != start && (pulled_up(model, index) || is_port(model, net)) {
            // A port of another board is brought in from outside: its pull-up is not seen.
            return if pulled_up(model, index) {
                Beyond::PulledUp(seen)
            } else {
                Beyond::Unknown
            };
        }
        for &t in net
            .terminals
            .iter()
            .filter(|&&t| is_connector_pin(model, t))
        {
            let Some(other) = mated_pin(model, t) else {
                return Beyond::Unknown;
            };
            if let Some(part) = model.owner_of(t) {
                mated.push(format!("`{}`", model.path_of(part)));
            }
            let Some(far) = model.nets().iter().position(|n| n.pins.contains(&other)) else {
                // The mated contact is on no net of its board: nothing is behind it.
                continue;
            };
            if !seen.contains(&far) {
                seen.push(far);
                next.push(far);
            }
        }
    }
    mated.sort();
    mated.dedup();
    Beyond::Open { seen, mated }
}

/// Whether the line is a port of the board: it is brought in from outside, and its pull-up
/// is the other board's.
fn is_port(model: &Model, net: &Net) -> bool {
    net.pins.iter().any(|&p| {
        is_line(model, p)
            && model
                .owner_of(p)
                .is_some_and(|o| model.instance(o).kind == InstKind::Board)
    })
}
