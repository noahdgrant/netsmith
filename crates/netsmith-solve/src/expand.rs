//! Tidying a derivative so that what cancels, cancels.
//!
//! Interval arithmetic cannot see that `s * (rt + rb) - s * rb` is `s * rt`: it subtracts two
//! wide ranges and gets a range that spans zero, so a derivative that is plainly positive
//! looks as if it might change sign. Multiplying such an expression out and collecting like
//! terms makes the cancellation visible before anything is evaluated.

use netsmith_check::{FieldId, RExpr};
use netsmith_lang::ast::BinOp;
use netsmith_quantity::{Dimension, Quantity};

/// One product: `coef` of the dimension `dim`, times the fields, kept in sorted order.
#[derive(Clone)]
struct Term {
    coef: f64,
    dim: Dimension,
    fields: Vec<FieldId>,
}

/// `e` with every division-free part multiplied out and its like terms collected. A
/// quotient keeps its shape, with its top and bottom tidied separately. `None` is exactly
/// zero.
pub fn tidy(e: &RExpr) -> Option<RExpr> {
    match terms(e) {
        Some(terms) => build(collect(terms)?),
        None => match e {
            RExpr::Bin(BinOp::Div, a, b) => {
                // A bottom that tidies to zero is left alone: evaluation reports it.
                let Some(b) = tidy(b) else {
                    return Some(e.clone());
                };
                let a = tidy(a)?;
                Some(RExpr::Bin(BinOp::Div, Box::new(a), Box::new(b)))
            }
            RExpr::Bin(op, a, b) => match (tidy(a), tidy(b)) {
                (Some(a), Some(b)) => Some(RExpr::Bin(*op, Box::new(a), Box::new(b))),
                _ => Some(e.clone()),
            },
            RExpr::Neg(x) => tidy(x).map(|x| RExpr::Neg(Box::new(x))),
            _ => Some(e.clone()),
        },
    }
}

/// The terms of `e`, or `None` if it holds a division or a constant that is not one value.
fn terms(e: &RExpr) -> Option<Vec<Term>> {
    match e {
        RExpr::Const(q) if q.is_point() => Some(vec![Term {
            coef: q.lo(),
            dim: q.dim(),
            fields: vec![],
        }]),
        RExpr::Field(f) => Some(vec![Term {
            coef: 1.0,
            dim: Dimension::NONE,
            fields: vec![*f],
        }]),
        RExpr::Neg(x) => Some(terms(x)?.into_iter().map(negate).collect()),
        RExpr::Bin(BinOp::Add, a, b) => {
            let mut all = terms(a)?;
            all.extend(terms(b)?);
            Some(all)
        }
        RExpr::Bin(BinOp::Sub, a, b) => {
            let mut all = terms(a)?;
            all.extend(terms(b)?.into_iter().map(negate));
            Some(all)
        }
        RExpr::Bin(BinOp::Mul, a, b) => {
            let (a, b) = (terms(a)?, terms(b)?);
            let mut all = Vec::with_capacity(a.len() * b.len());
            for x in &a {
                for y in &b {
                    let mut fields = x.fields.clone();
                    fields.extend(&y.fields);
                    fields.sort();
                    all.push(Term {
                        coef: x.coef * y.coef,
                        dim: x.dim * y.dim,
                        fields,
                    });
                }
            }
            Some(all)
        }
        _ => None,
    }
}

fn negate(t: Term) -> Term {
    Term { coef: -t.coef, ..t }
}

/// Like terms added together and the ones that cancel dropped. `None` if two like terms
/// disagree about their unit, which leaves the expression as it was.
fn collect(terms: Vec<Term>) -> Option<Vec<Term>> {
    let mut out: Vec<Term> = Vec::new();
    for t in terms {
        match out.iter_mut().find(|o| o.fields == t.fields) {
            Some(o) if o.dim == t.dim => o.coef += t.coef,
            Some(_) => return None,
            None => out.push(t),
        }
    }
    out.retain(|t| t.coef != 0.0);
    Some(out)
}

/// The terms as an expression; `None` when there are none.
fn build(terms: Vec<Term>) -> Option<RExpr> {
    let mut sum: Option<RExpr> = None;
    for t in terms {
        let mut product = RExpr::Const(Quantity::point(t.dim, t.coef).ok()?);
        for f in t.fields {
            product = RExpr::Bin(BinOp::Mul, Box::new(product), Box::new(RExpr::Field(f)));
        }
        sum = Some(match sum {
            None => product,
            Some(s) => RExpr::Bin(BinOp::Add, Box::new(s), Box::new(product)),
        });
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(n: usize) -> RExpr {
        RExpr::Field(FieldId(n))
    }
    fn bin(op: BinOp, a: RExpr, b: RExpr) -> RExpr {
        RExpr::Bin(op, Box::new(a), Box::new(b))
    }

    #[test]
    fn what_cancels_is_gone() {
        // s * (t + b) - s * b  is  s * t
        let (s, t, b) = (field(0), field(1), field(2));
        let e = bin(
            BinOp::Sub,
            bin(BinOp::Mul, s.clone(), bin(BinOp::Add, t.clone(), b.clone())),
            bin(BinOp::Mul, s.clone(), b),
        );
        let tidy = tidy(&e).unwrap();
        let mut fields = Vec::new();
        crate::eval::fields_in(&tidy, &mut fields);
        fields.sort();
        assert_eq!(fields.len(), 2, "only s and t are left: {tidy:?}");
    }

    #[test]
    fn a_difference_of_equals_is_zero() {
        let e = bin(BinOp::Sub, field(0), field(0));
        assert!(tidy(&e).is_none());
    }
}
