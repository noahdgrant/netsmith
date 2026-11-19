//! The `soft margin` statement of a physical file (issue 1310): `soft margin 3mm round .u1`.
//!
//! **Decision** (owner, issue 1310). A hardware design note that says to keep 3 mm round a part
//! free of other parts (soldering, rework, an antenna) is a soft requirement of the physical file:
//! the placer keeps the courtyards of other parts out of the margin where it can, and a margin
//! it could not keep is a warning, an error with `--strict`. It is neither a courtyard nor a
//! copper keepout, so it is no `.routing_margin` (a hard rule of the placer) and no `Keepout`.
//!
//! **Readings** (the implementer's). The words `soft`, `margin` and `round` are read as words
//! only at the start of the statement (`soft` begins one only when `margin` follows), so they stay
//! usable as names. The distance is one fixed length of zero or more, in the same terms as a
//! polygon's: a range, a tolerance or a relation is refused. The part is a part (not a module:
//! the margin is measured from a part's courtyard); a part states one margin, in a board's file (not in a module's own file, which states only how its parts stand to one another). The checker keeps
//! the part and the distance on the model; the design stage lists them, the placer pays for a
//! courtyard inside the margin, and the checks measure it on the placed board.

use netsmith_lang::ast::SoftMargin;

use crate::checker::{Checker, Res};
use crate::model::{InstKind, SoftMargin as Stated};

impl Checker<'_> {
    /// Keep a `soft margin` statement on the model. Every refusal names the statement and what
    /// to write instead.
    pub(crate) fn soft_margin(&mut self, m: &SoftMargin) {
        let text = m.span_text();
        if self.phys.is_none() {
            self.error(
                m.span,
                "a `soft margin` belongs in a physical file (`physical for Board { soft margin 3mm round .u1 }`, ADR 0048); the circuit source holds no placement",
            );
            return;
        }
        if self.phys.as_ref().is_some_and(|p| p.module.is_some()) {
            self.error(
                m.span,
                "a module's own physical file states how its parts stand to one another, and a `soft margin` is kept by the board: state it in the board's physical file (`soft margin 3mm round .radio.u1`)",
            );
            return;
        }
        let Some(margin) = self.fixed_length(&m.margin, &text, "margin", "3mm") else {
            return;
        };
        let part = match self.resolve_path(&m.part) {
            Res::Inst(i) if self.model.instance(i).kind == InstKind::Part => i,
            Res::Inst(i) => {
                let kind = self.model.instance(i).kind.word();
                self.error(
                    m.part.span,
                    format!(
                        "`{text}` names `{}`, which is a {kind}; a soft margin is kept round a part's courtyard, so name the part (`soft margin 3mm round .u1`)",
                        m.part.to_source()
                    ),
                );
                return;
            }
            Res::Bad => return,
            _ => {
                self.error(
                    m.part.span,
                    format!(
                        "`{text}` names `{}`, which is not a part; a soft margin is kept round a part (`soft margin 3mm round .u1`)",
                        m.part.to_source()
                    ),
                );
                return;
            }
        };
        if self.model.soft_margins.iter().any(|s| s.part == part) {
            self.error(
                m.span,
                format!(
                    "`{}` already has a `soft margin`; a part keeps one distance, so state the larger one",
                    m.part.to_source()
                ),
            );
            return;
        }
        let source = self.cur_source();
        self.model.soft_margins.push(Stated {
            part,
            margin,
            source,
            span: m.span,
        });
    }
}
