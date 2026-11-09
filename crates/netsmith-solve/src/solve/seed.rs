//! Starting a circle of values from the range a board states (ADR 0053).
//!
//! A switching regulator's input current depends on its input voltage (the power it passes,
//! over the efficiency), and the input voltage is the supply's less the drop that current
//! makes in what is in series. Propagation sets a field from an equation only when everything
//! the equation reads has a value, so with no value anywhere in such a circle nothing starts
//! (`crate::cycles` names the circle). A range stated on one field of it starts the solver,
//! and the range then matters: propagation only narrows a stated range onto what the field's
//! own equation gives when the equation lies wholly inside it, so a start taken from the
//! part's whole operating range (3 V to 65 V) never narrows once the first pass gives a value
//! that reaches below it, and the current that was worked out from its low end stays.
//!
//! Here the solver starts the circle itself, from the other end. The start is the range of a
//! value that the field's own equation reads and that already has one (the supply's), and it
//! is grown until the equation maps it into itself. That range is a claim, and it is accepted
//! only when the part's own range holds no other value that the equations allow: the part's
//! range is what its relations on the field bound (`>=` and `<=` with values), and the rest
//! of it, below and above the claim, is shown empty by the same propagation. Otherwise
//! nothing changes, and the circle stays unevaluated, as before.

use std::collections::HashSet;

use netsmith_check::{Domain, FieldId, FieldKind, RExpr, Relation};
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::{Dimension, Quantity};

use super::Solver;
use crate::eval::{Val, eval, fields_in};

/// The most times a claim is grown, and a region narrowed, before the solver gives up.
const STEPS: usize = 64;

/// The room, as a fraction, that a claim is given beyond what it was found to need, so that
/// rounding in the last digit cannot make the claim fail the check against its own equation.
const MARGIN: f64 = 1e-9;

/// A value the solver started a circle of values from, instead of waiting for a range stated
/// by the board.
#[derive(Debug, Clone)]
pub struct Seeded {
    /// The field that was given a range.
    pub field: FieldId,
    /// The range the circle's own equation maps into itself.
    pub range: Quantity,
    /// The fields with a value that the start was taken from.
    pub from: Vec<FieldId>,
    /// The part's range on the field: the claim is the only part of it that the equations
    /// allow.
    pub within: Quantity,
}

/// What the cut field's equation gives while the field is held to a range.
enum Trial {
    Value(Quantity),
    /// Propagation found no value in the range that the model allows.
    Inconsistent,
    Unknown,
}

fn interval(dim: Dimension, lo: f64, hi: f64) -> Option<Quantity> {
    Quantity::interval(dim, lo, hi).ok()
}

impl Solver<'_> {
    /// Start every circle of unset values that can be started soundly. True if any was.
    pub(super) fn seed_circles(&mut self) -> bool {
        let groups = crate::cycles::find(&self.model);
        if groups.is_empty() || !self.consistent() {
            return false;
        }
        let mut any = false;
        for group in groups {
            // An earlier start may have set some of them.
            if group
                .iter()
                .any(|&f| !matches!(self.domain(f), Domain::Unset))
            {
                continue;
            }
            for &cut in &group {
                if let Some(seed) = self.try_seed(cut) {
                    self.model
                        .set_domain(seed.field, Domain::Quantity(seed.range));
                    self.seeded.push(seed);
                    // The rest of the circle takes its values from this one.
                    self.propagate();
                    any = true;
                    break;
                }
            }
        }
        any
    }

    /// Whether propagation finds nothing wrong with the model as it is. A model that already
    /// contradicts itself would "show" every region empty, and is reported on its own.
    fn consistent(&mut self) -> bool {
        let saved = self.model.snapshot();
        let derived = self.derived.clone();
        let clean = self.propagate().is_empty();
        self.model.restore(&saved);
        self.derived = derived;
        clean
    }

    /// The fields that hold the value of `field`: those joined to it by `~`.
    pub(super) fn equated(&self, field: FieldId) -> HashSet<FieldId> {
        let mut class = HashSet::from([field]);
        loop {
            let before = class.len();
            for &(a, b) in self.model.equalities() {
                if class.contains(&a) || class.contains(&b) {
                    class.insert(a);
                    class.insert(b);
                }
            }
            if class.len() == before {
                return class;
            }
        }
    }

    /// The range the part works in: the lowest lower bound and the highest upper bound that
    /// the relations put on the fields of `class` with values the model has. `None` if either
    /// side has no bound, since then there is no end to the region to be shown empty.
    fn operating_range(&self, class: &HashSet<FieldId>, dim: Dimension) -> Option<Quantity> {
        let (mut lo, mut hi): (Option<f64>, Option<f64>) = (None, None);
        for r in self.model.relations() {
            let (other, lower) = match (&r.op, &r.lhs, &r.rhs) {
                (CmpOp::Ge, RExpr::Field(f), e) | (CmpOp::Le, e, RExpr::Field(f))
                    if class.contains(f) =>
                {
                    (e, true)
                }
                (CmpOp::Le, RExpr::Field(f), e) | (CmpOp::Ge, e, RExpr::Field(f))
                    if class.contains(f) =>
                {
                    (e, false)
                }
                _ => continue,
            };
            let Ok(Val::Q(q)) = eval(&self.model, other) else {
                continue;
            };
            if q.dim() != dim {
                continue;
            }
            if lower {
                lo = Some(lo.map_or(q.lo(), |m: f64| m.min(q.lo())));
            } else {
                hi = Some(hi.map_or(q.hi(), |m: f64| m.max(q.hi())));
            }
        }
        interval(dim, lo?, hi?)
    }

    /// Hold `cut` to `range` on a copy of the model, propagate, and read what its equation
    /// then gives.
    fn trial(&mut self, cut: FieldId, rel: &Relation, range: Quantity) -> Trial {
        let saved = self.model.snapshot();
        let derived = self.derived.clone();
        self.model.set_domain(cut, Domain::Quantity(range));
        let conflicts = self.propagate();
        let out = if !conflicts.is_empty() {
            Trial::Inconsistent
        } else {
            match eval(&self.model, &rel.rhs) {
                Ok(Val::Q(q)) => Trial::Value(q),
                _ => Trial::Unknown,
            }
        };
        self.model.restore(&saved);
        self.derived = derived;
        out
    }

    fn try_seed(&mut self, cut: FieldId) -> Option<Seeded> {
        let FieldKind::Quantity(dim) = self.model.field(cut).kind else {
            return None;
        };
        let rel = self
            .model
            .relations()
            .iter()
            .find(|r| r.op == CmpOp::EqEq && matches!(&r.lhs, RExpr::Field(f) if *f == cut))?
            .clone();
        let within = self.operating_range(&self.equated(cut), dim)?;
        // The start: the values of the field's own kind that the equation reads, through the
        // fields without a value that it reads in turn (the supply's voltage, for a voltage
        // that is the supply's less a drop).
        let mut from: Vec<(FieldId, Quantity)> = Vec::new();
        let mut seen: HashSet<FieldId> = HashSet::new();
        let mut todo: Vec<FieldId> = Vec::new();
        fields_in(&rel.rhs, &mut todo);
        while let Some(g) = todo.pop() {
            if g == cut || !seen.insert(g) {
                continue;
            }
            match &self.model.field(g).domain {
                Domain::Quantity(q) if q.dim() == dim => from.push((g, *q)),
                Domain::Unset if self.model.field(g).kind == FieldKind::Quantity(dim) => {
                    for r in self.model.relations() {
                        if r.op == CmpOp::EqEq && matches!(&r.lhs, RExpr::Field(f) if *f == g) {
                            fields_in(&r.rhs, &mut todo);
                        }
                    }
                }
                _ => {}
            }
        }
        from.sort_by_key(|&(g, _)| g);
        let first = from.first()?.1;
        let (lo, hi) = from
            .iter()
            .fold((first.lo(), first.hi()), |(lo, hi), (_, q)| {
                (lo.min(q.lo()), hi.max(q.hi()))
            });
        let mut claim = interval(dim, lo, hi)?;
        if !within.encloses(&claim).unwrap_or(false) {
            return None;
        }

        // Grow the claim until the equation maps it into itself. A step that moves an end
        // moves it twice as far, so the sequence does not creep up on its limit.
        let mut grown = false;
        for _ in 0..STEPS {
            let Trial::Value(v) = self.trial(cut, &rel, claim) else {
                return None;
            };
            if claim.encloses(&v).unwrap_or(false) {
                grown = true;
                break;
            }
            let lo = if v.lo() < claim.lo() {
                2.0 * v.lo() - claim.lo()
            } else {
                claim.lo()
            };
            let hi = if v.hi() > claim.hi() {
                2.0 * v.hi() - claim.hi()
            } else {
                claim.hi()
            };
            claim = interval(dim, lo, hi)?;
            if !within.encloses(&claim).unwrap_or(false) {
                return None;
            }
        }
        if !grown {
            return None;
        }
        // The growth overshoots, so the claim is narrowed to what the equation gives from it
        // (the result of mapping a claim into itself is again such a claim), until it stops
        // moving, with room for rounding at each step. Then it is checked once more as it
        // will be stated.
        let room = |q: &Quantity| MARGIN * q.lo().abs().max(q.hi().abs()).max(1e-30);
        for _ in 0..STEPS {
            let Trial::Value(v) = self.trial(cut, &rel, claim) else {
                return None;
            };
            let tighter = interval(dim, v.lo() - room(&v), v.hi() + room(&v))?;
            if !claim.encloses(&tighter).unwrap_or(false) {
                break;
            }
            let moved = (claim.lo() - tighter.lo())
                .abs()
                .max((claim.hi() - tighter.hi()).abs());
            claim = tighter;
            if moved <= 1e-6 * claim.lo().abs().max(claim.hi().abs()) {
                break;
            }
        }
        let claim = interval(dim, claim.lo() - room(&claim), claim.hi() + room(&claim))?;
        if !within.encloses(&claim).unwrap_or(false) {
            return None;
        }
        match self.trial(cut, &rel, claim) {
            Trial::Value(v) if claim.encloses(&v).unwrap_or(false) => {}
            _ => return None,
        }

        // The rest of the part's range, below and above the claim, has no value that the
        // equation allows.
        for (a, b) in [(within.lo(), claim.lo()), (claim.hi(), within.hi())] {
            if a < b && !self.empty_region(cut, &rel, dim, a, b) {
                return None;
            }
        }
        Some(Seeded {
            field: cut,
            range: claim,
            from: from.iter().map(|&(g, _)| g).collect(),
            within,
        })
    }

    /// Whether no value between `a` and `b` is one the equation allows: held to the region,
    /// propagation either finds the model inconsistent or the equation's value leaves nothing
    /// of it, after the region is narrowed to what the equation gives as often as it takes.
    fn empty_region(
        &mut self,
        cut: FieldId,
        rel: &Relation,
        dim: Dimension,
        a: f64,
        b: f64,
    ) -> bool {
        let Some(mut region) = interval(dim, a, b) else {
            return false;
        };
        for _ in 0..STEPS * 2 {
            let v = match self.trial(cut, rel, region) {
                Trial::Inconsistent => return true,
                Trial::Unknown => return false,
                Trial::Value(v) => v,
            };
            match region.intersect(&v) {
                Ok(None) => return true,
                Ok(Some(next)) => {
                    // No narrower: there may be a value in it.
                    let same = (next.lo() - region.lo()).abs()
                        <= 1e-12 * region.lo().abs().max(1e-30)
                        && (next.hi() - region.hi()).abs() <= 1e-12 * region.hi().abs().max(1e-30);
                    if same {
                        return false;
                    }
                    region = next;
                }
                Err(_) => return false,
            }
        }
        false
    }
}
