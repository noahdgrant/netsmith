//! Go to definition: where the name under the cursor is defined.

use netsmith_check::Loader;
use netsmith_editor::api::Location;

use super::outline::{self, Kinds};
use netsmith_lang::{
    Span,
    ast::{Decl, Expr, File, Item, Path},
    floor_boundary, parse,
};

/// A parsed source file.
struct Parsed {
    /// What the tree and the diagnostics call it.
    path: String,
    text: String,
    file: File,
}

impl Parsed {
    fn of(path: &str, text: &str) -> Option<Parsed> {
        Some(Parsed {
            path: path.to_string(),
            text: text.to_string(),
            file: parse(text).ok()?,
        })
    }

    fn at(&self, span: Span) -> Location {
        let position = |offset: usize| {
            let before = &self.text[..floor_boundary(&self.text, offset)];
            (
                before.matches('\n').count() as u32 + 1,
                before.rsplit('\n').next().map_or(0, |l| l.chars().count()) as u32 + 1,
            )
        };
        let (line, column) = position(span.start);
        let (end_line, end_column) = position(span.end);
        Location {
            file: self.path.clone(),
            line,
            column,
            end_line,
            end_column,
        }
    }
}

/// Where a name is defined, and what a hover card can say of it without reading the file.
pub struct Found {
    pub at: Location,
    /// `module LdoStage { .vin: Rail .vout: Rail ... }`, or `.vin: Rail` for a member.
    pub signature: Option<String>,
    /// The `///` lines above it (the `//!` lines of a module's file for an imported
    /// module), joined with newlines.
    pub doc: Option<String>,
}

impl Found {
    fn definition(module: &Parsed, index: usize, kinds: &Kinds) -> Found {
        let def = &module.file.definitions[index];
        Found {
            at: module.at(def.name.span),
            signature: Some(outline::definition_signature(def, &module.file, kinds)),
            doc: outline::doc_text(&def.docs),
        }
    }

    fn member(module: &Parsed, decl: &Decl) -> Found {
        Found {
            at: module.at(decl.name.segments[0].span),
            signature: Some(outline::declaration_signature(decl)),
            doc: outline::doc_text(&decl.docs),
        }
    }
}

/// Where the name at character `offset` (UTF-16 units, as the page counts) of `text`,
/// which is the file `path`, is defined: an imported module or name, a type, or a member
/// of the definition it is written in or of the type of the member before it.
pub fn find(loader: &Loader, path: &str, text: &str, offset: usize) -> Option<Found> {
    let kinds = Kinds::new(loader);
    let here = Parsed::of(path, text)?;
    let byte = byte_of(text, offset)?;

    // `import a::b::Name`: the module, or the name inside it.
    for import in &here.file.imports {
        let all: Vec<_> = import
            .path
            .iter()
            .chain(import.names.iter().flatten())
            .collect();
        let Some(clicked) = all.iter().position(|i| contains(i.span, byte)) else {
            continue;
        };
        let module_len = if import.names.is_some() {
            import.path.len()
        } else {
            import.path.len() - 1
        };
        let key = |n: usize| {
            import.path[..n]
                .iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>()
                .join("::")
        };
        let module = module_of(
            loader,
            &key(if clicked < module_len {
                clicked + 1
            } else {
                module_len
            }),
        )?;
        if clicked < module_len {
            return Some(Found {
                at: module.at(Span::new(0, 0)),
                signature: None,
                doc: outline::doc_text(&module.file.inner_docs),
            });
        }
        let index = module
            .file
            .definitions
            .iter()
            .position(|d| d.name.name == all[clicked].name)?;
        return Some(Found::definition(&module, index, &kinds));
    }

    let (index, def) = here
        .file
        .definitions
        .iter()
        .enumerate()
        .find(|(_, d)| contains(d.span, byte))?;

    // `part X provides G`: the generic.
    if let Some(name) = def.provides.iter().find(|n| contains(n.span, byte)) {
        let (module, i) = resolve_type(loader, &here, &name.name)?;
        return Some(Found::definition(&module, i, &kinds));
    }

    // Every declaration and path of the definition, with the declarations around each.
    let mut found = Vec::new();
    for item in &def.items {
        visit(item, &mut Vec::new(), &mut found);
    }
    for (inside, thing) in &found {
        match thing {
            Thing::Type(decl) => {
                if let Some(seg) = decl.ty.segments.iter().find(|s| contains(s.span, byte)) {
                    let (module, i) = resolve_type(loader, &here, &seg.name)?;
                    return Some(Found::definition(&module, i, &kinds));
                }
            }
            Thing::Path(p) if contains(p.span, byte) => {
                let clicked = p.segments.iter().position(|s| contains(s.span, byte))?;
                // Names are looked for in the definition, then, one at a time, in the
                // definition each member's type names.
                let mut current = Parsed::of(&here.path, &here.text)?;
                let mut at = index;
                for decl in inside {
                    let (next, i) = resolve_type(loader, &current, &decl.ty.segments.last()?.name)?;
                    current = next;
                    at = i;
                }
                for (n, segment) in p.segments.iter().enumerate().take(clicked + 1) {
                    let decl = decls_of(&current.file.definitions[at].items)
                        .into_iter()
                        .find(|d| {
                            d.name
                                .segments
                                .first()
                                .is_some_and(|s| s.name == segment.name)
                        })?;
                    if n == clicked {
                        return Some(Found::member(&current, decl));
                    }
                    let ty = decl.ty.segments.last()?.name.clone();
                    let (next, i) = resolve_type(loader, &current, &ty)?;
                    current = next;
                    at = i;
                }
            }
            Thing::Path(_) => {}
        }
    }
    None
}

enum Thing<'a> {
    Type(&'a Decl),
    Path(&'a Path),
}

/// Collect the declarations (for their types) and paths under `item`, each with the
/// declarations whose body it is in.
fn visit<'a>(
    item: &'a Item,
    inside: &mut Vec<&'a Decl>,
    out: &mut Vec<(Vec<&'a Decl>, Thing<'a>)>,
) {
    let mut path = |p: &'a Path, inside: &Vec<&'a Decl>| out.push((inside.clone(), Thing::Path(p)));
    match item {
        Item::Decl(d) => {
            out.push((inside.clone(), Thing::Type(d)));
            out.push((inside.clone(), Thing::Path(&d.name)));
            if let Some(body) = &d.body {
                inside.push(d);
                for inner in body {
                    visit(inner, inside, out);
                }
                inside.pop();
            }
        }
        Item::Connect(c) => {
            path(&c.first, inside);
            for (_, p) in &c.rest {
                path(p, inside);
            }
        }
        Item::Constraint(c) => {
            let mut paths = Vec::new();
            expr_paths(&c.lhs, &mut paths);
            expr_paths(&c.rhs, &mut paths);
            if let Some(at) = &c.at {
                expr_paths(at, &mut paths);
            }
            for p in paths {
                path(p, inside);
            }
        }
        Item::Dc(d) => {
            for p in &d.operands {
                path(p, inside);
            }
        }
        Item::Row(r) => {
            path(&r.first, inside);
            path(&r.last, inside);
            let mut paths = Vec::new();
            expr_paths(&r.pitch, &mut paths);
            for p in paths {
                path(p, inside);
            }
        }
        Item::Bridge(b) => {
            path(&b.from, inside);
            path(&b.to, inside);
        }
        Item::Catalog(c) => {
            for p in c.key.iter().chain(&c.fields) {
                path(p, inside);
            }
        }
        Item::Tie(t) => {
            path(&t.a, inside);
            path(&t.b, inside);
        }
        Item::Pins(pins) => {
            for row in &pins.rows {
                path(&row.pin, inside);
            }
        }
        // The lines of an interface are its own pins, and the line a row crosses to.
        Item::Lines(lines) => {
            for row in &lines.rows {
                path(&row.pin, inside);
                if let Some(other) = &row.crosses {
                    path(other, inside);
                }
            }
        }
        // A peripheral names functions, which are words and not paths.
        Item::Peripheral(_) => {}
        Item::Shared(shared) => {
            for member in &shared.members {
                if let netsmith_lang::ast::SharedMember::Pin(p) = member {
                    path(p, inside);
                }
            }
        }
        Item::Case(case) => {
            for inner in &case.items {
                visit(inner, inside, out);
            }
        }
    }
}

fn expr_paths<'a>(e: &'a Expr, out: &mut Vec<&'a Path>) {
    match e {
        Expr::Path(p) => out.push(p),
        Expr::Neg(inner, _) | Expr::Paren(inner, _) => expr_paths(inner, out),
        Expr::Binary { lhs, rhs, .. } => {
            expr_paths(lhs, out);
            expr_paths(rhs, out);
        }
        Expr::Range { lo, hi, .. } => {
            expr_paths(lo, out);
            expr_paths(hi, out);
        }
        Expr::Tolerance { center, tol, .. } => {
            expr_paths(center, out);
            expr_paths(tol, out);
        }
        Expr::Number(..) | Expr::Str(..) | Expr::Variant { .. } => {}
    }
}

fn decls_of(items: &[Item]) -> Vec<&Decl> {
    items
        .iter()
        .filter_map(|i| match i {
            Item::Decl(d) => Some(d),
            _ => None,
        })
        .collect()
}

/// A module by its import path, parsed.
fn module_of(loader: &Loader, key: &str) -> Option<Parsed> {
    let (_, file, text) = loader.modules().into_iter().find(|(k, _, _)| *k == key)?;
    Parsed::of(file, text)
}

/// The file and definition that the type name `name`, written in `from`, means: one it
/// defines, or one it imports.
fn resolve_type(loader: &Loader, from: &Parsed, name: &str) -> Option<(Parsed, usize)> {
    if let Some(i) = from
        .file
        .definitions
        .iter()
        .position(|d| d.name.name == name)
    {
        return Some((Parsed::of(&from.path, &from.text)?, i));
    }
    for import in &from.file.imports {
        let key = |n: usize| {
            import.path[..n]
                .iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>()
                .join("::")
        };
        let module = match &import.names {
            Some(names) if names.iter().any(|n| n.name == name) => key(import.path.len()),
            None if import.path.last().is_some_and(|l| l.name == name) => {
                key(import.path.len() - 1)
            }
            _ => continue,
        };
        let module = module_of(loader, &module)?;
        let i = module
            .file
            .definitions
            .iter()
            .position(|d| d.name.name == name)?;
        return Some((module, i));
    }
    None
}

fn contains(span: Span, byte: usize) -> bool {
    span.start <= byte && byte < span.end.max(span.start + 1)
}

fn byte_of(text: &str, utf16: usize) -> Option<usize> {
    let mut units = 0;
    for (byte, c) in text.char_indices() {
        if units >= utf16 {
            return Some(byte);
        }
        units += c.len_utf16();
    }
    (units >= utf16).then_some(text.len())
}
