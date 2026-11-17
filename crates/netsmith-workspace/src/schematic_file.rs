//! `netsmith schematic` (issue #1093): the schematic layout file `<stem>.sch.toml` (ADR 0061,
//! 0139) shown, written one entry at a time, checked against the board and used to arrange the
//! sheets of a big board, so that a script or an agent that keeps a drawing legible need not
//! write the TOML itself. The editor reads and writes the same file over
//! `GET`/`PUT /api/schematic-hints`; this is the command line's way, by the same code:
//! [`netsmith_view::hints`] reads and writes the file, [`netsmith_view::LayoutHints::resolve`]
//! matches it with the design, and [`crate::hinted_layout`] draws with it (the editor's analysis
//! calls it too), so an entry the editor lists as a finding is a finding here.
//!
//! The four steps: [`show`] says the sheets of the board and each entry of the file, with
//! whether the drawing uses it; [`set`] writes one entry and refuses a name the board does not
//! have; [`check`] says which entries name no sheet or part and which the drawing could not
//! honour (a block that would lie on another part, a box on another box); [`arrange`] says where
//! the drawing sets each sheet's box and, with `write`, keeps the arrangement as `at` entries.
//!
//! **Comments are kept.** The file is read as the editor reads it and written as
//! [`netsmith_view::hints::to_toml`] writes it (the same hints always give the same text), with
//! the comments of the file as it was carried into it (`netsmith_pcb::layout_notes::carry`, as
//! the layout file's writer does, issue #827), so every other table and every comment stays; the
//! spacing and order of a hand-edited file are normalised. A file that already says what is asked
//! is not written. The editor's `PUT` writes the page's text as `to_toml` has it and keeps no
//! comment: the page has no comment to keep.
//!
//! A file with a fault in its text (a line with its message) is ignored by the drawing as a whole,
//! so `show`, `set` and `arrange` refuse it, naming the lines, and `check` reports each fault.
//! Nothing here places the PCB or needs KiCad.

use std::fs;
use std::path::Path;

use netsmith_pipeline::{Built, Libraries};
use netsmith_view::hints::{self, GRID_MM};
use netsmith_view::{
    DesignSheet, Entry, EntryKind, FindingKind, HintError, HintFinding, Layout, LayoutHints,
    Located, NoteHint, PartHint, Place, SheetHints, Steps, design_sheets,
};
use serde::Serialize;

/// `value` as the JSON document `--format json` prints.
pub fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).expect("plain data")
}

/// Hundredths of a millimetre are all the file writes.
fn mm(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn num(value: f64) -> String {
    format!("{}", mm(value))
}

/// The schematic layout file beside the source file at `source` (named `name` in the project),
/// and its name in the project: `src/main.sch.toml` beside `src/main.nsm`.
pub fn file_of(source: &Path, name: &str) -> (std::path::PathBuf, String) {
    (
        netsmith_view::schematic_hints_path(source),
        crate::hints::name_of(name),
    )
}

// ---- reading the file ----

/// The file as it is on disk.
struct File {
    hints: LayoutHints,
    text: String,
    exists: bool,
}

/// Why the file cannot be used.
enum Fault {
    Unreadable(String),
    Text(Vec<HintError>),
}

fn open(file: &Path) -> Result<File, Fault> {
    match fs::read_to_string(file) {
        Ok(text) => match hints::parse(&text) {
            Ok(hints) => Ok(File {
                hints,
                text,
                exists: true,
            }),
            Err(errors) => Err(Fault::Text(errors)),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(File {
            hints: LayoutHints::default(),
            text: String::new(),
            exists: false,
        }),
        Err(e) => Err(Fault::Unreadable(e.to_string())),
    }
}

/// The refusal for a file that cannot be used, one line for each fault.
fn refusal(name: &str, fault: Fault) -> String {
    match fault {
        Fault::Unreadable(why) => format!("{name} cannot be read: {why}"),
        Fault::Text(errors) => {
            let mut said: Vec<String> = errors
                .iter()
                .map(|e| format!("{name}:{}: {}", e.line, e.message))
                .collect();
            said.push(format!(
                "the drawing ignores a file with a fault, so fix {} first",
                if errors.len() == 1 {
                    "the line"
                } else {
                    "the lines"
                }
            ));
            said.join("\n")
        }
    }
}

/// What a board's drawing says about the file.
struct Drawn {
    /// The entries that match nothing, and those the drawing could not honour, in the order of
    /// the file.
    findings: Vec<HintFinding>,
    layout: Result<Layout, String>,
}

fn draw(built: &Built, libraries: &Libraries, hints: &LayoutHints) -> Drawn {
    let groups = crate::module_groups(built);
    let (resolved, mut findings) = hints.resolve(&built.design, &groups);
    let layout = match crate::hinted_layout(built, &groups, libraries, &resolved) {
        Ok((layout, unusable)) => {
            findings.extend(unusable);
            Ok(layout)
        }
        Err(e) => Err(e),
    };
    findings.sort_by(|a, b| (a.line, &a.message).cmp(&(b.line, &b.message)));
    Drawn { findings, layout }
}

// ---- show ----

/// A box on the page, in millimetres, x to the right and y down.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A sheet of the board: a module instance, or the parts the board declares itself.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShownSheet {
    pub name: String,
    /// Whether it is the board's own sheet.
    pub own: bool,
    pub parts: usize,
    /// Whether the file places its box (`at`) and the drawing could use it.
    pub placed: bool,
    /// Where the drawing puts its box: left out when the board is not drawn by module, the sheet
    /// is drawn as several boxes, or the drawing failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<Frame>,
}

/// One entry of the file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShownEntry {
    /// The sheet's name; left out for the board's own sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet: Option<String>,
    /// `sheet` (its place), `part` or `note`.
    pub kind: &'static str,
    /// A part's path in the design (or its key on its sheet when the design has no such part),
    /// a note's key, a sheet's name.
    pub path: String,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dx: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dy: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotate: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirror: Option<bool>,
    /// Whether the drawing uses the entry; if not, `message` says why.
    pub used: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// What `schematic show` says.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Shown {
    pub board: String,
    /// The file's name in the project (`src/main.sch.toml`).
    pub file: String,
    pub exists: bool,
    /// Whether the board is drawn by module (more than 16 parts), the only drawing whose sheets
    /// have boxes to place.
    pub by_module: bool,
    pub sheets: Vec<ShownSheet>,
    pub entries: Vec<ShownEntry>,
    /// Why the board could not be drawn, when it could not: `used` is then only what matching
    /// the file with the design says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drawing_error: Option<String>,
}

fn steps_mm(s: Steps) -> [f64; 2] {
    let (x, y) = s.mm();
    [mm(x), mm(y)]
}

fn shown_entry(entry: &Entry, sheets: &[DesignSheet], findings: &[HintFinding]) -> ShownEntry {
    let design_path = |sheet: &Option<String>, key: &str| {
        sheets
            .iter()
            .find(|s| match sheet {
                Some(name) => !s.own && &s.name == name,
                None => s.own,
            })
            .and_then(|s| s.parts.get(key).cloned())
            .unwrap_or_else(|| key.to_string())
    };
    let path = match entry.kind {
        EntryKind::Sheet => entry.sheet.clone().unwrap_or_default(),
        EntryKind::Part => design_path(&entry.sheet, &entry.key),
        EntryKind::Note => entry.key.clone(),
    };
    let finding = findings.iter().find(|f| f.entry == entry.header);
    let (mut dx, mut dy, mut pin) = (None, None, None);
    match entry.place {
        Some(Place::Offset(s)) => {
            let [x, y] = steps_mm(s);
            (dx, dy) = (Some(x), Some(y));
        }
        Some(Place::Pinned(s)) => pin = Some(steps_mm(s)),
        None => {}
    }
    ShownEntry {
        sheet: entry.sheet.clone(),
        kind: match entry.kind {
            EntryKind::Sheet => "sheet",
            EntryKind::Part => "part",
            EntryKind::Note => "note",
        },
        path,
        line: entry.line,
        dx,
        dy,
        pin,
        at: entry.at.map(steps_mm),
        rotate: entry.rotate,
        mirror: entry.mirror,
        used: finding.is_none(),
        message: finding.map(|f| f.message.clone()),
    }
}

/// The box of the sheet called `name` in the drawing, when it is drawn as one box.
fn frame_of(layout: &Layout, name: &str) -> Option<Frame> {
    let mut boxes = layout.sheets.iter().filter(|s| s.name == name);
    let sheet = boxes.next()?;
    boxes.next().is_none().then(|| Frame {
        x: mm(sheet.frame.min.x),
        y: mm(sheet.frame.min.y),
        width: mm(sheet.frame.width()),
        height: mm(sheet.frame.height()),
    })
}

/// What the name `wanted` could be, for a refusal: the nearest of `names`.
fn say_nearest(names: &[String], wanted: &str, what: &str) -> String {
    let near = netsmith_fabricate::layout_edit::nearest(names, wanted, 5);
    if near.is_empty() {
        format!("the board has no {what} `{wanted}`")
    } else {
        format!(
            "the board has no {what} `{wanted}`; the nearest: {}",
            near.iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn part_paths(sheets: &[DesignSheet]) -> Vec<String> {
    let mut all: Vec<String> = sheets
        .iter()
        .flat_map(|s| s.parts.values().cloned())
        .collect();
    all.sort();
    all
}

fn sheet_names(sheets: &[DesignSheet]) -> Vec<String> {
    sheets.iter().map(|s| s.name.clone()).collect()
}

/// A note named in full, as the design names what it is about: `stage.output_cap#doc` for the
/// key `output_cap#doc` on the sheet `stage`.
fn note_name(sheet: &DesignSheet, key: &str) -> String {
    if sheet.own {
        key.to_string()
    } else if key.starts_with('#') {
        format!("{}{key}", sheet.name)
    } else {
        format!("{}.{key}", sheet.name)
    }
}

/// Whether the file places the box of `sheet` and the drawing could use the place.
fn has_place(entries: &[Entry], findings: &[HintFinding], sheet: &DesignSheet) -> bool {
    entries.iter().any(|e| {
        e.kind == EntryKind::Sheet
            && match &e.sheet {
                Some(n) => !sheet.own && *n == sheet.name,
                None => sheet.own,
            }
            && !findings.iter().any(|f| f.entry == e.header)
    })
}

/// `show`: the sheets of the board and the entries of the file, for the sheets and parts in
/// `only` (all of them when it is empty). A name the board has neither a sheet nor a part by is
/// refused.
pub fn show(
    built: &Built,
    libraries: &Libraries,
    file: &Path,
    name: &str,
    only: &[String],
) -> Result<Shown, String> {
    let opened = open(file).map_err(|f| refusal(name, f))?;
    let groups = crate::module_groups(built);
    let sheets = design_sheets(&built.design, &groups);
    for wanted in only {
        let known = sheets.iter().any(|s| {
            s.name == *wanted
                || s.parts.values().any(|p| p == wanted)
                || s.notes.iter().any(|k| note_name(s, k) == *wanted)
        });
        if !known {
            let mut names = sheet_names(&sheets);
            names.extend(part_paths(&sheets));
            return Err(say_nearest(&names, wanted, "sheet or part"));
        }
    }
    let drawn = draw(built, libraries, &opened.hints);
    let board = &built.design.name;
    let entries: Vec<Entry> = opened.hints.entries(board);
    let by_module = drawn
        .layout
        .as_ref()
        .is_ok_and(|layout| !layout.sheets.is_empty());
    let shown_sheets: Vec<ShownSheet> = sheets
        .iter()
        .filter(|s| only.is_empty() || only.iter().any(|w| *w == s.name || owns(s, w)))
        .map(|s| ShownSheet {
            name: s.name.clone(),
            own: s.own,
            parts: s.parts.len(),
            placed: has_place(&entries, &drawn.findings, s),
            frame: drawn
                .layout
                .as_ref()
                .ok()
                .and_then(|layout| frame_of(layout, &s.name)),
        })
        .collect();
    let keep = |entry: &Entry| {
        if only.is_empty() {
            return true;
        }
        let sheet = sheets.iter().find(|s| match &entry.sheet {
            Some(n) => !s.own && s.name == *n,
            None => s.own,
        });
        only.iter().any(|w| match (entry.kind, sheet) {
            (_, Some(s)) if s.name == *w => true,
            (EntryKind::Part, Some(s)) => s.parts.get(&entry.key).is_some_and(|p| p == w),
            (EntryKind::Note, Some(s)) => note_name(s, &entry.key) == *w,
            _ => false,
        })
    };
    Ok(Shown {
        board: board.clone(),
        file: name.to_string(),
        exists: opened.exists,
        by_module,
        sheets: shown_sheets,
        entries: entries
            .iter()
            .filter(|e| keep(e))
            .map(|e| shown_entry(e, &sheets, &drawn.findings))
            .collect(),
        drawing_error: drawn.layout.err(),
    })
}

/// Whether `sheet` has the part or note `wanted`.
fn owns(sheet: &DesignSheet, wanted: &str) -> bool {
    sheet.parts.values().any(|p| p == wanted)
        || sheet.notes.iter().any(|k| note_name(sheet, k) == wanted)
}

impl Shown {
    pub fn text(&self) -> String {
        let mut out = format!("board {}: {}", self.board, self.file);
        if !self.exists {
            out.push_str(" (no file: the drawing is the automatic one)");
        }
        out.push('\n');
        if let Some(why) = &self.drawing_error {
            out.push_str(&format!("the drawing failed: {why}\n"));
        }
        if self.by_module {
            out.push_str("sheets (the board is drawn by module):\n");
        } else {
            out.push_str("sheets (the board is drawn as one sheet: their boxes are not placed):\n");
        }
        for sheet in &self.sheets {
            out.push_str(&format!(
                "  {}{}: {} part{}",
                sheet.name,
                if sheet.own { " (the board's own)" } else { "" },
                sheet.parts,
                if sheet.parts == 1 { "" } else { "s" },
            ));
            if let Some(f) = &sheet.frame {
                out.push_str(&format!(
                    ", box at {}, {} ({} x {} mm){}",
                    num(f.x),
                    num(f.y),
                    num(f.width),
                    num(f.height),
                    if sheet.placed { ", placed" } else { "" }
                ));
            }
            out.push('\n');
        }
        if self.entries.is_empty() {
            out.push_str("entries: none\n");
        } else {
            out.push_str("entries:\n");
        }
        for e in &self.entries {
            out.push_str(&format!(
                "  {}:{}: {} {}",
                self.file, e.line, e.kind, e.path
            ));
            let mut fields = Vec::new();
            if let Some(a) = e.at {
                fields.push(format!("at {}, {}", num(a[0]), num(a[1])));
            }
            if let (Some(x), Some(y)) = (e.dx, e.dy) {
                fields.push(format!("dx {} dy {}", num(x), num(y)));
            }
            if let Some(p) = e.pin {
                fields.push(format!("pin {}, {}", num(p[0]), num(p[1])));
            }
            if let Some(r) = e.rotate {
                fields.push(format!("rotate {r}"));
            }
            if let Some(m) = e.mirror {
                fields.push(format!("mirror {m}"));
            }
            if !fields.is_empty() {
                out.push_str(&format!("  {}", fields.join("  ")));
            }
            if let Some(why) = &e.message {
                out.push_str(&format!("\n    ignored: {why}"));
            }
            out.push('\n');
        }
        out
    }
}

// ---- check ----

/// One thing wrong with the file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    /// `invalid` (a fault of the file's text, which makes the drawing ignore the file),
    /// `stale_sheet`, `stale_entry`, `wrong_sheet`, `unknown_board` (an entry or table that
    /// matches nothing) or `unusable` (the drawing could not honour the entry).
    pub kind: &'static str,
    /// The line in the file; 0 for none.
    pub line: u32,
    /// The entry's table as the file names it; empty for a fault of the text.
    pub entry: String,
    pub message: String,
}

fn kind_word(kind: FindingKind) -> &'static str {
    match kind {
        FindingKind::StaleSheet => "stale_sheet",
        FindingKind::StaleEntry => "stale_entry",
        FindingKind::WrongSheet => "wrong_sheet",
        FindingKind::UnknownBoard => "unknown_board",
        FindingKind::Unusable => "unusable",
    }
}

/// What `schematic check` says.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Checked {
    pub board: String,
    pub file: String,
    pub exists: bool,
    pub ok: bool,
    pub findings: Vec<Finding>,
    /// Why the board could not be drawn, which leaves the entries the drawing could not honour
    /// unchecked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drawing_error: Option<String>,
}

/// `check`: the faults of the file's text, the entries that name no sheet, part or note of the
/// board (and the tables for boards the source lacks, `boards` being all its boards), and the
/// entries the drawing could not honour, each with its line. The same findings the editor lists
/// in the Problems dock (stage `layout`).
pub fn check(
    built: &Built,
    libraries: &Libraries,
    file: &Path,
    name: &str,
    boards: &[String],
) -> Result<Checked, String> {
    let board = built.design.name.clone();
    let opened = match open(file) {
        Ok(opened) => opened,
        Err(Fault::Unreadable(why)) => return Err(format!("{name} cannot be read: {why}")),
        Err(Fault::Text(errors)) => {
            return Ok(Checked {
                board,
                file: name.to_string(),
                exists: true,
                ok: false,
                findings: errors
                    .into_iter()
                    .map(|e| Finding {
                        kind: "invalid",
                        line: e.line,
                        entry: String::new(),
                        message: e.message,
                    })
                    .collect(),
                drawing_error: None,
            });
        }
    };
    let drawn = draw(built, libraries, &opened.hints);
    let names: Vec<&str> = boards.iter().map(String::as_str).collect();
    let mut found = drawn.findings.clone();
    found.extend(opened.hints.unknown_boards(&names));
    found.sort_by(|a, b| (a.line, &a.message).cmp(&(b.line, &b.message)));
    let findings: Vec<Finding> = found
        .into_iter()
        .map(|f| Finding {
            kind: kind_word(f.kind),
            line: f.line,
            entry: f.entry,
            message: f.message,
        })
        .collect();
    let drawing_error = drawn.layout.err();
    Ok(Checked {
        board,
        file: name.to_string(),
        exists: opened.exists,
        ok: findings.is_empty() && drawing_error.is_none(),
        findings,
        drawing_error,
    })
}

impl Checked {
    pub fn text(&self) -> String {
        let mut out = String::new();
        for f in &self.findings {
            out.push_str(&if f.line == 0 {
                format!("{}: {}\n", self.file, f.message)
            } else {
                format!("{}:{}: {}\n", self.file, f.line, f.message)
            });
        }
        if let Some(why) = &self.drawing_error {
            out.push_str(&format!(
                "the board could not be drawn, so what the drawing cannot honour is not checked: {why}\n"
            ));
        }
        if self.ok {
            out.push_str(&if self.exists {
                format!(
                    "{}: every entry is used by the drawing of {}\n",
                    self.file, self.board
                )
            } else {
                format!("{}: no file; nothing to check\n", self.file)
            });
        }
        out
    }
}

// ---- set ----

/// What `schematic set` changes: the flags of the command.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Change {
    /// A part's, or a note's, move from where the drawing puts its block, in millimetres.
    pub by: Option<(f64, f64)>,
    /// A part's block pinned to this place from the sheet's reference part.
    pub pin: Option<(f64, f64)>,
    /// A part's turn: 0, 90, 180 or 270 degrees counterclockwise.
    pub rotate: Option<i32>,
    pub mirror: Option<bool>,
    /// A sheet's box's top-left corner on the page.
    pub at: Option<(f64, f64)>,
    /// Remove the entry (a sheet's `at`).
    pub clear: bool,
}

/// What `schematic set` did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Written {
    pub board: String,
    pub file: String,
    pub path: String,
    /// `sheet`, `part` or `note`.
    pub kind: &'static str,
    /// The entry as written; left out when it was cleared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<ShownEntry>,
    pub cleared: bool,
    /// Whether the file is different now.
    pub written: bool,
    /// What the drawing says about the entry once written (it is written all the same).
    pub warnings: Vec<String>,
}

enum Target {
    Sheet(DesignSheet),
    Part { sheet: DesignSheet, key: String },
    Note { sheet: DesignSheet, key: String },
}

impl Target {
    fn kind(&self) -> &'static str {
        match self {
            Target::Sheet(_) => "sheet",
            Target::Part { .. } => "part",
            Target::Note { .. } => "note",
        }
    }

    /// The sheet's name as a table of the file names it: `None` for the board's own.
    fn table(&self) -> Option<String> {
        let sheet = match self {
            Target::Sheet(s) | Target::Part { sheet: s, .. } | Target::Note { sheet: s, .. } => s,
        };
        (!sheet.own).then(|| sheet.name.clone())
    }
}

fn find_target(sheets: &[DesignSheet], path: &str, change: &Change) -> Result<Target, String> {
    let sheet_named = || sheets.iter().find(|s| s.name == path).cloned();
    if change.at.is_some() {
        return sheet_named().map(Target::Sheet).ok_or_else(|| {
            format!(
                "{}; a sheet is a module instance of the board, or the board's own",
                say_nearest(&sheet_names(sheets), path, "sheet")
            )
        });
    }
    for sheet in sheets {
        if let Some((key, _)) = sheet.parts.iter().find(|(_, p)| *p == path) {
            return Ok(Target::Part {
                sheet: sheet.clone(),
                key: key.clone(),
            });
        }
    }
    for sheet in sheets {
        if let Some(key) = sheet.notes.iter().find(|k| note_name(sheet, k) == path) {
            return Ok(Target::Note {
                sheet: sheet.clone(),
                key: key.clone(),
            });
        }
    }
    if change.clear
        && let Some(sheet) = sheet_named()
    {
        return Ok(Target::Sheet(sheet));
    }
    let mut names = part_paths(sheets);
    if change.clear {
        names.extend(sheet_names(sheets));
    }
    Err(say_nearest(&names, path, "part"))
}

/// `value` millimetres as grid steps, refused when it is not on the grid the file keeps.
fn steps_of(value: f64, what: &str) -> Result<i32, String> {
    let exact = value / GRID_MM;
    let steps = exact.round();
    if !value.is_finite() || (value - steps * GRID_MM).abs() > 0.0051 {
        let lo = (exact.floor() * GRID_MM * 100.0).round() / 100.0;
        let hi = (exact.ceil() * GRID_MM * 100.0).round() / 100.0;
        return Err(format!(
            "{what} {} is not on the schematic's 1.27 mm grid; the nearest are {} and {}",
            num(value),
            num(lo),
            num(hi)
        ));
    }
    if steps.abs() > 4000.0 {
        return Err(format!(
            "{what} {} is more than 5080 mm: that is a typo and not a drawing",
            num(value)
        ));
    }
    Ok(steps as i32)
}

fn point(at: (f64, f64), what: &str) -> Result<Steps, String> {
    Ok(Steps::new(steps_of(at.0, what)?, steps_of(at.1, what)?))
}

/// The change's flags as they go with the target, or why they do not.
fn check_change(change: &Change, target: &Target) -> Result<(), String> {
    let part_flags = change.pin.is_some() || change.rotate.is_some() || change.mirror.is_some();
    if change.clear && (part_flags || change.by.is_some() || change.at.is_some()) {
        return Err("`--clear` removes the entry and goes with no other flag".into());
    }
    if change.by.is_some() && change.pin.is_some() {
        return Err(
            "an entry is moved (`--by`) or pinned (`--pin`), not both: `pin` with `dx` or `dy` is an error of the file"
                .into(),
        );
    }
    if !change.clear && change.by.is_none() && change.at.is_none() && !part_flags {
        return Err("nothing to set: give `--by`, `--pin`, `--rotate`, `--mirror` or `--no-mirror` for a part, `--by` for a note, `--at` for a sheet, or `--clear`".into());
    }
    match target {
        Target::Sheet(_) if !change.clear && change.at.is_none() => {
            Err("a sheet takes `--at X,Y` (the top-left corner of its box) or `--clear`".into())
        }
        Target::Part { .. } if change.at.is_some() => Err(
            "`--at` places a sheet's box; a part takes `--by`, `--pin`, `--rotate` and `--mirror`"
                .into(),
        ),
        Target::Note { .. } if part_flags || change.at.is_some() => {
            Err("a note takes `--by` (how far its frame moves) or `--clear`".into())
        }
        _ => Ok(()),
    }
}

fn sheet_hints_mut<'a>(
    hints: &'a mut LayoutHints,
    board: &str,
    table: &Option<String>,
) -> &'a mut SheetHints {
    let board_hints = &mut hints.boards.entry(board.to_string()).or_default().value;
    match table {
        None => &mut board_hints.board,
        Some(name) => &mut board_hints.sheets.entry(name.clone()).or_default().value,
    }
}

/// The text to write for `new` over the file as it is: the file itself when it already says
/// `new`, the comments of the file carried into [`hints::to_toml`] when it exists.
fn text_for(old: &File, new: &LayoutHints) -> Result<String, String> {
    let new = pruned(new);
    if old.exists && pruned(&old.hints) == new {
        return Ok(old.text.clone());
    }
    let canonical = hints::to_toml(&new);
    let text = if old.exists {
        netsmith_pcb::layout_notes::carry(&old.text, &canonical)
    } else {
        canonical
    };
    match hints::parse(&text) {
        Ok(read) if read == new => Ok(text),
        _ => Err(
            "internal error: the entry did not read back as written; the file is untouched".into(),
        ),
    }
}

/// `hints` without the tables that hold no entry, which the file does not write.
fn pruned(hints: &LayoutHints) -> LayoutHints {
    let mut out = hints.clone();
    for board in out.boards.values_mut() {
        board
            .value
            .sheets
            .retain(|_, sheet| !sheet.value.is_empty());
    }
    out.boards.retain(|_, board| !board.value.is_empty());
    out
}

/// Write `text` over `file` if it differs from what is there, and say whether it did.
fn put(file: &Path, old: &File, text: &str) -> Result<bool, String> {
    if old.exists && old.text == text {
        return Ok(false);
    }
    crate::files::write_atomic(file, text).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(true)
}

/// `set`: write the entry at `path` (a part's path, a note's `<path>#doc`, a sheet's name for
/// `--at` or `--clear`), keeping every other table and comment. Refused, with the file as it
/// was: a name the board has none by (the nearest are listed), flags that do not go with it, a
/// number off the 1.27 mm grid, a turn that is not 0, 90, 180 or 270, and a file with a fault.
/// An entry the drawing then cannot honour is written all the same, and said in `warnings`.
pub fn set(
    built: &Built,
    libraries: &Libraries,
    file: &Path,
    name: &str,
    path: &str,
    change: &Change,
) -> Result<Written, String> {
    let old = open(file).map_err(|f| refusal(name, f))?;
    let board = built.design.name.clone();
    let sheets = design_sheets(&built.design, &crate::module_groups(built));
    let target = find_target(&sheets, path, change)?;
    check_change(change, &target)?;
    let table = target.table();
    let mut new = old.hints.clone();
    let cleared = change.clear;
    match &target {
        Target::Sheet(_) => {
            let sheet = sheet_hints_mut(&mut new, &board, &table);
            sheet.at = match change.at {
                Some(at) => Some(Located::new(point(at, "`--at`")?)),
                None => None,
            };
        }
        Target::Part { key, .. } => {
            let sheet = sheet_hints_mut(&mut new, &board, &table);
            if change.clear {
                sheet.parts.remove(key);
            } else {
                let mut hint = sheet
                    .parts
                    .get(key)
                    .map_or_else(PartHint::default, |h| h.value);
                if let Some(by) = change.by {
                    hint.place = Some(Place::Offset(point(by, "`--by`")?));
                }
                if let Some(pin) = change.pin {
                    hint.place = Some(Place::Pinned(point(pin, "`--pin`")?));
                }
                if let Some(turn) = change.rotate {
                    hint.rotate = Some(match turn {
                        0 | 90 | 180 | 270 => turn as u16,
                        -90 => 270,
                        _ => {
                            return Err(format!(
                                "a part is turned to 0, 90, 180 or 270 degrees counterclockwise, and `{turn}` is not"
                            ));
                        }
                    });
                }
                if let Some(mirror) = change.mirror {
                    hint.mirror = Some(mirror);
                }
                sheet.parts.insert(key.clone(), Located::new(hint));
            }
        }
        Target::Note { key, .. } => {
            let sheet = sheet_hints_mut(&mut new, &board, &table);
            if change.clear {
                sheet.notes.remove(key);
            } else if let Some(by) = change.by {
                sheet.notes.insert(
                    key.clone(),
                    Located::new(NoteHint {
                        by: point(by, "`--by`")?,
                    }),
                );
            }
        }
    }
    let text = text_for(&old, &new)?;
    let written = put(file, &old, &text)?;
    // The entry as the file has it now, and what the drawing says about it.
    let reread = hints::parse(&text).unwrap_or_default();
    let entries = reread.entries(&board);
    let entry = entries.iter().find(|e| {
        e.sheet == table
            && match &target {
                Target::Sheet(_) => e.kind == EntryKind::Sheet,
                Target::Part { key: k, .. } => e.kind == EntryKind::Part && e.key == *k,
                Target::Note { key: k, .. } => e.kind == EntryKind::Note && e.key == *k,
            }
    });
    let drawn = draw(built, libraries, &reread);
    let warnings = entry
        .map(|e| {
            drawn
                .findings
                .iter()
                .filter(|f| f.entry == e.header)
                .map(|f| f.message.clone())
                .collect()
        })
        .unwrap_or_default();
    Ok(Written {
        board,
        file: name.to_string(),
        path: path.to_string(),
        kind: target.kind(),
        entry: entry.map(|e| shown_entry(e, &sheets, &[])),
        cleared,
        written,
        warnings,
    })
}

impl Written {
    pub fn text(&self) -> String {
        let mut out = match (&self.entry, self.cleared) {
            (_, true) => format!("cleared the entry of {} {}", self.kind, self.path),
            (Some(e), false) => {
                let mut fields = Vec::new();
                if let Some(a) = e.at {
                    fields.push(format!("at {}, {}", num(a[0]), num(a[1])));
                }
                if let (Some(x), Some(y)) = (e.dx, e.dy) {
                    fields.push(format!("dx {} dy {}", num(x), num(y)));
                }
                if let Some(p) = e.pin {
                    fields.push(format!("pin {}, {}", num(p[0]), num(p[1])));
                }
                if let Some(r) = e.rotate {
                    fields.push(format!("rotate {r}"));
                }
                if let Some(m) = e.mirror {
                    fields.push(format!("mirror {m}"));
                }
                format!(
                    "{} {}: {} ({}:{})",
                    self.kind,
                    self.path,
                    fields.join("  "),
                    self.file,
                    e.line
                )
            }
            (None, false) => format!("{} {}", self.kind, self.path),
        };
        if !self.written {
            out.push_str("; the file is as it was");
        }
        out
    }
}

// ---- arrange ----

/// Where the drawing puts one sheet's box.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArrangedSheet {
    pub name: String,
    pub own: bool,
    /// The box's top-left corner on the page, on the grid: what `at` holds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<[f64; 2]>,
    pub width: f64,
    pub height: f64,
    /// `file` when the file places the box, `arranged` when the drawing sets it out by its
    /// connections.
    pub source: &'static str,
    /// Why no `at` can be kept for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
}

/// What `schematic arrange` says.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Arranged {
    pub board: String,
    pub file: String,
    /// Whether the arrangement is of the sheets with the file's `at` entries dropped.
    pub reset: bool,
    pub sheets: Vec<ArrangedSheet>,
    /// Whether the file is different now.
    pub written: bool,
}

/// The place of a box on the grid.
fn at_of(frame: &netsmith_view::geom::Rect) -> Steps {
    Steps::new(
        (frame.min.x / GRID_MM).round() as i32,
        (frame.min.y / GRID_MM).round() as i32,
    )
}

/// `arrange`: where the drawing sets each sheet's box, as the editor's overview sets them out
/// (the boxes most tied by nets one after the other, the ones the file places kept where they
/// are, issue #798, ADR 0139). With `reset` the file's `at` entries are dropped first, so the
/// arrangement is the one by connections alone. With `write` every sheet that can be placed
/// gets an `at` entry with the place it has, so the arrangement stays when the circuit changes;
/// the file's other entries and its comments are kept. Refused when the board is not drawn by
/// module (16 parts or fewer, or in one module): it has no boxes to set out.
pub fn arrange(
    built: &Built,
    libraries: &Libraries,
    file: &Path,
    name: &str,
    reset: bool,
    write: bool,
) -> Result<Arranged, String> {
    let old = open(file).map_err(|f| refusal(name, f))?;
    let board = built.design.name.clone();
    let mut hints = old.hints.clone();
    if reset && let Some(table) = hints.boards.get_mut(&board) {
        table.value.board.at = None;
        for sheet in table.value.sheets.values_mut() {
            sheet.value.at = None;
        }
    }
    let drawn = draw(built, libraries, &hints);
    let layout = drawn
        .layout
        .map_err(|e| format!("the board could not be drawn: {e}"))?;
    if layout.sheets.is_empty() {
        return Err(format!(
            "{board} is drawn as one sheet (a board of {} parts or fewer, or all in one module), so it has no sheets to arrange",
            netsmith_view::sheets::SHEET_THRESHOLD
        ));
    }
    let sheets = design_sheets(&built.design, &crate::module_groups(built));
    let entries = hints.entries(&board);
    let mut out = Vec::new();
    let mut keep: Vec<(Option<String>, Steps)> = Vec::new();
    for sheet in &layout.sheets {
        let boxes = layout
            .sheets
            .iter()
            .filter(|s| s.name == sheet.name)
            .count();
        let design = sheets.iter().find(|s| s.name == sheet.name);
        let (at, skipped) = match design {
            Some(_) if boxes == 1 => (Some(at_of(&sheet.frame)), None),
            _ => (
                None,
                Some(
                    "the sheet is drawn as several boxes, so it has no one place on the page"
                        .to_string(),
                ),
            ),
        };
        let placed = design.is_some_and(|d| has_place(&entries, &drawn.findings, d));
        if let (Some(d), Some(at)) = (design, at) {
            keep.push(((!d.own).then(|| d.name.clone()), at));
        }
        out.push(ArrangedSheet {
            name: sheet.name.clone(),
            own: design.is_some_and(|d| d.own),
            at: at.map(steps_mm),
            width: mm(sheet.frame.width()),
            height: mm(sheet.frame.height()),
            source: if placed { "file" } else { "arranged" },
            skipped,
        });
    }
    let mut written = false;
    if write {
        let mut new = hints.clone();
        for (table, at) in keep {
            sheet_hints_mut(&mut new, &board, &table).at = Some(Located::new(at));
        }
        let text = text_for(&old, &new)?;
        written = put(file, &old, &text)?;
    }
    Ok(Arranged {
        board,
        file: name.to_string(),
        reset,
        sheets: out,
        written,
    })
}

impl Arranged {
    pub fn text(&self) -> String {
        let mut out = format!(
            "board {}: the sheets as the drawing sets them{}\n",
            self.board,
            if self.reset {
                " (the file's places dropped)"
            } else {
                ""
            }
        );
        for s in &self.sheets {
            match (&s.at, &s.skipped) {
                (Some(at), _) => out.push_str(&format!(
                    "  {}: at {}, {} ({} x {} mm), {}\n",
                    s.name,
                    num(at[0]),
                    num(at[1]),
                    num(s.width),
                    num(s.height),
                    if s.source == "file" {
                        "placed by the file"
                    } else {
                        "set out by its connections"
                    }
                )),
                (None, why) => out.push_str(&format!(
                    "  {}: not kept ({})\n",
                    s.name,
                    why.as_deref().unwrap_or("")
                )),
            }
        }
        out
    }
}
