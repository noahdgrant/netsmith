//! The windows that a design's requirements on positions leave (ADR 0045).
//!
//! A field of `.place` is narrowed by `&=` (and `==`), which the solver does like any field,
//! and bounded by `>=` and `<=`. A bound on any other field is a claim to verify; on a
//! `.place` field it narrows, but a range here cannot be open at one end, so the bounds stay
//! relations and this module intersects them with the field's range on demand.

use netsmith_check::{FieldId, FieldKind, InstKind, Model, RExpr};
use netsmith_lang::Span;
use netsmith_lang::ast::{BinOp, CmpOp};
use netsmith_quantity::Dimension;

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
        let said = format!(
            "`{} {} {}`",
            model.show_expr(&r.lhs),
            if r.op == CmpOp::Ge { ">=" } else { "<=" },
            model.show_expr(&r.rhs)
        );
        let mut mentioned = Vec::new();
        other.fields(&mut mentioned);
        // A bound against the place of another part is a relation between two parts
        // ([`offsets`]), which the placer and the check evaluate, not a bound on this one.
        if mentioned.iter().any(|&f| model.is_place_field(f)) {
            continue;
        }
        w.sites.push((r.source, r.span));
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

/// A relation between the places of two parts that the placer and the check evaluate
/// (ADR 0045, section 3): in metres, the measure of the field `b` minus that of the field `a`
/// lies in `lo..=hi`. `relation` is its index in `model.relations()`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceOffset {
    pub relation: usize,
    pub a: FieldId,
    pub b: FieldId,
    pub lo: f64,
    pub hi: f64,
}

/// A linear form in `.place` fields, plus a range of constants.
struct Linear {
    terms: Vec<(FieldId, f64)>,
    k: (f64, f64),
}

impl Linear {
    fn constant(lo: f64, hi: f64) -> Self {
        Self {
            terms: Vec::new(),
            k: (lo, hi),
        }
    }

    fn negated(mut self) -> Self {
        for t in &mut self.terms {
            t.1 = -t.1;
        }
        self.k = (-self.k.1, -self.k.0);
        self
    }

    fn plus(mut self, other: Linear) -> Self {
        for (f, c) in other.terms {
            match self.terms.iter_mut().find(|t| t.0 == f) {
                Some(t) => t.1 += c,
                None => self.terms.push((f, c)),
            }
        }
        self.k = (self.k.0 + other.k.0, self.k.1 + other.k.1);
        self
    }
}

/// `e` as a sum of `.place` fields with coefficients, and constants. A part of the
/// expression that mentions no `.place` field is evaluated over the domains (the outline's
/// width, a value the design gives); one that does is only a sum, a difference or a negation
/// of such parts.
fn linear(model: &Model, e: &RExpr) -> Option<Linear> {
    let mut fields = Vec::new();
    e.fields(&mut fields);
    if !fields.iter().any(|&f| model.is_place_field(f)) {
        return match eval(model, e) {
            Ok(Val::Q(q)) if q.dim() == Dimension::LENGTH => Some(Linear::constant(q.lo(), q.hi())),
            _ => None,
        };
    }
    match e {
        RExpr::Field(f) => Some(Linear {
            terms: vec![(*f, 1.0)],
            k: (0.0, 0.0),
        }),
        RExpr::Neg(inner) => linear(model, inner).map(Linear::negated),
        RExpr::Bin(BinOp::Add, a, b) => Some(linear(model, a)?.plus(linear(model, b)?)),
        RExpr::Bin(BinOp::Sub, a, b) => Some(linear(model, a)?.plus(linear(model, b)?.negated())),
        _ => None,
    }
}

/// Which relations of `model` are relations between the places of two parts, and what each
/// says: `b - a` in a window. They are the ones whose two sides, with everything else
/// evaluated, leave exactly one `.place` field of one part with the coefficient 1 and one
/// of another part with the coefficient -1, both a position along one axis (`x`, `left` and
/// `right`, or `y`, `top` and `bottom`) and a length. A relation that is anything else (a
/// sum of two places, a turn, a place of a module, a product) is not here, and stays
/// unevaluated.
pub fn offsets(model: &Model) -> Vec<PlaceOffset> {
    let mut found = Vec::new();
    for (index, r) in model.relations().iter().enumerate() {
        if r.op == CmpOp::Narrow {
            continue;
        }
        // Only a relation that mentions two places is looked at: most relations of a design
        // are about the circuit.
        let mut fields = Vec::new();
        r.lhs.fields(&mut fields);
        r.rhs.fields(&mut fields);
        if fields.iter().filter(|&&f| model.is_place_field(f)).count() < 2 {
            continue;
        }
        let (Some(l), Some(rr)) = (linear(model, &r.lhs), linear(model, &r.rhs)) else {
            continue;
        };
        // `l op rr` as `terms op k`: the places on the left, the constants on the right.
        let mut terms = l.terms;
        for (f, c) in rr.terms {
            match terms.iter_mut().find(|t| t.0 == f) {
                Some(t) => t.1 -= c,
                None => terms.push((f, -c)),
            }
        }
        terms.retain(|t| t.1.abs() > 1e-12);
        let [first, second] = terms.as_slice() else {
            continue;
        };
        let (plus, minus) = match (first.1, second.1) {
            (p, m) if (p - 1.0).abs() < 1e-9 && (m + 1.0).abs() < 1e-9 => (first.0, second.0),
            (m, p) if (p - 1.0).abs() < 1e-9 && (m + 1.0).abs() < 1e-9 => (second.0, first.0),
            _ => continue,
        };
        let (kl, kr) = (l.k, rr.k);
        // The range the sum of the places may take, in the worst case of the constants:
        // `==` keeps the whole of the left inside the right, a bound keeps every value of
        // the left on its side of every value of the right.
        let (lo, hi) = match r.op {
            CmpOp::EqEq => (kr.0 - kl.0, kr.1 - kl.1),
            CmpOp::Ge => (kr.1 - kl.0, f64::INFINITY),
            CmpOp::Le => (f64::NEG_INFINITY, kr.0 - kl.1),
            CmpOp::Narrow => continue,
        };
        // The terms were `first*f1 + second*f2`, so when the plus is the first the sum is
        // `plus - minus`: the window is for `b - a` with `b` the plus.
        if !is_position_of_parts(model, plus, minus) {
            continue;
        }
        found.push(PlaceOffset {
            relation: index,
            a: minus,
            b: plus,
            lo,
            hi,
        });
    }
    found
}

/// Whether the two fields are positions along one axis of two different parts.
fn is_position_of_parts(model: &Model, a: FieldId, b: FieldId) -> bool {
    let axis = |f: FieldId| match model.field(f).name.as_str() {
        "x" | "left" | "right" => Some(0),
        "y" | "top" | "bottom" => Some(1),
        _ => None,
    };
    let owner = |f: FieldId| {
        let place = model.field(f).owner;
        let part = model.instance(place).parent?;
        (model.instance(part).kind == InstKind::Part).then_some(part)
    };
    let length = |f: FieldId| model.field(f).kind == FieldKind::Quantity(Dimension::LENGTH);
    axis(a).is_some()
        && axis(a) == axis(b)
        && length(a)
        && length(b)
        && owner(a).is_some()
        && owner(b).is_some()
        && owner(a) != owner(b)
}
