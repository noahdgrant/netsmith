//! The pick lock beside a source file: `<stem>.lock.toml` (see `GLOSSARY.md`, ADR 0100).
//!
//! It says which catalog row each part of each board was picked as, so that a change of a
//! catalog, of the solver or of the order of the rows does not move a pick without a line in a
//! pull request. `build` and `fab` write it when it does not exist and read it when it does:
//! the solver then offers each locked part only its locked row, and judges that row by
//! everything the design says (`netsmith_solve::Catalogs::with_pick_lock`). A lock that no
//! longer matches the picks is told, and with `--locked` is an error; it is never rewritten
//! behind a person's back. `netsmith lock --update` rewrites it on purpose and `lock --check`
//! says whether it is current ([`refresh`]).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use netsmith_check::{Assignment, Loader, Model};
use netsmith_solve::{Catalogs, LockedProvider, LockedRow, PickLock, Solution};

use crate::{outcome_with, render_problems};

/// The version of the file's format.
const VERSION: i64 = 1;

/// The first lines of a written file.
const HEADER: &str = "# The catalog row each part of each board was picked as, written by `netsmith build` and\n# `netsmith fab` when this file does not exist. Commit it: a change to a catalog or to the\n# solver that would move a pick shows here and not only in the bill of materials. Refresh it\n# with `netsmith lock --update`. Nothing in it is edited by hand.\n";

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
/// picks file beside the source is an error that says how to move it into the source (ADR 0121).
pub fn open(
    source: &Path,
    catalogs: &Catalogs,
    locked: bool,
) -> Result<(Option<Catalogs>, Use), String> {
    crate::picks::refuse(source)?;
    let file = lock_path(source);
    let held = read(&file)?.map(|lock| catalogs.clone().with_pick_lock(lock));
    Ok((held, Use { file, locked }))
}

fn parse(text: &str, name: &str) -> Result<PickLock, String> {
    let wrong = |what: String| format!("error: {name}: {what}");
    let table: toml::Table = text.parse().map_err(|e| wrong(format!("{e}")))?;
    match table.get("version") {
        Some(toml::Value::Integer(VERSION)) => {}
        Some(other) => {
            return Err(wrong(format!(
                "version {other} is not a version of the lock this netsmith reads (version {VERSION}); delete the file to write it again"
            )));
        }
        None => return Err(wrong("it has no `version`".to_string())),
    }
    let mut lock = PickLock::new(name);
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
            if let Some(extra) = entry
                .keys()
                .find(|k| !["catalog", "row", "lcsc", "mpn"].contains(&k.as_str()))
            {
                return Err(wrong(format!(
                    "{at} has `{extra}`; it holds `catalog`, `row`, `lcsc` and `mpn`"
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
            rows.push(LockedRow {
                path: path.clone(),
                catalog: text("catalog", true)?.unwrap_or_default(),
                row: text("row", true)?.unwrap_or_default(),
                lcsc: text("lcsc", false)?,
                mpn: text("mpn", false)?,
            });
        }
        lock.set(unit, rows);
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
            if let Some(lcsc) = &row.lcsc {
                let _ = writeln!(out, "lcsc = {}", string(lcsc));
            }
            if let Some(mpn) = &row.mpn {
                let _ = writeln!(out, "mpn = {}", string(mpn));
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
    let numbers: Vec<String> = [("lcsc", &row.lcsc), ("mpn", &row.mpn)]
        .into_iter()
        .filter_map(|(name, number)| number.as_ref().map(|n| format!("{name} {n}")))
        .collect();
    if !numbers.is_empty() {
        text.push_str(&format!(" ({})", numbers.join(", ")));
    }
    text
}

/// What differs between two locks, one line for each part, in the order board, part path:
/// `Board: `r1`: `A` (lcsc C1, mpn M1) -> `B` (lcsc C2, mpn M2)` for a part whose row, or whose
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
                    let with_catalog = b.catalog != row.catalog;
                    format!(
                        "{unit}: `{}`: {} -> {}",
                        row.path,
                        describe(b, with_catalog),
                        describe(row, with_catalog)
                    )
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

/// What `lock --update` and `lock --check` found for the lock of one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refresh {
    /// The lock file: beside the source.
    pub file: PathBuf,
    /// Whether the file exists now.
    pub existed: bool,
    /// The rows the solver picks now, without the lock's holding: what `--update` writes.
    pub now: PickLock,
    /// The difference between the file and `now`, one line for each part ([`diff`]); all the
    /// parts of the board as added when there is no file. Empty when the lock is current.
    pub diff: Vec<String>,
    /// What the solver said when it held the parts to the rows of the file as it is: the rows
    /// that no longer pass or are no longer in a catalog, in its own words (the messages of a
    /// `build`, with the relation and its numbers). Empty when every locked row still holds.
    pub reported: String,
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
    crate::picks::refuse(source)?;
    let file = lock_path(source);
    let old = read(&file)?;
    let outcome = outcome_with(name, text, loader, catalogs, None)
        .map_err(|problems| render_problems(&problems))?;
    let now = outcome.lock;
    let mut reported = String::new();
    if let Some(old) = &old {
        let held = catalogs.clone().with_pick_lock(old.clone());
        if let Err(problems) = outcome_with(name, text, loader, &held, None) {
            reported = render_problems(&problems);
        }
    }
    let empty = PickLock::new("");
    let diff = diff(old.as_ref().unwrap_or(&empty), &now);
    let mut now = now;
    now.file = file.display().to_string();
    Ok(Refresh {
        file,
        existed: old.is_some(),
        now,
        diff,
        reported,
    })
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
            lcsc: lcsc.map(str::to_string),
            mpn: None,
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
        assert_eq!(parse(&text, "a.lock.toml").unwrap(), made);
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
        assert!(text.contains("\nversion = 1\n"), "{text}");
    }

    #[test]
    fn a_wrong_file_is_refused_with_the_place_in_it() {
        let wrong = |text: &str| parse(text, "x.lock.toml").unwrap_err();
        assert!(wrong("").contains("no `version`"));
        assert!(wrong("version = 2").contains("version 2 is not a version"));
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
                "Board: `r1`: `A` (lcsc C1, mpn M1) -> `A2` (lcsc C2, mpn M2)",
                "Board: `r2`: dropped (it was `B` in the `resistor` catalog, and is no longer a part of the board)",
                "Board: `r3`: added as `C` in the `resistor` catalog",
                "Other: `d1`: dropped (it was `L` in the `led` catalog (lcsc C9), and is no longer a part of the board)",
            ]
        );
        assert!(diff(&old, &old).is_empty());
        // The same row with other numbers is a change, and a row in another catalog says so.
        let renumbered = lock(&[("Board", row("r2", "capacitor", "B", Some("C7")))]);
        let was = lock(&[("Board", row("r2", "resistor", "B", Some("C6")))]);
        assert_eq!(
            diff(&was, &renumbered),
            vec![
                "Board: `r2`: `B` in the `resistor` catalog (lcsc C6) -> `B` in the `capacitor` catalog (lcsc C7)"
            ]
        );
    }
}
