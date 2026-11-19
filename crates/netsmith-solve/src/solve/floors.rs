//! The floor of a rail (issue #1301): the voltage a rail may fall to while the regulator behind
//! it is in dropout, which the board states as `.voltage_floor` on the rail
//! (`.v3.voltage_floor &= 2.7V`, `std::sources::Rail`).
//!
//! The DC analysis holds the rail at its `.voltage` (the set point and its tolerance), and the
//! regulator's own relations (its divider, its on-time) are judged there. A rail with a floor
//! may also sit anywhere in its _window_, from the floor up to the top of `.voltage`, so a bound
//! that speaks of the least voltage the rail reaches is judged over the window instead:
//!
//! - **A bound that reads `min(.voltage)`** of the rail: the dropout check of `LinearRegulator`
//!   and `Buck` (`.vin.voltage >= min(.vout.voltage) + .dropout`), or a board's own bound. The
//!   least voltage is then the floor, so the regulator may run deeper in dropout, down to an
//!   input of the floor plus its dropout.
//! - **A bound between the rail's voltage and numbers alone**: a load's supply range
//!   (`.vcc.voltage >= 2.4V`). The load must work over the whole window, so a floor below what a
//!   load on the rail accepts is an error that names the floor.
//!
//! Every other rail voltage the bound reads is held to its window as well (a regulator fed from
//! another one's output is judged from the first one's floor). A bound that reads the floor
//! itself (`Rail`'s `.voltage_floor <= min(.voltage)`), or the centre of a range
//! (`nominal(.voltage)`), is judged as written. A floor at or above the rail's least voltage
//! changes nothing. Rails joined with `~` share the floor, as they share `.voltage`. Not judged
//! over the window: a relation that works a value out of the rail's voltage (a load's
//! `.levels.vih &= 0.6 * .vcc.voltage`, a resistor's current), which stays at `.voltage`.

use std::collections::{HashMap, HashSet};

use netsmith_check::{Domain, FieldId, FieldKind, Member, Model, RExpr, RangeEnd, Relation};
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::{Dimension, Quantity};

use super::Solver;
use crate::eval::{EvalError, Val, eval_with, fields_in};

/// The member a rail states its floor in.
pub(super) const FLOOR: &str = "voltage_floor";

/// The rails of a design that have a floor member, made once for a solve.
#[derive(Debug, Default)]
pub(super) struct Floors {
    /// Each rail's `.voltage` whose floor a statement gives, with the fields that may hold it:
    /// the rail's own `.voltage_floor` and those joined to it.
    of: HashMap<FieldId, Vec<FieldId>>,
    /// Every floor field, so a bound that reads one is judged as written.
    floor_fields: HashSet<FieldId>,
}

impl Floors {
    pub fn of(model: &Model) -> Self {
        let voltage = |f: FieldId| model.field(f).kind == FieldKind::Quantity(Dimension::VOLTAGE);
        let mut pairs = Vec::new();
        for (id, _) in model.instances() {
            if let (Some(Member::Field(v)), Some(Member::Field(f))) =
                (model.member(id, "voltage"), model.member(id, FLOOR))
                && voltage(v)
                && voltage(f)
            {
                pairs.push((v, f));
            }
        }
        if pairs.is_empty() {
            return Self::default();
        }
        // The classes of floor fields that joins hold equal.
        let mut parent: HashMap<FieldId, FieldId> = pairs.iter().map(|&(_, f)| (f, f)).collect();
        fn root(parent: &mut HashMap<FieldId, FieldId>, mut f: FieldId) -> FieldId {
            while parent[&f] != f {
                let up = parent[&parent[&f]];
                parent.insert(f, up);
                f = up;
            }
            f
        }
        for &(a, b) in model.equalities() {
            if parent.contains_key(&a) && parent.contains_key(&b) {
                let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                if ra != rb {
                    parent.insert(ra, rb);
                }
            }
        }
        let mut class: HashMap<FieldId, Vec<FieldId>> = HashMap::new();
        for &(_, f) in &pairs {
            let r = root(&mut parent, f);
            class.entry(r).or_default().push(f);
        }
        // A floor has a value only where a statement gives one: a narrowing (`&=`, or a default
        // that applied) or an equation with the floor on its left. A class with neither is
        // left out, so a design that states no floor pays nothing in `verify`.
        let equated: Vec<FieldId> = model
            .relations()
            .iter()
            .filter_map(|r| match (r.op, &r.lhs) {
                (CmpOp::EqEq, RExpr::Field(f)) => Some(*f),
                _ => None,
            })
            .collect();
        let stated = |f: &FieldId| !model.field(*f).narrowed_at.is_empty() || equated.contains(f);
        let mut out = Self::default();
        for &(v, f) in &pairs {
            out.floor_fields.insert(f);
            let r = root(&mut parent, f);
            if class[&r].iter().any(stated) {
                out.of.insert(v, class[&r].clone());
            }
        }
        out
    }
}

/// `min(.f)` or `nominal(.f)` of a field anywhere in an expression.
fn reads_end_of(e: &RExpr, end: RangeEnd, f: FieldId) -> bool {
    match e {
        RExpr::End(e, g) => *e == end && *g == f,
        RExpr::Neg(x) => reads_end_of(x, end, f),
        RExpr::Bin(_, a, b) => reads_end_of(a, end, f) || reads_end_of(b, end, f),
        RExpr::Range(a, b) => [a, b]
            .into_iter()
            .flatten()
            .any(|x| reads_end_of(x, end, f)),
        RExpr::Const(_) | RExpr::Text(_) | RExpr::Field(_) => false,
    }
}

impl Solver<'_> {
    /// The floor a rail's voltage field has, as the least value of what states it: none when
    /// nothing states one, or when it is not below the rail's least voltage.
    fn floor_of(&self, voltage: FieldId) -> Option<f64> {
        let floors = self.floors.of.get(&voltage)?;
        let Domain::Quantity(v) = &self.model.field(voltage).domain else {
            return None;
        };
        let floor = floors
            .iter()
            .filter_map(|&f| match &self.model.field(f).domain {
                Domain::Quantity(q) if q.dim() == Dimension::VOLTAGE => Some(q.lo()),
                _ => None,
            })
            .reduce(f64::min)?;
        (floor.is_finite() && floor < v.lo()).then_some(floor)
    }

    /// The rail voltages a bound is judged at over their windows (see the module), each with its
    /// window: empty for a bound that is judged as written.
    pub(super) fn floor_values(&self, r: &Relation) -> Vec<(FieldId, Quantity)> {
        if self.floors.of.is_empty() || !matches!(r.op, CmpOp::Ge | CmpOp::Le) {
            return Vec::new();
        }
        let mut fields = Vec::new();
        fields_in(&r.lhs, &mut fields);
        fields_in(&r.rhs, &mut fields);
        fields.dedup();
        if fields.iter().any(|f| self.floors.floor_fields.contains(f)) {
            return Vec::new();
        }
        let reads = |end, f| reads_end_of(&r.lhs, end, f) || reads_end_of(&r.rhs, end, f);
        let mut windows = Vec::new();
        for &f in &fields {
            if windows.iter().any(|(g, _)| *g == f) {
                continue;
            }
            if let Some(floor) = self.floor_of(f) {
                if reads(RangeEnd::Nominal, f) {
                    return Vec::new();
                }
                let Domain::Quantity(v) = &self.model.field(f).domain else {
                    continue;
                };
                if let Ok(w) = Quantity::interval(Dimension::VOLTAGE, floor, v.hi()) {
                    windows.push((f, w));
                }
            }
        }
        let alone = matches!(fields.as_slice(), [f] if matches!(&r.lhs, RExpr::Field(g) if g == f)
            || matches!(&r.rhs, RExpr::Field(g) if g == f));
        let qualifies = alone || windows.iter().any(|&(f, _)| reads(RangeEnd::Min, f));
        if qualifies { windows } else { Vec::new() }
    }

    /// Both sides of a bound, with the rail voltages of `windows` held to their windows.
    pub(super) fn eval_bound(
        &self,
        r: &Relation,
        windows: &[(FieldId, Quantity)],
    ) -> Result<(Val, Val), EvalError> {
        Ok((
            eval_with(&self.model, &r.lhs, windows)?,
            eval_with(&self.model, &r.rhs, windows)?,
        ))
    }

    /// `` (with `v3.voltage` over its window 2.7V..3.36V, from its `voltage_floor`) ``, for a
    /// message of a bound judged over windows.
    pub(super) fn floor_clause(&self, windows: &[(FieldId, Quantity)]) -> String {
        let each: Vec<String> = windows
            .iter()
            .map(|(f, w)| format!("`{}` over its window {w}", self.model.path_of_field(*f)))
            .collect();
        format!(
            " (with {}, from the rail's `.{FLOOR}`: the least voltage it may fall to in dropout)",
            each.join(" and ")
        )
    }
}
