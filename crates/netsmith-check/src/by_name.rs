//! `.left ~ .right by name { a = b } except { c }`: the members of two ports joined name by
//! name (ADR 0117). The statement pairs the pins and interfaces of the two sides, refuses a pair
//! of different kinds and a member left with no partner, and then joins each pair as `~` does.

use std::collections::{BTreeMap, BTreeSet};

use netsmith_lang::ast::{self, ConnectOp, Ident};
use netsmith_lang::error::Span;

use crate::checker::{Checker, Res};
use crate::model::{InstId, InstKind, Member};

/// One side of the statement: the port and its joinable members in declaration order.
struct Side {
    path: ast::Path,
    inst: InstId,
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
    pub(crate) fn join_by_name(&mut self, b: &ast::ByName) {
        let (Some(left), Some(right)) = (self.by_name_side(&b.left), self.by_name_side(&b.right))
        else {
            return;
        };
        let (l, r) = (left.src(), right.src());
        let (li, ri) = (left.inst, right.inst);
        if li == ri {
            self.error(
                b.span,
                format!("`{l}` is joined to itself; `by name` joins the members of two ports"),
            );
            return;
        }
        let (lk, rk) = (self.model.instance(li).kind, self.model.instance(ri).kind);
        let (lt, rt) = (
            self.model.instance(li).type_name.clone(),
            self.model.instance(ri).type_name.clone(),
        );
        if lk == InstKind::Interface && rk == InstKind::Interface && lt == rt {
            self.error(
                b.span,
                format!(
                    "`{l}` and `{r}` are both `{lt}`, and `~` already joins two interfaces of the same kind member by member; write `{l} ~ {r}`"
                ),
            );
            return;
        }

        let mut ok = true;
        // The pairs, keyed by the left name, and what each right name is taken by.
        let mut pairs: BTreeMap<String, (String, Span)> = BTreeMap::new();
        let mut taken: BTreeSet<String> = BTreeSet::new();
        let mut except: BTreeSet<String> = BTreeSet::new();

        for name in &b.except {
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

        for (from, to) in &b.renames {
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
                pairs.insert(name.clone(), (name.clone(), b.span));
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
                b.span,
                format!(
                    "{} {} no member of the same name on the other side: {}; give each a partner with a rename, `by name {{ a = b }}`, or leave it out with `except {{ a }}`",
                    if parts.len() == 1 { "this member" } else { "these members" },
                    if parts.len() == 1 { "has" } else { "have" },
                    parts.join(", ")
                ),
            );
        }

        // The kinds of each pair agree: a pin with a pin, an interface with one of its kind.
        let mut joins = Vec::new();
        for (name, (partner, span)) in &pairs {
            let a = left.get(name).expect("a paired left member exists");
            let c = right.get(partner).expect("a paired right member exists");
            let (ka, kc) = (self.kind_word(a), self.kind_word(c));
            if ka != kc {
                self.error(
                    *span,
                    format!(
                        "`{l}.{name}` is {ka} and `{r}.{partner}` is {kc}; `by name` joins a pin to a pin and an interface to an interface of the same kind, so rename one or leave it out with `except`"
                    ),
                );
                continue;
            }
            joins.push((name.clone(), partner.clone(), *span));
        }
        for (name, partner, span) in joins {
            let c = ast::Connect {
                first: extend(&left.path, &name, span),
                rest: vec![(ConnectOp::Same, extend(&right.path, &partner, span))],
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
            "is not a pin or an interface, and `by name` joins only those".to_string()
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
    fn by_name_side(&mut self, p: &ast::Path) -> Option<Side> {
        let inst = match self.resolve_path(p) {
            Res::Inst(i) => i,
            Res::Field(_) => {
                self.error(
                    p.span,
                    format!(
                        "`{}` is a value; `by name` joins the members of two parts, modules or interfaces",
                        p.to_source()
                    ),
                );
                return None;
            }
            Res::Poison | Res::Bad => return None,
        };
        let kind = self.model.instance(inst).kind;
        if !matches!(
            kind,
            InstKind::Part | InstKind::Module | InstKind::Interface
        ) {
            self.error(
                p.span,
                format!(
                    "`{}` is a {}; `by name` joins the members of two parts, modules or interfaces",
                    p.to_source(),
                    kind.word()
                ),
            );
            return None;
        }
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
        Some(Side {
            path: p.clone(),
            inst,
            members,
            others,
        })
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
