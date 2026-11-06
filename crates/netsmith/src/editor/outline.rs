//! The outline of a source file as the page needs it: each definition with the lines it
//! spans and the declarations in its body. All of it is read from the parser's spans, never
//! from the text.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use netsmith_check::Loader;
use netsmith_editor::api::{Def, DefChild};
use netsmith_lang::{
    Span,
    ast::{Decl, DefKind, Definition, File, Item},
    parse,
};

/// The most declarations a signature names before it says `...`.
const SIGNATURE_MEMBERS: usize = 6;

/// What kind of definition a type name means, from a file: one the file defines, or one it
/// imports from a module of the project or of the standard library. A module is parsed the
/// first time a name needs it.
pub struct Kinds<'a> {
    texts: HashMap<&'a str, &'a str>,
    parsed: RefCell<HashMap<String, Option<Rc<File>>>>,
}

impl<'a> Kinds<'a> {
    pub fn new(loader: &'a Loader) -> Self {
        Self {
            texts: loader
                .modules()
                .into_iter()
                .map(|(key, _, text)| (key, text))
                .collect(),
            parsed: RefCell::new(HashMap::new()),
        }
    }

    /// Knows no module: only the definitions of a file itself resolve.
    pub fn alone() -> Self {
        Self {
            texts: HashMap::new(),
            parsed: RefCell::new(HashMap::new()),
        }
    }

    fn module(&self, key: &str) -> Option<Rc<File>> {
        let text = self.texts.get(key)?;
        self.parsed
            .borrow_mut()
            .entry(key.to_string())
            .or_insert_with(|| parse(text).ok().map(Rc::new))
            .clone()
    }

    /// The kind of the definition that `name`, written in `from`, means.
    pub fn kind(&self, from: &File, name: &str) -> Option<DefKind> {
        if let Some(d) = from.definitions.iter().find(|d| d.name.name == name) {
            return Some(d.kind);
        }
        for import in &from.imports {
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
            let file = self.module(&module)?;
            return file
                .definitions
                .iter()
                .find(|d| d.name.name == name)
                .map(|d| d.kind);
        }
        None
    }
}

/// The 1-based line of the byte `offset` of `text`.
pub fn line_at(text: &str, offset: usize) -> u32 {
    text[..offset.min(text.len())].matches('\n').count() as u32 + 1
}

/// The line the last character of `span` is on (`span.end` is one past it).
fn last_line(text: &str, span: Span) -> u32 {
    line_at(text, span.end.saturating_sub(1).max(span.start))
}

/// The definitions of a source text with the lines each spans and the declarations in its
/// body. A text that does not parse has none shown.
pub fn definitions(text: &str, kinds: &Kinds) -> Vec<Def> {
    let Ok(file) = parse(text) else {
        return Vec::new();
    };
    file.definitions
        .iter()
        .map(|d| {
            let children: Vec<DefChild> = d
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Decl(decl) => Some(child(text, &file, decl, kinds)),
                    _ => None,
                })
                .collect();
            Def {
                kind: d.kind.keyword().to_string(),
                name: d.name.name.clone(),
                line: line_at(text, d.span.start),
                end_line: Some(last_line(text, d.span)),
                children: (!children.is_empty()).then_some(children),
            }
        })
        .collect()
}

fn child(text: &str, file: &File, decl: &Decl, kinds: &Kinds) -> DefChild {
    let type_name = type_name(decl);
    DefChild {
        name: decl
            .name
            .segments
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join("."),
        kind: kinds
            .kind(file, &type_name)
            .map(|k| k.keyword().to_string()),
        line: line_at(text, decl.span.start),
        end_line: last_line(text, decl.span),
        type_name,
    }
}

fn type_name(decl: &Decl) -> String {
    decl.ty
        .segments
        .last()
        .map(|s| s.name.clone())
        .unwrap_or_default()
}

/// `.vin: Rail`, as a declaration reads without its block.
pub fn declaration_signature(decl: &Decl) -> String {
    format!(
        "{}{}: {}",
        if decl.name.leading_dot { "." } else { "" },
        decl.name
            .segments
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join("."),
        decl.ty
            .segments
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join("::"),
    )
}

/// A one-line signature of a definition: its keyword and name and the declarations that
/// make its interface, `module LdoStage { .vin: Rail .vout: Rail ... }`. The declarations
/// are those whose type is an interface (the ports of a module, a board's rails and
/// process), or, where there are none or the type is not found, the declarations in
/// source order; at most six, then `...` if the body has anything more. The same
/// definition always gives the same line.
pub fn definition_signature(def: &Definition, file: &File, kinds: &Kinds) -> String {
    let decls: Vec<&Decl> = def
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Decl(d) => Some(d),
            _ => None,
        })
        .collect();
    let ports: Vec<&Decl> = decls
        .iter()
        .copied()
        .filter(|d| kinds.kind(file, &type_name(d)) == Some(DefKind::Interface))
        .collect();
    // An interface or a part is its own declarations; so is any definition with no port.
    let shown = if ports.is_empty() || matches!(def.kind, DefKind::Interface | DefKind::Part) {
        decls
    } else {
        ports
    };
    let mut out = format!("{} {}", def.kind.keyword(), def.name.name);
    if shown.is_empty() && def.items.is_empty() {
        out.push_str(" {}");
        return out;
    }
    out.push_str(" {");
    for d in shown.iter().take(SIGNATURE_MEMBERS) {
        out.push(' ');
        out.push_str(&declaration_signature(d));
    }
    let more = def.items.len() > shown.len().min(SIGNATURE_MEMBERS);
    if more {
        out.push_str(" ...");
    }
    out.push_str(" }");
    out
}

/// The `///` lines above a definition or declaration, joined with newlines.
pub fn doc_text(lines: &[String]) -> Option<String> {
    let text = lines
        .iter()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n");
    (!text.trim().is_empty()).then_some(text)
}
