//! `unused .processor.usb`: a board states that nothing on it uses an interface of a part or a
//! module (ADR 0083).
//!
//! The statement is read like any other, and acts when the checker has read every statement and
//! joined every pair of pins, so that its place in the board never matters:
//!
//! - the board may not also use the interface: a pin of it on a net with a pin that is not a
//!   passive one (a test pad, a header) of another part, or a function stated on one of its pins,
//!   is an error that names both lines;
//! - the relations that read only values of the interface (`usb.vbus.voltage >= 4.35V`) are
//!   taken out of the model, so the solver neither checks them nor lists them as not
//!   evaluated;
//! - the currents of its rails are taken out of the balances, so the power view has no net that
//!   nothing supplies for it;
//! - the model keeps the list ([`Model::unused`]), for the checks of the later stages that
//!   skip the pins of the interface (the levels and absolute maximums, the pins that must be
//!   connected).

use std::collections::HashSet;
use std::sync::Arc;

use netsmith_lang::Span;
use netsmith_lang::ast::{self, PinKind};

use crate::checker::{Checker, Res};
use crate::diag::SourceId;
use crate::model::{InstId, InstKind, Member, Unused};

/// How a message says the board joined it, when it names the statement that did.
const HERE: &str = "connected here";

/// What a statement of the board touched: an operand of a `~` or a `~>`, and where.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Touch {
    pub inst: InstId,
    /// The module or board whose body states the connection.
    pub scope: InstId,
    pub source: SourceId,
    pub span: Span,
}

impl Checker<'_> {
    /// Read `unused .processor.usb`.
    pub(crate) fn unused(&mut self, u: &ast::Unused) {
        if !self.in_board() {
            self.error(
                u.span,
                "`unused` is a statement of a board, which is what knows what its parts are used for; a part or a module cannot say that a function of its own is unused, and the board states it for the instance it uses",
            );
            return;
        }
        let inst = match self.resolve_path(&u.path) {
            Res::Inst(i) if self.model.instance(i).kind == InstKind::Interface => i,
            Res::Inst(i) => {
                let inst = self.model.instance(i);
                let what = match inst.kind {
                    InstKind::Pin => "a pin".to_string(),
                    _ => format!("a `{}`", inst.type_name),
                };
                self.error(
                    u.path.span,
                    format!(
                        "`{}` is {what}, and `unused` names an interface of a part or a module (`unused .processor.usb`); a pin that is left open is not connected, and one that must stay open is a `no_connect` pin in its table",
                        u.path.to_source(),
                    ),
                );
                return;
            }
            Res::Field(_) => {
                self.error(
                    u.path.span,
                    format!(
                        "`{}` is a value, and `unused` names an interface of a part or a module (`unused .processor.usb`)",
                        u.path.to_source()
                    ),
                );
                return;
            }
            Res::Poison | Res::Bad => return,
        };
        let source = self.cur_source();
        if let Some(first) = self.unused_stmts.iter().find(|s| s.inst == inst).copied() {
            let place = self.place_of(first.source, first.span);
            self.error(
                u.span,
                format!(
                    "`{}` is already stated `unused` at {place}",
                    self.model.path_of(inst)
                ),
            );
            return;
        }
        self.unused_stmts.push(Unused {
            inst,
            source,
            span: u.span,
        });
    }

    /// Act on every `unused` statement of the board, after all statements are read.
    pub(crate) fn apply_unused(&mut self) {
        if self.unused_stmts.is_empty() {
            return;
        }
        let mut unused = std::mem::take(&mut self.unused_stmts);
        unused.sort_by_key(|u| self.model.path_of(u.inst));
        self.model.unused = unused;
        self.refuse_used();

        let model = &self.model;
        let in_unused = |f: crate::model::FieldId| model.unused_by(model.field(f).owner).is_some();
        let keep: Vec<bool> = model
            .relations()
            .iter()
            .map(|r| {
                let mut fields = Vec::new();
                r.lhs.fields(&mut fields);
                r.rhs.fields(&mut fields);
                fields.is_empty() || !fields.into_iter().all(in_unused)
            })
            .collect();
        let mut index = 0;
        Arc::make_mut(&mut self.model.relations).retain(|_| {
            index += 1;
            keep[index - 1]
        });

        let model = &self.model;
        let mut balances = (*model.balances).clone();
        for b in &mut balances {
            b.draws
                .retain(|d| model.unused_by(model.field(d.field).owner).is_none());
            b.unmodelled.retain(|&p| model.unused_by(p).is_none());
        }
        balances.retain(|b| !b.draws.is_empty());
        self.model.balances = Arc::new(balances);
        let model = &self.model;
        let tests: Vec<_> = model
            .level_tests
            .iter()
            .filter(|t| model.unused_by(model.field(t.level).owner).is_none())
            .cloned()
            .collect();
        self.model.level_tests = tests;
    }

    /// An interface the board states unused and also uses: a pin of it that is on a net with a
    /// pin of another part (a passive pin only brings a net out: a test pad or a header does not
    /// use the function, and nor does a return to ground), or a function the board states for a
    /// pin. The two statements say opposite things, and the message shows both.
    fn refuse_used(&mut self) {
        let unused = self.model.unused.clone();
        let mut reports: Vec<(SourceId, Span, String)> = Vec::new();
        let mut said_at: HashSet<(SourceId, Span)> = HashSet::new();
        for u in &unused {
            let model = &self.model;
            let holder = model.owner_of(u.inst);
            let name = model.path_of(u.inst);
            let said = self.place_of(u.source, u.span);
            let inside: Vec<InstId> = model
                .instances()
                .filter(|(id, _)| model.is_inside(*id, u.inst))
                .map(|(id, _)| id)
                .collect();
            // The return of a supply is not a use: every part's ground is the same net.
            let returns: HashSet<InstId> = inside
                .iter()
                .filter_map(|&i| model.rail_of(i).map(|(_, _, neg)| neg))
                .collect();
            let mut pins: Vec<InstId> = inside
                .iter()
                .copied()
                .filter(|&i| model.instance(i).kind == InstKind::Pin)
                .collect();
            pins.sort_by_key(|&p| model.path_of(p));
            let mut pinned = false;
            for pin in pins {
                let path = model.path_of(pin);
                if let Some(Member::Field(f)) = model.member(pin, "function")
                    && let Some(&(source, span)) = model.field(f).narrowed_at.last()
                {
                    reports.push((
                        source,
                        span,
                        format!(
                            "`{path}` is given a function here, but `{name}` is stated `unused` at {said}: a function the board chooses on a pin is used; remove the choice, or remove `unused`"
                        ),
                    ));
                    continue;
                }
                if returns.contains(&pin) {
                    continue;
                }
                let Some(net) = model.net_of(pin) else {
                    continue;
                };
                let other = model.others_on_net(pin).into_iter().find(|&t| {
                    model.is_terminal(t)
                        && model.unused_by(t).is_none()
                        && !holder.is_some_and(|h| model.is_inside(t, h))
                        && !matches!(model.pin_kind(t), None | Some(PinKind::Passive))
                });
                let Some(other) = other else { continue };
                let with = model.path_of(other);
                let net = net.name.clone();
                let (source, span, here) = self.where_joined(pin, holder, u);
                // One error for a statement that joins several pins: the first one says it.
                if here == HERE && !said_at.insert((source, span)) {
                    continue;
                }
                reports.push((
                    source,
                    span,
                    format!(
                        "`{path}` is {here} to `{with}` (net `{net}`), but `{name}` is stated `unused` at {said}: the interface is either used or not; remove the connection, or remove `unused`"
                    ),
                ));
                pinned = true;
            }
            // An interface joined to another with `~` equates their values, with or without a
            // pin of a part on the other side: a rail of the board that supplies the port is one.
            if !pinned {
                let model = &self.model;
                let mut joined: Vec<(InstId, InstId)> = model
                    .equalities()
                    .iter()
                    .filter_map(|&(a, b)| {
                        let (a, b) = (model.field(a).owner, model.field(b).owner);
                        let outside = |x: InstId| {
                            model.unused_by(x).is_none()
                                && !holder.is_some_and(|h| model.is_inside(x, h))
                        };
                        match (model.is_inside(a, u.inst), model.is_inside(b, u.inst)) {
                            (true, false) if outside(b) => Some((a, b)),
                            (false, true) if outside(a) => Some((b, a)),
                            _ => None,
                        }
                    })
                    .collect();
                joined.sort_by_key(|&(a, b)| (model.path_of(a), model.path_of(b)));
                if let Some(&(mine, theirs)) = joined.first() {
                    let (source, span, here) = self.where_joined(mine, holder, u);
                    let (mine, theirs) = (model.path_of(mine), model.path_of(theirs));
                    reports.push((
                        source,
                        span,
                        format!(
                            "`{mine}` is {here} to `{theirs}`, but `{name}` is stated `unused` at {said}: the interface is either used or not; remove the connection, or remove `unused`"
                        ),
                    ));
                }
            }
        }
        for (source, span, message) in reports {
            self.report(source, span, message);
        }
    }

    /// Where the board joins `inst`, which is in an interface stated unused: the `~` statement
    /// that names it or an interface around it, when one outside the part or module that holds
    /// the interface does; else the `unused` statement, for a net that the pins' names made.
    fn where_joined(
        &self,
        inst: InstId,
        holder: Option<InstId>,
        u: &Unused,
    ) -> (SourceId, Span, String) {
        let model = &self.model;
        let touch = self.touches.iter().find(|t| {
            model.is_inside(inst, t.inst) && !holder.is_some_and(|h| model.is_inside(t.scope, h))
        });
        match touch {
            Some(t) => (t.source, t.span, HERE.to_string()),
            None => (u.source, u.span, "connected".to_string()),
        }
    }
}
