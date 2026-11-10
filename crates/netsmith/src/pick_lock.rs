//! The pick lock beside a source file: `<stem>.lock.toml` (see `GLOSSARY.md`, ADR 0100).
//!
//! It says which catalog row each part of each board was picked as, so that a change of a
//! catalog, of the solver or of the order of the rows does not move a pick without a line in a
//! pull request. `build` and `fab` write it when it does not exist and read it when it does:
//! the solver then offers each locked part only its locked row, and judges that row by
//! everything the design says (`netsmith_solve::Catalogs::with_pick_lock`). A lock that no
//! longer matches the picks is told, and with `--locked` is an error; it is never rewritten
//! behind a person's back.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use netsmith_solve::{Catalogs, LockedRow, PickLock, Solution};

/// The version of the file's format.
const VERSION: i64 = 1;

/// The first lines of a written file.
const HEADER: &str = "# The catalog row each part of each board was picked as, written by `netsmith build` and\n# `netsmith fab` when this file does not exist. Commit it: a change to a catalog or to the\n# solver that would move a pick shows here and not only in the bill of materials. Delete the\n# file to pick again. Nothing in it is edited by hand.\n";

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
/// catalogs hold the parts to the lock if the file exists.
pub fn open(
    source: &Path,
    catalogs: &Catalogs,
    locked: bool,
) -> Result<(Option<Catalogs>, Use), String> {
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
        if let Some(extra) = unit_table.keys().find(|k| *k != "parts") {
            return Err(wrong(format!(
                "`[{unit}]` has `{extra}`; it holds only `parts`"
            )));
        }
        let parts = unit_table
            .get("parts")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| wrong(format!("`[{unit}]` has no table `parts`")))?;
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
    for (unit, rows) in lock.units() {
        for row in rows {
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

/// How the picks of a run differ from the lock they were made under.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    /// Parts picked that the lock has no entry for: `(board, path, row)`.
    pub added: Vec<(String, String, String)>,
    /// Entries of the lock for parts the board no longer has: `(board, path, row)`.
    pub gone: Vec<(String, String, String)>,
    /// Parts whose pick is not the locked row: `(board, path, locked row, row picked)`.
    pub changed: Vec<(String, String, String, String)>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.gone.is_empty() && self.changed.is_empty()
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
        assert!(wrong("version = 1\n[B]\nx = 1").contains("`[B]` has `x`"));
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
}
