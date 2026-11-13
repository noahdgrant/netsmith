//! Groups of identical members (ADR 0119). `.decoupling: 4 x Capacitor { ... }` is four
//! members, `.decoupling[0]` to `.decoupling[3]`, each declared with the group's body, in
//! index order where the group is declared. The group's doc comment goes with the first. A chain with a group alone between its two ends
//! (`.v.pos ~> .decoupling ~> .v.neg`) is one chain for each member, so each stands in
//! parallel between the ends. A group written in a chain (`~> 2 x Capacitor { ... } ~>`) is
//! declared where the chain is, under the name the parser gave it.
//!
//! The expansion works on the syntax tree, before the checker, so every later stage sees plain
//! members whose names end in an index. The parser keeps the tree as written, so that it
//! prints back to its source.

use std::collections::HashMap;

use crate::ast::*;
use crate::error::ParseError;

/// Expand every group of `file` into its members, and every chain through a group into one
/// chain for each member. Returns what cannot be expanded; the statements it names are dropped.
pub fn expand_groups(file: &mut File) -> Vec<ParseError> {
    let mut errors = Vec::new();
    for def in &mut file.definitions {
        expand_items(&mut def.items, &mut errors);
    }
    for unit in &mut file.physicals {
        expand_items(&mut unit.items, &mut errors);
    }
    errors
}

/// A group declared in one body: its count, and whether it was written in a chain.
struct Group {
    n: u32,
    inline: bool,
}

fn expand_items(items: &mut Vec<Item>, errors: &mut Vec<ParseError>) {
    for item in items.iter_mut() {
        match item {
            Item::Decl(d) => {
                if let Some(body) = &mut d.body {
                    expand_items(body, errors);
                }
            }
            Item::Support(s) => expand_items(&mut s.items, errors),
            Item::Case(c) => expand_items(&mut c.items, errors),
            Item::Connect(c) => {
                for g in &mut c.groups {
                    if let Some(body) = &mut g.body {
                        expand_items(body, errors);
                    }
                }
            }
            _ => {}
        }
    }

    let mut groups: HashMap<String, Group> = HashMap::new();
    // The groups in chains whose names are taken: their chains are dropped with the error.
    let mut refused = Vec::new();
    for item in items.iter() {
        match item {
            Item::Decl(d) => {
                if let (Some(count), [name]) = (d.count, d.name.segments.as_slice()) {
                    groups.insert(
                        name.name.clone(),
                        Group {
                            n: count.n,
                            inline: false,
                        },
                    );
                }
            }
            Item::Connect(c) => {
                for g in &c.groups {
                    let name = c.rest[g.at].1.segments[0].name.clone();
                    let ends = format!(
                        "`{}` between `{}` and `{}`",
                        g.ty.to_source(),
                        c.first.to_source(),
                        c.rest[1].1.to_source()
                    );
                    if let Some(other) = groups.get(&name) {
                        let message = if other.inline {
                            format!(
                                "a second group of {ends}: a group in a chain is named from its kind and its ends (`{name}`), so two would have one name; declare each group by a name of its own (`.bulk: 2 x {kind} {{ ... }}`, then `{} ~> .bulk ~> {}`)",
                                c.first.to_source(),
                                c.rest[1].1.to_source(),
                                kind = g.ty.to_source(),
                            )
                        } else {
                            format!(
                                "this group of {ends} is named `{name}`, from its kind and its ends, and that is a member of this definition already; name the group yourself (`.bulk: 2 x {kind} {{ ... }}`)",
                                kind = g.ty.to_source(),
                            )
                        };
                        errors.push(ParseError::new(g.span, message));
                        refused.push(g.span);
                        continue;
                    }
                    groups.insert(
                        name,
                        Group {
                            n: g.count.n,
                            inline: true,
                        },
                    );
                }
            }
            _ => {}
        }
    }
    if groups.is_empty() {
        return;
    }

    let mut out = Vec::with_capacity(items.len());
    for item in std::mem::take(items) {
        match item {
            Item::Decl(d) if d.count.is_some() => {
                let count = d.count.expect("matched");
                let name = &d.name.segments[0];
                for i in 0..count.n {
                    let mut member = d.clone();
                    member.count = None;
                    // The group's documentation is one note, on its first member.
                    if i > 0 {
                        member.docs.clear();
                    }
                    member.name.segments = vec![indexed(name, i)];
                    out.push(Item::Decl(member));
                }
            }
            Item::Connect(c) if c.groups.iter().any(|g| refused.contains(&g.span)) => {}
            Item::Connect(mut c) => {
                for g in std::mem::take(&mut c.groups) {
                    let name = c.rest[g.at].1.segments[0].clone();
                    for i in 0..g.count.n {
                        out.push(Item::Decl(Decl {
                            docs: Vec::new(),
                            name: Path {
                                leading_dot: true,
                                segments: vec![indexed(&name, i)],
                                span: g.span,
                            },
                            ty: g.ty.clone(),
                            count: None,
                            optional: None,
                            body: g.body.clone(),
                            span: g.span,
                        }));
                    }
                }
                expand_chain(c, &groups, &mut out, errors);
            }
            other => out.push(other),
        }
    }
    *items = out;
}

/// `decoupling[2]`, at the span of the group's name.
fn indexed(name: &Ident, i: u32) -> Ident {
    Ident {
        name: format!("{}[{i}]", name.name),
        quoted: false,
        span: name.span,
    }
}

/// The name of the group `path` is, if it is one: `.decoupling`, a single name with a dot.
fn group_of<'a>(path: &'a Path, groups: &HashMap<String, Group>) -> Option<&'a str> {
    match path.segments.as_slice() {
        [name] if path.leading_dot && groups.contains_key(&name.name) => Some(&name.name),
        _ => None,
    }
}

/// One chain for each member of the group the chain passes through, or the chain itself if
/// it passes through none.
fn expand_chain(
    c: Connect,
    groups: &HashMap<String, Group>,
    out: &mut Vec<Item>,
    errors: &mut Vec<ParseError>,
) {
    let operands: Vec<&Path> = std::iter::once(&c.first)
        .chain(c.rest.iter().map(|(_, p)| p))
        .collect();
    let found: Vec<(usize, &str)> = operands
        .iter()
        .enumerate()
        .filter_map(|(i, p)| group_of(p, groups).map(|g| (i, g)))
        .collect();
    let Some(&(at, name)) = found.first() else {
        out.push(Item::Connect(c));
        return;
    };
    let alone = found.len() == 1
        && at == 1
        && c.rest.len() == 2
        && c.rest.iter().all(|(op, _)| *op == ConnectOp::Series);
    let group = &groups[name];
    if !alone {
        let path = operands[at];
        errors.push(ParseError::new(
            path.span,
            format!(
                "`.{name}` is a group of {n}, and a group in a chain stands alone between the chain's two ends, each member in parallel (`.v.pos ~> .{name} ~> .v.neg`); join one member by its index, `.{name}[0]` to `.{name}[{last}]`",
                n = group.n,
                last = group.n - 1,
            ),
        ));
        return;
    }
    for i in 0..group.n {
        let mut member = c.clone();
        let segment = &mut member.rest[0].1.segments[0];
        *segment = indexed(segment, i);
        out.push(Item::Connect(member));
    }
}
