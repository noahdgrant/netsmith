//! The analysis: every combination of the corners of the stated ranges and of the states of the
//! elements, each solved as one operating point, and the range each quantity takes over all
//! of them.
//!
//! Interval arithmetic would carry each range through the equations as if every use of a value
//! could be at a different end of its range (`V * R2 / (R1 + R2)` with `R1` and `R2` both
//! `1 kohm +/- 1 %` is `4.90 V..5.10 V` that way, and `4.95 V..5.05 V` for the circuit). A corner
//! solve evaluates the circuit with one value for each: nothing is counted twice.
//!
//! What a corner enumeration does not promise: its range is the hull of the corners, which is
//! the range of the circuit only where the quantity is monotone in each value. A node voltage
//! or a branch current of a network with one source and no bridge is. A power
//! (`V^2 / R`) can have its largest inside the box, and a ladder with two sources can have a
//! node voltage that rises and falls. [`check_interior`] samples the inside of the box and
//! says how many results fell outside the hull.

use crate::{
    netlist::{Netlist, Point},
    op::{self, Fail},
    range::Range,
};

/// More combinations than this is an error: the island is too big to enumerate and must be cut
/// at a node whose voltage is held (a regulator's output, a rail).
pub const DEFAULT_MAX_COMBINATIONS: u64 = 1 << 22;

#[derive(Debug, PartialEq)]
pub enum Error {
    /// The product of the variants of the elements is more than the limit.
    TooManyCombinations { combinations: u64, limit: u64 },
    /// Every combination was inconsistent or failed: there is no operating point.
    NoOperatingPoint(String),
    /// An element has no variant (a logic pin with no state).
    EmptyElement(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::TooManyCombinations {
                combinations,
                limit,
            } => write!(
                f,
                "{combinations} combinations of corners and states is more than the limit of {limit}: cut the island at a node whose voltage is held"
            ),
            Error::NoOperatingPoint(why) => write!(f, "no operating point: {why}"),
            Error::EmptyElement(name) => write!(f, "`{name}` has no state to try"),
        }
    }
}

/// The result: for each quantity, its range, and the combination that made each end.
pub struct Report {
    pub labels: Vec<String>,
    pub ranges: Vec<Range>,
    at_lo: Vec<u64>,
    at_hi: Vec<u64>,
    /// For each element, the labels of its variants (for naming a combination).
    variant_labels: Vec<Vec<String>>,
    element_names: Vec<String>,
    /// How many combinations there were, how many solved, how many were inconsistent states.
    pub combinations: u64,
    pub solved: u64,
    pub inconsistent: u64,
    /// Combinations where the sources could not give the power asked: each is a finding.
    pub collapsed: Vec<(u64, String)>,
    /// The most iterations of a fixed point any combination took.
    pub max_iterations: usize,
    /// Regulators whose input was below the minimum in some combination: `(name, combination)`.
    pub below_min: Vec<(String, u64)>,
}

impl Report {
    pub fn index(&self, label: &str) -> Option<usize> {
        self.labels.iter().position(|l| l == label)
    }

    /// The range of a quantity by its label: `v(NODE)`, `i(r1)`, `p(r1)`, `i(reg.in)`.
    pub fn get(&self, label: &str) -> Option<Range> {
        self.index(label).map(|i| self.ranges[i])
    }

    /// The combination that gave the low or the high end of a quantity, in words: the variant
    /// each element had.
    pub fn produced_by(&self, label: &str, high: bool) -> Option<String> {
        let i = self.index(label)?;
        let c = if high { self.at_hi[i] } else { self.at_lo[i] };
        Some(self.describe(c))
    }

    /// A combination as the variants that differ from the first of each element, which is the
    /// lower end of every range: only the elements with a choice are named.
    pub fn describe(&self, mut combination: u64) -> String {
        let mut parts = Vec::new();
        for (e, labels) in self.variant_labels.iter().enumerate() {
            let n = labels.len() as u64;
            let k = (combination % n) as usize;
            combination /= n;
            if n > 1 {
                parts.push(format!("{}: {}", self.element_names[e], labels[k]));
            }
        }
        parts.join("; ")
    }

    /// Whether the combination count of the analysis is one a person would call cheap.
    pub fn solves(&self) -> u64 {
        self.solved + self.inconsistent + self.collapsed.len() as u64
    }
}

/// The variants of each element: a label and the element as a point.
type Variants = Vec<Vec<(String, Point)>>;

fn variant_counts(nl: &Netlist) -> Result<(Variants, u64), Error> {
    let mut all = Vec::new();
    let mut total: u64 = 1;
    for e in &nl.elements {
        let v = e.variants();
        if v.is_empty() {
            return Err(Error::EmptyElement(e.name.clone()));
        }
        total = total.saturating_mul(v.len() as u64);
        all.push(v);
    }
    Ok((all, total))
}

/// Solve every combination and collect the ranges.
pub fn analyze(nl: &Netlist) -> Result<Report, Error> {
    analyze_with_limit(nl, DEFAULT_MAX_COMBINATIONS)
}

pub fn analyze_with_limit(nl: &Netlist, limit: u64) -> Result<Report, Error> {
    let (variants, total) = variant_counts(nl)?;
    if total > limit {
        return Err(Error::TooManyCombinations {
            combinations: total,
            limit,
        });
    }
    let labels = nl.quantity_labels();
    let mut ranges = vec![Range::EMPTY; labels.len()];
    let mut at_lo = vec![0u64; labels.len()];
    let mut at_hi = vec![0u64; labels.len()];
    let mut report = Report {
        labels,
        ranges: Vec::new(),
        at_lo: Vec::new(),
        at_hi: Vec::new(),
        variant_labels: variants
            .iter()
            .map(|v| v.iter().map(|(l, _)| l.clone()).collect())
            .collect(),
        element_names: nl.elements.iter().map(|e| e.name.clone()).collect(),
        combinations: total,
        solved: 0,
        inconsistent: 0,
        collapsed: Vec::new(),
        max_iterations: 0,
        below_min: Vec::new(),
    };

    let mut points: Vec<Point> = variants.iter().map(|v| v[0].1.clone()).collect();
    let mut last_failure = String::new();
    for combination in 0..total {
        let mut c = combination;
        for (k, v) in variants.iter().enumerate() {
            let n = v.len() as u64;
            points[k] = v[(c % n) as usize].1.clone();
            c /= n;
        }
        match op::solve(&points, nl.node_count()) {
            Ok(o) => {
                report.solved += 1;
                report.max_iterations = report.max_iterations.max(o.iterations);
                for (i, &v) in o.values.iter().enumerate() {
                    if v < ranges[i].lo {
                        at_lo[i] = combination;
                    }
                    if v > ranges[i].hi {
                        at_hi[i] = combination;
                    }
                    ranges[i].include(v);
                }
                for k in o.below_min {
                    report
                        .below_min
                        .push((nl.elements[k].name.clone(), combination));
                }
            }
            Err(Fail::Inconsistent) => report.inconsistent += 1,
            Err(Fail::Singular) => {
                last_failure = "the equations are singular (a loop of voltage sources?)".into();
                report.inconsistent += 1;
            }
            Err(Fail::Collapse(why)) => {
                last_failure = why.clone();
                report.collapsed.push((combination, why));
            }
        }
    }
    if report.solved == 0 {
        return Err(Error::NoOperatingPoint(if last_failure.is_empty() {
            "no combination of states is consistent".to_string()
        } else {
            last_failure
        }));
    }
    report.ranges = ranges;
    report.at_lo = at_lo;
    report.at_hi = at_hi;
    Ok(report)
}

/// A deterministic stream of numbers in `0..1` (an xorshift): no dependency, and the same
/// samples on every run.
fn stream(seed: u64) -> impl FnMut() -> f64 {
    let mut s = seed | 1;
    move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Draw `samples` points at random inside the ranges and states, solve each, and count the
/// results that fell outside the report's range for their quantity. A count of zero is
/// evidence that the hull of the corners covers the box, not proof. Returns the quantities
/// (label, how many samples fell outside) that failed, and the number of samples solved.
pub fn check_interior(
    nl: &Netlist,
    report: &Report,
    samples: usize,
) -> (Vec<(String, usize)>, usize) {
    let mut rng = stream(0x9E37_79B9_7F4A_7C15);
    let mut outside = vec![0usize; report.labels.len()];
    let mut solved = 0;
    for _ in 0..samples {
        let points: Vec<Point> = nl.elements.iter().map(|e| e.sample(&mut rng)).collect();
        if let Ok(o) = op::solve(&points, nl.node_count()) {
            solved += 1;
            for (i, &v) in o.values.iter().enumerate() {
                let r = report.ranges[i];
                let slack = 1e-9 * r.abs_max() + 1e-15;
                if !r.contains(v, slack) {
                    outside[i] += 1;
                }
            }
        }
    }
    let failed = outside
        .iter()
        .enumerate()
        .filter(|&(_, &n)| n > 0)
        .map(|(i, &n)| (report.labels[i].clone(), n))
        .collect();
    (failed, solved)
}
