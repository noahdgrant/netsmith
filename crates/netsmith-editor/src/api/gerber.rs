//! The Gerber view of the PCB area (issue #1107): the files of a fab package as netsmith's own
//! reader reads and draws them, the reader `netsmith view gerber` prints with
//! (`netsmith_fab::viewer`, issue #1105). The summary is its `Summary` and the drawing its SVG of
//! one view, so the page and the command line cannot differ; the field names are camelCase here
//! and nothing else differs.
//!
//! **`GET /api/gerber?board=&layer=`** reads the package `fab` last wrote for the board: in this
//! session, else the one an earlier run left in the fab folder. **`POST /api/gerber?name=&layer=`**
//! reads a package the page sends (the bytes of a `.zip`, or of one Gerber or drill file) and keeps
//! it, and **`GET /api/gerber?opened=<name>&layer=`** draws another view of it. `layer` is one of
//! the summary's `views` (`top`, `inner1`, `bottom`, `all`); with none, the first. `404` for a board
//! `fab` has written no package for or an opened package that is not kept; `400` for a view the
//! package has no file for, with the views it has.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A box in the viewer's frame (mm from the outline's top left corner, y down).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GerberExtent {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// One Gerber or drill file of the package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GerberFile {
    /// Its path in the package; the drawing's group of it has it as `data-file`.
    pub file: String,
    /// `gerber` or `excellon`.
    pub format: String,
    /// `copper`, `mask`, `paste`, `silk`, `outline`, `drill` or `other`.
    pub function: String,
    /// The view that draws it as its own: `top`, `bottom`, `inner<n>`; null for the outline, the
    /// drill and a file of no known function.
    pub layer: Option<String>,
    /// What said its function: `x2`, `name`, `format` (a drill file); null when nothing did.
    pub identified_by: Option<String>,
    /// Its X2 `%TF.FileFunction`, as written.
    pub file_function: Option<String>,
    /// Whether it was read; a file not read is in `unread` and is not drawn.
    pub read: bool,
    pub extent: Option<GerberExtent>,
    /// A drill file's round holes and slots.
    pub holes: Option<usize>,
    pub slots: Option<usize>,
}

/// The holes of one size, of every drill file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GerberHoles {
    /// The tool's diameter in mm.
    pub diameter: f64,
    pub plated: bool,
    pub holes: usize,
    pub slots: usize,
}

/// A file the reader did not understand, and the first thing in it it did not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GerberUnread {
    pub file: String,
    /// The line of the command, from 1.
    pub line: Option<usize>,
    pub message: String,
}

/// A file of the package that is neither Gerber nor Excellon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GerberSkipped {
    pub file: String,
    pub why: String,
}

/// A package read and one view of it drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GerberView {
    /// The zip's or the folder's name.
    pub package: String,
    /// `fab`: the package `fab` wrote for the board; `opened`: one the page sent.
    pub source: String,
    /// Where the package is, from the project's folder where it is in it, for the command that
    /// draws the same (`netsmith view gerber <path> --layer <layer>`); null for an opened one.
    pub path: Option<String>,
    /// The board's size, the box of the outline's centrelines; null with no outline file.
    pub board: Option<GerberExtent>,
    /// The views the package can be drawn as, in the reader's order (its `--layer` words).
    pub views: Vec<String>,
    pub files: Vec<GerberFile>,
    /// Holes by size, smallest first, plated before not.
    pub holes: Vec<GerberHoles>,
    pub unread: Vec<GerberUnread>,
    pub skipped: Vec<GerberSkipped>,
    /// The view drawn.
    pub layer: String,
    /// The SVG of the view: what `netsmith view gerber --format svg` writes. Its `viewBox` is
    /// `region`, in mm in the viewer's frame; the bottom is mirrored left to right.
    pub svg: String,
    /// The region drawn: x, y, width, height in mm.
    pub region: Vec<f64>,
    /// Whether the drawing is mirrored (the bottom, as a person sees it turned over).
    pub mirrored: bool,
    /// The files drawn, in the order drawn.
    pub drawn: Vec<String>,
}
