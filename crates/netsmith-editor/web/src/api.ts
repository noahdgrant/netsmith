// Generated from crates/netsmith-editor/src/api.rs. Do not edit: run
// `NETSMITH_WRITE_API_TS=1 cargo test -p netsmith-editor` after changing the Rust types.

export type Location = { file: string, line: number, column: number, endLine: number, endColumn: number, };

export type Severity = "error" | "warning" | "note";

export type ProblemStage = "syntax" | "check" | "solve" | "lower" | "guideline";

export type Problem = { severity: Severity, 
/**
 * Without the file and line.
 */
message: string, 
/**
 * What the command line prints, with the source line underlined.
 */
rendered: string, at: Location | null, 
/**
 * The stage that found it. Always set by this server; optional so that a page written
 * before it was added still type checks.
 */
stage?: ProblemStage, 
/**
 * The board it is about, as `BoardSummary::name` has it (`Demo/main` for a board of a
 * system), where the problem belongs to one board: every solver, lowering and guideline
 * message and note, and a checker error written inside the definition of a board. A
 * parse error, and a mistake in a module or a system (which boards share), have none.
 */
board?: string, 
/**
 * The instance it is about, from the board (`status_indicator.led`), where the server
 * knows one: a note, a not-chosen warning and a guideline finding. The mistakes of the
 * checker and the solver have none: they are about a statement, which `at` locates.
 */
path?: string, };

export type FileRole = "top_level" | "module" | "std" | "manifest" | "physical";

export type Def = { kind: string, name: string, line: number, 
/**
 * The line its closing brace is on (the last line of the definition), so the source
 * outline and the breadcrumbs know which definition a line is in without reading text.
 */
endLine?: number, 
/**
 * The declarations written directly in its body (`.vin: Rail`), in source order, not the
 * ones inside a declaration's own block. Left out when there are none.
 */
children?: Array<DefChild>, };

export type TreeFile = { 
/**
 * The name in the tree, which is also what diagnostics call the file.
 */
path: string, role: FileRole, readonly: boolean, defs: Array<Def>, };

export type Project = { title: string, 
/**
 * Whether there is a `netsmith.toml`.
 */
hasManifest: boolean, 
/**
 * Whether KiCad's command-line tool was found. Nothing needs it: the design rules check
 * and the fabrication files are netsmith's own. With it the Findings tab can also run
 * KiCad's check as a second opinion.
 */
kicad: boolean, files: Array<TreeFile>, 
/**
 * The folders below `src/`, with their path from the project root (`src/sheets`), so the
 * tree can show one that has no file in it yet.
 */
folders: Array<string>, 
/**
 * The version of the netsmith that serves this page (`0.1.0`, the crate's).
 */
version?: string, 
/**
 * `git describe --tags --always` of the checkout netsmith was built from, as it was when
 * the build script last ran (a commit made later with no other change is not seen until
 * the next build; uncommitted edits are not marked). Absent when netsmith was built
 * outside a git checkout or without `git`.
 */
gitDescribe?: string, 
/**
 * The project folder, absolute: where `netsmith.toml` is, or for a single file the folder
 * it is in.
 */
root?: string, 
/**
 * Where `kicad-cli` was found (`$KICAD_CLI`, `PATH`, or the Windows KiCad under WSL).
 * Absent when `kicad` is false.
 */
kicadPath?: string, 
/**
 * The `SOURCE-HASH` of the page embedded in this binary: a hash of the sources in
 * `web/` that the page was built from. Absent when the binary has no page. It is the
 * hash of the page's sources, which is not the hash of `web/` as it is now if the
 * binary was built from a checkout whose `web-dist` was out of date.
 */
bundleHash?: string, };

export type FileContent = { path: string, text: string, 
/**
 * What the file was like when read; a save must give it back.
 */
stamp: string, readonly: boolean, };

export type Saved = { stamp: string, };

export type NewFile = { path: string, text: string, };

export type NewFolder = { path: string, };

export type Rename = { from: string, to: string, };

export type Delete = { path: string, confirm: boolean, };

export type Changed = { path: string, };

export type BoardSummary = { 
/**
 * `LedExample`, or `Demo/main` for a board of a system.
 */
name: string, 
/**
 * The `board` definition it is made from: `LedBoard`.
 */
definition: string, file: string, system: string | null, 
/**
 * The source has a mistake now; this is the last board that built.
 */
stale: boolean, parts: number, nets: number, };

export type SystemSummary = { name: string, file: string, boards: Array<string>, };

export type PanelSummary = { name: string, file: string, 
/**
 * The `board` definition it repeats.
 */
board: string, columns: number, rows: number, };

export type PanelFinding = { 
/**
 * An error stops `fab`; otherwise it is advice.
 */
error: boolean, message: string, };

export type PanelView = { name: string, board: string, columns: number, rows: number, 
/**
 * Sizes in mm.
 */
cellWidth: number, cellHeight: number, width: number, height: number, rail: number, svg: string, findings: Array<PanelFinding>, };

export type Analysis = { version: number, errors: number, warnings: number, notes: number, problems: Array<Problem>, boards: Array<BoardSummary>, systems: Array<SystemSummary>, panels: Array<PanelSummary>, 
/**
 * How long the analysis took, in milliseconds: of the files on disk for `/api/analysis`
 * and `rebuilt`, of the text on the page for `checked`.
 */
elapsedMs?: number, };

export type Pending = { 
/**
 * Always `true`: what tells this answer from the route's own.
 */
pending: boolean, 
/**
 * How long the first analysis has been running, in milliseconds.
 */
elapsedMs: number, };

export type PartInfo = { refdes: string, kind: string, value: string, package: string | null, 
/**
 * Where it is from the board: `stage.regulator`.
 */
path: string, lcsc: string | null, assembled: boolean, 
/**
 * The declaration that made it (`.r1: Resistor`), in the file it is written in.
 */
decl: Location | null, 
/**
 * The declarations of what contains it, innermost first (`.stage: LdoStage`).
 */
scopes: Array<Location>, };

export type PinInfo = { refdes: string, pin: string, };

export type NetInfo = { name: string, named: boolean, pins: Array<PinInfo>, };

export type BoardDetail = { name: string, file: string, parts: Array<PartInfo>, nets: Array<NetInfo>, notes: Array<string>, };

export type Range = { lo: number, hi: number, text: string, };

export type Draw = { 
/**
 * The rail or part it belongs to, from the board.
 */
who: string, 
/**
 * `rail`, or `chain` for the current through a chain of parts.
 */
kind: string, 
/**
 * `None` while the solver has not settled it.
 */
range: Range | null, at: Location | null, };

export type Budget = "within" | "over" | "unlimited" | "unknown";

export type PowerNode = { id: string, net: string, 
/**
 * The board it is on, in a system.
 */
board: string | null, voltage: Range | null, loads: Array<Draw>, supplies: Array<Draw>, 
/**
 * The sum of the loads.
 */
demand: Range | null, 
/**
 * What the part that supplies it can give (`max_current`).
 */
capacity: Range | null, capacityFrom: string | null, budget: Budget, 
/**
 * Where to look when the node has an issue: the declaration of the part or stage that
 * supplies it (the one `capacity_from` names), else of its first supply, else of its first
 * load. A `Draw` has its own `at`.
 */
at?: Location, };

export type EdgeKind = "converts" | "join" | "net";

export type Edge = { from: string, to: string, label: string, kind: EdgeKind, at: Location | null, };

export type PowerTree = { title: string, nodes: Array<PowerNode>, edges: Array<Edge>, 
/**
 * The converters of the edges, with the efficiency of each that has one.
 */
stages: Array<Stage>, };

export type Stage = { 
/**
 * The label of the edge the converter makes (`Edge::label`).
 */
label: string, 
/**
 * Its efficiency, a range of fractions, when it has one and the solver settled it.
 */
efficiency: Range | null, 
/**
 * It has an efficiency that the solver did not settle: its input current is then not worked out.
 */
efficiencyUnsettled: boolean, 
/**
 * The declaration of the converter.
 */
at?: Location, };

export type Port = { name: string, 
/**
 * `3.135 V .. 3.465 V`, if the port is a rail.
 */
voltage: string | null, current: string | null, };

export type SystemNode = { id: string, name: string, 
/**
 * The definition: `PowerBoard`, `Resistor`.
 */
typeName: string, 
/**
 * `board`, `module`, `part` or `port`.
 */
kind: string, ports: Array<Port>, at: Location | null, };

export type MateContact = { 
/**
 * From 1.
 */
number: number, 
/**
 * `ground`, `power`, `signal` or `open`, on the first connector's board.
 */
aRole: string, bRole: string, 
/**
 * The net the pin is on in its board, if it is on one.
 */
aNet: string | null, bNet: string | null, };

export type MateView = { 
/**
 * The connector of the first board: `carrier.slot_a`.
 */
a: string, b: string, 
/**
 * The members the two connectors are on.
 */
aBoard: string, bBoard: string, contacts: Array<MateContact>, 
/**
 * The ports the mate joins, `carrier.cards ~ sensor.supply`.
 */
ports: Array<string>, 
/**
 * Set when the pinout would also work turned end for end and neither connector is keyed.
 */
reversible: string | null, at: Location | null, };

export type GroundGroupView = { 
/**
 * `carrier.GND`: the board member and the name of its net.
 */
nets: Array<string>, 
/**
 * `carrier.slot_a / sensor.j1: 2`: how many ground contacts each mate gives the group.
 */
contacts: Array<string>, };

export type SystemView = { name: string, 
/**
 * `system` or `board`.
 */
kind: string, nodes: Array<SystemNode>, edges: Array<Edge>, 
/**
 * The mated connectors of a system; none for a board.
 */
mates: Array<MateView>, 
/**
 * The grounds the mates join; none for a board.
 */
groundGroups: Array<GroundGroupView>, };

export type BomLine = { designators: Array<string>, quantity: number, value: string, kind: string, package: string | null, footprint: string | null, lcsc: string | null, mpn: string | null, 
/**
 * `Basic` or `Extended`.
 */
library: string | null, unitUsd: number | null, lineUsd: number | null, stock: number | null, 
/**
 * The day the price and stock were read, `2026-10-31`: the day of the provider's
 * answer (ADR 0047), or the row's `date` cell if no provider answered. `None` if
 * they are unknown.
 */
date: string | null, 
/**
 * How many days before today that was.
 */
ageDays: number | null, 
/**
 * The price and stock were read more than [`Bom::stale_after_days`] days ago. A line
 * whose price is unknown is not stale: see [`BomLine::unknown`].
 */
stale: boolean, 
/**
 * Where the price and stock come from and which stock it is: `file, JLCPCB assembly
 * stock`. `None` if they are unknown.
 */
supply: string | null, 
/**
 * Why the price and stock are unknown, for a part with a supplier number that no
 * provider has an answer for (ADR 0047). Never a guess, and never a stock of 0.
 */
unknown: string | null, 
/**
 * For a generic part that no catalog row stands for: what is wanted and the nearest
 * rows (ADR 0047). Such a line has no supplier number and cannot be ordered.
 */
unresolved: string | null, 
/**
 * Soldered by hand: the board house does not place it.
 */
handSolder: boolean, };

export type Bom = { board: string, 
/**
 * Assembled by the board house, grouped as `bom.csv` is.
 */
lines: Array<BomLine>, 
/**
 * Soldered by hand.
 */
hand: Array<BomLine>, 
/**
 * The sum of the lines that have a price.
 */
totalUsd: number | null, 
/**
 * Where the prices and stock come from, and how old they are.
 */
note: string, 
/**
 * A price or stock read more than this many days ago (its `date`) is marked stale.
 */
staleAfterDays: number, 
/**
 * The project's own catalog files, which "Check parts against LCSC" checks (ADR 0039). Empty if the
 * manifest lists none.
 */
catalogs: Array<string>, };

export type CatalogChange = { column: string, from: string, to: string, };

export type CatalogRow = { line: number, lcsc: string, 
/**
 * `VERIFIED`, `CONFLICT`, `MISSING` or `SKIPPED`.
 */
status: string, 
/**
 * Why, for the last three.
 */
message: string, changes: Array<CatalogChange>, notes: Array<string>, };

export type CatalogFile = { file: string, 
/**
 * How many rows were looked up.
 */
lookedUp: number, 
/**
 * How many of them are verified: LCSC's record agrees with the row.
 */
verified: number, 
/**
 * The rows that need a look (a conflict, a missing part, a note); a verified row without
 * a note is only counted.
 */
rows: Array<CatalogRow>, 
/**
 * The `checked` cells were written.
 */
written: boolean, notes: Array<string>, };

export type CatalogRefresh = { dryRun: boolean, offline: boolean, files: Array<CatalogFile>, requests: number, 
/**
 * Why nothing was done or why the run stopped, if it did; then nothing was written.
 */
error: string | null, 
/**
 * The command line's summary, as text.
 */
summary: string, 
/**
 * What the refresh does and never writes, why JLCPCB is not asked, and what is unchecked
 * (`netsmith::refresh::NOTICE`), the same words as the command prints.
 */
notice: string, };

export type PcbReport = { board: string, routed: boolean, unrouted: Array<string>, placementProblems: Array<string>, tracks: number, vias: number, 
/**
 * The router stopped before it was done: it ran out of time, or it was cancelled.
 */
stopped: boolean, 
/**
 * Why there is no board to draw, if there is none.
 */
error: string | null, 
/**
 * How long placing and routing took, in milliseconds.
 */
elapsedMs: number, };

export type PcbBox = { x: number, y: number, width: number, height: number, };

export type PcbSide = "top" | "bottom";

export type PcbPad = { number: string, x: number, y: number, net: string | null, };

export type PcbPart = { refdes: string, 
/**
 * The part's path in the design: what the layout file keys it by.
 */
path: string, kind: string, value: string, package: string | null, 
/**
 * The footprint's origin on the sheet.
 */
x: number, y: number, 
/**
 * Degrees, counterclockwise on screen.
 */
rotation: number, side: PcbSide, 
/**
 * The placer and the editor leave a locked part where it is.
 */
locked: boolean, 
/**
 * `derived` while the layout file has no entry for the part, else who wrote the entry
 * (`hand`, `auto` or `regen`).
 */
placement: string, courtyard: PcbBox, pads: Array<PcbPad>, };

export type KeepoutOrigin = "source" | "placed" | "drawn" | "part";

export type PcbKeepout = { name: string, origin: KeepoutOrigin, 
/**
 * Where it is on the sheet, if it has a place.
 */
area: PcbBox | null, width: number, height: number, layers: Array<string>, tracks: boolean, vias: boolean, copper: boolean, parts: boolean, };

export type PcbCutout = { name: string, area: PcbBox | null, };

export type PcbScene = { board: string, 
/**
 * The drawing (`netsmith_pcb::svg::render_view`): its groups, `data-ref`, `data-path`,
 * `data-pad`, `data-net`, `data-keepout` and `data-cutout` tie it to the rest.
 */
svg: string, 
/**
 * The outline's top left corner on the sheet: a position in the layout file is this far
 * from the sheet's origin.
 */
originX: number, originY: number, outline: PcbBox, 
/**
 * The layout file beside the source, which may not exist yet.
 */
layoutFile: string, 
/**
 * What it was like when read: an edit must give it back.
 */
layoutStamp: string, 
/**
 * The copper layers of the board, top first: `top`, `inner1`, `bottom`.
 */
layers: Array<string>, 
/**
 * Whether the board has routed copper (an edit takes it away).
 */
routed: boolean, parts: Array<PcbPart>, keepouts: Array<PcbKeepout>, cutouts: Array<PcbCutout>, };

export type PartPlacement = { x: number, y: number, rotation: number, side: PcbSide, locked: boolean, 
/**
 * Who wrote it: `hand`, `auto` or `regen`; the editor writes `hand`.
 */
source: string | null, };

export type DrawnKeepout = { width: number, height: number, 
/**
 * `top`, `bottom`, `inner1`, ...
 */
layers: Array<string>, tracks: boolean, vias: boolean, copper: boolean, parts: boolean, };

export type KeepoutPlacement = { x: number, y: number, drawn: DrawnKeepout | null, };

export type PcbChange = { "what": "part", path: string, before: PartPlacement | null, after: PartPlacement | null, } | { "what": "keepout", name: string, before: KeepoutPlacement | null, after: KeepoutPlacement | null, };

export type PcbEdit = { board: string, 
/**
 * `PcbScene::layout_stamp` as the page read it.
 */
stamp: string, changes: Array<PcbChange>, };

export type PcbEdited = { scene: PcbScene, };

export type FindingSource = "placement" | "unrouted" | "drc" | "kicad";

export type PcbFinding = { 
/**
 * Names it within one answer: the same finding has the same id in the next.
 */
id: string, source: FindingSource, severity: Severity, 
/**
 * KiCad's name for the rule (`clearance`, `courtyards_overlap`) or ours (`overlap`).
 */
rule: string, message: string, 
/**
 * Where it is on the sheet, in mm: the point, and the box the page zooms to.
 */
x: number, y: number, area: PcbBox, 
/**
 * The references of the parts it is about.
 */
parts: Array<string>, 
/**
 * The line of a physical file (or of the circuit source) that states the requirement the
 * finding quotes, where it quotes one; the first of them if it quotes several. A point at
 * the start of the line (`column` and `endColumn` are 1), `file` as the checker names it.
 */
at?: Location, };

export type KicadState = "not_run" | "ran" | "unavailable";

export type PcbFindings = { board: string, 
/**
 * Whether the board has routed copper: only then are the connections that are left a finding.
 */
routed: boolean, 
/**
 * Errors first, then warnings, each in the order found.
 */
findings: Array<PcbFinding>, kicad: KicadState, kicadNote: string, };

export type BuildReport = { ok: boolean, written: Array<string>, output: string, 
/**
 * How long it took, in milliseconds.
 */
elapsedMs: number, };

export type Drc = "passed" | "failed" | "not_run";

export type Download = { 
/**
 * `gerbers`, `bom`, `cpl`, `notes` or `pcb`.
 */
key: string, name: string, };

export type FabReport = { board: string, ok: boolean, drc: Drc, summary: string, 
/**
 * The text of `fab-notes.txt`.
 */
notes: string, 
/**
 * What `fab` printed when it stopped.
 */
output: string, downloads: Array<Download>, 
/**
 * How long it took, in milliseconds.
 */
elapsedMs: number, };

export type OrderBoardInfo = { 
/**
 * A board as its folder is named, or a system for all its members.
 */
name: string, quantity: number, };

export type OrderInfo = { name: string, boards: Array<OrderBoardInfo>, };

export type OrderReport = { name: string, ok: boolean, 
/**
 * Where the order was written.
 */
dir: string, 
/**
 * The text of `order.txt`: the boards, the combined bill of materials, the notes to
 * read before ordering and the unverified items.
 */
summary: string, 
/**
 * What `fab` printed when it stopped.
 */
output: string, 
/**
 * `summary` and `bom`.
 */
downloads: Array<Download>, 
/**
 * How long it took, in milliseconds.
 */
elapsedMs: number, };

export type Definition = { at: Location, 
/**
 * A one-line signature of what is defined, in a stable short form: the keyword, the
 * name and the declarations that make its interface, at most six, then `...` if the
 * body has more (`module LdoStage { .vin: Rail .vout: Rail ... }`; the ports of a
 * module or board are the declarations whose type is an interface, and an interface or
 * part lists its own). For a member it is the declaration, `.vin: Rail`. Absent when
 * the name is an imported module (see `doc`).
 */
signature?: string, 
/**
 * The `///` lines directly above the definition or declaration, one line of the answer
 * for each, the leading space of a line trimmed; for an imported module, the `//!` lines
 * at the top of its file. Absent when there are none. A hover card needs no library file.
 */
doc?: string, };

export type ClientMessage = { "type": "check", id: number, path: string, text: string, };

export type ServerMessage = { "type": "rebuilt", analysis: Analysis, elapsedMs?: number, } | { "type": "analysing", since: number, } | { "type": "checked", id: number, analysis: Analysis, } | { "type": "progress", task: string, board: string, message: string, stage?: string, done?: number, total?: number, };
