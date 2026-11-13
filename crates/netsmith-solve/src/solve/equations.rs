//! Reading the model's equations and statements before the solver starts.

use std::collections::{HashMap, HashSet};

use crate::eval::Val;
use netsmith_check::{Domain, FieldId, InstKind, Model, RExpr};
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::Quantity;

/// The currents in a balance that the design gave a range (`.x.current &= a..b`) and that
/// no `==` states: their range is a budget (ADR 0038). A current an equation determines is
/// not stated, whatever range it was also written with; the equation checks that.
pub(super) fn stated_currents(model: &Model) -> HashMap<FieldId, Quantity> {
    let equated: HashSet<FieldId> = model
        .relations()
        .iter()
        .filter_map(|r| match (&r.op, &r.lhs) {
            (CmpOp::EqEq, RExpr::Field(f)) => Some(*f),
            _ => None,
        })
        .collect();
    model
        .balances()
        .iter()
        .flat_map(|b| b.draws.iter().map(|d| d.field))
        .filter(|f| !equated.contains(f))
        // What a statement gives a part's own current is the part's claim, not an allowance.
        .filter(|f| !inside_part(model, *f))
        .filter_map(|f| match &model.field(f).domain {
            Domain::Quantity(q) => Some((f, *q)),
            _ => None,
        })
        .collect()
}

/// Whether a field is part of a part (the draw of its supply pin, the current through it):
/// what a part states of itself is the part's claim (ADR 0066, 3).
pub(super) fn inside_part(model: &Model, field: FieldId) -> bool {
    let mut at = Some(model.field(field).owner);
    while let Some(inst) = at {
        if model.instance(inst).kind == InstKind::Part {
            return true;
        }
        at = model.instance(inst).parent;
    }
    false
}

/// The currents in a balance that a range with a value at an end states, outside a part's
/// own body: they are stated like the same range written with numbers, once the ends have a
/// value (ADR 0066, 3).
pub(super) fn stated_late(model: &Model) -> HashSet<FieldId> {
    let equated: HashSet<FieldId> = model
        .relations()
        .iter()
        .filter_map(|r| match (&r.op, &r.lhs) {
            (CmpOp::EqEq, RExpr::Field(f)) => Some(*f),
            _ => None,
        })
        .collect();
    let on_balance: HashSet<FieldId> = model
        .balances()
        .iter()
        .flat_map(|b| b.draws.iter().map(|d| d.field))
        .collect();
    claimed_by_ranges(model)
        .into_iter()
        .filter(|f| on_balance.contains(f) && !equated.contains(f) && !inside_part(model, *f))
        .collect()
}

/// The fields a range with a value at its end narrows (ADR 0066). A part states its own draw
/// that way, and the draw is the part's to state: while an end has no value (a catalog row not
/// picked yet) the balance of the net must not work the current out from the others, because
/// the range would later be met with what the others allowed and the part's own maximum would
/// be lost in the overlap (a supply allowed 500 nA, a part that may draw 750 nA).
pub(super) fn claimed_by_ranges(model: &Model) -> HashSet<FieldId> {
    model
        .relations()
        .iter()
        .filter_map(|r| match (&r.op, &r.lhs, &r.rhs) {
            (CmpOp::Narrow | CmpOp::Default, RExpr::Field(f), RExpr::Range(..)) => Some(*f),
            _ => None,
        })
        .collect()
}

pub(super) fn equations_of(model: &Model) -> HashMap<FieldId, Vec<usize>> {
    let mut out: HashMap<FieldId, Vec<usize>> = HashMap::new();
    for (i, r) in model.relations().iter().enumerate() {
        if let (CmpOp::EqEq, RExpr::Field(f)) = (&r.op, &r.lhs) {
            out.entry(*f).or_default().push(i);
        }
    }
    out
}

/// The fields on the left of more than one `==` (counting a field and the fields joined to it as
/// one), not counting the relation of a `~>` chain where the design writes an equation of its
/// own (see `equations`).
pub(super) fn several_equations(model: &Model) -> HashSet<FieldId> {
    let eq = equations(model);
    model
        .relations()
        .iter()
        .zip(&eq.sides)
        .filter_map(|(r, side)| match (&r.op, &r.lhs, side) {
            (CmpOp::EqEq, RExpr::Field(f), Some(root))
                if eq.written[*root] >= 2 || (eq.written[*root] == 0 && eq.count[*root] >= 2) =>
            {
                Some(*f)
            }
            _ => None,
        })
        .collect()
}

/// For each relation, whether it is the only `==` whose left side is its field or a field
/// that holds the same value (`~` joins interfaces and so their fields, ADR 0007).
///
/// The relation a `~>` chain across a rail gives its current (ADR 0113) is the equation of
/// that current only where the design writes none: an equation the design writes about the
/// same current (`.led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance`)
/// stays the one that narrows it, as it was before the chain had one, and the chain's is checked
/// against it. Counted as a second equation, it would make the current a value that two
/// equations state (ADR 0065, 9.1), whose window a member link (`.led.current &= .current`)
/// does not hold.
pub(super) fn sole_equations(model: &Model) -> Vec<bool> {
    let eq = equations(model);
    eq.sides
        .iter()
        .zip(&eq.chain_law)
        .map(|(side, &law)| {
            side.is_some_and(|r| {
                if law {
                    eq.written[r] == 0 && eq.count[r] == 1
                } else {
                    eq.written[r] == 1
                }
            })
        })
        .collect()
}

/// The `==` relations of a model by the field on their left, a field and the fields joined to
/// it counting as one.
pub(super) struct Equations {
    /// For each relation, the root of its left side's field when it is an `==` with a field on
    /// the left.
    sides: Vec<Option<usize>>,
    /// For each relation, whether it is the relation of a `~>` chain (ADR 0113).
    chain_law: Vec<bool>,
    /// For each root, how many `==` have it on the left, and how many of them are not a chain's.
    count: Vec<usize>,
    written: Vec<usize>,
}

pub(super) fn equations(model: &Model) -> Equations {
    let mut parent: Vec<usize> = (0..model.fields().count()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for &(a, b) in model.equalities() {
        let (ra, rb) = (find(&mut parent, a.0), find(&mut parent, b.0));
        parent[ra] = rb;
    }
    let chains: HashSet<_> = model.chains().iter().map(|c| (c.source, c.span)).collect();
    let mut count = vec![0_usize; parent.len()];
    let mut written = vec![0_usize; parent.len()];
    let mut sides = Vec::new();
    let mut chain_law = Vec::new();
    for r in model.relations().iter() {
        let side = match (&r.op, &r.lhs) {
            (CmpOp::EqEq, RExpr::Field(f)) => Some(find(&mut parent, f.0)),
            _ => None,
        };
        let law = side.is_some() && chains.contains(&(r.source, r.span));
        if let Some(root) = side {
            count[root] += 1;
            if !law {
                written[root] += 1;
            }
        }
        sides.push(side);
        chain_law.push(law);
    }
    Equations {
        sides,
        chain_law,
        count,
        written,
    }
}

pub(super) fn merge(a: &Domain, b: &Domain) -> Option<Domain> {
    match (a, b) {
        (Domain::Unset, x) | (x, Domain::Unset) => Some(x.clone()),
        (Domain::Quantity(x), Domain::Quantity(y)) => {
            x.intersect(y).ok().flatten().map(Domain::Quantity)
        }
        (Domain::Text(x), Domain::Text(y)) => (x == y).then(|| a.clone()),
        _ => None,
    }
}

pub(super) fn domain_of(v: Val) -> Domain {
    match v {
        Val::Q(q) => Domain::Quantity(q),
        Val::T(t) => Domain::Text(t),
    }
}

/// How far `v` sticks out of `allowed`, as a fraction of `allowed`.
pub(super) fn overshoot(allowed: &Quantity, v: &Quantity) -> f64 {
    let out = (allowed.lo() - v.lo()).max(v.hi() - allowed.hi()).max(0.0);
    out / allowed.lo().abs().max(allowed.hi().abs()).max(1e-30)
}
