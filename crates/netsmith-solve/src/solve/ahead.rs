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
//! A bound that must hold over the whole range of a value (`verify`), and that some value of the
//! hulls meets, is asked again of the *cores* (`always_refused`): each part still to be chosen
//! takes the range that every candidate of it that fits holds (the highest low end to the lowest
//! high end of each column), and a bound that the values worked out from the cores already put
//! across is put across by every choice, since a choice holds the core and what is worked out
//! from a wider range holds what is worked out from a narrower one. Seven LED branches whose
//! current is over the regulator's only at the worst case are refused that way before a trial.
//! Only where that holds: a column that feeds the bound and has no core (a bank, a row that leaves
//! it open, derates it or lacks it, rows that share no value), a part of another group still to
//! be chosen that feeds it, a pick-time analysis that may still write a voltage, and a
//! propagation that kept a value because what it worked out did not lie inside it
//! (`Solver::kept_apart`) each leave the bound to the search. `Catalogs::with_look_ahead_audit`
//! searches each refused choice again without the look and stops if it finds a completion.
//!
//! What does not change: the picks of a group that is solved (a choice refused here had no
//! choice of the later parts that a trial accepts, so the first solution is the same), and the
//! limit of the search. What changes: a group with no solution can end sooner, with a message
//! that names the parts the bound reads and the range they can give, or, for a bound over the
//! whole range, the range every choice of them takes in.

use std::collections::{BTreeSet, HashSet};

use netsmith_check::{Domain, FieldId};
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::{Dimension, Quantity};

use super::{Coupling, Job, Rejection, Solver, Verdict};
use crate::catalog::{Candidate, Value};
use crate::eval::{Val, eval, relation_reads_end};

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
    /// The core of each column (`column_core`): `None` for a column with no core (every column
    /// when nothing fits), `Some(None)` for one that every row leaves without a value.
    cores: Vec<Option<Option<Domain>>>,
}

impl Solver<'_> {
    /// Whether no choice of the parts of `group` from `from` on can be accepted with the choices
    /// before it: their hulls give a conflict, or a bound they feed that no value meets, or
    /// their cores give a bound that must always hold and does not (`always_refused`). The
    /// model and the solver are put back as they were. `elsewhere` are the parts of other groups
    /// still to be chosen.
    pub(super) fn look_ahead(
        &mut self,
        jobs: &[Job],
        group: &[usize],
        from: usize,
        coupling: &Coupling,
        elsewhere: &HashSet<usize>,
    ) -> Option<Ahead> {
        let backup = self.model.checkpoint();
        let derived = self.derived.clone();
        let mut hulled: HashSet<usize> = HashSet::new();
        // The columns of the parts still to be chosen, each with its core, or `None` where it
        // has none: every column of a bank, and its count.
        let mut cores: Vec<(FieldId, Option<Option<Domain>>)> = Vec::new();
        for &j in &group[from..] {
            let job = &jobs[j];
            if job.count.is_some() {
                cores.extend(
                    job.columns
                        .iter()
                        .map(|(f, _)| *f)
                        .chain(job.count)
                        .map(|f| (f, None)),
                );
                continue;
            }
            let (columns, core) = {
                let hull = self.hull_of(jobs, j);
                (hull.columns.clone(), hull.cores.clone())
            };
            cores.extend(job.columns.iter().map(|(f, _)| *f).zip(core));
            let mut any = false;
            for ((field, _), domain) in job.columns.iter().zip(columns) {
                if let Some(domain) = domain {
                    self.model.set_domain(*field, domain);
                    any = true;
                }
            }
            if any {
                hulled.insert(j);
            }
        }
        let mut found = if hulled.is_empty() {
            None
        } else {
            self.ahead_refused(jobs, group, from, coupling, &hulled, &backup)
        };
        self.derived = derived.clone();
        self.model.rollback(backup);
        if found.is_none() {
            found = self.always_refused(group, from, coupling, elsewhere, &cores);
            self.derived = derived;
        }
        found
    }

    /// The hull and the core of a job's candidates that fit, from the cache while the domains of
    /// its columns are the same.
    fn hull_of(&mut self, jobs: &[Job], j: usize) -> &Hull {
        let job = &jobs[j];
        let domains: Vec<Domain> = job
            .columns
            .iter()
            .map(|(f, _)| self.model.field(*f).domain.clone())
            .collect();
        if self.hulls.get(&j).is_none_or(|h| h.domains != domains) {
            let fitting: Vec<&Candidate> = job
                .catalog
                .candidates
                .iter()
                .filter(|c| self.fits(job.part, &job.columns, c))
                .collect();
            self.work += job.catalog.candidates.len() as u64;
            // A part with nothing that fits is left alone: the search says so when it gets
            // there.
            let (columns, cores) = if fitting.is_empty() {
                (vec![None; job.columns.len()], vec![None; job.columns.len()])
            } else {
                (
                    job.columns
                        .iter()
                        .map(|(_, key)| column_hull(&fitting, key))
                        .collect(),
                    job.columns
                        .iter()
                        .map(|(_, key)| column_core(&fitting, key))
                        .collect(),
                )
            };
            self.hulls.insert(
                j,
                Hull {
                    domains,
                    columns,
                    cores,
                },
            );
        }
        &self.hulls[&j]
    }

    /// Whether a bound that must hold over the whole range of a value (`verify`) fails for every
    /// choice of the parts still to be chosen: each of them is given the *core* of its
    /// candidates that fit (the range from the highest low end to the lowest high end of each
    /// column, which every candidate's range holds), the values are propagated, and a `>=` or
    /// `<=` that the cores already put across its bound is put across it by every choice too.
    /// A choice's range holds the core, and what is worked out from a wider range holds what is
    /// worked out from a narrower one, so the range the bound reads holds the one read here.
    ///
    /// Asked only of a bound that two or more of those parts feed, that no part without a core
    /// feeds (a bank, a column some row leaves open, derates or lacks, or whose rows share no
    /// value) and no part of another group still to be chosen feeds (the trial of the last part
    /// of this group judges it over its whole range only when no such part does), and only while
    /// the pick-time analysis can write nothing (`network_quiet`): what it writes narrows a
    /// value beyond what propagation gives, so a range read here would not be held by the one
    /// a trial reads. A conflict among the cores says nothing: a candidate is wider.
    fn always_refused(
        &mut self,
        group: &[usize],
        from: usize,
        coupling: &Coupling,
        elsewhere: &HashSet<usize>,
        cores: &[(FieldId, Option<Option<Domain>>)],
    ) -> Option<Ahead> {
        let relations = std::sync::Arc::clone(self.model.shared_relations());
        let later: HashSet<usize> = group[from..].iter().copied().collect();
        let design = self.reads();
        let bounds: Vec<usize> = (0..self.original_relations)
            .filter(|&index| {
                let r = &relations[index];
                let feeding = &coupling.feeding[index];
                if !matches!(r.op, CmpOp::Ge | CmpOp::Le)
                    || !feeding.is_disjoint(elsewhere)
                    || feeding.intersection(&later).nth(1).is_none()
                    || relation_reads_end(r)
                    || self.place_side(r).is_some()
                {
                    return false;
                }
                // No column without a core is among what the bound is worked out from.
                let sources = coupling.sources(design.fields_of(index));
                cores
                    .iter()
                    .all(|(f, core)| core.is_some() || !sources.contains(f))
            })
            .collect();
        if bounds.is_empty() || !self.network_quiet() {
            return None;
        }
        let backup = self.model.checkpoint();
        for (field, core) in cores {
            if let Some(Some(domain)) = core {
                self.model.set_domain(*field, domain.clone());
            }
        }
        let mut found = None;
        let kept = self.kept_apart;
        if self.propagate().is_empty() && self.kept_apart == kept {
            for index in bounds {
                let r = &relations[index];
                self.work += 1;
                let Verdict::Fail { margin, .. } = self.verify(r) else {
                    continue;
                };
                let (Ok(Val::Q(l)), Ok(Val::Q(v))) =
                    (eval(&self.model, &r.lhs), eval(&self.model, &r.rhs))
                else {
                    continue;
                };
                let (word, side) = if r.op == CmpOp::Ge {
                    ("at least", "above")
                } else {
                    ("at most", "below")
                };
                let message = format!(
                    "{} must always be {word} {}, but its range takes in {l}, which is not always {side} {v}",
                    self.model.show_expr(&r.lhs),
                    self.model.show_expr(&r.rhs),
                );
                let message = self.relation_message(index, r, message);
                let later: BTreeSet<usize> = (from..group.len())
                    .filter(|&d| coupling.feeding[index].contains(&group[d]))
                    .collect();
                let mut own = Vec::new();
                crate::eval::fields_in(&r.lhs, &mut own);
                crate::eval::fields_in(&r.rhs, &mut own);
                found = Some(Ahead {
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
                break;
            }
        }
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

/// The domain that every candidate's value of a column holds: the range from the highest low end
/// to the lowest high end, or the one text. `Some(None)` when no row has a value (a trial leaves
/// the column as the design has it, as this does). `None` when the ranges share no value, a row
/// leaves it open, derates it, or lacks it while another has it, or the rows differ in kind.
fn column_core(candidates: &[&Candidate], key: &str) -> Option<Option<Domain>> {
    let values: Vec<Option<&Value>> = candidates.iter().map(|c| c.column(key)).collect();
    if values.iter().all(Option::is_none) {
        return Some(None);
    }
    let mut range: Option<(Dimension, f64, f64)> = None;
    let mut text: Option<&str> = None;
    for value in values {
        match value? {
            Value::Quantity(q) => {
                if text.is_some() {
                    return None;
                }
                range = Some(match range {
                    None => (q.dim(), q.lo(), q.hi()),
                    Some((d, _, _)) if d != q.dim() => return None,
                    Some((d, l, h)) => (d, l.max(q.lo()), h.min(q.hi())),
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
        (Some((dim, lo, hi)), _) if lo <= hi => Quantity::interval(dim, lo, hi)
            .ok()
            .map(|q| Some(Domain::Quantity(q))),
        (None, Some(t)) => Some(Some(Domain::Text(t.to_string()))),
        _ => None,
    }
}
