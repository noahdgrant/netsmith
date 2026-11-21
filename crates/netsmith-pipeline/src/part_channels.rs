//! The check of the parts that name their own supply channel (`[supply.parts]` of
//! `netsmith.toml`, ADR 0143 item 4) against the design of one file, before it is solved.
//!
//! An entry names a part as the pick lock does: by the board or system it is solved in and its
//! path there (`[supply.parts.Board] "ldo.regulator"`), or by a bare path for a part of the
//! project's only board. An entry under a board or system of this file must name a part of it
//! that picks from a catalog; an entry under a name that no top-level file of the project
//! declares names nothing, and so does a bare path in a project with several boards. Each is a
//! problem that names `netsmith.toml` and the key. An entry under a board or system of
//! another top-level file is that file's to check. The solver then buys such a part by its
//! channel only (`netsmith_solve`).

use netsmith_check::{Checked, InstKind, Model, closest};
use netsmith_solve::{Catalogs, PartChannel};

use crate::{Problem, Stage};

/// The problems of the `[supply.parts]` entries of `catalogs` against the boards and systems of
/// `checked`, empty when every entry this file can judge names a part it has.
pub(crate) fn problems(checked: &Checked, catalogs: &Catalogs) -> Vec<Problem> {
    let part_channels = catalogs.part_channels();
    if part_channels.is_empty() {
        return Vec::new();
    }
    // The units a lock's table is named by: the boards solved alone, and the systems.
    let units: Vec<(&str, &Model)> = checked
        .standalone_boards()
        .map(|b| (b.name.as_str(), &b.model))
        .chain(checked.systems.iter().map(|s| (s.name.as_str(), &s.model)))
        .collect();
    let mut names: Vec<String> = part_channels.declared.clone();
    names.extend(units.iter().map(|(n, _)| n.to_string()));
    names.sort();
    names.dedup();
    let others: Vec<&String> = names
        .iter()
        .filter(|n| !units.iter().any(|(u, _)| u == n))
        .filter(|n| !checked.boards.iter().any(|b| &b.name == *n))
        .collect();
    let mut out = Vec::new();
    let mut seen: Vec<(&str, &str, &str)> = Vec::new();
    for entry in &part_channels.entries {
        let key = &entry.named.key;
        let unit = match &entry.unit {
            Some(unit) => unit.as_str(),
            None => match (units.as_slice(), others.is_empty()) {
                ([(only, _)], true) => only,
                _ => {
                    out.push(problem(format!(
                        "{key} names a part by its path alone, and the project has several boards and systems ({}): name its board as the pick lock does, `[supply.parts.{}]` with `\"{}\" = \"{}\"`",
                        names.join(", "),
                        units.first().map_or("Board", |(n, _)| n),
                        entry.path,
                        entry.named.channel
                    )));
                    continue;
                }
            },
        };
        if let Some((_, model)) = units.iter().find(|(u, _)| *u == unit) {
            if let Some(why) = not_a_part(model, entry) {
                out.push(problem(format!("{key}: {why}")));
            } else if let Some((.., other)) =
                seen.iter().find(|(u, p, _)| *u == unit && *p == entry.path)
            {
                out.push(problem(format!(
                    "{key} names the part that {} names too: a part names one channel, so keep one of the two",
                    other.split_once(": ").map_or(*other, |(_, k)| k)
                )));
            } else {
                seen.push((unit, &entry.path, key));
            }
            continue;
        }
        if let Some(system) = checked
            .systems
            .iter()
            .find(|s| s.boards.iter().any(|b| b.type_name == unit))
        {
            let member = system
                .boards
                .iter()
                .find(|b| b.type_name == unit)
                .map_or("board", |b| b.name.as_str());
            out.push(problem(format!(
                "{key}: the board `{unit}` is solved in the system `{}`, so its parts are named under the system, as its pick lock names them: `[supply.parts.{}]` with `\"{member}.{}\" = \"{}\"`",
                system.name, system.name, entry.path, entry.named.channel
            )));
            continue;
        }
        if part_channels.declared.iter().any(|d| d == unit) {
            // A board or system of another top-level file: that file's build checks it.
            continue;
        }
        let hint = closest(unit, &names).map_or(String::new(), |n| format!(" (`{n}`?)"));
        out.push(problem(format!(
            "{key} names the board or system `{unit}`{hint}, which no top-level file of the project declares; the project has {}. A part's path with a dot in it is quoted (`\"ldo.regulator\" = ...`), or TOML reads its first name as a board",
            if names.is_empty() {
                "none".to_string()
            } else {
                names.join(", ")
            }
        )));
    }
    out
}

/// Why the path of `entry` names no part of `model` that a channel can buy, if it does not.
fn not_a_part(model: &Model, entry: &PartChannel) -> Option<String> {
    let found = model
        .instances()
        .find(|(id, _)| model.path_of(*id) == entry.path);
    match found {
        Some((_, inst)) if inst.kind == InstKind::Part && inst.catalog.is_some() => None,
        Some((_, inst)) if inst.kind == InstKind::Part => Some(format!(
            "`{}` (`{}`) picks from no catalog, so no channel buys it by a row's offer: remove the entry",
            entry.path, inst.type_name
        )),
        Some((_, inst)) => Some(format!(
            "`{}` is a `{}`, not a part; an entry names one part by its path, as a hand pick does (`{}.u1`)",
            entry.path, inst.type_name, entry.path
        )),
        None => {
            let parts: Vec<String> = model
                .instances()
                .filter(|(_, i)| i.kind == InstKind::Part && i.catalog.is_some())
                .map(|(id, _)| model.path_of(id))
                .collect();
            let hint = closest(&entry.path, &parts)
                .map_or(String::new(), |p| format!(": did you mean `{p}`?"));
            Some(format!(
                "there is no part at the path `{}`{hint}",
                entry.path
            ))
        }
    }
}

fn problem(message: String) -> Problem {
    let rendered = format!("error: {message}\n");
    Problem::general(Stage::Check, message, rendered)
}
