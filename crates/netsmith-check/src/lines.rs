//! `lines` in a bus interface (ADR 0064): the roles of a bus and, for each role, the kind of
//! each line and a few properties of a line.
//!
//! The table belongs to the interface's definition: it is read once, when an instance of the
//! interface is made or when a peripheral names the interface (`as Spi SpiRole::Master`), and
//! its mistakes are reported then, so that the order of statements never matters. The rules
//! that use it, which read the nets of a board, are in `busnets.rs`.

use netsmith_lang::Span;
use netsmith_lang::ast::{self, DefKind, Item, PinKind};

use crate::checker::{Builtin, Checker, Ty};
use crate::lower::{VTy, list_of};
use crate::model::{BusDef, EnumId, LineRow, RExpr, RoleLines};

impl Checker<'_> {
    /// A `lines` statement: refused anywhere but directly inside an interface, and otherwise
    /// the definition's tables are read (once).
    pub(crate) fn lines(&mut self, l: &ast::Lines) {
        let cur = self.current();
        let inst = self.model.instance(cur);
        let direct = inst.kind == crate::model::InstKind::Interface && inst.def == self.cur_def;
        let Some(def_ref) = self.cur_def.filter(|_| direct) else {
            self.error(
                l.span,
                "`lines` can only appear directly inside an interface: it states the roles of a bus",
            );
            return;
        };
        self.read_bus_def(def_ref);
    }

    /// Read the `lines` of an interface definition into the model, if it has any and they are
    /// not read yet. The result is whether the interface has a table the rules can use.
    pub(crate) fn read_bus_def(&mut self, def_ref: (usize, usize)) -> bool {
        if self.model.bus_defs.contains_key(&def_ref) {
            return true;
        }
        if self.bus_def_failed.contains(&def_ref) {
            return false;
        }
        let def = self.modules[def_ref.0].defs[def_ref.1].clone();
        if def.kind != DefKind::Interface {
            return false;
        }
        let saved = self.enter_def(def_ref);
        let built = self.build_bus_def(&def);
        self.leave_def(saved);
        match built {
            Some(bus) => {
                self.model.bus_defs.insert(def_ref, bus);
                true
            }
            None => {
                self.bus_def_failed.insert(def_ref);
                false
            }
        }
    }

    fn build_bus_def(&mut self, def: &ast::Definition) -> Option<BusDef> {
        let source = self.cur_source();
        // The pins the interface declares, and its `.role` if it has one.
        let mut pins: Vec<String> = Vec::new();
        let mut role: Option<(EnumId, String)> = None;
        for item in &def.items {
            let Item::Decl(d) = item else { continue };
            if !(d.name.leading_dot && d.name.segments.len() == 1) {
                continue;
            }
            let name = d.name.segments[0].name.clone();
            match self.resolve_type(&d.ty) {
                Ty::Builtin(Builtin::Pin) => pins.push(name),
                Ty::Enum(m, i) if name == "role" => {
                    let id = self.enum_id(m, i);
                    let enum_name = self.model.enum_info(id).name.clone();
                    role = Some((id, enum_name));
                }
                _ => {}
            }
        }
        let mut ok = true;
        let mut tables: Vec<RoleLines> = Vec::new();
        for item in &def.items {
            let Item::Lines(l) = item else { continue };
            let Some(table) = self.lines_table(l, &pins, role.as_ref(), source) else {
                ok = false;
                continue;
            };
            if tables.iter().any(|t| t.role == table.role) {
                let what = match (&table.role, &role) {
                    (Some(variant), Some((_, enum_name))) => format!("`{enum_name}::{variant}`"),
                    _ => "this interface".to_string(),
                };
                self.error(
                    l.span,
                    format!(
                        "{what} already has a `lines` table; two would give a line two answers"
                    ),
                );
                ok = false;
                continue;
            }
            tables.push(table);
        }
        if tables.is_empty() {
            return None;
        }
        // A `crosses` names the other line, and that line, if it names one, names this one
        // back: `tx crosses rx` and `rx crosses tx` are the same pair, `rx crosses cts` is not.
        let mut partner: Vec<(&str, &str, Span)> = Vec::new();
        for table in &tables {
            for row in &table.rows {
                if let Some(other) = &row.crosses {
                    partner.push((&row.name, other, row.span));
                }
            }
        }
        let mut conflicts: Vec<(Span, String)> = Vec::new();
        for (a, b, span) in &partner {
            for (c, d, _) in &partner {
                // `a` crosses `b`: nothing else may cross `b` or `a` unless it is the same pair.
                let same = (*c == *a && *d == *b) || (*c == *b && *d == *a);
                let touches = *c == *a || *c == *b || *d == *a || *d == *b;
                if touches && !same {
                    conflicts.push((
                        *span,
                        format!(
                            "`.{a}` crosses `.{b}`, but `.{c}` crosses `.{d}`; a line crosses one other line, and the pair is written on one of the two"
                        ),
                    ));
                }
            }
        }
        for (span, message) in conflicts {
            self.error(span, message);
            ok = false;
        }
        if !ok {
            return None;
        }
        Some(BusDef {
            name: def.name.name.clone(),
            role_enum: role.map(|(_, name)| name),
            tables,
        })
    }

    /// One `lines` table, if it is sound.
    fn lines_table(
        &mut self,
        l: &ast::Lines,
        pins: &[String],
        role: Option<&(EnumId, String)>,
        source: crate::diag::SourceId,
    ) -> Option<RoleLines> {
        let mut ok = true;
        let variant = match (&l.role, role) {
            (Some(expr), Some((id, enum_name))) => {
                match self.lower(expr) {
                    Some(typed) => match (&typed.e, typed.ty) {
                        (RExpr::Text(v), VTy::Enum(found)) if found == *id => Some(v.clone()),
                        (_, VTy::Enum(found)) => {
                            let other = self.model.enum_info(found).name.clone();
                            self.error(
                                l.span,
                                format!(
                                    "`.role` of this interface is a `{enum_name}`, and `{other}` is another enum; name a variant of `{enum_name}`"
                                ),
                            );
                            ok = false;
                            None
                        }
                        _ => {
                            self.error(
                                l.span,
                                format!("the role of `lines` is a variant of `{enum_name}`"),
                            );
                            ok = false;
                            None
                        }
                    },
                    // The variant is not one of the enum's: `lower` said so.
                    None => {
                        ok = false;
                        None
                    }
                }
            }
            (Some(expr), None) => {
                self.error(
                    expr.span(),
                    "this interface has no `.role` to hold a role: declare `.role: SomeEnum`, or write `lines` with no role",
                );
                ok = false;
                None
            }
            (None, Some((_, enum_name))) => {
                self.error(
                    l.span,
                    format!(
                        "this interface has a `.role: {enum_name}`, so each `lines` names the role it is for, such as `lines {enum_name}::{}`",
                        self.first_variant(role)
                    ),
                );
                ok = false;
                None
            }
            (None, None) => None,
        };
        let mut rows: Vec<LineRow> = Vec::new();
        for r in &l.rows {
            let Some(name) = self.line_name(&r.pin, pins) else {
                ok = false;
                continue;
            };
            if rows.iter().any(|x| x.name == name) {
                self.error(
                    r.pin.span,
                    format!("`.{name}` is listed twice in this `lines` table"),
                );
                ok = false;
                continue;
            }
            if matches!(r.kind, PinKind::Power | PinKind::NoConnect) {
                self.error(
                    r.span,
                    format!(
                        "a line of a bus is a signal: `.{name}` is `input`, `output`, `tri_state`, `bidirectional` or `passive`, not `{}`",
                        r.kind.word()
                    ),
                );
                ok = false;
                continue;
            }
            let crosses = match &r.crosses {
                Some(other) => match self.line_name(other, pins) {
                    Some(o) if o == name => {
                        self.error(other.span, format!("`.{name}` cannot cross itself"));
                        ok = false;
                        None
                    }
                    Some(o) => Some(o),
                    None => {
                        ok = false;
                        None
                    }
                },
                None => None,
            };
            rows.push(LineRow {
                name,
                kind: r.kind,
                requirement: r.requirement.map(|(q, _)| q),
                single: r.single.is_some(),
                crosses,
                span: r.span,
            });
        }
        ok.then_some(RoleLines {
            role: variant,
            limit: l.limit.map(|(n, _)| n),
            rows,
            source,
            span: l.span,
        })
    }

    fn first_variant(&self, role: Option<&(EnumId, String)>) -> String {
        role.and_then(|(id, _)| self.model.enum_info(*id).variants.first().cloned())
            .unwrap_or_else(|| "Variant".to_string())
    }

    /// The name of the pin of this interface that a row of `lines` is about.
    fn line_name(&mut self, path: &ast::Path, pins: &[String]) -> Option<String> {
        if path.leading_dot && path.segments.len() == 1 {
            let name = &path.segments[0].name;
            if pins.contains(name) {
                return Some(name.clone());
            }
            let near = crate::lower::closest(name, pins)
                .map(|n| format!("; did you mean `.{n}`?"))
                .unwrap_or_default();
            let listed: Vec<String> = pins.iter().map(|p| format!("`.{p}`")).collect();
            self.error(
                path.span,
                format!(
                    "`{}` is not a pin of this interface; its pins are {}{near}",
                    path.to_source(),
                    list_of(&listed)
                ),
            );
            return None;
        }
        self.error(
            path.span,
            format!(
                "a line is a pin of the interface itself, such as `.sclk`; `{}` is not",
                path.to_source()
            ),
        );
        None
    }
}
