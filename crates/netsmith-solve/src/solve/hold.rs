//! Holding a part to a row of a pick lock, or to a stated choice.

use super::{HandPick, Solver, Unresolved};
use crate::catalog::{Candidate, Catalog};
use netsmith_check::{Domain, FieldId, FieldKind, InstId, InstKind, Member};

impl Solver<'_> {
    /// A part held to one row (`Catalogs::with_pin`) stands in for a part the design stated
    /// a value for. The design's statements about the numbers of its catalog columns (its
    /// value, a rating) are what the alternative replaces, so they are dropped before
    /// anything is propagated from them; the statements about text (the package, the
    /// colour) and every relation stay, and judge the alternative.
    pub(super) fn relax_pinned(&mut self) {
        let parts: Vec<(InstId, String)> = self
            .model
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Part && i.catalog.is_some())
            .map(|(id, _)| (id, self.model.path_of(id)))
            .filter(|(_, path)| self.catalogs.pinned(path).is_some())
            .collect();
        for (id, _) in parts {
            let fields = self
                .model
                .instance(id)
                .catalog
                .as_ref()
                .map(|c| c.fields.clone())
                .unwrap_or_default();
            for f in fields {
                if matches!(self.model.field(f).kind, FieldKind::Quantity(_)) {
                    self.model.set_domain(f, Domain::Unset);
                }
            }
        }
    }

    /// A part that the pick lock names is offered only its locked row, which every statement of
    /// the design and every relation then judge. Returns `false` when there is no such row, which
    /// it has reported: the part is left unresolved, and no other row is tried in its place.
    pub(super) fn hold_to_lock(&mut self, part: InstId, path: &str, cat: &mut Catalog) -> bool {
        // A part held to a row by `with_pin` (a check of an alternative) is that one's.
        if self.catalogs.pinned(path).is_some() {
            return true;
        }
        if let Some(chosen) = self.chosen.get(path).cloned() {
            return self.hold_to_choice(part, path, &chosen, cat);
        }
        let Some(locked) = self.locked.get(path).cloned() else {
            return true;
        };
        let file = self.lock_file();
        let same_number = |c: &Candidate| {
            locked
                .lcsc
                .as_ref()
                .is_none_or(|l| c.lcsc.as_ref() == Some(l))
                && locked
                    .mpn
                    .as_ref()
                    .is_none_or(|m| c.mpn.as_ref() == Some(m))
        };
        let named: Vec<Candidate> = cat
            .candidates
            .iter()
            .filter(|c| c.name == locked.row)
            .cloned()
            .collect();
        let reason = if locked.catalog != cat.name {
            Some(format!(
                "the part now picks from the `{}` catalog, and the lock has it in `{}`",
                cat.name, locked.catalog
            ))
        } else if named.is_empty() {
            Some(format!(
                "the `{}` catalog has no row `{}` now",
                cat.name, locked.row
            ))
        } else if let Some(changed) = named.iter().find(|c| !same_number(c)) {
            Some(format!(
                "the row `{}` of the `{}` catalog now has the LCSC number {} and the manufacturer part number {}, and the lock has {} and {}",
                locked.row,
                cat.name,
                changed.lcsc.as_deref().unwrap_or("none"),
                changed.mpn.as_deref().unwrap_or("none"),
                locked.lcsc.as_deref().unwrap_or("none"),
                locked.mpn.as_deref().unwrap_or("none"),
            ))
        } else {
            None
        };
        match reason {
            None => {
                cat.candidates.retain(|c| c.name == locked.row);
                true
            }
            Some(reason) => {
                let (source, span) = self.model.instance(part).at();
                self.report(
                    source,
                    span,
                    format!(
                        "`{path}` is locked to the row `{}` of the `{}` catalog by {file}, and {reason}: no other row was tried for it; change the design or the catalog, or delete {file} to pick again",
                        locked.row, locked.catalog
                    ),
                );
                self.unresolved.push(Unresolved {
                    part,
                    reason: "the row its lock holds it to is not in the catalog".into(),
                });
                false
            }
        }
    }

    /// A `.lcsc` narrowed on a part that picks from no catalog names a row of nothing (ADR
    /// 0121): an error at the narrowing, as a member that does not exist would be.
    pub(super) fn refuse_stray_hand_picks(&mut self) {
        let stray: Vec<(InstId, FieldId)> = self
            .model
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Part && i.catalog.is_none())
            .filter_map(|(id, _)| match self.model.member(id, "lcsc") {
                Some(Member::Field(f)) => Some((id, f)),
                _ => None,
            })
            .filter(|(_, f)| matches!(self.domain(*f), Domain::Text(_)))
            .collect();
        for (id, f) in stray {
            let path = self.model.path_of(id);
            let inst = self.model.instance(id);
            let ty = inst.type_name.clone();
            let (source, span) = self
                .model
                .field(f)
                .narrowed_at
                .last()
                .copied()
                .unwrap_or_else(|| inst.at());
            self.report(
                source,
                span,
                format!(
                    "`{path}.lcsc` holds the part to a catalog row, but `{path}` (`{ty}`) picks from no catalog: remove the narrowing"
                ),
            );
        }
    }

    /// A part that the design holds to one row by its `.lcsc` or `.mpn` (a hand pick, ADR 0122)
    /// is offered only that row, which every statement of the design and every relation then
    /// judge, as for a locked row. Returns `false` when the catalog has no such row, which it has
    /// reported at the narrowing.
    fn hold_to_choice(
        &mut self,
        part: InstId,
        path: &str,
        chosen: &HandPick,
        cat: &mut Catalog,
    ) -> bool {
        if cat.candidates.iter().any(|c| chosen.is(c)) {
            cat.candidates.retain(|c| chosen.is(c));
            return true;
        }
        let (source, span) = chosen.at.unwrap_or_else(|| self.model.instance(part).at());
        self.report(
            source,
            span,
            format!(
                "`{path}` is held by {} to a row of the `{}` catalog, and the catalog has no row with {}: no other row was tried for it; change the narrowing to a number the catalog has, or remove it to let the solver pick",
                chosen.statement(),
                cat.name,
                chosen.number()
            ),
        );
        self.unresolved.push(Unresolved {
            part,
            reason: "the catalog has no row with the number its hand pick names".into(),
        });
        false
    }

    /// What a message calls the file of the pick lock.
    pub(super) fn lock_file(&self) -> String {
        self.catalogs
            .pick_lock()
            .map_or_else(String::new, |l| l.file.clone())
    }
}
