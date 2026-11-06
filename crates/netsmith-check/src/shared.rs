//! `peripheral` and `shared` in a part (ADR 0062).
//!
//! A peripheral names the functions of a part that belong together; it is in use when the
//! board chooses one of them on a pin. A shared resource names the members of which at most
//! one is in use at a time. The part states both once, in its definition; the board is
//! judged for every instance of the part, before the solve, because the rule reads only the
//! board's choices of functions.
//!
//! Two halves: the definition is read and its mistakes reported when the part's body is done
//! ([`Checker::finish_rules`], so that the order of statements never matters), and the
//! board's use of the part is judged by [`overuse`].

use std::collections::{BTreeMap, BTreeSet};

use netsmith_lang::Span;
use netsmith_lang::ast;

use crate::checker::Checker;
use crate::diag::SourceId;
use crate::lower::{closest, list_of};
use crate::model::{
    InstId, InstKind, Model, Peripheral, PeripheralFunction, SharedMember, SharedRes,
};

/// How many names a message lists before it says `...`.
const LISTED: usize = 6;

impl Checker<'_> {
    /// The part a `peripheral` or `shared` statement is in, if it may be there.
    fn rules_part(&mut self, span: Span, what: &str, noun: &str) -> Option<InstId> {
        if self.in_kind(ast::DefKind::Generic) {
            self.error(
                span,
                format!(
                    "a generic states what every provider has; {noun} is a fact of one part's datasheet, so `{what}` goes in a part that provides it"
                ),
            );
            return None;
        }
        let cur = self.current();
        let inst = self.model.instance(cur);
        if inst.kind != InstKind::Part || inst.def != self.cur_def {
            self.error(
                span,
                format!("`{what}` can only appear directly inside a part"),
            );
            return None;
        }
        Some(cur)
    }

    pub(crate) fn peripheral(&mut self, p: &ast::Peripheral) {
        let Some(part) = self.rules_part(p.span, "peripheral", "a peripheral") else {
            return;
        };
        let source = self.cur_source();
        let functions = p
            .functions
            .iter()
            .map(|f| PeripheralFunction {
                name: f.name.name.clone(),
                kind: f.kind.map(|(k, _)| k),
                span: f.name.span,
            })
            .collect();
        self.model.instances[part.0]
            .rules
            .peripherals
            .push(Peripheral {
                name: p.name.name.clone(),
                class: p.on.as_ref().map(|c| c.name.clone()),
                class_span: p.on.as_ref().map_or(p.name.span, |c| c.span),
                functions,
                source,
                span: p.span,
                name_span: p.name.span,
            });
    }

    pub(crate) fn shared(&mut self, s: &ast::Shared) {
        let Some(part) = self.rules_part(s.span, "shared", "a shared resource") else {
            return;
        };
        let source = self.cur_source();
        let members = s
            .members
            .iter()
            .map(|m| SharedMember {
                name: m.name.clone(),
                span: m.span,
            })
            .collect();
        self.model.instances[part.0].rules.shares.push(SharedRes {
            name: s.name.name.clone(),
            members,
            source,
            span: s.span,
            name_span: s.name.span,
        });
    }

    /// The mistakes of a part's own peripherals and shared resources, said when its body has
    /// been read: they are the part author's, and they point at the statements that are wrong.
    pub(crate) fn finish_rules(&mut self, part: InstId) {
        let inst = self.model.instance(part);
        if inst.rules.peripherals.is_empty() && inst.rules.shares.is_empty() {
            return;
        }
        let rules = inst.rules.clone();
        let ty = inst.type_name.clone();
        let tables = inst.tables.clone();
        let mut found: Vec<(SourceId, Span, String)> = Vec::new();

        // The functions the rows list, with a pin that lists each.
        let mut row_functions: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut row_order: Vec<String> = Vec::new();
        for table in &tables {
            for row in &table.rows {
                for f in &row.functions {
                    let pin = format!("`.{}`", self.model.path_below(row.pin, part));
                    if !row_order.contains(&f.name) {
                        row_order.push(f.name.clone());
                    }
                    let pins = row_functions.entry(f.name.clone()).or_default();
                    if !pins.contains(&pin) {
                        pins.push(pin);
                    }
                }
            }
        }

        // A peripheral is declared once, and its name is not a function's.
        let mut seen: BTreeMap<&str, ()> = BTreeMap::new();
        let mut owner: BTreeMap<&str, &str> = BTreeMap::new();
        for p in &rules.peripherals {
            if seen.insert(&p.name, ()).is_some() {
                found.push((
                    p.source,
                    p.name_span,
                    format!(
                        "`peripheral {}` is declared twice in `{ty}`; one statement lists all of its functions",
                        p.name
                    ),
                ));
                continue;
            }
            for f in &p.functions {
                owner.entry(&f.name).or_insert(&p.name);
            }
        }
        let mut done: BTreeSet<&str> = BTreeSet::new();
        for p in &rules.peripherals {
            if !done.insert(&p.name) {
                continue;
            }
            if let Some(pins) = row_functions.get(&p.name) {
                found.push((
                    p.source,
                    p.name_span,
                    format!(
                        "`{}` is a peripheral of `{ty}` and also a function listed by {}; a peripheral and a function have different names, so give one of them another",
                        p.name,
                        list_of(pins)
                    ),
                ));
            } else if let Some(other) = owner.get(p.name.as_str()) {
                found.push((
                    p.source,
                    p.name_span,
                    format!(
                        "`{}` is a peripheral of `{ty}` and also a function of `peripheral {other}`; a peripheral and a function have different names, so give one of them another",
                        p.name
                    ),
                ));
            }
            if let Some(class) = &p.class
                && !row_functions.contains_key(class)
            {
                let have = if row_order.is_empty() {
                    "no row lists any function".to_string()
                } else {
                    format!("the functions that rows list are {}", listed(&row_order))
                };
                found.push((
                    p.source,
                    p.class_span,
                    format!(
                        "`peripheral {} on {class}`: no row of any `pins` table of `{ty}` lists the function `{class}`, so {} is offered on no pin; {have}",
                        p.name, p.name
                    ),
                ));
            }
            let mut listed_here: BTreeSet<&str> = BTreeSet::new();
            for f in &p.functions {
                if !listed_here.insert(&f.name) {
                    found.push((
                        p.source,
                        f.span,
                        format!(
                            "function `{}` is listed twice in `peripheral {}`",
                            f.name, p.name
                        ),
                    ));
                    continue;
                }
                match owner.get(f.name.as_str()) {
                    Some(first) if *first != p.name => {
                        found.push((
                            p.source,
                            f.span,
                            format!(
                                "function `{}` is in `peripheral {first}` and in `peripheral {}`; a function belongs to one peripheral",
                                f.name, p.name
                            ),
                        ));
                        continue;
                    }
                    _ => {}
                }
                if p.class.is_none() && !row_functions.contains_key(&f.name) {
                    found.push((
                        p.source,
                        f.span,
                        format!(
                            "`peripheral {}` lists `{}`, but no row of any `pins` table of `{ty}` lists it, so no pin can be it; list it as `fn {}` on the pins that carry it",
                            p.name, f.name, f.name
                        ),
                    ));
                }
            }
        }

        // A row that lists a function that its peripheral already offers on every pin of a class
        // that the row lists.
        for table in &tables {
            for row in &table.rows {
                for f in &row.functions {
                    let Some(p) = owner
                        .get(f.name.as_str())
                        .and_then(|first| rules.peripherals.iter().find(|p| p.name == *first))
                    else {
                        continue;
                    };
                    if let Some(class) = &p.class
                        && row.functions.iter().any(|g| g.name == *class)
                    {
                        let pin = format!(".{}", self.model.path_below(row.pin, part));
                        found.push((
                            table.source,
                            row.span,
                            format!(
                                "`{pin}` lists `{}`, which `peripheral {}` already offers on every pin that lists `{class}`, as `{pin}` does; remove it from the row",
                                f.name, p.name
                            ),
                        ));
                    }
                }
            }
        }

        // A row that writes a kind for a function whose peripheral gives it its kind.
        for table in &tables {
            for row in &table.rows {
                for f in row.functions.iter().filter(|f| f.kind.is_some()) {
                    if let Some(first) = owner.get(f.name.as_str()) {
                        let kind = f.kind.expect("it has a kind").word();
                        found.push((
                            table.source,
                            row.span,
                            format!(
                                "`.{}` lists `{}:{kind}`, a function of `peripheral {first}`, which gives it its kind; write `{}` without a kind",
                                self.model.path_below(row.pin, part),
                                f.name,
                                f.name
                            ),
                        ));
                    }
                }
            }
        }

        // A shared resource is declared once, and its members are peripherals of the part.
        let names: Vec<String> = {
            let mut v: Vec<String> = Vec::new();
            for p in &rules.peripherals {
                if !v.contains(&p.name) {
                    v.push(p.name.clone());
                }
            }
            v
        };
        let mut shared_seen: BTreeSet<&str> = BTreeSet::new();
        for s in &rules.shares {
            if !shared_seen.insert(&s.name) {
                found.push((
                    s.source,
                    s.name_span,
                    format!(
                        "`shared {}` is declared twice in `{ty}`; one statement lists all of its members",
                        s.name
                    ),
                ));
                continue;
            }
            let mut members: BTreeSet<&str> = BTreeSet::new();
            for m in &s.members {
                if !names.contains(&m.name) {
                    let hint = closest(&m.name, &names)
                        .map(|n| format!("; did you mean `{n}`?"))
                        .unwrap_or_default();
                    let sep = if hint.is_empty() { "; " } else { " " };
                    let have = if names.is_empty() {
                        format!(
                            "{sep}`{ty}` has no peripherals, so declare one with `peripheral NAME {{ FUNCTION ... }}`"
                        )
                    } else {
                        format!("{sep}the peripherals are {}", listed(&names))
                    };
                    found.push((
                        s.source,
                        m.span,
                        format!(
                            "`{}` in `shared {}` is not a peripheral of `{ty}`{hint}{have}",
                            m.name, s.name
                        ),
                    ));
                } else if !members.insert(&m.name) {
                    found.push((
                        s.source,
                        m.span,
                        format!("`{}` is listed twice in `shared {}`", m.name, s.name),
                    ));
                }
            }
        }

        for (source, span, message) in found {
            self.report(source, span, message);
        }
    }
}

/// Names for a message, six at most, then `...`.
fn listed(names: &[String]) -> String {
    if names.len() <= LISTED {
        list_of(names)
    } else {
        format!("{}, ...", names[..LISTED].join(", "))
    }
}

/// One use of a peripheral: a pin of the part whose chosen function is one of its functions.
struct Use {
    path: String,
    function: String,
    at: Option<(SourceId, Span)>,
}

/// The shared resources of a part that have more members in use than they allow, as the
/// message and the statement that goes over: the first use of the member after the ones the
/// resource allows, in path order.
pub(crate) fn overuse(model: &Model, part: InstId) -> Vec<((SourceId, Span), String)> {
    let inst = model.instance(part);
    if inst.rules.shares.is_empty() {
        return Vec::new();
    }
    let path = model.path_of(part);
    let ty = &inst.type_name;

    // The pins of the part, in path order: every pin that any of its tables has a row for.
    let mut pins: Vec<InstId> = inst
        .tables
        .iter()
        .flat_map(|t| t.rows.iter().map(|r| r.pin))
        .collect();
    pins.sort_by_key(|&p| model.path_of(p));
    pins.dedup();

    // The uses of each peripheral.
    let mut uses: BTreeMap<&str, Vec<Use>> = BTreeMap::new();
    for pin in pins {
        let Some(function) = model.function_of(pin) else {
            continue;
        };
        let Some((peripheral, _)) = model.peripheral_of(part, &function) else {
            continue;
        };
        uses.entry(&peripheral.name).or_default().push(Use {
            path: model.path_of(pin),
            function,
            at: model.function_stated_at(pin),
        });
    }

    let place = |u: &Use| -> String {
        u.at.and_then(|(source, span)| model.place_of_statement(source, span))
            .map(|p| format!(" ({p})"))
            .unwrap_or_default()
    };
    let member_text = |name: &str, used: &[Use]| -> String {
        let each: Vec<String> = used
            .iter()
            .map(|u| format!("on `{}` as {}{}", u.path, u.function, place(u)))
            .collect();
        format!("{name} {}", list_of(&each))
    };
    // The members of a resource that are in use, in the path order of their first use.
    let in_use = |s: &SharedRes| -> Vec<(String, &Vec<Use>)> {
        let mut found: Vec<(String, &Vec<Use>)> = s
            .members
            .iter()
            .filter_map(|m| uses.get(m.name.as_str()).map(|u| (m.name.clone(), u)))
            .collect();
        found.sort_by(|a, b| a.1[0].path.cmp(&b.1[0].path));
        found.dedup_by(|a, b| a.0 == b.0);
        found
    };

    let mut shares: Vec<&SharedRes> = inst.rules.shares.iter().collect();
    shares.sort_by(|a, b| a.name.cmp(&b.name));
    shares.dedup_by(|a, b| a.name == b.name);

    let mut out = Vec::new();
    for s in &shares {
        let used = in_use(s);
        if used.len() < 2 {
            continue;
        }
        let Some(at) = used[1].1[0].at else {
            continue;
        };
        let names: Vec<String> = used.iter().map(|(n, _)| n.clone()).collect();
        let members: Vec<String> = used.iter().map(|(n, u)| member_text(n, u)).collect();
        // The other resources: which have a member in use, and which are free.
        let others: Vec<String> = shares
            .iter()
            .filter(|o| o.name != s.name)
            .map(|o| {
                let by: Vec<String> = in_use(o).into_iter().map(|(n, _)| n).collect();
                if by.is_empty() {
                    format!("`{}` has nothing in use", o.name)
                } else {
                    format!("`{}` by {}", o.name, list_of(&by))
                }
            })
            .collect();
        let all_busy = shares
            .iter()
            .filter(|o| o.name != s.name)
            .all(|o| !in_use(o).is_empty());
        let rest = match (others.is_empty(), all_busy) {
            (true, _) => String::new(),
            (false, true) => format!(
                "; the other shared resources of `{ty}` are in use as well, {}, so none is free",
                list_of(&others)
            ),
            (false, false) => format!(
                "; the other shared resources of `{ty}`: {}",
                list_of(&others)
            ),
        };
        out.push((
            at,
            format!(
                "`{path}` (`{ty}`) uses {}, which share `{}` and run one at a time: {}{rest}",
                list_of(&names),
                s.name,
                members.join(", ")
            ),
        ));
    }
    out
}
