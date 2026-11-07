//! The schematic layout file as the page sees it (ADR 0061): the text of `<stem>.sch.toml`,
//! what it says in structure, the faults of its text and the entries that do not match the
//! design.
//!
//! Lengths are millimetres, as the file writes them (a multiple of the schematic's 1.27 mm
//! grid), x to the right and y down, as on the page.

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
