//! The types of `GET /api/git`, `GET /api/compare` and `GET /api/git/file`: what the project
//! can be compared with, and what changed in the circuit since (ADR 0059).
//!
//! Every list is sorted (boards by name; parts by path; nets by name; rails by net; problems by
//! severity, board and message) and keyed by a path or a name the page already knows, so a
//! drawing can be marked with it: `data-ref` by designator (`designatorHead`, `designatorBase`),
//! and the PCB by `data-path`. A field that says nothing is left out of the JSON, not `null`.
//!
//! **Why the circuit and not the lines.** In a code-first tool the design lives in git, so the
//! question about a branch is "what did it change in the circuit?" and not "which lines?". The
//! comparison reduces the project at a git ref (the base) and the files on disk (the head), or
//! a second ref, to parts, nets, rails, ports, systems, the PCB, the bill of materials and
//! problems, and says what differs. It is the one view that is about two readings of the
//! project; every other endpoint describes the last one. The identity rules, the sentence and
//! the defaults the owner has not reviewed are in `crates/netsmith/src/editor/diff.rs`; how a
//! ref is read, kept and limited is in `git.rs` and `compare.rs` beside it.
//!
//! **The requests.**
//!
//! | Request | Answer |
//! |---|---|
//! | `GET /api/git` | [`GitInfo`]: `available` (and `reason` when not: not in a work tree, git not installed), `root`, `projectDir`, `branch`, `head`, `dirty`, `projectDirty`, `refs` (HEAD, the upstream, the default branch, up to 50 local branches by recent commit, up to 30 tags newest first, each once, with its commit) and the last 30 `commits`. Needs no analysis. |
//! | `GET /api/compare?ref=<rev>&board=<name>` | [`Compare`]: `ref` as asked, `base` (the commit), `head` (`commit`, `dirty`, `errors`), `error` and `baseProblems` when the ref cannot be compared, `cached`, `elapsedMs`, a one-line `summary`, `changed`, `source`, `boards` (by name), `systems` and the `problems` that name no board. `404` for a ref that is not a commit, `400` for one that is not a name, `422` with `git_unavailable` for a project not in a work tree. `board` limits `boards` to one (`404` if neither side has it), and a board asked for alone has no systems. It waits for the first analysis, as the views do, and is told `503` pending to a request that asks (`X-Netsmith-Pending`). |
//! | `GET /api/compare?ref=A&against=B` | The same, with the commit `A` as the base and `B` as the head instead of the files on disk. Both names are resolved before anything is read, so a name that is no commit is a `404` (a hostile one a `400`) at once. `against` is in the answer, `head.commit` is `B`, `source` is `git diff --numstat A B` with no untracked files, and the `N errors on disk:` prefix of the sentence is not written. A side that does not analyse ends the answer at once, as a ref does. |
//! | `GET /api/git/file?ref=<rev>&path=<path>` | [`FileContent`] with `readonly` set and the commit's hash as its stamp; a path inside the project that ends `.nsm`, `.toml` or `.csv`, else `403`. With `ref=:worktree` the file is read from disk (a regular file, never a link), with the stamp `worktree`; `:worktree` is no name a ref can have (git refuses a colon). The editor does not list a board's layout file among its files, so `GET /api/file` answers `404` for it, and a comparison asks for the files it does not have through this route. |
//!
//! `source` has the numbers of `git diff --stat` for the project's folder between the commit and
//! the files on disk (`--numstat --no-renames`, untracked files counted as all added), per file
//! and in all.
//!
//! **Progress.** The comparison is one request answered when it is done, as `fab` is, and the
//! socket carries `progress` with `task: "compare"` and `board` the ref as asked, with `stage`
//! (`extract`, then the analysis's `check`, `solve`, `design`, `draw`, `modules`, then `diff`)
//! and `done` of `total` of that stage. For two refs the stages of each ref's reading come in
//! turn, then `diff`. A start and a poll was rejected: the long jobs of the editor (`fab`,
//! `route`) are one request with progress on the socket, and a compare is one more.
//!
//! **Limits.** Panels are not compared, and the schematic of the base is not drawn (the page
//! marks the head's drawing). The scratch folder of a server killed in the middle of a compare
//! is left until the next editor starts.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{Budget, Location, PcbBox, PcbSide, Problem, Range};

/// A commit, as `git log` has it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GitCommit {
    /// The full hash: what a compare is cached by.
    pub hash: String,
    pub short: String,
    pub subject: String,
    pub author: String,
    /// The author date, ISO 8601 with its offset (`2026-11-04T09:30:00+01:00`).
    pub date: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum GitRefKind {
    /// `HEAD`.
    Head,
    /// The upstream of the current branch (`origin/feature`).
    Upstream,
    /// The default branch: `origin/HEAD`'s, else `origin/main`, `origin/master`, `main` or
    /// `master`, whichever is there first.
    Default,
    /// A local branch, the most recently committed first.
    Branch,
    /// A tag, the newest first.
    Tag,
}

/// A name `GET /api/compare?ref=` takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GitRef {
    /// What to pass as `ref`: `HEAD`, `origin/main`, `feature`, `v1.2`.
    pub name: String,
    pub kind: GitRefKind,
    /// The commit it points at (a tag's own commit, not the tag object).
    pub hash: String,
    pub short: String,
    pub subject: String,
    pub date: String,
    /// The branch that is checked out.
    pub current: bool,
}

/// `GET /api/git`: whether the project is in a git work tree, and what it can be compared
/// with. When it is not, `available` is false, `reason` says why in a sentence, and the
/// lists are empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GitInfo {
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
    /// The top of the work tree, an absolute path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub root: Option<String>,
    /// Where the project is in the work tree, with `/`: empty if it is the top, else
    /// `hardware/board`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project_dir: Option<String>,
    /// The checked-out branch; absent when HEAD is detached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub head: Option<GitCommit>,
    /// Anything in the work tree differs from HEAD, untracked files included.
    pub dirty: bool,
    /// The same for the project's folder: what a compare against HEAD sees.
    pub project_dirty: bool,
    /// `HEAD`, the upstream, the default branch, the local branches (50 at most) and the
    /// tags (30 at most), in that order; a name that comes twice is listed once.
    pub refs: Vec<GitRef>,
    /// The last 30 commits reachable from HEAD, newest first.
    pub commits: Vec<GitCommit>,
}

/// What the live project is, for a compare.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompareHead {
    /// HEAD, if the project is in a work tree that has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub commit: Option<GitCommit>,
    /// The project's folder has changes that are not committed: the files on disk are what is
    /// compared, so they are in the diff.
    pub dirty: bool,
    /// How many errors the files on disk have now. While there are some, a board that does
    /// not build is shown as of its last good reading (`CompareBoard.stale`).
    pub errors: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum SourceStatus {
    Modified,
    Added,
    Deleted,
    /// Not in git yet (and not ignored).
    Untracked,
}

/// One file of the project's folder that differs from the ref.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SourceFile {
    /// Relative to the project's folder, with `/`.
    pub path: String,
    pub status: SourceStatus,
    pub added: u32,
    pub removed: u32,
}

/// `git diff --stat` for the project's folder, ref against the files on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SourceDiff {
    pub files_changed: u32,
    pub added: u32,
    pub removed: u32,
    pub files: Vec<SourceFile>,
    /// Why the numbers are missing, if git could not give them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum BoardStatus {
    /// Only in the files on disk.
    Added,
    /// Only at the ref.
    Removed,
    Changed,
    Unchanged,
}

/// A part, by its path in the board (`stage.regulator`): the same part before and after,
/// whatever its designator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComparePart {
    pub path: String,
    pub designator: String,
    pub kind: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub package: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lcsc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mpn: Option<String>,
    /// `fab`, `hand` or `none` (ADR 0024).
    pub assembly: String,
    /// The declaration: on the head side for an added part, on the base side for a removed one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Location>,
}

/// One field that differs. `field` is `kind`, `value`, `package`, `lcsc`, `mpn`, `assembly`,
/// `designator` or `path`; an absent side is a part that did not have it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompareField {
    pub field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub old: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub new: Option<String>,
}

/// A part that is on both sides and differs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComparePartChange {
    /// Its path on the head side.
    pub path: String,
    /// Its path at the ref, when the part moved (a module was renamed or the part now sits in
    /// another): then `fields` has a `path` entry too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base_path: Option<String>,
    pub designator_base: String,
    pub designator_head: String,
    /// What the part is on the head side: `Capacitor`, `Led`, `Header1x2` (the library part it
    /// is made from). A changed row can say what it is.
    pub kind: String,
    /// What it was at the ref, when that is another kind: the part is a different library part,
    /// not the same one with another value (and `fields` has a `kind` entry). Absent when the
    /// kind is the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base_kind: Option<String>,
    /// In a fixed order: `path`, `kind`, `value`, `package`, `lcsc`, `mpn`, `assembly`,
    /// `designator`. Never empty; a part in `renumbered` has only `designator`.
    pub fields: Vec<CompareField>,
    /// Its declaration on the head side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Location>,
    /// Its declaration at the ref.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base_at: Option<Location>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PartsDiff {
    pub added: Vec<ComparePart>,
    pub removed: Vec<ComparePart>,
    /// Something other than the designator differs (a moved part is here).
    pub changed: Vec<ComparePartChange>,
    /// Only the designator differs: adding a part renumbers the ones after it, so this is
    /// long and mostly noise.
    pub renumbered: Vec<ComparePartChange>,
    pub unchanged: u32,
}

/// A pin of a part on a net.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComparePin {
    /// The part's path on the side the pin is listed from.
    pub part: String,
    pub designator: String,
    pub pin: String,
    /// The part's declaration on that side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Location>,
}

/// A net that is on one side only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompareNet {
    pub name: String,
    /// The design names it (`3V3`), as against `net-r1.1`, which netsmith names after the
    /// smallest pin it joins.
    pub named: bool,
    pub pins: Vec<ComparePin>,
    /// The first pin's part, where the net can be pointed at in the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Location>,
}

/// A net that is on both sides and differs: its name, its pins, or both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompareNetChange {
    /// The name on the head side.
    pub name: String,
    /// The name at the ref.
    pub base_name: String,
    /// The names differ, and at least one is a name the design gave. (A change between two
    /// generated names follows from the pins and is listed as `changed`.)
    pub renamed: bool,
    /// Pins on the head side that the net did not have at the ref, with the head side's
    /// designators.
    pub joined: Vec<ComparePin>,
    /// Pins the net had at the ref and has no more, with the ref's designators.
    pub left: Vec<ComparePin>,
    /// The first joined pin's part, else the first that left (on its own side).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Location>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NetsDiff {
    pub added: Vec<CompareNet>,
    pub removed: Vec<CompareNet>,
    /// Same pins (or most of them) under another name.
    pub renamed: Vec<CompareNetChange>,
    /// Same name, other pins.
    pub changed: Vec<CompareNetChange>,
    pub unchanged: u32,
}

/// A rail: the power tree's node for a net a rail draws from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompareRail {
    pub net: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub voltage: Option<Range>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub demand: Option<Range>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub capacity: Option<Range>,
    pub budget: Budget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Location>,
}

/// A quantity before and after; an absent side was not settled (or the rail was not there).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RangeChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base: Option<Range>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub head: Option<Range>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BudgetChange {
    pub base: Budget,
    pub head: Budget,
}

/// A rail on both sides that differs. Only what differs is present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompareRailChange {
    /// The net's name on the head side.
    pub net: String,
    /// Its name at the ref, when the net was renamed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base_net: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub voltage: Option<RangeChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub demand: Option<RangeChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub capacity: Option<RangeChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub budget: Option<BudgetChange>,
    /// Amperes the supply can give above the largest demand (`capacity.hi - demand.hi`),
    /// when both are known on that side. Shrinking is the thing to look at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub headroom_base: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub headroom_head: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Location>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base_at: Option<Location>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RailsDiff {
    pub added: Vec<CompareRail>,
    pub removed: Vec<CompareRail>,
    pub changed: Vec<CompareRailChange>,
    pub unchanged: u32,
}

/// A module instance or an interface instance (a port of the board or of a module), by path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompareInstance {
    pub path: String,
    /// The definition it was made from: `LdoStage`, `Rail`.
    pub type_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Location>,
}

/// An instance at the same path that is made from another definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompareInstanceChange {
    pub path: String,
    pub base_type: String,
    pub head_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Location>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InstancesDiff {
    pub added: Vec<CompareInstance>,
    pub removed: Vec<CompareInstance>,
    pub changed: Vec<CompareInstanceChange>,
}

/// A line of the bill of materials, as `GET /api/bom` groups them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompareBomLine {
    pub kind: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub package: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub footprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lcsc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mpn: Option<String>,
    pub hand_solder: bool,
    pub quantity: u32,
    pub designators: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unit_usd: Option<f64>,
}

/// A line on both sides (same kind, value, footprint, LCSC number, MPN and assembly) whose
/// quantity or unit price differs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompareBomChange {
    pub kind: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub package: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub footprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lcsc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mpn: Option<String>,
    pub hand_solder: bool,
    pub quantity_base: u32,
    pub quantity_head: u32,
    pub designators_base: Vec<String>,
    pub designators_head: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unit_usd_base: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unit_usd_head: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BomDiff {
    pub added: Vec<CompareBomLine>,
    pub removed: Vec<CompareBomLine>,
    pub changed: Vec<CompareBomChange>,
    pub unchanged: u32,
    /// The total of the priced lines for one board, as `GET /api/bom` has it, on each side.
    /// Prices are those of each side's own catalogs and the supply file as they are now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub total_usd_base: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub total_usd_head: Option<f64>,
    /// `total_usd_head - total_usd_base`, when both are known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delta_usd: Option<f64>,
}

/// What the problems list does between the two sides. A problem is the same on both sides
/// when its severity, stage, board, instance path and message are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProblemsDiff {
    /// On the head side only, with the head side's `at`.
    pub new: Vec<Problem>,
    /// At the ref only, with the ref's `at`.
    pub resolved: Vec<Problem>,
    pub unchanged: u32,
}

/// One board (or a board of a system, `Demo/main`) on either side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompareBoard {
    /// As `BoardSummary.name` has it: what `?board=` and the other endpoints take.
    pub name: String,
    pub status: BoardStatus,
    /// The source file of the board on each side, absent on the side that has no such board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub head_file: Option<String>,
    /// The head side is the last good reading of a file that does not build now.
    pub stale: bool,
    pub summary: String,
    pub parts: PartsDiff,
    pub nets: NetsDiff,
    pub rails: RailsDiff,
    /// Module instances by path.
    pub modules: InstancesDiff,
    /// Interface instances that are ports of the board or of a module, by path.
    pub interfaces: InstancesDiff,
    /// Absent when the bill of materials cannot be made on one side (`bom_error` says why).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bom: Option<BomDiff>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bom_error: Option<String>,
    pub problems: ProblemsDiff,
    /// What changed on the PCB: where the parts are, the outline, the layers, the keepouts and
    /// cutouts. Absent when the board has no PCB on either side (it has no process) or
    /// `pcb_error` says it cannot be compared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pcb: Option<PcbDiff>,
    /// Why the PCB is not compared: the board is placed on one side only, or cannot be placed on
    /// one of them (a footprint is missing), and a comparison of the other side alone would say
    /// nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pcb_error: Option<String>,
}

/// A part's place on the PCB: millimetres from the outline's top left corner, as the layout file
/// has them (`PartPlacement`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComparePcbPlace {
    pub x: f64,
    pub y: f64,
    /// Degrees, counterclockwise on screen.
    pub rotation: f64,
    pub side: PcbSide,
    pub locked: bool,
    /// `derived` when the layout file has no entry for the part (the placer chose), else who
    /// wrote the entry: `hand`, `auto` or `regen` (`PcbPart.placement`).
    pub placement: String,
}

/// A part that is on both sides and is somewhere else, turned, on the other face or locked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComparePcbPartChange {
    /// Its path on the head side.
    pub path: String,
    /// Its path at the ref, when the part moved in the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base_path: Option<String>,
    pub designator_base: String,
    pub designator_head: String,
    pub base: ComparePcbPlace,
    pub head: ComparePcbPlace,
    /// In a fixed order, only what differs: `position` (`12.5, 3 mm`), `rotation`, `side`,
    /// `locked`.
    pub fields: Vec<CompareField>,
    /// How far the origin moved, in millimetres (0 when it did not).
    pub distance_mm: f64,
}

/// The board's size, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct CompareSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct OutlineChange {
    pub base: CompareSize,
    pub head: CompareSize,
}

/// The copper layers, top first, on each side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LayersChange {
    pub base: Vec<String>,
    pub head: Vec<String>,
}

/// A keepout or a cutout that is on one side only, or whose area changed. Areas are in the
/// frame of the outline's top left corner, as the parts are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComparePcbArea {
    pub name: String,
    /// `added`, `removed` or `changed`.
    pub status: BoardStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base: Option<PcbBox>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub head: Option<PcbBox>,
}

/// What differs on the PCB of a board between the two sides (the vocabulary of the PCB view:
/// `PcbPart`, `PcbKeepout`, `PcbCutout`, the outline and the layers of `PcbScene`). A part that
/// the placer chose on both sides (`derived`) is not listed: it moves with every part added,
/// and that is not something the design said.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbDiff {
    /// The layout file of the board, as the project names it (`src/main.pcb.toml`), which the
    /// page can open at both refs.
    pub layout_file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub outline: Option<OutlineChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub layers: Option<LayersChange>,
    pub moved: Vec<ComparePcbPartChange>,
    pub keepouts: Vec<ComparePcbArea>,
    pub cutouts: Vec<ComparePcbArea>,
    /// Parts on both sides that are where they were.
    pub unchanged: u32,
}

/// A port of a board of a system (`SystemNode.ports`): its voltage and current as the System
/// view prints them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompareSystemPort {
    /// The member of the system: `power`.
    pub board: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub voltage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub current: Option<String>,
}

/// A port on both sides whose voltage or current differs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompareSystemPortChange {
    pub board: String,
    pub name: String,
    /// `voltage`, `current`, in that order, only what differs; an absent side had none.
    pub fields: Vec<CompareField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SystemPortsDiff {
    pub added: Vec<CompareSystemPort>,
    pub removed: Vec<CompareSystemPort>,
    pub changed: Vec<CompareSystemPortChange>,
}

/// Texts that are on one side only: the joins (`power.output ~ led.input`) and the ground groups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NamesDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

/// Two connectors a system mates, by the paths of the two ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompareMate {
    pub a: String,
    pub b: String,
    /// The ports it joins.
    pub ports: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Location>,
}

/// A mate on both sides whose contacts or ports differ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompareMateChange {
    pub a: String,
    pub b: String,
    /// One per contact that differs, `contact 3`: what each side had behind it (`ground`,
    /// `power on 3V3`, `signal on SDA`, `open`), then the ports that joined and left as `ports`.
    pub fields: Vec<CompareField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Location>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MatesDiff {
    pub added: Vec<CompareMate>,
    pub removed: Vec<CompareMate>,
    pub changed: Vec<CompareMateChange>,
}

/// One system of the project on either side, by its name (the System view's).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompareSystem {
    pub name: String,
    pub status: BoardStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub head_file: Option<String>,
    pub summary: String,
    /// The boards it holds, by their member name (`led`); the type is the board's definition.
    pub boards: InstancesDiff,
    pub ports: SystemPortsDiff,
    pub joins: NamesDiff,
    pub mates: MatesDiff,
    pub ground_groups: NamesDiff,
    /// The rails of the system's power tree that no board's own comparison already lists: what a
    /// join gives a rail (its capacity, so its budget) differs from one side to the other. A
    /// rail is `led:3V3` (the member, the net).
    pub rails: RailsDiff,
}

/// `GET /api/compare?ref=<rev>&board=<name>`: what changed in the circuit between `ref` and
/// the files on disk.
///
/// A ref that does not exist is a `404` and a project that is not in git a `422` with the
/// code `git_unavailable`. A project that does not analyse at the ref is a `200` whose
/// `error` says so and whose `baseProblems` are what the compiler said; the lists are empty.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Compare {
    /// The ref as asked.
    #[serde(rename = "ref")]
    pub rev: String,
    /// The second ref, when the comparison is of two refs (`?against=`): then `head.commit` is
    /// the commit it resolved to, `head.dirty` is false and the files on disk are not read.
    /// Absent when the ref is compared with the files on disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub against: Option<String>,
    /// What it resolved to.
    pub base: GitCommit,
    pub head: CompareHead,
    /// Set when the project at the ref cannot be compared: it has no `netsmith.toml` there, or
    /// it does not build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    /// The errors of the analysis at the ref, when `error` is set because of them.
    pub base_problems: Vec<Problem>,
    /// The analysis of the ref was kept from an earlier compare (by commit hash).
    pub cached: bool,
    /// The whole request, in milliseconds.
    pub elapsed_ms: u32,
    /// One line: `3 parts added, 1 removed, 2 values changed, 1 rail's headroom shrank from
    /// 80 mA to 20 mA`; `No circuit changes` when there are none.
    pub summary: String,
    /// Whether anything in the circuit differs (a renumbering alone is not).
    pub changed: bool,
    pub source: SourceDiff,
    /// By name; every board of either side, or the one asked for.
    pub boards: Vec<CompareBoard>,
    /// The systems of either side, by name; empty when a board is asked for.
    pub systems: Vec<CompareSystem>,
    /// The problems that name no board (a mistake in a module, a file that did not read).
    pub problems: ProblemsDiff,
}
