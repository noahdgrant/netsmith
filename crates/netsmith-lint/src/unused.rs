//! [`Rule::UnusedDefinition`]: what the project declares and never uses.
//!
//! - A definition (a module, an interface, a part, an enum, a generic, an enclosure) that no
//!   board, system or panel of the project reaches, through the names the definitions write,
//!   the physical files' units and the definitions the manifest names. A provider is reached
//!   with the generic it provides. Only when the whole project is linted, and only in a project
//!   that has a board: a project of modules only is a library of them.
//! - A member that a module or a board declares (`.r3: Resistor`, `.vin: Power`, `.en: Pin`)
//!   and no instance of it joins: no pin of it is on a net with another pin. A member with no
//!   pin (a mounting hole, an `Environment`) is not judged by this, and neither is a member of a
//!   physical file, which places parts and joins none.
//! - A value member of a module or a board (`.gain: Ratio`) that no statement of any instance
//!   narrows, defaults or reads.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use netsmith_check::{FieldId, InstId, InstKind, Member, Model};
use netsmith_lang::Span;
use netsmith_lang::ast::{DefKind, Item};

use crate::{Context, FileKind, Lint, Rule};

pub(crate) fn lint(cx: &Context) -> Vec<Lint> {
    let mut out = definitions(cx);
    out.extend(members(cx));
    out
}

/// The word for a kind of definition in a message.
fn kind_word(kind: DefKind) -> &'static str {
    kind.keyword()
}

fn definitions(cx: &Context) -> Vec<Lint> {
    if !cx.project.whole {
        return Vec::new();
    }
    // Every definition of the project: (file, index).
    let mut by_name: HashMap<&str, Vec<(&str, usize)>> = HashMap::new();
    let mut roots: VecDeque<(&str, usize)> = VecDeque::new();
    let mut has_board = false;
    for (name, file) in &cx.files {
        for (i, def) in file.ast.definitions.iter().enumerate() {
            by_name
                .entry(def.name.name.as_str())
                .or_default()
                .push((name.as_str(), i));
            if matches!(def.kind, DefKind::Board | DefKind::System | DefKind::Panel) {
                roots.push_back((name.as_str(), i));
                has_board = true;
            }
        }
    }
    if !has_board {
        return Vec::new();
    }
    // The providers of each generic, by the generic's name.
    let mut providers: HashMap<&str, Vec<(&str, usize)>> = HashMap::new();
    for (name, file) in &cx.files {
        for (i, def) in file.ast.definitions.iter().enumerate() {
            for g in &def.provides {
                providers
                    .entry(g.name.as_str())
                    .or_default()
                    .push((name.as_str(), i));
            }
        }
    }
    // The bare names each definition writes, and those the files write outside a definition
    // (a physical unit, a `layout` block keyed to a generic).
    let mut written: HashMap<(&str, usize), Vec<String>> = HashMap::new();
    let mut outside: Vec<String> = cx.project.named.clone();
    for (name, file) in &cx.files {
        let Some(names) = netsmith_lang::bare_names(&file.text) else {
            continue;
        };
        let spans: Vec<Span> = file.ast.definitions.iter().map(|d| d.span).collect();
        let imports: Vec<Span> = file.ast.imports.iter().map(|i| i.span).collect();
        for (n, span) in names {
            if imports
                .iter()
                .any(|s| s.start <= span.start && span.start < s.end)
            {
                continue;
            }
            match spans
                .iter()
                .position(|s| s.start <= span.start && span.start < s.end)
            {
                Some(i) => written.entry((name.as_str(), i)).or_default().push(n),
                None => outside.push(n),
            }
        }
    }
    type Defs<'a> = HashMap<&'a str, Vec<(&'a str, usize)>>;
    fn reach<'a>(
        by_name: &Defs<'a>,
        providers: &Defs<'a>,
        n: &str,
        queue: &mut VecDeque<(&'a str, usize)>,
    ) {
        for &d in by_name.get(n).into_iter().flatten() {
            queue.push_back(d);
        }
        for &d in providers.get(n).into_iter().flatten() {
            queue.push_back(d);
        }
    }
    let mut reached: HashSet<(&str, usize)> = HashSet::new();
    for n in &outside {
        reach(&by_name, &providers, n, &mut roots);
    }
    let mut queue = roots;
    while let Some(d) = queue.pop_front() {
        if !reached.insert(d) {
            continue;
        }
        for n in written.get(&d).into_iter().flatten() {
            reach(&by_name, &providers, n, &mut queue);
        }
    }
    let mut out = Vec::new();
    for (name, file) in &cx.files {
        for (i, def) in file.ast.definitions.iter().enumerate() {
            if reached.contains(&(name.as_str(), i)) {
                continue;
            }
            out.push(file.lint(
                Rule::UnusedDefinition,
                def.name.span,
                format!(
                    "the {} `{}` is never used: no board of the project reaches it; use it, or delete it",
                    kind_word(def.kind),
                    def.name.name
                ),
            ));
        }
    }
    out
}

/// What is known of one declaration over every instance it made.
struct Seen {
    name: String,
    /// Whether one instance of it has a pin, a pin on a net with another, or a value a
    /// statement states or reads.
    judged: bool,
    used: bool,
    value: bool,
}

fn members(cx: &Context) -> Vec<Lint> {
    // By (file, start of the declaration).
    let mut seen: BTreeMap<(String, usize), Seen> = BTreeMap::new();
    for (checked, model) in cx.models() {
        let joined = joined_pins(model);
        let read = read_fields(model);
        for (id, inst) in model.instances() {
            let Some((source, span)) = inst.declared else {
                continue;
            };
            if id == model.root || !declared_in_module_or_board(model, inst.parent) {
                continue;
            }
            let file = &checked.sources.get(source).name;
            let Some(f) = cx.files.get(file) else {
                continue;
            };
            if f.kind == FileKind::Physical {
                continue;
            }
            let pins = pins_of(model, id);
            let entry = seen.entry((file.clone(), span.start)).or_insert(Seen {
                name: inst.name.clone(),
                judged: false,
                used: false,
                value: false,
            });
            if pins.is_empty() {
                continue;
            }
            entry.judged = true;
            if pins.iter().any(|p| joined.contains(p)) {
                entry.used = true;
            }
        }
        // The value members of modules and boards: found by their declaration in the
        // definition's text.
        for (_, inst) in model.instances() {
            if !matches!(inst.kind, InstKind::Module | InstKind::Board) {
                continue;
            }
            let file = &checked.sources.get(inst.source).name;
            let Some(f) = cx.files.get(file) else {
                continue;
            };
            let Some(def) = f
                .ast
                .definitions
                .iter()
                .find(|d| d.span.start == inst.span.start)
            else {
                continue;
            };
            for (name, member) in &inst.children {
                let Member::Field(field) = member else {
                    continue;
                };
                let Some(decl) = value_decl(&def.items, name) else {
                    continue;
                };
                let entry = seen.entry((file.clone(), decl.start)).or_insert(Seen {
                    name: name.clone(),
                    judged: true,
                    used: false,
                    value: true,
                });
                if read.contains(field) || is_stated(model, *field) {
                    entry.used = true;
                }
            }
        }
    }
    let mut out = Vec::new();
    for ((file, start), s) in seen {
        if !s.judged || s.used {
            continue;
        }
        let f = &cx.files[&file];
        let span = Span::new(start, start);
        let message = if s.value {
            format!(
                "`.{}` is declared and never used: no statement states or reads it; state it, read it, or delete it",
                s.name
            )
        } else {
            format!(
                "`.{}` is declared and never joined: no pin of it is on a net; join it, or delete it",
                s.name
            )
        };
        out.push(f.lint(Rule::UnusedDefinition, span, message));
    }
    out
}

/// Whether a member whose parent is `parent` is written in a module's or a board's body.
fn declared_in_module_or_board(model: &Model, parent: Option<InstId>) -> bool {
    parent.is_some_and(|p| {
        matches!(
            model.instance(p).kind,
            InstKind::Module | InstKind::Board | InstKind::System
        )
    })
}

/// The pins of `inst`, itself when it is one, and every pin below it.
fn pins_of(model: &Model, inst: InstId) -> Vec<InstId> {
    let mut out = Vec::new();
    let mut stack = vec![inst];
    while let Some(i) = stack.pop() {
        let it = model.instance(i);
        if it.kind == InstKind::Pin {
            out.push(i);
        }
        for (_, m) in &it.children {
            if let Member::Inst(c) = m {
                stack.push(*c);
            }
        }
    }
    out
}

/// The pins that are on a net with another pin, or are a port a system joins.
fn joined_pins(model: &Model) -> HashSet<InstId> {
    let mut out = HashSet::new();
    for net in model.nets() {
        if net.pins.len() >= 2 {
            out.extend(net.pins.iter().copied());
        }
    }
    for (a, b) in model.joins() {
        out.extend(pins_of(model, *a));
        out.extend(pins_of(model, *b));
    }
    out
}

/// The fields a relation reads, or an equality joins.
fn read_fields(model: &Model) -> HashSet<FieldId> {
    let mut out = Vec::new();
    for r in model.relations() {
        r.lhs.fields(&mut out);
        r.rhs.fields(&mut out);
    }
    for (a, b) in model.equalities() {
        out.push(*a);
        out.push(*b);
    }
    out.into_iter().collect()
}

/// Whether a statement narrows the field or gives it a default.
fn is_stated(model: &Model, field: FieldId) -> bool {
    let f = model.field(field);
    !f.narrowed_at.is_empty() || f.default_from.is_some() || !f.roles.is_empty()
}

/// The span of the declaration of the value member `name` among `items`.
fn value_decl(items: &[Item], name: &str) -> Option<Span> {
    items.iter().find_map(|item| match item {
        Item::Decl(d)
            if d.count.is_none() && matches!(d.name.segments.as_slice(), [n] if n.name == name) =>
        {
            Some(d.name.span)
        }
        _ => None,
    })
}
