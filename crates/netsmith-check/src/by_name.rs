//! `.left ~ .right { a = b } except { c }`: the members of two ports joined name by name (ADR
//! 0117). `~` between two parts, modules or interfaces of different kinds pairs the pins and
//! interfaces of the two sides, refuses a pin against an interface and a member left with no
//! partner, and then joins each pair as `~` does.

use std::collections::{BTreeMap, BTreeSet};

use netsmith_lang::ast::{self, ConnectOp, Ident};
use netsmith_lang::error::Span;

use crate::checker::{Checker, Res};
use crate::model::{InstId, InstKind, Member};

/// One side of the statement: the port and its joinable members in declaration order.
struct Side {
    path: ast::Path,
    /// `(name, instance)`: each pin and interface a path through the port reaches.
    members: Vec<(String, InstId)>,
    /// The other members (values, parts inside), named so a rename to one is explained.
    others: BTreeSet<String>,
}

impl Side {
    fn names(&self) -> Vec<String> {
        self.members.iter().map(|(n, _)| n.clone()).collect()
    }

    fn get(&self, name: &str) -> Option<InstId> {
        self.members
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, i)| *i)
    }

    fn src(&self) -> String {
        self.path.to_source()
    }
}

impl Checker<'_> {
    /// `.a ~ .b { x = y } except { z }`: a join of two ports with renames or members left out.
    pub(crate) fn join_map(&mut self, b: &ast::JoinMap) {
        if self.in_system() || self.in_panel() {
            self.error(
                b.span,
                "a rename map or `except` joins the members of two ports on a board, a module or a part; a system joins ports with plain `~`",
            );
            return;
        }
        if !self.join_ports_by_name(&b.left, &b.right, &b.renames, &b.except, b.span) {
            // `join_ports_by_name` declines only two sides that resolved: say why.
            self.error(
                b.span,
                format!(
                    "a rename map or `except` is for two ports whose members are joined by name; `{}` and `{}` are not two parts, modules or interfaces of different kinds, so join them with plain `~`",
                    b.left.to_source(),
                    b.right.to_source()
                ),
            );
        }
    }

    /// Joins the members of two ports by name (ADR 0117), when both sides are parts, modules
    /// or interfaces and not two interfaces of one kind (which `~` joins whole). Returns false,
    /// with nothing reported, when a side is something else, so that `~` joins it as before;
    /// true when it joined them or reported why not, a side that does not resolve included.
    pub(crate) fn join_ports_by_name(
        &mut self,
        left: &ast::Path,
        right: &ast::Path,
        renames: &[(Ident, Ident)],
        except_list: &[Ident],
        span: Span,
    ) -> bool {
        let (li, ri) = match (self.resolve_path(left), self.resolve_path(right)) {
            (Res::Inst(a), Res::Inst(c)) => (a, c),
            (Res::Poison | Res::Bad, _) | (_, Res::Poison | Res::Bad) => return true,
            _ => return false,
        };
        let port =
            |k: InstKind| matches!(k, InstKind::Part | InstKind::Module | InstKind::Interface);
        let (ia, ic) = (self.model.instance(li), self.model.instance(ri));
        if !port(ia.kind) || !port(ic.kind) {
            return false;
        }
        if ia.kind == InstKind::Interface && ic.kind == InstKind::Interface && ia.def == ic.def {
            return false;
        }
        let l = left.to_source();
        if li == ri {
            self.error(
                span,
                format!("`{l}` is joined to itself; `~` joins the members of two ports"),
            );
            return true;
        }
        let left = self.port_side(left, li);
        let right = self.port_side(right, ri);
        self.pair_and_join(left, right, renames, except_list, span);
        true
    }

    fn pair_and_join(
        &mut self,
        left: Side,
        right: Side,
        renames: &[(Ident, Ident)],
        except_list: &[Ident],
        span: Span,
    ) {
        let (l, r) = (left.src(), right.src());
        let mut ok = true;
        // The pairs, keyed by the left name, and what each right name is taken by.
        let mut pairs: BTreeMap<String, (String, Span)> = BTreeMap::new();
        let mut taken: BTreeSet<String> = BTreeSet::new();
        let mut except: BTreeSet<String> = BTreeSet::new();

        for name in except_list {
            if !except.insert(name.name.clone()) {
                self.error(
                    name.span,
                    format!("`{}` is left out twice; list it once", name.name),
                );
                ok = false;
                continue;
            }
            if left.get(&name.name).is_none() && right.get(&name.name).is_none() {
                let mut all = left.names();
                all.extend(right.names());
                let near = crate::lower::closest(&name.name, &all)
                    .map(|n| format!("; did you mean `{n}`?"))
                    .unwrap_or_default();
                self.error(
                    name.span,
                    format!(
                        "`except` leaves out `{}`, but neither `{l}` nor `{r}` has a pin or an interface of that name{near}",
                        name.name
                    ),
                );
                ok = false;
            }
        }

        for (from, to) in renames {
            let mut good = true;
            if from.name == to.name {
                self.error(
                    from.span,
                    format!(
                        "`{0} = {0}` renames nothing: members with the same name are joined without it; leave it out",
                        from.name
                    ),
                );
                ok = false;
                continue;
            }
            for (name, side, which) in [(from, &left, "left"), (to, &right, "right")] {
                if side.get(&name.name).is_none() {
                    self.no_such_member(name, side, which);
                    good = false;
                }
                if except.contains(&name.name) {
                    self.error(
                        name.span,
                        format!(
                            "`{}` is both renamed and left out with `except`; keep one",
                            name.name
                        ),
                    );
                    good = false;
                }
            }
            if pairs.contains_key(&from.name) {
                self.error(
                    from.span,
                    format!(
                        "`{}` of `{l}` is renamed twice; a member has one partner",
                        from.name
                    ),
                );
                good = false;
            }
            if !taken.insert(to.name.clone()) {
                self.error(
                    to.span,
                    format!(
                        "`{}` of `{r}` is the partner of two members; a member has one partner",
                        to.name
                    ),
                );
                good = false;
            }
            if good {
                pairs.insert(from.name.clone(), (to.name.clone(), to.span));
            } else {
                ok = false;
            }
        }
        if !ok {
            return;
        }

        // Every other member is paired with the member of the same name on the other side.
        for (name, _) in &left.members {
            if except.contains(name) || pairs.contains_key(name) {
                continue;
            }
            if right.get(name).is_some() && !taken.contains(name) {
                taken.insert(name.clone());
                pairs.insert(name.clone(), (name.clone(), span));
            }
        }
        let lone_left: Vec<&String> = left
            .members
            .iter()
            .map(|(n, _)| n)
            .filter(|n| !except.contains(*n) && !pairs.contains_key(*n))
            .collect();
        let lone_right: Vec<String> = right
            .members
            .iter()
            .map(|(n, _)| n.clone())
            .filter(|n| !except.contains(n) && !taken.contains(n))
            .collect();
        if !lone_left.is_empty() || !lone_right.is_empty() {
            let lone_left_names: Vec<String> = lone_left.iter().map(|n| (*n).clone()).collect();
            let mut parts = Vec::new();
            for n in &lone_left {
                let near = crate::lower::closest(n, &lone_right)
                    .map(|m| format!(" (did you mean `{n} = {m}`?)"))
                    .unwrap_or_default();
                parts.push(format!("`{l}.{n}`{near}"));
            }
            for n in &lone_right {
                // A near name on the left is already suggested from that side.
                let near = crate::lower::closest(n, &lone_left_names)
                    .filter(|m| crate::lower::closest(m, &lone_right).as_deref() != Some(n))
                    .map(|m| format!(" (did you mean `{m} = {n}`?)"))
                    .unwrap_or_default();
                parts.push(format!("`{r}.{n}`{near}"));
            }
            self.error(
                span,
                format!(
                    "{} {} no member of the same name on the other side: {}; give each a partner with a rename, `{{ a = b }}`, or leave it out with `except {{ a }}`",
                    if parts.len() == 1 { "this member" } else { "these members" },
                    if parts.len() == 1 { "has" } else { "have" },
                    parts.join(", ")
                ),
            );
        }

        // A pin pairs with a pin; an interface with an interface, joined whole when the two are
        // of one kind and by name again when not.
        let mut joins = Vec::new();
        for (name, (partner, at)) in &pairs {
            let a = left.get(name).expect("a paired left member exists");
            let c = right.get(partner).expect("a paired right member exists");
            let (pa, pc) = (
                self.model.instance(a).kind == InstKind::Pin,
                self.model.instance(c).kind == InstKind::Pin,
            );
            if pa != pc {
                let (ka, kc) = (self.kind_word(a), self.kind_word(c));
                self.error(
                    *at,
                    format!(
                        "`{l}.{name}` is {ka} and `{r}.{partner}` is {kc}; `~` joins a pin to a pin and an interface to an interface, so rename one or leave it out with `except`"
                    ),
                );
                continue;
            }
            joins.push((name.clone(), partner.clone(), *at));
        }
        for (name, partner, span) in joins {
            let c = ast::Connect {
                first: extend(&left.path, &name, span),
                rest: vec![(ConnectOp::Same, extend(&right.path, &partner, span))],
                groups: Vec::new(),
                span,
            };
            self.connect(&c);
        }
    }

    /// Whether an instance is a pin or an interface with a pin somewhere inside it.
    fn carries_pins(&self, inst: InstId) -> bool {
        let i = self.model.instance(inst);
        match i.kind {
            InstKind::Pin => true,
            InstKind::Interface => i.children.iter().any(|(_, m)| match m {
                Member::Inst(c) => self.carries_pins(*c),
                _ => false,
            }),
            _ => false,
        }
    }

    /// `a pin` or `a Rail`: what a member is, for the message about a pair of two kinds.
    fn kind_word(&self, inst: InstId) -> String {
        let i = self.model.instance(inst);
        match i.kind {
            InstKind::Pin => "a pin".to_string(),
            _ => format!("a `{}`", i.type_name),
        }
    }

    fn no_such_member(&mut self, name: &Ident, side: &Side, which: &str) {
        let what = if side.others.contains(&name.name) {
            "is not a pin or an interface, and `~` joins only those by name".to_string()
        } else {
            let near = crate::lower::closest(&name.name, &side.names())
                .map(|n| format!("; did you mean `{n}`?"))
                .unwrap_or_default();
            format!("is not a pin or an interface of it{near}")
        };
        self.error(
            name.span,
            format!(
                "the rename's {which} side is a member of `{}`, and `{}` {what}",
                side.src(),
                name.name
            ),
        );
    }

    /// A side of the statement: a part, a module or an interface, and its members.
    fn port_side(&mut self, p: &ast::Path, inst: InstId) -> Side {
        let mut members = Vec::new();
        let mut others = BTreeSet::new();
        let children = self.model.instance(inst).children.clone();
        for (name, m) in children {
            // A pin, or an interface with a pin in it: the built-in `.place` and `.model` and an
            // interface of values only (`Levels`) carry no connection to join.
            let joinable = match m {
                Member::Inst(c) => self.carries_pins(c),
                _ => false,
            };
            // A use of a generic reaches only the members the generic declares (ADR 0068).
            if joinable && self.outside_contract(inst, &name).is_none() {
                let Member::Inst(c) = m else { unreachable!() };
                members.push((name, c));
            } else {
                others.insert(name);
            }
        }
        Side {
            path: p.clone(),
            members,
            others,
        }
    }
}

/// `.headers` and `io4`: `.headers.io4`, with the new segment at `span`.
fn extend(path: &ast::Path, name: &str, span: Span) -> ast::Path {
    let mut p = path.clone();
    p.segments.push(Ident {
        name: name.to_string(),
        quoted: false,
        span,
    });
    p.span = span;
    p
}
