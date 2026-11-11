//! The schematic layout file: where a person moved a part, kept apart from the source
//! (ADR 0061).
//!
//! `<stem>.sch.toml` sits beside the top-level `.nsm` file, as `<stem>.pcb.toml` does (ADR 0032).
//! It is optional, never needed to build, written by the editor and editable by hand. It holds
//! **hints**: how far a part's block was moved from where the automatic drawing puts it, a
//! turn or a mirror, a place a part is pinned to, how far a note was moved. It never holds a
//! wire, a label or a net, so nothing in it can change what is connected: the drawing is only
//! moved, never rewired.
//!
//! ```toml
//! version = 1
//!
//! [Trellis.parts.j1]                       # a part the board itself declares
//! dx = 2.54                                # mm, a multiple of 1.27
//! dy = -1.27
//!
//! [Trellis.sheets.stage.parts."output_cap"] # a part of the module instance `stage`
//! rotate = 90                              # counterclockwise on the page, as the library draws it at 0
//! mirror = true                            # left to right, before the turn
//!
//! [Trellis.sheets.stage.parts."filter.l1"] # `stage.filter.l1`, named from the sheet
//! pin = [12.7, 0]                          # from the sheet's reference part, whatever else changes
//!
//! [Trellis.sheets.stage.notes."output_cap#doc"]  # the note on `stage.output_cap`
//! dx = 5.08
//! dy = 0
//! ```
//!
//! This module only reads, writes and matches the file. [`parse`] reads the text (every error
//! has its line), [`to_toml`] writes it deterministically, and [`LayoutHints::resolve`] puts
//! the entries on a design: the entry of each part and note that is still there, and a
//! [`HintFinding`] for each entry that is not (a stale path, a sheet that is gone). An entry
//! is never an error once the file is read; at worst it is a finding and is ignored. The
//! drawing engine reads the [`ResolvedHints`] (`layout_sheets_hinted`, ADR 0061 stage 2) and
//! raises a finding of its own for an entry it cannot honour ([`HintFinding::unusable`]).
//!
//! **Why a file of its own.** The schematic is a pure function of the design. That is right for
//! a first drawing and wrong for a drawing someone has worked on: a person who moves a connector
//! to the left edge, or turns a crystal, loses the move at the next edit, because the layout
//! engine decides again. A move is not circuit and not a requirement: no check consumes it (the
//! test of ADR 0048) and it says nothing about where the part goes on the board. It is the
//! editor's own state about its own drawing, as the PCB's solution is (ADR 0032), so it has a
//! home of its own. It follows ADR 0003 (the language stays text; a position is not circuit) and
//! DESIGN-RULES (statement order never matters). The file is `.sch.toml` and not the
//! `.layout.toml` that ADR 0019 first sketched: the glossary's _Layout file_ is the PCB's, and
//! a file called `layout` that is not it would be read as it (atopile has the same pair,
//! `.pcb.hl` and `.sch.hl`).
//!
//! **The file.** One file for all the boards of a source file, one table for each board, keyed by
//! the board's definition name. A module has no file of its own: it is drawn as a sheet of a
//! board, and its entries are in that board's table, so a module used by two boards is arranged
//! twice, as it is drawn twice. `version = 1` is required in a file that has anything else; any
//! other version is an error. A comment in a file the editor rewrites is not kept (the PCB
//! file's known limit).
//!
//! **Keyed by instance path, grouped by sheet.** A part is keyed by its path in the design
//! (`Part::path`, as ADR 0032), never by reference designator, which a renumber changes. The
//! entries are grouped by the sheet the part is drawn on (a module instance of the board, named
//! as `sheets::Group::name` names it, or the board's own sheet), and inside a sheet the
//! part is keyed by its path from the sheet: `filter.l1` for `stage.filter.l1` on the sheet
//! `stage`. So a module that is renamed keeps its entries and the edit is one table header. A
//! sheet cut into numbered boxes does not say which box: the box is a fact of the drawing. A
//! sheet is the same whether or not the board is drawn split, so an entry does not change
//! meaning when the board grows past 16 parts. A note is keyed `<path>#doc` or `<path>#relation`
//! (`#relation2`, `#relation3` for the next ones of that path, in the order the design lists
//! them); the path of a note on a module itself is empty (`"#relation"`).
//!
//! **A value is an offset of the block, on the 1.27 mm grid.** `dx` and `dy` (millimetres, x to
//! the right, y down) move the part's **block**, the part with the satellites that serve it, from
//! where the automatic drawing puts it; the block moves as one, and the engine routes the wires
//! after the move. A satellite also has an entry of its own to move it within its block. The
//! value is a multiple of 1.27 mm (the editor snaps; a hand-written value off the grid is an
//! error that names the nearest two), and the largest is 5080 mm, which is a typo and not a
//! drawing. `rotate` (0, 90, 180 or 270 degrees counterclockwise, `-90` and `360` read as 270
//! and 0) **replaces** the automatic turn; `mirror` **replaces** the automatic mirror (the
//! symbol is flipped left to right first, then turned: the eight orientations the engine has).
//! `pin = [x, y]` instead of `dx` and `dy` pins the block's origin to a place from the sheet's
//! reference part, for a part someone wants fixed (the connector at the left edge) while
//! everything else is arranged round it; `pin` with `dx` or `dy` is an error. The frame is the
//! sheet's anchor-relative one (ADR 0030's stable layout): the reference part is at the origin,
//! so an edit that makes the sheet grow upward or leftward moves no pinned part and no offset.
//! No entry ever holds a coordinate in the absolute page frame, which moves whenever a
//! sheet's neighbour changes size.
//!
//! **Staleness is a finding, not an error.** ADR 0032 makes an entry for a part the board lacks
//! an error, because a PCB layout that disagrees with its board is wrong to fab. A schematic hint
//! is a convenience: a refactor that renames a part must not turn the view into an error. An
//! entry whose path is not in the design on that sheet, a sheet that is not a sheet of the
//! board, a table for a board the source does not have, and an entry the drawing cannot honour
//! are findings: named with the file's line and ignored. What the file says wrong (bad TOML, an
//! unknown key, a value off the grid, `rotate` not a multiple of 90, `pin` with `dx`, a key that
//! is not a path) is still an error with its line, and all errors of a file are returned at
//! once, in file order. The difference: an error is what the file says; a finding is what no
//! longer matches the design.
//!
//! **It changes nothing but the picture.** The drawing is a pure function of the design and the
//! hints, and the hints are an input and never ambient state (a file read inside the engine
//! would make a drawing depend on the disk). Empty hints give byte for byte the drawing there is
//! without them. The file holds no wire, label, bus or net, and the engine routes the wires
//! again after a move, so `netsmith_view::check`'s invariants hold for any hints: a hint can make
//! a drawing ugly and cannot make it wrong. [`to_toml`] writes deterministically (`version`,
//! boards in name order, in a board its own sheet and then the sheets in name order, in a sheet
//! the parts and then the notes, by key in byte order, numbers spelled `2.54`, `-1.27`, `0`), so
//! `parse(to_toml(h)) == h` and a hand edit that parses and is written back changes only
//! formatting. An entry with no field is kept; a table with no entry is not written.
//!
//! **Rejected.**
//! - Absolute page coordinates: the page's frame moves when any sheet changes size.
//! - Keying by reference designator: `R5` is `R6` after an insert.
//! - Keying by full path with no sheet table (`[Trellis.parts."stage.filter.l1"]`): simpler, but
//!   the file reads as the drawing is read with the sheet grouping, and a module rename is one
//!   edit and not one per part. It costs one more level, and a part cannot be moved to the wrong
//!   sheet without a finding.
//! - One file for the PCB and the schematic (ADR 0030, open question 5): a regeneration of one
//!   would touch the other, they are edited by different views, and the schematic's frame has
//!   nothing to do with the board's.
//! - Inside the source (a `schematic { }` block: a position is not circuit), in the physical
//!   file (ADR 0048: it holds requirements, and a move is no requirement), in `netsmith.toml`
//!   (a project file, not a design's).
//! - Stale entries as errors (a rename would break the view), and stale entries silently
//!   dropped (a person who dragged a part and finds it back after a rename could not know why).
//! - Free floating-point offsets: parts and wires sit on the 1.27 mm grid, a hand-written 1.3
//!   would put a pin off it, and an exact integer step count makes the file compare and diff
//!   exactly.
//! - A `lock` flag as the PCB file has: the PCB's placer needs to know what to leave alone while
//!   it places the rest; here the engine is a drawing, and `pin` says it by place.
//! - Stored wire routes: wires are routed again after every move by design, a stored route would
//!   go stale at every edit and could join what the netlist does not.
//! - JSON or a custom syntax: TOML is what ADR 0032 uses, reads and diffs well, and the reader
//!   is already a dependency of the workspace (`toml`, with its spans for line numbers).
//!
//! **Cost and what is not built.** The file's name, keys, grid, frame, findings and the Alt
//! modifier that pins were left to review: accepted as built by the owner, 2026-11-11. Two
//! sidecars per source file now (`.pcb.toml`, `.sch.toml`), three with the physical file
//! (ADR 0048); each is optional. A rename of a part inside a module leaves entries with a
//! finding; the editor could rewrite the file in the same edit that renames, which is not built
//! (issue #19). A sheet is a top-level module instance of the board: a part of a module nested
//! in a module is keyed by its path from that instance, and if sheets ever become nested the key
//! of a part changes with them. A part with several units (ADR 0050) is moved as the box of its
//! primary unit and no unit has a key of its own (`u1#2` would be the shape; not built, not
//! designed, issue #19). The sheets' arrangement on the page (the grid of boxes of a big board) is
//! not hinted: only what is inside a sheet (not built, issue #19). Not built, at any stage: wires,
//! labels, buses or nets in the file; a per-unit key; a tool that renames entries with a part.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::ops::{Deref, Range};
use std::path::{Path, PathBuf};

use netsmith_core::{Design, NoteKind};
use toml::Spanned;
use toml::de::{DeInteger, DeTable, DeValue};

use crate::sheets::{Group, clusters, sheet_of_note};

/// The version of the file format this reads and writes.
pub const VERSION: i64 = 1;

/// The grid, in millimetres, every offset is a multiple of: the schematic's grid.
pub const GRID_MM: f64 = 1.27;

/// The name of a finding's stage in the editor's Problems list.
pub const FINDING_STAGE: &str = "layout";

/// The largest offset or pinned place, in grid steps (about 5 m): a bigger number is a typo.
const MAX_STEPS: i32 = 4000;

/// How far a written number may be from a multiple of the grid and still be one: the
/// writer's two decimals.
const GRID_TOLERANCE_MM: f64 = 0.0051;

/// The file that holds the schematic hints of a source file: `main.nsm` has `main.sch.toml`.
/// It is the PCB layout file's rule (`led.pcb.toml`): beside the top-level source file, named
/// by its stem, one file for all the boards the source file has.
pub fn schematic_hints_path(source: &Path) -> PathBuf {
    source.with_extension("sch.toml")
}

/// A place or a move in grid steps of [`GRID_MM`]: x to the right, y down, as on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord)]
pub struct Steps {
    pub x: i32,
    pub y: i32,
}

impl Steps {
    pub const ZERO: Steps = Steps { x: 0, y: 0 };

    pub fn new(x: i32, y: i32) -> Steps {
        Steps { x, y }
    }

    /// The steps in millimetres.
    pub fn mm(self) -> (f64, f64) {
        (f64::from(self.x) * GRID_MM, f64::from(self.y) * GRID_MM)
    }
}

/// Where a part's block goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Moved by this much from where the automatic drawing puts it (`dx`, `dy`).
    Offset(Steps),
    /// At this place from the sheet's reference part, whatever the automatic drawing does
    /// (`pin`).
    Pinned(Steps),
}

/// What the file says about one part: its block, the part with the satellites that serve it,
/// is moved, turned or mirrored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PartHint {
    pub place: Option<Place>,
    /// 0, 90, 180 or 270 degrees counterclockwise on the page, from the symbol as the library
    /// draws it. It replaces the automatic turn.
    pub rotate: Option<u16>,
    /// Flipped left to right before the turn. It replaces the automatic mirror.
    pub mirror: Option<bool>,
}

/// What the file says about one note: how far the framed text was moved from where the
/// drawing puts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoteHint {
    pub by: Steps,
}

/// A value and the line of the file it was read from (0 for one that was not read from a
/// file). Two are equal when their values are: a line is where something was written, not
/// what it says.
#[derive(Debug, Clone, Copy, Eq)]
pub struct Located<T> {
    pub value: T,
    pub line: u32,
}

impl<T> Located<T> {
    pub fn new(value: T) -> Self {
        Located { value, line: 0 }
    }
}

impl<T: PartialEq> PartialEq for Located<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T> Deref for Located<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T: Default> Default for Located<T> {
    fn default() -> Self {
        Located::new(T::default())
    }
}

/// The entries of one sheet. A part is keyed by its path from the sheet (`filter.l1` for
/// `stage.filter.l1` on the sheet `stage`; the path itself for a part of the board), so a
/// module that is renamed or moved keeps its entries, which are about it and not about where
/// it is. A note is keyed by `<path>#doc` or `<path>#relation` (`#relation2` for the second
/// relation of one path), the path being given the same way.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SheetHints {
    pub parts: BTreeMap<String, Located<PartHint>>,
    pub notes: BTreeMap<String, Located<NoteHint>>,
}

impl SheetHints {
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty() && self.notes.is_empty()
    }

    fn len(&self) -> usize {
        self.parts.len() + self.notes.len()
    }
}

/// The entries of one board: the sheet of the parts the board declares itself, and the sheet
/// of each module instance, by its name (`stage`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BoardHints {
    /// The parts and notes of the board's own sheet.
    pub board: SheetHints,
    pub sheets: BTreeMap<String, Located<SheetHints>>,
}

impl BoardHints {
    pub fn is_empty(&self) -> bool {
        self.board.is_empty() && self.sheets.values().all(|s| s.is_empty())
    }
}

/// The whole file: one table for each board, by the board's definition name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LayoutHints {
    pub boards: BTreeMap<String, Located<BoardHints>>,
}

impl LayoutHints {
    pub fn is_empty(&self) -> bool {
        self.boards.values().all(|b| b.is_empty())
    }
}

/// A fault in the text of the file, with the line it is on (1 for the first).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintError {
    pub line: u32,
    pub message: String,
}

impl fmt::Display for HintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for HintError {}

// ---- reading ----

/// Reads the text of a schematic layout file. An empty file, or one of comments, has no hints.
/// Every fault found is returned, in the order of the file; nothing is left half read.
pub fn parse(text: &str) -> Result<LayoutHints, Vec<HintError>> {
    let table = match DeTable::parse(text) {
        Ok(t) => t,
        Err(e) => {
            let at = e.span().map_or(0, |s| s.start);
            return Err(vec![HintError {
                line: line_at(text, at),
                message: e.message().to_string(),
            }]);
        }
    };
    let mut reader = Reader {
        text,
        errors: Vec::new(),
    };
    let hints = reader.document(table.get_ref());
    if reader.errors.is_empty() {
        Ok(hints)
    } else {
        let mut errors = reader.errors;
        errors.sort_by(|a, b| (a.line, &a.message).cmp(&(b.line, &b.message)));
        Err(errors)
    }
}

/// The 1-based line a byte offset of `text` is on; one past the end is the last line.
fn line_at(text: &str, at: usize) -> u32 {
    let at = at.min(text.len());
    let breaks = text.as_bytes()[..at]
        .iter()
        .filter(|&&b| b == b'\n')
        .count();
    u32::try_from(breaks + 1).unwrap_or(u32::MAX)
}

struct Reader<'a> {
    text: &'a str,
    errors: Vec<HintError>,
}

impl Reader<'_> {
    fn line(&self, span: Range<usize>) -> u32 {
        line_at(self.text, span.start)
    }

    fn fail(&mut self, span: Range<usize>, message: String) {
        let line = self.line(span);
        self.errors.push(HintError { line, message });
    }

    fn document(&mut self, table: &DeTable<'_>) -> LayoutHints {
        let mut hints = LayoutHints::default();
        let mut versioned = false;
        let mut first_board: Option<Range<usize>> = None;
        for (key, value) in table.iter() {
            let name = key.get_ref().as_ref();
            if name == "version" {
                versioned = true;
                match value.get_ref() {
                    DeValue::Integer(i) if int_of(i) == Some(VERSION) => {}
                    other => {
                        let found = describe(other);
                        self.fail(
                            value.span(),
                            format!("`version` is {found}, and this reads only version {VERSION}"),
                        );
                    }
                }
                continue;
            }
            first_board.get_or_insert(key.span());
            let at = vec![name.to_string()];
            let Some(board) = self.table(value, &at) else {
                continue;
            };
            let parsed = self.board(board, &at);
            if !parsed.is_empty() {
                let line = self.line(key.span());
                hints.boards.insert(
                    name.to_string(),
                    Located {
                        value: parsed,
                        line,
                    },
                );
            }
        }
        if !versioned && let Some(span) = first_board {
            self.fail(span, "it has no `version`; write `version = 1`".into());
        }
        hints
    }

    /// The table a value is, or a fault naming `at`.
    fn table<'t, 'i>(
        &mut self,
        value: &'t Spanned<DeValue<'i>>,
        at: &[String],
    ) -> Option<&'t DeTable<'i>> {
        match value.get_ref() {
            DeValue::Table(t) => Some(t),
            other => {
                let found = describe(other);
                self.fail(
                    value.span(),
                    format!("{} must be a table, not {found}", header(at)),
                );
                None
            }
        }
    }

    fn board(&mut self, table: &DeTable<'_>, at: &[String]) -> BoardHints {
        let mut board = BoardHints::default();
        for (key, value) in table.iter() {
            let name = key.get_ref().as_ref();
            let mut here = at.to_vec();
            here.push(name.to_string());
            match name {
                "parts" => {
                    if let Some(t) = self.table(value, &here) {
                        self.parts(t, &here, &mut board.board);
                    }
                }
                "notes" => {
                    if let Some(t) = self.table(value, &here) {
                        self.notes(t, &here, &mut board.board);
                    }
                }
                "sheets" => {
                    let Some(sheets) = self.table(value, &here) else {
                        continue;
                    };
                    for (skey, svalue) in sheets.iter() {
                        let sname = skey.get_ref().as_ref();
                        let mut sat = here.clone();
                        sat.push(sname.to_string());
                        if sname.is_empty() || sname.contains('.') {
                            self.fail(
                                skey.span(),
                                format!(
                                    "{}: a sheet is a module instance of the board, a name with no dot",
                                    header(&sat)
                                ),
                            );
                            continue;
                        }
                        let Some(st) = self.table(svalue, &sat) else {
                            continue;
                        };
                        let sheet = self.sheet(st, &sat);
                        if !sheet.is_empty() {
                            let line = self.line(skey.span());
                            board
                                .sheets
                                .insert(sname.to_string(), Located { value: sheet, line });
                        }
                    }
                }
                _ => self.unknown(key.span(), &here, "parts, notes, sheets"),
            }
        }
        board
    }

    fn sheet(&mut self, table: &DeTable<'_>, at: &[String]) -> SheetHints {
        let mut sheet = SheetHints::default();
        for (key, value) in table.iter() {
            let name = key.get_ref().as_ref();
            let mut here = at.to_vec();
            here.push(name.to_string());
            match name {
                "parts" => {
                    if let Some(t) = self.table(value, &here) {
                        self.parts(t, &here, &mut sheet);
                    }
                }
                "notes" => {
                    if let Some(t) = self.table(value, &here) {
                        self.notes(t, &here, &mut sheet);
                    }
                }
                _ => self.unknown(key.span(), &here, "parts, notes"),
            }
        }
        sheet
    }

    fn unknown(&mut self, span: Range<usize>, at: &[String], known: &str) {
        let name = at.last().cloned().unwrap_or_default();
        self.fail(
            span,
            format!(
                "unknown key `{name}` in {} (it has: {known})",
                header(&at[..at.len() - 1])
            ),
        );
    }

    fn parts(&mut self, table: &DeTable<'_>, at: &[String], sheet: &mut SheetHints) {
        for (key, value) in table.iter() {
            let path = key.get_ref().to_string();
            let mut here = at.to_vec();
            here.push(path.clone());
            if path.is_empty() || path.split('.').any(str::is_empty) {
                self.fail(
                    key.span(),
                    format!(
                        "{}: a part is keyed by its path (`filter.l1`): names joined by single dots",
                        header(&here)
                    ),
                );
                continue;
            }
            let Some(t) = self.table(value, &here) else {
                continue;
            };
            if let Some(hint) = self.part_entry(t, &here) {
                let line = self.line(key.span());
                sheet.parts.insert(path, Located { value: hint, line });
            }
        }
    }

    fn notes(&mut self, table: &DeTable<'_>, at: &[String], sheet: &mut SheetHints) {
        for (key, value) in table.iter() {
            let name = key.get_ref().to_string();
            let mut here = at.to_vec();
            here.push(name.clone());
            if !is_note_key(&name) {
                self.fail(
                    key.span(),
                    format!(
                        "{}: a note is keyed `<path>#doc` or `<path>#relation` (`#relation2` for the second relation at one path)",
                        header(&here)
                    ),
                );
                continue;
            }
            let Some(t) = self.table(value, &here) else {
                continue;
            };
            if let Some(hint) = self.note_entry(t, &here) {
                let line = self.line(key.span());
                sheet.notes.insert(name, Located { value: hint, line });
            }
        }
    }

    fn part_entry(&mut self, table: &DeTable<'_>, at: &[String]) -> Option<PartHint> {
        let mut ok = true;
        let (mut dx, mut dy, mut pin): (Option<i32>, Option<i32>, Option<Steps>) =
            (None, None, None);
        let mut hint = PartHint::default();
        let mut pin_span = None;
        for (key, value) in table.iter() {
            let name = key.get_ref().as_ref();
            match name {
                "dx" | "dy" => match self.steps(value, at, name) {
                    Some(s) => {
                        if name == "dx" {
                            dx = Some(s);
                        } else {
                            dy = Some(s);
                        }
                    }
                    None => ok = false,
                },
                "pin" => match self.pair(value, at, name) {
                    Some(p) => {
                        pin_span = Some(key.span());
                        pin = Some(p);
                    }
                    None => ok = false,
                },
                "rotate" => match self.rotate(value, at) {
                    Some(r) => hint.rotate = Some(r),
                    None => ok = false,
                },
                "mirror" => match value.get_ref() {
                    DeValue::Boolean(b) => hint.mirror = Some(*b),
                    other => {
                        let found = describe(other);
                        self.fail(
                            value.span(),
                            format!(
                                "{}: `mirror` is {found}, and must be true or false",
                                header(at)
                            ),
                        );
                        ok = false;
                    }
                },
                _ => {
                    self.fail(
                        key.span(),
                        format!(
                            "unknown key `{name}` in {} (a part has: dx, dy, pin, rotate, mirror)",
                            header(at)
                        ),
                    );
                    ok = false;
                }
            }
        }
        match (pin, dx.is_some() || dy.is_some()) {
            (Some(_), true) => {
                self.fail(
                    pin_span.unwrap_or(0..0),
                    format!(
                        "{}: `pin` (a place from the sheet's reference part) and `dx`, `dy` (a move from the automatic place) cannot be given together",
                        header(at)
                    ),
                );
                ok = false;
            }
            (Some(p), false) => hint.place = Some(Place::Pinned(p)),
            (None, true) => {
                hint.place = Some(Place::Offset(Steps::new(
                    dx.unwrap_or_default(),
                    dy.unwrap_or_default(),
                )));
            }
            (None, false) => {}
        }
        ok.then_some(hint)
    }

    fn note_entry(&mut self, table: &DeTable<'_>, at: &[String]) -> Option<NoteHint> {
        let mut ok = true;
        let mut by = Steps::ZERO;
        for (key, value) in table.iter() {
            let name = key.get_ref().as_ref();
            match name {
                "dx" | "dy" => match self.steps(value, at, name) {
                    Some(s) if name == "dx" => by.x = s,
                    Some(s) => by.y = s,
                    None => ok = false,
                },
                _ => {
                    self.fail(
                        key.span(),
                        format!(
                            "unknown key `{name}` in {} (a note has: dx, dy)",
                            header(at)
                        ),
                    );
                    ok = false;
                }
            }
        }
        ok.then_some(NoteHint { by })
    }

    /// A number of millimetres on the grid, as steps.
    fn steps(&mut self, value: &Spanned<DeValue<'_>>, at: &[String], name: &str) -> Option<i32> {
        let Some((v, raw)) = number(value.get_ref()) else {
            let found = describe(value.get_ref());
            self.fail(
                value.span(),
                format!(
                    "{}: `{name}` is {found}, and must be a number of millimetres",
                    header(at)
                ),
            );
            return None;
        };
        match to_steps(v) {
            Ok(s) => Some(s),
            Err(why) => {
                self.fail(
                    value.span(),
                    format!("{}: `{name}` = {raw} {why}", header(at)),
                );
                None
            }
        }
    }

    fn pair(&mut self, value: &Spanned<DeValue<'_>>, at: &[String], name: &str) -> Option<Steps> {
        let items = match value.get_ref() {
            DeValue::Array(a) if a.len() == 2 => a,
            other => {
                let found = describe(other);
                self.fail(
                    value.span(),
                    format!(
                        "{}: `{name}` is {found}, and must be two numbers of millimetres, `[x, y]`",
                        header(at)
                    ),
                );
                return None;
            }
        };
        let (x, y) = (&items[0], &items[1]);
        let x = self.steps(x, at, name);
        let y = self.steps(y, at, name);
        Some(Steps::new(x?, y?))
    }

    fn rotate(&mut self, value: &Spanned<DeValue<'_>>, at: &[String]) -> Option<u16> {
        let degrees = number(value.get_ref())
            .map(|(v, _)| v)
            .filter(|v| v.fract() == 0.0 && v % 90.0 == 0.0 && v.abs() < 1e6);
        let Some(v) = degrees else {
            let found = describe(value.get_ref());
            self.fail(
                value.span(),
                format!(
                    "{}: `rotate` is {found}, and must be a multiple of 90 degrees (0, 90, 180 or 270)",
                    header(at)
                ),
            );
            return None;
        };
        Some((v as i64).rem_euclid(360) as u16)
    }
}

fn int_of(i: &DeInteger<'_>) -> Option<i64> {
    i64::from_str_radix(i.as_str(), i.radix()).ok()
}

/// A number the file wrote, and as it wrote it.
fn number(v: &DeValue<'_>) -> Option<(f64, String)> {
    match v {
        DeValue::Integer(i) => int_of(i).map(|n| (n as f64, i.as_str().to_string())),
        DeValue::Float(f) => f
            .as_str()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .map(|n| (n, f.as_str().to_string())),
        _ => None,
    }
}

/// A value as a message names it.
fn describe(v: &DeValue<'_>) -> String {
    match v {
        DeValue::String(s) => format!("{s:?}"),
        DeValue::Integer(i) => i.as_str().to_string(),
        DeValue::Float(f) => f.as_str().to_string(),
        DeValue::Boolean(b) => b.to_string(),
        DeValue::Datetime(d) => d.to_string(),
        DeValue::Array(_) => "an array".into(),
        DeValue::Table(_) => "a table".into(),
    }
}

/// Millimetres to grid steps, or why the number is not a place on the grid.
fn to_steps(v: f64) -> Result<i32, String> {
    let n = (v / GRID_MM).round();
    if (v - n * GRID_MM).abs() > GRID_TOLERANCE_MM {
        let below = (v / GRID_MM).floor() as i32;
        return Err(format!(
            "is not on the {GRID_MM} mm grid (the nearest places are {} and {})",
            mm_text(below),
            mm_text(below + 1)
        ));
    }
    if n.abs() > f64::from(MAX_STEPS) {
        return Err(format!(
            "is further than {} mm from where it belongs",
            mm_text(MAX_STEPS)
        ));
    }
    Ok(n as i32)
}

/// Whether `key` is a note's key as written: `<path>#doc`, `<path>#relation` or
/// `<path>#relation<N>` (N from 2), the path from the sheet (empty for the sheet's own).
fn is_note_key(key: &str) -> bool {
    let Some((path, tail)) = key.rsplit_once('#') else {
        return false;
    };
    if !path.is_empty() && path.split('.').any(str::is_empty) {
        return false;
    }
    if tail == "doc" || tail == "relation" {
        return true;
    }
    tail.strip_prefix("relation").is_some_and(|digits| {
        !digits.starts_with('0')
            && digits.bytes().all(|b| b.is_ascii_digit())
            && digits.parse::<usize>().is_ok_and(|n| n >= 2)
    })
}

fn note_key(path: &str, kind: NoteKind, n: usize) -> String {
    let word = match kind {
        NoteKind::Doc => "doc",
        NoteKind::Relation => "relation",
    };
    if n <= 1 {
        format!("{path}#{word}")
    } else {
        format!("{path}#{word}{n}")
    }
}

// ---- writing ----

/// A key as TOML writes it: bare when it can be, else quoted.
fn quote_key(key: &str) -> String {
    let bare = !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if bare {
        return key.to_string();
    }
    let mut out = String::from("\"");
    for c in key.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A table's header as a message names it: `[Trellis.sheets.stage]`.
fn header(at: &[String]) -> String {
    let keys: Vec<String> = at.iter().map(|k| quote_key(k)).collect();
    format!("[{}]", keys.join("."))
}

/// Grid steps in millimetres with at most two decimals and no trailing zeros: `2.54`, `-1.27`,
/// `0`. A multiple of 1.27 has at most two decimals, so nothing is rounded.
fn mm_text(steps: i32) -> String {
    let hundredths = i64::from(steps) * 127;
    let sign = if hundredths < 0 { "-" } else { "" };
    let (whole, frac) = (hundredths.abs() / 100, hundredths.abs() % 100);
    if frac == 0 {
        format!("{sign}{whole}")
    } else if frac % 10 == 0 {
        format!("{sign}{whole}.{}", frac / 10)
    } else {
        format!("{sign}{whole}.{frac:02}")
    }
}

/// The text of the file for `hints`: `version`, then each board in name order; in a board its
/// own sheet, then the sheets in name order; in a sheet the parts, then the notes, in the order
/// of their keys. The same hints always give the same text, and [`parse`] reads it back to the
/// same hints. A comment in a file that was edited by hand is not kept.
pub fn to_toml(hints: &LayoutHints) -> String {
    let mut out = format!("version = {VERSION}\n");
    for (name, board) in &hints.boards {
        if board.is_empty() {
            continue;
        }
        let at = vec![quote_key(name)];
        write_sheet(&mut out, &at, &board.board);
        for (sheet, entries) in &board.sheets {
            let mut here = at.clone();
            here.push("sheets".into());
            here.push(quote_key(sheet));
            write_sheet(&mut out, &here, entries);
        }
    }
    out
}

fn write_sheet(out: &mut String, at: &[String], sheet: &SheetHints) {
    let prefix = at.join(".");
    for (path, hint) in &sheet.parts {
        out.push_str(&format!("\n[{prefix}.parts.{}]\n", quote_key(path)));
        match hint.place {
            Some(Place::Offset(s)) => {
                out.push_str(&format!("dx = {}\ndy = {}\n", mm_text(s.x), mm_text(s.y)));
            }
            Some(Place::Pinned(s)) => {
                out.push_str(&format!("pin = [{}, {}]\n", mm_text(s.x), mm_text(s.y)));
            }
            None => {}
        }
        if let Some(r) = hint.rotate {
            out.push_str(&format!("rotate = {r}\n"));
        }
        if let Some(m) = hint.mirror {
            out.push_str(&format!("mirror = {m}\n"));
        }
    }
    for (key, hint) in &sheet.notes {
        out.push_str(&format!("\n[{prefix}.notes.{}]\n", quote_key(key)));
        out.push_str(&format!(
            "dx = {}\ndy = {}\n",
            mm_text(hint.by.x),
            mm_text(hint.by.y)
        ));
    }
}

// ---- matching the file with a design ----

/// What kind of entry a finding is about, and why it is not used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingKind {
    /// The sheet is not a sheet of the board (no such module instance, or it has no part).
    StaleSheet,
    /// The sheet has no such part or note.
    StaleEntry,
    /// The part or note is there, but on another sheet.
    WrongSheet,
    /// The file has a table for a board the source does not have.
    UnknownBoard,
    /// The entry cannot be honoured (it would put the block on another part's body). Raised by
    /// the drawing, which knows the geometry.
    Unusable,
}

/// An entry that is not used, with the line of the file it is on. It is a note in the editor's
/// Problems list, stage [`FINDING_STAGE`]: the drawing is made without the entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintFinding {
    pub kind: FindingKind,
    /// The line of the entry's table header in the file; 0 if it has no line.
    pub line: u32,
    /// The entry as the file names it: `[Trellis.sheets.stage.parts."output_cap"]`.
    pub entry: String,
    /// What is wrong with it, ending with what happens to it.
    pub message: String,
}

impl HintFinding {
    /// A finding about an entry the drawing could not honour.
    pub fn unusable(entry: String, line: u32, why: &str) -> HintFinding {
        HintFinding {
            kind: FindingKind::Unusable,
            line,
            message: format!("{entry}: {why}; it is ignored"),
            entry,
        }
    }

    /// The finding as a line of text, in the shape of a compiler's: `main.sch.toml:12: ...`.
    pub fn render(&self, file: &str) -> String {
        if self.line == 0 {
            format!("{file}: {}", self.message)
        } else {
            format!("{file}:{}: {}", self.line, self.message)
        }
    }
}

/// A part's hint, with where it is in the design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPart {
    /// The sheet the part is drawn on: the module instance (`stage`), or the board's name for
    /// the parts it declares itself.
    pub sheet: String,
    /// The part's path in the design (`stage.filter.l1`).
    pub path: String,
    pub hint: PartHint,
    /// The entry's line in the file.
    pub line: u32,
    /// The entry as the file names it (`[Trellis.sheets.stage.parts."output_cap"]`), for a
    /// finding the drawing raises about it.
    pub entry: String,
}

/// A note's hint, with where it is in the design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedNote {
    pub sheet: String,
    /// The note's key as the file writes it.
    pub key: String,
    pub hint: NoteHint,
    pub line: u32,
    /// The entry as the file names it, for a finding the drawing raises about it.
    pub entry: String,
}

/// The entries of a file that match a design, by the index of the part in `design.parts` and
/// of the note in `design.notes`. The drawing reads this and never the file. An empty one
/// (`ResolvedHints::default()`) draws what the drawing draws with no file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedHints {
    pub parts: BTreeMap<usize, ResolvedPart>,
    pub notes: BTreeMap<usize, ResolvedNote>,
}

impl ResolvedHints {
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty() && self.notes.is_empty()
    }
}

/// A part's path from its sheet: the key the file gives the part. The path itself on the
/// board's own sheet, and for a part of a module instance what follows the instance's name
/// (`filter.l1` for `stage.filter.l1` on `stage`; empty for the instance itself, which a note
/// can be on).
pub fn sheet_key(sheet: &str, board_sheet: bool, path: &str) -> String {
    if board_sheet {
        return path.to_string();
    }
    if path == sheet {
        return String::new();
    }
    path.strip_prefix(sheet)
        .and_then(|rest| rest.strip_prefix('.'))
        .unwrap_or(path)
        .to_string()
}

impl LayoutHints {
    /// The entries for the board `design` is, and a finding for each that is not used.
    ///
    /// The sheets are the ones [`crate::layout_sheets`] draws for `groups`: a group of parts
    /// is a sheet named for the group, and the parts in no group are the board's own sheet,
    /// whether the drawing is cut into boxes or not. A part with no path (a design that does
    /// not know its paths) has no entry. A part with no entry, and a design with no table in
    /// the file, are drawn automatically.
    pub fn resolve(&self, design: &Design, groups: &[Group]) -> (ResolvedHints, Vec<HintFinding>) {
        let mut resolved = ResolvedHints::default();
        let mut findings = Vec::new();
        let Some(table) = self.boards.get(&design.name) else {
            return (resolved, findings);
        };
        let board = &table.value;
        if board.is_empty() {
            return (resolved, findings);
        }

        let sheets = Sheets::of(design, groups);
        let at = vec![design.name.clone()];

        self.match_sheet(
            &sheets,
            &SheetRef {
                cluster: sheets.board_cluster(),
                at: at.clone(),
            },
            &board.board,
            &mut resolved,
            &mut findings,
        );
        for (name, entries) in &board.sheets {
            let mut here = at.clone();
            here.push("sheets".into());
            here.push(name.clone());
            let cluster = sheets.module_cluster(name);
            match cluster {
                Some(_) => self.match_sheet(
                    &sheets,
                    &SheetRef { cluster, at: here },
                    entries,
                    &mut resolved,
                    &mut findings,
                ),
                None if entries.is_empty() => {}
                None => {
                    let have = sheets.module_names();
                    let list = if have.is_empty() {
                        "it has none".to_string()
                    } else {
                        format!(
                            "its sheets: {}",
                            have.iter()
                                .map(|n| format!("`{n}`"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    let entry = header(&here);
                    let n = entries.len();
                    findings.push(HintFinding {
                        kind: FindingKind::StaleSheet,
                        line: entries.line,
                        message: format!(
                            "{entry}: the board has no sheet `{name}` ({list}); its {n} {} {} ignored",
                            if n == 1 { "entry" } else { "entries" },
                            if n == 1 { "is" } else { "are" },
                        ),
                        entry,
                    });
                }
            }
        }
        findings.sort_by(|a, b| (a.line, &a.message).cmp(&(b.line, &b.message)));
        (resolved, findings)
    }

    /// A finding for each table of the file that names a board the source does not have
    /// (`boards` are all the board names of the source file), so that a file is not silently
    /// ignored for a renamed board. A table with no entries is not a finding.
    pub fn unknown_boards(&self, boards: &[&str]) -> Vec<HintFinding> {
        let mut findings = Vec::new();
        for (name, table) in &self.boards {
            if boards.contains(&name.as_str()) || table.is_empty() {
                continue;
            }
            let entry = header(std::slice::from_ref(name));
            let list = if boards.is_empty() {
                "it has none".to_string()
            } else {
                format!(
                    "its boards: {}",
                    boards
                        .iter()
                        .map(|b| format!("`{b}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            findings.push(HintFinding {
                kind: FindingKind::UnknownBoard,
                line: table.line,
                message: format!(
                    "{entry}: the source has no board `{name}` ({list}); the table is ignored"
                ),
                entry,
            });
        }
        findings.sort_by_key(|f| f.line);
        findings
    }

    fn match_sheet(
        &self,
        sheets: &Sheets,
        sheet: &SheetRef,
        entries: &SheetHints,
        resolved: &mut ResolvedHints,
        findings: &mut Vec<HintFinding>,
    ) {
        let c = sheet.cluster;
        let name = |key: &str, what: &str| {
            let mut at = sheet.at.clone();
            at.push(what.to_string());
            at.push(key.to_string());
            header(&at)
        };
        for (key, hint) in &entries.parts {
            let entry = name(key, "parts");
            let found = c.and_then(|c| sheets.parts[c].get(key.as_str()).copied());
            match found {
                Some(p) => {
                    resolved.parts.insert(
                        p,
                        ResolvedPart {
                            sheet: sheets.title(c.expect("a part was found in a sheet")),
                            path: sheets.design_path(p),
                            hint: hint.value,
                            line: hint.line,
                            entry: entry.clone(),
                        },
                    );
                }
                None => findings.push(sheets.stale_part(&entry, hint.line, key, sheet)),
            }
        }
        for (key, hint) in &entries.notes {
            let entry = name(key, "notes");
            let found = c.and_then(|c| sheets.notes[c].get(key.as_str()).copied());
            match found {
                Some(n) => {
                    resolved.notes.insert(
                        n,
                        ResolvedNote {
                            sheet: sheets.title(c.expect("a note was found in a sheet")),
                            key: key.clone(),
                            hint: hint.value,
                            line: hint.line,
                            entry: entry.clone(),
                        },
                    );
                }
                None => findings.push(sheets.stale_note(&entry, hint.line, key)),
            }
        }
    }
}

struct SheetRef {
    /// The sheet of the design the table is for, if there is one.
    cluster: Option<usize>,
    /// The table's keys, for naming an entry.
    at: Vec<String>,
}

/// The design's sheets as the hints see them, with every part and note keyed as the file keys
/// it.
struct Sheets<'d> {
    design: &'d Design,
    /// The sheet of each part of the design.
    owner: Vec<usize>,
    /// Name and whether it is the board's own sheet, for each sheet.
    names: Vec<(String, bool)>,
    /// For each sheet, its parts by the file's key, as indices of `design.parts`.
    parts: Vec<HashMap<String, usize>>,
    /// For each sheet, its notes by the file's key, as indices of `design.notes`.
    notes: Vec<HashMap<String, usize>>,
}

impl<'d> Sheets<'d> {
    fn of(design: &'d Design, groups: &[Group]) -> Self {
        let clusters = clusters(design, groups);
        let grouped: HashSet<usize> = groups
            .iter()
            .flat_map(|g| g.parts.iter().copied())
            .filter(|&p| p < design.parts.len())
            .collect();
        let mut owner = vec![0; design.parts.len()];
        let mut names = Vec::new();
        for (c, cluster) in clusters.iter().enumerate() {
            for &p in &cluster.parts {
                owner[p] = c;
            }
            let own = cluster.parts.first().is_some_and(|p| !grouped.contains(p));
            names.push((cluster.name.clone(), own));
        }
        let mut parts: Vec<HashMap<String, usize>> = vec![HashMap::new(); clusters.len()];
        for (p, part) in design.parts.iter().enumerate() {
            if part.path.is_empty() {
                continue;
            }
            let (name, own) = &names[owner[p]];
            parts[owner[p]].insert(sheet_key(name, *own, &part.path), p);
        }
        let by_path: HashMap<&str, usize> = design
            .parts
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.path.is_empty())
            .map(|(i, p)| (p.path.as_str(), i))
            .collect();
        let mut notes: Vec<HashMap<String, usize>> = vec![HashMap::new(); clusters.len()];
        let mut count: BTreeMap<(&str, NoteKind), usize> = BTreeMap::new();
        for (i, note) in design.notes.iter().enumerate() {
            if names.is_empty() {
                break;
            }
            let c = sheet_of_note(design, note, &by_path, &owner);
            let n = count.entry((note.path.as_str(), note.kind)).or_insert(0);
            *n += 1;
            let (name, own) = &names[c];
            let path = sheet_key(name, *own, &note.path);
            notes[c].insert(note_key(&path, note.kind, *n), i);
        }
        Sheets {
            design,
            owner,
            names,
            parts,
            notes,
        }
    }

    fn board_cluster(&self) -> Option<usize> {
        self.names.iter().position(|(_, own)| *own)
    }

    fn module_cluster(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|(n, own)| !own && n == name)
    }

    fn module_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .names
            .iter()
            .filter(|(_, own)| !own)
            .map(|(n, _)| n.as_str())
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    fn title(&self, cluster: usize) -> String {
        self.names[cluster].0.clone()
    }

    fn design_path(&self, part: usize) -> String {
        self.design.parts[part].path.clone()
    }

    /// The finding for a part entry with no part at its key: stale, or in the wrong table when
    /// the part of that path is drawn on another sheet (a module's part under the board's own
    /// table, say), with where it belongs.
    fn stale_part(&self, entry: &str, line: u32, key: &str, sheet: &SheetRef) -> HintFinding {
        let own = sheet.at.len() == 1;
        let full = match sheet.at.last() {
            Some(name) if !own => format!("{name}.{key}"),
            _ => key.to_string(),
        };
        let elsewhere = self
            .design
            .parts
            .iter()
            .position(|p| !p.path.is_empty() && p.path == full)
            .map(|p| self.owner[p])
            .filter(|&c| Some(c) != sheet.cluster);
        let Some(c) = elsewhere else {
            return HintFinding {
                kind: FindingKind::StaleEntry,
                line,
                message: format!(
                    "{entry}: the design has no part `{key}` on this sheet; the entry is ignored"
                ),
                entry: entry.to_string(),
            };
        };
        let (name, c_own) = &self.names[c];
        let there = sheet_key(name, *c_own, &full);
        let mut at = vec![self.design.name.clone()];
        if !c_own {
            at.push("sheets".into());
            at.push(name.clone());
        }
        at.push("parts".into());
        at.push(there);
        HintFinding {
            kind: FindingKind::WrongSheet,
            line,
            message: format!(
                "{entry}: the part `{full}` is drawn on the sheet `{name}`, not on this one; its entry belongs in {}, and is ignored here",
                header(&at)
            ),
            entry: entry.to_string(),
        }
    }

    fn stale_note(&self, entry: &str, line: u32, key: &str) -> HintFinding {
        HintFinding {
            kind: FindingKind::StaleEntry,
            line,
            message: format!(
                "{entry}: the design has no note `{key}` on this sheet; the entry is ignored"
            ),
            entry: entry.to_string(),
        }
    }
}
