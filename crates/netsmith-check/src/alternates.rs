//! The `alternate` statement (issue 1312): `.u: ModuleA; alternate .u: ModuleB`.
//!
//! **Decision** (owner, issue 1312). One footprint can take two parts, such as two radio
//! modules with the same pads. The board states the part it is drawn with and, beside it, each
//! other part the footprint may take. The checks of the board run with each fitted and say which
//! one fails; the bill of materials lists both, the alternate marked not fitted (DNP).
//!
//! **Readings** (the implementer's). `alternate` is followed by a declaration of one member, as
//! the member's own would be written, with a body that narrows it as that declaration's body
//! would. It names a member that the same body declares, a part of one member (not a group, a
//! list or a module), and a kind of part that is not the member's own. A board or a module holds
//! one, never a part or a physical file, and a module's alternate is one of each place the
//! module is used. When an alternate is fitted (`check_fitted`), its declaration stands in the
//! place of the member's: the rest of the source (connections, narrowings from outside) is read
//! as it is, so a pin the alternate lacks is refused where it is used. The checker records each
//! alternate on the model (`Model::alternates`); the pipeline checks, solves and designs the
//! board again with each fitted and compares the pads of the two parts.

use std::borrow::Cow;

use netsmith_lang::ast::{Alternate, DefKind, Item};

use crate::checker::{Checker, Ty};
use crate::model::{Alternate as Recorded, InstId, InstKind, Member};

impl Checker<'_> {
    /// `items`, with the declaration of each member whose alternate is fitted replaced by the
    /// alternate's. `inst` is the instance the items are the body of.
    pub(crate) fn fit_alternates<'a>(
        &self,
        inst: InstId,
        items: Cow<'a, [Item]>,
    ) -> Cow<'a, [Item]> {
        if self.fitted.is_empty() || !items.iter().any(|i| matches!(i, Item::Alternate(_))) {
            return items;
        }
        let prefix = self.member_path(inst);
        let mut seen: std::collections::BTreeMap<&str, usize> = Default::default();
        let mut chosen = Vec::new();
        for item in items.iter() {
            let Item::Alternate(a) = item else { continue };
            let name = a.decl.name.segments[0].name.as_str();
            let index = seen.entry(name).or_default();
            if self.fitted.get(&join(&prefix, name)) == Some(index) {
                chosen.push((name.to_string(), a.decl.clone()));
            }
            *index += 1;
        }
        if chosen.is_empty() {
            return items;
        }
        let mut items = items.into_owned();
        for item in &mut items {
            if let Item::Decl(d) = item
                && d.name.leading_dot
                && d.name.segments.len() == 1
                && let Some((_, fitted)) =
                    chosen.iter().find(|(n, _)| *n == d.name.segments[0].name)
            {
                *d = fitted.clone();
            }
        }
        Cow::Owned(items)
    }

    /// The path of `inst` from the root of the model, which is left out: `""` for the root.
    fn member_path(&self, inst: InstId) -> String {
        let mut names = Vec::new();
        let mut cur = inst;
        while let Some(parent) = self.model.instance(cur).parent {
            names.push(self.model.instance(cur).name.clone());
            cur = parent;
        }
        names.reverse();
        names.join(".")
    }

    /// Check an `alternate` statement once every member of `items`, the body it is in, exists,
    /// and record it on the model. Every refusal names the statement and what to write.
    pub(crate) fn alternate(&mut self, a: &Alternate, items: &[Item]) {
        let name = a.decl.name.segments[0].name.clone();
        let text = format!(
            "alternate .{name}: {}",
            a.decl
                .ty
                .segments
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join("::")
        );
        if self.phys.is_some() {
            self.error(
                a.span,
                format!("`{text}` belongs in the circuit source, beside the declaration of `.{name}`; a physical file says where parts go"),
            );
            return;
        }
        if !self.in_kind(DefKind::Board) && !self.in_kind(DefKind::Module) {
            self.error(
                a.span,
                format!("`{text}`: only a board or a module states an alternate, beside the declaration of the part it may take the place of"),
            );
            return;
        }
        let primary = items.iter().find_map(|i| match i {
            Item::Decl(d)
                if d.name.leading_dot
                    && d.name.segments.len() == 1
                    && d.name.segments[0].name == name =>
            {
                Some(d)
            }
            _ => None,
        });
        let group = format!("{name}[");
        let in_group = items.iter().any(|i| {
            matches!(i, Item::Decl(d) if d.name.segments.len() == 1 && d.name.segments[0].name.starts_with(&group))
        });
        if primary.is_none() && in_group {
            self.error(
                a.decl.name.span,
                format!("`{text}`: `.{name}` is not one part, and an alternate takes the place of one part on its footprint"),
            );
            return;
        }
        let Some(primary) = primary else {
            self.error(
                a.decl.name.span,
                format!("`{text}` takes the place of `.{name}`, which this body does not declare: declare the part the board is drawn with beside it (`.{name}: ModuleA`)"),
            );
            return;
        };
        let primary_kind = primary
            .ty
            .segments
            .last()
            .map_or(String::new(), |s| s.name.clone());
        let cur = self.current();
        let member = self.model.member(cur, &name);
        let is_part = matches!(member, Some(Member::Inst(i)) if self.model.instance(i).kind == InstKind::Part);
        if primary.count.is_some() || primary.list.is_some() {
            self.error(
                a.decl.name.span,
                format!("`{text}`: `.{name}` is not one part, and an alternate takes the place of one part on its footprint"),
            );
            return;
        }
        if !is_part {
            if let Some(Member::Inst(i)) = member {
                let what = match self.model.instance(i).kind {
                    InstKind::Module => "a module",
                    InstKind::Interface => "an interface",
                    InstKind::Board => "a board",
                    _ => "not a part",
                };
                self.error(
                    a.decl.name.span,
                    format!("`{text}`: `.{name}` is {what}, and an alternate takes the place of one part on its footprint"),
                );
            } else if let Some(Member::Field(_)) = member {
                self.error(
                    a.decl.name.span,
                    format!("`{text}`: `.{name}` is a value, and an alternate takes the place of one part on its footprint"),
                );
            }
            return;
        }
        let kind = a
            .decl
            .ty
            .segments
            .last()
            .map_or(String::new(), |s| s.name.clone());
        let part_shaped = match self.resolve_type(&a.decl.ty) {
            Ty::Def(m, i) => {
                let def = &self.modules[m].defs[i];
                match def.kind {
                    DefKind::Part => Some(true),
                    DefKind::Generic => Some(crate::providers::generic_is_part_shaped(def)),
                    _ => Some(false),
                }
            }
            Ty::Param { .. } => Some(true),
            Ty::Builtin(_) | Ty::Enum(..) => Some(false),
            // Already reported.
            Ty::Poison | Ty::Bad => None,
        };
        match part_shaped {
            None => return,
            Some(false) => {
                self.error(
                    a.span,
                    format!("`{text}`: `{kind}` is not a part, and an alternate is a part fitted on the footprint of `.{name}` in its place"),
                );
                return;
            }
            Some(true) => {}
        }
        if kind == primary_kind && primary.ty.args == a.decl.ty.args && self.fitted.is_empty() {
            self.error(
                a.span,
                format!("`{text}`: `.{name}` is a `{kind}` already; an alternate is another kind of part, and a different row of the same kind is a hand pick (`.{name}.lcsc &= \"C1234\"`)"),
            );
            return;
        }
        let index = self
            .model
            .alternates
            .iter()
            .filter(|r| r.path == join(&self.member_path(cur), &name))
            .count();
        let path = join(&self.member_path(cur), &name);
        let source = self.cur_source();
        self.model.alternates.push(Recorded {
            path,
            index,
            kind,
            primary: primary_kind,
            source,
            span: a.span,
        });
    }
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}
