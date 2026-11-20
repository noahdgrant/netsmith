//! A pick lock: which catalog row each part of a board was picked as (see `GLOSSARY.md`).
//!
//! The solver reads a lock through [`Catalogs::with_pick_lock`](crate::Catalogs::with_pick_lock)
//! and offers a locked part only its locked row, judged by everything the design says. The file a
//! lock is kept in, and what a command does with it, are in `netsmith-pipeline` (`pick_lock`) and `netsmith-cli`.
//!
//! A hand pick (`.mpn &= "CL21A106KAYNNNE"` in the source, ADR 0122, ADR 0143) comes before the
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
    /// The row's manufacturer and manufacturer part number when it is a real part and the row
    /// names them, so that a row that was renamed or changed in place is found and not taken for
    /// the locked one: they are the part's identity (ADR 0143). A lock of version 1 has no
    /// manufacturer.
    pub manufacturer: Option<String>,
    pub mpn: Option<String>,
    /// The sku the part resolved to for each supply channel that has an offer of it (ADR 0143),
    /// by the channel's word (`jlcpcb-assembly`), in the order of [`CHANNELS`]. A record: a
    /// change of an offer's sku shows in the lock's diff, and the solver does not read it.
    pub skus: Vec<(String, String)>,
    /// Why a person holds the part to this row (`held = "reason"` in the lock file, issue
    /// #1197): `project lock --check` does not call the row not current, and `--update` keeps it
    /// unless the part is named. The solver does not read it; a person writes it.
    pub held: Option<String>,
}

impl LockedRow {
    /// The lock entry a pick makes, for the part at `path`.
    pub fn of_pick(path: String, pick: &Pick) -> LockedRow {
        LockedRow {
            path,
            catalog: pick.catalog.clone(),
            row: pick.candidate.clone(),
            skus: pick.skus.clone(),
            manufacturer: pick.manufacturer.clone(),
            mpn: pick.mpn.clone(),
            held: None,
        }
    }
}

/// The supply channels a lock may record a sku for, by their words, in the order a lock lists
/// them: every channel netsmith knows ([`crate::Channel::ALL`]).
pub const CHANNELS: [&str; 4] = [
    crate::Channel::ALL[0].word,
    crate::Channel::ALL[1].word,
    crate::Channel::ALL[2].word,
    crate::Channel::ALL[3].word,
];

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
    /// What makes the file of an older format than the one this netsmith writes, as the first
    /// line of a diff says it (`version 1 -> 2: ...` for a lock that records no manufacturer, or
    /// the line for one that records each part's `lcsc` and no sku, ADR 0143): the lock is read
    /// and holds its parts as before, and `project lock --check` calls it not current. `None`
    /// for a lock of the current format and for one a run makes.
    pub outdated: Option<String>,
    units: BTreeMap<String, Vec<LockedRow>>,
    providers: BTreeMap<String, Vec<LockedProvider>>,
}

impl PickLock {
    pub fn new(file: &str) -> PickLock {
        PickLock {
            file: file.to_string(),
            outdated: None,
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
