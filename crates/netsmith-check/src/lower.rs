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
use crate::model::{Domain, FieldId, FieldKind, RExpr, Relation};

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
            (Res::Field(f), Some(t)) => self.apply(f, t, CmpOp::Narrow, c.span),
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
            }
            None => self.report(
                source,
                span,
                format!(
                    "contradiction on `{path}`: it is already {old}, which does not overlap {new}"
                ),
            ),
        }
    }
}
