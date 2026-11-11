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
//!
//! # The fault, and why this fix (ADR 0065, section 1)
//!
//! A probe showed that `.lamp.balance_resistance >= 20kohm` on a member of a module that writes
//! `.top.resistance &= .balance_resistance` picked two 330 ohm resistors and printed only
//! `not evaluated`. A bound is a relation and never a domain, and `propagate` evaluates only
//! `==` and `&=` with a field alone on the left. The link carries a value one way, from the
//! member into the part's field, and the member never has one. `try_candidate` judges the
//! relations that name a field inside the part, or one a candidate changed, and the bound names
//! only the member, which does not change. `Coupling::feeding` has no edge from the member to
//! the part, since the edge runs the other way. After the picks the bound and the `&=` are not
//! evaluated. ADR 0057's case worked because its equation is written the other way round
//! (`.member == .part.field`). The two spellings mean the same to a reader and differ in the
//! solver.
//!
//! If `x &= g` (or `x == g` with `g` the whole right side) and the design states `g >= e`, then
//! `x >= e`; likewise `<=`. The rule repeats along a chain. The transferred bounds are judged
//! where every relation is: at a candidate (a row that breaks the bound is refused where it is
//! picked) and after the picks (a stated value that breaks it fails the build with the numbers).
//! They are never listed as not evaluated, since they are not the design's lines. This is sound
//! because it adds a refusal and never a value, and `x &= g` places `x` inside `g`; it cannot
//! refuse a design that meets what it stated. A build that passed before can fail now, and it is
//! exactly a build that broke a stated bound and said nothing. Tolerance counts:
//! `20kohm +/- 1%` is not always at least 20 kohm.
//!
//! Where it differs from what the ADR first described. The transfer is worked out just before
//! the picks, after the first propagation and the start of circles, and kept out of the model
//! except while parts are picked and checked (`Model::push_relation`, `Model::truncate_relations`):
//! worked out earlier, a member that got its value a step later had added a relation that joined
//! two capacitors' group with the regulator. A bound on a member that has a value, a link `x ==
//! g` whose left side has a value of its own, and a field that is a position (`.place`) are left
//! alone; a chain goes on only through a field with no value. An `==` puts the right side inside
//! what the left side allows, so a range on the left says nothing about the right. A candidate is
//! judged by the added bound only when no part still to be chosen feeds the right side. When no
//! row meets the bound each part with no row has its own message, since nothing joins them in one
//! search; a part with a stated value that a row stands for is refused at the pick and again by
//! the check after, which gives two messages, as a bound written on the part itself does. The
//! message says `X must always be at least E, but V is not always above E: it is held to `g`
//! (`.x &= .g`, F:N), and `.g >= E` (F:N) bounds it`, from the snapshot of the sources that every
//! model keeps (`Model::place_of_statement`, `Model::statement_text`).
//!
//! Found while building it: `hopeless` skipped a row that the design could not tell from a row
//! it had refused, comparing only the columns that another field reads, so a 2 uF row looked
//! like its 0.5 uF twin and a part of a group of two was called hopeless though a row met
//! `.a.effective_capacitance >= 1uF`. The refused row now carries the columns of the relation
//! that refused it.
//!
//! # Rejected
//!
//! A value that flows back from the part to the member (`x &= g` sets an unset `g` from `x`):
//! with `top.resistance &= .m` and `bottom.resistance &= .m` and `.m` unset, the first pick would
//! set `.m` to a point value and force the second part to the same row, a requirement nobody
//! wrote, decided by which part is picked first. A bound that narrows the field's domain: it
//! changes every domain a bound reaches, the starts of ADR 0053 read bounds as the part's range,
//! and a half-open range has no value in an expression. Only the check afterwards: the solver
//! would take the 330 ohm row when a 20 kohm row existed. Transfer through `~` joins and
//! through arithmetic (`x &= g * 2`): not built; a join is not written for values of parts, and
//! arithmetic needs monotonic reasoning. The cost is about 120 lines of the solver and a way to
//! add relations to the model and cut them off again.

use std::collections::{HashMap, HashSet};

use netsmith_check::{Domain, FieldId, FieldKind, RExpr, Relation};
use netsmith_lang::ast::CmpOp;

use super::Solver;
use crate::eval::{fields_in, relation_reads_end};

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
            // A bound read from the end of a range may be stale once the range narrows (ADR 0098).
            if relation_reads_end(b) {
                continue;
            }
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
