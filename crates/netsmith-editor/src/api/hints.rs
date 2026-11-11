//! The schematic layout file as the page sees it (ADR 0061): the text of `<stem>.sch.toml`,
//! what it says in structure, the faults of its text and the entries that do not match the
//! design.
//!
//! Lengths are millimetres, as the file writes them (a multiple of the schematic's 1.27 mm
//! grid), x to the right and y down, as on the page.
//!
//! **`GET /api/schematic-hints?board=`** answers the file of the source file the board is in
//! (`BoardSummary.name`, `Demo/main` for a board of a system; `404` for a board that is not
//! there; it waits for the first analysis like the views and answers `503` to a request that
//! sends `X-Netsmith-Pending: 1`). `text` is the file as it is on disk (`""` with none); `stamp`
//! is as `GET /api/file` has it (`none` for no file); `errors` are the faults of the text, and
//! when there are any `boards` is empty (the file is ignored as a whole); `boards` is what the
//! text says, in millimetres as written, so a page needs no TOML reader; `findings` are the
//! entries not used for this board plus the tables for boards the source lacks, in file order;
//! `version` is the analysis they are of.
//!
//! **`PUT /api/schematic-hints?board=`** takes the text of the file and an optional `X-Stamp`
//! and answers as `GET` does. Refusals leave the file as it was: `422` with
//! `{code: "schematic_hints_invalid", message, file, line, lines}` (`message` is every fault as
//! `src/main.sch.toml:4: ...`, one to a line; `line` is the first fault's, `lines` all of them,
//! comma separated, since the structured body holds only strings), `409` when the stamp is given
//! and the file is not as it was, `404` for a board that is not there.
//!
//! **What the page does with it** (the page is `web/src/schematic/` and `views/`; its choices
//! are recorded here because they are what the file's format was made for). To write `rotate`,
//! `mirror` and `pin` from what is on screen, each part's box in the drawing carries
//! `data-origin`, `data-orient` and, on the sheet's reference part, `data-reference`; a key writes
//! a turn from what is drawn, because the file's `rotate` *replaces* the automatic turn and the
//! page cannot add a quarter to a turn it does not know.
//! - *Dragging.* A press on a part's body and a move of 4 screen pixels is a drag; a press that
//!   does not move is a click (select; double-click or Enter opens Source). The block moves as a
//!   transform of the dragged part, a whole number of grid steps, with a ghost where it was and
//!   a readout (`R1  dx +2.54  dy -1.27 mm`). The preview moves the part only: its satellites and
//!   wires are carried by the engine when the file is written, because the page does not know the
//!   block. Nothing is written while the pointer moves; the drop writes once; Esc cancels. Notes
//!   drag the same way.
//! - *What a drop writes.* The steps are added to what the entry has; a drop that comes back to
//!   the start removes the entry; a part that is pinned is moved by moving its pin; `rotate` and
//!   `mirror` are kept. Alt held at the drop writes `pin = [x, y]` instead of an offset (Shift
//!   was not taken because Shift+arrow is ten steps; a window manager that takes Alt+drag leaves
//!   the file to be edited by hand).
//! - *The reference part is inert.* Its offset translates the whole drawing and changes no
//!   distance, so it cannot be dragged or nudged, and a `pin` is measured from it. It can be
//!   turned and mirrored. So is a block that holds the reference part (on a board where
//!   everything hangs from one part): the page finds out afterwards, when the part is where it was
//!   from the reference part in the drawing that came back, says "nothing was arranged" and writes
//!   the entry back. Marking such blocks in the drawing would let the page refuse before the
//!   write; not built, issue #19.
//! - *Keys.* With a part selected whose block can move: the arrows nudge one grid step and
//!   `Shift+Arrow` ten; `r` turns a quarter counterclockwise from the orientation drawn,
//!   `Shift+R` the other way; `m` mirrors; `Backspace` or `Delete` clears the entry. The arrows
//!   keep panning when nothing movable is selected. "Clear the layout of this module" and "Clear
//!   every layout entry of the file" ask first, and clearing the file writes `version = 1`.
//! - *One write at a time.* A write analyses the whole project. A second drop waits behind the
//!   first, one only, and is made on the file as the first left it; a third is refused with a
//!   toast. While a write is on its way the drawing is dimmed. `409`: the file is read again and
//!   the drop is lost. A file with a fault is ignored as a whole by the drawing, so writing
//!   would drop it: the page refuses and says to fix the line first.
//! - *A drop the drawing refuses* (a finding of kind `unusable` at the new entry's line): the
//!   page writes the entry back to what it was and says why. An entry that was refused and is
//!   still in a hand-edited file is drawn with a red dashed outline; a part with an entry wears a
//!   dashed outline in `--sch-muted`.
//! - *The file is a tree file* (role `schematic_layout`, listed only when it exists, only for a
//!   top-level source), read and saved by its name like any file and shown as TOML, so a problem
//!   of stage `layout` opens it at its line. It cannot be renamed on its own; it can be deleted
//!   on its own (the file is then gone, not `version = 1`), and it goes with its source.
//!
//! Not built (issue #19): the preview of a drag moves the part and not its block, so satellites
//! and wires jump when the file is written (showing the block needs its membership in the
//! drawing); a drag in the overview (all modules) is not offered, the cards being thumbnails.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A fault in the text of the file: what the file says wrong. The file is then ignored as a
/// whole, and the drawing is the one there is without it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HintFault {
    /// 1 for the first line of the file.
    pub line: u32,
    pub message: String,
}

/// What kind of entry is not used, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum HintFindingKind {
    /// The table names a sheet the board does not have.
    StaleSheet,
    /// The sheet has no such part or note.
    StaleEntry,
    /// The part or note is on another sheet.
    WrongSheet,
    /// The file has a table for a board the source file does not have.
    UnknownBoard,
    /// The drawing could not honour the entry (it would lie on another part, or break a rule).
    Unusable,
}

/// An entry that matches nothing in the design, or that the drawing could not honour. It is
/// ignored, and it is a note of stage `layout` in the problems list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HintFindingInfo {
    pub kind: HintFindingKind,
    /// The line of the entry's table header; 0 if it has none.
    pub line: u32,
    /// The entry as the file names it: `[Trellis.sheets.stage.parts."output_cap"]`.
    pub entry: String,
    /// What is wrong with it, ending with what happens to it.
    pub message: String,
}

/// What the file says about one part: its block, the part with the satellites that serve it,
/// is moved, turned or mirrored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HintPart {
    /// Moved by this much from where the automatic drawing puts it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dx: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dy: Option<f64>,
    /// Instead of `dx` and `dy`: the block's origin at this place from the sheet's reference
    /// part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pin: Option<(f64, f64)>,
    /// 0, 90, 180 or 270 degrees counterclockwise, replacing the automatic turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rotate: Option<u16>,
    /// Flipped left to right before the turn, replacing the automatic mirror.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mirror: Option<bool>,
    /// The line of the entry's table header in the file.
    pub line: u32,
}

/// What the file says about one note: how far its frame was moved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HintNote {
    pub dx: f64,
    pub dy: f64,
    pub line: u32,
}

/// The entries of one sheet: a part by its path from the sheet, a note by `<path>#doc` or
/// `<path>#relation`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HintSheet {
    pub parts: BTreeMap<String, HintPart>,
    pub notes: BTreeMap<String, HintNote>,
}

/// The table of one board: the entries of the sheet of the parts the board declares itself,
/// and of each module instance's sheet.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HintBoard {
    pub parts: BTreeMap<String, HintPart>,
    pub notes: BTreeMap<String, HintNote>,
    pub sheets: BTreeMap<String, HintSheet>,
}

/// The layout file of the source file a board is in, and how it matches the board
/// (`GET`, and the answer of `PUT`, `/api/schematic-hints?board=`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SchematicHints {
    /// The board asked for, as `BoardSummary.name` has it.
    pub board: String,
    /// The file's name in the project (`src/main.sch.toml`): where the file is or goes.
    pub file: String,
    /// Whether the file is on disk. Without it there are no hints and nothing is wrong.
    pub exists: bool,
    /// What the file was like when read: a `PUT` that sends it back is refused (`409`) if the
    /// file is different now. `none` for a file that is not there.
    pub stamp: String,
    /// The file as it is on disk; empty without one.
    pub text: String,
    /// What the text says wrong, in the order of the file. When there is any the file is
    /// ignored: `boards` is empty and the drawing is the one there is without a file.
    pub errors: Vec<HintFault>,
    /// The tables of the file, by the board's definition name, as the text says them.
    pub boards: BTreeMap<String, HintBoard>,
    /// The entries not used for this board, and the tables for boards the source lacks, in the
    /// order of the file. The same notes are in the problems list.
    pub findings: Vec<HintFindingInfo>,
    /// The version of the analysis the drawing and the findings are of (`Analysis.version`).
    pub version: u64,
}
