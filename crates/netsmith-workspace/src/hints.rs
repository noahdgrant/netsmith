//! The schematic layout file beside a top-level source file (ADR 0061): reading it for the
//! drawing, turning what is wrong with it into problems of stage `layout`, and the two routes
//! that read and write it.
//!
//! The file is an input of the drawing and never ambient state: the analysis reads it once for
//! each source file, matches it with each board and passes the result to the drawing. A file
//! that cannot be read is no reason for the drawing to fail: the drawing is then the one there
//! is without a file, and a problem says why.
//!
//! **Where the file is read: in the analysis.** The drawing is made by the analysis (the `draw`
//! step, once for each board), so that is where the file is read: once for each top-level source
//! file, matched with each of its boards (`LayoutHints::resolve`, with `module_groups`), and
//! passed to the drawing. A check of typed text reads the file too, so its problems list agrees
//! with the saved one.
//!
//! **What is wrong with the file is in the Problems list, stage `layout`**, at the file and line
//! (the table header's line, the range the whole line):
//! - an entry that matches nothing, and one the drawing cannot honour: a **note**, with the
//!   board's name in `board`. Notes show in the dock only once its toggle is on.
//! - a table for a board the source file does not have: a note with no `board`.
//! - a fault in the file's text, one problem for each, in the order of the file: a **warning**,
//!   not an error. The file as a whole is ignored and the drawing is the one there is without a
//!   file, byte for byte; the board builds, so nothing is an error and no view is dimmed as
//!   stale. A file that cannot be read at all (not a missing one) is the same warning with no
//!   line. A missing file is nothing.
//!
//! The layout findings are left out of a comparison with a git ref (ADR 0059): it is about the
//! circuit, and a layout note is about how it is drawn.
//!
//! **The file is watched with the sources** (`Target::watched` lists the `.sch.toml` beside each
//! top-level file that exists), so creating, editing and deleting it, in the editor or by another
//! program, analyses the project again once, as a change to the source does. There is no
//! separate bookkeeping for the editor's own writes, as the PCB layout file has: the file
//! changes the drawing, which the analysis makes, so a write of our own is one analysis and not
//! an echo. `crates/netsmith-workspace/tests/editor_hints.rs` counts the analyses that begin across a
//! `PUT`, three more rounds of reads, a change by another program and a delete: exactly one each.
//!
//! **The `PUT`** takes the **text** of the file as a `text/plain` body (as `PUT /api/file` does,
//! and unlike `POST /api/pcb/edit`, which takes changes) and an optional `X-Stamp`. It reads the
//! text with `parse` and writes `to_toml` of what it says, atomically, so the page may send any
//! text that reads (a person's comments and spelling are normalised) and the file on disk is
//! always the canonical one. It answers with the same body as `GET`, of the analysis that has
//! the new file. A text with no entry writes `version = 1` and not a deleted file: deleting is
//! the page's, or a person's, to do, and an empty file draws what no file draws. Two `PUT`s are
//! one after the other (the lock the PCB edit holds).
//!
//! **Cost, and what is not built.** A change to the file analyses the **whole project**, not only
//! the drawing: check, solve, design stage and draw run again, because the drawing is made inside
//! the analysis and a board's build (`Built`) is not kept apart from it. For a project whose
//! analysis takes seconds that is the delay of every drag. The fix is to redraw the boards from
//! the last build when only a `.sch.toml` changed (the drawing needs only the design, groups,
//! rails and marks the build holds, plus the hints); it needs `Built` to be shared (it is not
//! `Clone`) and a version bump with no analysis, as the PCB layout file has. Not built; issue #19. A board a system places twice has one table (its definition's name), and
//! each of the two members gets the findings: a problem shows once, as the dock does for any
//! problem of two boards with the same message and place.

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
};

use netsmith_editor::{
    ApiError,
    api::{
        Cause, HintBoard, HintFault, HintFindingInfo, HintFindingKind, HintNote, HintPart,
        HintSheet, Location, Problem, ProblemStage, SchematicHints, Severity,
    },
};
use netsmith_view::{
    FindingKind, HintError, HintFinding, LayoutHints, Place, SheetHints, hints,
    schematic_hints_path,
};

use super::{Workspace, analysis::BoardData, files};

/// What the analysis keeps of the layout file for each board of its source file: where it is,
/// the names of the boards the source file has, and the entries that are not used for this
/// board (the file's matching with the design and the drawing's own: a block that would lie
/// on another part).
pub struct Hints {
    /// The file on disk (it may not be there).
    pub path: PathBuf,
    /// The file's name in the project (`src/main.sch.toml`), as a problem's location names it.
    pub name: String,
    /// The boards of the source file, by definition name.
    pub boards: Vec<String>,
    pub findings: Vec<HintFinding>,
}

/// The file as the analysis found it.
pub(super) enum Read {
    /// There is no file: nothing is hinted and nothing is wrong.
    Missing,
    /// The file could not be read at all.
    Unreadable(String),
    /// The text does not read; every fault is here, in the order of the file.
    Invalid {
        text: String,
        errors: Vec<HintError>,
    },
    Hints(LayoutHints, String),
}

/// The name of the layout file of the source file `name` (`src/main.nsm`): `src/main.sch.toml`.
pub(super) fn name_of(source: &str) -> String {
    schematic_hints_path(Path::new(source))
        .to_string_lossy()
        .into_owned()
}

pub(super) fn read(path: &Path) -> Read {
    read_in(path, &HashMap::new())
}

/// [`read`], with the text a person has typed and not saved (`unsaved`, by the file's path) read
/// in place of the file, as a check of typed text reads a source.
pub(super) fn read_in(path: &Path, unsaved: &HashMap<PathBuf, String>) -> Read {
    let text = match unsaved.get(path) {
        Some(text) => Ok(text.clone()),
        None => fs::read_to_string(path),
    };
    match text {
        Ok(text) => match hints::parse(&text) {
            Ok(h) => Read::Hints(h, text),
            Err(errors) => Read::Invalid { text, errors },
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Read::Missing,
        Err(e) => Read::Unreadable(e.to_string()),
    }
}

impl Read {
    /// What the drawing is given of the file: nothing when it is missing or cannot be used.
    pub(super) fn hints(&self) -> Option<&LayoutHints> {
        match self {
            Read::Hints(h, _) => Some(h),
            _ => None,
        }
    }

    /// The problems that belong to the file as a whole and not to a board: a fault of its text
    /// (the file is ignored), and a table for a board the source file does not have. `boards`
    /// are all the boards of the source file.
    pub(super) fn problems(&self, name: &str, boards: &[&str]) -> Vec<Problem> {
        match self {
            Read::Missing => Vec::new(),
            Read::Unreadable(why) => vec![fault(name, 0, "", &format!("cannot be read: {why}"))],
            Read::Invalid { text, errors } => errors
                .iter()
                .map(|e| fault(name, e.line, text, &e.message))
                .collect(),
            Read::Hints(h, text) => h
                .unknown_boards(boards)
                .iter()
                .map(|f| finding_problem(f, name, text, None))
                .collect(),
        }
    }

    pub(super) fn text(&self) -> &str {
        match self {
            Read::Invalid { text, .. } | Read::Hints(_, text) => text,
            _ => "",
        }
    }
}

/// A fault in the file's text: a warning, because the drawing is whole without the file but
/// is not what the file asked for. It says the file is ignored.
fn fault(name: &str, line: u32, text: &str, message: &str) -> Problem {
    let message = format!("{message}; the file is ignored and the schematic is drawn without it");
    let (head, at) = if line == 0 {
        (format!("{name}:"), None)
    } else {
        (format!("{name}:{line}:"), Some(location(name, line, text)))
    };
    Problem {
        severity: Severity::Warning,
        rendered: format!("warning: {head} {message}\n"),
        message,
        at,
        stage: Some(ProblemStage::Layout),
        board: None,
        path: None,
        cause: None,
    }
}

/// An entry that is not used: a note, at its table's line in the file.
pub(super) fn finding_problem(
    finding: &HintFinding,
    name: &str,
    text: &str,
    board: Option<String>,
) -> Problem {
    Problem {
        severity: Severity::Note,
        message: finding.message.clone(),
        rendered: format!("note: {}\n", finding.render(name)),
        at: (finding.line > 0).then(|| location(name, finding.line, text)),
        stage: Some(ProblemStage::Layout),
        board,
        path: None,
        cause: Some(Cause::Other),
    }
}

/// The line of the file, from its first character to its last.
fn location(name: &str, line: u32, text: &str) -> Location {
    let width = text
        .lines()
        .nth(line.saturating_sub(1) as usize)
        .map_or(0, |l| l.chars().count());
    Location {
        file: name.to_string(),
        line,
        column: 1,
        end_line: line,
        end_column: width as u32 + 1,
    }
}

/// Hundredths of a millimetre are all the file writes.
fn mm(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn part_of(hint: &netsmith_view::Located<netsmith_view::PartHint>) -> HintPart {
    let (mut dx, mut dy, mut pin) = (None, None, None);
    match hint.place {
        Some(Place::Offset(by)) => {
            let (x, y) = by.mm();
            (dx, dy) = (Some(mm(x)), Some(mm(y)));
        }
        Some(Place::Pinned(at)) => {
            let (x, y) = at.mm();
            pin = Some((mm(x), mm(y)));
        }
        None => {}
    }
    HintPart {
        dx,
        dy,
        pin,
        rotate: hint.rotate,
        mirror: hint.mirror,
        line: hint.line,
    }
}

fn sheet_of(sheet: &SheetHints) -> HintSheet {
    let (parts, notes) = entries(sheet);
    HintSheet { parts, notes }
}

type Entries = (BTreeMap<String, HintPart>, BTreeMap<String, HintNote>);

fn entries(sheet: &SheetHints) -> Entries {
    (
        sheet
            .parts
            .iter()
            .map(|(key, part)| (key.clone(), part_of(part)))
            .collect(),
        sheet
            .notes
            .iter()
            .map(|(key, note)| {
                let (dx, dy) = note.by.mm();
                (
                    key.clone(),
                    HintNote {
                        dx: mm(dx),
                        dy: mm(dy),
                        line: note.line,
                    },
                )
            })
            .collect(),
    )
}

fn boards_of(hints: &LayoutHints) -> BTreeMap<String, HintBoard> {
    hints
        .boards
        .iter()
        .map(|(name, board)| {
            let (parts, notes) = entries(&board.board);
            let sheets = board
                .sheets
                .iter()
                .map(|(sheet, entries)| (sheet.clone(), sheet_of(entries)))
                .collect();
            (
                name.clone(),
                HintBoard {
                    parts,
                    notes,
                    sheets,
                },
            )
        })
        .collect()
}

fn kind_of(kind: FindingKind) -> HintFindingKind {
    match kind {
        FindingKind::StaleSheet => HintFindingKind::StaleSheet,
        FindingKind::StaleEntry => HintFindingKind::StaleEntry,
        FindingKind::WrongSheet => HintFindingKind::WrongSheet,
        FindingKind::UnknownBoard => HintFindingKind::UnknownBoard,
        FindingKind::Unusable => HintFindingKind::Unusable,
    }
}

fn info_of(finding: &HintFinding) -> HintFindingInfo {
    HintFindingInfo {
        kind: kind_of(finding.kind),
        line: finding.line,
        entry: finding.entry.clone(),
        message: finding.message.clone(),
    }
}

impl Workspace {
    /// The file beside the source of `board`, as the page sees it: its text, what it says and
    /// the entries that do not match. `version` is the analysis the findings are of.
    fn hints_answer(&self, version: u64, board: &str, data: &BoardData) -> SchematicHints {
        let hints = &data.hints;
        let stamp = files::stamp_of(&hints.path);
        let read = read(&hints.path);
        let mut findings = hints.findings.clone();
        if let Some(h) = read.hints() {
            let boards: Vec<&str> = hints.boards.iter().map(String::as_str).collect();
            findings.extend(h.unknown_boards(&boards));
        }
        findings.sort_by(|a, b| (a.line, &a.message).cmp(&(b.line, &b.message)));
        let errors = match &read {
            Read::Invalid { errors, .. } => errors
                .iter()
                .map(|e| HintFault {
                    line: e.line,
                    message: e.message.clone(),
                })
                .collect(),
            _ => Vec::new(),
        };
        SchematicHints {
            board: board.to_string(),
            file: hints.name.clone(),
            exists: !matches!(read, Read::Missing | Read::Unreadable(_)),
            stamp,
            text: read.text().to_string(),
            errors,
            boards: read.hints().map(boards_of).unwrap_or_default(),
            findings: findings.iter().map(info_of).collect(),
            version,
        }
    }

    pub(super) fn schematic_hints_of(&self, board: &str) -> Result<SchematicHints, ApiError> {
        let (version, data) = {
            let state = self.fresh();
            (state.version, state.snapshot.board_handle(board)?)
        };
        Ok(self.hints_answer(version, board, &data))
    }

    /// Write the layout file with the canonical text of `text`, if it reads and the file is as
    /// the page saw it. The project is analysed again by the same read of the files that any
    /// request makes: the file's stamp changed, once, and the answer is of the analysis that
    /// has it. The server's watcher then finds nothing new.
    pub(super) fn write_schematic_hints_of(
        &self,
        board: &str,
        text: &str,
        stamp: Option<&str>,
    ) -> Result<SchematicHints, ApiError> {
        let _one_at_a_time = self.editing.lock().unwrap_or_else(|e| e.into_inner());
        let data = self.fresh().snapshot.board_handle(board)?;
        let (path, name) = (&data.hints.path, &data.hints.name);
        let parsed = hints::parse(text).map_err(|errors| {
            let said: Vec<String> = errors
                .iter()
                .map(|e| format!("{name}:{}: {}", e.line, e.message))
                .collect();
            let first = errors.first().map_or(0, |e| e.line).to_string();
            let lines: Vec<String> = errors.iter().map(|e| e.line.to_string()).collect();
            ApiError::unprocessable(
                said.join("\n"),
                "schematic_hints_invalid",
                &[
                    ("file", name.as_str()),
                    ("line", first.as_str()),
                    ("lines", lines.join(",").as_str()),
                ],
            )
        })?;
        if let Some(stamp) = stamp
            && files::stamp_of(path) != stamp
        {
            return Err(ApiError::conflict(format!(
                "{name} changed on disk since it was read: read it again"
            )));
        }
        files::write_atomic(path, &hints::to_toml(&parsed))
            .map_err(|e| ApiError::internal(format!("{name}: {e}")))?;
        let (version, data) = {
            let state = self.fresh();
            let now = state.snapshot.board_handle(board).unwrap_or(data);
            (state.version, now)
        };
        Ok(self.hints_answer(version, board, &data))
    }
}
