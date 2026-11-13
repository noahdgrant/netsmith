//! A pick lock: which catalog row each part of a board was picked as (see `GLOSSARY.md`).
//!
//! The solver reads a lock through [`Catalogs::with_pick_lock`](crate::Catalogs::with_pick_lock)
//! and offers a locked part only its locked row, judged by everything the design says. The file a
//! lock is kept in, and what a command does with it, are in `netsmith-pipeline` (`pick_lock`) and `netsmith-cli`.
//!
//! A hand pick (`.lcsc &= "C307331"` or `.mpn &= ...` in the source, ADR 0118) comes before the
//! lock: the lock records the solver's picks, and a hand pick is a requirement of the design.

use std::collections::BTreeMap;

use crate::solve::Pick;

/// The row one part was picked as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedRow {
    /// The part's path in the board, as the solver and the layout file name it (`r1`,
    /// `stage.output_cap`).
    pub path: String,
    /// The catalog the row is in (`resistor`).
    pub catalog: String,
    /// The row's name, as a message calls it (`Candidate::name`).
    pub row: String,
    /// The row's LCSC number and manufacturer part number when it is a real part, so that a
    /// row that was renamed or changed in place is found and not taken for the locked one.
    pub lcsc: Option<String>,
    pub mpn: Option<String>,
}

impl LockedRow {
    /// The lock entry a pick makes, for the part at `path`.
    pub fn of_pick(path: String, pick: &Pick) -> LockedRow {
        LockedRow {
            path,
            catalog: pick.catalog.clone(),
            row: pick.candidate.clone(),
            lcsc: pick.lcsc.clone(),
            mpn: pick.mpn.clone(),
        }
    }
}

/// The provider one use of a generic was picked as (ADR 0068, ADR 0113): a use with more than
/// one provider in scope, such as a board's `.process: TwoLayer`, whose copper weight picks
/// one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedProvider {
    /// The use's path in the board (`process`, `stage.reg`).
    pub path: String,
    /// The generic (`TwoLayer`).
    pub generic: String,
    /// The provider's id, its module and name (`std::mfg::jlcpcb::JlcTwoLayer1oz`): what an
    /// assignment of the checker names.
    pub provider: String,
}

/// The locked rows of every board and system of one source file, by the name of the board or
/// system (the name the solver solves it under), each with its parts in path order, and the
/// locked providers of its uses of generics, in path order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PickLock {
    /// What a message calls the file the lock is kept in.
    pub file: String,
    units: BTreeMap<String, Vec<LockedRow>>,
    providers: BTreeMap<String, Vec<LockedProvider>>,
}

impl PickLock {
    pub fn new(file: &str) -> PickLock {
        PickLock {
            file: file.to_string(),
            providers: BTreeMap::new(),
            units: BTreeMap::new(),
        }
    }

    /// Sets the rows of one board or system. They are kept in path order.
    pub fn set(&mut self, unit: &str, mut rows: Vec<LockedRow>) {
        rows.sort_by(|a, b| a.path.cmp(&b.path));
        self.units.insert(unit.to_string(), rows);
    }

    /// The rows of one board or system: none if the lock has no table for it.
    pub fn rows(&self, unit: &str) -> &[LockedRow] {
        self.units.get(unit).map_or(&[], Vec::as_slice)
    }

    /// Every board or system the lock has a table for, in name order, with its rows.
    pub fn units(&self) -> impl Iterator<Item = (&str, &[LockedRow])> {
        self.units.iter().map(|(n, r)| (n.as_str(), r.as_slice()))
    }

    /// Sets the providers of one board's or system's uses. They are kept in path order; a
    /// board with none has no entry.
    pub fn set_providers(&mut self, unit: &str, mut providers: Vec<LockedProvider>) {
        providers.sort_by(|a, b| a.path.cmp(&b.path));
        if providers.is_empty() {
            self.providers.remove(unit);
        } else {
            self.providers.insert(unit.to_string(), providers);
        }
    }

    /// The providers of one board or system: none if the lock has no table for it.
    pub fn providers(&self, unit: &str) -> &[LockedProvider] {
        self.providers.get(unit).map_or(&[], Vec::as_slice)
    }

    /// Every board or system the lock has providers for, in name order, with them.
    pub fn provider_units(&self) -> impl Iterator<Item = (&str, &[LockedProvider])> {
        self.providers
            .iter()
            .map(|(n, p)| (n.as_str(), p.as_slice()))
    }

    /// Whether no part and no provider is locked.
    pub fn is_empty(&self) -> bool {
        self.units.values().all(Vec::is_empty) && self.providers.values().all(Vec::is_empty)
    }
}
