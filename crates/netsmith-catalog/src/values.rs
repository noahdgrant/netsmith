//! The two findings about a value that nobody stated (ADR 0065): a pick that nothing limits, and
//! a value declared in a module or board that a check waits for. Both are guidelines of the kind
//! a project may make blocking (`guidelines = "error"`, `--strict`): the build passes with a
//! warning by default, because the design may be right, and the finding is what was missing when
//! the first row of a catalog was taken without a word.

use std::collections::BTreeMap;

use netsmith_check::{FieldId, FieldKind, InstId, Model, Sources};
use netsmith_design::guidelines::Guideline;
use netsmith_quantity::Dimension;
use netsmith_solve::{Solution, UnconstrainedPick};

/// `a`, `a and b`, `a, b and c`.
fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// A value to state and a range that says "any", for a field, as they are written.
fn examples(model: &Model, field: FieldId) -> (String, String) {
    match model.field(field).kind {
        FieldKind::Quantity(d) if d == Dimension::RESISTANCE => {
            ("4.7kohm +/- 5%".into(), "10kohm..100kohm".into())
        }
        FieldKind::Quantity(d) if d == Dimension::CAPACITANCE => {
            ("100nF +/- 10%".into(), "10nF..10uF".into())
        }
        FieldKind::Quantity(d) if d == Dimension::INDUCTANCE => {
            ("10uH +/- 20%".into(), "1uH..100uH".into())
        }
        FieldKind::Quantity(d) if d == Dimension::VOLTAGE => {
            ("3.3V +/- 5%".into(), "0V..5V".into())
        }
        FieldKind::Quantity(d) if d == Dimension::CURRENT => {
            ("10mA +/- 10%".into(), "0mA..100mA".into())
        }
        FieldKind::Quantity(d) if d == Dimension::FREQUENCY => {
            ("1MHz +/- 1%".into(), "1kHz..10MHz".into())
        }
        FieldKind::Quantity(_) => ("1".into(), "0..10".into()),
        FieldKind::Enum(e) => {
            let info = model.enum_info(e);
            let first = info.variants.first().cloned().unwrap_or_default();
            let variant = format!("{}::{first}", info.name);
            (variant.clone(), variant)
        }
        _ => ("\"text\"".into(), "\"text\"".into()),
    }
}

/// `src/lamp.nsm:6 and 7`: where statements are, the lines of one file together.
fn places(sources: &Sources, at: &[(netsmith_check::SourceId, netsmith_lang::Span)]) -> String {
    let mut by_file: Vec<(String, Vec<usize>)> = Vec::new();
    for &(source, span) in at {
        let file = sources.get(source);
        let (line, _) = netsmith_lang::ParseError::new(span, "").line_col(&file.text);
        match by_file.iter_mut().find(|(name, _)| *name == file.name) {
            Some((_, lines)) => {
                if !lines.contains(&line) {
                    lines.push(line);
                }
            }
            None => by_file.push((file.name.clone(), vec![line])),
        }
    }
    by_file
        .into_iter()
        .map(|(name, lines)| {
            let mut lines = lines;
            lines.sort_unstable();
            let lines: Vec<String> = lines.iter().map(usize::to_string).collect();
            format!("{name}:{}", list(&lines))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The picks that nothing limited, as findings (stage 1a). For one board of a system, only its
/// parts.
pub fn unconstrained_pick_findings(
    solution: &Solution,
    sources: &Sources,
    board: Option<InstId>,
) -> Vec<Guideline> {
    let whole = &solution.model;
    let model = &board.map_or_else(|| whole.clone(), |b| whole.scoped_to(b));
    let inside = |inst: InstId| board.is_none_or(|b| whole.is_inside(inst, b));
    // Parts that hang from one member that has no value are one finding, and the findings come
    // in the order the parts were declared.
    let mut groups: BTreeMap<(Option<String>, String), Vec<&UnconstrainedPick>> = BTreeMap::new();
    for u in solution.unconstrained.iter().filter(|u| inside(u.part)) {
        let key = match &u.held_by {
            Some(h) => (Some(model.path_of_field(h.member)), String::new()),
            // Parts that only the same checks limit, as a ratio, are one finding.
            None if !u.ratio.is_empty() => (None, format!("~{:?}", u.ratio)),
            None => (None, model.path_of(u.part)),
        };
        groups.entry(key).or_default().push(u);
    }
    let mut groups: Vec<Vec<&UnconstrainedPick>> = groups.into_values().collect();
    for picks in &mut groups {
        picks.sort_by_key(|u| u.part);
    }
    groups.sort_by_key(|picks| picks[0].part);
    groups
        .into_iter()
        .map(|picks| {
            let names: Vec<String> = picks
                .iter()
                .map(|u| format!("`{}`", model.path_of(u.part)))
                .collect();
            let mut types: Vec<String> = Vec::new();
            for u in &picks {
                let part = whole.instance(u.part);
                let ty = part.placed_as.clone().unwrap_or_else(|| part.type_name.clone());
                if !types.contains(&ty) {
                    types.push(ty);
                }
            }
            let many = picks.len() > 1;
            let (its, each) = if many { ("their", " for each") } else { ("its", "") };
            let mut rows: Vec<&str> = Vec::new();
            for u in &picks {
                if !rows.contains(&u.row.as_str()) {
                    rows.push(&u.row);
                }
            }
            let took = if rows.len() == 1 {
                format!("took the first row that fits, {}{each}", rows[0])
            } else {
                let each: Vec<String> = picks
                    .iter()
                    .map(|u| format!("{} for `{}`", u.row, model.path_of(u.part)))
                    .collect();
                format!("took the first row that fits for each: {}", list(&each))
            };
            let column = &picks[0].column;
            let (value, any) = examples(model, picks[0].field);
            // A check that holds whatever the size is limits a ratio, and the value is still
            // free in size (ADR 0065).
            let ratio: Vec<(netsmith_check::SourceId, netsmith_lang::Span)> = picks[0].ratio.clone();
            let head = format!(
                "{} ({}) {} chosen with nothing limiting {its} {column}{}, so the solver {took}",
                list(&names),
                types.join(", "),
                if many { "were" } else { "was" },
                if ratio.is_empty() { "" } else { " in size" },
            );
            let head = if ratio.is_empty() {
                head
            } else {
                let checks: Vec<String> = ratio
                    .iter()
                    .map(|&(source, span)| {
                        format!(
                            "`{}`",
                            model
                                .statement_text(source, span)
                                .unwrap_or_else(|| "a check".to_string())
                        )
                    })
                    .collect();
                format!(
                    "{head}; {its} {column} is limited as a ratio, not in size, by {} ({}), which hold whatever the size is",
                    list(&checks),
                    places(sources, &ratio)
                )
            };
            let message = match &picks[0].held_by {
                Some(held) => {
                    let member = model.path_of_field(held.member);
                    let links: Vec<(netsmith_check::SourceId, netsmith_lang::Span)> = picks
                        .iter()
                        .filter_map(|u| u.held_by.as_ref()?.link)
                        .map(|(_, s, sp)| (s, sp))
                        .collect();
                    let mut ops: Vec<&str> = Vec::new();
                    for u in &picks {
                        if let Some((op, _, _)) = u.held_by.as_ref().and_then(|h| h.link)
                            && !ops.contains(&op.symbol())
                        {
                            ops.push(op.symbol());
                        }
                    }
                    let at = match (links.is_empty(), ops.is_empty()) {
                        (false, false) => format!(
                            " (`{}` at {})",
                            ops.join("`, `"),
                            places(sources, &links)
                        ),
                        _ => String::new(),
                    };
                    let are = if many { "both are held" } else { "it is held" };
                    let are = if picks.len() > 2 { "all are held" } else { are };
                    format!(
                        "{head}; {are} to `{member}`{at}, which nothing states: state it, `.{member} &= {value}`, or say that any value will do, `.{member} &= {any}`"
                    )
                }
                None => {
                    let path = model.path_of(picks[0].part);
                    format!(
                        "{head}: state {}, `.{path}.{column} &= {value}`, or bound a value that {} (a current or a voltage the design checks), or say that any value will do, `.{path}.{column} &= {any}`",
                        if many { "one of them" } else { "it" },
                        if ratio.is_empty() {
                            "it decides"
                        } else {
                            "changes with the size, not only with the ratio"
                        }
                    )
                }
            };
            Guideline {
                rule: "unconstrained pick",
                message,
                at: Some(picks[0].part),
            }
        })
        .collect()
}

/// The values that nothing states and a check waits for, as findings (stage 1b). For one board
/// of a system, only those declared inside it.
pub fn unstated_value_findings(
    solution: &Solution,
    sources: &Sources,
    board: Option<InstId>,
) -> Vec<Guideline> {
    let whole = &solution.model;
    let model = &board.map_or_else(|| whole.clone(), |b| whole.scoped_to(b));
    let inside = |inst: InstId| board.is_none_or(|b| whole.is_inside(inst, b));
    solution
        .unstated
        .iter()
        .filter(|u| u.fields.iter().any(|&f| inside(whole.field(f).owner)))
        .map(|u| {
            let fields: Vec<String> = u
                .fields
                .iter()
                .map(|&f| format!("`{}`", model.path_of_field(f)))
                .collect();
            let mut declared: Vec<String> = Vec::new();
            for &f in &u.fields {
                let owner = whole.instance(whole.field(f).owner);
                let ty = format!("`{}`", owner.type_name);
                if !declared.contains(&ty) {
                    declared.push(ty);
                }
            }
            // The design's own checks come before the library's.
            let mut order = u.checks.clone();
            order.sort_by_key(|&i| {
                sources
                    .get(whole.relations()[i].source)
                    .name
                    .starts_with("std/")
            });
            let checks: Vec<String> = order
                .iter()
                .map(|&i| {
                    let r = &whole.relations()[i];
                    format!(
                        "`{} {} {}`",
                        model.show_expr(&r.lhs),
                        r.op.symbol(),
                        model.show_expr(&r.rhs)
                    )
                })
                .collect();
            let shown: Vec<String> = checks.iter().take(3).cloned().collect();
            let more = match checks.len().saturating_sub(3) {
                0 => String::new(),
                n => format!(" and {n} more"),
            };
            let first = u.fields[0];
            let (value, _) = examples(model, first);
            let many = u.fields.len() > 1;
            let message = format!(
                "{} {} declared by {} and nothing states or works {} out, so the {} {}{more} {} not evaluated; state {}, for example `.{} &= {value}`",
                list(&fields),
                if many { "are" } else { "is" },
                list(&declared),
                if many { "them" } else { "it" },
                if checks.len() == 1 { "check" } else { "checks" },
                list(&shown),
                if checks.len() == 1 { "is" } else { "are" },
                if many { "them" } else { "it" },
                model.path_of_field(first),
            );
            Guideline {
                rule: "unstated value",
                message,
                at: Some(whole.field(first).owner),
            }
        })
        .collect()
}
