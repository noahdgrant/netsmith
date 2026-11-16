//! The `layout` block of a part (ADR 0030, issue #829): what the part's datasheet says about
//! where the parts around it go, with the document that says it.
//!
//! ```text
//! part Lmr16006y provides BuckIc {
//!     ...
//!     layout {
//!         source "SNVSA24, 11.1 Layout Guidelines, p.17"
//!         /// "The input bypass capacitor Cin must be placed close to the VIN pin."
//!         near input_cap bridging .vin.pos .vin.neg to .vin.pos
//!     }
//! }
//! ```
//!
//! A statement names the part's own pins; the board's parts it is about are found by the nets
//! when the board is placed (`netsmith_pcb::near_pairs`), so the part names no other part and
//! no net. The `source` is mandatory: a guideline is knowledge of a part, written once with the
//! document it comes from (ADR 0030, the second of its three places). Only a part's own
//! definition holds a block, as only it holds a `support` section: a generic picks parts from
//! a catalog, whose datasheets differ.

use netsmith_core::PartKind;
use netsmith_lang::ast::{Layout, LayoutStatement, Near, Select};
use netsmith_quantity::Dimension;

use crate::checker::Checker;
use crate::lower::VTy;
use crate::model::{LayoutNear, NearSelect, RExpr};

impl Checker<'_> {
    /// A `layout` block: its `near` statements become [`LayoutNear`]s of the part instance.
    pub(crate) fn layout(&mut self, block: &Layout) {
        if !self.support_allowed() {
            let what = if self.in_kind(netsmith_lang::ast::DefKind::Generic) {
                "a generic, whose catalog parts each have their own datasheet"
            } else {
                "not a part's own definition"
            };
            self.error(
                block.span,
                format!(
                    "`layout` can only appear directly inside a part, for the guidelines of its datasheet, and this is {what}"
                ),
            );
            return;
        }
        let sources: Vec<(&str, netsmith_lang::Span)> = block
            .statements
            .iter()
            .filter_map(|s| match s {
                LayoutStatement::Source { text, span } => Some((text.as_str(), *span)),
                LayoutStatement::Near(_) => None,
            })
            .collect();
        let source = match sources.as_slice() {
            [] => {
                self.error(
                    block.span,
                    "this `layout` block states no source: add `source \"...\"` with the document and the section that give its guidelines, such as `source \"SNVSA24, 11.1 Layout Guidelines, p.17\"`",
                );
                return;
            }
            [(text, span)] if text.trim().is_empty() => {
                self.error(
                    *span,
                    "the `source` of a `layout` block is empty: name the document and the section that give its guidelines",
                );
                return;
            }
            [(text, _)] => text.to_string(),
            [_, (_, second), ..] => {
                self.error(
                    *second,
                    "a `layout` block has one `source`; put the guidelines of another document in a `layout` block of their own",
                );
                return;
            }
        };
        for statement in &block.statements {
            if let LayoutStatement::Near(near) = statement {
                self.near(near, &source);
            }
        }
    }

    fn near(&mut self, near: &Near, source: &str) {
        let part = self.current();
        let site = self.place_of(self.cur_source(), near.span);
        let name = near.name.name.clone();
        if let Some(other) = self
            .model
            .layout
            .iter()
            .find(|l| l.part == part && l.name == name)
        {
            if other.site != site {
                let message = format!(
                    "the part already states a guideline `{name}` (at {}); give this one another name",
                    other.site
                );
                self.error(near.name.span, message);
            }
            return;
        }
        let select = match &near.select {
            Select::Bridging(a, b) => {
                let (a, b) = (self.part_pin(a), self.part_pin(b));
                let (Some(a), Some(b)) = (a, b) else { return };
                if a == b {
                    self.error(
                        near.span,
                        "`bridging` names two different pins: the part with a pad on the net of each",
                    );
                    return;
                }
                NearSelect::Bridging(a, b)
            }
            Select::On { pin, kind } => {
                let Some(pin) = self.part_pin(pin) else {
                    return;
                };
                let kind = match kind {
                    None => None,
                    Some(k) => match PartKind::from_word(&k.name) {
                        Some(found) => Some(found),
                        None => {
                            let known: Vec<&str> = PartKind::ALL.iter().map(|k| k.word()).collect();
                            self.error(
                                k.span,
                                format!(
                                    "there is no kind `{}`; the kinds are {}",
                                    k.name,
                                    known.join(", ")
                                ),
                            );
                            return;
                        }
                    },
                };
                NearSelect::On { pin, kind }
            }
        };
        let Some(to) = self.part_pin(&near.to) else {
            return;
        };
        let within = match &near.within {
            None => None,
            Some(e) => {
                let lowered = self.lower(e);
                let Some(t) = lowered else { return };
                let length = match (&t.e, t.ty) {
                    (RExpr::Const(q), VTy::Q(d)) if d == Dimension::LENGTH && q.lo() == q.hi() => {
                        Some(q.lo())
                    }
                    _ => None,
                };
                match length {
                    Some(metres) if metres > 0.0 => Some(metres * 1000.0),
                    _ => {
                        self.error(
                            e.span(),
                            format!(
                                "`within` of `near {name}` is one length above zero, such as `2mm`, and `{}` is not",
                                e.to_source()
                            ),
                        );
                        return;
                    }
                }
            }
        };
        self.model.layout.push(LayoutNear {
            part,
            name,
            source: source.to_string(),
            site,
            select,
            to,
            within,
        });
    }
}
