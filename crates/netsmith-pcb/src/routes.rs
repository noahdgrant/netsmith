//! The routes file: copper a person, an agent or an import decided, kept apart from the source so
//! that a rebuild keeps it (ADR 0030, decision 2; issue #431, S10).
//!
//! `<stem>.routes.toml` sits beside the top-level `.nsm` file and its layout file
//! (`<stem>.pcb.toml`, [`crate::layout`]). It is optional: with no file every board is routed
//! as before, byte for byte. One table for each board of the source, keyed by the board's
//! name; inside it one record for each net, keyed by the net's name. Places are in mm from the
//! outline's top left corner, y down, as in the layout file.
//!
//! ```toml
//! version = 1
//!
//! [Rp2350Minimal.nets."+3V3"]
//! pads = "6c1f0a9e2b7d3354"     # where the net's pads were when the record was written
//! lock = true                   # the router adds no copper to the net
//! tracks = [
//!   { layer = "top", width = 0.2, from = [24.2, 35.6], to = [23.5, 36.3] },
//!   { layer = "top", width = 0.2, from = [23.5, 36.3], mid = [23.1, 36.6], to = [22.6, 36.7] },
//!   { layer = "bottom", width = 0.4, from = [5.0, 5.0], to = [9.0, 5.0], lock = true },
//! ]
//! vias = [
//!   { at = [12.3, 4.56], diameter = 0.6, drill = 0.25 },
//! ]
//!
//! [[Rp2350Minimal.nets."+3V3".zones]]
//! layers = ["top"]
//! priority = 1
//! clearance = 0.15
//! min_width = 0.15
//! pads = "solid"
//! thermal_gap = 0.508
//! spoke_width = 0.508
//! islands = "remove"
//! outline = [[23.2, 27.2], [28.8, 27.2], [28.8, 32.8], [23.2, 32.8]]
//! holes = [
//!   [[24.0, 28.0], [25.0, 28.0], [25.0, 29.0]],
//! ]
//! ```
//!
//! A track is straight from `from` to `to`, or an arc through `mid` (KiCad's `(arc (start)
//! (mid) (end))`); `layer` is `top` or `bottom` (tracks are drawn on the outer layers only, as
//! the router's are); `width`, `diameter` and `drill` are in mm. An unknown key is an error.
//!
//! **Zones** ([`RouteZone`]) are KiCad's zones (issue #724): copper filled inside `outline`,
//! less each of `holes`, on each of `layers` (`top`, `bottom`, `inner1`, ...), joined to the
//! record's net, with the settings of KiCad's zone that change the copper: `priority` (where two
//! zones of a layer overlap, the higher has the area; 0 when not given), `clearance` from other
//! nets' copper, `min_width` (KiCad's `min_thickness`), `pads` (KiCad's `connect_pads`: `solid`,
//! `thermal`, the default, `none` or `thru_hole_only`), `thermal_gap` and `spoke_width` of a
//! thermal relief, and `islands` (`remove`, the default, `keep`, or `remove_below` with
//! `island_min_area` in mm²). A layer that has a zone of the file takes its zones from the file
//! alone: the design's pour or plane on it is not drawn ([`Board::zoned`]); the other layers
//! keep theirs. The board file holds each zone as KiCad's, unfilled, so that KiCad's fill
//! makes the copper KiCad's own board has; netsmith's own fill, for the Gerbers, the check and
//! the ratsnest, is `netsmith_geom::fill`'s, with its differences (it removes no island and
//! trims no sliver). A zone is not an obstacle to the router: its fill keeps its clearance from
//! whatever the router draws, as KiCad's does.
//!
//! **What the copper does.** `fab`, the editor's PCB view and `netsmith route` (through the
//! placed board `fab --save-placed` saves) put every shape of a record on the placed board
//! before the router runs ([`apply_routes`]): the router keeps it as an obstacle of its net
//! and never moves or removes it, and routes only the connections it leaves (the ratsnest
//! counts the copper already there). The passes after the router (meanders, arcs, teardrops)
//! leave it as it is. The design rules check checks it with the rest of the copper.
//!
//! **Locks.** `lock = true` on a net says its copper is the file's alone: the router adds no
//! track, via or teardrop of that net, and the ratsnest counts the net's pours (the board's
//! pours on the outer layers) as joining what of the net they touch, as the design rules check
//! does. A connection a locked net still lacks is left, and `fab` says so. `lock = true` on a
//! shape keeps the shape when its record no longer matches the board (below), and the
//! `netsmith routes` commands do not delete a locked shape or net.
//!
//! **A record that no longer matches** is said, never used silently. `pads` is a hash of the
//! net's pads ([`pad_hash`]): each pad's part (by its path in the design, which a renumbered
//! reference designator does not change), number, centre to 1 µm and copper layers. When a
//! pad of the net moved, or one joined or left it, the record's shapes are dropped and the
//! net is routed again, except the shapes with `lock = true`, which are kept; the net's own
//! lock does not hold for that run. A record for a net the board does not have (a net renamed,
//! say) is dropped whole. Each is a [`RoutesNote`] that `fab` prints as a warning.
//! `netsmith routes show` lists them too, and writing the record again (an import, or `routes
//! lock`) takes the pads as they are now.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::board::{Board, BoardZone, CopperLayer, Track, Via, ZoneIslands, ZonePads};
use crate::geometry::Pt;
use crate::layout::Face;

/// The version of the file format this reads and writes.
pub const VERSION: u32 = 1;

/// The routes file of a source file: `led.nsm` has `led.routes.toml`.
pub fn routes_path(source: &std::path::Path) -> std::path::PathBuf {
    source.with_extension("routes.toml")
}

/// A track of a record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteTrack {
    pub layer: Face,
    pub width: f64,
    pub from: [f64; 2],
    /// For an arc, a point on it halfway from `from` to `to`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mid: Option<[f64; 2]>,
    pub to: [f64; 2],
    #[serde(default)]
    pub lock: bool,
}

/// A via of a record, through both outer layers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteVia {
    pub at: [f64; 2],
    pub diameter: f64,
    pub drill: f64,
    #[serde(default)]
    pub lock: bool,
}

/// A zone of a record: KiCad's zone of the record's net (the module's header).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteZone {
    /// `top`, `bottom`, `inner1`, ...
    pub layers: Vec<String>,
    #[serde(default)]
    pub priority: u32,
    pub clearance: f64,
    pub min_width: f64,
    #[serde(default)]
    pub pads: ZonePads,
    pub thermal_gap: f64,
    pub spoke_width: f64,
    #[serde(default)]
    pub islands: ZoneIslands,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub island_min_area: Option<f64>,
    pub outline: Vec<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Vec<[f64; 2]>>,
    #[serde(default)]
    pub lock: bool,
}

/// One net's record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetRoutes {
    /// [`pad_hash`] of the net when the record was written.
    pub pads: String,
    #[serde(default)]
    pub lock: bool,
    #[serde(default)]
    pub tracks: Vec<RouteTrack>,
    #[serde(default)]
    pub vias: Vec<RouteVia>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<RouteZone>,
}

/// What the file holds for one board.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardRoutes {
    #[serde(default)]
    pub nets: BTreeMap<String, NetRoutes>,
}

/// A whole routes file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Routes {
    pub boards: BTreeMap<String, BoardRoutes>,
}

/// A routes file that cannot be used.
#[derive(Debug, Clone, PartialEq)]
pub enum RoutesError {
    /// The file is not TOML of this shape, or of another version.
    Parse { file: String, message: String },
    /// A table for a board the source does not have.
    NoBoard {
        file: String,
        board: String,
        known: Vec<String>,
    },
    /// A shape that is wrong in itself.
    BadShape {
        file: String,
        net: String,
        why: String,
    },
}

impl std::fmt::Display for RoutesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoutesError::Parse { file, message } => write!(f, "{file}: {message}"),
            RoutesError::NoBoard { file, board, known } => write!(
                f,
                "{file}: `{board}` is not a board of the source (it has {}); rename or remove its table",
                known.join(", ")
            ),
            RoutesError::BadShape { file, net, why } => write!(f, "{file}: net `{net}`: {why}"),
        }
    }
}

impl std::error::Error for RoutesError {}

/// What [`apply_routes`] did not use as the file says, for a warning.
#[derive(Debug, Clone, PartialEq)]
pub enum RoutesNote {
    /// A record for a net the board does not have: nothing of it was drawn.
    NotANet { net: String, shapes: usize },
    /// The net's pads are not where the record says: its unlocked shapes were dropped and the
    /// net is routed again; its locked ones were kept.
    Moved {
        net: String,
        dropped: usize,
        kept: usize,
    },
}

impl std::fmt::Display for RoutesNote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoutesNote::NotANet { net, shapes } => write!(
                f,
                "the routes file has net `{net}`, which the board does not have: its {shapes} shape(s) are not drawn; rename the record or delete it (`netsmith routes delete`)"
            ),
            RoutesNote::Moved { net, dropped, kept } => write!(
                f,
                "the pads of net `{net}` are not where the routes file's record was written: {dropped} unlocked shape(s) dropped and the net routed again, {kept} locked shape(s) kept; write the record again (`netsmith routes lock`) or delete it"
            ),
        }
    }
}

impl Routes {
    /// Read a routes file. `file` is what a message calls it.
    pub fn parse(text: &str, file: &str) -> Result<Routes, RoutesError> {
        let parse_error = |message: String| RoutesError::Parse {
            file: file.to_string(),
            message,
        };
        let mut table: toml::Table = text
            .parse()
            .map_err(|e: toml::de::Error| parse_error(e.message().to_string()))?;
        match table.remove("version") {
            Some(toml::Value::Integer(v)) if v == i64::from(VERSION) => {}
            Some(other) => {
                return Err(parse_error(format!(
                    "version {other}, but this netsmith reads version {VERSION}"
                )));
            }
            None => return Err(parse_error(format!("no `version = {VERSION}`"))),
        }
        let mut boards = BTreeMap::new();
        for (name, value) in table {
            let board: BoardRoutes = value
                .try_into()
                .map_err(|e: toml::de::Error| parse_error(format!("[{name}]: {}", e.message())))?;
            boards.insert(name, board);
        }
        Ok(Routes { boards })
    }

    /// An error for a table of a board the source does not have, so a board renamed or removed
    /// is found.
    pub fn check_boards(&self, file: &str, boards: &[&str]) -> Result<(), RoutesError> {
        match self.boards.keys().find(|b| !boards.contains(&b.as_str())) {
            Some(board) => Err(RoutesError::NoBoard {
                file: file.to_string(),
                board: board.clone(),
                known: boards.iter().map(|b| b.to_string()).collect(),
            }),
            None => Ok(()),
        }
    }

    /// The file's text: records one after the other, each shape on a line of its own, so a
    /// change to one shape is a change to one line.
    pub fn to_text(&self) -> String {
        let mut out = format!("version = {VERSION}\n");
        for (board, routes) in &self.boards {
            for (net, record) in &routes.nets {
                let _ = writeln!(out, "\n[{}.nets.{}]", key(board), key(net));
                let _ = writeln!(out, "pads = {}", quoted(&record.pads));
                if record.lock {
                    out.push_str("lock = true\n");
                }
                if !record.tracks.is_empty() {
                    out.push_str("tracks = [\n");
                    for t in &record.tracks {
                        let layer = match t.layer {
                            Face::Top => "top",
                            Face::Bottom => "bottom",
                        };
                        let _ = write!(
                            out,
                            "  {{ layer = \"{layer}\", width = {}, from = {}",
                            num(t.width),
                            point(t.from)
                        );
                        if let Some(mid) = t.mid {
                            let _ = write!(out, ", mid = {}", point(mid));
                        }
                        let _ = write!(out, ", to = {}", point(t.to));
                        if t.lock {
                            out.push_str(", lock = true");
                        }
                        out.push_str(" },\n");
                    }
                    out.push_str("]\n");
                }
                if !record.vias.is_empty() {
                    out.push_str("vias = [\n");
                    for v in &record.vias {
                        let _ = write!(
                            out,
                            "  {{ at = {}, diameter = {}, drill = {}",
                            point(v.at),
                            num(v.diameter),
                            num(v.drill)
                        );
                        if v.lock {
                            out.push_str(", lock = true");
                        }
                        out.push_str(" },\n");
                    }
                    out.push_str("]\n");
                }
                for z in &record.zones {
                    let _ = writeln!(out, "\n[[{}.nets.{}.zones]]", key(board), key(net));
                    let layers: Vec<String> = z.layers.iter().map(|l| quoted(l)).collect();
                    let _ = writeln!(out, "layers = [{}]", layers.join(", "));
                    let _ = writeln!(out, "priority = {}", z.priority);
                    let _ = writeln!(out, "clearance = {}", num(z.clearance));
                    let _ = writeln!(out, "min_width = {}", num(z.min_width));
                    let _ = writeln!(out, "pads = {}", quoted(z.pads.word()));
                    let _ = writeln!(out, "thermal_gap = {}", num(z.thermal_gap));
                    let _ = writeln!(out, "spoke_width = {}", num(z.spoke_width));
                    let _ = writeln!(out, "islands = {}", quoted(z.islands.word()));
                    if let Some(a) = z.island_min_area {
                        let _ = writeln!(out, "island_min_area = {}", num(a));
                    }
                    let _ = writeln!(out, "outline = {}", ring(&z.outline));
                    if !z.holes.is_empty() {
                        out.push_str("holes = [\n");
                        for h in &z.holes {
                            let _ = writeln!(out, "  {},", ring(h));
                        }
                        out.push_str("]\n");
                    }
                    if z.lock {
                        out.push_str("lock = true\n");
                    }
                }
            }
        }
        out
    }
}

/// A ring of points on one line.
fn ring(points: &[[f64; 2]]) -> String {
    let p: Vec<String> = points.iter().map(|p| point(*p)).collect();
    format!("[{}]", p.join(", "))
}

impl ZonePads {
    /// The word the routes file writes.
    pub fn word(self) -> &'static str {
        match self {
            ZonePads::Solid => "solid",
            ZonePads::Thermal => "thermal",
            ZonePads::None => "none",
            ZonePads::ThruHoleOnly => "thru_hole_only",
        }
    }
}

impl ZoneIslands {
    /// The word the routes file writes.
    pub fn word(self) -> &'static str {
        match self {
            ZoneIslands::Remove => "remove",
            ZoneIslands::Keep => "keep",
            ZoneIslands::RemoveBelow => "remove_below",
        }
    }
}

/// A bare TOML key, or a quoted one.
fn key(name: &str) -> String {
    let bare = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare { name.to_string() } else { quoted(name) }
}

fn quoted(text: &str) -> String {
    toml::Value::String(text.to_string()).to_string()
}

/// A number in mm to 0.000001 mm, the precision of a KiCad file, written as a TOML float.
fn num(v: f64) -> String {
    let rounded = (v * 1e6).round() / 1e6;
    let mut text = format!("{rounded:.6}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.push('0');
    }
    if text == "-0.0" {
        text = "0.0".to_string();
    }
    text
}

fn point(p: [f64; 2]) -> String {
    format!("[{}, {}]", num(p[0]), num(p[1]))
}

/// The hash of a net's pads that a record keeps: for every pad of the net, its part's path in
/// the design, its number, its centre in mm from the outline's top left corner to 1 µm, and
/// whether it has copper on the top and on the bottom; sorted, and hashed with 64-bit FNV-1a
/// (16 hex digits). `None` for a net the board does not have.
pub fn pad_hash(board: &Board, net: &str) -> Option<String> {
    let net = board.nets.iter().find(|n| n.name == net)?;
    let mut lines: Vec<String> = Vec::new();
    for pad in &net.pads {
        let Some(part) = board.parts.iter().find(|p| p.refdes == pad.refdes) else {
            continue;
        };
        for p in part.footprint.pads_numbered(&pad.number) {
            let c = part.pad_centre(p) - board.outline.min;
            let (front, back) = part.pad_layers(p);
            lines.push(format!(
                "{}:{}@{:.3},{:.3}:{}{}",
                part.path,
                pad.number,
                c.x + 0.0,
                c.y + 0.0,
                u8::from(front),
                u8::from(back)
            ));
        }
    }
    lines.sort();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in lines.join("\n").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Some(format!("{hash:016x}"))
}

fn layer_of(face: Face) -> CopperLayer {
    match face {
        Face::Top => CopperLayer::Front,
        Face::Bottom => CopperLayer::Back,
    }
}

fn face_of(layer: CopperLayer) -> Face {
    match layer {
        CopperLayer::Front => Face::Top,
        CopperLayer::Back => Face::Bottom,
    }
}

impl RouteTrack {
    /// The track on `board`, of `net`.
    pub fn on(&self, board: &Board, net: &str) -> Track {
        let at = |p: [f64; 2]| board.outline.min + Pt::new(p[0], p[1]);
        Track {
            net: net.to_string(),
            layer: layer_of(self.layer),
            from: at(self.from),
            to: at(self.to),
            width: self.width,
            mid: self.mid.map(at),
        }
    }

    /// The record's form of a track of `board`.
    pub fn of(board: &Board, track: &Track) -> RouteTrack {
        let at = |p: Pt| {
            let q = p - board.outline.min;
            [q.x, q.y]
        };
        RouteTrack {
            layer: face_of(track.layer),
            width: track.width,
            from: at(track.from),
            mid: track.mid.map(at),
            to: at(track.to),
            lock: false,
        }
    }

    fn check(&self) -> Result<(), String> {
        let points = [Some(self.from), self.mid, Some(self.to)];
        if points.iter().flatten().flatten().any(|v| !v.is_finite()) {
            return Err("a track at a place that is not a number".into());
        }
        if !(self.width.is_finite() && self.width > 0.0) {
            return Err(format!("a track of width {}", self.width));
        }
        Ok(())
    }
}

impl RouteVia {
    /// The via on `board`, of `net`.
    pub fn on(&self, board: &Board, net: &str) -> Via {
        Via {
            net: net.to_string(),
            at: board.outline.min + Pt::new(self.at[0], self.at[1]),
            diameter: self.diameter,
            drill: self.drill,
        }
    }

    /// The record's form of a via of `board`.
    pub fn of(board: &Board, via: &Via) -> RouteVia {
        let q = via.at - board.outline.min;
        RouteVia {
            at: [q.x, q.y],
            diameter: via.diameter,
            drill: via.drill,
            lock: false,
        }
    }

    fn check(&self) -> Result<(), String> {
        if self.at.iter().any(|v| !v.is_finite()) {
            return Err("a via at a place that is not a number".into());
        }
        if !(self.drill.is_finite() && self.drill > 0.0 && self.diameter > self.drill) {
            return Err(format!(
                "a via of diameter {} and drill {}: the drill must be above 0 and below the diameter",
                self.diameter, self.drill
            ));
        }
        Ok(())
    }
}

/// KiCad's name of a routes file's layer (`top` is `F.Cu`, `inner2` is `In2.Cu`), or `None` for
/// a word that names no layer.
pub fn kicad_layer(word: &str) -> Option<String> {
    match word {
        "top" => Some("F.Cu".into()),
        "bottom" => Some("B.Cu".into()),
        _ => {
            let n: u32 = word.strip_prefix("inner")?.parse().ok()?;
            (n > 0).then(|| format!("In{n}.Cu"))
        }
    }
}

/// The routes file's word for a KiCad copper layer (`F.Cu` is `top`, `In2.Cu` is `inner2`).
pub fn layer_word(kicad: &str) -> Option<String> {
    match kicad {
        "F.Cu" => Some("top".into()),
        "B.Cu" => Some("bottom".into()),
        _ => {
            let n: u32 = kicad
                .strip_prefix("In")?
                .strip_suffix(".Cu")?
                .parse()
                .ok()?;
            (n > 0).then(|| format!("inner{n}"))
        }
    }
}

impl RouteZone {
    /// The zone on `board`, of `net`. The layers must have been checked ([`RouteZone::check`]).
    pub fn on(&self, board: &Board, net: &str) -> BoardZone {
        let at = |p: &[f64; 2]| board.outline.min + Pt::new(p[0], p[1]);
        BoardZone {
            net: net.to_string(),
            layers: self.layers.iter().filter_map(|l| kicad_layer(l)).collect(),
            outline: self.outline.iter().map(at).collect(),
            holes: self
                .holes
                .iter()
                .map(|h| h.iter().map(at).collect())
                .collect(),
            priority: self.priority,
            clearance: self.clearance,
            min_width: self.min_width,
            pads: self.pads,
            thermal_gap: self.thermal_gap,
            spoke_width: self.spoke_width,
            islands: self.islands,
            island_min_area: self.island_min_area,
        }
    }

    /// The record's form of a zone of `board`.
    pub fn of(board: &Board, zone: &BoardZone) -> RouteZone {
        let at = |p: &Pt| {
            let q = *p - board.outline.min;
            [q.x, q.y]
        };
        RouteZone {
            layers: zone.layers.iter().filter_map(|l| layer_word(l)).collect(),
            priority: zone.priority,
            clearance: zone.clearance,
            min_width: zone.min_width,
            pads: zone.pads,
            thermal_gap: zone.thermal_gap,
            spoke_width: zone.spoke_width,
            islands: zone.islands,
            island_min_area: zone.island_min_area,
            outline: zone.outline.iter().map(at).collect(),
            holes: zone
                .holes
                .iter()
                .map(|h| h.iter().map(at).collect())
                .collect(),
            lock: false,
        }
    }

    /// A zone of the design's own plane settings, on `layers` (the file's words, `top`,
    /// `bottom`, `inner1`...) with `outline` (the file's places): the clearance the process's
    /// planes keep, the minimum width and the thermal relief of KiCad's defaults, priority 0, a
    /// thermal relief at each pad, islands removed, no holes, not locked.
    pub fn new(board: &Board, layers: Vec<String>, outline: Vec<[f64; 2]>) -> RouteZone {
        RouteZone {
            layers,
            priority: 0,
            clearance: board.rules.plane_clearance,
            min_width: crate::export::ZONE_MIN_THICKNESS,
            pads: ZonePads::default(),
            thermal_gap: crate::export::THERMAL,
            spoke_width: crate::export::THERMAL,
            islands: ZoneIslands::default(),
            island_min_area: None,
            outline,
            holes: Vec::new(),
            lock: false,
        }
    }

    /// An error for a zone that is wrong in itself, or on a layer `board` does not have.
    pub fn check(&self, board: &Board) -> Result<(), String> {
        let mut have = vec!["F.Cu".to_string(), "B.Cu".to_string()];
        have.extend(board.inner_layers());
        if self.layers.is_empty() {
            return Err("a zone on no layer".into());
        }
        for l in &self.layers {
            match kicad_layer(l) {
                Some(k) if have.contains(&k) => {}
                Some(_) => return Err(format!("a zone on `{l}`, which the board does not have")),
                None => {
                    return Err(format!(
                        "a zone on `{l}`: a layer is `top`, `bottom` or `inner1`, `inner2`, ..."
                    ));
                }
            }
        }
        let finite = |r: &[[f64; 2]]| r.iter().flatten().all(|v| v.is_finite());
        if self.outline.len() < 3 || !finite(&self.outline) {
            return Err("a zone whose outline is not three or more places".into());
        }
        if self.holes.iter().any(|h| h.len() < 3 || !finite(h)) {
            return Err("a zone with a hole that is not three or more places".into());
        }
        let sizes = [
            ("clearance", self.clearance, false),
            ("min_width", self.min_width, true),
            ("thermal_gap", self.thermal_gap, false),
            ("spoke_width", self.spoke_width, true),
        ];
        for (name, v, positive) in sizes {
            if !v.is_finite() || v < 0.0 || (positive && v == 0.0) {
                return Err(format!("a zone with {name} {v}"));
            }
        }
        match (self.islands, self.island_min_area) {
            (ZoneIslands::RemoveBelow, Some(a)) if a.is_finite() && a >= 0.0 => Ok(()),
            (ZoneIslands::RemoveBelow, _) => Err(
                "a zone with `islands = \"remove_below\"` needs `island_min_area`, in mm²".into(),
            ),
            (_, Some(_)) => {
                Err("a zone with `island_min_area` but not `islands = \"remove_below\"`".into())
            }
            _ => Ok(()),
        }
    }
}

impl NetRoutes {
    /// How many shapes (tracks, vias and zones) the record has.
    pub fn shapes(&self) -> usize {
        self.tracks.len() + self.vias.len() + self.zones.len()
    }
}

/// Put the copper of `board`'s table in `routes` on the board, before it is routed (the
/// module's header): the shapes go first in [`Board::tracks`] and [`Board::vias`], counted in
/// [`Board::fixed`], the zones into [`Board::zones`], and the locked nets are named there. Returns what was not used as the file
/// says. A shape that is wrong in itself is an error naming its net.
pub fn apply_routes(
    board: &mut Board,
    routes: &Routes,
    file: &str,
) -> Result<Vec<RoutesNote>, RoutesError> {
    let mut notes = Vec::new();
    let Some(table) = routes.boards.get(&board.name) else {
        return Ok(notes);
    };
    let mut tracks = Vec::new();
    let mut vias = Vec::new();
    let mut zones = Vec::new();
    for (net, record) in &table.nets {
        for t in &record.tracks {
            t.check().map_err(|why| RoutesError::BadShape {
                file: file.to_string(),
                net: net.clone(),
                why,
            })?;
        }
        for v in &record.vias {
            v.check().map_err(|why| RoutesError::BadShape {
                file: file.to_string(),
                net: net.clone(),
                why,
            })?;
        }
        for z in &record.zones {
            z.check(board).map_err(|why| RoutesError::BadShape {
                file: file.to_string(),
                net: net.clone(),
                why,
            })?;
        }
        let Some(hash) = pad_hash(board, net) else {
            notes.push(RoutesNote::NotANet {
                net: net.clone(),
                shapes: record.shapes(),
            });
            continue;
        };
        let current = hash == record.pads;
        let keep_track = |t: &&RouteTrack| current || t.lock;
        let keep_via = |v: &&RouteVia| current || v.lock;
        let keep_zone = |z: &&RouteZone| current || z.lock;
        let kept = record.tracks.iter().filter(keep_track).count()
            + record.vias.iter().filter(keep_via).count()
            + record.zones.iter().filter(keep_zone).count();
        if !current {
            notes.push(RoutesNote::Moved {
                net: net.clone(),
                dropped: record.shapes() - kept,
                kept,
            });
        }
        tracks.extend(
            record
                .tracks
                .iter()
                .filter(keep_track)
                .map(|t| t.on(board, net)),
        );
        vias.extend(
            record
                .vias
                .iter()
                .filter(keep_via)
                .map(|v| v.on(board, net)),
        );
        zones.extend(
            record
                .zones
                .iter()
                .filter(keep_zone)
                .map(|z| z.on(board, net)),
        );
        if current && record.lock {
            board.fixed.locked.insert(net.clone());
        }
    }
    board.fixed.tracks += tracks.len();
    board.fixed.vias += vias.len();
    board.tracks.splice(0..0, tracks);
    board.vias.splice(0..0, vias);
    board.zones.extend(zones);
    Ok(notes)
}

/// The record of `net` as `board` has it now: every track, via and zone of the net on the board,
/// none of them locked, with the net's pads as they are; `None` for a net the board does not
/// have.
pub fn record_of(board: &Board, net: &str, lock: bool) -> Option<NetRoutes> {
    Some(NetRoutes {
        pads: pad_hash(board, net)?,
        lock,
        tracks: board
            .tracks
            .iter()
            .filter(|t| t.net == net)
            .map(|t| RouteTrack::of(board, t))
            .collect(),
        vias: board
            .vias
            .iter()
            .filter(|v| v.net == net)
            .map(|v| RouteVia::of(board, v))
            .collect(),
        zones: board
            .zones
            .iter()
            .filter(|z| z.net == net)
            .map(|z| RouteZone::of(board, z))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::tests::led_board;

    fn sample() -> Routes {
        let mut nets = BTreeMap::new();
        nets.insert(
            "mcu.usb.dp".to_string(),
            NetRoutes {
                pads: "0011223344556677".into(),
                lock: true,
                tracks: vec![
                    RouteTrack {
                        layer: Face::Top,
                        width: 0.2,
                        from: [1.0, 2.5],
                        mid: None,
                        to: [3.125, 2.5],
                        lock: false,
                    },
                    RouteTrack {
                        layer: Face::Bottom,
                        width: 0.15,
                        from: [3.125, 2.5],
                        mid: Some([3.5, 2.6]),
                        to: [3.8, 2.9],
                        lock: true,
                    },
                ],
                vias: vec![RouteVia {
                    at: [3.125, 2.5],
                    diameter: 0.6,
                    drill: 0.3,
                    lock: false,
                }],
                zones: vec![zone()],
            },
        );
        nets.insert("GND".to_string(), NetRoutes::default());
        let mut boards = BTreeMap::new();
        boards.insert("Led".to_string(), BoardRoutes { nets });
        Routes { boards }
    }

    fn zone() -> RouteZone {
        RouteZone {
            layers: vec!["top".into(), "bottom".into()],
            priority: 2,
            clearance: 0.2,
            min_width: 0.25,
            pads: ZonePads::Solid,
            thermal_gap: 0.5,
            spoke_width: 0.508,
            islands: ZoneIslands::RemoveBelow,
            island_min_area: Some(30.0),
            outline: vec![[0.5, 0.5], [8.0, 0.5], [8.0, 4.0], [0.5, 4.0]],
            holes: vec![vec![[1.0, 1.0], [2.0, 1.0], [2.0, 2.0]]],
            lock: false,
        }
    }

    #[test]
    fn a_file_reads_back_as_it_was_written_one_shape_a_line() {
        let routes = sample();
        let text = routes.to_text();
        assert!(text.starts_with("version = 1\n"), "{text}");
        assert!(text.contains("[Led.nets.\"mcu.usb.dp\"]\n"), "{text}");
        assert!(text.contains(
            "  { layer = \"bottom\", width = 0.15, from = [3.125, 2.5], mid = [3.5, 2.6], to = [3.8, 2.9], lock = true },\n"
        ));
        assert!(text.contains("  { at = [3.125, 2.5], diameter = 0.6, drill = 0.3 },\n"));
        assert!(
            text.contains(
                "\n[[Led.nets.\"mcu.usb.dp\".zones]]\nlayers = [\"top\", \"bottom\"]\npriority = 2\n\
                 clearance = 0.2\nmin_width = 0.25\npads = \"solid\"\nthermal_gap = 0.5\n\
                 spoke_width = 0.508\nislands = \"remove_below\"\nisland_min_area = 30.0\n\
                 outline = [[0.5, 0.5], [8.0, 0.5], [8.0, 4.0], [0.5, 4.0]]\n\
                 holes = [\n  [[1.0, 1.0], [2.0, 1.0], [2.0, 2.0]],\n]\n"
            ),
            "{text}"
        );
        assert_eq!(Routes::parse(&text, "f").unwrap(), routes);
        // What is not given takes KiCad's defaults: priority 0, thermal reliefs, islands
        // removed.
        let short = "version = 1\n[[Led.nets.GND.zones]]\nlayers = [\"top\"]\nclearance = 0.2\n\
                     min_width = 0.2\nthermal_gap = 0.3\nspoke_width = 0.3\n\
                     outline = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]\n";
        let short = Routes::parse(short, "f");
        let e = short.unwrap_err().to_string();
        assert!(e.contains("pads"), "a record needs its pad hash: {e}");
        let ok = "version = 1\n[Led.nets.GND]\npads = \"x\"\n[[Led.nets.GND.zones]]\n\
                  layers = [\"top\"]\nclearance = 0.2\nmin_width = 0.2\nthermal_gap = 0.3\n\
                  spoke_width = 0.3\noutline = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]\n";
        let z = &Routes::parse(ok, "f").unwrap().boards["Led"].nets["GND"].zones[0];
        assert_eq!(
            (z.priority, z.pads, z.islands, z.lock),
            (0, ZonePads::Thermal, ZoneIslands::Remove, false)
        );
    }

    #[test]
    fn a_zone_is_on_the_board_on_kicads_layers_and_reads_back_from_it() {
        let mut board = led_board();
        let net = board.nets[0].name.clone();
        let mut z = zone();
        z.layers = vec!["top".into()];
        let record = NetRoutes {
            pads: pad_hash(&board, &net).unwrap(),
            zones: vec![z.clone()],
            ..NetRoutes::default()
        };
        let mut routes = Routes::default();
        routes
            .boards
            .entry(board.name.clone())
            .or_default()
            .nets
            .insert(net.clone(), record);
        apply_routes(&mut board, &routes, "f").unwrap();
        assert_eq!(board.zones.len(), 1);
        assert_eq!(board.zones[0].layers, ["F.Cu"]);
        assert_eq!(
            board.zones[0].outline[0],
            board.outline.min + Pt::new(0.5, 0.5)
        );
        assert!(board.zoned("F.Cu") && !board.zoned("B.Cu"));
        assert_eq!(record_of(&board, &net, false).unwrap().zones, [z]);
    }

    #[test]
    fn a_zone_that_is_wrong_in_itself_or_on_a_layer_the_board_lacks_is_an_error() {
        type Spoil = fn(&mut RouteZone);
        let cases: [(Spoil, &str); 5] = [
            (|z| z.layers = vec!["inner1".into()], "inner1"),
            (|z| z.layers = vec!["middle".into()], "middle"),
            (|z| z.outline.truncate(2), "outline"),
            (|z| z.spoke_width = 0.0, "spoke_width"),
            (|z| z.island_min_area = None, "island_min_area"),
        ];
        for (spoil, said) in cases {
            let mut board = led_board();
            let net = board.nets[0].name.clone();
            let mut z = zone();
            spoil(&mut z);
            let mut routes = Routes::default();
            routes
                .boards
                .entry(board.name.clone())
                .or_default()
                .nets
                .insert(
                    net.clone(),
                    NetRoutes {
                        pads: pad_hash(&board, &net).unwrap(),
                        zones: vec![z],
                        ..NetRoutes::default()
                    },
                );
            let e = apply_routes(&mut board, &routes, "f")
                .unwrap_err()
                .to_string();
            assert!(e.contains(said) && e.contains(&net), "{e}");
        }
    }

    #[test]
    fn the_layer_words_are_kicads_layers() {
        for (word, kicad) in [("top", "F.Cu"), ("bottom", "B.Cu"), ("inner3", "In3.Cu")] {
            assert_eq!(kicad_layer(word).as_deref(), Some(kicad));
            assert_eq!(layer_word(kicad).as_deref(), Some(word));
        }
        assert_eq!(kicad_layer("inner0"), None);
        assert_eq!(layer_word("Edge.Cuts"), None);
    }

    #[test]
    fn an_unknown_key_or_version_is_an_error() {
        let bad = "version = 1\n[Led.nets.GND]\npads = \"x\"\ncolour = 1\n";
        let e = Routes::parse(bad, "f").unwrap_err().to_string();
        assert!(e.contains("colour"), "{e}");
        let e = Routes::parse("version = 2\n", "f").unwrap_err().to_string();
        assert!(e.contains("version 2"), "{e}");
        let e = Routes::parse("", "f").unwrap_err().to_string();
        assert!(e.contains("no `version"), "{e}");
    }

    #[test]
    fn numbers_keep_a_kicad_files_precision() {
        assert_eq!(num(1.0), "1.0");
        assert_eq!(num(39.891942), "39.891942");
        assert_eq!(num(0.1 + 0.2), "0.3");
        assert_eq!(num(-1e-9), "0.0");
    }

    #[test]
    fn copper_of_a_current_record_is_fixed_and_a_moved_one_keeps_only_its_locked_shapes() {
        let mut board = led_board();
        let net = board.nets[0].name.clone();
        let record = NetRoutes {
            pads: pad_hash(&board, &net).unwrap(),
            lock: true,
            tracks: vec![RouteTrack {
                layer: Face::Top,
                width: 0.25,
                from: [1.0, 1.0],
                mid: None,
                to: [2.0, 1.0],
                lock: false,
            }],
            vias: vec![RouteVia {
                at: [2.0, 1.0],
                diameter: 0.6,
                drill: 0.3,
                lock: true,
            }],
            zones: Vec::new(),
        };
        let mut routes = Routes::default();
        routes
            .boards
            .entry(board.name.clone())
            .or_default()
            .nets
            .insert(net.clone(), record.clone());
        let notes = apply_routes(&mut board, &routes, "f").unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!((board.fixed.tracks, board.fixed.vias), (1, 1));
        assert!(board.fixed.is_locked(&net));
        assert_eq!(board.tracks[0].from, board.outline.min + Pt::new(1.0, 1.0));
        assert_eq!(record_of(&board, &net, true).unwrap().tracks, record.tracks);

        // A pad moved: the unlocked track goes, the locked via stays, and the net is not locked.
        let mut board = led_board();
        let refdes = board.nets[0].pads[0].refdes.clone();
        board
            .parts
            .iter_mut()
            .find(|p| p.refdes == refdes)
            .unwrap()
            .at
            .x += 0.5;
        let notes = apply_routes(&mut board, &routes, "f").unwrap();
        assert_eq!(
            notes,
            [RoutesNote::Moved {
                net: net.clone(),
                dropped: 1,
                kept: 1
            }]
        );
        assert_eq!((board.tracks.len(), board.vias.len()), (0, 1));
        assert!(!board.fixed.is_locked(&net));

        // A net the board does not have is said, and nothing of it drawn.
        let mut routes = Routes::default();
        routes
            .boards
            .entry(board.name.clone())
            .or_default()
            .nets
            .insert("gone".into(), record);
        let mut board = led_board();
        let notes = apply_routes(&mut board, &routes, "f").unwrap();
        assert_eq!(
            notes,
            [RoutesNote::NotANet {
                net: "gone".into(),
                shapes: 2
            }]
        );
        assert!(board.fixed.is_empty() && board.tracks.is_empty());
    }

    #[test]
    fn a_shape_that_is_wrong_in_itself_is_an_error_naming_its_net() {
        let mut board = led_board();
        let mut routes = sample();
        let mut table = routes.boards.remove("Led").unwrap();
        table.nets.get_mut("mcu.usb.dp").unwrap().vias[0].drill = 0.7;
        routes.boards.insert(board.name.clone(), table);
        let e = apply_routes(&mut board, &routes, "f")
            .unwrap_err()
            .to_string();
        assert!(e.contains("mcu.usb.dp") && e.contains("drill"), "{e}");
    }
}
