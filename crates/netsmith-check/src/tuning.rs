//! A position that is not fitted until it is tuned (issue #1273): a passive that states
//! `.fitted &= false`, and often `.tune &= 1pF..10pF`, the range its value is chosen in on the
//! bench (a shunt of an RF matching network).
//!
//! The members are the library's (`Resistor`, `Capacitor` and `Inductor` in
//! `std/parts/passive.nsm`); what this pass does is the one rule the library cannot say: a part
//! that is not fitted and whose value the design does not state is given no catalog row. Its
//! catalog is taken out of the model, so the solver picks nothing for it and `fab` resolves
//! nothing, and its value is narrowed to its `.tune`, so every relation that reads the value is
//! judged over the whole range the bench may choose from. A part that is not fitted and states
//! its value (`.capacitance &= 4.7pF`) is picked as any other part is, and its row is the value
//! to start tuning from. The library's bounds `.capacitance >= min(.tune)` and
//! `.capacitance <= max(.tune)` check a stated value against the range.
//!
//! "Stated" is the test of `optional` (`optional.rs`): an `&=`, a `?=` default that applied, an
//! equation or `&=` with the value alone on its left, or a bound against numbers alone, on the
//! value or on a member `~` joins to it. The library's bounds against `.tune` are checks, not
//! statements.
//!
//! The value is the first column of the part's catalog that has the dimension of `.tune`
//! (`.resistance`, `.capacitance`, `.inductance`). A part that is not fitted, has no row and no
//! `.package` has no footprint, so that is refused here with what to write.
//!
//! What is read is the value `.fitted` has when the checker is done, which a `&=` or a `?=` of
//! numbers alone gives it. A `.fitted` that a module ties to a value of its own
//! (`.shunt.fitted &= .tuned`) is known only to the solver: such a part is picked, and the
//! design stage still marks it not fitted.

use std::collections::HashSet;

use netsmith_lang::ast::CmpOp;

use crate::checker::Checker;
use crate::model::{Domain, FieldId, InstId, InstKind, Member, RExpr, Relation};
use crate::optional::states;

impl Checker<'_> {
    /// Take the catalog out of every part that is not fitted and states no value, and narrow
    /// its value to its `.tune`.
    pub(crate) fn leave_tuned_parts_unpicked(&mut self) {
        let field_of = |model: &crate::model::Model, part: InstId, name: &str| match model
            .member(part, name)
        {
            Some(Member::Field(f)) => Some(f),
            _ => None,
        };
        let mut open: Vec<(InstId, FieldId, FieldId, FieldId)> = Vec::new();
        for (id, inst) in self.model.instances() {
            if inst.kind != InstKind::Part {
                continue;
            }
            let Some(catalog) = &inst.catalog else {
                continue;
            };
            let (Some(fitted), Some(tune)) = (
                field_of(&self.model, id, "fitted"),
                field_of(&self.model, id, "tune"),
            ) else {
                continue;
            };
            if !matches!(&self.model.field(fitted).domain, Domain::Text(t) if t == "false") {
                continue;
            }
            let kind = self.model.field(tune).kind;
            let Some(&value) = catalog
                .fields
                .iter()
                .find(|&&f| self.model.field(f).kind == kind)
            else {
                continue;
            };
            open.push((id, fitted, tune, value));
        }
        for (part, fitted, tune, value) in open {
            if self.value_stated(value) {
                continue;
            }
            let at = self
                .model
                .field(fitted)
                .narrowed_at
                .last()
                .copied()
                .or(self.model.instance(part).declared)
                .unwrap_or((
                    self.model.instance(part).source,
                    self.model.instance(part).span,
                ));
            let path = self.model.path_of(part);
            let value_name = self.model.field(value).name.clone();
            let package = field_of(&self.model, part, "package");
            if let Some(package) = package
                && self.model.field(package).domain == Domain::Unset
                && !self.value_stated(package)
            {
                self.report(
                    at.0,
                    at.1,
                    format!(
                        "`{path}` is not fitted and states no `.{value_name}`, so no catalog row is picked for it and nothing gives its footprint: state its package (`.{path}.package &= \"0402\"`), or the value to start tuning from (`.{path}.{value_name} &= ...`), which is then picked"
                    ),
                );
            }
            self.model.instances[part.0].catalog = None;
            self.model.push_relation(Relation {
                lhs: RExpr::Field(value),
                op: CmpOp::Narrow,
                rhs: RExpr::Field(tune),
                source: at.0,
                span: at.1,
                written: None,
                own: true,
            });
        }
    }

    /// Whether a statement of the design states `field`, or a member `~` joins to it (the test
    /// of `optional`, `optional.rs`).
    fn value_stated(&self, field: FieldId) -> bool {
        let mut class: HashSet<FieldId> = HashSet::from([field]);
        loop {
            let before = class.len();
            for &(a, b) in &self.model.equalities {
                if class.contains(&a) || class.contains(&b) {
                    class.insert(a);
                    class.insert(b);
                }
            }
            if class.len() == before {
                break;
            }
        }
        class
            .iter()
            .any(|f| self.stated.contains(f) || !self.model.fields[f.0].narrowed_at.is_empty())
            || self
                .model
                .relations
                .iter()
                .any(|r| states(r.op, &r.lhs, &r.rhs).is_some_and(|f| class.contains(&f)))
    }
}
