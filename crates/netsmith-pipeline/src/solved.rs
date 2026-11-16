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

use netsmith_check::{Domain, Model};
use serde_json::{Map, Value, json};

use crate::Built;

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
    })
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
