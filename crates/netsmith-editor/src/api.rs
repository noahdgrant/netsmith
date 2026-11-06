//! The types that cross between the server and the page. They are defined here once;
//! `web/src/api.ts` is generated from them (`cargo test -p netsmith-editor` checks it is
//! current, and `NETSMITH_WRITE_API_TS=1 cargo test -p netsmith-editor` rewrites it).
//!
//! Lines and columns are 1-based, a column counts characters.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
    /// Lowering the solved design to parts and nets, with its notes, and the checks that the
    /// schematic and the footprints agree with it.
    Lower,
    /// A guideline (ADR 0036), a warning unless the project makes it an error.
    Guideline,
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
    /// system), where the problem belongs to one board: every solver, lowering and guideline
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
}

/// A definition in a file: `board`, `system`, `panel`, `part`, `interface` or `module`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Def {
    pub kind: String,
    pub name: String,
    pub line: u32,
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
    /// The `SOURCE-HASH` of the page embedded in this binary: a hash of the sources in
    /// `web/` that the page was built from. Absent when the binary has no page. It is the
    /// hash of the page's sources, which is not the hash of `web/` as it is now if the
    /// binary was built from a checkout whose `web-dist` was out of date.
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
    /// How long the analysis took, in milliseconds: of the files on disk for `/api/analysis`
    /// and `rebuilt`, of the text on the page for `checked`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub elapsed_ms: Option<u32>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PinInfo {
    pub refdes: String,
    pub pin: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NetInfo {
    pub name: String,
    pub named: bool,
    pub pins: Vec<PinInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BoardDetail {
    pub name: String,
    pub file: String,
    pub parts: Vec<PartInfo>,
    pub nets: Vec<NetInfo>,
    pub notes: Vec<String>,
}

/// A quantity's range as it reads, for example `3.135 V .. 3.465 V`.
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Port {
    pub name: String,
    /// `3.135 V .. 3.465 V`, if the port is a rail.
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
    /// (`netsmith::refresh::NOTICE`), the same words as the command prints.
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
    /// The drawing (`netsmith_pcb::svg::render_view`): its groups, `data-ref`, `data-path`,
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
}

/// One thing wrong with the board, with where it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
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
    /// The line of a physical file (or of the circuit source) that states the requirement the
    /// finding quotes, where it quotes one; the first of them if it quotes several. A point at
    /// the start of the line (`column` and `endColumn` are 1), `file` as the checker names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Location>,
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
}

/// What the page tells the server over the socket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Check the project with `path` as `text`, nothing written. `id` comes back.
    Check { id: u32, path: String, text: String },
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
    /// What a long job is doing now: `task` is `route`, `fab`, `order`, `build`, `catalog` or
    /// `analyse`, `message` is the line the command line would print (`BoardName: placing 73
    /// parts`). An `analyse` message also has `stage` (`check`, `solve`, `lower`, `draw` or
    /// `modules`), and `done` of `total` of the things that stage works through (files for
    /// `check`, boards and systems for `solve`, `lower` and `draw`, modules for `modules`).
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
    },
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
        Problem,
        FileRole,
        Def,
        TreeFile,
        Project,
        FileContent,
        Saved,
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
        Analysis,
        Pending,
        PartInfo,
        PinInfo,
        NetInfo,
        BoardDetail,
        Range,
        Draw,
        Budget,
        PowerNode,
        EdgeKind,
        Edge,
        PowerTree,
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
        ClientMessage,
        ServerMessage,
    );
    out.trim_end().to_string() + "\n"
}
