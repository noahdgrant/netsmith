//! A bound on a value that a part's field is held to reaches the part (ADR 0065, stage 0).
//!
//! `.top.resistance &= .balance_resistance` links the member to the part's field in one
//! direction only: a value of the member flows into the field. A bound on the member
//! (`.balance_resistance >= 20kohm`) is a relation and not a value, so it flowed nowhere and the
//! pick never saw it. The bound says that the member lies in a set, and `x &= g` puts `x` inside
//! `g`, so it is a bound on `x` too. The solver adds that bound as a relation of its own, before
//! the picks, and takes it out of the model it returns.
//!
//! It adds a refusal and never a value: no domain, range or pick changes, except that a candidate
//! or a state that breaks a bound the design wrote is refused where it is picked (or, for a part
//! that is not a pick, by the check after the picks).

use std::collections::{HashMap, HashSet};

use netsmith_check::{Domain, FieldId, FieldKind, RExpr, Relation};
use netsmith_lang::ast::CmpOp;

use super::Solver;
use crate::eval::fields_in;

/// Why a relation the solver added exists: the statements that link the part's field to the
/// member (`x &= g`, from the part outward along a chain), and the bound the design wrote.
pub(super) struct Transfer {
    /// The indices, in the design's own relations, of the links `x &= g1`, `g1 &= g2`, ...
    links: Vec<usize>,
    /// The index of the design's bound.
    bound: usize,
}

impl Transfer {
    /// The index of the design's own bound that this relation stands for.
    pub(super) fn bound(&self) -> usize {
        self.bound
    }
}

impl Solver<'_> {
    /// Work out the relations that the design's bounds entail for the fields held to the
    /// bounded one. They are kept apart from the model (`add_transfers` puts them in), so that the
    /// first propagations and the starts of circles read the design's relations alone.
    pub(super) fn find_transfers(&mut self) {
        let relations = self.model.relations().to_vec();
        self.original_relations = relations.len();
        let quantity = |s: &Self, f: FieldId| {
            matches!(s.model.field(f).kind, FieldKind::Quantity(_)) && !s.model.is_place_field(f)
        };
        // The fields held to each field by a link `x &= g` (or `x == g` when `x` has no value of
        // its own: an equation puts the right side inside the left's allowance, not the other way
        // round, so a left side with a range of its own says nothing about the right).
        let mut held: HashMap<FieldId, Vec<(FieldId, usize)>> = HashMap::new();
        for (i, r) in relations.iter().enumerate() {
            let (RExpr::Field(x), RExpr::Field(g)) = (&r.lhs, &r.rhs) else {
                continue;
            };
            if x == g || !matches!(r.op, CmpOp::Narrow | CmpOp::EqEq) {
                continue;
            }
            if !quantity(self, *x) || !quantity(self, *g) {
                continue;
            }
            if r.op == CmpOp::EqEq && !matches!(self.domain(*x), Domain::Unset) {
                continue;
            }
            held.entry(*g).or_default().push((*x, i));
        }
        if held.is_empty() {
            return;
        }
        for (j, b) in relations.iter().enumerate() {
            // The bound each side that is a bare field is under: `g >= e` bounds `g` below by `e`
            // and `e` above by `g`.
            let sides: [(&RExpr, &RExpr, CmpOp); 2] = match b.op {
                CmpOp::Ge => [(&b.lhs, &b.rhs, CmpOp::Ge), (&b.rhs, &b.lhs, CmpOp::Le)],
                CmpOp::Le => [(&b.lhs, &b.rhs, CmpOp::Le), (&b.rhs, &b.lhs, CmpOp::Ge)],
                _ => continue,
            };
            for (field, other, op) in sides {
                let RExpr::Field(g) = field else { continue };
                // A member that has a value is judged on that value, as before: the bound is
                // checked against it and the value reaches the part by `propagate`.
                if !matches!(self.domain(*g), Domain::Unset) || !held.contains_key(g) {
                    continue;
                }
                let mut read = Vec::new();
                fields_in(other, &mut read);
                let mut seen: HashSet<FieldId> = HashSet::from([*g]);
                let mut todo: Vec<(FieldId, Vec<usize>)> = vec![(*g, Vec::new())];
                while let Some((h, above)) = todo.pop() {
                    for &(x, via) in held.get(&h).into_iter().flatten() {
                        if !seen.insert(x) || read.contains(&x) {
                            continue;
                        }
                        let mut links = vec![via];
                        links.extend(above.iter().copied());
                        self.extras.push((
                            Relation {
                                lhs: RExpr::Field(x),
                                op,
                                rhs: other.clone(),
                                source: b.source,
                                span: b.span,
                                written: None,
                                own: false,
                            },
                            Transfer {
                                links: links.clone(),
                                bound: j,
                            },
                        ));
                        // The chain goes on through a field that has no value of its own.
                        if matches!(self.domain(x), Domain::Unset) {
                            todo.push((x, links));
                        }
                    }
                }
            }
        }
    }

    /// The design's bounds whose effect on a part was judged: the bound that the solver added
    /// from them passed or failed once the model was solved. Such a bound is a statement about the
    /// member it bounds, made through the part, and is not a check that waits (ADR 0065).
    pub(super) fn bounds_judged_on_parts(&self) -> std::collections::HashSet<usize> {
        self.extras
            .iter()
            .filter(|(r, _)| !matches!(self.verify(r), super::Verdict::Unknown))
            .map(|(_, t)| t.bound)
            .collect()
    }

    /// Put the relations of `find_transfers` at the end of the model's, once.
    pub(super) fn add_transfers(&mut self) {
        if self.model.relations().len() == self.original_relations {
            for (r, _) in &self.extras {
                self.model.push_relation(r.clone());
            }
        }
    }

    /// Take them out again: the model that is returned, and the steps that read the design's
    /// relations alone, never see them.
    pub(super) fn drop_transfers(&mut self) {
        self.model.truncate_relations(self.original_relations);
    }

    /// The reason for a failure of the relation at `index` of the model when it is one of the
    /// solver's own: the statements that make the design's bound a bound on this field.
    pub(super) fn transferred_message(&self, index: usize, message: String) -> String {
        let Some((_, t)) = self.extras.get(index.wrapping_sub(self.original_relations)) else {
            return message;
        };
        let relations = self.model.relations();
        // A statement as written, and where it is, if the files are known.
        let describe = |r: &Relation| -> (String, Option<String>) {
            let text = self
                .model
                .statement_text(r.source, r.span)
                .unwrap_or_else(|| {
                    format!(
                        "{} {} {}",
                        self.model.show_expr(&r.lhs),
                        r.op.symbol(),
                        self.model.show_expr(&r.rhs)
                    )
                });
            (text, self.model.place_of_statement(r.source, r.span))
        };
        let mut held = String::new();
        for (n, &link) in t.links.iter().enumerate() {
            let r = &relations[link];
            let to = self.model.show_expr(&r.rhs);
            let (text, place) = describe(r);
            let at = place.map_or(format!("`{text}`"), |p| format!("`{text}`, {p}"));
            held.push_str(&match n {
                0 => format!("it is held to `{to}` ({at})"),
                _ => format!(", which is held to `{to}` ({at})"),
            });
        }
        let bound = &relations[t.bound];
        let (text, place) = describe(bound);
        let at = place.map_or(String::new(), |p| format!(" ({p})"));
        format!("{message}: {held}, and `{text}`{at} bounds it")
    }
}
