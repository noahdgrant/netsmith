//! The pick lock beside a source file: `<stem>.lock.toml` (see `GLOSSARY.md`, ADR 0100).
//!
//! It says which catalog row each part of each board was picked as, so that a change of a
//! catalog, of the solver or of the order of the rows does not move a pick without a line in a
//! pull request. `build` and `fab` write it when it does not exist and read it when it does:
//! the solver then offers each locked part only its locked row, and judges that row by
//! everything the design says (`netsmith_solve::Catalogs::with_pick_lock`). A lock that no
//! longer matches the picks is told, and with `--locked` is an error; it is never rewritten
//! behind a person's back. `netsmith project lock --update` rewrites it on purpose and `project lock --check`
//! says whether it is current ([`refresh`]).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use netsmith_check::{Assignment, Loader, Model};
use netsmith_solve::lock::CHANNELS;
use netsmith_solve::{Catalogs, LockedProvider, LockedRow, PickLock, Solution};

use crate::{outcome_with, render_problems};

/// The version of the file's format. Version 2 (ADR 0143) records each part's `manufacturer`
/// and `mpn`, its identity, and the `sku` of its offer for each supply channel
/// (`sku = { jlcpcb-assembly = "C25104", lcsc-retail = "C25104" }`), where version 1 recorded
/// the part's `lcsc`. A file of version 1, and one of version 2 that still records `lcsc` (as the
/// lock did before the catalogs lost the column), is still read, holds its parts as before, and
/// is not current, so `project lock --check` says so and `project lock --update` writes it again.
const VERSION: i64 = 2;

/// The versions of the file this netsmith reads.
const READS: [i64; 2] = [1, 2];

/// The first lines of a written file.
const HEADER: &str = "# The catalog row each part of each board was picked as, written by `netsmith board build` and\n# `netsmith board fab` when this file does not exist. Commit it: a change to a catalog or to the\n# solver that would move a pick shows here and not only in the bill of materials. Refresh it\n# with `netsmith project lock --update`. Nothing in it is edited by hand but `held = \"reason\"`,\n# which a person adds to a part's entry to keep the part on its row.\n";

/// The lock of a source file: `led.nsm` has `led.lock.toml`.
pub fn lock_path(source: &Path) -> PathBuf {
    source.with_extension("lock.toml")
}

/// What a command does with the lock of one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use {
    pub file: PathBuf,
    /// `--locked`: the lock must exist and must not change.
    pub locked: bool,
    /// The project says this source has no lock (`lock` in `netsmith.toml`): the file is not
    /// read, written or compared, and `--locked` has nothing to refuse.
    pub off: bool,
}

/// The rows a solution picked, one for each pick, named by the part's path.
pub fn rows_of(solution: &Solution) -> Vec<LockedRow> {
    solution
        .picks
        .iter()
        .map(|p| LockedRow::of_pick(solution.model.path_of(p.part), p))
        .collect()
}

/// The provider each use of a generic with more than one provider in scope was picked as, by
/// the use's path (ADR 0113). A use with one provider, or whose pick is the generic itself,
/// has no choice to hold.
pub fn providers_of(model: &Model) -> Vec<LockedProvider> {
    model
        .uses
        .iter()
        .filter(|u| u.candidates.len() > 1)
        .filter_map(|u| {
            let chosen = u.candidates.get(u.chosen)?;
            (!chosen.own).then(|| LockedProvider {
                path: u.path.clone(),
                generic: u.generic.clone(),
                provider: chosen.id.clone(),
            })
        })
        .collect()
}

/// The providers a lock holds the uses of each board and system to, as the checker's
/// assignment (ADR 0113).
pub fn held_providers(lock: Option<&PickLock>) -> Assignment {
    let mut held = Assignment::new();
    if let Some(lock) = lock {
        for (unit, providers) in lock.provider_units() {
            held.insert(
                unit.to_string(),
                providers
                    .iter()
                    .map(|p| (p.path.clone(), p.provider.clone()))
                    .collect(),
            );
        }
    }
    held
}

/// The lock in the file at `file`, or `None` if there is no such file. A file that cannot be
/// read or is wrong is an error naming it and the place in it.
pub fn read(file: &Path) -> Result<Option<PickLock>, String> {
    let name = file.display().to_string();
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("error: {name}: {e}")),
    };
    parse(&text, &name).map(Some)
}

/// The catalogs to solve one source file with, and what to do with its lock afterwards: the
/// catalogs hold the parts to the rows of the lock, when it exists. `None` when it does not. A
/// picks file beside the source is an error that says how to move it into the source (ADR 0122).
/// With `applies` false (the project's `lock` key leaves this source out) the file is not read:
/// no catalogs, and a [`Use`] that [`settle`] does nothing with.
pub fn open(
    source: &Path,
    catalogs: &Catalogs,
    locked: bool,
    applies: bool,
) -> Result<(Option<Catalogs>, Use), String> {
    crate::picks::refuse(source)?;
    let file = lock_path(source);
    if !applies {
        return Ok((
            None,
            Use {
                file,
                locked,
                off: true,
            },
        ));
    }
    let held = read(&file)?.map(|lock| catalogs.clone().with_pick_lock(lock));
    Ok((
        held,
        Use {
            file,
            locked,
            off: false,
        },
    ))
}

fn parse(text: &str, name: &str) -> Result<PickLock, String> {
    let wrong = |what: String| format!("error: {name}: {what}");
    let table: toml::Table = text.parse().map_err(|e| wrong(format!("{e}")))?;
    let version = match table.get("version") {
        Some(toml::Value::Integer(v)) if READS.contains(v) => *v,
        Some(other) => {
            return Err(wrong(format!(
                "version {other} is not a version of the lock this netsmith reads (version {VERSION}); delete the file to write it again"
            )));
        }
        None => return Err(wrong("it has no `version`".to_string())),
    };
    let mut lock = PickLock::new(name);
    lock.outdated = (version < VERSION).then(|| outdated_line(version));
    let mut records_lcsc = false;
    for (unit, value) in &table {
        if unit == "version" {
            continue;
        }
        let unit_table = value.as_table().ok_or_else(|| {
            wrong(format!(
                "`{unit}` must be a table of a board's or system's parts, not {value}"
            ))
        })?;
        if let Some(extra) = unit_table
            .keys()
            .find(|k| *k != "parts" && *k != "providers")
        {
            return Err(wrong(format!(
                "`[{unit}]` has `{extra}`; it holds only `parts` and `providers`"
            )));
        }
        let providers = match unit_table.get("providers") {
            None => Vec::new(),
            Some(value) => {
                let table = value.as_table().ok_or_else(|| {
                    wrong(format!("`[{unit}.providers]` must be a table of uses"))
                })?;
                parse_providers(unit, table, &wrong)?
            }
        };
        lock.set_providers(unit, providers);
        let parts = match unit_table.get("parts") {
            Some(parts) => parts
                .as_table()
                .ok_or_else(|| wrong(format!("`[{unit}.parts]` must be a table of parts")))?,
            None if unit_table.contains_key("providers") => continue,
            None => return Err(wrong(format!("`[{unit}]` has no table `parts`"))),
        };
        let mut rows = Vec::new();
        for (path, entry) in parts {
            let at = format!("[{unit}.parts.\"{path}\"]");
            let entry = entry
                .as_table()
                .ok_or_else(|| wrong(format!("{at} must be a table")))?;
            if let Some(extra) = entry.keys().find(|k| {
                ![
                    "catalog",
                    "row",
                    "lcsc",
                    "manufacturer",
                    "mpn",
                    "sku",
                    "held",
                ]
                .contains(&k.as_str())
            }) {
                return Err(wrong(format!(
                    "{at} has `{extra}`; it holds `catalog`, `row`, `manufacturer`, `mpn`, `sku` and `held`"
                )));
            }
            let text = |key: &str, needed: bool| -> Result<Option<String>, String> {
                match entry.get(key) {
                    Some(toml::Value::String(s)) => Ok(Some(s.clone())),
                    Some(other) => Err(wrong(format!("{at} `{key}` must be text, not {other}"))),
                    None if needed => Err(wrong(format!("{at} has no `{key}`"))),
                    None => Ok(None),
                }
            };
            // The number a lock recorded before a part's supplier numbers were its offers (ADR
            // 0143): read, so that the file still holds its parts, and the lock is not current.
            if text("lcsc", false)?.is_some() {
                records_lcsc = true;
            }
            let skus = match entry.get("sku") {
                None => Vec::new(),
                Some(toml::Value::Table(t)) => {
                    let mut skus = Vec::new();
                    for channel in CHANNELS {
                        match t.get(channel) {
                            None => {}
                            Some(toml::Value::String(sku)) => {
                                skus.push((channel.to_string(), sku.clone()))
                            }
                            Some(other) => {
                                return Err(wrong(format!(
                                    "{at} `sku.{channel}` must be text, not {other}"
                                )));
                            }
                        }
                    }
                    if let Some(other) = t.keys().find(|k| !CHANNELS.contains(&k.as_str())) {
                        return Err(wrong(format!(
                            "{at} `sku` has `{other}`, which is not a supply channel netsmith knows: {}",
                            netsmith_solve::Channel::words()
                        )));
                    }
                    skus
                }
                Some(other) => {
                    return Err(wrong(format!(
                        "{at} `sku` must be a table of a sku for each channel (`sku = {{ jlcpcb-assembly = \"C25104\" }}`), not {other}"
                    )));
                }
            };
            rows.push(LockedRow {
                path: path.clone(),
                catalog: text("catalog", true)?.unwrap_or_default(),
                row: text("row", true)?.unwrap_or_default(),
                skus,
                manufacturer: text("manufacturer", false)?,
                mpn: text("mpn", false)?,
                held: match text("held", false)? {
                    Some(reason) if reason.trim().is_empty() => {
                        return Err(wrong(format!(
                            "{at} `held` is empty: it says why the part is held to its row"
                        )));
                    }
                    held => held,
                },
            });
        }
        lock.set(unit, rows);
    }
    if records_lcsc && lock.outdated.is_none() {
        lock.outdated = Some(
            "lcsc -> sku: the file records each part's `lcsc`, and the lock now records the sku of the part's offer for each supply channel in its place (ADR 0143)".to_string(),
        );
    }
    Ok(lock)
}

/// The `[<unit>.providers."<path>"]` entries of one board or system: each its `generic` and
/// its `provider`.
fn parse_providers(
    unit: &str,
    table: &toml::Table,
    wrong: &dyn Fn(String) -> String,
) -> Result<Vec<LockedProvider>, String> {
    let mut providers = Vec::new();
    for (path, entry) in table {
        let at = format!("[{unit}.providers.\"{path}\"]");
        let entry = entry
            .as_table()
            .ok_or_else(|| wrong(format!("{at} must be a table")))?;
        if let Some(extra) = entry
            .keys()
            .find(|k| !["generic", "provider"].contains(&k.as_str()))
        {
            return Err(wrong(format!(
                "{at} has `{extra}`; it holds `generic` and `provider`"
            )));
        }
        let text = |key: &str| -> Result<String, String> {
            match entry.get(key) {
                Some(toml::Value::String(s)) => Ok(s.clone()),
                Some(other) => Err(wrong(format!("{at} `{key}` must be text, not {other}"))),
                None => Err(wrong(format!("{at} has no `{key}`"))),
            }
        };
        providers.push(LockedProvider {
            path: path.clone(),
            generic: text("generic")?,
            provider: text("provider")?,
        });
    }
    Ok(providers)
}

/// A TOML key: bare when it can be, else quoted.
fn key(text: &str) -> String {
    if !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        text.to_string()
    } else {
        string(text)
    }
}

/// A TOML basic string.
fn string(text: &str) -> String {
    toml::Value::String(text.to_string()).to_string()
}

/// The text of a lock file: boards in name order, parts in path order, nothing that depends on
/// the run, so that two runs write the same bytes.
pub fn render(lock: &PickLock) -> String {
    let mut out = String::from(HEADER);
    let _ = writeln!(out, "version = {VERSION}");
    let mut units: Vec<&str> = lock
        .units()
        .map(|(u, _)| u)
        .chain(lock.provider_units().map(|(u, _)| u))
        .collect();
    units.sort_unstable();
    units.dedup();
    for unit in units {
        // The providers first: a provider decides which parts there are.
        for p in lock.providers(unit) {
            let _ = writeln!(out, "\n[{}.providers.{}]", key(unit), string(&p.path));
            let _ = writeln!(out, "generic = {}", string(&p.generic));
            let _ = writeln!(out, "provider = {}", string(&p.provider));
        }
        for row in lock.rows(unit) {
            let _ = writeln!(out, "\n[{}.parts.{}]", key(unit), string(&row.path));
            let _ = writeln!(out, "catalog = {}", string(&row.catalog));
            let _ = writeln!(out, "row = {}", string(&row.row));
            if let Some(manufacturer) = &row.manufacturer {
                let _ = writeln!(out, "manufacturer = {}", string(manufacturer));
            }
            if let Some(mpn) = &row.mpn {
                let _ = writeln!(out, "mpn = {}", string(mpn));
            }
            if !row.skus.is_empty() {
                let skus: Vec<String> = row
                    .skus
                    .iter()
                    .map(|(channel, sku)| format!("{} = {}", key(channel), string(sku)))
                    .collect();
                let _ = writeln!(out, "sku = {{ {} }}", skus.join(", "));
            }
            if let Some(reason) = &row.held {
                let _ = writeln!(out, "held = {}", string(reason));
            }
        }
    }
    out
}

/// A row as a line of a diff names it: its name, and the numbers it is a check against.
fn describe(row: &LockedRow, with_catalog: bool) -> String {
    let mut text = format!("`{}`", row.row);
    if with_catalog {
        text.push_str(&format!(" in the `{}` catalog", row.catalog));
    }
    let numbers: Vec<String> = [("manufacturer", &row.manufacturer), ("mpn", &row.mpn)]
        .into_iter()
        .filter_map(|(name, number)| number.as_ref().map(|n| format!("{name} {n}")))
        .chain(
            row.skus
                .iter()
                .map(|(channel, sku)| format!("{channel} {sku}")),
        )
        .collect();
    if !numbers.is_empty() {
        text.push_str(&format!(" ({})", numbers.join(", ")));
    }
    text
}

/// The line that says a lock is of an older version of the format: what `project lock --check`
/// prints for it and `--update` changes.
fn outdated_line(version: i64) -> String {
    format!(
        "version {version} -> {VERSION}: the file is of version {version} of the lock, and version {VERSION} records each part's manufacturer and mpn, and the sku of its offer for each supply channel in place of its lcsc (ADR 0143)"
    )
}

/// What differs between two locks, one line for each part, in the order board, part path:
/// `Board: `r1`: `A` (mpn M1, jlcpcb-assembly C1) -> `B` (mpn M2, jlcpcb-assembly C2)` for a part whose row, or whose
/// numbers, changed, `added as ...` for a part the old lock has no entry for, and `dropped (was
/// ...)` for an entry of a part that is no longer there. Empty when the locks hold the same.
pub fn diff(old: &PickLock, new: &PickLock) -> Vec<String> {
    let mut found: Vec<(String, String, String)> = Vec::new();
    for (unit, rows) in new.units() {
        let before = old.rows(unit);
        for row in rows {
            let line = match before.iter().find(|b| b.path == row.path) {
                None => format!("{unit}: `{}`: added as {}", row.path, describe(row, true)),
                Some(b) if b != row => {
                    let same_row = (&b.catalog, &b.row, &b.skus, &b.manufacturer, &b.mpn)
                        == (
                            &row.catalog,
                            &row.row,
                            &row.skus,
                            &row.manufacturer,
                            &row.mpn,
                        );
                    let with_catalog = b.catalog != row.catalog;
                    let hold = match (&b.held, &row.held) {
                        (Some(was), None) => format!("no longer held: {was}"),
                        (None, Some(now)) => format!("now held: {now}"),
                        (Some(was), Some(now)) => format!("held: {was} -> {now}"),
                        (None, None) => String::new(),
                    };
                    if same_row {
                        format!("{unit}: `{}`: {hold}", row.path)
                    } else if hold.is_empty() {
                        format!(
                            "{unit}: `{}`: {} -> {}",
                            row.path,
                            describe(b, with_catalog),
                            describe(row, with_catalog)
                        )
                    } else {
                        format!(
                            "{unit}: `{}`: {} -> {} ({hold})",
                            row.path,
                            describe(b, with_catalog),
                            describe(row, with_catalog)
                        )
                    }
                }
                Some(_) => continue,
            };
            found.push((unit.to_string(), row.path.clone(), line));
        }
    }
    for (unit, rows) in old.units() {
        let after = new.rows(unit);
        for row in rows {
            if !after.iter().any(|a| a.path == row.path) {
                found.push((
                    unit.to_string(),
                    row.path.clone(),
                    format!(
                        "{unit}: `{}`: dropped (it was {}, and is no longer a part of the board)",
                        row.path,
                        describe(row, true)
                    ),
                ));
            }
        }
    }
    found.extend(provider_lines(old, new, |unit, path, was, now| match (was, now) {
        (None, Some(now)) => format!(
            "{unit}: `{path}`: added as provider `{}` of `{}`",
            now.provider, now.generic
        ),
        (Some(was), Some(now)) => format!(
            "{unit}: `{path}`: provider `{}` -> `{}`",
            was.provider, now.provider
        ),
        (Some(was), None) => format!(
            "{unit}: `{path}`: dropped (it was provider `{}` of `{}`, and is no longer a use with a choice of provider)",
            was.provider, was.generic
        ),
        (None, None) => unreachable!("a line is about a provider in one lock or the other"),
    }));
    found.sort();
    found.into_iter().map(|(_, _, line)| line).collect()
}

/// A line for each use whose provider differs between `old` and `new` (added, changed or
/// dropped), with its board and path, in no order.
fn provider_lines(
    old: &PickLock,
    new: &PickLock,
    line: impl Fn(&str, &str, Option<&LockedProvider>, Option<&LockedProvider>) -> String,
) -> Vec<(String, String, String)> {
    let mut found = Vec::new();
    for (unit, providers) in new.provider_units() {
        for p in providers {
            let was = old.providers(unit).iter().find(|b| b.path == p.path);
            if was.is_some_and(|b| b.provider == p.provider) {
                continue;
            }
            found.push((
                unit.to_string(),
                p.path.clone(),
                line(unit, &p.path, was, Some(p)),
            ));
        }
    }
    for (unit, providers) in old.provider_units() {
        for p in providers {
            if !new.providers(unit).iter().any(|a| a.path == p.path) {
                found.push((
                    unit.to_string(),
                    p.path.clone(),
                    line(unit, &p.path, Some(p), None),
                ));
            }
        }
    }
    found
}

/// What `project lock --update` and `project lock --check` found for the lock of one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refresh {
    /// The lock file: beside the source.
    pub file: PathBuf,
    /// Whether the file exists now.
    pub existed: bool,
    /// The rows the solver picks now, without the lock's holding: what `--update` writes.
    pub now: PickLock,
    /// The difference between the file and `now`, one line for each part ([`diff`]); all the
    /// parts of the board as added when there is no file. A file of an older version of the
    /// format has a first line that says so (`version 1 -> 2`). Empty when the lock is current.
    pub diff: Vec<String>,
    /// The path of every part of the source's boards, and of every entry of the file, in order
    /// of the board and then the path: what a part named to `project lock --update` is looked up in.
    pub paths: Vec<String>,
    /// What the solver said when it held the parts to the rows of the file as it is: the rows
    /// that no longer pass or are no longer in a catalog, in its own words (the messages of a
    /// `build`, with the relation and its numbers). Empty when every locked row still holds.
    pub reported: String,
    /// The rows a person holds on purpose (`held = "reason"`, issue #1197) and that stay as they
    /// are: they are not called not current, however the solver would pick now. In board order,
    /// then path.
    pub held: Vec<Kept>,
    /// With `Parts::Except` on a check: the parts the run did not judge, held to the row the
    /// lock has for them (a held row is in `held`, not here).
    pub not_judged: Vec<Kept>,
}

/// A part of a board that a run left on the row the lock has for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    /// The board or system.
    pub unit: String,
    /// The part's path.
    pub path: String,
    /// The row as a line of a diff names it ([`describe`]).
    pub row: String,
    /// Why a person holds it, for a held row.
    pub reason: Option<String>,
}

impl Kept {
    /// `Board: `r1`: `A` in the `resistor` catalog (mpn M1)`, then `: reason` for a held row.
    pub fn line(&self) -> String {
        let mut line = format!("{}: `{}`: {}", self.unit, self.path, self.row);
        if let Some(reason) = &self.reason {
            line.push_str(&format!(": {reason}"));
        }
        line
    }
}

impl Refresh {
    /// Whether the lock is as `--update` would write it. A source with no part to lock needs
    /// no file.
    pub fn is_current(&self) -> bool {
        self.diff.is_empty()
    }

    /// Writes the file as it would be after the refresh. Nothing is written when the lock is
    /// current, so a file that holds the right rows keeps its bytes.
    pub fn write(&self) -> Result<(), String> {
        if self.is_current() {
            return Ok(());
        }
        std::fs::write(&self.file, render(&self.now))
            .map_err(|e| format!("error: {}: {e}", self.file.display()))
    }
}

/// Which parts of a source `project lock --update` takes new picks for (the rest keep the row the lock
/// holds them to), named by their path in the board (`r1`, `stage.output_cap`), in every board
/// of the file that has one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Parts {
    /// Every part: the solver's picks of today.
    #[default]
    All,
    /// Only the named parts take a new pick.
    Only(Vec<String>),
    /// Every part but the named ones takes a new pick.
    Except(Vec<String>),
}

impl Parts {
    /// The paths the person named, none for [`Parts::All`].
    pub fn named(&self) -> &[String] {
        match self {
            Parts::All => &[],
            Parts::Only(p) | Parts::Except(p) => p,
        }
    }

    /// Whether the person named the part at `path` for a new pick.
    fn names(&self, path: &str) -> bool {
        matches!(self, Parts::Only(p) if p.iter().any(|n| n == path))
    }

    /// Whether the part at `path` takes a new pick.
    fn refreshes(&self, path: &str) -> bool {
        match self {
            Parts::All => true,
            Parts::Only(p) => p.iter().any(|n| n == path),
            Parts::Except(p) => !p.iter().any(|n| n == path),
        }
    }
}

/// Solves each board and system of the source called `name` (with text `text`, at `source`)
/// twice: held to the rows of its lock file as it is, to say what no longer holds, and then
/// with no lock, to find the rows the solver picks now. Writes nothing, and reads no KiCad
/// library. `catalogs` must hold no pick lock. An error is the message to print: the file
/// cannot be read or is not a lock, or the source does not build even with no lock.
pub fn refresh(
    source: &Path,
    name: &str,
    text: &str,
    loader: &Loader,
    catalogs: &Catalogs,
) -> Result<Refresh, String> {
    refresh_parts(source, name, text, loader, catalogs, &Parts::All)
}

/// [`refresh`] with only some parts taking a new pick. The parts that do not are held to the row
/// the lock has for them, as `build` holds them, and judged by everything the design says: a
/// held row that no longer passes is an error with the solver's words, and `parts` is the way
/// to let it take a new pick. A part the lock has no row for takes the solver's pick, whether it
/// is named or not. With no lock file every part does. A row of the lock whose part is now in a
/// built-in catalog under another name (a release added a part the project's catalog had, issue
/// #1119) is rewritten to the built-in row and kept on it, whichever parts are named, unless
/// `Parts::Only` names it; if that row no longer passes, the part takes a new pick.
pub fn refresh_parts(
    source: &Path,
    name: &str,
    text: &str,
    loader: &Loader,
    catalogs: &Catalogs,
    parts: &Parts,
) -> Result<Refresh, String> {
    crate::picks::refuse(source)?;
    let file = lock_path(source);
    let old = read(&file)?;
    let mut reported = String::new();
    // The rows whose part a release added to a built-in catalog under another name (issue
    // #1119): they are rewritten to the built-in row, and stay on it with no other change.
    let (migrated, moved) = match &old {
        Some(old) => migrate(old, catalogs),
        None => (None, Vec::new()),
    };
    // A row the lock says is held on purpose stays on its row unless the part is named for a new
    // pick (issue #1197), as a part `Parts::Except` names does.
    let held_on_purpose = |unit: &str, path: &str| {
        !parts.names(path)
            && old.as_ref().is_some_and(|o| {
                o.rows(unit)
                    .iter()
                    .any(|r| r.path == path && r.held.is_some())
            })
    };
    let has_hold = old.as_ref().is_some_and(|o| {
        o.units()
            .any(|(u, rows)| rows.iter().any(|r| held_on_purpose(u, &r.path)))
    });
    let held_by = |unit: &str, path: &str| {
        !parts.refreshes(path)
            || (moved.iter().any(|m| m == path) && !parts.names(path))
            || held_on_purpose(unit, path)
    };
    let narrowed = !matches!(parts, Parts::All) || migrated.is_some() || has_hold;
    let outcome = match &old {
        Some(old) if narrowed => {
            let from = migrated.as_ref().unwrap_or(old);
            let mut held = PickLock::new(&old.file);
            for (unit, rows) in from.units() {
                let kept = rows.iter().filter(|r| held_by(unit, &r.path)).cloned();
                held.set(unit, kept.collect());
            }
            for (unit, providers) in old.provider_units() {
                held.set_providers(unit, providers.to_vec());
            }
            let held = catalogs.clone().with_pick_lock(held);
            match outcome_with(name, text, loader, &held, None) {
                Ok(outcome) => {
                    if !moved.is_empty() {
                        reported = report_old(name, text, loader, catalogs, old);
                    }
                    outcome
                }
                // A row moved to the built-in one that no longer passes takes a new pick.
                Err(_) if *parts == Parts::All && !has_hold => {
                    reported = report_old(name, text, loader, catalogs, old);
                    outcome_with(name, text, loader, catalogs, None)
                        .map_err(|problems| render_problems(&problems))?
                }
                Err(problems) => {
                    let how = if has_hold {
                        "the parts held on purpose (`held` in the lock) and the parts not named for a new pick are held to the rows of {}, which no longer pass: name the part with `--update --part` to take a new pick, or remove its `held` line"
                    } else {
                        "the parts not named for a new pick are held to the rows of {}, which no longer pass: name them to take a new pick"
                    };
                    return Err(format!(
                        "{}{}\n",
                        render_problems(&problems),
                        how.replace("{}", &file.display().to_string())
                    ));
                }
            }
        }
        _ => {
            let outcome = outcome_with(name, text, loader, catalogs, None)
                .map_err(|problems| render_problems(&problems))?;
            if let Some(old) = &old {
                reported = report_old(name, text, loader, catalogs, old);
            }
            outcome
        }
    };
    let mut now = outcome.lock;
    // The rows held on purpose keep their reason, when the pick is still the row the lock has.
    let mut held = Vec::new();
    let mut not_judged = Vec::new();
    if let Some(old) = &old {
        let base = migrated.as_ref().unwrap_or(old);
        let units: Vec<String> = now.units().map(|(u, _)| u.to_string()).collect();
        for unit in units {
            let mut rows = now.rows(&unit).to_vec();
            for row in &mut rows {
                let was = base.rows(&unit).iter().find(|b| {
                    b.path == row.path && (&b.catalog, &b.row) == (&row.catalog, &row.row)
                });
                let Some(was) = was else { continue };
                let kept = Kept {
                    unit: unit.clone(),
                    path: row.path.clone(),
                    row: describe(row, true),
                    reason: was.held.clone(),
                };
                if was.held.is_some() && held_on_purpose(&unit, &row.path) {
                    row.held = was.held.clone();
                    held.push(kept);
                } else if matches!(parts, Parts::Except(n) if n.contains(&row.path)) {
                    not_judged.push(Kept {
                        reason: None,
                        ..kept
                    });
                }
            }
            now.set(&unit, rows);
        }
    }
    let mut paths: Vec<String> = now
        .units()
        .chain(old.iter().flat_map(PickLock::units))
        .flat_map(|(_, rows)| rows.iter().map(|r| r.path.clone()))
        .collect();
    paths.sort();
    paths.dedup();
    let empty = PickLock::new("");
    let mut diff = diff(old.as_ref().unwrap_or(&empty), &now);
    if let Some(line) = old.as_ref().and_then(|o| o.outdated.clone()) {
        diff.insert(0, line);
    }
    now.file = file.display().to_string();
    Ok(Refresh {
        file,
        existed: old.is_some(),
        now,
        diff,
        paths,
        reported,
        held,
        not_judged,
    })
}

/// What the solver says of a build held to the rows of `old`: the rows that no longer pass or
/// are no longer in a catalog, in its own words. Empty when every locked row still holds.
fn report_old(
    name: &str,
    text: &str,
    loader: &Loader,
    catalogs: &Catalogs,
    old: &PickLock,
) -> String {
    let held = catalogs.clone().with_pick_lock(old.clone());
    match outcome_with(name, text, loader, &held, None) {
        Ok(_) => String::new(),
        Err(problems) => render_problems(&problems),
    }
}

/// The lock with each row whose part is now in a built-in catalog under another name rewritten
/// to that row (issue #1119): `None` when no row is, and the paths of the parts that moved. A
/// release adds a part that a project's own catalog already had; the project's row goes (the
/// catalogs cannot hold both), and the lock's `row` is the name the project gave it. The
/// lock's other rows are untouched.
fn migrate(old: &PickLock, catalogs: &Catalogs) -> (Option<PickLock>, Vec<String>) {
    let mut moved = Vec::new();
    let mut lock = PickLock::new(&old.file);
    for (unit, rows) in old.units() {
        let rows: Vec<LockedRow> = rows
            .iter()
            .map(|row| match catalogs.renamed_row(row) {
                Some(now) => {
                    moved.push(row.path.clone());
                    now
                }
                None => row.clone(),
            })
            .collect();
        lock.set(unit, rows);
    }
    for (unit, providers) in old.provider_units() {
        lock.set_providers(unit, providers.to_vec());
    }
    ((!moved.is_empty()).then_some(lock), moved)
}

/// How the picks of a run differ from the lock they were made under.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    /// Parts picked that the lock has no entry for: `(board, path, row)`.
    pub added: Vec<(String, String, String)>,
    /// Entries of the lock for parts the board no longer has: `(board, path, row)`.
    pub gone: Vec<(String, String, String)>,
    /// Parts whose pick is not the locked row: `(board, path, locked row, row picked)`.
    pub changed: Vec<(String, String, String, String)>,
    /// A line for each use of a generic whose provider is not the locked one, or that the lock
    /// has no entry for, or that is gone (ADR 0113), in the order board, use.
    pub providers: Vec<String>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.gone.is_empty()
            && self.changed.is_empty()
            && self.providers.is_empty()
    }

    /// One line for each difference, in the order board, part.
    pub fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for (unit, path, row) in &self.added {
            lines.push(format!(
                "{unit}: `{path}` is not in the lock (it was picked as `{row}`)"
            ));
        }
        for (unit, path, row) in &self.gone {
            lines.push(format!(
                "{unit}: `{path}` is in the lock as `{row}` and is no longer a part of the board"
            ));
        }
        for (unit, path, was, now) in &self.changed {
            lines.push(format!(
                "{unit}: `{path}` is locked to `{was}` and was picked as `{now}`"
            ));
        }
        lines.extend(self.providers.iter().cloned());
        lines
    }
}

/// How `new`, the picks of a run, differ from `old`, the lock it was made under.
pub fn compare(old: &PickLock, new: &PickLock) -> Changes {
    let mut changes = Changes::default();
    for (unit, rows) in new.units() {
        let before = old.rows(unit);
        for row in rows {
            match before.iter().find(|b| b.path == row.path) {
                None => changes
                    .added
                    .push((unit.to_string(), row.path.clone(), row.row.clone())),
                // A lock written by hand may leave out the numbers; the row is the same.
                Some(b) if (&b.catalog, &b.row) != (&row.catalog, &row.row) => {
                    changes.changed.push((
                        unit.to_string(),
                        row.path.clone(),
                        b.row.clone(),
                        row.row.clone(),
                    ))
                }
                Some(_) => {}
            }
        }
    }
    for (unit, rows) in old.units() {
        let after = new.rows(unit);
        for row in rows {
            if !after.iter().any(|a| a.path == row.path) {
                changes
                    .gone
                    .push((unit.to_string(), row.path.clone(), row.row.clone()));
            }
        }
    }
    let mut providers = provider_lines(old, new, |unit, path, was, now| match (was, now) {
        (None, Some(now)) => format!(
            "{unit}: `{path}` is not in the lock (its provider was picked as `{}`)",
            now.provider
        ),
        (Some(was), Some(now)) => format!(
            "{unit}: `{path}` is locked to the provider `{}` and was picked as `{}`",
            was.provider, now.provider
        ),
        (Some(was), None) => format!(
            "{unit}: `{path}` is in the lock as the provider `{}` and is no longer a use with a choice of provider",
            was.provider
        ),
        (None, None) => unreachable!("a line is about a provider in one lock or the other"),
    });
    providers.sort();
    changes.providers = providers.into_iter().map(|(_, _, l)| l).collect();
    changes
}

/// What a run says about the lock, besides the error that stops it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settled {
    /// The file written, if the lock did not exist.
    pub wrote: Option<PathBuf>,
    /// Lines for a person, each to be printed as a note.
    pub notes: Vec<String>,
}

/// Decides what the picks of a run (`new`) do to the lock they were made under (`old`, `None`
/// if the file does not exist), and does it: writes the file when there is none and `--locked`
/// is not given, says how the picks differ when there is one, and with `--locked` refuses, in
/// both cases, before anything else is written. The file that exists is never rewritten.
pub fn settle(using: &Use, old: Option<&PickLock>, new: &PickLock) -> Result<Settled, String> {
    let name = using.file.display().to_string();
    let mut settled = Settled::default();
    if using.off {
        return Ok(settled);
    }
    match old {
        None if new.is_empty() => {}
        None if using.locked => {
            return Err(format!(
                "error: {name} does not exist, and --locked says it must not be written: run without --locked to write it, and commit it"
            ));
        }
        None => {
            std::fs::write(&using.file, render(new)).map_err(|e| format!("error: {name}: {e}"))?;
            settled.wrote = Some(using.file.clone());
        }
        Some(old) => {
            // An older format still holds the parts as it says: a run tells, and changes nothing.
            if old.outdated.is_some() {
                settled.notes.push(format!(
                    "{name} is of an older format of the lock, which records no manufacturer or no sku of each channel (ADR 0143); it holds the parts as before, and `netsmith project lock --update` writes it as version {VERSION}"
                ));
            }
            let changes = compare(old, new);
            if !changes.is_empty() {
                let lines = changes.lines();
                if using.locked {
                    return Err(format!(
                        "error: the parts picked differ from {name}, and --locked says it must not change:\n{}\n",
                        lines
                            .iter()
                            .map(|l| format!("  {l}\n"))
                            .collect::<String>()
                            .trim_end()
                    ));
                }
                settled.notes.push(format!(
                    "the parts picked differ from {name}, which is not changed (delete it to write it again, or run with --locked to make this an error):\n{}",
                    lines.iter().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n")
                ));
            }
        }
    }
    Ok(settled)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(path: &str, catalog: &str, name: &str, lcsc: Option<&str>) -> LockedRow {
        LockedRow {
            path: path.into(),
            catalog: catalog.into(),
            row: name.into(),
            skus: lcsc
                .map(|l| vec![("jlcpcb-assembly".to_string(), l.to_string())])
                .unwrap_or_default(),
            manufacturer: None,
            mpn: None,
            held: None,
        }
    }

    fn lock(rows: &[(&str, LockedRow)]) -> PickLock {
        let mut lock = PickLock::new("a.lock.toml");
        for unit in ["Board", "Other"] {
            let mine: Vec<LockedRow> = rows
                .iter()
                .filter(|(u, _)| *u == unit)
                .map(|(_, r)| r.clone())
                .collect();
            if !mine.is_empty() {
                lock.set(unit, mine);
            }
        }
        lock
    }

    #[test]
    fn a_lock_is_read_back_as_it_was_written() {
        let made = lock(&[
            (
                "Board",
                row("r1", "resistor", "330Ω 1% 0402", Some("C25104")),
            ),
            ("Board", row("stage.cap", "capacitor", "generic 22uF", None)),
            ("Other", row("d1", "led", "red \"0603\"", Some("C2286"))),
        ]);
        let text = render(&made);
        assert!(
            text.contains("\nsku = { jlcpcb-assembly = \"C25104\" }\n"),
            "{text}"
        );
        assert_eq!(parse(&text, "a.lock.toml").unwrap(), made);
        // Both channels, in their order.
        let mut both = row("r1", "resistor", "330Ω", None);
        both.skus = vec![
            ("jlcpcb-assembly".into(), "C25104".into()),
            ("lcsc-retail".into(), "C25104".into()),
        ];
        let both = lock(&[("Board", both)]);
        let both_text = render(&both);
        assert!(
            both_text
                .contains("\nsku = { jlcpcb-assembly = \"C25104\", lcsc-retail = \"C25104\" }\n"),
            "{both_text}"
        );
        assert_eq!(parse(&both_text, "a.lock.toml").unwrap(), both);
        // Two writes of the same picks are the same bytes.
        assert_eq!(render(&made), text);
    }

    fn provider(path: &str, id: &str) -> LockedProvider {
        LockedProvider {
            path: path.into(),
            generic: "TwoLayer".into(),
            provider: id.into(),
        }
    }

    #[test]
    fn a_lock_records_the_manufacturer_beside_the_numbers_and_reads_it_back() {
        let mut r1 = row("r1", "resistor", "ZZR-330", None);
        r1.manufacturer = Some("Acme Passives".into());
        r1.mpn = Some("ZZR-330".into());
        let made = lock(&[("Board", r1.clone())]);
        let text = render(&made);
        assert!(
            text.contains("\nversion = 2\n")
                && text.contains("[Board.parts.\"r1\"]\ncatalog = \"resistor\"\nrow = \"ZZR-330\"\nmanufacturer = \"Acme Passives\"\nmpn = \"ZZR-330\"\n"),
            "{text}"
        );
        assert_eq!(parse(&text, "a.lock.toml").unwrap(), made);
        // A manufacturer that changes is a line of the diff, with the numbers.
        let mut other = r1.clone();
        other.manufacturer = Some("Zenith Passives".into());
        assert_eq!(
            diff(&made, &lock(&[("Board", other)])),
            vec![
                "Board: `r1`: `ZZR-330` (manufacturer Acme Passives, mpn ZZR-330) -> `ZZR-330` (manufacturer Zenith Passives, mpn ZZR-330)"
            ]
        );
    }

    #[test]
    fn a_lock_of_version_1_is_read_and_called_outdated() {
        let v1 = "version = 1\n[Board.parts.\"r1\"]\ncatalog = \"resistor\"\nrow = \"330Ω\"\nlcsc = \"C25104\"\nmpn = \"0402WGF3300TCE\"\n";
        let old = parse(v1, "a.lock.toml").unwrap();
        assert_eq!(old.outdated, Some(outdated_line(1)));
        assert_eq!(old.rows("Board")[0].mpn.as_deref(), Some("0402WGF3300TCE"));
        assert_eq!(old.rows("Board")[0].skus, vec![]);
        assert_eq!(old.rows("Board")[0].manufacturer, None);
        // What a run would write is version 2, and reads back as current.
        let again = parse(&render(&old), "a.lock.toml").unwrap();
        assert_eq!(again.outdated, None);
        assert_eq!(again.rows("Board"), old.rows("Board"));
        // A run tells that the lock is old, and with the same picks finds no change.
        let mut picks = old.clone();
        picks.outdated = None;
        assert!(compare(&old, &picks).is_empty());
        let using = Use {
            file: "a.lock.toml".into(),
            locked: true,
            off: false,
        };
        let settled = settle(&using, Some(&old), &picks).unwrap();
        assert_eq!(settled.wrote, None);
        assert_eq!(
            settled.notes,
            vec![
                "a.lock.toml is of an older format of the lock, which records no manufacturer or no sku of each channel (ADR 0143); it holds the parts as before, and `netsmith project lock --update` writes it as version 2"
            ]
        );
        assert_eq!(
            outdated_line(1),
            "version 1 -> 2: the file is of version 1 of the lock, and version 2 records each part's manufacturer and mpn, and the sku of its offer for each supply channel in place of its lcsc (ADR 0143)"
        );
    }

    #[test]
    fn a_lock_of_version_2_that_records_lcsc_is_called_outdated() {
        let old = "version = 2\n[Board.parts.\"r1\"]\ncatalog = \"resistor\"\nrow = \"330Ω\"\nlcsc = \"C25104\"\nmpn = \"0402WGF3300TCE\"\n";
        let old = parse(old, "a.lock.toml").unwrap();
        assert_eq!(
            old.outdated.as_deref(),
            Some(
                "lcsc -> sku: the file records each part's `lcsc`, and the lock now records the sku of the part's offer for each supply channel in its place (ADR 0143)"
            )
        );
        let again = parse(&render(&old), "a.lock.toml").unwrap();
        assert_eq!(again.outdated, None);
    }

    #[test]
    fn a_sku_of_a_channel_that_is_not_one_is_refused() {
        let text = "version = 2\n[Board.parts.\"r1\"]\ncatalog = \"resistor\"\nrow = \"330Ω\"\nsku = { farnell-retail = \"311-330\" }\n";
        let e = parse(text, "a.lock.toml").unwrap_err();
        assert_eq!(
            e,
            "error: a.lock.toml: [Board.parts.\"r1\"] `sku` has `farnell-retail`, which is not a supply channel netsmith knows: `jlcpcb-assembly`, `lcsc-retail`, `digikey-retail` or `mouser-retail`"
        );
        // Every channel netsmith knows is one, in the order of the lock.
        let text = "version = 2\n[Board.parts.\"r1\"]\ncatalog = \"resistor\"\nrow = \"330Ω\"\nsku = { mouser-retail = \"71-330\", digikey-retail = \"311-330\" }\n";
        let lock = parse(text, "a.lock.toml").unwrap();
        assert_eq!(
            lock.rows("Board")[0].skus,
            [
                ("digikey-retail".to_string(), "311-330".to_string()),
                ("mouser-retail".to_string(), "71-330".to_string())
            ]
        );
    }

    #[test]
    fn a_lock_with_providers_is_read_back_as_it_was_written() {
        let mut made = lock(&[("Board", row("r1", "resistor", "330Ω", Some("C25104")))]);
        made.set_providers(
            "Board",
            vec![provider("process", "std::mfg::jlcpcb::JlcTwoLayer2oz")],
        );
        // A board with a provider and no part.
        made.set_providers(
            "Bare",
            vec![provider("process", "std::mfg::jlcpcb::JlcTwoLayer1oz")],
        );
        let text = render(&made);
        assert!(
            text.contains("\n[Board.providers.\"process\"]\ngeneric = \"TwoLayer\"\nprovider = \"std::mfg::jlcpcb::JlcTwoLayer2oz\"\n"),
            "{text}"
        );
        assert!(
            text.find("[Board.providers").unwrap() < text.find("[Board.parts").unwrap(),
            "{text}"
        );
        assert_eq!(parse(&text, "a.lock.toml").unwrap(), made);
        assert!(!made.is_empty());
        let held = held_providers(Some(&made));
        assert_eq!(held["Board"]["process"], "std::mfg::jlcpcb::JlcTwoLayer2oz");
    }

    #[test]
    fn a_changed_provider_is_a_change_and_a_line_of_the_diff() {
        let mut old = lock(&[]);
        old.set_providers("Board", vec![provider("process", "a::One")]);
        let mut new = lock(&[]);
        new.set_providers("Board", vec![provider("process", "a::Two")]);
        assert_eq!(
            compare(&old, &new).lines(),
            vec!["Board: `process` is locked to the provider `a::One` and was picked as `a::Two`"]
        );
        assert_eq!(
            diff(&old, &new),
            vec!["Board: `process`: provider `a::One` -> `a::Two`"]
        );
        assert_eq!(
            diff(&lock(&[]), &new),
            vec!["Board: `process`: added as provider `a::Two` of `TwoLayer`"]
        );
        assert_eq!(
            compare(&new, &lock(&[])).lines(),
            vec![
                "Board: `process` is in the lock as the provider `a::Two` and is no longer a use with a choice of provider"
            ]
        );
        assert!(compare(&new, &new).is_empty());
        let wrong = parse(
            "version = 1\n[B.providers.\"process\"]\ngeneric = \"TwoLayer\"\n",
            "x.lock.toml",
        )
        .unwrap_err();
        assert!(wrong.contains("has no `provider`"), "{wrong}");
    }

    #[test]
    fn the_file_lists_boards_in_name_order_and_parts_in_path_order() {
        let made = lock(&[
            ("Other", row("a", "led", "x", None)),
            ("Board", row("r2", "resistor", "y", None)),
            ("Board", row("r1", "resistor", "z", None)),
        ]);
        let text = render(&made);
        let at = |needle: &str| {
            text.find(needle)
                .unwrap_or_else(|| panic!("{needle}: {text}"))
        };
        assert!(at("[Board.parts.\"r1\"]") < at("[Board.parts.\"r2\"]"));
        assert!(at("[Board.parts.\"r2\"]") < at("[Other.parts.\"a\"]"));
        assert!(text.starts_with("# "), "{text}");
        assert!(text.contains("\nversion = 2\n"), "{text}");
    }

    #[test]
    fn a_wrong_file_is_refused_with_the_place_in_it() {
        let wrong = |text: &str| parse(text, "x.lock.toml").unwrap_err();
        assert!(wrong("").contains("no `version`"));
        assert!(wrong("version = 3").contains("version 3 is not a version"));
        assert!(wrong("version = 1\nB = 3").contains("`B` must be a table"));
        assert!(
            wrong("version = 1\n[B]\nx = 1")
                .contains("`[B]` has `x`; it holds only `parts` and `providers`")
        );
        let no_row = wrong("version = 1\n[B.parts.\"r1\"]\ncatalog = \"resistor\"");
        assert!(no_row.contains("[B.parts.\"r1\"] has no `row`"), "{no_row}");
        let extra = wrong("version = 1\n[B.parts.\"r1\"]\ncatalog = \"a\"\nrow = \"b\"\nprice = 1");
        assert!(extra.contains("has `price`"), "{extra}");
        assert!(wrong("version = 1\n[B.parts.\"r1\"\n").starts_with("error: x.lock.toml:"));
    }

    #[test]
    fn the_changes_name_each_part_that_is_added_gone_or_changed() {
        let old = lock(&[
            ("Board", row("r1", "resistor", "A", None)),
            ("Board", row("r2", "resistor", "B", None)),
            ("Other", row("d1", "led", "L", None)),
        ]);
        let new = lock(&[
            ("Board", row("r1", "resistor", "A2", None)),
            ("Board", row("r3", "resistor", "C", None)),
        ]);
        let changes = compare(&old, &new);
        assert_eq!(
            changes.lines(),
            vec![
                "Board: `r3` is not in the lock (it was picked as `C`)",
                "Board: `r2` is in the lock as `B` and is no longer a part of the board",
                "Other: `d1` is in the lock as `L` and is no longer a part of the board",
                "Board: `r1` is locked to `A` and was picked as `A2`",
            ]
        );
        assert!(compare(&old, &old).is_empty());
    }

    #[test]
    fn the_diff_names_each_part_with_its_numbers_in_board_and_path_order() {
        let mut old_a = row("r1", "resistor", "A", Some("C1"));
        old_a.mpn = Some("M1".into());
        let mut new_a = row("r1", "resistor", "A2", Some("C2"));
        new_a.mpn = Some("M2".into());
        let old = lock(&[
            ("Board", old_a),
            ("Board", row("r2", "resistor", "B", None)),
            ("Other", row("d1", "led", "L", Some("C9"))),
            ("Board", row("same", "led", "S", Some("C5"))),
        ]);
        let new = lock(&[
            ("Board", new_a),
            ("Board", row("r3", "resistor", "C", None)),
            ("Board", row("same", "led", "S", Some("C5"))),
        ]);
        assert_eq!(
            diff(&old, &new),
            vec![
                "Board: `r1`: `A` (mpn M1, jlcpcb-assembly C1) -> `A2` (mpn M2, jlcpcb-assembly C2)",
                "Board: `r2`: dropped (it was `B` in the `resistor` catalog, and is no longer a part of the board)",
                "Board: `r3`: added as `C` in the `resistor` catalog",
                "Other: `d1`: dropped (it was `L` in the `led` catalog (jlcpcb-assembly C9), and is no longer a part of the board)",
            ]
        );
        assert!(diff(&old, &old).is_empty());
        // The same row with other numbers is a change, and a row in another catalog says so.
        let renumbered = lock(&[("Board", row("r2", "capacitor", "B", Some("C7")))]);
        let was = lock(&[("Board", row("r2", "resistor", "B", Some("C6")))]);
        assert_eq!(
            diff(&was, &renumbered),
            vec![
                "Board: `r2`: `B` in the `resistor` catalog (jlcpcb-assembly C6) -> `B` in the `capacitor` catalog (jlcpcb-assembly C7)"
            ]
        );
    }

    #[test]
    fn a_held_row_is_read_back_written_and_told_when_it_is_released() {
        let mut held = row("r1", "resistor", "A", Some("C1"));
        held.held = Some("the supplier's stock of the other row is 0".into());
        let made = lock(&[
            ("Board", held.clone()),
            ("Board", row("r2", "led", "L", None)),
        ]);
        let text = render(&made);
        assert!(
            text.contains("[Board.parts.\"r1\"]\ncatalog = \"resistor\"\nrow = \"A\"\nsku = { jlcpcb-assembly = \"C1\" }\nheld = \"the supplier's stock of the other row is 0\"\n"),
            "{text}"
        );
        assert_eq!(text.matches("\nheld = ").count(), 1, "{text}");
        assert_eq!(parse(&text, "a.lock.toml").unwrap(), made);
        assert_eq!(render(&made), text);
        // A held row is equal to itself, and a hold that goes is a line of the diff.
        assert!(diff(&made, &made).is_empty());
        let released = lock(&[
            ("Board", row("r1", "resistor", "A", Some("C1"))),
            ("Board", row("r2", "led", "L", None)),
        ]);
        assert_eq!(
            diff(&made, &released),
            vec!["Board: `r1`: no longer held: the supplier's stock of the other row is 0"]
        );
        let moved = lock(&[
            ("Board", row("r1", "resistor", "B", Some("C2"))),
            ("Board", row("r2", "led", "L", None)),
        ]);
        assert_eq!(
            diff(&made, &moved),
            vec![
                "Board: `r1`: `A` (jlcpcb-assembly C1) -> `B` (jlcpcb-assembly C2) (no longer held: the supplier's stock of the other row is 0)"
            ]
        );
        // The lock a build compares picks with does not read the reason.
        assert!(compare(&made, &released).is_empty());
    }

    #[test]
    fn a_held_that_is_not_a_reason_is_refused_with_the_place_in_it() {
        let wrong = |held: &str| {
            parse(
                &format!(
                    "version = 1\n[B.parts.\"r1\"]\ncatalog = \"a\"\nrow = \"b\"\nheld = {held}"
                ),
                "x.lock.toml",
            )
            .unwrap_err()
        };
        assert!(wrong("\"  \"").contains("[B.parts.\"r1\"] `held` is empty"));
        assert!(wrong("true").contains("`held` must be text"));
    }
}
