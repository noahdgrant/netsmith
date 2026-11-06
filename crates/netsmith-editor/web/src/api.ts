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

export type Def = { kind: string, name: string, line: number, };

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
folders: Array<string>, };

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

export type Definition = { at: Location, };

export type GitCommit = { 
/**
 * The full hash: what a compare is cached by.
 */
hash: string, short: string, subject: string, author: string, 
/**
 * The author date, ISO 8601 with its offset (`2026-11-04T09:30:00+01:00`).
 */
date: string, };

export type GitRefKind = "head" | "upstream" | "default" | "branch" | "tag";

export type GitRef = { 
/**
 * What to pass as `ref`: `HEAD`, `origin/main`, `feature`, `v1.2`.
 */
name: string, kind: GitRefKind, 
/**
 * The commit it points at (a tag's own commit, not the tag object).
 */
hash: string, short: string, subject: string, date: string, 
/**
 * The branch that is checked out.
 */
current: boolean, };

export type GitInfo = { available: boolean, reason?: string, 
/**
 * The top of the work tree, an absolute path.
 */
root?: string, 
/**
 * Where the project is in the work tree, with `/`: empty if it is the top, else
 * `hardware/board`.
 */
projectDir?: string, 
/**
 * The checked-out branch; absent when HEAD is detached.
 */
branch?: string, head?: GitCommit, 
/**
 * Anything in the work tree differs from HEAD, untracked files included.
 */
dirty: boolean, 
/**
 * The same for the project's folder: what a compare against HEAD sees.
 */
projectDirty: boolean, 
/**
 * `HEAD`, the upstream, the default branch, the local branches (50 at most) and the
 * tags (30 at most), in that order; a name that comes twice is listed once.
 */
refs: Array<GitRef>, 
/**
 * The last 30 commits reachable from HEAD, newest first.
 */
commits: Array<GitCommit>, };

export type CompareHead = { 
/**
 * HEAD, if the project is in a work tree that has one.
 */
commit?: GitCommit, 
/**
 * The project's folder has changes that are not committed: the files on disk are what is
 * compared, so they are in the diff.
 */
dirty: boolean, 
/**
 * How many errors the files on disk have now. While there are some, a board that does
 * not build is shown as of its last good reading (`CompareBoard.stale`).
 */
errors: number, };

export type SourceStatus = "modified" | "added" | "deleted" | "untracked";

export type SourceFile = { 
/**
 * Relative to the project's folder, with `/`.
 */
path: string, status: SourceStatus, added: number, removed: number, };

export type SourceDiff = { filesChanged: number, added: number, removed: number, files: Array<SourceFile>, 
/**
 * Why the numbers are missing, if git could not give them.
 */
error?: string, };

export type BoardStatus = "added" | "removed" | "changed" | "unchanged";

export type ComparePart = { path: string, designator: string, kind: string, value: string, package?: string, lcsc?: string, mpn?: string, 
/**
 * `fab`, `hand` or `none` (ADR 0024).
 */
assembly: string, 
/**
 * The declaration: on the head side for an added part, on the base side for a removed one.
 */
at?: Location, };

export type CompareField = { field: string, old?: string, new?: string, };

export type ComparePartChange = { 
/**
 * Its path on the head side.
 */
path: string, 
/**
 * Its path at the ref, when the part moved (a module was renamed or the part now sits in
 * another): then `fields` has a `path` entry too.
 */
basePath?: string, designatorBase: string, designatorHead: string, 
/**
 * In a fixed order: `path`, `kind`, `value`, `package`, `lcsc`, `mpn`, `assembly`,
 * `designator`. Never empty; a part in `renumbered` has only `designator`.
 */
fields: Array<CompareField>, 
/**
 * Its declaration on the head side.
 */
at?: Location, 
/**
 * Its declaration at the ref.
 */
baseAt?: Location, };

export type PartsDiff = { added: Array<ComparePart>, removed: Array<ComparePart>, 
/**
 * Something other than the designator differs (a moved part is here).
 */
changed: Array<ComparePartChange>, 
/**
 * Only the designator differs: adding a part renumbers the ones after it, so this is
 * long and mostly noise.
 */
renumbered: Array<ComparePartChange>, unchanged: number, };

export type ComparePin = { 
/**
 * The part's path on the side the pin is listed from.
 */
part: string, designator: string, pin: string, 
/**
 * The part's declaration on that side.
 */
at?: Location, };

export type CompareNet = { name: string, 
/**
 * The design names it (`3V3`), as against `net-r1.1`, which netsmith names after the
 * smallest pin it joins.
 */
named: boolean, pins: Array<ComparePin>, 
/**
 * The first pin's part, where the net can be pointed at in the source.
 */
at?: Location, };

export type CompareNetChange = { 
/**
 * The name on the head side.
 */
name: string, 
/**
 * The name at the ref.
 */
baseName: string, 
/**
 * The names differ, and at least one is a name the design gave. (A change between two
 * generated names follows from the pins and is listed as `changed`.)
 */
renamed: boolean, 
/**
 * Pins on the head side that the net did not have at the ref, with the head side's
 * designators.
 */
joined: Array<ComparePin>, 
/**
 * Pins the net had at the ref and has no more, with the ref's designators.
 */
left: Array<ComparePin>, 
/**
 * The first joined pin's part, else the first that left (on its own side).
 */
at?: Location, };

export type NetsDiff = { added: Array<CompareNet>, removed: Array<CompareNet>, 
/**
 * Same pins (or most of them) under another name.
 */
renamed: Array<CompareNetChange>, 
/**
 * Same name, other pins.
 */
changed: Array<CompareNetChange>, unchanged: number, };

export type CompareRail = { net: string, voltage?: Range, demand?: Range, capacity?: Range, budget: Budget, at?: Location, };

export type RangeChange = { base?: Range, head?: Range, };

export type BudgetChange = { base: Budget, head: Budget, };

export type CompareRailChange = { 
/**
 * The net's name on the head side.
 */
net: string, 
/**
 * Its name at the ref, when the net was renamed.
 */
baseNet?: string, voltage?: RangeChange, demand?: RangeChange, capacity?: RangeChange, budget?: BudgetChange, 
/**
 * Amperes the supply can give above the largest demand (`capacity.hi - demand.hi`),
 * when both are known on that side. Shrinking is the thing to look at.
 */
headroomBase?: number, headroomHead?: number, at?: Location, baseAt?: Location, };

export type RailsDiff = { added: Array<CompareRail>, removed: Array<CompareRail>, changed: Array<CompareRailChange>, unchanged: number, };

export type CompareInstance = { path: string, 
/**
 * The definition it was made from: `LdoStage`, `Rail`.
 */
typeName: string, at?: Location, };

export type CompareInstanceChange = { path: string, baseType: string, headType: string, at?: Location, };

export type InstancesDiff = { added: Array<CompareInstance>, removed: Array<CompareInstance>, changed: Array<CompareInstanceChange>, };

export type CompareBomLine = { kind: string, value: string, package?: string, footprint?: string, lcsc?: string, mpn?: string, handSolder: boolean, quantity: number, designators: Array<string>, unitUsd?: number, };

export type CompareBomChange = { kind: string, value: string, package?: string, footprint?: string, lcsc?: string, mpn?: string, handSolder: boolean, quantityBase: number, quantityHead: number, designatorsBase: Array<string>, designatorsHead: Array<string>, unitUsdBase?: number, unitUsdHead?: number, };

export type BomDiff = { added: Array<CompareBomLine>, removed: Array<CompareBomLine>, changed: Array<CompareBomChange>, unchanged: number, 
/**
 * The total of the priced lines for one board, as `GET /api/bom` has it, on each side.
 * Prices are those of each side's own catalogs and the supply file as they are now.
 */
totalUsdBase?: number, totalUsdHead?: number, 
/**
 * `total_usd_head - total_usd_base`, when both are known.
 */
deltaUsd?: number, };

export type ProblemsDiff = { 
/**
 * On the head side only, with the head side's `at`.
 */
new: Array<Problem>, 
/**
 * At the ref only, with the ref's `at`.
 */
resolved: Array<Problem>, unchanged: number, };

export type CompareBoard = { 
/**
 * As `BoardSummary.name` has it: what `?board=` and the other endpoints take.
 */
name: string, status: BoardStatus, 
/**
 * The source file of the board on each side, absent on the side that has no such board.
 */
baseFile?: string, headFile?: string, 
/**
 * The head side is the last good reading of a file that does not build now.
 */
stale: boolean, summary: string, parts: PartsDiff, nets: NetsDiff, rails: RailsDiff, 
/**
 * Module instances by path.
 */
modules: InstancesDiff, 
/**
 * Interface instances that are ports of the board or of a module, by path.
 */
interfaces: InstancesDiff, 
/**
 * Absent when the bill of materials cannot be made on one side (`bom_error` says why).
 */
bom?: BomDiff, bomError?: string, problems: ProblemsDiff, };

export type Compare = { 
/**
 * The ref as asked.
 */
ref: string, 
/**
 * What it resolved to.
 */
base: GitCommit, head: CompareHead, 
/**
 * Set when the project at the ref cannot be compared: it has no `netsmith.toml` there, or
 * it does not build.
 */
error?: string, 
/**
 * The errors of the analysis at the ref, when `error` is set because of them.
 */
baseProblems: Array<Problem>, 
/**
 * The analysis of the ref was kept from an earlier compare (by commit hash).
 */
cached: boolean, 
/**
 * The whole request, in milliseconds.
 */
elapsedMs: number, 
/**
 * One line: `3 parts added, 1 removed, 2 values changed, 1 rail's headroom shrank from
 * 80 mA to 20 mA`; `No circuit changes` when there are none.
 */
summary: string, 
/**
 * Whether anything in the circuit differs (a renumbering alone is not).
 */
changed: boolean, source: SourceDiff, 
/**
 * By name; every board of either side, or the one asked for.
 */
boards: Array<CompareBoard>, 
/**
 * The problems that name no board (a mistake in a module, a file that did not read).
 */
problems: ProblemsDiff, };

export type ClientMessage = { "type": "check", id: number, path: string, text: string, };

export type ServerMessage = { "type": "rebuilt", analysis: Analysis, elapsedMs?: number, } | { "type": "analysing", since: number, } | { "type": "checked", id: number, analysis: Analysis, } | { "type": "progress", task: string, board: string, message: string, stage?: string, done?: number, total?: number, };
