//! Two findings about a value nobody stated (ADR 0065, stage 1).
//!
//! A pick of a part whose value nothing limits (an *unconstrained pick*) and a value declared in
//! a module or board that a check reads and that nothing states or works out (an *unstated
//! value*). Both only report: they change no value, range or pick. They are made on the solved
//! model, because whether a pick is limited depends on what propagation settled and on whether
//! a bound could be judged, and the test that is true is the one made after the solve.
//!
//! # Why a finding, and not `required` (ADR 0065)
//!
//! `required` (ADR 0069, removed) was meant to stop a board from leaving a value to the
//! solver's first row. It was opt-in per member, so a plain `.r: Resistor` in a board was not
//! covered; it was a test on text, so `>= 0ohm` passed it; and it fired on members that did not
//! need to be stated. The solver knows, when it picks, whether anything limits the pick; when
//! nothing does it says so. The owner decided on 2026-11-06 to put a finding in the place of
//! `required` and to drop `required`; the rest of this is an agent's.
//!
//! # The unconstrained pick
//!
//! Reported for a part of a catalog with a value column that is walked (`value_columns` in
//! `table.rs`: `resistance`; `capacitance` and `effective_capacitance`; `inductance`: the three
//! E-series generics) and picked while nothing limited that value. Another catalog joins the list
//! when its use shows the same failure. It is **limited** when one of these holds, and the check
//! it names has been judged (passes or fails) once everything is picked and propagated:
//!
//! 1. a value column has a domain: a stated value, a range or a tolerance, a `?=` default that
//!    applied, a narrowing carried in from a member that has one, a `~` join to a field that has
//!    one. Any range counts, however wide: it is how a board says "any" on purpose
//!    (`.r.resistance &= 10kohm..100kohm`), and the finding cannot tell wide from narrow;
//! 2. a bound has a value column alone on one side, written by the design or made by the
//!    transfer (`transfer.rs`);
//! 3. a check that the pick decides: a relation that names a field worked out from a value
//!    column and is a bound, or an equation or `&=` whose left side has a stated range before
//!    the pick (`.led.current &= 5mA..15mA` with `.led.current == (V - Vf) / .r.resistance`);
//! 4. a budget: a stated current on a net where a term other than the stated one is worked out
//!    from the pick. A budget limits a pick only if the pick's value changes whether it holds
//!    (the traversal that finds what a pick decides carries a value across a balance only
//!    where `solve_balance_but` could set that term, so a pull-up's current no longer reaches
//!    a load's stated current through two balances).
//!
//! What does not limit a pick: a bound on a rating or the package, a relation written inside
//! the definition of any part (a diode's `current >= 0mA` as much as a resistor's own power
//! check, the library's and not the design's), and the catalog being filtered. A ratio is not a
//! size: `.out >= 1.5V` and `.out <= 1.8V` of a divider pass for 330 ohm and for 330 kohm. A
//! check limits the size of a pick only if scaling the unlimited picks together changes it.
//! `SizeBlind` scales the value fields of the unlimited picks by one factor and expands each
//! check through the equations that define what it reads (exactly one `==`, no join, no
//! balance), by degree of homogeneity (`+` and `-` need equal degrees, `*` adds, `/` subtracts,
//! a field no pick feeds has degree 0). A check whose two sides have one degree is blind to the
//! scaling; what it cannot expand it does not call blind, so the finding is missed and never
//! made wrongly. It is a fixed point: a pick that a check limits leaves the set. A pick limited
//! only by blind checks is reported and the message says so. So a board that uses
//! `FeedbackDivider` and states the output but nothing that depends on the divider's size gets
//! the warning, which says to state `.divider.current <= ...`; no example does, and the guide's
//! `Adjustable` block states it. A bound that waits for a value nothing sets (the open circle of
//! ADR 0057) judges nothing, and a pick that only such a bound reaches is reported unless an
//! open circle finding already names that check: one finding for one cause.
//!
//! A pick is judged on the solved model, not right after its group: a bound that waits for a
//! value another pick sets limits the pick it concerns. Parts held to one unstated member are
//! one finding in declaration order, with the lines of their links, and `held_by` names the
//! member the board can state. A warning by default and an error under `--strict` or
//! `guidelines = "error"`: the design may be right (a pull-up of any value is sometimes
//! right), a board in progress should build, and an error by default would refuse boards that
//! build today. It costs one scan of the relations per part with a value column and one
//! `verify` of a few relations after the group is solved. The gate when it was built: every
//! `examples/*.nsm` and every solving block of the guide builds with no new line (the guide's
//! test fails for a block that has one and is not marked `warns`); about 66 declarations in
//! tests would have warned, and none needed a value line.
//!
//! # The unstated value
//!
//! An enum, a text or a voltage that a board states for a check (a processor's high level on a
//! line) is not a pick. A relation that reads one that nothing states is not evaluated, and
//! ADR 0057 rejected a finding for every such relation. The finding names only the **root**: a
//! field that is declared in a module or board itself (not in a part, an interface or a column:
//! a part's unset datasheet value is the library's gap, ADR 0046, and an interface's optional
//! values are offered open on purpose, ADR 0058), has no value after the solve, is not set by
//! an equation, join, balance or column (a member that a wrapper forwards to is not a root: the
//! wrapper's member is, and the board can state that one), and is read by at least one check
//! that waits: a bound, or an equation whose left side is not a value still to be worked out. A
//! bound on a member that reached a part and was judged there is how the board states the
//! member. A member that no relation reads is not reported, so a module that only passes one
//! along costs nothing, and a text or enum that only offers a choice is not a relation that
//! fails to evaluate. `UnstatedValue` holds the fields that the same checks wait for, so one
//! finding names them together. A member that is both a root and feeds an unconstrained pick is
//! named once, by the pick finding. A module that no board uses is not checked alone. What is
//! lost against `required`: a module author can no longer force the board to state a value
//! that no check reads; the remedy is the check, which is what the value was for.
//!
//! # Not built
//!
//! The transfer through `~` joins and through arithmetic, and `==` between two bare fields as a
//! join ("a separate decision about `==`"): not now. A bound on a column other than the three
//! E-series generics' value columns: not planned until a board needs it. A parse error that
//! points at the removed word `required` is not kept (a message for a removed word is another
//! spelling to carry): `required` stays removed. The owner accepted these choices as built on
//! 2026-11-11.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use netsmith_check::{Domain, FieldId, InstId, InstKind, RExpr, Relation, SourceId};
use netsmith_lang::Span;
use netsmith_lang::ast::{BinOp, CmpOp};

use super::{Job, OpenCircle, Solver, Verdict};
use crate::eval::fields_in;
use crate::table::value_columns;

/// A part of a catalog with a value column that was picked while nothing limited that value:
/// the solver took the first row that fits.
#[derive(Debug, Clone)]
pub struct UnconstrainedPick {
    pub part: InstId,
    /// The value column named in the message: `resistance`.
    pub column: String,
    pub field: FieldId,
    /// The name of the row that was taken.
    pub row: String,
    /// The member of a module or board that the part's value is held to and that has no value,
    /// if there is one: the one a board can state, and what two parts that hang from one
    /// member are reported together by.
    pub held_by: Option<HeldBy>,
    /// The checks that limit the pick only as a ratio, where there are any: they hold whatever
    /// the size of the value is, as long as the picks they name are scaled together (a divider's
    /// output voltage fixes the ratio of its two resistors and not either of them). Where each
    /// is written. The pick is unconstrained in size all the same.
    pub ratio: Vec<(SourceId, Span)>,
    /// The part that keeps the `~>` chain this part is in from setting its current (ADR 0113),
    /// where there is one: the chain runs across a rail, and this part in it is not a
    /// `dc resistor` or a `dc diode` the way the chain runs.
    pub stopped_by: Option<InstId>,
}

/// The member a part's value is held to.
#[derive(Debug, Clone)]
pub struct HeldBy {
    pub member: FieldId,
    /// The statement that holds the part's field to the member (the link next to the part), if
    /// it is a statement and not a join: its operator and where it is.
    pub link: Option<(CmpOp, SourceId, Span)>,
}

/// Values declared in a module or a board that no statement gives or works out, and that
/// checks which were not evaluated read.
#[derive(Debug, Clone)]
pub struct UnstatedValue {
    /// The values, in field order. Those that the same checks read are one finding.
    pub fields: Vec<FieldId>,
    /// Indices into `model.relations()`: the checks that wait for them.
    pub checks: Vec<usize>,
}

/// Something that limits a pick, found by reading the design before the pick is made. It limits
/// only if it is judged (it passes or fails) once the model is solved.
#[derive(Debug, Clone)]
enum Limit {
    /// A relation, with the index of the design's own relation it stands for.
    Relation(Relation, Option<usize>),
    /// The balance of a net that has a stated current, by its place in the model's list (ADR
    /// 0038).
    Budget(usize),
}

/// A pick of a part with a value column that no statement of the design gave a value or a range
/// to, with what might still limit it.
pub(super) struct Survey {
    part: InstId,
    column: String,
    field: FieldId,
    limits: Vec<Limit>,
    held_by: Option<HeldBy>,
    /// The fields that hold the pick's value (the part's, the members it is held to, what `~`
    /// joins to them), and what is worked out from them.
    family: HashSet<FieldId>,
    reach: HashSet<FieldId>,
}

/// How fields are linked, built once for a solve.
pub(super) struct Links {
    /// Fields that hold one value: `~` joins, and a field held to another bare field by `&=` or
    /// `==`. With the relation that links them, if it is one.
    family: HashMap<FieldId, Vec<(FieldId, Option<usize>)>>,
    /// For each field, the fields that are worked out from it: what an equation's left side is
    /// worked out from the right side's fields, joined fields, and the other terms of a balance.
    worked_out: HashMap<FieldId, Vec<FieldId>>,
}

impl Links {
    /// The fields worked out from `field`, however directly.
    pub(super) fn worked_out(&self, field: FieldId) -> &[FieldId] {
        self.worked_out.get(&field).map_or(&[], Vec::as_slice)
    }
}

impl Solver<'_> {
    pub(super) fn links(&self) -> Links {
        let mut family: HashMap<FieldId, Vec<(FieldId, Option<usize>)>> = HashMap::new();
        let mut worked_out: HashMap<FieldId, Vec<FieldId>> = HashMap::new();
        for &(a, b) in self.model.equalities() {
            family.entry(a).or_default().push((b, None));
            family.entry(b).or_default().push((a, None));
            worked_out.entry(a).or_default().push(b);
            worked_out.entry(b).or_default().push(a);
        }
        for (i, r) in self.model.relations().iter().enumerate() {
            let (CmpOp::EqEq | CmpOp::Narrow, RExpr::Field(out)) = (&r.op, &r.lhs) else {
                continue;
            };
            let mut reads = Vec::new();
            fields_in(&r.rhs, &mut reads);
            for &g in &reads {
                worked_out.entry(g).or_default().push(*out);
            }
            if let RExpr::Field(g) = &r.rhs
                && g != out
            {
                family.entry(*out).or_default().push((*g, Some(i)));
                family.entry(*g).or_default().push((*out, Some(i)));
            }
        }
        // A balance carries a value to a term only where the solver could set that term from
        // the others (`solve_balance_but`): never to a stated current, which is a budget and is
        // judged and not set (ADR 0038), and never to a part's own current on a net of more than
        // two live terms or an incomplete balance (ADR 0026, ADR 0027). Otherwise a pick would
        // seem to decide the current of every load on the nets it is on.
        for b in self.model.balances().iter().filter(|b| !b.returns) {
            let live = b
                .draws
                .iter()
                .filter(|d| !self.model.draw_is_zero(d))
                .count();
            for d in &b.draws {
                for o in &b.draws {
                    if o.field == d.field || self.stated.contains_key(&o.field) {
                        continue;
                    }
                    if self.is_part_current(o) && (live > 2 || !b.is_complete()) {
                        continue;
                    }
                    worked_out.entry(d.field).or_default().push(o.field);
                }
            }
        }
        Links { family, worked_out }
    }

    /// Whether a relation was written inside the definition of a part (a datasheet's own
    /// requirement: `drop * current <= max_power`, a diode's `current >= 0mA`), as against one a
    /// module or board wrote. Library parts state what is always true of themselves; they are
    /// not what limits a value that the design chose.
    fn is_a_parts_own(&self, r: &Relation, parts: &[(SourceId, Span)]) -> bool {
        parts.iter().any(|&(source, span)| {
            r.source == source && r.span.start >= span.start && r.span.end <= span.end
        })
    }

    /// Survey the parts of `jobs` that have a value column, before any is picked: what they are
    /// held to and what might limit them. A part whose value (or a value it is held to) already
    /// has a range or a number is limited, and is not surveyed.
    pub(super) fn survey_picks(&self, jobs: &[Job]) -> Vec<Survey> {
        let candidates: Vec<(&Job, Vec<(FieldId, &str)>)> = jobs
            .iter()
            .filter_map(|job| {
                let wanted = value_columns(&job.catalog.name);
                let seeds: Vec<(FieldId, &str)> = job
                    .columns
                    .iter()
                    .filter(|(_, key)| wanted.contains(&key.as_str()))
                    .map(|(f, key)| (*f, key.as_str()))
                    .collect();
                (!seeds.is_empty()).then_some((job, seeds))
            })
            .collect();
        if candidates.is_empty() {
            return Vec::new();
        }
        let links = self.links();
        // The definitions of parts, by where their text is.
        let parts: Vec<(SourceId, Span)> = self
            .model
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Part)
            .map(|(_, i)| (i.source, i.span))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let relations = self.model.relations();
        // The relations that name each field, once for all the parts.
        let mut readers: HashMap<FieldId, Vec<usize>> = HashMap::new();
        for (i, r) in relations.iter().enumerate() {
            let mut fields = Vec::new();
            fields_in(&r.lhs, &mut fields);
            fields_in(&r.rhs, &mut fields);
            fields.sort_unstable();
            fields.dedup();
            for f in fields {
                readers.entry(f).or_default().push(i);
            }
        }
        let mut out = Vec::new();
        for (job, seeds) in candidates {
            // Everything that holds the same value: the part's field, the members it is held
            // to, and what `~` joins to them. Any of them with a range limits the pick.
            let mut family: HashMap<FieldId, Option<(FieldId, Option<usize>)>> = HashMap::new();
            let mut todo: VecDeque<FieldId> = VecDeque::new();
            for &(f, _) in &seeds {
                family.insert(f, None);
                todo.push_back(f);
            }
            while let Some(f) = todo.pop_front() {
                for &(g, via) in links.family.get(&f).into_iter().flatten() {
                    if let std::collections::hash_map::Entry::Vacant(e) = family.entry(g) {
                        e.insert(Some((f, via)));
                        todo.push_back(g);
                    }
                }
            }
            if family
                .keys()
                .any(|&f| !matches!(self.domain(f), Domain::Unset))
            {
                continue;
            }
            // What the pick decides: the values worked out from the family's.
            let mut reach: HashSet<FieldId> = family.keys().copied().collect();
            let mut todo: Vec<FieldId> = reach.iter().copied().collect();
            while let Some(f) = todo.pop() {
                for &g in links.worked_out.get(&f).into_iter().flatten() {
                    if reach.insert(g) {
                        todo.push(g);
                    }
                }
            }
            let mut limits: Vec<Limit> = Vec::new();
            // The relations that name a field of the pick's reach, in the model's order.
            let mut named: Vec<usize> = reach
                .iter()
                .filter_map(|f| readers.get(f))
                .flatten()
                .copied()
                .collect();
            named.sort_unstable();
            named.dedup();
            for i in named {
                let r = &relations[i];
                if self.is_a_parts_own(r, &parts) {
                    continue;
                }
                let is_bound = matches!(r.op, CmpOp::Ge | CmpOp::Le);
                // An equation or narrowing whose left side has a range of its own is a check on
                // what the pick works out to (`.led.current &= 5mA..15mA` with
                // `.led.current == (V - Vf) / .r.resistance`).
                let is_check = matches!(r.op, CmpOp::EqEq | CmpOp::Narrow)
                    && matches!(&r.lhs, RExpr::Field(f)
                        if matches!(self.domain(*f), Domain::Quantity(_)) && !self.derived.contains(f));
                if is_bound || is_check {
                    limits.push(Limit::Relation(r.clone(), self.original_of(i)));
                }
            }
            for (i, b) in self.model.balances().iter().enumerate() {
                // A stated current on the net limits the pick only if the pick's value changes
                // whether the budget holds: some other term of the net is worked out from it.
                // A load that the pick does not feed is compared with the net's other terms,
                // which the pick does not change.
                if !b.returns
                    && b.draws.iter().any(|d| self.stated.contains_key(&d.field))
                    && b.draws
                        .iter()
                        .any(|d| reach.contains(&d.field) && !self.stated.contains_key(&d.field))
                {
                    limits.push(Limit::Budget(i));
                }
            }
            // The member that the value is held to: the outermost member of a module or board in
            // the family that has no value, which is the one a board can state.
            let member = family
                .keys()
                .copied()
                .filter(|&f| {
                    matches!(
                        self.model.instance(self.model.field(f).owner).kind,
                        InstKind::Module | InstKind::Board
                    )
                })
                .min_by_key(|&f| {
                    let path = self.model.path_of_field(f);
                    (path.matches('.').count(), path)
                });
            let held_by = member.map(|member| {
                // The link next to the part: walk back from the member to the seed.
                let mut link = None;
                let mut at = member;
                while let Some(Some((from, via))) = family.get(&at) {
                    link = via.map(|i| (relations[i].op, relations[i].source, relations[i].span));
                    at = *from;
                }
                HeldBy { member, link }
            });
            let (field, column) = seeds[0];
            out.push(Survey {
                part: job.part,
                column: column.to_string(),
                field,
                limits,
                held_by,
                family: family.keys().copied().collect(),
                reach,
            });
        }
        out
    }

    /// The design's own relation that the relation at `index` stands for: itself, or for a
    /// bound that the solver added, the bound it was made from.
    fn original_of(&self, index: usize) -> Option<usize> {
        if index < self.original_relations {
            Some(index)
        } else {
            self.extras
                .get(index - self.original_relations)
                .map(|(_, t)| t.bound())
        }
    }

    /// The surveyed picks that nothing limited: none of what might limit them was judged once
    /// the model was solved, or only checks that hold whatever the size of the values is. A
    /// pick whose only limits wait for a value that nothing sets is left to the open circle that
    /// names those checks (one finding for one cause).
    pub(super) fn unconstrained_picks(
        &self,
        surveys: &[Survey],
        circles: &[OpenCircle],
    ) -> Vec<UnconstrainedPick> {
        let balances = self.model.balances();
        let judged = |l: &Limit| match l {
            Limit::Relation(r, _) => !matches!(self.verify(r), Verdict::Unknown),
            Limit::Budget(i) => self.budget(&balances[*i]).is_some(),
        };
        // A check an open circle names, or a budget on a net it runs through or a value of it:
        // the circle's finding says what waits.
        let named_by_a_circle = |l: &Limit| match l {
            Limit::Relation(_, Some(i)) => circles.iter().any(|c| c.checks.contains(i)),
            Limit::Relation(_, None) => false,
            Limit::Budget(i) => {
                let b = &balances[*i];
                circles.iter().any(|c| {
                    c.nets.iter().any(|n| n.net == b.net && n.scope == b.scope)
                        || b.draws.iter().any(|d| c.fields.contains(&d.field))
                })
            }
        };
        let picked: Vec<&Survey> = surveys
            .iter()
            .filter(|s| self.picks.iter().any(|p| p.part == s.part))
            .collect();
        // The picks whose size nothing limits. A check that holds whatever the size is, when
        // the picks that it reads are all scaled together (the output of a divider of two
        // unstated resistors), limits the ratio and not the size (`SizeBlind`). A pick that a
        // check limits in size leaves the set, and a check that read it is then no longer
        // blind to the picks that remain: they are tied to a value that is not free.
        let definitions = SizeBlind::new(self);
        let mut free: Vec<bool> = vec![true; picked.len()];
        loop {
            let scaled: HashSet<FieldId> = picked
                .iter()
                .zip(&free)
                .filter(|(_, f)| **f)
                .flat_map(|(s, _)| s.family.iter().copied())
                .collect();
            let reach: HashSet<FieldId> = picked
                .iter()
                .zip(&free)
                .filter(|(_, f)| **f)
                .flat_map(|(s, _)| s.reach.iter().copied())
                .collect();
            let mut changed = false;
            for (i, s) in picked.iter().enumerate() {
                if !free[i] {
                    continue;
                }
                let limited = s.limits.iter().any(|l| {
                    judged(l)
                        && match l {
                            Limit::Relation(r, _) => !definitions.blind(self, r, &scaled, &reach),
                            Limit::Budget(_) => true,
                        }
                });
                if limited {
                    free[i] = false;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let scaled: HashSet<FieldId> = picked
            .iter()
            .zip(&free)
            .filter(|(_, f)| **f)
            .flat_map(|(s, _)| s.family.iter().copied())
            .collect();
        let reach: HashSet<FieldId> = picked
            .iter()
            .zip(&free)
            .filter(|(_, f)| **f)
            .flat_map(|(s, _)| s.reach.iter().copied())
            .collect();
        let mut out = Vec::new();
        for (s, free) in picked.iter().zip(free) {
            if !free {
                continue;
            }
            let Some(pick) = self.picks.iter().find(|p| p.part == s.part) else {
                continue;
            };
            let ratio: Vec<(SourceId, Span)> = s
                .limits
                .iter()
                .filter_map(|l| match l {
                    Limit::Relation(r, _)
                        if judged(l) && definitions.blind(self, r, &scaled, &reach) =>
                    {
                        Some((r.source, r.span))
                    }
                    _ => None,
                })
                .collect();
            if ratio.is_empty() && !s.limits.is_empty() && s.limits.iter().all(named_by_a_circle) {
                continue;
            }
            out.push(UnconstrainedPick {
                part: s.part,
                column: s.column.clone(),
                field: s.field,
                row: pick.candidate.clone(),
                held_by: s.held_by.clone(),
                ratio,
                stopped_by: self
                    .model
                    .chains()
                    .iter()
                    .filter(|c| c.parts.contains(&s.part))
                    .find_map(|c| c.stopped_by),
            });
        }
        out
    }

    /// The values declared in a module or board that a check which was not evaluated waits for
    /// and that nothing states or works out (ADR 0065, stage 1b). `already` are the members
    /// that an unconstrained pick names, which are named once, there.
    pub(super) fn unstated_values(
        &self,
        deferred: &[usize],
        circles: &[OpenCircle],
        already: &HashSet<FieldId>,
    ) -> Vec<UnstatedValue> {
        let relations = self.model.relations();
        // What works a field out: the equations and narrowings that have it alone on the left.
        let mut defined: HashMap<FieldId, Vec<usize>> = HashMap::new();
        for (i, r) in relations.iter().enumerate() {
            if let (CmpOp::EqEq | CmpOp::Narrow, RExpr::Field(f)) = (&r.op, &r.lhs) {
                defined.entry(*f).or_default().push(i);
            }
        }
        let joined: HashSet<FieldId> = self
            .model
            .equalities()
            .iter()
            .flat_map(|&(a, b)| [a, b])
            .collect();
        let in_a_balance: HashSet<FieldId> = self
            .model
            .balances()
            .iter()
            .flat_map(|b| b.draws.iter().map(|d| d.field))
            .collect();
        // A bound on a member that reached a part was judged there: it is how the design states the
        // member, not a check that waits for it.
        let through_parts = self.bounds_judged_on_parts();
        let waiting: HashSet<FieldId> = circles
            .iter()
            .flat_map(|c| c.waiting.iter().copied())
            .collect();
        let mut found: Vec<(BTreeSet<FieldId>, usize)> = Vec::new();
        for &i in deferred {
            let r = &relations[i];
            // A check: a bound, or an equation whose left side is not a value still to be worked
            // out. A definition that waits (`.x == .y * 2`, `.part.field &= .member`) is not.
            let is_check = match r.op {
                CmpOp::Ge | CmpOp::Le => !through_parts.contains(&i),
                CmpOp::EqEq => !matches!(&r.lhs, RExpr::Field(f)
                    if matches!(self.domain(*f), Domain::Unset)),
                _ => false,
            };
            if !is_check {
                continue;
            }
            let mut roots: BTreeSet<FieldId> = BTreeSet::new();
            let mut seen: HashSet<FieldId> = HashSet::new();
            let mut todo: Vec<FieldId> = Vec::new();
            fields_in(&r.lhs, &mut todo);
            fields_in(&r.rhs, &mut todo);
            while let Some(f) = todo.pop() {
                if !seen.insert(f) || !matches!(self.domain(f), Domain::Unset) {
                    continue;
                }
                if let Some(defs) = defined.get(&f) {
                    for &d in defs {
                        fields_in(&relations[d].rhs, &mut todo);
                    }
                    continue;
                }
                let owner = self.model.instance(self.model.field(f).owner);
                if matches!(owner.kind, InstKind::Module | InstKind::Board)
                    && !joined.contains(&f)
                    && !in_a_balance.contains(&f)
                    && !already.contains(&f)
                    && !waiting.contains(&f)
                {
                    roots.insert(f);
                }
            }
            if !roots.is_empty() {
                found.push((roots, i));
            }
        }
        // The values that some check reads together are one finding.
        let mut groups: Vec<(BTreeSet<FieldId>, BTreeSet<usize>)> = Vec::new();
        for (roots, check) in found {
            let (mut fields, mut checks) = (roots, BTreeSet::from([check]));
            let mut i = 0;
            while i < groups.len() {
                if groups[i].0.is_disjoint(&fields) {
                    i += 1;
                } else {
                    let (f, c) = groups.remove(i);
                    fields.extend(f);
                    checks.extend(c);
                }
            }
            groups.push((fields, checks));
        }
        groups.sort_by_key(|(fields, _)| fields.iter().next().copied());
        groups
            .into_iter()
            .map(|(fields, checks)| UnstatedValue {
                fields: fields.into_iter().collect(),
                checks: checks.into_iter().collect(),
            })
            .collect()
    }
}

/// Whether a check holds whatever the size of the unconstrained picks is (ADR 0065, "A ratio is
/// not a size"). The picks scale together by one factor, and the check is expanded through the
/// equations that define the values it reads: if both of its sides are homogeneous of the same
/// degree, the check is unchanged by the scaling, so it says nothing about the size. What it
/// cannot expand (a value worked out by a balance, by a join, by two equations) it cannot call
/// blind, which leaves the pick limited: the finding is missed and never made wrongly.
struct SizeBlind {
    /// The equation that defines each field, when it has exactly one `==` with it alone on the
    /// left: the index into the model's relations.
    defined: HashMap<FieldId, usize>,
    /// Fields that a join or a balance also gives a value to.
    otherwise: HashSet<FieldId>,
}

impl SizeBlind {
    fn new(solver: &Solver<'_>) -> Self {
        let mut count: HashMap<FieldId, (usize, usize)> = HashMap::new();
        for (i, r) in solver.model.relations().iter().enumerate() {
            if let (CmpOp::EqEq, RExpr::Field(f)) = (&r.op, &r.lhs) {
                let e = count.entry(*f).or_insert((0, i));
                e.0 += 1;
            }
        }
        let defined = count
            .into_iter()
            .filter(|(_, (n, _))| *n == 1)
            .map(|(f, (_, i))| (f, i))
            .collect();
        let mut otherwise: HashSet<FieldId> = solver
            .model
            .equalities()
            .iter()
            .flat_map(|&(a, b)| [a, b])
            .collect();
        otherwise.extend(
            solver
                .model
                .balances()
                .iter()
                .flat_map(|b| b.draws.iter().map(|d| d.field)),
        );
        Self { defined, otherwise }
    }

    /// The degree of homogeneity of `e` when every field of `scaled` is multiplied by one
    /// factor: `None` if it is not homogeneous or cannot be told.
    fn degree(
        &self,
        solver: &Solver<'_>,
        e: &RExpr,
        scaled: &HashSet<FieldId>,
        reach: &HashSet<FieldId>,
        open: &mut Vec<FieldId>,
    ) -> Option<i32> {
        match e {
            RExpr::Const(_) => Some(0),
            RExpr::Text(_) => None,
            // A range with a value at an end (ADR 0066), or the end of a field's range (ADR 0098), is not told apart: never homogeneous here.
            RExpr::Range(..) | RExpr::End(..) => None,
            RExpr::Neg(inner) => self.degree(solver, inner, scaled, reach, open),
            RExpr::Bin(op, a, b) => {
                let da = self.degree(solver, a, scaled, reach, open)?;
                let db = self.degree(solver, b, scaled, reach, open)?;
                match op {
                    BinOp::Add | BinOp::Sub => (da == db).then_some(da),
                    BinOp::Mul => Some(da + db),
                    BinOp::Div => Some(da - db),
                }
            }
            RExpr::Field(f) => {
                if scaled.contains(f) {
                    return Some(1);
                }
                // Not worked out from any of the picks: it does not change with them.
                if !reach.contains(f) {
                    return Some(0);
                }
                if open.contains(f) || self.otherwise.contains(f) {
                    return None;
                }
                let i = *self.defined.get(f)?;
                open.push(*f);
                let degree = self.degree(
                    solver,
                    &solver.model.relations()[i].rhs,
                    scaled,
                    reach,
                    open,
                );
                open.pop();
                degree
            }
        }
    }

    /// Whether the check `r` is the same for every scaling of the picks.
    fn blind(
        &self,
        solver: &Solver<'_>,
        r: &Relation,
        scaled: &HashSet<FieldId>,
        reach: &HashSet<FieldId>,
    ) -> bool {
        let mut open = Vec::new();
        match (&r.op, &r.lhs) {
            // A check that an equation or narrowing makes against a range its left side has of
            // its own: that range does not scale.
            (CmpOp::EqEq | CmpOp::Narrow, RExpr::Field(_)) => {
                self.degree(solver, &r.rhs, scaled, reach, &mut open) == Some(0)
            }
            _ => {
                let l = self.degree(solver, &r.lhs, scaled, reach, &mut open);
                let rr = self.degree(solver, &r.rhs, scaled, reach, &mut open);
                l.is_some() && l == rr
            }
        }
    }
}
