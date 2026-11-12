//! The picks file beside a source file: `<stem>.picks.toml` (#462, see `GLOSSARY.md`).
//!
//! It holds the catalog rows a person chose for parts, keyed as the pick lock's entries are, by
//! board and part path, and naming each row by its catalog and its LCSC number (or its
//! manufacturer part number in a catalog with no LCSC column). The solver holds each part it
//! names to that row and judges the row by everything the design says, before the lock
//! (`netsmith_solve::Catalogs::with_picks`): a row that fails stops the run with the reason, and
//! no other row is tried. The lock then records the chosen row as it records any pick. netsmith
//! never writes the file. An entry whose part is gone is stale, and follows the rule of a stale
//! lock entry (ADR 0100): a note on `build` and `fab`, an error with `--locked`, and a line of
//! `lock --check`, which then exits 1.

use std::path::{Path, PathBuf};

use netsmith_solve::{ChosenRow, PickLock, Picks};

/// The version of the file's format.
const VERSION: i64 = 1;

/// The picks file of a source file: `led.nsm` has `led.picks.toml`.
pub fn picks_path(source: &Path) -> PathBuf {
    source.with_extension("picks.toml")
}

/// The picks in the file at `file`, or `None` if there is no such file. A file that cannot be
/// read or is wrong is an error naming it and the place in it.
pub fn read(file: &Path) -> Result<Option<Picks>, String> {
    let name = file.display().to_string();
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("error: {name}: {e}")),
    };
    parse(&text, &name).map(Some)
}

/// `catalogs` holding the parts of the source at `source` to the rows of its picks file, when
/// there is one: what the editor solves with, which reads no lock (#462).
pub fn holding(
    source: &Path,
    catalogs: &netsmith_solve::Catalogs,
) -> Result<netsmith_solve::Catalogs, String> {
    Ok(match read(&picks_path(source))? {
        Some(picks) => catalogs.clone().with_picks(picks),
        None => catalogs.clone(),
    })
}

pub(crate) fn parse(text: &str, name: &str) -> Result<Picks, String> {
    let wrong = |what: String| format!("error: {name}: {what}");
    let table: toml::Table = text.parse().map_err(|e| wrong(format!("{e}")))?;
    match table.get("version") {
        Some(toml::Value::Integer(VERSION)) => {}
        Some(other) => {
            return Err(wrong(format!(
                "version {other} is not a version of the picks file this netsmith reads (version {VERSION})"
            )));
        }
        None => return Err(wrong("it has no `version`".to_string())),
    }
    let mut picks = Picks::new(name);
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
                .find(|k| !["catalog", "lcsc", "mpn"].contains(&k.as_str()))
            {
                return Err(wrong(format!(
                    "{at} has `{extra}`; it holds `catalog` and one of `lcsc` and `mpn`"
                )));
            }
            let text = |key: &str| -> Result<Option<String>, String> {
                match entry.get(key) {
                    Some(toml::Value::String(s)) => Ok(Some(s.clone())),
                    Some(other) => Err(wrong(format!("{at} `{key}` must be text, not {other}"))),
                    None => Ok(None),
                }
            };
            let catalog =
                text("catalog")?.ok_or_else(|| wrong(format!("{at} has no `catalog`")))?;
            let (lcsc, mpn) = (text("lcsc")?, text("mpn")?);
            if lcsc.is_some() == mpn.is_some() {
                return Err(wrong(format!(
                    "{at} names the row by `lcsc` or by `mpn`: it has {}",
                    if lcsc.is_some() { "both" } else { "neither" }
                )));
            }
            rows.push(ChosenRow {
                path: path.clone(),
                catalog,
                lcsc,
                mpn,
            });
        }
        picks.set(unit, rows);
    }
    Ok(picks)
}

/// The entries of `picks` whose part is not a part that the run picked a row for (`picked`, the
/// rows of the run as the lock writes them): one line for each, in the order board, part path.
pub fn stale(picks: &Picks, picked: &PickLock) -> Vec<String> {
    let mut lines = Vec::new();
    for (unit, rows) in picks.units() {
        let have = picked.rows(unit);
        for row in rows {
            if !have.iter().any(|h| h.path == row.path) {
                lines.push(format!(
                    "{unit}: `{}` is in {} as the row with {} of the `{}` catalog, and is no longer a part of the board that picks a row",
                    row.path,
                    picks.file,
                    row.number(),
                    row.catalog
                ));
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_solve::LockedRow;

    #[test]
    fn a_picks_file_is_read_with_its_rows_in_path_order() {
        let picks = parse(
            "version = 1\n[B.parts.\"c5\"]\ncatalog = \"capacitor\"\nlcsc = \"C307331\"\n[B.parts.\"c1\"]\ncatalog = \"mine\"\nmpn = \"X-1\"\n",
            "a.picks.toml",
        )
        .unwrap();
        let rows = picks.rows("B");
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            ChosenRow {
                path: "c1".into(),
                catalog: "mine".into(),
                lcsc: None,
                mpn: Some("X-1".into()),
            }
        );
        assert_eq!(rows[1].lcsc.as_deref(), Some("C307331"));
        assert_eq!(rows[1].number(), "lcsc C307331");
    }

    #[test]
    fn a_wrong_file_is_refused_with_the_place_in_it() {
        let wrong = |text: &str| parse(text, "x.picks.toml").unwrap_err();
        assert!(wrong("").contains("no `version`"));
        assert!(wrong("version = 2").contains("version 2 is not a version"));
        assert!(wrong("version = 1\nB = 3").contains("`B` must be a table"));
        assert!(wrong("version = 1\n[B]\nx = 1").contains("`[B]` has `x`"));
        let none = wrong("version = 1\n[B.parts.\"r1\"]\ncatalog = \"resistor\"");
        assert!(
            none.contains("[B.parts.\"r1\"] names the row by `lcsc` or by `mpn`: it has neither"),
            "{none}"
        );
        let both =
            wrong("version = 1\n[B.parts.\"r1\"]\ncatalog = \"r\"\nlcsc = \"C1\"\nmpn = \"M\"");
        assert!(both.contains("it has both"), "{both}");
        let row = wrong("version = 1\n[B.parts.\"r1\"]\ncatalog = \"r\"\nrow = \"x\"");
        assert!(row.contains("has `row`"), "{row}");
        let no_catalog = wrong("version = 1\n[B.parts.\"r1\"]\nlcsc = \"C1\"");
        assert!(no_catalog.contains("has no `catalog`"), "{no_catalog}");
    }

    #[test]
    fn an_entry_whose_part_was_not_picked_is_stale() {
        let picks = parse(
            "version = 1\n[B.parts.\"r1\"]\ncatalog = \"resistor\"\nlcsc = \"C1\"\n[B.parts.\"gone\"]\ncatalog = \"resistor\"\nlcsc = \"C2\"\n[Old.parts.\"x\"]\ncatalog = \"led\"\nlcsc = \"C3\"\n",
            "a.picks.toml",
        )
        .unwrap();
        let mut picked = PickLock::new("a.lock.toml");
        picked.set(
            "B",
            vec![LockedRow {
                path: "r1".into(),
                catalog: "resistor".into(),
                row: "R".into(),
                lcsc: Some("C1".into()),
                mpn: None,
            }],
        );
        assert_eq!(
            stale(&picks, &picked),
            vec![
                "B: `gone` is in a.picks.toml as the row with lcsc C2 of the `resistor` catalog, and is no longer a part of the board that picks a row",
                "Old: `x` is in a.picks.toml as the row with lcsc C3 of the `led` catalog, and is no longer a part of the board that picks a row",
            ]
        );
    }
}
