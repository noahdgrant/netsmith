//! Choosing real parts and checking that the design works in the worst case.
//!
//! The solver does four things, in order:
//!
//! 1. **Propagate.** Connected interfaces hold one value, so equal fields are narrowed
//!    to what they all allow. A field defined by an equation whose inputs are known takes
//!    that value. At each net the currents of the rails, and of the chains of parts that
//!    start there, add up to zero, so when only one of them is unknown it is the negative
//!    of the rest.
//! 2. **Pick.** For each part with a catalog and an unset catalog column, take candidates
//!    in catalog order, skip those that do not fit what the design already says, and use
//!    the first one for which every relation that mentions the part holds.
//! 3. **Verify.** Check every relation again with the picks in place.
//! 4. **Report.** Whatever could not be evaluated is deferred, and whatever fails is
//!    reported with the numbers.
//!
//! A relation holds in the worst case: a requirement on an outcome must contain the whole
//! range the inputs can produce, not just a typical value. Parts that share a relation are
//! picked together by a depth-first search over their candidates (parts in path order,
//! candidates in catalog order): a choice that makes a later part impossible is undone and
//! the next candidate is tried, up to a fixed number of trials. A failure remembers which
//! earlier parts it depends on, and the search goes back to the nearest of them rather than
//! choosing the parts in between again (conflict-directed backjumping).

use std::collections::{BTreeSet, HashMap, HashSet};

use netsmith_check::{
    Balance, Board, Diagnostic, Domain, Draw, FieldId, FieldKind, InstId, InstKind, Model, RExpr,
    Relation, SourceId, System,
};
use netsmith_lang::Span;
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::{Dimension, Quantity};

use crate::catalog::{Candidate, Catalog, Catalogs, Derating, DeratingError, Library, Value};
use crate::generic::{Need, Sense};
use crate::eval::{EvalError, Val, eval, fields_in};

/// The result of solving one board.
#[derive(Debug)]
pub struct Solution {
    /// The board's model with every value the solver could settle filled in.
    pub model: Model,
    pub picks: Vec<Pick>,
    /// Parts with a catalog that were not picked, and why.
    pub unresolved: Vec<Unresolved>,
    /// Indices into `model.relations()` that could not be evaluated: something they
    /// depend on has no value.
    pub deferred: Vec<usize>,
    pub diagnostics: Vec<Diagnostic>,
    /// The project's default for who assembles a part (`fab`, `hand` or `none`), from the
    /// catalogs the board was solved with (ADR 0028). Lowering applies it.
    pub default_assembly: Option<String>,
    /// The logic level and absolute maximum checks that could not be made, and why
    /// (ADR 0046): never a pass.
    pub not_evaluated: Vec<crate::levels::NotEvaluated>,
    /// How many candidates the search tried in all, over every group of parts: the effort
    /// spent choosing parts. Tests use it to show that a failing design is not searched
    /// once per combination of parts that have nothing to do with the failure.
    pub trials: usize,
}

#[derive(Debug, Clone)]
pub struct Pick {
    pub part: InstId,
    pub catalog: String,
    pub candidate: String,
    /// The value for a schematic: `300`.
    pub label: String,
    /// The supplier's and manufacturer's part numbers and JLCPCB's library, if the
    /// candidate is a real part.
    pub lcsc: Option<String>,
    pub mpn: Option<String>,
    pub library: Option<Library>,
    /// The row's default for who assembles the part (ADR 0024).
    pub assembly: Option<String>,
    /// Set if the candidate is a generic part (ADR 0047): it has no part number yet, and
    /// [`crate::resolve`] finds the row that stands for it.
    pub generic: Option<crate::Generic>,
    /// How many candidates fit the design's constraints but failed a relation first.
    pub rejected: usize,
}

#[derive(Debug, Clone)]
pub struct Unresolved {
    pub part: InstId,
    pub reason: String,
}

enum Verdict {
    Pass,
    Unknown,
    Fail { message: String, margin: f64 },
}

/// Why a candidate's effective value is unknown.
enum Underated {
    /// The design gave the part no bias voltage (the path of the field).
    NoBias(String),
    Curve(DeratingError),
}

/// The most the search will try before it stops and says so.
const MAX_TRIALS: usize = 250_000;

/// One part that needs a real part chosen for it.
struct Job {
    part: InstId,
    path: String,
    columns: Vec<(FieldId, String)>,
    /// For each column, whether a relation or connection uses its field. A column nothing
    /// uses cannot change what any other part or relation sees.
    used: Vec<bool>,
    catalog: Catalog,
}

/// `a` or `an` for a type name in a message. A name like `Mcu32` is read letter by letter,
/// so it takes `an`.
fn article(word: &str) -> &'static str {
    let mut chars = word.chars();
    let spelled = matches!(
        chars.next(),
        Some('F' | 'H' | 'L' | 'M' | 'N' | 'R' | 'S' | 'X')
    ) && chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() && !"aeiou".contains(c));
    if word.starts_with(['A', 'E', 'I', 'O', 'U']) || spelled {
        "an"
    } else {
        "a"
    }
}

/// What a search has learned so far.
#[derive(Default)]
struct Search {
    trials: usize,
    gave_up: bool,
    /// How many candidates fit the first part's constraints, for the message.
    first_level_fits: usize,
    /// The part, if any, that had nothing fitting what the design already says.
    no_fit: Option<usize>,
    /// The choices currently made, as `path = candidate`.
    chosen: Vec<String>,
    picks: Vec<Pick>,
    closest: Option<Closest>,
}

/// The choice that came nearest to working, kept for the error message: the one that got
/// furthest through the parts, and among those the one that missed by the least.
struct Closest {
    /// How many parts had been chosen before the one that failed.
    depth: usize,
    margin: f64,
    assignment: Vec<String>,
    message: String,
    at: (SourceId, Span),
}

struct Conflict {
    source: SourceId,
    span: Span,
    message: String,
    /// The fields whose values the conflict was found in.
    fields: Vec<FieldId>,
}

struct Rejection {
    candidate: String,
    message: String,
    margin: f64,
    at: (SourceId, Span),
    /// The fields whose values the rejection and its message were worked out from.
    reads: Vec<FieldId>,
}

/// How a search of a group's parts ended.
enum Outcome {
    /// Every part has a candidate; they are in the search's picks.
    Solved,
    /// No candidate for the part at some depth works with the choices before it. The set
    /// holds the depths of the earlier parts whose choices that failure depends on (see
    /// [`Coupling`]); a failure that does not depend on the part just above is not undone
    /// by changing that part.
    Failed(BTreeSet<usize>),
    /// The search stopped at the trial limit.
    GaveUp,
}

/// Which parts can see each other's choices. A part's choice sets its columns; propagation
/// then carries the values along the connections, the equations and the balances of
/// currents, and nowhere else. The fields those join form a class, and a value in a class
/// depends only on the choices of parts whose columns are in it. A check that reads some
/// fields (a relation, a conflict, the domains that decide which candidates fit) is
/// therefore settled by the parts that share a class with them, and by no others.
struct Coupling {
    /// For each job, the classes of its columns.
    columns: Vec<HashSet<usize>>,
    /// For each job, the classes of every field inside its part.
    inside: Vec<HashSet<usize>>,
    /// The class of each field.
    class: Vec<usize>,
}

impl Coupling {
    /// The depths below `depth` of the parts whose columns share a class with `reads`.
    fn earlier(&self, jobs_at: &[usize], depth: usize, reads: &HashSet<usize>) -> BTreeSet<usize> {
        (0..depth)
            .filter(|&p| !self.columns[jobs_at[p]].is_disjoint(reads))
            .collect()
    }

    fn classes_of(&self, fields: &[FieldId]) -> HashSet<usize> {
        fields.iter().map(|f| self.class[f.0]).collect()
    }
}

fn find(parent: &mut [usize], x: usize) -> usize {
    if parent[x] != x {
        let root = find(parent, parent[x]);
        parent[x] = root;
    }
    parent[x]
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[ra] = rb;
    }
}

pub fn solve(board: &Board, catalogs: &Catalogs) -> Solution {
    solve_model(&board.model, catalogs)
}

/// Solve every board of a system together. Propagation already carries values across a
/// join (the equated fields, the balance of currents at the joined ports), and picking
/// reads whatever propagation settled, so one pass over the whole model reaches the fixed
/// point the boards need and no board is solved before the ports it depends on are known.
/// The solution's model is the system's; read one board through
/// [`Model::scoped_to`].
pub fn solve_system(system: &System, catalogs: &Catalogs) -> Solution {
    solve_model(&system.model, catalogs)
}

fn solve_model(model: &Model, catalogs: &Catalogs) -> Solution {
    let mut s = Solver {
        model: model.clone(),
        catalogs,
        diagnostics: Vec::new(),
        seen: HashSet::new(),
        picks: Vec::new(),
        unresolved: Vec::new(),
        derived: HashSet::new(),
        stated: stated_currents(model),
        sole_equation: sole_equations(model),
        trials: 0,
    };
    for c in s.propagate() {
        s.report(c.source, c.span, c.message);
    }
    s.pick_parts();
    for c in s.propagate() {
        s.report(c.source, c.span, c.message);
    }
    s.check_windows();

    let mut deferred = Vec::new();
    for (i, r) in s.model.relations().to_vec().iter().enumerate() {
        match s.verify(r) {
            Verdict::Pass => {}
            Verdict::Unknown => deferred.push(i),
            Verdict::Fail { message, .. } => {
                let message = s.with_currents(r, message);
                s.report(r.source, r.span, message)
            }
        }
    }
    s.verify_balances();
    for d in netsmith_check::pin_faults_solved(model, &s.model) {
        s.report(d.source, d.span, d.message);
    }
    let (level_faults, not_evaluated) = crate::levels::check(&s.model);
    for d in level_faults {
        s.report(d.source, d.span, d.message);
    }
    Solution {
        model: s.model,
        picks: s.picks,
        unresolved: s.unresolved,
        deferred,
        diagnostics: s.diagnostics,
        default_assembly: catalogs.default_assembly().map(str::to_string),
        not_evaluated,
        trials: s.trials,
    }
}

/// A stated current and what the rest of its net determines it to be (ADR 0038). Both are
/// in the balance's terms: negated when the current is flipped.
struct Budget {
    field: FieldId,
    index: usize,
    flipped: bool,
    stated: Quantity,
    works_out: Quantity,
}

impl Budget {
    /// What the current is, as the field holds it.
    fn field_value(&self) -> Quantity {
        if self.flipped {
            self.works_out.neg()
        } else {
            self.works_out
        }
    }
}

struct Solver<'a> {
    model: Model,
    catalogs: &'a Catalogs,
    diagnostics: Vec<Diagnostic>,
    seen: HashSet<Diagnostic>,
    picks: Vec<Pick>,
    unresolved: Vec<Unresolved>,
    /// Currents that propagation worked out from a balance (they were not stated). Such a
    /// current is worked out again when the terms it came from narrow, so it never goes
    /// stale: a value the design stated is not touched.
    derived: HashSet<FieldId>,
    /// The currents in a balance that the design stated with a range and that no equation
    /// determines, with the range as written (ADR 0038): each is a budget, and what the
    /// circuit determines must lie inside it.
    stated: HashMap<FieldId, Quantity>,
    /// Per relation: whether it is an equation on a field that no other equation states
    /// (through the field or any field joined to it). Only such an equation narrows a field
    /// that has a range; see `propagate`.
    sole_equation: Vec<bool>,
    /// Candidates tried so far, over every group.
    trials: usize,
}

/// The currents in a balance that the design gave a range (`.x.current &= a..b`) and that
/// no `==` states: their range is a budget (ADR 0038). A current an equation determines is
/// not stated, whatever range it was also written with; the equation checks that.
fn stated_currents(model: &Model) -> HashMap<FieldId, Quantity> {
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
        .filter_map(|f| match &model.field(f).domain {
            Domain::Quantity(q) => Some((f, *q)),
            _ => None,
        })
        .collect()
}

/// For each relation, whether it is the only `==` whose left side is its field or a field
/// that holds the same value (`~` joins interfaces and so their fields, ADR 0007).
fn sole_equations(model: &Model) -> Vec<bool> {
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
    let mut count = vec![0_usize; parent.len()];
    let sides: Vec<Option<usize>> = model
        .relations()
        .iter()
        .map(|r| match (&r.op, &r.lhs) {
            (CmpOp::EqEq, RExpr::Field(f)) => Some(find(&mut parent, f.0)),
            _ => None,
        })
        .collect();
    for root in sides.iter().flatten() {
        count[*root] += 1;
    }
    sides
        .iter()
        .map(|root| root.is_some_and(|r| count[r] == 1))
        .collect()
}

fn merge(a: &Domain, b: &Domain) -> Option<Domain> {
    match (a, b) {
        (Domain::Unset, x) | (x, Domain::Unset) => Some(x.clone()),
        (Domain::Quantity(x), Domain::Quantity(y)) => {
            x.intersect(y).ok().flatten().map(Domain::Quantity)
        }
        (Domain::Text(x), Domain::Text(y)) => (x == y).then(|| a.clone()),
        _ => None,
    }
}

fn domain_of(v: Val) -> Domain {
    match v {
        Val::Q(q) => Domain::Quantity(q),
        Val::T(t) => Domain::Text(t),
    }
}

/// How far `v` sticks out of `allowed`, as a fraction of `allowed`.
fn overshoot(allowed: &Quantity, v: &Quantity) -> f64 {
    let out = (allowed.lo() - v.lo()).max(v.hi() - allowed.hi()).max(0.0);
    out / allowed.lo().abs().max(allowed.hi().abs()).max(1e-30)
}

impl Solver<'_> {
    fn report(&mut self, source: SourceId, span: Span, message: String) {
        let d = Diagnostic {
            source,
            span,
            message,
        };
        if self.seen.insert(d.clone()) {
            self.diagnostics.push(d);
        }
    }

    fn domain(&self, f: FieldId) -> Domain {
        self.model.field(f).domain.clone()
    }

    fn propagate(&mut self) -> Vec<Conflict> {
        let mut conflicts: Vec<Conflict> = Vec::new();
        let mut said: HashSet<String> = HashSet::new();
        // The structure does not change while values narrow, so it is copied once.
        let equalities = self.model.equalities().to_vec();
        let relations = self.model.relations().to_vec();
        let balances = self.model.balances().to_vec();
        for _ in 0..64 {
            let mut changed = false;

            for &(a, b) in &equalities {
                let (da, db) = (self.domain(a), self.domain(b));
                match merge(&da, &db) {
                    Some(m) => {
                        if m != da {
                            self.model.set_domain(a, m.clone());
                            changed = true;
                        }
                        if m != db {
                            self.model.set_domain(b, m);
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
                            "connected values disagree: `{}` is {da} but `{}` is {db}",
                            self.model.path_of_field(a),
                            self.model.path_of_field(b)
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
                if !matches!(r.op, CmpOp::EqEq | CmpOp::Narrow) {
                    continue;
                }
                let RExpr::Field(f) = &r.lhs else { continue };
                let Ok(v) = eval(&self.model, &r.rhs) else {
                    continue;
                };
                let value = domain_of(v);
                let current = self.domain(*f);
                match (r.op, &current) {
                    (_, Domain::Unset) => {
                        self.model.set_domain(*f, value);
                        changed = true;
                    }
                    (CmpOp::Narrow, _) => match merge(&current, &value) {
                        Some(m) => {
                            if m != current {
                                self.model.set_domain(*f, m);
                                changed = true;
                            }
                        }
                        None => {
                            let message = format!(
                                "contradiction on `{}`: it is already {current}, which does not overlap {value}",
                                self.model.path_of_field(*f)
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
                            self.model.set_domain(*f, Domain::Quantity(both));
                            changed = true;
                        }
                    }
                    (CmpOp::EqEq, Domain::Quantity(d)) if self.sole_equation[index] => {
                        if let Domain::Quantity(v) = &value
                            && v != d
                            && d.encloses(v).unwrap_or(false)
                        {
                            self.model.set_domain(*f, value);
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
                for b in &balances {
                    let is_join = self.model.instance(b.scope).kind == InstKind::System;
                    if is_join != joins || b.returns {
                        continue;
                    }
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
                                    self.model.set_domain(field, Domain::Quantity(both));
                                    changed = true;
                                    continue;
                                }
                                None => continue,
                            },
                            _ => continue,
                        }
                        self.model.set_domain(field, Domain::Quantity(value));
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
                            self.model.set_domain(field, Domain::Quantity(both));
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
    fn contribution(&self, d: &Draw) -> Option<Quantity> {
        match &self.model.field(d.field).domain {
            Domain::Quantity(q) => Some(if d.flipped { q.neg() } else { *q }),
            _ => None,
        }
    }

    /// Whether a term is a part's through-current (the first or last part of a chain, ADR
    /// 0014) and not the current of a rail or a port.
    fn is_part_current(&self, d: &Draw) -> bool {
        let owner = self.model.field(d.field).owner;
        self.model.instance(owner).kind == InstKind::Part
    }

    /// If exactly one current on the net is unknown, its value: whatever makes the rest add up
    /// to zero. The current of a rail or a port is set this way; a part's through-current only
    /// from a complete balance (ADR 0027) of two terms (ADR 0026). From a longer balance it is
    /// verified and never set: the residual of several wide ranges says little about one part,
    /// and its own relations (Ohm's law, the LED equation, a stated value) are what determine it.
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
        // An incomplete balance (ADR 0027) has pins whose currents are not terms. What the
        // terms leave is theirs as much as a part's, so no part is set from it; a rail is, as
        // before (an enable pin on a supply net does not stop the supply being worked out),
        // but not from nothing: a lone port is not taken to be zero when something unmodelled
        // is on the net.
        if !b.is_complete() && (self.is_part_current(u) || rest.is_none()) {
            return None;
        }
        // A balance of two terms equates them (a fuse in series with a load carries the
        // load's current); a longer one subtracts, and what it leaves is not a part's.
        if b.draws.len() > 2 && self.is_part_current(u) {
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
    fn budget(&self, b: &Balance) -> Option<Budget> {
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
            .filter(|(_, d)| self.stated.contains_key(&d.field));
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
        let range = self.stated[&d.field];
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
    fn verify_balances(&mut self) {
        let mut balances = self.model.balances().to_vec();
        balances.sort_by_key(|b| b.returns);
        for b in balances {
            if b.returns && !self.diagnostics.is_empty() {
                continue;
            }
            // An incomplete balance says nothing about the parts on it (ADR 0027).
            if !b.is_complete() && b.draws.iter().any(|d| self.is_part_current(d)) {
                continue;
            }
            let known: Option<Vec<Quantity>> =
                b.draws.iter().map(|d| self.contribution(d)).collect();
            let Some(known) = known else { continue };
            if self.model.instance(b.scope).kind == InstKind::System {
                self.verify_join(&b, &known);
                continue;
            }
            // A port that only gives to what is inside, with nothing inside to take it that
            // the checker knows of (a part that is in no chain and has no rail of its own,
            // ADR 0014), is feeding what is not modelled: take what is stated.
            if known.len() == 1 && known[0].hi() <= 0.0 {
                continue;
            }
            let all: Vec<String> = b
                .draws
                .iter()
                .zip(&known)
                .map(|(d, q)| {
                    let sign = if d.flipped { "-" } else { "" };
                    format!("`{sign}{}` is {q}", self.model.path_of_field(d.field))
                })
                .collect();
            let slack = |qs: &[&Quantity]| {
                // A little slack, for the outward rounding of the sums.
                1e-9 * qs
                    .iter()
                    .flat_map(|q| [q.lo(), q.hi()])
                    .fold(1e-12_f64, |m, x| m.max(x.abs()))
            };
            // A stated current is a budget (ADR 0038): what the rest of the net determines
            // has to lie inside it. It need not fill it.
            if let Some(budget) = self.budget(&b) {
                let (stated, works_out) = (budget.stated, budget.works_out);
                let slack = slack(&[&stated, &works_out]);
                if stated.lo() - slack <= works_out.lo() && works_out.hi() <= stated.hi() + slack {
                    continue;
                }
                let d = &b.draws[budget.index];
                let name = self.model.path_of_field(d.field);
                let sign = if d.flipped { "-" } else { "" };
                let message = format!(
                    "the currents on net `{}` do not add up: `{sign}{name}` must stay within {stated}, but the others on it work out to {works_out} (all of them: {})",
                    b.net,
                    all.join(", ")
                );
                let (source, span) = self.balance_site(&b);
                self.report(source, span, message);
                continue;
            }
            // Otherwise the terms are ranges that all hold or all are worked out from one
            // another, and the balance only has to be able to reach zero.
            let total = known
                .iter()
                .skip(1)
                .try_fold(known[0], |sum, q| sum.add(q).ok());
            let Some(total) = total else { continue };
            if total.lo() - slack(&[&total]) <= 0.0 && 0.0 <= total.hi() + slack(&[&total]) {
                continue;
            }
            // The first term, against the others.
            let others = known
                .iter()
                .skip(1)
                .try_fold(None, |sum: Option<Quantity>, q| match sum {
                    None => Some(Some(*q)),
                    Some(s) => s.add(q).ok().map(Some),
                })
                .map(|sum| sum.or(Quantity::point(Dimension::CURRENT, 0.0).ok()));
            let Some(Some(others)) = others else { continue };
            let d = &b.draws[0];
            let name = self.model.path_of_field(d.field);
            let sign = if d.flipped { "-" } else { "" };
            let message = format!(
                "the currents on net `{}` do not add up: `{sign}{name}` is {}, so the others on it must add up to {}, but they can only give {others} (all of them: {})",
                b.net,
                known[0],
                known[0].neg(),
                all.join(", ")
            );
            let (source, span) = self.balance_site(&b);
            self.report(source, span, message);
        }
    }

    /// The ports a system joins, with every current known. Each port's current is what the
    /// outside draws through it (a board's rail is written that way), so they are mirror
    /// statements of one flow and must be able to add up to zero. A port that cannot draw
    /// (its current is never negative) is a supply, and its range is how much the board is
    /// built to give; what the other ports draw must lie within it. A port that draws
    /// states only what its board takes, so it is not checked against what the supply says
    /// (`.output.current &= 0mA..50mA` against a board that takes 5 mA to 15 mA is fine).
    fn verify_join(&mut self, b: &Balance, known: &[Quantity]) {
        let zero = Quantity::point(Dimension::CURRENT, 0.0).ok();
        let sum = |skip: Option<usize>| {
            known
                .iter()
                .enumerate()
                .filter(|&(j, _)| Some(j) != skip)
                .try_fold(zero, |sum, (_, q)| sum.map(|s| s.add(q).ok()))
                .flatten()
        };
        let slack = |qs: &[&Quantity]| {
            1e-9 * qs
                .iter()
                .flat_map(|q| [q.lo(), q.hi()])
                .fold(1e-12_f64, |m, x| m.max(x.abs()))
        };
        let describe = |this: &Self, i: usize| {
            let field = b.draws[i].field;
            let owner = this.model.field(field).owner;
            let board = this
                .model
                .owner_of(owner)
                .map(|inst| this.model.instance(inst).type_name.clone())
                .unwrap_or_default();
            format!(
                "`{}` (board `{board}`) is {}",
                this.model.path_of_field(field),
                known[i]
            )
        };
        let all: Vec<String> = (0..known.len()).map(|i| describe(self, i)).collect();
        let (source, span) = self.balance_site(b);

        let Some(total) = sum(None) else { return };
        if total.lo() - slack(&[&total]) > 0.0 || total.hi() + slack(&[&total]) < 0.0 {
            let message = format!(
                "the currents at the join `{}` cannot add up to zero: {}, which add up to {total}",
                b.net,
                all.join(" and ")
            );
            self.report(source, span, message);
            return;
        }
        for (i, current) in known.iter().enumerate() {
            let Some(others) = sum(Some(i)) else { continue };
            if current.lo() < 0.0 {
                continue;
            }
            let demand = others.neg();
            let slack = slack(&[current, &demand]);
            if current.lo() - slack <= demand.lo() && demand.hi() <= current.hi() + slack {
                continue;
            }
            let rest: Vec<&str> = all
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, d)| d.as_str())
                .collect();
            let message = format!(
                "the join `{}` asks more of a port than its board allows: {}, but the other ports there ({}) draw {demand}",
                b.net,
                all[i],
                rest.join(", ")
            );
            self.report(source, span, message);
        }
    }

    /// A failed requirement on a current, with what that current is made of: each net it
    /// is summed on, with the terms and their ranges, and the nets of the terms in turn (a
    /// regulator's output is the board's whole load, which is the rails and the parts on
    /// the net). Return nets repeat the same terms and are left out. A requirement that
    /// mentions no current in a balance is returned as it was.
    fn with_currents(&self, r: &Relation, message: String) -> String {
        let balances: Vec<Balance> = self
            .model
            .balances()
            .iter()
            .filter(|b| !b.returns && self.model.instance(b.scope).kind != InstKind::System)
            .cloned()
            .collect();
        let mut fields = Vec::new();
        fields_in(&r.lhs, &mut fields);
        fields_in(&r.rhs, &mut fields);
        let mut seen = vec![false; balances.len()];
        let mut notes = Vec::new();
        while let Some(f) = fields.pop() {
            for (i, b) in balances.iter().enumerate() {
                if seen[i] || !b.draws.iter().any(|d| d.field == f) {
                    continue;
                }
                seen[i] = true;
                let terms: Vec<String> = b
                    .draws
                    .iter()
                    .map(|d| {
                        let sign = if d.flipped { "-" } else { "" };
                        let name = self.model.path_of_field(d.field);
                        format!("{sign}{name} ({})", self.model.field(d.field).domain)
                    })
                    .collect();
                let scope = match self.model.path_of(b.scope) {
                    path if path.is_empty() => String::new(),
                    path => format!(" in `{path}`"),
                };
                let unmodelled = if b.is_complete() {
                    String::new()
                } else {
                    let pins: Vec<String> = b
                        .unmodelled
                        .iter()
                        .map(|&p| format!("`{}`", self.model.path_of(p)))
                        .collect();
                    format!(
                        ", but the balance is incomplete: {} {} not modelled",
                        pins.join(", "),
                        if pins.len() == 1 { "is" } else { "are" }
                    )
                };
                notes.push(format!(
                    "net `{}`{scope}: {} add up to zero{unmodelled}",
                    b.net,
                    terms.join(" + ")
                ));
                fields.extend(b.draws.iter().map(|d| d.field));
            }
        }
        if notes.is_empty() {
            message
        } else {
            format!("{message} (the currents on {})", notes.join("; "))
        }
    }

    /// Where to point about a net's currents: at a statement that set one of them, or else
    /// at the board or module they are in.
    fn balance_site(&self, b: &Balance) -> (SourceId, Span) {
        b.draws
            .iter()
            .find_map(|d| self.model.field(d.field).narrowed_at.last().copied())
            .unwrap_or_else(|| {
                let scope = self.model.instance(b.scope);
                (scope.source, scope.span)
            })
    }

    /// Whether `e` is a text field that nothing has set.
    fn is_unset_text(&self, e: &RExpr) -> bool {
        matches!(e, RExpr::Field(f)
            if self.model.field(*f).kind == FieldKind::Text
                && self.model.field(*f).domain == Domain::Unset)
    }

    /// A requirement on a position (ADR 0045) that nothing satisfies: its bounds leave an
    /// empty window. Reported at the last statement that narrows the field.
    fn check_windows(&mut self) {
        let fields: Vec<FieldId> = self
            .model
            .fields()
            .filter(|(id, _)| self.model.is_place_field(*id))
            .map(|(id, _)| id)
            .collect();
        for field in fields {
            let w = crate::place::window(&self.model, field);
            if !w.is_empty() {
                continue;
            }
            let FieldKind::Quantity(dim) = self.model.field(field).kind else {
                continue;
            };
            let name = self.model.path_of_field(field);
            if let Some(&(source, span)) = w.sites.last() {
                self.report(source, span, w.clash(&name, dim));
            }
        }
    }

    /// Whether a relation is a bound on a `.place` field or a value tied to one: those are
    /// requirements that lowering reads (ADR 0045), not claims to verify.
    fn place_side<'r>(&self, r: &'r Relation) -> Option<(FieldId, &'r RExpr)> {
        let is_place = |e: &RExpr| match e {
            RExpr::Field(f) if self.model.is_place_field(*f) => Some(*f),
            _ => None,
        };
        match r.op {
            CmpOp::Ge | CmpOp::Le => is_place(&r.lhs)
                .map(|f| (f, &r.rhs))
                .or_else(|| is_place(&r.rhs).map(|f| (f, &r.lhs))),
            CmpOp::Narrow => is_place(&r.lhs).map(|f| (f, &r.rhs)),
            CmpOp::EqEq => None,
        }
    }

    fn verify(&self, r: &Relation) -> Verdict {
        let show = |e: &RExpr| self.model.show_expr(e);
        let fail = |message: String, margin: f64| Verdict::Fail { message, margin };
        // A bound on a position narrows it. Its other side may still be waiting on a value;
        // a value a module offers (`.j.place.y &= .shell_y`, ADR 0042) that the board leaves
        // alone is no note.
        if let Some((_, other)) = self.place_side(r)
            && r.op != CmpOp::Narrow
        {
            return match eval(&self.model, other) {
                Ok(Val::Q(_)) => Verdict::Pass,
                Err(EvalError::Unknown(_)) => Verdict::Unknown,
                Ok(Val::T(_)) | Err(EvalError::Type) => {
                    fail("a position is a number".to_string(), 1.0)
                }
                Err(e) => fail(format!("{}: {e}", show(other)), 1.0),
            };
        }
        if let Some((_, RExpr::Field(g))) = self.place_side(r)
            && r.op == CmpOp::Narrow
            && self.model.field(*g).domain == Domain::Unset
        {
            return Verdict::Pass;
        }
        let eval_both = || -> Result<(Val, Val), EvalError> {
            let l = eval(&self.model, &r.lhs);
            let rr = eval(&self.model, &r.rhs);
            Ok((l?, rr?))
        };

        match r.op {
            CmpOp::Narrow => match eval(&self.model, &r.rhs) {
                Ok(_) => Verdict::Pass,
                // A text field nobody set narrows nothing: a value a module offers
                // (`.j.assembly &= .assembly_mode`, ADR 0042) may be left alone. A text
                // value is never worked out, so there is nothing still to wait for.
                Err(EvalError::Unknown(_)) if self.is_unset_text(&r.rhs) => Verdict::Pass,
                Err(EvalError::Unknown(_)) => Verdict::Unknown,
                Err(e) => fail(format!("{}: {e}", show(&r.rhs)), 1.0),
            },
            CmpOp::EqEq => {
                let rhs = match eval(&self.model, &r.rhs) {
                    Ok(v) => v,
                    Err(EvalError::Unknown(_)) => return Verdict::Unknown,
                    Err(e) => return fail(format!("{}: {e}", show(&r.rhs)), 1.0),
                };
                if let RExpr::Field(f) = &r.lhs {
                    let name = self.model.path_of_field(*f);
                    return match (&self.model.field(*f).domain, rhs) {
                        (Domain::Unset, _) => Verdict::Unknown,
                        (Domain::Quantity(d), Val::Q(v)) => {
                            // See `propagate`: a derived current only has to share a value
                            // with the equation.
                            let derived = self.derived.contains(f);
                            if d.encloses(&v).unwrap_or(false)
                                || (derived && d.intersect(&v).ok().flatten().is_some())
                            {
                                Verdict::Pass
                            } else {
                                fail(
                                    format!(
                                        "`{name}` must stay within {d}, but {} works out to {v}",
                                        show(&r.rhs)
                                    ),
                                    overshoot(d, &v),
                                )
                            }
                        }
                        (Domain::Text(d), Val::T(v)) if *d == v => Verdict::Pass,
                        (Domain::Text(d), Val::T(v)) => fail(
                            format!("`{name}` must be \"{d}\", but {} is \"{v}\"", show(&r.rhs)),
                            1.0,
                        ),
                        _ => fail(
                            format!("`{name}` and {} are different kinds of value", show(&r.rhs)),
                            1.0,
                        ),
                    };
                }
                match eval_both() {
                    Ok((Val::Q(l), Val::Q(v))) => {
                        if l.intersect(&v).ok().flatten().is_some() {
                            Verdict::Pass
                        } else {
                            fail(
                                format!(
                                    "{} is {l} but {} is {v}; they can never be equal",
                                    show(&r.lhs),
                                    show(&r.rhs)
                                ),
                                overshoot(&l, &v),
                            )
                        }
                    }
                    Err(EvalError::Unknown(_)) => Verdict::Unknown,
                    _ => Verdict::Pass,
                }
            }
            CmpOp::Ge | CmpOp::Le => match eval_both() {
                Ok((Val::Q(l), Val::Q(v))) => {
                    let (ok, word, gap) = if r.op == CmpOp::Ge {
                        (l.lo() >= v.hi(), "at least", v.hi() - l.lo())
                    } else {
                        (l.hi() <= v.lo(), "at most", l.hi() - v.lo())
                    };
                    if ok {
                        Verdict::Pass
                    } else {
                        let scale = l
                            .lo()
                            .abs()
                            .max(l.hi().abs())
                            .max(v.lo().abs())
                            .max(v.hi().abs())
                            .max(1e-30);
                        fail(
                            format!(
                                "{} must always be {word} {}, but {l} is not always {} {v}",
                                show(&r.lhs),
                                show(&r.rhs),
                                if r.op == CmpOp::Ge { "above" } else { "below" },
                            ),
                            gap / scale,
                        )
                    }
                }
                Err(EvalError::Unknown(_)) => Verdict::Unknown,
                Err(e) => fail(format!("{e}"), 1.0),
                Ok(_) => fail("a comparison needs numbers, not text".into(), 1.0),
            },
        }
    }

    // ---- picking -------------------------------------------------------------

    fn pick_parts(&mut self) {
        let mut parts: Vec<(String, InstId)> = self
            .model
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Part && i.catalog.is_some())
            .map(|(id, _)| (self.model.path_of(id), id))
            .collect();
        parts.sort_by(|a, b| a.0.cmp(&b.0));

        let mut jobs: Vec<Job> = Vec::new();
        for (path, part) in parts {
            let catalog = self
                .model
                .instance(part)
                .catalog
                .clone()
                .expect("filtered above");
            let columns: Vec<(FieldId, String)> = catalog
                .fields
                .iter()
                .map(|&f| {
                    let full = self.model.path_of_field(f);
                    let key = full
                        .strip_prefix(&format!("{path}."))
                        .unwrap_or(&full)
                        .to_string();
                    (f, key)
                })
                .collect();
            let kinds: Vec<(String, FieldKind)> = columns
                .iter()
                .map(|(f, key)| (key.clone(), self.model.field(*f).kind))
                .collect();
            match self.catalogs.resolve(&catalog.name, &kinds) {
                None => self.unresolved.push(Unresolved {
                    part,
                    reason: format!("there is no `{}` catalog to choose from", catalog.name),
                }),
                Some(Err(problem)) => {
                    let inst = self.model.instance(part);
                    let (source, span) = inst.at();
                    self.report(
                        source,
                        span,
                        format!(
                            "`{path}` cannot be chosen from the `{}` catalog: {problem}",
                            catalog.name
                        ),
                    );
                    self.unresolved.push(Unresolved {
                        part,
                        reason: format!("the `{}` catalog cannot be read", catalog.name),
                    });
                }
                Some(Ok(cat)) if self.unnamed_key(part, &path, &catalog, &columns, &cat) => {}
                Some(Ok(cat)) => {
                    let used = columns.iter().map(|(f, _)| self.mentioned(*f)).collect();
                    jobs.push(Job {
                        part,
                        path,
                        columns,
                        used,
                        catalog: cat,
                    });
                }
            }
        }

        let coupling = self.coupling(&jobs);
        for group in self.group_jobs(&jobs) {
            self.solve_group(&jobs, &group, &coupling);
        }
    }

    /// A keyed catalog (`catalog mcu by .mpn`) is not searched: the design names the row.
    /// If it has not, say which rows it could name. True if the key was missing and
    /// reported. A key that no row fits is left to the search, whose message says what the
    /// design asks for.
    fn unnamed_key(
        &mut self,
        part: InstId,
        path: &str,
        catalog: &netsmith_check::Catalog,
        columns: &[(FieldId, String)],
        cat: &Catalog,
    ) -> bool {
        let Some(key) = catalog.key else { return false };
        if !matches!(self.domain(key), Domain::Unset) {
            return false;
        }
        let key_name = columns
            .iter()
            .find(|(f, _)| *f == key)
            .map(|(_, k)| k.clone())
            .unwrap_or_default();
        let mut names: Vec<String> = Vec::new();
        for c in cat
            .candidates
            .iter()
            .filter(|c| self.fits(part, columns, c))
        {
            if let Some(Value::Text(t)) = c.column(&key_name)
                && !names.contains(t)
            {
                names.push(t.clone());
            }
        }
        if names.is_empty() {
            return false;
        }
        let inst = self.model.instance(part);
        let ty = inst.type_name.clone();
        let (source, span) = inst.at();
        self.report(
            source,
            span,
            format!(
                "`{path}` is {} `{ty}`; name the part with .{key_name}: {}",
                article(&ty),
                names.join(", ")
            ),
        );
        self.unresolved.push(Unresolved {
            part,
            reason: format!(
                "its `{}` catalog needs `.{key_name}` to say which part",
                cat.name
            ),
        });
        true
    }

    /// The classes of fields that propagation joins, and which jobs' columns and parts are in
    /// them. Only what propagation does joins fields: a connection, an equation (`==` or
    /// `&=`) and the currents of a balance. A bound (`>=`, `<=`) only judges.
    fn coupling(&self, jobs: &[Job]) -> Coupling {
        let n_fields = self.model.fields().count();
        let mut parent: Vec<usize> = (0..n_fields).collect();
        for (a, b) in self.model.equalities() {
            union(&mut parent, a.0, b.0);
        }
        for r in self.model.relations() {
            if !matches!(r.op, CmpOp::EqEq | CmpOp::Narrow) {
                continue;
            }
            let mut fields = Vec::new();
            fields_in(&r.lhs, &mut fields);
            fields_in(&r.rhs, &mut fields);
            for w in fields.windows(2) {
                union(&mut parent, w[0].0, w[1].0);
            }
        }
        for b in self.model.balances() {
            for w in b.draws.windows(2) {
                union(&mut parent, w[0].field.0, w[1].field.0);
            }
        }
        let class: Vec<usize> = (0..n_fields).map(|f| find(&mut parent, f)).collect();
        let columns = jobs
            .iter()
            .map(|job| job.columns.iter().map(|(f, _)| class[f.0]).collect())
            .collect();
        let mut inside: Vec<HashSet<usize>> = vec![HashSet::new(); jobs.len()];
        for (id, field) in self.model.fields() {
            for (j, job) in jobs.iter().enumerate() {
                if self.model.is_inside(field.owner, job.part) {
                    inside[j].insert(class[id.0]);
                }
            }
        }
        Coupling {
            columns,
            inside,
            class,
        }
    }

    /// Split the parts to pick into groups that share a relation. Parts in different
    /// groups cannot affect each other, so each group is searched on its own: one part
    /// with no valid choice does not undo the picks of parts unrelated to it.
    fn group_jobs(&self, jobs: &[Job]) -> Vec<Vec<usize>> {
        // Fields joined by a connection hold one value, so they count as one.
        let n_fields = self.model.fields().count();
        let mut fparent: Vec<usize> = (0..n_fields).collect();
        for (a, b) in self.model.equalities() {
            union(&mut fparent, a.0, b.0);
        }
        let class = |f: FieldId, fparent: &mut Vec<usize>| find(fparent, f.0);

        // The value classes inside each part.
        let mut inside: Vec<std::collections::HashSet<usize>> =
            vec![Default::default(); jobs.len()];
        for (id, field) in self.model.fields() {
            for (j, job) in jobs.iter().enumerate() {
                if self.model.is_inside(field.owner, job.part) {
                    inside[j].insert(class(id, &mut fparent));
                }
            }
        }

        let mut jparent: Vec<usize> = (0..jobs.len()).collect();
        for r in self.model.relations() {
            let mut fields = Vec::new();
            fields_in(&r.lhs, &mut fields);
            fields_in(&r.rhs, &mut fields);
            let classes: std::collections::HashSet<usize> =
                fields.iter().map(|&f| class(f, &mut fparent)).collect();
            let touched: Vec<usize> = (0..jobs.len())
                .filter(|&j| inside[j].iter().any(|c| classes.contains(c)))
                .collect();
            for w in touched.windows(2) {
                union(&mut jparent, w[0], w[1]);
            }
        }

        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut index_of: std::collections::HashMap<usize, usize> = Default::default();
        for j in 0..jobs.len() {
            let root = find(&mut jparent, j);
            let g = *index_of.entry(root).or_insert_with(|| {
                groups.push(Vec::new());
                groups.len() - 1
            });
            groups[g].push(j);
        }
        groups
    }

    /// Find a choice for every part in a group that satisfies every relation together,
    /// backing up when a later part has nothing that works. The first solution in catalog
    /// order (parts in path order) is the one used.
    fn solve_group(&mut self, jobs: &[Job], group: &[usize], coupling: &Coupling) {
        let mut search = Search::default();
        // A part with no candidate that fits what the design already says cannot be chosen
        // whatever the others are, so that is the cause to report. Searching first would
        // blame whichever earlier part's choices failed on the way to it.
        let starved = group.iter().copied().find(|&j| {
            let job = &jobs[j];
            !job.catalog
                .candidates
                .iter()
                .any(|c| self.fits(job.part, &job.columns, c))
        });
        let solved = starved.is_none()
            && matches!(
                self.search(jobs, group, 0, &mut search, coupling),
                Outcome::Solved
            );
        self.trials += search.trials;
        if solved {
            self.picks.extend(search.picks);
            return;
        }

        let names: Vec<String> = group
            .iter()
            .map(|&j| format!("`{}`", jobs[j].path))
            .collect();
        let first = &jobs[group[0]];
        let part = self.model.instance(first.part);
        let (part_source, part_span) = part.at();

        if search.gave_up {
            self.report(
                part_source,
                part_span,
                format!(
                    "gave up after trying {MAX_TRIALS} combinations of candidates for {}; narrow what they need or give them fewer choices",
                    names.join(" and ")
                ),
            );
        } else if let Some(c) = search.closest.take() {
            let (source, span) = c.at;
            let message = if group.len() == 1 {
                format!(
                    "no `{}` candidate satisfies `{}`: the closest of the {} that fit, {}, fails because {}",
                    first.catalog.name,
                    first.path,
                    search.first_level_fits,
                    c.assignment[0],
                    c.message
                )
            } else {
                format!(
                    "no combination of candidates satisfies {} together: the closest, {}, fails because {}",
                    names.join(" and "),
                    c.assignment.join(" and "),
                    c.message
                )
            };
            let notes: Vec<String> = group
                .iter()
                .flat_map(|&j| {
                    [
                        self.derating_note(&jobs[j]),
                        self.package_note(&jobs[j]),
                        self.blank_note(&jobs[j]),
                    ]
                })
                .flatten()
                .collect();
            let message = if notes.is_empty() {
                message
            } else {
                format!("{message} ({})", notes.join("; "))
            };
            self.report(source, span, message);
        } else {
            // Nothing fit what the design already says, so there was nothing to try.
            let j = starved.or(search.no_fit).unwrap_or(group[0]);
            let job = &jobs[j];
            let wants: Vec<String> = job
                .columns
                .iter()
                .filter_map(|(f, key)| match self.domain(*f) {
                    Domain::Unset => None,
                    d => Some(format!("{key} within {d}")),
                })
                .collect();
            let wants = if wants.is_empty() {
                "nothing in particular".to_string()
            } else {
                wants.join(" and ")
            };
            let notes: Vec<String> = [
                self.derating_note(job),
                self.package_note(job),
                self.blank_note(job),
            ]
            .into_iter()
            .flatten()
            .collect();
            let wants = if notes.is_empty() {
                wants
            } else {
                format!("{wants} ({})", notes.join("; "))
            };
            let inst = self.model.instance(job.part);
            let (source, span) = inst.at();
            self.report(
                source,
                span,
                format!(
                    "the `{}` catalog has nothing for `{}`, which needs {wants}",
                    job.catalog.name, job.path
                ),
            );
        }
        for &j in group {
            self.unresolved.push(Unresolved {
                part: jobs[j].part,
                reason: if group.len() == 1 {
                    "no candidate satisfies its requirements".into()
                } else {
                    "no combination of candidates satisfies these parts together".into()
                },
            });
        }
    }

    /// Depth-first: choose a candidate for the part at `depth`, then the rest. Every
    /// relation that can be evaluated is checked as soon as it can, so a bad choice is
    /// dropped before the parts after it are tried.
    ///
    /// Conflict-directed backjumping: a failure says which earlier parts' choices it
    /// depends on (see [`Coupling`]). When every way of choosing the parts after this one
    /// fails for reasons that do not include this part, no other candidate for it can help,
    /// so the search goes straight back to the nearest part the failure does depend on,
    /// instead of choosing the parts in between again and reaching the same failure each
    /// time. What it skips could only fail the same way, so the first solution, and the
    /// closest failure, are those of the plain search; the remaining candidates of a skipped
    /// part are still tried (without going deeper) so that the closest failure is the same.
    fn search(
        &mut self,
        jobs: &[Job],
        group: &[usize],
        depth: usize,
        search: &mut Search,
        coupling: &Coupling,
    ) -> Outcome {
        if depth == group.len() {
            return Outcome::Solved;
        }
        let job = &jobs[group[depth]];
        let inside = &coupling.inside[group[depth]];
        let fitting: Vec<&Candidate> = job
            .catalog
            .candidates
            .iter()
            .filter(|c| self.fits(job.part, &job.columns, c))
            .collect();
        if depth == 0 {
            search.first_level_fits = fitting.len();
        }
        // Which candidates fit depends on the domains of this part's fields.
        let mut conflict = coupling.earlier(group, depth, inside);
        if fitting.is_empty() {
            search.no_fit.get_or_insert(group[depth]);
            return Outcome::Failed(conflict);
        }

        // Set once the parts after this one failed without depending on it: the choices of
        // the parts before it that they depend on.
        let mut jump: Option<BTreeSet<usize>> = None;
        // Candidates whose every completion failed. A later candidate that behaves the same
        // fails the same way, so it is not tried.
        let mut failed: Vec<&Candidate> = Vec::new();
        for (examined, c) in fitting.iter().enumerate() {
            if failed.iter().any(|f| self.interchangeable(job, f, c)) {
                continue;
            }
            if search.trials >= MAX_TRIALS {
                search.gave_up = true;
                return Outcome::GaveUp;
            }
            search.trials += 1;
            let snapshot = self.model.clone();
            match self.try_candidate(job.part, &job.columns, c) {
                Ok(_) if jump.is_some() => {
                    // The parts after this one would fail again, as they did.
                    self.model = snapshot;
                    failed.push(c);
                }
                Ok(needs) => {
                    search.chosen.push(format!("{} = {}", job.path, c.name));
                    search.picks.push(Pick {
                        part: job.part,
                        catalog: job.catalog.name.clone(),
                        candidate: c.name.clone(),
                        label: c.label.clone(),
                        lcsc: c.lcsc.clone(),
                        mpn: c.mpn.clone(),
                        library: c.library,
                        assembly: c.assembly.clone(),
                        generic: c.generic.as_ref().map(|g| g.with_needs(&needs)),
                        rejected: examined,
                    });
                    let outcome = self.search(jobs, group, depth + 1, search, coupling);
                    if let Outcome::Solved = outcome {
                        return outcome;
                    }
                    search.chosen.pop();
                    search.picks.pop();
                    self.model = snapshot;
                    match outcome {
                        Outcome::Failed(mut depends_on) => {
                            if depends_on.remove(&depth) {
                                conflict.extend(depends_on);
                            } else {
                                jump = Some(depends_on);
                            }
                        }
                        _ => return Outcome::GaveUp,
                    }
                    failed.push(c);
                }
                Err(r) => {
                    let mut assignment = search.chosen.clone();
                    assignment.push(format!("{} = {}", job.path, c.name));
                    if search.closest.as_ref().is_none_or(|best| {
                        depth > best.depth || (depth == best.depth && r.margin < best.margin)
                    }) {
                        search.closest = Some(Closest {
                            depth,
                            margin: r.margin,
                            assignment: if group.len() == 1 {
                                vec![r.candidate]
                            } else {
                                assignment
                            },
                            message: r.message,
                            at: r.at,
                        });
                    }
                    // The rejection was worked out from these fields and the candidate's own.
                    let mut reads = coupling.classes_of(&r.reads);
                    reads.extend(inside.iter().copied());
                    conflict.extend(coupling.earlier(group, depth, &reads));
                }
            }
        }
        Outcome::Failed(jump.unwrap_or(conflict))
    }

    /// Whether the design cannot tell two candidates of one part apart: they are equal in
    /// every column that a relation or a connection uses. The others (the package, the
    /// color, a capacitance nobody relates) are only matched against what the design says,
    /// which both candidates already satisfy, so they cannot change any other part's choice
    /// or any relation's verdict. A candidate that behaves like one whose every completion
    /// failed fails the same way; skipping it is what keeps a large catalog from being
    /// searched once per copy of the same electrical part.
    fn interchangeable(&self, job: &Job, a: &Candidate, b: &Candidate) -> bool {
        job.columns
            .iter()
            .zip(&job.used)
            .all(|((_, key), &used)| !used || a.column(key) == b.column(key))
    }

    /// Whether anything but the part's own constraints reads a field: a connection joins it
    /// to another field, a net sums it, or a relation also names another field. A relation
    /// that names only this field (`.effective_capacitance >= 1uF`) only judges the part that
    /// holds it.
    fn mentioned(&self, field: FieldId) -> bool {
        self.model
            .equalities()
            .iter()
            .any(|&(a, b)| a == field || b == field)
            || self
                .model
                .balances()
                .iter()
                .any(|b| b.draws.iter().any(|d| d.field == field))
            || self.model.relations().iter().any(|r| {
                let mut fields = Vec::new();
                fields_in(&r.lhs, &mut fields);
                fields_in(&r.rhs, &mut fields);
                fields.contains(&field) && fields.iter().any(|&g| g != field)
            })
    }

    fn fits(&self, part: InstId, columns: &[(FieldId, String)], c: &Candidate) -> bool {
        if !self.package_has_the_pins(part, c) {
            return false;
        }
        columns.iter().all(|(f, key)| {
            let domain = &self.model.field(*f).domain;
            match (c.column(key), domain) {
                (None, _) => false,
                (Some(Value::Open(sense)), _) => self.need_of(*f, *sense).is_some(),
                (Some(Value::Derated(d)), domain) => match self.derated(part, d) {
                    Ok(q) => match domain {
                        Domain::Quantity(want) => want.encloses(&q).unwrap_or(false),
                        Domain::Unset => true,
                        Domain::Text(_) => false,
                    },
                    // Not knowing the value matters only if the design asks for it.
                    Err(_) => !self.asked(*f),
                },
                (Some(_), Domain::Unset) => true,
                (Some(Value::Quantity(q)), Domain::Quantity(d)) => d.encloses(q).unwrap_or(false),
                (Some(Value::Text(t)), Domain::Text(d)) => t == d,
                _ => false,
            }
        })
    }

    /// Whether a candidate's package has every pin that the board connects (ADR 0016). A
    /// package that lacks one cannot be used, whatever else the row says.
    fn package_has_the_pins(&self, part: InstId, c: &Candidate) -> bool {
        match c.column("package") {
            Some(Value::Text(package)) => self.model.missing_in(part, package).is_empty(),
            _ => true,
        }
    }

    /// What to add to a message when candidates were left out for lacking a pin that the
    /// board connects.
    fn package_note(&self, job: &Job) -> Option<String> {
        let mut left_out: Vec<(String, Vec<String>)> = Vec::new();
        for c in &job.catalog.candidates {
            let Some(Value::Text(package)) = c.column("package") else {
                continue;
            };
            if left_out.iter().any(|(p, _)| p == package) {
                continue;
            }
            let lacking = self.model.missing_in(job.part, package);
            if !lacking.is_empty() {
                let names = lacking
                    .iter()
                    .map(|&p| format!("`{}`", self.model.path_below(p, job.part)))
                    .collect();
                left_out.push((package.clone(), names));
            }
        }
        if left_out.is_empty() {
            return None;
        }
        let each: Vec<String> = left_out
            .iter()
            .map(|(package, pins)| {
                format!(
                    "package `{package}` has no {}, which the board connects",
                    pins.join(", ")
                )
            })
            .collect();
        Some(format!("rows were left out because {}", each.join("; ")))
    }

    /// What to add to a message when rows were left out for an empty cell: a row with no
    /// value for a column fits no design, so a part without a rating (`max_power`) is never
    /// picked, and the message says how many rows that is.
    fn blank_note(&self, job: &Job) -> Option<String> {
        let blank: Vec<String> = job
            .columns
            .iter()
            .filter_map(|(_, key)| {
                let n = job
                    .catalog
                    .candidates
                    .iter()
                    .filter(|c| c.column(key).is_none())
                    .count();
                (n > 0).then(|| format!("{n} for `{key}`"))
            })
            .collect();
        (!blank.is_empty()).then(|| {
            format!(
                "rows with an empty cell are never offered, since a part with no stated rating cannot be shown to stay within it: {}",
                blank.join(", ")
            )
        })
    }

    /// Whether the design says anything about a field: it narrows it or a relation uses it.
    fn asked(&self, field: FieldId) -> bool {
        if !matches!(self.domain(field), Domain::Unset) {
            return true;
        }
        self.model.relations().iter().any(|r| {
            let mut fields = Vec::new();
            fields_in(&r.lhs, &mut fields);
            fields_in(&r.rhs, &mut fields);
            fields.contains(&field)
        })
    }

    /// A candidate's value after derating, at the bias the design gives its part.
    fn derated(&self, part: InstId, d: &Derating) -> Result<Quantity, Underated> {
        let path = format!("{}.{}", self.model.path_of(part), d.against);
        let bias = match self.model.field_at(&path).map(|f| self.domain(f)) {
            Some(Domain::Quantity(q)) => q,
            _ => return Err(Underated::NoBias(path)),
        };
        d.at(&bias).map_err(Underated::Curve)
    }

    /// What to add to a message when a derated column hides candidates: the design gave no
    /// bias, or some candidates have no curve or none that reaches the bias.
    fn derating_note(&self, job: &Job) -> Option<String> {
        for (f, key) in &job.columns {
            let derated: Vec<&Derating> = job
                .catalog
                .candidates
                .iter()
                .filter(|c| c.generic.is_none())
                .filter_map(|c| match c.column(key) {
                    Some(Value::Derated(d)) => Some(d),
                    _ => None,
                })
                .collect();
            let generic = job
                .catalog
                .candidates
                .iter()
                .any(|c| c.generic.is_some() && matches!(c.column(key), Some(Value::Derated(_))));
            if (derated.is_empty() && !generic) || !self.asked(*f) {
                continue;
            }
            let (mut none, mut short) = (0, 0);
            let (mut known_to, mut wanted) = (0.0_f64, 0.0_f64);
            for d in derated {
                match self.derated(job.part, d) {
                    Err(Underated::NoBias(path)) => {
                        return Some(format!(
                            "`{path}` is not known, so `{}.{key}` cannot be worked out: place `{}` between a rail's pos and neg, whose voltage is known, or state `{path}`",
                            job.path, job.path
                        ));
                    }
                    Err(Underated::Curve(DeratingError::NoData)) => none += 1,
                    Err(Underated::Curve(DeratingError::BeyondData {
                        wanted: w,
                        known_to: k,
                    })) => {
                        short += 1;
                        wanted = w;
                        known_to = known_to.max(k);
                    }
                    Ok(_) => {}
                }
            }
            let mut why = Vec::new();
            if none > 0 {
                why.push(format!("no derating data for {none}"));
            }
            if short > 0 {
                why.push(format!(
                    "data only up to {known_to} V, but the bias reaches {wanted} V, for {short}"
                ));
            }
            if generic {
                why.push(
                    "generic parts have no recorded bias curve, and a generic ceramic capacitor is never used where the effective capacitance matters"
                        .to_string(),
                );
            }
            if !why.is_empty() {
                return Some(format!(
                    "`{}.{key}` is unknown for some candidates, so they were not used: {}",
                    job.path,
                    why.join(" and ")
                ));
            }
        }
        None
    }

    /// Try one candidate. On success its values stay in the model; on failure the model
    /// is put back exactly as it was.
    fn try_candidate(
        &mut self,
        part: InstId,
        columns: &[(FieldId, String)],
        c: &Candidate,
    ) -> Result<Vec<(String, Need)>, Rejection> {
        let backup = self.model.clone();
        let mut needs: Vec<(String, Need)> = Vec::new();
        for (f, key) in columns {
            let value = match c.column(key).expect("`fits` checked the column") {
                Value::Quantity(q) => Domain::Quantity(*q),
                Value::Text(t) => Domain::Text(t.clone()),
                Value::Open(sense) => {
                    // A generic part takes what the design says. A relation's demand is
                    // taken exactly, so the relation holds with nothing to spare and any
                    // row that meets the need holds it too.
                    let need = self.need_of(*f, *sense).unwrap_or(Need::Nothing);
                    if let (Need::AtLeast(x) | Need::AtMost(x), FieldKind::Quantity(dim)) =
                        (&need, self.model.field(*f).kind)
                        && let Ok(q) = Quantity::point(dim, *x)
                    {
                        self.model.set_domain(*f, Domain::Quantity(q));
                    }
                    needs.push((key.clone(), need));
                    continue;
                }
                Value::Derated(d) => match self.derated(part, d) {
                    Ok(q) => Domain::Quantity(q),
                    // The design does not ask for it, and `fits` let it through.
                    Err(_) => continue,
                },
            };
            self.model.set_domain(*f, value);
        }

        let reject = |this: &mut Self,
                      message: String,
                      margin: f64,
                      at: (SourceId, Span),
                      reads: Vec<FieldId>| {
            this.model = backup.clone();
            Err(Rejection {
                candidate: c.name.clone(),
                message,
                margin,
                at,
                reads,
            })
        };

        if let Some(conflict) = self.propagate().into_iter().next() {
            return reject(
                self,
                conflict.message,
                1.0,
                (conflict.source, conflict.span),
                conflict.fields,
            );
        }

        let relevant: Vec<Relation> = self
            .model
            .relations()
            .iter()
            .filter(|r| {
                let mut fields = Vec::new();
                fields_in(&r.lhs, &mut fields);
                fields_in(&r.rhs, &mut fields);
                fields
                    .iter()
                    .any(|&f| self.model.is_inside(self.model.field(f).owner, part))
            })
            .cloned()
            .collect();
        // The relations are checked in order, so the one that fails is the first, and the
        // rejection depends on every relation up to it.
        let mut reads = Vec::new();
        for r in &relevant {
            fields_in(&r.lhs, &mut reads);
            fields_in(&r.rhs, &mut reads);
            if let Verdict::Fail { message, margin } = self.verify(r) {
                let message = self.with_currents(r, message);
                return reject(self, message, margin, (r.source, r.span), reads);
            }
        }
        Ok(needs)
    }

    /// What the design asks of a field that a generic part leaves open (ADR 0047), or `None`
    /// if it asks something the part cannot meet by taking it: a value that other values are
    /// worked out from (`Sense::Stated`) must be stated.
    ///
    /// A stated text or range is the need. A rating (`AtLeast`, `AtMost`) with no statement
    /// is the most extreme bound that a relation of the form `rating >= expression` or
    /// `rating <= expression` puts on it, if the expression has a value. If nothing bounds
    /// it the need is `Nothing`: the field stays unknown and a check that mentions it is
    /// listed as not evaluated, as for any unknown value.
    fn need_of(&self, field: FieldId, sense: Sense) -> Option<Need> {
        match self.domain(field) {
            Domain::Text(t) => return Some(Need::Text(t)),
            Domain::Quantity(q) => {
                return Some(Need::Within {
                    lo: q.lo(),
                    hi: q.hi(),
                });
            }
            Domain::Unset => {}
        }
        match sense {
            Sense::Stated => (!self.asked(field)).then_some(Need::Nothing),
            Sense::AtLeast => {
                let demand = self
                    .bounds_on(field, true)
                    .into_iter()
                    .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))));
                Some(demand.map_or(Need::Nothing, Need::AtLeast))
            }
            Sense::AtMost => {
                let allowed = self
                    .bounds_on(field, false)
                    .into_iter()
                    .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.min(x))));
                Some(allowed.map_or(Need::Nothing, Need::AtMost))
            }
        }
    }

    /// The bounds relations put on a field by naming it alone on one side: for `above`, the
    /// highest value of the other side of each `field >= x` (the field must be at least that);
    /// otherwise the lowest value of the other side of each `field <= x`.
    fn bounds_on(&self, field: FieldId, above: bool) -> Vec<f64> {
        let mut out = Vec::new();
        for r in self.model.relations() {
            let (larger, smaller) = match r.op {
                CmpOp::Ge => (&r.lhs, &r.rhs),
                CmpOp::Le => (&r.rhs, &r.lhs),
                _ => continue,
            };
            let (mine, other) = if above {
                (larger, smaller)
            } else {
                (smaller, larger)
            };
            if matches!(mine, RExpr::Field(f) if *f == field)
                && let Ok(Val::Q(q)) = eval(&self.model, other)
            {
                out.push(if above { q.hi() } else { q.lo() });
            }
        }
        out
    }
}
