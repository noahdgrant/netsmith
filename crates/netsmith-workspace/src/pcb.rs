//! The PCB view: place a board, route it when asked, and draw it (ADR 0030, section e).
//!
//! The page edits the layout file and the server only checks and writes it. A part is
//! selected by click, box, Shift or Ctrl and the parts list, and shows its pads, nets and
//! ratsnest; it is moved by dragging on a grid (a setting of the page), turned (`R`),
//! flipped (`F`), locked (`L`, drawn as a padlock) and handed back to the placer (`Delete`),
//! and every edit can be undone and redone (`Ctrl+Z`, `Ctrl+Shift+Z`). Keepouts are drawn
//! with a rectangle tool, moved, resized and deleted; the outline and the cutouts are the
//! source's and are only drawn. The keys and the tooltips are listed in
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
//! the data that finds things in it. An edit throws the routed copper away (there is no
//! routes cache), and a flip changes what the derived placer is told, so parts nobody placed
//! by hand may move.
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
//! **Not built** (ADR 0030, issue #59): waivers (the page has no way to write one),
//! regeneration of one module, partial routing, a copy of the findings for an agent, and `layout show` overlays.

use std::{
    collections::HashMap,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use netsmith_editor::{
    ApiError, Say,
    api::{
        ComparePcbPlace, CompareSize, DrawnKeepout, KeepoutOrigin, KeepoutPlacement, KicadState,
        PartPlacement, PcbBox, PcbChange, PcbCrossed, PcbCrossedTrack, PcbCrossedVia, PcbCutout,
        PcbFinding, PcbFindings, PcbKeepout, PcbNetWidth, PcbPad, PcbPart, PcbPoint, PcbReport,
        PcbRouteNet, PcbRouteTrack, PcbRouteVia, PcbScene, PcbShoved, PcbShovedTrack, PcbShovedVia,
        PcbSide, PcbWalk, PcbWalked, RoutesChange, Severity,
    },
};
use netsmith_pcb::{
    Board, BoardLayout, KeepoutPlace, Layout, LayoutFace, PartPlace, Rect, Routes, RoutesNote,
    Side, copper_layers, render_svg_view,
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
    /// What the layout file was like when it was read.
    layout_stamp: String,
    /// What KiCad's check found on the board as it is, if it was run: an edit places the
    /// board again, which forgets it.
    pub kicad: Option<KicadResult>,
    /// What netsmith's design rules check finds on the routed board: made when first asked
    /// for, and forgotten when the board is routed again.
    drc: OnceLock<Vec<PcbFinding>>,
    /// What the design requires of where the parts go (ADR 0045), if it says anything.
    constraints: Option<netsmith_geom::Constraints>,
    /// Those requirements measured on the placed board: made when first asked for. Routing
    /// does not move a part, so it stays.
    constraint_findings: OnceLock<Vec<PcbFinding>>,
    /// The board's fit in its case models (ADR 0109), measured when it is placed: routing does
    /// not move a part, so it stays.
    case_findings: Vec<PcbFinding>,
    /// The board as placed, before the routes file's copper is put on it: what a change to the
    /// routes file is made and checked on, as `netsmith routes` does.
    placed: Board,
    /// The routes file beside the layout file (epic #599), if the board has a layout file.
    routes_file: Option<PathBuf>,
    /// What it was like when read, its records, and what of them was not used as written.
    routes_stamp: String,
    routes: Routes,
    routes_notes: Vec<RoutesNote>,
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

/// [`place`], keeping what the pre-check proved apart from other failures.
pub fn place_checked(data: &BoardData, name: &str) -> Result<Entry, PlaceError> {
    let (layout, stamp) = match &data.layout {
        Some((file, boards)) => {
            // The stamp first: a file that changes meanwhile then makes an edit that
            // was based on this reading refused, not applied over the change.
            let stamp = super::files::stamp_of(file);
            let layout = netsmith_fabricate::layout::read(file, boards).map_err(|e| {
                PlaceError::Other(ApiError::unprocessable(
                    format!("{name}: {e}"),
                    "layout_unreadable",
                    &[("board", name)],
                ))
            })?;
            (layout, stamp)
        }
        None => (None, "none".to_string()),
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

/// The board of `name` placed with `layout`, which was in the file at `stamp`.
pub fn place_with(
    data: &BoardData,
    name: &str,
    layout: Option<(Layout, String)>,
    layout_stamp: String,
) -> Result<Entry, PlaceError> {
    let other = |e: ApiError| PlaceError::Other(e);
    let libraries = data.libraries().map_err(other)?;
    let board = netsmith_fabricate::layout::place(
        &data.built.design,
        layout.as_ref(),
        libraries,
        data.constraints_warn,
        data.place_seeds,
    )
    .map_err(|e| match e {
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
    })?
    .board;
    Entry::with_routes(data, name, board, layout, layout_stamp)
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
            board,
            routed: false,
            stopped: false,
            unrouted: Vec::new(),
            elapsed: Duration::ZERO,
            layout,
            layout_stamp,
            kicad: None,
            drc: OnceLock::new(),
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
        })
    }

    /// The entry again with the routes file as it is now, the board as placed and unrouted: what
    /// the view shows once a change to the routes file is written.
    pub fn routes_changed(&self, data: &BoardData, name: &str) -> Result<Entry, PlaceError> {
        Entry::with_routes(
            data,
            name,
            self.placed.clone(),
            self.layout.clone(),
            self.layout_stamp.clone(),
        )
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
    /// the router's time limit lasts or until [`cancel`].
    pub fn route(&mut self, name: &str, say: &Say) {
        let started = Instant::now();
        CANCEL.store(false, Ordering::Relaxed);
        let _reporting = Reporting::to(say);
        say(&format!(
            "{name}: placed {} parts on a {} by {} mm board",
            self.board.parts.len(),
            self.board.outline.width(),
            self.board.outline.height()
        ));
        let routing = route_with(
            &mut self.board,
            RouteOptions {
                time_limit: Some(DEFAULT_ROUTE_TIMEOUT),
                progress: Some(forward),
                cancel: Some(&CANCEL),
                ..RouteOptions::default()
            },
        );
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

/// The board as placed (and routed) in `entry`, as the `.glb` file `board3d --glb` writes
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
    let parts = board
        .parts
        .iter()
        .map(|p| {
            let place = entries.and_then(|e| e.parts.get(&p.path));
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
                body: sheet_box(p.body()),
                height: p.height,
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
            } else if k.drawn {
                KeepoutOrigin::Drawn
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
            })
            .collect(),
        via_diameter: board.rules.via_diameter,
        via_drill: board.rules.via_drill,
        board: name.to_string(),
        svg: render_svg_view(board),
        origin_x: board.outline.min.x,
        origin_y: board.outline.min.y,
        outline: sheet_box(board.outline),
        layout_file,
        layout_stamp: entry.layout_stamp.clone(),
        layers: entry.layers(),
        routed: entry.routed,
        parts,
        keepouts,
        cutouts,
    }
}

/// The routes file's records for the board, on the sheet, each shape marked as drawn or not as
/// `netsmith_pcb::apply_routes` decides: every shape of a record whose pads are where it was
/// written, only the locked ones of one whose pads moved, none of a net the board does not have.
fn route_nets(entry: &Entry) -> Vec<PcbRouteNet> {
    let board = &entry.placed;
    let Some(table) = entry.routes.boards.get(&board.name) else {
        return Vec::new();
    };
    let at = |p: [f64; 2]| PcbPoint {
        x: board.outline.min.x + p[0],
        y: board.outline.min.y + p[1],
    };
    let face = |f: LayoutFace| match f {
        LayoutFace::Top => "top",
        LayoutFace::Bottom => "bottom",
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
                        layer: face(t.layer).to_string(),
                        width: t.width,
                        from: at(t.from),
                        mid: t.mid.map(at),
                        to: at(t.to),
                        lock: t.lock,
                        drawn: known && (current || t.lock),
                    })
                    .collect(),
                vias: record
                    .vias
                    .iter()
                    .map(|v| PcbRouteVia {
                        at: at(v.at),
                        diameter: v.diameter,
                        drill: v.drill,
                        lock: v.lock,
                        drawn: known && (current || v.lock),
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
            layer: match layer.as_str() {
                "top" => LayoutFace::Top,
                "bottom" => LayoutFace::Bottom,
                other => {
                    return Err(ApiError::bad_request(format!(
                        "a track is drawn on `top` or `bottom`, not `{other}`"
                    )));
                }
            },
            width: *width,
            from: at(from),
            mid: None,
            to: at(to),
            lock: false,
        },
        RoutesChange::AddVia { net, at: p } => Edit::AddVia {
            net: net.clone(),
            at: at(p),
            diameter: None,
            drill: None,
            lock: false,
        },
        RoutesChange::MoveTrack {
            net,
            track,
            from,
            to,
        } => Edit::MoveTrack {
            net: net.clone(),
            track: *track as usize,
            from: at(from),
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
    /// without the shapes that may move, and those shapes. A shape may move when it is a straight
    /// track or a via of the routes file that is not locked, in a record that is not locked and
    /// is current (its pads are where it was written). Everything else of the routes file stays
    /// on the board.
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
                    if t.lock || t.mid.is_some() {
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

/// The way for a straight track from its last corner to a point on `board` (the copper a track
/// is checked against, [`Entry::board_for_walk`]): the corners of `netsmith_router::walk`, or
/// the reason there are none. A way not found is an answer, not an error.
pub fn walk(
    board: &Board,
    router: &RouterCopper,
    shove: Option<&(Board, Vec<Movable>)>,
    walk: &PcbWalk,
) -> Result<PcbWalked, ApiError> {
    use netsmith_pcb::CopperLayer;
    let layer = match walk.layer.as_str() {
        "top" => CopperLayer::Front,
        "bottom" => CopperLayer::Back,
        other => {
            return Err(ApiError::bad_request(format!(
                "a track is drawn on `top` or `bottom`, not `{other}`"
            )));
        }
    };
    if !board.nets.iter().any(|n| n.name == walk.net) {
        return Err(ApiError::bad_request(format!(
            "the board {} has no net `{}`",
            board.name, walk.net
        )));
    }
    let ground = netsmith_router::walk::Ground::of(board).map_err(ApiError::internal)?;
    let point = |p: &PcbPoint| netsmith_pcb::Pt::new(p.x, p.y);
    let asked = netsmith_router::walk::Walk {
        net: &walk.net,
        layer,
        width: walk.width.unwrap_or_else(|| board.track_width(&walk.net)),
        from: point(&walk.from),
        to: point(&walk.to),
        heading: walk.heading.as_ref().map(point),
    };
    // The corners of the track so far.
    let mut so_far: Vec<netsmith_pcb::Pt> = walk.track.iter().map(point).collect();
    if so_far.last().is_none_or(|l| l.distance(asked.from) > 1e-9) {
        so_far.push(asked.from);
    }
    let mut shoved = PcbShoved::default();
    let mut stuck: Option<String> = None;
    let mut walked = None;
    if let Some((shove_board, movable)) = shove {
        // The way through the copper that may move, and then that copper out of the way of it;
        // where that cannot be done, the way round all of it below.
        let g = netsmith_router::walk::Ground::of(shove_board).map_err(ApiError::internal)?;
        match g.walker().path(&asked) {
            Ok(found) => {
                let mut corners = so_far.clone();
                corners.extend(found.points.iter().skip(1).copied());
                let pieces: Vec<netsmith_router::shove::Piece> = movable
                    .iter()
                    .enumerate()
                    .map(|(id, m)| netsmith_router::shove::Piece {
                        id,
                        what: match &m.shape {
                            MovableShape::Track(t) => {
                                netsmith_router::shove::What::Track(t.clone())
                            }
                            MovableShape::Via(v) => netsmith_router::shove::What::Via(v.clone()),
                        },
                    })
                    .collect();
                let drawn = netsmith_router::shove::Drawn {
                    net: &walk.net,
                    layer,
                    width: asked.width,
                    corners: &corners,
                };
                match g.shove(&drawn, &pieces) {
                    Ok(done) => {
                        shoved = shoved_of(movable, &done);
                        walked = Some(Ok(found));
                    }
                    Err(why) => stuck = Some(stuck_text(movable, &why)),
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
    let crossed = ground.crossed(
        &walk.net,
        layer,
        asked.width,
        &corners,
        &router.tracks,
        &router.vias,
    );
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
            (Moved::Track { from, to, .. }, MovableShape::Track(t)) => {
                out.tracks.push(PcbShovedTrack {
                    net: found.net.clone(),
                    track: found.number,
                    layer: if t.layer == netsmith_pcb::CopperLayer::Back {
                        "bottom".to_string()
                    } else {
                        "top".to_string()
                    },
                    width: t.width,
                    was_from: at(t.from),
                    was_to: at(t.to),
                    from: at(*from),
                    to: at(*to),
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
                    layer: if t.layer == netsmith_pcb::CopperLayer::Back {
                        "bottom".to_string()
                    } else {
                        "top".to_string()
                    },
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
/// the reference text is on) is kept from the entry it replaces.
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

fn same_keepout(
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

fn keepout_place(after: &KeepoutPlacement, layers: &[String]) -> Result<KeepoutPlace, ApiError> {
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
    let layout_file = netsmith_fabricate::layout::layout_path(std::path::Path::new(&data.file))
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
