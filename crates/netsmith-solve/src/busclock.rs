//! The clock of a bus against the limits of its parts (ADR 0064, stage 3).
//!
//! A part states the limit of its datasheet as a bound on the clock of its interface
//! (`.spi.frequency <= 10MHz`), and the board states the clock it will run
//! (`.cpu.spi.frequency &= 8MHz`). The interfaces on one bus hold one value, so the solver
//! judges each bound against it. This module says the fault in the words of the bus: the value,
//! the part that does not allow it, and what the other parts allow; and says when a bus has
//! limits and no clock to hold them against.

use netsmith_check::{BusGroup, Domain, FieldId, InstId, Model, RExpr, Relation, bus_groups};
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::Quantity;

use crate::levels::NotEvaluated;

/// A bound that a part states on the clock of one interface of a bus: `<=` or `>=` a constant.
struct Bound {
    end: InstId,
    field: FieldId,
    op: CmpOp,
    limit: Quantity,
}

/// The bounds on the clock of the ends of a bus, in the order of the ends.
fn bounds(model: &Model, group: &BusGroup) -> Vec<Bound> {
    let Some(clock) = &group.clock else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for &end in &group.ends {
        let Some(netsmith_check::Member::Field(field)) = model.member(end, clock) else {
            continue;
        };
        for r in model.relations() {
            if let (RExpr::Field(f), CmpOp::Le | CmpOp::Ge, RExpr::Const(limit)) =
                (&r.lhs, r.op, &r.rhs)
                && *f == field
            {
                found.push(Bound {
                    end,
                    field,
                    op: r.op,
                    limit: *limit,
                });
            }
        }
    }
    found
}

/// `a`, `a and b`, `a, b and c`.
fn and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// The part of an interface and its words for a message: `imu` (`Zimu`).
fn part_of(model: &Model, end: InstId) -> (InstId, String) {
    let part = model.owner_of(end).unwrap_or(end);
    (
        part,
        format!(
            "`{}` (`{}`)",
            model.path_of(part),
            model.instance(part).type_name
        ),
    )
}

/// The failure of a bound on the clock of a bus, said in the words of the bus, and the part it
/// is about; None for a relation that is not such a bound.
pub(crate) fn failure(model: &Model, r: &Relation) -> Option<(InstId, String)> {
    let (RExpr::Field(f), CmpOp::Le | CmpOp::Ge, RExpr::Const(limit)) = (&r.lhs, r.op, &r.rhs)
    else {
        return None;
    };
    let end = model.field(*f).owner;
    let group = bus_groups(model).into_iter().find(|g| {
        g.ends.contains(&end) && g.clock.as_deref() == Some(model.field(*f).name.as_str())
    })?;
    let all = bounds(model, &group);
    let (part, who) = part_of(model, end);
    // The bus is named by the end that the board gave the clock, else by its first.
    let named = group
        .ends
        .iter()
        .copied()
        .find(|&e| {
            let clock = group.clock.as_deref().unwrap_or_default();
            matches!(
                model.member(e, clock),
                Some(netsmith_check::Member::Field(c)) if !model.field(c).narrowed_at.is_empty()
            )
        })
        .unwrap_or(group.ends[0]);
    let value = model.show_value(*f, &model.field(*f).domain);
    let name = model.path_of(named);
    let field = model.path_of_field(*f);
    let message = if r.op == CmpOp::Le {
        let others: Vec<String> = all
            .iter()
            .filter(|b| b.end != end && b.op == CmpOp::Le)
            .enumerate()
            .map(|(i, b)| {
                let (_, who) = part_of(model, b.end);
                let v = model.show_value(b.field, &Domain::Quantity(b.limit));
                if i == 0 {
                    format!("{who} allows {v}")
                } else {
                    format!("{who} {v}")
                }
            })
            .collect();
        let lowest = all
            .iter()
            .filter(|b| b.op == CmpOp::Le)
            .min_by(|a, b| a.limit.lo().total_cmp(&b.limit.lo()))
            .map(|b| model.show_value(b.field, &Domain::Quantity(b.limit)))
            .unwrap_or_else(|| model.show_value(*f, &Domain::Quantity(*limit)));
        let mut text = format!(
            "bus `{name}` is joined at {value}, which {who} does not allow: its limit `{field}` is at most {}",
            model.show_value(*f, &Domain::Quantity(*limit)),
        );
        if !others.is_empty() {
            text.push_str(&format!("; {}", and(&others)));
        }
        text.push_str(&format!(", so the bus runs at {lowest} at most"));
        text
    } else {
        format!(
            "bus `{name}` is joined at {value}, which {who} does not allow: its limit `{field}` is at least {}",
            model.show_value(*f, &Domain::Quantity(*limit)),
        )
    };
    Some((part, message))
}

/// The buses that have limits and no clock to hold them against: a note each, never a pass.
pub(crate) fn notes(model: &Model) -> Vec<NotEvaluated> {
    let mut out = Vec::new();
    for group in bus_groups(model) {
        let Some(clock) = &group.clock else { continue };
        let all = bounds(model, &group);
        if all.is_empty() {
            continue;
        }
        let stated = group.ends.iter().any(|&e| {
            matches!(
                model.member(e, clock),
                Some(netsmith_check::Member::Field(c)) if model.field(c).domain != Domain::Unset
            )
        });
        if stated {
            continue;
        }
        let limits: Vec<String> = all
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let (_, who) = part_of(model, b.end);
                let v = model.show_value(b.field, &Domain::Quantity(b.limit));
                let word = if b.op == CmpOp::Le {
                    "at most"
                } else {
                    "at least"
                };
                if i == 0 {
                    format!("{who} {word} {v}")
                } else {
                    format!("{who} {v}")
                }
            })
            .collect();
        let first = model.path_of(group.ends[0]);
        out.push(NotEvaluated {
            at: model.owner_of(group.ends[0]).unwrap_or(group.ends[0]),
            message: format!(
                "not evaluated: the clock of bus `{first}` is not stated, so the limits on it are not checked ({}); state the clock the board runs, `.{first}.{clock} &= <the clock>`",
                and(&limits)
            ),
        });
    }
    out
}
