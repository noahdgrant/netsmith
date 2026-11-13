//! Holding a part to a row of a pick lock, or to a stated choice.

use super::{Solver, Unresolved};
use crate::catalog::{Candidate, Catalog};
use crate::lock::ChosenRow;
use netsmith_check::{Domain, FieldKind, InstId, InstKind};

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

    /// A part that a picks file names is offered only the row a person chose for it (#462), which
    /// every statement of the design and every relation then judge, as for a locked row. Returns
    /// `false` when the catalog has no such row, which it has reported.
    fn hold_to_choice(
        &mut self,
        part: InstId,
        path: &str,
        chosen: &ChosenRow,
        cat: &mut Catalog,
    ) -> bool {
        let file = self.picks_file();
        let is_chosen = |c: &Candidate| match (&chosen.lcsc, &chosen.mpn) {
            (Some(l), _) => c.lcsc.as_ref() == Some(l),
            (None, Some(m)) => c.mpn.as_ref() == Some(m),
            (None, None) => false,
        };
        let reason = if chosen.catalog != cat.name {
            Some(format!(
                "the part picks from the `{}` catalog, and {file} names `{}`",
                cat.name, chosen.catalog
            ))
        } else if !cat.candidates.iter().any(is_chosen) {
            Some(format!(
                "the `{}` catalog has no row with {}",
                cat.name,
                chosen.number()
            ))
        } else {
            None
        };
        match reason {
            None => {
                cat.candidates.retain(is_chosen);
                true
            }
            Some(reason) => {
                let (source, span) = self.model.instance(part).at();
                self.report(
                    source,
                    span,
                    format!(
                        "`{path}` is picked as the row with {} of the `{}` catalog by {file}, and {reason}: no other row was tried for it; change the entry in {file}, or remove it to let the solver pick",
                        chosen.number(),
                        chosen.catalog
                    ),
                );
                self.unresolved.push(Unresolved {
                    part,
                    reason: "the row its picks file chose is not in the catalog".into(),
                });
                false
            }
        }
    }

    /// What a message calls the picks file.
    pub(super) fn picks_file(&self) -> String {
        self.catalogs
            .picks()
            .map_or_else(String::new, |p| p.file.clone())
    }

    /// What a message calls the file of the pick lock.
    pub(super) fn lock_file(&self) -> String {
        self.catalogs
            .pick_lock()
            .map_or_else(String::new, |l| l.file.clone())
    }
}
