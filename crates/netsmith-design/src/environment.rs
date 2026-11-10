//! A board's environment against the temperatures of its parts (ADR 0099).
//!
//! The board's `.environment.temperature` is a range, and every fitted part (not a mounting
//! hole, a fiducial or a label) is judged against it, in this order of what says the part's
//! range: the design (`.temp_min`, `.temp_max` of the part), else the catalog row it was picked as.
//! A part whose range does not cover the environment is refused here, as an error (the solver
//! already left such rows out of the choice, so a refusal here is a range the design states, or
//! a row held by a pin). A part that states none, at one end or both, is kept, and a guideline
//! finding names it: a pick the check could not judge is never passed in silence.

use std::collections::BTreeMap;

use netsmith_check::environment::{Fit, show, show_range};
use netsmith_check::{Domain, InstId, Member, Model};
use netsmith_solve::Pick;

use crate::Assembly;
use crate::guidelines::Guideline;

/// What the design stage knows about a part for this judgement.
pub(crate) struct Fitted<'a> {
    pub part: InstId,
    pub kind: &'a str,
    pub assembly: Assembly,
    pub pick: Option<&'a Pick>,
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

/// Judge every fitted part of a board against its environment. Errors are refusals, and the
/// returned findings are the guidelines. A board with no `.environment` is not judged and says
/// nothing (a board with none states no requirement).
pub(crate) fn judge(
    model: &Model,
    parts: &[Fitted<'_>],
    errors: &mut Vec<String>,
) -> Vec<Guideline> {
    let board = model.root;
    let Some(Member::Inst(environment)) = model.member(board, "environment") else {
        return Vec::new();
    };
    let Some(field) = model.environment_temperature_of(board) else {
        // A member called `environment` that is not an environment: say which field is missing.
        errors.push(
            "`environment` must be an environment such as `std::env::Environment`, with a `temperature` in kelvin (`.environment.temperature &= 233.15K..398.15K` is -40 C to 125 C)"
                .to_string(),
        );
        return Vec::new();
    };
    let path = model.path_of_field(field);
    let (lo, hi) = match &model.field(field).domain {
        Domain::Quantity(q) => (q.lo(), q.hi()),
        _ => {
            return vec![Guideline {
                rule: "environment",
                message: format!(
                    "the board has an environment and `{path}` states no temperature, so no part is judged against it: state it, `.{path} &= 233.15K..398.15K` is -40 C to 125 C"
                ),
                at: Some(environment),
            }];
        }
    };
    let range = show_range(lo, hi);
    // The parts nothing judged, by what to fix: a catalog row, or a kind with no row.
    let mut unjudged: BTreeMap<(bool, String), Unjudged> = BTreeMap::new();
    let mut types: BTreeMap<(bool, String), String> = BTreeMap::new();
    for fitted in parts {
        if fitted.assembly == Assembly::None || fitted.kind == "Label" {
            continue;
        }
        let part = fitted.part;
        let (min, max) = (
            stated(model, part, "temp_min", true),
            stated(model, part, "temp_max", false),
        );
        let row_min = fitted.pick.and_then(|p| p.temp_min);
        let row_max = fitted.pick.and_then(|p| p.temp_max);
        let (min, max) = (min.or(row_min), max.or(row_max));
        let fit = Fit::of((lo, hi), min, max);
        let name = model.path_of(part);
        if fit.refused() {
            let from = |own: Option<f64>, row: Option<f64>, member: &str| match (own, row) {
                (Some(_), _) => format!("the design states `.{name}.{member}`"),
                (None, Some(_)) => format!(
                    "the catalog row {} states `{member}`",
                    fitted.pick.map_or("", |p| p.candidate.as_str())
                ),
                (None, None) => String::new(),
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
            let key = match fitted.pick {
                Some(p) if p.generic.is_none() => (true, format!("{} {}", p.catalog, p.candidate)),
                Some(p) => (false, format!("{} generic", p.catalog)),
                None => (false, fitted.kind.to_string()),
            };
            types.entry(key.clone()).or_insert_with(|| {
                model
                    .instance(part)
                    .placed_as
                    .clone()
                    .unwrap_or_else(|| fitted.kind.to_string())
            });
            unjudged.entry(key).or_default().push((part, fit.missing()));
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
                format!("`.{first}.temp_min &= 233.15K` for a datasheet that says -40 C")
            }
            _ => format!("`.{first}.temp_max &= 358.15K` for a datasheet that says 85 C"),
        };
        let ty = &types[&key];
        let (is_row, what) = &key;
        let message = if *is_row {
            let row = what.split_once(' ').map_or(what.as_str(), |(_, row)| row);
            format!(
                "{} ({ty}) {} picked as {row} and its catalog row states {ends}, so {} not judged against the board's environment (`{path}` {range}): add the missing temperature of the datasheet to the row (`temp_min`, `temp_max`, in kelvin) or state it on the part, as {example}",
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
    findings
}
