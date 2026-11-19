//! The voltage classes of a SIM bus against its supply (issue 1280), checked after the solve.
//!
//! A `std::bus::Sim` states the role of its part (`SimRole::Host` or `SimRole::Card`) and the
//! voltage classes the part supports (`.classes`, a `SimClasses`). For each bus, the host and the
//! card of it are held against each other and against the supply, the voltage of the bus's
//! `vcc` rail, which `~` equates along the bus:
//!
//! - **A class in common.** A host that supplies no class the card works at cannot power it.
//! - **The supply within a class.** The supply's range must lie within the range of one class
//!   that both support: class A is 4.5 V to 5.5 V, class B 2.7 V to 3.3 V and class C 1.62 V to
//!   1.98 V (ISO/IEC 7816-3, ETSI TS 102 221 clause 6.2).
//!
//! A side that states no classes, or a supply with no value after the solve, is listed as not
//! evaluated. A bus with no host or no card (a host alone, a card on a test connector) is not
//! judged.

use netsmith_check::{Diagnostic, Domain, InstId, Member, Model, bus_groups};
use netsmith_core::Cause;
use netsmith_quantity::{Dimension, Quantity};

use crate::charge::{known, range, show, statement_of};
use crate::levels::NotEvaluated;

/// The type name of the bus in the standard library.
const SIM: &str = "Sim";

/// The classes, each with the range of its supply in volts (ETSI TS 102 221 clause 6.2).
const CLASSES: [(char, f64, f64); 3] = [('A', 4.5, 5.5), ('B', 2.7, 3.3), ('C', 1.62, 1.98)];

/// The faults of the SIM buses of a design, and the checks that could not be made.
pub(crate) fn check(model: &Model) -> (Vec<Diagnostic>, Vec<NotEvaluated>) {
    let mut faults = Vec::new();
    let mut notes = Vec::new();
    for group in bus_groups(model).into_iter().filter(|g| g.name == SIM) {
        let with = |role: &str| -> Vec<InstId> {
            group
                .ends
                .iter()
                .copied()
                .filter(|&e| role_of(model, e).as_deref() == Some(role))
                .collect()
        };
        for host in with("Host") {
            for card in with("Card") {
                judge(model, host, card, &mut faults, &mut notes);
            }
        }
    }
    (faults, notes)
}

/// The variant of an interface's `.role`, without its enum's name.
fn role_of(model: &Model, iface: InstId) -> Option<String> {
    variant(model, iface, "role")
}

/// The variant a member of an enum has, without its enum's name (`BC` of `SimClasses::BC`).
fn variant(model: &Model, inst: InstId, name: &str) -> Option<String> {
    let Some(Member::Field(f)) = model.member(inst, name) else {
        return None;
    };
    match &model.field(f).domain {
        Domain::Text(v) => Some(v.rsplit("::").next().unwrap_or(v).to_string()),
        _ => None,
    }
}

/// The classes a side states, in the order A, B, C.
fn classes(model: &Model, iface: InstId) -> Option<Vec<char>> {
    let v = variant(model, iface, "classes")?;
    Some(
        CLASSES
            .iter()
            .map(|c| c.0)
            .filter(|c| v.contains(*c))
            .collect(),
    )
}

/// `class B` or `classes B and C`.
fn words(classes: &[char]) -> String {
    let names: Vec<String> = classes.iter().map(|c| c.to_string()).collect();
    match names.as_slice() {
        [one] => format!("class {one}"),
        [rest @ .., last] => format!("classes {} and {last}", rest.join(", ")),
        [] => "no class".to_string(),
    }
}

/// The range of a class's supply, in volts.
fn class_range(class: char) -> (f64, f64) {
    CLASSES
        .iter()
        .find(|c| c.0 == class)
        .map_or((0.0, 0.0), |c| (c.1, c.2))
}

fn judge(
    model: &Model,
    host: InstId,
    card: InstId,
    faults: &mut Vec<Diagnostic>,
    notes: &mut Vec<NotEvaluated>,
) {
    let h = model.path_of(host);
    let c = model.path_of(card);
    let mut unknown = Vec::new();
    let supplied = classes(model, host);
    let taken = classes(model, card);
    if supplied.is_none() {
        unknown.push(format!("`{h}.classes`"));
    }
    if taken.is_none() {
        unknown.push(format!("`{c}.classes`"));
    }
    let vcc = match model.member(host, "vcc") {
        Some(Member::Inst(r)) => Some(r),
        _ => None,
    };
    let supply = vcc.and_then(|r| known(model, r, "voltage"));
    if let (Some(supplied), Some(taken)) = (&supplied, &taken) {
        let common: Vec<char> = supplied
            .iter()
            .copied()
            .filter(|x| taken.contains(x))
            .collect();
        if common.is_empty() {
            let (source, span) = statement_of(model, card, "classes");
            faults.push(Diagnostic {
                source,
                span,
                message: format!(
                    "the SIM host `{h}` supplies {} (`{h}.classes`) and the card `{c}` works at {} (`{c}.classes`): they share no class, so the host cannot power the card; use a card or a host that supports a class of the other",
                    words(supplied),
                    words(taken),
                ),
            });
        } else if let Some(v) = supply
            && !common.iter().any(|&x| within(v, class_range(x)))
        {
            let (source, span) = match vcc {
                Some(r) => statement_of(model, r, "voltage"),
                None => model.instance(host).at(),
            };
            let ranges: Vec<String> = common
                .iter()
                .map(|&x| {
                    let (lo, hi) = class_range(x);
                    format!(
                        "class {x}, {} to {}",
                        show(Dimension::VOLTAGE, lo),
                        show(Dimension::VOLTAGE, hi)
                    )
                })
                .collect();
            faults.push(Diagnostic {
                source,
                span,
                message: format!(
                    "the SIM supply `{h}.vcc` is {}, within no class that the host `{h}` and the card `{c}` both support ({}); run the supply within one of them",
                    range(Dimension::VOLTAGE, v),
                    ranges.join("; "),
                ),
            });
        }
    }
    if supply.is_none() {
        unknown.push(format!("`{h}.vcc.voltage`"));
    }
    if !unknown.is_empty() {
        notes.push(NotEvaluated {
            at: host,
            involves: vec![host, card],
            message: format!(
                "the SIM bus of the host `{h}` and the card `{c}` is not judged against its voltage classes: {} {} no value; state {}",
                unknown.join(", "),
                if unknown.len() == 1 { "has" } else { "have" },
                if unknown.len() == 1 { "it" } else { "them" },
            ),
            cause: Cause::NotStated,
        });
    }
}

/// Whether `inner` lies within the range `(lo, hi)`, both ends, to a part in a billion.
fn within(inner: Quantity, (lo, hi): (f64, f64)) -> bool {
    let slack = 1e-9 * hi.abs();
    inner.lo() >= lo - slack && inner.hi() <= hi + slack
}
