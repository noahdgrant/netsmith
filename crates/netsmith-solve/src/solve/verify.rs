//! Verifying the relations and balances with the picks in place.

use std::collections::HashSet;

use super::equations::overshoot;
use super::{Solver, Unresolved, Verdict};
use crate::catalog::{Catalog, Value};
use crate::eval::{EvalError, Val, eval, fields_in, reads_end, reads_nominal, relation_reads_end};
use netsmith_check::{
    Balance, Domain, FieldId, FieldKind, InstId, InstKind, RExpr, Relation, SourceId,
};
use netsmith_core::job;
use netsmith_lang::Span;
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::{Dimension, Quantity};

impl Solver<'_> {
    pub(super) fn verify_balances(&mut self) {
        let mut balances = self.model.balances().to_vec();
        balances.sort_by_key(|b| b.returns);
        for b in balances {
            job::checkpoint();
            if b.returns && !self.diagnostics.is_empty() {
                continue;
            }
            // An incomplete balance says nothing about the parts on it (ADR 0027).
            if !b.is_complete() && b.draws.iter().any(|d| self.is_part_current(d)) {
                continue;
            }
            let known: Option<Vec<Quantity>> =
                b.draws.iter().map(|d| self.contribution(d)).collect();
            let Some(known) = known else { continue };
            if self.model.instance(b.scope).kind == InstKind::System {
                self.verify_join(&b, &known);
                continue;
            }
            // A port that only gives to what is inside, with nothing inside to take it that
            // the checker knows of (a part that is in no chain and has no rail of its own,
            // ADR 0014), is feeding what is not modelled: take what is stated.
            if known.len() == 1 && known[0].hi() <= 0.0 {
                continue;
            }
            let all: Vec<String> = b
                .draws
                .iter()
                .zip(&known)
                .map(|(d, q)| {
                    let sign = if d.flipped { "-" } else { "" };
                    format!("`{sign}{}` is {q}", self.model.path_of_field(d.field))
                })
                .collect();
            let slack = |qs: &[&Quantity]| {
                // A little slack, for the outward rounding of the sums.
                1e-9 * qs
                    .iter()
                    .flat_map(|q| [q.lo(), q.hi()])
                    .fold(1e-12_f64, |m, x| m.max(x.abs()))
            };
            // A stated current is a budget (ADR 0038): what the rest of the net determines
            // has to lie inside it. It need not fill it.
            if let Some(budget) = self.budget(&b) {
                let (stated, works_out) = (budget.stated, budget.works_out);
                let slack = slack(&[&stated, &works_out]);
                if stated.lo() - slack <= works_out.lo() && works_out.hi() <= stated.hi() + slack {
                    continue;
                }
                let d = &b.draws[budget.index];
                let name = self.model.path_of_field(d.field);
                let sign = if d.flipped { "-" } else { "" };
                let message = format!(
                    "the currents on net `{}` do not add up: `{sign}{name}` must stay within {stated}, but the others on it work out to {works_out} (all of them: {})",
                    b.net,
                    all.join(", ")
                );
                self.budget_failed.insert(d.field);
                let (source, span) = self.balance_site(&b);
                self.report(source, span, message);
                continue;
            }
            // Otherwise the terms are ranges that all hold or all are worked out from one
            // another, and the balance only has to be able to reach zero.
            let total = known
                .iter()
                .skip(1)
                .try_fold(known[0], |sum, q| sum.add(q).ok());
            let Some(total) = total else { continue };
            if total.lo() - slack(&[&total]) <= 0.0 && 0.0 <= total.hi() + slack(&[&total]) {
                continue;
            }
            // The first term, against the others.
            let others = known
                .iter()
                .skip(1)
                .try_fold(None, |sum: Option<Quantity>, q| match sum {
                    None => Some(Some(*q)),
                    Some(s) => s.add(q).ok().map(Some),
                })
                .map(|sum| sum.or(Quantity::point(Dimension::CURRENT, 0.0).ok()));
            let Some(Some(others)) = others else { continue };
            let d = &b.draws[0];
            let name = self.model.path_of_field(d.field);
            let sign = if d.flipped { "-" } else { "" };
            let message = format!(
                "the currents on net `{}` do not add up: `{sign}{name}` is {}, so the others on it must add up to {}, but they can only give {others} (all of them: {})",
                b.net,
                known[0],
                known[0].neg(),
                all.join(", ")
            );
            let (source, span) = self.balance_site(&b);
            self.report(source, span, message);
        }
    }

    /// The ports a system joins, with every current known. Each port's current is what the
    /// outside draws through it (a board's rail is written that way), so they are mirror
    /// statements of one flow and must be able to add up to zero. A port that cannot draw
    /// (its current is never negative) is a supply, and its range is how much the board is
    /// built to give; what the other ports draw must lie within it. A port that draws
    /// states only what its board takes, so it is not checked against what the supply says
    /// (`.output.current &= 0mA..50mA` against a board that takes 5 mA to 15 mA is fine).
    fn verify_join(&mut self, b: &Balance, known: &[Quantity]) {
        let zero = Quantity::point(Dimension::CURRENT, 0.0).ok();
        let sum = |skip: Option<usize>| {
            known
                .iter()
                .enumerate()
                .filter(|&(j, _)| Some(j) != skip)
                .try_fold(zero, |sum, (_, q)| sum.map(|s| s.add(q).ok()))
                .flatten()
        };
        let slack = |qs: &[&Quantity]| {
            1e-9 * qs
                .iter()
                .flat_map(|q| [q.lo(), q.hi()])
                .fold(1e-12_f64, |m, x| m.max(x.abs()))
        };
        let describe = |this: &Self, i: usize| {
            let field = b.draws[i].field;
            let owner = this.model.field(field).owner;
            let board = this
                .model
                .owner_of(owner)
                .map(|inst| this.model.instance(inst).type_name.clone())
                .unwrap_or_default();
            format!(
                "`{}` (board `{board}`) is {}",
                this.model.path_of_field(field),
                known[i]
            )
        };
        let all: Vec<String> = (0..known.len()).map(|i| describe(self, i)).collect();
        let (source, span) = self.balance_site(b);

        let Some(total) = sum(None) else { return };
        if total.lo() - slack(&[&total]) > 0.0 || total.hi() + slack(&[&total]) < 0.0 {
            let message = format!(
                "the currents at the join `{}` cannot add up to zero: {}, which add up to {total}",
                b.net,
                all.join(" and ")
            );
            self.report(source, span, message);
            return;
        }
        for (i, current) in known.iter().enumerate() {
            let Some(others) = sum(Some(i)) else { continue };
            if current.lo() < 0.0 {
                continue;
            }
            let demand = others.neg();
            let slack = slack(&[current, &demand]);
            if current.lo() - slack <= demand.lo() && demand.hi() <= current.hi() + slack {
                continue;
            }
            let rest: Vec<&str> = all
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, d)| d.as_str())
                .collect();
            let message = format!(
                "the join `{}` asks more of a port than its board allows: {}, but the other ports there ({}) draw {demand}",
                b.net,
                all[i],
                rest.join(", ")
            );
            self.report(source, span, message);
        }
    }

    /// A failed requirement on a current, with what that current is made of: each net it
    /// is summed on, with the terms and their ranges, and the nets of the terms in turn (a
    /// regulator's output is the board's whole load, which is the rails and the parts on
    /// the net). Return nets repeat the same terms and are left out. A requirement that
    /// mentions no current in a balance is returned as it was.
    pub(super) fn with_currents(&self, r: &Relation, message: String) -> String {
        let balances = self.model.balances();
        let reads = self.reads();
        let mut fields = Vec::new();
        fields_in(&r.lhs, &mut fields);
        fields_in(&r.rhs, &mut fields);
        let mut seen: HashSet<usize> = HashSet::new();
        let mut notes = Vec::new();
        while let Some(f) = fields.pop() {
            // The balances that sum the field, in the order the model lists them.
            for &i in reads.supply_balances_of(f) {
                let b = &balances[i];
                if !seen.insert(i) {
                    continue;
                }
                let terms: Vec<String> = b
                    .draws
                    .iter()
                    .map(|d| {
                        let sign = if d.flipped { "-" } else { "" };
                        let name = self.model.path_of_field(d.field);
                        format!("{sign}{name} ({})", self.model.field(d.field).domain)
                    })
                    .collect();
                let scope = match self.model.path_of(b.scope) {
                    path if path.is_empty() => String::new(),
                    path => format!(" in `{path}`"),
                };
                let unmodelled = if b.is_complete() {
                    String::new()
                } else {
                    let pins: Vec<String> = b
                        .unmodelled
                        .iter()
                        .map(|&p| format!("`{}`", self.model.path_of(p)))
                        .collect();
                    format!(
                        ", but the balance is incomplete: {} {} not modelled",
                        pins.join(", "),
                        if pins.len() == 1 { "is" } else { "are" }
                    )
                };
                notes.push(format!(
                    "net `{}`{scope}: {} add up to zero{unmodelled}",
                    b.net,
                    terms.join(" + ")
                ));
                fields.extend(b.draws.iter().map(|d| d.field));
            }
        }
        if notes.is_empty() {
            message
        } else {
            format!("{message} (the currents on {})", notes.join("; "))
        }
    }

    /// Where to point about a net's currents: at a statement that set one of them, or else
    /// at the board or module they are in.
    fn balance_site(&self, b: &Balance) -> (SourceId, Span) {
        b.draws
            .iter()
            .find_map(|d| self.model.field(d.field).narrowed_at.last().copied())
            .unwrap_or_else(|| {
                let scope = self.model.instance(b.scope);
                (scope.source, scope.span)
            })
    }

    /// A cell of a column that holds an enum must be one of its variants (ADR 0054): a row
    /// that names a class the enum does not have is refused, with the row and the variants.
    pub(super) fn check_enum_cells(
        &self,
        catalog: &str,
        columns: &[(FieldId, String)],
        cat: Catalog,
    ) -> Result<Catalog, String> {
        for (field, key) in columns {
            let FieldKind::Enum(e) = self.model.field(*field).kind else {
                continue;
            };
            let info = self.model.enum_info(e);
            for c in &cat.candidates {
                if let Some(Value::Text(word)) = c.column(key)
                    && !info.variants.contains(word)
                {
                    let listed: Vec<String> =
                        info.variants.iter().map(|v| format!("`{v}`")).collect();
                    return Err(format!(
                        "the `{key}` of `{}` in the `{catalog}` catalog is \"{word}\", which is not a variant of `{}` ({})",
                        c.name,
                        info.name,
                        listed.join(", ")
                    ));
                }
            }
        }
        Ok(cat)
    }

    /// A package the design states that no row of the part's catalog is in. It is reported at
    /// the statement, with the nearest package the catalog has (ADR 0118), and the part is left
    /// unresolved: no row is tried for it. A catalog whose rows do not all name a package (a
    /// generic row that leaves it open) is not judged here.
    pub(super) fn unknown_package(
        &mut self,
        part: InstId,
        path: &str,
        catalog: &str,
        columns: &[(FieldId, String)],
        cat: &Catalog,
    ) -> bool {
        let Some((field, key)) = columns.iter().find(|(_, key)| key == "package") else {
            return false;
        };
        let Domain::Text(stated) = self.domain(*field) else {
            return false;
        };
        let Some(&(source, span)) = self.model.field(*field).narrowed_at.last() else {
            return false;
        };
        let mut packages: Vec<String> = Vec::new();
        for c in &cat.candidates {
            match c.column(key) {
                Some(Value::Text(p)) => {
                    if !packages.contains(p) {
                        packages.push(p.clone());
                    }
                }
                _ => return false,
            }
        }
        if packages.is_empty() || packages.contains(&stated) {
            return false;
        }
        packages.sort();
        let hint = netsmith_check::closest(&stated, &packages)
            .map(|p| format!("; did you mean \"{p}\"?"))
            .unwrap_or_default();
        let listed: Vec<String> = packages.iter().map(|p| format!("\"{p}\"")).collect();
        self.report(
            source,
            span,
            format!(
                "`{path}.package` is \"{stated}\", and no row of the `{catalog}` catalog is in that package; its packages are {}{hint}",
                listed.join(", ")
            ),
        );
        self.unresolved.push(Unresolved {
            part,
            reason: format!("no row of the `{catalog}` catalog is in package \"{stated}\""),
        });
        true
    }

    /// Whether `e` is a text field that nothing has set.
    fn is_unset_text(&self, e: &RExpr) -> bool {
        matches!(e, RExpr::Field(f)
            if matches!(self.model.field(*f).kind, FieldKind::Text | FieldKind::Enum(_))
                && self.model.field(*f).domain == Domain::Unset)
    }

    /// A requirement on a position (ADR 0045) that nothing satisfies: its bounds leave an
    /// empty window. Reported at the last statement that narrows the field.
    pub(super) fn check_windows(&mut self) {
        let fields: Vec<FieldId> = self
            .model
            .fields()
            .filter(|(id, _)| self.model.is_place_field(*id))
            .map(|(id, _)| id)
            .collect();
        for field in fields {
            job::checkpoint();
            let w = crate::place::window(&self.model, field);
            if !w.is_empty() {
                continue;
            }
            let FieldKind::Quantity(dim) = self.model.field(field).kind else {
                continue;
            };
            let name = self.model.path_of_field(field);
            if let Some(&(source, span)) = w.sites.last() {
                self.report(source, span, w.clash(&name, dim));
            }
        }
    }

    /// Whether a relation is a bound on a `.place` field or a value tied to one: those are
    /// requirements that the design stage reads (ADR 0045), not claims to verify.
    pub(super) fn place_side<'r>(&self, r: &'r Relation) -> Option<(FieldId, &'r RExpr)> {
        let is_place = |e: &RExpr| match e {
            RExpr::Field(f) if self.model.is_place_field(*f) => Some(*f),
            _ => None,
        };
        match r.op {
            CmpOp::Ge | CmpOp::Le => is_place(&r.lhs)
                .map(|f| (f, &r.rhs))
                .or_else(|| is_place(&r.rhs).map(|f| (f, &r.lhs))),
            CmpOp::Narrow | CmpOp::Default => is_place(&r.lhs).map(|f| (f, &r.rhs)),
            CmpOp::EqEq => None,
        }
    }

    /// Whether the bound `r`, a `>=` or a `<=`, can hold for some value of the ranges as they
    /// are: `Fail` only when it holds for none, which no later narrowing of the ranges
    /// changes. A range that is unknown, or a comparison of anything but quantities, is
    /// `Unknown`: it is judged when it is known. So is a relation that reads the end of a
    /// range (`min(.f)`, `max(.f)`, ADR 0098): the end is one point of the range as it is, and
    /// a later narrowing moves it (the least end only rises, the greatest only falls), and the
    /// hull of a block of candidates puts its own edge there, which is no enclosure of a row's
    /// point. Neither "holds for no value" claim is sound for it, so it is judged only on the
    /// values a trial gives (`verify`).
    pub(super) fn verify_possible(&self, r: &Relation) -> Verdict {
        if relation_reads_end(r) {
            return Verdict::Unknown;
        }
        let (Ok(Val::Q(l)), Ok(Val::Q(v))) = (eval(&self.model, &r.lhs), eval(&self.model, &r.rhs))
        else {
            return Verdict::Unknown;
        };
        let (possible, word, gap) = if r.op == CmpOp::Ge {
            (l.hi() >= v.lo(), "at least", v.lo() - l.hi())
        } else {
            (l.lo() <= v.hi(), "at most", l.lo() - v.hi())
        };
        if possible {
            return Verdict::Pass;
        }
        let scale = l
            .lo()
            .abs()
            .max(l.hi().abs())
            .max(v.lo().abs())
            .max(v.hi().abs())
            .max(1e-30);
        Verdict::Fail {
            message: format!(
                "{} must be {word} {}, but {l} is always {} {v}",
                self.model.show_expr(&r.lhs),
                self.model.show_expr(&r.rhs),
                if r.op == CmpOp::Ge { "below" } else { "above" },
            ),
            margin: gap / scale,
        }
    }

    pub(super) fn verify(&self, r: &Relation) -> Verdict {
        let show = |e: &RExpr| self.model.show_expr(e);
        let fail = |message: String, margin: f64| Verdict::Fail { message, margin };
        // A bound on a position narrows it. Its other side may still be waiting on a value;
        // a value a module offers (`.j.place.y &= .shell_y`, ADR 0042) that the board leaves
        // alone is no note.
        if let Some((_, other)) = self.place_side(r)
            && r.op != CmpOp::Narrow
        {
            return match eval(&self.model, other) {
                Ok(Val::Q(_)) => Verdict::Pass,
                Err(EvalError::Unknown(_)) => Verdict::Unknown,
                Ok(Val::T(_)) | Err(EvalError::Type) => {
                    fail("a position is a number".to_string(), 1.0)
                }
                Err(e) => fail(format!("{}: {e}", show(other)), 1.0),
            };
        }
        if let Some((_, RExpr::Field(g))) = self.place_side(r)
            && r.op == CmpOp::Narrow
            && self.model.field(*g).domain == Domain::Unset
        {
            return Verdict::Pass;
        }
        let eval_both = || -> Result<(Val, Val), EvalError> {
            let l = eval(&self.model, &r.lhs);
            let rr = eval(&self.model, &r.rhs);
            Ok((l?, rr?))
        };

        match r.op {
            // The checker turns every `?=` into a `&=` or drops it (ADR 0051), so the solver
            // reads `Default` as the narrowing it became.
            CmpOp::Narrow | CmpOp::Default => match eval(&self.model, &r.rhs) {
                Ok(_) => Verdict::Pass,
                // A text field nobody set narrows nothing: a value a module offers
                // (`.j.assembly &= .assembly_mode`, ADR 0042) may be left alone. A text
                // value is never worked out, so there is nothing still to wait for.
                Err(EvalError::Unknown(_)) if self.is_unset_text(&r.rhs) => Verdict::Pass,
                Err(EvalError::Unknown(_)) => Verdict::Unknown,
                Err(e) => fail(format!("{}: {e}", show(&r.rhs)), 1.0),
            },
            CmpOp::EqEq => {
                let rhs = match eval(&self.model, &r.rhs) {
                    Ok(v) => v,
                    Err(EvalError::Unknown(_)) => return Verdict::Unknown,
                    Err(e) => return fail(format!("{}: {e}", show(&r.rhs)), 1.0),
                };
                if let RExpr::Field(f) = &r.lhs {
                    let name = self.model.path_of_field(*f);
                    // A value that several equations state and no statement bounds has no
                    // allowance of its own: it is what the equations say, and a pick sets it.
                    // An equation whose right side no pick changes (a window the board stated,
                    // reached through a join or a link such as `.osc.fsw == .fsw`) is an
                    // allowance, and what the other equations work out must lie inside it. With
                    // no allowance among them, equations that a pick changes only have to share
                    // a value: both are what the circuit works out, and neither is a promise.
                    if let (Domain::Quantity(dx), Val::Q(v)) = (&self.model.field(*f).domain, &rhs)
                        && self.several_equations.contains(f)
                        && self.fed.contains(f)
                        && self.model.field(*f).narrowed_at.is_empty()
                        && !self.derived.contains(f)
                    {
                        let changes = |e: &RExpr| {
                            let mut read = Vec::new();
                            fields_in(e, &mut read);
                            read.iter().any(|g| self.fed.contains(g))
                        };
                        // What the equations that no pick changes allow, together.
                        let mut allowance: Option<Quantity> = None;
                        for &i in self.equations_of.get(f).into_iter().flatten() {
                            let other = &self.model.relations()[i];
                            if changes(&other.rhs) {
                                continue;
                            }
                            if let Ok(Val::Q(q)) = eval(&self.model, &other.rhs) {
                                allowance = match allowance {
                                    None => Some(q),
                                    Some(a) => match a.intersect(&q) {
                                        Ok(Some(both)) => Some(both),
                                        _ => Some(a),
                                    },
                                };
                            }
                        }
                        return if !changes(&r.rhs) {
                            if v.encloses(dx).unwrap_or(false) {
                                Verdict::Pass
                            } else {
                                fail(
                                    format!(
                                        "`{name}` must stay within {v}, which {} says, but it works out to {dx}",
                                        show(&r.rhs)
                                    ),
                                    overshoot(v, dx),
                                )
                            }
                        } else if let Some(a) = allowance {
                            if a.encloses(v).unwrap_or(false) {
                                Verdict::Pass
                            } else {
                                fail(
                                    format!(
                                        "`{name}` must stay within {a}, but {} works out to {v}",
                                        show(&r.rhs)
                                    ),
                                    overshoot(&a, v),
                                )
                            }
                        } else if dx.intersect(v).ok().flatten().is_some() {
                            Verdict::Pass
                        } else {
                            fail(
                                format!(
                                    "`{name}` is {dx} but {} works out to {v}; they can never be equal",
                                    show(&r.rhs)
                                ),
                                overshoot(dx, v),
                            )
                        };
                    }
                    return match (&self.model.field(*f).domain, rhs) {
                        (Domain::Unset, _) => Verdict::Unknown,
                        (Domain::Quantity(d), Val::Q(v)) => {
                            // See `propagate`: a derived current only has to share a value
                            // with the equation.
                            let derived = self.derived.contains(f);
                            if d.encloses(&v).unwrap_or(false)
                                || (derived && d.intersect(&v).ok().flatten().is_some())
                            {
                                Verdict::Pass
                            } else {
                                fail(
                                    format!(
                                        "`{name}` must stay within {d}, but {} works out to {v}",
                                        show(&r.rhs)
                                    ),
                                    overshoot(d, &v),
                                )
                            }
                        }
                        (Domain::Text(d), Val::T(v)) if *d == v => Verdict::Pass,
                        (Domain::Text(d), Val::T(v)) => {
                            let (d, v) = (
                                self.model.show_value(*f, &Domain::Text(d.clone())),
                                self.model.show_value(*f, &Domain::Text(v)),
                            );
                            let message = match (&r.rhs, self.model.field(*f).kind) {
                                // A variant written in the check: it is not an expression
                                // that works out to something else.
                                (RExpr::Text(_), FieldKind::Enum(_)) => {
                                    format!("`{name}` is {d}, but this check requires {v}")
                                }
                                _ => format!("`{name}` must be {d}, but {} is {v}", show(&r.rhs)),
                            };
                            fail(message, 1.0)
                        }
                        _ => fail(
                            format!("`{name}` and {} are different kinds of value", show(&r.rhs)),
                            1.0,
                        ),
                    };
                }
                match eval_both() {
                    Ok((Val::Q(l), Val::Q(v))) => {
                        if l.intersect(&v).ok().flatten().is_some() {
                            Verdict::Pass
                        } else {
                            fail(
                                format!(
                                    "{} is {l} but {} is {v}; they can never be equal",
                                    show(&r.lhs),
                                    show(&r.rhs)
                                ),
                                overshoot(&l, &v),
                            )
                        }
                    }
                    Err(EvalError::Unknown(_)) => Verdict::Unknown,
                    _ => Verdict::Pass,
                }
            }
            CmpOp::Ge | CmpOp::Le => match eval_both() {
                Ok((Val::Q(l), Val::Q(v))) => {
                    let (ok, word, gap) = if r.op == CmpOp::Ge {
                        (l.lo() >= v.hi(), "at least", v.hi() - l.lo())
                    } else {
                        (l.hi() <= v.lo(), "at most", l.hi() - v.lo())
                    };
                    if ok {
                        Verdict::Pass
                    } else {
                        let scale = l
                            .lo()
                            .abs()
                            .max(l.hi().abs())
                            .max(v.lo().abs())
                            .max(v.hi().abs())
                            .max(1e-30);
                        // An end of a range is one number (ADR 0098): the two numbers are
                        // compared, and "always" would say more than the check does.
                        let message = if reads_nominal(&r.lhs) || reads_nominal(&r.rhs) {
                            // `nominal(.f)` is the value a field is stated at, whatever its
                            // tolerance; a bound on it is no claim about the worst case.
                            format!(
                                "{} must be {word} {}, but {l} is {} {v}: it compares the value a field is stated at, without its tolerance, so change the value or the bound",
                                show(&r.lhs),
                                show(&r.rhs),
                                if r.op == CmpOp::Ge { "below" } else { "above" },
                            )
                        } else if reads_end(&r.lhs) || reads_end(&r.rhs) {
                            format!(
                                "{} must be {word} {}, but {l} is {} {v}: no value of the two ranges meets it, so change one of them",
                                show(&r.lhs),
                                show(&r.rhs),
                                if r.op == CmpOp::Ge { "below" } else { "above" },
                            )
                        } else {
                            format!(
                                "{} must always be {word} {}, but {l} is not always {} {v}",
                                show(&r.lhs),
                                show(&r.rhs),
                                if r.op == CmpOp::Ge { "above" } else { "below" },
                            )
                        };
                        fail(message, gap / scale)
                    }
                }
                Err(EvalError::Unknown(_)) => Verdict::Unknown,
                Err(e) => fail(format!("{e}"), 1.0),
                Ok(_) => fail("a comparison needs numbers, not text".into(), 1.0),
            },
        }
    }

    // ---- picking -------------------------------------------------------------
}
