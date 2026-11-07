//! Two findings about a value nobody stated (ADR 0065, stage 1).
//!
//! A pick of a part whose value nothing limits (an *unconstrained pick*) and a value declared in
//! a module or board that a check reads and that nothing states or works out (an *unstated
//! value*). Both only report: they change no value, range or pick. They are made on the solved
//! model, because whether a pick is limited depends on what propagation settled and on whether
//! a bound could be judged, and the test that is true is the one made after the solve.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use netsmith_check::{Domain, FieldId, InstId, InstKind, RExpr, Relation, SourceId};
use netsmith_lang::Span;
use netsmith_lang::ast::CmpOp;

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
        for b in self.model.balances().iter().filter(|b| !b.returns) {
            for d in &b.draws {
                for o in &b.draws {
                    if o.field != d.field {
                        worked_out.entry(d.field).or_default().push(o.field);
                    }
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
                if !b.returns
                    && b.draws.iter().any(|d| reach.contains(&d.field))
                    && b.draws.iter().any(|d| self.stated.contains_key(&d.field))
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
    /// the model was solved. A pick whose only limits wait for a value that nothing sets is
    /// left to the open circle that names those checks (one finding for one cause).
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
        let mut out = Vec::new();
        for s in surveys {
            let Some(pick) = self.picks.iter().find(|p| p.part == s.part) else {
                continue;
            };
            if s.limits.iter().any(judged) {
                continue;
            }
            if !s.limits.is_empty() && s.limits.iter().all(named_by_a_circle) {
                continue;
            }
            out.push(UnconstrainedPick {
                part: s.part,
                column: s.column.clone(),
                field: s.field,
                row: pick.candidate.clone(),
                held_by: s.held_by.clone(),
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
