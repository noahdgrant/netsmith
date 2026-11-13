//! The `polygon` statement of an `Outline` in a physical file (issue #57): the board edge as an
//! ordered run of corners, `polygon (0mm, 0mm) (40mm, 0mm) round 2mm (40mm, 15mm) arc through
//! (30mm, 20mm) (0mm, 15mm)`.
//!
//! **Decisions** (owner, 2026-11-12, the comment on issue #57). The form is the one board
//! outlines have elsewhere: KiCad's `Edge.Cuts` lines and arcs, IPC-2581's `Polygon`, the ODB++
//! profile, an ordered run of corner points. `round R` after a corner is a tangent fillet there;
//! `arc through (x, y)` after a corner makes the edge to the next corner an arc through that
//! point, as KiCad's `gr_arc` gives one by its start, middle and end. Every value is fixed: a
//! range, a tolerance or a relation in a point is refused, because the outline is what the case
//! drawing says and the placer does not solve for it. `dxf import` keeps writing source (ADR
//! 0045 decision 8): a polygon is what it prints for an outline that is not a rounded rectangle.
//!
//! **Readings.** The words `round`, `arc` and `through` are read as words only inside a
//! `polygon` statement, and `polygon` begins one only when `(` follows, so all four stay usable
//! as names. The statement belongs in the body of the `Outline` of a physical file; anywhere
//! else it is refused. The checker keeps the numbers (in mm) on the model; the design stage
//! turns the fillets into arcs, checks the shape and refuses `.width` and `.height` beside it.

use netsmith_lang::ast::{BinOp, Expr, Point, Polygon};

use crate::checker::Checker;
use crate::lower::VTy;
use crate::model::{OutlinePolygon, PolygonCorner, RExpr};
use netsmith_quantity::{Dimension, Quantity};

/// What a value of a point is, when it is not one fixed length.
enum NotFixed {
    /// It reads a field: a relation.
    Relation,
    /// A range or a tolerance.
    Range(Quantity),
    /// Not a length.
    Kind,
    /// Already reported (the lowering said why).
    Reported,
}

/// The value of an expression of constants, in metres, or why it is not one.
fn fixed(e: &RExpr) -> Result<Quantity, NotFixed> {
    match e {
        RExpr::Const(q) => Ok(*q),
        RExpr::Neg(a) => Ok(fixed(a)?.neg()),
        RExpr::Bin(op, a, b) => {
            let (a, b) = (fixed(a)?, fixed(b)?);
            let r = match op {
                BinOp::Add => a.add(&b),
                BinOp::Sub => a.sub(&b),
                BinOp::Mul => a.mul(&b),
                BinOp::Div => a.div(&b),
            };
            r.map_err(|_| NotFixed::Kind)
        }
        RExpr::Field(_) | RExpr::End(..) => Err(NotFixed::Relation),
        RExpr::Range(..) => Err(NotFixed::Relation),
        RExpr::Text(_) => Err(NotFixed::Kind),
    }
}

impl Checker<'_> {
    /// Keep a `polygon` statement's corners on the model, for the `Outline` it is in. Every
    /// refusal names the value and what to write instead.
    pub(crate) fn polygon(&mut self, p: &Polygon) {
        let cur = self.current();
        let inst = self.model.instance(cur);
        if self.phys.is_none() || inst.type_name != "Outline" {
            self.error(
                p.span,
                "a `polygon` is the board edge, and it belongs in the board's `.outline: Outline { ... }` in the `case` section of its physical file (issue #57)",
            );
            return;
        }
        if self.model.polygon_of(cur).is_some() {
            self.error(
                p.span,
                "the outline already has a `polygon`, and a board has one edge",
            );
            return;
        }
        let mut corners = Vec::new();
        let mut ok = true;
        for c in &p.corners {
            let at = self.point(&c.at, "a corner");
            let round = c
                .round
                .as_ref()
                .and_then(|r| self.length(r, "a fillet radius"));
            let through = c
                .through
                .as_ref()
                .map(|t| self.point(t, "the point an arc passes through"));
            if let Some(r) = round
                && r <= 0.0
            {
                self.error(
                    c.round.as_ref().map_or(c.span, Expr::span),
                    format!("a fillet radius is more than 0 mm, and `round {r}mm` is not; leave `round` out for a sharp corner"),
                );
                ok = false;
            }
            match (at, through) {
                (Some(at), None) => corners.push(PolygonCorner {
                    at,
                    round,
                    through: None,
                }),
                (Some(at), Some(Some(t))) => corners.push(PolygonCorner {
                    at,
                    round,
                    through: Some(t),
                }),
                _ => ok = false,
            }
            ok &= c.round.is_none() || round.is_some();
        }
        if !ok {
            return;
        }
        if corners.len() < 3 {
            self.error(
                p.span,
                format!(
                    "a `polygon` has at least three corners, and this one has {}",
                    corners.len()
                ),
            );
            return;
        }
        let source = self.cur_source();
        self.model.polygons.push(OutlinePolygon {
            outline: cur,
            corners,
            source,
            span: p.span,
        });
    }

    /// `(x, y)` in mm, or `None` after reporting why not.
    fn point(&mut self, p: &Point, what: &str) -> Option<(f64, f64)> {
        let x = self.length(&p.x, what);
        let y = self.length(&p.y, what);
        Some((x?, y?))
    }

    /// One fixed length in mm, or `None` after reporting why not.
    fn length(&mut self, e: &Expr, what: &str) -> Option<f64> {
        let text = e.to_source();
        let why = match self.lower(e) {
            None => NotFixed::Reported,
            Some(t) if t.ty != VTy::Q(Dimension::LENGTH) => NotFixed::Kind,
            Some(t) => match fixed(&t.e) {
                // Arithmetic rounds outward by a hair; that is still one value.
                Ok(q) if q.hi() - q.lo() < 1e-12 => return Some((q.mid() * 1e12).round() / 1e9),
                Ok(q) => NotFixed::Range(q),
                Err(why) => why,
            },
        };
        let message = match why {
            NotFixed::Reported => return None,
            NotFixed::Kind => {
                format!("{what} of a `polygon` is a length such as `12mm`, and `{text}` is not")
            }
            NotFixed::Range(q) => {
                let mm = |v: f64| {
                    let s = format!("{:.4}", v * 1e3);
                    s.trim_end_matches('0').trim_end_matches('.').to_string()
                };
                format!(
                    "{what} of a `polygon` is one value, and `{text}` is a range ({}mm..{}mm): the outline is what the case drawing says, so write the one number",
                    mm(q.lo()),
                    mm(q.hi())
                )
            }
            NotFixed::Relation => format!(
                "{what} of a `polygon` is a fixed number, and `{text}` reads another value: write the number the case drawing gives"
            ),
        };
        self.error(e.span(), message);
        None
    }
}
