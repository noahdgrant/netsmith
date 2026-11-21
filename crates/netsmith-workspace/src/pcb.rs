//! The PCB view: place a board, route it when asked, and draw it (ADR 0030, section e).
//!
//! The page edits the layout file and the server only checks and writes it. A part is
//! selected by click, box, Shift or Ctrl and the parts list, and shows its pads, nets and
//! ratsnest; it is moved by dragging on a grid (a setting of the page), turned (`R`),
//! flipped (`F`), locked (`L`, drawn as a padlock) and handed back to the placer (`Delete`),
//! and every edit can be undone and redone (`Ctrl+Z`, `Ctrl+Shift+Z`). Keepouts are drawn
//! with a rectangle tool, moved, resized and deleted, and written to the board's physical file,
//! as requirements are (ADR 0048, `keepouts.rs`); the outline and the cutouts are the source's
//! and are only drawn. The keys and the tooltips are listed in
//! `.agents/skills/netsmith-dev-verify-editor/features/pcb.md`.
//!
//! **The edit protocol.** The page keeps the history and the server keeps none. An edit is a
//! `PcbEdit` of `PcbChange`s, each saying what an entry of the layout file was and what it
//! becomes (`POST /api/pcb/edit`, typed in `api.rs`), so the inverse of an edit is the same
//! edit turned round. The server answers 409 when the file, or an entry, is not as the page
//! last saw it, and 400 when the board does not take the result: it is placed with the new
//! layout first, and the file is written only then, atomically. `GET /api/pcb/scene` has the
//! drawing (`render_svg_view`, with `data-ref`, `data-path`, `data-pad`, `data-net`,
//! `data-keepout`, `data-cutout`, and `data-from` and `data-to` on the ratsnest lines) and
//! the data that finds things in it. An edit throws the routed copper away, and a flip changes
//! what the derived placer is told, so parts nobody placed by hand may move.
//!
//! **Regenerate and partial routing** (issue #825). `POST /api/pcb/regenerate` places one module
//! again with the rest held ([`regenerate`], `netsmith_fabricate::regenerate`) and answers with a
//! proposal, the edit that makes it among it; the page sends that edit as any other, one step of
//! undo. The workspace keeps the last [`ROUTED_KEPT`] routed boards of each board
//! ([`RoutedBoards`]): a route starts from the board routed last and keeps the copper of every net
//! the change left alone (`netsmith_router::carry`), and an edit or a route whose board is one
//! routed before takes that copper again with no router (an undo). The router's copper is kept on
//! disk too (`netsmith_fabricate::kept_routes`, issue #1019): a route that ran to its end writes
//! it, and a route with no routed board in memory (a new session) carries from it.
//!
//! **Findings** (`GET /api/pcb/findings`, `POST /api/pcb/drc`): netsmith's own placement
//! problems, the connections left on a routed board, netsmith's design rules check of the
//! routed copper (ADR 0037, source `drc`) and KiCad's check as a second opinion (its JSON
//! report, run on demand and kept until the layout changes), each with a point and a box. A
//! click zooms to it and marks it, and the list follows the edits.
//!
//! **The routes file** (`<stem>.routes.toml`, epic #599) is read with the layout file and its
//! copper put on the placed board before anything is routed, as `fab` does. The scene lists its
//! records with what of them is drawn, a record not used as written is a `routes` finding, and
//! `POST /api/pcb/routes` draws and moves tracks and vias, deletes, locks and unlocks through
//! `netsmith_fabricate::routes::edit_all`, the code of the `netsmith routes` commands, on the
//! board as placed with no copper; the board is then made again from the file, unrouted. The
//! answer carries the net's record before and after as text, which the page keeps as a step of
//! undo and sends back turned round (`Restore`). The routes file is watched with the layout
//! file.
//!
//! **Walkaround** (`POST /api/pcb/walk`, issue #726): while a track is drawn the page asks, for
//! the point under the pointer, the way from the track's last corner round the copper there
//! ([`walk`], `netsmith_router::walk`). The board it asks about is the one the check reads when
//! the track is ended ([`Entry::board_for_walk`]: as placed with the routes file's copper, not
//! the router's), so the corners it offers are ones the check accepts (the check still decides
//! when the track is ended). Writes nothing. The router's copper is no obstacle to it, since
//! the edit discards it ([`Entry::router_copper`]); the answer says which of it the track
//! crosses, for the page to show it giving way ([`PcbWalked::crossed`], the second slice of
//! #726). With `shove` the way is found through the unlocked tracks and vias of the routes file
//! ([`Entry::board_for_shove`]) and those are moved out of its way (`netsmith_router::shove`, the
//! third slice): the answer carries their new places ([`PcbWalked::shoved`]) for the page to preview
//! and to send, with the track, as one edit of several nets (an undo for each net in the answer, one
//! step in the page); where a shape cannot move far enough the way goes round all of them and the
//! answer says which ([`PcbWalked::stuck`]).
//!
//! **Not built** (ADR 0030, issue #59): waivers (the page has no way to write one), a copy of the
//! findings for an agent, and `layout show` overlays.

use std::{
    collections::{BTreeSet, HashMap},
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use netsmith_core::job::{self, Cancel};
use netsmith_editor::{
    ApiError, Say, Show,
    api::{
        ComparePcbPlace, CompareSize, DrawnKeepout, KeepoutOrigin, KeepoutPlacement, KicadState,
        PartPlacement, PcbBox, PcbChange, PcbCopper, PcbCrossed, PcbCrossedTrack, PcbCrossedVia,
        PcbCutout, PcbFill, PcbFinding, PcbFindings, PcbFit, PcbFitFault, PcbFitMiss, PcbFitNear,
        PcbFitPart, PcbFitPlace, PcbKeepout, PcbLive, PcbLivePart, PcbLiveTrack, PcbLiveVia,
        PcbNetWidth, PcbNets, PcbPad, PcbPart, PcbPoint, PcbRegenerate, PcbRegenerated,
        PcbRegeneratedPart, PcbReport, PcbRequirement, PcbRerouted, PcbRouteNet, PcbRouteTrack,
        PcbRouteVia, PcbScene, PcbShapeRef, PcbShove, PcbShoveAnswer, PcbShoved, PcbShovedTrack,
        PcbShovedVia, PcbSide, PcbStitching, PcbSummary, PcbTotals, PcbViaOffer, PcbWalk,
        PcbWalked, RequiredPlace, RoutesChange, Severity, StitchingVia,
    },
};
use netsmith_pcb::{
    Board, BoardLayout, CopperLayer, KeepoutPlace, Layout, LayoutFace, PartPlace, Rect, Routes,
    RoutesNote, Side, copper_layers, render_svg_view,
};
use netsmith_router::{RouteOptions, route_with};
use std::path::{Path, PathBuf};

use super::{
    analysis::BoardData,
    diff::{PcbDigest, PcbPartDigest},
    findings::{self, KicadResult},
    progress::{Reporting, forward},
};
use netsmith_fabricate::fab::DEFAULT_ROUTE_TIMEOUT;

/// Set to stop the route that is running; cleared when the next starts.
static CANCEL: AtomicBool = AtomicBool::new(false);

/// Stops the route that is running, if any: it keeps what it has routed.
pub fn cancel() {
    CANCEL.store(true, Ordering::Relaxed);
}

/// A board as placed, and as routed once that was asked for.
pub struct Entry {
    pub board: Board,
    routed: bool,
    stopped: bool,
    unrouted: Vec<String>,
    elapsed: Duration,
    /// The layout file the board was placed with, and the name to quote it by.
    layout: Option<(Layout, String)>,
    /// What the layout file and the board's physical file were like when they were read
    /// ([`stamp`]).
    layout_stamp: String,
    /// What KiCad's check found on the board as it is, if it was run: an edit places the
    /// board again, which forgets it.
    pub kicad: Option<KicadResult>,
    /// What netsmith's design rules check finds on the routed board: made when first asked
    /// for, and forgotten when the board is routed again.
    drc: OnceLock<Vec<PcbFinding>>,
    /// The grounds the track tool's walks and shoves are asked on, built when first asked for
    /// (issue 1009). Placed board and routes file only: routing does not change them.
    grounds: OnceLock<std::sync::Arc<Grounds>>,
    shove_grounds: OnceLock<std::sync::Arc<ShoveGrounds>>,
    /// What the design requires of where the parts go (ADR 0045), if it says anything.
    constraints: Option<netsmith_geom::Constraints>,
    /// Those requirements measured on the placed board: made when first asked for. Routing
    /// does not move a part, so it stays.
    constraint_findings: OnceLock<Vec<PcbFinding>>,
    /// The board's fit in its case models (ADR 0109), measured when it is placed: routing does
    /// not move a part, so it stays.
    case_findings: Vec<PcbFinding>,
    /// The rounds of rip-up the router gives a try that leaves connections (`rip_rounds` in
    /// `netsmith.toml`, issue #1088).
    rip_rounds: usize,
    /// The weights of the router's quality objective (the project's keys, issue 1536).
    weights: Option<netsmith_router::Weights>,
    /// Where the placer refused the layout file and the board is shown where it ended (issue
    /// #779): that it was refused and the lines and locks that cannot all hold. Empty for a
    /// board that was placed.
    refusal: Vec<PcbFinding>,
    /// What the placer was told of where the parts go, and why it refused the layout file when
    /// it did: what the KiCad download measures the placement's findings with (issue #1001).
    place_options: netsmith_pcb::PlaceOptions,
    refused: Option<String>,
    /// The board as placed, before the routes file's copper is put on it: what a change to the
    /// routes file is made and checked on, as `netsmith routes` does.
    placed: Board,
    /// The routes file beside the layout file (epic #599), if the board has a layout file.
    routes_file: Option<PathBuf>,
    /// What it was like when read, its records, and what of them was not used as written.
    routes_stamp: String,
    routes: Routes,
    routes_notes: Vec<RoutesNote>,
    /// The nets whose router copper the last route kept from the board routed before the
    /// change ([`netsmith_router::carry`]), and the nets it routed again; empty when it routed
    /// from nothing.
    carried: Vec<String>,
    rerouted: Vec<String>,
}

/// How many routed boards of each board the editor keeps ([`RoutedBoards`]).
pub const ROUTED_KEPT: usize = 4;

/// A board as the router left it, with the board it started from (placed, with the routes
/// file's copper): a route of a board equal to `start` is this one again, with no router.
#[derive(Clone)]
pub struct RoutedBoard {
    start: Board,
    board: Board,
    unrouted: Vec<String>,
    stopped: bool,
    carried: Vec<String>,
    rerouted: Vec<String>,
}

/// The last [`ROUTED_KEPT`] routed boards of one board, the newest last: what partial routing
/// starts from, and what an undo back to a placement routed before shows again at once (issue
/// #825). Kept across edits and readings of the files; a board is only taken again when it
/// starts from exactly the same board.
#[derive(Default)]
pub struct RoutedBoards {
    list: std::collections::VecDeque<RoutedBoard>,
}

impl RoutedBoard {
    /// The board it started from: placed, with the routes file's copper.
    pub fn start(&self) -> &Board {
        &self.start
    }

    /// The board as the router left it.
    pub fn board(&self) -> &Board {
        &self.board
    }
}

impl RoutedBoards {
    /// The routed board that started from `start`, if one is kept.
    pub fn find(&self, start: &Board) -> Option<&RoutedBoard> {
        self.list.iter().rev().find(|r| &r.start == start)
    }

    /// The board routed last, to carry the copper of from: only one of the same board name,
    /// outline and rules, so that copper drawn for other rules is never kept.
    pub fn last_like(&self, board: &Board) -> Option<&Board> {
        self.list.back().map(|r| &r.board).filter(|b| {
            b.name == board.name && b.outline == board.outline && b.rules == board.rules
        })
    }

    /// Keep `routed`, dropping the oldest past [`ROUTED_KEPT`] and one that started from the
    /// same board.
    pub fn keep(&mut self, routed: RoutedBoard) {
        self.list.retain(|r| r.start != routed.start);
        self.list.push_back(routed);
        while self.list.len() > ROUTED_KEPT {
            self.list.pop_front();
        }
    }
}

/// Why a board could not be placed.
pub enum PlaceError {
    /// The layout does not fit the board: the file is out of date, or an edit is wrong.
    Layout(String),
    /// The requirements provably cannot be met, found before the placer ran (ADR 0045
    /// section 14): the board is not placed, and the page lists the findings.
    Unplaceable(String, Box<netsmith_pcb::Precheck>),
    Other(ApiError),
}

impl PlaceError {
    /// The error for a page that only reads: a layout file that does not fit is the project's
    /// mistake, not the request's.
    pub fn read(self) -> ApiError {
        match self {
            PlaceError::Layout(message) => ApiError::unprocessable(message, "layout_mismatch", &[]),
            PlaceError::Unplaceable(name, found) => ApiError::unprocessable(
                format!("{name}: {}", found.errors().join("\n")),
                "unplaceable",
                &[("board", &name)],
            ),
            PlaceError::Other(e) => e,
        }
    }
}

/// The board of `name` placed: slow on a big board, so it is done with no lock held. The layout
/// file beside the source, if there is one, tells the placer which face each part is on and
/// what to keep clear of, and puts the positions it holds on the board.
pub fn place(data: &BoardData, name: &str) -> Result<Entry, ApiError> {
    place_checked(data, name).map_err(PlaceError::read)
}

/// What the files an edit of the PCB view writes are like now: the layout file beside the
/// source and the board's physical file (ADR 0048), which holds the keepouts the view draws.
/// An edit gives back the stamp of the scene it was made on and is refused if the stamp is
/// different now, so a change to either file by another program refuses it (and drops the
/// page's undo). With no physical file it is the layout file's stamp alone (`none` if there is
/// no layout file either); else the two joined with `+`.
pub fn stamp(data: &BoardData) -> String {
    let layout = data.layout.as_ref().map_or_else(
        || "none".to_string(),
        |(file, _)| super::files::stamp_of(file),
    );
    match super::files::stamp_of(&data.physical) {
        physical if physical == "none" => layout,
        physical => format!("{layout}+{physical}"),
    }
}

/// [`place`], keeping what the pre-check proved apart from other failures.
pub fn place_checked(data: &BoardData, name: &str) -> Result<Entry, PlaceError> {
    // The stamp first: a file that changes meanwhile then makes an edit that was based on
    // this reading refused, not applied over the change.
    let stamp = stamp(data);
    let layout = match &data.layout {
        Some((file, boards)) => netsmith_fabricate::layout::read_checked(file, boards.as_deref())
            .map_err(|e| {
            PlaceError::Other(ApiError::unprocessable(
                format!("{name}: {e}"),
                "layout_unreadable",
                &[("board", name)],
            ))
        })?,
        None => None,
    };
    place_with(data, name, layout, stamp)
}

/// A board that cannot be made from what the project says (a part with no footprint, a pad
/// that the footprint does not have, a size the parts do not fit): the project's mistake, so
/// `422` with a `code` for the page (`ApiError::unprocessable`) and not a fault of the
/// server. The fields name what is wrong: `board`, and where there is one `part` (a
/// designator), `pad`, `net` and `footprint`.
pub fn board_error(name: &str, error: &netsmith_pcb::BoardError) -> ApiError {
    use netsmith_pcb::BoardError as E;
    let message = format!("{name}: {error}");
    let board = ("board", name);
    match error {
        E::NoPad {
            net,
            refdes,
            pad,
            footprint,
            ..
        } => ApiError::unprocessable(
            message,
            "footprint_lacks_pad",
            &[
                board,
                ("part", refdes),
                ("pad", pad),
                ("net", net),
                ("footprint", footprint),
            ],
        ),
        E::Map { refdes, .. } => {
            ApiError::unprocessable(message, "no_kicad_mapping", &[board, ("part", refdes)])
        }
        E::Footprint { refdes, .. } => {
            ApiError::unprocessable(message, "footprint_unreadable", &[board, ("part", refdes)])
        }
        E::NoPart { refdes } => {
            ApiError::unprocessable(message, "no_such_part", &[board, ("part", refdes)])
        }
        E::NoProcess { .. } | E::Process(_) => {
            ApiError::unprocessable(message, "process_unusable", &[board])
        }
        E::DoesNotFit(_) => ApiError::unprocessable(message, "does_not_fit", &[board]),
        E::Constraints(_) | E::Unplaceable(_) => {
            ApiError::unprocessable(message, "requirements_unmet", &[board])
        }
        E::Design(_) | E::Outline(_) => {
            ApiError::unprocessable(message, "invalid_design", &[board])
        }
    }
}

/// [`place`] for the PCB view, watched (issue #764): the placer's progress lines go to `say`
/// (`Board: placing: run 1 in ... [6 s]`, as `fab` prints them), where it has the parts goes to
/// `show` as it works, and a cancel of `token` stops it (`ApiError::cancelled`).
pub fn place_watched(
    data: &BoardData,
    name: &str,
    say: &Say,
    show: &Show,
    token: &Cancel,
) -> Result<Entry, ApiError> {
    let progress = netsmith_fabricate::layout::place_progress(name.to_string(), {
        let say = say.clone();
        move |line: &str| say(line)
    });
    job::catch(|| {
        job::scope(token, || {
            job::reporting(progress, || {
                watching_live(name, "place", show, || place(data, name))
            })
        })
    })
    .unwrap_or_else(|_| {
        Err(ApiError::cancelled(format!(
            "{name}: the placing was cancelled; nothing it placed is kept"
        )))
    })
}

/// How often a glimpse of the placer is passed to the page, at most: the first is passed at
/// once. The router keeps its own pace (at most four a second, and always the end of a try), so
/// its glimpses are all passed.
const LIVE_EVERY: Duration = Duration::from_millis(200);

/// `work` with the placer's or the router's glimpses passed to `show` as [`PcbLive`]s of
/// `task`, the placer's at most once in [`LIVE_EVERY`]. The last glimpse the pace held back is
/// passed when `work` ends, so the page is left with where the placer had the parts last, on a
/// machine of any speed: a placing that ends within [`LIVE_EVERY`] of its first glimpse (which
/// may be of an earlier part of the placing, with fewer parts) still shows its last (issue
/// #1249). A `work` that unwinds (a cancel) passes nothing more.
fn watching_live<T>(name: &str, task: &str, show: &Show, work: impl FnOnce() -> T) -> T {
    let held = std::rc::Rc::new(std::cell::RefCell::new(None::<netsmith_pcb::Glimpse>));
    let sink = {
        let (name, task, show, held) = (
            name.to_string(),
            task.to_string(),
            show.clone(),
            held.clone(),
        );
        let last = std::cell::Cell::new(None::<Instant>);
        std::rc::Rc::new(move |g: &netsmith_pcb::Glimpse| {
            let placing = matches!(g, netsmith_pcb::Glimpse::Placing { .. });
            if placing && last.get().is_some_and(|at| at.elapsed() < LIVE_EVERY) {
                *held.borrow_mut() = Some(g.clone());
                return;
            }
            held.borrow_mut().take();
            last.set(Some(Instant::now()));
            show(&live_of(&name, &task, g));
        })
    };
    let out = netsmith_pcb::watching_glimpses(sink, work);
    if let Some(g) = held.borrow_mut().take() {
        show(&live_of(name, task, &g));
    }
    out
}

/// A glimpse as the page takes it.
pub fn live_of(name: &str, task: &str, g: &netsmith_pcb::Glimpse) -> PcbLive {
    let mut live = PcbLive {
        board: name.to_string(),
        task: task.to_string(),
        region: None,
        parts: Vec::new(),
        tracks: Vec::new(),
        vias: Vec::new(),
    };
    match g {
        netsmith_pcb::Glimpse::Placing { region, parts } => {
            live.region = Some(sheet_box(*region));
            live.parts = parts
                .iter()
                .map(|p| PcbLivePart {
                    refdes: p.refdes.clone(),
                    court: sheet_box(p.court),
                    back: p.back,
                })
                .collect();
        }
        netsmith_pcb::Glimpse::Routing { tracks, vias } => {
            live.tracks = tracks
                .iter()
                .map(|t| PcbLiveTrack {
                    x1: t.from.x,
                    y1: t.from.y,
                    x2: t.to.x,
                    y2: t.to.y,
                    width: t.width,
                    layer: t.layer.word().to_string(),
                })
                .collect();
            live.vias = vias
                .iter()
                .map(|v| PcbLiveVia {
                    x: v.at.x,
                    y: v.at.y,
                    diameter: v.diameter,
                })
                .collect();
        }
    }
    live
}

/// The board of `name` placed with `layout`, which was in the file at `stamp`.
pub fn place_with(
    data: &BoardData,
    name: &str,
    layout: Option<(Layout, String)>,
    layout_stamp: String,
) -> Result<Entry, PlaceError> {
    let other = |e: ApiError| PlaceError::Other(e);
    let libraries = data.libraries().map_err(other)?;
    let placed = netsmith_fabricate::layout::place(
        &data.built.design,
        layout.as_ref(),
        libraries,
        data.constraints_warn,
        data.place_seeds,
    );
    // A layout file the placer cannot keep (two locks that overlap, locks that leave the free
    // parts no room) is shown where the placer ended, with its findings, and not refused: the
    // person who dragged a part there sees what is wrong (issue #779).
    if let Err(failure) = &placed
        && let Some(ended) = netsmith_fabricate::layout::where_ended(
            &data.built.design,
            layout.as_ref(),
            libraries,
            data.constraints_warn,
            failure,
        )
    {
        let explained = ended.explain(&data.built.design);
        let refusal =
            findings::refusal_findings(&ended.board, &failure.message(), &explained.proofs);
        let mut entry = Entry::with_routes(data, name, ended.board, layout, layout_stamp)?;
        entry.refusal = refusal;
        entry.refused = Some(failure.message());
        entry.place_options = ended.options;
        return Ok(entry);
    }
    let placed = placed.map_err(|e| match e {
        netsmith_fabricate::layout::PlaceFailure::Board(netsmith_pcb::BoardError::Unplaceable(
            found,
        )) => PlaceError::Unplaceable(name.to_string(), found),
        netsmith_fabricate::layout::PlaceFailure::Board(e) => {
            PlaceError::Other(board_error(name, &e))
        }
        netsmith_fabricate::layout::PlaceFailure::Layout(e) => {
            PlaceError::Layout(format!("{name}: {e}"))
        }
        failure @ netsmith_fabricate::layout::PlaceFailure::Unplaced { .. } => {
            PlaceError::Other(ApiError::unprocessable(
                format!("{name}: {}", failure.message()),
                "does_not_fit",
                &[("board", name)],
            ))
        }
    })?;
    let mut entry = Entry::with_routes(data, name, placed.board, layout, layout_stamp)?;
    entry.place_options = placed.options;
    Ok(entry)
}

impl Entry {
    /// The entry of `placed`, a board placed with `layout` and no copper, with the copper of
    /// the routes file beside the layout file on it (ADR 0030), read now.
    pub fn with_routes(
        data: &BoardData,
        name: &str,
        placed: Board,
        layout: Option<(Layout, String)>,
        layout_stamp: String,
    ) -> Result<Entry, PlaceError> {
        let routes_file = data
            .layout
            .as_ref()
            .map(|(file, _)| netsmith_fabricate::routes::beside_layout(file));
        let mut board = placed.clone();
        let (routes_stamp, routes, routes_notes) = match &routes_file {
            Some(file) => {
                // The stamp first, as for the layout file.
                let stamp = super::files::stamp_of(file);
                let routes = netsmith_fabricate::routes::load(file)
                    .map_err(|e| PlaceError::Layout(format!("{name}: {e}")))?;
                let shown = file.display().to_string();
                let notes = netsmith_pcb::apply_routes(&mut board, &routes, &shown)
                    .map_err(|e| PlaceError::Layout(format!("{name}: {e}")))?;
                (stamp, routes, notes)
            }
            None => ("none".to_string(), Routes::default(), Vec::new()),
        };
        let case_findings = if data.built.design.features.case_models.is_empty() {
            Vec::new()
        } else {
            findings::case_findings(
                &board,
                &netsmith_fabricate::board3d::scene(&data.built, &board).case,
            )
        };
        Ok(Entry {
            case_findings,
            rip_rounds: data.rip_rounds,
            weights: data.weights,
            refusal: Vec::new(),
            place_options: netsmith_pcb::PlaceOptions::default(),
            refused: None,
            board,
            routed: false,
            stopped: false,
            unrouted: Vec::new(),
            elapsed: Duration::ZERO,
            layout,
            layout_stamp,
            kicad: None,
            drc: OnceLock::new(),
            grounds: OnceLock::new(),
            shove_grounds: OnceLock::new(),
            constraints: netsmith_pipeline::constraints::of_design(
                &data.built.design,
                data.constraints_warn,
            ),
            constraint_findings: OnceLock::new(),
            placed,
            routes_file,
            routes_stamp,
            routes,
            routes_notes,
            carried: Vec::new(),
            rerouted: Vec::new(),
        })
    }

    /// The entry again with the routes file as it is now, the board as placed and unrouted: what
    /// the view shows once a change to the routes file is written.
    pub fn routes_changed(&self, data: &BoardData, name: &str) -> Result<Entry, PlaceError> {
        let mut entry = Entry::with_routes(
            data,
            name,
            self.placed.clone(),
            self.layout.clone(),
            self.layout_stamp.clone(),
        )?;
        entry.refusal = self.refusal.clone();
        entry.place_options = self.place_options.clone();
        entry.refused = self.refused.clone();
        Ok(entry)
    }

    /// The placed board, not routed, as the files `netsmith layout export` writes (issue
    /// #1001): the board the view shows placed, with the findings of its placement, also when
    /// the placer refused the layout file.
    pub fn kicad_package(
        &self,
        data: &BoardData,
        label: &str,
    ) -> netsmith_fabricate::layout_kicad::Package {
        let conflicts = self
            .placed
            .explain_requirements(&data.built.design, &self.place_options)
            .map(|r| r.proofs)
            .unwrap_or_default();
        let placing = netsmith_fabricate::board3d::Placing {
            conflicts,
            refused: self.refused.clone(),
        };
        let out = format!("{label}.placed.kicad_pcb");
        netsmith_fabricate::layout_kicad::package(label, &self.placed, placing, Path::new(&out))
    }

    /// The board as placed with no copper on it.
    pub fn placed(&self) -> &Board {
        &self.placed
    }

    /// The routes file beside the layout file, if the board has a layout file.
    pub fn routes_file(&self) -> Option<&Path> {
        self.routes_file.as_deref()
    }

    pub fn routes_stamp(&self) -> &str {
        &self.routes_stamp
    }
}

impl Entry {
    /// Route the board, telling `say` the stages and how far the router has got, for as long as
    /// the router's time limit lasts or until [`cancel`]. With `before`, the same board routed
    /// before a change of its placement (kept in memory, or on disk:
    /// `netsmith_fabricate::kept_routes`) and the nets to route again whatever it says, the
    /// copper of every net the change left alone is kept and only the rest is routed
    /// ([`netsmith_router::carry::reroute`], issue #825). Gives the routed board to keep
    /// ([`RoutedBoards`]).
    pub fn route(
        &mut self,
        name: &str,
        say: &Say,
        show: &Show,
        before: Option<(&Board, &BTreeSet<String>)>,
    ) -> RoutedBoard {
        watching_live(name, "route", show, || {
            self.route_unwatched(name, say, before)
        })
    }

    fn route_unwatched(
        &mut self,
        name: &str,
        say: &Say,
        before: Option<(&Board, &BTreeSet<String>)>,
    ) -> RoutedBoard {
        let started = Instant::now();
        CANCEL.store(false, Ordering::Relaxed);
        let _reporting = Reporting::to(say);
        say(&format!(
            "{name}: placed {} parts on a {} by {} mm board",
            self.board.parts.len(),
            self.board.outline.width(),
            self.board.outline.height()
        ));
        let start = self.board.clone();
        let options = RouteOptions {
            time_limit: Some(DEFAULT_ROUTE_TIMEOUT),
            progress: Some(forward),
            cancel: Some(&CANCEL),
            rip_rounds: self.rip_rounds,
            weights: self.weights,
            ..RouteOptions::default()
        };
        let routing = match before {
            Some((before, rip)) => {
                let done = netsmith_router::carry::reroute(before, &mut self.board, rip, options);
                self.carried = done.carried.kept.clone();
                let mut again = done.carried.ripped.clone();
                again.extend(done.also.iter().cloned());
                again.sort();
                again.dedup();
                self.rerouted = again;
                say(&format!(
                    "{name}: kept the copper of {} net(s), routed {} again",
                    self.carried.len(),
                    self.rerouted.len()
                ));
                done.routing
            }
            None => {
                self.carried.clear();
                self.rerouted.clear();
                route_with(&mut self.board, options)
            }
        };
        self.routed = true;
        self.drc = OnceLock::new();
        self.stopped = routing.stopped;
        self.unrouted = routing.left.lines.iter().map(ToString::to_string).collect();
        self.elapsed = started.elapsed();
        say(&format!(
            "{name}: routed in {:.1} s, {} not routed",
            self.elapsed.as_secs_f64(),
            self.unrouted.len()
        ));
        RoutedBoard {
            start,
            board: self.board.clone(),
            unrouted: self.unrouted.clone(),
            stopped: self.stopped,
            carried: self.carried.clone(),
            rerouted: self.rerouted.clone(),
        }
    }

    /// The board as `routed` has it, when it started from the board this entry has: an undo
    /// back to a placement that was routed shows its copper again, with no router. Whether it
    /// did.
    pub fn restore(&mut self, routed: &RoutedBoards) -> bool {
        let Some(found) = routed.find(&self.board) else {
            return false;
        };
        self.board = found.board.clone();
        self.routed = true;
        self.drc = OnceLock::new();
        self.stopped = found.stopped;
        self.unrouted = found.unrouted.clone();
        self.carried = found.carried.clone();
        self.rerouted = found.rerouted.clone();
        self.elapsed = Duration::ZERO;
        true
    }

    pub fn report(&self, name: &str) -> PcbReport {
        PcbReport {
            board: name.to_string(),
            routed: self.routed,
            unrouted: self.unrouted.clone(),
            placement_problems: self
                .board
                .check_placement()
                .iter()
                .map(ToString::to_string)
                .collect(),
            tracks: self.board.tracks.len() as u32,
            vias: self.board.vias.len() as u32,
            stopped: self.stopped,
            carried: self.carried.clone(),
            rerouted: self.rerouted.clone(),
            error: None,
            elapsed_ms: self.elapsed.as_millis().min(u128::from(u32::MAX)) as u32,
        }
    }
}

fn sheet_box(r: Rect) -> PcbBox {
    PcbBox {
        x: r.min.x,
        y: r.min.y,
        width: r.width(),
        height: r.height(),
    }
}

fn side_of(side: Side) -> PcbSide {
    if side == Side::Back {
        PcbSide::Bottom
    } else {
        PcbSide::Top
    }
}

impl Entry {
    /// The layout file's entries for the board.
    fn entries(&self) -> Option<&BoardLayout> {
        self.layout.as_ref()?.0.boards.get(&self.board.name)
    }

    pub fn set_layout_stamp(&mut self, stamp: String) {
        self.layout_stamp = stamp;
    }

    pub fn is_routed(&self) -> bool {
        self.routed
    }

    pub fn layout_stamp(&self) -> &str {
        &self.layout_stamp
    }

    /// The copper layers of the board, top first.
    pub fn layers(&self) -> Vec<String> {
        copper_layers(&self.board)
    }
}

/// The findings of a board that is not placed because the requirements prove it cannot be
/// (ADR 0045 section 14): `constraint` findings, and nothing else to list.
pub fn unplaceable_findings(name: &str, pre: &netsmith_pcb::Precheck) -> PcbFindings {
    PcbFindings {
        board: name.to_string(),
        routed: false,
        findings: findings::precheck_findings(pre),
        kicad: KicadState::NotRun,
        kicad_note: "KiCad's check was not run: the board is not placed".to_string(),
    }
}

impl Entry {
    /// Everything wrong with the board as it is: netsmith's checks of the placement and (once
    /// it is routed) of the connections left and of the copper, and what KiCad's check found
    /// if it was asked for.
    pub fn findings(&self, name: &str) -> PcbFindings {
        let mut all = findings::placement_findings(&self.board);
        all.extend(findings::layout_findings(
            &self.board,
            self.layout
                .as_ref()
                .and_then(|(l, _)| l.boards.get(&self.board.name)),
        ));
        all.extend(self.refusal.iter().cloned());
        all.extend(self.case_findings.iter().cloned());
        let file = self
            .routes_file
            .as_deref()
            .and_then(Path::file_name)
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        all.extend(findings::routes_findings(
            &self.board,
            &file,
            &self.routes_notes,
        ));
        if let Some(constraints) = &self.constraints {
            all.extend(
                self.constraint_findings
                    .get_or_init(|| findings::constraint_findings(&self.board, constraints))
                    .iter()
                    .cloned(),
            );
        }
        if self.routed {
            all.extend(findings::unrouted_findings(&self.board));
            // Copper to check only once there is some.
            all.extend(
                self.drc
                    .get_or_init(|| findings::drc_findings(&self.board))
                    .iter()
                    .cloned(),
            );
        }
        let (kicad, kicad_note) = match &self.kicad {
            Some(k) => {
                all.extend(k.findings.iter().cloned());
                (k.state, k.note.clone())
            }
            None => (
                KicadState::NotRun,
                "KiCad's check was not run on this layout".to_string(),
            ),
        };
        // Errors first, then warnings, each in the order found.
        all.sort_by_key(|f| f.severity != Severity::Error);
        PcbFindings {
            board: name.to_string(),
            routed: self.routed,
            findings: all,
            kicad,
            kicad_note,
        }
    }
}

/// The keys of a JSON document in camelCase, as the page's types have them: `largest_free` is
/// `largestFree`. Only keys change; every field of `netsmith_pcb::Summary` is a plain word or
/// words joined by `_`.
fn camel_keys(v: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let mut out = String::new();
                    let mut up = false;
                    for c in k.chars() {
                        if c == '_' {
                            up = true;
                        } else if up {
                            out.extend(c.to_uppercase());
                            up = false;
                        } else {
                            out.push(c);
                        }
                    }
                    (out, camel_keys(v))
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(camel_keys).collect()),
        other => other,
    }
}

/// The board of `entry` as placed now, summed up (`netsmith layout summary`, issue #1003): its
/// neighbours of the parts `near` within `within` mm, and the distance of each pair of pads of
/// `between`. A part or pad the board has none of is a 400.
pub fn summary(
    entry: &Entry,
    name: &str,
    near: &[String],
    within: f64,
    between: &[(String, String)],
) -> Result<PcbSummary, ApiError> {
    let board = &entry.board;
    let summary = netsmith_pcb::Summary::of(board)
        .neighbours(board, near, within)
        .and_then(|s| s.pad_distances(board, between))
        .map_err(ApiError::bad_request)?;
    let mut value = serde_json::to_value(&summary).expect("plain data");
    value["layout_stamp"] = entry.layout_stamp.clone().into();
    let mut answer: PcbSummary = serde_json::from_value(camel_keys(value))
        .map_err(|e| ApiError::internal(format!("the summary of {name}: {e}")))?;
    name.clone_into(&mut answer.board);
    Ok(answer)
}

/// The copper areas of the board of `data` (`netsmith board copper`, issue #1085): its planes,
/// pours and the zones of the routes file beside its layout file, read now from the source and
/// the files, with no placing and no routing. The teardrop fills are counted when `routed` is
/// the board after the router has drawn them (the entry of a board routed here), and not
/// otherwise.
pub fn copper(data: &BoardData, name: &str, routed: Option<&Board>) -> Result<PcbCopper, ApiError> {
    let design = &data.built.design;
    let areas = match &data.layout {
        Some((layout, _)) => netsmith_fabricate::routes::copper_areas(design, layout),
        None => netsmith_pcb::CopperAreas::of(design, None),
    }
    .map_err(|e| ApiError::internal(format!("the copper areas of {name}: {e}")))?;
    let areas = match routed {
        Some(board) => areas.with_teardrops(board),
        None => areas,
    };
    let value = serde_json::to_value(&areas).expect("plain data");
    serde_json::from_value(camel_keys(value))
        .map_err(|e| ApiError::internal(format!("the copper areas of {name}: {e}")))
}

/// The board of an entry and the stamps of what it was placed from, copied out so that the fill
/// of [`Filling::fill`], which takes longer than anything else the view asks for, and the table
/// of [`Filling::nets`] run with no lock held.
pub struct Filling {
    board: Board,
    layout_stamp: String,
    routes_stamp: String,
    routed: bool,
}

impl Filling {
    pub fn of(entry: &Entry) -> Filling {
        Filling {
            board: entry.board.clone(),
            layout_stamp: entry.layout_stamp().to_string(),
            routes_stamp: entry.routes_stamp().to_string(),
            routed: entry.is_routed(),
        }
    }

    /// How the fill of the board's pours joins its pads (`netsmith board fill`, issue #1187), as
    /// the entry has it placed (and routed): [`netsmith_pcb::FillReport`] with camelCase keys.
    pub fn fill(self, name: &str) -> Result<PcbFill, ApiError> {
        let report = netsmith_pcb::FillReport::of(&self.board)
            .map_err(|e| ApiError::internal(format!("the fill of {name}: {e}")))?;
        let mut value = serde_json::to_value(&report).expect("plain data");
        value["board"] = name.into();
        value["layout_stamp"] = self.layout_stamp.into();
        value["routes_stamp"] = self.routes_stamp.into();
        value["routed"] = self.routed.into();
        serde_json::from_value(camel_keys(value))
            .map_err(|e| ApiError::internal(format!("the fill of {name}: {e}")))
    }

    /// The stitching vias the export will have that the routes file does not (`netsmith view
    /// pcb --stitching`, issue #1489), as the entry has it placed (and routed):
    /// [`netsmith_router::stitching_vias`], the stitching `fab` runs.
    pub fn stitching(self, name: &str) -> PcbStitching {
        let vias = netsmith_router::stitching_vias(&self.board)
            .into_iter()
            .map(|v| StitchingVia {
                net: v.net,
                x: v.at.x,
                y: v.at.y,
                diameter: v.diameter,
            })
            .collect();
        PcbStitching {
            board: name.to_string(),
            layout_stamp: self.layout_stamp,
            routes_stamp: self.routes_stamp,
            routed: self.routed,
            vias,
        }
    }

    /// The table of the board's nets and diff pairs (`netsmith board nets`, issue #1223), as the
    /// entry has it placed (and routed): [`netsmith_fab::NetTable`] with camelCase keys.
    pub fn nets(self, name: &str) -> Result<PcbNets, ApiError> {
        let table = netsmith_fabricate::net_table::of_board(&self.board)
            .map_err(|e| ApiError::internal(format!("the nets of {name}: {e}")))?;
        let mut value = serde_json::to_value(&table).expect("plain data");
        value["board"] = name.into();
        value["layout_stamp"] = self.layout_stamp.into();
        value["routes_stamp"] = self.routes_stamp.into();
        value["routed"] = self.routed.into();
        serde_json::from_value(camel_keys(value))
            .map_err(|e| ApiError::internal(format!("the nets of {name}: {e}")))
    }
}

/// Where the part `part` fits on the board of `entry` as placed now, with every other part where
/// it is (`netsmith layout fit`, issue #1002). Nothing is placed or changed: the board in the
/// entry is only read. A part the board has none of, a `near` that is neither a part nor `X,Y`, a
/// step not above 0 and a side that is neither face are a 400.
pub fn fit(
    entry: &Entry,
    data: &BoardData,
    name: &str,
    part: &str,
    near: Option<&str>,
    step: f64,
    side: Option<&str>,
) -> Result<PcbFit, ApiError> {
    let near = netsmith_pcb::FitNear::parse(near)
        .map_err(|e| ApiError::bad_request(format!("`near`: {e}")))?;
    let face = side.map(|s| {
        if s == "bottom" {
            Side::Back
        } else {
            Side::Front
        }
    });
    let fit = entry
        .board
        .fit_part(&data.built.design, part, &near, step, face)
        .map_err(ApiError::bad_request)?;
    let part_of = |p: &netsmith_pcb::PartRef| PcbFitPart {
        refdes: p.refdes.clone(),
        path: p.path.clone(),
    };
    let place = |p: &netsmith_pcb::FitPlace| PcbFitPlace {
        turn: p.turn,
        side: p.side.to_string(),
        at: p.at,
        distance: p.distance,
        fits: p.fits,
        placement: p
            .placement
            .iter()
            .map(|f| PcbFitFault {
                kind: f.kind.to_string(),
                parts: f.parts.iter().map(|b| b.part.refdes.clone()).collect(),
            })
            .collect(),
        requirements: p
            .requirements
            .iter()
            .map(|m| PcbFitMiss {
                id: m.id.clone(),
                source: m.source.clone(),
                parts: m.parts.clone(),
            })
            .collect(),
        messages: p.messages.clone(),
    };
    Ok(PcbFit {
        board: name.to_string(),
        layout_stamp: entry.layout_stamp.clone(),
        part: part_of(&fit.part),
        near: PcbFitNear {
            x: fit.near.x,
            y: fit.near.y,
            part: fit.near.part.as_ref().map(part_of),
        },
        step: fit.step,
        now: place(&fit.now),
        poses: fit.poses.iter().map(place).collect(),
    })
}

/// The board as placed (and routed) in `entry`, as the `.glb` file `dev board3d --glb` writes
/// (ADR 0105), with the stamp of the layout file it was placed with.
pub fn glb(entry: &Entry, data: &BoardData) -> netsmith_editor::Pcb3d {
    netsmith_editor::Pcb3d {
        glb: netsmith_fabricate::board3d::glb(&data.built, &entry.board),
        layout_stamp: entry.layout_stamp.clone(),
    }
}

/// The board as the canvas draws and edits it: the drawing (routed if it was routed since the
/// files were last read, else only placed; it has no key, the page lists the nets itself), and
/// what finds things in it. `layout_file` names the layout file in the page.
pub fn scene(entry: &Entry, data: &BoardData, name: &str) -> PcbScene {
    let board = &entry.board;
    let entries = entry.entries();
    let net_of: HashMap<(&str, &str), &str> = board
        .nets
        .iter()
        .flat_map(|n| {
            n.pads
                .iter()
                .map(move |p| ((p.refdes.as_str(), p.number.as_str()), n.name.as_str()))
        })
        .collect();
    // The physical file: the keepouts the editor drew there and the places "Require here" wrote.
    let physical_text = std::fs::read_to_string(&data.physical).unwrap_or_default();
    let physical_name = data
        .physical
        .file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    let design = &data.built.design;
    let mut required = crate::required::scan(&physical_text, &design.name, &physical_name);
    let parts = board
        .parts
        .iter()
        .map(|p| {
            let place = entries.and_then(|e| e.parts.get(&p.path));
            let stated = required.remove(&p.path).unwrap_or_default();
            // A relation with another part (a `row` among them) is changed in the file too.
            let related = design
                .features
                .offsets
                .iter()
                .find(|o| o.a.path == p.path || o.b.path == p.path)
                .map(|o| {
                    let other = if o.a.path == p.path {
                        &o.b.path
                    } else {
                        &o.a.path
                    };
                    if o.source.is_empty() {
                        format!("a relation with `{other}`")
                    } else {
                        format!("a relation with `{other}` ({})", o.source)
                    }
                });
            let end = |v: f64| v.is_finite().then_some(v);
            let windows = netsmith_pcb::part_windows(design, &p.path);
            // A requirement of the part's own that the physical file does not state in the
            // editor's shape (a module's source may state one for its part).
            let own = (stated.place == RequiredPlace::default())
                .then(|| windows.iter().find(|w| !w.module))
                .flatten()
                .map(|w| {
                    if w.source.is_empty() {
                        format!("`{}`", w.id)
                    } else {
                        format!("`{}` ({})", w.id, w.source)
                    }
                });
            PcbPart {
                refdes: p.refdes.clone(),
                path: p.path.clone(),
                kind: p.kind.clone(),
                value: p.value.clone(),
                package: data
                    .built
                    .design
                    .parts
                    .iter()
                    .find(|d| d.refdes == p.refdes)
                    .and_then(|d| d.package.clone()),
                x: p.at.x,
                y: p.at.y,
                rotation: p.rotation,
                side: side_of(p.side),
                locked: place.is_some_and(|e| e.lock),
                placement: place.map_or("derived".to_string(), |e| {
                    e.source.clone().unwrap_or_else(|| "hand".to_string())
                }),
                courtyard: sheet_box(p.courtyard()),
                courtyards: p.courtyards().into_iter().map(sheet_box).collect(),
                body: sheet_box(p.body()),
                height: p.height,
                requirements: windows
                    .into_iter()
                    .map(|w| PcbRequirement {
                        field: w.field.word().to_string(),
                        lo: end(w.lo),
                        hi: end(w.hi),
                        id: w.id,
                        source: w.source,
                        module: w.module,
                    })
                    .collect(),
                required: stated.place,
                required_elsewhere: stated.elsewhere.or(related).or(own),
                pads: p
                    .footprint
                    .pads
                    .iter()
                    .map(|pad| {
                        let at = p.pad_centre(pad);
                        PcbPad {
                            number: pad.number.clone(),
                            x: at.x,
                            y: at.y,
                            net: net_of
                                .get(&(p.refdes.as_str(), pad.number.as_str()))
                                .map(ToString::to_string),
                        }
                    })
                    .collect(),
            }
        })
        .collect();
    let areas = board.keepout_areas();
    // The keepouts of the physical file that the editor drew, or could have: it changes them there.
    let in_physical = crate::keepouts::drawn_in(&physical_text, &data.built.design.name);
    // Those it moves and resizes there by rewriting the numbers of their place and size.
    let stated_in_physical = crate::keepouts::stated_in(&physical_text, &data.built.design.name);
    let keepouts = board
        .keepouts
        .iter()
        .map(|k| {
            let stated = data
                .built
                .design
                .features
                .keepouts
                .iter()
                .any(|d| d.name == k.name && d.at.is_some());
            let origin = if k.part.is_some() {
                KeepoutOrigin::Part
            } else if in_physical.contains_key(&k.name) {
                KeepoutOrigin::Drawn
            } else if stated && stated_in_physical.contains_key(&k.name) {
                KeepoutOrigin::Stated
            } else if stated {
                KeepoutOrigin::Source
            } else {
                KeepoutOrigin::Placed
            };
            PcbKeepout {
                name: k.name.clone(),
                origin,
                area: areas
                    .iter()
                    .find(|a| a.name == k.name)
                    .map(|a| sheet_box(a.rect)),
                width: k.size.0,
                height: k.size.1,
                layers: k.layers.clone(),
                tracks: k.tracks,
                vias: k.vias,
                copper: k.copper,
                parts: k.parts,
                max_height: k.max_height,
            }
        })
        .collect();
    let cutouts = board
        .cutouts
        .iter()
        .map(|c| PcbCutout {
            name: c.name.clone(),
            area: board
                .cutout_shapes()
                .into_iter()
                .find(|(shape, _)| shape.name == c.name)
                .map(|(_, shape)| sheet_box(shape.rect)),
        })
        .collect();
    let layout_file = data
        .layout
        .as_ref()
        .and_then(|(path, _)| path.file_name())
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    let routes_file = entry
        .routes_file
        .as_deref()
        .and_then(Path::file_name)
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    PcbScene {
        routes_file,
        routes_stamp: entry.routes_stamp.clone(),
        routes: route_nets(entry),
        net_widths: board
            .nets
            .iter()
            .map(|n| PcbNetWidth {
                net: n.name.clone(),
                width: board.track_width(&n.name),
                inner_width: board
                    .signal_layers()
                    .first()
                    .map(|&l| board.track_width_on(&n.name, l)),
            })
            .collect(),
        via_diameter: board.rules.via_diameter,
        via_drill: board.rules.via_drill,
        via_offers: board
            .via_offers()
            .iter()
            .map(|o| PcbViaOffer {
                layers: o
                    .span
                    .map(|s| vec![s.from.word().to_string(), s.to.word().to_string()])
                    .unwrap_or_default(),
                diameter: o.diameter,
                drill: o.drill,
            })
            .collect(),
        board: name.to_string(),
        svg: render_svg_view(board),
        origin_x: board.outline.min.x,
        origin_y: board.outline.min.y,
        outline: sheet_box(board.outline),
        layout_file,
        layout_stamp: entry.layout_stamp.clone(),
        layers: entry.layers(),
        track_layers: std::iter::once(CopperLayer::Front)
            .chain(board.signal_layers())
            .chain([CopperLayer::Back])
            .map(|l| l.word().to_string())
            .collect(),
        routed: entry.routed,
        parts,
        keepouts,
        cutouts,
    }
}

/// The routes file's records for the board, on the sheet, each shape marked as drawn or not as
/// `netsmith_pcb::apply_routes` decides: every shape of a net the board has, whether or not its
/// pads moved (issue #1328), none of a net the board does not have.
fn route_nets(entry: &Entry) -> Vec<PcbRouteNet> {
    let board = &entry.placed;
    let Some(table) = entry.routes.boards.get(&board.name) else {
        return Vec::new();
    };
    let at = |p: [f64; 2]| PcbPoint {
        x: board.outline.min.x + p[0],
        y: board.outline.min.y + p[1],
    };
    table
        .nets
        .iter()
        .map(|(net, record)| {
            let hash = netsmith_pcb::pad_hash(board, net);
            let current = hash.as_deref() == Some(record.pads.as_str());
            let known = hash.is_some();
            PcbRouteNet {
                net: net.clone(),
                lock: record.lock,
                current,
                tracks: record
                    .tracks
                    .iter()
                    .map(|t| PcbRouteTrack {
                        layer: t.layer.word().to_string(),
                        width: t.width,
                        from: at(t.from),
                        mid: t.mid.map(at),
                        to: at(t.to),
                        lock: t.lock,
                        drawn: known,
                    })
                    .collect(),
                vias: record
                    .vias
                    .iter()
                    .map(|v| PcbRouteVia {
                        at: at(v.at),
                        diameter: v.diameter,
                        drill: v.drill,
                        layers: v.layers.clone(),
                        lock: v.lock,
                        drawn: known,
                    })
                    .collect(),
            }
        })
        .collect()
}

/// A change of the PCB view to the routes file of `board` (placed), as `netsmith routes` would
/// make it: its places from the sheet to the file's frame, the outline's top left corner.
pub fn routes_edit(
    board: &Board,
    change: &RoutesChange,
) -> Result<netsmith_fabricate::routes::Edit, ApiError> {
    use netsmith_fabricate::routes::{Edit, Shape};
    let at = |p: &PcbPoint| [p.x - board.outline.min.x, p.y - board.outline.min.y];
    let record = |text: &Option<String>, net: &str| {
        netsmith_fabricate::routes::record_of_text(&board.name, net, text.as_deref())
            .map_err(ApiError::bad_request)
    };
    let shape = |track: Option<u32>, via: Option<u32>| match (track, via) {
        (Some(_), Some(_)) => Err(ApiError::bad_request(
            "a change names a track or a via, not both",
        )),
        (Some(n), None) => Ok(Some(Shape::Track(n as usize))),
        (None, Some(n)) => Ok(Some(Shape::Via(n as usize))),
        (None, None) => Ok(None),
    };
    Ok(match change {
        RoutesChange::Delete { net, track, via } => Edit::Delete {
            net: net.clone(),
            shape: shape(*track, *via)?,
        },
        RoutesChange::Lock {
            net,
            track,
            via,
            lock,
        } => Edit::Lock {
            net: net.clone(),
            shape: shape(*track, *via)?,
            on: *lock,
        },
        RoutesChange::AddTrack {
            net,
            layer,
            width,
            from,
            to,
        } => Edit::AddTrack {
            net: net.clone(),
            layer: track_layer(board, layer)?,
            width: *width,
            from: at(from),
            mid: None,
            to: at(to),
            lock: false,
        },
        RoutesChange::AddVia { net, at: p, layers } => Edit::AddVia {
            net: net.clone(),
            at: at(p),
            diameter: None,
            drill: None,
            layers: via_layers(board, layers.as_deref())?,
            lock: false,
        },
        RoutesChange::MoveTrack {
            net,
            track,
            from,
            to,
            mid,
        } => Edit::MoveTrack {
            net: net.clone(),
            track: *track as usize,
            from: at(from),
            mid: mid.as_ref().map(at),
            to: at(to),
        },
        RoutesChange::MoveVia { net, via, at: p } => Edit::MoveVia {
            net: net.clone(),
            via: *via as usize,
            at: at(p),
        },
        RoutesChange::Restore { net, before, after } => Edit::Restore {
            net: net.clone(),
            before: record(before, net)?,
            after: record(after, net)?,
        },
    })
}

impl Entry {
    /// The board a track drawn now is checked against: as placed, with the copper of the routes
    /// file on it and none of the router's, which an edit of the routes file discards, as
    /// `netsmith_fabricate::routes::edit_all` reads it.
    pub fn board_for_walk(&self) -> Board {
        let mut board = self.placed.clone();
        // A file that does not apply is the scene's finding; the walk is then of the board as placed.
        let _ = netsmith_pcb::apply_routes(&mut board, &self.routes, "the routes file");
        board
    }

    /// The board a shove is asked about (issue 726, the third slice): [`Entry::board_for_walk`]
    /// without the shapes that may move, and those shapes. A shape may move when it is a track
    /// (straight or an arc, issue 1009) or a via of the routes file that is not locked, in a
    /// record that is not locked and is current (its pads are where it was written). Everything
    /// else of the routes file stays on the board.
    pub fn board_for_shove(&self) -> (Board, Vec<Movable>) {
        let mut routes = self.routes.clone();
        let mut movable = Vec::new();
        if let Some(table) = routes.boards.get_mut(&self.placed.name) {
            for (net, record) in table.nets.iter_mut() {
                let current = netsmith_pcb::pad_hash(&self.placed, net)
                    .is_some_and(|hash| hash == record.pads);
                if record.lock || !current {
                    continue;
                }
                let mut n = 0u32;
                record.tracks.retain(|t| {
                    n += 1;
                    if t.lock {
                        return true;
                    }
                    movable.push(Movable {
                        net: net.clone(),
                        number: n,
                        shape: MovableShape::Track(t.on(&self.placed, net)),
                    });
                    false
                });
                let mut n = 0u32;
                record.vias.retain(|v| {
                    n += 1;
                    if v.lock {
                        return true;
                    }
                    movable.push(Movable {
                        net: net.clone(),
                        number: n,
                        shape: MovableShape::Via(v.on(&self.placed, net)),
                    });
                    false
                });
            }
        }
        let mut board = self.placed.clone();
        // A file that does not apply is the scene's finding; the walk is then of the board as placed.
        let _ = netsmith_pcb::apply_routes(&mut board, &routes, "the routes file");
        (board, movable)
    }

    /// What a walk is asked on ([`Grounds`]): built the first time it is asked for and kept for
    /// as long as the entry is (an edit of the routes file or of the layout makes a new entry),
    /// so that a walk asked as the pointer moves builds no ground (issue 1009).
    pub fn walk_grounds(&self) -> Result<std::sync::Arc<Grounds>, ApiError> {
        if let Some(g) = self.grounds.get() {
            return Ok(g.clone());
        }
        let g = std::sync::Arc::new(Grounds::of(self.board_for_walk())?);
        Ok(self.grounds.get_or_init(|| g).clone())
    }

    /// What a shove is asked on ([`ShoveGrounds`]), kept as [`Entry::walk_grounds`] is.
    pub fn shove_grounds(&self) -> Result<std::sync::Arc<ShoveGrounds>, ApiError> {
        if let Some(g) = self.shove_grounds.get() {
            return Ok(g.clone());
        }
        let g = std::sync::Arc::new(ShoveGrounds::of(self.board_for_shove())?);
        Ok(self.shove_grounds.get_or_init(|| g).clone())
    }

    /// The router's copper on the board, if it was routed: what the board has beyond the copper
    /// of the routes file (`walk_board`, [`Entry::board_for_walk`]), pieces equal to it counted
    /// once each.
    pub fn router_copper(&self, walk_board: &Board) -> RouterCopper {
        if !self.routed {
            return RouterCopper::default();
        }
        fn beyond<T: PartialEq + Clone>(all: &[T], fixed: &[T]) -> Vec<T> {
            let mut left: Vec<&T> = fixed.iter().collect();
            all.iter()
                .filter(|t| match left.iter().position(|f| f == t) {
                    Some(i) => {
                        left.swap_remove(i);
                        false
                    }
                    None => true,
                })
                .cloned()
                .collect()
        }
        RouterCopper {
            tracks: beyond(&self.board.tracks, &walk_board.tracks),
            vias: beyond(&self.board.vias, &walk_board.vias),
        }
    }
}

/// The router's copper on a board: the tracks and vias a routing added to the copper of the
/// routes file, which an edit of the routes file discards.
#[derive(Debug, Clone, Default)]
pub struct RouterCopper {
    pub tracks: Vec<netsmith_pcb::Track>,
    pub vias: Vec<netsmith_pcb::Via>,
}

/// What a walk is asked on, built once for a placed board and its routes file and kept with its
/// [`Entry`] (issue 1009): the board as [`Entry::board_for_walk`] makes it and its ground.
pub struct Grounds {
    pub board: Board,
    pub ground: netsmith_router::walk::Ground,
}

impl Grounds {
    pub fn of(board: Board) -> Result<Grounds, ApiError> {
        let ground = netsmith_router::walk::Ground::of(&board).map_err(ApiError::internal)?;
        Ok(Grounds { board, ground })
    }
}

/// What a shove is asked on ([`Entry::board_for_shove`]): the shapes that may move, and the
/// ground of the board without them. Built once and kept as [`Grounds`] is.
pub struct ShoveGrounds {
    pub movable: Vec<Movable>,
    pub ground: netsmith_router::walk::Ground,
}

impl ShoveGrounds {
    pub fn of((board, movable): (Board, Vec<Movable>)) -> Result<ShoveGrounds, ApiError> {
        let ground = netsmith_router::walk::Ground::of(&board).map_err(ApiError::internal)?;
        Ok(ShoveGrounds { movable, ground })
    }

    /// The shapes that may move, as the shove takes them, less those `moving` names (a drag's,
    /// which its new copper replaces).
    fn pieces(&self, moving: &[PcbShapeRef]) -> Vec<netsmith_router::shove::Piece> {
        self.movable
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                !moving.iter().any(|r| {
                    r.net == m.net
                        && match m.shape {
                            MovableShape::Track(_) => r.track == Some(m.number),
                            MovableShape::Via(_) => r.via == Some(m.number),
                        }
                })
            })
            .map(|(id, m)| netsmith_router::shove::Piece {
                id,
                what: match &m.shape {
                    MovableShape::Track(t) => netsmith_router::shove::What::Track(t.clone()),
                    MovableShape::Via(v) => netsmith_router::shove::What::Via(v.clone()),
                },
            })
            .collect()
    }
}

/// The copper of a track drawn before its last layer change (`PcbWalk::earlier`) and the vias
/// at its changes (`PcbWalk::vias`), as new copper of the net.
fn earlier_copper(
    board: &Board,
    walk: &PcbWalk,
) -> Result<Vec<netsmith_router::shove::What>, ApiError> {
    use netsmith_router::shove::What;
    let point = |p: &PcbPoint| netsmith_pcb::Pt::new(p.x, p.y);
    let mut out = Vec::new();
    for run in &walk.earlier {
        let layer = track_layer(board, &run.layer)?;
        let width = board.track_width_on(&walk.net, layer);
        for w in run.points.windows(2) {
            out.push(What::Track(netsmith_pcb::Track::straight(
                &walk.net,
                layer,
                point(&w[0]),
                point(&w[1]),
                width,
            )));
        }
    }
    for v in &walk.vias {
        out.push(What::Via(process_via(
            board,
            &walk.net,
            point(v),
            None,
            None,
        )?));
    }
    Ok(out)
}

/// A via of `net` at `at`, of the process's size unless `diameter` is given: a through via, or
/// with `layers` the process's blind or buried via that ends on them (issue 1071).
fn process_via(
    board: &Board,
    net: &str,
    at: netsmith_pcb::Pt,
    diameter: Option<f64>,
    layers: Option<&[String]>,
) -> Result<netsmith_pcb::Via, ApiError> {
    let offer = netsmith_fabricate::routes::via_offer(board, via_layers(board, layers)?.as_ref())
        .map_err(|r| match r {
        netsmith_fabricate::routes::Refusal::Refused(why)
        | netsmith_fabricate::routes::Refusal::Failed(why) => ApiError::bad_request(why),
    })?;
    Ok(netsmith_pcb::Via {
        net: net.to_string(),
        at,
        diameter: diameter.unwrap_or(offer.diameter),
        drill: offer.drill,
        span: offer.span,
    })
}

/// The two layers a via of the view names (`["top", "inner1"]`), `None` for none.
fn via_layers(
    board: &Board,
    words: Option<&[String]>,
) -> Result<Option<[CopperLayer; 2]>, ApiError> {
    let Some(words) = words else {
        return Ok(None);
    };
    match words {
        [a, b] => match (CopperLayer::of_word(a), CopperLayer::of_word(b)) {
            (Some(a), Some(b)) if a != b => Ok(Some([a, b])),
            _ => Err(ApiError::bad_request(format!(
                "a via of {} ends on two layers, `top`, `bottom` or `inner1`, ..., not `{a}` and `{b}`",
                board.name
            ))),
        },
        _ => Err(ApiError::bad_request(format!(
            "a via of {} ends on two layers, not {}",
            board.name,
            words.len()
        ))),
    }
}

/// The router's copper crossed by the new copper of a walk: each run of the track on its layer.
fn crossed_by(
    ground: &netsmith_router::walk::Ground,
    board: &Board,
    router: &RouterCopper,
    walk: &PcbWalk,
    corners: &[netsmith_pcb::Pt],
    layer: CopperLayer,
    width: f64,
) -> Result<netsmith_router::walk::Crossed, ApiError> {
    let point = |p: &PcbPoint| netsmith_pcb::Pt::new(p.x, p.y);
    let mut all = ground.crossed(
        &walk.net,
        layer,
        width,
        corners,
        &router.tracks,
        &router.vias,
    );
    for run in &walk.earlier {
        let l = track_layer(board, &run.layer)?;
        let pts: Vec<netsmith_pcb::Pt> = run.points.iter().map(point).collect();
        let w = board.track_width_on(&walk.net, l);
        let more = ground.crossed(&walk.net, l, w, &pts, &router.tracks, &router.vias);
        all.tracks.extend(more.tracks);
        all.vias.extend(more.vias);
    }
    all.tracks.sort_unstable();
    all.tracks.dedup();
    all.vias.sort_unstable();
    all.vias.dedup();
    Ok(all)
}

/// The way for a straight track from its last corner to a point, asked on `grounds` (the copper
/// a track is checked against, [`Entry::board_for_walk`]): the corners of
/// `netsmith_router::walk`, or the reason there are none. A way not found is an answer, not an
/// error.
pub fn walk(
    grounds: &Grounds,
    router: &RouterCopper,
    shove: Option<&ShoveGrounds>,
    walk: &PcbWalk,
) -> Result<PcbWalked, ApiError> {
    let board = &grounds.board;
    let layer = track_layer(board, &walk.layer)?;
    if !board.nets.iter().any(|n| n.name == walk.net) {
        return Err(ApiError::bad_request(format!(
            "the board {} has no net `{}`",
            board.name, walk.net
        )));
    }
    let ground = &grounds.ground;
    let point = |p: &PcbPoint| netsmith_pcb::Pt::new(p.x, p.y);
    let asked = netsmith_router::walk::Walk {
        net: &walk.net,
        layer,
        width: walk
            .width
            .unwrap_or_else(|| board.track_width_on(&walk.net, layer)),
        from: point(&walk.from),
        to: point(&walk.to),
        heading: walk.heading.as_ref().map(point),
    };
    let earlier = earlier_copper(board, walk)?;
    // The corners of the track so far.
    let mut so_far: Vec<netsmith_pcb::Pt> = walk.track.iter().map(point).collect();
    if so_far.last().is_none_or(|l| l.distance(asked.from) > 1e-9) {
        so_far.push(asked.from);
    }
    let mut shoved = PcbShoved::default();
    let mut stuck: Option<String> = None;
    let mut walked = None;
    if let Some(sg) = shove {
        // The way through the copper that may move, and then that copper out of the way of it;
        // where that cannot be done, the way round all of it below.
        match sg.ground.walker().path(&asked) {
            Ok(found) => {
                let mut corners = so_far.clone();
                corners.extend(found.points.iter().skip(1).copied());
                let pieces = sg.pieces(&[]);
                let drawn = netsmith_router::shove::Drawn {
                    net: &walk.net,
                    layer,
                    width: asked.width,
                    corners: &corners,
                    others: &earlier,
                };
                match sg.ground.shove(&drawn, &pieces) {
                    Ok(done) => {
                        shoved = shoved_of(&sg.movable, &done);
                        walked = Some(Ok(found));
                    }
                    Err(why) => stuck = Some(stuck_text(&sg.movable, &why)),
                }
            }
            // No way even through the copper that may move: the way round it has none either.
            Err(blocked) => walked = Some(Err(blocked)),
        }
    }
    let walked = match walked {
        Some(w) => w,
        None => ground.walker().path(&asked),
    };
    // The router's copper the track crosses: the track so far and the way found. The router's
    // copper is no obstacle to the walk (an edit discards it), so this only says what gives way.
    let mut corners = so_far;
    if let Ok(found) = &walked {
        corners.extend(found.points.iter().skip(1).copied());
    }
    let crossed = crossed_by(ground, board, router, walk, &corners, layer, asked.width)?;
    let crossed = crossed_of(router, &crossed);
    Ok(match walked {
        Ok(found) => PcbWalked {
            points: Some(
                found
                    .points
                    .iter()
                    .map(|p| PcbPoint { x: p.x, y: p.y })
                    .collect(),
            ),
            reason: None,
            tried: found.tried as u32,
            crossed,
            shoved,
            stuck,
        },
        Err(blocked) => PcbWalked {
            points: None,
            reason: Some(blocked.to_string()),
            tried: blocked.tried() as u32,
            crossed,
            shoved,
            stuck,
        },
    })
}

/// Whether the copper of a via or of a drag may go where it is, and with `shove` the shapes
/// moved out of its way (issue 1009): first on the ground of the shove (what does not move: the
/// copper is refused where it is too near that), then the shove; where the shove is stuck, or
/// not asked, the copper is checked against all of the routes file's copper where it is.
pub fn shove_answer(
    grounds: &Grounds,
    shove: Option<&ShoveGrounds>,
    ask: &PcbShove,
) -> Result<PcbShoveAnswer, ApiError> {
    use netsmith_router::shove::What;
    let board = &grounds.board;
    if !board.nets.iter().any(|n| n.name == ask.net) {
        return Err(ApiError::bad_request(format!(
            "the board {} has no net `{}`",
            board.name, ask.net
        )));
    }
    let point = |p: &PcbPoint| netsmith_pcb::Pt::new(p.x, p.y);
    let mut copper = Vec::new();
    for t in &ask.tracks {
        let layer = track_layer(board, &t.layer)?;
        let width = t
            .width
            .unwrap_or_else(|| board.track_width_on(&ask.net, layer));
        copper.push(What::Track(netsmith_pcb::Track::straight(
            &ask.net,
            layer,
            point(&t.from),
            point(&t.to),
            width,
        )));
    }
    for v in &ask.vias {
        copper.push(What::Via(process_via(
            board,
            &ask.net,
            point(&v.at),
            v.diameter,
            v.layers.as_deref(),
        )?));
    }
    let mut answer = PcbShoveAnswer {
        reason: None,
        shoved: PcbShoved::default(),
        stuck: None,
    };
    if let Some(sg) = shove {
        if let Some(by) = sg.ground.in_the_way(&ask.net, &copper) {
            answer.reason = Some(no_room(&by));
            return Ok(answer);
        }
        let drawn = netsmith_router::shove::Drawn {
            net: &ask.net,
            layer: CopperLayer::Front,
            width: 0.0,
            corners: &[],
            others: &copper,
        };
        match sg.ground.shove(&drawn, &sg.pieces(&ask.moving)) {
            Ok(done) => {
                answer.shoved = shoved_of(&sg.movable, &done);
                return Ok(answer);
            }
            Err(why) => answer.stuck = Some(stuck_text(&sg.movable, &why)),
        }
    }
    answer.reason = grounds
        .ground
        .in_the_way(&ask.net, &copper)
        .map(|by| no_room(&by));
    Ok(answer)
}

/// What a via or a drag with no room says: what it is too near.
fn no_room(by: &str) -> String {
    format!("it is on or too near {by}")
}

/// The router's nets the track of `walk` crosses, routed again round it: the preview of
/// `netsmith_router::round` (issue 1009). `walk.track` is the run on `walk.layer`, with the way
/// found.
pub fn reroute(
    grounds: &Grounds,
    router: &RouterCopper,
    walk: &PcbWalk,
) -> Result<PcbRerouted, ApiError> {
    use netsmith_router::shove::What;
    let board = &grounds.board;
    let layer = track_layer(board, &walk.layer)?;
    let width = walk
        .width
        .unwrap_or_else(|| board.track_width_on(&walk.net, layer));
    let point = |p: &PcbPoint| netsmith_pcb::Pt::new(p.x, p.y);
    let corners: Vec<netsmith_pcb::Pt> = walk.track.iter().map(point).collect();
    let crossed = crossed_by(&grounds.ground, board, router, walk, &corners, layer, width)?;
    let nets: std::collections::BTreeSet<String> = crossed
        .tracks
        .iter()
        .map(|&i| router.tracks[i].net.clone())
        .chain(crossed.vias.iter().map(|&i| router.vias[i].net.clone()))
        .collect();
    let mut tracks: Vec<netsmith_pcb::Track> = corners
        .windows(2)
        .map(|w| netsmith_pcb::Track::straight(&walk.net, layer, w[0], w[1], width))
        .collect();
    let mut vias = Vec::new();
    for c in earlier_copper(board, walk)? {
        match c {
            What::Track(t) => tracks.push(t),
            What::Via(v) => vias.push(v),
        }
    }
    let round = netsmith_router::round::reroute_round(
        board,
        &router.tracks,
        &router.vias,
        &tracks,
        &vias,
        &nets,
        None,
    );
    let at = |p: netsmith_pcb::Pt| PcbPoint { x: p.x, y: p.y };
    Ok(PcbRerouted {
        nets: nets.into_iter().collect(),
        tracks: round
            .tracks
            .iter()
            .map(|t| PcbCrossedTrack {
                net: t.net.clone(),
                layer: t.layer.word().to_string(),
                width: t.width,
                from: at(t.from),
                mid: t.mid.map(at),
                to: at(t.to),
            })
            .collect(),
        vias: round
            .vias
            .iter()
            .map(|v| PcbCrossedVia {
                net: v.net.clone(),
                at: at(v.at),
                diameter: v.diameter,
            })
            .collect(),
        left: round.left as u32,
    })
}

/// A shape of the routes file that a shove may move (`Entry::board_for_shove`): its net, its
/// number in the net's record from 1, and the shape on the sheet.
pub struct Movable {
    pub net: String,
    pub number: u32,
    pub shape: MovableShape,
}

pub enum MovableShape {
    Track(netsmith_pcb::Track),
    Via(netsmith_pcb::Via),
}

/// The shapes a shove moved, each with where it was and where it goes.
fn shoved_of(movable: &[Movable], done: &netsmith_router::shove::Shoved) -> PcbShoved {
    use netsmith_router::shove::Moved;
    let at = |p: netsmith_pcb::Pt| PcbPoint { x: p.x, y: p.y };
    let mut out = PcbShoved::default();
    for m in &done.moved {
        let found = &movable[m.id()];
        match (m, &found.shape) {
            (Moved::Track { from, to, mid, .. }, MovableShape::Track(t)) => {
                out.tracks.push(PcbShovedTrack {
                    net: found.net.clone(),
                    track: found.number,
                    layer: t.layer.word().to_string(),
                    width: t.width,
                    was_from: at(t.from),
                    was_to: at(t.to),
                    from: at(*from),
                    to: at(*to),
                    was_mid: t.mid.map(at),
                    mid: mid.map(at),
                });
            }
            (Moved::Via { at: now, .. }, MovableShape::Via(v)) => out.vias.push(PcbShovedVia {
                net: found.net.clone(),
                via: found.number,
                diameter: v.diameter,
                was_at: at(v.at),
                at: at(*now),
            }),
            _ => {}
        }
    }
    out
}

/// What a shove that cannot be done says: the shape that cannot move out of the way, by name.
fn stuck_text(movable: &[Movable], why: &netsmith_router::shove::Stuck) -> String {
    use netsmith_router::shove::Stuck;
    match why {
        Stuck::Blocked { by: Some(id), .. } => {
            let m = &movable[*id];
            let what = match m.shape {
                MovableShape::Track(_) => "track",
                MovableShape::Via(_) => "via",
            };
            format!(
                "{what} {} of net {} cannot be moved out of the way (copper that stays is too near, or its ends are held)",
                m.number, m.net
            )
        }
        Stuck::Blocked { by: None, .. } => {
            "the copper in the way cannot be moved out of the way".to_string()
        }
        Stuck::TooMany { .. } => {
            "too many shapes would have to be moved out of the way".to_string()
        }
    }
}

/// The pieces of the router's copper at `crossed`'s indexes, with their places.
fn crossed_of(router: &RouterCopper, crossed: &netsmith_router::walk::Crossed) -> PcbCrossed {
    let at = |p: netsmith_pcb::Pt| PcbPoint { x: p.x, y: p.y };
    PcbCrossed {
        tracks: crossed
            .tracks
            .iter()
            .map(|&i| {
                let t = &router.tracks[i];
                PcbCrossedTrack {
                    net: t.net.clone(),
                    layer: t.layer.word().to_string(),
                    width: t.width,
                    from: at(t.from),
                    mid: t.mid.map(at),
                    to: at(t.to),
                }
            })
            .collect(),
        vias: crossed
            .vias
            .iter()
            .map(|&i| {
                let v = &router.vias[i];
                PcbCrossedVia {
                    net: v.net.clone(),
                    at: at(v.at),
                    diameter: v.diameter,
                }
            })
            .collect(),
    }
}

/// The entry `place` of the layout file as the page describes it.
fn placement_of(place: &PartPlace) -> PartPlacement {
    PartPlacement {
        x: place.at[0],
        y: place.at[1],
        rotation: place.rotation,
        side: if place.side == LayoutFace::Bottom {
            PcbSide::Bottom
        } else {
            PcbSide::Top
        },
        locked: place.lock,
        source: place.source.clone(),
    }
}

/// One module of `entry`'s board placed again with the rest held (issue #825,
/// `netsmith_fabricate::regenerate`): the proposal, with the edit that makes it. Nothing is
/// written; a module that cannot be regenerated is 422 with the reason.
pub fn regenerate(
    entry: &Entry,
    ask: &PcbRegenerate,
    say: &dyn Fn(&str),
) -> Result<PcbRegenerated, ApiError> {
    let request = netsmith_fabricate::regenerate::Request {
        module: ask.module.clone(),
        seed: u64::from(ask.seed),
        margin: ask
            .margin
            .unwrap_or(netsmith_fabricate::regenerate::DEFAULT_MARGIN),
    };
    if !request.margin.is_finite() || request.margin < 0.0 {
        return Err(ApiError::bad_request("a margin must be 0 mm or more"));
    }
    let (proposal, after, _) = netsmith_fabricate::regenerate::propose_on(
        &entry.placed,
        entry.layout.as_ref().map(|(l, _)| l),
        Some(&entry.routes),
        &request,
        say,
    )
    .map_err(|why| {
        ApiError::unprocessable(
            format!("{}: {why}", ask.board),
            "regenerate_refused",
            &[("board", &ask.board), ("module", &ask.module)],
        )
    })?;
    let o = entry.placed.outline.min;
    let changes = proposal
        .parts
        .iter()
        .map(|m| PcbChange::Part {
            path: m.path.clone(),
            before: m.before.as_ref().map(placement_of),
            after: Some(placement_of(&m.after)),
        })
        .collect();
    let parts = proposal
        .parts
        .iter()
        .filter_map(|m| {
            after.part(&m.refdes).map(|p| PcbRegeneratedPart {
                path: m.path.clone(),
                refdes: m.refdes.clone(),
                courtyard: sheet_box(p.courtyard()),
            })
        })
        .collect();
    let totals = |t: &netsmith_pcb::Totals| PcbTotals {
        hpwl: t.hpwl,
        ratsnest: t.ratsnest,
        crossings: u32::try_from(t.crossings).unwrap_or(u32::MAX),
    };
    let c = &proposal.comparison;
    Ok(PcbRegenerated {
        module: proposal.module.clone(),
        seed: ask.seed,
        region: PcbBox {
            x: proposal.region.x + o.x,
            y: proposal.region.y + o.y,
            width: proposal.region.width,
            height: proposal.region.height,
        },
        changes,
        parts,
        held: proposal.held.clone(),
        nets: proposal.nets.clone(),
        routes_dropped: proposal.routes_dropped.clone(),
        appeared: c.appeared.clone(),
        gone: c.gone.clone(),
        before: totals(&c.before),
        after: totals(&c.after),
    })
}

/// `before` is what the page last saw of an entry and `now` what the file has: the same, to
/// the hundredth of a millimetre the file keeps.
fn same_part(before: Option<&PartPlacement>, now: Option<&PartPlace>) -> bool {
    match (before, now) {
        (None, None) => true,
        (Some(b), Some(n)) => {
            (b.x - n.at[0]).abs() < 0.006
                && (b.y - n.at[1]).abs() < 0.006
                && turn_between(b.rotation, n.rotation) < 0.006
                && (b.side == PcbSide::Bottom) == (n.side == LayoutFace::Bottom)
                && b.locked == n.lock
        }
        _ => false,
    }
}

/// The smaller turn from one angle to another, in degrees.
fn turn_between(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// The entry for a part as the page describes it. What the page does not show (which side
/// the reference text is on, and the designator's place) is kept from the entry it replaces.
fn part_place(after: &PartPlacement, old: Option<&PartPlace>) -> Result<PartPlace, ApiError> {
    if ![after.x, after.y, after.rotation]
        .iter()
        .all(|v| v.is_finite())
    {
        return Err(ApiError::bad_request(
            "a position or a rotation that is not a number",
        ));
    }
    Ok(PartPlace {
        at: [round(after.x), round(after.y)],
        rotation: round(after.rotation.rem_euclid(360.0)),
        side: if after.side == PcbSide::Bottom {
            LayoutFace::Bottom
        } else {
            LayoutFace::Top
        },
        text: old.map(|o| o.text).unwrap_or_default(),
        designator: old.and_then(|o| o.designator),
        lock: after.locked,
        source: after.source.clone().or_else(|| Some("hand".to_string())),
    })
}

fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// A keepout's entry as the page describes it, with the layers the board has.
fn keepout_placement(place: &KeepoutPlace, layers: &[String]) -> KeepoutPlacement {
    let forbids = |word: &Option<String>| word.as_deref() != Some("allow");
    KeepoutPlacement {
        x: place.at[0],
        y: place.at[1],
        drawn: place.size.map(|size| DrawnKeepout {
            width: size[0],
            height: size[1],
            layers: match place.layers.as_deref().map(str::trim) {
                None | Some("all") => layers.to_vec(),
                Some(list) => list.split(',').map(|l| l.trim().to_string()).collect(),
            },
            tracks: forbids(&place.tracks),
            vias: forbids(&place.vias),
            copper: forbids(&place.copper),
            parts: forbids(&place.parts),
        }),
    }
}

pub(crate) fn same_keepout(
    before: Option<&KeepoutPlacement>,
    now: Option<&KeepoutPlace>,
    layers: &[String],
) -> bool {
    match (before, now) {
        (None, None) => true,
        (Some(b), Some(n)) => {
            let now = keepout_placement(n, layers);
            (b.x - now.x).abs() < 0.006
                && (b.y - now.y).abs() < 0.006
                && match (&b.drawn, &now.drawn) {
                    (None, None) => true,
                    (Some(b), Some(n)) => {
                        (b.width - n.width).abs() < 0.006
                            && (b.height - n.height).abs() < 0.006
                            && (b.tracks, b.vias, b.copper, b.parts)
                                == (n.tracks, n.vias, n.copper, n.parts)
                            && layers
                                .iter()
                                .filter(|l| b.layers.contains(l))
                                .eq(layers.iter().filter(|l| n.layers.contains(l)))
                    }
                    _ => false,
                }
        }
        _ => false,
    }
}

pub(crate) fn keepout_place(
    after: &KeepoutPlacement,
    layers: &[String],
) -> Result<KeepoutPlace, ApiError> {
    let mut place = KeepoutPlace::at(round(after.x), round(after.y));
    if !after.x.is_finite() || !after.y.is_finite() {
        return Err(ApiError::bad_request(
            "a keepout at a position that is not a number",
        ));
    }
    if let Some(d) = &after.drawn {
        place.size = Some([round(d.width), round(d.height)]);
        // In board order, whatever order the page listed them in.
        let chosen: Vec<&String> = layers.iter().filter(|l| d.layers.contains(l)).collect();
        if let Some(unknown) = d.layers.iter().find(|l| !layers.contains(l)) {
            return Err(ApiError::bad_request(format!(
                "the keepout names the layer \"{unknown}\", and this board has {}",
                layers
                    .iter()
                    .map(|l| format!("\"{l}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        if chosen.len() != layers.len() {
            place.layers = Some(
                chosen
                    .iter()
                    .map(|l| l.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        let word = |forbidden: bool| (!forbidden).then(|| "allow".to_string());
        place.tracks = word(d.tracks);
        place.vias = word(d.vias);
        place.copper = word(d.copper);
        place.parts = word(d.parts);
    }
    Ok(place)
}

/// Put `changes` in `layout` for the board `board`, whose copper layers are `layers`. Nothing is
/// changed if one is refused: with 409 if an entry is not as the change says it was, and with
/// 400 if the new entry is not a position or a keepout.
pub fn change_layout(
    layout: &mut Layout,
    board: &str,
    layers: &[String],
    changes: &[PcbChange],
) -> Result<(), ApiError> {
    let mut next = layout.clone();
    let table = next.boards.entry(board.to_string()).or_default();
    for change in changes {
        match change {
            PcbChange::Part {
                path,
                before,
                after,
            } => {
                if !same_part(before.as_ref(), table.parts.get(path)) {
                    return Err(ApiError::conflict(format!(
                        "the layout of `{path}` is not what the page last saw (the file changed, or the page is out of date): reload the board"
                    )));
                }
                match after {
                    Some(after) => {
                        let entry = part_place(after, table.parts.get(path))?;
                        table.parts.insert(path.clone(), entry);
                    }
                    None => {
                        table.parts.remove(path);
                    }
                }
            }
            // The physical file's, written by `Workspace::physical_change`.
            PcbChange::Required { .. } => {}
            PcbChange::Keepout {
                name,
                before,
                after,
            } => {
                if !same_keepout(before.as_ref(), table.keepouts.get(name), layers) {
                    return Err(ApiError::conflict(format!(
                        "the layout of the keepout `{name}` is not what the page last saw (the file changed, or the page is out of date): reload the board"
                    )));
                }
                match after {
                    Some(after) => {
                        table
                            .keepouts
                            .insert(name.clone(), keepout_place(after, layers)?);
                    }
                    None => {
                        table.keepouts.remove(name);
                    }
                }
            }
        }
    }
    if next
        .boards
        .get(board)
        .is_some_and(|t| *t == BoardLayout::default())
    {
        next.boards.remove(board);
    }
    *layout = next;
    Ok(())
}

/// What a comparison reads of a placed board (ADR 0059, the extensions): where each part is, in
/// millimetres from the outline's top left corner as the layout file has them, the outline's
/// size, the copper layers, and the keepouts and cutouts, without drawing anything.
pub fn digest(entry: &Entry, data: &BoardData) -> PcbDigest {
    let board = &entry.board;
    let entries = entry.entries();
    let o = board.outline.min;
    // The layout file keeps the hundredth of a millimetre; so does a comparison.
    let round = |v: f64| (v * 100.0).round() / 100.0;
    let rel = |r: Rect| PcbBox {
        x: round(r.min.x - o.x),
        y: round(r.min.y - o.y),
        width: round(r.width()),
        height: round(r.height()),
    };
    let parts = board
        .parts
        .iter()
        .map(|p| {
            let place = entries.and_then(|e| e.parts.get(&p.path));
            PcbPartDigest {
                path: p.path.clone(),
                designator: p.refdes.clone(),
                place: ComparePcbPlace {
                    x: round(p.at.x - o.x),
                    y: round(p.at.y - o.y),
                    rotation: round(p.rotation),
                    side: side_of(p.side),
                    locked: place.is_some_and(|e| e.lock),
                    placement: place.map_or("derived".to_string(), |e| {
                        e.source.clone().unwrap_or_else(|| "hand".to_string())
                    }),
                },
            }
        })
        .collect();
    let areas = board.keepout_areas();
    let keepouts = board
        .keepouts
        .iter()
        .map(|k| {
            (
                k.name.clone(),
                areas.iter().find(|a| a.name == k.name).map(|a| rel(a.rect)),
            )
        })
        .collect();
    let cutouts = board
        .cutouts
        .iter()
        .map(|c| {
            (
                c.name.clone(),
                board
                    .cutout_shapes()
                    .into_iter()
                    .find(|(shape, _)| shape.name == c.name)
                    .map(|(_, shape)| rel(shape.rect)),
            )
        })
        .collect();
    // A board's own layout file (a board of a system defined in another file, issue #1327).
    let layout_file = match &data.layout {
        Some((file, None)) => file.clone(),
        _ => netsmith_fabricate::layout::layout_path(std::path::Path::new(&data.file)),
    }
    .to_string_lossy()
    .into_owned();
    PcbDigest {
        layout_file,
        outline_stated: data.built.design.features.outline.is_some(),
        outline: CompareSize {
            width: round(board.outline.width()),
            height: round(board.outline.height()),
        },
        layers: entry.layers(),
        parts,
        keepouts,
        cutouts,
    }
}

/// The copper layer a track tool names by its word: `top`, `bottom` or one of the board's inner
/// signal layers (`inner2`, issue 833).
fn track_layer(board: &Board, word: &str) -> Result<CopperLayer, ApiError> {
    CopperLayer::of_word(word)
        .filter(|l| l.is_outer() || board.signal_layers().contains(l))
        .ok_or_else(|| {
            ApiError::bad_request(format!(
                "a track of {} is drawn on `top`, `bottom` or an inner signal layer of its process, not `{word}`",
                board.name
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_pcb::{Glimpse, GlimpsePart, Pt, Rect};
    use std::sync::{Arc, Mutex};

    fn placing(parts: usize) -> Glimpse {
        Glimpse::Placing {
            region: Rect::new(Pt::new(0.0, 0.0), Pt::new(10.0, 5.0)),
            parts: (0..parts)
                .map(|i| GlimpsePart {
                    refdes: format!("R{i}"),
                    court: Rect::new(Pt::new(1.0, 1.0), Pt::new(3.0, 2.0)),
                    back: false,
                })
                .collect(),
        }
    }

    #[test]
    fn the_last_glimpse_of_a_placing_reaches_the_page_however_soon_it_ends() {
        // Two glimpses one after the other: the second is within the pace of the first on any
        // machine but the slowest, and is held back; it is passed when the placing ends
        // (issue #1249), so the page is left with it, whatever the speed.
        let shown = Arc::new(Mutex::new(Vec::<PcbLive>::new()));
        let show: Show = {
            let shown = Arc::clone(&shown);
            Arc::new(move |l: &PcbLive| shown.lock().unwrap().push(l.clone()))
        };
        watching_live("B", "place", &show, || {
            netsmith_pcb::show_glimpse(|| placing(6));
            netsmith_pcb::show_glimpse(|| placing(12));
        });
        let shown = shown.lock().unwrap();
        assert_eq!(
            shown.first().map(|l| l.parts.len()),
            Some(6),
            "the first is passed at once"
        );
        assert_eq!(
            shown.last().map(|l| l.parts.len()),
            Some(12),
            "the last is passed: {shown:?}"
        );
        assert!(shown.len() <= 2, "no glimpse is passed twice: {shown:?}");
    }
}
