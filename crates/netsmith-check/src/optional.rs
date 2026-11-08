//! `optional` on a member declaration (ADR 0074): `.max_inductor_dcr: Resistance optional`.
//!
//! A relation that reads an optional member that nothing states is skipped: it is taken out of
//! the model before the solver sees it, so it is neither checked, nor listed as not evaluated,
//! nor does it ask anything of a catalog row. A member that something states is read as any
//! other value is, and the relations that read it are checked, or listed as not evaluated
//! when another value they need is still missing.
//!
//! "Stated" is a test on what is written, as ADR 0069's was, made when the checker has read
//! every statement and joined every pair of interfaces, and before the solve. A member is
//! stated when its class (the members that `~` joins to it, which hold one value) has one
//! that:
//!
//! - an `&=` names, or a `?=` default was applied to;
//! - an equation or an `&=` has alone on its left, when the right side reads no optional
//!   member that is itself unstated;
//! - a bound (`>=`, `<=`) holds against numbers alone (`.max_dcr <= 0.1ohm`).
//!
//! A bound against another field is a check, not a statement: the library's own relation
//! `.inductor.dc_resistance <= .buck.max_dcr` has the member alone on one side, and counting
//! it would make every optional member stated by the relation that reads it. A value that a
//! catalog row or a pick gives is never a statement either.

use std::collections::HashSet;
use std::sync::Arc;

use netsmith_lang::ast::{CmpOp, Decl};

use crate::checker::{Checker, Ty};
use crate::model::{FieldId, Member, OptionalMark, RExpr};

/// The field that a statement states, when it does: one with the field alone on its left
/// (`&=`, `?=`, `==`), or a bound of the field against numbers alone.
pub(crate) fn states(op: CmpOp, lhs: &RExpr, rhs: &RExpr) -> Option<FieldId> {
    let no_fields = |e: &RExpr| {
        let mut f = Vec::new();
        e.fields(&mut f);
        f.is_empty()
    };
    match (op, lhs, rhs) {
        (CmpOp::Narrow | CmpOp::Default | CmpOp::EqEq, RExpr::Field(f), _) => Some(*f),
        (CmpOp::Ge | CmpOp::Le, RExpr::Field(f), r) if no_fields(r) => Some(*f),
        (CmpOp::Ge | CmpOp::Le, l, RExpr::Field(f)) if no_fields(l) => Some(*f),
        _ => None,
    }
}

impl Checker<'_> {
    /// Whether the `optional` of a declaration (if it has one) is allowed where it stands,
    /// reporting it when not. Only a value (a quantity, text or an enum) declared in a part,
    /// a module, an interface or a generic may be optional: those definitions have a user,
    /// who is the one to state the member or to leave it.
    pub(crate) fn optional_ok(&mut self, d: &Decl, ty: &Ty) -> bool {
        let Some(span) = d.optional else {
            return false;
        };
        let member = d.name.to_source();
        let holder = if self.phys.is_some() {
            Some("a physical file")
        } else if self.in_board() {
            Some("a board")
        } else if self.in_system() {
            Some("a system")
        } else if self.in_panel() {
            Some("a panel")
        } else {
            None
        };
        if let Some(holder) = holder {
            self.error(
                span,
                format!(
                    "`optional` is for a member that the user of a definition may leave unstated, and {holder} has no user above it: `{member}` can be stated where it is declared, or left out; remove `optional`"
                ),
            );
            return false;
        }
        let what = match ty {
            Ty::Poison | Ty::Bad => return false,
            Ty::Builtin(crate::checker::Builtin::Quantity(_) | crate::checker::Builtin::Text)
            | Ty::Enum(..) => return true,
            Ty::Builtin(crate::checker::Builtin::Pin) => {
                "a pin; a pin that need not be connected says so in its `pins` row (`optional`)"
                    .to_string()
            }
            Ty::Def(m, i) => {
                let def = &self.modules[*m].defs[*i];
                let word = crate::providers::kind_word(def);
                let article = if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
                    "an"
                } else {
                    "a"
                };
                format!(
                    "{article} {word} (`{}`); mark the values inside it instead",
                    def.name.name
                )
            }
        };
        self.error(
            span,
            format!(
                "`optional` marks a value that a design may leave unstated, but `{member}` is {what}"
            ),
        );
        false
    }

    /// Mark the field that the declaration just made as optional.
    pub(crate) fn mark_optional(&mut self, d: &Decl) {
        let (Some(span), Some(name)) = (d.optional, d.name.segments.first()) else {
            return;
        };
        let cur = self.current();
        if let Some(Member::Field(f)) = self.model.member(cur, &name.name) {
            self.model.fields[f.0].optional = Some(OptionalMark {
                source: self.cur_source(),
                span,
                declared_in: self.cur_def,
            });
        }
    }

    /// A statement that states an optional member in the definition that declares it would
    /// state it for every use, so the checks that read it would never be skipped: refused,
    /// like a `?=` against `required` was (ADR 0069).
    pub(crate) fn refuse_own_statement(
        &mut self,
        field: FieldId,
        text: &str,
        span: netsmith_lang::Span,
    ) {
        let f = self.model.field(field);
        let Some(mark) = f.optional else {
            return;
        };
        // The definition whose text declared the member, not the one that holds the instance:
        // a provider states the optional member of a generic it provides, as a use states
        // the member of an interface it has.
        if mark.declared_in != self.cur_def {
            return;
        }
        let name = f.name.clone();
        let place = self.place_of(mark.source, mark.span);
        self.error(
            span,
            format!(
                "`{name}` is `optional` (at {place}) and is stated here with `{text}`, in the definition that declares it: a statement here would state it for every use, so the checks that read it would never be skipped; keep `optional` and let the design state it, or remove `optional` and keep the statement"
            ),
        );
    }

    /// Take out of the model the relations that read an optional member that nothing states.
    pub(crate) fn skip_unstated_optional(&mut self) {
        let n = self.model.fields.len();
        if !self.model.fields.iter().any(|f| f.optional.is_some()) {
            return;
        }
        // The class of each field: what `~` joins holds one value.
        let mut parent: Vec<usize> = (0..n).collect();
        fn find(parent: &mut [usize], mut x: usize) -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        }
        for &(a, b) in &self.model.equalities {
            let (ra, rb) = (find(&mut parent, a.0), find(&mut parent, b.0));
            if ra != rb {
                parent[ra] = rb;
            }
        }
        let mut stated: HashSet<usize> = HashSet::new();
        let mut needs: HashSet<usize> = HashSet::new();
        let mut defs: Vec<(usize, Vec<usize>)> = Vec::new();
        for i in 0..n {
            let root = find(&mut parent, i);
            let field = &self.model.fields[i];
            if field.optional.is_some() {
                needs.insert(root);
            }
            if self.stated.contains(&FieldId(i)) || !field.narrowed_at.is_empty() {
                stated.insert(root);
            }
        }
        let relations = Arc::clone(&self.model.relations);
        for r in relations.iter() {
            let Some(f) = states(r.op, &r.lhs, &r.rhs) else {
                continue;
            };
            let root = find(&mut parent, f.0);
            let mut read = Vec::new();
            r.rhs.fields(&mut read);
            if matches!(r.op, CmpOp::Ge | CmpOp::Le) || read.is_empty() {
                stated.insert(root);
            } else {
                // A member worked out from others is itself waiting on them.
                needs.insert(root);
                let reads = read.iter().map(|g| find(&mut parent, g.0)).collect();
                defs.push((root, reads));
            }
        }
        // A member worked out from members that are stated, or that need no statement, is
        // stated; one worked out from an optional member nothing states is not.
        loop {
            let mut changed = false;
            for (root, reads) in &defs {
                if !stated.contains(root)
                    && reads
                        .iter()
                        .all(|g| !needs.contains(g) || stated.contains(g))
                {
                    stated.insert(*root);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let unstated: HashSet<FieldId> = (0..n)
            .filter(|&i| {
                self.model.fields[i].optional.is_some() && {
                    let root = find(&mut parent, i);
                    !stated.contains(&root)
                }
            })
            .map(FieldId)
            .collect();
        if unstated.is_empty() {
            return;
        }
        Arc::make_mut(&mut self.model.relations).retain(|r| {
            let mut fields = Vec::new();
            r.lhs.fields(&mut fields);
            r.rhs.fields(&mut fields);
            !fields.iter().any(|f| unstated.contains(f))
        });
    }
}
