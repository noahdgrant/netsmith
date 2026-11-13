//! The `support` section of a part (ADR 0115): the circuit its datasheet's application circuit
//! puts around it (a bootstrap capacitor, the resistor on `RT`, the capacitors on its supply
//! pins), which is placed wherever the part is.
//!
//! ```text
//! part Lt8641 provides BuckIc {
//!     ...
//!     support {
//!         .boost: Capacitor { .capacitance &= 100nF +/- 20% }
//!         .bst ~> .boost ~> .sw.pos
//!     }
//! }
//! ```
//!
//! A support part is a member of the part, at the path below it (`reg.ic.boost`), and is
//! otherwise a part like any other: picked, checked, drawn, placed and listed. The section holds
//! parts and the statements about them (narrowings, relations, connections to the part's pins);
//! values, pins and tables belong to the part itself. Only a part's own definition holds one; a
//! part declares no part outside it, so that the circuit a part carries is in one place.

use netsmith_lang::ast::{Decl, DefKind, Item, Support};

use crate::checker::{Checker, Ty};
use crate::model::InstKind;
use crate::providers::generic_is_part_shaped;

/// The items of a section that are read: declarations, statements and connections.
pub(crate) fn wiring(support: &Support) -> Vec<Item> {
    support
        .items
        .iter()
        .filter(|i| {
            matches!(
                i,
                Item::Decl(_) | Item::Constraint(_) | Item::Connect(_) | Item::JoinMap(_)
            )
        })
        .cloned()
        .collect()
}

impl Checker<'_> {
    /// Whether the definition being read is a part's own, the one place a `support` section may be.
    pub(crate) fn support_allowed(&self) -> bool {
        self.in_kind(DefKind::Part) && self.in_own_definition()
    }

    /// Whether a type is placed as a part.
    fn part_like(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Def(m, i) => {
                let def = &self.modules[*m].defs[*i];
                def.kind == DefKind::Part
                    || (def.kind == DefKind::Generic && generic_is_part_shaped(def))
            }
            _ => false,
        }
    }

    /// A `support` section: its parts are declared as members of the part.
    pub(crate) fn declare_support(&mut self, support: &Support) {
        if !self.support_allowed() {
            let holder = self.model.instance(self.current()).kind;
            let what = if self.in_kind(DefKind::Generic) {
                "a generic".to_string()
            } else if holder == InstKind::Part {
                "the block of a member".to_string()
            } else {
                format!("a {}", holder.word())
            };
            self.error(
                support.span,
                format!(
                    "`support` can only appear directly inside a part, for the circuit its datasheet puts around it, and this is {what}; a module or a board places such parts as members"
                ),
            );
            return;
        }
        for item in &support.items {
            let span = match item {
                Item::Decl(_) | Item::Constraint(_) | Item::Connect(_) | Item::JoinMap(_) => {
                    continue;
                }
                Item::Pins(t) => t.span,
                Item::Catalog(c) => c.span,
                Item::Bridge(b) => b.span,
                Item::Dc(d) => d.span,
                Item::Tie(t) => t.span,
                Item::Support(s) => s.span,
                _ => support.span,
            };
            self.error(
                span,
                "a `support` section holds parts and the statements that wire them to the part; this belongs in the part itself",
            );
        }
        let outer = std::mem::replace(&mut self.in_support, true);
        let items = wiring(support);
        self.declare(&items);
        self.in_support = outer;
    }

    /// Whether a member may be declared here: in a `support` section only parts are, and in a
    /// part's own definition outside one, no part is.
    pub(crate) fn fits_support(&mut self, ty: &Ty, d: &Decl) -> bool {
        let name = d.name.to_source();
        let ty_name = d.ty.to_source();
        let part = self.part_like(ty);
        if self.in_support && self.support_allowed() {
            if part || matches!(ty, Ty::Poison | Ty::Bad) {
                return true;
            }
            self.error(
                d.ty.segments[0].span,
                format!(
                    "a `support` section holds the parts of the part's application circuit, and `{name}: {ty_name}` is not a part; declare it in the part itself"
                ),
            );
            return false;
        }
        if part && self.support_allowed() {
            self.error(
                d.ty.segments[0].span,
                format!(
                    "a part holds other parts only in its `support` section, the circuit its datasheet puts around it: move `{name}: {ty_name}` into `support {{ ... }}`"
                ),
            );
            return false;
        }
        true
    }
}
