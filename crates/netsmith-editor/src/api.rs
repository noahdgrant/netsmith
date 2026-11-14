//! The types that cross between the server and the page. They are defined here once;
//! `web/src/api.ts` is generated from them (`cargo test -p netsmith-editor` checks it is
//! current, and `NETSMITH_WRITE_API_TS=1 cargo test -p netsmith-editor` rewrites it).
//!
//! Lines and columns are 1-based, a column counts characters.
//!
//! **Written once, in Rust.** `ts-rs` generates `web/src/api.ts` from these plain serde types
//! (`ts-rs` and not `specta`: it is a derive on the types as they are, with no registry to keep
//! in step), so the page does not hand-copy a schema. Every change since the first is additive:
//! an optional field is left out of the JSON when it has nothing to say, so a page written
//! before it reads the rest as it did.
//!
//! **A diagnostic is a value, not text.** The checker and the solver report each mistake with
//! a file, a span and a message. A [`Problem`] carries those, the text the command line prints,
//! and the stage that made it, so the page puts a squiggle under the span and lists the real
//! message, and reads no stage, board or instance out of a sentence. A problem in a module is
//! reported at the module's file, wherever the importing board is. What the build does not stop
//! for is a warning (a part not chosen: the design has a hole) or a note (what the solver left
//! alone, mostly expected); the dock lists errors, then warnings, and the notes only once their
//! toggle is on.
//!
//! **Source and elements are tied by declaration spans.** Every instance in the checked model
//! records the member declaration that made it (`.r1: Resistor`, with its file and span), and
//! the design stage numbers the parts from the instances, so a part of a built board knows
//! the instance it came from. The page gets for each part the line of its declaration and of
//! the declarations that contain it (`.stage: LdoStage` for the regulator in the stage): a click
//! on a part opens the file at the innermost one, and the cursor on a line lights every part
//! whose declarations include it. Nets are tied to their parts and not to a line: only the board
//! names them (ADR 0005), and the statement that names a net may be one of several.
//!
//! **A running analysis is told to the page, not only its result.** `analysing` says when one
//! begins, and `progress` carries the stage and the counts of what the stage works through (the
//! top-level files for `check`, the boards and systems for `solve` and design, the modules), so
//! a page says "solving 3 of 7" and never a percentage of the whole, which is not known before
//! the checker has found the boards. `rebuilt` ends it and has `elapsedMs`. The page's clock
//! runs from `analysing.since`, the server's own start, so an edit made outside the editor shows
//! it too. A page that sends the pending header can show the tree and the source while it
//! waits, and loads the views on `rebuilt`; it asks again only as a safety (every 30 s with the
//! socket open, 1, 2, 4 and 5 s without it, since each is a failed request in the browser's
//! console).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

mod compare;
mod hints;
mod view;

pub use compare::*;
pub use hints::*;
pub use view::*;

/// A place in a file, from its first character to its last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The source does not build.
    Error,
    /// The build went on, with a hole in the design: a part not chosen.
    Warning,
    /// What the solver left alone and says so: a relation not evaluated, a balance that is
    /// incomplete. Mostly expected; the page lists these apart and does not count them.
    Note,
}

/// The stage of the build a problem comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ProblemStage {
    /// The text does not parse.
    Syntax,
    /// The checker: names, types, connections, pin tables, and a file that cannot be read.
    Check,
    /// The solver: a choice it could not make, a relation it could not evaluate, a value that
    /// contradicts another; also the solver's notes and warnings of a build that went on (a
    /// part not chosen).
    Solve,
    /// The design stage: making the parts and nets of the solved model (the `Design` the
    /// exporters read), with its notes, and the checks that the schematic and the footprints
    /// agree with it.
    Design,
    /// A guideline (ADR 0036), a warning unless the project makes it an error.
    Guideline,
    /// The power model (issue #187): a rail that can draw more than its supply gives (an
    /// error), a rail nothing supplies, a current or an efficiency the solver did not settle
    /// (notes).
    Power,
    /// The schematic layout file (ADR 0061): a fault in its text, or an entry that matches
    /// nothing in the design or that the drawing could not honour. `at` is the file and line.
    Layout,
}

/// Why a note of the build is there: what would make it go (issue #192). The tool that makes
/// the note says it; the dock groups the notes by it. A closed set (`netsmith_core::Cause`,
/// which the tools use, is the same list): a note that fits none is `other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    /// A check waits on a value that nothing states.
    NotStated,
    /// A part's model leaves a value open that a check needs.
    PartModel,
    /// netsmith cannot judge the pin or the net yet.
    Unjudged,
    /// Values that depend on each other while none has a value.
    Circular,
    /// A rail nothing supplies, a load or a converter the solver did not settle.
    Supply,
    /// A part a provider stands in for, or one the catalog says is out of stock.
    Pick,
    /// A net whose track width is a lower bound, for a current that is not known.
    Track,
    /// Anything else the build says about what it did.
    Other,
}

impl From<netsmith_core::Cause> for Cause {
    fn from(c: netsmith_core::Cause) -> Cause {
        use netsmith_core::Cause as C;
        match c {
            C::NotStated => Cause::NotStated,
            C::PartModel => Cause::PartModel,
            C::Unjudged => Cause::Unjudged,
            C::Circular => Cause::Circular,
            C::Supply => Cause::Supply,
            C::Pick => Cause::Pick,
            C::Track => Cause::Track,
            C::Other => Cause::Other,
        }
    }
}

/// One message of the compiler or solver, or a note of the build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub severity: Severity,
    /// Without the file and line.
    pub message: String,
    /// What the command line prints, with the source line underlined.
    pub rendered: String,
    pub at: Option<Location>,
    /// The stage that found it. Always set by this server; optional so that a page written
    /// before it was added still type checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stage: Option<ProblemStage>,
    /// The board it is about, as `BoardSummary::name` has it (`Demo/main` for a board of a
    /// system), where the problem belongs to one board: every solver, design-stage and guideline
    /// message and note, and a checker error written inside the definition of a board. A
    /// parse error, and a mistake in a module or a system (which boards share), have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub board: Option<String>,
    /// The instance it is about, from the board (`status_indicator.led`), where the server
    /// knows one: a note, a not-chosen warning and a guideline finding. The mistakes of the
    /// checker and the solver have none: they are about a statement, which `at` locates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path: Option<String>,
    /// Why a note is there, which the dock groups the notes by. Set on every note of a
    /// build (the tool that made it says it); a problem that is not a note has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cause: Option<Cause>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum FileRole {
    /// Listed in `top_level`: holds boards.
    TopLevel,
    /// Under `src/`: imported by others.
    Module,
    /// The standard library, built into netsmith.
    Std,
    /// `netsmith.toml`.
    Manifest,
    /// `<stem>.physical.nsm` beside a top-level file or a module: the physical intent of its
    /// boards (ADR 0048).
    Physical,
    /// `<stem>.sch.toml` beside a top-level file: where a person arranged its schematic
    /// (ADR 0061). Listed only when it exists.
    SchematicLayout,
}

/// A definition in a file: `board`, `system`, `panel`, `part`, `interface` or `module`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Def {
    pub kind: String,
    pub name: String,
    pub line: u32,
    /// The line its closing brace is on (the last line of the definition), so the source
    /// outline and the breadcrumbs know which definition a line is in without reading text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub end_line: Option<u32>,
    /// The declarations written directly in its body (`.vin: Rail`), in source order, not the
    /// ones inside a declaration's own block. Left out when there are none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub children: Option<Vec<DefChild>>,
}

/// A declaration in the body of a definition: `.regulator: LinearRegulator`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DefChild {
    /// As written, without the leading dot (`stage.regulator` for a path).
    pub name: String,
    /// What the type is: `board`, `system`, `panel`, `part`, `interface` or `module`. Left
    /// out when the type names no definition that the file or its imports reach.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kind: Option<String>,
    /// The line the declaration starts on.
    pub line: u32,
    /// The line it ends on; the same as `line` for a declaration with no block.
    pub end_line: u32,
    /// The type as written, last segment only: `LinearRegulator` for `std::parts::LinearRegulator`.
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TreeFile {
    /// The name in the tree, which is also what diagnostics call the file.
    pub path: String,
    pub role: FileRole,
    pub readonly: bool,
    pub defs: Vec<Def>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub title: String,
    /// Whether there is a `netsmith.toml`.
    pub has_manifest: bool,
    /// Whether KiCad's command-line tool was found. Nothing needs it: the design rules check
    /// and the fabrication files are netsmith's own. With it the Findings tab can also run
    /// KiCad's check as a second opinion.
    pub kicad: bool,
    pub files: Vec<TreeFile>,
    /// The folders below `src/`, with their path from the project root (`src/sheets`), so the
    /// tree can show one that has no file in it yet.
    pub folders: Vec<String>,
    /// The version of the netsmith that serves this page (`0.1.0`, the crate's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub version: Option<String>,
    /// `git describe --tags --always` of the checkout netsmith was built from, as it was when
    /// the build script last ran (a commit made later with no other change is not seen until
    /// the next build; uncommitted edits are not marked). Absent when netsmith was built
    /// outside a git checkout or without `git`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub git_describe: Option<String>,
    /// The project folder, absolute: where `netsmith.toml` is, or for a single file the folder
    /// it is in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub root: Option<String>,
    /// Where `kicad-cli` was found (`$KICAD_CLI`, `PATH`, or the Windows KiCad under WSL).
    /// Absent when `kicad` is false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kicad_path: Option<String>,
    /// The hash of the page embedded in this binary: a hash of the sources in `web/` that the
    /// page was built from. Absent when the binary has only the placeholder page (it was
    /// built without Node).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bundle_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    pub path: String,
    pub text: String,
    /// What the file was like when read; a save must give it back.
    pub stamp: String,
    pub readonly: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub stamp: String,
}

/// Lock or unlock the pin of one request (`POST /api/pins/lock`): the pin file's `lock` line.
/// Locking a request that has no entry in the file yet writes the entry, with the pin it has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PinLock {
    /// The board as the page names it (`GET /api/board?name=`).
    pub board: String,
    /// The request's path, the key of its entry in the pin file (`status`).
    pub request: String,
    pub locked: bool,
}

/// Move one request to another pin that offers its function (`POST /api/pins/move`): the pin
/// file's `pin` line, and `source = "hand"`, because a person chose it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PinMove {
    pub board: String,
    pub request: String,
    /// The pin's path in the part (`p03`).
    pub pin: String,
}

/// What a write of the pin file did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PinWritten {
    /// The pin file, as the project names it (`src/main.pins.toml`).
    pub file: String,
}

/// A new file (`POST /api/fs/create-file`): its path from the project root (`src/power.nsm`)
/// and its text. Refused if the path is taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NewFile {
    pub path: String,
    pub text: String,
}

/// A new folder (`POST /api/fs/create-dir`), by its path from the project root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NewFolder {
    pub path: String,
}

/// A file or folder moved to another name or folder (`POST /api/fs/rename`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Rename {
    pub from: String,
    pub to: String,
}

/// A file or folder to remove (`POST /api/fs/delete`). `confirm` is what the person said yes
/// to: a folder with something in it and a top-level file are refused without it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Delete {
    pub path: String,
    pub confirm: bool,
}

/// What a change of the files made: the path that is new, or for a delete the one that is gone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Changed {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardSummary {
    /// `LedExample`, or `Demo/main` for a board of a system.
    pub name: String,
    /// The `board` definition it is made from: `LedBoard`.
    pub definition: String,
    pub file: String,
    pub system: Option<String>,
    /// The source has a mistake now; this is the last board that built.
    pub stale: bool,
    pub parts: u32,
    pub nets: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SystemSummary {
    pub name: String,
    pub file: String,
    pub boards: Vec<String>,
}

/// A `panel` of the project (ADR 0040).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PanelSummary {
    pub name: String,
    pub file: String,
    /// The `board` definition it repeats.
    pub board: String,
    pub columns: u32,
    pub rows: u32,
}

/// One thing the panel checks found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PanelFinding {
    /// An error stops `fab`; otherwise it is advice.
    pub error: bool,
    pub message: String,
}

/// A panel drawn: its boards outlined and named, the V-cut lines, rails and marks, and the
/// panel checks, for a board as it is placed now. Read only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PanelView {
    pub name: String,
    pub board: String,
    pub columns: u32,
    pub rows: u32,
    /// Sizes in mm.
    pub cell_width: f64,
    pub cell_height: f64,
    pub width: f64,
    pub height: f64,
    pub rail: f64,
    pub svg: String,
    pub findings: Vec<PanelFinding>,
}

/// A module definition and the boards that use it (`Analysis::module_uses`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModuleUse {
    /// The module's name: `LdoStage`.
    pub name: String,
    /// The file it is defined in, as the tree names it, and the line of its definition
    /// (`Def::line`).
    pub file: String,
    pub line: u32,
    /// The boards that have an instance of it, as `BoardSummary::name` has them
    /// (`Demo/main` for a board of a system), in the order of `Analysis::boards`.
    pub used_by: Vec<String>,
}

/// What a check found: the version of the files it read, and every problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    #[ts(type = "number")]
    pub version: u64,
    pub errors: u32,
    pub warnings: u32,
    pub notes: u32,
    pub problems: Vec<Problem>,
    pub boards: Vec<BoardSummary>,
    pub systems: Vec<SystemSummary>,
    pub panels: Vec<PanelSummary>,
    /// For each module definition that some board uses, directly or inside another module,
    /// the boards that use it, so a declaration in a shared module can show its card for
    /// each board. Sorted by file and line; left out when no board uses a module. It is here
    /// and not on `Def` because `GET /api/project` is answered before the first analysis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub module_uses: Option<Vec<ModuleUse>>,
    /// How long the analysis took, in milliseconds: of the files on disk for `/api/analysis`
    /// and `rebuilt`, of the text on the page for `checked`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub elapsed_ms: Option<u32>,
    /// The last analysis of the files on disk was cancelled (ADR 0062): this is the reading
    /// before it (or, for the first, an empty one with a note), which does not show the files as
    /// they are. It stays so until a file changes or `POST /api/analysis/run`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cancelled: Option<bool>,
    /// A file on disk changed and was not analysed, because the page asked for no analysis on
    /// change (`POST /api/analysis/auto`): this is the reading before it. It stays so until
    /// `POST /api/analysis/run`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stale: Option<bool>,
}

/// The body of `POST /api/analysis/auto`: whether a change to a file on disk is analysed at once
/// (the default), or only when `POST /api/analysis/run` asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AutoBuild {
    pub auto: bool,
}

/// What a route that cancels a job answers (`POST /api/analysis/cancel`, `/api/compare/cancel`,
/// `/api/build/cancel`, `/api/fab/cancel`, `/api/order/cancel`): whether a job was running to be
/// cancelled. The job ends at its next checkpoint, a fraction of a second later, and the page
/// hears `cancelled` over the socket when it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CancelAnswer {
    pub cancelled: bool,
}

/// The answer of a route that needs the analysis while the first one is still running, to a
/// request that sent the `X-Netsmith-Pending: 1` header: HTTP 503 with `Retry-After: 1`.
/// Nothing was done. The socket says when it is over (`rebuilt`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    /// Always `true`: what tells this answer from the route's own.
    pub pending: bool,
    /// How long the first analysis has been running, in milliseconds.
    pub elapsed_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PartInfo {
    pub refdes: String,
    pub kind: String,
    pub value: String,
    pub package: Option<String>,
    /// Where it is from the board: `stage.regulator`.
    pub path: String,
    pub lcsc: Option<String>,
    pub assembled: bool,
    /// The declaration that made it (`.r1: Resistor`), in the file it is written in.
    pub decl: Option<Location>,
    /// The declarations of what contains it, innermost first (`.stage: LdoStage`).
    pub scopes: Vec<Location>,
    /// What the solver worked out for the part and the source did not state. Absent when
    /// there is nothing to say (a part with no catalog row and no derived value).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub solved: Option<Solved>,
    /// The pin requests that ask this part for a pin (`.status: .u.GPIO`, ADR 0101), in the byte
    /// order of their paths, each with the pin it got or the message of the check when it got
    /// none. Absent for a part no request asks. What the editor shows of the pin file: it
    /// computes the assignment in memory; the page's padlock and move write only the `lock`, `pin` and
    /// `source` lines of one entry (`POST /api/pins/lock`, `/api/pins/move`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pin_requests: Option<Vec<PinRequestInfo>>,
    /// The statement that gave the part its package, and what kind of statement it is (issue
    /// 625): a `&=`, a `?=` on the field, or a line of the board's `defaults`. Absent when no
    /// statement states the package (the solver chose it with the row).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub package_from: Option<ValueFrom>,
}

/// Where in the source a value of a part came from (issue 625).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ValueFrom {
    /// The statement: the last one that narrowed the field.
    pub at: Location,
    pub how: ValueHow,
}

/// What kind of statement gave a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ValueHow {
    /// A `&=` on the field, in the part, a module around it or the board.
    Stated,
    /// A `?=` on the field, which nothing replaced (ADR 0051).
    Default,
    /// A line of the board's `defaults` block, for every part of the kind (issue 625).
    BoardDefault,
}

/// One pin request of a part: what it asked for and what it got.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PinRequestInfo {
    /// The request's path in the board, the key of its entry in the pin file (`status`).
    pub request: String,
    /// The function it asks the part for, which the pin offers and the file records (`GPIO`).
    pub function: String,
    /// The pin it got, by its path in the part (`p02`); absent when it got none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pin: Option<String>,
    /// The pad of that pin in the package, as the board's design has it (`2`); several pads
    /// are joined with a comma. Absent when the pin has no pad in the design.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pad: Option<String>,
    /// Whether the entry is locked (`lock = true`): the optimizer never moves it.
    pub locked: bool,
    /// The entry's `source` in the pin file: `hand` or `auto`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub entry: Option<String>,
    /// Where the pin came from: `file` (the pin file beside the source holds this entry and it
    /// still holds) or `run` (picked now, from the source's request, with no entry in the file
    /// yet: `build` would write it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from: Option<String>,
    /// Why the request has no pin: the message the check gave, in its own words. Absent when
    /// the request has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
    /// The pins the request may be moved to, which is what the page offers to move it: every
    /// pin of the part that offers the function and that the source does not hold (the one it
    /// has now is among them), in the byte order of the paths. Empty for a request with no pin.
    #[serde(default)]
    pub offers: Vec<PinOffer>,
}

/// A pin that a request may have, and whether another request has it now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PinOffer {
    /// The pin's path in the part (`p03`).
    pub pin: String,
    /// Its pad in the package, as the design has it; absent when it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pad: Option<String>,
    /// The request that has this pin now, which a move to it is refused for; absent for a free
    /// pin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub held_by: Option<String>,
}

/// What the solver derived for a part, as numbers and as text (ADR 0009, ADR 0013): the
/// worst case over every range the part can take. Each member is left out when it does not
/// apply or the source stated it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Solved {
    /// A capacitor's capacitance in use: the marked value after the DC bias across it and
    /// after its tolerance (`1.72uF..2.916uF` for a 10 uF part at 5 V). Present where the
    /// bias derating applies, that is when the part has a curve and a bias.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub effective_capacitance: Option<Range>,
    /// The DC voltage across the part that the derating used (taken from the rail the part
    /// sits across, ADR 0013). Present with `effective_capacitance`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bias: Option<Range>,
    /// The tolerance of the resistance, capacitance or inductance the solver chose, when the
    /// source did not state the value's range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tolerance: Option<Tolerance>,
    /// The ratings of the row that was picked (`max_voltage`, `max_current`, `max_power`,
    /// `rated_current`, `saturation_current`, ...), the voltages, currents and powers the
    /// part is built for, in the order its definition lists them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ratings: Option<Vec<Rating>>,
    /// The catalog row that stands for the part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub row: Option<SolvedRow>,
}

/// A tolerance as a part's range gives it: half the range over its middle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Tolerance {
    /// `20` for `+/- 20%`, rounded to three decimals.
    pub percent: f64,
    /// `+/- 20%`.
    pub text: String,
}

/// One rating of a part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Rating {
    /// The field of the part: `max_voltage`.
    pub name: String,
    pub range: Range,
}

/// The catalog row a part was made from, or the generic part that stands for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SolvedRow {
    /// The catalog table the part's definition draws from (`resistor`, `ldo`), if it names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub catalog: Option<String>,
    /// The manufacturer's part number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mpn: Option<String>,
    /// The LCSC number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lcsc: Option<String>,
    /// `Basic`, `Extended` or `Preferred`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub library: Option<String>,
    /// The solver chose only a value and a package (ADR 0047) and no row stands for the part
    /// yet; `mpn`, `lcsc` and `library` are then absent.
    pub generic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PinInfo {
    pub refdes: String,
    /// The pad number as the symbol names it (`"1"`, `"A5"`).
    pub pin: String,
    /// The name the part's datasheet prints for the pin (`VIN`, `EN`, `SW`), where the part
    /// has a pin table that lists the pad. Absent for a part with no pin table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// The function the board chose for the pin (`UART1_TX`), when the pin can be several
    /// things and the board picked one. Absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub function: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NetInfo {
    pub name: String,
    pub named: bool,
    pub pins: Vec<PinInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct BoardDetail {
    pub name: String,
    pub file: String,
    pub parts: Vec<PartInfo>,
    pub nets: Vec<NetInfo>,
    pub notes: Vec<String>,
    /// The pick lock beside the board's source file (ADR 0100): whether it exists and is
    /// current, read-only. Absent when there is nothing to compare, because the source does not
    /// build (the problems list says why).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lock: Option<PickLockInfo>,
}

/// What `netsmith lock --check` says of the pick lock of one source file, which holds every
/// board of the file. The editor shows it and writes nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PickLockInfo {
    /// The lock file, named as the files of the project are (`src/main.lock.toml`).
    pub file: String,
    /// Whether the file exists. A source with no part to lock needs none.
    pub exists: bool,
    /// Whether the lock is as `netsmith lock --update` would write it.
    pub current: bool,
    /// What `lock --check` prints when it is not: one line for each part whose row changed, was
    /// added or was dropped. Empty when the file does not exist: `--check` prints no lines then.
    pub diff: Vec<String>,
    /// What the solver said when it held the parts to the rows of the file as it is: the rows
    /// that no longer pass or are no longer in a catalog, in its own words. Empty when every
    /// locked row still holds.
    pub reported: String,
    /// Why the lock could not be checked: the file is not a lock this netsmith reads (its own
    /// words, with the place in it). Then the other fields say nothing of the rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

/// A quantity's range as the language writes it, for example `3.135V..3.465V`: the unit follows
/// the number with no space, as in source and in the compiler's messages (`netsmith-quantity`'s
/// `Display`, which the language guide's examples depend on), so a page that wants a space
/// formats `lo` and `hi` itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Range {
    pub lo: f64,
    pub hi: f64,
    pub text: String,
}

/// One current on a net.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Draw {
    /// The rail or part it belongs to, from the board.
    pub who: String,
    /// `rail`, or `chain` for the current through a chain of parts.
    pub kind: String,
    /// `None` while the solver has not settled it.
    pub range: Option<Range>,
    pub at: Option<Location>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Budget {
    /// What is drawn can be given.
    Within,
    /// What is drawn can exceed what the supply can give.
    Over,
    /// Nothing says what the supply can give.
    Unlimited,
    /// A current is not settled.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PowerNode {
    pub id: String,
    pub net: String,
    /// The board it is on, in a system.
    pub board: Option<String>,
    pub voltage: Option<Range>,
    pub loads: Vec<Draw>,
    pub supplies: Vec<Draw>,
    /// The sum of the loads.
    pub demand: Option<Range>,
    /// What the part that supplies it can give (`max_current`).
    pub capacity: Option<Range>,
    pub capacity_from: Option<String>,
    pub budget: Budget,
    /// Where to look when the node has an issue: the declaration of the part or stage that
    /// supplies it (the one `capacity_from` names), else of its first supply, else of its first
    /// load. A `Draw` has its own `at`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Location>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// A regulator or module that takes one rail and makes another.
    Converts,
    /// Two ports that a system joins.
    Join,
    /// A net that two members of a board share.
    Net,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub label: String,
    pub kind: EdgeKind,
    pub at: Option<Location>,
}

/// What the solver settled for the efficiency of a converter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Stage {
    /// The label of the edge the converter makes (`Edge::label`).
    pub label: String,
    /// Its efficiency, a range of fractions, when it has one and the solver settled it.
    pub efficiency: Option<Range>,
    /// It has an efficiency that the solver did not settle: its input current is then not worked out.
    pub efficiency_unsettled: bool,
    /// The declaration of the converter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Location>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PowerTree {
    pub title: String,
    pub nodes: Vec<PowerNode>,
    pub edges: Vec<Edge>,
    /// The converters of the edges, with the efficiency of each that has one.
    pub stages: Vec<Stage>,
    /// The rails and converters with trouble, worst first: what `netsmith build` and the
    /// analysis report as the `power` stage. Computed here, not by the page.
    #[serde(default)]
    pub issues: Vec<PowerIssue>,
}

/// One trouble of the power model: an error where a supply can be asked for more than it
/// gives, a note for what the solver left unsettled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PowerIssue {
    /// The node it is on: a rail's id, or `stage:<label>` for a converter.
    pub id: String,
    /// `Error` for an overdrawn supply, `Note` for the rest.
    pub severity: Severity,
    /// What it is about: the rail's net (`V3V3`), or the converter's name (`ldo`).
    pub subject: String,
    /// What is wrong, to read after the subject: `has no source: nothing supplies it`.
    pub text: String,
    /// The place in the source to look at: the node's, else its first draw's; a converter's
    /// declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Location>,
    /// The board of a system it is on, as `PowerNode::board` has it; none for a stand-alone
    /// board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub board: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Port {
    pub name: String,
    /// `3.135V..3.465V` (see `Range::text`), if the port is a rail.
    pub voltage: Option<String>,
    pub current: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SystemNode {
    pub id: String,
    pub name: String,
    /// The definition: `PowerBoard`, `Resistor`.
    pub type_name: String,
    /// `board`, `module`, `part` or `port`.
    pub kind: String,
    pub ports: Vec<Port>,
    pub at: Option<Location>,
}

/// One contact of two mated connectors: what each board has behind it (ADR 0040).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MateContact {
    /// From 1.
    pub number: u32,
    /// `ground`, `power`, `signal` or `open`, on the first connector's board.
    pub a_role: String,
    pub b_role: String,
    /// The net the pin is on in its board, if it is on one.
    pub a_net: Option<String>,
    pub b_net: Option<String>,
}

/// Two connectors that a system mates, with their contacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MateView {
    /// The connector of the first board: `carrier.slot_a`.
    pub a: String,
    pub b: String,
    /// The members the two connectors are on.
    pub a_board: String,
    pub b_board: String,
    pub contacts: Vec<MateContact>,
    /// The ports the mate joins, `carrier.cards ~ sensor.supply`.
    pub ports: Vec<String>,
    /// Set when the pinout would also work turned end for end and neither connector is keyed.
    pub reversible: Option<String>,
    pub at: Option<Location>,
}

/// The nets of the boards that mated ground contacts join.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GroundGroupView {
    /// `carrier.GND`: the board member and the name of its net.
    pub nets: Vec<String>,
    /// `carrier.slot_a / sensor.j1: 2`: how many ground contacts each mate gives the group.
    pub contacts: Vec<String>,
}

/// The boards of a system and the joins between their ports, or the members of one board
/// and the nets between them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SystemView {
    pub name: String,
    /// `system` or `board`.
    pub kind: String,
    pub nodes: Vec<SystemNode>,
    pub edges: Vec<Edge>,
    /// The mated connectors of a system; none for a board.
    pub mates: Vec<MateView>,
    /// The grounds the mates join; none for a board.
    pub ground_groups: Vec<GroundGroupView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BomLine {
    pub designators: Vec<String>,
    pub quantity: u32,
    pub value: String,
    pub kind: String,
    pub package: Option<String>,
    pub footprint: Option<String>,
    pub lcsc: Option<String>,
    pub mpn: Option<String>,
    /// `Basic` or `Extended`.
    pub library: Option<String>,
    pub unit_usd: Option<f64>,
    pub line_usd: Option<f64>,
    #[ts(type = "number | null")]
    pub stock: Option<u64>,
    /// The day the price and stock were read, `2026-10-31`: the day of the provider's
    /// answer (ADR 0047), or the row's `date` cell if no provider answered. `None` if
    /// they are unknown.
    pub date: Option<String>,
    /// How many days before today that was.
    pub age_days: Option<i32>,
    /// The price and stock were read more than [`Bom::stale_after_days`] days ago. A line
    /// whose price is unknown is not stale: see [`BomLine::unknown`].
    pub stale: bool,
    /// Where the price and stock come from and which stock it is: `file, JLCPCB assembly
    /// stock`. `None` if they are unknown.
    pub supply: Option<String>,
    /// Why the price and stock are unknown, for a part with a supplier number that no
    /// provider has an answer for (ADR 0047). Never a guess, and never a stock of 0.
    pub unknown: Option<String>,
    /// For a generic part that no catalog row stands for: what is wanted and the nearest
    /// rows (ADR 0047). Such a line has no supplier number and cannot be ordered.
    pub unresolved: Option<String>,
    /// Soldered by hand: the board house does not place it.
    pub hand_solder: bool,
    /// The generic this part was picked to provide (`CoaxReceptacle`, ADR 0068): `kind` is
    /// the provider's name. `None` for a part written by its own type.
    pub via: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Bom {
    pub board: String,
    /// Assembled by the board house, grouped as `bom.csv` is.
    pub lines: Vec<BomLine>,
    /// Soldered by hand.
    pub hand: Vec<BomLine>,
    /// The sum of the lines that have a price.
    pub total_usd: Option<f64>,
    /// Where the prices and stock come from, and how old they are.
    pub note: String,
    /// A price or stock read more than this many days ago (its `date`) is marked stale.
    pub stale_after_days: u32,
    /// The project's own catalog files, which "Check parts against LCSC" checks (ADR 0039). Empty if the
    /// manifest lists none.
    pub catalogs: Vec<String>,
}

/// One cell that a refresh changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CatalogChange {
    pub column: String,
    pub from: String,
    pub to: String,
}

/// What a refresh made of one row of a catalog file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRow {
    pub line: u32,
    pub lcsc: String,
    /// `VERIFIED`, `CONFLICT`, `MISSING` or `SKIPPED`.
    pub status: String,
    /// Why, for the last three.
    pub message: String,
    pub changes: Vec<CatalogChange>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFile {
    pub file: String,
    /// How many rows were looked up.
    pub looked_up: u32,
    /// How many of them are verified: LCSC's record agrees with the row.
    pub verified: u32,
    /// The rows that need a look (a conflict, a missing part, a note); a verified row without
    /// a note is only counted.
    pub rows: Vec<CatalogRow>,
    /// The `checked` cells were written.
    pub written: bool,
    pub notes: Vec<String>,
}

/// What `catalog refresh` did for the page's "Check parts against LCSC" (ADR 0039).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRefresh {
    pub dry_run: bool,
    pub offline: bool,
    pub files: Vec<CatalogFile>,
    pub requests: u32,
    /// Why nothing was done or why the run stopped, if it did; then nothing was written.
    pub error: Option<String>,
    /// The command line's summary, as text.
    pub summary: String,
    /// What the refresh does and never writes, why JLCPCB is not asked, and what is unchecked
    /// (`netsmith_fabricate::refresh::NOTICE`), the same words as the command prints.
    pub notice: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbReport {
    pub board: String,
    pub routed: bool,
    pub unrouted: Vec<String>,
    pub placement_problems: Vec<String>,
    pub tracks: u32,
    pub vias: u32,
    /// The router stopped before it was done: it ran out of time, or it was cancelled.
    pub stopped: bool,
    /// Why there is no board to draw, if there is none.
    pub error: Option<String>,
    /// How long placing and routing took, in milliseconds.
    pub elapsed_ms: u32,
}

/// A box on the sheet, in mm: the coordinates the PCB drawing uses (y grows downward). The
/// layout file counts from the outline's top left corner, which is [`PcbScene::origin`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A face of the board. The layout file and the process call them `top` and `bottom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum PcbSide {
    Top,
    Bottom,
}

/// A pad of a placed part, with its place on the sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbPad {
    pub number: String,
    pub x: f64,
    pub y: f64,
    pub net: Option<String>,
}

/// A part as the PCB canvas needs it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbPart {
    pub refdes: String,
    /// The part's path in the design: what the layout file keys it by.
    pub path: String,
    pub kind: String,
    pub value: String,
    pub package: Option<String>,
    /// The footprint's origin on the sheet.
    pub x: f64,
    pub y: f64,
    /// Degrees, counterclockwise on screen.
    pub rotation: f64,
    pub side: PcbSide,
    /// The placer and the editor leave a locked part where it is.
    pub locked: bool,
    /// `derived` while the layout file has no entry for the part, else who wrote the entry
    /// (`hand`, `auto` or `regen`).
    pub placement: String,
    pub courtyard: PcbBox,
    /// What stands above the board: the footprint's fabrication outline, else its courtyard
    /// (the box a height limit is measured on, ADR 0045 section 11).
    pub body: PcbBox,
    /// How tall the body stands, in mm, if the design states it (never a guess: `None` is
    /// unknown).
    pub height: Option<f64>,
    pub pads: Vec<PcbPad>,
}

/// Where a keepout comes from, which says what the editor may do to it (ADR 0029).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum KeepoutOrigin {
    /// The source states its size and its place: change it there.
    Source,
    /// The source states its size and leaves its place to the layout file: it can be moved.
    Placed,
    /// Drawn in the editor and kept in the layout file: it can be moved, resized and removed.
    Drawn,
    /// Carried by a part, in the part's frame: it moves with the part.
    Part,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbKeepout {
    pub name: String,
    pub origin: KeepoutOrigin,
    /// Where it is on the sheet, if it has a place.
    pub area: Option<PcbBox>,
    pub width: f64,
    pub height: f64,
    pub layers: Vec<String>,
    pub tracks: bool,
    pub vias: bool,
    pub copper: bool,
    pub parts: bool,
    /// The tallest a part may stand where it is, in mm, on the faces in `layers`.
    pub max_height: Option<f64>,
}

/// A rectangular hole through the board, as drawn. The editor does not move it: the outline
/// and the cutouts are the source's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbCutout {
    pub name: String,
    pub area: Option<PcbBox>,
}

/// A board placed, as the PCB canvas draws and edits it: the drawing, and the data that
/// finds things in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbScene {
    pub board: String,
    /// The drawing (`netsmith_pcb::render_svg_view`): its groups, `data-ref`, `data-path`,
    /// `data-pad`, `data-net`, `data-keepout` and `data-cutout` tie it to the rest.
    pub svg: String,
    /// The outline's top left corner on the sheet: a position in the layout file is this far
    /// from the sheet's origin.
    pub origin_x: f64,
    pub origin_y: f64,
    pub outline: PcbBox,
    /// The layout file beside the source, which may not exist yet.
    pub layout_file: String,
    /// What it was like when read: an edit must give it back.
    pub layout_stamp: String,
    /// The copper layers of the board, top first: `top`, `inner1`, `bottom`.
    pub layers: Vec<String>,
    /// Whether the board has routed copper (an edit takes it away).
    pub routed: bool,
    pub parts: Vec<PcbPart>,
    pub keepouts: Vec<PcbKeepout>,
    pub cutouts: Vec<PcbCutout>,
    /// The routes file beside the source (`<stem>.routes.toml`, issue 599), which may not
    /// exist yet.
    pub routes_file: String,
    /// What it was like when read: a change to it must give it back.
    pub routes_stamp: String,
    /// Its records for this board, by net: the fixed copper, drawn on the board before the
    /// router's.
    pub routes: Vec<PcbRouteNet>,
    /// The width of each net's tracks by the board's rules: what a track drawn in the view
    /// gets.
    pub net_widths: Vec<PcbNetWidth>,
    /// The process's via, which a via placed in the view gets: diameter and drill in mm.
    pub via_diameter: f64,
    pub via_drill: f64,
}

/// The copper pours of a board as the PCB view draws them under the tracks and pads: the fill of
/// each pour, plane and routes-file zone, as `netsmith fab` plots it, by copper layer. It is a
/// separate answer from [`PcbScene`] because filling a board takes longer than placing it: the
/// page draws the board first and the fill when it arrives, and drops an answer whose stamps
/// are not those of the scene it shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbPours {
    pub board: String,
    /// [`PcbScene::layout_stamp`], [`PcbScene::routes_stamp`] and [`PcbScene::routed`] of the
    /// placement that was filled.
    pub layout_stamp: String,
    pub routes_stamp: String,
    pub routed: bool,
    /// Each copper layer that has a fill, front to back.
    pub layers: Vec<PcbPourLayer>,
    /// Why the board could not be filled, if it could not: then `layers` is empty.
    pub error: Option<String>,
}

/// The fill of one copper layer, drawn in the layer's own colour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbPourLayer {
    /// KiCad's name of the layer: `F.Cu`, `In1.Cu`, `B.Cu`.
    pub layer: String,
    /// One entry for each net that has a fill on the layer (and one for a fill of no net).
    pub nets: Vec<PcbPourNet>,
}

/// The fill of one net on one layer, as an SVG path on the sheet, in mm: a closed subpath for
/// each outline and for each hole of a piece of copper (an island is a piece of its own), the
/// outlines clockwise on the screen and the holes the other way, so the path is filled by the
/// non-zero rule. The points are rounded to a micrometre, and a point within 2 micrometres of
/// the line between its neighbours is left out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbPourNet {
    pub net: Option<String>,
    pub d: String,
}

/// A point on the sheet, in mm.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbPoint {
    pub x: f64,
    pub y: f64,
}

/// One net's record of the routes file, as the PCB view shows it. Its shapes are numbered from
/// 1 in the order they are listed, as `netsmith routes show` numbers them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbRouteNet {
    pub net: String,
    /// The router adds no copper to the net.
    pub lock: bool,
    /// The net's pads are where the record was written: every shape is drawn. When not, only
    /// the locked shapes are, and the findings say so; a net the board does not have draws none.
    pub current: bool,
    pub tracks: Vec<PcbRouteTrack>,
    pub vias: Vec<PcbRouteVia>,
}

/// A track of a record, on the sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbRouteTrack {
    /// `top` or `bottom`.
    pub layer: String,
    pub width: f64,
    pub from: PcbPoint,
    /// For an arc, a point on it halfway from `from` to `to`.
    pub mid: Option<PcbPoint>,
    pub to: PcbPoint,
    /// Kept when the net's pads move; not deleted until unlocked.
    pub lock: bool,
    /// Whether it is on the board (see [`PcbRouteNet::current`]).
    pub drawn: bool,
}

/// A via of a record, on the sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbRouteVia {
    pub at: PcbPoint,
    pub diameter: f64,
    pub drill: f64,
    pub lock: bool,
    pub drawn: bool,
}

/// A change the PCB view makes to the routes file, as `netsmith routes add-track`, `add-via`,
/// `delete`, `lock` and `unlock` make it (one implementation, `netsmith_fabricate::routes::edit`).
/// `track` and `via` name one shape of the net's record, numbered from 1; with neither it is the
/// whole record. Places are on the sheet, as the scene's are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "what", rename_all = "snake_case")]
pub enum RoutesChange {
    Delete {
        net: String,
        track: Option<u32>,
        via: Option<u32>,
    },
    Lock {
        net: String,
        track: Option<u32>,
        via: Option<u32>,
        /// Lock, or unlock.
        lock: bool,
    },
    /// A straight track of the net on `layer` (`top` or `bottom`); `width` `None` is the net's
    /// width from the board's rules.
    AddTrack {
        net: String,
        layer: String,
        width: Option<f64>,
        from: PcbPoint,
        to: PcbPoint,
    },
    /// A via of the net, of the process's size.
    AddVia { net: String, at: PcbPoint },
    /// A straight track of the net with its ends at `from` and `to`; refused while it is
    /// locked.
    MoveTrack {
        net: String,
        track: u32,
        from: PcbPoint,
        to: PcbPoint,
    },
    /// A via of the net at `at`; refused while it is locked.
    MoveVia { net: String, via: u32, at: PcbPoint },
    /// The net's record put back as `after`, refused unless the file has `before` now: the
    /// undo (and redo) of an edit, with the texts its [`RoutesUndo`] gave.
    Restore {
        net: String,
        before: Option<String>,
        after: Option<String>,
    },
}

/// One edit of the routes file: all its changes are made or none. They may be of several nets
/// (a track drawn with the copper of other nets shoved out of its way, issue 726), each made to
/// its own net's record, and the design rules check is made once for all of them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbRoutesEdit {
    pub board: String,
    /// `PcbScene::routes_stamp` as the page read it.
    pub stamp: String,
    pub changes: Vec<RoutesChange>,
}

/// The board after a change to the routes file was written, what was done, and how to undo it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbRoutesEdited {
    pub scene: PcbScene,
    pub said: String,
    /// One for each net the edit changed, in the order the nets first appear in its changes: all
    /// of them are one step of undo (and of redo).
    pub undo: Vec<RoutesUndo>,
}

/// The net's record before and after an edit, each as the text the routes file has of it
/// (`None`: no record). The page keeps it as a step of undo and hands it back unread, turned
/// round, in [`RoutesChange::Restore`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RoutesUndo {
    pub net: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// A question of the track tool: the way for a straight track of `net` on `layer` from its last
/// corner `from` to `to`, round the copper of the board as the routes file's check reads it
/// (issue #726). Nothing is written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbWalk {
    pub board: String,
    pub net: String,
    /// `top` or `bottom`.
    pub layer: String,
    /// `None` is the net's width from the board's rules, as in [`RoutesChange::AddTrack`].
    pub width: Option<f64>,
    pub from: PcbPoint,
    pub to: PcbPoint,
    /// The direction of the segment that arrives at `from`, if one does: the way the first
    /// segment may turn from by 45 degrees at most.
    pub heading: Option<PcbPoint>,
    /// The corners of the track drawn so far, the last of them `from`: the router's copper that
    /// the whole track crosses is answered in [`PcbWalked::crossed`], not only what the one
    /// segment crosses. Empty: the segment alone.
    #[serde(default)]
    pub track: Vec<PcbPoint>,
    /// Shove the unlocked shapes of the routes file out of the way of the track (issue 726, the
    /// third slice) before going round them: `false` (also when the field is missing) is the
    /// walkaround alone.
    #[serde(default)]
    pub shove: bool,
}

/// The answer to a [`PcbWalk`]: the corners from `from` to `to`, both included, or why there is
/// no way. Having no way is an answer, not an error. `tried` counts the states the search took
/// (0 where the straight segment or one bend was free).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbWalked {
    pub points: Option<Vec<PcbPoint>>,
    pub reason: Option<String>,
    pub tried: u32,
    /// The router's copper that the track (`PcbWalk::track`, then `points`) crosses: copper the
    /// edit discards, which the walk does not go round (issue 726). Empty on a board the
    /// router has not routed.
    pub crossed: PcbCrossed,
    /// The unlocked shapes of the routes file that the way found shoves out of its way, in their
    /// new places (`PcbWalk::shove`). Empty where nothing is in the way or the walk went round.
    #[serde(default)]
    pub shoved: PcbShoved,
    /// Why nothing was shoved though a shove was asked and something is in the way: the shape
    /// that cannot move far enough, and the walkaround was used instead.
    #[serde(default)]
    pub stuck: Option<String>,
}

/// The shapes of the routes file a track shoves, each with where it was and where it goes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbShoved {
    pub tracks: Vec<PcbShovedTrack>,
    pub vias: Vec<PcbShovedVia>,
}

/// A track of a record, shoved: `track` numbers it in its net's record from 1, and the page makes
/// `RoutesChange::MoveTrack { net, track, from, to }` of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbShovedTrack {
    pub net: String,
    pub track: u32,
    /// `top` or `bottom`.
    pub layer: String,
    pub width: f64,
    pub was_from: PcbPoint,
    pub was_to: PcbPoint,
    pub from: PcbPoint,
    pub to: PcbPoint,
}

/// A via of a record, shoved: `via` numbers it in its net's record from 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbShovedVia {
    pub net: String,
    pub via: u32,
    pub diameter: f64,
    pub was_at: PcbPoint,
    pub at: PcbPoint,
}

/// The router's copper a track crosses, as the PCB view draws it giving way: each piece with
/// its place, so the page needs no lookup.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbCrossed {
    pub tracks: Vec<PcbCrossedTrack>,
    pub vias: Vec<PcbCrossedVia>,
}

/// A track of the router that is crossed. `mid` is set for an arc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbCrossedTrack {
    pub net: String,
    /// `top` or `bottom`.
    pub layer: String,
    pub width: f64,
    pub from: PcbPoint,
    pub mid: Option<PcbPoint>,
    pub to: PcbPoint,
}

/// A via of the router that is crossed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbCrossedVia {
    pub net: String,
    pub at: PcbPoint,
    pub diameter: f64,
}

/// The width of a net's tracks by the board's rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbNetWidth {
    pub net: String,
    pub width: f64,
}

/// Where a part is in the layout file: millimetres from the outline's top left corner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PartPlacement {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub side: PcbSide,
    pub locked: bool,
    /// Who wrote it: `hand`, `auto` or `regen`; the editor writes `hand`.
    pub source: Option<String>,
}

/// A keepout drawn in the editor: all of it is the layout file's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DrawnKeepout {
    pub width: f64,
    pub height: f64,
    /// `top`, `bottom`, `inner1`, ...
    pub layers: Vec<String>,
    pub tracks: bool,
    pub vias: bool,
    pub copper: bool,
    pub parts: bool,
}

/// A keepout's entry in the layout file: its corner (from the outline's top left corner), and
/// the rest of it if the editor drew it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct KeepoutPlacement {
    pub x: f64,
    pub y: f64,
    pub drawn: Option<DrawnKeepout>,
}

/// One entry of the layout file changing from `before` to `after`: `None` is no entry (a part
/// the placer places, a keepout that is not there). The server refuses the change if the
/// entry is not `before` now, so an edit never overwrites what another program wrote, and
/// the inverse of a change is the change with the two swapped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "what", rename_all = "snake_case")]
pub enum PcbChange {
    Part {
        path: String,
        before: Option<PartPlacement>,
        after: Option<PartPlacement>,
    },
    Keepout {
        name: String,
        before: Option<KeepoutPlacement>,
        after: Option<KeepoutPlacement>,
    },
}

/// A set of changes that is one step of undo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbEdit {
    pub board: String,
    /// `PcbScene::layout_stamp` as the page read it.
    pub stamp: String,
    pub changes: Vec<PcbChange>,
}

/// The board after an edit was written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PcbEdited {
    pub scene: PcbScene,
}

/// Where a finding comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum FindingSource {
    /// netsmith's own check of where the parts are (`Board::check_placement`).
    Placement,
    /// A connection the router left (the ratsnest of the routed board).
    Unrouted,
    /// netsmith's own design rules check of the routed copper (ADR 0037).
    Drc,
    /// KiCad's design rules check, run on demand as a second opinion.
    Kicad,
    /// A record of the routes file that is not used as written (its net's pads moved, or the
    /// board has no such net): the warning `fab` prints.
    Routes,
}

/// One thing wrong with the board, with where it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbFinding {
    /// Names it within one answer: the same finding has the same id in the next.
    pub id: String,
    pub source: FindingSource,
    pub severity: Severity,
    /// KiCad's name for the rule (`clearance`, `courtyards_overlap`) or ours (`overlap`).
    pub rule: String,
    pub message: String,
    /// Where it is on the sheet, in mm: the point, and the box the page zooms to.
    pub x: f64,
    pub y: f64,
    pub area: PcbBox,
    /// The references of the parts it is about.
    pub parts: Vec<String>,
    /// The keepout the finding is about, by name, where it is about one (a part in it, or too
    /// tall for its limit): the page lights it when the finding is picked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub keepout: Option<String>,
    /// The line of a physical file (or of the circuit source) that states the requirement the
    /// finding quotes, where it quotes one; the first of them if it quotes several. A point at
    /// the start of the line (`column` and `endColumn` are 1), `file` as the checker names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Location>,
    /// The case model's solid the finding is about (ADR 0109), as the 3D scene's node names it
    /// (`case:<model>:<solid>`), or the model's group (`case:<model>`): the 3D view lights it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub case_solid: Option<String>,
    /// Where to look in 3D, in mm in the scene's frame (`docs/BOARD3D.md`), for a finding of
    /// the case fit check: the 3D view flies there when no part is named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub point3d: Option<[f64; 3]>,
}

/// Whether KiCad's check has been run on the board as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum KicadState {
    /// Not on this layout: it has not been asked for, or the board changed since.
    NotRun,
    Ran,
    /// There is no `kicad-cli`, or it failed; `kicad_note` says which.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbFindings {
    pub board: String,
    /// Whether the board has routed copper: only then are the connections that are left a finding.
    pub routed: bool,
    /// Errors first, then warnings, each in the order found.
    pub findings: Vec<PcbFinding>,
    pub kicad: KicadState,
    pub kicad_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BuildReport {
    pub ok: bool,
    pub written: Vec<String>,
    pub output: String,
    /// How long it took, in milliseconds.
    pub elapsed_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Drc {
    Passed,
    Failed,
    /// `fab` stopped before the design rules check: the board could not be placed or routed.
    NotRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    /// `gerbers`, `bom`, `cpl`, `notes` or `pcb`.
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FabReport {
    pub board: String,
    pub ok: bool,
    pub drc: Drc,
    pub summary: String,
    /// The text of `fab-notes.txt`.
    pub notes: String,
    /// What `fab` printed when it stopped.
    pub output: String,
    pub downloads: Vec<Download>,
    /// How long it took, in milliseconds.
    pub elapsed_ms: u32,
}

/// One line of an order of `netsmith.toml` (ADR 0035).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OrderBoardInfo {
    /// A board as its folder is named, or a system for all its members.
    pub name: String,
    pub quantity: u32,
}

/// A named order of the manifest: the boards to make and how many of each.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OrderInfo {
    pub name: String,
    pub boards: Vec<OrderBoardInfo>,
}

/// What running an order (`netsmith fab --order`) did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OrderReport {
    pub name: String,
    pub ok: bool,
    /// Where the order was written.
    pub dir: String,
    /// The text of `order.txt`: the boards, the combined bill of materials, the notes to
    /// read before ordering and the unverified items.
    pub summary: String,
    /// What `fab` printed when it stopped.
    pub output: String,
    /// `summary` and `bom`.
    pub downloads: Vec<Download>,
    /// How long it took, in milliseconds.
    pub elapsed_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Definition {
    pub at: Location,
    /// A one-line signature of what is defined, in a stable short form: the keyword, the
    /// name and the declarations that make its interface, at most six, then `...` if the
    /// body has more (`module LdoStage { .vin: Rail .vout: Rail ... }`; the ports of a
    /// module or board are the declarations whose type is an interface, and an interface or
    /// part lists its own). For a member it is the declaration, `.vin: Rail`. Absent when
    /// the name is an imported module (see `doc`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub signature: Option<String>,
    /// The `///` lines directly above the definition or declaration, one line of the answer
    /// for each, the leading space of a line trimmed; for an imported module, the `//!` lines
    /// at the top of its file. Absent when there are none. A hover card needs no library file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub doc: Option<String>,
}

/// What the page tells the server over the socket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Check the project with `path` as `text`, nothing written. `id` comes back.
    Check { id: u32, path: String, text: String },
    /// The page's window has the focus (sent when it gets it, and on connecting if it has it):
    /// `/api/view` drives the page that had it last.
    Focus,
    /// The answer to a `view`: the 3D view as it is now, after the change.
    ViewState { id: u32, state: Box<ViewState> },
    /// A `view` or a `view_picture` the page could not do, and why. `status` is the HTTP status
    /// the route answers with (400 when it is not given: the change is wrong).
    ViewError {
        id: u32,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        status: Option<u16>,
    },
}

/// What the server tells the page over the socket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// A file changed and the project was built again. `elapsed_ms` is how long the analysis
    /// took (the same as `analysis.elapsedMs`).
    Rebuilt {
        analysis: Analysis,
        #[serde(default, rename = "elapsedMs", skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        elapsed_ms: Option<u32>,
    },
    /// An analysis of the files on disk has begun (the first one, or one for a change made by
    /// another program); `rebuilt` ends it. `since` is when, in milliseconds since the Unix
    /// epoch. A page that connects while one runs is sent this at once.
    Analysing {
        #[ts(type = "number")]
        since: u64,
    },
    /// The answer to a `Check`.
    Checked { id: u32, analysis: Analysis },
    /// What a long job is doing now: `task` is `route`, `fab`, `order`, `build`, `catalog`,
    /// `analyse` or `compare`, `message` is the line the command line would print (`BoardName: placing 73
    /// parts`). An `analyse` message also has `stage` (`check`, `solve`, `design`, `draw` or
    /// `modules`), and `done` of `total` of the things that stage works through (files for
    /// `check`, boards and systems for `solve`, `design` and `draw`, modules for `modules`). A
    /// `compare` message has `board` set to the ref asked for and the same `stage`, `done` and
    /// `total` (of the ref's analysis), with two stages more: `extract` (reading the ref out of
    /// git) and `diff` (comparing the two readings).
    ///
    /// A `solve` message also says how far the solver has got inside the board when it counts
    /// something: `unit` (`parts chosen`), `unitDone` and, when it is known, `unitTotal`
    /// (ADR 0062). The count only grows within a board; no total is ever guessed.
    Progress {
        task: String,
        board: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        stage: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        done: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        total: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        unit: Option<String>,
        #[serde(default, rename = "unitDone", skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        unit_done: Option<u32>,
        #[serde(default, rename = "unitTotal", skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        unit_total: Option<u32>,
    },
    /// A job was cancelled (ADR 0062) and has ended; nothing it made was kept. `task` is
    /// `analyse` (the last reading stays, and `analysis.cancelled` says so until a file changes
    /// or the page asks to run it again), `compare` (`board` is the ref asked for), `build`,
    /// `fab` (`board`) or `order` (`board` is the order's name).
    Cancelled {
        task: String,
        board: String,
        message: String,
    },
    /// To one page, the one `/api/view` drives: apply `change` to the 3D view (none: change
    /// nothing) and answer with `view_state`, or `view_error`.
    View {
        id: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        change: Option<ViewChange>,
    },
    /// To one page: draw the 3D view and answer with a binary frame, the `id` as 4 bytes
    /// little-endian then the PNG of the canvas, or with `view_error`.
    ViewPicture { id: u32 },
}

/// Every type, for the TypeScript file.
pub fn typescript() -> String {
    let mut out = String::from(
        "// Generated from crates/netsmith-editor/src/api.rs. Do not edit: run\n// `NETSMITH_WRITE_API_TS=1 cargo test -p netsmith-editor` after changing the Rust types.\n\n",
    );
    let config = ts_rs::Config::default();
    macro_rules! decls {
        ($($t:ty),* $(,)?) => {$(
            out.push_str("export ");
            out.push_str(&<$t as TS>::decl(&config));
            out.push_str("\n\n");
        )*};
    }
    decls!(
        Location,
        Severity,
        ProblemStage,
        Cause,
        Problem,
        HintFault,
        HintFindingKind,
        HintFindingInfo,
        HintPart,
        HintNote,
        HintSheet,
        HintBoard,
        SchematicHints,
        FileRole,
        Def,
        DefChild,
        TreeFile,
        Project,
        FileContent,
        Saved,
        PinLock,
        PinMove,
        PinWritten,
        NewFile,
        NewFolder,
        Rename,
        Delete,
        Changed,
        BoardSummary,
        SystemSummary,
        PanelSummary,
        PanelFinding,
        PanelView,
        ModuleUse,
        Analysis,
        Pending,
        PartInfo,
        PinRequestInfo,
        ValueFrom,
        ValueHow,
        PinOffer,
        Solved,
        Tolerance,
        Rating,
        SolvedRow,
        PinInfo,
        NetInfo,
        BoardDetail,
        PickLockInfo,
        Range,
        Draw,
        Budget,
        PowerNode,
        EdgeKind,
        Edge,
        PowerTree,
        PowerIssue,
        Stage,
        Port,
        SystemNode,
        MateContact,
        MateView,
        GroundGroupView,
        SystemView,
        BomLine,
        Bom,
        CatalogChange,
        CatalogRow,
        CatalogFile,
        CatalogRefresh,
        PcbReport,
        PcbBox,
        PcbSide,
        PcbPad,
        PcbPart,
        KeepoutOrigin,
        PcbKeepout,
        PcbCutout,
        PcbScene,
        PcbPours,
        PcbPourLayer,
        PcbPourNet,
        PcbPoint,
        PcbRouteNet,
        PcbRouteTrack,
        PcbRouteVia,
        RoutesChange,
        PcbRoutesEdit,
        PcbRoutesEdited,
        RoutesUndo,
        PcbWalk,
        PcbWalked,
        PcbCrossed,
        PcbCrossedTrack,
        PcbCrossedVia,
        PcbShoved,
        PcbShovedTrack,
        PcbShovedVia,
        PcbNetWidth,
        PartPlacement,
        DrawnKeepout,
        KeepoutPlacement,
        PcbChange,
        PcbEdit,
        PcbEdited,
        FindingSource,
        PcbFinding,
        KicadState,
        PcbFindings,
        BuildReport,
        Drc,
        Download,
        FabReport,
        OrderBoardInfo,
        OrderInfo,
        OrderReport,
        Definition,
        GitCommit,
        GitRefKind,
        GitRef,
        GitInfo,
        CompareHead,
        SourceStatus,
        SourceFile,
        SourceDiff,
        BoardStatus,
        ComparePart,
        CompareField,
        ComparePartChange,
        PartsDiff,
        ComparePin,
        CompareNet,
        CompareNetChange,
        NetsDiff,
        CompareRail,
        RangeChange,
        BudgetChange,
        CompareRailChange,
        RailsDiff,
        CompareInstance,
        CompareInstanceChange,
        InstancesDiff,
        CompareBomLine,
        CompareBomChange,
        BomDiff,
        ProblemsDiff,
        ComparePcbPlace,
        ComparePcbPartChange,
        CompareSize,
        OutlineChange,
        LayersChange,
        ComparePcbArea,
        PcbDiff,
        CompareSystemPort,
        CompareSystemPortChange,
        SystemPortsDiff,
        NamesDiff,
        CompareMate,
        CompareMateChange,
        MatesDiff,
        CompareSystem,
        CompareBoard,
        Compare,
        CancelAnswer,
        AutoBuild,
        RenderView,
        CaseMode,
        ColourBy,
        CameraPlace,
        PreviewSetting,
        ViewChange,
        ViewState,
        ClientMessage,
        ServerMessage,
    );
    out.trim_end().to_string() + "\n"
}
