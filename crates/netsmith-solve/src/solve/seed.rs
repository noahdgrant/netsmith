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
//!
//! # What was found (ADR 0053)
//!
//! The first board of a real design stated `3V..65V`, the regulator's whole input range, by
//! hand, and the solver settled at 4.4 A where the true worst case was 1.3 A. The loose answer is
//! not the interval arithmetic widening and not an order of evaluation. It is a rule of
//! `propagate`: an equation narrows a field that already has a range only when the value it
//! gives lies wholly inside it. That rule is right and stays: a range written with `&=` is a claim
//! that a check holds against, and narrowing it to the overlap would make a check pass for a range
//! nobody guaranteed. In a circle it has this effect. A load of 0.5 W behind 36 ohm on a supply of
//! 9 V to 16 V, with the load's voltage started at `3V..30V` (invented values, the last test of
//! `tests/seed.rs`): the first pass works the current out at the bottom of the range, up to
//! 166.7 mA; the drop at that current is 6 V, so the equation gives 2.94 V to 15.41 V, which
//! reaches below 3 V, so it does not narrow `3V..30V`. That is a fixed point of the propagator,
//! not of the circuit. With a smaller drop the first pass lies inside the range and the answer
//! is tight, so which answer a board gets depends on the start and not on the circuit.
//!
//! A start from the other end, the supply's `9V..16V`, does not have this effect: for 10 ohm the
//! equation maps it into 8.44 V to 15.7 V and it settles at 8.398 V to 15.69 V. But it is a claim
//! and not a derived value, and the circle has more than one solution. A constant-power load
//! behind a resistor has a second working point near 0 V for any resistance: `v * v - s * v + r *
//! p = 0` has two roots, 8.4 V and 0.6 V for 10 ohm, 9 V and 0.5 W. Interval arithmetic cannot
//! tell the points apart, and no range the board states decides between them either.
//!
//! # The steps
//!
//! When a circle is found and nothing in it has a value, the solver tries to start it, once before
//! the parts are picked and again after, for a circle that a pick closed.
//!
//! 1. The field to start (the cut) is a field of the circle whose own equation (`==`) sets it. Its
//!    start is the hull of the ranges of the values of the same kind that the equation reads,
//!    directly or through other fields without a value, that already have one: the supply's voltage
//!    for a voltage. If there is none, nothing is done.
//! 2. The part's range is what the relations (`>=`, `<=`) on the field, or the fields joined to it
//!    by `~`, bound, as values the model has: the lowest lower bound and the highest upper bound.
//!    If either end is missing, nothing is done.
//! 3. The start must be inside the part's range. It is grown until the equation maps it into itself
//!    (an end that moves is moved twice as far, so the sequence does not creep), then narrowed to
//!    what the equation gives from it until it stops moving, with a room of one part in 10^9 so
//!    that rounding in the last digit cannot fail the equation's own check. If it leaves the part's
//!    range, or does not settle in 64 steps, nothing is done.
//! 4. The rest of the part's range, below the claim and above it, is shown to hold no value that
//!    the equation allows: the field is held to the region, `propagate` runs, and the region is
//!    narrowed to what the equation gives, again and again. If propagation finds the model
//!    inconsistent, or the region becomes empty, it is shown; if it stops getting narrower, or
//!    does not settle, it is not, and nothing is done. This step decides soundness.
//! 5. Otherwise the field has the claim as its range, as if the board had stated it. The
//!    equation's check then holds the claim to what it gives, so a claim that is not a fixed point
//!    cannot pass.
//!
//! The solver says so in a note (`Solution::seeded`; `netsmith build` prints `note: ...`; the
//! wording is `started_note` in `crates/netsmith/src/lib.rs`). A range the board states on any
//! field of the circle is used as before and nothing is started. The claim: every value of the
//! field that satisfies the circuit's equations, with the part inside its stated range, is in the
//! range found. Nothing is claimed below the part's range: a regulator that states a `.vin.voltage`
//! of at least 3V and at most 65V works between 3 V and 65 V and is off below, and a state with the
//! input at 0.2 V is the converter off, which the input-current equation (written for a converter
//! that is on) does not describe. A design that must also hold in states the part's range leaves
//! out (a supply that sags below the regulator's minimum) writes a relation of its own.
//!
//! # Rejected
//!
//! Narrowing a stated range onto the overlap with the equation (the first idea): tight, but not
//! sound for a claim (`7V..16V` stated where the circuit gives `5.9V..16V` would pass). Starting
//! from the whole part's range and iterating down: it stops where `propagate` stops, the loose
//! answer. Accepting the first range the equation maps into itself with no check of the rest: it
//! passes checks on a range the circle does not guarantee when a second working point is inside
//! the part's range. Excluding the region without the part's range: impossible for a
//! constant-power load. Refusing to start a circle and keeping the hand seed: it leaves the
//! checks of a regulator board unevaluated until a person verifies a tight range by hand. Starting
//! a range the board stated: it is a claim and not a start.
//!
//! # Cost, limits
//!
//! A circle costs a few dozen copies of the model and propagation passes when started, and a
//! handful when refused (usually on the first step); a board with no circle pays one scan of its
//! relations. The model is copied for each step, which is slow for a very large board, and the
//! circle is looked at twice. The cut is the first field of the circle, in field order, that has
//! an equation and passes the steps; every claim accepted has been shown to hold, so the answers
//! are all sound, and the first is used so that the same board always gives the same note. The
//! start is a guess about where to look and the checks make it sound, so a circle with no value
//! of its own kind to start from (a current circle with no stated current) is not started even
//! when the equations could be; the guide's example with no range on the load is such a board. A
//! part that states its limits the other way (`.vin.voltage - .vout.voltage >= 1V`) has no range
//! for this purpose.
//!
//! # Left open
//!
//! "What counts as a state of a part (the part's own `>=` and `<=` on a field say where its
//! equations apply, and a state outside is not a state of the circuit for the checks) is the
//! owner's to accept or change; the alternative is to refuse more circles." Also for the owner to
//! look at: that a value starts only from a stated range of the same kind, and that
//! `Solution::seeded` and the note are the whole interface (no new syntax, no new member).

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
