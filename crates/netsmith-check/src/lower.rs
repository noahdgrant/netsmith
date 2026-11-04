//! Turning syntax into checked expressions, and applying constraints.
//!
//! A constraint whose right-hand side is a constant narrows its field right away. That
//! is order independent, because intersecting is. Anything that mentions another field
//! is recorded as a [`Relation`] for the solver instead of being evaluated here, so the
//! meaning of a block never depends on the order of its statements.

use netsmith_lang::Span;
use netsmith_lang::ast::{self, BinOp, CmpOp, Expr};
use netsmith_quantity::{Dimension, Quantity, parse_unit};

use crate::checker::{Checker, Res};
use crate::model::{Domain, FieldId, FieldKind, InstKind, Member, RExpr, Relation};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum VTy {
    Q(Dimension),
    Text,
}

impl VTy {
    fn describe(self) -> String {
        match self {
            Self::Q(d) => d.to_string(),
            Self::Text => "text".to_string(),
        }
    }
}

pub(crate) struct Typed {
    pub e: RExpr,
    pub ty: VTy,
}

impl Checker<'_> {
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
            Expr::Path(p) => match self.resolve_path(p) {
                Res::Field(f) => {
                    let ty = match self.model.field(f).kind {
                        FieldKind::Quantity(d) => VTy::Q(d),
                        FieldKind::Text => VTy::Text,
                    };
                    Some(Typed {
                        e: RExpr::Field(f),
                        ty,
                    })
                }
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
                        self.error(*span, "both ends of a range must be fixed numbers");
                        None
                    }
                }
            }
            Expr::Tolerance { center, tol, span } => {
                let c = self.lower(center);
                let t = self.lower(tol);
                let (c, t) = (c?, t?);
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
        match c.op {
            CmpOp::Narrow => self.narrow_statement(c),
            _ => self.relation_statement(c),
        }
    }

    fn narrow_statement(&mut self, c: &ast::Constraint) {
        let Expr::Path(p) = &c.lhs else {
            self.error(
                c.lhs.span(),
                "`&=` needs a field on its left, such as `.voltage &= 5V`",
            );
            return;
        };
        let target = self.resolve_path(p);
        let rhs = self.lower(&c.rhs);
        match (target, rhs) {
            (Res::Field(f), Some(t)) => {
                let mut fields = vec![f];
                t.e.fields(&mut fields);
                if self.physical_gate(&fields, c.span) {
                    self.apply(f, t, CmpOp::Narrow, c.span);
                }
            }
            (Res::Inst(i), _) => {
                let word = self.model.instance(i).kind.word();
                self.error(
                    p.span,
                    format!("`&=` sets a value, but `{}` is a {word}", p.to_source()),
                );
            }
            _ => {}
        }
    }

    fn relation_statement(&mut self, c: &ast::Constraint) {
        let l = self.lower(&c.lhs);
        let r = self.lower(&c.rhs);
        let (Some(l), Some(r)) = (l, r) else { return };
        let mut fields = Vec::new();
        l.e.fields(&mut fields);
        r.e.fields(&mut fields);
        if !self.physical_gate(&fields, c.span) {
            return;
        }

        let comparable = match (l.ty, r.ty) {
            (VTy::Q(a), VTy::Q(b)) => a == b,
            (VTy::Text, VTy::Text) => c.op == CmpOp::EqEq,
            _ => false,
        };
        if !comparable {
            self.error(
                c.span,
                format!(
                    "the two sides do not match: left is {}, right is {}",
                    l.ty.describe(),
                    r.ty.describe()
                ),
            );
            return;
        }
        let source = self.cur_source();
        self.model.relations.push(Relation {
            lhs: l.e,
            op: c.op,
            rhs: r.e,
            source,
            span: c.span,
        });
    }

    /// Narrow `field` by a constant now, or queue the constraint for the solver.
    fn apply(&mut self, field: FieldId, value: Typed, op: CmpOp, span: Span) {
        // Net names are board-wide, so a module or part that named one would give every
        // instance of itself the same net. Only the board names nets: that holds for a
        // pin's `net` and for the `net` of a plane in a manufacturing process alike.
        if self.model.field(field).name == "net" && !self.in_board() {
            let path = self.model.path_of_field(field);
            let message = if self.in_system() {
                format!(
                    "`{path}` names a net, and a system does not: each board names its own nets, and a system only says which ports meet"
                )
            } else {
                format!(
                    "`{path}` names a net, and only a board names nets: set it where the board uses this, so that every instance does not share one name"
                )
            };
            self.error(span, message);
            return;
        }
        let kind = self.model.field(field).kind;
        let matches = match (kind, value.ty) {
            (FieldKind::Quantity(a), VTy::Q(b)) => a == b,
            (FieldKind::Text, VTy::Text) => true,
            _ => false,
        };
        if !matches {
            let holds = match kind {
                FieldKind::Quantity(d) => d.to_string(),
                FieldKind::Text => "text".to_string(),
            };
            let path = self.model.path_of_field(field);
            self.error(
                span,
                format!(
                    "`{path}` holds {holds}, but this value is {}",
                    value.ty.describe()
                ),
            );
            return;
        }
        match value.e {
            RExpr::Const(q) => self.narrow(field, Domain::Quantity(q), span),
            RExpr::Text(s) => self.narrow(field, Domain::Text(s), span),
            other => {
                let source = self.cur_source();
                self.model.relations.push(Relation {
                    lhs: RExpr::Field(field),
                    op,
                    rhs: other,
                    source,
                    span,
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
                self.report(
                    source,
                    span,
                    format!(
                        "contradiction on `{path}`: it is already {old}, which does not overlap {new}"
                    ),
                );
            }
        }
    }
}
