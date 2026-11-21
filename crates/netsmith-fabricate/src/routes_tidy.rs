//! `netsmith routes tidy` (issue #1192): a pass over the copper of the routes file (and, with
//! `--reuse-routes`, over the router's copper kept on disk) that makes the repairs a person makes
//! by hand on imported, hand-laid or scripted copper:
//!
//! - **corners**: two straight tracks of a net that meet outside its pads and bend by more than
//!   45 degrees (the design rules check's `track_angle`, or its `acid_trap` "two tracks meet") are
//!   cut: the corner is replaced by short tracks that each bend by 45 degrees at most, a 45
//!   degree chamfer at a right angle. The chamfer starts [`CHAMFERS`] widths from the corner,
//!   no more than 45 % of the shorter track, and is tried smaller when the larger is refused;
//! - **collinear pairs**: two straight tracks of one net, layer, width and owner (the file or
//!   the kept copper) that meet end to end on one line, with no third track there, become one;
//! - **dangling vias and stubs**: a via the check finds joined to copper of its net on fewer
//!   than two layers (`via_dangling`) and a track with an end that touches nothing
//!   (`track_dangling`) are removed, when the copper on the board makes every connection of the
//!   net (the router may still start from a stub of a net it has to finish), or, for a via, when
//!   it joins nothing at all;
//! - **acid traps** (`acid_trap`): a track that leaves a pad or a via at an acute angle to its
//!   edge is made to leave it straight out, at a right angle, and bend after the edge; a track
//!   that ends on the middle of another at an acute angle is made to meet it at a right angle
//!   and bend after it (the check's own advice, "make it a right angle"). The bend after is cut
//!   as a corner is when it would bend by more than 45 degrees.
//!
//! - **tracks inside a pad, duplicate ways, overlapping copper and runs of tiny tracks** (issue #1485), the
//!   defects of a routed board that the check finds nothing of: a track from a pad's or a via's
//!   centre to a point still inside it is removed, the longer of two ways between the same two
//!   places of a net is removed, a track inside another is removed and two that overlap on one
//!   line become one, and a run of tiny tracks round a corner becomes one track or one corner
//!   ([`shapes`]).
//!
//! With `widen` (`routes tidy --widen`, issue #1294) one more: **narrow tracks**. A straight
//! track narrower than the width its net wants on its layer ([`wanted_width`]: the width the
//! router draws the net at there, for its current, its class or its impedance target) is
//! widened to it. A widening refused by the rule of every change (the wider track comes too
//! near the copper of another net) is tried narrower, at [`WIDENINGS`] of the way from its
//! width to the wanted one, so that each track is made as wide as its neighbours' clearance
//! allows; a track is never made narrower, and one widened part of the way is not tried again.
//!
//! **The rule of every change.** A change is made only when the design rules check of the board
//! with it (the check `routes add-track` makes, [`crate::routes::drc_errors`], with its warnings
//! too) finds nothing it did not find before, and the connections left to route (the ratsnest)
//! are the same: no connection is lost and none made. A finding is the same finding when its
//! rule, severity, message, place and layer are, the lengths of the tracks it names left out (a
//! track made shorter by a chamfer is the same track to a finding at its other end). The changes
//! of a pass are checked together, and a group that is refused is split in two and each half
//! checked again, down to the single change, which is then refused and said with the first new
//! finding or the connection that changed. A refused change is tried again in its other forms
//! (a smaller chamfer, the other side of a pad's edge), then left as it was.
//!
//! **Checked where the change is** (issue #1225). The check of the whole board took seconds on
//! a board of a few thousand tracks, and the pass checked it for every group: half an hour. Now
//! the changes of a pass are taken net by net, and each net's changes are a group, checked
//! together and split in two when refused as before. A group is checked by the check of its
//! neighbourhood ([`near`]): the items within the board's largest space of the copper the
//! changes remove and add, of every net (found in a grid of the board's items built once), with
//! the changes and without them, and the findings with a place in the neighbourhood compared.
//! The connections are judged there too: when the changes leave each net with as many pieces
//! there as it had, they join and part nothing; when they do not, the connections left of the
//! whole board decide, as before (the ratsnest of a board fills its zones, which costs as much
//! as a check). After each pass the whole board is checked once. The next pass takes its
//! changes from that check's findings, and when it finds what the pass's changes add that no
//! neighbourhood could see (a rule of a whole net or a fill, [`near`]), those changes are checked
//! again against the whole board, split as a refused group is, so that what is made is what the
//! check of the whole board allows. The report counts the checks and the items they looked at
//! ([`Tidied::checks`], [`Tidied::items_checked`]): the work grows with the changes, and with the
//! board only by the check of the whole board after each pass. [`Checking::Whole`] is the pass
//! as it was, every group checked on the whole board, which the tests compare with.
//!
//! **Progress and cancel** (ADR 0062). Each pass says how far it has got, net by net
//! ([`netsmith_core::job::report_detail`], `nets tidied`: the net, how many of the pass's nets,
//! the changes made so far and the time the pass has left at the pace so far), and each check is
//! a checkpoint. Nothing is written until the whole pass has run, so a cancel, or Ctrl+C, leaves
//! the files as they were.
//!
//! **What is never touched.** A net the routes file locks, a locked shape, an arc, the shapes of
//! a record whose pads moved (kept on the board as they are, issue #1328), zones, and the kept copper of a net
//! whose pads moved since it was routed (`fab --reuse-routes` routes it again).
//!
//! **The kept copper.** With `reuse_routes`, the router's copper kept in `.netsmith/routes/` is
//! on the board as `fab --reuse-routes` would start from it, and is tidied too; the kept file is
//! written again with the hash of the tidied routes file, so that `fab --reuse-routes` still
//! starts from it. Without it, the kept copper is not read, and a kept file that a change to the
//! routes file makes stale is said.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use netsmith_core::job;
use netsmith_pcb::{
    Board, CopperLayer, Placed, Pt, Rect, RouteTrack, RouteVia, Routes, Track, Via,
};
use netsmith_router::kept::Kept;
use serde::Serialize;

use crate::kept_routes::KeptRoutes;
use crate::routes::{beside_layout, load, near, save, with_routes};

mod shapes;
pub use shapes::{TINY, TINY_RUN};

/// How [`tidy_with`] checks a group of changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checking {
    /// By the check of the group's neighbourhood, and the whole board's after each pass (the
    /// module's header).
    Near,
    /// By the check of the whole board: the pass before issue #1225, one group of all the
    /// changes of a pass, which the tests compare [`Checking::Near`] with.
    Whole,
}

/// The unit of the pass's progress.
pub const PROGRESS_UNIT: &str = "nets tidied";

/// How far from the corner a chamfer starts, in widths of the narrower track, the largest
/// first: each is tried when the one before it is refused.
pub const CHAMFERS: [f64; 3] = [4.0, 2.0, 1.0];

/// The most passes over the board: each pass makes the changes the one before it left possible
/// (a stub under a removed stub, the second of three collinear tracks), and tries a refused
/// change in its next form.
const PASSES: usize = 8;

/// How far from its width to the width its net wants a narrow track is widened, as a share of
/// the way, the most first: each is tried when the one before it is refused (issue #1294).
pub const WIDENINGS: [f64; 4] = [1.0, 0.75, 0.5, 0.25];

/// Two places nearer than this (mm) are one: where the check's findings are matched to copper.
const NEAR: f64 = 1e-3;

/// What kind of repair a change is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A bend of more than 45 degrees cut into bends of 45 degrees at most.
    Corner,
    /// Two collinear tracks made one.
    Merge,
    /// A dangling via removed.
    Via,
    /// A track with an end that touches nothing removed.
    Stub,
    /// An acid trap removed.
    AcidTrap,
    /// A track narrower than its net wants widened (`--widen`).
    Widen,
    /// A track from a pad's or a via's centre to a point still inside it removed.
    InPad,
    /// A second way between two places of a net removed.
    Duplicate,
    /// A track inside another removed, or two overlapping on one line made one.
    Overlap,
    /// A run of tiny tracks made one track or one corner.
    TinyRun,
}

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Kind::Corner => "corner",
            Kind::Merge => "collinear pair",
            Kind::Via => "dangling via",
            Kind::Stub => "stub",
            Kind::AcidTrap => "acid trap",
            Kind::Widen => "narrow track",
            Kind::InPad => "track inside a pad",
            Kind::Duplicate => "duplicate way",
            Kind::Overlap => "overlapping copper",
            Kind::TinyRun => "run of tiny tracks",
        }
    }
}

/// Whose copper a change is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Owner {
    /// The routes file beside the source.
    RoutesFile,
    /// The router's copper kept on disk (`.netsmith/routes/`).
    Kept,
}

/// One change, made or refused.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Change {
    pub kind: Kind,
    pub net: String,
    /// The layer, as the routes file names it (`top`); none for a via.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// The place, in mm from the outline's top left corner, y down, as the routes file has it.
    pub at: [f64; 2],
    pub copper: Owner,
    /// What it does: "cut to 2 bends of 45 degrees".
    pub said: String,
    /// Why a refused change was refused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// How many changes of each kind were made.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub corners: usize,
    pub merges: usize,
    pub vias: usize,
    pub stubs: usize,
    pub acid_traps: usize,
    /// Narrow tracks widened (`--widen`).
    pub widened: usize,
    /// Tracks from a pad's or a via's centre to a point still inside it, removed.
    pub in_pad_tracks: usize,
    /// Second ways between two places of a net, removed.
    pub duplicates: usize,
    /// Tracks inside another removed, and pairs overlapping on one line made one.
    pub overlaps: usize,
    /// Runs of tiny tracks made one track or one corner.
    pub tiny_runs: usize,
}

/// What [`tidy`] did, or with `dry_run` would do.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tidied {
    pub board: String,
    pub dry_run: bool,
    /// Whether narrow tracks were widened too (`--widen`).
    pub widen: bool,
    pub counts: Counts,
    /// The changes made, in the order they were made.
    pub changes: Vec<Change>,
    /// The changes tried and refused, each with why.
    pub refused: Vec<Change>,
    /// The findings of the design rules check (all but the connections left), before and after.
    pub findings_before: usize,
    pub findings_after: usize,
    /// The connections left to route, before and after: the same, by the rule of every change.
    pub connections_left: usize,
    /// How many times the design rules check ran (of a neighbourhood or of the whole board):
    /// the work done.
    pub checks: usize,
    /// How many items (tracks, vias, pads and zones) those checks looked at, in all.
    pub items_checked: usize,
    /// How many pairs of tracks the overlap pass compared, in all its rounds: the work done, which
    /// grows with the tracks near each track and not with the tracks of its net.
    #[serde(skip)]
    pub overlap_compared: usize,
    /// What else there is to know: a kept file not used, or made stale.
    pub notes: Vec<String>,
    /// The files written (none for a dry run, or when nothing changed).
    pub wrote: Vec<String>,
    /// The routes file as tidied.
    #[serde(skip)]
    pub routes: Routes,
    /// The kept copper as tidied, when it was used.
    #[serde(skip)]
    pub kept: Option<Kept>,
}

impl Tidied {
    /// The text for a person.
    pub fn text(&self) -> String {
        use std::fmt::Write as _;
        let mut s = String::new();
        let c = &self.counts;
        let _ = write!(
            s,
            "{}: {} {} corner(s), {} collinear pair(s), {} dangling via(s), {} stub(s), {} acid trap(s), {} track(s) inside a pad, {} duplicate way(s), {} overlapping track(s), {} run(s) of tiny tracks",
            self.board,
            if self.dry_run { "would tidy" } else { "tidied" },
            c.corners,
            c.merges,
            c.vias,
            c.stubs,
            c.acid_traps,
            c.in_pad_tracks,
            c.duplicates,
            c.overlaps,
            c.tiny_runs
        );
        if self.widen {
            let _ = write!(
                s,
                ", {} narrow track(s) {}",
                c.widened,
                if self.dry_run { "to widen" } else { "widened" }
            );
        }
        let _ = writeln!(s);
        let line = |s: &mut String, c: &Change| {
            let layer = c
                .layer
                .as_ref()
                .map(|l| format!(" on {l}"))
                .unwrap_or_default();
            let owner = match c.copper {
                Owner::RoutesFile => "",
                Owner::Kept => " (the router's kept copper)",
            };
            let _ = write!(
                s,
                "  {}: net {}{layer} at ({}, {}){owner}: {}",
                c.kind.word(),
                c.net,
                c.at[0],
                c.at[1],
                c.said
            );
            if let Some(why) = &c.why {
                let _ = write!(s, ": refused, {why}");
            }
            let _ = writeln!(s);
        };
        for c in &self.changes {
            line(&mut s, c);
        }
        if !self.refused.is_empty() {
            let _ = writeln!(s, "left as it is ({} refused):", self.refused.len());
            for c in &self.refused {
                line(&mut s, c);
            }
        }
        let _ = writeln!(
            s,
            "the design rules check: {} finding(s) before, {} after; {} connection(s) left to route, as before",
            self.findings_before, self.findings_after, self.connections_left
        );
        let _ = writeln!(
            s,
            "checked {} time(s), {} item(s) in all",
            self.checks, self.items_checked
        );
        for n in &self.notes {
            let _ = writeln!(s, "note: {n}");
        }
        if self.dry_run {
            let _ = writeln!(s, "nothing written (--dry-run)");
        } else if self.wrote.is_empty() {
            let _ = writeln!(s, "nothing to write");
        }
        for w in &self.wrote {
            let _ = writeln!(s, "wrote {w}");
        }
        s
    }
}

/// The router's kept copper to tidy with the routes file: the kept file's content and the nets
/// whose pads moved since it was routed (left as they are, and off the board).
pub struct KeptCopper<'a> {
    pub kept: &'a Kept,
    pub stale: &'a BTreeSet<String>,
}

/// Tidy the routes file beside `layout` of `board` (placed, with no routes on it), and with
/// `reuse_routes` the router's copper kept for it, and write what changed unless `dry_run`.
pub fn tidy_files(
    board: &Board,
    layout: &Path,
    reuse_routes: bool,
    widen: bool,
    dry_run: bool,
) -> Result<Tidied, String> {
    let file = beside_layout(layout);
    let routes = load(&file)?;
    let keep = KeptRoutes::beside_layout(layout);
    let start = with_routes(board, &routes)?.0;
    let mut notes = Vec::new();
    let mut kept: Option<(Kept, BTreeSet<String>)> = None;
    let kept_file = keep.file(&board.name);
    if reuse_routes {
        match keep.read(&board.name) {
            Ok(Some(k)) => match k.before(&start, &keep.source) {
                Ok((_, stale)) => kept = Some((k, stale)),
                Err(why) => notes.push(format!(
                    "{}: not used: {why}; the routes file is tidied alone",
                    kept_file.display()
                )),
            },
            Ok(None) => notes.push(format!(
                "{} does not exist: the routes file is tidied alone (a `board fab --reuse-routes` writes it)",
                kept_file.display()
            )),
            Err(why) => notes.push(format!("{why}; the routes file is tidied alone")),
        }
    }
    let mut tidied = tidy_passes(
        board,
        &routes,
        kept.as_ref()
            .map(|(k, stale)| KeptCopper { kept: k, stale }),
        Checking::Near,
        widen,
    )?;
    tidied.dry_run = dry_run;
    tidied.notes.splice(0..0, notes);
    let routes_changed = tidied.routes != routes;
    let kept_changed = match (&tidied.kept, &kept) {
        (Some(new), Some((old, _))) => new != old,
        _ => false,
    };
    if routes_changed && kept.is_none() && kept_file.exists() {
        tidied.notes.push(format!(
            "the routes file's copper changes, so `board fab --reuse-routes` does not start from the router's copper in {} (tidy with `--reuse-routes` to keep it)",
            kept_file.display()
        ));
    }
    if dry_run {
        return Ok(tidied);
    }
    if routes_changed {
        save(&file, &tidied.routes)?;
        tidied.wrote.push(file.display().to_string());
    }
    if kept_changed && let Some(k) = &tidied.kept {
        let written = keep.save(k)?;
        tidied.wrote.push(written.display().to_string());
    }
    Ok(tidied)
}

/// Tidy `routes` of `board` (placed, with no routes on it), and `kept`, the router's copper, when
/// given: the module's header. Nothing is written; [`Tidied::routes`] and [`Tidied::kept`] are
/// the copper as tidied.
pub fn tidy(
    board: &Board,
    routes: &Routes,
    kept: Option<KeptCopper<'_>>,
) -> Result<Tidied, String> {
    tidy_with(board, routes, kept, Checking::Near)
}

/// [`tidy`], with the groups of changes checked as `checking` says.
pub fn tidy_with(
    board: &Board,
    routes: &Routes,
    kept: Option<KeptCopper<'_>>,
    checking: Checking,
) -> Result<Tidied, String> {
    tidy_passes(board, routes, kept, checking, false)
}

/// [`tidy_with`], and with `widen` the narrow tracks widened too (the module's header).
pub fn tidy_passes(
    board: &Board,
    routes: &Routes,
    kept: Option<KeptCopper<'_>>,
    checking: Checking,
    widen: bool,
) -> Result<Tidied, String> {
    let mut work = Work::new(board, routes, kept.as_ref())?;
    work.widen = widen;
    let first = work.look(&work.copper.clone())?;
    let mut look = first.clone();
    let mut made: Vec<Change> = Vec::new();
    // The last refusal of each change by its place, and how many of its forms were tried.
    let mut refused: BTreeMap<Key, Change> = BTreeMap::new();
    let mut tries: HashMap<Key, usize> = HashMap::new();
    for pass in 1..=PASSES {
        job::checkpoint();
        let candidates = work.candidates(&look, &tries);
        if candidates.is_empty() {
            break;
        }
        for c in &candidates {
            *tries.entry(c.key.clone()).or_default() += 1;
        }
        let mut outcome = Outcome::default();
        match checking {
            Checking::Whole => work.settle(candidates, &mut look, &mut outcome)?,
            Checking::Near => {
                work.pass_near(pass, candidates, &mut look, &mut outcome, made.len())?
            }
        }
        for c in outcome.made {
            refused.remove(&c.key);
            made.push(c.change);
        }
        for (c, why) in outcome.refused {
            let mut change = c.change;
            change.why = Some(why);
            refused.insert(c.key, change);
        }
    }
    let mut counts = Counts::default();
    for c in &made {
        match c.kind {
            Kind::Corner => counts.corners += 1,
            Kind::Merge => counts.merges += 1,
            Kind::Via => counts.vias += 1,
            Kind::Stub => counts.stubs += 1,
            Kind::AcidTrap => counts.acid_traps += 1,
            Kind::Widen => counts.widened += 1,
            Kind::InPad => counts.in_pad_tracks += 1,
            Kind::Duplicate => counts.duplicates += 1,
            Kind::Overlap => counts.overlaps += 1,
            Kind::TinyRun => counts.tiny_runs += 1,
        }
    }
    let new_routes = work.routes_of(&work.copper);
    let new_kept = kept.map(|k| work.kept_of(&work.copper, k.kept, &new_routes));
    let new_kept = match new_kept {
        Some(k) => Some(k?),
        None => None,
    };
    Ok(Tidied {
        board: board.name.clone(),
        dry_run: false,
        widen,
        counts,
        changes: made,
        refused: refused.into_values().collect(),
        findings_before: first.keys.len(),
        findings_after: look.keys.len(),
        connections_left: look.lines.len(),
        checks: work.checks,
        overlap_compared: work.compared.get(),
        items_checked: work.items,
        notes: Vec::new(),
        wrote: Vec::new(),
        routes: new_routes,
        kept: new_kept,
    })
}

/// One track of the copper being tidied.
#[derive(Debug, Clone)]
struct Wire {
    owner: Owner,
    track: Track,
    /// The track as the board has it: of a track of the routes file, its record's form put on
    /// the board, as `with_routes` puts it.
    drawn: Track,
    /// The record's form as the file had it, while it is unchanged.
    was: Option<RouteTrack>,
    /// Locked, or of a locked net: never changed.
    fixed: bool,
    alive: bool,
    /// Made by a widening: not widened again.
    widened: bool,
    /// Where it is written: the place of the track it replaced, then the order it was made in.
    rank: (usize, usize),
}

#[derive(Debug, Clone)]
struct Hole {
    owner: Owner,
    via: Via,
    was: Option<RouteVia>,
    fixed: bool,
    alive: bool,
}

#[derive(Debug, Clone, Default)]
struct Copper {
    wires: Vec<Wire>,
    holes: Vec<Hole>,
}

/// One change to try: the tracks and vias it removes and the tracks it adds in their place.
#[derive(Debug, Clone)]
struct Candidate {
    key: Key,
    change: Change,
    remove_wires: Vec<usize>,
    remove_holes: Vec<usize>,
    add: Vec<Track>,
}

/// A change by its kind, net, layer and place (to the micrometre): the same change in each pass.
type Key = (Kind, String, String, (i64, i64));

/// What the check found of a board: its findings by key, and the connections left.
#[derive(Debug, Clone)]
struct Look {
    report: netsmith_geom::DrcReport,
    keys: Vec<String>,
    lines: Vec<String>,
    /// The nets with a connection left to route.
    open: BTreeSet<String>,
}

#[derive(Default)]
struct Outcome {
    made: Vec<Candidate>,
    refused: Vec<(Candidate, String)>,
}

struct Work<'a> {
    board: &'a Board,
    routes: &'a Routes,
    copper: Copper,
    /// The nets whose records are current and unlocked: their shapes are rebuilt from the copper.
    rebuilt: BTreeSet<String>,
    /// The board with the routes file's zones and none of its copper or parts: what each check
    /// puts the copper and the parts it looks at on.
    shell: Board,
    parts: Vec<Placed>,
    part_boxes: Vec<Rect>,
    /// The zones of the board, its planes and pours as the routes file would hold them too:
    /// what the check of a neighbourhood cuts to it ([`near::clipped`]).
    zones: Vec<netsmith_pcb::BoardZone>,
    /// What the check is given beside the board file: the same for every check of the pass.
    project: String,
    extras: netsmith_geom::DrcExtras,
    /// The board's items, by where they are ([`near::Grid`]), and the margin of a neighbourhood.
    grid: near::Grid,
    reach: f64,
    /// The connections left to route of the copper as it is now, sorted.
    lines: Vec<String>,
    /// Whether narrow tracks are widened too.
    widen: bool,
    /// The pads of each net, for the passes of [`shapes`].
    spots: BTreeMap<String, Vec<shapes::Spot>>,
    checks: usize,
    items: usize,
    /// The pairs of tracks the overlap pass compared ([`Tidied::overlap_compared`]).
    compared: std::cell::Cell<usize>,
}

/// A defect that the passes of [`shapes`] find in the copper of a board as it stands, for the
/// quality report of `board nets` (issue #1481): its kind, its net and how many tracks and vias
/// of the copper it is, by their places in the board's `tracks` and `vias` (the tracks and vias
/// of a duplicate way, the one track of a nudge).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Defect {
    pub kind: Kind,
    pub net: String,
    pub wires: Vec<usize>,
    pub holes: Vec<usize>,
}

/// The tracks inside a pad, the duplicate ways, the overlapping copper and the copper that ends in nothing
/// of `board`'s tracks and vias, as `routes tidy` finds them (each detector once, nothing checked or changed): the measure the
/// quality report counts by, so that what tidy removes is what the report counts.
pub(crate) fn shape_defects(board: &Board) -> Vec<Defect> {
    let routes = Routes::default();
    let work = Work::looking(board, &routes);
    work.in_pad_tracks()
        .into_iter()
        .chain(work.duplicates())
        .chain(work.overlaps())
        .chain(work.dead_copper())
        .map(|c| Defect {
            kind: c.change.kind,
            net: c.change.net,
            wires: c.remove_wires,
            holes: c.remove_holes,
        })
        .collect()
}

impl<'a> Work<'a> {
    /// The copper of `board` as it stands, its tracks and vias, every one of them open to the
    /// detectors of [`shapes`], with nothing to check a change by: for [`shape_defects`].
    fn looking(board: &'a Board, routes: &'a Routes) -> Self {
        let mut copper = Copper::default();
        for (n, t) in board.tracks.iter().enumerate() {
            copper.wires.push(Wire {
                owner: Owner::RoutesFile,
                track: t.clone(),
                drawn: t.clone(),
                was: None,
                fixed: false,
                alive: true,
                widened: false,
                rank: (n, 0),
            });
        }
        for v in &board.vias {
            copper.holes.push(Hole {
                owner: Owner::RoutesFile,
                via: v.clone(),
                was: None,
                fixed: false,
                alive: true,
            });
        }
        Work {
            board,
            routes,
            copper,
            rebuilt: BTreeSet::new(),
            shell: board.clone(),
            parts: Vec::new(),
            part_boxes: Vec::new(),
            zones: Vec::new(),
            project: String::new(),
            extras: netsmith_geom::DrcExtras::default(),
            grid: near::Grid::new(board.outline, 1.0),
            reach: 0.0,
            lines: Vec::new(),
            widen: false,
            spots: shapes::spots(board),
            checks: 0,
            items: 0,
            compared: std::cell::Cell::new(0),
        }
    }

    fn new(
        board: &'a Board,
        routes: &'a Routes,
        kept: Option<&KeptCopper<'_>>,
    ) -> Result<Self, String> {
        let mut copper = Copper::default();
        let mut rebuilt = BTreeSet::new();
        let table = routes.boards.get(&board.name);
        for (net, record) in table.map(|t| t.nets.iter()).into_iter().flatten() {
            let Some(hash) = netsmith_pcb::pad_hash(board, net) else {
                continue;
            };
            let current = hash == record.pads;
            if current && !record.lock {
                rebuilt.insert(net.clone());
            }
            for t in &record.tracks {
                let n = copper.wires.len();
                copper.wires.push(Wire {
                    owner: Owner::RoutesFile,
                    track: t.on(board, net),
                    drawn: t.on(board, net),
                    was: Some(t.clone()),
                    fixed: record.lock || t.lock || !current,
                    alive: true,
                    widened: false,
                    rank: (n, 0),
                });
            }
            for v in &record.vias {
                copper.holes.push(Hole {
                    owner: Owner::RoutesFile,
                    via: v.on(board, net),
                    was: Some(v.clone()),
                    fixed: record.lock || v.lock || !current,
                    alive: true,
                });
            }
        }
        let locked: BTreeSet<&str> = table
            .map(|t| {
                t.nets
                    .iter()
                    .filter(|(_, r)| r.lock)
                    .map(|(n, _)| n.as_str())
                    .collect()
            })
            .unwrap_or_default();
        if let Some(k) = kept {
            for (net, record) in &k.kept.nets {
                if k.stale.contains(net) {
                    continue;
                }
                let fixed = locked.contains(net.as_str());
                for t in &record.tracks {
                    let n = copper.wires.len();
                    copper.wires.push(Wire {
                        owner: Owner::Kept,
                        track: t.clone(),
                        drawn: t.clone(),
                        was: None,
                        fixed,
                        alive: true,
                        widened: false,
                        rank: (n, 0),
                    });
                }
                for v in &record.vias {
                    copper.holes.push(Hole {
                        owner: Owner::Kept,
                        via: v.clone(),
                        was: None,
                        fixed,
                        alive: true,
                    });
                }
            }
        }
        let mut shell = with_routes(board, routes)?.0;
        shell.tracks.clear();
        shell.vias.clear();
        let parts = std::mem::take(&mut shell.parts);
        let part_boxes: Vec<Rect> = parts.iter().map(near::part_box).collect();
        let reach = near::reach(&shell);
        let mut area = board.outline;
        for r in &part_boxes {
            area = area.union(r);
        }
        let mut grid = near::Grid::new(area, 2.0 * reach);
        for (i, r) in part_boxes.iter().enumerate() {
            grid.put(near::Item::Part(i), r);
        }
        for (i, w) in copper.wires.iter().enumerate() {
            grid.put(near::Item::Wire(i), &near::track_box(&w.drawn));
        }
        for (i, h) in copper.holes.iter().enumerate() {
            grid.put(near::Item::Hole(i), &near::via_box(&h.via));
        }
        let mut whole = shell.clone();
        whole.parts = parts.clone();
        let zones = near::zones_of(&whole);
        Ok(Work {
            board,
            routes,
            copper,
            rebuilt,
            project: netsmith_pcb::kicad_pro(&whole),
            extras: crate::fab::drc_extras(&whole),
            shell,
            parts,
            part_boxes,
            zones,
            grid,
            reach,
            lines: Vec::new(),
            widen: false,
            spots: shapes::spots(board),
            checks: 0,
            items: 0,
            compared: std::cell::Cell::new(0),
        })
    }

    /// The routes file with the copper's tracks and vias in the records it rebuilds.
    fn routes_of(&self, copper: &Copper) -> Routes {
        let mut routes = self.routes.clone();
        let Some(table) = routes.boards.get_mut(&self.board.name) else {
            return routes;
        };
        for net in &self.rebuilt {
            let Some(record) = table.nets.get_mut(net) else {
                continue;
            };
            let mut wires: Vec<&Wire> = copper
                .wires
                .iter()
                .filter(|w| w.alive && w.owner == Owner::RoutesFile && w.track.net == *net)
                .collect();
            wires.sort_by_key(|w| w.rank);
            record.tracks = wires
                .iter()
                .map(|w| {
                    w.was
                        .clone()
                        .unwrap_or_else(|| RouteTrack::of(self.board, &w.track))
                })
                .collect();
            record.vias = copper
                .holes
                .iter()
                .filter(|h| h.alive && h.owner == Owner::RoutesFile && h.via.net == *net)
                .map(|h| {
                    h.was
                        .clone()
                        .unwrap_or_else(|| RouteVia::of(self.board, &h.via))
                })
                .collect();
        }
        table.nets.retain(|_, r| r.shapes() > 0 || r.lock);
        if table.nets.is_empty() {
            routes.boards.remove(&self.board.name);
        }
        routes
    }

    /// The kept file with the copper's kept tracks and vias, for the tidied `routes`.
    fn kept_of(&self, copper: &Copper, kept: &Kept, routes: &Routes) -> Result<Kept, String> {
        let mut next = kept.clone();
        let mut wires: Vec<&Wire> = copper
            .wires
            .iter()
            .filter(|w| w.alive && w.owner == Owner::Kept)
            .collect();
        wires.sort_by_key(|w| w.rank);
        let touched: BTreeSet<&str> = copper
            .wires
            .iter()
            .filter(|w| w.owner == Owner::Kept)
            .map(|w| w.track.net.as_str())
            .chain(
                copper
                    .holes
                    .iter()
                    .filter(|h| h.owner == Owner::Kept)
                    .map(|h| h.via.net.as_str()),
            )
            .collect();
        for net in touched {
            let Some(record) = next.nets.get_mut(net) else {
                continue;
            };
            record.tracks = wires
                .iter()
                .filter(|w| w.track.net == net)
                .map(|w| w.track.clone())
                .collect();
            record.vias = copper
                .holes
                .iter()
                .filter(|h| h.alive && h.owner == Owner::Kept && h.via.net == net)
                .map(|h| h.via.clone())
                .collect();
        }
        next.nets
            .retain(|_, r| !r.tracks.is_empty() || !r.vias.is_empty());
        next.routes = netsmith_router::kept::routes_hash(&with_routes(self.board, routes)?.0);
        Ok(next)
    }

    /// The board with the live `copper` that `wire` and `hole` take, and the parts `part`
    /// takes, in the order the board with all of it has them: the routes file's tracks net by
    /// net in the order its records are written, then the router's kept copper (what
    /// `with_routes` of [`Work::routes_of`], then the kept copper, makes).
    fn board_with(
        &self,
        copper: &Copper,
        wire: impl Fn(usize) -> bool,
        hole: impl Fn(usize) -> bool,
        part: impl Fn(usize) -> bool,
    ) -> Board {
        let mut board = self.shell.clone();
        board.parts = (0..self.parts.len())
            .filter(|&i| part(i))
            .map(|i| self.parts[i].clone())
            .collect();
        let live = |i: usize, owner: Owner| {
            let w = &copper.wires[i];
            w.alive && w.owner == owner && wire(i)
        };
        let mut file: Vec<usize> = (0..copper.wires.len())
            .filter(|&i| live(i, Owner::RoutesFile))
            .collect();
        file.sort_by(|&a, &b| {
            let (wa, wb) = (&copper.wires[a], &copper.wires[b]);
            (&wa.track.net, wa.rank, a).cmp(&(&wb.track.net, wb.rank, b))
        });
        board.tracks = file
            .into_iter()
            .chain((0..copper.wires.len()).filter(|&i| live(i, Owner::Kept)))
            .map(|i| copper.wires[i].drawn.clone())
            .collect();
        let live = |i: usize, owner: Owner| {
            let h = &copper.holes[i];
            h.alive && h.owner == owner && hole(i)
        };
        let mut file: Vec<usize> = (0..copper.holes.len())
            .filter(|&i| live(i, Owner::RoutesFile))
            .collect();
        file.sort_by(|&a, &b| (&copper.holes[a].via.net, a).cmp(&(&copper.holes[b].via.net, b)));
        board.vias = file
            .into_iter()
            .chain((0..copper.holes.len()).filter(|&i| live(i, Owner::Kept)))
            .map(|i| copper.holes[i].via.clone())
            .collect();
        board
    }

    /// The whole board with `copper` on it.
    fn board_of(&self, copper: &Copper) -> Board {
        self.board_with(copper, |_| true, |_| true, |_| true)
    }

    /// The design rules check of `board`, counted.
    fn check(&mut self, board: &Board) -> Result<netsmith_geom::DrcReport, String> {
        job::checkpoint();
        self.checks += 1;
        self.items += board.tracks.len()
            + board.vias.len()
            + board.zones.len()
            + board
                .parts
                .iter()
                .map(|p| p.footprint.pads.len())
                .sum::<usize>();
        netsmith_geom::drc_check(
            &netsmith_pcb::kicad_pcb(board),
            Some(&self.project),
            &self.extras,
        )
        .map_err(|e| format!("the design rules check: {e}"))
    }

    /// The connections left to route of the board with `copper`, sorted, with the nets of each.
    fn lines_of(&self, copper: &Copper) -> (Vec<String>, BTreeSet<String>) {
        let ratsnest = netsmith_pcb::Ratsnest::of(&self.board_of(copper));
        let mut lines: Vec<String> = ratsnest.lines.iter().map(ToString::to_string).collect();
        lines.sort();
        (
            lines,
            ratsnest.lines.iter().map(|l| l.net.clone()).collect(),
        )
    }

    /// The check of the whole board with `copper`.
    fn look(&mut self, copper: &Copper) -> Result<Look, String> {
        let board = self.board_of(copper);
        let report = self.check(&board)?;
        let mut keys: Vec<String> = report
            .findings
            .iter()
            .filter(|f| f.rule != netsmith_geom::DrcRule::UnconnectedItems)
            .map(|f| finding_key(f, &board))
            .collect();
        keys.sort();
        let (lines, open) = self.lines_of(copper);
        Ok(Look {
            report,
            keys,
            lines,
            open,
        })
    }

    /// The pass of `candidates` checked by neighbourhoods (the module's header): net by net,
    /// then the whole board once, which becomes `look`; the changes made go to `out`, after
    /// `before` made by the passes before.
    fn pass_near(
        &mut self,
        pass: usize,
        candidates: Vec<Candidate>,
        look: &mut Look,
        out: &mut Outcome,
        before: usize,
    ) -> Result<(), String> {
        let start = self.copper.clone();
        self.lines = look.lines.clone();
        let tried = candidates.len();
        // The changes made are said in the order of the pass's list, as one group of them all
        // makes them.
        let order: HashMap<Key, usize> = candidates
            .iter()
            .enumerate()
            .map(|(i, c)| (c.key.clone(), i))
            .collect();
        let made_before = out.made.len();
        let mut by_net: BTreeMap<String, Vec<Candidate>> = BTreeMap::new();
        for c in candidates {
            by_net.entry(c.change.net.clone()).or_default().push(c);
        }
        let total = by_net.len() as u64;
        job::report_detail(PROGRESS_UNIT, 0, Some(total), || {
            format!("pass {pass}: {tried} change(s) to try on {total} net(s), {before} made so far")
        });
        let began = std::time::Instant::now();
        for (k, (net, group)) in by_net.into_iter().enumerate() {
            self.settle_near(group, out)?;
            let done = k as u64 + 1;
            job::report_detail(PROGRESS_UNIT, done, Some(total), || {
                let left = if done < total {
                    let each = began.elapsed().as_secs_f64() / done as f64;
                    format!(
                        ", about {:.0} s left in the pass",
                        each * (total - done) as f64
                    )
                } else {
                    String::new()
                };
                format!(
                    "pass {pass}: net {net}, {done} of {total} net(s): {} change(s) made so far{left}",
                    before + out.made.len()
                )
            });
        }
        out.made[made_before..].sort_by_key(|c| order.get(&c.key).copied());
        if out.made.is_empty() {
            return Ok(());
        }
        let end = self.look(&self.copper.clone())?;
        if refusal(look, &end).is_none() {
            *look = end;
            return Ok(());
        }
        // The whole board finds what no neighbourhood could see: the pass's changes are checked
        // against the whole board, as `Checking::Whole` checks them.
        job::report_detail(PROGRESS_UNIT, total, Some(total), || {
            format!(
                "pass {pass}: the whole board finds what its changes add beyond their neighbourhoods; its {} change(s) are checked again on the whole board",
                out.made.len()
            )
        });
        // The grid keeps listing the tracks the pass added; a search tests what it finds.
        self.copper = start;
        let made = std::mem::take(&mut out.made);
        self.settle(made, look, out)
    }

    /// [`Work::settle`] by the check of the group's neighbourhood.
    fn settle_near(&mut self, group: Vec<Candidate>, out: &mut Outcome) -> Result<(), String> {
        if group.is_empty() {
            return Ok(());
        }
        let mut trial = self.copper.clone();
        for c in &group {
            apply(self.board, &mut trial, c);
        }
        match self.refusal_near(&group, &trial)? {
            None => {
                self.accept(trial);
                out.made.extend(group);
            }
            Some(why) if group.len() == 1 => {
                out.refused
                    .push((group.into_iter().next().expect("one"), why));
            }
            Some(_) => {
                let mut first = group;
                let second = first.split_off(first.len() / 2);
                self.settle_near(first, out)?;
                self.settle_near(second, out)?;
            }
        }
        Ok(())
    }

    /// `trial` made the copper: the tracks it adds put in the grid.
    fn accept(&mut self, trial: Copper) {
        for (i, w) in trial.wires.iter().enumerate().skip(self.copper.wires.len()) {
            self.grid
                .put(near::Item::Wire(i), &near::track_box(&w.drawn));
        }
        self.copper = trial;
    }

    /// The box round the copper `c` removes from the copper and adds to it.
    fn changed(&self, c: &Candidate) -> Rect {
        let boxes = c
            .remove_wires
            .iter()
            .map(|&i| near::track_box(&self.copper.wires[i].drawn))
            .chain(
                c.remove_holes
                    .iter()
                    .map(|&i| near::via_box(&self.copper.holes[i].via)),
            )
            .chain(c.add.iter().map(near::track_box));
        boxes
            .reduce(|a, b| a.union(&b))
            .unwrap_or(Rect::new(self.board.outline.min, self.board.outline.min))
    }

    /// Why `trial`, the copper with the changes of `group`, is refused, by the check of the
    /// neighbourhoods of the changes with and without them; `None` when it is allowed.
    fn refusal_near(
        &mut self,
        group: &[Candidate],
        trial: &Copper,
    ) -> Result<Option<String>, String> {
        let hoods: Vec<Rect> = group
            .iter()
            .map(|c| self.changed(c).expanded(self.reach))
            .collect();
        let wides: Vec<Rect> = hoods.iter().map(|h| h.expanded(self.reach)).collect();
        let mut wires = BTreeSet::new();
        let mut holes = BTreeSet::new();
        let mut parts = BTreeSet::new();
        for &wide in &wides {
            for item in self.grid.near(&wide) {
                match item {
                    near::Item::Wire(i)
                        if i < self.copper.wires.len()
                            && near::meets(
                                &near::track_box(&self.copper.wires[i].drawn),
                                &wide,
                            ) =>
                    {
                        wires.insert(i);
                    }
                    near::Item::Hole(i)
                        if near::meets(&near::via_box(&self.copper.holes[i].via), &wide) =>
                    {
                        holes.insert(i);
                    }
                    near::Item::Part(i) if near::meets(&self.part_boxes[i], &wide) => {
                        parts.insert(i);
                    }
                    _ => {}
                }
            }
        }
        let now = self.copper.wires.len();
        let zones = near::clipped(&self.zones, &near::clusters(wides));
        let local = |mut board: Board| {
            board.zones = zones.clone();
            board.planes.clear();
            board.pours.clear();
            board
        };
        let before = local(self.board_with(
            &self.copper,
            |i| wires.contains(&i),
            |i| holes.contains(&i),
            |i| parts.contains(&i),
        ));
        let after = local(self.board_with(
            trial,
            |i| i >= now || wires.contains(&i),
            |i| holes.contains(&i),
            |i| parts.contains(&i),
        ));
        let (old, old_pieces) = self.keys_near(&before, &hoods)?;
        let (new, new_pieces) = self.keys_near(&after, &hoods)?;
        // The pieces of each net in the neighbourhoods are as many as they were: the changes
        // join and part nothing, and the connections left are as they were (a change that
        // parts what joins only further off is the whole board's, after the pass). Otherwise
        // the connections left of the whole board say.
        if old_pieces != new_pieces {
            let (lines, _) = self.lines_of(trial);
            if lines != self.lines {
                return Ok(lines_refusal(&self.lines, &lines));
            }
        }
        Ok(keys_refusal(&old, &new))
    }

    /// The keys of the findings of `board` with a place in one of `hoods`, sorted, and the nets
    /// of its unconnected items (each a piece of a net not joined to the rest), sorted.
    fn keys_near(
        &mut self,
        board: &Board,
        hoods: &[Rect],
    ) -> Result<(Vec<String>, Vec<String>), String> {
        let report = self.check(board)?;
        let unconnected =
            |f: &&netsmith_geom::DrcFinding| f.rule == netsmith_geom::DrcRule::UnconnectedItems;
        let mut keys: Vec<String> = report
            .findings
            .iter()
            .filter(|f| !unconnected(f))
            .filter(|f| {
                let at = Pt::new(f.at.x, f.at.y);
                hoods.iter().any(|h| near::holds(h, at))
            })
            .map(|f| finding_key(f, board))
            .collect();
        keys.sort();
        let mut pieces: Vec<String> = report
            .findings
            .iter()
            .filter(unconnected)
            .map(|f| {
                f.items
                    .first()
                    .and_then(|i| i.net.clone())
                    .unwrap_or_default()
            })
            .collect();
        pieces.sort();
        Ok((keys, pieces))
    }

    /// Make the changes of `group` that the rule allows, splitting a refused group in two.
    fn settle(
        &mut self,
        group: Vec<Candidate>,
        look: &mut Look,
        out: &mut Outcome,
    ) -> Result<(), String> {
        if group.is_empty() {
            return Ok(());
        }
        let mut trial = self.copper.clone();
        for c in &group {
            apply(self.board, &mut trial, c);
        }
        let after = self.look(&trial)?;
        match refusal(look, &after) {
            None => {
                self.accept(trial);
                *look = after;
                out.made.extend(group);
            }
            Some(why) if group.len() == 1 => {
                out.refused
                    .push((group.into_iter().next().expect("one"), why));
            }
            Some(_) => {
                let mut first = group;
                let second = first.split_off(first.len() / 2);
                self.settle(first, look, out)?;
                self.settle(second, look, out)?;
            }
        }
        Ok(())
    }

    /// The changes to try in a pass, each touching copper no other touches, in the form after
    /// those `tries` refused.
    fn candidates(&self, look: &Look, tries: &HashMap<Key, usize>) -> Vec<Candidate> {
        let mut used_wires: BTreeSet<usize> = BTreeSet::new();
        let mut used_holes: BTreeSet<usize> = BTreeSet::new();
        let mut out: Vec<Candidate> = Vec::new();
        let complete: BTreeSet<&str> = self
            .board
            .nets
            .iter()
            .map(|n| n.name.as_str())
            .filter(|n| !look.open.contains(*n))
            .collect();
        let mut push = |c: Option<Candidate>, out: &mut Vec<Candidate>| {
            let Some(c) = c else { return };
            if c.remove_wires.iter().any(|w| used_wires.contains(w))
                || c.remove_holes.iter().any(|h| used_holes.contains(h))
            {
                return;
            }
            used_wires.extend(c.remove_wires.iter().copied());
            used_holes.extend(c.remove_holes.iter().copied());
            out.push(c);
        };
        let attempt = |key: &Key| tries.get(key).copied().unwrap_or(0);
        for f in &look.report.findings {
            use netsmith_geom::DrcRule as R;
            let c = match f.rule {
                R::ViaDangling => self.dangling_via(f, &complete, &attempt),
                R::TrackDangling => self.stub(f, &complete, &attempt),
                R::TrackAngle => self.corner_at(f, Kind::Corner, &attempt),
                R::AcidTrap => self.acid_trap(f, &attempt),
                _ => None,
            };
            push(c, &mut out);
        }
        // A merge, a track inside a pad, a duplicate way and an overlap have one form: refused once, each
        // is not tried again.
        let once = self
            .merges()
            .into_iter()
            .chain(self.in_pad_tracks())
            .chain(self.overlaps())
            .chain(self.duplicates());
        for c in once {
            if attempt(&c.key) == 0 {
                push(Some(c), &mut out);
            }
        }
        for c in self.tiny_runs(&attempt) {
            push(Some(c), &mut out);
        }
        if self.widen {
            for i in 0..self.copper.wires.len() {
                push(self.widening(i, &attempt), &mut out);
            }
        }
        out
    }

    /// The narrow track `i` widened, in the form after those refused: none when it is as wide
    /// as its net wants, was widened already, or cannot be changed.
    fn widening(&self, i: usize, attempt: &dyn Fn(&Key) -> usize) -> Option<Candidate> {
        if !self.editable(i) || self.copper.wires[i].widened {
            return None;
        }
        let w = &self.copper.wires[i];
        let t = &w.track;
        let wanted = wanted_width(self.board, &t.net, t.layer);
        if t.width >= wanted - 1e-6 {
            return None;
        }
        let at = Pt::new((t.from.x + t.to.x) / 2.0, (t.from.y + t.to.y) / 2.0);
        let key = self.key(Kind::Widen, &t.net, Some(t.layer), at);
        let tried = attempt(&key);
        let share = *WIDENINGS.get(tried)?;
        let width = if tried == 0 {
            wanted
        } else {
            // Part of the way, to the 0.01 mm below.
            ((t.width + share * (wanted - t.width)) * 100.0 + 1e-9).floor() / 100.0
        };
        if width <= t.width + 1e-6 {
            return None;
        }
        let mut wider = t.clone();
        wider.width = width;
        let said = if tried == 0 {
            format!(
                "widened from {:.3} mm to the {:.3} mm its net wants",
                t.width, wanted
            )
        } else {
            format!(
                "widened from {:.3} mm to {:.3} mm, of the {:.3} mm its net wants: the clearance to its neighbours allows no more",
                t.width, width, wanted
            )
        };
        Some(Candidate {
            key,
            change: self.change(Kind::Widen, &t.net, Some(t.layer), at, w.owner, said),
            remove_wires: vec![i],
            remove_holes: Vec::new(),
            add: vec![wider],
        })
    }

    fn key(&self, kind: Kind, net: &str, layer: Option<CopperLayer>, at: Pt) -> Key {
        let q = |v: f64| (v * 1000.0).round() as i64;
        (
            kind,
            net.to_string(),
            layer.map(|l| l.word().to_string()).unwrap_or_default(),
            (q(at.x), q(at.y)),
        )
    }

    fn change(
        &self,
        kind: Kind,
        net: &str,
        layer: Option<CopperLayer>,
        at: Pt,
        owner: Owner,
        said: String,
    ) -> Change {
        let q = at - self.board.outline.min;
        Change {
            kind,
            net: net.to_string(),
            layer: layer.map(|l| l.word().to_string()),
            at: [to_um(q.x), to_um(q.y)],
            copper: owner,
            said,
            why: None,
        }
    }

    /// The live tracks of `net` on `layer` with an end at `at`.
    fn ends_at(&self, net: &str, layer: CopperLayer, at: Pt) -> Vec<usize> {
        (0..self.copper.wires.len())
            .filter(|&i| {
                let w = &self.copper.wires[i];
                w.alive
                    && w.track.net == net
                    && w.track.layer == layer
                    && (w.track.from.distance(at) < NEAR || w.track.to.distance(at) < NEAR)
            })
            .collect()
    }

    fn editable(&self, i: usize) -> bool {
        let w = &self.copper.wires[i];
        w.alive && !w.fixed && w.track.mid.is_none()
    }

    fn dangling_via(
        &self,
        f: &netsmith_geom::DrcFinding,
        complete: &BTreeSet<&str>,
        attempt: &dyn Fn(&Key) -> usize,
    ) -> Option<Candidate> {
        let net = f.items.first()?.net.clone()?;
        let alone = f.message.contains("on 0 layer(s)");
        if !alone && !complete.contains(net.as_str()) {
            return None;
        }
        let at = Pt::new(f.at.x, f.at.y);
        let key = self.key(Kind::Via, &net, None, at);
        if attempt(&key) > 0 {
            return None;
        }
        let i = (0..self.copper.holes.len()).find(|&i| {
            let h = &self.copper.holes[i];
            h.alive && !h.fixed && h.via.net == net && h.via.at.distance(at) < NEAR
        })?;
        let owner = self.copper.holes[i].owner;
        let said = if alone {
            "removed: it joins no copper of its net".to_string()
        } else {
            "removed: it joins copper of its net on one layer only".to_string()
        };
        Some(Candidate {
            key,
            change: self.change(Kind::Via, &net, None, at, owner, said),
            remove_wires: Vec::new(),
            remove_holes: vec![i],
            add: Vec::new(),
        })
    }

    fn stub(
        &self,
        f: &netsmith_geom::DrcFinding,
        complete: &BTreeSet<&str>,
        attempt: &dyn Fn(&Key) -> usize,
    ) -> Option<Candidate> {
        let item = f.items.first()?;
        let net = item.net.clone()?;
        if !complete.contains(net.as_str()) {
            return None;
        }
        let layer = CopperLayer::of_name(f.layer.as_deref()?)?;
        let at = Pt::new(f.at.x, f.at.y);
        let key = self.key(Kind::Stub, &net, Some(layer), at);
        if attempt(&key) > 0 {
            return None;
        }
        let start = Pt::new(item.at.x, item.at.y);
        let i = self
            .ends_at(&net, layer, at)
            .into_iter()
            .filter(|&i| self.editable(i) && self.copper.wires[i].track.from.distance(start) < NEAR)
            .min_by(|&a, &b| {
                length(&self.copper.wires[a].track).total_cmp(&length(&self.copper.wires[b].track))
            })?;
        let owner = self.copper.wires[i].owner;
        Some(Candidate {
            key,
            change: self.change(
                Kind::Stub,
                &net,
                Some(layer),
                at,
                owner,
                format!(
                    "removed a track of {:.3} mm with an end that touches nothing",
                    length(&self.copper.wires[i].track)
                ),
            ),
            remove_wires: vec![i],
            remove_holes: Vec::new(),
            add: Vec::new(),
        })
    }

    /// The corner of two straight tracks at the finding's place, cut.
    fn corner_at(
        &self,
        f: &netsmith_geom::DrcFinding,
        kind: Kind,
        attempt: &dyn Fn(&Key) -> usize,
    ) -> Option<Candidate> {
        let net = f.items.first()?.net.clone()?;
        let layer = CopperLayer::of_name(f.layer.as_deref()?)?;
        let at = Pt::new(f.at.x, f.at.y);
        let key = self.key(kind, &net, Some(layer), at);
        let size = *CHAMFERS.get(attempt(&key))?;
        let ends = self.ends_at(&net, layer, at);
        let [a, b] = ends[..] else { return None };
        if !self.editable(a) || !self.editable(b) {
            return None;
        }
        let (wa, wb) = (&self.copper.wires[a], &self.copper.wires[b]);
        if wa.owner != wb.owner {
            return None;
        }
        let far = |t: &Track| {
            if t.from.distance(at) < NEAR {
                t.to
            } else {
                t.from
            }
        };
        let (fa, fb) = (far(&wa.track), far(&wb.track));
        let width = wa.track.width.min(wb.track.width);
        let leg = fa.distance(at).min(fb.distance(at));
        let t = (size * width).min(0.45 * leg);
        let points = bend(fa, at, fb, t)?;
        if points.len() <= 3 {
            return None;
        }
        let bends = points.len() - 2;
        let add = self.polyline(&points, &wa.track, width);
        Some(Candidate {
            key,
            change: self.change(
                kind,
                &net,
                Some(layer),
                at,
                wa.owner,
                format!(
                    "cut to {bends} bends of {:.0} degrees or less, {:.3} mm from the corner",
                    bend_angle(fa, at, fb) / bends as f64,
                    t
                ),
            ),
            remove_wires: vec![a, b],
            remove_holes: Vec::new(),
            add,
        })
    }

    /// The tracks along `points`, of `like`'s net and layer and `width`, at the micrometre.
    fn polyline(&self, points: &[Pt], like: &Track, width: f64) -> Vec<Track> {
        let min = self.board.outline.min;
        let snap = |p: Pt| {
            let q = p - min;
            min + Pt::new(to_um(q.x), to_um(q.y))
        };
        points
            .windows(2)
            .map(|w| Track::straight(&like.net, like.layer, snap(w[0]), snap(w[1]), width))
            .collect()
    }

    fn acid_trap(
        &self,
        f: &netsmith_geom::DrcFinding,
        attempt: &dyn Fn(&Key) -> usize,
    ) -> Option<Candidate> {
        if f.message.starts_with("two tracks meet") {
            return self.corner_at(f, Kind::AcidTrap, attempt);
        }
        let pad = f.message.starts_with("a track leaves a pad");
        let via = f.message.starts_with("a track leaves a via");
        let tee = f.message.starts_with("a track meets another track");
        if !(pad || via || tee) {
            return None;
        }
        let item = f.items.first()?;
        let net = item.net.clone()?;
        let layer = CopperLayer::of_name(f.layer.as_deref()?)?;
        let at = Pt::new(f.at.x, f.at.y);
        let key = self.key(Kind::AcidTrap, &net, Some(layer), at);
        let tried = attempt(&key);
        let start = Pt::new(item.at.x, item.at.y);
        // The track of the trap: it starts where the finding's item does, and passes the place.
        let k = (0..self.copper.wires.len()).find(|&i| {
            let w = &self.copper.wires[i];
            w.alive
                && w.track.net == net
                && w.track.layer == layer
                && w.track.mid.is_none()
                && w.track.from.distance(start) < NEAR
                && segment_distance(at, w.track.from, w.track.to) < NEAR
        })?;
        if !self.editable(k) {
            return None;
        }
        let w = &self.copper.wires[k];
        let t = &w.track;
        let (end, other) = if t.from.distance(at) <= t.to.distance(at) {
            (t.from, t.to)
        } else {
            (t.to, t.from)
        };
        let way = unit(other - end);
        let width = t.width;
        let (normal, rise, depth, said) = if tee {
            if tried > 0 {
                return None;
            }
            // The other track: of the net and layer, straight, with the place on its middle.
            let on = f.items.get(1)?;
            let on_start = Pt::new(on.at.x, on.at.y);
            let m = self.copper.wires.iter().find(|o| {
                o.alive
                    && o.track.net == net
                    && o.track.layer == layer
                    && o.track.mid.is_none()
                    && o.track.from.distance(on_start) < NEAR
                    && segment_distance(at, o.track.from, o.track.to) < NEAR
            })?;
            let along = unit(m.track.to - m.track.from);
            let mut n = Pt::new(-along.y, along.x);
            if dot(n, way) < 0.0 {
                n = scale(n, -1.0);
            }
            let rise = m.track.width / 2.0 + 2.0 * width;
            // The foot must stay on the other track's middle.
            let foot = at + scale(way, rise / dot(way, n).max(1e-9)) - scale(n, rise);
            let span = m.track.to - m.track.from;
            let s = dot(foot - m.track.from, span) / dot(span, span).max(1e-18);
            let margin = (m.track.width.max(width)) / length_of(span).max(1e-9);
            if !(margin..=1.0 - margin).contains(&s) {
                return None;
            }
            (
                n,
                rise,
                0.0,
                "made it meet the other track at a right angle".to_string(),
            )
        } else {
            let n = if via {
                if tried > 0 {
                    return None;
                }
                let centre = f.items.get(1).map(|i| Pt::new(i.at.x, i.at.y))?;
                unit(at - centre)
            } else {
                // The angle to the pad's edge the check gives, to the whole degree: the edge's
                // normal is that far off the track, on one side or the other.
                let acute = degrees_in(&f.message)?;
                let off = 90.0 - acute;
                match tried {
                    0 => way.rotate(off),
                    1 => way.rotate(-off),
                    _ => return None,
                }
            };
            let depth = dot(at - end, n).max(0.0) / 2.0;
            (
                n,
                2.0 * width,
                depth.max(width / 2.0),
                format!(
                    "made it leave the {} at a right angle",
                    if via { "via" } else { "pad" }
                ),
            )
        };
        let cos = dot(way, normal);
        if cos < 0.1 {
            return None;
        }
        // Out from the edge by `rise` along the normal, on the track's line, then back to it.
        let s = rise / cos;
        if s > 0.8 * other.distance(at) {
            return None;
        }
        let turn = at + scale(way, s);
        let inner = turn - scale(normal, rise + depth);
        let room = rise.min(other.distance(turn)) * 0.45;
        let mut points = vec![inner];
        points.extend(
            bend(inner, turn, other, width.min(room))?
                .into_iter()
                .skip(1),
        );
        let add = self.polyline(&points, t, width);
        Some(Candidate {
            key,
            change: self.change(Kind::AcidTrap, &net, Some(layer), at, w.owner, said),
            remove_wires: vec![k],
            remove_holes: Vec::new(),
            add,
        })
    }

    /// Each pair of collinear straight tracks that meet end to end with nothing else there.
    fn merges(&self) -> Vec<Candidate> {
        let q = |p: Pt| ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64);
        type Joint = (String, &'static str, (i64, i64));
        let mut joints: BTreeMap<Joint, Vec<usize>> = BTreeMap::new();
        for (i, w) in self.copper.wires.iter().enumerate() {
            if !w.alive {
                continue;
            }
            for p in [w.track.from, w.track.to] {
                joints
                    .entry((w.track.net.clone(), w.track.layer.word(), q(p)))
                    .or_default()
                    .push(i);
            }
        }
        let mut out = Vec::new();
        for ((net, _, _), ends) in &joints {
            let [a, b] = ends[..] else { continue };
            if a == b || !self.editable(a) || !self.editable(b) {
                continue;
            }
            let (wa, wb) = (&self.copper.wires[a], &self.copper.wires[b]);
            if wa.owner != wb.owner || (wa.track.width - wb.track.width).abs() > 1e-9 {
                continue;
            }
            let layer = &wa.track.layer;
            let p = if wa.track.from.distance(wb.track.from) < NEAR
                || wa.track.from.distance(wb.track.to) < NEAR
            {
                wa.track.from
            } else {
                wa.track.to
            };
            let far = |t: &Track| {
                if t.from.distance(p) < NEAR {
                    t.to
                } else {
                    t.from
                }
            };
            let (fa, fb) = (far(&wa.track), far(&wb.track));
            let (u, v) = (unit(fa - p), unit(fb - p));
            if (u.x * v.y - u.y * v.x).abs() > 1e-6 || dot(u, v) > -0.999_999 {
                continue;
            }
            // No other track of the net on the layer passes over the joint (a tee onto it).
            let crossed = self.copper.wires.iter().enumerate().any(|(j, o)| {
                j != a
                    && j != b
                    && o.alive
                    && o.track.net == *net
                    && o.track.layer == *layer
                    && segment_distance(p, o.track.from, o.track.to) < NEAR
            });
            if crossed {
                continue;
            }
            // Kept in the first track's direction.
            let (from, to) = if wa.track.to.distance(p) < NEAR {
                (fa, fb)
            } else {
                (fb, fa)
            };
            let mut merged = wa.track.clone();
            merged.from = from;
            merged.to = to;
            out.push(Candidate {
                key: self.key(Kind::Merge, net, Some(*layer), p),
                change: self.change(
                    Kind::Merge,
                    net,
                    Some(*layer),
                    p,
                    wa.owner,
                    format!("made one track of {:.3} mm of two", from.distance(to)),
                ),
                remove_wires: vec![a, b],
                remove_holes: Vec::new(),
                add: vec![merged],
            });
        }
        out
    }
}

/// `c` made on `copper`: what it removes marked so, what it adds written in the place of the
/// first track it removes.
fn apply(board: &Board, copper: &mut Copper, c: &Candidate) {
    let rank = c
        .remove_wires
        .iter()
        .map(|&i| copper.wires[i].rank)
        .min()
        .unwrap_or((usize::MAX, 0));
    let owner = c
        .remove_wires
        .first()
        .map(|&i| copper.wires[i].owner)
        .unwrap_or(Owner::RoutesFile);
    for &i in &c.remove_wires {
        copper.wires[i].alive = false;
    }
    for &i in &c.remove_holes {
        copper.holes[i].alive = false;
    }
    for (n, t) in c.add.iter().enumerate() {
        let drawn = match owner {
            Owner::RoutesFile => RouteTrack::of(board, t).on(board, &t.net),
            Owner::Kept => t.clone(),
        };
        copper.wires.push(Wire {
            owner,
            track: t.clone(),
            drawn,
            was: None,
            fixed: false,
            alive: true,
            widened: c.change.kind == Kind::Widen,
            rank: (rank.0, rank.1 + n + 1),
        });
    }
}

/// The width a track of `net` on `layer` is widened to by `--widen` (issue #1294): the width the
/// router draws the net at there ([`Board::track_width_on`]: its rails' width, never less than
/// the rules' track width, the width an inner layer needs for its current, or the width of its
/// impedance target). A net with pins that state the current of their branch (issue #1113) has
/// lines drawn at the branch's width as well as at the net's, and a track is not told which
/// line it is of: it is widened to the narrowest of those, so that no branch is made wider
/// than the router would draw it.
pub fn wanted_width(board: &Board, net: &str, layer: CopperLayer) -> f64 {
    let width = board.track_width_on(net, layer);
    if board.impedance_widths.iter().any(|w| w.net == net) {
        return width;
    }
    board
        .branch_currents
        .iter()
        .filter(|b| b.net == net)
        .map(|b| {
            let w = b.width.max(board.rules.track_width);
            match b.inner_least_width.filter(|_| !layer.is_outer()) {
                Some(least) => w.max((least * 100.0 - 1e-9).ceil() / 100.0),
                None => w,
            }
        })
        .fold(width, f64::min)
}

/// Why the board `after` is refused against `before`: its first new finding, or the connections
/// left that changed; `None` when it is allowed.
fn refusal(before: &Look, after: &Look) -> Option<String> {
    if after.lines != before.lines {
        return lines_refusal(&before.lines, &after.lines);
    }
    keys_refusal(&before.keys, &after.keys)
}

/// Why connections left `after` are refused against `before`: the first that is new.
fn lines_refusal(before: &[String], after: &[String]) -> Option<String> {
    if after == before {
        return None;
    }
    let lost: Vec<&String> = after.iter().filter(|l| !before.contains(l)).collect();
    Some(match lost.first() {
        Some(l) => format!("it leaves a connection open: {l}"),
        None => "it changes the connections left to route".to_string(),
    })
}

/// Why findings `after` (keys, sorted) are refused against `before`: the first that is new.
fn keys_refusal(before: &[String], after: &[String]) -> Option<String> {
    let mut old = before.to_vec();
    let mut new: Vec<&String> = Vec::new();
    for k in after {
        match old.binary_search(k) {
            Ok(i) => {
                old.remove(i);
            }
            Err(_) => new.push(k),
        }
    }
    let first = new.first()?;
    Some(format!(
        "the design rules check finds {} new finding(s) with it, the first: {first}",
        new.len()
    ))
}

/// A finding as the rule compares it: its severity, rule, message and layer, with the lengths
/// of the tracks it names left out, and its place in the routes file's frame.
fn finding_key(f: &netsmith_geom::DrcFinding, board: &Board) -> String {
    let at = Pt::new(f.at.x, f.at.y) - board.outline.min;
    format!(
        "{} [{}] {} at ({:.3}, {:.3}){}",
        f.severity.word(),
        f.rule.id(),
        without_lengths(&f.message),
        at.x,
        at.y,
        f.layer
            .as_ref()
            .map(|l| format!(" on {l}"))
            .unwrap_or_default()
    )
}

/// `text` without the `, length 1.2345 mm` the check gives each track it names.
fn without_lengths(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(", length ") {
        out.push_str(&rest[..i]);
        let after = &rest[i + ", length ".len()..];
        let digits = after
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .unwrap_or(after.len());
        match after[digits..].strip_prefix(" mm") {
            Some(tail) if digits > 0 => rest = tail,
            _ => {
                out.push_str(", length ");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The angle a finding's message gives, `... at 45 degrees ...`.
fn degrees_in(message: &str) -> Option<f64> {
    let i = message.find(" degrees")?;
    let head = &message[..i];
    let j = head.rfind(" at ")?;
    head[j + 4..].trim().parse().ok()
}

/// The polyline from `a` through the corner `p` to `b` with the corner cut, starting `t` from
/// it along each track, so that it bends by 45 degrees at most at each point: `[a, p, b]` when
/// it bends by that little already, `None` when it doubles back on itself.
fn bend(a: Pt, p: Pt, b: Pt, t: f64) -> Option<Vec<Pt>> {
    let turn = bend_angle(a, p, b);
    if turn <= 45.0 + 1e-6 {
        return Some(vec![a, p, b]);
    }
    if turn >= 180.0 - 1e-6 || t <= 1e-6 {
        return None;
    }
    let (ua, ub) = (unit(a - p), unit(b - p));
    let bends = (turn / 45.0 - 1e-9).ceil() as usize;
    let step = turn / bends as f64;
    let heading = scale(ua, -1.0);
    let cross = heading.x * ub.y - heading.y * ub.x;
    // `Pt::rotate` turns counterclockwise as seen on screen (y down): `rotate(-d)` turns the way
    // of a positive cross product in these coordinates.
    let sign = if cross > 0.0 { -1.0 } else { 1.0 };
    let ways: Vec<Pt> = (1..bends)
        .map(|i| heading.rotate(sign * step * i as f64))
        .collect();
    let sum = ways.iter().fold(Pt::new(0.0, 0.0), |s, w| s + *w);
    let start = p + scale(ua, t);
    let end = p + scale(ub, t);
    let long = length_of(end - start) / length_of(sum).max(1e-12);
    let mut points = vec![a, start];
    let mut at = start;
    for w in &ways {
        at = at + scale(*w, long);
        points.push(at);
    }
    if at.distance(end) > 1e-6 {
        return None;
    }
    *points.last_mut().expect("a point") = end;
    points.push(b);
    Some(points)
}

/// How far the way turns at `p`, from `a` through `p` to `b`, in degrees: 0 straight on.
fn bend_angle(a: Pt, p: Pt, b: Pt) -> f64 {
    let (u, v) = (unit(a - p), unit(b - p));
    180.0 - dot(u, v).clamp(-1.0, 1.0).acos().to_degrees()
}

fn unit(p: Pt) -> Pt {
    scale(p, 1.0 / length_of(p).max(1e-18))
}

fn scale(p: Pt, k: f64) -> Pt {
    Pt::new(p.x * k, p.y * k)
}

fn dot(a: Pt, b: Pt) -> f64 {
    a.x * b.x + a.y * b.y
}

fn length_of(p: Pt) -> f64 {
    p.x.hypot(p.y)
}

fn length(t: &Track) -> f64 {
    t.from.distance(t.to)
}

/// The distance from `p` to the segment from `a` to `b`.
fn segment_distance(p: Pt, a: Pt, b: Pt) -> f64 {
    let ab = b - a;
    let s = (dot(p - a, ab) / dot(ab, ab).max(1e-18)).clamp(0.0, 1.0);
    p.distance(a + scale(ab, s))
}

fn to_um(v: f64) -> f64 {
    (v * 1e6).round() / 1e6 + 0.0
}

impl KeptRoutes {
    /// Write `kept` over its board's kept file (what [`KeptRoutes::write`] writes, made
    /// elsewhere: `routes tidy --reuse-routes`). Gives the file written.
    pub fn save(&self, kept: &Kept) -> Result<PathBuf, String> {
        let file = self.file(&kept.board);
        let text = toml::to_string(kept).map_err(|e| format!("{}: {e}", file.display()))?;
        crate::routes::write_atomic(&file, &text)
            .map_err(|e| format!("{}: {e}", file.display()))?;
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_pcb::NetRoutes;

    /// The LED example placed, a fixture of `netsmith-pcb` (15 by 7 mm; nets GND, VCC and
    /// `net-led.anode`; R1's pad 2 at (9.61, 4.4) and D1's pad 2 at (13.3875, 4.2) on top).
    pub(super) fn led() -> Board {
        serde_json::from_str(include_str!(
            "../../netsmith-pcb/tests/fixtures/led.board.json"
        ))
        .expect("a fixture is the JSON of a Board")
    }

    pub(super) const W: f64 = 0.15;

    pub(super) fn track(layer: CopperLayer, from: [f64; 2], to: [f64; 2]) -> RouteTrack {
        RouteTrack {
            layer,
            width: W,
            from,
            mid: None,
            to,
            lock: false,
        }
    }

    /// Straight tracks through `points`, one after another.
    pub(super) fn path(layer: CopperLayer, points: &[[f64; 2]]) -> Vec<RouteTrack> {
        points
            .windows(2)
            .map(|p| track(layer, p[0], p[1]))
            .collect()
    }

    pub(super) fn via(at: [f64; 2]) -> RouteVia {
        RouteVia {
            at,
            diameter: 0.45,
            drill: 0.2,
            layers: Vec::new(),
            lock: false,
        }
    }

    pub(super) fn routes_of(
        board: &Board,
        records: Vec<(&str, bool, Vec<RouteTrack>, Vec<RouteVia>)>,
    ) -> Routes {
        let mut routes = Routes::default();
        let table = routes.boards.entry(board.name.clone()).or_default();
        for (net, lock, tracks, vias) in records {
            table.nets.insert(
                net.to_string(),
                NetRoutes {
                    pads: netsmith_pcb::pad_hash(board, net).unwrap(),
                    lock,
                    tracks,
                    vias,
                    zones: Vec::new(),
                },
            );
        }
        routes
    }

    pub(super) fn findings(board: &Board, routes: &Routes) -> Vec<String> {
        let mut w = Work::new(board, routes, None).unwrap();
        // The board as the file draws it, checked.
        let copper = w.copper.clone();
        w.look(&copper).unwrap().keys
    }

    /// [`tidy`], which checks neighbourhoods, and the same with every group checked on the whole
    /// board (issue #1225): the same changes, refusals and copper.
    pub(super) fn tidied(board: &Board, routes: &Routes, kept: Option<KeptCopper<'_>>) -> Tidied {
        let kept2 = kept.as_ref().map(|k| KeptCopper {
            kept: k.kept,
            stale: k.stale,
        });
        let near = tidy(board, routes, kept).unwrap();
        let whole = tidy_with(board, routes, kept2, Checking::Whole).unwrap();
        assert_eq!(
            near.changes,
            whole.changes,
            "{}\n{}",
            near.text(),
            whole.text()
        );
        assert_eq!(
            near.refused,
            whole.refused,
            "{}\n{}",
            near.text(),
            whole.text()
        );
        assert_eq!(near.routes, whole.routes);
        assert_eq!(near.kept, whole.kept);
        assert_eq!(near.findings_after, whole.findings_after);
        near
    }

    /// The board the checks put the copper on is the board `with_routes` makes of the routes
    /// file the copper writes, with the kept copper after it, track for track.
    fn same_board(board: &Board, routes: &Routes, kept: Option<KeptCopper<'_>>) {
        let w = Work::new(board, routes, kept.as_ref()).unwrap();
        let mut by_file = with_routes(board, &w.routes_of(&w.copper)).unwrap().0;
        for wire in w.copper.wires.iter().filter(|x| x.owner == Owner::Kept) {
            by_file.tracks.push(wire.track.clone());
        }
        for h in w.copper.holes.iter().filter(|x| x.owner == Owner::Kept) {
            by_file.vias.push(h.via.clone());
        }
        assert_eq!(w.board_of(&w.copper), by_file);
    }

    pub(super) fn lines(board: &Board, routes: &Routes) -> Vec<String> {
        let b = with_routes(board, routes).unwrap().0;
        let mut l: Vec<String> = netsmith_pcb::Ratsnest::of(&b)
            .lines
            .iter()
            .map(ToString::to_string)
            .collect();
        l.sort();
        l
    }

    const BOTTOM: CopperLayer = CopperLayer::Back;

    #[test]
    fn a_corner_an_acid_trap_a_collinear_pair_and_a_dangling_via_are_tidied_and_a_locked_net_kept()
    {
        let board = led();
        let gnd = {
            let mut t = path(BOTTOM, &[[2.0, 0.8], [5.0, 0.8], [5.0, 2.0]]);
            t.extend(path(BOTTOM, &[[6.0, 2.5], [7.0, 2.5], [8.5, 2.5]]));
            t.push(track(BOTTOM, [8.0, 0.8], [12.0, 0.8]));
            // A tee onto the middle of that track at 45 degrees: an acid trap.
            t.push(track(BOTTOM, [9.0, 2.0], [10.2, 0.8]));
            t
        };
        let vcc = path(BOTTOM, &[[0.8, 1.2], [0.8, 2.4], [1.8, 2.4]]);
        let routes = routes_of(
            &board,
            vec![
                ("GND", false, gnd, vec![via([13.5, 2.0])]),
                ("VCC", true, vcc, Vec::new()),
            ],
        );
        let before = findings(&board, &routes);
        assert!(
            before
                .iter()
                .any(|f| f.contains("[track_angle] two tracks of net GND")),
            "{before:#?}"
        );
        assert!(
            before.iter().any(|f| f.contains("[acid_trap]")),
            "{before:#?}"
        );

        same_board(&board, &routes, None);
        let tidied = tidied(&board, &routes, None);
        let said = tidied.text();
        assert_eq!(
            tidied.counts,
            Counts {
                corners: 1,
                merges: 1,
                vias: 1,
                stubs: 0,
                acid_traps: 1,
                widened: 0,
                ..Counts::default()
            },
            "{said}"
        );
        assert!(tidied.refused.iter().all(|c| c.net != "GND"), "{said}");

        // The check finds nothing new, and the connections are the same.
        let after = findings(&board, &tidied.routes);
        let mut old = before.clone();
        for f in &after {
            let i = old.iter().position(|o| o == f);
            assert!(i.is_some(), "a new finding: {f}\n{said}");
            old.remove(i.unwrap());
        }
        assert!(
            !after.iter().any(|f| f.contains("two tracks of net GND")),
            "{after:#?}"
        );
        assert!(
            !after.iter().any(|f| f.contains("[acid_trap]")),
            "{after:#?}"
        );
        assert_eq!(lines(&board, &tidied.routes), lines(&board, &routes));
        let errors = crate::routes::new_errors(
            &with_routes(&board, &routes).unwrap().0,
            &with_routes(&board, &tidied.routes).unwrap().0,
        )
        .unwrap();
        assert!(errors.is_empty(), "{errors:#?}");

        // The locked net is as it was, its right angle too.
        let table = |r: &Routes| r.boards[&board.name].clone();
        assert_eq!(
            table(&tidied.routes).nets["VCC"],
            table(&routes).nets["VCC"]
        );
        assert!(after.iter().any(|f| f.contains("two tracks of net VCC")));
        let gnd = &table(&tidied.routes).nets["GND"];
        assert!(gnd.vias.is_empty(), "{gnd:#?}");
        // Corner 2 + 1 chamfer, the pair as one, the long track, the tee as a leg and the rest.
        assert_eq!(gnd.tracks.len(), 3 + 1 + 1 + 2, "{gnd:#?}");
        assert!(
            gnd.tracks
                .iter()
                .any(|t| t.from == [6.0, 2.5] && t.to == [8.5, 2.5]),
            "{gnd:#?}"
        );
        // A second pass has nothing to do.
        same_board(&board, &tidied.routes, None);
        let again = tidy(&board, &tidied.routes, None).unwrap();
        assert_eq!(again.changes, Vec::new(), "{}", again.text());
        assert_eq!(again.routes, tidied.routes);
    }

    #[test]
    fn a_change_that_adds_a_finding_is_refused_and_the_file_is_untouched() {
        let board = led();
        // A short of VCC across the corner's inside: every chamfer of it crosses the short.
        let routes = routes_of(
            &board,
            vec![
                (
                    "GND",
                    false,
                    path(BOTTOM, &[[2.0, 0.8], [5.0, 0.8], [5.0, 2.0]]),
                    Vec::new(),
                ),
                (
                    "VCC",
                    false,
                    vec![track(BOTTOM, [4.75, 1.05], [4.95, 0.85])],
                    Vec::new(),
                ),
            ],
        );
        let dir = netsmith_testkit::TempDir::new("routes-tidy-refused");
        let layout = dir.join("b.pcb.toml");
        let file = beside_layout(&layout);
        save(&file, &routes).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        let _ = tidied(&board, &routes, None);
        let tidied = tidy_files(&board, &layout, false, false, false).unwrap();
        let said = tidied.text();
        assert_eq!(tidied.changes, Vec::new(), "{said}");
        assert_eq!(tidied.refused.len(), 1, "{said}");
        let why = tidied.refused[0].why.clone().unwrap();
        assert!(why.contains("new finding"), "{said}");
        assert_eq!(tidied.refused[0].kind, Kind::Corner);
        // Each chamfer was tried, the largest first.
        assert!(tidied.checks > CHAMFERS.len(), "{said}");
        assert!(tidied.wrote.is_empty(), "{said}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
    }

    #[test]
    fn a_stub_of_a_finished_net_goes_and_a_track_leaves_its_pad_straight_out() {
        let board = led();
        // R1.2 to D1.2 over the top: it leaves R1.2's top side at 73 degrees, an acid trap, and
        // has a stub up from its middle.
        let mut anode = path(
            CopperLayer::Front,
            &[[9.61, 4.4], [10.0475, 3.0], [13.3875, 3.0], [13.3875, 4.2]],
        );
        anode.push(track(CopperLayer::Front, [12.0, 3.0], [12.0, 2.0]));
        let routes = routes_of(&board, vec![("net-led.anode", false, anode, Vec::new())]);
        let before = findings(&board, &routes);
        assert!(
            before
                .iter()
                .any(|f| f.contains("[acid_trap] a track leaves a pad")),
            "{before:#?}"
        );
        let tidied = tidied(&board, &routes, None);
        let said = tidied.text();
        assert_eq!(tidied.counts.stubs, 1, "{said}");
        assert_eq!(tidied.counts.acid_traps, 1, "{said}");
        assert_eq!(tidied.counts.corners, 2, "{said}");
        let after = findings(&board, &tidied.routes);
        assert!(
            !after.iter().any(|f| f.contains("[acid_trap]")),
            "{after:#?}"
        );
        assert!(
            !after.iter().any(|f| f.contains("[track_angle]")),
            "{after:#?}"
        );
        assert!(
            !after.iter().any(|f| f.contains("track_dangling")),
            "{after:#?}"
        );
        assert_eq!(lines(&board, &tidied.routes), lines(&board, &routes));
    }

    #[test]
    fn the_routers_kept_copper_is_tidied_with_the_routes_file() {
        let board = led();
        let routes = Routes::default();
        let start = with_routes(&board, &routes).unwrap().0;
        let mut routed = start.clone();
        let at = |x: f64, y: f64| board.outline.min + Pt::new(x, y);
        routed.tracks.push(Track::straight(
            "GND",
            BOTTOM,
            at(2.0, 0.8),
            at(5.0, 0.8),
            W,
        ));
        routed.tracks.push(Track::straight(
            "GND",
            BOTTOM,
            at(5.0, 0.8),
            at(5.0, 2.0),
            W,
        ));
        let kept = Kept::of(&start, &routed, "led.nsm");
        let stale = BTreeSet::new();
        let with = || {
            Some(KeptCopper {
                kept: &kept,
                stale: &stale,
            })
        };
        same_board(&board, &routes, with());
        let tidied = tidied(&board, &routes, with());
        assert_eq!(tidied.counts.corners, 1, "{}", tidied.text());
        assert_eq!(tidied.changes[0].copper, Owner::Kept);
        let new = tidied.kept.unwrap();
        assert_eq!(new.nets["GND"].tracks.len(), 3);
        assert_eq!(new.routes, kept.routes);
        assert_eq!(tidied.routes, routes);
    }

    #[test]
    fn a_cancelled_pass_writes_nothing_and_one_not_cancelled_writes() {
        // Issue #1225 (ADR 0062): a cancel at any check of the pass leaves the file as it was.
        let board = led();
        let routes = routes_of(
            &board,
            vec![(
                "GND",
                false,
                path(BOTTOM, &[[2.0, 0.8], [5.0, 0.8], [5.0, 2.0]]),
                Vec::new(),
            )],
        );
        let dir = netsmith_testkit::TempDir::new("routes-tidy-cancel");
        let layout = dir.join("b.pcb.toml");
        let file = beside_layout(&layout);
        save(&file, &routes).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        let token = job::Cancel::new();
        let whole = job::scope(&token, || tidy_files(&board, &layout, false, false, true)).unwrap();
        let reached = token.checkpoints();
        assert!(reached > 2, "{reached} checkpoint(s): {}", whole.text());
        for at in [1, reached / 2, reached] {
            let token = job::Cancel::at_checkpoint(at);
            let ended = job::catch(|| {
                job::scope(&token, || tidy_files(&board, &layout, false, false, false))
            });
            assert!(ended.is_err(), "cancelled at checkpoint {at} of {reached}");
            assert_eq!(token.checkpoints(), at, "no work after the cancel");
            assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
        }
        let done = tidy_files(&board, &layout, false, false, false).unwrap();
        assert_eq!(done.counts.corners, 1, "{}", done.text());
        assert_ne!(std::fs::read_to_string(&file).unwrap(), text);
    }

    #[test]
    fn a_right_angle_is_cut_by_one_chamfer_and_an_acute_one_by_two() {
        let p = Pt::new(0.0, 0.0);
        let right = bend(Pt::new(-2.0, 0.0), p, Pt::new(0.0, 2.0), 0.5).unwrap();
        assert_eq!(right.len(), 4, "{right:?}");
        assert!(right[1].distance(Pt::new(-0.5, 0.0)) < 1e-9, "{right:?}");
        assert!(right[2].distance(Pt::new(0.0, 0.5)) < 1e-9, "{right:?}");
        for w in right.windows(3) {
            assert!(bend_angle(w[0], w[1], w[2]) <= 45.0 + 1e-6, "{right:?}");
        }
        let acute = bend(Pt::new(-2.0, 0.0), p, Pt::new(-2.0, 2.0), 0.3).unwrap();
        assert_eq!(acute.len(), 5, "{acute:?}");
        for w in acute.windows(3) {
            assert!(bend_angle(w[0], w[1], w[2]) <= 45.0 + 1e-6, "{acute:?}");
        }
        // A 45 degree bend is left as it is.
        let gentle = bend(Pt::new(-2.0, 0.0), p, Pt::new(1.0, 1.0), 0.3).unwrap();
        assert_eq!(gentle.len(), 3);
    }

    #[test]
    fn a_finding_is_compared_without_the_lengths_of_its_tracks() {
        assert_eq!(
            without_lengths("Track [GND] on B.Cu, length 3.0000 mm has an end"),
            "Track [GND] on B.Cu has an end"
        );
        assert_eq!(without_lengths("a, length of it"), "a, length of it");
        assert_eq!(
            degrees_in("a track leaves a pad at 73 degrees on F.Cu"),
            Some(73.0)
        );
    }
}
