//! Judging one candidate of a part against the design, and what a refusal says.

use std::collections::HashSet;

use super::{Accepted, Coupling, Job, Rejection, Solver, Underated, Verdict, reads};
use crate::catalog::{Candidate, Derating, DeratingError, Value};
use crate::eval::{Val, eval, fields_in, relation_reads_end};
use crate::generic::{Need, Sense};
use netsmith_check::{Checkpoint, Domain, FieldId, FieldKind, InstId, Member, RExpr, SourceId};
use netsmith_lang::Span;
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::Quantity;

impl Solver<'_> {
    /// Whether anything but the part's own constraints reads a field: a connection joins it
    /// to another field, a net sums it, or a relation also names another field. A relation
    /// that names only this field (`.effective_capacitance >= 1uF`) only judges the part that
    /// holds it.
    pub(super) fn mentioned(&self, field: FieldId) -> bool {
        self.reads().shared(field)
    }

    /// What the design reads, worked out again when the relations are not those it was worked
    /// out for.
    pub(super) fn reads(&self) -> std::rc::Rc<reads::Reads> {
        let mut cache = self.reads.borrow_mut();
        match cache.as_ref() {
            Some(r) if r.is_of(&self.model) => std::rc::Rc::clone(r),
            _ => {
                let r = std::rc::Rc::new(reads::Reads::of(&self.model));
                *cache = Some(std::rc::Rc::clone(&r));
                r
            }
        }
    }

    pub(super) fn fits(&self, part: InstId, columns: &[(FieldId, String)], c: &Candidate) -> bool {
        if !self.package_has_the_pins(part, c) {
            return false;
        }
        if self.outside_environment(part, c) {
            return false;
        }
        columns.iter().all(|(f, key)| {
            let domain = &self.model.field(*f).domain;
            match (c.column(key), domain) {
                // A row with no value there fits no design that asks for one (a rating it
                // does not state cannot be shown to be met). A design that does not mention
                // the field has no use for the value, so the row is still offered, but only
                // for a column that a catalog may leave empty (marked `optional`, ADR 0078): an empty
                // package or resistance is not a value that nothing needs.
                (None, _) => self.may_be_empty(part, *f, key) && !self.asked_by_design(*f),
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

    /// The range of temperatures the board's environment states for a part, in kelvin: the ends
    /// of `.environment.temperature` of the board it is on (ADR 0099). `None` when the board has
    /// no environment or its environment states no temperature.
    fn environment_range(&self, part: InstId) -> Option<(f64, f64)> {
        let field = self.model.environment_temperature_of(part)?;
        match &self.model.field(field).domain {
            Domain::Quantity(q) => Some((q.lo(), q.hi())),
            _ => None,
        }
    }

    /// The temperatures a part states for itself, in kelvin, each end the worse of its range
    /// (a stated `.temp_max &= 350K..360K` is read as 350 K, a stated `.temp_min` as its
    /// highest). A design that states an end speaks for the part, whatever its row says
    /// (ADR 0099), as for the height (ADR 0094).
    fn stated_temperatures(&self, part: InstId) -> (Option<f64>, Option<f64>) {
        let Some((min, max)) = self.model.temperature_range_of(part) else {
            return (None, None);
        };
        let end = |field: FieldId, hi: bool| match &self.model.field(field).domain {
            Domain::Quantity(q) => Some(if hi { q.hi() } else { q.lo() }),
            _ => None,
        };
        (end(min, true), end(max, false))
    }

    /// Whether a candidate is out of the board's environment: its own range of temperatures
    /// does not cover it, or it says it is not what the board requires of qualification.
    fn outside_environment(&self, part: InstId, c: &Candidate) -> bool {
        self.outside_temperatures(part, c) || self.unqualified(part, c)
    }

    /// Whether a candidate's own range of temperatures, where the design states none for the
    /// part, does not cover the environment of its board at an end the row states. A row that
    /// states nothing is not out: it is offered, and the design stage names it (ADR 0099).
    fn outside_temperatures(&self, part: InstId, c: &Candidate) -> bool {
        let (row_min, row_max) = c.temperatures();
        if row_min.is_none() && row_max.is_none() {
            return false;
        }
        let Some(environment) = self.environment_range(part) else {
            return false;
        };
        let (stated_min, stated_max) = self.stated_temperatures(part);
        netsmith_check::environment::Fit::of(
            environment,
            stated_min.or(row_min),
            stated_max.or(row_max),
        )
        .refused()
    }

    /// The variant of `Qualification` that a field of a part or of an environment states
    /// (`Aec`, `None`), or `None` when nothing states it.
    fn stated_variant(&self, field: FieldId) -> Option<String> {
        match &self.model.field(field).domain {
            Domain::Text(variant) => Some(variant.clone()),
            _ => None,
        }
    }

    /// Whether the board of a part requires a qualification of its parts
    /// (`.environment.qualification &= Qualification::Aec`, ADR 0099).
    fn requires_qualification(&self, part: InstId) -> bool {
        let required = self
            .model
            .environment_qualification_of(part)
            .and_then(|f| self.stated_variant(f));
        netsmith_check::environment::requires_qualification(required.as_deref())
    }

    /// Whether a candidate says it is not qualified where the board requires qualification: the
    /// design's own `.qualification` of the part, else the row's cell, is a variant that is not
    /// `Aec`. A row that says nothing is not out: it is offered, and the design stage names it,
    /// or refuses it if the design states `None` (ADR 0099), as for a range.
    fn unqualified(&self, part: InstId, c: &Candidate) -> bool {
        if c.qualification.is_none() || !self.requires_qualification(part) {
            return false;
        }
        let stated = self
            .model
            .member(part, "qualification")
            .and_then(|m| match m {
                Member::Field(f) => self.stated_variant(f),
                _ => None,
            });
        let claim = stated.or_else(|| c.qualification.clone());
        netsmith_check::environment::QualificationFit::of(claim.as_deref())
            == netsmith_check::environment::QualificationFit::NotMet
    }

    /// What to add to a message when rows were left out because the temperatures they state
    /// do not cover the board's environment, or because they say they are not qualified where
    /// the board requires it.
    pub(super) fn environment_note(&self, job: &Job) -> Option<String> {
        let mut notes: Vec<String> = Vec::new();
        if let Some((lo, hi)) = self.environment_range(job.part) {
            let out: Vec<&Candidate> = job
                .catalog
                .candidates
                .iter()
                .filter(|c| self.outside_temperatures(job.part, c))
                .collect();
            let left_out = out.len();
            // A row judged by the range of its class has said nothing of its own about an end:
            // say which, because that range is the class's and a datasheet's may be narrower or wider.
            let mut by_class: Vec<&str> = out
                .iter()
                .filter(|c| {
                    c.class_range().is_some() && (c.temp_min.is_none() || c.temp_max.is_none())
                })
                .filter_map(|c| c.class_range().map(|r| r.class))
                .collect();
            by_class.sort_unstable();
            by_class.dedup();
            let class_note = if by_class.is_empty() {
                String::new()
            } else {
                format!(
                    "; a row that states no temperature of its own is judged by the range of its dielectric class ({}), which is the class's and not its datasheet's: add `temp_min` and `temp_max` to the row to say its own",
                    by_class.join(", ")
                )
            };
            if left_out > 0 {
                notes.push(format!(
                    "{left_out} {} left out because the temperatures {} state do not cover the board's environment, `.environment.temperature` {}: a row's `temp_min` and `temp_max` are its datasheet's{class_note}",
                    if left_out == 1 { "row was" } else { "rows were" },
                    if left_out == 1 { "it" } else { "they" },
                    netsmith_check::environment::show_range(lo, hi),
                ));
            }
        }
        let left_out = job
            .catalog
            .candidates
            .iter()
            .filter(|c| self.unqualified(job.part, c))
            .count();
        if left_out > 0 {
            notes.push(format!(
                "{left_out} {} left out because {} not qualified (`None`) and the board's `.environment.qualification` requires `Aec`: a row's `qualification` is its datasheet's",
                if left_out == 1 { "row was" } else { "rows were" },
                if left_out == 1 { "it says it is" } else { "they say they are" },
            ));
        }
        (!notes.is_empty()).then(|| notes.join("; "))
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
    pub(super) fn package_note(&self, job: &Job) -> Option<String> {
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
    /// value for a column the design asks about fits no design, so a part without a rating
    /// (`max_power`) is never picked, and the message says how many rows that is.
    pub(super) fn blank_note(&self, job: &Job) -> Option<String> {
        let blank: Vec<String> = job
            .columns
            .iter()
            .filter(|(f, key)| !self.may_be_empty(job.part, *f, key) || self.asked_by_design(*f))
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

    /// Whether a row of the part's catalog may have no value in `column` and still be offered
    /// to a design that does not mention it (ADR 0055): only the columns the part's `catalog`
    /// statement marks `optional` (ADR 0078), and the ones the standard kinds had before the
    /// marker. For any other column an empty cell is a value the design needs and does not
    /// get, and the row is never offered.
    fn may_be_empty(&self, part: InstId, field: FieldId, column: &str) -> bool {
        self.model.instance(part).catalog.as_ref().is_some_and(|c| {
            c.optional.contains(&field) || crate::table::is_optional(&c.name, column, &[])
        })
    }

    /// Whether the design says anything about a field: it narrows it, a relation uses it, a
    /// connection joins it to another field, or a balance of currents sums it. A value that
    /// reaches the field by any of these routes is a requirement on it, and a row with no
    /// value for it cannot be shown to meet that.
    pub(super) fn asked(&self, field: FieldId) -> bool {
        if !matches!(self.domain(field), Domain::Unset) {
            return true;
        }
        self.reads().read(field)
    }

    /// Whether the design says anything about a field, as `asked`, but a relation that the
    /// part's own definition states does not count: `.gate_source_voltage >=
    /// .on_resistance_vgs` is the part's check of a value its row may record, not a
    /// requirement of the design on it. A row that records no value is then offered, and that
    /// relation is not evaluated for it (ADR 0092). A relation of the part that names an
    /// `optional` member counts, since the design's statement of that member is why it is in
    /// the model (ADR 0074).
    fn asked_by_design(&self, field: FieldId) -> bool {
        if !matches!(self.domain(field), Domain::Unset) {
            return true;
        }
        self.reads().read_by_design(field)
    }

    /// A candidate's value after derating, at the bias the design gives its part.
    pub(super) fn derated(&self, part: InstId, d: &Derating) -> Result<Quantity, Underated> {
        let path = format!("{}.{}", self.model.path_of(part), d.against);
        let bias = match self.model.field_at(&path).map(|f| self.domain(f)) {
            Some(Domain::Quantity(q)) => q,
            _ => return Err(Underated::NoBias(path)),
        };
        d.at(&bias).map_err(Underated::Curve)
    }

    /// What to add to a message when a derated column hides candidates: the design gave no
    /// bias, or some candidates have no curve or none that reaches the bias.
    pub(super) fn derating_note(&self, job: &Job) -> Option<String> {
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
            let generic =
                job.catalog.candidates.iter().any(|c| {
                    c.generic.is_some() && matches!(c.column(key), Some(Value::Derated(_)))
                });
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

    /// The fields a rejection was worked out from: those of the relation or conflict that failed,
    /// and, for an equation, those of the other equations on the same value, which `verify` holds
    /// it against. The relations that were judged before it and passed are not among them: a
    /// candidate that fails this one is refused whatever those say.
    pub(super) fn failure_reads(&self, r: &Rejection) -> Vec<FieldId> {
        let mut fields = r.own.clone();
        if let Some(index) = r.relation
            && let relation = &self.model.relations()[index]
            && let (CmpOp::EqEq, RExpr::Field(f)) = (&relation.op, &relation.lhs)
            && let Some(others) = self.equations_of.get(f)
        {
            for &k in others {
                let other = &self.model.relations()[k];
                fields_in(&other.lhs, &mut fields);
                fields_in(&other.rhs, &mut fields);
            }
        }
        fields
    }

    /// Try one candidate. On success its values stay in the model; on failure the model
    /// is put back exactly as it was.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_candidate(
        &mut self,
        part: InstId,
        columns: &[(FieldId, String)],
        c: &Candidate,
        bank: Option<(FieldId, f64)>,
        coupling: &Coupling,
        later: &HashSet<usize>,
        leaf: bool,
    ) -> Result<Accepted, Rejection> {
        let backup = self.model.checkpoint();
        // What the DC analysis of a leaf writes besides the model's fields (which the rollback
        // puts back).
        let saved = (leaf && self.network.active != Some(false))
            .then(|| (self.derived.clone(), self.dc_ends.clone()));
        let result =
            self.try_candidate_from(part, columns, c, bank, coupling, later, leaf, &backup);
        // A candidate that is refused leaves the model as it was.
        match result {
            Ok(_) => self.model.commit(backup),
            Err(_) => {
                self.model.rollback(backup);
                if let Some((derived, ends)) = saved {
                    self.derived = derived;
                    self.dc_ends = ends;
                }
            }
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn try_candidate_from(
        &mut self,
        part: InstId,
        columns: &[(FieldId, String)],
        c: &Candidate,
        bank: Option<(FieldId, f64)>,
        coupling: &Coupling,
        later: &HashSet<usize>,
        leaf: bool,
        backup: &Checkpoint,
    ) -> Result<Accepted, Rejection> {
        let mut needs: Vec<(String, Need)> = Vec::new();
        // A bank of this many of the row (ADR 0052): the count is as much the choice as the
        // row is.
        if let Some((field, n)) = bank
            && let FieldKind::Quantity(dim) = self.model.field(field).kind
            && let Ok(q) = Quantity::point(dim, n)
        {
            self.model.set_domain(field, Domain::Quantity(q));
        }
        // What the DC analysis read, when it wrote a value this trial judges.
        let networked: std::cell::RefCell<Option<(Vec<FieldId>, bool)>> =
            std::cell::RefCell::new(None);
        let reject = |message: String,
                      margin: f64,
                      at: (SourceId, Span),
                      mut own: Vec<FieldId>,
                      relation: Option<usize>| {
            let mut network = false;
            if let Some((reads, opaque)) = &*networked.borrow() {
                own.extend(reads.iter().copied());
                network = *opaque;
            }
            Err(Rejection {
                message,
                margin,
                at,
                own,
                relation,
                network,
            })
        };

        // The columns a generic part leaves open (ADR 0047), in column order.
        let mut open: Vec<(FieldId, &String, Sense)> = Vec::new();
        for (f, key) in columns {
            // A row with no value for a column that the design does not ask about stays
            // unset (`fits` let it through).
            let Some(value) = c.column(key) else { continue };
            let value = match value {
                Value::Quantity(q) => Domain::Quantity(*q),
                Value::Text(t) => Domain::Text(t.clone()),
                Value::Open(sense) => {
                    open.push((*f, key, *sense));
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

        // A generic part takes what the design says of the columns it leaves open, and what
        // the design says can depend on the columns it does have: the ripple of a buck
        // converter's inductor needs its inductance, and the stage asks for a saturation
        // current of the load and half that ripple. So the values that follow from the marked
        // columns are worked out first, and the demand on each open column is read after
        // them (ADR 0057). A relation's demand is taken exactly, so the relation holds with
        // nothing to spare and any row that meets the need holds it too.
        if !open.is_empty()
            && let Some(conflict) = self.propagate().into_iter().next()
        {
            return reject(
                conflict.message,
                1.0,
                (conflict.source, conflict.span),
                conflict.fields,
                None,
            );
        }
        for (f, key, sense) in open {
            let need = self.need_of(f, sense).unwrap_or(Need::Nothing);
            if let (Need::AtLeast(x) | Need::AtMost(x), FieldKind::Quantity(dim)) =
                (&need, self.model.field(f).kind)
                && let Ok(q) = Quantity::point(dim, *x)
            {
                self.model.set_domain(f, Domain::Quantity(q));
            }
            needs.push((key.clone(), need));
        }

        if let Some(conflict) = self.propagate().into_iter().next() {
            return reject(
                conflict.message,
                1.0,
                (conflict.source, conflict.span),
                conflict.fields,
                None,
            );
        }

        // The last part of a group is chosen: the network the group is in is solved with the
        // picks, and the voltage it gives a rail that nothing else gave is judged with the rest
        // (ADR 0076, amendment 10).
        if leaf && let Some(read) = self.network_trial() {
            *networked.borrow_mut() = Some(read);
            if let Some(conflict) = self.propagate().into_iter().next() {
                return reject(
                    conflict.message,
                    1.0,
                    (conflict.source, conflict.span),
                    conflict.fields,
                    None,
                );
            }
        }

        // The relations this choice can decide: those that name a field of the part, and
        // those that name a field the choice made known or narrower (a value an equation works
        // out from the part's columns: a named ratio, a module's declared current, a frequency
        // two equations away from the resistor) when no part still to be chosen feeds any
        // field the relation names. A bound on such a value judges the part just as a bound
        // written on its columns does (ADR 0057); without it the pick is made without the
        // bound and only the check afterwards refuses it. While a part that feeds the value is
        // unchosen the value is a range of what that part may still be, and a bound that
        // fails on it can still hold for the part that is chosen. Such a bound is not judged,
        // but the candidate is refused if no value of the range meets it (`verify_possible`,
        // ADR 0065, 9.5).
        let changed = self.model.changed_since(backup);
        let relations = std::sync::Arc::clone(self.model.shared_relations());
        let design = self.reads();
        let touched = design.touching(part, changed.iter().copied());
        let relevant: Vec<usize> = touched
            .iter()
            .copied()
            .filter(|&index| {
                let fields = design.fields_of(index);
                let inside = fields
                    .iter()
                    .any(|&f| self.model.is_inside(self.model.field(f).owner, part));
                // A bound that the solver added reads a value another part may still decide
                // (the right side), so it waits for those parts as a bound on a worked-out
                // value does.
                if index >= self.original_relations {
                    return inside && coupling.feeding[index].is_disjoint(later);
                }
                inside
                    || (fields.iter().any(|f| changed.contains(f))
                        && coupling.feeding[index].is_disjoint(later))
            })
            .collect();
        // The bounds the choice changed that wait for a part still to be chosen. They are not
        // judged, but one that no value of the ranges as they are can meet is refused: the
        // parts chosen later only narrow the ranges, so they cannot meet it either. Without
        // this a bound on a value that another group's picks feed (the input voltage of a
        // stage, behind a filter whose drop its load decides) never reached the pick that
        // sets the value, however plainly the pick broke it.
        let waiting: Vec<usize> = touched
            .iter()
            .copied()
            .filter(|&index| {
                index < self.original_relations
                    && matches!(relations[index].op, CmpOp::Ge | CmpOp::Le)
                    && !coupling.feeding[index].is_disjoint(later)
                    && !relevant.contains(&index)
                    && design.fields_of(index).iter().any(|f| changed.contains(f))
            })
            .collect();
        // The relations are checked in order, so the one that fails is the first.
        let mut whole_range = true;
        let judged = relevant
            .iter()
            .map(|&index| (index, true))
            .chain(waiting.iter().map(|&index| (index, false)));
        for (index, whole) in judged {
            self.work += 1;
            let r = &relations[index];
            let mut own = Vec::new();
            fields_in(&r.lhs, &mut own);
            fields_in(&r.rhs, &mut own);
            let verdict = if whole {
                self.verify(r)
            } else {
                self.verify_possible(r)
            };
            if let Verdict::Fail { message, margin } = verdict {
                let message = self.relation_message(index, r, message);
                return reject(message, margin, (r.source, r.span), own, Some(index));
            }
            // A bound that can hold, but not for every value of the range as it is. A relation
            // that reads an end of a range is not placed in the second tier for what the ranges
            // are now: a later pick moves the end (`verify_possible`).
            if !whole && !relation_reads_end(r) && matches!(self.verify(r), Verdict::Fail { .. }) {
                whole_range = false;
            }
        }
        Ok(Accepted {
            needs,
            whole: whole_range,
        })
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
    pub(super) fn need_of(&self, field: FieldId, sense: Sense) -> Option<Need> {
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
            // The end of a range read now may not be the end it has at the last (ADR 0098).
            if relation_reads_end(r) {
                continue;
            }
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
