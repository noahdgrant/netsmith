//! Looking ahead at the parts of a group still to be chosen (issue #851).
//!
//! A bound that reads every part of a group (a total that a chain of parts feeds) is judged only
//! when the last of them is chosen: before that it reads a part that has no value yet, and a
//! bound on a value that is not known is not judged. So a chain that no choice can satisfy was
//! searched to the end of every combination, and gave up at the limit of the search: nine
//! resistors of at most 800 ohm whose total must be at least 100 Mohm is 8^9 combinations, every
//! one refused at its last part.
//!
//! Before the search chooses a part, every part of the group still to be chosen is given the
//! *hull* of its candidates that fit (the range from the lowest low end to the highest high end
//! of each column, as in `narrow.rs`), the values are propagated, and each bound (`>=` or `<=`)
//! that one of those parts feeds is asked whether any value of the ranges can meet it
//! (`Solver::verify_possible`). A conflict found while propagating, or a bound that no value of
//! the ranges meets, is met by no choice of the candidates either: a choice lies inside the
//! hull, and what is worked out from a part of a range lies in what the range gives. The search
//! then goes back without trying the part, as if every candidate of it had been tried and
//! refused, and the failure is made of the parts that feed the bound.
//!
//! A refusal that only one part still to be chosen feeds is left to that part's trials: they
//! refuse it as surely, the search goes back from there to the part it depends on, and their
//! message names the candidate that came closest.
//!
//! What is left as the design has it: a part with a bank count (its count changes the values), a
//! column that a row leaves open (a generic part) or that is derated (it depends on a voltage
//! still to be worked out), a column whose rows do not agree on one kind, a solver-added bound,
//! a bound on a position, a bound that reads the end of a range, and a bound that reads a value
//! the DC analysis wrote (it only has to share a value with what is worked out). Each makes the
//! check weaker, never wrong.
//!
//! What does not change: the picks of a group that is solved (a choice refused here had no
//! choice of the later parts that a trial accepts, so the first solution is the same), and the
//! limit of the search. What changes: a group with no solution can end sooner, with a message
//! that names the parts the bound reads and the range they can give. A bound that must hold over
//! the whole range of a value (`verify`) is asked here only whether some value meets it, so a
//! chain whose every choice puts the range across the bound (six LED branches whose worst-case
//! current is over the regulator's) is still searched to the end.

use std::collections::{BTreeSet, HashSet};

use netsmith_check::Domain;
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::{Dimension, Quantity};

use super::{Coupling, Job, Rejection, Solver, Verdict};
use crate::catalog::{Candidate, Value};

/// What the look-ahead found: the refusal, and the depths of the parts given a hull that feed
/// what it read.
pub(super) struct Ahead {
    pub(super) rejection: Rejection,
    pub(super) later: BTreeSet<usize>,
}

/// The hull of a part's candidates that fit, column by column, and the domains of its columns
/// that decided which fit: it stands while they are the same.
pub(super) struct Hull {
    domains: Vec<Domain>,
    columns: Vec<Option<Domain>>,
}

impl Solver<'_> {
    /// Whether no choice of the parts of `group` from `from` on can be accepted with the choices
    /// before it: their hulls give a conflict, or a bound they feed that no value meets. The
    /// model and the solver are put back as they were.
    pub(super) fn look_ahead(
        &mut self,
        jobs: &[Job],
        group: &[usize],
        from: usize,
        coupling: &Coupling,
    ) -> Option<Ahead> {
        let backup = self.model.checkpoint();
        let derived = self.derived.clone();
        let mut hulled: HashSet<usize> = HashSet::new();
        for &j in &group[from..] {
            let job = &jobs[j];
            if job.count.is_some() {
                continue;
            }
            let domains: Vec<Domain> = job
                .columns
                .iter()
                .map(|(f, _)| self.model.field(*f).domain.clone())
                .collect();
            let hulls = match self.hulls.get(&j) {
                Some(h) if h.domains == domains => h.columns.clone(),
                _ => {
                    let fitting: Vec<&Candidate> = job
                        .catalog
                        .candidates
                        .iter()
                        .filter(|c| self.fits(job.part, &job.columns, c))
                        .collect();
                    self.work += job.catalog.candidates.len() as u64;
                    // A part with nothing that fits is left alone: the search says so when it
                    // gets there.
                    let columns: Vec<Option<Domain>> = if fitting.is_empty() {
                        vec![None; job.columns.len()]
                    } else {
                        job.columns
                            .iter()
                            .map(|(_, key)| column_hull(&fitting, key))
                            .collect()
                    };
                    self.hulls.insert(
                        j,
                        Hull {
                            domains,
                            columns: columns.clone(),
                        },
                    );
                    columns
                }
            };
            let mut any = false;
            for ((field, _), domain) in job.columns.iter().zip(hulls) {
                if let Some(domain) = domain {
                    self.model.set_domain(*field, domain);
                    any = true;
                }
            }
            if any {
                hulled.insert(j);
            }
        }
        let found = if hulled.is_empty() {
            None
        } else {
            self.ahead_refused(jobs, group, from, coupling, &hulled, &backup)
        };
        self.derived = derived;
        self.model.rollback(backup);
        found
    }

    fn ahead_refused(
        &mut self,
        jobs: &[Job],
        group: &[usize],
        from: usize,
        coupling: &Coupling,
        hulled: &HashSet<usize>,
        backup: &netsmith_check::Checkpoint,
    ) -> Option<Ahead> {
        // The parts given a hull that feed what was read, or all of them when the dependence
        // names none (a conflict between values that no part is known to feed).
        let later_of = |fed: &HashSet<usize>| -> BTreeSet<usize> {
            let given = (from..group.len()).filter(|&d| hulled.contains(&group[d]));
            let feeding: BTreeSet<usize> =
                given.clone().filter(|&d| fed.contains(&group[d])).collect();
            if feeding.is_empty() {
                given.collect()
            } else {
                feeding
            }
        };
        if let Some(conflict) = self.propagate().into_iter().next() {
            let fed = coupling.fed_by(&conflict.fields);
            let later = later_of(&fed);
            if later.len() < 2 {
                return None;
            }
            return Some(Ahead {
                later,
                rejection: Rejection {
                    message: conflict.message,
                    margin: 1.0,
                    at: (conflict.source, conflict.span),
                    own: conflict.fields,
                    relation: None,
                    network: false,
                },
            });
        }
        let changed = self.model.changed_since(backup);
        let relations = std::sync::Arc::clone(self.model.shared_relations());
        let design = self.reads();
        let mut touched: Vec<usize> = hulled
            .iter()
            .flat_map(|&j| design.touching(jobs[j].part, changed.iter().copied()))
            .collect();
        touched.sort_unstable();
        touched.dedup();
        for index in touched {
            let r = &relations[index];
            // A bound that a part given a hull feeds: the trial of that part judges it, so what
            // no value meets here is refused there.
            if index >= self.original_relations
                || !matches!(r.op, CmpOp::Ge | CmpOp::Le)
                || coupling.feeding[index].is_disjoint(hulled)
                || self.place_side(r).is_some()
            {
                continue;
            }
            let fields = design.fields_of(index);
            if fields.iter().any(|f| self.derived.contains(f))
                || !fields.iter().any(|f| changed.contains(f))
            {
                continue;
            }
            self.work += 1;
            if let Verdict::Fail { message, margin } = self.verify_possible(r) {
                let mut own = Vec::new();
                crate::eval::fields_in(&r.lhs, &mut own);
                crate::eval::fields_in(&r.rhs, &mut own);
                let fed = coupling.fed_by(&own);
                let later = later_of(&fed);
                if later.len() < 2 {
                    continue;
                }
                let message = self.relation_message(index, r, message);
                return Some(Ahead {
                    later,
                    rejection: Rejection {
                        message,
                        margin,
                        at: (r.source, r.span),
                        own,
                        relation: Some(index),
                        network: false,
                    },
                });
            }
        }
        None
    }
}

/// The domain a column holds over the candidates: the range from the lowest low end to the
/// highest high end, or the one text. `None` when a row leaves it open, has no value, is
/// derated, or the rows do not agree on one kind.
fn column_hull(candidates: &[&Candidate], key: &str) -> Option<Domain> {
    let mut range: Option<(Dimension, f64, f64)> = None;
    let mut text: Option<&str> = None;
    for c in candidates {
        match c.column(key)? {
            Value::Quantity(q) => {
                if text.is_some() {
                    return None;
                }
                range = Some(match range {
                    None => (q.dim(), q.lo(), q.hi()),
                    Some((d, _, _)) if d != q.dim() => return None,
                    Some((d, l, h)) => (d, l.min(q.lo()), h.max(q.hi())),
                });
            }
            Value::Text(t) => {
                if range.is_some() || text.is_some_and(|s| s != t) {
                    return None;
                }
                text = Some(t);
            }
            Value::Open(_) | Value::Derated(_) => return None,
        }
    }
    match (range, text) {
        (Some((dim, lo, hi)), _) => Quantity::interval(dim, lo, hi).ok().map(Domain::Quantity),
        (None, Some(t)) => Some(Domain::Text(t.to_string())),
        _ => None,
    }
}
