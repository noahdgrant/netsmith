//! Canonical source text for a syntax tree. Plain comments and blank lines are not
//! kept (doc comments are), so this is for tests and tools, not an in-place formatter.

use std::fmt::Write as _;

use crate::ast::*;

impl File {
    pub fn to_source(&self) -> String {
        let mut out = String::new();
        write_docs(&mut out, &self.inner_docs, "//!", 0);
        let has_body = !(self.imports.is_empty()
            && self.definitions.is_empty()
            && self.physicals.is_empty()
            && self.layouts.is_empty());
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
            write_attrs(&mut out, &def.attrs, 0);
            let _ = write!(out, "{} {}", def.kind.keyword(), def.name.to_source());
            if !def.params.is_empty() {
                let params: Vec<String> = def
                    .params
                    .iter()
                    .map(|p| format!("{}: {}", p.name.to_source(), p.bound.to_source()))
                    .collect();
                let _ = write!(out, "<{}>", params.join(", "));
            }
            out.push(' ');
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
        for (i, block) in self.layouts.iter().enumerate() {
            if i > 0
                || !(self.imports.is_empty()
                    && self.definitions.is_empty()
                    && self.physicals.is_empty())
            {
                out.push('\n');
            }
            write_layout(&mut out, block, 0);
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

impl TypePath {
    /// A type as written: `Rail`, `Buck<Lt8641>`. Not for a pin request (`.u.GPIO`).
    pub fn to_source(&self) -> String {
        let mut out = join(&self.segments, "::");
        if !self.args.is_empty() {
            let args: Vec<String> = self.args.iter().map(TypePath::to_source).collect();
            let _ = write!(out, "<{}>", args.join(", "));
        }
        out
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
        if self.class.is_some() {
            out.push_str(" class");
        }
        if let Some(of) = &self.of {
            let _ = write!(out, " of {}", of.to_source());
        }
        if let Some(line) = &self.line {
            let _ = write!(out, " as {}", line.to_source());
        }
        out
    }
}

impl PinLimits {
    /// The limits as a row ends with them, with the space before them.
    pub fn to_source(&self) -> String {
        let mut out = String::new();
        if let Some(min) = &self.min {
            let _ = write!(out, " abs_min {}", min.value.to_source());
        }
        if let Some(max) = &self.max {
            let _ = write!(out, " abs_max {}", max.value.to_source());
        }
        if let Some(from) = &self.from {
            let _ = write!(out, " from {}", from.to_source());
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
        if let Some(limits) = &self.limits {
            out.push_str(&limits.to_source());
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
            Self::Call { name, args, .. } => {
                let args: Vec<String> = args.iter().map(Expr::to_source).collect();
                format!("{}({})", name.to_source(), args.join(", "))
            }
            Self::Range { lo, hi, .. } => {
                let lo = lo.as_ref().map(|e| e.to_source()).unwrap_or_default();
                let hi = hi.as_ref().map(|e| e.to_source()).unwrap_or_default();
                // A path with a leading dot after `..` would read as `...`.
                let gap = if hi.starts_with('.') { " " } else { "" };
                format!("{lo}..{gap}{hi}")
            }
            Self::Tolerance { center, tol, .. } => {
                format!("{} +/- {}", center.to_source(), tol.to_source())
            }
            Self::SupplySteps {
                rows, interpolated, ..
            } => {
                let rows = rows
                    .iter()
                    .map(|r| format!("{} at {}", r.value.to_source(), r.at.to_source()))
                    .collect::<Vec<_>>()
                    .join(", ");
                if *interpolated {
                    format!("{rows} interpolated")
                } else {
                    rows
                }
            }
            Self::Steps { steps, .. } => steps
                .iter()
                .map(|s| format!("{} to {}", s.value.to_source(), s.to.to_source()))
                .collect::<Vec<_>>()
                .join(", "),
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

pub(crate) fn quote(s: &str) -> String {
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

/// One attribute per line, indented to `depth`: `#[series(E24, E96)]`.
fn write_attrs(out: &mut String, attrs: &[Attribute], depth: usize) {
    for attr in attrs {
        out.push_str(&"    ".repeat(depth));
        out.push_str(&attr.to_source());
        out.push('\n');
    }
}

impl Attribute {
    pub fn to_source(&self) -> String {
        if self.args.is_empty() {
            return format!("#[{}]", self.name.to_source());
        }
        let args: Vec<String> = self
            .args
            .iter()
            .map(|a| match a {
                AttrArg::Name(i) => i.to_source(),
                AttrArg::Text { text, .. } => quote(text),
            })
            .collect();
        format!("#[{}({})]", self.name.to_source(), args.join(", "))
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
            write_attrs(out, &d.attrs, depth + 1);
        }
        out.push_str(&"    ".repeat(depth + 1));
        match item {
            Item::Decl(d) => {
                let ty = if d.ty.of_member {
                    format!(".{}", join(&d.ty.segments, "."))
                } else {
                    d.ty.to_source()
                };
                let count = d.count.map_or(String::new(), |c| format!("{} x ", c.n));
                let ty = if d.list.is_some() {
                    format!("[{ty}]")
                } else {
                    ty
                };
                let _ = write!(out, "{}: {count}{ty}", d.name.to_source());
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
                if let Some(at) = &c.at {
                    let _ = write!(out, " at {}", at.to_source());
                }
            }
            Item::Dc(d) => {
                let _ = write!(out, "dc {}", d.kind.name);
                for operand in &d.operands {
                    let _ = write!(out, " {}", operand.to_source());
                }
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
            Item::Direction(d) => {
                let _ = write!(
                    out,
                    "direction {} to {} when {} {}",
                    d.from.to_source(),
                    d.to.to_source(),
                    d.control.to_source(),
                    if d.high { "high" } else { "low" }
                );
            }
            Item::Unused(u) => {
                let _ = write!(out, "unused {}", u.path.to_source());
            }
            Item::Sequence(q) => {
                out.push_str("sequence {\n");
                for step in &q.steps {
                    out.push_str(&"    ".repeat(depth + 2));
                    let _ = match step {
                        SequenceStep::Event(p) => writeln!(out, "{}", p.to_source()),
                        SequenceStep::Then {
                            delay: Some((n, _)),
                            ..
                        } => {
                            writeln!(out, "then {}", n.text)
                        }
                        SequenceStep::Then { delay: None, .. } => writeln!(out, "then"),
                        SequenceStep::After { event, .. } => {
                            writeln!(out, "after {}", event.to_source())
                        }
                    };
                }
                out.push_str(&"    ".repeat(depth + 1));
                out.push('}');
            }
            Item::Scenario(q) => {
                let _ = writeln!(out, "scenario {} {{", q.name.to_source());
                for step in &q.steps {
                    out.push_str(&"    ".repeat(depth + 2));
                    let _ = writeln!(out, "{} {}", step.word.word(), step.path.to_source());
                }
                out.push_str(&"    ".repeat(depth + 1));
                out.push('}');
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
                for (package, _) in &p.packages {
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
            Item::Pads(p) => {
                out.push_str("pads ");
                for (package, _) in &p.packages {
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
            Item::CaseOf(c) => {
                out.push_str("case ");
                out.push_str(&c.name.to_source());
            }
            Item::Support(c) => {
                out.push_str("support ");
                write_block(out, &[], &c.items, depth + 1);
            }
            Item::Defaults(c) => {
                out.push_str("defaults ");
                write_block(out, &[], &c.items, depth + 1);
            }
            Item::Layout(l) => write_layout(out, l, depth + 1),
            Item::Polygon(p) => {
                out.push_str("polygon");
                let point = |p: &Point| format!("({}, {})", p.x.to_source(), p.y.to_source());
                for (k, c) in p.corners.iter().enumerate() {
                    // A long run is put three corners to a line, under the first.
                    if k > 0 && k % 3 == 0 {
                        out.push('\n');
                        out.push_str(&"    ".repeat(depth + 1));
                        out.push_str("       ");
                    }
                    let _ = write!(out, " {}", point(&c.at));
                    if let Some(r) = &c.round {
                        let _ = write!(out, " round {}", r.to_source());
                    }
                    if let Some(t) = &c.through {
                        let _ = write!(out, " arc through {}", point(t));
                    }
                }
            }
            Item::PhysicalFile(f) => {
                let _ = write!(out, "physical {}", quote(&f.file));
            }
            Item::Anchor(a) => {
                let _ = write!(
                    out,
                    "anchor .{} = {}",
                    a.name.to_source(),
                    a.target.to_source()
                );
            }
            Item::Within(w) => {
                let _ = write!(
                    out,
                    "{} within {} of {}",
                    w.subject.to_source(),
                    w.distance.to_source(),
                    w.pin.to_source()
                );
            }
            Item::Route(r) => {
                let _ = write!(out, "{} route {}", r.pin.to_source(), r.word());
            }
            Item::SoftMargin(m) => {
                let _ = write!(
                    out,
                    "soft margin {} round {}",
                    m.margin.to_source(),
                    m.part.to_source()
                );
            }
            Item::KeepApart(k) => {
                let _ = write!(
                    out,
                    "keep {} between {} and {}",
                    k.distance.to_source(),
                    k.kinds[0].name,
                    k.kinds[1].name
                );
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
            Item::JoinMap(b) => {
                let _ = write!(out, "{} ~ {}", b.left.to_source(), b.right.to_source());
                if !b.renames.is_empty() {
                    let renames: Vec<String> = b
                        .renames
                        .iter()
                        .map(|(from, to)| format!("{} = {}", from.to_source(), to.to_source()))
                        .collect();
                    let _ = write!(out, " {{ {} }}", renames.join("; "));
                }
                if !b.except.is_empty() {
                    let names: Vec<String> = b.except.iter().map(Ident::to_source).collect();
                    let _ = write!(out, " except {{ {} }}", names.join(" "));
                }
            }
            Item::Connect(c) => {
                out.push_str(&c.first.to_source());
                for (i, (op, path)) in c.rest.iter().enumerate() {
                    let sym = match op {
                        ConnectOp::Same => "~",
                        ConnectOp::Series => "~>",
                        ConnectOp::Default => "?~",
                    };
                    match c.groups.iter().find(|g| g.at == i) {
                        Some(g) => {
                            let _ = write!(out, " {sym} {} x {}", g.count.n, g.ty.to_source());
                            if let Some(body) = &g.body {
                                out.push(' ');
                                write_block(out, &[], body, depth + 1);
                            }
                        }
                        None => {
                            let _ = write!(out, " {sym} {}", path.to_source());
                        }
                    }
                }
            }
        }
        out.push('\n');
    }
    out.push_str(&"    ".repeat(depth));
    out.push('}');
}

impl Select {
    /// The selector as written: `bridging .a .b`, `on .pin kind inductor`.
    pub fn to_source(&self) -> String {
        match self {
            Select::Bridging(a, b) => format!("bridging {} {}", a.to_source(), b.to_source()),
            Select::On { pin, kind } => match kind {
                Some(k) => format!("on {} kind {}", pin.to_source(), k.to_source()),
                None => format!("on {}", pin.to_source()),
            },
        }
    }
}

impl Guideline {
    /// The statement as written, without its doc comment.
    pub fn to_source(&self) -> String {
        let after = |out: &mut String, word: &str, e: &Option<Expr>| {
            if let Some(e) = e {
                let _ = write!(out, " {word} {}", e.to_source());
            }
        };
        let mut out = format!("{} {}", self.rule.word(), self.name.to_source());
        match &self.rule {
            Rule::Near { select, to, within } => {
                let _ = write!(out, " {} to {}", select.to_source(), to.to_source());
                after(&mut out, "within", within);
            }
            Rule::Away {
                select,
                from,
                beyond,
            } => {
                let _ = write!(
                    out,
                    " {} from {} beyond {}",
                    select.to_source(),
                    from.to_source(),
                    beyond.to_source()
                );
            }
            Rule::Loop { a, b, within } => {
                let _ = write!(out, " bridging {} {}", a.to_source(), b.to_source());
                after(&mut out, "within", within);
            }
            Rule::Keepout(area) => {
                let KeepoutArea {
                    x,
                    y,
                    width,
                    height,
                    forbid,
                } = &**area;
                let _ = write!(
                    out,
                    " at {}, {} size {}, {}",
                    x.to_source(),
                    y.to_source(),
                    width.to_source(),
                    height.to_source()
                );
                if !forbid.is_empty() {
                    out.push_str(" forbid");
                    for f in forbid {
                        let _ = write!(out, " {}", f.to_source());
                    }
                }
            }
            Rule::Edge { pin, within } => {
                let _ = write!(out, " {}", pin.to_source());
                after(&mut out, "within", within);
            }
            Rule::Vias { pin, count } => {
                let _ = write!(out, " {} {}", pin.to_source(), count.to_source());
            }
            Rule::Route { pin, width, length } => {
                let _ = write!(out, " {}", pin.to_source());
                after(&mut out, "width", width);
                after(&mut out, "length", length);
            }
            Rule::Kelvin { select, to } => {
                let _ = write!(out, " {} to {}", select.to_source(), to.to_source());
            }
        }
        out
    }
}

/// A `layout` block, from its keyword to its closing brace.
fn write_layout(out: &mut String, l: &Layout, depth: usize) {
    match (&l.keyed, &l.package) {
        (Some(k), _) => {
            let _ = writeln!(
                out,
                "layout {} for {} {{",
                quote(&k.mpn.0),
                k.generic.to_source()
            );
        }
        (None, Some((package, _))) => {
            let _ = writeln!(out, "layout {} {{", quote(package));
        }
        (None, None) => out.push_str("layout {\n"),
    }
    let indent = "    ".repeat(depth + 1);
    for statement in &l.statements {
        match statement {
            LayoutStatement::Source { text, .. } => {
                let _ = writeln!(out, "{indent}source {}", quote(text));
            }
            LayoutStatement::Guideline(g) => {
                write_docs(out, &g.docs, "///", depth + 1);
                let _ = writeln!(out, "{indent}{}", g.to_source());
            }
        }
    }
    out.push_str(&"    ".repeat(depth));
    out.push('}');
}
