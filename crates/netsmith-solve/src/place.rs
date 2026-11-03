//! The windows that a design's requirements on positions leave (ADR 0045).
//!
//! A field of `.place` is narrowed by `&=` (and `==`), which the solver does like any field,
//! and bounded by `>=` and `<=`. A bound on any other field is a claim to verify; on a
//! `.place` field it narrows, but a range here cannot be open at one end, so the bounds stay
//! relations and this module intersects them with the field's range on demand.

use netsmith_check::{FieldId, Model, RExpr};
use netsmith_lang::Span;
use netsmith_lang::ast::CmpOp;

use crate::eval::{Val, eval};
use netsmith_check::{Domain, SourceId};

/// The values a `.place` field may take after every statement that narrows it, in the
/// field's base unit (metres for a length, a plain number for the rotation). `lo` is
/// negative infinity and `hi` positive infinity when nothing bounds that side.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub lo: f64,
    pub hi: f64,
    /// The statements that narrowed it, in source order.
    pub sites: Vec<(SourceId, Span)>,
    /// A statement that bounds the field but could not be evaluated, because a value it
    /// needs is not known.
    pub unknown: bool,
    /// The board's `outline.width` or `outline.height`, by path, when a bound is measured from
    /// it and the outline does not state that size as one value: the bound then has no
    /// number (ADR 0045).
    pub open_outline: Vec<String>,
    /// What gave the bound that decides each end, for a message: `usb.place.y >= 3mm`.
    pub lo_from: String,
    pub hi_from: String,
}

impl Window {
    /// Whether nothing is stated.
    pub fn is_open(&self) -> bool {
        self.lo == f64::NEG_INFINITY && self.hi == f64::INFINITY
    }

    /// Whether no value is left. A hair of rounding (a picometre) is not an empty window.
    pub fn is_empty(&self) -> bool {
        self.lo > self.hi + 1e-12
    }

    /// Why nothing is left, in words that give both numbers, for a message about `name`.
    pub fn clash(&self, name: &str, dim: netsmith_quantity::Dimension) -> String {
        let show = |v: f64| {
            netsmith_quantity::Quantity::point(dim, v).map_or(format!("{v}"), |q| q.to_string())
        };
        format!(
            "the requirements on `{name}` cannot all hold: it must be at least {} ({}) and at most {} ({})",
            show(self.lo),
            self.lo_from,
            show(self.hi),
            self.hi_from
        )
    }
}

/// The window of the `.place` field `field` of the (solved) `model`.
pub fn window(model: &Model, field: FieldId) -> Window {
    let f = model.field(field);
    let mut w = Window {
        lo: f64::NEG_INFINITY,
        hi: f64::INFINITY,
        sites: f.narrowed_at.clone(),
        unknown: false,
        open_outline: Vec::new(),
        lo_from: String::new(),
        hi_from: String::new(),
    };
    if let Domain::Quantity(q) = &f.domain {
        w.lo = q.lo();
        w.hi = q.hi();
        w.lo_from = format!("its value {q}");
        w.hi_from = w.lo_from.clone();
    }
    for r in model.relations() {
        // `field >= other`, `other <= field`: a lower bound; the mirror images an upper one.
        let (other, lower) = match (&r.lhs, &r.rhs, r.op) {
            (RExpr::Field(a), other, CmpOp::Ge) if *a == field => (other, true),
            (RExpr::Field(a), other, CmpOp::Le) if *a == field => (other, false),
            (other, RExpr::Field(b), CmpOp::Le) if *b == field => (other, true),
            (other, RExpr::Field(b), CmpOp::Ge) if *b == field => (other, false),
            _ => continue,
        };
        w.sites.push((r.source, r.span));
        let said = format!(
            "`{} {} {}`",
            model.show_expr(&r.lhs),
            if r.op == CmpOp::Ge { ">=" } else { "<=" },
            model.show_expr(&r.rhs)
        );
        let mut mentioned = Vec::new();
        other.fields(&mut mentioned);
        for f in mentioned {
            let name = model.path_of_field(f);
            let fixed = matches!(&model.field(f).domain, Domain::Quantity(q) if q.lo() == q.hi());
            if (name == "outline.width" || name == "outline.height")
                && !fixed
                && !w.open_outline.contains(&name)
            {
                w.open_outline.push(name);
            }
        }
        match eval(model, other) {
            Ok(Val::Q(v)) if lower && v.hi() > w.lo => {
                w.lo = v.hi();
                w.lo_from = said;
            }
            Ok(Val::Q(v)) if !lower && v.lo() < w.hi => {
                w.hi = v.lo();
                w.hi_from = said;
            }
            Ok(Val::Q(_)) => {}
            _ => w.unknown = true,
        }
    }
    w
}
