//! [`Rule::Redundant`]: a statement that says what the design gets without it.
//!
//! - A narrowing (`&=`) whose value a default already gives: a `?=` of the
//!   kind's own definition (the standard library's `Resistor` says `.package ?= "0402"`), of a
//!   module round the part, or a line of the board's `defaults` block. A statement is a candidate
//!   when some default of a field of the same name has the same text; the linter then checks the
//!   project again without it, and the statement is redundant when every board, system and panel
//!   that reads its file checks and has every value it had.
//! - A narrowing of a column of a part that is picked by a key (`catalog mcu by .mpn`) whose
//!   value is the cell of the row that the key names: the row gives it. The linter checks the
//!   project again without it, and the statement is redundant when the project still checks.
//!   A part in a use of a generic with several providers is left out: there a narrowing also
//!   chooses the provider, which the row cannot do.
//!
//! An attribute that a row or a default gives is not judged yet.

use std::collections::{HashMap, HashSet};

use netsmith_check::{Checked, Domain, Loader, Model, check_with};
use netsmith_lang::Span;
use netsmith_lang::ast::{CmpOp, Constraint, Expr, Item};
use netsmith_solve::Value;

use crate::{Context, FileKind, Lint, Rule, SourceFile};

pub(crate) fn lint(cx: &Context) -> Vec<Lint> {
    let defaults = defaults(cx);
    let mut out = Vec::new();
    let mut done: HashSet<(String, usize)> = HashSet::new();
    for file in cx.files.values() {
        let mut statements = Vec::new();
        collect(&file.ast, &mut statements);
        for c in statements {
            // Only a narrowing: a second `?=` on a field is the checker's error, and a line of
            // a `defaults` block that no field takes is the checker's note.
            if c.op != CmpOp::Narrow {
                continue;
            }
            let Some(field) = field_name(c) else {
                continue;
            };
            let text = normal(&c.rhs);
            if !defaults
                .get(field.as_str())
                .is_some_and(|texts| texts.contains(&text))
            {
                continue;
            }
            if let Some(from) = gives_the_same(cx, file, c.span) {
                done.insert((file.name.clone(), c.span.start));
                let stated = statement(file, c.span);
                let message = match from {
                    Some(from) => format!(
                        "`{stated}` says what the default at {from} already gives; delete it"
                    ),
                    None => format!("`{stated}` says what a default already gives; delete it"),
                };
                out.push(file.lint(Rule::Redundant, c.span, message));
            }
        }
    }
    out.extend(row_gives(cx, &done));
    out
}

/// The text of a statement on one line.
fn statement(file: &SourceFile, span: Span) -> String {
    file.text
        .get(span.start..span.end.min(file.text.len()))
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The text of a value with no white space: what two defaults are compared by.
fn normal(e: &Expr) -> String {
    e.to_source().split_whitespace().collect()
}

/// The field a statement states, by its last name, when its left side is a path.
fn field_name(c: &Constraint) -> Option<String> {
    if !matches!(c.op, CmpOp::Narrow | CmpOp::Default) {
        return None;
    }
    match &c.lhs {
        Expr::Path(p) => p.segments.last().map(|s| s.name.clone()),
        _ => None,
    }
}

/// Every `&=` and `?=` statement of a file, wherever it is written.
fn collect<'a>(file: &'a netsmith_lang::ast::File, out: &mut Vec<&'a Constraint>) {
    fn items<'a>(list: &'a [Item], out: &mut Vec<&'a Constraint>) {
        for item in list {
            match item {
                Item::Constraint(c) => out.push(c),
                Item::Decl(d) => {
                    if let Some(body) = &d.body {
                        items(body, out);
                    }
                }
                Item::Connect(c) => {
                    for g in &c.groups {
                        if let Some(body) = &g.body {
                            items(body, out);
                        }
                    }
                }
                Item::Case(c) => items(&c.items, out),
                Item::Support(s) => items(&s.items, out),
                Item::Defaults(d) => items(&d.items, out),
                _ => {}
            }
        }
    }
    for d in &file.definitions {
        items(&d.items, out);
    }
    for p in &file.physicals {
        items(&p.items, out);
    }
}

/// The texts of the defaults (`?=`) of every field name, in every file the loader holds and
/// in the top-level files.
fn defaults(cx: &Context) -> HashMap<String, HashSet<String>> {
    let mut out: HashMap<String, HashSet<String>> = HashMap::new();
    let mut add = |text: &str| {
        let Ok(file) = netsmith_lang::parse(text) else {
            return;
        };
        let mut statements = Vec::new();
        collect(&file, &mut statements);
        for c in statements {
            if c.op != CmpOp::Default {
                continue;
            }
            if let Some(field) = field_name(c) {
                out.entry(field).or_default().insert(normal(&c.rhs));
            }
        }
    };
    for (_, _, text) in cx.project.loader.modules() {
        add(text);
    }
    for file in cx.files.values() {
        add(&file.text);
    }
    out
}

/// `text` with the span made blank: the same lines, without the statement.
fn without(text: &str, span: Span) -> String {
    let end = span.end.min(text.len());
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..span.start]);
    out.extend(
        text[span.start..end]
            .chars()
            .map(|c| if c == '\n' { '\n' } else { ' ' }),
    );
    out.push_str(&text[end..]);
    out
}

/// The top-level files checked again with the statement at `span` of `file` taken out: each
/// that reads the file, with what it was before. `None` when no top-level file reads it.
fn recheck<'c>(
    cx: &'c Context,
    file: &SourceFile,
    span: Span,
) -> Option<Vec<(&'c Checked, Checked)>> {
    let text = without(&file.text, span);
    let mut loader: Option<Loader> = None;
    match file.kind {
        FileKind::Module => {
            let mut l = cx.project.loader.clone();
            l.insert(file.key.as_deref()?, &file.name, &text);
            loader = Some(l);
        }
        FileKind::Physical => {
            let mut l = cx.project.loader.clone();
            l.attach_physical(file.beside.as_deref()?, &file.name, &text);
            loader = Some(l);
        }
        FileKind::Root => {}
    }
    let loader = loader.as_ref().unwrap_or(cx.project.loader);
    let mut out = Vec::new();
    for (root, before) in &cx.checked {
        let reads = before.sources.iter().any(|s| s.name == file.name);
        if !reads {
            continue;
        }
        let root = cx.project.roots.iter().find(|r| &r.name == root)?;
        let root_text = if root.name == file.name && file.kind == FileKind::Root {
            text.clone()
        } else {
            root.text.clone()
        };
        out.push((
            before,
            check_with(&root.name, &root_text, loader, &root.assignment),
        ));
    }
    (!out.is_empty()).then_some(out)
}

/// The models of a checked file, the boards', the systems' and the panels'.
fn models(c: &Checked) -> Vec<&Model> {
    c.boards
        .iter()
        .map(|b| &b.model)
        .chain(c.systems.iter().map(|s| &s.model))
        .chain(c.panels.iter().map(|p| &p.model))
        .collect()
}

/// Whether every value of every model is the same in the two.
fn same_values(a: &Checked, b: &Checked) -> bool {
    let (ma, mb) = (models(a), models(b));
    ma.len() == mb.len()
        && ma.iter().zip(&mb).all(|(x, y)| {
            x.fields().count() == y.fields().count()
                && x.fields()
                    .zip(y.fields())
                    .all(|((_, f), (_, g))| f.name == g.name && f.domain == g.domain)
        })
}

/// Whether `checked` has a field that the statement at `span` of `file` narrows.
fn narrows_here(checked: &Checked, file: &SourceFile, span: Span) -> bool {
    models(checked).iter().any(|m| {
        m.fields().any(|(_, f)| {
            f.narrowed_at
                .iter()
                .any(|(s, at)| *at == span && checked.sources.get(*s).name == file.name)
        })
    })
}

/// Whether taking the statement out of `file` leaves every model that reads it with the same
/// values; with the place of the default that gives them then, when one field says. A statement
/// that narrows no field of any model (inside a provider that no use picks) says nothing here.
fn gives_the_same(cx: &Context, file: &SourceFile, span: Span) -> Option<Option<String>> {
    if !cx.checked.iter().any(|(_, c)| narrows_here(c, file, span)) {
        return None;
    }
    let rechecked = recheck(cx, file, span)?;
    if !rechecked
        .iter()
        .all(|(before, after)| crate::checks(after) && same_values(before, after))
    {
        return None;
    }
    // A field the statement stated before, and the default that states it now.
    for (before, after) in &rechecked {
        for (m_before, m_after) in models(before).into_iter().zip(models(after)) {
            for ((_, f), (_, g)) in m_before.fields().zip(m_after.fields()) {
                let stated_here = f
                    .narrowed_at
                    .iter()
                    .any(|(s, at)| *at == span && before.sources.get(*s).name == file.name);
                if !stated_here || g.default_from.is_none() {
                    continue;
                }
                if let Some((s, at)) = g.narrowed_at.last() {
                    return Some(after.sources.place(*s, *at));
                }
            }
        }
    }
    Some(None)
}

/// The narrowings of a keyed part's columns that the row its key names gives.
fn row_gives(cx: &Context, done: &HashSet<(String, usize)>) -> Vec<Lint> {
    let Some(catalogs) = cx.project.catalogs else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen: HashSet<(String, usize)> = done.clone();
    for (checked, model) in cx.models() {
        for (part, inst) in model.instances() {
            let Some(catalog) = &inst.catalog else {
                continue;
            };
            let Some(key) = catalog.key else {
                continue;
            };
            // In a use of a generic with several providers, a narrowing also chooses the
            // provider (ADR 0113): what the row gives, it says of the others too.
            if model.in_varied_use(part) {
                continue;
            }
            let Domain::Text(wanted) = &model.field(key).domain else {
                continue;
            };
            let path = model.path_of(part);
            let column = |f| {
                let full = model.path_of_field(f);
                full.strip_prefix(&format!("{path}."))
                    .unwrap_or(&full)
                    .to_string()
            };
            let kinds: Vec<(String, netsmith_check::FieldKind)> = catalog
                .fields
                .iter()
                .map(|&f| (column(f), model.field(f).kind))
                .collect();
            let optional: Vec<String> = catalog.optional.iter().map(|&f| column(f)).collect();
            let Some(Ok(rows)) = catalogs.identity(&catalog.name, &kinds, &optional) else {
                continue;
            };
            let key_column = column(key);
            let Some(row) = rows
                .candidates
                .iter()
                .find(|c| matches!(c.column(&key_column), Some(Value::Text(t)) if t == wanted))
            else {
                continue;
            };
            for &f in &catalog.fields {
                if f == key {
                    continue;
                }
                let field = model.field(f);
                let [(source, span)] = field.narrowed_at.as_slice() else {
                    continue;
                };
                if field.default_from.is_some() {
                    continue;
                }
                let gives = match (&field.domain, row.column(&column(f))) {
                    (Domain::Text(a), Some(Value::Text(b))) => a == b,
                    (Domain::Quantity(a), Some(Value::Quantity(b))) => a == b,
                    _ => false,
                };
                if !gives {
                    continue;
                }
                let name = &checked.sources.get(*source).name;
                let Some(file) = cx.files.get(name) else {
                    continue;
                };
                if !seen.insert((name.clone(), span.start)) {
                    continue;
                }
                let still = recheck(cx, file, *span)
                    .is_some_and(|r| r.iter().all(|(_, after)| crate::checks(after)));
                if !still {
                    continue;
                }
                let stated = statement(file, *span);
                out.push(file.lint(
                    Rule::Redundant,
                    *span,
                    format!(
                        "`{stated}` says what the row of the catalog `{}` that `.{key_column}` names (`{wanted}`) already gives; delete it",
                        catalog.name
                    ),
                ));
            }
        }
    }
    out
}
