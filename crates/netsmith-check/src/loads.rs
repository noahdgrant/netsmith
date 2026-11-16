//! `shared NAME [limit N] { .load ... }` in a board or a module: at most N of the named loads are
//! on at once (issue #806).
//!
//! Without the statement every load on a net draws at once, a pessimistic sum (ADR 0076, the
//! owner's decision 3). A board whose heaters, motors or radios take turns states it:
//!
//! ```text
//! shared HEATERS limit 1 { .heater_a .heater_b .heater_c }
//! ```
//!
//! - It is the statement a part uses for its peripherals (ADR 0071) with the meaning it has
//!   there, at most `limit` members in use at a time, one when it states none; in a board or a
//!   module its members are loads. A member is a rail (an interface that draws, such as a `Rail`):
//!   a rail of the board or the module, of a part, or a module's port, by its path.
//! - A load that is off draws the low end of its range (0 A for `0mA..50mA`); one that is on
//!   draws anything in its range. The members of a group on one net then add up to the sum of the
//!   low ends plus the N largest spans (`hi - lo`), not the sum of every top: two heaters of
//!   0 A..2 A with `limit 1` draw 0 A..2 A together, not 0 A..4 A ([`Model::sum_draws`]).
//! - What reads that sum: the balance of the net, so the current a supply gives (and so a
//!   regulator's `.max_current` check) is the worst N; a stated budget on the net; the DC
//!   analysis, which leaves out the corners with more than N members at their top (and so the
//!   currents it writes, its checks and the tracks it sizes); the track width of a net the
//!   analysis did not solve; and the power page's demand. A message about the currents of a net
//!   that holds more than N members names the group.
//! - Refused: a member that is not a rail, a member twice, a rail in two groups, two groups of
//!   one name in one board or module, and a limit that is not fewer than the members (it says
//!   nothing). The statement is refused in an interface, a generic, a system, a panel and a
//!   physical file; in a part it is the part's own `shared` (ADR 0071).
//!
//! A module's group holds for each instance of the module, with the members its paths reach
//! there.

use netsmith_lang::Span;
use netsmith_lang::ast;
use netsmith_quantity::{Dimension, Quantity};

use crate::checker::{Checker, Res};
use crate::diag::SourceId;
use crate::model::{Draw, FieldId, InstId, InstKind, Model};

/// A group of loads and the sign its members count with in a balance.
type GroupSide = (usize, bool);

/// A group of loads of which at most `limit` draw at once.
#[derive(Debug, Clone)]
pub struct LoadGroup {
    pub name: String,
    /// The board or module instance that states it.
    pub scope: InstId,
    pub limit: usize,
    /// Each member: the rail, and the field of its current.
    pub members: Vec<(InstId, FieldId)>,
    pub source: SourceId,
    pub span: Span,
}

/// The sum of loads of which at most `limit` are on at once, each a current `(lo, hi)`: the low
/// ends, plus the `limit` largest spans. A load that is off draws its low end.
pub fn worst_of(limit: usize, loads: &[(f64, f64)]) -> (f64, f64) {
    let lo: f64 = loads.iter().map(|l| l.0).sum();
    let mut spans: Vec<f64> = loads.iter().map(|l| l.1 - l.0).collect();
    spans.sort_by(|a, b| b.total_cmp(a));
    let more: f64 = spans.iter().take(limit).sum();
    (lo, lo + more)
}

impl Model {
    /// The groups of loads the boards and modules state, in the order they were read.
    pub fn load_groups(&self) -> &[LoadGroup] {
        &self.load_groups
    }

    /// The group a rail's current belongs to, by the field.
    pub fn load_group_of(&self, field: FieldId) -> Option<usize> {
        self.load_groups
            .iter()
            .position(|g| g.members.iter().any(|m| m.1 == field))
    }

    /// The group and the member a rail stands for: the member itself, or a rail inside the module
    /// whose port the member is (what the module draws is what is inside it).
    pub fn load_member_of(&self, rail: InstId) -> Option<(usize, usize)> {
        for (g, group) in self.load_groups.iter().enumerate() {
            for (m, &(member, _)) in group.members.iter().enumerate() {
                if member == rail {
                    return Some((g, m));
                }
                let Some(owner) = self.owner_of(member) else {
                    continue;
                };
                if self.instance(owner).kind == InstKind::Module
                    && rail != member
                    && self.is_inside(rail, owner)
                {
                    return Some((g, m));
                }
            }
        }
        None
    }

    /// The sum of the terms of a balance, each a draw with its contribution. The members of a
    /// group of loads add up to the worst `limit` of them ([`worst_of`]), the rest as they are.
    /// `None` when a sum cannot be formed (dimensions that differ).
    pub fn sum_draws(&self, terms: &[(Draw, Quantity)]) -> Option<Quantity> {
        if self.load_groups.is_empty() {
            let (first, rest) = terms.split_first()?;
            return rest.iter().try_fold(first.1, |sum, (_, q)| sum.add(q).ok());
        }
        let mut sum: Option<Quantity> = None;
        let add = |q: Quantity, sum: &mut Option<Quantity>| -> Option<()> {
            *sum = Some(match sum {
                None => q,
                Some(s) => s.add(&q).ok()?,
            });
            Some(())
        };
        // The members of each group on the net, by the group and the sign they count with.
        let mut grouped: Vec<(GroupSide, Vec<(f64, f64)>)> = Vec::new();
        for (d, q) in terms {
            match self.load_group_of(d.field) {
                Some(g) => {
                    let raw = if d.flipped { q.neg() } else { *q };
                    match grouped.iter_mut().find(|(k, _)| *k == (g, d.flipped)) {
                        Some((_, v)) => v.push((raw.lo(), raw.hi())),
                        None => grouped.push(((g, d.flipped), vec![(raw.lo(), raw.hi())])),
                    }
                }
                None => add(*q, &mut sum)?,
            }
        }
        for ((g, flipped), loads) in grouped {
            let (lo, hi) = worst_of(self.load_groups[g].limit, &loads);
            let q = Quantity::interval(Dimension::CURRENT, lo, hi).ok()?;
            add(if flipped { q.neg() } else { q }, &mut sum)?;
        }
        sum
    }

    /// The groups of loads that hold on a balance's draws (more members than the limit on the
    /// net), as a clause a message about the net's currents ends with: `, with at most 1 of
    /// `HEATERS` on at once`. Empty when none does.
    pub fn group_clause<'a>(&self, draws: impl IntoIterator<Item = &'a Draw>) -> String {
        let mut counts: Vec<(usize, usize)> = Vec::new();
        for d in draws {
            if let Some(g) = self.load_group_of(d.field) {
                match counts.iter_mut().find(|(k, _)| *k == g) {
                    Some((_, n)) => *n += 1,
                    None => counts.push((g, 1)),
                }
            }
        }
        let held: Vec<String> = counts
            .into_iter()
            .filter(|&(g, n)| n > self.load_groups[g].limit)
            .map(|(g, _)| {
                let group = &self.load_groups[g];
                let at = match self.path_of(group.scope) {
                    path if path.is_empty() => String::new(),
                    path => format!(" of `{path}`"),
                };
                format!("at most {} of `{}`{at} on at once", group.limit, group.name)
            })
            .collect();
        if held.is_empty() {
            String::new()
        } else {
            format!(", with {}", held.join(" and "))
        }
    }
}

impl Checker<'_> {
    /// Read `shared NAME [limit N] { .load ... }` in a board or a module.
    pub(crate) fn load_group(&mut self, s: &ast::Shared) {
        let scope = self.current();
        let source = self.cur_source();
        let mut members: Vec<(InstId, FieldId)> = Vec::new();
        for m in &s.members {
            let path = match m {
                ast::SharedMember::Pin(path) => path,
                ast::SharedMember::Name(n) => {
                    self.error(
                        n.span,
                        format!(
                            "a `shared` group of a board or a module lists loads by their path (`.{0}`), each a rail that draws; `{0}` is a name, which only a part's `shared` lists (a peripheral or a function)",
                            n.name
                        ),
                    );
                    continue;
                }
            };
            let what = match self.resolve_path(path) {
                Res::Inst(i) => match self.model.draw_of(i) {
                    Some((field, _)) => {
                        if members.iter().any(|m| m.0 == i) {
                            self.error(
                                path.span,
                                format!(
                                    "`{}` is listed twice in `shared {}`",
                                    path.to_source(),
                                    s.name.name
                                ),
                            );
                        } else if let Some(g) = self.model.load_group_of(field) {
                            let other = &self.model.load_groups[g];
                            let place = self.place_of(other.source, other.span);
                            self.error(
                                path.span,
                                format!(
                                    "`{}` is already a load of `shared {}` at {place}; a load is in one group",
                                    path.to_source(),
                                    other.name
                                ),
                            );
                        } else {
                            members.push((i, field));
                        }
                        continue;
                    }
                    None => {
                        let inst = self.model.instance(i);
                        match inst.kind {
                            InstKind::Pin => "a pin".to_string(),
                            _ => format!("a `{}`", inst.type_name),
                        }
                    }
                },
                Res::Field(_) => "a value".to_string(),
                Res::Poison | Res::Bad => continue,
            };
            self.error(
                path.span,
                format!(
                    "`{}` is {what}, and a `shared` group of a board or a module lists loads: rails that draw, such as `.heater: Rail`",
                    path.to_source()
                ),
            );
        }
        if let Some(other) = self
            .model
            .load_groups
            .iter()
            .find(|g| g.scope == scope && g.name == s.name.name)
        {
            let place = self.place_of(other.source, other.span);
            self.error(
                s.name.span,
                format!(
                    "`shared {}` is already stated at {place}; one statement lists all of its loads",
                    s.name.name
                ),
            );
            return;
        }
        let limit = s.limit.map_or(1, |(n, _)| n as usize);
        if members.len() != s.members.len() {
            return;
        }
        if members.len() == 1 {
            self.error(
                s.name.span,
                format!(
                    "`shared {}` lists one load, which is on whenever it is; a group lists two or more loads that take turns",
                    s.name.name
                ),
            );
            return;
        }
        if limit >= members.len() {
            let span = s.limit.map_or(s.name.span, |(_, span)| span);
            self.error(
                span,
                format!(
                    "`shared {}` allows {limit} of its {} loads on at once, which is every one of them, so it says nothing; list more loads or give a smaller limit",
                    s.name.name,
                    members.len()
                ),
            );
            return;
        }
        self.model.load_groups.push(LoadGroup {
            name: s.name.name.clone(),
            scope,
            limit,
            members,
            source,
            span: s.span,
        });
    }
}
