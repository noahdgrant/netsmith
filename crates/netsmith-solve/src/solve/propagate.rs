//! Propagation: narrowing every value to what the connections, equations and balances of currents allow.

use std::collections::HashSet;

use super::equations::{domain_of, merge};
use super::{Budget, Conflict, CurrentBound, Solver, Verdict};
use crate::eval::fields_in;
use netsmith_check::{
    Balance, Diagnostic, Domain, Draw, FieldId, FieldKind, InstKind, Member, RExpr, Relation,
    SourceId,
};
use netsmith_core::job;
use netsmith_lang::Span;
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::{Dimension, Quantity};

impl Solver<'_> {
    pub(super) fn report(&mut self, source: SourceId, span: Span, message: String) {
        let d = Diagnostic {
            source,
            span,
            message,
        };
        if self.seen.insert(d.clone()) {
            self.diagnostics.push(d);
        }
    }

    pub(super) fn domain(&self, f: FieldId) -> Domain {
        self.model.field(f).domain.clone()
    }

    /// A failure of a relation as it is reported: with what the currents say, or, for a bound
    /// that the solver added, with the statements that make it a bound on the part (ADR 0065).
    pub(super) fn relation_message(&self, index: usize, r: &Relation, message: String) -> String {
        if index >= self.original_relations {
            self.transferred_message(index, message)
        } else {
            let mut fields = Vec::new();
            r.lhs.fields(&mut fields);
            r.rhs.fields(&mut fields);
            let note = self.dc_note(&fields);
            format!("{}{note}", self.with_currents(r, message))
        }
    }

    /// Set a field's domain in `propagate`, noting when.
    pub(super) fn set_field(&mut self, field: FieldId, domain: Domain) {
        self.clock += 1;
        self.stamp[field.0] = self.clock;
        self.model.set_domain(field, domain);
    }

    pub(super) fn propagate(&mut self) -> Vec<Conflict> {
        let mut conflicts: Vec<Conflict> = Vec::new();
        let mut said: HashSet<String> = HashSet::new();
        // The structure does not change while values narrow, so it is copied once.
        let equalities = self.model.equalities().to_vec();
        let relations = std::sync::Arc::clone(self.model.shared_relations());
        let balances = std::sync::Arc::clone(self.model.shared_balances());
        let reads = self.reads();
        // The pass in which each item was last worked through, as a time on the clock of
        // `set_field`: an item whose fields have not changed since is worked through again to
        // the same effect (it changed nothing, or it changed its own field, which is a change).
        let mut equalities_at = vec![0u64; equalities.len()];
        let mut relations_at = vec![0u64; relations.len()];
        let mut balances_at = vec![0u64; balances.len()];
        for _ in 0..64 {
            job::checkpoint();
            let mut changed = false;
            // A pass reads every equation, relation and balance, to skip those that did not
            // change: the work of a trial grows with the board (`SearchBudget::work`).
            self.work += (equalities.len() + relations.len() + balances.len()) as u64;

            for (item, &(a, b)) in equalities.iter().enumerate() {
                let at = equalities_at[item];
                if at != 0 && self.stamp[a.0] <= at && self.stamp[b.0] <= at {
                    continue;
                }
                equalities_at[item] = self.clock;
                let (da, db) = (self.domain(a), self.domain(b));
                match merge(&da, &db) {
                    Some(m) => {
                        if m != da {
                            self.set_field(a, m.clone());
                            changed = true;
                        }
                        if m != db {
                            self.set_field(b, m);
                            changed = true;
                        }
                    }
                    None => {
                        let owner = self.model.instance(self.model.field(a).owner);
                        // Point at a statement that set one of the two values, if any.
                        let (source, span) = [a, b]
                            .iter()
                            .find_map(|&f| self.model.field(f).narrowed_at.last().copied())
                            .unwrap_or((owner.source, owner.span));
                        let message = format!(
                            "connected values disagree: `{}` is {} but `{}` is {}",
                            self.model.path_of_field(a),
                            self.model.show_value(a, &da),
                            self.model.path_of_field(b),
                            self.model.show_value(b, &db)
                        );
                        if said.insert(message.clone()) {
                            conflicts.push(Conflict {
                                source,
                                span,
                                message,
                                fields: vec![a, b],
                            });
                        }
                    }
                }
            }

            for (index, r) in relations.iter().enumerate() {
                job::checkpoint();
                if !matches!(r.op, CmpOp::EqEq | CmpOp::Narrow) {
                    continue;
                }
                let RExpr::Field(f) = &r.lhs else { continue };
                let at = relations_at[index];
                if at != 0 && reads.inputs_of(index).iter().all(|f| self.stamp[f.0] <= at) {
                    continue;
                }
                relations_at[index] = self.clock;
                let Some(v) = reads.value_of_rhs(&self.model, index, r) else {
                    continue;
                };
                let value = domain_of(v);
                let current = self.domain(*f);
                match (r.op, &current) {
                    (_, Domain::Unset) => {
                        self.set_field(*f, value);
                        changed = true;
                    }
                    (CmpOp::Narrow, _) => match merge(&current, &value) {
                        Some(m) => {
                            if m != current {
                                self.set_field(*f, m);
                                changed = true;
                            }
                        }
                        None => {
                            let message = format!(
                                "contradiction on `{}`: it is already {}, which does not overlap {}",
                                self.model.path_of_field(*f),
                                self.model.show_value(*f, &current),
                                self.model.show_value(*f, &value)
                            );
                            if said.insert(message.clone()) {
                                let mut fields = vec![*f];
                                fields_in(&r.rhs, &mut fields);
                                conflicts.push(Conflict {
                                    source: r.source,
                                    span: r.span,
                                    message,
                                    fields,
                                });
                            }
                        }
                    },
                    // An equation on a field that already has a range: the field's value
                    // is the expression's, so its set is the intersection of the two. When
                    // the expression's range lies inside the field's, the intersection is
                    // the expression's range, and the field takes it (a stated window of
                    // current becomes the current the circuit really carries, which is
                    // what a power rating must be checked against). When it does not lie
                    // inside, `verify` reports the equation with both ranges as they
                    // were, so the field is left alone here: narrowing it to the overlap
                    // would make the report name a range nobody wrote. Text and fields of
                    // another dimension are left to `verify` as well.
                    // A current a balance worked out is an enclosure of the real value,
                    // not a range anyone wrote (ADR 0038), so an equation on it and the
                    // balance both hold: it takes what the two say. Only a pair that share
                    // nothing is a fault, which `verify` reports.
                    (CmpOp::EqEq, Domain::Quantity(d)) if self.derived.contains(f) => {
                        if let Domain::Quantity(v) = &value
                            && let Ok(Some(both)) = d.intersect(v)
                            && both != *d
                        {
                            self.set_field(*f, Domain::Quantity(both));
                            changed = true;
                        }
                    }
                    (CmpOp::EqEq, Domain::Quantity(d)) if self.sole_equation[index] => {
                        if let Domain::Quantity(v) = &value
                            && v != d
                            && d.encloses(v).unwrap_or(false)
                        {
                            self.set_field(*f, value);
                            changed = true;
                        }
                    }
                    // A value that several equations state and no statement bounds (the
                    // frequency of an oscillator that the board's window and the circuit both
                    // give): the first equation that could be worked out gave it a range, and
                    // that range is stale once a pick narrows what the equation reads. The
                    // value is what each equation says, so it follows an equation whose range
                    // lies inside it. Without this the equation that came later (the window)
                    // was judged against the range of the earlier one and passed whatever the
                    // pick was (ADR 0065, "A value that two equations state"). Only for a value
                    // that a pick can change (`fed`), as in `verify`: two equations that no pick
                    // changes keep the old rule (ADR 0065, section 9.1, "Not done").
                    (CmpOp::EqEq, Domain::Quantity(d))
                        if self.several_equations.contains(f)
                            && (self.fed.contains(f) || self.fed_at_start.contains(f))
                            && self.model.field(*f).narrowed_at.is_empty() =>
                    {
                        if let Domain::Quantity(v) = &value
                            && v != d
                            && d.encloses(v).unwrap_or(false)
                        {
                            self.set_field(*f, value);
                            changed = true;
                        }
                    }
                    _ => {}
                }
            }

            // What a board draws is settled from inside it. A joined port that no board
            // can settle that way takes its value from the join, which can only say what the
            // others allow, so the joins wait until nothing else changes.
            for joins in [false, true] {
                for (item, b) in balances.iter().enumerate() {
                    job::checkpoint();
                    let is_join = self.model.instance(b.scope).kind == InstKind::System;
                    if is_join != joins || b.returns {
                        continue;
                    }
                    let at = balances_at[item];
                    if at != 0 && b.draws.iter().all(|d| self.stamp[d.field.0] <= at) {
                        continue;
                    }
                    balances_at[item] = self.clock;
                    if let Some((field, value)) = self.solve_balance(b) {
                        match self.domain(field) {
                            Domain::Unset => {
                                self.derived.insert(field);
                            }
                            // Worked out before: it becomes what the old and the new
                            // value both say, so a value that went stale on one side is
                            // tightened and nothing the circuit allows is lost. Values
                            // that share nothing are left to `verify_balances`.
                            Domain::Quantity(old) => match self.reworked(b, &old, &value) {
                                Some(both) => {
                                    self.set_field(field, Domain::Quantity(both));
                                    changed = true;
                                    continue;
                                }
                                None => continue,
                            },
                            _ => continue,
                        }
                        self.set_field(field, Domain::Quantity(value));
                        changed = true;
                    } else if let Some(budget) = self.budget(b)
                        && budget.stated.encloses(&budget.works_out).unwrap_or(false)
                    {
                        // A stated current is a budget (ADR 0038): what the rest of the
                        // net determines lies inside it, and is what the term carries.
                        let field = budget.field;
                        let Domain::Quantity(old) = self.domain(field) else {
                            continue;
                        };
                        if let Ok(Some(both)) = old.intersect(&budget.field_value())
                            && both != old
                        {
                            self.set_field(field, Domain::Quantity(both));
                            changed = true;
                        }
                    }
                }
                if changed {
                    break;
                }
            }

            if !changed {
                break;
            }
        }
        conflicts
    }

    /// What a rail or chain adds to its net's balance: its current, negated if it is flipped.
    /// `None` while the current is unknown.
    pub(super) fn contribution(&self, d: &Draw) -> Option<Quantity> {
        match &self.model.field(d.field).domain {
            Domain::Quantity(q) => Some(if d.flipped { q.neg() } else { *q }),
            _ => None,
        }
    }

    /// Whether a term is a part's through-current (the first or last part of a chain, ADR
    /// 0014) and not the current of a rail or a port.
    pub(super) fn is_part_current(&self, d: &Draw) -> bool {
        let owner = self.model.field(d.field).owner;
        self.model.instance(owner).kind == InstKind::Part
    }

    /// If exactly one current on the net is unknown, its value: whatever makes the rest add up
    /// to zero. The current of a rail or a port is set this way; a part's through-current only
    /// from a complete balance (ADR 0027) of two terms (ADR 0026). From a longer balance it is
    /// verified and never set: the residual of several wide ranges says little about one part,
    /// and its own relations (Ohm's law, the LED equation, a stated value) are what determine it.
    ///
    /// The fault that the rule answers (ADR 0026): a resistor heading a divider on a regulated
    /// rail, whose current was the one unknown of a balance that held the regulator's supplied
    /// current (0 to 2.1 A) and several loads (0 to 2 A), got the residual, -2.1 A to 2.1 A or
    /// worse, and its power check failed with `-1.7e-199 fW..461 kW`. The terms of a balance are
    /// independent ranges, so the difference of their sums is wider than any one of them and says
    /// nothing about a part. A balance of two terms is an equality and nothing is widened: a fuse
    /// in series with a load carries the load's current, and the inductor of `Buck`, in a
    /// chain from the IC's switch pin to the output, carries the output's. Refusing these would
    /// have meant stating `.fuse.current == .load.current` on every board, which ADR 0014
    /// removed; a first version that refused every part broke four tests, all two-term cases.
    /// The cost: a part at the head of a chain on a busy rail whose current nothing else states
    /// is not checked against its rating, and the build says `not evaluated`; the design can
    /// state the current. ADR 0027 (only a complete balance) and ADR 0038 (a stated current is a
    /// budget) narrowed later what a balance sets and verifies.
    fn solve_balance(&self, b: &Balance) -> Option<(FieldId, Quantity)> {
        if let Some(found) = self.solve_balance_but(b, None) {
            return Some(found);
        }
        // Everything is known: a current worked out earlier, from terms that have narrowed
        // since, is worked out again. The first one whose value changes is returned.
        b.draws
            .iter()
            .filter(|d| self.derived.contains(&d.field))
            .filter_map(|d| self.solve_balance_but(b, Some(d.field)))
            .find(|(f, v)| {
                matches!(self.domain(*f), Domain::Quantity(old)
                    if self.reworked(b, &old, v).is_some())
            })
    }

    /// What a current worked out before becomes when a balance works it out again as `new`,
    /// if that changes it. It is what both say. At a join between boards it is `new` only
    /// when that lies inside the old one: the join says what the others allow, and clipping
    /// a port's draw to it would hide a board that asks for more than the other gives
    /// (`verify_join`).
    fn reworked(&self, b: &Balance, old: &Quantity, new: &Quantity) -> Option<Quantity> {
        let both = if self.model.instance(b.scope).kind == InstKind::System {
            old.encloses(new).unwrap_or(false).then_some(*new)
        } else {
            old.intersect(new).ok().flatten()
        };
        both.filter(|both| both != old)
    }

    /// `solve_balance` with the current of `again`, if any, taken as unknown.
    fn solve_balance_but(
        &self,
        b: &Balance,
        again: Option<FieldId>,
    ) -> Option<(FieldId, Quantity)> {
        let mut unknown = None;
        let mut rest: Option<Quantity> = None;
        for d in &b.draws {
            let known = if Some(d.field) == again {
                None
            } else {
                self.contribution(d)
            };
            match known {
                None if unknown.is_none() => unknown = Some(d),
                None => return None,
                Some(q) => {
                    rest = match rest {
                        None => Some(q),
                        Some(sum) => Some(sum.add(&q).ok()?),
                    }
                }
            }
        }
        let u = unknown?;
        // A current that a range with values at its ends will state, and whose ends are not
        // known yet, is not worked out from the others (ADR 0066).
        if self.claimed.contains(&u.field) && self.domain(u.field) == Domain::Unset {
            return None;
        }
        // An incomplete balance (ADR 0027) has pins whose currents are not terms. What the
        // terms leave is theirs as much as a part's, so no part is set from it; a rail is, as
        // before (an enable pin on a supply net does not stop the supply being worked out),
        // but not from nothing: a lone port is not taken to be zero when something unmodelled
        // is on the net.
        if !b.is_complete() && (self.is_part_current(u) || rest.is_none()) {
            return None;
        }
        // A balance of two terms equates them (a fuse in series with a load carries the
        // load's current); a longer one subtracts, and what it leaves is not a part's. A term
        // that is exactly zero (a board's rail stated `&= 0A`: nothing else draws here) adds
        // nothing, so it is not a term: the same two currents are equal.
        let live = b
            .draws
            .iter()
            .filter(|d| !self.model.draw_is_zero(d))
            .count();
        if live > 2 && self.is_part_current(u) {
            return None;
        }
        let value = match rest {
            Some(sum) if u.flipped => sum,
            Some(sum) => sum.neg(),
            None => Quantity::point(Dimension::CURRENT, 0.0).ok()?,
        };
        Some((u.field, value))
    }

    /// The budget on a net, if it has one (ADR 0038): a complete balance with every current
    /// known, exactly one of them stated (a range the design wrote, which is an allowance)
    /// and no current that a balance worked out (such a current takes up whatever the rest
    /// leaves, so it says nothing about the stated one). What the other terms determine is
    /// what the stated one works out to, and it has to lie inside what was stated.
    /// The range a design stated for a current in a balance, if it is a budget term.
    fn stated_range(&self, field: FieldId) -> Option<Quantity> {
        if let Some(q) = self.stated.get(&field) {
            return Some(*q);
        }
        match self.domain(field) {
            Domain::Quantity(q) if self.stated_late.contains(&field) => Some(q),
            _ => None,
        }
    }

    pub(super) fn budget(&self, b: &Balance) -> Option<Budget> {
        if !b.is_complete()
            || b.draws.len() < 2
            || self.model.instance(b.scope).kind == InstKind::System
            || b.draws.iter().any(|d| self.derived.contains(&d.field))
        {
            return None;
        }
        let known: Vec<Quantity> = b
            .draws
            .iter()
            .map(|d| self.contribution(d))
            .collect::<Option<_>>()?;
        let mut stated = b
            .draws
            .iter()
            .enumerate()
            // A current stated as exactly zero adds nothing to the net and is not an
            // allowance (a part's reference rail, ADR 0066), so it is not the stated term.
            .filter(|(_, d)| self.stated_range(d.field).is_some() && !self.model.draw_is_zero(d));
        let (index, d) = stated.next()?;
        if stated.next().is_some() {
            return None;
        }
        let mut others = known
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != index)
            .map(|(_, q)| *q);
        let first = others.next()?;
        let sum = others.try_fold(first, |sum, q| sum.add(&q).ok())?;
        let range = self.stated_range(d.field)?;
        Some(Budget {
            field: d.field,
            index,
            flipped: d.flipped,
            stated: if d.flipped { range.neg() } else { range },
            works_out: sum.neg(),
        })
    }

    /// With every current known, the net has to be able to balance. A stated current is a
    /// budget (ADR 0038): when the rest of the net determines it, that must lie inside what
    /// was stated, and need not fill it. Otherwise the currents together have to be able to
    /// add up to zero. The currents are independent, so this is a necessary condition, not a
    /// sufficient one, and a net that fails it cannot balance. A return net is checked after the supply nets,
    /// and only when nothing has been reported: a fault shows there again, and the first
    /// message is enough.
    /// Of the relations that wait on a value, those that a bound from Ohm's law proves (see
    /// [`crate::ohm`]) are made, and the rest stay as they are. The relations of a part whose
    /// current has no value and is bounded are read once more with the bound standing in for
    /// the current (and for the drop across it, where that has no value either), and each that
    /// passes is no longer waiting. The values stand in only while a relation is read: nothing
    /// is kept in the model, and a relation that fails or still cannot be judged is as before.
    pub(super) fn prove_by_ohm(&mut self, deferred: Vec<usize>) -> (Vec<usize>, Vec<CurrentBound>) {
        let bounds = crate::ohm::bounds(&self.model);
        let mut kept = Vec::new();
        for i in deferred {
            let r = self.model.relations()[i].clone();
            let mut fields = Vec::new();
            r.lhs.fields(&mut fields);
            r.rhs.fields(&mut fields);
            let owner = fields.first().map(|&f| self.model.field(f).owner);
            let bound = owner.and_then(|o| bounds.iter().find(|b| b.part == o));
            let proven = match bound {
                Some(b)
                    if fields
                        .iter()
                        .all(|&f| Some(self.model.field(f).owner) == owner) =>
                {
                    self.proven_with(b, &r)
                }
                _ => false,
            };
            if !proven {
                kept.push(i);
            }
        }
        let bounds = bounds
            .iter()
            .map(|b| CurrentBound {
                part: b.part,
                current: b.current,
            })
            .collect();
        (kept, bounds)
    }

    /// Whether `r` passes when the current of the part of `bound` is in `+-bound.current` and
    /// the drop across it in `+-bound.drop`, for each of the two that has no value. A drop
    /// that is the current through the greatest resistance (the wider of the two ranges that
    /// both hold the real drop) is tried too, for `.drop == .current * .resistance` asks that
    /// the drop lies where the current puts it. Each is a range the real values lie in, so
    /// either one that passes proves the relation.
    fn proven_with(&mut self, bound: &crate::ohm::Bound, r: &Relation) -> bool {
        [bound.drop, bound.drop_through]
            .into_iter()
            .any(|drop| self.passes_within(bound, drop, r))
    }

    fn passes_within(&mut self, bound: &crate::ohm::Bound, drop: f64, r: &Relation) -> bool {
        let model = &self.model;
        let part = bound.part;
        let Some(current) = model.through_of(part) else {
            return false;
        };
        let across = match model.member(part, "drop") {
            Some(Member::Field(f))
                if model.field(f).kind == FieldKind::Quantity(Dimension::VOLTAGE) =>
            {
                Some(f)
            }
            _ => None,
        };
        let span = |dim: Dimension, x: f64| {
            let lo = Quantity::point(dim, -x).ok()?;
            let hi = Quantity::point(dim, x).ok()?;
            Quantity::range(&lo, &hi).ok()
        };
        let mut set: Vec<FieldId> = Vec::new();
        if let Some(q) = span(Dimension::CURRENT, bound.current)
            && self.model.field(current).domain == Domain::Unset
        {
            self.model.set_domain(current, Domain::Quantity(q));
            set.push(current);
        }
        if let (Some(f), Some(q)) = (across, span(Dimension::VOLTAGE, drop))
            && self.model.field(f).domain == Domain::Unset
        {
            self.model.set_domain(f, Domain::Quantity(q));
            set.push(f);
        }
        let verdict = self.verify(r);
        for f in set {
            self.model.set_domain(f, Domain::Unset);
        }
        matches!(verdict, Verdict::Pass)
    }
}
