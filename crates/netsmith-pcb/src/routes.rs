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
//! ```
//!
//! A track is straight from `from` to `to`, or an arc through `mid` (KiCad's `(arc (start)
//! (mid) (end))`); `layer` is `top` or `bottom` (tracks are drawn on the outer layers only, as
//! the router's are); `width`, `diameter` and `drill` are in mm. An unknown key is an error.
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

use crate::board::{Board, CopperLayer, Track, Via};
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
            }
        }
        out
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

impl NetRoutes {
    /// How many shapes the record has.
    pub fn shapes(&self) -> usize {
        self.tracks.len() + self.vias.len()
    }
}

/// Put the copper of `board`'s table in `routes` on the board, before it is routed (the
/// module's header): the shapes go first in [`Board::tracks`] and [`Board::vias`], counted in
/// [`Board::fixed`], and the locked nets are named there. Returns what was not used as the file
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
        let kept = record.tracks.iter().filter(keep_track).count()
            + record.vias.iter().filter(keep_via).count();
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
        if current && record.lock {
            board.fixed.locked.insert(net.clone());
        }
    }
    board.fixed.tracks += tracks.len();
    board.fixed.vias += vias.len();
    board.tracks.splice(0..0, tracks);
    board.vias.splice(0..0, vias);
    Ok(notes)
}

/// The record of `net` as `board` has it now: every track and via of the net on the board,
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
            },
        );
        nets.insert("GND".to_string(), NetRoutes::default());
        let mut boards = BTreeMap::new();
        boards.insert("Led".to_string(), BoardRoutes { nets });
        Routes { boards }
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
        assert_eq!(Routes::parse(&text, "f").unwrap(), routes);
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
