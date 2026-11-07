//! `pins` tables and `tie` statements in a part (ADR 0016).
//!
//! A row of a table names a pin of the part. A plain pin (`.sense`) is declared by the first
//! row that names it, in whichever table; a pin of an interface (`.vout.pos`) is the one the
//! interface already has. The rows are kept per table, because the pads and the functions of
//! a pin depend on the package. What a pin is and what it is called do not, so the tables
//! must agree on them.

use netsmith_lang::Span;
use netsmith_lang::ast::{self, PinKind, Requirement};

use crate::checker::{Checker, Res};
use crate::model::{InstId, InstKind, Member, PinFunction, PinRow, PinTable, Tie};

impl Checker<'_> {
    /// First pass: make the plain pins that the rows of a table name.
    pub(crate) fn declare_table_pins(&mut self, table: &ast::Pins) {
        let cur = self.current();
        if self.model.instance(cur).kind != InstKind::Part {
            return;
        }
        for row in &table.rows {
            let path = &row.pin;
            if !(path.leading_dot && path.segments.len() == 1) {
                continue;
            }
            let name = path.segments[0].name.clone();
            match self.model.member(cur, &name) {
                None => {
                    let pin = self.add_pin_for_table(cur, &name, path.span);
                    self.table_pins.insert(pin);
                }
                Some(Member::Inst(pin)) if self.table_pins.contains(&pin) => {}
                Some(_) => self.error(
                    path.span,
                    format!("`{name}` is already declared in this block"),
                ),
            }
        }
    }

    fn add_pin_for_table(&mut self, part: InstId, name: &str, span: Span) -> InstId {
        let pin = self.add_pin(part, name, span, Vec::new());
        self.model.add_member(part, name, Member::Inst(pin));
        pin
    }

    /// The pin a row or a tie names, which must be a pin of the part being defined.
    pub(crate) fn part_pin(&mut self, path: &ast::Path) -> Option<InstId> {
        let part = self.current();
        match self.resolve_path(path) {
            Res::Inst(i) => {
                let inst = self.model.instance(i);
                match inst.kind {
                    InstKind::Pin if self.model.owner_of(i) == Some(part) => Some(i),
                    InstKind::Interface => {
                        let ty = inst.type_name.clone();
                        let first = inst.children.iter().find_map(|(n, m)| match m {
                            Member::Inst(c) if self.model.instance(*c).kind == InstKind::Pin => {
                                Some(n.clone())
                            }
                            _ => None,
                        });
                        let hint = first
                            .map(|n| format!(", such as `{}.{n}`", path.to_source()))
                            .unwrap_or_default();
                        self.error(
                            path.span,
                            format!(
                                "`{}` is a `{ty}`, not a pin; name one of its pins{hint}",
                                path.to_source()
                            ),
                        );
                        None
                    }
                    _ => {
                        self.error(
                            path.span,
                            format!("`{}` is not a pin of this part", path.to_source()),
                        );
                        None
                    }
                }
            }
            Res::Field(_) => {
                self.error(
                    path.span,
                    format!("`{}` is a value, not a pin", path.to_source()),
                );
                None
            }
            Res::Poison | Res::Bad => None,
        }
    }

    /// A generic's `pins` table whose rows have no name and no pads (ADR 0059): what a
    /// provider's own rows must agree with. It is not a table of the instance, which has no
    /// pads to give.
    fn contract_table(&mut self, table: &ast::Pins) {
        let part = self.current();
        let generic = self
            .cur_def
            .map(|(m, i)| self.modules[m].defs[i].name.name.clone())
            .unwrap_or_default();
        for row in &table.rows {
            let Some(pin) = self.part_pin(&row.pin) else {
                continue;
            };
            if self
                .contract_rows
                .iter()
                .any(|r| r.inst == part && r.pin == pin && r.generic == generic)
            {
                self.error(
                    row.pin.span,
                    format!(
                        "`{}` is listed twice in this pin table; one row gives its kind",
                        row.pin.to_source()
                    ),
                );
                continue;
            }
            let Some(built) = self.table_row(pin, row) else {
                continue;
            };
            self.contract_rows.push(crate::checker::ContractRow {
                inst: part,
                pin,
                kind: row.kind,
                requirement: built.requirement,
                generic: generic.clone(),
            });
        }
    }

    pub fn pins(&mut self, table: &ast::Pins) {
        let part = self.current();
        if self.model.instance(part).kind != InstKind::Part {
            self.error(table.span, "`pins` can only appear directly inside a part");
            return;
        }
        if !table.rows.is_empty() && table.rows.iter().all(|r| r.is_contract()) {
            if let Some((_, span)) = &table.package {
                self.error(
                    *span,
                    "a generic's `pins` table is the same for every package: leave the package out",
                );
            }
            self.contract_table(table);
            return;
        }
        let package = table.package.as_ref().map(|(p, _)| p.clone());

        // One unnamed table, or one table for each package.
        let existing = &self.model.instance(part).tables;
        let clash = match (&package, existing.first()) {
            (_, None) => None,
            (None, Some(_)) => Some(if existing.iter().any(|t| t.package.is_none()) {
                "this part already has a `pins` table for every package; two would give a pin two answers".to_string()
            } else {
                "this part has `pins` tables for packages; an unnamed table would apply to all of them, so give this one a package or remove the others".to_string()
            }),
            (Some(name), Some(_)) => {
                if existing.iter().any(|t| t.package.is_none()) {
                    Some("this part has a `pins` table for every package; a table for one package would give its pins a second answer, so remove one of them".to_string())
                } else if existing
                    .iter()
                    .any(|t| t.package.as_deref() == Some(name.as_str()))
                {
                    Some(format!(
                        "this part already has a `pins` table for package `{name}`"
                    ))
                } else {
                    None
                }
            }
        };
        if let Some(message) = clash {
            let at = table.package.as_ref().map_or(table.span, |(_, s)| *s);
            self.error(at, message);
            return;
        }
        if let Some((name, span)) = &table.package {
            match self.model.member(part, "package") {
                Some(Member::Field(f))
                    if self.model.field(f).kind == crate::model::FieldKind::Text => {}
                _ => self.error(
                    *span,
                    format!(
                        "`pins \"{name}\"` names a package, but this part has no `.package: Package` to hold it"
                    ),
                ),
            }
        }

        let source = self.cur_source();
        let mut rows: Vec<PinRow> = Vec::new();
        for row in &table.rows {
            if row.is_contract() {
                self.error(
                    row.span,
                    format!(
                        "`{}` has no datasheet name and no pad, and the other rows of this table have; a generic's rows with no name and no pads go in a table of their own",
                        row.pin.to_source()
                    ),
                );
                continue;
            }
            let Some(pin) = self.part_pin(&row.pin) else {
                continue;
            };
            if rows.iter().any(|r| r.pin == pin) {
                self.error(
                    row.pin.span,
                    format!(
                        "`{}` is listed twice in this pin table; one row gives all of its pads",
                        row.pin.to_source()
                    ),
                );
                continue;
            }
            let Some(built) = self.table_row(pin, row) else {
                continue;
            };
            self.agree_with_other_tables(part, &package, &built, row);
            rows.push(built);
        }
        self.model.instances[part.0].tables.push(PinTable {
            package,
            rows,
            source,
            span: table.span,
        });
    }

    /// The row as the model keeps it, if it makes sense by itself.
    fn table_row(&mut self, pin: InstId, row: &ast::PinRow) -> Option<PinRow> {
        let text = row.pin.to_source();
        let mut ok = true;
        let mut seen: Vec<&str> = Vec::new();
        for pad in &row.pads {
            if seen.contains(&pad.text.as_str()) {
                self.error(
                    pad.span,
                    format!("pad \"{}\" is listed twice for `{text}`", pad.text),
                );
                ok = false;
            }
            seen.push(&pad.text);
        }
        let mut seen: Vec<&str> = Vec::new();
        for f in &row.functions {
            if seen.contains(&f.name.name.as_str()) {
                self.error(
                    f.name.span,
                    format!("function `{}` is listed twice for `{text}`", f.name.name),
                );
                ok = false;
            }
            seen.push(&f.name.name);
        }
        if let Some((requirement, span)) = row.requirement {
            let useless = match (requirement, row.kind) {
                (_, PinKind::NoConnect) => Some(
                    "a no_connect pin must stay unconnected, so it is neither optional nor required"
                        .to_string(),
                ),
                (Requirement::Optional, PinKind::Power | PinKind::Input) => None,
                (Requirement::Optional, kind) => Some(format!(
                    "`optional` is for a power or input pin, which must be connected unless it says so; `{text}` is {} and may float already",
                    article(kind)
                )),
                (Requirement::Required, PinKind::Output | PinKind::Bidirectional | PinKind::Passive) => None,
                (Requirement::Required, kind) => Some(format!(
                    "`required` is for an output, bidirectional or passive pin, which may float unless it says so; `{text}` is {} and must be connected already",
                    article(kind)
                )),
            };
            if let Some(message) = useless {
                self.error(span, message);
                ok = false;
            }
        }
        ok.then(|| PinRow {
            pin,
            kind: row.kind,
            name: row.name.clone(),
            pads: row.pads.iter().map(|p| p.text.clone()).collect(),
            functions: row
                .functions
                .iter()
                .map(|f| PinFunction {
                    name: f.name.name.clone(),
                    kind: f.kind.map(|(k, _)| k),
                })
                .collect(),
            requirement: row.requirement.map(|(r, _)| r),
            span: row.span,
        })
    }

    /// A pin has one kind, one datasheet name and one `optional` or `required` in every
    /// package. The pads and the functions may differ.
    fn agree_with_other_tables(
        &mut self,
        part: InstId,
        package: &Option<String>,
        row: &PinRow,
        written: &ast::PinRow,
    ) {
        let describe = |r: &PinRow| {
            let requirement = r
                .requirement
                .map(|q| format!(" ({})", q.word()))
                .unwrap_or_default();
            format!("{} named `{}`{requirement}", article(r.kind), r.name)
        };
        let earlier = self
            .model
            .instance(part)
            .tables
            .iter()
            .find_map(|t| t.row(row.pin).map(|r| (t.describe(), r.clone())));
        let Some((table, other)) = earlier else {
            return;
        };
        if (other.kind, &other.name, other.requirement) == (row.kind, &row.name, row.requirement) {
            return;
        }
        let here = match package {
            Some(p) => format!("pins \"{p}\""),
            None => "pins".to_string(),
        };
        self.error(
            written.span,
            format!(
                "`{}` is {} in `{here}` but {} in `{table}`; a pin has one kind and one datasheet name in every package",
                written.pin.to_source(),
                describe(row),
                describe(&other),
            ),
        );
    }

    pub fn tie(&mut self, tie: &ast::Tie) {
        let part = self.current();
        if self.model.instance(part).kind != InstKind::Part {
            self.error(tie.span, "`tie` can only appear directly inside a part");
            return;
        }
        let (a, b) = (self.part_pin(&tie.a), self.part_pin(&tie.b));
        let (Some(a), Some(b)) = (a, b) else { return };
        if a == b {
            self.error(tie.span, "a tie needs two different pins");
            return;
        }
        let already = self
            .model
            .instance(part)
            .ties
            .iter()
            .any(|t| (t.a, t.b) == (a, b) || (t.a, t.b) == (b, a));
        if already {
            self.error(tie.span, "this tie is already stated");
            return;
        }
        let source = self.cur_source();
        self.model.instances[part.0].ties.push(Tie {
            a,
            b,
            source,
            span: tie.span,
        });
    }
}

/// `an input`, `a power pin`: a kind as it reads in a sentence.
pub(crate) fn article(kind: PinKind) -> &'static str {
    match kind {
        PinKind::Power => "a power pin",
        PinKind::Input => "an input",
        PinKind::Output => "an output",
        PinKind::Bidirectional => "a bidirectional pin",
        PinKind::Passive => "a passive pin",
        PinKind::NoConnect => "a no_connect pin",
    }
}
