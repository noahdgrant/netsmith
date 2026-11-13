//! The search of one group of parts: depth first, with backjumping.

use std::collections::{BTreeSet, HashSet};

use super::{
    Accepted, Closest, Coupling, Failure, Job, MAX_BANK, Outcome, Pick, Search, Solver, Unresolved,
    choice_name, dcpass, give_up_message, nearer,
};
use crate::catalog::Candidate;
use netsmith_check::{Domain, FieldId};
use netsmith_core::job;

impl Solver<'_> {
    /// Find a choice for every part in a group that satisfies every relation together,
    /// backing up when a later part has nothing that works. The first solution in catalog
    /// order (parts in path order) is the one used.
    pub(super) fn solve_group(
        &mut self,
        jobs: &[Job],
        group: &[usize],
        coupling: &Coupling,
        elsewhere: HashSet<usize>,
    ) {
        let ordered = self.order_group(jobs, group);
        let group = &ordered[..];
        self.network = dcpass::NetworkTrial::of(group.iter().map(|&j| jobs[j].part).collect());
        let mut search = Search {
            elsewhere,
            work_from: self.work,
            ..Search::default()
        };
        // A part with no candidate that fits what the design already says cannot be chosen
        // whatever the others are, so that is the cause to report. Searching first would
        // blame whichever earlier part's choices failed on the way to it.
        // A part that waits for another of the group is not starved yet: what it needs
        // is set by that part's pick, and the search finds it out if it still has nothing.
        let starved = group.iter().copied().find(|&j| {
            let job = &jobs[j];
            let others: Vec<usize> = group.iter().copied().filter(|&o| o != j).collect();
            !job.catalog
                .candidates
                .iter()
                .any(|c| self.fits(job.part, &job.columns, c))
                && !self.waits_for(jobs, &[j], &others)
        });
        // A part that every candidate of fails whatever the others are is the cause: say so
        // now, and do not search the combinations of the parts before it (ADR 0057).
        let hopeless = if starved.is_none() && group.len() > 1 {
            self.hopeless(jobs, group, coupling, &search.elsewhere)
        } else {
            None
        };
        let culprit = hopeless.as_ref().map(|h| h.0);
        if let Some((_, closest, fits)) = hopeless {
            search.closest = Some(closest);
            search.first_level_fits = fits;
        }
        let outcome = if starved.is_none() && culprit.is_none() {
            Some(self.search_group(jobs, group, &mut search, coupling))
        } else {
            None
        };
        let solved = matches!(outcome, Some(Outcome::Solved));
        // What the search learned: the parts that no choice of candidates can serve together,
        // when that is fewer than the group.
        let learned = match outcome {
            Some(Outcome::Failed(f)) if f.core.len() < group.len() && f.closest.is_some() => {
                Some(f)
            }
            _ => None,
        };
        self.trials += search.trials;
        if solved {
            self.picks.extend(search.picks);
            return;
        }

        let names: Vec<String> = group
            .iter()
            .map(|&j| format!("`{}`", jobs[j].path))
            .collect();
        let first = &jobs[culprit.unwrap_or(group[0])];
        let part = self.model.instance(first.part);
        let (part_source, part_span) = part.at();

        if search.gave_up {
            let message = give_up_message(search.trials, &names, search.out_of_work);
            self.report(part_source, part_span, message);
        } else if let Some(c) = learned
            .as_ref()
            .and_then(|f| f.closest.clone())
            .or_else(|| search.closest.take())
        {
            let (source, span) = c.at;
            // The parts of the group that the failure is made of, when the search learned that
            // it is fewer than all of them, and the closest choice for those parts only.
            let (involved, assignment): (Vec<usize>, Vec<String>) = match &learned {
                Some(f) => (
                    f.core.iter().map(|&d| group[d]).collect(),
                    c.assignment
                        .iter()
                        .zip(&c.depths)
                        .filter(|&(_, d)| f.core.contains(d))
                        .map(|(a, _)| a.clone())
                        .collect(),
                ),
                None => (group.to_vec(), c.assignment.clone()),
            };
            let involved_names: Vec<String> = involved
                .iter()
                .map(|&j| format!("`{}`", jobs[j].path))
                .collect();
            let message = if learned.is_some() && involved.len() == 1 {
                format!(
                    "no `{}` candidate satisfies `{}` whatever the other parts of the group are: the closest, {}, fails because {}",
                    jobs[involved[0]].catalog.name,
                    jobs[involved[0]].path,
                    assignment.join(" and "),
                    c.message
                )
            } else if learned.is_some() {
                format!(
                    "no combination of candidates satisfies {} together, whatever the other parts of the group are: the closest, {}, fails because {}",
                    involved_names.join(" and "),
                    assignment.join(" and "),
                    c.message
                )
            } else if group.len() == 1 || culprit.is_some() {
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
            let notes: Vec<String> = involved
                .iter()
                .flat_map(|&j| {
                    [
                        self.derating_note(&jobs[j]),
                        self.package_note(&jobs[j]),
                        self.environment_note(&jobs[j]),
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
                .map(|(f, key)| (*f, key.as_str()))
                .chain(job.count.map(|f| (f, "count")))
                .filter_map(|(f, key)| match self.domain(f) {
                    Domain::Unset => None,
                    d => Some(format!("{key} within {}", self.model.show_value(f, &d))),
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
                self.environment_note(job),
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
        // A part that the lock holds to one row was offered no other: the failure above may be the
        // lock's, and the message says how to look at it.
        for &j in group {
            let job = &jobs[j];
            if self.catalogs.pinned(&job.path).is_some() {
                continue;
            }
            if let Some(chosen) = self.chosen.get(&job.path).cloned() {
                let (source, span) = self.model.instance(job.part).at();
                let file = self.picks_file();
                self.report(
                    source,
                    span,
                    format!(
                        "`{}` is picked as the row with {} of the `{}` catalog by {file}, and no other row was tried for it: if the failure above is about it, change the design so that the row fits, or change the entry in {file}",
                        job.path,
                        chosen.number(),
                        chosen.catalog
                    ),
                );
                continue;
            }
            if let Some(locked) = self.locked.get(&job.path).cloned() {
                let (source, span) = self.model.instance(job.part).at();
                let file = self.lock_file();
                self.report(
                    source,
                    span,
                    format!(
                        "`{}` is locked to the row `{}` of the `{}` catalog by {file}, and no other row was tried for it: if the failure above is about it, change the design so that the row fits, or delete {file} to pick again",
                        job.path, locked.row, locked.catalog
                    ),
                );
            }
        }
        for &j in group {
            if let Some(count) = jobs[j].count {
                self.refused_counts.insert(count);
            }
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

    /// The part of a group that no candidate can serve, whatever the other parts are: each
    /// candidate that fits is tried alone, and every one that fits is refused by a relation
    /// whose value only this part's choice decides (no other part's column is among what it
    /// is worked out from, `Coupling::feeding`). The others cannot help it, so searching them
    /// first would only repeat the failure. With its place in the group, the candidate that
    /// came nearest, and how many fitted. A part with a bank count, and a part with any
    /// candidate that passes alone or is refused for what other parts decide, is not it.
    fn hopeless(
        &mut self,
        jobs: &[Job],
        group: &[usize],
        coupling: &Coupling,
        elsewhere: &HashSet<usize>,
    ) -> Option<(usize, Closest, usize)> {
        for &j in group {
            let job = &jobs[j];
            let fitting: Vec<&Candidate> = job
                .catalog
                .candidates
                .iter()
                .filter(|c| self.fits(job.part, &job.columns, c))
                .collect();
            if fitting.is_empty() {
                continue;
            }
            // A bank is tried at every count its range holds: a row is refused only when no
            // count of it is accepted (ADR 0052). A part with no count has the one pass.
            let counts = self.counts_of(job);
            if counts.is_empty() {
                continue;
            }
            let mut later = elsewhere.clone();
            later.extend(group.iter().copied().filter(|&o| o != j));
            let mut nearest: Option<Closest> = None;
            // A refused candidate, with the columns that the relation that refused it reads:
            // a candidate that is equal to it in those, and in every column the design uses, is
            // refused the same way. A column that only that relation reads (a bound on one
            // field, which no other field names) is not among the used ones, so the used
            // columns alone would call a row with another value there a twin.
            let mut dead: Vec<(&Candidate, Vec<&str>)> = Vec::new();
            let mut every = true;
            'candidates: for c in &fitting {
                if dead.iter().any(|(d, read)| {
                    self.interchangeable(job, d, c)
                        && read.iter().all(|k| d.column(k) == c.column(k))
                }) {
                    continue;
                }
                let mut read: Vec<&str> = Vec::new();
                for &count in &counts {
                    let checkpoint = self.model.checkpoint();
                    let bank = job.count.zip(count);
                    let outcome = self.try_candidate(
                        job.part,
                        &job.columns,
                        c,
                        bank,
                        coupling,
                        &later,
                        false,
                    );
                    self.model.rollback(checkpoint);
                    match outcome {
                        Err(r)
                            if r.relation
                                .is_some_and(|i| coupling.feeding[i].iter().all(|&k| k == j)) =>
                        {
                            read.extend(
                                job.columns
                                    .iter()
                                    .filter(|(f, _)| r.own.contains(f))
                                    .map(|(_, key)| key.as_str()),
                            );
                            if nearest.as_ref().is_none_or(|n| r.margin < n.margin) {
                                nearest = Some(Closest {
                                    depth: 0,
                                    margin: r.margin,
                                    assignment: vec![choice_name(c, count)],
                                    depths: vec![0],
                                    message: r.message,
                                    at: r.at,
                                });
                            }
                            // A refusal that reads nothing worked out from the count is the
                            // same at every count (as in `search`).
                            if let Some(cf) = job.count
                                && !coupling.classes_of(&r.own).contains(&coupling.class[cf.0])
                            {
                                break;
                            }
                        }
                        _ => {
                            every = false;
                            break 'candidates;
                        }
                    }
                }
                dead.push((c, read));
            }
            if every && let Some(closest) = nearest {
                return Some((j, closest, fitting.len()));
            }
        }
        None
    }

    /// The search of a group, from its first part. Candidates that a trial would refuse are
    /// skipped (`narrow.rs`); a group that is then solved has the picks it always had. A group
    /// that is not solved, when something could be skipped, is searched again with every candidate
    /// tried, from the state it was in, so that its message, its counts and its work are what
    /// they were.
    fn search_group(
        &mut self,
        jobs: &[Job],
        group: &[usize],
        search: &mut Search,
        coupling: &Coupling,
    ) -> Outcome {
        if !self.catalogs.narrowing() {
            return self.search(jobs, group, 0, search, coupling);
        }
        let (work, derived) = (self.work, self.derived.clone());
        let mut narrowed = Search {
            narrow: true,
            elsewhere: search.elsewhere.clone(),
            work_from: search.work_from,
            closest: search.closest.clone(),
            first_level_fits: search.first_level_fits,
            ..Search::default()
        };
        let outcome = self.search(jobs, group, 0, &mut narrowed, coupling);
        if matches!(outcome, Outcome::Solved) {
            *search = narrowed;
            return outcome;
        }
        if narrowed.pruned == 0 {
            // Nothing was left out, so this is the plain search, but for the work of the
            // questions that found nothing to leave out.
            self.work -= narrowed.probe_work;
            *search = narrowed;
            return outcome;
        }
        self.work = work;
        self.derived = derived;
        self.search(jobs, group, 0, search, coupling)
    }

    /// With the audit on (`Catalogs::with_narrowing_audit`), the trial of a candidate that was
    /// skipped is made, and must be refused: the proof that the narrowing leaves out only what
    /// would have been refused.
    fn audit_skipped(
        &mut self,
        job: &Job,
        c: &Candidate,
        coupling: &Coupling,
        later: &HashSet<usize>,
    ) {
        let (work, derived) = (self.work, self.derived.clone());
        let checkpoint = self.model.checkpoint();
        let tried = self.try_candidate(job.part, &job.columns, c, None, coupling, later, false);
        self.model.rollback(checkpoint);
        self.work = work;
        self.derived = derived;
        assert!(
            tried.is_err(),
            "the narrowing skipped `{}` for `{}`, which a trial accepts",
            c.name,
            job.path
        );
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
        if depth > search.deepest {
            search.deepest = depth;
            job::report(
                "parts chosen",
                (self.chosen_before + depth).min(self.to_choose) as u64,
                Some(self.to_choose as u64),
            );
        }
        if depth == group.len() {
            return Outcome::Solved;
        }
        let budget = self.catalogs.search_budget();
        let job = &jobs[group[depth]];
        self.work += job.catalog.candidates.len() as u64;
        let fitting: Vec<&Candidate> = job
            .catalog
            .candidates
            .iter()
            .filter(|c| self.fits(job.part, &job.columns, c))
            .collect();
        if depth == 0 {
            search.first_level_fits = fitting.len();
        }
        // Which candidates fit depends on the domains of this part's columns, which the picks
        // before it may have narrowed.
        let own_fields: Vec<FieldId> = job
            .columns
            .iter()
            .map(|(f, _)| *f)
            .chain(job.count)
            .collect();
        let mut conflict = coupling.earlier_jobs(group, depth, &coupling.fed_by(&own_fields));
        let failure = |depends: BTreeSet<usize>, core: BTreeSet<usize>, closest| Failure {
            core: core.union(&depends).copied().chain([depth]).collect(),
            depends,
            closest,
        };
        if fitting.is_empty() {
            search.no_fit.get_or_insert(group[depth]);
            return Outcome::Failed(failure(conflict, BTreeSet::new(), None));
        }

        // The whole numbers of a bank's count that the design leaves open, fewest first (ADR
        // 0052); one pass with no count for a part that has none.
        let counts = self.counts_of(job);
        if counts.is_empty() {
            search.no_fit.get_or_insert(group[depth]);
            return Outcome::Failed(failure(conflict, BTreeSet::new(), None));
        }

        // The parts still to be chosen after this one, here or in another group: a value
        // that one of them feeds is not worked out yet, and a bound on it cannot be judged.
        let mut later = search.elsewhere.clone();
        later.extend(group[depth + 1..].iter().copied());

        // The candidates that a trial would refuse, found without trying each (`narrow.rs`).
        // Only for a part with no count to choose: a bank's refusals are tracked per count.
        let skip =
            if search.narrow && job.count.is_none() && fitting.len() > 16 && counts.len() == 1 {
                let found = self.skippable(job, &fitting, coupling, &later, search);
                if found.is_some() {
                    search.pruned += 1;
                    // What a refusal depended on is not worked out for the candidates skipped, so
                    // the failure of this part depends on every earlier pick of the group: a
                    // later part does not jump over one of them for it.
                    conflict.extend(0..depth);
                }
                found
            } else {
                None
            };

        // Set once the parts after this one failed without depending on it: the choices of
        // the parts before it that they depend on.
        let mut jump: Option<Failure> = None;
        // What the failures of this part's candidates were made of, for `Failure::core` and
        // `Failure::closest`.
        let mut core: BTreeSet<usize> = BTreeSet::new();
        let mut nearest: Option<Closest> = None;
        // Choices whose every completion failed. A later one that behaves the same fails the
        // same way, so it is not tried.
        let mut failed: Vec<(&Candidate, Option<f64>)> = Vec::new();
        // Candidates rejected for a reason that no count changes (a rating too low for the
        // rail): a larger count is not tried with them.
        let mut count_free_failure = vec![false; fitting.len()];
        // Two tiers within each count (ADR 0065, 9.6): the first takes only a candidate for which
        // every bound that waits holds over the whole range its values have, the second the
        // candidates that were left out because a bound holds only for some values, in the same
        // order. The fewest count still comes first.
        let mut deferred = vec![false; counts.len() * fitting.len()];
        for (pass, &count) in counts.iter().enumerate() {
            for tier in 0..2 {
                for (index, c) in fitting.iter().enumerate() {
                    let examined = pass * fitting.len() + index;
                    if tier == 1 && !deferred[examined] {
                        continue;
                    }
                    if count_free_failure[index] {
                        continue;
                    }
                    if skip.as_ref().is_some_and(|s| s[index]) {
                        search.skipped += 1;
                        if self.catalogs.narrowing_audit() {
                            self.audit_skipped(job, c, coupling, &later);
                        }
                        continue;
                    }
                    self.work += failed.len() as u64;
                    if failed
                        .iter()
                        .any(|&(f, n)| n == count && self.interchangeable(job, f, c))
                    {
                        continue;
                    }
                    if search.trials >= budget.trials || self.work - search.work_from >= budget.work
                    {
                        search.gave_up = true;
                        search.out_of_work = search.trials < budget.trials;
                        return Outcome::GaveUp;
                    }
                    // A trial is a rollback and a propagation: milliseconds, so one checkpoint for
                    // each is the grain a cancel is heard at.
                    job::checkpoint();
                    search.trials += 1;
                    let checkpoint = self.model.checkpoint();
                    let bank = job.count.zip(count);
                    let leaf = depth + 1 == group.len();
                    match self.try_candidate(
                        job.part,
                        &job.columns,
                        c,
                        bank,
                        coupling,
                        &later,
                        leaf,
                    ) {
                        Ok(accepted) if tier == 0 && !accepted.whole => {
                            // Not yet: a candidate that meets every waiting bound comes first.
                            self.model.rollback(checkpoint);
                            deferred[examined] = true;
                        }
                        Ok(_) if jump.is_some() => {
                            // The parts after this one would fail again, as they did.
                            self.model.rollback(checkpoint);
                            failed.push((c, count));
                        }
                        Ok(Accepted { needs, .. }) => {
                            search
                                .chosen
                                .push(format!("{} = {}", job.path, choice_name(c, count)));
                            search.picks.push(Pick {
                                part: job.part,
                                catalog: job.catalog.name.clone(),
                                candidate: c.name.clone(),
                                label: c.label.clone(),
                                lcsc: c.lcsc.clone(),
                                mpn: c.mpn.clone(),
                                library: c.library,
                                assembly: c.assembly.clone(),
                                height: c.height,
                                temp_min: c.temp_min,
                                temp_max: c.temp_max,
                                qualification: c.qualification.clone(),
                                temp_class: c.class_range(),
                                generic: c.generic.as_ref().map(|g| g.with_needs(&needs)),
                                rejected: examined,
                                count: count.map_or(1, |n| n as u32),
                            });
                            let outcome = self.search(jobs, group, depth + 1, search, coupling);
                            if let Outcome::Solved = outcome {
                                self.model.commit(checkpoint);
                                return outcome;
                            }
                            search.chosen.pop();
                            search.picks.pop();
                            self.model.rollback(checkpoint);
                            match outcome {
                                Outcome::Failed(mut below) => {
                                    if below.depends.remove(&depth) {
                                        conflict.extend(below.depends.iter().copied());
                                        core.extend(below.core.iter().copied());
                                        nearer(&mut nearest, below.closest.as_ref());
                                    } else {
                                        jump = Some(below);
                                    }
                                }
                                _ => return Outcome::GaveUp,
                            }
                            failed.push((c, count));
                        }
                        Err(r) => {
                            // `try_candidate` put the model back.
                            self.model.commit(checkpoint);
                            let mut assignment = search.chosen.clone();
                            assignment.push(format!("{} = {}", job.path, choice_name(c, count)));
                            let failure_fed_by = coupling.fed_by(&self.failure_reads(&r));
                            let rejected = Closest {
                                depth,
                                margin: r.margin,
                                assignment: if group.len() == 1 {
                                    vec![choice_name(c, count)]
                                } else {
                                    assignment
                                },
                                depths: if group.len() == 1 {
                                    vec![0]
                                } else {
                                    (0..=depth).collect()
                                },
                                message: r.message,
                                at: r.at,
                            };
                            nearer(&mut nearest, Some(&rejected));
                            nearer(&mut search.closest, Some(&rejected));
                            // The rejection was worked out from these fields, and so from the parts
                            // that their values are worked out from.
                            let reads = coupling.earlier_jobs(group, depth, &failure_fed_by);
                            conflict.extend(reads.iter().copied());
                            core.extend(reads);
                            // A value the DC analysis wrote is worked out from every pick of the
                            // group.
                            if r.network {
                                conflict.extend(0..depth);
                                core.extend(0..depth);
                            }
                            // A value that a part still to be chosen feeds is judged on the range
                            // it has, so the failure is also that part's: it is named with the
                            // rest, though no choice of it is undone.
                            core.extend(
                                (depth + 1..group.len())
                                    .filter(|&p| failure_fed_by.contains(&group[p])),
                            );
                            // A failure that reads nothing worked out from the count is the same
                            // at every count.
                            if let (Some(cf), Some(_)) = (job.count, count)
                                && !coupling.classes_of(&r.own).contains(&coupling.class[cf.0])
                            {
                                count_free_failure[index] = true;
                            }
                        }
                    }
                }
            }
        }
        Outcome::Failed(match jump {
            Some(below) => below,
            None => failure(conflict, core, nearest),
        })
    }

    /// The counts a part's search goes through, fewest first (ADR 0052): the whole numbers of
    /// its `.count` range, or the one number the design states, or a single pass with none for
    /// a part that has no count or whose count is unset (that is one part). Empty if the range
    /// holds no whole number.
    fn counts_of(&self, job: &Job) -> Vec<Option<f64>> {
        let Some(field) = job.count else {
            return vec![None];
        };
        let Domain::Quantity(q) = self.domain(field) else {
            return vec![None];
        };
        let (first, last) = (q.lo().ceil().max(1.0), q.hi().floor().min(MAX_BANK));
        let mut counts = Vec::new();
        let mut n = first;
        while n <= last {
            counts.push(Some(n));
            n += 1.0;
        }
        counts
    }

    /// Whether the design cannot tell two candidates of one part apart: they are equal in
    /// every column that a relation or a connection uses. The others (the package, the
    /// color, a capacitance nobody relates) are only matched against what the design says,
    /// which both candidates already satisfy, so they cannot change any other part's choice
    /// or any relation's verdict. A candidate that behaves like one whose every completion
    /// failed fails the same way; skipping it is what keeps a large catalog from being
    /// searched once per copy of the same electrical part.
    fn interchangeable(&self, job: &Job, a: &Candidate, b: &Candidate) -> bool {
        // The DC analysis of a leaf reads the values of the part's columns that no relation
        // names (a fuse's resistance), so two candidates are alike only when they are equal
        // in all of them.
        let every = self.network.active == Some(true);
        job.columns
            .iter()
            .zip(&job.used)
            .all(|((_, key), &used)| !(used || every) || a.column(key) == b.column(key))
    }
}
