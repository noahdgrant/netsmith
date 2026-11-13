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
use crate::model::{InstId, InstKind, Member, PinFunction, PinRow, PinTable, RowLimit, Tie};

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

    /// A generic's `pins` table whose rows have no name and no pads (ADR 0068): what a
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
        // A generic's table whose rows are all contract rows, or that has none: a generic that
        // says "a part, whose pins are its own" (`Connector`) has an empty table.
        let in_generic = self
            .cur_def
            .is_some_and(|(m, i)| self.modules[m].defs[i].kind == ast::DefKind::Generic);
        if table.rows.iter().all(|r| r.is_contract()) && (!table.rows.is_empty() || in_generic) {
            if let Some((_, span)) = table.packages.first() {
                self.error(
                    *span,
                    "a generic's `pins` table is the same for every package: leave the package out",
                );
            }
            self.contract_table(table);
            return;
        }

        // The packages of the header, each once (ADR 0124): several share the table.
        let mut packages: Vec<(String, Span)> = Vec::new();
        for (name, span) in &table.packages {
            if packages.iter().any(|(p, _)| p == name) {
                self.error(
                    *span,
                    format!("package \"{name}\" is listed twice in this `pins` header"),
                );
                continue;
            }
            packages.push((name.clone(), *span));
        }
        let here = if packages.is_empty() {
            "pins".to_string()
        } else {
            let names: Vec<String> = packages.iter().map(|(p, _)| format!("\"{p}\"")).collect();
            format!("pins {}", names.join(" "))
        };

        // One unnamed table, or one table for each package.
        let existing = &self.model.instance(part).tables;
        let mut clashes: Vec<(Span, String)> = Vec::new();
        if !existing.is_empty() {
            let unnamed = existing.iter().any(|t| t.package.is_none());
            if packages.is_empty() {
                clashes.push((table.span, if unnamed {
                    "this part already has a `pins` table for every package; two would give a pin two answers".to_string()
                } else {
                    "this part has `pins` tables for packages; an unnamed table would apply to all of them, so give this one a package or remove the others".to_string()
                }));
            } else if unnamed {
                clashes.push((packages[0].1, "this part has a `pins` table for every package; a table for one package would give its pins a second answer, so remove one of them".to_string()));
            } else {
                for (name, span) in &packages {
                    if existing
                        .iter()
                        .any(|t| t.package.as_deref() == Some(name.as_str()))
                    {
                        clashes.push((
                            *span,
                            format!("this part already has a `pins` table for package `{name}`"),
                        ));
                    }
                }
            }
        }
        if !clashes.is_empty() {
            for (at, message) in clashes {
                self.error(at, message);
            }
            return;
        }
        for (name, span) in &packages {
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
        // The rows whose `abs_max` takes the table's ground, found once every row is read.
        let mut grounded: Vec<(usize, Span)> = Vec::new();
        let mut written: Vec<String> = Vec::new();
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
            let Some(mut built) = self.table_row(pin, row) else {
                continue;
            };
            if let Some(limit) = &row.abs_max {
                match self.row_limit(pin, row, limit) {
                    Some((built_limit, needs_ground)) => {
                        if needs_ground {
                            grounded.push((rows.len(), limit.span));
                        }
                        built.abs_max = Some(built_limit);
                    }
                    None => continue,
                }
            }
            self.agree_with_other_tables(part, &here, &built, row);
            rows.push(built);
            written.push(row.pin.to_source());
        }
        if !grounded.is_empty() {
            let mut grounds: Vec<InstId> = rows
                .iter()
                .map(|r| r.pin)
                .filter(|&p| self.model.is_rail_ground(p))
                .collect();
            grounds.dedup();
            if let [ground] = grounds[..] {
                for (i, _) in &grounded {
                    if let Some(limit) = &mut rows[*i].abs_max {
                        limit.reference = Some(ground);
                    }
                }
            } else {
                let found = if grounds.is_empty() {
                    "this table has no rail's `neg` to be it".to_string()
                } else {
                    let names: Vec<String> = grounds
                        .iter()
                        .filter_map(|&g| rows.iter().position(|r| r.pin == g))
                        .map(|i| format!("`{}`", written[i]))
                        .collect();
                    format!("this table has several, {}", names.join(" and "))
                };
                for (i, span) in grounded {
                    let pin = &written[i];
                    self.error(
                        span,
                        format!(
                            "the `abs_max` of `{pin}` is measured from the part's ground, the `neg` of its rail, and {found}; name the pin it is measured from, such as `abs_max 65V from .gnd`"
                        ),
                    );
                }
            }
        }
        if packages.is_empty() {
            self.model.instances[part.0].tables.push(PinTable {
                package: None,
                rows,
                source,
                span: table.span,
            });
        } else {
            for (name, _) in packages {
                self.model.instances[part.0].tables.push(PinTable {
                    package: Some(name),
                    rows: rows.clone(),
                    source,
                    span: table.span,
                });
            }
        }
    }

    /// The `abs_max` at the end of a row (ADR 0124): the limit as the model keeps it, and
    /// whether it is measured from the table's rail ground, which the caller finds once every
    /// row is read. The value narrows the pin's `.abs_max`, once for the part, however many
    /// tables state it.
    fn row_limit(
        &mut self,
        pin: InstId,
        row: &ast::PinRow,
        limit: &ast::AbsMax,
    ) -> Option<(RowLimit, bool)> {
        let text = row.pin.to_source();
        let by_levels = self.model.grounded_by_levels(pin);
        let (reference, needs_ground) = match &limit.from {
            Some(from) if by_levels => {
                self.error(
                    from.span,
                    format!(
                        "`{text}` is measured from the ground its `Logic` or its bus's `levels` give, the `neg` of their reference rail; leave out `from`"
                    ),
                );
                return None;
            }
            Some(from) => {
                let reference = self.part_pin(from)?;
                if reference == pin {
                    self.error(
                        from.span,
                        format!("`{text}` cannot be measured from itself; name its ground"),
                    );
                    return None;
                }
                (Some(reference), false)
            }
            None => (None, !by_levels),
        };
        let mut written = limit.value.to_source();
        if let Some(from) = &limit.from {
            written = format!("{written} from {}", from.to_source());
        }
        let part = self.current();
        let stated = self
            .model
            .instance(part)
            .tables
            .iter()
            .any(|t| t.row(pin).is_some_and(|r| r.abs_max.is_some()));
        if !stated {
            let mut path = row.pin.clone();
            path.segments.push(ast::Ident {
                name: "abs_max".to_string(),
                quoted: false,
                span: limit.span,
            });
            path.span = Span::new(row.pin.span.start, limit.span.end);
            self.constraint(&ast::Constraint {
                lhs: ast::Expr::Path(path),
                op: ast::CmpOp::Narrow,
                rhs: limit.value.clone(),
                at: None,
                span: Span::new(limit.span.start, limit.value.span().end),
            });
        }
        Some((
            RowLimit {
                text: written,
                reference,
            },
            needs_ground,
        ))
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
                (
                    Requirement::Required,
                    PinKind::Output
                    | PinKind::TriState
                    | PinKind::OpenDrain
                    | PinKind::Bidirectional
                    | PinKind::Passive,
                ) => None,
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
                    class: f.class.is_some(),
                })
                .collect(),
            requirement: row.requirement.map(|(r, _)| r),
            abs_max: None,
            span: row.span,
        })
    }

    /// A pin has one kind, one datasheet name and one `optional` or `required` in every
    /// package. The pads and the functions may differ.
    fn agree_with_other_tables(
        &mut self,
        part: InstId,
        here: &str,
        row: &PinRow,
        written: &ast::PinRow,
    ) {
        let describe = |r: &PinRow| {
            let requirement = r
                .requirement
                .map(|q| format!(" ({})", q.word()))
                .unwrap_or_default();
            let limit = r
                .abs_max
                .as_ref()
                .map(|l| format!(" with `abs_max {}`", l.text))
                .unwrap_or_default();
            format!("{} named `{}`{requirement}{limit}", article(r.kind), r.name)
        };
        let limit = |r: &PinRow| r.abs_max.as_ref().map(|l| l.text.clone());
        let earlier = self
            .model
            .instance(part)
            .tables
            .iter()
            .find_map(|t| t.row(row.pin).map(|r| (t.describe(), r.clone())));
        let Some((table, other)) = earlier else {
            return;
        };
        if (other.kind, &other.name, other.requirement, limit(&other))
            == (row.kind, &row.name, row.requirement, limit(row))
        {
            return;
        }
        self.error(
            written.span,
            format!(
                "`{}` is {} in `{here}` but {} in `{table}`; a pin has one kind, one datasheet name and one absolute maximum in every package",
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
        PinKind::TriState => "a tri_state output",
        PinKind::OpenDrain => "an open_drain output",
    }
}
