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
        // A part is its manufacturer and its number (ADR 0143): the skus the lock records are a
        // record of what the part was bought by, and a changed offer is a line of the lock's
        // diff, not another part.
        let same_number = |c: &Candidate| {
            locked
                .mpn
                .as_ref()
                .is_none_or(|m| c.mpn.as_ref() == Some(m))
                && locked
                    .manufacturer
                    .as_ref()
                    .is_none_or(|m| c.manufacturer.as_ref() == Some(m))
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
            // A row of the part's number under another name: a release added the part to the
            // built-in catalog, and the project's row was removed for it (issue #1119).
            let renamed = locked
                .mpn
                .as_ref()
                .and_then(|_| cat.candidates.iter().find(|c| same_number(c)))
                .map(|c| {
                    format!(
                        "; the row with the manufacturer part number {} is `{}` now, and `netsmith project lock --update` sets the lock to it",
                        c.mpn.as_deref().unwrap_or_default(),
                        c.name
                    )
                })
                .unwrap_or_default();
            Some(format!(
                "the `{}` catalog has no row `{}` now{renamed}",
                cat.name, locked.row
            ))
        } else if let Some(changed) = named.first().filter(|_| !named.iter().any(&same_number)) {
            // The manufacturer is named only when one of the two has one, so a lock and a row
            // that name none read as before.
            let maker = |m: &Option<String>| m.as_deref().unwrap_or("none").to_string();
            let makers = if changed.manufacturer.is_some() || locked.manufacturer.is_some() {
                format!(
                    " (the manufacturer {}, and the lock has {})",
                    maker(&changed.manufacturer),
                    maker(&locked.manufacturer)
                )
            } else {
                String::new()
            };
            Some(
                format!(
                    "the row `{}` of the `{}` catalog now has the manufacturer part number {}, and the lock has {}",
                    locked.row,
                    cat.name,
                    changed.mpn.as_deref().unwrap_or("none"),
                    locked.mpn.as_deref().unwrap_or("none"),
                ) + &makers,
            )
        } else {
            None
        };
        match reason {
            None => {
                // Two rows of one `mpn` from two manufacturers may share a name (the default is
                // the `mpn`): the lock's numbers say which of them it holds.
                cat.candidates
                    .retain(|c| c.name == locked.row && same_number(c));
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

    /// The narrowings of a part's catalog row that hold it to nothing, each an error at the
    /// narrowing, as a member that does not exist would be: a `.manufacturer` on a part that
    /// picks from no catalog, or with no `.mpn` beside it (it says which of two parts of one
    /// number the hand pick is, ADR 0143), and any `.lcsc`, the supplier's number a hand pick
    /// was once written with, whose message gives the `.mpn` to write, read from the offers.
    pub(super) fn refuse_stray_hand_picks(&mut self) {
        let text = |s: &Self, id: InstId, name: &str| match s.model.member(id, name) {
            Some(Member::Field(f)) => match s.domain(f) {
                Domain::Text(t) => Some((f, t.clone())),
                _ => None,
            },
            _ => None,
        };
        let parts: Vec<InstId> = self
            .model
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Part)
            .map(|(id, _)| id)
            .collect();
        for id in parts {
            let path = self.model.path_of(id);
            let inst = self.model.instance(id);
            let (ty, no_catalog, at) = (inst.type_name.clone(), inst.catalog.is_none(), inst.at());
            let place =
                |s: &Self, f: FieldId| s.model.field(f).narrowed_at.last().copied().unwrap_or(at);
            if let Some((f, sku)) = text(self, id, "lcsc") {
                let (source, span) = place(self, f);
                let message = self.lcsc_refusal(&path, &sku);
                self.report(source, span, message);
            }
            if let Some((f, maker)) = text(self, id, "manufacturer") {
                let (source, span) = place(self, f);
                if no_catalog {
                    self.report(
                        source,
                        span,
                        format!(
                            "`{path}.manufacturer` holds the part to a catalog row, but `{path}` (`{ty}`) picks from no catalog: remove the narrowing"
                        ),
                    );
                } else if text(self, id, "mpn").is_none() {
                    self.report(
                        source,
                        span,
                        format!(
                            "`{path}.manufacturer &= \"{maker}\"` says which of two parts of one number a hand pick is, and `{path}` has no `.mpn` narrowed: a hand pick is by `.mpn` (ADR 0143); narrow `.mpn` too, or remove the narrowing"
                        ),
                    );
                }
            }
        }
    }

    /// The error for `.lcsc &= "<sku>"` on the part at `path`: netsmith holds a part to a row by
    /// its manufacturer's number (ADR 0143), and the offer of that sku says which number it is.
    fn lcsc_refusal(&self, path: &str, sku: &str) -> String {
        let head = format!(
            "`.lcsc &= \"{sku}\"` on `{path}`: a hand pick is by the manufacturer's part number now, and a supplier's number is an offer (ADR 0143)"
        );
        let offers = self.catalogs.offers();
        let Some(offer) = offers.by_sku(sku) else {
            return format!(
                "{head}; no offer has the sku {sku}, so write `.mpn &= \"...\"` with the part number of the row it named (its `source` has it); `netsmith project migrate-offers` moves a project's catalog columns into its offers files and rewrites these narrowings"
            );
        };
        // The manufacturer is written only when the number alone is two parts.
        let shared = offers.of(&offer.supplier, None, &offer.mpn).is_err();
        let narrowing = match (&offer.manufacturer, shared) {
            (Some(m), true) => {
                format!("`.mpn &= \"{}\"` and `.manufacturer &= \"{m}\"`", offer.mpn)
            }
            _ => format!("`.mpn &= \"{}\"`", offer.mpn),
        };
        format!(
            "{head}: write {narrowing} in its place, the part the {} offer {sku} is of ({}); `netsmith project migrate-offers` rewrites these narrowings in a project's sources",
            offer.supplier, offer.at
        )
    }

    /// A part that the design holds to one row by its `.mpn`, and its `.manufacturer` when two
    /// parts share the number (a hand pick, ADR 0122, ADR 0143), is offered only that row, which
    /// every statement of the design and every relation then judge, as for a locked row. Two rows
    /// of one number from two manufacturers are both offered when the pick names no
    /// manufacturer, and the first that fits is taken, as for any rows. Returns `false` when the
    /// catalog has no such row, which it has reported at the narrowing.
    fn hold_to_choice(
        &mut self,
        part: InstId,
        path: &str,
        chosen: &HandPick,
        cat: &mut Catalog,
    ) -> bool {
        let (source, span) = chosen
            .at()
            .unwrap_or_else(|| self.model.instance(part).at());
        if cat.candidates.iter().any(|c| chosen.is(c)) {
            cat.candidates.retain(|c| chosen.is(c));
            return true;
        }
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

    /// A part that names its own supply channel (`[supply.parts]`, ADR 0143 item 4) is offered
    /// only the rows that channel has an offer of, each bought there: its sku and class are that
    /// offer's. A generic part is kept and names the channel, so that only such rows stand for
    /// it when it is resolved. Returns `false` when no row is left, which it has reported by
    /// the part's name and the manifest's entry: the part is left unresolved.
    pub(super) fn buy_in_named_channel(
        &mut self,
        part: InstId,
        path: &str,
        cat: &mut Catalog,
    ) -> bool {
        let Some(named) = self.named_channels.get(path).cloned() else {
            return true;
        };
        let had_rows = cat.candidates.iter().any(|c| c.generic.is_none());
        let candidates = std::mem::take(&mut cat.candidates);
        cat.candidates = candidates
            .into_iter()
            .filter_map(|mut c| match c.generic.as_mut() {
                Some(g) => {
                    g.channel = Some(named.clone());
                    Some(c)
                }
                None => self.catalogs.bought_in(&c, named.channel),
            })
            .collect();
        if !had_rows || !cat.candidates.is_empty() {
            return true;
        }
        let (source, span) = self.model.instance(part).at();
        self.report(
            source,
            span,
            format!(
                "`{path}` cannot be bought: no row of the `{}` catalog it could be has an offer in {}; add the offer of its row to the project's `offers/{}.csv`, or remove the entry to buy it from the project's channels",
                cat.name,
                named.named_by(),
                named.channel.supplier()
            ),
        );
        self.unresolved.push(Unresolved {
            part,
            reason: format!("no row it could be has an offer in `{}`", named.channel),
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
