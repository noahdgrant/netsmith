//! A board's environment against the temperatures of its parts (ADR 0099).
//!
//! The board's `.environment.temperature` is a range, and every fitted part (not a mounting
//! hole, a fiducial or a label) is judged against it, in this order of what says the part's
//! range: the design (`.temp_min`, `.temp_max` of the part), else the catalog row it was picked as,
//! else the cited range of the row's dielectric class (`netsmith_check::environment::ClassRange`,
//! a class's property and not a datasheet's, which every message that rests on it says).
//! A part whose range does not cover the environment is refused here, as an error (the solver
//! already left such rows out of the choice, so a refusal here is a range the design states, or
//! a row held by a pin). A part that states none, at one end or both, is kept, and a guideline
//! finding names it: a pick the check could not judge is never passed in silence.
//!
//! A board that also states `.environment.qualification &= Qualification::Aec` has every fitted
//! part judged the same way against its qualification: the design's own `.qualification` of the
//! part, else the catalog row's. A part that says `Aec` is met, one that says `None` (its
//! datasheet says the ordering code is not qualified) is refused, and one that says nothing is
//! kept and named in a guideline finding.

use std::collections::BTreeMap;

use netsmith_check::environment::{
    Fit, QualificationFit, requires_qualification, show, show_range,
};
use netsmith_check::{Domain, InstId, Member, Model};
use netsmith_solve::Pick;

use crate::Assembly;
use crate::guidelines::Guideline;

/// What the design stage knows about a part for this judgement. `fab` makes the same record for a
/// generic part once it has resolved it to a catalog row (ADR 0047), with the row's range in `pick`.
pub struct Fitted<'a> {
    pub part: InstId,
    pub kind: &'a str,
    pub assembly: Assembly,
    pub pick: Option<&'a Pick>,
}

/// What a judgement did, for the line of `fab-notes.txt` about the environment: how many fitted
/// parts were compared with the board's range and how many were not. A part is counted once.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tally {
    /// The board's range, in kelvin, and the path of the field that states it: `None` when the
    /// board has an environment that states no temperature, so that no part was judged.
    pub range: Option<((f64, f64), String)>,
    /// Parts whose stated range covers the environment at both ends.
    pub met: usize,
    /// Parts whose range does not cover it: each is an error that stops the run.
    pub not_met: usize,
    /// Parts kept, with a range stated at one end only or at neither (a row with no `temp_min`
    /// or `temp_max`, and the design stating none).
    pub no_range: usize,
    /// Parts that no catalog row stands for (a kind with no catalog), and the design states no
    /// range for.
    pub no_row: usize,
    /// Generic parts (ADR 0047) whose row `fab` finds, and then judges: not judged yet.
    pub pending: Vec<InstId>,
    /// Generic parts the design stage found unjudged for the qualification the board requires,
    /// and which are not in `pending` (their temperatures were judged): `fab` judges their
    /// qualification with the row it finds. Not counted in the line of `fab-notes.txt`, which is
    /// about temperatures.
    pub qualification_pending: Vec<InstId>,
}

impl Tally {
    /// Adds what a second judgement found of other parts (`fab`'s, of the generic parts it
    /// resolved).
    pub fn add(&mut self, other: &Tally) {
        self.met += other.met;
        self.not_met += other.not_met;
        self.no_range += other.no_range;
        self.no_row += other.no_row;
    }

    /// How many parts were not judged, at one end or both.
    pub fn unjudged(&self) -> usize {
        self.no_range + self.no_row + self.pending.len()
    }

    /// The lines of `fab-notes.txt`: what the check did and did not do (ADR 0099). Plain, and
    /// the same for every board that has an environment.
    pub fn notes(&self) -> Vec<String> {
        let mut lines = Vec::new();
        match &self.range {
            None => lines.push(
                "environment: the board has an environment that states no temperature, so no part was compared with it".to_string(),
            ),
            Some(((lo, hi), path)) => {
                let mut why: Vec<String> = Vec::new();
                if self.no_range > 0 {
                    why.push(format!(
                        "{} with a range stated at one end only or at neither",
                        self.no_range
                    ));
                }
                if self.no_row > 0 {
                    why.push(format!("{} with no catalog row", self.no_row));
                }
                if !self.pending.is_empty() {
                    why.push(format!(
                        "{} generic with no row resolved",
                        self.pending.len()
                    ));
                }
                let why = if why.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", why.join(", "))
                };
                lines.push(format!(
                    "environment: temperature of each fitted part against `{path}` {}: {} met, {} NOT MET, {} UNJUDGED{why}",
                    show_range(*lo, *hi),
                    self.met,
                    self.not_met,
                    self.unjudged(),
                ));
            }
        }
        lines.push(
            "environment not checked: radiation, outgassing, screening and lot traceability (no catalog row states them), and the part's own heating"
                .to_string(),
        );
        lines
    }
}

/// What [`judge`] made of a board.
#[derive(Debug, Default)]
pub struct Judged {
    /// The guideline findings.
    pub findings: Vec<Guideline>,
    /// The counts, for a board that has an environment; `None` for one that has none.
    pub tally: Option<Tally>,
}

/// The parts that one row (or one kind with no row) left unjudged, each with the ends missing.
type Unjudged = Vec<(InstId, Vec<&'static str>)>;

/// A part's own statement of one end of its range, in kelvin, and the end that is the worse
/// of a range it states (the highest of the lowest temperature, the lowest of the highest).
fn stated(model: &Model, part: InstId, name: &str, hi: bool) -> Option<f64> {
    match model.member(part, name)? {
        Member::Field(f) => match &model.field(f).domain {
            Domain::Quantity(q) => Some(if hi { q.hi() } else { q.lo() }),
            _ => None,
        },
        _ => None,
    }
}

/// Judge every fitted part of a board against its environment: the temperatures and, where the
/// board requires one, the qualification. Errors are refusals, and the returned findings are
/// the guidelines. A board with no `.environment` is not judged and says nothing (a board with
/// none states no requirement). The tally counts the temperature judgement only: the line of
/// `fab-notes.txt` is about temperatures, and a qualification is reported by its findings and
/// errors.
pub fn judge(model: &Model, parts: &[Fitted<'_>], errors: &mut Vec<String>) -> Judged {
    let mut judged = judge_temperatures(model, parts, errors);
    let (findings, waiting) = judge_qualification(model, parts, errors);
    judged.findings.extend(findings);
    if let Some(tally) = &mut judged.tally {
        tally.qualification_pending = waiting
            .into_iter()
            .filter(|p| !tally.pending.contains(p))
            .collect();
    }
    judged
}

/// What groups the parts of one finding: one catalog row, or one kind with no row.
fn group_key(fitted: &Fitted<'_>) -> (bool, String) {
    match fitted.pick {
        Some(p) if p.generic.is_none() => (true, format!("{} {}", p.catalog, p.candidate)),
        Some(p) => (false, format!("{} generic", p.catalog)),
        None => (false, fitted.kind.to_string()),
    }
}

/// What a message calls a part's type: the name it was placed as, else its kind.
fn type_of(model: &Model, fitted: &Fitted<'_>) -> String {
    model
        .instance(fitted.part)
        .placed_as
        .clone()
        .unwrap_or_else(|| fitted.kind.to_string())
}

fn judge_temperatures(model: &Model, parts: &[Fitted<'_>], errors: &mut Vec<String>) -> Judged {
    let board = model.root;
    let Some(Member::Inst(environment)) = model.member(board, "environment") else {
        return Judged::default();
    };
    let Some(field) = model.environment_temperature_of(board) else {
        // A member called `environment` that is not an environment: say which field is missing.
        errors.push(
            "`environment` must be an environment such as `std::env::Environment`, with a `temperature` (`.environment.temperature &= -40degC..125degC`)"
                .to_string(),
        );
        return Judged::default();
    };
    let path = model.path_of_field(field);
    let (lo, hi) = match &model.field(field).domain {
        Domain::Quantity(q) => (q.lo(), q.hi()),
        _ => {
            return Judged {
                findings: vec![Guideline {
                    rule: "environment",
                    message: format!(
                        "the board has an environment and `{path}` states no temperature, so no part is judged against it: state it, `.{path} &= -40degC..125degC`"
                    ),
                    at: Some(environment),
                }],
                tally: Some(Tally::default()),
            };
        }
    };
    let range = show_range(lo, hi);
    let mut tally = Tally {
        range: Some(((lo, hi), path.clone())),
        ..Tally::default()
    };
    // The parts nothing judged, by what to fix: a catalog row, or a kind with no row.
    let mut unjudged: BTreeMap<(bool, String), Unjudged> = BTreeMap::new();
    let mut types: BTreeMap<(bool, String), String> = BTreeMap::new();
    for fitted in parts {
        if fitted.assembly == Assembly::None
            || model.instance(fitted.part).marks.declared_kind
                == Some(netsmith_core::PartKind::Label)
        {
            continue;
        }
        let part = fitted.part;
        let (min, max) = (
            stated(model, part, "temp_min", true),
            stated(model, part, "temp_max", false),
        );
        let row_min = fitted.pick.and_then(|p| p.temp_min);
        let row_max = fitted.pick.and_then(|p| p.temp_max);
        // The range of the dielectric class of the row fills an end that neither the design nor
        // the row states. A generic part has no row yet (its row is found at `fab`), so it has no
        // class here either.
        let class = fitted
            .pick
            .filter(|p| p.generic.is_none())
            .and_then(|p| p.temp_class);
        let (min, max) = (
            min.or(row_min).or(class.map(|c| c.min)),
            max.or(row_max).or(class.map(|c| c.max)),
        );
        let fit = Fit::of((lo, hi), min, max);
        let name = model.path_of(part);
        if fit.refused() {
            tally.not_met += 1;
            let from = |own: Option<f64>, row: Option<f64>, member: &str| match (own, row) {
                (Some(_), _) => format!("the design states `.{name}.{member}`"),
                (None, Some(_)) => format!(
                    "the catalog row {}{} states `{member}`",
                    fitted.pick.map_or("", |p| p.candidate.as_str()),
                    fitted
                        .pick
                        .and_then(|p| p.lcsc.as_deref())
                        .map_or(String::new(), |lcsc| format!(" ({lcsc})"))
                ),
                (None, None) => class.map(|c| c.from_class()).unwrap_or_default(),
            };
            if fit.hot == Some(false) {
                errors.push(format!(
                    "`{name}` works up to {} only ({}), under the board's environment, which reaches {} (`{path}` {range}): choose a part rated for it or narrow the environment",
                    show(max.unwrap_or(0.0)),
                    from(stated(model, part, "temp_max", false), row_max, "temp_max"),
                    show(hi),
                ));
            }
            if fit.cold == Some(false) {
                errors.push(format!(
                    "`{name}` works down to {} only ({}), over the board's environment, which reaches {} (`{path}` {range}): choose a part rated for it or narrow the environment",
                    show(min.unwrap_or(0.0)),
                    from(stated(model, part, "temp_min", true), row_min, "temp_min"),
                    show(lo),
                ));
            }
        } else if fit.unjudged() {
            let key = group_key(fitted);
            match fitted.pick {
                Some(p) if p.generic.is_none() => tally.no_range += 1,
                // A generic part has its row when `fab` resolves it, and is judged then.
                Some(_) => tally.pending.push(part),
                None => tally.no_row += 1,
            }
            types
                .entry(key.clone())
                .or_insert_with(|| type_of(model, fitted));
            unjudged.entry(key).or_default().push((part, fit.missing()));
        } else {
            tally.met += 1;
        }
    }
    let mut findings: Vec<Guideline> = Vec::new();
    for (key, mut group) in unjudged {
        group.sort_by_key(|(p, _)| *p);
        let names: Vec<String> = group
            .iter()
            .map(|(p, _)| format!("`{}`", model.path_of(*p)))
            .collect();
        let missing = group[0].1.clone();
        let ends = match missing.as_slice() {
            ["temp_min"] => "no `temp_min`",
            ["temp_max"] => "no `temp_max`",
            _ => "no `temp_min` and no `temp_max`",
        };
        let first = model.path_of(group[0].0);
        // The shape of the statement, with a figure that a datasheet could print: the place to
        // write the part's own number is the part's, and a figure copied from the environment
        // would only silence the finding.
        let example = match missing.last() {
            Some(&"temp_min") => {
                format!("`.{first}.temp_min &= -40degC` for a datasheet that says -40degC")
            }
            _ => format!("`.{first}.temp_max &= 85degC` for a datasheet that says 85degC"),
        };
        let ty = &types[&key];
        let (is_row, what) = &key;
        let message = if *is_row {
            let row = what.split_once(' ').map_or(what.as_str(), |(_, row)| row);
            format!(
                "{} ({ty}) {} picked as {row} and its catalog row states {ends}, so {} not judged against the board's environment (`{path}` {range}): add the missing temperature of the datasheet to the row (`temp_min`, `temp_max`, such as `85degC`) or state it on the part, as {example}",
                names.join(", "),
                if group.len() > 1 { "were" } else { "was" },
                if group.len() > 1 { "they are" } else { "it is" },
            )
        } else {
            format!(
                "{} ({ty}) {} no catalog row to state a temperature range, and the design states {ends}, so {} not judged against the board's environment (`{path}` {range}): state it from the datasheet, as {example}",
                names.join(", "),
                if group.len() > 1 { "have" } else { "has" },
                if group.len() > 1 { "they are" } else { "it is" },
            )
        };
        findings.push(Guideline {
            rule: "environment",
            message,
            at: Some(group[0].0),
        });
    }
    Judged {
        findings,
        tally: Some(tally),
    }
}

/// A part's own statement of its qualification, `Aec` or `None`, if the design states one.
fn stated_qualification(model: &Model, part: InstId) -> Option<String> {
    match model.member(part, "qualification")? {
        Member::Field(f) => match &model.field(f).domain {
            Domain::Text(variant) => Some(variant.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Judge every fitted part against the qualification the board's environment requires
/// (`.environment.qualification &= Qualification::Aec`). A board that states none, or states
/// `Qualification::None`, requires nothing and says nothing. The second answer is the generic
/// parts that have no row yet, which `fab` judges again once it has resolved them.
pub fn judge_qualification(
    model: &Model,
    parts: &[Fitted<'_>],
    errors: &mut Vec<String>,
) -> (Vec<Guideline>, Vec<InstId>) {
    let board = model.root;
    let Some(field) = model.environment_qualification_of(board) else {
        return (Vec::new(), Vec::new());
    };
    let required = match &model.field(field).domain {
        Domain::Text(variant) => Some(variant.as_str()),
        _ => None,
    };
    if !requires_qualification(required) {
        return (Vec::new(), Vec::new());
    }
    let path = model.path_of_field(field);
    let mut unjudged: BTreeMap<(bool, String), Vec<InstId>> = BTreeMap::new();
    let mut types: BTreeMap<(bool, String), String> = BTreeMap::new();
    // The generic parts with no row yet: `fab` judges them again with the row it finds.
    let mut waiting: Vec<InstId> = Vec::new();
    for fitted in parts {
        if fitted.assembly == Assembly::None
            || model.instance(fitted.part).marks.declared_kind
                == Some(netsmith_core::PartKind::Label)
        {
            continue;
        }
        let part = fitted.part;
        let own = stated_qualification(model, part);
        let row = fitted.pick.and_then(|p| p.qualification.clone());
        let claim = own.clone().or(row.clone());
        let name = model.path_of(part);
        match QualificationFit::of(claim.as_deref()) {
            QualificationFit::Met => {}
            QualificationFit::NotMet => {
                let from = match (&own, &row) {
                    (Some(_), _) => format!("the design states `.{name}.qualification`"),
                    _ => format!(
                        "the catalog row {} states `qualification` `None`, which is its datasheet's: the ordering code is not the qualified one",
                        fitted.pick.map_or("", |p| p.candidate.as_str())
                    ),
                };
                errors.push(format!(
                    "`{name}` is not qualified ({from}), but the board's environment requires `Qualification::Aec` (`{path}`): choose a part whose datasheet says it is qualified, or drop the requirement"
                ));
            }
            QualificationFit::Unjudged => {
                let key = group_key(fitted);
                types
                    .entry(key.clone())
                    .or_insert_with(|| type_of(model, fitted));
                if fitted.pick.is_some_and(|p| p.generic.is_some()) {
                    waiting.push(part);
                }
                unjudged.entry(key).or_default().push(part);
            }
        }
    }
    let mut findings = Vec::new();
    for (key, mut group) in unjudged {
        group.sort();
        let names: Vec<String> = group
            .iter()
            .map(|p| format!("`{}`", model.path_of(*p)))
            .collect();
        let first = model.path_of(group[0]);
        let example = format!(
            "`.{first}.qualification &= Qualification::Aec` for a datasheet that says the part is AEC qualified"
        );
        let ty = &types[&key];
        let (is_row, what) = &key;
        let many = group.len() > 1;
        let message = if *is_row {
            let row = what.split_once(' ').map_or(what.as_str(), |(_, row)| row);
            format!(
                "{} ({ty}) {} picked as {row} and its catalog row states no `qualification`, so {} not judged against the board's environment, which requires `Qualification::Aec` (`{path}`): add `qualification` of the datasheet to the row (`Aec` where it says the part is qualified, `None` where it says it is not) or state it on the part, as {example}",
                names.join(", "),
                if many { "were" } else { "was" },
                if many { "they are" } else { "it is" },
            )
        } else {
            format!(
                "{} ({ty}) {} no catalog row to state a qualification, and the design states none, so {} not judged against the board's environment, which requires `Qualification::Aec` (`{path}`): state it from the datasheet, as {example}",
                names.join(", "),
                if many { "have" } else { "has" },
                if many { "they are" } else { "it is" },
            )
        };
        findings.push(Guideline {
            rule: "environment",
            message,
            at: Some(group[0]),
        });
    }
    (findings, waiting)
}
