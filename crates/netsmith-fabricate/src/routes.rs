//! The routes file beside a source file (ADR 0030, decision 2): `<stem>.routes.toml`, beside the
//! layout file `<stem>.pcb.toml`. Its format and what its copper does are in the header of
//! the routes module of `netsmith-pcb` (`netsmith_pcb::Routes`); this module finds it, reads it and puts it on a placed board, for
//! `fab`, the editor's PCB view and the `netsmith routes` commands. The import of a KiCad
//! board's copper is `netsmith_tools::routes_import`.

use std::path::{Path, PathBuf};

use netsmith_pcb::{
    Board, LayoutFace as Face, NetRoutes, Pt, RouteTrack, RouteVia, RouteZone, Routes, ZonePads,
    apply_routes,
};

/// The routes file of a source file: `led.nsm` has `led.routes.toml`.
pub fn routes_path(source: &Path) -> PathBuf {
    netsmith_pcb::routes_path(source)
}

/// The routes file beside a layout file: `led.pcb.toml` has `led.routes.toml`. Whatever reads
/// a source's layout file reads its routes file too.
pub fn beside_layout(layout: &Path) -> PathBuf {
    let name = layout
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let stem = name.strip_suffix(".pcb.toml").unwrap_or(name);
    layout.with_file_name(format!("{stem}.routes.toml"))
}

/// The routes in the file at `file`, with the name to quote it by, or `None` if there is no
/// such file. `boards` are the names of all the boards of the source file, so that a table for
/// a board that is gone is found. A file that cannot be read or is wrong is an error naming it.
pub fn read(file: &Path, boards: &[String]) -> Result<Option<(Routes, String)>, String> {
    let name = file.display().to_string();
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{name}: {e}")),
    };
    let routes = Routes::parse(&text, &name).map_err(|e| e.to_string())?;
    let known: Vec<&str> = boards.iter().map(String::as_str).collect();
    routes
        .check_boards(&name, &known)
        .map_err(|e| e.to_string())?;
    Ok(Some((routes, name)))
}

/// Put the routes file beside the layout file `layout` (if there is one) on the placed `board`,
/// and return what of it was not used as written, as warnings (the routes module of `netsmith-pcb` (`netsmith_pcb::Routes`)).
pub fn apply_beside(board: &mut Board, layout: Option<&Path>) -> Result<Vec<String>, String> {
    let Some(layout) = layout else {
        return Ok(Vec::new());
    };
    let file = beside_layout(layout);
    let Some((routes, name)) = read_unchecked(&file)? else {
        return Ok(Vec::new());
    };
    let notes = apply_routes(board, &routes, &name).map_err(|e| e.to_string())?;
    Ok(notes.iter().map(|n| format!("{name}: {n}")).collect())
}

/// [`read`] without the check of the boards' names, which `fab` makes once for the source: a
/// file with the tables of several boards is applied to each board in turn.
fn read_unchecked(file: &Path) -> Result<Option<(Routes, String)>, String> {
    let name = file.display().to_string();
    match std::fs::read_to_string(file) {
        Ok(text) => Ok(Some((
            Routes::parse(&text, &name).map_err(|e| e.to_string())?,
            name,
        ))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{name}: {e}")),
    }
}

/// The routes file at `file`, or none when there is none.
pub fn load(file: &Path) -> Result<Routes, String> {
    Ok(read_unchecked(file)?.map(|(r, _)| r).unwrap_or_default())
}

/// Write `routes` to `file`, or remove the file when it holds no record.
pub fn save(file: &Path, routes: &Routes) -> Result<(), String> {
    let empty = routes.boards.values().all(|b| b.nets.is_empty());
    let result = if empty {
        match std::fs::remove_file(file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        std::fs::write(file, routes.to_text())
    };
    result.map_err(|e| format!("{}: {e}", file.display()))
}

/// `board` with `routes` on it, as `fab` would route it, and what of the file was not used.
pub fn with_routes(board: &Board, routes: &Routes) -> Result<(Board, Vec<String>), String> {
    let mut board = board.clone();
    let notes = apply_routes(&mut board, routes, "the routes file").map_err(|e| e.to_string())?;
    Ok((board, notes.iter().map(ToString::to_string).collect()))
}

/// The errors of netsmith's design rules check of `board` as it is, each as a line, less those of
/// copper not yet drawn: the connections left (the router makes them) and a track or via end that
/// touches nothing (a route is drawn one shape at a time, so its first shape ends in the open
/// until the next is added; `fab`'s check still finds an end left so). The place of each is in
/// the routes file's frame (mm from the outline's top left corner, y down), the frame the
/// commands that edit the file take their places in, not KiCad's page.
pub fn drc_errors(board: &Board) -> Result<Vec<String>, String> {
    let report = netsmith_geom::drc_check(
        &netsmith_pcb::kicad_pcb(board),
        Some(&netsmith_pcb::kicad_pro(board)),
        &crate::fab::drc_extras(board),
    )
    .map_err(|e| format!("the design rules check: {e}"))?;
    Ok(report
        .errors()
        .filter(|f| {
            !matches!(
                f.rule,
                netsmith_geom::DrcRule::UnconnectedItems
                    | netsmith_geom::DrcRule::TrackDangling
                    | netsmith_geom::DrcRule::ViaDangling
            )
        })
        .map(|f| {
            let mut f = f.clone();
            f.at = netsmith_geom::Pt {
                x: f.at.x - board.outline.min.x,
                y: f.at.y - board.outline.min.y,
            };
            f.to_string()
        })
        .collect())
}

/// The errors of the design rules check that `after` has and `before` does not: what a change
/// to the routes file broke. A finding is the same finding when its text is.
pub fn new_errors(before: &Board, after: &Board) -> Result<Vec<String>, String> {
    let mut old = drc_errors(before)?;
    let mut new = Vec::new();
    for e in drc_errors(after)? {
        match old.iter().position(|o| *o == e) {
            Some(k) => {
                old.swap_remove(k);
            }
            None => new.push(e),
        }
    }
    Ok(new)
}

/// The board's own outline as a zone's outline, in the routes file's places: the corners of a
/// polygon outline, or the rectangle drawn in by the least space to the edge (and as far as a
/// rounded corner needs, so that its corners stay on the board). The fill keeps the edge's space
/// whatever the outline. Places are to 0.000001 mm, as the file keeps them.
pub fn board_outline(board: &Board) -> Vec<[f64; 2]> {
    let corners: Vec<Pt> = match &board.polygon {
        Some(p) => p.corners(),
        None => {
            let round = board.corner_radius * (1.0 - std::f64::consts::FRAC_1_SQRT_2);
            board
                .outline
                .expanded(-board.rules.copper_to_edge.max(round))
                .corners()
                .to_vec()
        }
    };
    corners
        .iter()
        .map(|c| {
            let c = *c - board.outline.min;
            let to_um = |v: f64| (v * 1e6).round() / 1e6;
            [to_um(c.x), to_um(c.y)]
        })
        .collect()
}

/// One shape of a net's record, numbered from 1 as `netsmith routes show` lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Track(usize),
    Via(usize),
    Zone(usize),
}

/// One change to the routes file: what `netsmith routes add-track`, `add-via`, `add-zone`,
/// `delete`, `lock` and `unlock` do, and the editor's PCB view with them. [`edit`] is the one implementation.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// A track of `net`; `width` `None` is the net's own width.
    AddTrack {
        net: String,
        layer: Face,
        width: Option<f64>,
        from: [f64; 2],
        mid: Option<[f64; 2]>,
        to: [f64; 2],
        lock: bool,
    },
    /// A via of `net`; `None` is the process's via.
    AddVia {
        net: String,
        at: [f64; 2],
        diameter: Option<f64>,
        drill: Option<f64>,
        lock: bool,
    },
    /// A zone of `net` on `layers` (`top`, `bottom`, `inner1`, ...) inside the polygon
    /// `outline`, in the routes file's places. A setting that is `None` is the design's own
    /// plane's ([`RouteZone::new`]). A zone is refused when it is wrong in itself, off the
    /// board, or brings an error of the design rules check.
    AddZone {
        net: String,
        layers: Vec<String>,
        outline: Vec<[f64; 2]>,
        clearance: Option<f64>,
        min_width: Option<f64>,
        thermal_gap: Option<f64>,
        spoke_width: Option<f64>,
        priority: u32,
        pads: ZonePads,
        lock: bool,
    },
    /// The net's whole record (`shape` `None`), or one shape of it. A locked one is refused.
    Delete { net: String, shape: Option<Shape> },
    /// Lock (`on`) or unlock the net (`shape` `None`) or one shape. Locking a net writes its
    /// record again with its pads as they are now.
    Lock {
        net: String,
        shape: Option<Shape>,
        on: bool,
    },
    /// Track `track` of the net (numbered from 1) with its ends at `from` and `to`: what the
    /// editor's drag of a track's end makes. A locked track, and an arc, are refused.
    MoveTrack {
        net: String,
        track: usize,
        from: [f64; 2],
        to: [f64; 2],
    },
    /// Via `via` of the net (numbered from 1) at `at`. A locked via is refused.
    MoveVia {
        net: String,
        via: usize,
        at: [f64; 2],
    },
    /// The net's record as `after` (`None`: no record), refused unless the file has `before`
    /// now: the editor's undo and redo of an edit, which put back the record it changed. A
    /// lock does not refuse it, as it puts back what the person had.
    Restore {
        net: String,
        before: Option<NetRoutes>,
        after: Option<NetRoutes>,
    },
}

impl Edit {
    /// The net the edit changes.
    pub fn net(&self) -> &str {
        match self {
            Edit::AddTrack { net, .. }
            | Edit::AddVia { net, .. }
            | Edit::AddZone { net, .. }
            | Edit::Delete { net, .. }
            | Edit::Lock { net, .. }
            | Edit::MoveTrack { net, .. }
            | Edit::MoveVia { net, .. }
            | Edit::Restore { net, .. } => net,
        }
    }
}

/// Why [`edit`] did not change the file.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    /// The edit is refused, and why: a shape that breaks the design rules, a locked shape, a net
    /// or shape the board or the file does not have. The file is as it was.
    Refused(String),
    /// It could not be tried: the file or the board could not be read.
    Failed(String),
}

impl From<String> for Refusal {
    fn from(e: String) -> Self {
        Refusal::Failed(e)
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Refused(e) | Refusal::Failed(e) => f.write_str(e),
        }
    }
}

/// Make `change` to `routes` for `board` (placed, with no routes on it), or refuse it and leave
/// `routes` as it was. An added or moved shape, a record put back, and a net locked again (which
/// can draw shapes a record whose pads moved had dropped), is refused when the board with it has
/// an error of the design rules check that it did not have before. Says what it did.
pub fn edit(board: &Board, routes: &mut Routes, change: &Edit) -> Result<String, Refusal> {
    edit_all(board, routes, std::slice::from_ref(change))
}

/// [`edit`] of several changes to one net's record at once (a track drawn in several segments,
/// the tracks that meet at a corner moved together): all are made, or none. The design rules
/// check is made once, on the board with all of them.
pub fn edit_all(board: &Board, routes: &mut Routes, changes: &[Edit]) -> Result<String, Refusal> {
    let Some(first) = changes.first() else {
        return Err(Refusal::Refused("no change to make".to_string()));
    };
    let net = first.net();
    if let Some(other) = changes.iter().find(|c| c.net() != net) {
        return Err(Refusal::Refused(format!(
            "the changes of one edit are of one net, not of {net} and {}",
            other.net()
        )));
    }
    let Some(hash) = netsmith_pcb::pad_hash(board, net) else {
        return Err(Refusal::Refused(format!(
            "the board {} has no net `{net}`",
            board.name
        )));
    };
    let check = changes.iter().any(|c| {
        matches!(
            c,
            Edit::AddTrack { .. }
                | Edit::AddVia { .. }
                | Edit::AddZone { .. }
                | Edit::MoveTrack { .. }
                | Edit::MoveVia { .. }
                | Edit::Restore { .. }
                | Edit::Lock { on: true, .. }
        )
    });
    let mut next = routes.clone();
    let zoned = next
        .boards
        .get(&board.name)
        .map(zoned_layers)
        .unwrap_or_default();
    let table = next.boards.entry(board.name.clone()).or_default();
    let mut said: Vec<String> = Vec::new();
    for change in changes {
        if let Edit::Restore { before, after, .. } = change {
            let now = record_text(&board.name, net, table.nets.get(net));
            if now != record_text(&board.name, net, before.as_ref()) {
                return Err(Refusal::Refused(format!(
                    "net {net}'s record in the routes file changed since that edit: it is not put back"
                )));
            }
            match after {
                Some(record) => table.nets.insert(net.to_string(), record.clone()),
                None => table.nets.remove(net),
            };
            said.push(format!("put back net {net}'s record"));
            continue;
        }
        let record = table
            .nets
            .entry(net.to_string())
            .or_insert_with(|| NetRoutes {
                pads: hash.clone(),
                ..NetRoutes::default()
            });
        said.push(change_record(board, record, change, &zoned)?);
        if matches!(change, Edit::Lock { on: true, .. }) {
            record.pads = hash.clone();
        }
        if record.shapes() == 0 && !record.lock {
            table.nets.remove(net);
        }
    }
    if table.nets.is_empty() {
        next.boards.remove(&board.name);
    }
    if check {
        let before = with_routes(board, routes)?.0;
        let after = with_routes(board, &next)?.0;
        if let Some(e) = new_zone_error(&before, &after) {
            return Err(Refusal::Refused(format!(
                "refused: {e}, and the routes file is not changed"
            )));
        }
        let new = new_errors(&before, &after)?;
        if !new.is_empty() {
            return Err(Refusal::Refused(format!(
                "refused: the design rules check finds {} new error(s) with it, and the routes file is not changed:\n  {}",
                new.len(),
                new.join("\n  ")
            )));
        }
    }
    *routes = next;
    Ok(summary(&said))
}

/// What stops the zones of `after` being filled (two zones of different nets with the same
/// priority that overlap, say), when it did not stop those of `before`: the design rules check
/// does not fill them, and `fab` stops on it.
fn new_zone_error(before: &Board, after: &Board) -> Option<String> {
    let error = |b: &Board| {
        if b.zones.is_empty() {
            return None;
        }
        netsmith_pcb::zone_fills(b)
            .err()
            .map(|e| e.0.trim_start_matches("the board's copper: ").to_string())
    };
    let new = error(after)?;
    (error(before).as_ref() != Some(&new)).then_some(new)
}

/// What several changes did, in a line: each thing said once, with how many times.
fn summary(said: &[String]) -> String {
    let mut out: Vec<(&str, usize)> = Vec::new();
    for s in said {
        match out.iter_mut().find(|(t, _)| t == s) {
            Some((_, n)) => *n += 1,
            None => out.push((s, 1)),
        }
    }
    out.into_iter()
        .map(|(s, n)| {
            if n == 1 {
                s.to_string()
            } else {
                format!("{s} ({n} times)")
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// [`edit`] of the routes file at `file`, written when the edit is made. Says what it did and
/// the file it wrote.
pub fn edit_file(board: &Board, file: &Path, change: &Edit) -> Result<String, Refusal> {
    edit_file_all(board, file, std::slice::from_ref(change))
}

/// [`edit_all`] of the routes file at `file`, written when the edits are made.
pub fn edit_file_all(board: &Board, file: &Path, changes: &[Edit]) -> Result<String, Refusal> {
    let mut routes = load(file)?;
    let said = edit_all(board, &mut routes, changes)?;
    save(file, &routes)?;
    Ok(format!("{said}; wrote {}", file.display()))
}

/// The text of one net's record as the routes file writes it, or `None` for no record: what the
/// editor keeps to undo an edit and hands back to put the record back ([`Edit::Restore`]). Two
/// records are the same when their texts are, to the 0.000001 mm the file keeps.
pub fn record_text(board: &str, net: &str, record: Option<&NetRoutes>) -> Option<String> {
    let record = record?;
    let mut routes = Routes::default();
    routes
        .boards
        .entry(board.to_string())
        .or_default()
        .nets
        .insert(net.to_string(), record.clone());
    Some(routes.to_text())
}

/// The record a [`record_text`] holds, or `None` for none.
pub fn record_of_text(
    board: &str,
    net: &str,
    text: Option<&str>,
) -> Result<Option<NetRoutes>, String> {
    let Some(text) = text else {
        return Ok(None);
    };
    let mut routes = Routes::parse(text, "the record").map_err(|e| e.to_string())?;
    routes
        .boards
        .remove(board)
        .and_then(|mut b| b.nets.remove(net))
        .map(Some)
        .ok_or_else(|| format!("the text holds no record of net {net} of {board}"))
}

/// `board`'s KiCad layers that a zone of the routes file's table is on.
fn zoned_layers(table: &netsmith_pcb::BoardRoutes) -> Vec<String> {
    table
        .nets
        .values()
        .flat_map(|r| r.zones.iter().flat_map(|z| z.layers.iter()))
        .filter_map(|l| netsmith_pcb::routes_kicad_layer(l))
        .collect()
}

fn change_record(
    board: &Board,
    record: &mut NetRoutes,
    change: &Edit,
    zoned: &[String],
) -> Result<String, Refusal> {
    let missing = |net: &str, what: &str, n: usize, have: usize| {
        Refusal::Refused(format!(
            "net {net} has no {what} {n} in the routes file (it has {have}; `netsmith routes show` numbers them)"
        ))
    };
    match change {
        Edit::AddTrack {
            net,
            layer,
            width,
            from,
            mid,
            to,
            lock,
        } => {
            record.tracks.push(RouteTrack {
                layer: *layer,
                width: width.unwrap_or_else(|| board.track_width(net)),
                from: *from,
                mid: *mid,
                to: *to,
                lock: *lock,
            });
            Ok(format!("added a track to net {net}"))
        }
        Edit::AddVia {
            net,
            at,
            diameter,
            drill,
            lock,
        } => {
            record.vias.push(RouteVia {
                at: *at,
                diameter: diameter.unwrap_or(board.rules.via_diameter),
                drill: drill.unwrap_or(board.rules.via_drill),
                lock: *lock,
            });
            Ok(format!("added a via to net {net}"))
        }
        Edit::AddZone {
            net,
            layers,
            outline,
            clearance,
            min_width,
            thermal_gap,
            spoke_width,
            priority,
            pads,
            lock,
        } => {
            let mut zone = RouteZone::new(board, layers.clone(), outline.clone());
            zone.clearance = clearance.unwrap_or(zone.clearance);
            zone.min_width = min_width.unwrap_or(zone.min_width);
            zone.thermal_gap = thermal_gap.unwrap_or(zone.thermal_gap);
            zone.spoke_width = spoke_width.unwrap_or(zone.spoke_width);
            zone.priority = *priority;
            zone.pads = *pads;
            zone.lock = *lock;
            zone.check(board).map_err(Refusal::Refused)?;
            if zone.clearance < board.rules.clearance {
                return Err(Refusal::Refused(format!(
                    "the zone's clearance {} mm is less than the process's space between copper, {} mm",
                    zone.clearance, board.rules.clearance
                )));
            }
            if zone.min_width < board.rules.min_track_width {
                return Err(Refusal::Refused(format!(
                    "the zone's minimum width {} mm is less than the narrowest track the process makes, {} mm",
                    zone.min_width, board.rules.min_track_width
                )));
            }
            let edge = board.edge();
            if let Some(p) = zone
                .outline
                .iter()
                .find(|p| !edge.contains(board.outline.min + Pt::new(p[0], p[1])))
            {
                return Err(Refusal::Refused(format!(
                    "the zone's corner ({}, {}) is off the board (the places are mm from the outline's top left corner, which is (0, 0); the board is {} by {} mm)",
                    p[0],
                    p[1],
                    board.outline.width(),
                    board.outline.height()
                )));
            }
            let words = layers.join(" and ");
            record.zones.push(zone);
            let replaced: Vec<String> = layers
                .iter()
                .filter_map(|l| {
                    let kicad = netsmith_pcb::routes_kicad_layer(l)?;
                    let had = board.pours.iter().any(|p| p.layer.name() == kicad)
                        || board.planes.iter().any(|p| p.layer == kicad);
                    (had && !zoned.contains(&kicad)).then(|| l.clone())
                })
                .collect();
            let mut said = format!("added a zone to net {net} on {words}");
            if !replaced.is_empty() {
                said.push_str(&format!(
                    "; the design's pour or plane on {} is not drawn now: that layer takes its zones from the routes file alone",
                    replaced.join(" and ")
                ));
            }
            Ok(said)
        }
        Edit::Delete { net, shape: None } => {
            let locked = record.lock
                || record.tracks.iter().any(|t| t.lock)
                || record.vias.iter().any(|v| v.lock)
                || record.zones.iter().any(|z| z.lock);
            if locked {
                return Err(Refusal::Refused(format!(
                    "net {net} or a shape of it is locked: unlock it first (`netsmith routes unlock`)"
                )));
            }
            let n = record.shapes();
            record.tracks.clear();
            record.vias.clear();
            record.zones.clear();
            Ok(format!("deleted net {net}'s record ({n} shape(s))"))
        }
        Edit::Delete {
            net,
            shape: Some(Shape::Track(n)),
        } => {
            let have = record.tracks.len();
            let t = record
                .tracks
                .get(n.wrapping_sub(1))
                .ok_or_else(|| missing(net, "track", *n, have))?;
            if t.lock {
                return Err(Refusal::Refused(format!(
                    "track {n} of net {net} is locked: unlock it first"
                )));
            }
            record.tracks.remove(n - 1);
            Ok(format!("deleted track {n} of net {net}"))
        }
        Edit::Delete {
            net,
            shape: Some(Shape::Via(n)),
        } => {
            let have = record.vias.len();
            let v = record
                .vias
                .get(n.wrapping_sub(1))
                .ok_or_else(|| missing(net, "via", *n, have))?;
            if v.lock {
                return Err(Refusal::Refused(format!(
                    "via {n} of net {net} is locked: unlock it first"
                )));
            }
            record.vias.remove(n - 1);
            Ok(format!("deleted via {n} of net {net}"))
        }
        Edit::Delete {
            net,
            shape: Some(Shape::Zone(n)),
        } => {
            let have = record.zones.len();
            let z = record
                .zones
                .get(n.wrapping_sub(1))
                .ok_or_else(|| missing(net, "zone", *n, have))?;
            if z.lock {
                return Err(Refusal::Refused(format!(
                    "zone {n} of net {net} is locked: unlock it first"
                )));
            }
            record.zones.remove(n - 1);
            Ok(format!("deleted zone {n} of net {net}"))
        }
        Edit::Lock {
            net,
            shape: None,
            on,
        } => {
            record.lock = *on;
            Ok(format!(
                "{} net {net}",
                if *on { "locked" } else { "unlocked" }
            ))
        }
        Edit::Lock {
            net,
            shape: Some(shape),
            on,
        } => {
            let word = if *on { "locked" } else { "unlocked" };
            match *shape {
                Shape::Track(n) => {
                    let have = record.tracks.len();
                    record
                        .tracks
                        .get_mut(n.wrapping_sub(1))
                        .ok_or_else(|| missing(net, "track", n, have))?
                        .lock = *on;
                    Ok(format!("{word} track {n} of net {net}"))
                }
                Shape::Via(n) => {
                    let have = record.vias.len();
                    record
                        .vias
                        .get_mut(n.wrapping_sub(1))
                        .ok_or_else(|| missing(net, "via", n, have))?
                        .lock = *on;
                    Ok(format!("{word} via {n} of net {net}"))
                }
                Shape::Zone(n) => {
                    let have = record.zones.len();
                    record
                        .zones
                        .get_mut(n.wrapping_sub(1))
                        .ok_or_else(|| missing(net, "zone", n, have))?
                        .lock = *on;
                    Ok(format!("{word} zone {n} of net {net}"))
                }
            }
        }
        Edit::MoveTrack {
            net,
            track: n,
            from,
            to,
        } => {
            let have = record.tracks.len();
            let t = record
                .tracks
                .get_mut(n.wrapping_sub(1))
                .ok_or_else(|| missing(net, "track", *n, have))?;
            if t.lock {
                return Err(Refusal::Refused(format!(
                    "track {n} of net {net} is locked: unlock it first"
                )));
            }
            if t.mid.is_some() {
                return Err(Refusal::Refused(format!(
                    "track {n} of net {net} is an arc, whose ends are not moved: delete it and draw it again"
                )));
            }
            t.from = *from;
            t.to = *to;
            Ok(format!("moved track {n} of net {net}"))
        }
        Edit::MoveVia { net, via: n, at } => {
            let have = record.vias.len();
            let v = record
                .vias
                .get_mut(n.wrapping_sub(1))
                .ok_or_else(|| missing(net, "via", *n, have))?;
            if v.lock {
                return Err(Refusal::Refused(format!(
                    "via {n} of net {net} is locked: unlock it first"
                )));
            }
            v.at = *at;
            Ok(format!("moved via {n} of net {net}"))
        }
        Edit::Restore { .. } => Err(Refusal::Failed(
            "a record put back is made by edit_all".to_string(),
        )),
    }
}

/// What `netsmith routes show` says of a board: each record, what of the file is not used, and
/// the connections left once the file's copper is on the board.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Shown {
    pub board: String,
    pub nets: Vec<ShownNet>,
    /// What of the file is not used as written.
    pub notes: Vec<String>,
    /// The connections the router would route.
    pub to_route: Vec<String>,
    /// The connections of locked nets that no copper of the file makes: left unrouted.
    pub locked_left: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ShownNet {
    pub net: String,
    pub lock: bool,
    pub tracks: Vec<RouteTrack>,
    pub vias: Vec<RouteVia>,
    pub zones: Vec<netsmith_pcb::RouteZone>,
}

/// [`Shown`] of `board` (placed, with no routes) and `routes`.
pub fn show(board: &Board, routes: &Routes) -> Result<Shown, String> {
    let (fixed, notes) = with_routes(board, routes)?;
    let nets = routes
        .boards
        .get(&board.name)
        .map(|t| {
            t.nets
                .iter()
                .map(|(net, r)| ShownNet {
                    net: net.clone(),
                    lock: r.lock,
                    tracks: r.tracks.clone(),
                    vias: r.vias.clone(),
                    zones: r.zones.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let (mut to_route, mut locked_left) = (Vec::new(), Vec::new());
    for line in netsmith_pcb::Ratsnest::of(&fixed).lines {
        if fixed.fixed.is_locked(&line.net) {
            locked_left.push(line.to_string());
        } else {
            to_route.push(line.to_string());
        }
    }
    Ok(Shown {
        board: board.name.clone(),
        nets,
        notes,
        to_route,
        locked_left,
    })
}

impl Shown {
    /// The text for a person: each net's record with its shapes numbered from 1, as `routes
    /// delete`, `lock` and `unlock` name them.
    pub fn text(&self) -> String {
        use std::fmt::Write as _;
        let mut s = String::new();
        let pt = |p: [f64; 2]| format!("({}, {})", p[0], p[1]);
        for n in &self.nets {
            let _ = writeln!(
                s,
                "net {}{}: {} track(s), {} via(s){}",
                n.net,
                if n.lock { " (locked)" } else { "" },
                n.tracks.len(),
                n.vias.len(),
                if n.zones.is_empty() {
                    String::new()
                } else {
                    format!(", {} zone(s)", n.zones.len())
                }
            );
            for (i, t) in n.tracks.iter().enumerate() {
                let layer = if t.layer == Face::Top {
                    "top"
                } else {
                    "bottom"
                };
                let mid = t
                    .mid
                    .map(|m| format!(" through {}", pt(m)))
                    .unwrap_or_default();
                let _ = writeln!(
                    s,
                    "  track {}: {layer}, {} mm, {}{mid} to {}{}",
                    i + 1,
                    t.width,
                    pt(t.from),
                    pt(t.to),
                    if t.lock { " (locked)" } else { "" }
                );
            }
            for (i, v) in n.vias.iter().enumerate() {
                let _ = writeln!(
                    s,
                    "  via {}: {}, {} mm, drill {} mm{}",
                    i + 1,
                    pt(v.at),
                    v.diameter,
                    v.drill,
                    if v.lock { " (locked)" } else { "" }
                );
            }
            for (i, z) in n.zones.iter().enumerate() {
                let _ = writeln!(
                    s,
                    "  zone {}: {}, priority {}, {} corners from {}, {} hole(s){}",
                    i + 1,
                    z.layers.join(" and "),
                    z.priority,
                    z.outline.len(),
                    pt(z.outline.first().copied().unwrap_or_default()),
                    z.holes.len(),
                    if z.lock { " (locked)" } else { "" }
                );
            }
        }
        if self.nets.is_empty() {
            let _ = writeln!(s, "{}: the routes file fixes no copper", self.board);
        }
        for note in &self.notes {
            let _ = writeln!(s, "warning: {note}");
        }
        let _ = writeln!(
            s,
            "the router would route {} connection(s):",
            self.to_route.len()
        );
        for l in &self.to_route {
            let _ = writeln!(s, "  {l}");
        }
        if !self.locked_left.is_empty() {
            let _ = writeln!(
                s,
                "{} connection(s) of locked nets have no copper, and stay unrouted:",
                self.locked_left.len()
            );
            for l in &self.locked_left {
                let _ = writeln!(s, "  {l}");
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_routes_file_is_beside_the_layout_file() {
        assert_eq!(
            beside_layout(Path::new("src/main.pcb.toml")),
            PathBuf::from("src/main.routes.toml")
        );
        assert_eq!(
            routes_path(Path::new("src/main.nsm")),
            PathBuf::from("src/main.routes.toml")
        );
    }

    /// The LED example placed, a fixture of `netsmith-pcb` (12 by 8 mm; nets GND, VCC and
    /// `net-led.anode`).
    fn led() -> Board {
        serde_json::from_str(include_str!(
            "../../netsmith-pcb/tests/fixtures/led.board.json"
        ))
        .expect("a fixture is the JSON of a Board")
    }

    fn zone(net: &str, outline: Vec<[f64; 2]>, priority: u32) -> Edit {
        Edit::AddZone {
            net: net.into(),
            layers: vec!["top".into()],
            outline,
            clearance: None,
            min_width: None,
            thermal_gap: None,
            spoke_width: None,
            priority,
            pads: ZonePads::Thermal,
            lock: false,
        }
    }

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Vec<[f64; 2]> {
        vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]]
    }

    fn refusal(r: Result<String, Refusal>) -> String {
        match r {
            Err(Refusal::Refused(e)) => e,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_zone_takes_the_designs_plane_settings_and_the_nets_record_is_made() {
        let board = led();
        let mut routes = Routes::default();
        let said = edit(&board, &mut routes, &zone("GND", board_outline(&board), 0)).unwrap();
        assert_eq!(said, "added a zone to net GND on top");
        let record = &routes.boards[&board.name].nets["GND"];
        assert_eq!(record.pads, netsmith_pcb::pad_hash(&board, "GND").unwrap());
        let z = &record.zones[0];
        assert_eq!(z.layers, ["top"]);
        assert_eq!(z.clearance, board.rules.plane_clearance);
        assert_eq!(
            (z.min_width, z.thermal_gap, z.spoke_width),
            (0.25, 0.5, 0.5)
        );
        // The board's outline is drawn in by the least space to the edge.
        let e = board.rules.copper_to_edge;
        assert_eq!(z.outline[0], [e, e]);
        assert_eq!(
            z.outline[2],
            [board.outline.width() - e, board.outline.height() - e]
        );
    }

    #[test]
    fn a_zone_is_numbered_locked_and_deleted_like_a_track() {
        let board = led();
        let mut routes = Routes::default();
        let add = |routes: &mut Routes, x| {
            edit(&board, routes, &zone("GND", rect(x, 1.0, 3.0, 3.0), 0)).unwrap()
        };
        add(&mut routes, 1.0);
        add(&mut routes, 5.0);
        let lock = |on| Edit::Lock {
            net: "GND".into(),
            shape: Some(Shape::Zone(2)),
            on,
        };
        let delete = |n| Edit::Delete {
            net: "GND".into(),
            shape: Some(Shape::Zone(n)),
        };
        edit(&board, &mut routes, &lock(true)).unwrap();
        let before = routes.clone();
        let said = refusal(edit(&board, &mut routes, &delete(2)));
        assert!(said.contains("zone 2 of net GND is locked"), "{said}");
        // A locked zone keeps the record from being deleted whole.
        let whole = Edit::Delete {
            net: "GND".into(),
            shape: None,
        };
        let said = refusal(edit(&board, &mut routes, &whole));
        assert!(said.contains("is locked"), "{said}");
        assert_eq!(routes, before);
        // A zone the record does not have is refused, with how many it has.
        let said = refusal(edit(&board, &mut routes, &delete(3)));
        assert!(
            said.contains("has no zone 3 in the routes file (it has 2"),
            "{said}"
        );
        edit(&board, &mut routes, &lock(false)).unwrap();
        edit(&board, &mut routes, &delete(2)).unwrap();
        assert_eq!(routes.boards[&board.name].nets["GND"].zones.len(), 1);
        // Delete of the record takes its zones, and the board's table with it.
        edit(&board, &mut routes, &whole).unwrap();
        assert!(routes.boards.is_empty(), "{routes:?}");
    }

    #[test]
    fn a_zone_off_the_board_or_against_another_nets_is_refused_and_the_file_kept() {
        let board = led();
        let mut routes = Routes::default();
        edit(
            &board,
            &mut routes,
            &zone("GND", rect(1.0, 1.0, 6.0, 5.0), 0),
        )
        .unwrap();
        let before = routes.clone();
        let mut refused = |change: Edit| {
            let said = refusal(edit(&board, &mut routes, &change));
            assert_eq!(routes, before, "{said}");
            said
        };

        // A corner past the board's edge, quoted in the routes file's places.
        let said = refused(zone("VCC", rect(8.0, 1.0, 6.0, 2.0), 0));
        assert!(said.contains("corner (14, 1) is off the board"), "{said}");
        // Two zones of different nets with the same priority that overlap: KiCad would choose.
        let said = refused(zone("VCC", rect(5.0, 2.0, 5.0, 3.0), 0));
        assert!(said.contains("have the same priority (0)"), "{said}");
        assert!(said.contains("the routes file is not changed"), "{said}");
        // A clearance the process does not make.
        let mut thin = zone("VCC", rect(8.0, 2.0, 2.0, 2.0), 0);
        if let Edit::AddZone { clearance, .. } = &mut thin {
            *clearance = Some(0.01);
        }
        let said = refused(thin);
        assert!(
            said.contains("less than the process's space between copper"),
            "{said}"
        );
        // A layer the board does not have, a zone of two corners, a net it does not have.
        let mut inner = zone("VCC", rect(8.0, 2.0, 2.0, 2.0), 0);
        if let Edit::AddZone { layers, .. } = &mut inner {
            *layers = vec!["inner1".into()];
        }
        assert!(refused(inner).contains("`inner1`"));
        assert!(refused(zone("VCC", vec![[1.0, 1.0], [2.0, 2.0]], 0)).contains("three or more"));
        let said = refused(zone("NOPE", rect(1.0, 1.0, 1.0, 1.0), 0));
        assert!(said.contains("has no net `NOPE`"), "{said}");

        // With the higher priority the zone has the area, and is added.
        edit(
            &board,
            &mut routes,
            &zone("VCC", rect(5.0, 2.0, 5.0, 3.0), 1),
        )
        .unwrap();
        assert_eq!(routes.boards[&board.name].nets["VCC"].zones[0].priority, 1);
    }

    #[test]
    fn a_zone_that_takes_the_place_of_a_pour_says_so() {
        let mut board = led();
        board.pours.push(netsmith_pcb::BoardPour {
            layer: netsmith_pcb::CopperLayer::Front,
            net: "GND".into(),
            stitch_pitch: None,
        });
        let mut routes = Routes::default();
        let said = edit(
            &board,
            &mut routes,
            &zone("GND", rect(1.0, 1.0, 3.0, 3.0), 0),
        )
        .unwrap();
        assert!(
            said.contains("the design's pour or plane on top is not drawn now"),
            "{said}"
        );
        // Once the file has a zone there, the next says nothing of it.
        let said = edit(
            &board,
            &mut routes,
            &zone("GND", rect(5.0, 1.0, 3.0, 3.0), 0),
        )
        .unwrap();
        assert_eq!(said, "added a zone to net GND on top");
    }
}
