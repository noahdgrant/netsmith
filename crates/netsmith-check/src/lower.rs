//! Turning syntax into checked expressions, and applying constraints.
//!
//! A constraint whose right-hand side is a constant narrows its field right away. That
//! is order independent, because intersecting is. Anything that mentions another field
//! is recorded as a [`Relation`] for the solver instead of being evaluated here, so the
//! meaning of a block never depends on the order of its statements.
//!
//! A default (`?=`) waits until every statement of the board has been read. It then becomes
//! the same narrowing as `&=` for each field that no `&=` statement states, and is dropped
//! for the rest (ADR 0051).

use netsmith_lang::Span;
use netsmith_lang::ast::{self, BinOp, CmpOp, DefKind, Expr};
use netsmith_quantity::{Dimension, Quantity, is_offset_unit, parse_unit};

use crate::checker::{Checker, Res, Ty};
use crate::diag::SourceId;
use crate::model::{
    Domain, EnumId, FieldId, FieldKind, InstId, InstKind, Member, RExpr, RangeEnd, Relation,
};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum VTy {
    Q(Dimension),
    Text,
    /// A variant of an enum (ADR 0054).
    Enum(EnumId),
}

pub(crate) struct Typed {
    pub e: RExpr,
    pub ty: VTy,
}

/// A `?=` statement that has been checked and waits to be resolved.
pub(crate) struct PendingDefault {
    field: FieldId,
    value: Typed,
    source: SourceId,
    span: Span,
}

/// What decides whether a net default applies, by class of pins joined with `~`.
#[derive(Default)]
pub(crate) struct NetClasses {
    stated: std::collections::HashSet<usize>,
    terminal: std::collections::HashSet<usize>,
    joined: std::collections::HashSet<usize>,
}

impl Checker<'_> {
    /// What a value of this type is called in a message.
    fn describe(&self, ty: VTy) -> String {
        match ty {
            VTy::Q(d) => d.to_string(),
            VTy::Text => "text".to_string(),
            VTy::Enum(e) => format!("a variant of `{}`", self.model.enum_info(e).name),
        }
    }

    /// What a field holds, for a message.
    fn describe_kind(&self, kind: FieldKind) -> String {
        match kind {
            FieldKind::Quantity(d) => d.to_string(),
            FieldKind::Text => "text".to_string(),
            FieldKind::Enum(e) => format!("a variant of `{}`", self.model.enum_info(e).name),
        }
    }

    /// `AssemblyMode::Hand`: the type of the enum and the variant, if the enum has it.
    fn lower_variant(
        &mut self,
        ty: &ast::TypePath,
        variant: &ast::Ident,
        span: Span,
    ) -> Option<Typed> {
        let (m, i) = match self.resolve_type(ty) {
            Ty::Enum(m, i) => (m, i),
            Ty::Poison | Ty::Bad => return None,
            Ty::Def(m, i) | Ty::Param { arg: (m, i), .. } => {
                let kind = self.module_def_kind(m, i);
                self.error(
                    span,
                    format!(
                        "`{}` is a {kind}, not an enum, so `{}::{}` is not a value",
                        ty.segments.last()?.name,
                        ty.segments.last()?.name,
                        variant.name
                    ),
                );
                return None;
            }
            Ty::Builtin(_) => {
                self.error(
                    span,
                    format!(
                        "`{}` is not an enum, so `{}::{}` is not a value",
                        ty.segments.last()?.name,
                        ty.segments.last()?.name,
                        variant.name
                    ),
                );
                return None;
            }
        };
        let id = self.enum_id(m, i);
        let info = self.model.enum_info(id).clone();
        if !info.variants.contains(&variant.name) {
            let listed: Vec<String> = info
                .variants
                .iter()
                .map(|v| format!("`{}::{v}`", info.name))
                .collect();
            let hint = closest(&variant.name, &info.variants)
                .map(|v| format!("; did you mean `{}::{v}`?", info.name))
                .unwrap_or_default();
            self.error(
                variant.span,
                format!(
                    "`{}` has no variant `{}`; its variants are {}{hint}",
                    info.name,
                    variant.name,
                    list_of(&listed)
                ),
            );
            return None;
        }
        Some(Typed {
            e: RExpr::Text(variant.name.clone()),
            ty: VTy::Enum(id),
        })
    }

    /// A text that a field of an enum may not hold, said with the variant it was meant to be.
    fn text_for_enum(&self, e: EnumId, text: &str) -> String {
        let info = self.model.enum_info(e);
        let listed: Vec<String> = info
            .variants
            .iter()
            .map(|v| format!("`{}::{v}`", info.name))
            .collect();
        let found = info
            .variants
            .iter()
            .find(|v| v.eq_ignore_ascii_case(text))
            .or_else(|| {
                closest(text, &info.variants).and_then(|c| info.variants.iter().find(|v| *v == &c))
            });
        match found {
            Some(v) => format!(
                "a variant of `{}` is not written as text: write `{}::{v}` (the variants are {})",
                info.name,
                info.name,
                list_of(&listed)
            ),
            None => format!(
                "a variant of `{}` is not written as text, and \"{text}\" is none of them: the variants are {}",
                info.name,
                list_of(&listed)
            ),
        }
    }

    /// The right side of a statement whose other side is `beside`: a bare name (`USART1_TX`)
    /// is a function when `beside` is the `function` of a pin (ADR 0118); a function is a name
    /// of the part's pin table, which no import brings in. Anything else is lowered as it
    /// always is.
    fn lower_beside(&mut self, beside: &Typed, e: &Expr) -> Option<Typed> {
        let function = matches!(beside.e, RExpr::Field(f) if self.model.is_pin_function(f));
        if function {
            if let Some(name) = bare_name(e) {
                return Some(Typed {
                    e: RExpr::Text(name.name.clone()),
                    ty: VTy::Text,
                });
            }
            if let Expr::Str(text, span) = e {
                let RExpr::Field(f) = beside.e else {
                    unreachable!("a function is a field")
                };
                let pin = self.model.path_of(self.model.field(f).owner);
                let written = if is_plain_name(text) {
                    format!("write `{text}`, without quotes")
                } else {
                    "write the name as its `fn` list has it, without quotes".to_string()
                };
                self.error(
                    *span,
                    format!(
                        "the function of `{pin}` is a name from the pin table of its part, not text: {written}"
                    ),
                );
                // The pin is still read as having that function, so the refusal is the one
                // message about the line.
                return Some(Typed {
                    e: RExpr::Text(text.clone()),
                    ty: VTy::Text,
                });
            }
        }
        // A bare name beside a field of an enum that is not in scope: say how the variant is
        // written, by its enum or by importing it (ADR 0118).
        if let VTy::Enum(id) = beside.ty
            && let Some(name) = bare_name(e)
            && self.imported_variant(&name.name).is_none()
            && !self.outer_member(&name.name)
            && let Some((m, i)) = self.enum_def(id)
        {
            let info = self.model.enum_info(id).clone();
            let variant = closest(&name.name, &info.variants);
            let message = match variant {
                Some(v) => format!(
                    "`{}` is not defined here; a variant of `{}` is written {}",
                    name.name,
                    info.name,
                    self.variant_spellings(m, i, &v)
                ),
                None => {
                    let listed: Vec<String> =
                        info.variants.iter().map(|v| format!("`{v}`")).collect();
                    format!(
                        "`{}` is not defined here, and `{}` has no such variant: its variants are {}",
                        name.name,
                        info.name,
                        list_of(&listed)
                    )
                }
            };
            self.error(name.span, message);
            return None;
        }
        self.lower(e)
    }

    /// Whether a bare name is a member of an enclosing block.
    fn outer_member(&self, name: &str) -> bool {
        let outer = self.scope.len().saturating_sub(1);
        self.scope[..outer]
            .iter()
            .any(|&s| self.model.member(s, name).is_some())
    }

    /// The definition of an enum the model has an id for.
    fn enum_def(&self, id: EnumId) -> Option<(usize, usize)> {
        self.enum_ids
            .iter()
            .find(|(_, e)| **e == id)
            .map(|(&def, _)| def)
    }

    fn module_def_kind(&self, module: usize, index: usize) -> &'static str {
        crate::providers::kind_word(&self.module_defs(module)[index])
    }

    /// A field as a value of its own kind.
    fn typed_field(&self, f: FieldId) -> Typed {
        let ty = match self.model.field(f).kind {
            FieldKind::Quantity(d) => VTy::Q(d),
            FieldKind::Text => VTy::Text,
            FieldKind::Enum(e) => VTy::Enum(e),
        };
        Typed {
            e: RExpr::Field(f),
            ty,
        }
    }

    pub fn lower(&mut self, e: &Expr) -> Option<Typed> {
        match e {
            Expr::Number(n, span) => match Quantity::literal(n.value, n.unit.as_deref()) {
                Ok(lit) => Some(Typed {
                    ty: VTy::Q(lit.quantity.dim()),
                    e: RExpr::Const(lit.quantity),
                }),
                Err(err) => {
                    self.error(*span, err.to_string());
                    None
                }
            },
            Expr::Str(s, _) => Some(Typed {
                e: RExpr::Text(s.clone()),
                ty: VTy::Text,
            }),
            Expr::Variant { ty, variant, span } => self.lower_variant(ty, variant, *span),
            // A variant imported by name (`import std::parts::passive::Color::Red`, ADR 0118).
            Expr::Path(_)
                if let Some(name) = bare_name(e)
                    && let Some((m, i, variant)) = self.imported_variant(&name.name) =>
            {
                let id = self.enum_id(m, i);
                Some(Typed {
                    e: RExpr::Text(variant),
                    ty: VTy::Enum(id),
                })
            }
            Expr::Path(p) => match self.resolve_path(p) {
                Res::Field(f) => Some(self.typed_field(f)),
                Res::Inst(i) => {
                    let word = self.model.instance(i).kind.word();
                    self.error(
                        p.span,
                        format!("expected a value, but `{}` is a {word}", p.to_source()),
                    );
                    None
                }
                Res::Poison | Res::Bad => None,
            },
            Expr::Paren(inner, _) => self.lower(inner),
            Expr::Call { name, args, span } => self.lower_call(name, args, *span),
            // A sign written on a literal in an offset unit is the literal's: `-40degC` is
            // 233.15 K, not minus 313.15 K.
            Expr::Neg(inner, span) if matches!(&**inner, Expr::Number(n, _) if n.unit.as_deref().is_some_and(is_offset_unit)) =>
            {
                let Expr::Number(n, _) = &**inner else {
                    unreachable!("the guard matched a number")
                };
                match Quantity::literal(-n.value, n.unit.as_deref()) {
                    Ok(lit) => Some(Typed {
                        ty: VTy::Q(lit.quantity.dim()),
                        e: RExpr::Const(lit.quantity),
                    }),
                    Err(err) => {
                        self.error(*span, err.to_string());
                        None
                    }
                }
            }
            Expr::Neg(inner, span) => {
                let t = self.lower(inner)?;
                let VTy::Q(_) = t.ty else {
                    self.error(*span, "cannot negate text");
                    return None;
                };
                let e = match t.e {
                    RExpr::Const(q) => RExpr::Const(q.neg()),
                    other => RExpr::Neg(Box::new(other)),
                };
                Some(Typed { e, ty: t.ty })
            }
            Expr::Binary { op, lhs, rhs, span } => {
                let l = self.lower(lhs);
                let r = self.lower(rhs);
                self.binary(*op, l?, r?, *span)
            }
            Expr::Range { lo, hi, span } => {
                let l = self.lower(lo);
                let h = self.lower(hi);
                let (l, h) = (l?, h?);
                match (&l.e, &h.e) {
                    (RExpr::Const(a), RExpr::Const(b)) => match Quantity::range(a, b) {
                        Ok(q) => Some(Typed {
                            ty: VTy::Q(q.dim()),
                            e: RExpr::Const(q),
                        }),
                        Err(err) => {
                            self.error(*span, err.to_string());
                            None
                        }
                    },
                    _ => {
                        // An end that is a value (a member, an expression over members) is
                        // read when the solver knows it (ADR 0066). The ends are numbers of
                        // one dimension, and an end text is not one.
                        let (VTy::Q(ld), VTy::Q(hd)) = (l.ty, h.ty) else {
                            self.error(*span, "the ends of a range are numbers, not text");
                            return None;
                        };
                        if ld != hd {
                            self.error(*span, format!("cannot form a range from {ld} and {hd}"));
                            return None;
                        }
                        Some(Typed {
                            ty: VTy::Q(ld),
                            e: RExpr::Range(Box::new(l.e), Box::new(h.e)),
                        })
                    }
                }
            }
            Expr::Tolerance { center, tol, span } => {
                let c = self.lower(center);
                let t = self.lower(tol);
                let (c, t) = (c?, t?);
                if matches!(&**tol, Expr::Number(n, _) if n.unit.as_deref().is_some_and(is_offset_unit))
                {
                    self.error(
                        *span,
                        "a tolerance is a difference of temperatures: write it in kelvin (`20degC +/- 5K`), not in degC",
                    );
                    return None;
                }
                let relative = matches!(
                    &**tol,
                    Expr::Number(n, _)
                        if n.unit.as_deref().and_then(|u| parse_unit(u).ok()).is_some_and(|u| u.relative)
                );
                match (&c.e, &t.e) {
                    (RExpr::Const(a), RExpr::Const(b)) => match Quantity::tolerance(a, b, relative)
                    {
                        Ok(q) => Some(Typed {
                            ty: VTy::Q(q.dim()),
                            e: RExpr::Const(q),
                        }),
                        Err(err) => {
                            self.error(*span, err.to_string());
                            None
                        }
                    },
                    _ => {
                        self.error(
                            *span,
                            "a tolerance needs a fixed value and a fixed tolerance",
                        );
                        None
                    }
                }
            }
        }
    }

    /// `min(.f)` and `max(.f)` (ADR 0098), the least and the greatest value of the range of the
    /// field `.f`, and `nominal(.f)`, its centre, each as one value of the field's own kind.
    /// They are the only functions.
    fn lower_call(&mut self, name: &ast::Ident, args: &[Expr], span: Span) -> Option<Typed> {
        let word = name.name.as_str();
        let end = match word {
            "min" => RangeEnd::Min,
            "max" => RangeEnd::Max,
            "nominal" => RangeEnd::Nominal,
            _ => {
                self.error(
                    name.span,
                    format!(
                        "`{word}` is not a function; the functions are `min(.field)` and `max(.field)`, the least and the greatest value of a field's range, and `nominal(.field)`, its centre"
                    ),
                );
                return None;
            }
        };
        let [arg] = args else {
            let count = match args.len() {
                0 => "no argument".to_string(),
                n => format!("{n} arguments"),
            };
            let message = match end {
                RangeEnd::Nominal => format!(
                    "`{word}` takes one field, such as `{word}(.resistance)`, but this has {count}: it reads the value one field is stated at, the centre of its range"
                ),
                _ => format!(
                    "`{word}` takes one field, such as `{word}(.bias)`, but this has {count}: it reads the {} value of one field's range, and is not the {} of several values",
                    if end == RangeEnd::Min {
                        "least"
                    } else {
                        "greatest"
                    },
                    if end == RangeEnd::Min {
                        "smaller"
                    } else {
                        "larger"
                    },
                ),
            };
            self.error(span, message);
            return None;
        };
        if !matches!(arg, Expr::Path(_)) {
            let what = if end == RangeEnd::Nominal {
                "the centre of a field's range"
            } else {
                "one end of a field's range"
            };
            self.error(
                arg.span(),
                format!(
                    "the argument of `{word}` is a field, such as `{word}(.bias)`, not `{}`: it reads {what}",
                    arg.to_source()
                ),
            );
            return None;
        }
        if !self.range_ends {
            let what = if end == RangeEnd::Nominal {
                "the centre of a range"
            } else {
                "one end of a range"
            };
            self.error(
                span,
                format!(
                    "`{}` can only be used in a `<=` or `>=` statement: it reads {what}, and nothing is worked out from it",
                    Expr::Call { name: name.clone(), args: args.to_vec(), span }.to_source()
                ),
            );
            return None;
        }
        let t = self.lower(arg)?;
        let (RExpr::Field(f), VTy::Q(dim)) = (&t.e, t.ty) else {
            let what = self.describe(t.ty);
            self.error(
                arg.span(),
                format!(
                    "`{word}` reads the {} of a number's range, but `{}` is {what}",
                    if end == RangeEnd::Nominal {
                        "centre"
                    } else {
                        "end"
                    },
                    arg.to_source()
                ),
            );
            return None;
        };
        Some(Typed {
            e: RExpr::End(end, *f),
            ty: VTy::Q(dim),
        })
    }

    fn binary(&mut self, op: BinOp, l: Typed, r: Typed, span: Span) -> Option<Typed> {
        let (VTy::Q(ld), VTy::Q(rd)) = (l.ty, r.ty) else {
            self.error(span, "arithmetic needs numbers, not text");
            return None;
        };
        let dim = match op {
            BinOp::Add | BinOp::Sub => {
                if ld != rd {
                    let verb = if op == BinOp::Add { "add" } else { "subtract" };
                    self.error(span, format!("cannot {verb} {ld} and {rd}"));
                    return None;
                }
                ld
            }
            BinOp::Mul => ld * rd,
            BinOp::Div => ld / rd,
        };
        let e = match (l.e, r.e) {
            (RExpr::Const(a), RExpr::Const(b)) => {
                let result = match op {
                    BinOp::Add => a.add(&b),
                    BinOp::Sub => a.sub(&b),
                    BinOp::Mul => a.mul(&b),
                    BinOp::Div => a.div(&b),
                };
                match result {
                    Ok(q) => RExpr::Const(q),
                    Err(err) => {
                        self.error(span, err.to_string());
                        return None;
                    }
                }
            }
            (a, b) => {
                if let (BinOp::Div, RExpr::Const(d)) = (op, &b)
                    && d.lo() <= 0.0
                    && d.hi() >= 0.0
                {
                    self.error(span, "the divisor's range includes zero");
                    return None;
                }
                RExpr::Bin(op, Box::new(a), Box::new(b))
            }
        };
        Some(Typed { e, ty: VTy::Q(dim) })
    }

    pub fn constraint(&mut self, c: &ast::Constraint) {
        let field = match c.op {
            CmpOp::Narrow | CmpOp::Default => self.narrow_statement(c),
            _ => self.relation_statement(c),
        };
        // `at 2mA` changes nothing of what the statement means; it is checked and recorded
        // beside it (ADR 0076, amendment 1).
        if let Some(at) = &c.at {
            self.level_test(c, at, field);
        }
    }

    /// The test current of a level: `.voh &= .vdd.voltage - 0.4V at 2mA`. `field` is the field
    /// the statement states, when the statement could be read.
    fn level_test(&mut self, c: &ast::Constraint, at: &Expr, field: Option<FieldId>) {
        let Some(level) = field else {
            if !matches!(c.lhs, Expr::Path(_)) {
                self.error(
                    c.lhs.span(),
                    "`at` needs a level on the left of the statement, such as `.voh &= .vdd.voltage - 0.4V at 2mA`",
                );
            }
            return;
        };
        let f = self.model.field(level);
        let name = f.name.clone();
        // The level as the definition that states it names it, so that the message reads the
        // same when the part is checked alone and when a board has made one.
        let path = self.model.field_in_scope(level);
        if !self.model.is_level(level) || !matches!(name.as_str(), "voh" | "vol") {
            let why = if self.model.is_level(level) {
                format!(
                    "`{path}` is a level that a datasheet states without a load: only an output level, `voh` or `vol`, is measured at a current"
                )
            } else {
                format!(
                    "`{path}` is not an output level (`voh` or `vol` of a `Logic` or of `Levels`)"
                )
            };
            self.error(
                at.span(),
                format!("`at <current>` states the load of an output level; {why}"),
            );
            return;
        }
        let Some(t) = self.lower(at) else { return };
        let current = match (t.ty, t.e) {
            (VTy::Q(d), RExpr::Const(q)) if d == Dimension::CURRENT => q,
            (VTy::Q(d), _) if d != Dimension::CURRENT => {
                let word = crate::dc::quantity_word(d);
                self.error(
                    at.span(),
                    format!(
                        "`at` takes the current the level is measured at, such as `2mA`, not a {word}"
                    ),
                );
                return;
            }
            (VTy::Q(_), _) => {
                self.error(
                    at.span(),
                    "the test current is the number the datasheet prints (`at 2mA`), not a value that depends on another field",
                );
                return;
            }
            (ty, _) => {
                let word = self.describe(ty);
                self.error(
                    at.span(),
                    format!(
                        "`at` takes the current the level is measured at, such as `2mA`, not {word}"
                    ),
                );
                return;
            }
        };
        if current.lo() != current.hi() {
            self.error(
                at.span(),
                format!("the test current is one number, not the range {current}"),
            );
            return;
        }
        if current.lo() <= 0.0 {
            self.error(
                at.span(),
                format!("the test current must be above zero, not {current}"),
            );
            return;
        }
        if let Some(earlier) = self.model.level_tests.iter().find(|t| t.level == level) {
            let (source, span, amps) = (earlier.source, earlier.span, earlier.current);
            let place = self.place_of(source, span);
            self.error(
                at.span(),
                format!(
                    "`{path}` is already stated at {amps} (at {place}): a level is one value at one load, so keep one `at`"
                ),
            );
            return;
        }
        let source = self.cur_source();
        self.model.level_tests.push(crate::model::LevelTest {
            level,
            op: c.op,
            current,
            source,
            span: c.span,
        });
    }

    /// `&=` narrows now (or queues a relation), `?=` is kept until every statement is read.
    fn narrow_statement(&mut self, c: &ast::Constraint) -> Option<FieldId> {
        let symbol = c.op.symbol();
        let Expr::Path(p) = &c.lhs else {
            let example = if c.op == CmpOp::Default {
                ".voltage ?= 5V"
            } else {
                ".voltage &= 5V"
            };
            self.error(
                c.lhs.span(),
                format!("`{symbol}` needs a field on its left, such as `{example}`"),
            );
            return None;
        };
        // Only a narrowing reaches inside a module (ADR 0116); a default does not.
        self.narrowing = c.op == CmpOp::Narrow;
        let target = self.resolve_path(p);
        self.narrowing = false;
        let rhs = match target {
            Res::Field(f) => {
                let beside = self.typed_field(f);
                self.lower_beside(&beside, &c.rhs)
            }
            _ => self.lower(&c.rhs),
        };
        let stated = match target {
            Res::Field(f) => Some(f),
            _ => None,
        };
        match (target, rhs) {
            (Res::Field(f), _) if self.is_pin_limit(f) && !self.stating_row_limits => {
                self.refuse_limit_statement(p, c);
            }
            (Res::Field(f), rhs) => {
                self.refuse_own_statement(f, symbol, c.span);
                if let Some(t) = &rhs {
                    self.refuse_fixed_count(f, p, &t.e, c.span);
                }
                if c.op == CmpOp::Narrow {
                    self.stated.insert(f);
                }
                let Some(t) = rhs else { return stated };
                let mut fields = vec![f];
                t.e.fields(&mut fields);
                if !self.physical_gate(&fields, c.span) {
                    return stated;
                }
                if c.op == CmpOp::Narrow {
                    self.apply(f, t, CmpOp::Narrow, c.span);
                } else if self.check_value(f, &t, c.span, true) {
                    let source = self.cur_source();
                    self.defaults.push(PendingDefault {
                        field: f,
                        value: t,
                        source,
                        span: c.span,
                    });
                }
            }
            (Res::Inst(i), _) => {
                let word = self.model.instance(i).kind.word();
                self.error(
                    p.span,
                    format!(
                        "`{symbol}` sets a value, but `{}` is a {word}",
                        p.to_source()
                    ),
                );
            }
            _ => {}
        }
        stated
    }

    /// Whether the field is the `.abs_min` or the `.abs_max` that every pin has.
    fn is_pin_limit(&self, field: FieldId) -> bool {
        let f = self.model.field(field);
        matches!(f.name.as_str(), "abs_min" | "abs_max")
            && self.model.instance(f.owner).kind == InstKind::Pin
    }

    /// A pin's absolute limit is written in its pin row and nowhere else (ADR 0126): the
    /// statement `.x.abs_max &= 65V` is refused, with the row that says it.
    fn refuse_limit_statement(&mut self, path: &ast::Path, c: &ast::Constraint) {
        let word = path
            .segments
            .last()
            .map_or("abs_max", |s| s.name.as_str())
            .to_string();
        let pin = {
            let mut owner = path.clone();
            owner.segments.pop();
            owner.to_source()
        };
        let value = c.rhs.to_source();
        let what = if word == "abs_min" {
            "minimum"
        } else {
            "maximum"
        };
        self.error(
            c.span,
            format!(
                "a pin's absolute {what} is written in its pin row, not with `{}`: end the row of `{pin}` in the `pins` table of its part with `{word} {value}`, such as `{pin} input \"NAME\" \"3\" {word} {value}` (ADR 0126)",
                c.op.symbol()
            ),
        );
    }

    /// A whole number written as the `.count` of a board's own `CapacitorBank` is `N x Capacitor`
    /// written another way (ADR 0119, ADR 0123): the board has a group for a number it knows. A
    /// count the solver chooses (a range), one that reads another field, and one a module passes
    /// through its interface (`.reg.output_count`, or a bank inside a module) stay. Proposed, for
    /// the owner: ADR 0123.
    fn refuse_fixed_count(&mut self, field: FieldId, path: &ast::Path, e: &RExpr, span: Span) {
        let RExpr::Const(q) = e else { return };
        if !self.in_board() || self.is_library(self.cur_source()) || q.lo() != q.hi() {
            return;
        }
        let n = q.lo();
        if n.fract() != 0.0 || n < 1.0 {
            return;
        }
        let owner = self.model.field(field).owner;
        let own = self.model.bank_count(owner) == Some(field)
            && self
                .model
                .instance(owner)
                .parent
                .is_some_and(|p| self.model.instance(p).kind == InstKind::Board);
        if !own {
            return;
        }
        let name = self.model.instance(owner).name.clone();
        let source = path.to_source();
        let message = if n == 1.0 {
            format!(
                "`{source}` fixes a bank at one capacitor, which is a plain `Capacitor`: write `.{name}: Capacitor`. `.count` is for a number the solver chooses (`.count &= 1..8`) or a module passes through its interface"
            )
        } else {
            let n = n as u32;
            format!(
                "`{source}` fixes the count of a bank the board itself declares: write `{n} x Capacitor` instead, `.{name}: {n} x Capacitor {{ ... }}`, whose members are `.{name}[0]` to `.{name}[{}]`. `.count` is for a number the solver chooses (`.count &= 1..8`) or a module passes through its interface",
                n - 1
            )
        };
        self.error(span, message);
    }

    /// Give each field its default unless a `&=` states it (ADR 0051). The rule looks at
    /// statements only: a value that reaches the field through `==` or `~` is not stated.
    /// Two defaults for one field are an error, whatever else is stated.
    pub(crate) fn resolve_defaults(&mut self) {
        let mut pending = std::mem::take(&mut self.defaults);
        pending.sort_by_key(|d| (d.field.0, d.source.0, d.span.start));
        let mut apply = Vec::new();
        let mut at = 0;
        while at < pending.len() {
            let field = pending[at].field;
            let end = pending[at..]
                .iter()
                .position(|d| d.field != field)
                .map_or(pending.len(), |n| at + n);
            if end - at > 1 {
                let places: Vec<String> = pending[at..end]
                    .iter()
                    .map(|d| self.place_of(d.source, d.span))
                    .collect();
                let path = self.model.path_of_field(field);
                let (source, span) = (pending[at + 1].source, pending[at + 1].span);
                self.report(
                    source,
                    span,
                    format!(
                        "`{path}` has {} defaults, at {}: a field has one default; keep one `?=`",
                        end - at,
                        places.join(" and ")
                    ),
                );
            }
            at = end;
        }
        // Decide with every statement in view before narrowing anything: a default that
        // narrows a face for the parts of a module must not change which others are dropped.
        let mut count = std::collections::HashMap::new();
        for d in &pending {
            *count.entry(d.field).or_insert(0) += 1;
        }
        let classes = self.net_classes();
        for d in pending {
            if count[&d.field] == 1
                && !self.stated.contains(&d.field)
                && self.net_default_holds(&d, &classes)
            {
                apply.push(d);
            }
        }
        for d in apply {
            self.source_override = Some(d.source);
            self.apply_checked(d.field, d.value, CmpOp::Narrow, d.span);
        }
        self.source_override = None;
    }

    /// The classes of pins (joined with `~`) that decide a net default: those with a net named
    /// by a `&=`, those with a pin of a part on them, and those with more than one pin.
    fn net_classes(&mut self) -> NetClasses {
        let pins: Vec<InstId> = self
            .model
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Pin)
            .map(|(id, _)| id)
            .collect();
        let mut classes = NetClasses::default();
        let mut size = std::collections::HashMap::new();
        for pin in pins {
            let class = self.find(pin.0);
            if let Some(Member::Field(f)) = self.model.member(pin, "net")
                && self.stated.contains(&f)
            {
                classes.stated.insert(class);
            }
            if self.model.is_terminal(pin) {
                classes.terminal.insert(class);
            }
            *size.entry(class).or_insert(0usize) += 1;
        }
        classes.joined = size
            .into_iter()
            .filter(|&(_, n)| n > 1)
            .map(|(c, _)| c)
            .collect();
        classes
    }

    /// Whether a default applies. A net name is board-wide, and a default for one (a rail's
    /// ground, ADR 0121) is an offer with three more limits than any other default: it needs a
    /// board to name the net on; it needs a pin of a part on the net and a second pin joined to
    /// it, since a name for a pin nothing is joined to would only make a port that holds it look
    /// connected, or ask a pad of a part that has none (a bus port's reference rail is on no net
    /// until a part joins it); and it gives way to a name
    /// that reaches the pin by a join, since a pin joined to one named `AGND` is on `AGND`,
    /// and a default of `GND` on the other would contradict the join.
    fn net_default_holds(&mut self, d: &PendingDefault, classes: &NetClasses) -> bool {
        let field = self.model.field(d.field);
        if field.name != "net" {
            return true;
        }
        let owner = field.owner;
        if self.model.board_of(owner).is_none() {
            return false;
        }
        if self.model.instance(owner).kind != InstKind::Pin {
            return true;
        }
        let class = self.find(owner.0);
        classes.terminal.contains(&class)
            && classes.joined.contains(&class)
            && !classes.stated.contains(&class)
    }

    /// `file:line` of a span.
    pub(crate) fn place_of(&self, source: SourceId, span: Span) -> String {
        let source = self.sources.get(source);
        let (line, _) = netsmith_lang::ParseError::new(span, "").line_col(&source.text);
        format!("{}:{line}", source.name)
    }

    /// The statement at `span` as the source has it, for a drawing to show, if it is one the
    /// design states: not a statement of a physical file (where parts go on the board, not
    /// what the circuit is), not one inside a part (the part's own model, which its
    /// definition states), and not one of the standard library.
    fn written(&self, span: Span) -> Option<crate::model::Written> {
        let source = self.cur_source();
        if self.phys.is_some()
            || self.is_library(source)
            || !matches!(
                self.model.instance(self.current()).kind,
                InstKind::Board | InstKind::Module
            )
        {
            return None;
        }
        let text = self.sources.get(source).text.as_str();
        let end = netsmith_lang::floor_boundary(text, span.end);
        let start = netsmith_lang::floor_boundary(text, span.start).min(end);
        let (file, line) = self.line_in(source, span);
        Some(crate::model::Written {
            scope: self.current(),
            text: text[start..end]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            file,
            line,
            chain: Vec::new(),
        })
    }

    fn relation_statement(&mut self, c: &ast::Constraint) -> Option<FieldId> {
        self.range_ends = matches!(c.op, CmpOp::Le | CmpOp::Ge);
        // A bare name on one side is read beside the other side (ADR 0118).
        let (l, r) = if bare_name(&c.lhs).is_some() && bare_name(&c.rhs).is_none() {
            let r = self.lower(&c.rhs);
            let l = match &r {
                Some(r) => self.lower_beside(r, &c.lhs),
                None => None,
            };
            (l, r)
        } else {
            let l = self.lower(&c.lhs);
            let r = match &l {
                Some(l) => self.lower_beside(l, &c.rhs),
                None => self.lower(&c.rhs),
            };
            (l, r)
        };
        self.range_ends = false;
        let (Some(l), Some(r)) = (l, r) else {
            return None;
        };
        let stated = match &l.e {
            RExpr::Field(f) => Some(*f),
            _ => None,
        };
        let mut fields = Vec::new();
        l.e.fields(&mut fields);
        r.e.fields(&mut fields);
        if !self.physical_gate(&fields, c.span) {
            return stated;
        }

        let comparable = match (l.ty, r.ty) {
            (VTy::Q(a), VTy::Q(b)) => a == b,
            (VTy::Text, VTy::Text) => c.op == CmpOp::EqEq,
            (VTy::Enum(a), VTy::Enum(b)) => a == b && c.op == CmpOp::EqEq,
            _ => false,
        };
        if !comparable {
            let mut message = format!(
                "the two sides do not match: left is {}, right is {}",
                self.describe(l.ty),
                self.describe(r.ty)
            );
            if l.ty == r.ty && !matches!(l.ty, VTy::Q(_)) {
                message = format!(
                    "`{}` compares numbers; {} are compared with `==` only",
                    c.op.symbol(),
                    if matches!(l.ty, VTy::Enum(_)) {
                        "variants"
                    } else {
                        "text values"
                    }
                );
            }
            // A variant against a quoted word: the migration from text to the enum.
            for (side, text) in [(&l, &r), (&r, &l)] {
                if let (VTy::Enum(e), VTy::Text, RExpr::Text(word)) = (side.ty, text.ty, &text.e) {
                    message = format!("{message}; {}", self.text_for_enum(e, word));
                }
            }
            self.error(c.span, message);
            return stated;
        }
        if let Some(f) = crate::optional::states(c.op, &l.e, &r.e) {
            self.refuse_own_statement(f, c.op.symbol(), c.span);
        }
        let source = self.cur_source();
        let written = self.written(c.span);
        let own = self.in_own_definition();
        std::sync::Arc::make_mut(&mut self.model.relations).push(Relation {
            lhs: l.e,
            op: c.op,
            rhs: r.e,
            source,
            span: c.span,
            written,
            own,
        });
        stated
    }

    /// Narrow `field` by a constant now, or queue the constraint for the solver.
    fn apply(&mut self, field: FieldId, value: Typed, op: CmpOp, span: Span) {
        if self.check_value(field, &value, span, false) {
            self.apply_checked(field, value, op, span);
        }
    }

    /// Whether `value` may be stated for `field`, reporting it when not: only a board names
    /// nets, and the value must be the kind the field holds. `default` is a `?=`.
    fn check_value(&mut self, field: FieldId, value: &Typed, span: Span, default: bool) -> bool {
        // Net names are board-wide, so a module or part that named one would give every
        // instance of itself the same net. Only the board names nets: that holds for a
        // pin's `net` and for the `net` of a plane in a manufacturing process alike. The one
        // exception is a default in an interface (`Rail`'s ground): the name is an offer that
        // takes effect only where the interface is on a board, and a rail is the one thing
        // whose instances are meant to share a name (ADR 0116).
        let net_default = default && self.in_kind(DefKind::Interface);
        if self.model.field(field).name == "net" && !self.in_board() && !net_default {
            let path = self.model.path_of_field(field);
            let message = if self.in_system() {
                format!(
                    "`{path}` names a net, and a system does not: each board names its own nets, and a system only says which ports meet"
                )
            } else if default {
                format!(
                    "`{path}` names a net by default, and only a board names nets: an interface such as a rail may offer a net name, since every instance of it is meant to share one; set it where the board uses this"
                )
            } else {
                format!(
                    "`{path}` names a net, and only a board names nets: set it where the board uses this, so that every instance does not share one name"
                )
            };
            self.error(span, message);
            return false;
        }
        let kind = self.model.field(field).kind;
        let matches = match (kind, value.ty) {
            (FieldKind::Quantity(a), VTy::Q(b)) => a == b,
            (FieldKind::Text, VTy::Text) => true,
            (FieldKind::Enum(a), VTy::Enum(b)) => a == b,
            _ => false,
        };
        if !matches {
            let holds = self.describe_kind(kind);
            let path = self.model.path_of_field(field);
            let mut message = format!(
                "`{path}` holds {holds}, but this value is {}",
                self.describe(value.ty)
            );
            if let (FieldKind::Enum(e), VTy::Text, RExpr::Text(word)) = (kind, value.ty, &value.e) {
                message = format!(
                    "`{path}` holds {holds}, but this value is text; {}",
                    self.text_for_enum(e, word)
                );
            }
            self.error(span, message);
            return false;
        }
        true
    }

    fn apply_checked(&mut self, field: FieldId, value: Typed, op: CmpOp, span: Span) {
        match value.e {
            RExpr::Const(q) => self.narrow(field, Domain::Quantity(q), span),
            RExpr::Text(s) => self.narrow(field, Domain::Text(s), span),
            other => {
                let source = self.cur_source();
                let own = self.in_own_definition();
                std::sync::Arc::make_mut(&mut self.model.relations).push(Relation {
                    lhs: RExpr::Field(field),
                    op,
                    rhs: other,
                    source,
                    span,
                    written: None,
                    own,
                });
            }
        }
    }

    /// A face stated for a module is a requirement on every part of it, at any depth
    /// (ADR 0048, "Syntax decisions" 5): a part that needs the other face is stated part by
    /// part and cannot be under a module that states one. Narrow the parts' own faces.
    fn face_of_module(&mut self, field: FieldId, face: &str, span: Span) {
        let place = self.model.field(field).owner;
        let Some(module) = self.model.instance(place).parent else {
            return;
        };
        if self.model.field(field).name != "side"
            || !self.model.is_place_field(field)
            || self.model.instance(module).kind != InstKind::Module
        {
            return;
        }
        let module_path = self.model.path_of(module);
        let mut parts = Vec::new();
        let mut stack = vec![module];
        while let Some(inst) = stack.pop() {
            for (_, member) in self.model.instance(inst).children.clone() {
                let Member::Inst(child) = member else {
                    continue;
                };
                match self.model.instance(child).kind {
                    InstKind::Part => parts.push(child),
                    InstKind::Module => stack.push(child),
                    _ => {}
                }
            }
        }
        parts.sort_by_key(|p| p.0);
        for part in parts {
            let Some(Member::Inst(part_place)) = self.model.member(part, "place") else {
                continue;
            };
            let Some(Member::Field(side)) = self.model.member(part_place, "side") else {
                continue;
            };
            if let Domain::Text(old) = &self.model.field(side).domain
                && old != face
            {
                let part_path = self.model.path_of(part);
                let at = self.stated_at(side);
                let source = self.cur_source();
                self.report(
                    source,
                    span,
                    format!(
                        "`{module_path}` is on the {face} face, which is a requirement on every part of it, but `{part_path}.place.side` is already \"{old}\"{at}; a module with a part on the other face cannot state a face, state the faces of its parts one by one"
                    ),
                );
                continue;
            }
            self.narrow(side, Domain::Text(face.to_string()), span);
        }
    }

    /// Whether `inst` is, or lies inside, a use of a generic that has more than one provider
    /// in scope (ADR 0113).
    fn in_varied_use(&self, inst: InstId) -> bool {
        let mut cur = Some(inst);
        while let Some(at) = cur {
            if self
                .uses
                .iter()
                // A use being made has `InstId(0)`, the root's, until its instance exists.
                .any(|u| u.inst == at && u.inst != InstId(0) && u.candidates.len() > 1)
            {
                return true;
            }
            cur = self.model.instance(at).parent;
        }
        false
    }

    /// ` (stated at file:line)` for the first statement that narrowed `field`, or nothing.
    fn stated_at(&self, field: FieldId) -> String {
        let Some((src, sp)) = self.model.field(field).narrowed_at.first().copied() else {
            return String::new();
        };
        let source = self.sources.get(src);
        let (line, _) = netsmith_lang::ParseError::new(sp, "").line_col(&source.text);
        format!(" (stated at {}:{line})", source.name)
    }

    /// The module above a part that already states the face `old` for every part of it, when
    /// `field` is the part's own `.place.side`: the face it asks for now is the other one.
    fn module_with_face(&self, field: FieldId, old: &Domain) -> Option<(String, FieldId)> {
        if self.model.field(field).name != "side" || !self.model.is_place_field(field) {
            return None;
        }
        let part = self.model.instance(self.model.field(field).owner).parent?;
        let mut cur = self.model.instance(part).parent;
        while let Some(inst) = cur {
            if self.model.instance(inst).kind == InstKind::Module
                && let Some(Member::Inst(place)) = self.model.member(inst, "place")
                && let Some(Member::Field(side)) = self.model.member(place, "side")
                && &self.model.field(side).domain == old
            {
                return Some((self.model.path_of(inst), side));
            }
            cur = self.model.instance(inst).parent;
        }
        None
    }

    fn narrow(&mut self, field: FieldId, new: Domain, span: Span) {
        let source = self.cur_source();
        let path = self.model.path_of_field(field);
        let old = self.model.field(field).domain.clone();
        let merged = match (&old, &new) {
            (Domain::Unset, _) => Some(new.clone()),
            (Domain::Quantity(a), Domain::Quantity(b)) => match a.intersect(b) {
                Ok(Some(q)) => Some(Domain::Quantity(q)),
                _ => None,
            },
            (Domain::Text(a), Domain::Text(b)) => (a == b).then(|| new.clone()),
            _ => None,
        };
        match merged {
            Some(domain) => {
                let f = &mut self.model.fields[field.0];
                f.domain = domain;
                f.narrowed_at.push((source, span));
                if let Domain::Text(face) = &new {
                    self.face_of_module(field, face, span);
                }
            }
            None => {
                // A part's face against the face its module states for every part of it: say
                // so, as the other order of the two lines does.
                if let (Domain::Text(face), Domain::Text(other)) = (&old, &new)
                    && let Some((module_path, module_side)) = self.module_with_face(field, &old)
                {
                    let at = self.stated_at(module_side);
                    self.report(
                        source,
                        span,
                        format!(
                            "`{module_path}` is on the {face} face, which is a requirement on every part of it{at}, but `{path}` is \"{other}\"; a module with a part on the other face cannot state a face, state the faces of its parts one by one"
                        ),
                    );
                    return;
                }
                let (old, new) = (
                    self.model.show_value(field, &old),
                    self.model.show_value(field, &new),
                );
                let message = format!(
                    "contradiction on `{path}`: it is already {old}, which does not overlap {new}"
                );
                // Inside a use with several providers, another provider may not state what
                // this one does: the build tries them before it stops (ADR 0113).
                if self.in_varied_use(self.model.field(field).owner) {
                    self.provider_conflicts.push(crate::diag::Diagnostic {
                        source,
                        span,
                        message: message.clone(),
                    });
                }
                self.report(source, span, message);
            }
        }
    }
}

/// The name written bare on one side of a statement (`Red`, `USART1_TX`): one plain segment
/// with no leading dot.
fn bare_name(e: &Expr) -> Option<&ast::Ident> {
    match e {
        Expr::Path(p) if !p.leading_dot && p.segments.len() == 1 && !p.segments[0].quoted => {
            Some(&p.segments[0])
        }
        _ => None,
    }
}

/// Whether a text could be written as a plain name.
fn is_plain_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `` `A`, `B` and `C` ``
pub(crate) fn list_of(items: &[String]) -> String {
    match items.split_last() {
        None => String::new(),
        Some((only, [])) => only.clone(),
        Some((last, init)) => format!("{} and {last}", init.join(", ")),
    }
}

/// The word of `options` that `word` is most likely a slip for: the same letters in another
/// case, or at most two edits away (and fewer than half its length).
pub fn closest(word: &str, options: &[String]) -> Option<String> {
    let lower = word.to_lowercase();
    if let Some(same) = options.iter().find(|o| o.to_lowercase() == lower) {
        return Some(same.clone());
    }
    let limit = (word.chars().count() / 2).min(2);
    options
        .iter()
        .map(|o| (edit_distance(&lower, &o.to_lowercase()), o))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, _)| *d)
        .map(|(_, o)| o.clone())
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1)
                .min(row[j] + 1)
                .min(diagonal + usize::from(ca != cb));
            diagonal = above;
        }
    }
    row[b.len()]
}
