//! A pick lock: which catalog row each part of a board was picked as (see `GLOSSARY.md`).
//!
//! The solver reads a lock through [`Catalogs::with_pick_lock`](crate::Catalogs::with_pick_lock)
//! and offers a locked part only its locked row, judged by everything the design says. The file a
//! lock is kept in, and what a command does with it, are in the `netsmith` crate.

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

/// The locked rows of every board and system of one source file, by the name of the board or
/// system (the name the solver solves it under), each with its parts in path order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PickLock {
    /// What a message calls the file the lock is kept in.
    pub file: String,
    units: BTreeMap<String, Vec<LockedRow>>,
}

impl PickLock {
    pub fn new(file: &str) -> PickLock {
        PickLock {
            file: file.to_string(),
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

    /// Whether no part is locked.
    pub fn is_empty(&self) -> bool {
        self.units.values().all(Vec::is_empty)
    }
}
