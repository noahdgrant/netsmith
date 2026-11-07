//! The schematic layout file beside a top-level source file (ADR 0061): reading it for the
//! drawing, turning what is wrong with it into problems of stage `layout`, and the two routes
//! that read and write it.
//!
//! The file is an input of the drawing and never ambient state: the analysis reads it once for
//! each source file, matches it with each board and passes the result to the drawing. A file
//! that cannot be read is no reason for the drawing to fail: the drawing is then the one there
//! is without a file, and a problem says why.

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
};

use netsmith_editor::{
    ApiError,
    api::{
        HintBoard, HintFault, HintFindingInfo, HintFindingKind, HintNote, HintPart, HintSheet,
        Location, Problem, ProblemStage, SchematicHints, Severity,
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
