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
//! it open, derates it or lacks it, rows that share no value but for the sets below), a part of
//! another group still to be chosen that feeds it, a pick-time analysis that may still write a voltage, and a
//! propagation that kept a value because what it worked out did not lie inside it
//! (`Solver::kept_apart`) each leave the bound to the search. `Catalogs::with_look_ahead_audit`
//! searches each refused choice again without the look and stops if it finds a completion.
//!
//! A quantity column whose rows share no value (LEDs of 1.8 V to 2.1 V beside one of 2.2 V to
//! 2.4 V) has no core, and a board that needs most of its branches to take the one LED that
//! works searched to the limit (issue #1017). Such a part's candidates are split into a few sets
//! that each have a core (`core_sets`, at most `MOST_SETS`), and a bound that only those columns
//! kept from `always_refused` is asked of every combination of the sets of the parts it reads
//! (`split_refused`, at most `MOST_COMBINATIONS` in one look): each choice holds the cores of one
//! combination, so what every combination puts across the bound, every choice puts across it.
//! The same conditions hold, and a combination whose cores conflict or keep a value apart says
//! nothing. Seven such branches are solved in about 130 trials instead of 250,000.
//!
//! What does not change: the picks of a group that is solved (a choice refused here had no
//! choice of the later parts that a trial accepts, so the first solution is the same), and the
//! limit of the search. What changes: a group with no solution can end sooner, with a message
//! that names the parts the bound reads and the range they can give, or, for a bound over the
//! whole range, the range every choice of them takes in.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use netsmith_check::{Domain, FieldId, RExpr};
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

/// A part still to be chosen whose candidates split into sets that each have a core: its
/// columns, and the cores of each set in the same order (`Hull::sets`).
struct Split {
    fields: Vec<FieldId>,
    sets: Vec<Vec<Option<Option<Domain>>>>,
}

/// The hull of a part's candidates that fit, column by column, and the domains of its columns
/// that decided which fit: it stands while they are the same.
pub(super) struct Hull {
    domains: Vec<Domain>,
    columns: Vec<Option<Domain>>,
    /// The core of each column (`column_core`): `None` for a column with no core (every column
    /// when nothing fits), `Some(None)` for one that every row leaves without a value.
    cores: Vec<Option<Option<Domain>>>,
    /// When some quantity column has no core because the rows' ranges share no value, the
    /// candidates split into a few sets that each have one (`core_sets`): the cores of each
    /// set, column by column. Empty when the candidates have one core, or split into too many.
    sets: Vec<Vec<Option<Option<Domain>>>>,
}

/// The most sets a part's candidates are split into for `core_sets`, and the most combinations
/// of sets that `split_refused` propagates in one look: each is about the cost of a trial.
const MOST_SETS: usize = 4;
const MOST_COMBINATIONS: usize = 64;

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
        // The parts whose candidates split into sets that each have a core: their columns and
        // the cores of each set.
        let mut split: Vec<Split> = Vec::new();
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
            let (columns, core, sets) = {
                let hull = self.hull_of(jobs, j);
                (hull.columns.clone(), hull.cores.clone(), hull.sets.clone())
            };
            cores.extend(job.columns.iter().map(|(f, _)| *f).zip(core));
            if !sets.is_empty() {
                split.push(Split {
                    fields: job.columns.iter().map(|(f, _)| *f).collect(),
                    sets,
                });
            }
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
            self.derived = derived.clone();
        }
        if found.is_none() && !split.is_empty() {
            found = self.split_refused(group, from, coupling, elsewhere, &cores, &split);
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
            let (columns, cores, sets) = if fitting.is_empty() {
                (
                    vec![None; job.columns.len()],
                    vec![None; job.columns.len()],
                    Vec::new(),
                )
            } else {
                let keys: Vec<&str> = job.columns.iter().map(|(_, key)| key.as_str()).collect();
                let cores: Vec<Option<Option<Domain>>> =
                    keys.iter().map(|key| column_core(&fitting, key)).collect();
                self.work += fitting.len() as u64;
                let sets = core_sets(&fitting, &keys, &cores);
                (
                    keys.iter().map(|key| column_hull(&fitting, key)).collect(),
                    cores,
                    sets,
                )
            };
            self.hulls.insert(
                j,
                Hull {
                    domains,
                    columns,
                    cores,
                    sets,
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
                    || self.set_aside(index)
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
                found = Some(self.refused_always(index, message, margin, group, from, coupling));
                break;
            }
        }
        self.model.rollback(backup);
        found
    }

    /// The refusal of a bound that must hold over the whole range of a value, by
    /// `always_refused` or `split_refused`: the parts still to be chosen that feed it.
    fn refused_always(
        &mut self,
        index: usize,
        message: String,
        margin: f64,
        group: &[usize],
        from: usize,
        coupling: &Coupling,
    ) -> Ahead {
        let relations = std::sync::Arc::clone(self.model.shared_relations());
        let r = &relations[index];
        let message = self.relation_message(index, r, message);
        let later: BTreeSet<usize> = (from..group.len())
            .filter(|&d| coupling.feeding[index].contains(&group[d]))
            .collect();
        let mut own = Vec::new();
        crate::eval::fields_in(&r.lhs, &mut own);
        crate::eval::fields_in(&r.rhs, &mut own);
        Ahead {
            later,
            rejection: Rejection {
                message,
                margin,
                at: (r.source, r.span),
                own,
                relation: Some(index),
                network: false,
            },
        }
    }

    /// `always_refused` for a bound that a column with no core feeds, where that column's
    /// candidates split into a few sets that each have a core (`core_sets`; issue #1017). Every
    /// candidate is in a set, and its range holds the core of its set, so every choice of the
    /// parts holds the cores of one combination of sets: each combination of the sets of the
    /// parts the bound needs split is propagated, and a bound that every combination puts
    /// across its limit is put across it by every choice. Asked under the conditions of
    /// `always_refused`, of the bounds it could not ask. The bounds that need the same parts
    /// split are asked together, those that need the most parts first (the bounds of a long
    /// chain, which the search judges last), and one look propagates at most
    /// `MOST_COMBINATIONS` combinations in all.
    fn split_refused(
        &mut self,
        group: &[usize],
        from: usize,
        coupling: &Coupling,
        elsewhere: &HashSet<usize>,
        cores: &[(FieldId, Option<Option<Domain>>)],
        split: &[Split],
    ) -> Option<Ahead> {
        let relations = std::sync::Arc::clone(self.model.shared_relations());
        let later: HashSet<usize> = group[from..].iter().copied().collect();
        let design = self.reads();
        let coreless: HashSet<FieldId> = cores
            .iter()
            .filter(|(_, core)| core.is_none())
            .map(|(f, _)| *f)
            .collect();
        // The columns with no core that have one in every set of their part.
        let in_sets: HashSet<FieldId> = split
            .iter()
            .flat_map(|s| {
                s.fields
                    .iter()
                    .enumerate()
                    .filter(|&(i, _)| s.sets.iter().all(|set| set[i].is_some()))
                    .map(|(_, f)| *f)
            })
            .collect();
        // The bounds, by the parts (indices into `split`) they need split.
        let mut asks: BTreeMap<Vec<usize>, Vec<usize>> = BTreeMap::new();
        for index in 0..self.original_relations {
            let r = &relations[index];
            let feeding = &coupling.feeding[index];
            if !matches!(r.op, CmpOp::Ge | CmpOp::Le)
                || self.set_aside(index)
                || !feeding.is_disjoint(elsewhere)
                || feeding.intersection(&later).nth(1).is_none()
                || relation_reads_end(r)
                || self.place_side(r).is_some()
            {
                continue;
            }
            let sources = coupling.sources(design.fields_of(index));
            let read: Vec<&FieldId> = coreless.intersection(&sources).collect();
            if read.is_empty() || !read.iter().all(|f| in_sets.contains(f)) {
                continue;
            }
            let needed: Vec<usize> = (0..split.len())
                .filter(|&k| split[k].fields.iter().any(|f| read.contains(&f)))
                .collect();
            asks.entry(needed).or_default().push(index);
        }
        let mut asks: Vec<(Vec<usize>, Vec<usize>, usize)> = asks
            .into_iter()
            .filter_map(|(needed, bounds)| {
                let combinations = needed
                    .iter()
                    .try_fold(1usize, |n, &k| n.checked_mul(split[k].sets.len()))
                    .filter(|&n| n <= MOST_COMBINATIONS)?;
                Some((needed, bounds, combinations))
            })
            .collect();
        asks.sort_by_key(|(needed, _, combinations)| {
            (std::cmp::Reverse(needed.len()), *combinations)
        });
        if asks.is_empty() || !self.network_quiet() {
            return None;
        }
        let mut left = MOST_COMBINATIONS;
        for (needed, bounds, combinations) in asks {
            if combinations > left {
                continue;
            }
            left -= combinations;
            if let Some((index, message, margin)) =
                self.refused_by_sets(&needed, bounds, cores, split)
            {
                return Some(self.refused_always(index, message, margin, group, from, coupling));
            }
        }
        None
    }

    /// The first of `bounds` that every combination of the sets of the parts `needed` (indices
    /// into `split`) puts across its limit, with its message and its smallest margin. The other
    /// parts still to be chosen take their cores. A combination whose cores conflict, or whose
    /// propagation keeps a value apart (`Solver::kept_apart`), says nothing, so nothing is
    /// refused. The model is put back as it was.
    fn refused_by_sets(
        &mut self,
        needed: &[usize],
        bounds: Vec<usize>,
        cores: &[(FieldId, Option<Option<Domain>>)],
        split: &[Split],
    ) -> Option<(usize, String, f64)> {
        let relations = std::sync::Arc::clone(self.model.shared_relations());
        // For each bound still put across by every combination so far: the end of its range
        // nearest its limit over them, the limit, and the smallest margin.
        type Nearest = Option<(f64, Quantity, f64)>;
        let mut refused: Vec<(usize, Nearest)> =
            bounds.into_iter().map(|index| (index, None)).collect();
        let mut chosen = vec![0usize; needed.len()];
        loop {
            let backup = self.model.checkpoint();
            for (field, core) in cores {
                if let Some(Some(domain)) = core {
                    self.model.set_domain(*field, domain.clone());
                }
            }
            for (&k, &set) in needed.iter().zip(&chosen) {
                for (field, core) in split[k].fields.iter().zip(&split[k].sets[set]) {
                    if let Some(Some(domain)) = core {
                        self.model.set_domain(*field, domain.clone());
                    }
                }
            }
            let kept = self.kept_apart;
            self.work += 1;
            let said = self.propagate().is_empty() && self.kept_apart == kept;
            if said {
                refused.retain_mut(|(index, nearest)| {
                    let r = &relations[*index];
                    self.work += 1;
                    let Verdict::Fail { margin, .. } = self.verify(r) else {
                        return false;
                    };
                    let (Ok(Val::Q(l)), Ok(Val::Q(v))) =
                        (eval(&self.model, &r.lhs), eval(&self.model, &r.rhs))
                    else {
                        return false;
                    };
                    let end = if r.op == CmpOp::Ge { l.lo() } else { l.hi() };
                    *nearest = Some(match nearest.take() {
                        None => (end, v, margin),
                        Some((e, w, m)) => {
                            let nearer = if r.op == CmpOp::Ge {
                                e.max(end)
                            } else {
                                e.min(end)
                            };
                            (nearer, w, m.min(margin))
                        }
                    });
                    true
                });
            }
            self.model.rollback(backup);
            if !said || refused.is_empty() {
                return None;
            }
            // The next combination of sets, the last part's first; done after the last.
            let mut at = chosen.len();
            let more = loop {
                if at == 0 {
                    break false;
                }
                at -= 1;
                chosen[at] += 1;
                if chosen[at] < split[needed[at]].sets.len() {
                    break true;
                }
                chosen[at] = 0;
            };
            if !more {
                break;
            }
        }
        let (index, nearest) = refused.into_iter().next()?;
        let (end, v, margin) = nearest?;
        let r = &relations[index];
        let (word, side, beyond) = if r.op == CmpOp::Ge {
            ("at least", "below", "lower")
        } else {
            ("at most", "above", "higher")
        };
        let end = Quantity::point(v.dim(), end).ok()?;
        let message = format!(
            "{} must always be {word} {}, but its range reaches {end} or {beyond}, which is {side} {v}",
            self.model.show_expr(&r.lhs),
            self.model.show_expr(&r.rhs),
        );
        Some((index, message, margin))
    }

    /// After the search of a group found no choice, or gave up (issue #904): the bounds that the
    /// group's parts feed and that no value of them can meet, which a search again without them
    /// may then solve (`Solver::search_beyond`). A bound is one when, with the parts not yet
    /// chosen (their columns as the design states them, whatever a catalog offers) and every
    /// range the design states that the bound is worked out from (a load's current, a rail's
    /// voltage) held at one end, no value of it meets its limit (`verify_possible`). A choice
    /// of the parts lies inside what the design states of them and the end lies inside the
    /// stated range, so what is worked out from them lies inside what is worked out here, and
    /// holds no value that meets the limit either: the bound, which must hold over the whole
    /// range, fails for every choice. A load of 0 A to 3 A on a 2 A converter is one: at 3 A its
    /// current is over the rating whatever the divider beside it draws within its window, so
    /// the divider's resistors are not the cause. A bound that only the catalog's rows miss (an
    /// IC that asks for more inductance than any row has) is not one: a part with another value
    /// would meet it, so the part is the cause and its own message names it.
    ///
    /// Each end is asked in turn, every stated range at its high end and then every one at its
    /// low end. A range is held at an end only where the design states it and nothing works it
    /// out: not a column of a part (that is the choice), not the left side of an equation or a
    /// field joined to one (a divider's current is worked out from its resistors, so its window
    /// is no range of the load's), not a range that narrows one of those (the IC's recommended
    /// divider current, which narrows the divider's), and not a current a balance worked out.
    /// Asked only while the pick-time analysis can write nothing (`network_quiet`), of a bound
    /// that no part of another group still to be chosen and no bank feeds, whose columns of the
    /// group's parts have no value or only one the design states (not a row's, put there
    /// before the search), and not of a bound on a position or one that reads the end of a
    /// range. A propagation that finds a conflict says
    /// nothing, and so does one that moves a range held at an end. The model and the solver are
    /// put back as they were.
    ///
    /// Each bound comes with what its message adds: the end of its range that every choice
    /// reaches, nearest the limit over the ends asked (`whichever parts are picked, its range
    /// reaches 15.4uA or higher`), since the picks of the search again need not come near it.
    /// With no stated range to hold, the one propagation is the parts' own.
    ///
    /// This is asked only once the search has failed, so it changes no board that builds: the
    /// bounds it finds are set aside only for a search again, and the check after the picks
    /// still judges them.
    pub(super) fn beyond_the_group(
        &mut self,
        jobs: &[Job],
        group: &[usize],
        coupling: &Coupling,
        elsewhere: &HashSet<usize>,
    ) -> Vec<(usize, String)> {
        if !self.network_quiet() {
            return Vec::new();
        }
        let relations = std::sync::Arc::clone(self.model.shared_relations());
        let members: HashSet<usize> = group.iter().copied().collect();
        let bounds: Vec<usize> = (0..self.original_relations)
            .filter(|&index| {
                let r = &relations[index];
                let feeding = &coupling.feeding[index];
                matches!(r.op, CmpOp::Ge | CmpOp::Le)
                    && !self.set_aside(index)
                    && !feeding.is_disjoint(&members)
                    && feeding.is_disjoint(elsewhere)
                    && feeding.iter().all(|&j| jobs[j].count.is_none())
                    && !relation_reads_end(r)
                    && self.place_side(r).is_none()
            })
            .collect();
        // A bound that reads a column of the group's parts that has a value the design did not
        // state (the one row that fits, put there before the search) is judged by the catalog's
        // rows, not whatever the parts are: the part is the cause, and its message says so.
        let columns: HashSet<FieldId> = group
            .iter()
            .flat_map(|&j| jobs[j].columns.iter().map(|(f, _)| *f))
            .collect();
        let design = self.reads();
        let bounds: Vec<usize> = bounds
            .into_iter()
            .filter(|&index| {
                coupling
                    .sources(design.fields_of(index))
                    .iter()
                    .filter(|f| columns.contains(f))
                    .all(|&f| {
                        let field = self.model.field(f);
                        field.domain == Domain::Unset || !field.narrowed_at.is_empty()
                    })
            })
            .collect();
        if bounds.is_empty() {
            return Vec::new();
        }
        // The fields a pick or an equation works out, and those joined to them.
        let mut worked: HashSet<FieldId> = jobs
            .iter()
            .flat_map(|job| job.columns.iter().map(|(f, _)| *f).chain(job.count))
            .collect();
        for r in relations.iter() {
            if let (CmpOp::EqEq, RExpr::Field(out)) = (&r.op, &r.lhs) {
                let mut reads = Vec::new();
                crate::eval::fields_in(&r.rhs, &mut reads);
                if !reads.is_empty() {
                    worked.insert(*out);
                }
            }
        }
        // A range that narrows such a field (`.divider.current ?= .ic.recommended_divider_current`)
        // is the field's window, not a range of the load's: held at an end it would put the
        // field there, which no choice need do.
        let narrowings: Vec<(FieldId, Vec<FieldId>)> = relations
            .iter()
            .filter_map(|r| match (&r.op, &r.lhs) {
                (CmpOp::Narrow, RExpr::Field(out)) => {
                    let mut reads = Vec::new();
                    crate::eval::fields_in(&r.rhs, &mut reads);
                    Some((*out, reads))
                }
                _ => None,
            })
            .collect();
        let equalities = self.model.equalities().to_vec();
        loop {
            let before = worked.len();
            for &(a, b) in &equalities {
                if worked.contains(&a) || worked.contains(&b) {
                    worked.insert(a);
                    worked.insert(b);
                }
            }
            for (out, reads) in &narrowings {
                if worked.contains(out) {
                    worked.extend(reads.iter().copied());
                }
            }
            if worked.len() == before {
                break;
            }
        }
        // The stated ranges the bounds are worked out from.
        let mut held: BTreeSet<FieldId> = BTreeSet::new();
        let mut seen: HashSet<FieldId> = HashSet::new();
        let mut todo: Vec<FieldId> = Vec::new();
        for &index in &bounds {
            let r = &relations[index];
            crate::eval::fields_in(&r.lhs, &mut todo);
            crate::eval::fields_in(&r.rhs, &mut todo);
        }
        while let Some(f) = todo.pop() {
            if !seen.insert(f) {
                continue;
            }
            let field = self.model.field(f);
            if !worked.contains(&f)
                && !self.derived.contains(&f)
                && !field.narrowed_at.is_empty()
                && matches!(&field.domain, Domain::Quantity(q) if !q.is_point())
            {
                held.insert(f);
            }
            todo.extend(coupling.from.get(&f).into_iter().flatten().copied());
        }
        // For each bound found: the end of its range nearest its limit that every choice
        // reaches (the greatest least value of a `<=`, the least greatest value of a `>=`), and
        // its dimension.
        let mut found: BTreeMap<usize, (f64, Dimension)> = BTreeMap::new();
        let passes: &[bool] = if held.is_empty() {
            &[true]
        } else {
            &[true, false]
        };
        for &high in passes {
            let backup = self.model.checkpoint();
            let derived = self.derived.clone();
            let mut ends: Vec<(FieldId, Domain)> = Vec::new();
            for &f in &held {
                if let Domain::Quantity(q) = &self.model.field(f).domain
                    && let Ok(end) = Quantity::point(q.dim(), if high { q.hi() } else { q.lo() })
                {
                    self.model.set_domain(f, Domain::Quantity(end));
                    ends.push((f, Domain::Quantity(end)));
                }
            }
            self.work += 1;
            let said = self.propagate().is_empty()
                && ends
                    .iter()
                    .all(|(f, end)| self.model.field(*f).domain == *end);
            if said {
                for &index in &bounds {
                    let r = &relations[index];
                    self.work += 1;
                    if !matches!(self.verify_possible(r), Verdict::Fail { .. }) {
                        continue;
                    }
                    let Ok(Val::Q(l)) = eval(&self.model, &r.lhs) else {
                        continue;
                    };
                    let ge = r.op == CmpOp::Ge;
                    let end = if ge { l.hi() } else { l.lo() };
                    found
                        .entry(index)
                        .and_modify(|(e, _)| *e = if ge { e.min(end) } else { e.max(end) })
                        .or_insert((end, l.dim()));
                }
            }
            self.derived = derived;
            self.model.rollback(backup);
        }
        found
            .into_iter()
            .filter_map(|(index, (end, dim))| {
                let end = Quantity::point(dim, end).ok()?;
                let beyond = if relations[index].op == CmpOp::Ge {
                    "lower"
                } else {
                    "higher"
                };
                Some((
                    index,
                    format!("whichever parts are picked, its range reaches {end} or {beyond}"),
                ))
            })
            .collect()
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
                || self.set_aside(index)
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
            Value::Open(_) | Value::Derated(_) | Value::Curved(_) => return None,
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
            Value::Open(_) | Value::Derated(_) | Value::Curved(_) => return None,
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

/// The candidates split into sets whose ranges share a value in every quantity column that has
/// no core over all of them because the rows' ranges share none (`cores` holds the core of each
/// column over all the candidates): each candidate goes to the first set it shares a value with
/// in each such column, in catalog order, or starts one. The cores of each set, column by column
/// (`column_core`). Empty when no column is split so, or the candidates need more than
/// `MOST_SETS` sets.
fn core_sets(
    candidates: &[&Candidate],
    keys: &[&str],
    cores: &[Option<Option<Domain>>],
) -> Vec<Vec<Option<Option<Domain>>>> {
    let ranges = |key: &str| -> Option<Vec<(f64, f64)>> {
        let mut dim = None;
        candidates
            .iter()
            .map(|c| match c.column(key) {
                Some(Value::Quantity(q)) if dim.is_none_or(|d| d == q.dim()) => {
                    dim = Some(q.dim());
                    Some((q.lo(), q.hi()))
                }
                _ => None,
            })
            .collect()
    };
    let columns: Vec<Vec<(f64, f64)>> = keys
        .iter()
        .zip(cores)
        .filter(|(_, core)| core.is_none())
        .filter_map(|(key, _)| ranges(key))
        .collect();
    if columns.is_empty() {
        return Vec::new();
    }
    // Each set: its members, and the range they all share in each split column.
    type Set<'a> = (Vec<&'a Candidate>, Vec<(f64, f64)>);
    let mut sets: Vec<Set> = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        let own: Vec<(f64, f64)> = columns.iter().map(|ranges| ranges[i]).collect();
        let joined = sets.iter_mut().find_map(|(members, shared)| {
            let narrowed: Vec<(f64, f64)> = shared
                .iter()
                .zip(&own)
                .map(|(&(l, h), &(lo, hi))| (l.max(lo), h.min(hi)))
                .collect();
            narrowed.iter().all(|(l, h)| l <= h).then(|| {
                *shared = narrowed;
                members.push(*c);
            })
        });
        if joined.is_none() {
            if sets.len() == MOST_SETS {
                return Vec::new();
            }
            sets.push((vec![*c], own));
        }
    }
    sets.iter()
        .map(|(members, _)| keys.iter().map(|key| column_core(members, key)).collect())
        .collect()
}
