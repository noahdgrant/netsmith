//! The values the solve left each member of a board with, as JSON (issue #744).
//!
//! `netsmith build --emit-solved <file>` writes them, so a person can read the range of current an
//! inductor carries, the voltage a chain leaves at a node or the power in a resistor without making
//! a check fail. The file is a JSON array with one object per board, in the order the run built
//! them: the board (`System/Board` for a board of a system) and its members, each a member that
//! the source, a default, a catalog row or the solve gave a value, in the order the model made
//! them (a part's members together, in the order its definition declares them).
//!
//! A quantity is its `unit` (the symbol of its SI unit, or the dimension in words when it has no
//! named unit, or `""` for a plain number) and its `range` in that unit, `[lo, hi]`, with
//! `"inf"` and `"-inf"` for an end that is open, as the design file writes one; text and the
//! variant of an enum are a `value`. Every member also has `shown`, the value as a message prints
//! it. The same board is always the same bytes.
//!
//! A board also has its `picks`, one for each part the solver chose a catalog row for, in the
//! order it chose them, with why each row before it was passed over (issue
//! #744): the part's `path`, the `catalog`, the `row` and, for a bank of more than one, its `count`; `held`, what
//! held the part to its row when no other row was tried (a pick lock, a stated choice); and
//! `passed`, the rows the part's turn passed over: first those before the row in catalog order
//! that do not fit what the design states, then the rows the turn tried, in the order it tried
//! them. Each has its `row`, a bank's `count`, `why` (`unfit`, `refused` by the first relation
//! it failed, `later` when the parts after it in its group could not be chosen with it, `same`
//! as a row passed over before it, `skipped` when the search found without a trial that one
//! would refuse it, `partly` when a bound that waits holds for only some of its values), the
//! `message`, and for a refusal `at`, the `file:line` of the relation. The turn is the one that
//! made the pick: what an earlier turn tried with other choices of the parts before it is not
//! there.

use netsmith_check::{Domain, Model};
use serde_json::{Map, Value, json};

use crate::Built;
use netsmith_check::Sources;
use netsmith_solve::Pick;

/// The text of a solved file of the boards `boards` holds ([`to_value`] of each), ending with a
/// newline.
pub fn to_json(boards: Vec<Value>) -> String {
    let mut text = serde_json::to_string_pretty(&Value::Array(boards))
        .expect("a solved file always serialises to JSON");
    text.push('\n');
    text
}

/// One board of a solved file: its name and its members.
pub fn to_value(built: &Built) -> Value {
    json!({
        "board": built.label(),
        "members": members(&built.trace.model),
        "picks": picks(&built.picks, &built.trace.model, &built.trace.sources),
    })
}

/// The picks of a board as the solved file has them, with the rows each passed over.
pub fn picks(picks: &[Pick], model: &Model, sources: &Sources) -> Vec<Value> {
    picks
        .iter()
        .map(|p| {
            let mut pick = Map::new();
            pick.insert("path".into(), model.path_of(p.part).into());
            pick.insert("catalog".into(), p.catalog.clone().into());
            pick.insert("row".into(), p.candidate.clone().into());
            if p.count != 1 {
                pick.insert("count".into(), p.count.into());
            }
            if let Some(held) = &p.held {
                pick.insert("held".into(), held.clone().into());
            }
            let passed: Vec<Value> = p
                .passed
                .iter()
                .map(|r| {
                    let mut row = Map::new();
                    row.insert("row".into(), r.row.clone().into());
                    if let Some(n) = r.count {
                        row.insert("count".into(), n.into());
                    }
                    row.insert("why".into(), r.why.word().into());
                    row.insert("message".into(), r.message.clone().into());
                    if let Some(at) = r.at.and_then(|(source, span)| sources.place(source, span)) {
                        row.insert("at".into(), at.into());
                    }
                    Value::Object(row)
                })
                .collect();
            pick.insert("passed".into(), passed.into());
            Value::Object(pick)
        })
        .collect()
}

/// The members of `model` that have a value, as the solved file has them.
pub fn members(model: &Model) -> Vec<Value> {
    model
        .fields()
        .filter_map(|(id, field)| {
            let mut member = Map::new();
            member.insert("path".into(), model.path_of_field(id).into());
            match &field.domain {
                Domain::Unset => return None,
                Domain::Quantity(q) => {
                    let unit = match q.dim().unit_symbol() {
                        Some(symbol) => symbol.to_string(),
                        None if q.dim().is_dimensionless() => String::new(),
                        None => q.dim().to_string(),
                    };
                    member.insert("unit".into(), unit.into());
                    member.insert("range".into(), json!([end(q.lo()), end(q.hi())]));
                }
                Domain::Text(text) => {
                    member.insert("value".into(), text.clone().into());
                }
            }
            member.insert("shown".into(), model.show_value(id, &field.domain).into());
            Some(Value::Object(member))
        })
        .collect()
}

/// An end of a range: a number, or the word for an end JSON has no number for.
fn end(value: f64) -> Value {
    if value.is_finite() {
        json!(value)
    } else if value.is_nan() {
        "nan".into()
    } else if value > 0.0 {
        "inf".into()
    } else {
        "-inf".into()
    }
}
