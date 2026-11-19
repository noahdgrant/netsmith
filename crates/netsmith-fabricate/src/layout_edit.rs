//! `netsmith layout`: read, change and check the layout file `<stem>.pcb.toml` (issue #782).
//!
//! A script or an agent that moves a few parts would write the TOML itself, and a part path
//! misspelt, a part left out or a turn of 360 degrees or more would be found much later,
//! by a refusal with no hint of the line. These are the three steps the command line takes:
//! [`show`] says the entry of each part as the placer reads it, [`set`] writes one entry (the
//! file read and written as the editor does, [`Layout::parse`] and [`Layout::to_text_over`], so
//! a comment in it is kept) and refuses what the board does not have, and [`check`] says what
//! the file has wrong against the board, one finding for each with the line of its entry.
//!
//! The frame is the layout file's (the header of `netsmith_pcb::layout`): `at` is the part's
//! footprint origin in mm from the outline's top left corner, x to the right and y down; a turn
//! is in degrees, counterclockwise on screen, from 0 up to 360 (any angle, issue #822); a
//! part on the bottom face is the top face's footprint mirrored left to right, turned as it
//! is seen from the top.
//!
//! None of these places the board: [`check`] measures a locked entry against the requirements
//! of the physical file with the footprints loaded, and the parts the file places with the
//! placement check `fab` runs ([`Board::placement_faults`] on [`Board::laid_out_entries`], issue
//! #1224), and nothing more.

use std::path::Path;

use netsmith_core::Design;
use netsmith_pcb::{Board, Layout, LayoutFace, PartPlace, PlacementFault};
use serde::Serialize;

/// Why `turn` is not a turn a part may take, if it is not: a turn is any angle from 0 up to
/// 360 degrees, not 360 itself (issue #822; it used to be 0, 90, 180 or 270 only), so that one
/// turn has one spelling in the file.
pub fn turn_error(turn: f64) -> Option<String> {
    (!(0.0..360.0).contains(&turn)).then(|| {
        format!(
            "a turn is from 0 up to 360 degrees (not 360 itself), counterclockwise on screen, and `{}` is not",
            number(turn)
        )
    })
}

/// `value` as the JSON document `--format json` prints.
pub fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).expect("plain data")
}

fn number(v: f64) -> String {
    format!("{}", (v * 1e3).round() / 1e3)
}

fn face_word(face: LayoutFace) -> &'static str {
    match face {
        LayoutFace::Top => "top",
        LayoutFace::Bottom => "bottom",
    }
}

/// The edit distance of two names, for [`nearest`].
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if ca == *cb {
                diagonal
            } else {
                1 + diagonal.min(above).min(row[j])
            };
            diagonal = above;
        }
    }
    row[b.len()]
}

/// The `count` of `names` nearest to `wanted`, the nearest first (a name that holds `wanted`,
/// or that it holds, counts as nearer than any other).
pub fn nearest(names: &[String], wanted: &str, count: usize) -> Vec<String> {
    let wanted_lower = wanted.to_lowercase();
    let mut scored: Vec<(usize, &String)> = names
        .iter()
        .map(|n| {
            let lower = n.to_lowercase();
            let near = if !wanted_lower.is_empty()
                && (lower.contains(&wanted_lower) || wanted_lower.contains(&lower))
            {
                0
            } else {
                1 + distance(&lower, &wanted_lower)
            };
            (near, n)
        })
        .collect();
    scored.sort();
    scored
        .into_iter()
        .take(count)
        .map(|(_, n)| n.clone())
        .collect()
}

/// The paths of the parts of `design`, in name order.
fn part_paths(design: &Design) -> Vec<String> {
    let mut paths: Vec<String> = design.parts.iter().map(|p| p.path.clone()).collect();
    paths.sort();
    paths
}

/// The error for `path`, which is no part of the board.
fn no_such_part(design: &Design, path: &str, names: &[String]) -> String {
    let near = nearest(names, path, 5)
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "the board `{}` has no part `{path}`; the nearest names are {near} (`netsmith layout show` lists every part)",
        design.name
    )
}

/// The layout file at `file` as text and parsed, or nothing if there is no such file.
fn read(file: &Path) -> Result<Option<(String, Layout)>, String> {
    let name = file.display().to_string();
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{name}: {e}")),
    };
    let layout = Layout::parse(&text, &name).map_err(|e| e.to_string())?;
    Ok(Some((text, layout)))
}

/// One part's entry, as the placer reads it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    /// Where the footprint's origin is, in mm from the outline's top left corner, y down.
    pub at: [f64; 2],
    /// Degrees, counterclockwise on screen.
    pub turn: f64,
    /// `top` or `bottom`.
    pub side: &'static str,
    /// The placer leaves a locked part where it is.
    pub lock: bool,
    /// Who wrote it (`hand`, `auto`, `regen`, `import`), as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The line of the entry in the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

impl Entry {
    fn of(place: &PartPlace, line: Option<usize>) -> Self {
        Self {
            at: place.at,
            turn: place.rotation,
            side: face_word(place.side),
            lock: place.lock,
            source: place.source.clone(),
            line,
        }
    }

    fn text(&self) -> String {
        format!(
            "at {}, {} mm  turn {}  {}{}{}{}",
            number(self.at[0]),
            number(self.at[1]),
            number(self.turn),
            self.side,
            if self.lock { "  locked" } else { "" },
            self.source
                .as_ref()
                .map(|s| format!("  source {s}"))
                .unwrap_or_default(),
            self.line
                .map(|l| format!("  (line {l})"))
                .unwrap_or_default(),
        )
    }
}

/// A part of the board and its entry, if the file has one.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShownPart {
    pub path: String,
    pub refdes: String,
    /// Absent when the file has no entry: the placer places the part.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<Entry>,
}

/// What [`show`] says.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Shown {
    pub board: String,
    pub file: String,
    /// Whether the file exists.
    pub exists: bool,
    pub parts: Vec<ShownPart>,
}

impl Shown {
    /// For a person: a line for each part.
    pub fn text(&self) -> String {
        let width = self
            .parts
            .iter()
            .map(|p| p.path.chars().count() + p.refdes.chars().count() + 3)
            .max()
            .unwrap_or(0);
        let mut out = format!(
            "{}: {}{}\n",
            self.board,
            self.file,
            if self.exists { "" } else { " (no such file)" }
        );
        for part in &self.parts {
            let name = format!("{} ({})", part.path, part.refdes);
            let what = match &part.entry {
                Some(e) => e.text(),
                None => "no entry: the placer places it".to_string(),
            };
            out.push_str(&format!("{name:width$}  {what}\n"));
        }
        out
    }
}

/// The entry of each of `paths` (a part's path, or a module's: the parts inside it), or of
/// every part of `design` when there is none, as the layout file at `file` has them.
pub fn show(design: &Design, file: &Path, paths: &[String]) -> Result<Shown, String> {
    let read = read(file)?;
    let names = part_paths(design);
    let mut known = names.clone();
    known.extend(design.modules.iter().map(|m| m.path.clone()));
    known.sort();
    let mut wanted: Vec<&str> = Vec::new();
    for path in paths {
        let is_module = design
            .parts
            .iter()
            .any(|p| p.path.starts_with(&format!("{path}.")));
        if !names.contains(path) && !is_module {
            return Err(no_such_part(design, path, &known));
        }
        wanted.push(path);
    }
    let lines = read
        .as_ref()
        .map(|(text, _)| Layout::part_lines(text, &design.name))
        .unwrap_or_default();
    let entries = read
        .as_ref()
        .and_then(|(_, layout)| layout.boards.get(&design.name));
    let mut parts: Vec<ShownPart> = design
        .parts
        .iter()
        .filter(|p| {
            wanted.is_empty()
                || wanted
                    .iter()
                    .any(|w| p.path == *w || p.path.starts_with(&format!("{w}.")))
        })
        .map(|p| ShownPart {
            path: p.path.clone(),
            refdes: p.refdes.clone(),
            entry: entries
                .and_then(|e| e.parts.get(&p.path))
                .map(|place| Entry::of(place, lines.get(&p.path).copied())),
        })
        .collect();
    parts.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Shown {
        board: design.name.clone(),
        file: file.display().to_string(),
        exists: read.is_some(),
        parts,
    })
}

/// What [`set`] changes of an entry; a field left `None` keeps what the entry has.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Change {
    pub at: Option<[f64; 2]>,
    pub turn: Option<f64>,
    pub side: Option<LayoutFace>,
    pub lock: Option<bool>,
}

/// What [`set`] wrote.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Written {
    pub board: String,
    pub file: String,
    pub path: String,
    pub refdes: String,
    /// Whether the file had an entry for the part before.
    pub existed: bool,
    pub entry: Entry,
}

impl Written {
    /// The line the command prints.
    pub fn text(&self) -> String {
        format!(
            "{} the entry of `{}` ({}) in {}: {}",
            if self.existed { "changed" } else { "wrote" },
            self.path,
            self.refdes,
            self.file,
            self.entry.text()
        )
    }
}

/// Write the entry of the part at `path` in the layout file at `file` (made, with that alone,
/// if it does not exist), keeping every other table as the editor does. It refuses a path the
/// board has no part at, a turn that is not from 0 up to 360, a position that is not a number,
/// and, for a part with no entry yet, a change that gives no `at` (there is no place to start
/// from); the file is then as it was. A new entry is written `source = "hand"`; an entry that
/// exists keeps its `source`.
pub fn set(design: &Design, file: &Path, path: &str, change: &Change) -> Result<Written, String> {
    let part = design
        .parts
        .iter()
        .find(|p| p.path == path)
        .ok_or_else(|| no_such_part(design, path, &part_paths(design)))?;
    if let Some(turn) = change.turn
        && let Some(why) = turn_error(turn)
    {
        return Err(why);
    }
    if let Some(at) = change.at
        && !at.iter().all(|v| v.is_finite())
    {
        return Err("the position is not a number".to_string());
    }
    let name = file.display().to_string();
    let mut layout = read(file)?.map_or_else(Layout::default, |(_, layout)| layout);
    let table = layout.boards.entry(design.name.clone()).or_default();
    let existed = table.parts.contains_key(path);
    let place = match table.parts.get(path) {
        Some(old) => PartPlace {
            at: change.at.unwrap_or(old.at),
            rotation: change.turn.unwrap_or(old.rotation),
            side: change.side.unwrap_or(old.side),
            lock: change.lock.unwrap_or(old.lock),
            ..old.clone()
        },
        None => PartPlace {
            at: change.at.ok_or_else(|| {
                format!(
                    "`{path}` has no entry in {name}, so there is no place to start from: say where it goes with `--at X,Y` (mm from the outline's top left corner, y down)"
                )
            })?,
            rotation: change.turn.unwrap_or(0.0),
            side: change.side.unwrap_or_default(),
            text: Default::default(),
            lock: change.lock.unwrap_or(false),
            source: Some("hand".to_string()),
        },
    };
    table.parts.insert(path.to_string(), place);
    let text = crate::layout::text_over(file, &layout);
    std::fs::write(file, &text).map_err(|e| format!("{name}: {e}"))?;
    let line = Layout::part_lines(&text, &design.name).get(path).copied();
    let place = &layout.boards[&design.name].parts[path];
    Ok(Written {
        board: design.name.clone(),
        file: name,
        path: path.to_string(),
        refdes: part.refdes.clone(),
        existed,
        entry: Entry::of(place, line),
    })
}

/// How serious a finding of [`check`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The file is wrong: a build or `fab` would refuse it, or its entry cannot be what was meant.
    Error,
    /// Worth knowing, nothing is wrong.
    Note,
}

/// One thing [`check`] found.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    pub severity: Severity,
    /// `no_entry`, `no_part`, `bad_turn`, `bad_place`, `requirement`, `placement` or
    /// `unmeasured`.
    pub kind: &'static str,
    /// The part's path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The line of the entry in the file, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    pub message: String,
    /// For `placement`: the fault as `fab`'s placement check gives it (`board fab --format json`
    /// and `layout fit` have the same data), with the parts, their courtyards and the amounts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fault: Option<PlacementFault>,
}

/// What [`check`] says.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Checked {
    pub board: String,
    pub file: String,
    /// Whether the file exists.
    pub exists: bool,
    /// Whether there is no error (a note does not count).
    pub ok: bool,
    pub findings: Vec<Finding>,
}

impl Checked {
    /// For a person: a line for each finding, the entry's line number after the file name.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for f in &self.findings {
            let place = match f.line {
                Some(line) => format!("{}:{line}", self.file),
                None => self.file.clone(),
            };
            let severity = match f.severity {
                Severity::Error => "error",
                Severity::Note => "note",
            };
            out.push_str(&format!("{place}: {severity}: {}\n", f.message));
        }
        if self.findings.is_empty() {
            out.push_str(&format!(
                "{}: every part of `{}` has an entry and each entry fits\n",
                self.file, self.board
            ));
        }
        out
    }
}

/// What the parts that the file locks break of the requirements of the physical file: the path
/// and the message of each. The footprints are loaded (`libraries`), nothing is placed; a
/// design with no requirement, or a file that locks no part, loads nothing.
fn locked_breaks(
    design: &Design,
    layout: &Layout,
    libraries: &netsmith_pipeline::Libraries,
) -> Result<Vec<(String, String)>, String> {
    let options = netsmith_pcb::place_options(design, Some(layout));
    if options.fixed.is_empty() || design.features.constraints.is_empty() {
        return Ok(Vec::new());
    }
    let loaded = Board::load(design, &libraries.mapping, &libraries.footprints, &options)
        .map_err(|e| e.to_string())?;
    Ok(netsmith_pcb::constraint_locked_breaks(
        design,
        &loaded.parts,
        &loaded.placement,
    ))
}

/// The faults the placement check `fab` runs ([`Board::placement_faults`]) finds on the board of
/// the parts `layout` places, each where it says ([`Board::laid_out_entries`]): `fab` puts them
/// there whatever the seed, so a fault among them stops it. Nothing is placed. `layout.1` is the
/// layout file's name for the messages. `layout check` gives these as its `placement` findings,
/// and `fab` runs the same function before the placer starts (issue 1309), so that a group of
/// parts placed by hand is known to be consistent, as one list, before any placing.
pub fn entry_faults(
    design: &Design,
    layout: &(Layout, String),
    libraries: &netsmith_pipeline::Libraries,
) -> Result<Vec<PlacementFault>, String> {
    let options = netsmith_pcb::place_options(design, Some(&layout.0));
    let board = Board::laid_out_entries(
        design,
        &libraries.mapping,
        &libraries.footprints,
        &options,
        Some(layout),
    )?;
    Ok(board.placement_faults())
}

fn placement_faults(
    design: &Design,
    layout: &Layout,
    file: &Path,
    libraries: &netsmith_pipeline::Libraries,
) -> Result<Vec<PlacementFault>, String> {
    entry_faults(
        design,
        &(layout.clone(), file.display().to_string()),
        libraries,
    )
}

/// What the layout file at `file` has wrong against `design`: a part with no entry (a note: the
/// placer places it), an entry that names no part (with the nearest names), an entry with a
/// turn that is not from 0 up to 360 or a position that is not a number, a locked entry that
/// breaks a requirement of the physical file, and each fault the placement check `fab` runs finds
/// among the parts the file places (`placement`, issue #1224): two courtyards that overlap, a
/// part past the edge, in a keepout or a cutout, over a height limit, a hole too near another
/// part. Each finding has the line of its entry.
pub fn check(
    design: &Design,
    file: &Path,
    libraries: &netsmith_pipeline::Libraries,
) -> Result<Checked, String> {
    let read = read(file)?;
    let exists = read.is_some();
    let mut findings = Vec::new();
    let (text, layout) = read.unwrap_or_default();
    let lines = Layout::part_lines(&text, &design.name);
    let empty = Default::default();
    let entries = layout.boards.get(&design.name).unwrap_or(&empty);
    let names = part_paths(design);
    for (path, place) in &entries.parts {
        let line = lines.get(path).copied();
        if !names.contains(path) {
            let near = nearest(&names, path, 3)
                .iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", ");
            findings.push(
                Finding {
                    severity: Severity::Error,
                    kind: "no_part",
                    path: Some(path.clone()),
                    line,
                    message: format!(
                        "the entry of `{path}` names no part of the board `{}`; the nearest names are {near}: rename the entry or remove it",
                        design.name
                    ),
                    fault: None,
                },
            );
        } else if let Some(why) = turn_error(place.rotation) {
            findings.push(Finding {
                severity: Severity::Error,
                kind: "bad_turn",
                path: Some(path.clone()),
                line,
                message: format!("the entry of `{path}`: {why}"),
                fault: None,
            });
        } else if !place.at.iter().all(|v| v.is_finite()) {
            findings.push(Finding {
                severity: Severity::Error,
                kind: "bad_place",
                path: Some(path.clone()),
                line,
                message: format!("the entry of `{path}` has a position that is not a number"),
                fault: None,
            });
        }
    }
    let mut unmeasured = None;
    match locked_breaks(design, &layout, libraries) {
        Ok(breaks) => {
            for (path, message) in breaks {
                findings.push(Finding {
                    severity: Severity::Error,
                    kind: "requirement",
                    line: lines.get(&path).copied(),
                    path: Some(path),
                    message,
                    fault: None,
                });
            }
        }
        Err(e) => unmeasured = Some(e),
    }
    // The placement check `fab` runs, on the parts the file places (issue #1224). A wrong
    // entry is said above, and the board is not made of it.
    let mut unplaced = None;
    if findings.is_empty() {
        match placement_faults(design, &layout, file, libraries) {
            Ok(faults) => {
                for fault in faults {
                    // The entry the fault is about: the first of its parts the file places.
                    let path = fault
                        .parts
                        .iter()
                        .map(|p| &p.part.path)
                        .find(|p| entries.parts.contains_key(*p))
                        .or_else(|| fault.parts.first().map(|p| &p.part.path))
                        .cloned();
                    findings.push(Finding {
                        severity: Severity::Error,
                        kind: "placement",
                        line: path.as_ref().and_then(|p| lines.get(p).copied()),
                        path,
                        message: fault.text.clone(),
                        fault: Some(fault),
                    });
                }
            }
            Err(e) => unplaced = Some(e),
        }
    }
    findings.sort_by_key(|f| (f.line.unwrap_or(usize::MAX), f.path.clone()));
    if let Some(e) = unmeasured {
        findings.push(Finding {
            severity: Severity::Note,
            kind: "unmeasured",
            path: None,
            line: None,
            message: format!("the locked entries were not measured against the requirements: {e}"),
            fault: None,
        });
    }
    if let Some(e) = unplaced {
        findings.push(Finding {
            severity: Severity::Note,
            kind: "unmeasured",
            path: None,
            line: None,
            message: format!("the entries were not measured by the placement check: {e}"),
            fault: None,
        });
    }
    for part in &design.parts {
        if !entries.parts.contains_key(&part.path) {
            findings.push(Finding {
                severity: Severity::Note,
                kind: "no_entry",
                path: Some(part.path.clone()),
                line: None,
                message: format!(
                    "`{}` ({}) has no entry: the placer places it",
                    part.path, part.refdes
                ),
                fault: None,
            });
        }
    }
    let ok = findings.iter().all(|f| f.severity != Severity::Error);
    Ok(Checked {
        board: design.name.clone(),
        file: file.display().to_string(),
        exists,
        ok,
        findings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(of: &[&str]) -> Vec<String> {
        of.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_nearest_names_come_first() {
        let all = names(&["stage.output_cap", "stage.input_cap", "led", "stage.r1"]);
        assert_eq!(nearest(&all, "stage.outpt_cap", 2)[0], "stage.output_cap");
        assert_eq!(nearest(&all, "output_cap", 1), ["stage.output_cap"]);
        assert_eq!(nearest(&all, "x", 9).len(), 4);
    }

    #[test]
    fn a_turn_is_any_angle_from_0_up_to_360() {
        for turn in [0.0, 45.0, 90.0, 91.0, 180.0, 270.0, 359.5] {
            assert_eq!(turn_error(turn), None);
        }
        for turn in [360.0, -90.0, -0.5, 720.0, f64::NAN] {
            assert!(turn_error(turn).is_some(), "{turn}");
        }
    }
}
