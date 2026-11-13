// Generated from crates/netsmith-editor/src/api.rs. Do not edit: run
// `NETSMITH_WRITE_API_TS=1 cargo test -p netsmith-editor` after changing the Rust types.

export type Location = { file: string, line: number, column: number, endLine: number, endColumn: number, };

export type Severity = "error" | "warning" | "note";

export type ProblemStage = "syntax" | "check" | "solve" | "design" | "guideline" | "power" | "layout";

export type Cause = "not_stated" | "part_model" | "unjudged" | "circular" | "supply" | "pick" | "track" | "other";

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
 * system), where the problem belongs to one board: every solver, design-stage and guideline
 * message and note, and a checker error written inside the definition of a board. A
 * parse error, and a mistake in a module or a system (which boards share), have none.
 */
board?: string, 
/**
 * The instance it is about, from the board (`status_indicator.led`), where the server
 * knows one: a note, a not-chosen warning and a guideline finding. The mistakes of the
 * checker and the solver have none: they are about a statement, which `at` locates.
 */
path?: string, 
/**
 * Why a note is there, which the dock groups the notes by. Set on every note of a
 * build (the tool that made it says it); a problem that is not a note has none.
 */
cause?: Cause, };

export type HintFault = { 
/**
 * 1 for the first line of the file.
 */
line: number, message: string, };

export type HintFindingKind = "stale_sheet" | "stale_entry" | "wrong_sheet" | "unknown_board" | "unusable";

export type HintFindingInfo = { kind: HintFindingKind, 
/**
 * The line of the entry's table header; 0 if it has none.
 */
line: number, 
/**
 * The entry as the file names it: `[Trellis.sheets.stage.parts."output_cap"]`.
 */
entry: string, 
/**
 * What is wrong with it, ending with what happens to it.
 */
message: string, };

export type HintPart = { 
/**
 * Moved by this much from where the automatic drawing puts it.
 */
dx?: number, dy?: number, 
/**
 * Instead of `dx` and `dy`: the block's origin at this place from the sheet's reference
 * part.
 */
pin?: [number, number], 
/**
 * 0, 90, 180 or 270 degrees counterclockwise, replacing the automatic turn.
 */
rotate?: number, 
/**
 * Flipped left to right before the turn, replacing the automatic mirror.
 */
mirror?: boolean, 
/**
 * The line of the entry's table header in the file.
 */
line: number, };

export type HintNote = { dx: number, dy: number, line: number, };

export type HintSheet = { parts: { [key in string]: HintPart }, notes: { [key in string]: HintNote }, };

export type HintBoard = { parts: { [key in string]: HintPart }, notes: { [key in string]: HintNote }, sheets: { [key in string]: HintSheet }, };

export type SchematicHints = { 
/**
 * The board asked for, as `BoardSummary.name` has it.
 */
board: string, 
/**
 * The file's name in the project (`src/main.sch.toml`): where the file is or goes.
 */
file: string, 
/**
 * Whether the file is on disk. Without it there are no hints and nothing is wrong.
 */
exists: boolean, 
/**
 * What the file was like when read: a `PUT` that sends it back is refused (`409`) if the
 * file is different now. `none` for a file that is not there.
 */
stamp: string, 
/**
 * The file as it is on disk; empty without one.
 */
text: string, 
/**
 * What the text says wrong, in the order of the file. When there is any the file is
 * ignored: `boards` is empty and the drawing is the one there is without a file.
 */
errors: Array<HintFault>, 
/**
 * The tables of the file, by the board's definition name, as the text says them.
 */
boards: { [key in string]: HintBoard }, 
/**
 * The entries not used for this board, and the tables for boards the source lacks, in the
 * order of the file. The same notes are in the problems list.
 */
findings: Array<HintFindingInfo>, 
/**
 * The version of the analysis the drawing and the findings are of (`Analysis.version`).
 */
version: bigint, };

export type FileRole = "top_level" | "module" | "std" | "manifest" | "physical" | "schematic_layout";

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

export type DefChild = { 
/**
 * As written, without the leading dot (`stage.regulator` for a path).
 */
name: string, 
/**
 * What the type is: `board`, `system`, `panel`, `part`, `interface` or `module`. Left
 * out when the type names no definition that the file or its imports reach.
 */
kind?: string, 
/**
 * The line the declaration starts on.
 */
line: number, 
/**
 * The line it ends on; the same as `line` for a declaration with no block.
 */
endLine: number, 
/**
 * The type as written, last segment only: `LinearRegulator` for `std::parts::LinearRegulator`.
 */
typeName: string, };

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
 * The hash of the page embedded in this binary: a hash of the sources in `web/` that the
 * page was built from. Absent when the binary has only the placeholder page (it was
 * built without Node).
 */
bundleHash?: string, };

export type FileContent = { path: string, text: string, 
/**
 * What the file was like when read; a save must give it back.
 */
stamp: string, readonly: boolean, };

export type Saved = { stamp: string, };

export type PinLock = { 
/**
 * The board as the page names it (`GET /api/board?name=`).
 */
board: string, 
/**
 * The request's path, the key of its entry in the pin file (`status`).
 */
request: string, locked: boolean, };

export type PinMove = { board: string, request: string, 
/**
 * The pin's path in the part (`p03`).
 */
pin: string, };

export type PinWritten = { 
/**
 * The pin file, as the project names it (`src/main.pins.toml`).
 */
file: string, };

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

export type ModuleUse = { 
/**
 * The module's name: `LdoStage`.
 */
name: string, 
/**
 * The file it is defined in, as the tree names it, and the line of its definition
 * (`Def::line`).
 */
file: string, line: number, 
/**
 * The boards that have an instance of it, as `BoardSummary::name` has them
 * (`Demo/main` for a board of a system), in the order of `Analysis::boards`.
 */
usedBy: Array<string>, };

export type Analysis = { version: number, errors: number, warnings: number, notes: number, problems: Array<Problem>, boards: Array<BoardSummary>, systems: Array<SystemSummary>, panels: Array<PanelSummary>, 
/**
 * For each module definition that some board uses, directly or inside another module,
 * the boards that use it, so a declaration in a shared module can show its card for
 * each board. Sorted by file and line; left out when no board uses a module. It is here
 * and not on `Def` because `GET /api/project` is answered before the first analysis.
 */
moduleUses?: Array<ModuleUse>, 
/**
 * How long the analysis took, in milliseconds: of the files on disk for `/api/analysis`
 * and `rebuilt`, of the text on the page for `checked`.
 */
elapsedMs?: number, 
/**
 * The last analysis of the files on disk was cancelled (ADR 0062): this is the reading
 * before it (or, for the first, an empty one with a note), which does not show the files as
 * they are. It stays so until a file changes or `POST /api/analysis/run`.
 */
cancelled?: boolean, 
/**
 * A file on disk changed and was not analysed, because the page asked for no analysis on
 * change (`POST /api/analysis/auto`): this is the reading before it. It stays so until
 * `POST /api/analysis/run`.
 */
stale?: boolean, };

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
scopes: Array<Location>, 
/**
 * What the solver worked out for the part and the source did not state. Absent when
 * there is nothing to say (a part with no catalog row and no derived value).
 */
solved?: Solved, 
/**
 * The pin requests that ask this part for a pin (`.status: .u.GPIO`, ADR 0101), in the byte
 * order of their paths, each with the pin it got or the message of the check when it got
 * none. Absent for a part no request asks. What the editor shows of the pin file: it
 * computes the assignment in memory; the page's padlock and move write only the `lock`, `pin` and
 * `source` lines of one entry (`POST /api/pins/lock`, `/api/pins/move`).
 */
pinRequests?: Array<PinRequestInfo>, };

export type PinRequestInfo = { 
/**
 * The request's path in the board, the key of its entry in the pin file (`status`).
 */
request: string, 
/**
 * The function it asks the part for, which the pin offers and the file records (`GPIO`).
 */
function: string, 
/**
 * The pin it got, by its path in the part (`p02`); absent when it got none.
 */
pin?: string, 
/**
 * The pad of that pin in the package, as the board's design has it (`2`); several pads
 * are joined with a comma. Absent when the pin has no pad in the design.
 */
pad?: string, 
/**
 * Whether the entry is locked (`lock = true`): the optimizer never moves it.
 */
locked: boolean, 
/**
 * The entry's `source` in the pin file: `hand` or `auto`.
 */
entry?: string, 
/**
 * Where the pin came from: `file` (the pin file beside the source holds this entry and it
 * still holds) or `run` (picked now, from the source's request, with no entry in the file
 * yet: `build` would write it).
 */
from?: string, 
/**
 * Why the request has no pin: the message the check gave, in its own words. Absent when
 * the request has one.
 */
message?: string, 
/**
 * The pins the request may be moved to, which is what the page offers to move it: every
 * pin of the part that offers the function and that the source does not hold (the one it
 * has now is among them), in the byte order of the paths. Empty for a request with no pin.
 */
offers: Array<PinOffer>, };

export type PinOffer = { 
/**
 * The pin's path in the part (`p03`).
 */
pin: string, 
/**
 * Its pad in the package, as the design has it; absent when it has none.
 */
pad?: string, 
/**
 * The request that has this pin now, which a move to it is refused for; absent for a free
 * pin.
 */
heldBy?: string, };

export type Solved = { 
/**
 * A capacitor's capacitance in use: the marked value after the DC bias across it and
 * after its tolerance (`1.72uF..2.916uF` for a 10 uF part at 5 V). Present where the
 * bias derating applies, that is when the part has a curve and a bias.
 */
effectiveCapacitance?: Range, 
/**
 * The DC voltage across the part that the derating used (taken from the rail the part
 * sits across, ADR 0013). Present with `effective_capacitance`.
 */
bias?: Range, 
/**
 * The tolerance of the resistance, capacitance or inductance the solver chose, when the
 * source did not state the value's range.
 */
tolerance?: Tolerance, 
/**
 * The ratings of the row that was picked (`max_voltage`, `max_current`, `max_power`,
 * `rated_current`, `saturation_current`, ...), the voltages, currents and powers the
 * part is built for, in the order its definition lists them.
 */
ratings?: Array<Rating>, 
/**
 * The catalog row that stands for the part.
 */
row?: SolvedRow, };

export type Tolerance = { 
/**
 * `20` for `+/- 20%`, rounded to three decimals.
 */
percent: number, 
/**
 * `+/- 20%`.
 */
text: string, };

export type Rating = { 
/**
 * The field of the part: `max_voltage`.
 */
name: string, range: Range, };

export type SolvedRow = { 
/**
 * The catalog table the part's definition draws from (`resistor`, `ldo`), if it names one.
 */
catalog?: string, 
/**
 * The manufacturer's part number.
 */
mpn?: string, 
/**
 * The LCSC number.
 */
lcsc?: string, 
/**
 * `Basic`, `Extended` or `Preferred`.
 */
library?: string, 
/**
 * The solver chose only a value and a package (ADR 0047) and no row stands for the part
 * yet; `mpn`, `lcsc` and `library` are then absent.
 */
generic: boolean, };

export type PinInfo = { refdes: string, 
/**
 * The pad number as the symbol names it (`"1"`, `"A5"`).
 */
pin: string, 
/**
 * The name the part's datasheet prints for the pin (`VIN`, `EN`, `SW`), where the part
 * has a pin table that lists the pad. Absent for a part with no pin table.
 */
name?: string, 
/**
 * The function the board chose for the pin (`UART1_TX`), when the pin can be several
 * things and the board picked one. Absent otherwise.
 */
function?: string, };

export type NetInfo = { name: string, named: boolean, pins: Array<PinInfo>, };

export type BoardDetail = { name: string, file: string, parts: Array<PartInfo>, nets: Array<NetInfo>, notes: Array<string>, 
/**
 * The pick lock beside the board's source file (ADR 0100): whether it exists and is
 * current, read-only. Absent when there is nothing to compare, because the source does not
 * build (the problems list says why).
 */
lock?: PickLockInfo, };

export type PickLockInfo = { 
/**
 * The lock file, named as the files of the project are (`src/main.lock.toml`).
 */
file: string, 
/**
 * Whether the file exists. A source with no part to lock needs none.
 */
exists: boolean, 
/**
 * Whether the lock is as `netsmith lock --update` would write it.
 */
current: boolean, 
/**
 * What `lock --check` prints when it is not: one line for each part whose row changed, was
 * added or was dropped. Empty when the file does not exist: `--check` prints no lines then.
 */
diff: Array<string>, 
/**
 * What the solver said when it held the parts to the rows of the file as it is: the rows
 * that no longer pass or are no longer in a catalog, in its own words. Empty when every
 * locked row still holds.
 */
reported: string, 
/**
 * Why the lock could not be checked: the file is not a lock this netsmith reads (its own
 * words, with the place in it). Then the other fields say nothing of the rows.
 */
error?: string, };

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
stages: Array<Stage>, 
/**
 * The rails and converters with trouble, worst first: what `netsmith build` and the
 * analysis report as the `power` stage. Computed here, not by the page.
 */
issues: Array<PowerIssue>, };

export type PowerIssue = { 
/**
 * The node it is on: a rail's id, or `stage:<label>` for a converter.
 */
id: string, 
/**
 * `Error` for an overdrawn supply, `Note` for the rest.
 */
severity: Severity, 
/**
 * What it is about: the rail's net (`V3V3`), or the converter's name (`ldo`).
 */
subject: string, 
/**
 * What is wrong, to read after the subject: `has no source: nothing supplies it`.
 */
text: string, 
/**
 * The place in the source to look at: the node's, else its first draw's; a converter's
 * declaration.
 */
at?: Location, 
/**
 * The board of a system it is on, as `PowerNode::board` has it; none for a stand-alone
 * board.
 */
board?: string, };

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
 * `3.135V..3.465V` (see `Range::text`), if the port is a rail.
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
handSolder: boolean, 
/**
 * The generic this part was picked to provide (`CoaxReceptacle`, ADR 0068): `kind` is
 * the provider's name. `None` for a part written by its own type.
 */
via: string | null, };

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
placement: string, courtyard: PcbBox, 
/**
 * What stands above the board: the footprint's fabrication outline, else its courtyard
 * (the box a height limit is measured on, ADR 0045 section 11).
 */
body: PcbBox, 
/**
 * How tall the body stands, in mm, if the design states it (never a guess: `None` is
 * unknown).
 */
height: number | null, pads: Array<PcbPad>, };

export type KeepoutOrigin = "source" | "placed" | "drawn" | "part";

export type PcbKeepout = { name: string, origin: KeepoutOrigin, 
/**
 * Where it is on the sheet, if it has a place.
 */
area: PcbBox | null, width: number, height: number, layers: Array<string>, tracks: boolean, vias: boolean, copper: boolean, parts: boolean, 
/**
 * The tallest a part may stand where it is, in mm, on the faces in `layers`.
 */
maxHeight: number | null, };

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
 * The keepout the finding is about, by name, where it is about one (a part in it, or too
 * tall for its limit): the page lights it when the finding is picked.
 */
keepout?: string, 
/**
 * The line of a physical file (or of the circuit source) that states the requirement the
 * finding quotes, where it quotes one; the first of them if it quotes several. A point at
 * the start of the line (`column` and `endColumn` are 1), `file` as the checker names it.
 */
at?: Location, 
/**
 * The case model's solid the finding is about (ADR 0109), as the 3D scene's node names it
 * (`case:<model>:<solid>`), or the model's group (`case:<model>`): the 3D view lights it.
 */
caseSolid?: string, 
/**
 * Where to look in 3D, in mm in the scene's frame (`docs/BOARD3D.md`), for a finding of
 * the case fit check: the 3D view flies there when no part is named.
 */
point3d?: [number, number, number], };

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
 * What the part is on the head side: `Capacitor`, `Led`, `Header1x2` (the library part it
 * is made from). A changed row can say what it is.
 */
kind: string, 
/**
 * What it was at the ref, when that is another kind: the part is a different library part,
 * not the same one with another value (and `fields` has a `kind` entry). Absent when the
 * kind is the same.
 */
baseKind?: string, 
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

export type ComparePcbPlace = { x: number, y: number, 
/**
 * Degrees, counterclockwise on screen.
 */
rotation: number, side: PcbSide, locked: boolean, 
/**
 * `derived` when the layout file has no entry for the part (the placer chose), else who
 * wrote the entry: `hand`, `auto` or `regen` (`PcbPart.placement`).
 */
placement: string, };

export type ComparePcbPartChange = { 
/**
 * Its path on the head side.
 */
path: string, 
/**
 * Its path at the ref, when the part moved in the source.
 */
basePath?: string, designatorBase: string, designatorHead: string, base: ComparePcbPlace, head: ComparePcbPlace, 
/**
 * In a fixed order, only what differs: `position` (`12.5, 3 mm`), `rotation`, `side`,
 * `locked`.
 */
fields: Array<CompareField>, 
/**
 * How far the origin moved, in millimetres (0 when it did not).
 */
distanceMm: number, };

export type CompareSize = { width: number, height: number, };

export type OutlineChange = { base: CompareSize, head: CompareSize, };

export type LayersChange = { base: Array<string>, head: Array<string>, };

export type ComparePcbArea = { name: string, 
/**
 * `added`, `removed` or `changed`.
 */
status: BoardStatus, base?: PcbBox, head?: PcbBox, };

export type PcbDiff = { 
/**
 * The layout file of the board, as the project names it (`src/main.pcb.toml`), which the
 * page can open at both refs.
 */
layoutFile: string, outline?: OutlineChange, layers?: LayersChange, moved: Array<ComparePcbPartChange>, keepouts: Array<ComparePcbArea>, cutouts: Array<ComparePcbArea>, 
/**
 * Parts on both sides that are where they were.
 */
unchanged: number, };

export type CompareSystemPort = { 
/**
 * The member of the system: `power`.
 */
board: string, name: string, voltage?: string, current?: string, };

export type CompareSystemPortChange = { board: string, name: string, 
/**
 * `voltage`, `current`, in that order, only what differs; an absent side had none.
 */
fields: Array<CompareField>, };

export type SystemPortsDiff = { added: Array<CompareSystemPort>, removed: Array<CompareSystemPort>, changed: Array<CompareSystemPortChange>, };

export type NamesDiff = { added: Array<string>, removed: Array<string>, };

export type CompareMate = { a: string, b: string, 
/**
 * The ports it joins.
 */
ports: Array<string>, at?: Location, };

export type CompareMateChange = { a: string, b: string, 
/**
 * One per contact that differs, `contact 3`: what each side had behind it (`ground`,
 * `power on 3V3`, `signal on SDA`, `open`), then the ports that joined and left as `ports`.
 */
fields: Array<CompareField>, at?: Location, };

export type MatesDiff = { added: Array<CompareMate>, removed: Array<CompareMate>, changed: Array<CompareMateChange>, };

export type CompareSystem = { name: string, status: BoardStatus, baseFile?: string, headFile?: string, summary: string, 
/**
 * The boards it holds, by their member name (`led`); the type is the board's definition.
 */
boards: InstancesDiff, ports: SystemPortsDiff, joins: NamesDiff, mates: MatesDiff, groundGroups: NamesDiff, 
/**
 * The rails of the system's power tree that no board's own comparison already lists: what a
 * join gives a rail (its capacity, so its budget) differs from one side to the other. A
 * rail is `led:3V3` (the member, the net).
 */
rails: RailsDiff, };

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
bom?: BomDiff, bomError?: string, problems: ProblemsDiff, 
/**
 * What changed on the PCB: where the parts are, the outline, the layers, the keepouts and
 * cutouts. Absent when the board has no PCB on either side (it has no process) or
 * `pcb_error` says it cannot be compared.
 */
pcb?: PcbDiff, 
/**
 * Why the PCB is not compared: the board is placed on one side only, or cannot be placed on
 * one of them (a footprint is missing), and a comparison of the other side alone would say
 * nothing.
 */
pcbError?: string, };

export type Compare = { 
/**
 * The ref as asked.
 */
ref: string, 
/**
 * The second ref, when the comparison is of two refs (`?against=`): then `head.commit` is
 * the commit it resolved to, `head.dirty` is false and the files on disk are not read.
 * Absent when the ref is compared with the files on disk.
 */
against?: string, 
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
 * The systems of either side, by name; empty when a board is asked for.
 */
systems: Array<CompareSystem>, 
/**
 * The problems that name no board (a mistake in a module, a file that did not read).
 */
problems: ProblemsDiff, };

export type CancelAnswer = { cancelled: boolean, };

export type AutoBuild = { auto: boolean, };

export type RenderView = "top" | "bottom" | "front" | "back" | "left" | "right" | "iso" | "iso-bottom";

export type ColourBy = "provenance" | "kind" | "margin";

export type CameraPlace = { position: [number, number, number], target: [number, number, number], up?: [number, number, number], };

export type PreviewSetting = boolean | number;

export type ViewChange = { 
/**
 * The board, as `BoardSummary::name` has it (`Demo/main` for a board of a system), or a
 * system's name (`Demo`), which shows its System view (ADR 0110): every member placed by
 * its mates. The page's board is then a member of it: the one it is on, else the first.
 */
board?: string, 
/**
 * A view of `render`; with `camera` too, an error.
 */
view?: RenderView, 
/**
 * The camera put at a place (its `up` is not taken).
 */
camera?: CameraPlace, 
/**
 * `x=13`, `y=8` or `z=-0.8` as `render --section` reads it, or `none` for no cut.
 */
section?: string, 
/**
 * What is lit, as `render --highlight` names it (a designator, a path, `net:<name>`,
 * `limit:<name>`, `case:<name>`; an item may be comma-separated): the parts and the net
 * through the page's selection, the limits and the case models in the view. It replaces what
 * was lit; `[]` lights nothing. In the System view a part and a net are a member's, `led/J1`
 * and `net:led/GND` (`led/net:GND` too); one of another member makes it the page's board; a
 * case model is one board's, so `case:` there is refused.
 */
highlight?: Array<string>, 
/**
 * The page's selection, as a click makes it: parts by designator or path, or one
 * `net:<name>` (in the System view, `led/J1` and `net:led/GND`, of one member). The limits
 * and the case models lit stay as they are; `[]` clears it.
 */
select?: Array<string>, xray?: boolean, 
/**
 * `true` for the parallel projection; `perspective: true` is the same as `ortho: false`.
 */
ortho?: boolean, perspective?: boolean, colourBy?: ColourBy, heightPreview?: PreviewSetting, 
/**
 * The stackup pulled apart (`true`) or together.
 */
exploded?: boolean, };

export type ViewState = { 
/**
 * The board the PCB section is on, or the system whose System view the 3D view shows; none
 * when the project has no board.
 */
board: string | null, 
/**
 * In the System view, the member the page's board is (`led`): its parts and nets are named
 * `led/J1` and `net:led/GND`, as every member's are. Absent otherwise.
 */
member?: string, 
/**
 * The 3D view of `board` is on screen and drawn. When false, `view`, `camera`, `section`,
 * `exploded` and `ortho` are absent.
 */
shown: boolean, 
/**
 * The view of `render` the camera is at (within a few degrees), else none.
 */
view?: RenderView, camera?: CameraPlace, ortho?: boolean, 
/**
 * The cut as `render --section` writes it (`x=13`), or none.
 */
section?: string, exploded?: boolean, 
/**
 * What is lit, as `render --highlight` would name it: the parts picked by designator, the
 * net picked as `net:<name>`, the limits lit as `limit:<name>`, the case models lit as
 * `case:<name>` (in the System view, the member first: `led/J1`, `net:led/GND`).
 */
highlight: Array<string>, 
/**
 * The page's selection: the parts by designator, or one `net:<name>`.
 */
selection: Array<string>, 
/**
 * What the pointer (or a row of another view) lights now: a designator or `net:<name>`.
 */
hover?: string, xray: boolean, colourBy: ColourBy, 
/**
 * The height preview's height in mm while it is on, else none.
 */
heightPreview?: number, 
/**
 * The page's palette, as `render --theme` names one: `light` or `dark`. Read-only.
 */
theme: string, 
/**
 * The `netsmith render` command that draws what the view shows, as "Copy as command" has
 * it; absent when the view is not shown.
 */
command?: string, };

export type ClientMessage = { "type": "check", id: number, path: string, text: string, } | { "type": "focus" } | { "type": "view_state", id: number, state: ViewState, } | { "type": "view_error", id: number, message: string, status?: number, };

export type ServerMessage = { "type": "rebuilt", analysis: Analysis, elapsedMs?: number, } | { "type": "analysing", since: number, } | { "type": "checked", id: number, analysis: Analysis, } | { "type": "progress", task: string, board: string, message: string, stage?: string, done?: number, total?: number, unit?: string, unitDone?: number, unitTotal?: number, } | { "type": "cancelled", task: string, board: string, message: string, } | { "type": "view", id: number, change?: ViewChange, } | { "type": "view_picture", id: number, };
