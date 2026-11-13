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
//!
//! The same analysis runs at three more points, each an enclosure and not the final answer (the
//! amendments of ADR 0076 before its cut, `git show 20f8f0c:docs/adr/`, say why):
//!
//! - Before the picks, when a pick waits on a voltage nobody has given (a derated capacitor's
//!   bias, a divider's draw): `dc_start` solves the network once with each part still to be
//!   picked at the hull of what its candidates give (`enclose_loads`, `enclose_boundaries`),
//!   and writes voltages only. `enclose_regulator_loads` first writes a regulator's load as the
//!   sum of the loads behind it when every load has one. A stated window is used as stated.
//! - Between two searches: a component with a part that reads such a voltage is picked after
//!   the others (`run_groups`, `run_early` in `solve.rs`, `waiting_jobs`); the picks of the
//!   early ones lean on an assumed window for each regulator input (`assumed_windows`), which is
//!   taken out again before they are kept.
//! - Inside a search, when the last part of a group is chosen: `network_trial` solves the small
//!   island that has a part of the group, cut at the rails an earlier pass wrote (`network.cuts`),
//!   and writes the voltage of rails nothing has given, so a pick is refused on the circuit's
//!   own number.
//!
//! After the picks, `dc_field` and `dc_write_rail` write the results by the rules above, and the
//! checks of the design's statements are `dc_check_supplies` (a supply's own `.current` is a
//! budget the loads must be able to meet), `dc_check_efficiencies` (an efficiency is above 0 and
//! at most 1), `dc_check_stated` (a voltage the design states on a rail the analysis solved must
//! enclose the circuit's range) and `dc_disagree` (a worked-out current the analysis cannot
//! share is an error that names both ranges).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use netsmith_check::{Domain, FieldId, FieldKind, InstId};
use netsmith_quantity::{Dimension, Quantity};

use crate::catalog::Value;
use crate::dc::{self, Error, Modelled, Which};
use crate::eval::relation_reads_end;
use crate::generic::Need;
use crate::levels::{NetReads, NotEvaluated};

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

/// What the pick-time analysis knows about the group being searched (ADR 0076, amendment 10).
#[derive(Debug, Default)]
pub(super) struct NetworkTrial {
    /// The parts of the group.
    parts: Vec<InstId>,
    /// Whether the last pick of the group is judged with the analysis: unknown until the first
    /// one is made, then whether an island with a part of the group has a rail whose voltage no
    /// one gave and a relation reads.
    pub(super) active: Option<bool>,
    /// The rails whose voltage an earlier pass worked out: the network is cut at them.
    cuts: HashSet<InstId>,
    /// The fields that a relation reads.
    read: HashSet<FieldId>,
}

impl NetworkTrial {
    pub(super) fn of(parts: Vec<InstId>) -> Self {
        Self {
            parts,
            ..Self::default()
        }
    }
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

/// The high of each line that has a pin taken to be open drain: the island solved again with
/// every such pin high impedance, and the range of the line's voltage over it (ADR 0076,
/// amendment 8). The ends are widened as every range the pass writes is.
fn open_drain_reads(planned: &dc::Planned, reads: &mut NetReads) {
    let Some(ground) = planned.ground else { return };
    let mut island = planned.island.clone();
    for &e in &planned.open_drain {
        if let dc::net::Kind::Pin { low, .. } = &mut island.elements[e].kind {
            *low = None;
        }
    }
    let Ok(report) = island.analyze(dc::MAX_COMBINATIONS) else {
        return;
    };
    if report.consistent == 0 {
        return;
    }
    for &e in &planned.open_drain {
        let dc::net::Kind::Pin { node, .. } = planned.island.elements[e].kind else {
            continue;
        };
        let Some(&net) = planned.free_nets.get(node) else {
            continue;
        };
        let r = report.range(Which::NodeVoltage(node));
        if r.is_empty() || !r.lo.is_finite() || !r.hi.is_finite() {
            continue;
        }
        let (lo, hi) = widen(Dimension::VOLTAGE, r.lo, r.hi);
        reads.high.insert(net, (ground, lo, hi));
    }
}

impl Solver<'_> {
    /// Run the analysis and write its results. The notes are the islands it left alone.
    pub(super) fn dc_pass(
        &mut self,
    ) -> (
        DcSummary,
        Vec<NotEvaluated>,
        NetReads,
        Vec<super::NetCurrent>,
    ) {
        let mut net_currents: Vec<super::NetCurrent> = Vec::new();
        self.dc_check_efficiencies();
        let mut summary = DcSummary::default();
        let mut reads = NetReads::default();
        let mut notes = Vec::new();
        // The rails on the nodes of the islands, written when every island is done, and the
        // fields that a part's own write covers (a regulator's input voltage).
        let mut rails: Vec<(dc::RailNode, Rc<Run>)> = Vec::new();
        let mut covered: Vec<FieldId> = Vec::new();
        // What each island draws from a source rail that states its own `.current`
        // (amendment 13): the rail, the range of the draw, and the island's run.
        let mut drawn: SupplyDraws = Vec::new();
        let dc::Analysis { outcomes, left } =
            dc::analyze(&self.model, false, &HashMap::new(), &HashSet::new());
        notes.extend(left.into_iter().map(|l| NotEvaluated {
            at: l.at,
            involves: l.involves,
            cause: l.cause,
            message: l.message,
        }));
        for outcome in outcomes {
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
                    // An island with a pin taken to be open drain by inference gives the levels
                    // check the voltage of its lines, and nothing is written for its parts
                    // (amendment 8).
                    let inferred = !planned.open_drain.is_empty();
                    if inferred && solved_any {
                        open_drain_reads(&planned, &mut reads);
                    }
                    // The voltage of each free net, for the absolute maximum of a pin on it
                    // (amendment 12).
                    if solved_any
                        && !inferred
                        && let Some(ground) = planned.ground
                    {
                        for (k, &net) in planned.free_nets.iter().enumerate() {
                            let r = report.range(Which::NodeVoltage(k));
                            if r.is_empty() || !r.lo.is_finite() || !r.hi.is_finite() {
                                continue;
                            }
                            let (lo, hi) = widen(Dimension::VOLTAGE, r.lo, r.hi);
                            reads.volts.insert(net, (ground, lo, hi));
                        }
                    }
                    // The most that flows into each net the island solved, over every corner
                    // (amendment 8), unless an inferred open-drain pin is in it.
                    if solved_any && !inferred {
                        for (k, &net) in planned.free_nets.iter().enumerate() {
                            let r = report.range(Which::Through(k));
                            let pin = self.model.nets()[net].terminals.first().copied();
                            if let Some(pin) = pin
                                && r.hi.is_finite()
                                && !r.is_empty()
                            {
                                let (_, hi) = widen(Dimension::CURRENT, 0.0, r.hi);
                                net_currents.push(super::NetCurrent { pin, current: hi });
                            }
                        }
                    }
                    let supplies = planned.supplies.clone();
                    let run = Rc::new(Run {
                        island: planned.island,
                        report,
                    });
                    if solved_any {
                        for sp in supplies {
                            let r = run.report.range(Which::HeldDraw(sp.held));
                            if r.is_empty() || !r.hi.is_finite() {
                                continue;
                            }
                            drawn.push((sp, r.lo, r.hi, Rc::clone(&run)));
                        }
                    }
                    if solved_any && !inferred {
                        for m in &planned.parts {
                            summary.written += self.dc_write(m, &run, !planned.outputs.is_empty());
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
                // An island with no combination of states that is its circuit's, or a matrix
                // with no solution, writes nothing: it says so, and what to look at.
                Err(error) => {
                    let Some(at) = planned.parts.first().map(|m| m.part) else {
                        continue;
                    };
                    let involves: Vec<InstId> = planned.parts.iter().map(|m| m.part).collect();
                    let names = dc::names_of(&planned);
                    let message = match error {
                        Error::NoOperatingPoint => format!(
                            "the DC analysis found no operating point for the network of {names}: in every combination of corners and states something is refused (a switch is on while its gate is below its threshold, or off while it is above, or a diode conducts against its drop); the circuit may not be one that settles (a source follower, a switch whose own current moves its gate), and nothing is written for these parts; change the gate drive or the thresholds, or state the currents of the parts"
                        ),
                        _ => format!(
                            "the DC analysis could not solve the network of {names}: its equations have no solution in some combination (a loop of ideal sources, or a node that nothing connects to a source); nothing is written for these parts; join the node to a rail or remove the loop"
                        ),
                    };
                    notes.push(NotEvaluated {
                        at,
                        involves,
                        cause: netsmith_core::Cause::Unjudged,
                        message,
                    });
                }
            }
        }
        self.supply_draws = drawn;
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
        (summary, notes, reads, net_currents)
    }

    /// A source rail that states its own `.current` is a budget (ADR 0076, amendment 13): a
    /// supply states what it gives as a negative current (ADR 0014: a rail's current is what is
    /// drawn at its `pos`), `.supply.current &= -2A..0A`, and the loads that the analysis finds on
    /// its net must be able to draw no more than the low end says. It is ADR 0038's rule for two
    /// stated ranges, read with the analysis's range for the loads: the range the loads draw over
    /// every corner and the budget must be able to meet, and the finding is that they cannot,
    /// that even the least the loads draw is more than the supply gives. A balance that has
    /// reported the same supply says it once.
    ///
    /// The draw is the sum, over the networks on the net that were solved, of each one's range:
    /// a network that was left alone, or a pin that no network models, is not in it, so the sum
    /// is never above what the loads draw and the finding is never made on a load that is not
    /// there. A supply that states nothing has no budget, and a stated range with no negative
    /// end gives nothing to compare with.
    pub(super) fn dc_check_supplies(&mut self) -> bool {
        let mut reported = false;
        let drawn = std::mem::take(&mut self.supply_draws);
        let mut done: Vec<FieldId> = Vec::new();
        for (sp, ..) in &drawn {
            if done.contains(&sp.current) || self.budget_failed.contains(&sp.current) {
                continue;
            }
            done.push(sp.current);
            let mine: Vec<&(dc::Supply, f64, f64, Rc<Run>)> =
                drawn.iter().filter(|d| d.0.current == sp.current).collect();
            let field = self.model.field(sp.current);
            let stated = self
                .stated
                .get(&sp.current)
                .copied()
                .or_else(|| match field.domain {
                    Domain::Quantity(q)
                        if !field.narrowed_at.is_empty() && !self.derived.contains(&sp.current) =>
                    {
                        Some(q)
                    }
                    _ => None,
                });
            let Some(stated) = stated else { continue };
            if stated.dim() != Dimension::CURRENT || !stated.lo().is_finite() || stated.lo() >= 0.0
            {
                continue;
            }
            let budget = -stated.lo();
            let least: f64 = mine.iter().map(|d| d.1).sum();
            let most: f64 = mine.iter().map(|d| d.2).sum();
            let (least, _) = widen(Dimension::CURRENT, least, least);
            if !least.is_finite() || least <= budget + slack(Dimension::CURRENT, budget, budget) {
                continue;
            }
            let rail = self.model.path_of(sp.rail);
            let name = self.model.path_of_field(sp.current);
            let how = if let [(one, _, _, run)] = mine.as_slice() {
                let states = run
                    .report
                    .states_at(&run.island, Which::HeldDraw(one.held), false);
                if states.is_empty() {
                    String::new()
                } else {
                    format!(", the least with {states}")
                }
            } else {
                format!(", summed over the {} networks on it", mine.len())
            };
            let message = format!(
                "`{name}` is stated {stated}, so the supply `{rail}` gives at most {}, but the DC analysis finds the loads on its net drawing at least {} and up to {}{how}",
                end(Dimension::CURRENT, budget),
                end(Dimension::CURRENT, least),
                end(Dimension::CURRENT, most),
            );
            let (source, span) = self.model.instance(sp.rail).at();
            self.report(source, span, message);
            reported = true;
        }
        reported
    }

    /// Say the disagreements of `dc_field` (amendment 15), unless a supply's stated `.current`
    /// has been found over its budget: a budget narrows the balance's numbers along its chain,
    /// so each of them misses the circuit's by the same contradiction, which the finding about
    /// the supply has said once.
    pub(super) fn dc_report_disagreements(&mut self, supply_found: bool) {
        let found = std::mem::take(&mut self.dc_disagreements);
        if supply_found || !self.budget_failed.is_empty() {
            return;
        }
        for (source, span, message) in found {
            self.report(source, span, message);
        }
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
        // A voltage the design states on any member of the class is the design's (amendment 17).
        // It is checked against the circuit: the range it states has to enclose the one the
        // analysis found, and is left as stated when it does. A rail past a regulator's output
        // is the exception (amendment 5, choice 3): the part holds the node it names, and a
        // rail further on (past an inductor, a shunt) is a little below it, which no design
        // states as a window of the circuit's. Its statement is left alone.
        if let Some(by) = self.stated_in(&members) {
            if !rail.past_regulator {
                self.dc_check_stated(rail.rail, by, solved, &ends);
            }
            return 0;
        }
        let written = self.dc_field(rail.rail, rail.field, solved, ends, false);
        if written {
            // The members of the class that `~` joins take the range from the one written, and
            // are as much the analysis's as it is: a relation that reads one of them (a
            // hand-written `.vout.voltage == .vin.voltage - .fuse.drop`) only has to share a
            // value with it. Judged as a statement it would fail by the widening of the ranges
            // it adds (issue #253).
            self.derived.extend(members);
        }
        usize::from(written)
    }

    /// The pick-time analysis (ADR 0076, amendment 10): with the last part of a group chosen, the
    /// island of the network that has a part of the group is solved on its own, cut at the rails
    /// whose voltage an earlier pass worked out, and the voltage it gives a rail that no one gave
    /// is written, so the relations that read it judge the pick. That is what a hand-written
    /// `.vout.voltage == .vin.voltage - .r.drop` did for a resistor in series. Only an island with
    /// no regulator in it is solved (what a regulator needs is the island behind it, which is the
    /// pass after the picks'), and it is solved only when a relation reads such a voltage; a group
    /// with no such island is never solved again. When it wrote something, the fields its
    /// numbers were read from are returned (a relation that fails on what it wrote depends on
    /// the picks that those are worked out from), and whether some are not listed.
    pub(super) fn network_trial(&mut self) -> Option<(Vec<FieldId>, bool)> {
        if self.network.active == Some(false) {
            return None;
        }
        let index = self
            .dc_index
            .get_or_insert_with(|| dc::Index::new(&self.model));
        if self.network.active.is_none() {
            self.network.cuts = index
                .rails()
                .iter()
                .filter(|&&(_, voltage)| {
                    self.derived.contains(&voltage)
                        && matches!(self.model.field(voltage).domain, Domain::Quantity(_))
                })
                .map(|&(rail, _)| rail)
                .collect();
            let mut read = Vec::new();
            for r in self.model.relations() {
                crate::eval::fields_in(&r.lhs, &mut read);
                crate::eval::fields_in(&r.rhs, &mut read);
            }
            self.network.read = read.into_iter().collect();
        }
        let outcomes =
            dc::analyze_around(&self.model, index, &self.network.parts, &self.network.cuts);
        let mut applicable = false;
        let mut todo: Vec<(dc::RailNode, HashSet<FieldId>, Rc<Run>)> = Vec::new();
        let mut reads: Vec<FieldId> = Vec::new();
        let mut opaque = false;
        for outcome in outcomes {
            netsmith_core::job::checkpoint();
            let dc::Outcome { planned, result } = outcome;
            let Ok(report) = result else { continue };
            let solved = report.consistent > 0;
            let rails = planned.rails.clone();
            let (island_reads, island_opaque) = (planned.reads.clone(), planned.opaque);
            let run = Rc::new(Run {
                island: planned.island,
                report,
            });
            for rail in rails {
                let class = self.equated(rail.field);
                if class
                    .iter()
                    .any(|&f| !matches!(self.domain(f), Domain::Unset))
                    || !class.iter().any(|f| self.network.read.contains(f))
                {
                    continue;
                }
                applicable = true;
                if solved {
                    todo.push((rail, class, Rc::clone(&run)));
                    reads.extend(island_reads.iter().copied());
                    opaque |= island_opaque;
                }
            }
        }
        if self.network.active.is_none() {
            self.network.active = Some(applicable);
        }
        let mut done: Vec<FieldId> = Vec::new();
        let mut wrote = false;
        for (rail, class, run) in todo {
            if class.iter().any(|f| done.contains(f)) {
                continue;
            }
            done.extend(class.iter().copied());
            wrote |= self.dc_write_rail(&rail, &class, &run) > 0;
        }
        wrote.then_some((reads, opaque))
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
                // The end of a range read now may be stale once the range narrows (ADR 0098).
                if relation_reads_end(r) {
                    continue;
                }
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

    /// The range of the fields that a field is held equal to by `~` or by an equation of two
    /// fields (`.resistance == .resistor.resistance`), when one of them has a value: the
    /// intersection of those that have one.
    fn held_equal(&self, field: FieldId) -> Option<Quantity> {
        use netsmith_check::RExpr;
        use netsmith_lang::ast::CmpOp;
        let mut class = HashSet::from([field]);
        loop {
            let before = class.len();
            for &(a, b) in self.model.equalities() {
                if class.contains(&a) || class.contains(&b) {
                    class.insert(a);
                    class.insert(b);
                }
            }
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
        let mut window: Option<Quantity> = None;
        let mut members: Vec<FieldId> = class.into_iter().filter(|&f| f != field).collect();
        members.sort_by_key(|f| f.0);
        for f in members {
            if let Domain::Quantity(q) = self.domain(f) {
                window = match window {
                    None => Some(q),
                    Some(w) => w.intersect(&q).ok().flatten().or(Some(w)),
                };
            }
        }
        window
    }

    /// The load of each regulator, enclosed before the picks from what the loads on its output
    /// say (ADR 0076, amendment 11). A regulator's output current is worked out by the balance of
    /// its net when every other current on the net has a value; while a load waits on a pick (a
    /// pull-up whose resistor is not chosen yet) the balance cannot be worked, the picks of the
    /// parts in front of the regulator (a fuse, a shunt, a bead) are made with no current to
    /// judge a rating against, and the first part of a catalog that has none refused is taken.
    ///
    /// Here the balances are worked once with every column of every part still to be picked at
    /// the hull of its candidates, and a regulator whose output current they give (the sum of what
    /// the loads state, each over every pick it could have) has it written as a worked-out
    /// current, as a balance writes one: an enclosure that the balances narrow as the picks make
    /// the loads known. A load that states nothing leaves the balance with two unknown currents
    /// and the regulator with none: its rated limit is the enclosure the start takes, as before.
    /// The model is put back as it was, but for those currents.
    pub(super) fn enclose_regulator_loads(&mut self, jobs: &[super::Job]) {
        use netsmith_check::DcKind;
        let outs: Vec<FieldId> = self
            .model
            .dc_elements()
            .filter(|(part, element)| {
                matches!(
                    element.kind,
                    DcKind::LinearRegulator | DcKind::SwitchingRegulator
                ) && self.model.instance(*part).kind == netsmith_check::InstKind::Part
            })
            .filter_map(|(_, element)| self.model.draw_of(element.pins[1]).map(|(f, _)| f))
            .filter(|&f| matches!(self.domain(f), Domain::Unset))
            .collect();
        if outs.is_empty() || jobs.is_empty() {
            return;
        }
        let mark = self.mark();
        for job in jobs {
            for (field, key) in &job.columns {
                if !matches!(self.domain(*field), Domain::Unset) {
                    continue;
                }
                if let Some((dim, lo, hi)) = self.column_hull(job, *field, key)
                    && let Ok(q) = Quantity::interval(dim, lo, hi)
                {
                    // A value the design holds the column equal to (a pull-up's resistance, the
                    // window of the module that holds the resistor) is a bound the hull of the
                    // catalog has to lie in as well: every candidate that is picked does.
                    let q = match self.held_equal(*field) {
                        Some(w) => q.intersect(&w).ok().flatten().unwrap_or(q),
                        None => q,
                    };
                    self.model.set_domain(*field, Domain::Quantity(q));
                }
            }
        }
        let _ = self.propagate();
        let found: Vec<(FieldId, Quantity)> = outs
            .into_iter()
            .filter_map(|f| match self.domain(f) {
                // The regulator supplies: its current is never above zero.
                //
                // A sum that is not inside the limit the regulator is rated for is no
                // enclosure of use (a load that waits on a pick, at the hull of its catalog,
                // is wider than what the part can supply): the limit is then the load, as the
                // start takes it, and nothing is written.
                Domain::Quantity(q)
                    if q.dim() == Dimension::CURRENT
                        && q.lo().is_finite()
                        && q.hi().is_finite()
                        && q.hi() <= 1e-12
                        && dc::limit_of(&self.model, f).is_none_or(|max| -q.lo() <= max) =>
                {
                    Some((f, q))
                }
                _ => None,
            })
            .collect();
        self.restore(&mark);
        for (field, q) in found {
            self.set_field(field, Domain::Quantity(q));
            self.derived.insert(field);
        }
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
        let outcomes = dc::analyze(&self.model, true, &enclosed, &hulled).outcomes;
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
            self.dc_field(part, field, solved, ends, false);
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
            // The end of a range read now may be stale once the range narrows (ADR 0098).
            if relation_reads_end(r) {
                continue;
            }
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

    /// A switching regulator's efficiency is a fraction: above 0 and at most 1. One written as a
    /// percentage in a plain number (`85..95`) is a converter that makes power, and the input
    /// current `Pout / (eff * Vin)` would come out a hundred times too small with every rating
    /// that reads it passing, so it is an error that names the part and the value, and the
    /// regulator is not modelled (ADR 0076, amendment 14). A value that is not yet a range with
    /// finite ends is left to the extraction, which leaves the regulator alone.
    fn dc_check_efficiencies(&mut self) {
        let mut found: Vec<(InstId, FieldId, Quantity)> = Vec::new();
        for (part, dc) in self.model.dc_elements() {
            if dc.kind != netsmith_check::DcKind::SwitchingRegulator
                || self.model.instance(part).kind != netsmith_check::InstKind::Part
            {
                continue;
            }
            let Some(&field) = dc.values.first() else {
                continue;
            };
            if let Domain::Quantity(q) = &self.model.field(field).domain
                && q.dim() == Dimension::NONE
                && q.lo().is_finite()
                && q.hi().is_finite()
                && (q.lo() <= 0.0 || q.hi() > 1.0)
            {
                found.push((part, field, *q));
            }
        }
        for (part, field, q) in found {
            let message = format!(
                "the efficiency of `{}` is {q}, which a switching regulator cannot have: it is a fraction above 0 and at most 1 (`0.85..0.95` or `85%..95%`), and the DC analysis does not model `{}` with it",
                self.model.path_of(part),
                self.model.path_of(part),
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
                draw: dc::net::Draw::Switching { p, eff, .. },
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
    fn dc_write(&mut self, m: &Modelled, run: &Rc<Run>, past_regulator: bool) -> usize {
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
            if self.dc_field(m.part, field, solved, ends, !past_regulator) {
                written += 1;
            }
        }
        written
    }

    /// Write what the circuit gives a field, or check it against what the design states.
    /// `whole_class` says whether a voltage that a statement gave another member of the field's
    /// `~` class is a budget (amendment 17); it is not for a regulator's input past another
    /// regulator's output, where the stated window is the rail the part holds (amendment 5,
    /// choice 3).
    fn dc_field(
        &mut self,
        part: InstId,
        field: FieldId,
        solved: Quantity,
        ends: DcEnds,
        whole_class: bool,
    ) -> bool {
        match self.domain(field) {
            Domain::Unset => {
                self.set_field(field, Domain::Quantity(solved));
                self.derived.insert(field);
                self.dc_ends.insert(field, ends);
                true
            }
            Domain::Quantity(d) => {
                // A voltage that a statement gave any member of the `~` class of the field is a
                // budget as much as one on the field (amendment 17, issue #284). A current is
                // judged on its own field: a `~>` chain joins the currents of its parts, and a
                // worked-out current beside a stated one is amendment 15's finding.
                let stated = if whole_class && d.dim() == Dimension::VOLTAGE {
                    self.stated_of(field)
                } else {
                    self.stated_in(&[field])
                };
                if let Some(by) = stated {
                    self.dc_check_stated(part, by, solved, &ends);
                    return false;
                }
                match d.intersect(&solved) {
                    Ok(Some(both)) if both != d => {
                        self.set_field(field, Domain::Quantity(both));
                        self.derived.insert(field);
                        self.dc_ends.insert(field, ends);
                        true
                    }
                    // The balance's number and the circuit's share no value: the design
                    // contradicts itself (or the model of a part does), and the relations that
                    // read the field are judged on the balance's number alone. Say so (ADR 0076,
                    // amendment 15); the field is left as it is.
                    Ok(None) => {
                        if d.dim() == Dimension::CURRENT {
                            self.dc_disagree(part, field, d, solved, &ends);
                        }
                        false
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// A current the solver worked out (a balance, an equation) and the DC analysis find in
    /// ranges that cannot both be true: an error that names the field, both ranges and the
    /// states at the analysis's nearest end. It is kept until the supplies' budgets are judged
    /// (`dc_report_disagreements`).
    fn dc_disagree(
        &mut self,
        part: InstId,
        field: FieldId,
        worked: Quantity,
        solved: Quantity,
        ends: &DcEnds,
    ) {
        let name = self.model.path_of_field(field);
        // The end of the analysis's range that is nearest the other range is the one that
        // explains the miss: the low end when the analysis is above, the high end below.
        let above = solved.lo() > worked.hi();
        let states = ends.states(!above);
        let nearest = if above { "least" } else { "most" };
        let message = format!(
            "`{name}` is inconsistent: the solver worked it out as {worked}, but the DC analysis of the circuit around it finds {solved}, and the two share no value (the {nearest} the circuit gives{})",
            if states.is_empty() {
                String::new()
            } else {
                format!(", with {states}")
            }
        );
        let (source, span) = self.model.instance(part).at();
        self.dc_disagreements.push((source, span, message));
    }

    /// The first of `members` (the fields joined by `~`) that a statement narrowed and the
    /// analysis did not write: the one the design states.
    fn stated_in(&self, members: &[FieldId]) -> Option<FieldId> {
        members.iter().copied().find(|&f| {
            !self.model.field(f).narrowed_at.is_empty()
                && !self.derived.contains(&f)
                && matches!(self.domain(f), Domain::Quantity(_))
        })
    }

    /// The field of the `~` class of `field` that the design states, if any.
    fn stated_of(&self, field: FieldId) -> Option<FieldId> {
        let mut members: Vec<FieldId> = self.equated(field).into_iter().collect();
        members.sort();
        self.stated_in(&members)
    }

    /// A stated range is a budget: what the circuit needs has to lie inside it. `stated` is the
    /// field the statement is on (it names the field and points at the line), `part` the part
    /// or rail to point at when no statement can be found.
    fn dc_check_stated(&mut self, part: InstId, field: FieldId, solved: Quantity, ends: &DcEnds) {
        let Domain::Quantity(stated) = self.domain(field) else {
            return;
        };
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

pub(super) type SupplyDraws = Vec<(dc::Supply, f64, f64, Rc<Run>)>;

pub(super) type DcEndsMap = HashMap<FieldId, DcEnds>;
