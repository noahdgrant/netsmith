//! Narrowing the candidates of a part before they are tried (issue #184).
//!
//! The two resistors of a feedback divider are searched as a pair. After the first is chosen,
//! the second still has every row of the catalog to try, and nearly all of them are refused for
//! one reason: the output they give is outside its window, or the power in them is over their
//! rating. Both are intervals of the second resistor's value once the first is fixed, but a
//! trial costs a full propagation, so trying them one by one is what takes the time.
//!
//! The narrowing asks the question about blocks of candidates instead of one by one. The
//! candidates are cut into blocks (a k-d split on the column in which they differ most), and
//! for a block a *hull* of the candidates is put in the part's columns: each column holds the
//! range from the lowest low end to the highest high end of the block, and a column in which
//! the block is not one text (or in which a row has no value) is left as the design has it.
//! The hull is propagated and judged as a trial is, but only for what no candidate of the block
//! can meet:
//!
//! - the propagation finds two values that cannot be equal (a conflict), or
//! - a bound (`>=` or `<=`) that a trial would judge cannot hold for any value of the ranges
//!   (`Solver::verify_possible`), or
//! - an equation on a value that has a window of its own gives a range that does not touch the
//!   window.
//!
//! The range of every value a candidate of the block gives lies inside the range the hull gives
//! (interval arithmetic: a value worked out from a part of a range lies in what the range gives),
//! so what no value of the hull can meet is met by no candidate, and a trial of each would refuse
//! it. The candidates of such a block are skipped. The ones that remain are tried in their
//! order, so the first that satisfies everything is the one it was.
//!
//! What the search records from a refusal (which earlier picks it depends on, the closest
//! failure for a message) is not recorded for a candidate that is skipped. The first would let a
//! later part jump over an earlier pick that the skipped candidates' refusal depended on, so a
//! level that skipped anything takes every earlier pick of the group as a dependence (a jump is
//! only a saving of work, never a change of the pick). The second only matters when a group has
//! no solution, and then `Solver::search_group` searches it again without narrowing, so that the
//! message, the counts and the work are what they always were.

use std::collections::{HashSet, VecDeque};

use netsmith_check::Domain;
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::{Dimension, Quantity};

use super::{Coupling, Job, Search, Solver, Verdict};
use crate::catalog::{Candidate, Value};
use crate::eval::{Val, eval, fields_in};

/// Blocks of this many candidates or fewer are not asked about: trying each is as cheap.
const LEAF: usize = 4;

/// How many blocks may be asked about before the narrowing gives up on a part that it finds
/// nothing to skip in.
const GIVE_UP_AFTER: usize = 12;

/// What a candidate says of one column.
#[derive(Clone)]
enum Cell {
    Num(Dimension, f64, f64),
    Text(String),
    /// The row has no value for the column, or none that can be worked out now: the column is
    /// left as the design has it.
    Open,
}

impl Solver<'_> {
    /// Which of `fitting` can be skipped because a trial of it would be refused, by index. `None`
    /// when nothing is skipped. Adds the work of the questions to `search`.
    pub(super) fn skippable(
        &mut self,
        job: &Job,
        fitting: &[&Candidate],
        coupling: &Coupling,
        later: &HashSet<usize>,
        search: &mut Search,
    ) -> Option<Vec<bool>> {
        let cells = self.cells_of(job, fitting)?;
        let work = self.work;
        let mut skip = vec![false; fitting.len()];
        let mut skipped = 0usize;
        let mut asked = 0usize;
        let cap = 24 + fitting.len() / 3;
        let mut blocks: VecDeque<Vec<usize>> = VecDeque::new();
        blocks.push_back((0..fitting.len()).collect());
        while let Some(block) = blocks.pop_front() {
            if block.len() <= LEAF || asked >= cap || (asked >= GIVE_UP_AFTER && skipped == 0) {
                continue;
            }
            asked += 1;
            if self.block_refused(job, &cells, &block, coupling, later) {
                for &i in &block {
                    skip[i] = true;
                }
                skipped += block.len();
                continue;
            }
            if let Some((left, right)) = split(&cells, block) {
                blocks.push_back(left);
                blocks.push_back(right);
            }
        }
        search.probe_work += self.work - work;
        (skipped > 0).then_some(skip)
    }

    /// What each candidate says of each column of the part, or `None` when a candidate leaves a
    /// column open for the design to fill (a generic part: its trial reads the need of the
    /// column, which a hull cannot stand for).
    fn cells_of(&self, job: &Job, fitting: &[&Candidate]) -> Option<Vec<Vec<Cell>>> {
        let mut out = Vec::with_capacity(fitting.len());
        for c in fitting {
            let mut row = Vec::with_capacity(job.columns.len());
            for (_, key) in &job.columns {
                row.push(match c.column(key) {
                    Some(Value::Quantity(q)) => Cell::Num(q.dim(), q.lo(), q.hi()),
                    Some(Value::Text(t)) => Cell::Text(t.clone()),
                    Some(Value::Derated(d)) => match self.derated(job.part, d) {
                        Ok(q) => Cell::Num(q.dim(), q.lo(), q.hi()),
                        Err(_) => Cell::Open,
                    },
                    Some(Value::Open(_)) => return None,
                    None => Cell::Open,
                });
            }
            out.push(row);
        }
        Some(out)
    }

    /// Whether no candidate of the block can be accepted: a trial of the hull of the block is
    /// refused in a way that holds for every value inside it. The model and the solver are put
    /// back as they were.
    fn block_refused(
        &mut self,
        job: &Job,
        cells: &[Vec<Cell>],
        block: &[usize],
        coupling: &Coupling,
        later: &HashSet<usize>,
    ) -> bool {
        let backup = self.model.checkpoint();
        let derived = self.derived.clone();
        for (column, (field, _)) in job.columns.iter().enumerate() {
            if let Some(domain) = hull(cells, block, column) {
                self.model.set_domain(*field, domain);
            }
        }
        let refused = self.hull_refused(job, &backup, coupling, later);
        self.derived = derived;
        self.model.rollback(backup);
        refused
    }

    fn hull_refused(
        &mut self,
        job: &Job,
        backup: &netsmith_check::Checkpoint,
        coupling: &Coupling,
        later: &HashSet<usize>,
    ) -> bool {
        if !self.propagate().is_empty() {
            return true;
        }
        // The relations a trial of a candidate would judge, found as `try_candidate_from` finds
        // them. The fields that the hull changed are among those a candidate changes (a
        // candidate lies inside the hull), so a relation that is judged here is judged there.
        let changed = self.model.changed_since(backup);
        let relations = std::sync::Arc::clone(self.model.shared_relations());
        let design = self.reads();
        for index in design.touching(job.part, changed.iter().copied()) {
            let fields = design.fields_of(index);
            let inside = fields
                .iter()
                .any(|&f| self.model.is_inside(self.model.field(f).owner, job.part));
            let (judged, waiting) = if index >= self.original_relations {
                (inside && coupling.feeding[index].is_disjoint(later), false)
            } else if inside
                || (fields.iter().any(|f| changed.contains(f))
                    && coupling.feeding[index].is_disjoint(later))
            {
                (true, false)
            } else {
                (
                    false,
                    matches!(relations[index].op, CmpOp::Ge | CmpOp::Le)
                        && !coupling.feeding[index].is_disjoint(later)
                        && fields.iter().any(|f| changed.contains(f)),
                )
            };
            if !judged && !waiting {
                continue;
            }
            self.work += 1;
            let r = &relations[index];
            match r.op {
                CmpOp::Ge | CmpOp::Le => {
                    // A position is judged by what it can be set to, not by its range.
                    if judged && self.place_side(r).is_some() {
                        continue;
                    }
                    if matches!(self.verify_possible(r), Verdict::Fail { .. }) {
                        return true;
                    }
                }
                CmpOp::EqEq if judged && self.window_missed(index, r) => return true,
                _ => {}
            }
        }
        false
    }

    /// Whether the equation `r` (a field on its left) is one that `verify` refuses for every
    /// candidate of the block: the range it works out to does not touch the range that `verify`
    /// holds the value to. The branches are those of `verify`, read in the same order:
    ///
    /// - a value that several equations state, that a pick can change, and that no statement
    ///   bounds, is held to what the equations that no pick changes allow together, or, with
    ///   none, only has to share a value with the equation;
    /// - any other value is held to its own range.
    ///
    /// In each the check fails when the two ranges have nothing in common, which is what is read
    /// here; where `verify` asks for more (the range inside the other) a candidate is still
    /// tried.
    fn window_missed(&self, index: usize, r: &netsmith_check::Relation) -> bool {
        let netsmith_check::RExpr::Field(f) = &r.lhs else {
            return false;
        };
        if index >= self.original_relations {
            return false;
        }
        let (Domain::Quantity(dx), Ok(Val::Q(v))) =
            (&self.model.field(*f).domain, eval(&self.model, &r.rhs))
        else {
            return false;
        };
        let apart = |a: &Quantity, b: &Quantity| matches!(a.intersect(b), Ok(None));
        let several = self.several_equations.contains(f)
            && self.fed.contains(f)
            && self.model.field(*f).narrowed_at.is_empty()
            && !self.derived.contains(f);
        if !several {
            return apart(dx, &v);
        }
        let changes = |e: &netsmith_check::RExpr| {
            let mut read = Vec::new();
            fields_in(e, &mut read);
            read.iter().any(|g| self.fed.contains(g))
        };
        if !changes(&r.rhs) {
            // Nothing a pick does changes it: the same for every candidate of the block.
            return false;
        }
        let mut allowance: Option<Quantity> = None;
        for &i in self.equations_of.get(f).into_iter().flatten() {
            let other = &self.model.relations()[i];
            if changes(&other.rhs) {
                continue;
            }
            if let Ok(Val::Q(q)) = eval(&self.model, &other.rhs) {
                allowance = match allowance {
                    None => Some(q),
                    Some(a) => match a.intersect(&q) {
                        Ok(Some(both)) => Some(both),
                        _ => Some(a),
                    },
                };
            }
        }
        match allowance {
            Some(a) => apart(&a, &v),
            None => apart(dx, &v),
        }
    }
}

/// The domain a column holds over a block: the range from the lowest low end to the highest
/// high end, or the one text, and nothing when the block does not agree on one kind.
fn hull(cells: &[Vec<Cell>], block: &[usize], column: usize) -> Option<Domain> {
    let mut range: Option<(Dimension, f64, f64)> = None;
    let mut text: Option<&str> = None;
    for &i in block {
        match &cells[i][column] {
            Cell::Open => return None,
            Cell::Num(dim, lo, hi) => {
                if text.is_some() {
                    return None;
                }
                range = Some(match range {
                    None => (*dim, *lo, *hi),
                    Some((d, _, _)) if d != *dim => return None,
                    Some((d, l, h)) => (d, l.min(*lo), h.max(*hi)),
                });
            }
            Cell::Text(t) => {
                if range.is_some() || text.is_some_and(|s| s != t) {
                    return None;
                }
                text = Some(t);
            }
        }
    }
    match (range, text) {
        (Some((dim, lo, hi)), _) => Quantity::interval(dim, lo, hi).ok().map(Domain::Quantity),
        (None, Some(t)) => Some(Domain::Text(t.to_string())),
        _ => None,
    }
}

/// The block cut in two at the median of the column in which its candidates differ most (in
/// ratio, for positive values), or `None` when no column tells them apart.
fn split(cells: &[Vec<Cell>], block: Vec<usize>) -> Option<(Vec<usize>, Vec<usize>)> {
    let columns = cells[block[0]].len();
    let mut best: Option<(usize, f64)> = None;
    #[allow(clippy::needless_range_loop)]
    for column in 0..columns {
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for &i in &block {
            match &cells[i][column] {
                Cell::Num(_, a, b) => {
                    lo = lo.min(*a);
                    hi = hi.max(*b);
                }
                _ => {
                    lo = f64::INFINITY;
                    break;
                }
            }
        }
        if !lo.is_finite() || !hi.is_finite() || hi <= lo {
            continue;
        }
        let spread = if lo > 0.0 {
            (hi / lo).ln()
        } else {
            (hi - lo) / hi.abs().max(lo.abs())
        };
        if spread > 0.0 && best.is_none_or(|(_, s)| spread > s) {
            best = Some((column, spread));
        }
    }
    let (column, _) = best?;
    let mut block = block;
    let key = |i: usize| match &cells[i][column] {
        Cell::Num(_, a, b) => a + b,
        _ => 0.0,
    };
    block.sort_by(|&a, &b| key(a).total_cmp(&key(b)).then(a.cmp(&b)));
    let right = block.split_off(block.len() / 2);
    (!block.is_empty() && !right.is_empty()).then_some((block, right))
}
