//! Picking the parts: the order of the groups and the second search of a group that failed.

use std::collections::HashSet;

use super::{Coupling, Job, Mark, Pick, Solver, Step, Unresolved};
use netsmith_check::{Domain, FieldId, FieldKind, InstId, InstKind};
use netsmith_core::job;

use crate::generic::Series;

impl Solver<'_> {
    pub(super) fn pick_parts(&mut self) {
        let mut parts: Vec<(String, InstId)> = self
            .model
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Part && i.catalog.is_some())
            .map(|(id, _)| (self.model.path_of(id), id))
            .collect();
        parts.sort_by(|a, b| a.0.cmp(&b.0));

        let mut jobs: Vec<Job> = Vec::new();
        // What `Catalogs::resolve` gave for a catalog and its columns: the same for every part
        // that is of the type, so it is worked out once (it makes the generic parts, which
        // takes longer than the search of a part with a few rows).
        type Kinds = Vec<(String, FieldKind)>;
        type Resolved = Option<Result<crate::catalog::Catalog, String>>;
        /// The catalog's name, its columns, the optional ones and the series offered.
        type Key = (String, Kinds, Vec<String>, Vec<Series>);
        let mut resolved_before: Vec<(Key, Resolved)> = Vec::new();
        let to_prepare = parts.len() as u64;
        for (n, (path, part)) in parts.into_iter().enumerate() {
            job::checkpoint();
            job::report("parts prepared", n as u64, Some(to_prepare));
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
            // The columns the statement marks `optional` (ADR 0078), by their key.
            let optional: Vec<String> = columns
                .iter()
                .filter(|(f, _)| catalog.optional.contains(f))
                .map(|(_, key)| key.clone())
                .collect();
            // The series its definition offers (`#[series(...)]`, ADR 0123); the checker has
            // refused a name that is not one.
            let offered: Vec<Series> = self
                .model
                .instance(part)
                .marks
                .series
                .iter()
                .filter_map(|s| Series::parse(s))
                .collect();
            let key: Key = (
                catalog.name.clone(),
                kinds.clone(),
                optional.clone(),
                offered.clone(),
            );
            let found = match resolved_before.iter().find(|(k, _)| *k == key) {
                Some((_, found)) => found.clone(),
                None => {
                    let found = self
                        .catalogs
                        .resolve(&catalog.name, &kinds, &optional, &offered);
                    resolved_before.push((key, found.clone()));
                    found
                }
            };
            let resolved = found.map(|found| {
                found.and_then(|cat| self.check_enum_cells(&catalog.name, &columns, cat))
            });
            match resolved {
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
                Some(Ok(cat))
                    if self.unknown_package(part, &path, &catalog.name, &columns, &cat) => {}
                Some(Ok(mut cat)) => {
                    // A part held to one row (`Catalogs::with_pin`) is offered only that row.
                    if let Some(row) = self.catalogs.pinned(&path) {
                        cat.candidates
                            .retain(|c| c.generic.is_none() && c.name == row);
                    }
                    if !self.hold_to_lock(part, &path, &mut cat) {
                        continue;
                    }
                    let used = columns.iter().map(|(f, _)| self.mentioned(*f)).collect();
                    let count = self.model.bank_count(part);
                    jobs.push(Job {
                        part,
                        path,
                        columns,
                        used,
                        catalog: cat,
                        count,
                    });
                }
            }
        }

        self.to_choose = jobs.len();
        self.chosen_before = 0;
        job::report("parts chosen", 0, Some(self.to_choose as u64));
        // A regulator's load is enclosed from what the loads on its output state (ADR 0076,
        // amendment 11), so the parts in front of it are picked against a current.
        self.enclose_regulator_loads(&jobs);
        // A circle that a pick waits on is started from the network's own bounds (ADR 0076).
        self.dc_start(&jobs, false);
        for c in self.propagate() {
            self.report(c.source, c.span, c.message);
        }
        self.fed = self.fed_by(&jobs);
        self.surveys = self.survey_picks(&jobs);
        let coupling = self.coupling(&jobs);
        // A group waits while a value it needs is still to come from another group's picks
        // (see `next_group`), so the groups are not always searched in path order. The picks
        // are listed in path order all the same, so the order never shows.
        // The groups, and each of them split into the parts that share no value that a pick
        // can change (ADR 0057). A component keeps its group's place in the listing of picks.
        let whole = self.group_jobs(&jobs, None);
        let mut pending: Vec<(usize, Vec<usize>)> = Vec::new();
        for component in self.group_jobs(&jobs, Some(&coupling.constant)) {
            let position = whole
                .iter()
                .position(|g| g.contains(&component[0]))
                .expect("a component is part of a group");
            pending.push((position, component));
        }
        pending.sort_by_key(|(position, group)| (*position, group[0]));
        // A part that reads a voltage that the start could not give, and the parts of its
        // component, are chosen after the others (ADR 0076, amendment 7): the network is solved
        // again with the others picked, and the parts that read its results are picked after it.
        // A board with no such pick is searched in one go, as it was.
        let waiting = self.waiting_jobs(&jobs);
        let (late, early): (Vec<_>, Vec<_>) = pending
            .iter()
            .cloned()
            .partition(|(_, group)| group.iter().any(|j| waiting.contains(j)));
        let mut steps: Vec<Step> = Vec::new();
        // Jobs that were searched together again after a failure, once: the second search is not
        // started again for them.
        let mut joined: HashSet<usize> = HashSet::new();
        if late.is_empty() || early.is_empty() {
            self.run_groups(&mut jobs, &coupling, pending, &[], &mut steps, &mut joined);
        } else {
            self.run_early(&mut jobs, &coupling, early, &late, &mut steps, &mut joined);
            self.dc_start(&jobs, true);
            for c in self.propagate() {
                self.report(c.source, c.span, c.message);
            }
            // The groups after the start are searched on their own: a retry never goes back to a
            // step before it, whose model the start has since written to.
            let mut after: Vec<Step> = Vec::new();
            let mut joined_after: HashSet<usize> = HashSet::new();
            self.run_groups(
                &mut jobs,
                &coupling,
                late,
                &[],
                &mut after,
                &mut joined_after,
            );
            steps.extend(after);
        }
        let mut picked: Vec<(usize, Vec<Pick>)> = steps
            .into_iter()
            .map(|step| (step.position, step.picks))
            .collect();
        picked.sort_by_key(|(position, _)| *position);
        // The picks of the components of one group are listed in the order that one search of
        // the whole group would have made them.
        let mut listed: Vec<Pick> = Vec::new();
        let mut rest = picked.into_iter().peekable();
        while let Some((position, mut picks)) = rest.next() {
            let mut parts_of_group = false;
            while let Some((_, more)) = rest.next_if(|(next, _)| *next == position) {
                picks.extend(more);
                parts_of_group = true;
            }
            if parts_of_group {
                let order = self.order_group(&jobs, &whole[position]);
                picks.sort_by_key(|p| {
                    order
                        .iter()
                        .position(|&j| jobs[j].part == p.part)
                        .unwrap_or(usize::MAX)
                });
            }
            listed.extend(picks);
        }
        self.picks.extend(listed);
    }

    /// Search the groups of `pending` one after another, each once the groups it waits for are
    /// done (see `next_group`), and keep each step. The groups of `later` are not searched here,
    /// but a group still waits for what one of them can set.
    fn run_groups(
        &mut self,
        jobs: &mut [Job],
        coupling: &Coupling,
        mut pending: Vec<(usize, Vec<usize>)>,
        later: &[(usize, Vec<usize>)],
        steps: &mut Vec<Step>,
        joined: &mut HashSet<usize>,
    ) {
        while !pending.is_empty() {
            let next = {
                let everyone: Vec<(usize, Vec<usize>)> = pending
                    .iter()
                    .cloned()
                    .chain(later.iter().cloned())
                    .collect();
                self.next_group(jobs, &everyone, pending.len())
            };
            let (position, group) = pending.remove(next);
            let mark = self.mark();
            let elsewhere: HashSet<usize> = pending
                .iter()
                .chain(later.iter())
                .flat_map(|(_, g)| g.iter().copied())
                .collect();
            self.solve_group(jobs, &group, coupling, elsewhere);
            self.chosen_before += group.len();
            job::report(
                "parts chosen",
                self.chosen_before.min(self.to_choose) as u64,
                Some(self.to_choose as u64),
            );
            let picks = self.picks.split_off(mark.picks);
            let step = Step {
                position,
                group,
                mark,
                picks,
            };
            if self.failed(jobs, &step) {
                self.search_again_together(jobs, coupling, steps, step, &pending, joined);
            } else {
                steps.push(step);
            }
        }
    }

    /// Search the groups that no pick waits on. A regulator's input has no voltage yet, so a part
    /// in series with it (a fuse, a shunt, a bead) cannot be judged on the current it carries.
    /// The groups are first searched under the window that a valid design has the input in (the
    /// seed a board used to write), then the window is taken out again and they are searched
    /// once more from the model as it was, each part from the one row it was given: nothing the
    /// window gave is left in the model, and the passes after judge the choices on the circuit.
    fn run_early(
        &mut self,
        jobs: &mut [Job],
        coupling: &Coupling,
        early: Vec<(usize, Vec<usize>)>,
        later: &[(usize, Vec<usize>)],
        steps: &mut Vec<Step>,
        joined: &mut HashSet<usize>,
    ) {
        let windows = self.assumed_windows();
        if windows.is_empty() {
            self.run_groups(jobs, coupling, early, later, steps, joined);
            return;
        }
        let mark = self.mark();
        let chosen_before = self.chosen_before;
        for (field, window) in windows {
            self.set_field(field, Domain::Quantity(window));
        }
        let _ = self.propagate();
        let mut trial: Vec<Step> = Vec::new();
        let mut trial_joined: HashSet<usize> = HashSet::new();
        self.run_groups(
            jobs,
            coupling,
            early.clone(),
            later,
            &mut trial,
            &mut trial_joined,
        );
        let chosen: Vec<(InstId, String)> = trial
            .iter()
            .flat_map(|s| s.picks.iter())
            .map(|p| (p.part, p.candidate.clone()))
            .collect();
        let parts: Vec<usize> = early.iter().flat_map(|(_, g)| g.iter().copied()).collect();
        let complete = parts
            .iter()
            .all(|&j| chosen.iter().any(|(part, _)| *part == jobs[j].part));
        self.restore(&mark);
        self.chosen_before = chosen_before;
        if complete {
            for &j in &parts {
                if let Some((_, name)) = chosen.iter().find(|(part, _)| *part == jobs[j].part) {
                    jobs[j].catalog.candidates.retain(|c| &c.name == name);
                }
            }
        }
        self.run_groups(jobs, coupling, early, later, steps, joined);
    }

    /// What `search_again_together` needs to put the solver back as it was before a group.
    pub(super) fn mark(&mut self) -> Mark {
        Mark {
            model: self.model.bookmark(),
            diagnostics: self.diagnostics.len(),
            seen: self.seen.clone(),
            picks: self.picks.len(),
            unresolved: self.unresolved.len(),
            refused_counts: self.refused_counts.clone(),
            derived: self.derived.clone(),
        }
    }

    pub(super) fn restore(&mut self, mark: &Mark) {
        self.model.go_back_to(&mark.model);
        self.diagnostics.truncate(mark.diagnostics);
        self.seen = mark.seen.clone();
        self.picks.truncate(mark.picks);
        self.unresolved.truncate(mark.unresolved);
        self.refused_counts = mark.refused_counts.clone();
        self.derived = mark.derived.clone();
    }

    /// Whether a part of the step's group has no pick.
    fn failed(&self, jobs: &[Job], step: &Step) -> bool {
        step.group.iter().any(|&j| {
            self.unresolved[step.mark.unresolved..]
                .iter()
                .any(|u| u.part == jobs[j].part)
        })
    }

    /// Whether a relation reads values that parts of both groups decide: a bound between two
    /// groups that no equation joins, such as a window that one group's value must stay
    /// inside and another group's value sets.
    fn share_a_relation(coupling: &Coupling, a: &[usize], b: &[usize]) -> bool {
        coupling
            .feeding
            .iter()
            .any(|f| a.iter().any(|j| f.contains(j)) && b.iter().any(|j| f.contains(j)))
    }

    /// A group failed, and an earlier group shares a relation with it (a bound that reads a
    /// value of each). The earlier group was searched while that relation waited for the parts
    /// that feed it, so its first fit may leave nothing for this one; and which of two such
    /// groups comes first is decided by the path order of their names. So the groups are
    /// searched again with the failed one first: the others are then judged by the relation
    /// against its picks, as it was judged against theirs. The board is the same whatever the
    /// instances are called, which was not so before. If that does not find parts for every
    /// group, the solver is put back as the first failure left it, and its message stands.
    fn search_again_together(
        &mut self,
        jobs: &[Job],
        coupling: &Coupling,
        steps: &mut Vec<Step>,
        failed: Step,
        pending: &[(usize, Vec<usize>)],
        joined: &mut HashSet<usize>,
    ) {
        let linked: Vec<usize> = (0..steps.len())
            .filter(|&i| {
                !steps[i].group.iter().any(|j| joined.contains(j))
                    && Self::share_a_relation(coupling, &steps[i].group, &failed.group)
            })
            .collect();
        if linked.is_empty() || failed.group.iter().any(|j| joined.contains(j)) {
            steps.push(failed);
            return;
        }
        let first = linked[0];
        // The groups from the first linked one on, with the failed one ahead of them: the order
        // of the second attempt. Those the failed group shares nothing with keep the order they
        // had, after the linked ones.
        let order: Vec<(usize, Vec<usize>)> =
            std::iter::once((failed.position, failed.group.clone()))
                .chain(
                    linked
                        .iter()
                        .map(|&i| (steps[i].position, steps[i].group.clone())),
                )
                .chain(
                    (first..steps.len())
                        .filter(|i| !linked.contains(i))
                        .map(|i| (steps[i].position, steps[i].group.clone())),
                )
                .collect();
        // What the first failure left, which a failed retry puts back whole: `restore` cuts the
        // lists of diagnostics and unresolved parts at the length of a mark, and the retry may
        // add its own after that.
        let left_as_failed = self.mark();
        let first_diagnostics = self.diagnostics.clone();
        let first_unresolved = self.unresolved.clone();
        self.restore(&steps[first].mark.clone());
        let mut new_steps: Vec<Step> = Vec::new();
        let mut ok = true;
        for (n, (position, group)) in order.iter().cloned().enumerate() {
            let mark = self.mark();
            let elsewhere: HashSet<usize> = pending
                .iter()
                .map(|(_, g)| g)
                .chain(order[n + 1..].iter().map(|(_, g)| g))
                .flat_map(|g| g.iter().copied())
                .collect();
            self.solve_group(jobs, &group, coupling, elsewhere);
            let picks = self.picks.split_off(mark.picks);
            let step = Step {
                position,
                group,
                mark,
                picks,
            };
            if self.failed(jobs, &step) {
                ok = false;
                break;
            }
            new_steps.push(step);
        }
        if ok {
            steps.truncate(first);
            joined.extend(order.iter().flat_map(|(_, g)| g.iter().copied()));
            steps.extend(new_steps);
        } else {
            self.restore(&left_as_failed);
            self.diagnostics = first_diagnostics;
            self.unresolved = first_unresolved;
            steps.push(failed);
        }
    }
}
