//! Which parts can see each other's choices, and the groups made from them.

use std::collections::{HashMap, HashSet};

use super::{Coupling, FIELDS_REPORT_EVERY, Job, Solver, Unresolved, article, find, union};
use crate::catalog::{Candidate, Catalog, Value};
use crate::eval::fields_in;
use crate::generic::Sense;
use netsmith_check::{Domain, FieldId, InstId, InstKind, RExpr};
use netsmith_core::job;
use netsmith_lang::ast::CmpOp;

impl Solver<'_> {
    /// The fields that a pick can change: every field worked out, however indirectly, from the
    /// columns (and the count) of a part that is to be chosen.
    pub(super) fn fed_by(&self, jobs: &[Job]) -> HashSet<FieldId> {
        self.fed_from(
            jobs.iter()
                .flat_map(|job| job.columns.iter().map(|(f, _)| *f).chain(job.count)),
        )
    }

    /// `fed_by`, before the catalogs are read: the columns (and the count) of every part that
    /// has a catalog, whether or not it will be found.
    pub(super) fn fed_at_start(&self) -> HashSet<FieldId> {
        let roots: Vec<FieldId> = self
            .model
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Part)
            .flat_map(|(id, i)| {
                i.catalog
                    .iter()
                    .flat_map(|c| c.fields.iter().copied())
                    .chain(self.model.bank_count(id))
                    .collect::<Vec<_>>()
            })
            .collect();
        self.fed_from(roots.into_iter())
    }

    fn fed_from(&self, roots: impl Iterator<Item = FieldId>) -> HashSet<FieldId> {
        let links = self.links();
        let mut fed: HashSet<FieldId> = HashSet::new();
        let mut todo: Vec<FieldId> = Vec::new();
        for f in roots {
            if fed.insert(f) {
                todo.push(f);
            }
        }
        while let Some(f) = todo.pop() {
            for &g in links.worked_out(f) {
                if fed.insert(g) {
                    todo.push(g);
                }
            }
        }
        fed
    }

    /// The group to search next: the first, in path order, that is not waiting for another
    /// group (see `waits_for`). Every relation is verified again once all parts are
    /// picked, so the order changes no verdict, only whether a value was known when it was
    /// needed.
    ///
    /// If every group waits (they wait on each other), the first goes first and fails or
    /// not as it would have: a field that no pick can set is the design's to state.
    pub(super) fn next_group(
        &self,
        jobs: &[Job],
        pending: &[(usize, Vec<usize>)],
        eligible: usize,
    ) -> usize {
        (0..eligible)
            .find(|&i| {
                let others: Vec<usize> = pending
                    .iter()
                    .enumerate()
                    .filter(|&(k, _)| k != i)
                    .flat_map(|(_, (_, group))| group.iter().copied())
                    .collect();
                !self.waits_for(jobs, &pending[i].1, &others)
            })
            .unwrap_or(0)
    }

    /// The parts of a group in the order they are searched: path order, except that a part
    /// that waits for another part of the group (see `waits_for`) comes after it.
    pub(super) fn order_group(&self, jobs: &[Job], group: &[usize]) -> Vec<usize> {
        let mut left = group.to_vec();
        let mut ordered = Vec::with_capacity(group.len());
        while !left.is_empty() {
            let next = (0..left.len())
                .find(|&i| {
                    let others: Vec<usize> = left
                        .iter()
                        .enumerate()
                        .filter(|&(k, _)| k != i)
                        .map(|(_, &j)| j)
                        .collect();
                    !self.waits_for(jobs, &left[i..=i], &others)
                })
                .unwrap_or(0);
            ordered.push(left.remove(next));
        }
        ordered
    }

    /// Whether the parts `mine` should be searched after the parts `others`: a column of one
    /// of them needs a field that no pick has set yet (a capacitor's effective capacitance
    /// needs its bias, which a diode ahead of it on the rail sets with its forward voltage)
    /// and a column of one of the others can set that field. Only a pick decides such a
    /// value, so a part searched first would be refused for want of it, and the same board
    /// would build or not by the names of its parts.
    pub(super) fn waits_for(&self, jobs: &[Job], mine: &[usize], others: &[usize]) -> bool {
        let wanted: Vec<FieldId> = mine.iter().flat_map(|&j| self.waits_on(&jobs[j])).collect();
        if wanted.is_empty() {
            return false;
        }
        let reach = self.feeders(&wanted);
        others
            .iter()
            .any(|&j| jobs[j].columns.iter().any(|(f, _)| reach.contains(f)))
    }

    /// The bias of the derated columns of a job that the design asks for and that has no value:
    /// what a candidate cannot be judged without.
    pub(super) fn unset_biases(&self, job: &Job) -> Vec<FieldId> {
        let mut wanted: Vec<FieldId> = Vec::new();
        for (f, key) in &job.columns {
            let bias = job
                .catalog
                .candidates
                .iter()
                .filter_map(|c| c.column(key))
                .find_map(|v| match v {
                    Value::Derated(d) => Some(d.against.clone()),
                    _ => None,
                });
            if let Some(against) = bias
                && self.asked(*f)
            {
                let path = format!("{}.{against}", self.model.path_of(job.part));
                if let Some(bias) = self.model.field_at(&path)
                    && matches!(self.domain(bias), Domain::Unset)
                {
                    wanted.push(bias);
                }
            }
        }
        wanted
    }

    /// The fields without a value that a job's columns need before a candidate can be judged:
    /// the bias of a derated column the design asks for, and what the rating a generic part
    /// leaves open is bounded by.
    fn waits_on(&self, job: &Job) -> Vec<FieldId> {
        let mut wanted: Vec<FieldId> = self.unset_biases(job);
        for (f, key) in &job.columns {
            let mut candidates = job.catalog.candidates.iter().filter_map(|c| c.column(key));
            let open = candidates.any(|v| matches!(v, Value::Open(Sense::AtLeast | Sense::AtMost)));
            if open && matches!(self.domain(*f), Domain::Unset) {
                for r in self.model.relations() {
                    let other = match (&r.op, &r.lhs, &r.rhs) {
                        (CmpOp::Ge, RExpr::Field(g), other)
                        | (CmpOp::Le, other, RExpr::Field(g))
                            if g == f =>
                        {
                            other
                        }
                        _ => continue,
                    };
                    let mut read = Vec::new();
                    fields_in(other, &mut read);
                    wanted.extend(read.into_iter().filter(|&g| {
                        matches!(self.domain(g), Domain::Unset)
                            && !self.model.is_inside(self.model.field(g).owner, job.part)
                    }));
                }
            }
        }
        wanted.sort();
        wanted.dedup();
        wanted
    }

    /// Every field without a value that something in `wanted` is worked out from, however
    /// indirectly, and `wanted` itself: what propagation would carry a pick's value along.
    /// A field that has a value is not followed, since nothing waits for it.
    fn feeders(&self, wanted: &[FieldId]) -> HashSet<FieldId> {
        let mut from: HashMap<FieldId, Vec<FieldId>> = HashMap::new();
        for &(a, b) in self.model.equalities() {
            from.entry(a).or_default().push(b);
            from.entry(b).or_default().push(a);
        }
        for r in self.model.relations() {
            if let (CmpOp::EqEq | CmpOp::Narrow, RExpr::Field(out)) = (&r.op, &r.lhs) {
                let mut reads = Vec::new();
                fields_in(&r.rhs, &mut reads);
                from.entry(*out).or_default().extend(reads);
            }
        }
        for b in self.model.balances().iter().filter(|b| !b.returns) {
            for d in &b.draws {
                from.entry(d.field)
                    .or_default()
                    .extend(b.draws.iter().map(|o| o.field).filter(|&o| o != d.field));
            }
        }
        let mut reach: HashSet<FieldId> = wanted.iter().copied().collect();
        let mut todo: Vec<FieldId> = wanted.to_vec();
        while let Some(f) = todo.pop() {
            for &g in from.get(&f).into_iter().flatten() {
                if matches!(self.domain(g), Domain::Unset) && reach.insert(g) {
                    todo.push(g);
                }
            }
        }
        reach
    }

    /// A keyed catalog (`catalog mcu by .mpn`) is not searched for a choice among rows: when
    /// several rows fit, the design names the row. If it has not, say which rows it could
    /// name. True if the key was missing and reported. Exactly one row that fits (a catalog
    /// with one row, or narrowings that leave one) needs no name: the search takes it, since
    /// there is nothing to choose. A key that no row fits is left to the search, whose message
    /// says what the design asks for.
    pub(super) fn unnamed_key(
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
        let fitting: Vec<&Candidate> = cat
            .candidates
            .iter()
            .filter(|c| self.fits(part, columns, c))
            .collect();
        if fitting.len() == 1 {
            return false;
        }
        let mut names: Vec<String> = Vec::new();
        for c in fitting {
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
    pub(super) fn coupling(&self, jobs: &[Job]) -> Coupling {
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
        // What each field is worked out from: an equation's right side for its left side, a
        // connection both ways, and the draws of a balance each from the others.
        let mut from: HashMap<FieldId, Vec<FieldId>> = HashMap::new();
        for &(a, b) in self.model.equalities() {
            from.entry(a).or_default().push(b);
            from.entry(b).or_default().push(a);
        }
        for r in self.model.relations() {
            if let (CmpOp::EqEq | CmpOp::Narrow, RExpr::Field(out)) = (&r.op, &r.lhs) {
                let mut reads = Vec::new();
                fields_in(&r.rhs, &mut reads);
                from.entry(*out).or_default().extend(reads);
            }
        }
        for b in self.model.balances().iter().filter(|b| !b.returns) {
            for d in &b.draws {
                from.entry(d.field)
                    .or_default()
                    .extend(b.draws.iter().map(|o| o.field).filter(|&o| o != d.field));
            }
        }
        // A derated column is worked out from the part's bias (ADR 0013): a row's capacitance
        // at the voltage across it. The bias is the rail's voltage, which another part's pick
        // can decide, so what reads such a column reads that too.
        for job in jobs {
            for (f, key) in &job.columns {
                let against = job
                    .catalog
                    .candidates
                    .iter()
                    .find_map(|c| match c.column(key) {
                        Some(Value::Derated(d)) => Some(d.against.clone()),
                        _ => None,
                    });
                if let Some(against) = against {
                    let path = format!("{}.{against}", self.model.path_of(job.part));
                    if let Some(bias) = self.model.field_at(&path) {
                        from.entry(*f).or_default().push(bias);
                    }
                }
            }
        }
        let mut job_of: HashMap<FieldId, usize> = HashMap::new();
        for (j, job) in jobs.iter().enumerate() {
            for (f, _) in &job.columns {
                job_of.insert(*f, j);
            }
            if let Some(count) = job.count {
                job_of.insert(count, j);
            }
        }
        let constant = self.constants(jobs);
        let feeding = self
            .model
            .relations()
            .iter()
            .map(|r| {
                let mut start = Vec::new();
                fields_in(&r.lhs, &mut start);
                fields_in(&r.rhs, &mut start);
                start.retain(|f| !constant[f.0]);
                let mut seen: HashSet<FieldId> = start.iter().copied().collect();
                let mut todo = start;
                let mut found = HashSet::new();
                while let Some(f) = todo.pop() {
                    if let Some(&j) = job_of.get(&f) {
                        found.insert(j);
                    }
                    for &g in from.get(&f).into_iter().flatten() {
                        if !constant[g.0] && seen.insert(g) {
                            todo.push(g);
                        }
                    }
                }
                found
            })
            .collect();
        Coupling {
            class,
            feeding,
            constant,
            from,
            job_of,
            fed_by_field: Default::default(),
        }
    }

    /// Which fields have a value that no choice of parts can change: the field, with the
    /// fields joined to it by `~`, has a value now (stated, or settled by the first
    /// propagation), and none of them is worked out from anything (the left side of an
    /// equation that reads a field, a term of a balance of currents) or is a column of a
    /// part still to be chosen. A rail whose voltage the board states, and that nothing
    /// derives, is such a value: the parts on it read the same range whatever they are.
    fn constants(&self, jobs: &[Job]) -> Vec<bool> {
        let size = self.model.fields().map(|(f, _)| f.0 + 1).max().unwrap_or(0);
        let mut parent: Vec<usize> = (0..size).collect();
        for &(a, b) in self.model.equalities() {
            union(&mut parent, a.0, b.0);
        }
        let mut derivable = vec![false; size];
        let mut valued = vec![false; size];
        for r in self.model.relations() {
            if let (CmpOp::EqEq | CmpOp::Narrow, RExpr::Field(out)) = (&r.op, &r.lhs) {
                let mut reads = Vec::new();
                fields_in(&r.rhs, &mut reads);
                if !reads.is_empty() {
                    derivable[find(&mut parent, out.0)] = true;
                }
            }
        }
        for b in self.model.balances().iter().filter(|b| !b.returns) {
            for d in &b.draws {
                derivable[find(&mut parent, d.field.0)] = true;
            }
        }
        for job in jobs {
            for (f, _) in &job.columns {
                derivable[find(&mut parent, f.0)] = true;
            }
            if let Some(count) = job.count {
                derivable[find(&mut parent, count.0)] = true;
            }
        }
        for (id, field) in self.model.fields() {
            if field.domain != Domain::Unset {
                valued[find(&mut parent, id.0)] = true;
            }
        }
        (0..size)
            .map(|f| {
                let root = find(&mut parent, f);
                valued[root] && !derivable[root]
            })
            .collect()
    }

    /// Split the parts to pick into groups that share a relation. Parts in different
    /// groups cannot affect each other, so each group is searched on its own: one part
    /// with no valid choice does not undo the picks of parts unrelated to it.
    ///
    /// With `constant` (see `Solver::constants`), a field whose value no pick changes joins
    /// nothing: two parts that read the same stated rail, and have no other value in common,
    /// are two groups (ADR 0057).
    pub(super) fn group_jobs(&self, jobs: &[Job], constant: Option<&[bool]>) -> Vec<Vec<usize>> {
        let fixed = |f: FieldId| constant.is_some_and(|c| c[f.0]);
        // Fields joined by a connection hold one value, so they count as one.
        let n_fields = self.model.fields().count();
        let mut fparent: Vec<usize> = (0..n_fields).collect();
        for (a, b) in self.model.equalities() {
            union(&mut fparent, a.0, b.0);
        }
        let class = |f: FieldId, fparent: &mut Vec<usize>| find(fparent, f.0);

        // The jobs that each class of values is inside of: of a field, its owner and every
        // instance around the owner is a part to pick or is not.
        let job_of: HashMap<InstId, usize> = jobs
            .iter()
            .enumerate()
            .map(|(j, job)| (job.part, j))
            .collect();
        let mut jobs_of_class: HashMap<usize, Vec<usize>> = HashMap::new();
        for (id, field) in self.model.fields() {
            job::checkpoint();
            if id.0 % FIELDS_REPORT_EVERY == 0 {
                job::report("fields scanned", id.0 as u64, Some(n_fields as u64));
            }
            if fixed(id) {
                continue;
            }
            let class = class(id, &mut fparent);
            let mut at = Some(field.owner);
            while let Some(inst) = at {
                if let Some(&j) = job_of.get(&inst) {
                    jobs_of_class.entry(class).or_default().push(j);
                }
                at = self.model.instance(inst).parent;
            }
        }
        for list in jobs_of_class.values_mut() {
            list.sort_unstable();
            list.dedup();
        }

        let mut jparent: Vec<usize> = (0..jobs.len()).collect();
        for r in self.model.relations() {
            let mut fields = Vec::new();
            fields_in(&r.lhs, &mut fields);
            fields_in(&r.rhs, &mut fields);
            fields.retain(|&f| !fixed(f));
            let mut touched: Vec<usize> = fields
                .iter()
                .filter_map(|&f| jobs_of_class.get(&class(f, &mut fparent)))
                .flatten()
                .copied()
                .collect();
            touched.sort_unstable();
            touched.dedup();
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
}
