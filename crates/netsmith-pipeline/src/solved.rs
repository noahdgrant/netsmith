//! The values the solve left each member of a board with, as JSON (issue #744).
//!
//! `netsmith board build --emit-solved <file>` writes them, so a person can read the range of current an
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
//!
//! A board whose solve failed (issue 1008) is written too, with `"failed": true`: the `members`
//! and `picks` the solve got to (the parts of the groups that were chosen, and what the DC
//! analysis wrote), and `unpicked`, one entry for each part of a group that failed: its `path`,
//! the `catalog`, `with`, the choices of the parts before it in the group that its turn was
//! made with (left out for the first), `held` as for a pick, and `passed`, every row its first
//! turn passed over, in the shape a pick has. The first turn is the one the search made when it
//! first reached the part, with the parts before it at the first rows that fit.
//!
//! A board that is built also has its `pin_budgets` (issue 1284): for each host processor, the
//! pins and serial blocks each module uses of it and how many are left, as
//! [`crate::pin_budget`] says (the editor's `PinBudget`, field for field).
//!
//! [`explain`] reads the same values as text, for `netsmith board explain`.

use netsmith_check::{Domain, Model};
use netsmith_editor::api::PinBudget;
use serde_json::{Map, Value, json};

use crate::Built;
use netsmith_check::Sources;
use netsmith_solve::{Passed, Pick, Solution, Unpicked};

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
        "pin_budgets": serde_json::to_value(crate::pin_budget::of(built))
            .expect("a pin budget always serialises to JSON"),
    })
}

/// A board whose solve failed, as the solved file has it: what the solve got to, and for each
/// part of a group that failed the rows its first turn passed over. `label` names the board (or
/// the system, whose model holds every board of it).
pub fn failed_value(label: &str, solution: &Solution, sources: &Sources) -> Value {
    json!({
        "board": label,
        "failed": true,
        "members": members(&solution.model),
        "picks": picks(&solution.picks, &solution.model, sources),
        "unpicked": unpicked(&solution.unpicked, &solution.model, sources),
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
            pick.insert("passed".into(), passed(&p.passed, sources).into());
            Value::Object(pick)
        })
        .collect()
}

/// The parts of a failed group as the solved file has them.
fn unpicked(unpicked: &[Unpicked], model: &Model, sources: &Sources) -> Vec<Value> {
    unpicked
        .iter()
        .map(|u| {
            let mut part = Map::new();
            part.insert("path".into(), model.path_of(u.part).into());
            part.insert("catalog".into(), u.catalog.clone().into());
            if !u.with.is_empty() {
                part.insert("with".into(), u.with.clone().into());
            }
            if let Some(held) = &u.held {
                part.insert("held".into(), held.clone().into());
            }
            part.insert("passed".into(), passed(&u.passed, sources).into());
            Value::Object(part)
        })
        .collect()
}

/// The rows a turn passed over as the solved file has them.
fn passed(rows: &[Passed], sources: &Sources) -> Vec<Value> {
    rows.iter()
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

/// Whether `path` is `within` or inside it (`reg` holds `reg.ic.shdn_pullup`, not `register`).
fn inside(path: &str, within: &str) -> bool {
    path == within
        || path
            .strip_prefix(within)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// The pin budgets of the board's hosts at `path` or inside it (`mcu`, or `core` for
/// `core.mcu`).
fn budgets_at(board: &Value, path: &str) -> Vec<PinBudget> {
    board["pin_budgets"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|v| v["host"].as_str().is_some_and(|h| inside(h, path)))
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// How many paths [`explain`] lists when it finds nothing at the path it was given.
const LISTED: usize = 12;

/// The text `netsmith board explain` prints for `path` over the boards of a solved file (`to_value`
/// and `failed_value`): for each board, the record of each part at the path or inside it (the
/// row taken, what held it, the rows passed over and why; for a part of a group that failed, the
/// rows its first turn passed over), then the members at the path or inside it with the ranges
/// the solve left them. A part lists its first `rows` rows passed over when that is `Some`, and
/// says how many more there are. `Err` says that nothing is there, naming some parts that have a
/// record.
pub fn explain(boards: &[Value], path: &str, rows: Option<usize>) -> Result<String, String> {
    let path = path.trim().trim_matches('.');
    let mut out = String::new();
    for board in boards {
        let name = board["board"].as_str().unwrap_or("");
        let list = |key: &str| -> Vec<&Value> {
            board[key]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter(|v| v["path"].as_str().is_some_and(|p| inside(p, path)))
                        .collect()
                })
                .unwrap_or_default()
        };
        let (picks, unpicked, members) = (list("picks"), list("unpicked"), list("members"));
        let budgets = budgets_at(board, path);
        if picks.is_empty() && unpicked.is_empty() && members.is_empty() && budgets.is_empty() {
            continue;
        }
        let failed = board["failed"] == true;
        out.push_str(&format!(
            "board {name}{}\n",
            if failed { " (failed)" } else { "" }
        ));
        for pick in picks {
            out.push_str(&explain_pick(pick, true, rows));
        }
        for part in unpicked {
            out.push_str(&explain_pick(part, false, rows));
        }
        out.push_str(&crate::pin_budget::text(&budgets));
        if !members.is_empty() {
            let width = members
                .iter()
                .map(|m| m["path"].as_str().unwrap_or("").len())
                .max()
                .unwrap_or(0);
            out.push_str("  members:\n");
            for m in members {
                let shown = m["shown"].as_str().unwrap_or("");
                out.push_str(&format!(
                    "    {:width$}  {shown}\n",
                    m["path"].as_str().unwrap_or("")
                ));
            }
        }
    }
    if out.is_empty() {
        return Err(nothing_at(boards, path));
    }
    Ok(out)
}

/// The error [`explain`] and [`explain_json`] give when nothing is at `path`: it names some parts
/// that have a record.
fn nothing_at(boards: &[Value], path: &str) -> String {
    let mut parts: Vec<&str> = boards
        .iter()
        .flat_map(|b| {
            ["picks", "unpicked"]
                .into_iter()
                .filter_map(|k| b[k].as_array())
                .flatten()
        })
        .filter_map(|p| p["path"].as_str())
        .collect();
    parts.dedup();
    let more = parts.len().saturating_sub(LISTED);
    parts.truncate(LISTED);
    let mut named: Vec<String> = parts.iter().map(|p| format!("`{p}`")).collect();
    if more > 0 {
        named.push(format!("{more} more"));
    }
    if named.is_empty() {
        format!(
            "no member and no part with a pick at or inside `{path}`, and the boards have no picks"
        )
    } else {
        format!(
            "no member and no part with a pick at or inside `{path}`; parts the solver chose rows for: {}",
            named.join(", ")
        )
    }
}

/// What `netsmith board explain --format json` prints for `path`: the same record as [`explain`]
/// as data, `{"path", "boards": [...]}`. Each board that has something at the path is its
/// `board` name, `failed` when its solve failed, and the `picks`, `unpicked` and `members` at
/// the path or inside it, as the solved file has them ([`to_value`]); a part lists its first
/// `rows` rows `passed` over when that is `Some`, with `passed_more`, how many it leaves out.
/// `Err` is the message [`explain`] gives when nothing is there.
pub fn explain_json(boards: &[Value], path: &str, rows: Option<usize>) -> Result<Value, String> {
    let path = path.trim().trim_matches('.');
    let mut found = Vec::new();
    for board in boards {
        let list = |key: &str| -> Vec<Value> {
            board[key]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter(|v| v["path"].as_str().is_some_and(|p| inside(p, path)))
                        .map(|v| limited(v, rows))
                        .collect()
                })
                .unwrap_or_default()
        };
        let (picks, unpicked, members) = (list("picks"), list("unpicked"), list("members"));
        let budgets = budgets_at(board, path);
        if picks.is_empty() && unpicked.is_empty() && members.is_empty() && budgets.is_empty() {
            continue;
        }
        let mut one = Map::new();
        one.insert("board".into(), board["board"].clone());
        if board["failed"] == true {
            one.insert("failed".into(), true.into());
        }
        one.insert("picks".into(), picks.into());
        one.insert("unpicked".into(), unpicked.into());
        one.insert("members".into(), members.into());
        if !budgets.is_empty() {
            one.insert(
                "pin_budgets".into(),
                serde_json::to_value(&budgets).expect("a pin budget always serialises to JSON"),
            );
        }
        found.push(Value::Object(one));
    }
    if found.is_empty() {
        return Err(nothing_at(boards, path));
    }
    Ok(json!({ "path": path, "boards": found }))
}

/// A pick or a member with the rows it `passed` over cut to `rows`, and how many were left out.
fn limited(part: &Value, rows: Option<usize>) -> Value {
    let mut part = part.clone();
    let (Some(limit), Some(passed)) = (rows, part["passed"].as_array().cloned()) else {
        return part;
    };
    if passed.len() > limit {
        part["passed"] = passed[..limit].to_vec().into();
        part["passed_more"] = (passed.len() - limit).into();
    }
    part
}

/// One part's record as text: a pick (`picked`) or a part of a group that failed.
fn explain_pick(part: &Value, picked: bool, limit: Option<usize>) -> String {
    let path = part["path"].as_str().unwrap_or("");
    let catalog = part["catalog"].as_str().unwrap_or("");
    let mut out = if picked {
        let row = part["row"].as_str().unwrap_or("");
        let count = part["count"]
            .as_u64()
            .map_or(String::new(), |n| format!("{n} x "));
        format!("  {path}: {count}`{row}` from the `{catalog}` catalog\n")
    } else {
        format!("  {path}: no row of the `{catalog}` catalog could be chosen (its group failed)\n")
    };
    if let Some(held) = part["held"].as_str() {
        out.push_str(&format!("    held by {held}: no other row was tried\n"));
    }
    if let Some(with) = part["with"].as_array().filter(|w| !w.is_empty()) {
        let with: Vec<&str> = with.iter().filter_map(Value::as_str).collect();
        out.push_str(&format!(
            "    with the parts before it at {}\n",
            with.join(", ")
        ));
    }
    let rows = part["passed"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    if rows.is_empty() {
        if picked && part["held"].is_null() {
            out.push_str("    no row before it was passed over\n");
        }
        return out;
    }
    let shown = limit.map_or(rows.len(), |n| n.min(rows.len()));
    out.push_str(&format!("    rows passed over ({}):\n", rows.len()));
    for row in &rows[..shown] {
        let count = row["count"]
            .as_u64()
            .map_or(String::new(), |n| format!(" x{n}"));
        let at = row["at"]
            .as_str()
            .map_or(String::new(), |at| format!(" (at {at})"));
        out.push_str(&format!(
            "      `{}`{count}: {}{at}: {}\n",
            row["row"].as_str().unwrap_or(""),
            row["why"].as_str().unwrap_or(""),
            row["message"].as_str().unwrap_or("")
        ));
    }
    if shown < rows.len() {
        out.push_str(&format!(
            "      ... and {} more (`--all-rows` lists every row)\n",
            rows.len() - shown
        ));
    }
    out
}
