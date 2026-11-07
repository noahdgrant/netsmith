//! Which fields the design reads, worked out once for the relations the model has.
//!
//! `fits` asks of each column of each row of a catalog whether the design says anything of
//! its field, and `pick_parts` asks it of each column of each part. Each answer was a scan of
//! every equality, every balance and every relation of the model, so the cost of a search
//! grew with the rows tried times the size of the design. The answers depend only on the
//! lists, which change at the two moments the solver adds its bounds and takes them out
//! (`Model::push_relation`, `Model::truncate_relations`), so they are kept for the list of
//! relations they were worked out from and made again when it is another.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use netsmith_check::{Domain, FieldId, InstId, InstKind, Model, RExpr, Relation};

use crate::eval::{Val, eval, fields_in};

/// The domains a right side was worked out from, and what it came to.
type Worked = (Vec<Domain>, Option<Val>);

pub(super) struct Reads {
    /// The list of relations this was worked out from.
    relations: Arc<Vec<Relation>>,
    /// The fields that a relation names.
    in_relation: HashSet<FieldId>,
    /// The fields that a relation names together with another field.
    with_another: HashSet<FieldId>,
    /// The fields that a connection joins to another field.
    equated: HashSet<FieldId>,
    /// The fields that a balance of currents sums.
    summed: HashSet<FieldId>,
    /// The fields of each relation's right side, each once, in order.
    rhs_fields: Vec<Vec<FieldId>>,
    /// The fields of each relation, each once, in order.
    fields_of: Vec<Vec<FieldId>>,
    /// For each field, the balances of the model's list that sum it and are the supply
    /// balances of a board or module: not a return net, not a join between boards.
    supply_balances: HashMap<FieldId, Vec<usize>>,
    /// For each instance, the relations (by place, in order) that name a field of it or of
    /// something inside it.
    inside: HashMap<InstId, Vec<usize>>,
    /// For each field, the relations that name it.
    by_field: HashMap<FieldId, Vec<usize>>,
    /// The fields that decide what an equation of `propagate` does: its left side and its
    /// right side's fields.
    inputs: Vec<Vec<FieldId>>,
    /// For each relation, the last value its right side was worked out to, with the domains of
    /// its fields that it was worked out from (`value_of_rhs`).
    memo: RefCell<Vec<Option<Worked>>>,
}

impl Reads {
    pub(super) fn of(model: &Model) -> Self {
        let mut in_relation = HashSet::new();
        let mut with_another = HashSet::new();
        let mut fields = Vec::new();
        let mut rhs_fields = Vec::new();
        let mut fields_of = Vec::new();
        let mut inputs = Vec::new();
        let mut inside: HashMap<InstId, Vec<usize>> = HashMap::new();
        let mut by_field: HashMap<FieldId, Vec<usize>> = HashMap::new();
        for (index, r) in model.relations().iter().enumerate() {
            fields.clear();
            fields_in(&r.rhs, &mut fields);
            fields.sort();
            fields.dedup();
            rhs_fields.push(fields.clone());
            if let RExpr::Field(f) = &r.lhs {
                fields.push(*f);
            }
            inputs.push(fields.clone());
            fields.clear();
            fields_in(&r.lhs, &mut fields);
            fields_in(&r.rhs, &mut fields);
            let mut once = fields.clone();
            once.sort();
            once.dedup();
            for &f in &once {
                by_field.entry(f).or_default().push(index);
                let mut at = Some(model.field(f).owner);
                while let Some(inst) = at {
                    let list = inside.entry(inst).or_default();
                    if list.last() != Some(&index) {
                        list.push(index);
                    }
                    at = model.instance(inst).parent;
                }
            }
            fields_of.push(once);
            in_relation.extend(fields.iter().copied());
            // Every field of a relation that names two different fields has another; a relation
            // that names one field only, any number of times, gives none.
            if let Some(&first) = fields.first()
                && fields.iter().any(|&g| g != first)
            {
                with_another.extend(fields.iter().copied());
            }
        }
        let equated = model
            .equalities()
            .iter()
            .flat_map(|&(a, b)| [a, b])
            .collect();
        let summed = model
            .balances()
            .iter()
            .flat_map(|b| b.draws.iter().map(|d| d.field))
            .collect();
        let mut supply_balances: HashMap<FieldId, Vec<usize>> = HashMap::new();
        for (index, b) in model.balances().iter().enumerate() {
            if b.returns || model.instance(b.scope).kind == InstKind::System {
                continue;
            }
            for d in &b.draws {
                let list = supply_balances.entry(d.field).or_default();
                if list.last() != Some(&index) {
                    list.push(index);
                }
            }
        }
        Self {
            relations: Arc::clone(model.shared_relations()),
            in_relation,
            with_another,
            equated,
            summed,
            memo: RefCell::new(vec![None; rhs_fields.len()]),
            rhs_fields,
            fields_of,
            supply_balances,
            inside,
            by_field,
            inputs,
        }
    }

    /// Whether this was worked out from the relations the model has now.
    pub(super) fn is_of(&self, model: &Model) -> bool {
        Arc::ptr_eq(&self.relations, model.shared_relations())
    }

    /// What the right side of relation `index` (`r`) works out to over the domains now, or
    /// `None` when it does not work out. The value depends on nothing but the domains of the
    /// fields of the expression, so what was worked out the last time for the same domains is
    /// the answer, and `propagate` asks again for every relation at every step of a search.
    pub(super) fn value_of_rhs(&self, model: &Model, index: usize, r: &Relation) -> Option<Val> {
        let domains: Vec<Domain> = self.rhs_fields[index]
            .iter()
            .map(|&f| model.field(f).domain.clone())
            .collect();
        if let Some((seen, value)) = &self.memo.borrow()[index]
            && *seen == domains
        {
            return value.clone();
        }
        let value = eval(model, &r.rhs).ok();
        self.memo.borrow_mut()[index] = Some((domains, value.clone()));
        value
    }

    /// The supply balances that sum `field`, by place in the model's list, in order.
    pub(super) fn supply_balances_of(&self, field: FieldId) -> &[usize] {
        self.supply_balances.get(&field).map_or(&[], Vec::as_slice)
    }

    /// The relations that name a field of `part` or of something inside it, or one of the
    /// `fields`, in order, each once.
    pub(super) fn touching(
        &self,
        part: InstId,
        fields: impl Iterator<Item = FieldId>,
    ) -> Vec<usize> {
        let mut found: Vec<usize> = self.inside.get(&part).cloned().unwrap_or_default();
        for f in fields {
            if let Some(list) = self.by_field.get(&f) {
                found.extend_from_slice(list);
            }
        }
        found.sort_unstable();
        found.dedup();
        found
    }

    /// The fields that decide what `propagate` does with relation `index`.
    pub(super) fn inputs_of(&self, index: usize) -> &[FieldId] {
        &self.inputs[index]
    }

    /// The fields that relation `index` names, each once.
    pub(super) fn fields_of(&self, index: usize) -> &[FieldId] {
        &self.fields_of[index]
    }

    /// Whether a connection, a balance or a relation names the field.
    pub(super) fn read(&self, field: FieldId) -> bool {
        self.equated.contains(&field)
            || self.summed.contains(&field)
            || self.in_relation.contains(&field)
    }

    /// Whether a connection, a balance, or a relation that names another field too, names
    /// the field.
    pub(super) fn shared(&self, field: FieldId) -> bool {
        self.equated.contains(&field)
            || self.summed.contains(&field)
            || self.with_another.contains(&field)
    }
}
