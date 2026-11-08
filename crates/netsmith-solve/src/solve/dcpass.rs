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

use std::collections::HashMap;
use std::rc::Rc;

use netsmith_check::{Domain, FieldId, InstId};
use netsmith_quantity::{Dimension, Quantity};

use crate::dc::{self, Error, Modelled, Which};
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
        for outcome in dc::analyze(&self.model) {
            netsmith_core::job::checkpoint();
            summary.islands += 1;
            let dc::Outcome { planned, result } = outcome;
            match result {
                Ok(report) => {
                    summary.solved += 1;
                    summary.combinations += report.combinations;
                    let run = Rc::new(Run {
                        island: planned.island,
                        report,
                    });
                    for m in &planned.parts {
                        summary.written += self.dc_write(m, &run);
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
        (summary, notes)
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
