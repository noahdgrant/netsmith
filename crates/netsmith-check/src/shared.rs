//! `peripheral` and `shared` in a part (ADR 0071).
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
//!
//! # `peripheral NAME [on CLASS] [as Interface [Role]] { FUNCTION[:kind] [as line] ... }`
//!
//! - A peripheral is in use when one of its functions is the chosen function of a pin of the
//!   part (`Model::function_of`: the pin's own stated
//!   function, or the one a forwarding equation `.mcu.p04.function == .scl.function` gives it,
//!   to a depth of six). A function chosen on an unconnected pin is a use: the statement is
//!   deliberate.
//! - With `on CLASS` every pin whose row, in any table, lists the function `CLASS` offers the
//!   peripheral's functions as well. The pin table does not change, and a pin in a package that
//!   lacks it has no row. What a pin offers is computed when asked (`offers`, `class_peripherals`
//!   and `offered_count` in `pinout.rs`), because a `peripheral` may follow the table. `CLASS`
//!   must be a function some row lists. Without `on`, a function is offered only on the pins
//!   whose rows list it, and each must be listed by at least one row.
//! - A function belongs to one peripheral; a function a peripheral names takes its kind from it
//!   and a row that writes a kind for it is refused. A peripheral name and a function name are in
//!   one namespace. Refused too: a peripheral or a shared resource declared twice, a function
//!   listed twice in a peripheral, a limit that is not fewer than the members (`limit 1` is the
//!   default spelled out), a member twice in one shared resource, and a member that is a
//!   function of a peripheral that is also a member of the same resource.
//!
//! # `shared NAME [limit N] { MEMBER ... }`
//!
//! A member is a peripheral name, a function name or a pin (`.p10`). At most N members (the
//! default is one) are in use at once. The rule is judged for each instance of the part, in the board pass of
//! `pin_faults`, before the solve: the choices are statements of the text, so no catalog row or
//! package is needed. A part inside a module is judged with its full path. A member may be in
//! several shared resources (`UARTE0` in `SERIAL0` and in `UARTS`). Order of statements does
//! not matter.
//!
//! # The messages
//!
//! Each is one line, names the part by its path and type, the resource or the rule, every use
//! with the function and its line, and ends with what the author can do. The span is the
//! statement of the use that goes over the limit, the second in path order.
//!
//! 1. Two members of one shared resource: ``` `u` (`Zmcu`) uses SPIM1 and UARTE1, which share
//!    `SERIAL1` and run one at a time: SPIM1 on `u.p02` as SPIM1_SCK (main.nsm:24) ... ```, with
//!    a last clause that lists the other shared resources that have a member in use (or says
//!    `SERIAL2 has nothing in use`), so that the author sees "there is no free unit". The clause
//!    lists only the resources whose members are of the same kind and that have the same limit.
//!    Members are listed in the path order of their first use.
//! 2. More members in use than a limit allows: ``` `u` (`Zmcu`) uses 3 of the peripherals of
//!    `UARTS`, which allows 2 at a time: ... ```.
//! 3. Two pins that cannot both be used: ``` `u.p10` and `u.p11` of `Zmcu` are both in use, but
//!    they share `REFERENCE`, which allows 1 at a time ```. A pin connected with no function
//!    chosen reads `u.q0 connected to probe.a12` and points at the part's declaration, because a
//!    board's `~` has no statement the checker keeps. (The first text of the ADR chose the same
//!    function on both pins, which the rule of ADR 0016 already refuses; the guide and the tests
//!    choose different ones.)
//! 4. A function on a pin that does not offer it: ``` `u.p02` cannot be `HSSPI_SCK`; that
//!    function is offered only on `u.p08`; `u.p02` offers APP_GPIO, NET_GPIO and the lines of the
//!    peripherals on those pins: ... ```. A name no pin offers is a slip and gets `did you mean`.
//!    The list of names is cut after six with `...`, and the nearest name picks the first of
//!    equally near ones.
//! 5. A function choice a module does not pass on (the old silent no-op): ``` `hub.scl.function`
//!    is stated, but `hub.scl` is a pin of module `Hub` and nothing passes it to a pin of a part,
//!    so it has no effect; inside `Hub`, write `.mcu.p04.function == .scl.function` ```. The
//!    hint names the part pin on the same net inside the module when exactly one has functions.
//!
//! Three more are for the part's author, at the line that is wrong: a member of a `shared` that
//! is not a peripheral, function or pin of the part (with the nearest name and the peripherals);
//! `peripheral X on GPIO` when no row lists `GPIO` (with the functions rows do list); and a row
//! that writes a kind for a function a peripheral gives a kind. A pin that is connected and lists
//! only a class has several functions and needs a choice. A part with no `peripheral` keeps every
//! message it had, word for word.
//!
//! # A provider and a generic
//!
//! A provider may state its own `peripheral` and `shared`; they are the part's, judged on the
//! instance the solver picks like the rest of its checks (a trial that fails the shared rule is
//! passed over with the message as its reason, ADR 0068). A generic refuses them: its pins,
//! names, pads and functions are the provider's own, a use of the generic reaches only its
//! members, and the providers' `Item` rewriting would copy them into every provider. What a
//! generic can say about capability is a number the provider states (`.serial_units >= 3`).

use std::collections::{BTreeMap, BTreeSet};

use netsmith_lang::Span;
use netsmith_lang::ast;

use crate::checker::Checker;
use crate::diag::SourceId;
use crate::lower::{closest, list_of};
use crate::model::{
    InstId, InstKind, Instance, Model, Peripheral, PeripheralFunction, SharedMember, SharedRes,
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
        let (bus, lines) = self.peripheral_bus(p);
        let functions = p
            .functions
            .iter()
            .zip(lines)
            .map(|(f, line)| PeripheralFunction {
                name: f.name.name.clone(),
                kind: f.kind.map(|(k, _)| k),
                line,
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
                bus,
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
        let mut members = Vec::new();
        for m in &s.members {
            match m {
                ast::SharedMember::Name(n) => members.push(SharedMember {
                    name: n.name.clone(),
                    pin: None,
                    span: n.span,
                }),
                ast::SharedMember::Pin(path) => {
                    if let Some(pin) = self.part_pin(path) {
                        members.push(SharedMember {
                            name: path.to_source(),
                            pin: Some(pin),
                            span: path.span,
                        });
                    }
                }
            }
        }
        self.model.instances[part.0].rules.shares.push(SharedRes {
            name: s.name.name.clone(),
            limit: s.limit.map_or(1, |(n, _)| n),
            limit_span: s.limit.map(|(_, span)| span),
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

        // A shared resource is declared once, and its members are peripherals, functions or
        // pins of the part.
        let names: Vec<String> = {
            let mut v: Vec<String> = Vec::new();
            for p in &rules.peripherals {
                if !v.contains(&p.name) {
                    v.push(p.name.clone());
                }
            }
            v
        };
        let mut known: Vec<String> = names.clone();
        for f in row_order.iter().chain(
            rules
                .peripherals
                .iter()
                .flat_map(|p| p.functions.iter().map(|f| &f.name)),
        ) {
            if !known.contains(f) {
                known.push(f.clone());
            }
        }
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
            if let Some(span) = s.limit_span
                && s.limit as usize >= s.members.len()
            {
                found.push((
                    s.source,
                    span,
                    format!(
                        "`shared {} limit {}` allows every one of its {} members at once, so it states no limit at all; a limit is at least 1 and fewer than the number of members: lower it, or remove it for one at a time",
                        s.name,
                        s.limit,
                        s.members.len()
                    ),
                ));
            }
            let mut written: BTreeSet<&str> = BTreeSet::new();
            let mut pins: BTreeSet<InstId> = BTreeSet::new();
            for m in &s.members {
                if let Some(pin) = m.pin {
                    if !pins.insert(pin) {
                        found.push((
                            s.source,
                            m.span,
                            format!("`{}` is listed twice in `shared {}`", m.name, s.name),
                        ));
                    }
                    continue;
                }
                if !known.contains(&m.name) {
                    let hint = closest(&m.name, &known)
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
                            "`{}` in `shared {}` is not a peripheral, a function or a pin of `{ty}`{hint}{have}",
                            m.name, s.name
                        ),
                    ));
                } else if !written.insert(&m.name) {
                    found.push((
                        s.source,
                        m.span,
                        format!("`{}` is listed twice in `shared {}`", m.name, s.name),
                    ));
                } else if let Some(&first) = owner.get(m.name.as_str())
                    && s.members.iter().any(|o| o.pin.is_none() && o.name == first)
                {
                    found.push((
                        s.source,
                        m.span,
                        format!(
                            "`{}` is a function of `peripheral {first}`, which is also a member of `shared {}`; list one of them",
                            m.name, s.name
                        ),
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

/// What a member of a shared resource is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    Peripheral,
    Function,
    Pin,
}

/// One use of a member: the pin, the function the board chose on it (a pin member may be in
/// use by being connected only), and the statement that made it a use.
struct Use {
    path: String,
    function: Option<String>,
    /// For a pin that is in use by its connection only: another pin on its net.
    joined_to: Option<String>,
    at: Option<(SourceId, Span)>,
}

/// A member that is in use, with its uses.
struct Active {
    member: SharedMember,
    category: Category,
    uses: Vec<Use>,
}

fn category(inst: &crate::model::Instance, m: &SharedMember) -> Category {
    if m.pin.is_some() {
        Category::Pin
    } else if inst.rules.peripherals.iter().any(|p| p.name == m.name) {
        Category::Peripheral
    } else {
        Category::Function
    }
}

fn use_of(model: &Model, pin: InstId, path: &str, function: &str) -> Use {
    Use {
        path: path.to_string(),
        function: Some(function.to_string()),
        joined_to: None,
        at: model.function_stated_at(pin),
    }
}

/// The members of a resource that are in use, in the path order of their first use.
fn active(
    model: &Model,
    inst: &Instance,
    chosen: &[(InstId, String, String)],
    s: &SharedRes,
) -> Vec<Active> {
    let mut found: Vec<Active> = Vec::new();
    for m in &s.members {
        let category = category(inst, m);
        let uses: Vec<Use> = match (category, m.pin) {
            (Category::Pin, Some(pin)) => {
                let function = model.function_of(pin);
                let joined_to = if function.is_none() && model.is_connected(pin) {
                    let mut others: Vec<String> = model
                        .others_on_net(pin)
                        .into_iter()
                        .map(|o| model.path_of(o))
                        .collect();
                    others.sort();
                    Some(others.first().cloned().unwrap_or_else(|| {
                        model
                            .net_of(pin)
                            .map(|n| n.name.clone())
                            .unwrap_or_default()
                    }))
                } else {
                    None
                };
                if function.is_none() && joined_to.is_none() {
                    Vec::new()
                } else {
                    vec![Use {
                        path: model.path_of(pin),
                        at: model.function_stated_at(pin),
                        function,
                        joined_to,
                    }]
                }
            }
            (Category::Peripheral, _) => {
                let Some(p) = inst.rules.peripherals.iter().find(|p| p.name == m.name) else {
                    continue;
                };
                chosen
                    .iter()
                    .filter(|(_, _, f)| p.functions.iter().any(|g| g.name == *f))
                    .map(|(pin, path, f)| use_of(model, *pin, path, f))
                    .collect()
            }
            _ => chosen
                .iter()
                .filter(|(_, _, f)| *f == m.name)
                .map(|(pin, path, f)| use_of(model, *pin, path, f))
                .collect(),
        };
        if uses.is_empty() || found.iter().any(|a| a.member.name == m.name) {
            continue;
        }
        found.push(Active {
            member: m.clone(),
            category,
            uses,
        });
    }
    found.sort_by(|a, b| a.uses[0].path.cmp(&b.uses[0].path));
    found
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

    // The pins of the part, in path order: every pin that any of its tables has a row for, with
    // the function the board chose on it.
    let mut pins: Vec<InstId> = inst
        .tables
        .iter()
        .flat_map(|t| t.rows.iter().map(|r| r.pin))
        .collect();
    pins.sort_by_key(|&p| model.path_of(p));
    pins.dedup();
    let chosen: Vec<(InstId, String, String)> = pins
        .iter()
        .filter_map(|&pin| Some((pin, model.path_of(pin), model.function_of(pin)?)))
        .collect();
    let active = |s: &'_ SharedRes| active(model, inst, &chosen, s);

    let place = |u: &Use| -> String {
        u.at.and_then(|(source, span)| model.place_of_statement(source, span))
            .map(|p| format!(" ({p})"))
            .unwrap_or_default()
    };
    let one_use = |u: &Use, member: &SharedMember, category: Category| -> String {
        match category {
            Category::Peripheral => format!(
                "on `{}` as {}{}",
                u.path,
                u.function.as_deref().unwrap_or_default(),
                place(u)
            ),
            Category::Function => format!("on `{}`{}", u.path, place(u)),
            Category::Pin => match (&u.function, &u.joined_to) {
                (Some(f), _) => format!("`{}` as {f}{}", u.path, place(u)),
                (None, Some(other)) => format!("`{}` connected to `{other}`", u.path),
                (None, None) => format!("`{}`", member.name),
            },
        }
    };
    let member_text = |a: &Active| -> String {
        let each: Vec<String> = a
            .uses
            .iter()
            .map(|u| one_use(u, &a.member, a.category))
            .collect();
        match a.category {
            Category::Pin => list_of(&each),
            _ => format!("{} {}", a.member.name, list_of(&each)),
        }
    };
    let shown = |a: &Active| -> String {
        match a.category {
            Category::Pin => format!("`{}`", a.uses[0].path),
            _ => a.member.name.clone(),
        }
    };
    let kind_of_res = |s: &SharedRes| -> Option<Category> {
        let first = category(inst, s.members.first()?);
        s.members
            .iter()
            .all(|m| category(inst, m) == first)
            .then_some(first)
    };

    let mut shares: Vec<&SharedRes> = inst.rules.shares.iter().collect();
    shares.sort_by(|a, b| a.name.cmp(&b.name));
    shares.dedup_by(|a, b| a.name == b.name);

    let mut out = Vec::new();
    for s in &shares {
        let used = active(s);
        let limit = s.limit as usize;
        if used.len() <= limit {
            continue;
        }
        let over = &used[limit];
        let at = over.uses[0].at.unwrap_or_else(|| inst.at());
        let members: Vec<String> = used.iter().map(member_text).collect();
        let message = if limit > 1 {
            // More members in use than a limit allows.
            let word = match kind_of_res(s) {
                Some(Category::Peripheral) => "peripherals",
                Some(Category::Function) => "functions",
                Some(Category::Pin) => "pins",
                None => "members",
            };
            let listed_members: Vec<String> = s
                .members
                .iter()
                .map(|m| match m.pin {
                    Some(_) => format!("`{}`", m.name),
                    None => m.name.clone(),
                })
                .collect();
            format!(
                "`{path}` (`{ty}`) uses {} of the {word} of `{}`, which allows {} at a time: {}; `{}` lists {}",
                used.len(),
                s.name,
                s.limit,
                members.join(", "),
                s.name,
                list_of(&listed_members)
            )
        } else if used.iter().all(|a| a.category == Category::Pin) {
            // Pins that cannot both be used.
            let names: Vec<String> = used.iter().map(shown).collect();
            let both = if used.len() == 2 { "both" } else { "all" };
            format!(
                "{} of `{ty}` are {both} in use, but they share `{}`, which allows 1 at a time: {}",
                list_of(&names),
                s.name,
                list_of(&members)
            )
        } else {
            // Members that run one at a time: say whether another resource like this one is free.
            let names: Vec<String> = used.iter().map(shown).collect();
            let like = kind_of_res(s);
            let others: Vec<String> = shares
                .iter()
                .filter(|o| o.name != s.name && o.limit == s.limit && kind_of_res(o) == like)
                .map(|o| {
                    let by: Vec<String> = active(o).iter().map(shown).collect();
                    if by.is_empty() {
                        format!("`{}` has nothing in use", o.name)
                    } else {
                        format!("`{}` by {}", o.name, list_of(&by))
                    }
                })
                .collect();
            let all_busy = shares
                .iter()
                .filter(|o| o.name != s.name && o.limit == s.limit && kind_of_res(o) == like)
                .all(|o| !active(o).is_empty());
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
            format!(
                "`{path}` (`{ty}`) uses {}, which share `{}` and run one at a time: {}{rest}",
                list_of(&names),
                s.name,
                members.join(", ")
            )
        };
        out.push((at, message));
    }
    out
}
