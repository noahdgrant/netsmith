//! The DC analysis in the solver's pass (ADR 0076, phase 2): what it writes into the model, and
//! what it says.
//!
//! The pass goes after the picks and before the final verification of the relations, and is
//! followed by one more propagation, so that the relations that waited (`.drop == .current *
//! .resistance`, a part's power and its rating) are judged with the numbers it gave.
//!
//! - A field of a modelled part that nothing states (`.current`, `.drop`) takes the range the
//!   circuit gives it.
//! - A field a statement narrowed is a budget (ADR 0038): the circuit has to fit inside it, and
//!   if it needs more the pass reports an error that names the states that give it. The field is
//!   left as stated.
//! - A field that the solver worked out (from a balance, say) is narrowed to what both say.
//! - An island with more combinations than the limit is a note that names it, and is left to the
//!   cheap bound.
//!
//! The ranges are the hulls of the corners, widened outward a little (see `widen`) so that a
//! rounding does not take a value out of the range it is in.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use netsmith_check::{Domain, FieldId, FieldKind, InstId};
use netsmith_quantity::{Dimension, Quantity};

use crate::catalog::Value;
use crate::dc::{self, Error, Modelled, Which};
use crate::generic::Need;
use crate::levels::NotEvaluated;

use super::Solver;

/// What the pass did, for the solution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DcSummary {
    /// The islands that were extracted: complete, held by a source, with a part to write.
    pub islands: usize,
    /// How many of them were solved.
    pub solved: usize,
    /// How many were left alone for having more combinations than the limit.
    pub too_large: usize,
    /// The combinations solved, over every island.
    pub combinations: u64,
    /// The fields of parts that were given a range or narrowed.
    pub written: usize,
    /// The rails that were given the voltage of the node they are on (a chain carries it).
    pub rails: usize,
    /// The regulators whose input was solved with the power their output asks.
    pub regulators: usize,
    /// The MOSFETs solved as switches (on and off), and the logic pins solved as drivers
    /// (high, low, and high impedance where the pin can let go), over every island solved.
    pub switches: usize,
    pub pins: usize,
    /// How many regulators had no operating point in some combination of states: the source
    /// cannot give what the load asks.
    pub collapsed: usize,
}

/// An island and its result, kept to say which states give an end of a range.
#[derive(Debug)]
pub(super) struct Run {
    island: dc::net::Island,
    report: dc::Report,
}

/// The range the pass wrote for a field, and where to read the states at its ends from.
#[derive(Debug, Clone)]
pub(super) struct DcEnds {
    pub range: Quantity,
    run: Rc<Run>,
    which: Which,
    /// Whether the field's direction is the element's opposite: its low end is the element's
    /// high end.
    flipped: bool,
}

impl DcEnds {
    /// The states that give the low (`high` false) or the high end of the field.
    pub fn states(&self, high: bool) -> String {
        self.run
            .report
            .states_at(&self.run.island, self.which, high != self.flipped)
    }
}

/// How much of a stated range a result may stick out of before it is an error: a millionth of
/// it, and an absolute floor below any current or voltage a design means.
fn slack(dim: Dimension, a: f64, b: f64) -> f64 {
    let floor = if dim == Dimension::CURRENT {
        1e-12
    } else {
        1e-9
    };
    1e-6 * a.abs().max(b.abs()) + floor
}

/// The range widened outward by a billionth of its size and by a floor below any current or
/// voltage a design means (a picoampere, a nanovolt): the arithmetic of the solve is exact to
/// about a part in 1e15 of the largest value in the island, so a quantity that is zero in the
/// circuit (a capacitor in a chain) is a little above or below it in the numbers, and two
/// ranges that should meet must not miss by that.
fn widen(dim: Dimension, lo: f64, hi: f64) -> (f64, f64) {
    let floor = if dim == Dimension::CURRENT {
        1e-12
    } else {
        1e-9
    };
    let out = |x: f64, sign: f64| x + sign * (1e-9 * x.abs() + floor);
    (out(lo, -1.0), out(hi, 1.0))
}

fn list(names: &[String]) -> String {
    match names.len() {
        0 => String::new(),
        1..=4 => names.join(", "),
        n => format!("{} and {} more", names[..3].join(", "), n - 3),
    }
}

impl Solver<'_> {
    /// Run the analysis and write its results. The notes are the islands it left alone.
    pub(super) fn dc_pass(&mut self) -> (DcSummary, Vec<NotEvaluated>) {
        let mut summary = DcSummary::default();
        let mut notes = Vec::new();
        // The rails on the nodes of the islands, written when every island is done, and the
        // fields that a part's own write covers (a regulator's input voltage).
        let mut rails: Vec<(dc::RailNode, Rc<Run>)> = Vec::new();
        let mut covered: Vec<FieldId> = Vec::new();
        for outcome in dc::analyze(&self.model, false, &HashMap::new(), &HashSet::new()) {
            netsmith_core::job::checkpoint();
            summary.islands += 1;
            let dc::Outcome { planned, result } = outcome;
            match result {
                Ok(report) => {
                    summary.solved += 1;
                    summary.combinations += report.combinations;
                    summary.regulators += planned.sinks.len();
                    for e in &planned.island.elements {
                        match e.kind {
                            dc::net::Kind::Mosfet { .. } => summary.switches += 1,
                            dc::net::Kind::Pin { .. } => summary.pins += 1,
                            _ => {}
                        }
                    }
                    summary.collapsed += report.collapses.len();
                    for c in &report.collapses {
                        self.dc_collapse(&planned, &report, c);
                    }
                    let solved_any = report.consistent > 0;
                    let run = Rc::new(Run {
                        island: planned.island,
                        report,
                    });
                    if solved_any {
                        for m in &planned.parts {
                            summary.written += self.dc_write(m, &run);
                        }
                        covered.extend(planned.parts.iter().filter_map(|m| m.drop));
                        rails.extend(planned.rails.iter().map(|r| (*r, Rc::clone(&run))));
                    }
                }
                Err(Error::TooManyCombinations {
                    combinations,
                    limit,
                }) => {
                    summary.too_large += 1;
                    let names: Vec<String> = planned
                        .island
                        .elements
                        .iter()
                        .map(|e| format!("`{}`", e.name))
                        .collect();
                    let at = planned.parts.first().map(|m| m.part);
                    let involves: Vec<InstId> = planned.parts.iter().map(|m| m.part).collect();
                    if let Some(at) = at {
                        notes.push(NotEvaluated {
                            at,
                            involves,
                            cause: netsmith_core::Cause::Unjudged,
                            message: format!(
                                "the DC analysis left the network of {} to the bound from Ohm's law: its {combinations} combinations of corners and states are more than the limit of {limit}; cut it at a node whose voltage a rail holds",
                                list(&names)
                            ),
                        });
                    }
                }
                Err(_) => {}
            }
        }
        // A rail whose voltage a part's drop already gives (the input of a regulator) is that
        // write's, and one class of rails joined by `~` is written once.
        let mut done: Vec<FieldId> = Vec::new();
        for (rail, run) in rails {
            netsmith_core::job::checkpoint();
            let class = self.equated(rail.field);
            if class
                .iter()
                .any(|f| covered.contains(f) || done.contains(f))
            {
                continue;
            }
            done.extend(class.iter().copied());
            summary.rails += self.dc_write_rail(&rail, &class, &run);
        }
        (summary, notes)
    }

    /// Give a rail the voltage of its node (a `~>` chain carries the voltage, and so the drops of
    /// what is in it). The rule is the one of every field (amendment 2): an unset voltage takes
    /// the range, one the solver worked out is narrowed to what both say, and one a statement
    /// narrowed (or a rail joined to it by `~`) is the design's and is left as it is.
    fn dc_write_rail(
        &mut self,
        rail: &dc::RailNode,
        class: &HashSet<FieldId>,
        run: &Rc<Run>,
    ) -> usize {
        let which = Which::NodeVoltage(rail.node);
        let r = run.report.range(which);
        if r.is_empty() || !r.lo.is_finite() || !r.hi.is_finite() {
            return 0;
        }
        let (lo, hi) = widen(Dimension::VOLTAGE, r.lo, r.hi);
        let Ok(solved) = Quantity::interval(Dimension::VOLTAGE, lo, hi) else {
            return 0;
        };
        let ends = DcEnds {
            range: solved,
            run: Rc::clone(run),
            which,
            flipped: false,
        };
        let mut members: Vec<FieldId> = class.iter().copied().collect();
        members.sort();
        let stated = members
            .iter()
            .copied()
            .find(|&f| !self.model.field(f).narrowed_at.is_empty() && !self.derived.contains(&f));
        // A voltage the design states is the design's. It is not checked here: a regulator's
        // output is held at the node its part names, and a rail further on (past an inductor, a
        // shunt) is a little below it, which no design states as a window of the circuit's
        // (ADR 0076, amendment 5, "Left for the next stages").
        if stated.is_some() {
            return 0;
        }
        usize::from(self.dc_field(rail.rail, rail.field, solved, ends))
    }

    /// The jobs whose pick waits on a voltage no one has given: a derated column whose bias has
    /// no value, or a column that a relation reads together with the voltage of a rail that has
    /// none (a divider's draw, a Zener's test voltage). Those are the parts that read the DC
    /// analysis' results; the others are picked before it (ADR 0076, amendment 7).
    pub(super) fn waiting_jobs(&self, jobs: &[super::Job]) -> HashSet<usize> {
        let unset: HashSet<FieldId> = self
            .model
            .instances()
            .filter_map(|(i, _)| self.model.rail_of(i))
            .map(|(voltage, _, _)| voltage)
            .filter(|&v| matches!(self.domain(v), Domain::Unset))
            .collect();
        let mut by_column: HashMap<FieldId, Vec<usize>> = HashMap::new();
        for (j, job) in jobs.iter().enumerate() {
            for (f, _) in &job.columns {
                by_column.entry(*f).or_default().push(j);
            }
        }
        let mut out: HashSet<usize> = jobs
            .iter()
            .enumerate()
            .filter(|(_, j)| !self.unset_biases(j).is_empty())
            .map(|(i, _)| i)
            .collect();
        if unset.is_empty() {
            return out;
        }
        for r in self.model.relations().iter() {
            let mut reads = Vec::new();
            crate::eval::fields_in(&r.lhs, &mut reads);
            crate::eval::fields_in(&r.rhs, &mut reads);
            if reads.iter().any(|f| unset.contains(f)) {
                for f in &reads {
                    if let Some(js) = by_column.get(f) {
                        out.extend(js.iter().copied());
                    }
                }
            }
        }
        out
    }

    /// The windows a trial of the early picks leans on (ADR 0076, amendment 7): for each
    /// regulator whose input voltage has no value, between what its part and the design need of
    /// it (`.vin.voltage >= 3V`, `>= .vout.voltage`: any design that is valid has the input at
    /// or above the largest of them) and the highest supply that reaches it through series
    /// elements (a drop only lowers the voltage). It is what a board wrote by hand as the seed
    /// of a circle. It is an assumption about a valid design and not an enclosure of the
    /// circuit, so it is never written into the model that is kept.
    pub(super) fn assumed_windows(&self) -> Vec<(FieldId, Quantity)> {
        use netsmith_check::DcKind;
        use netsmith_check::RExpr;
        use netsmith_lang::ast::CmpOp;
        let ceilings = dc::ceiling::Ceilings::new(&self.model);
        let mut out = Vec::new();
        let mut seen: HashSet<FieldId> = HashSet::new();
        for (part, element) in self.model.dc_elements() {
            if !matches!(
                element.kind,
                DcKind::LinearRegulator | DcKind::SwitchingRegulator
            ) || self.model.instance(part).kind != netsmith_check::InstKind::Part
            {
                continue;
            }
            let Some((v_in, in_pos, _)) = self.model.rail_of(element.pins[0]) else {
                continue;
            };
            let class = self.equated(v_in);
            if class
                .iter()
                .any(|&f| !matches!(self.domain(f), Domain::Unset))
                || !seen.insert(v_in)
            {
                continue;
            }
            let Some(hi) = ceilings.of(in_pos) else {
                continue;
            };
            let mut lo: Option<f64> = None;
            for r in self.model.relations().iter() {
                let bound = match (&r.op, &r.lhs, &r.rhs) {
                    (CmpOp::Ge, RExpr::Field(g), bound) | (CmpOp::Le, bound, RExpr::Field(g))
                        if class.contains(g) =>
                    {
                        bound
                    }
                    _ => continue,
                };
                if let Ok(crate::eval::Val::Q(q)) = crate::eval::eval(&self.model, bound)
                    && q.dim() == Dimension::VOLTAGE
                    && q.hi().is_finite()
                {
                    lo = Some(lo.map_or(q.hi(), |l: f64| l.max(q.hi())));
                }
            }
            if let Some(lo) = lo
                && lo > 0.0
                && lo <= hi
                && let Ok(window) = Quantity::interval(Dimension::VOLTAGE, lo, hi)
            {
                out.push((v_in, window));
            }
        }
        out
    }

    /// The voltages with no value that a pick reads: the rails that a relation reads together
    /// with a column of a part to be chosen (a divider's draw is its input voltage over the two
    /// resistances, and its limit cannot refuse a candidate while the voltage is unknown).
    fn voltages_picks_read(&self, jobs: &[super::Job]) -> HashSet<FieldId> {
        let columns: HashSet<FieldId> = jobs
            .iter()
            .flat_map(|j| j.columns.iter().map(|(f, _)| *f))
            .collect();
        let unset: HashSet<FieldId> = self
            .model
            .instances()
            .filter_map(|(i, _)| self.model.rail_of(i))
            .map(|(voltage, _, _)| voltage)
            .filter(|&v| matches!(self.domain(v), Domain::Unset))
            .collect();
        let mut out = HashSet::new();
        if unset.is_empty() || columns.is_empty() {
            return out;
        }
        for r in self.model.relations().iter() {
            let mut reads = Vec::new();
            crate::eval::fields_in(&r.lhs, &mut reads);
            crate::eval::fields_in(&r.rhs, &mut reads);
            if reads.iter().any(|f| columns.contains(f)) {
                out.extend(reads.into_iter().filter(|f| unset.contains(f)));
            }
        }
        out
    }

    /// The start of a circle that a pick waits on (ADR 0076, amendment 5). A pick that reads a
    /// voltage no one has given yet (a capacitor's bias across a rail behind a fuse, a diode and
    /// a bead) cannot be judged, and the board used to write a window by hand to start it. The
    /// network is solved once before the picks with every part that is still to be picked at the
    /// hull of the values its candidates can have, and the voltage of each rail and each
    /// regulator input that the circuit gives is the range the picks see. It is an enclosure:
    /// whatever the picks choose, the voltage ends inside it, and the pass after the picks
    /// narrows it to the circuit that was picked. It runs only when a pick waits on a voltage
    /// that has none (a bias, or a relation that reads a column and a rail's voltage), so a board
    /// with none is solved as before, and it writes voltages only (a current a pick would then
    /// judge a rating against is the later pass's).
    pub(super) fn dc_start(&mut self, jobs: &[super::Job], closing: bool) {
        // The start runs when a pick waits on a voltage: the bias of a derated column, or a
        // voltage that a relation reads with a column.
        if !jobs.iter().any(|j| !self.unset_biases(j).is_empty())
            && self.voltages_picks_read(jobs).is_empty()
        {
            return;
        }
        // What the design's own relations and balances say before any pick (a regulator's output
        // current is at most the part's `max_output_current`, a winding's resistance at most
        // the IC's `max_inductor_dcr`) is worked through first: the propagation that follows the
        // start would do it all the same, and a bound it gives is a range the enclosure uses.
        // The conflicts are not said here: that propagation finds them again.
        let _ = self.propagate();
        let saved = self.model.snapshot();
        let mut hulled: HashSet<InstId> = HashSet::new();
        let enclosed = if closing {
            self.enclose_loads(jobs)
        } else {
            HashMap::new()
        };
        // Each part still to be picked takes the hull of the values its candidates can have for
        // what its `dc` element reads. Every candidate's value is inside the hull, so what comes
        // out is an enclosure of every choice.
        for job in jobs {
            let Some(dc) = self.model.instance(job.part).dc.clone() else {
                continue;
            };
            if enclosed.contains_key(&job.part) {
                continue;
            }
            for &field in &dc.values {
                if !matches!(self.domain(field), Domain::Unset) {
                    continue;
                }
                if closing {
                    hulled.insert(job.part);
                }
                let Some((_, key)) = job.columns.iter().find(|(f, _)| *f == field) else {
                    continue;
                };
                if let Some((dim, lo, hi)) = self.column_hull(job, field, key)
                    && let Ok(q) = Quantity::interval(dim, lo, hi)
                {
                    self.model.set_domain(field, Domain::Quantity(q));
                }
            }
        }
        if closing {
            self.enclose_boundaries(jobs);
        }
        let outcomes = dc::analyze(&self.model, true, &enclosed, &hulled);
        self.model.restore(&saved);
        let mut written: Vec<(dc::RailNode, Rc<Run>)> = Vec::new();
        let mut vins: Vec<(InstId, FieldId, Rc<Run>, usize)> = Vec::new();
        for outcome in outcomes {
            netsmith_core::job::checkpoint();
            let dc::Outcome { planned, result } = outcome;
            let Ok(report) = result else { continue };
            if report.consistent == 0 {
                continue;
            }
            let run = Rc::new(Run {
                island: planned.island,
                report,
            });
            for sink in planned.sinks.iter().filter(|s| !s.behind_hull) {
                if let Some(m) = planned.parts.iter().find(|m| m.element == sink.element)
                    && let Some(field) = m.drop
                {
                    vins.push((m.part, field, Rc::clone(&run), m.element));
                }
            }
            written.extend(planned.rails.iter().map(|r| (*r, Rc::clone(&run))));
        }
        // A regulator's input voltage is the drop across its input element.
        for (part, field, run, element) in vins {
            let r = run.report.range(Which::Drop(element));
            if r.is_empty() || !r.lo.is_finite() || !r.hi.is_finite() {
                continue;
            }
            let (lo, hi) = widen(Dimension::VOLTAGE, r.lo, r.hi);
            let Ok(solved) = Quantity::interval(Dimension::VOLTAGE, lo, hi) else {
                continue;
            };
            let class = self.equated(field);
            if class
                .iter()
                .any(|&f| !matches!(self.domain(f), Domain::Unset))
            {
                continue;
            }
            let ends = DcEnds {
                range: solved,
                run: Rc::clone(&run),
                which: Which::Drop(element),
                flipped: false,
            };
            self.dc_field(part, field, solved, ends);
        }
        let mut done: Vec<FieldId> = Vec::new();
        for (rail, run) in written {
            let class = self.equated(rail.field);
            if class.iter().any(|f| done.contains(f))
                || class
                    .iter()
                    .any(|&f| !matches!(self.domain(f), Domain::Unset))
            {
                continue;
            }
            done.extend(class.iter().copied());
            self.dc_write_rail(&rail, &class, &run);
        }
    }

    /// The parts still to be picked that are resistors and that the design limits the current of
    /// (`.divider_top.current <= .max_draw`, which `.divider_bottom.current` is held to): each is
    /// a load of at most that limit, and not a resistor at the hull of its catalog, whose low
    /// end (milliohms) would draw hundreds of amperes from the supply. The limit is a relation
    /// a pick that is made must satisfy, as the limit on a regulator's output is (amendment 6).
    fn enclose_loads(&self, jobs: &[super::Job]) -> HashMap<InstId, dc::Range> {
        let mut out = HashMap::new();
        for job in jobs {
            let Some(element) = &self.model.instance(job.part).dc else {
                continue;
            };
            if element.kind != netsmith_check::DcKind::Resistor
                || !matches!(self.domain(element.values[0]), Domain::Unset)
            {
                continue;
            }
            let Some(through) = self.model.through_of(job.part) else {
                continue;
            };
            if let Some(limit) = self.current_limit(through) {
                out.insert(job.part, dc::Range::new(0.0, limit));
            }
        }
        out
    }

    /// The most a relation lets a current be: the least value of `x` over the relations
    /// `.current <= x` that the field, or a field it is held equal to, has.
    fn current_limit(&self, current: FieldId) -> Option<f64> {
        use netsmith_check::RExpr;
        use netsmith_lang::ast::CmpOp;
        let mut class = self.equated(current);
        loop {
            let before = class.len();
            for r in self.model.relations().iter() {
                if let (CmpOp::EqEq, RExpr::Field(a), RExpr::Field(b)) = (&r.op, &r.lhs, &r.rhs)
                    && (class.contains(a) || class.contains(b))
                {
                    class.insert(*a);
                    class.insert(*b);
                }
            }
            if class.len() == before {
                break;
            }
        }
        let mut limit: Option<f64> = None;
        for r in self.model.relations().iter() {
            let bound = match (&r.op, &r.lhs, &r.rhs) {
                (CmpOp::Le, RExpr::Field(g), bound) | (CmpOp::Ge, bound, RExpr::Field(g))
                    if class.contains(g) =>
                {
                    bound
                }
                _ => continue,
            };
            if let Ok(crate::eval::Val::Q(q)) = crate::eval::eval(&self.model, bound)
                && q.dim() == Dimension::CURRENT
                && q.hi().is_finite()
                && q.hi() >= 0.0
            {
                limit = Some(limit.map_or(q.hi(), |m| m.min(q.hi())));
            }
        }
        limit
    }

    /// A part with a `bridge` and no `dc` element draws what its `.current` says, and a design
    /// can state that as a window that reads a column of the part (a Zener's `.current &= 0A ..
    /// .leakage_current`). The columns of such a part that are still to be picked are taken at
    /// the hull of their candidates, the relations are worked through once, and the current has
    /// the range every candidate lies in: without one the network it is in is open.
    fn enclose_boundaries(&mut self, jobs: &[super::Job]) {
        let mut any = false;
        for job in jobs {
            let inst = self.model.instance(job.part);
            if inst.dc.is_some() || inst.bridge.is_none() {
                continue;
            }
            let Some(current) = self.model.through_of(job.part) else {
                continue;
            };
            if !matches!(self.domain(current), Domain::Unset) {
                continue;
            }
            for (field, key) in &job.columns {
                if !matches!(self.domain(*field), Domain::Unset) {
                    continue;
                }
                if let Some((dim, lo, hi)) = self.column_hull(job, *field, key)
                    && let Ok(q) = Quantity::interval(dim, lo, hi)
                {
                    self.model.set_domain(*field, Domain::Quantity(q));
                    any = true;
                }
            }
        }
        if any {
            let _ = self.propagate();
        }
    }

    /// The hull of the values the candidates that fit can have in one column, when every one of
    /// them has a value: a quantity, or a generic row's (ADR 0047), which the design supplies
    /// within what it says of the field. `None` when a candidate has none that is known (a
    /// derated value, a text, a value that nothing bounds): no enclosure then.
    fn column_hull(
        &self,
        job: &super::Job,
        field: FieldId,
        key: &str,
    ) -> Option<(Dimension, f64, f64)> {
        let FieldKind::Quantity(dim) = self.model.field(field).kind else {
            return None;
        };
        let mut hull: Option<(f64, f64)> = None;
        for c in job
            .catalog
            .candidates
            .iter()
            .filter(|c| self.fits(job.part, &job.columns, c))
        {
            let (lo, hi) = match c.column(key) {
                Some(Value::Quantity(q)) if q.dim() == dim => (q.lo(), q.hi()),
                // A generic row has no value of its own: the design supplies it, within what the
                // design says (`.dc_resistance <= .max_inductor_dcr`).
                Some(Value::Open(sense)) => match self.need_of(field, *sense) {
                    Some(Need::Within { lo, hi }) => (lo, hi),
                    Some(Need::AtMost(x)) => (0.0, x),
                    _ => return None,
                },
                _ => return None,
            };
            hull = Some(match hull {
                None => (lo, hi),
                Some((l, h)) => (l.min(lo), h.max(hi)),
            });
        }
        let (lo, hi) = hull?;
        (lo.is_finite() && hi.is_finite()).then_some((dim, lo, hi))
    }

    /// A regulator that has no operating point in some combination of states: its output asks
    /// for more power than the source can give through the resistance in front of it. The states
    /// are those of the combination that is the circuit's, and not those of a refused one.
    fn dc_collapse(&mut self, planned: &dc::Planned, report: &dc::Report, c: &dc::Collapse) {
        let Some(m) = planned.parts.iter().find(|m| m.element == c.element) else {
            return;
        };
        let element = &planned.island.elements[c.element];
        let asks = match &element.kind {
            dc::net::Kind::Regulator {
                draw: dc::net::Draw::Switching { p, eff },
                ..
            } => {
                let percent = |x: f64| format!("{} %", (x * 1000.0).round() / 10.0);
                let power = Quantity::interval(Dimension::POWER, p.lo, p.hi)
                    .map_or_else(|_| String::new(), |q| q.to_string());
                let eff = if eff.is_point() {
                    percent(eff.lo)
                } else {
                    format!("{}..{}", percent(eff.lo), percent(eff.hi))
                };
                format!(" ({power} at its output, efficiency {eff})")
            }
            _ => String::new(),
        };
        let states = report.describe(c.combination);
        let message = format!(
            "the source cannot give what `{}` asks{asks}: its input falls to {} and no operating point exists{}",
            element.name,
            end(Dimension::VOLTAGE, c.vin.max(0.0)),
            if states.is_empty() {
                String::new()
            } else {
                format!(", with {states}")
            }
        );
        let (source, span) = self.model.instance(m.part).at();
        self.report(source, span, message);
    }

    /// Write one part's current and drop. Returns how many fields were written.
    fn dc_write(&mut self, m: &Modelled, run: &Rc<Run>) -> usize {
        let report = &run.report;
        let mut written = 0;
        let flip = |lo: f64, hi: f64, m: &Modelled| if m.flipped { (-hi, -lo) } else { (lo, hi) };
        let cases = [
            (m.current, Dimension::CURRENT, Which::Current(m.element)),
            (m.drop, Dimension::VOLTAGE, Which::Drop(m.element)),
        ];
        for (field, dim, which) in cases {
            let Some(field) = field else { continue };
            let r = report.range(which);
            if r.is_empty() || !r.lo.is_finite() || !r.hi.is_finite() {
                continue;
            }
            // A diode never carries a current below zero in its own direction.
            let lo_end = if m.diode && dim == Dimension::CURRENT {
                r.lo.max(0.0)
            } else {
                r.lo
            };
            let (lo, hi) = flip(lo_end, r.hi, m);
            let (lo, hi) = widen(dim, lo, hi);
            // The widening does not take a diode's current across zero either.
            let (lo, hi) = if m.diode && dim == Dimension::CURRENT {
                if m.flipped {
                    (lo, hi.min(0.0))
                } else {
                    (lo.max(0.0), hi)
                }
            } else {
                (lo, hi)
            };
            let Ok(solved) = Quantity::interval(dim, lo, hi) else {
                continue;
            };
            let ends = DcEnds {
                range: solved,
                run: Rc::clone(run),
                which,
                flipped: m.flipped,
            };
            if self.dc_field(m.part, field, solved, ends) {
                written += 1;
            }
        }
        written
    }

    fn dc_field(&mut self, part: InstId, field: FieldId, solved: Quantity, ends: DcEnds) -> bool {
        match self.domain(field) {
            Domain::Unset => {
                self.set_field(field, Domain::Quantity(solved));
                self.derived.insert(field);
                self.dc_ends.insert(field, ends);
                true
            }
            Domain::Quantity(d) => {
                let stated = !self.model.field(field).narrowed_at.is_empty()
                    && !self.derived.contains(&field);
                if stated {
                    self.dc_check_stated(part, field, d, solved, &ends);
                    return false;
                }
                match d.intersect(&solved) {
                    Ok(Some(both)) if both != d => {
                        self.set_field(field, Domain::Quantity(both));
                        self.derived.insert(field);
                        self.dc_ends.insert(field, ends);
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// A stated range is a budget: what the circuit needs has to lie inside it.
    fn dc_check_stated(
        &mut self,
        part: InstId,
        field: FieldId,
        stated: Quantity,
        solved: Quantity,
        ends: &DcEnds,
    ) {
        let dim = stated.dim();
        let tol = slack(
            dim,
            stated.lo().min(stated.hi()),
            stated.lo().max(stated.hi()),
        );
        let over = solved.hi() > stated.hi() + tol;
        let under = solved.lo() < stated.lo() - tol;
        if !over && !under {
            return;
        }
        let name = self.model.path_of_field(field);
        let mut why = Vec::new();
        let describe = |what: &str, states: &str| {
            if states.is_empty() {
                what.to_string()
            } else {
                format!("{what} with {states}")
            }
        };
        if over {
            why.push(describe(
                &format!("at most {} at the high end", end(dim, solved.hi())),
                &ends.states(true),
            ));
        }
        if under {
            why.push(describe(
                &format!("at least {} at the low end", end(dim, solved.lo())),
                &ends.states(false),
            ));
        }
        let message = format!(
            "`{name}` is stated {stated}, but the DC analysis of the circuit around it finds {solved}: {}",
            why.join("; ")
        );
        let (source, span) = self
            .model
            .field(field)
            .narrowed_at
            .last()
            .copied()
            .unwrap_or_else(|| self.model.instance(part).at());
        self.report(source, span, message);
    }

    /// What the DC analysis says of the fields a failed relation reads, to add to its message:
    /// the range it wrote and the states at its ends.
    pub(super) fn dc_note(&self, fields: &[FieldId]) -> String {
        let mut seen: Vec<FieldId> = Vec::new();
        let mut parts = Vec::new();
        for &f in fields {
            if seen.contains(&f) {
                continue;
            }
            seen.push(f);
            let Some(ends) = self.dc_ends.get(&f) else {
                continue;
            };
            let name = self.model.path_of_field(f);
            let mut text = format!("the DC analysis puts `{name}` at {}", ends.range);
            for (label, states) in [("high", ends.states(true)), ("low", ends.states(false))] {
                if !states.is_empty() {
                    text.push_str(&format!(", its {label} end with {states}"));
                }
            }
            parts.push(text);
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!(" ({})", parts.join("; "))
        }
    }
}

fn end(dim: Dimension, v: f64) -> String {
    Quantity::point(dim, v).map_or_else(|_| format!("{v}"), |q| q.to_string())
}

pub(super) type DcEndsMap = HashMap<FieldId, DcEnds>;
