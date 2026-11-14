//! A board's `defaults { Resistor.package ?= "0402" }` block (issue #625, ADR 0128): a value the
//! board offers to every part or module of a kind on it, at any depth, that states nothing on
//! the field.
//!
//! A line is checked where it is written: the kind is a part, a module or a generic in scope,
//! the field is a value it declares (or a generic it provides declares), and the value is one
//! the field holds. When every statement of the board has been read, and before the defaults
//! of ADR 0051 are settled, each line becomes a pending default of each field it reaches. It
//! gives way to anything a statement says of the field: a `&=` or a `?=` on it, or on a field
//! that `==` or a join makes equal to it. A line that reaches no field is a note.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use netsmith_lang::Span;
use netsmith_lang::ast::{self, CmpOp, DefKind, Definition, Expr, Item};

use crate::checker::{Binding, Checker};
use crate::diag::SourceId;
use crate::lower::{PendingDefault, Typed, VTy};
use crate::model::{DefaultNote, FieldId, FieldKind, InstId, InstKind, Member, RExpr};

/// One line of a `defaults` block, checked and waiting for the board to be read.
pub(crate) struct BoardDefault {
    board: InstId,
    /// The definition the line names.
    kind: (usize, usize),
    kind_name: String,
    field: String,
    value: Typed,
    source: SourceId,
    span: Span,
}

/// The type of a value member as its declaration writes it.
#[derive(Clone, Copy)]
enum Declared {
    Quantity(netsmith_quantity::Dimension),
    Text,
    /// An enum, by its definition.
    Enum(usize, usize),
}

const EXAMPLE: &str = "`Resistor.package ?= \"0402\"`";

impl Checker<'_> {
    /// Check the lines of a `defaults` block of the board being read and keep them.
    pub(crate) fn defaults_block(&mut self, block: &ast::Defaults) {
        if self.phys.is_some() {
            self.error(
                block.span,
                "`defaults` belongs in the board's source, not in its physical file: it offers values of the circuit",
            );
            return;
        }
        if !self.in_board() {
            self.error(
                block.span,
                "only a board holds `defaults`: it offers a value to every part or module of a kind on the board; a module or a part states its own default with `?=` on its member",
            );
            return;
        }
        let mut seen: HashMap<((usize, usize), String), Span> = HashMap::new();
        for item in &block.items {
            let Some(line) = self.default_line(item) else {
                continue;
            };
            let key = (line.kind, line.field.clone());
            if let Some(&first) = seen.get(&key) {
                let place = self.place_of(line.source, first);
                self.error(
                    line.span,
                    format!(
                        "`{}.{}` already has a default in this block, at {place}: keep one line",
                        line.kind_name, line.field
                    ),
                );
                continue;
            }
            seen.insert(key, line.span);
            self.board_defaults.push(line);
        }
    }

    /// One line of the block, if it reads.
    fn default_line(&mut self, item: &Item) -> Option<BoardDefault> {
        let Item::Constraint(c) = item else {
            let span = item_span(item);
            self.error(
                span,
                format!(
                    "a `defaults` block holds only lines that name a kind, one of its fields and a value, such as {EXAMPLE}"
                ),
            );
            return None;
        };
        if c.op != CmpOp::Default {
            self.error(
                c.span,
                format!(
                    "a line of `defaults` offers a value with `?=`, such as {EXAMPLE}, not `{}`: what a board requires of one part it states on that part",
                    c.op.symbol()
                ),
            );
            return None;
        }
        if let Some(at) = &c.at {
            self.error(at.span(), "a line of `defaults` takes no `at`");
            return None;
        }
        let Expr::Path(path) = &c.lhs else {
            self.error(
                c.lhs.span(),
                format!(
                    "a line of `defaults` names a kind and one of its fields, such as {EXAMPLE}"
                ),
            );
            return None;
        };
        if path.leading_dot {
            self.error(
                path.span,
                format!(
                    "`{}` is a member of the board; a line of `defaults` names a kind, such as {EXAMPLE}, and a member is stated in the board's body",
                    path.to_source()
                ),
            );
            return None;
        }
        if path.segments.len() != 2 {
            self.error(
                path.span,
                format!(
                    "a line of `defaults` names a kind and one field it declares, such as {EXAMPLE}; `{}` is not that",
                    path.to_source()
                ),
            );
            return None;
        }
        let (kind_id, field_id) = (&path.segments[0], &path.segments[1]);
        let kind = self.default_kind(kind_id)?;
        let fields = self.value_members(kind);
        let Some(&(_, declared)) = fields.iter().find(|(n, _)| *n == field_id.name) else {
            let kind_name = &kind_id.name;
            let names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
            let hint = crate::lower::closest(&field_id.name, &names)
                .map(|n| format!("; did you mean `{n}`?"))
                .unwrap_or_else(|| {
                    let mut listed: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
                    listed.sort();
                    if listed.is_empty() {
                        "; it declares no value".to_string()
                    } else {
                        format!("; its values are {}", listed.join(", "))
                    }
                });
            self.error(
                field_id.span,
                format!("`{kind_name}` declares no value `{}`{hint}", field_id.name),
            );
            return None;
        };
        let ty = match declared {
            Declared::Quantity(d) => VTy::Q(d),
            Declared::Text => VTy::Text,
            Declared::Enum(m, i) => VTy::Enum(self.enum_id(m, i)),
        };
        let beside = Typed {
            e: RExpr::Text(String::new()),
            ty,
        };
        let value = self.lower_beside(&beside, &c.rhs)?;
        if !matches!(value.e, RExpr::Const(_) | RExpr::Text(_)) {
            self.error(
                c.rhs.span(),
                "a line of `defaults` offers a value, such as `\"0402\"` or `25V..50V`, not one read from a field",
            );
            return None;
        }
        let kind_of_field = match declared {
            Declared::Quantity(d) => FieldKind::Quantity(d),
            Declared::Text => FieldKind::Text,
            Declared::Enum(m, i) => FieldKind::Enum(self.enum_id(m, i)),
        };
        let fits = match (kind_of_field, value.ty) {
            (FieldKind::Quantity(a), VTy::Q(b)) => a == b,
            (FieldKind::Text, VTy::Text) => true,
            (FieldKind::Enum(a), VTy::Enum(b)) => a == b,
            _ => false,
        };
        if !fits {
            let holds = self.describe_kind(kind_of_field);
            let is = self.describe(value.ty);
            self.error(
                c.rhs.span(),
                format!(
                    "`{}.{}` holds {holds}, but this value is {is}",
                    kind_id.name, field_id.name
                ),
            );
            return None;
        }
        Some(BoardDefault {
            board: self.current(),
            kind,
            kind_name: kind_id.name.clone(),
            field: field_id.name.clone(),
            value,
            source: self.cur_source(),
            span: c.span,
        })
    }

    /// The part, module or generic a line names, reported when it is none.
    fn default_kind(&mut self, name: &ast::Ident) -> Option<(usize, usize)> {
        let found = match self.modules[self.cur_module].scope.get(&name.name) {
            Some(Binding::Def(m, i)) => Some((*m, *i)),
            Some(Binding::Poison) => return None,
            Some(Binding::Variant(..)) | None => self.prelude_def(&name.name),
        };
        let Some((m, i)) = found else {
            let names = self.kinds_in_scope();
            let hint = crate::lower::closest(&name.name, &names)
                .map(|n| format!("; did you mean `{n}`?"))
                .unwrap_or_else(|| "; import it, or define it in this file".to_string());
            self.error(
                name.span,
                format!(
                    "no part, module or generic `{}` is in scope{hint}",
                    name.name
                ),
            );
            return None;
        };
        let def = &self.modules[m].defs[i];
        if !matches!(def.kind, DefKind::Part | DefKind::Module | DefKind::Generic) {
            let word = def.kind.keyword();
            self.error(
                name.span,
                format!(
                    "`{}` is {} {word}; a line of `defaults` names a part, a module or a generic",
                    name.name,
                    if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
                        "an"
                    } else {
                        "a"
                    }
                ),
            );
            return None;
        }
        Some((m, i))
    }

    /// The parts, modules and generics the file being read can name, for "did you mean".
    fn kinds_in_scope(&self) -> Vec<String> {
        let mut names: Vec<String> = self.modules[self.cur_module]
            .scope
            .iter()
            .filter_map(|(n, b)| match b {
                Binding::Def(m, i)
                    if matches!(
                        self.modules[*m].defs[*i].kind,
                        DefKind::Part | DefKind::Module | DefKind::Generic
                    ) =>
                {
                    Some(n.clone())
                }
                _ => None,
            })
            .collect();
        names.sort();
        names
    }

    /// The value members a kind declares, with their types: its own, then those of the
    /// generics it provides. A member whose type is not a value (a pin, an interface, a part)
    /// is left out.
    fn value_members(&mut self, kind: (usize, usize)) -> Vec<(String, Declared)> {
        let def = self.modules[kind.0].defs[kind.1].clone();
        let mut out = Vec::new();
        self.collect_values(kind.0, &def, &mut out);
        let saved = self.enter_def(kind);
        let provided = self.provided(&def);
        self.leave_def(saved);
        for (g, (m, _)) in provided {
            self.collect_values(m, &g, &mut out);
        }
        out
    }

    fn collect_values(
        &mut self,
        module: usize,
        def: &Rc<Definition>,
        out: &mut Vec<(String, Declared)>,
    ) {
        for item in &def.items {
            let Item::Decl(d) = item else { continue };
            if !d.name.leading_dot
                || d.name.segments.len() != 1
                || d.count.is_some()
                || d.ty.of_member
                || d.ty.segments.len() != 1
            {
                continue;
            }
            let name = d.name.segments[0].name.clone();
            if out.iter().any(|(n, _)| *n == name) {
                continue;
            }
            let ty = d.ty.segments[0].name.as_str();
            let declared = match crate::checker::builtin_value(ty) {
                Some(Some(dim)) => Declared::Quantity(dim),
                Some(None) => Declared::Text,
                None => {
                    let found = match self.modules[module].scope.get(ty) {
                        Some(Binding::Def(m, i)) => Some((*m, *i)),
                        _ => self.prelude_def(ty),
                    };
                    match found {
                        Some((m, i)) if self.modules[m].defs[i].kind == DefKind::Enum => {
                            Declared::Enum(m, i)
                        }
                        _ => continue,
                    }
                }
            };
            out.push((name, declared));
        }
    }

    /// Give each field that a line of a board's `defaults` reaches a pending default, before
    /// the defaults of ADR 0051 are settled; and keep a note of each line that reaches none.
    pub(crate) fn apply_board_defaults(&mut self) {
        let lines = std::mem::take(&mut self.board_defaults);
        if lines.is_empty() {
            return;
        }
        // What a statement says of a field reaches every field that `==` or a join makes equal
        // to it: the defaults of a board give way to it there as well.
        let mut eq: HashMap<usize, usize> = HashMap::new();
        fn find(eq: &mut HashMap<usize, usize>, x: usize) -> usize {
            let p = *eq.get(&x).unwrap_or(&x);
            if p == x {
                return x;
            }
            let r = find(eq, p);
            eq.insert(x, r);
            r
        }
        let pairs: Vec<(FieldId, FieldId)> = self
            .model
            .relations()
            .iter()
            .filter(|r| r.op == CmpOp::EqEq)
            .filter_map(|r| match (&r.lhs, &r.rhs) {
                (RExpr::Field(a), RExpr::Field(b)) => Some((*a, *b)),
                _ => None,
            })
            .chain(self.model.equalities.iter().copied())
            .collect();
        for (a, b) in pairs {
            let (ra, rb) = (find(&mut eq, a.0), find(&mut eq, b.0));
            if ra != rb {
                eq.insert(ra, rb);
            }
        }
        let spoken: Vec<FieldId> = self
            .stated
            .iter()
            .copied()
            .chain(self.defaults.iter().map(|d| d.field))
            .collect();
        let spoken_classes: HashSet<usize> = spoken.iter().map(|f| find(&mut eq, f.0)).collect();

        // Which definitions each instance is of: its own and the generics it provides.
        let insts: Vec<(InstId, (usize, usize))> = self
            .model
            .instances()
            .filter(|(_, i)| matches!(i.kind, InstKind::Part | InstKind::Module))
            .filter_map(|(id, i)| i.def.map(|d| (id, d)))
            .collect();
        let mut kinds_of: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new();
        let mut reached: HashMap<FieldId, Span> = HashMap::new();
        for line in lines {
            let mut found = 0usize;
            let mut given = 0usize;
            for &(inst, def) in &insts {
                if !self.model.is_inside(inst, line.board) {
                    continue;
                }
                let kinds = match kinds_of.entry(def) {
                    std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                    std::collections::hash_map::Entry::Vacant(e) => {
                        let d = self.modules[def.0].defs[def.1].clone();
                        let saved = self.enter_def(def);
                        let provided = self.provided(&d);
                        self.leave_def(saved);
                        e.insert(
                            std::iter::once(def)
                                .chain(provided.into_iter().map(|(_, r)| r))
                                .collect(),
                        )
                    }
                };
                if !kinds.contains(&line.kind) {
                    continue;
                }
                let Some(Member::Field(field)) = self.model.member(inst, &line.field) else {
                    continue;
                };
                found += 1;
                if spoken_classes.contains(&find(&mut eq, field.0)) {
                    continue;
                }
                if let Some(&other) = reached.get(&field) {
                    let path = self.model.path_of_field(field);
                    let place = self.place_of(line.source, other);
                    self.report(
                        line.source,
                        line.span,
                        format!(
                            "`{path}` is reached by two lines of `defaults`, this one and the one at {place}: a field takes one default, so state it on the part with `&=`"
                        ),
                    );
                    continue;
                }
                reached.insert(field, line.span);
                given += 1;
                self.board_defaulted.insert(field);
                self.defaults.push(PendingDefault {
                    field,
                    value: Typed {
                        e: line.value.e.clone(),
                        ty: line.value.ty,
                    },
                    source: line.source,
                    span: line.span,
                });
            }
            if given == 0 {
                let what = format!("`{}.{}`", line.kind_name, line.field);
                let message = if found == 0 {
                    format!(
                        "the default for {what} reaches nothing: the board has no `{}`",
                        line.kind_name
                    )
                } else {
                    format!(
                        "the default for {what} reaches nothing: every `{}` on the board states `.{}` itself",
                        line.kind_name, line.field
                    )
                };
                self.model.default_notes.push(DefaultNote {
                    board: line.board,
                    source: line.source,
                    span: line.span,
                    message,
                });
            }
        }
    }
}

fn item_span(item: &Item) -> Span {
    match item {
        Item::Decl(d) => d.span,
        Item::Constraint(c) => c.span,
        Item::Connect(c) => c.span,
        Item::JoinMap(j) => j.span,
        Item::Bridge(b) => b.span,
        Item::Dc(d) => d.span,
        Item::Catalog(c) => c.span,
        Item::Pins(p) => p.span,
        Item::Tie(t) => t.span,
        Item::Unused(u) => u.span,
        Item::Peripheral(p) => p.span,
        Item::Shared(s) => s.span,
        Item::Lines(l) => l.span,
        Item::Case(c) => c.span,
        Item::Support(s) => s.span,
        Item::Defaults(d) => d.span,
        Item::Row(r) => r.span,
        Item::Polygon(p) => p.span,
    }
}
