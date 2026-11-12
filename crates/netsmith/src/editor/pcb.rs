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
//! drawing (`svg::render_view`, with `data-ref`, `data-path`, `data-pad`, `data-net`,
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
//! **Not built** (ADR 0030, issue #59): waivers (the page has no way to write one),
//! regeneration of one module, partial routing, hand-drawn tracks, a copy of the findings for
//! an agent, and `layout show` overlays.

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
        PartPlacement, PcbBox, PcbChange, PcbCutout, PcbFinding, PcbFindings, PcbKeepout, PcbPad,
        PcbPart, PcbReport, PcbScene, PcbSide, Severity,
    },
};
use netsmith_pcb::{
    Board, Layout, Rect, RouteOptions, Side,
    layout::{BoardLayout, Face, KeepoutPlace, PartPlace, copper_layers},
    route_with, svg,
};

use super::{
    analysis::BoardData,
    diff::{PcbDigest, PcbPartDigest},
    findings::{self, KicadResult},
    progress::{Reporting, forward},
};
use crate::fab::DEFAULT_ROUTE_TIMEOUT;

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
    constraints: Option<netsmith_fab::drc::constraints::Constraints>,
    /// Those requirements measured on the placed board: made when first asked for. Routing
    /// does not move a part, so it stays.
    constraint_findings: OnceLock<Vec<PcbFinding>>,
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
            let layout = crate::layout::read(file, boards).map_err(|e| {
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
        E::Design(_) => ApiError::unprocessable(message, "invalid_design", &[board]),
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
    let (board, _) = crate::layout::place(
        &data.built.design,
        layout.as_ref(),
        libraries,
        data.constraints_warn,
    )
    .map_err(|e| match e {
        crate::layout::PlaceFailure::Board(netsmith_pcb::BoardError::Unplaceable(found)) => {
            PlaceError::Unplaceable(name.to_string(), found)
        }
        crate::layout::PlaceFailure::Board(e) => PlaceError::Other(board_error(name, &e)),
        crate::layout::PlaceFailure::Layout(e) => PlaceError::Layout(format!("{name}: {e}")),
    })?;
    Ok(Entry {
        board,
        routed: false,
        stopped: false,
        unrouted: Vec::new(),
        elapsed: Duration::ZERO,
        layout,
        layout_stamp,
        kicad: None,
        drc: OnceLock::new(),
        constraints: crate::constraints::of_design(&data.built.design, data.constraints_warn),
        constraint_findings: OnceLock::new(),
    })
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
        glb: crate::board3d::glb(&data.built, &entry.board),
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
    PcbScene {
        board: name.to_string(),
        svg: svg::render_view(board),
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

/// `before` is what the page last saw of an entry and `now` what the file has: the same, to
/// the hundredth of a millimetre the file keeps.
fn same_part(before: Option<&PartPlacement>, now: Option<&PartPlace>) -> bool {
    match (before, now) {
        (None, None) => true,
        (Some(b), Some(n)) => {
            (b.x - n.at[0]).abs() < 0.006
                && (b.y - n.at[1]).abs() < 0.006
                && turn_between(b.rotation, n.rotation) < 0.006
                && (b.side == PcbSide::Bottom) == (n.side == Face::Bottom)
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
            Face::Bottom
        } else {
            Face::Top
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
    let layout_file = crate::layout::layout_path(std::path::Path::new(&data.file))
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
