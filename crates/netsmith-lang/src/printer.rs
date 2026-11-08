//! Canonical source text for a syntax tree. Plain comments and blank lines are not
//! kept (doc comments are), so this is for tests and tools, not an in-place formatter.

use std::fmt::Write as _;

use crate::ast::*;

impl File {
    pub fn to_source(&self) -> String {
        let mut out = String::new();
        write_docs(&mut out, &self.inner_docs, "//!", 0);
        let has_body =
            !(self.imports.is_empty() && self.definitions.is_empty() && self.physicals.is_empty());
        if !self.inner_docs.is_empty() && has_body {
            out.push('\n');
        }
        for import in &self.imports {
            out.push_str(&import.to_source());
            out.push('\n');
        }
        for (i, def) in self.definitions.iter().enumerate() {
            if i > 0 || !self.imports.is_empty() {
                out.push('\n');
            }
            write_docs(&mut out, &def.docs, "///", 0);
            let _ = write!(out, "{} {} ", def.kind.keyword(), def.name.to_source());
            if !def.provides.is_empty() {
                let _ = write!(out, "provides {} ", join(&def.provides, ", "));
            }
            if def.kind == DefKind::Enum {
                write_variants(&mut out, &def.inner_docs, &def.variants);
            } else {
                write_block(&mut out, &def.inner_docs, &def.items, 0);
            }
            out.push('\n');
        }
        for (i, unit) in self.physicals.iter().enumerate() {
            if i > 0 || !self.imports.is_empty() || !self.definitions.is_empty() {
                out.push('\n');
            }
            write_docs(&mut out, &unit.docs, "///", 0);
            let _ = write!(out, "physical for {} ", unit.board.to_source());
            write_block(&mut out, &unit.inner_docs, &unit.items, 0);
            out.push('\n');
        }
        out
    }
}

impl Import {
    pub fn to_source(&self) -> String {
        let path = join(&self.path, "::");
        match &self.names {
            Some(names) => format!("import {path}::{{{}}}", join(names, ", ")),
            None => format!("import {path}"),
        }
    }
}

impl Ident {
    pub fn to_source(&self) -> String {
        if self.quoted {
            format!("'{}'", self.name)
        } else {
            self.name.clone()
        }
    }
}

impl Path {
    pub fn to_source(&self) -> String {
        let dot = if self.leading_dot { "." } else { "" };
        format!("{dot}{}", join(&self.segments, "."))
    }
}

impl PinFunction {
    pub fn to_source(&self) -> String {
        let mut out = match self.kind {
            Some((kind, _)) => format!("{}:{}", self.name.to_source(), kind.word()),
            None => self.name.to_source(),
        };
        if let Some(line) = &self.line {
            let _ = write!(out, " as {}", line.to_source());
        }
        out
    }
}

impl PinRow {
    pub fn to_source(&self) -> String {
        let mut out = format!("{} {}", self.pin.to_source(), self.kind.word());
        if !self.is_contract() {
            let _ = write!(out, " {}", quote(&self.name));
        }
        for pad in &self.pads {
            let _ = write!(out, " {}", quote(&pad.text));
        }
        if !self.functions.is_empty() {
            let functions: Vec<String> =
                self.functions.iter().map(PinFunction::to_source).collect();
            let _ = write!(out, " fn {}", functions.join(" "));
        }
        if let Some((requirement, _)) = self.requirement {
            let _ = write!(out, " {}", requirement.word());
        }
        out
    }
}

impl LineRow {
    pub fn to_source(&self) -> String {
        let mut out = format!("{} {}", self.pin.to_source(), self.kind.word());
        if self.single.is_some() {
            out.push_str(" single");
        }
        if let Some(other) = &self.crosses {
            let _ = write!(out, " crosses {}", other.to_source());
        }
        if let Some((requirement, _)) = self.requirement {
            let _ = write!(out, " {}", requirement.word());
        }
        out
    }
}

impl Expr {
    pub fn to_source(&self) -> String {
        match self {
            Self::Number(n, _) => n.text.clone(),
            Self::Str(s, _) => quote(s),
            Self::Path(p) => p.to_source(),
            Self::Variant { ty, variant, .. } => {
                format!("{}::{}", join(&ty.segments, "::"), variant.to_source())
            }
            Self::Neg(inner, _) => format!("-{}", inner.to_source()),
            Self::Paren(inner, _) => format!("({})", inner.to_source()),
            Self::Range { lo, hi, .. } => {
                // A path with a leading dot after `..` would read as `...`.
                let hi = hi.to_source();
                let gap = if hi.starts_with('.') { " " } else { "" };
                format!("{}..{gap}{hi}", lo.to_source())
            }
            Self::Tolerance { center, tol, .. } => {
                format!("{} +/- {}", center.to_source(), tol.to_source())
            }
            Self::Binary { op, lhs, rhs, .. } => {
                let sym = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                };
                format!("{} {sym} {}", lhs.to_source(), rhs.to_source())
            }
        }
    }
}

impl CmpOp {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Narrow => "&=",
            Self::Default => "?=",
            Self::EqEq => "==",
            Self::Ge => ">=",
            Self::Le => "<=",
        }
    }
}

fn join(items: &[Ident], sep: &str) -> String {
    items
        .iter()
        .map(Ident::to_source)
        .collect::<Vec<_>>()
        .join(sep)
}

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One doc line per entry, indented to `depth`. An empty line is just the marker.
fn write_docs(out: &mut String, docs: &[String], marker: &str, depth: usize) {
    for line in docs {
        out.push_str(&"    ".repeat(depth));
        out.push_str(marker);
        if !line.is_empty() {
            out.push(' ');
            out.push_str(line);
        }
        out.push('\n');
    }
}

/// Writes the `{ ... }` of an `enum`: one variant on each line.
fn write_variants(out: &mut String, inner_docs: &[String], variants: &[Variant]) {
    if variants.is_empty() && inner_docs.is_empty() {
        out.push_str("{}");
        return;
    }
    out.push_str("{\n");
    write_docs(out, inner_docs, "//!", 1);
    for variant in variants {
        write_docs(out, &variant.docs, "///", 1);
        out.push_str("    ");
        out.push_str(&variant.name.to_source());
        out.push_str(",\n");
    }
    out.push('}');
}

/// Writes `{ ... }` at `depth`, leaving the cursor after the closing brace.
fn write_block(out: &mut String, inner_docs: &[String], items: &[Item], depth: usize) {
    if items.is_empty() && inner_docs.is_empty() {
        out.push_str("{}");
        return;
    }
    out.push_str("{\n");
    write_docs(out, inner_docs, "//!", depth + 1);
    for item in items {
        if let Item::Decl(d) = item {
            write_docs(out, &d.docs, "///", depth + 1);
        }
        out.push_str(&"    ".repeat(depth + 1));
        match item {
            Item::Decl(d) => {
                let _ = write!(
                    out,
                    "{}: {}",
                    d.name.to_source(),
                    join(&d.ty.segments, "::")
                );
                if d.optional.is_some() {
                    out.push_str(" optional");
                }
                if let Some(body) = &d.body {
                    out.push(' ');
                    write_block(out, &[], body, depth + 1);
                }
            }
            Item::Constraint(c) => {
                let _ = write!(
                    out,
                    "{} {} {}",
                    c.lhs.to_source(),
                    c.op.symbol(),
                    c.rhs.to_source()
                );
            }
            Item::Bridge(b) => {
                let _ = write!(out, "bridge {} ~> {}", b.from.to_source(), b.to.to_source());
            }
            Item::Catalog(c) => {
                let fields: Vec<_> = c
                    .fields
                    .iter()
                    .zip(&c.optional)
                    .map(|(f, o)| match o {
                        Some(_) => format!("{} optional", f.to_source()),
                        None => f.to_source(),
                    })
                    .collect();
                let key = match &c.key {
                    Some(k) => format!(" by {}", k.to_source()),
                    None => String::new(),
                };
                let _ = write!(
                    out,
                    "catalog {}{key} {{ {} }}",
                    c.name.to_source(),
                    fields.join(", ")
                );
            }
            Item::Tie(t) => {
                let _ = write!(out, "tie {} ~ {}", t.a.to_source(), t.b.to_source());
            }
            Item::Peripheral(p) => {
                let functions: Vec<String> =
                    p.functions.iter().map(PinFunction::to_source).collect();
                let on = match &p.on {
                    Some(class) => format!(" on {}", class.to_source()),
                    None => String::new(),
                };
                let bus = match &p.bus {
                    Some(bus) => {
                        let role = match &bus.role {
                            Some(r) => format!(" {}", r.to_source()),
                            None => String::new(),
                        };
                        format!(" as {}{role}", join(&bus.interface.segments, "::"))
                    }
                    None => String::new(),
                };
                let _ = write!(
                    out,
                    "peripheral {}{on}{bus} {{ {} }}",
                    p.name.to_source(),
                    functions.join(" ")
                );
            }
            Item::Shared(s) => {
                let limit = match s.limit {
                    Some((n, _)) => format!(" limit {n}"),
                    None => String::new(),
                };
                let members: Vec<String> = s
                    .members
                    .iter()
                    .map(|m| match m {
                        SharedMember::Name(n) => n.to_source(),
                        SharedMember::Pin(p) => p.to_source(),
                    })
                    .collect();
                let _ = write!(
                    out,
                    "shared {}{limit} {{ {} }}",
                    s.name.to_source(),
                    members.join(" ")
                );
            }
            Item::Lines(l) => {
                out.push_str("lines");
                if let Some(role) = &l.role {
                    let _ = write!(out, " {}", role.to_source());
                }
                if let Some((n, _)) = l.limit {
                    let _ = write!(out, " limit {n}");
                }
                out.push_str(" {\n");
                for row in &l.rows {
                    out.push_str(&"    ".repeat(depth + 2));
                    out.push_str(&row.to_source());
                    out.push('\n');
                }
                out.push_str(&"    ".repeat(depth + 1));
                out.push('}');
            }
            Item::Pins(p) => {
                out.push_str("pins ");
                if let Some((package, _)) = &p.package {
                    let _ = write!(out, "{} ", quote(package));
                }
                if p.rows.is_empty() {
                    out.push_str("{}");
                } else {
                    out.push_str("{\n");
                    for row in &p.rows {
                        out.push_str(&"    ".repeat(depth + 2));
                        out.push_str(&row.to_source());
                        out.push('\n');
                    }
                    out.push_str(&"    ".repeat(depth + 1));
                    out.push('}');
                }
            }
            Item::Case(c) => {
                out.push_str("case ");
                write_block(out, &[], &c.items, depth + 1);
            }
            Item::Row(r) => {
                let _ = write!(
                    out,
                    "row {} .. {} along {} pitch {}",
                    r.first.to_source(),
                    r.last.to_source(),
                    r.axis.word(),
                    r.pitch.to_source()
                );
            }
            Item::Connect(c) => {
                out.push_str(&c.first.to_source());
                for (op, path) in &c.rest {
                    let sym = match op {
                        ConnectOp::Same => "~",
                        ConnectOp::Series => "~>",
                    };
                    let _ = write!(out, " {sym} {}", path.to_source());
                }
            }
        }
        out.push('\n');
    }
    out.push_str(&"    ".repeat(depth));
    out.push('}');
}
