//! What a board looks like, read from its `.kicad_pcb`: the pads, vias, tracks, zones,
//! drawings and text on every layer, all in board coordinates, ready for the Gerber and
//! drill writers. Nothing here depends on KiCad or on the other netsmith crates; the file
//! is the whole truth about the board.
//!
//! A footprint on the back of the board is stored by KiCad already mirrored and on the
//! other face's layers, so a point of a footprint is `at + turn(local)` whichever face it
//! is on. Angles of pads and of text are absolute (the footprint's turn is in them).
//!
//! It is the only reader of the board for plotting (ADR 0034), so a board ported from KiCad plots
//! as well as one netsmith wrote. It reads every pad shape KiCad has, with the footprint's turn,
//! drill, margins, zone connection and clearance, vias, tracks and arcs, zones with their rule
//! areas and teardrops, drawings and text, the origin and the mask settings. What it cannot make it refuses
//! with the pad, via or zone named: a pad with a `padstack`, a blind or buried via, a zone filled
//! in hatch mode, a custom pad primitive other than line, arc, circle, rectangle or polygon.

use crate::geom::{Arc, Pt};
use crate::sexp::{self, Sexp};

#[derive(Debug, PartialEq)]
pub enum ArtworkError {
    Parse(String),
    NotABoard,
    /// Something the file says that the writers cannot make; names what and where.
    Unsupported(String),
    /// A number or list the file should hold and does not.
    Malformed(String),
}

impl std::fmt::Display for ArtworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(m) => write!(f, "cannot read the .kicad_pcb file: {m}"),
            Self::NotABoard => write!(
                f,
                "the file is not a KiCad board: it has no `(kicad_pcb` list"
            ),
            Self::Unsupported(m) => write!(f, "{m}"),
            Self::Malformed(m) => write!(f, "the board file is malformed: {m}"),
        }
    }
}

impl std::error::Error for ArtworkError {}

/// The shape of a pad as the file names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadShape {
    Circle,
    Rect,
    Oval,
    RoundRect,
    Trapezoid,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadKind {
    Smd,
    /// A plated hole with a pad round it.
    Thru,
    /// A hole with no plating.
    Npth,
    /// An edge connector pad (no hole, no mask by default, copper only).
    Connect,
}

/// How a zone or a pad says the pad joins a zone of its net.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connect {
    /// Thermal relief: spokes through a gap.
    Thermal,
    /// Solid copper.
    Solid,
    /// No connection: the pad keeps the clearance like any other net.
    None,
    /// Thermal relief for through-hole pads, solid for surface-mount.
    ThruHoleOnly,
}

/// One drawing primitive of a custom pad, in the pad's own frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    Line {
        a: Pt,
        b: Pt,
        width: f64,
    },
    Arc {
        arc: Arc,
        width: f64,
    },
    Circle {
        centre: Pt,
        radius: f64,
        width: f64,
        fill: bool,
    },
    Poly {
        points: Vec<Pt>,
        width: f64,
        fill: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pad {
    /// The footprint's designator.
    pub part: String,
    pub number: String,
    /// What the pad's function is called in the footprint (`pinfunction`), if it is.
    pub function: Option<String>,
    pub net: Option<String>,
    pub kind: PadKind,
    pub shape: PadShape,
    /// Centre on the board.
    pub at: Pt,
    /// Turn on the board in degrees (the footprint's turn is in it).
    pub angle: f64,
    /// Width and height in the pad's own frame, before the turn.
    pub size: (f64, f64),
    /// Corner radius over the shorter side, for a rounded rectangle.
    pub roundrect_ratio: f64,
    /// A trapezoid's `rect_delta`: how far the ends of each side are moved.
    pub rect_delta: (f64, f64),
    /// Chamfered corners of a rectangle or rounded rectangle: the ratio of the shorter side
    /// and the corners (`top_left`, `top_right`, `bottom_left`, `bottom_right`).
    pub chamfer_ratio: f64,
    pub chamfer: [bool; 4],
    pub primitives: Vec<Primitive>,
    /// The anchor of a custom pad: `Circle` or `Rect`.
    pub anchor_rect: bool,
    pub drill: Option<Drill>,
    /// Layers the pad is on, expanded (`*.Cu` is every copper layer of the board).
    pub layers: Vec<String>,
    /// The mask margin that holds for the pad: its own `solder_mask_margin`, else its
    /// footprint's.
    pub mask_margin: Option<f64>,
    /// Whether `mask_margin` is the pad's own (not its footprint's), and the footprint's
    /// margin, which the pad falls back to. The cross-check needs the difference (ADR 0034).
    pub own_mask_margin: bool,
    pub footprint_mask_margin: Option<f64>,
    pub paste_margin: Option<f64>,
    pub paste_ratio: Option<f64>,
    pub zone_connect: Option<Connect>,
    /// `thermal_bridge_angle` of the pad: the turn of its thermal spokes, in degrees, from the
    /// pad's own axes. `None` when the file says nothing, which KiCad reads as 45 for a circle
    /// and 90 (along the axes) for any other shape ([`Pad::spoke_angle`]).
    pub thermal_angle: Option<f64>,
    pub remove_unused_layers: bool,
    /// The pad's own clearance, which a zone keeps if it is larger than its own.
    pub clearance: Option<f64>,
}

impl Pad {
    /// The turn of the pad's thermal spokes from its own axes, in degrees, as KiCad has it:
    /// the pad's `thermal_bridge_angle`, else 45 for a circle (and for a custom pad with a
    /// circular anchor) and 90 for every other shape, so that the spokes of a round pad run
    /// diagonally and those of any other pad along its axes (KiCad 9, `parsePAD`).
    pub fn spoke_angle(&self) -> f64 {
        self.thermal_angle.unwrap_or(match self.shape {
            PadShape::Circle => 45.0,
            PadShape::Custom if !self.anchor_rect => 45.0,
            _ => 90.0,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drill {
    /// Diameter of a round hole, or width and length of a slot.
    pub width: f64,
    pub length: f64,
    /// Offset of the hole from the pad centre in the pad's frame.
    pub offset: Pt,
}

impl Drill {
    pub fn is_slot(&self) -> bool {
        (self.length - self.width).abs() > 1e-9
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Via {
    pub at: Pt,
    pub diameter: f64,
    pub drill: f64,
    pub net: Option<String>,
    /// Whether solder mask covers the via on the front and on the back.
    pub tented: (bool, bool),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TrackShape {
    Line(Pt, Pt),
    Arc(Arc),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub shape: TrackShape,
    pub width: f64,
    pub layer: String,
    pub net: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Keepout {
    pub tracks: bool,
    pub vias: bool,
    pub pads: bool,
    pub copperpour: bool,
    pub footprints: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Zone {
    pub net: Option<String>,
    pub layers: Vec<String>,
    /// The outline of each `(polygon` of the zone.
    pub outlines: Vec<Vec<Pt>>,
    /// The fill the file already holds: `(layer, polygon)`.
    pub filled: Vec<(String, Vec<Pt>)>,
    /// A rule area, not a pour.
    pub keepout: Option<Keepout>,
    /// The designator of the footprint that carries it, empty for a zone of the board.
    pub owner: String,
    pub clearance: f64,
    pub min_thickness: f64,
    pub connect: Connect,
    pub thermal_gap: f64,
    pub thermal_bridge: f64,
    pub name: String,
    /// A teardrop (`(attr (teardrop ...))`, KiCad 9's own mark): copper that widens a track
    /// where it meets a pad or a via. Its fill is the copper; the check sees it as copper of its
    /// net, and pours of other nets keep their clearance from it.
    pub teardrop: bool,
}

/// A line, arc, circle or polygon drawn on a layer.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Line(Pt, Pt),
    Arc(Arc),
    Circle { centre: Pt, radius: f64 },
    Poly(Vec<Pt>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Graphic {
    pub layer: String,
    /// The designator of the footprint that draws it, empty for a drawing of the board.
    pub owner: String,
    pub shape: Shape,
    pub width: f64,
    pub fill: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Centre,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Centre,
    Bottom,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub layer: String,
    /// With the variables of its footprint (`${REFERENCE}`) already put in.
    pub text: String,
    pub at: Pt,
    /// Absolute turn in degrees.
    pub angle: f64,
    pub size: (f64, f64),
    pub thickness: f64,
    pub mirror: bool,
    pub italic: bool,
    pub h: HAlign,
    pub v: VAlign,
    /// Where it was found, for messages: `R1` or `the board`.
    pub owner: String,
}

/// Where a footprint sits.
#[derive(Debug, Clone, PartialEq)]
pub struct Footprint {
    pub part: String,
    pub at: Pt,
    pub angle: f64,
    /// On the back of the board.
    pub back: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Artwork {
    /// Copper layers from the front to the back, in the file's stack order.
    pub copper: Vec<String>,
    /// The drill/place origin if the board sets one, else (0, 0).
    pub origin: Pt,
    pub mask_margin: f64,
    pub paste_margin: f64,
    pub paste_ratio: f64,
    pub tent_front: bool,
    pub tent_back: bool,
    pub pads: Vec<Pad>,
    pub vias: Vec<Via>,
    pub tracks: Vec<Track>,
    pub zones: Vec<Zone>,
    pub graphics: Vec<Graphic>,
    pub texts: Vec<Text>,
    pub footprints: Vec<Footprint>,
    /// Things in the file the writers leave out, said in the notes.
    pub skipped: Vec<String>,
}

impl Artwork {
    pub fn parse(src: &str) -> Result<Artwork, ArtworkError> {
        let root = sexp::parse(src).map_err(|e| ArtworkError::Parse(e.to_string()))?;
        Self::from_sexp(&root)
    }

    /// [`Artwork::parse`] of a board file already read into its tree, or built as one in
    /// memory (`netsmith_pcb::copper` does, with no file and no text in between).
    pub fn from_sexp(root: &Sexp) -> Result<Artwork, ArtworkError> {
        if root.head() != Some("kicad_pcb") {
            return Err(ArtworkError::NotABoard);
        }
        Reader::new(root)?.read()
    }

    /// Whether `pad` has copper on `layer`: it is on the layer, unless it is a hole with no
    /// copper round it (a mounting hole whose pad is no larger than its drill), or it
    /// removes its unused inner layers and nothing of its net reaches it on this one
    /// (a track ending on the pad or a zone of the net over its centre).
    pub fn pad_copper_on(&self, pad: &Pad, layer: &str) -> bool {
        if !pad.layers.iter().any(|l| l == layer) {
            return false;
        }
        if pad.size.0 <= 0.0 || pad.size.1 <= 0.0 {
            return false;
        }
        if pad.kind == PadKind::Npth
            && let Some(d) = &pad.drill
            && d.width >= pad.size.0 - 1e-9
            && d.length >= pad.size.1 - 1e-9
        {
            return false;
        }
        let inner = self.copper.first().map(String::as_str) != Some(layer)
            && self.copper.last().map(String::as_str) != Some(layer);
        if pad.remove_unused_layers && inner && pad.kind != PadKind::Npth {
            let reach = pad.size.0.max(pad.size.1) / 2.0;
            let by_track = self.tracks.iter().any(|t| {
                t.layer == layer
                    && t.net == pad.net
                    && match &t.shape {
                        TrackShape::Line(a, b) => {
                            a.dist(pad.at) <= reach || b.dist(pad.at) <= reach
                        }
                        TrackShape::Arc(a) => {
                            a.start.dist(pad.at) <= reach || a.end.dist(pad.at) <= reach
                        }
                    }
            });
            let by_zone = self.zones.iter().any(|z| {
                z.keepout.is_none()
                    && z.net == pad.net
                    && z.layers.iter().any(|l| l == layer)
                    && z.outlines.iter().any(|o| crate::geom::contains(o, pad.at))
            });
            return by_track || by_zone;
        }
        true
    }
}

fn num(s: Option<&Sexp>, what: &str) -> Result<f64, ArtworkError> {
    s.and_then(Sexp::text)
        .and_then(|t| t.parse().ok())
        .ok_or_else(|| ArtworkError::Malformed(format!("{what} is not a number")))
}

/// `(head x y)` as a point.
fn point(node: &Sexp, what: &str) -> Result<Pt, ArtworkError> {
    Ok(Pt::new(
        num(node.items().get(1), what)?,
        num(node.items().get(2), what)?,
    ))
}

fn child_point(node: &Sexp, name: &str, what: &str) -> Result<Option<Pt>, ArtworkError> {
    node.child(name).map(|c| point(c, what)).transpose()
}

fn child_num(node: &Sexp, name: &str) -> Option<f64> {
    node.child(name)?.items().get(1)?.text()?.parse().ok()
}

/// A flag written `(name yes)`, `(name no)` or just `(name)`.
fn flag(node: &Sexp, name: &str) -> Option<bool> {
    let c = node.child(name)?;
    Some(!matches!(c.items().get(1).and_then(Sexp::text), Some("no")))
}

/// The stroke width of a drawing: `(stroke (width w))` or the older `(width w)`.
fn stroke_width(node: &Sexp) -> f64 {
    node.child("stroke")
        .and_then(|s| child_num(s, "width"))
        .or_else(|| child_num(node, "width"))
        .unwrap_or(0.0)
}

/// Whether a drawing is filled: `(fill yes)`, `(fill solid)`, or the older `(fill solid)`.
fn is_filled(node: &Sexp) -> bool {
    matches!(
        node.child("fill")
            .and_then(|f| f.items().get(1))
            .and_then(Sexp::text),
        Some("yes" | "solid")
    )
}

/// `(zone_connect n)` of a pad or a footprint: 0 none, 1 thermal, 2 solid, 3 thermal for
/// through-hole pads only.
fn zone_connect_of(node: &Sexp) -> Option<Connect> {
    match child_num(node, "zone_connect") {
        Some(0.0) => Some(Connect::None),
        Some(1.0) => Some(Connect::Thermal),
        Some(2.0) => Some(Connect::Solid),
        Some(3.0) => Some(Connect::ThruHoleOnly),
        _ => None,
    }
}

fn layer_of(node: &Sexp) -> String {
    node.child("layer")
        .and_then(|l| l.items().get(1)?.text())
        .unwrap_or("")
        .to_string()
}

fn points_of(node: &Sexp, what: &str) -> Result<Vec<Pt>, ArtworkError> {
    let Some(pts) = node.child("pts") else {
        return Err(ArtworkError::Malformed(format!("{what} has no (pts")));
    };
    pts.children("xy").map(|xy| point(xy, what)).collect()
}

struct Reader<'a> {
    root: &'a Sexp,
    copper: Vec<String>,
    nets: std::collections::BTreeMap<i64, String>,
    out: Artwork,
}

/// A footprint's place on the board.
struct Frame {
    at: Pt,
    angle: f64,
    part: String,
    value: String,
    footprint: String,
    /// What the footprint says for all its pads that a pad does not say itself.
    mask_margin: Option<f64>,
    paste_margin: Option<f64>,
    paste_ratio: Option<f64>,
    clearance: Option<f64>,
    zone_connect: Option<Connect>,
}

impl Frame {
    fn world(&self, local: Pt) -> Pt {
        self.at + local.rotate(self.angle)
    }
}

impl<'a> Reader<'a> {
    fn new(root: &'a Sexp) -> Result<Self, ArtworkError> {
        let copper: Vec<String> = root
            .child("layers")
            .map(|l| {
                l.items()
                    .iter()
                    .filter_map(|e| e.items().get(1)?.text())
                    .filter(|n| n.ends_with(".Cu"))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let mut nets = std::collections::BTreeMap::new();
        for n in root.children("net") {
            if let (Some(i), Some(name)) = (
                n.items()
                    .get(1)
                    .and_then(Sexp::text)
                    .and_then(|t| t.parse().ok()),
                n.items().get(2).and_then(Sexp::text),
            ) {
                nets.insert(i, name.to_string());
            }
        }
        let setup = root.child("setup");
        let origin = setup
            .and_then(|s| s.child("aux_axis_origin"))
            .map(|o| point(o, "aux_axis_origin"))
            .transpose()?
            .unwrap_or_default();
        let tenting: Vec<&str> = setup
            .and_then(|s| s.child("tenting"))
            .map(|t| t.items().iter().skip(1).filter_map(Sexp::text).collect())
            .unwrap_or_default();
        let out = Artwork {
            copper: copper.clone(),
            origin,
            mask_margin: setup
                .and_then(|s| child_num(s, "pad_to_mask_clearance"))
                .unwrap_or(0.0),
            paste_margin: setup
                .and_then(|s| child_num(s, "pad_to_paste_clearance"))
                .unwrap_or(0.0),
            paste_ratio: setup
                .and_then(|s| child_num(s, "pad_to_paste_clearance_ratio"))
                .unwrap_or(0.0),
            tent_front: tenting.contains(&"front"),
            tent_back: tenting.contains(&"back"),
            pads: Vec::new(),
            vias: Vec::new(),
            tracks: Vec::new(),
            zones: Vec::new(),
            graphics: Vec::new(),
            texts: Vec::new(),
            footprints: Vec::new(),
            skipped: Vec::new(),
        };
        Ok(Self {
            root,
            copper,
            nets,
            out,
        })
    }

    fn net_name(&self, node: &Sexp) -> Option<String> {
        let n = node.child("net")?;
        if let Some(name) = n.items().get(2).and_then(Sexp::text) {
            return Some(name).filter(|n| !n.is_empty()).map(str::to_string);
        }
        // KiCad 10 writes the name alone, `(net "GND")`, and has no table of numbered nets;
        // KiCad 9 and older write the number alone on tracks and vias, `(net 3)`.
        let first = n.items().get(1)?.text()?;
        match first.parse::<i64>().ok().and_then(|i| self.nets.get(&i)) {
            Some(name) => Some(name).filter(|n| !n.is_empty()).cloned(),
            None if self.nets.is_empty() => {
                Some(first).filter(|n| !n.is_empty()).map(str::to_string)
            }
            None => None,
        }
    }

    fn read(mut self) -> Result<Artwork, ArtworkError> {
        let root = self.root;
        for item in root.items().iter().skip(1) {
            match item.head() {
                Some("footprint") => self.footprint(item)?,
                Some("segment") => {
                    let track = Track {
                        shape: TrackShape::Line(
                            child_point(item, "start", "a segment's start")?.unwrap_or_default(),
                            child_point(item, "end", "a segment's end")?.unwrap_or_default(),
                        ),
                        width: child_num(item, "width").unwrap_or(0.0),
                        layer: layer_of(item),
                        net: self.net_name(item),
                    };
                    self.out.tracks.push(track);
                }
                Some("arc") => {
                    let at = |name: &str| child_point(item, name, "a track arc");
                    let (s, m, e) = (
                        at("start")?.unwrap_or_default(),
                        at("mid")?.unwrap_or_default(),
                        at("end")?.unwrap_or_default(),
                    );
                    let shape = match Arc::through(s, m, e) {
                        Some(arc) => TrackShape::Arc(arc),
                        None => TrackShape::Line(s, e),
                    };
                    let track = Track {
                        shape,
                        width: child_num(item, "width").unwrap_or(0.0),
                        layer: layer_of(item),
                        net: self.net_name(item),
                    };
                    self.out.tracks.push(track);
                }
                Some("via") => self.via(item)?,
                Some("zone") => self.zone(item)?,
                Some(h) if h.starts_with("gr_") => self.drawing(item, None)?,
                _ => {}
            }
        }
        Ok(self.out)
    }

    fn via(&mut self, item: &Sexp) -> Result<(), ArtworkError> {
        let at = child_point(item, "at", "a via")?.unwrap_or_default();
        let diameter = child_num(item, "size").unwrap_or(0.0);
        let layers: Vec<&str> = item
            .child("layers")
            .map(|l| l.items().iter().skip(1).filter_map(Sexp::text).collect())
            .unwrap_or_default();
        let through = self.copper.first().map(String::as_str) == layers.first().copied()
            && self.copper.last().map(String::as_str) == layers.last().copied();
        let kind = item.items().get(1).and_then(Sexp::text);
        if !through || matches!(kind, Some("blind" | "micro")) {
            return Err(ArtworkError::Unsupported(format!(
                "the via at ({}, {}) is not a through via (it runs {}): blind, buried and micro vias are not supported",
                at.x,
                at.y,
                layers.join(" to ")
            )));
        }
        // A via's own `(tenting front back)` (or `none`) replaces the board's.
        let tent = |name: &str, board: bool| match item.child("tenting") {
            Some(t) => t.items().iter().skip(1).any(|a| a.text() == Some(name)),
            None => board,
        };
        let via = Via {
            at,
            diameter,
            drill: child_num(item, "drill").unwrap_or(0.0),
            net: self.net_name(item),
            tented: (
                tent("front", self.out.tent_front),
                tent("back", self.out.tent_back),
            ),
        };
        self.out.vias.push(via);
        Ok(())
    }

    fn zone(&mut self, item: &Sexp) -> Result<(), ArtworkError> {
        let layers: Vec<String> = match item.child("layers") {
            Some(l) => l
                .items()
                .iter()
                .skip(1)
                .filter_map(Sexp::text)
                .map(str::to_string)
                .collect(),
            None => vec![layer_of(item)],
        };
        let keepout = item.child("keepout").map(|k| {
            let not_allowed = |name: &str| {
                k.child(name).and_then(|c| c.items().get(1)?.text()) == Some("not_allowed")
            };
            Keepout {
                tracks: not_allowed("tracks"),
                vias: not_allowed("vias"),
                pads: not_allowed("pads"),
                copperpour: not_allowed("copperpour"),
                footprints: not_allowed("footprints"),
            }
        });
        let name = item
            .child("name")
            .and_then(|n| n.items().get(1)?.text())
            .unwrap_or("")
            .to_string();
        let connect_pads = item.child("connect_pads");
        let connect = match connect_pads
            .and_then(|c| c.items().get(1))
            .and_then(Sexp::text)
        {
            None => Connect::Thermal,
            Some("yes") => Connect::Solid,
            Some("no") => Connect::None,
            Some("thru_hole_only") => Connect::ThruHoleOnly,
            Some(_) => Connect::Thermal,
        };
        let fill = item.child("fill");
        if let Some(mode) = fill
            .and_then(|f| f.child("mode"))
            .and_then(|m| m.items().get(1)?.text())
            && mode != "solid"
        {
            return Err(ArtworkError::Unsupported(format!(
                "the zone {} on {} is filled in `{mode}` mode; only solid fills are supported",
                if name.is_empty() { "(unnamed)" } else { &name },
                layers.join(", ")
            )));
        }
        let outlines = item
            .children("polygon")
            .map(|p| points_of(p, "a zone's polygon"))
            .collect::<Result<Vec<_>, _>>()?;
        let mut filled = Vec::new();
        for f in item.children("filled_polygon") {
            filled.push((layer_of(f), points_of(f, "a zone's filled polygon")?));
        }
        let zone = Zone {
            net: self.net_name(item),
            layers,
            outlines,
            filled,
            keepout,
            owner: String::new(),
            clearance: connect_pads
                .and_then(|c| child_num(c, "clearance"))
                .unwrap_or(0.0),
            min_thickness: child_num(item, "min_thickness").unwrap_or(0.0),
            connect,
            thermal_gap: fill
                .and_then(|f| child_num(f, "thermal_gap"))
                .unwrap_or(0.5),
            thermal_bridge: fill
                .and_then(|f| child_num(f, "thermal_bridge_width"))
                .unwrap_or(0.5),
            name,
            teardrop: item
                .child("attr")
                .is_some_and(|a| a.child("teardrop").is_some()),
        };
        self.out.zones.push(zone);
        Ok(())
    }

    /// A `gr_*` or `fp_*` drawing or text.
    fn drawing(&mut self, item: &Sexp, frame: Option<&Frame>) -> Result<(), ArtworkError> {
        let world = |p: Pt| frame.map_or(p, |f| f.world(p));
        let head = item.head().unwrap_or("");
        let kind = head.split_once('_').map_or(head, |(_, k)| k);
        let width = stroke_width(item);
        let fill = is_filled(item);
        let layer = layer_of(item);
        let pt = |name: &str| -> Result<Pt, ArtworkError> {
            Ok(world(
                child_point(item, name, &format!("a {head}'s {name}"))?.ok_or_else(|| {
                    ArtworkError::Malformed(format!("a {head} on {layer} has no ({name}"))
                })?,
            ))
        };
        let shape = match kind {
            "line" => Shape::Line(pt("start")?, pt("end")?),
            "rect" => {
                // Corners in the drawing's own frame, then put on the board, so that a
                // rectangle in a turned footprint is four corners of a turned shape.
                let local = |name: &str| -> Result<Pt, ArtworkError> {
                    child_point(item, name, &format!("a {head}'s {name}"))?.ok_or_else(|| {
                        ArtworkError::Malformed(format!("a {head} on {layer} has no ({name}"))
                    })
                };
                let (s, e) = (local("start")?, local("end")?);
                Shape::Poly(
                    [s, Pt::new(e.x, s.y), e, Pt::new(s.x, e.y)]
                        .map(world)
                        .to_vec(),
                )
            }
            "circle" => {
                let c = pt("center")?;
                let e = pt("end")?;
                Shape::Circle {
                    centre: c,
                    radius: c.dist(e),
                }
            }
            "arc" => {
                let (s, m, e) = (pt("start")?, pt("mid").or_else(|_| pt("end"))?, pt("end")?);
                match Arc::through(s, m, e) {
                    Some(a) => Shape::Arc(a),
                    None => Shape::Line(s, e),
                }
            }
            "poly" => {
                let points = points_of(item, &format!("a {head}"))?;
                Shape::Poly(points.into_iter().map(world).collect())
            }
            "text" => return self.text(item, frame),
            other => {
                self.out
                    .skipped
                    .push(format!("a {head} on {layer} ({other}) is not plotted",));
                return Ok(());
            }
        };
        self.out.graphics.push(Graphic {
            layer,
            owner: frame.map(|f| f.part.clone()).unwrap_or_default(),
            shape,
            width,
            fill,
        });
        Ok(())
    }

    fn text(&mut self, item: &Sexp, frame: Option<&Frame>) -> Result<(), ArtworkError> {
        let head = item.head().unwrap_or("");
        // `(fp_text user "x" ...)` has a kind before the text; `property` has the name.
        let first = if matches!(head, "fp_text" | "property") {
            2
        } else {
            1
        };
        let raw = item.items().get(first).and_then(Sexp::text).unwrap_or("");
        let effects = item.child("effects");
        let hidden = flag(item, "hide").unwrap_or(false)
            || effects.is_some_and(|e| flag(e, "hide").unwrap_or(false));
        if hidden || raw.is_empty() {
            return Ok(());
        }
        let layer = layer_of(item);
        let at_node = item.child("at");
        let local = match at_node {
            Some(a) => point(a, "a text's position")?,
            None => Pt::default(),
        };
        let angle = at_node
            .and_then(|a| a.items().get(3)?.text()?.parse::<f64>().ok())
            .unwrap_or(0.0);
        let font = effects.and_then(|e| e.child("font"));
        let size = font
            .and_then(|f| f.child("size"))
            .map(|s| -> Result<(f64, f64), ArtworkError> {
                Ok((
                    num(s.items().get(1), "a text's size")?,
                    num(s.items().get(2), "a text's size")?,
                ))
            })
            .transpose()?
            .unwrap_or((1.0, 1.0));
        let thickness = font
            .and_then(|f| child_num(f, "thickness"))
            .unwrap_or(size.1 * 0.15);
        let justify: Vec<&str> = effects
            .and_then(|e| e.child("justify"))
            .map(|j| j.items().iter().skip(1).filter_map(Sexp::text).collect())
            .unwrap_or_default();
        let owner = frame.map_or("the board".to_string(), |f| f.part.clone());
        let text = match frame {
            Some(f) => expand(raw, f),
            None => raw.to_string(),
        };
        self.out.texts.push(Text {
            layer,
            text,
            at: frame.map_or(local, |f| f.world(local)),
            angle,
            size,
            thickness,
            mirror: justify.contains(&"mirror"),
            italic: font.is_some_and(|f| {
                f.items().iter().any(|i| i.text() == Some("italic"))
                    || flag(f, "italic") == Some(true)
            }),
            h: if justify.contains(&"left") {
                HAlign::Left
            } else if justify.contains(&"right") {
                HAlign::Right
            } else {
                HAlign::Centre
            },
            v: if justify.contains(&"top") {
                VAlign::Top
            } else if justify.contains(&"bottom") {
                VAlign::Bottom
            } else {
                VAlign::Centre
            },
            owner,
        });
        Ok(())
    }

    fn footprint(&mut self, fp: &Sexp) -> Result<(), ArtworkError> {
        let at_node = fp.child("at");
        let at = at_node
            .map(|a| point(a, "a footprint's position"))
            .transpose()?
            .unwrap_or_default();
        let angle = at_node
            .and_then(|a| a.items().get(3)?.text()?.parse::<f64>().ok())
            .unwrap_or(0.0);
        let property = |name: &str| -> String {
            fp.children("property")
                .find(|p| p.items().get(1).and_then(Sexp::text) == Some(name))
                .and_then(|p| p.items().get(2)?.text())
                .unwrap_or("")
                .to_string()
        };
        let frame = Frame {
            at,
            angle,
            part: property("Reference"),
            value: property("Value"),
            footprint: fp
                .items()
                .get(1)
                .and_then(Sexp::text)
                .unwrap_or("")
                .to_string(),
            mask_margin: child_num(fp, "solder_mask_margin"),
            paste_margin: child_num(fp, "solder_paste_margin"),
            paste_ratio: child_num(fp, "solder_paste_ratio"),
            clearance: child_num(fp, "clearance"),
            zone_connect: zone_connect_of(fp),
        };
        self.out.footprints.push(Footprint {
            part: frame.part.clone(),
            at,
            angle,
            back: fp.child("layer").and_then(|l| l.items().get(1)?.text()) == Some("B.Cu"),
        });
        for item in fp.items().iter().skip(2) {
            match item.head() {
                Some("pad") => self.pad(item, &frame)?,
                Some("property") => self.text(item, Some(&frame))?,
                Some(h) if h.starts_with("fp_") => self.drawing(item, Some(&frame))?,
                // A zone inside a footprint (a keepout of the library): placed with it.
                Some("zone") => {
                    self.zone(item)?;
                    if let Some(z) = self.out.zones.last_mut() {
                        z.owner = frame.part.clone();
                        for o in &mut z.outlines {
                            for p in o.iter_mut() {
                                *p = frame.world(*p);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn pad(&mut self, item: &Sexp, frame: &Frame) -> Result<(), ArtworkError> {
        let items = item.items();
        let number = items.get(1).and_then(Sexp::text).unwrap_or("").to_string();
        let where_ = format!("pad {}.{}", frame.part, number);
        let kind = match items.get(2).and_then(Sexp::text) {
            Some("smd") => PadKind::Smd,
            Some("thru_hole") => PadKind::Thru,
            Some("np_thru_hole") => PadKind::Npth,
            Some("connect") => PadKind::Connect,
            other => {
                return Err(ArtworkError::Unsupported(format!(
                    "{where_} has the type `{}`",
                    other.unwrap_or("")
                )));
            }
        };
        let shape = match items.get(3).and_then(Sexp::text) {
            Some("circle") => PadShape::Circle,
            Some("rect") => PadShape::Rect,
            Some("oval") => PadShape::Oval,
            Some("roundrect") => PadShape::RoundRect,
            Some("trapezoid") => PadShape::Trapezoid,
            Some("custom") => PadShape::Custom,
            other => {
                return Err(ArtworkError::Unsupported(format!(
                    "{where_} has the shape `{}`",
                    other.unwrap_or("")
                )));
            }
        };
        if item.child("padstack").is_some() {
            return Err(ArtworkError::Unsupported(format!(
                "{where_} has a padstack (a different shape on different layers), which is not supported"
            )));
        }
        let at_node = item.child("at");
        let local = at_node
            .map(|a| point(a, &format!("{where_}'s position")))
            .transpose()?
            .unwrap_or_default();
        let angle = at_node
            .and_then(|a| a.items().get(3)?.text()?.parse::<f64>().ok())
            .unwrap_or(0.0);
        let size = item
            .child("size")
            .map(|s| -> Result<(f64, f64), ArtworkError> {
                let w = num(s.items().get(1), &format!("{where_}'s size"))?;
                let h = s.items().get(2).map_or(Ok(w), |_| {
                    num(s.items().get(2), &format!("{where_}'s size"))
                })?;
                Ok((w, h))
            })
            .transpose()?
            .unwrap_or((0.0, 0.0));
        let drill = item
            .child("drill")
            .map(|d| -> Result<Drill, ArtworkError> {
                let args: Vec<&Sexp> = d
                    .items()
                    .iter()
                    .skip(1)
                    .filter(|a| a.head().is_none())
                    .collect();
                let (w, l) = if args.first().and_then(|a| a.text()) == Some("oval") {
                    (
                        num(args.get(1).copied(), &format!("{where_}'s drill"))?,
                        num(args.get(2).copied(), &format!("{where_}'s drill"))?,
                    )
                } else {
                    let w = num(args.first().copied(), &format!("{where_}'s drill"))?;
                    let l = match args.get(1) {
                        Some(a) => num(Some(a), &format!("{where_}'s drill"))?,
                        None => w,
                    };
                    (w, l)
                };
                Ok(Drill {
                    width: w,
                    length: l,
                    offset: child_point(d, "offset", &format!("{where_}'s drill offset"))?
                        .unwrap_or_default(),
                })
            })
            .transpose()?
            .filter(|d| d.width > 0.0);
        let layers = self.expand_layers(
            item.child("layers")
                .map(|l| {
                    l.items()
                        .iter()
                        .skip(1)
                        .filter_map(Sexp::text)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        );
        let mut chamfer = [false; 4];
        if let Some(c) = item.child("chamfer") {
            for corner in c.items().iter().skip(1).filter_map(Sexp::text) {
                match corner {
                    "top_left" => chamfer[0] = true,
                    "top_right" => chamfer[1] = true,
                    "bottom_left" => chamfer[2] = true,
                    "bottom_right" => chamfer[3] = true,
                    _ => {}
                }
            }
        }
        let primitives = match item.child("primitives") {
            Some(p) => self.primitives(p, &where_)?,
            None => Vec::new(),
        };
        let anchor_rect = item
            .child("options")
            .and_then(|o| o.child("anchor")?.items().get(1)?.text())
            == Some("rect");
        let connect = zone_connect_of(item).or(frame.zone_connect);
        let pad = Pad {
            part: frame.part.clone(),
            number,
            function: item
                .child("pinfunction")
                .and_then(|f| f.items().get(1)?.text())
                .map(str::to_string),
            net: self.net_name(item),
            kind,
            shape,
            at: frame.world(local),
            angle,
            size,
            // KiCad reads a rounded rectangle with no ratio as 0.25.
            roundrect_ratio: child_num(item, "roundrect_rratio").unwrap_or(0.25),
            rect_delta: item
                .child("rect_delta")
                .map(|d| {
                    (
                        num(d.items().get(1), "rect_delta").unwrap_or(0.0),
                        num(d.items().get(2), "rect_delta").unwrap_or(0.0),
                    )
                })
                .unwrap_or((0.0, 0.0)),
            chamfer_ratio: child_num(item, "chamfer_ratio").unwrap_or(0.0),
            chamfer,
            primitives,
            anchor_rect,
            drill,
            layers,
            mask_margin: child_num(item, "solder_mask_margin").or(frame.mask_margin),
            own_mask_margin: child_num(item, "solder_mask_margin").is_some(),
            footprint_mask_margin: frame.mask_margin,
            paste_margin: child_num(item, "solder_paste_margin").or(frame.paste_margin),
            paste_ratio: child_num(item, "solder_paste_margin_ratio").or(frame.paste_ratio),
            zone_connect: connect,
            thermal_angle: child_num(item, "thermal_bridge_angle"),
            remove_unused_layers: flag(item, "remove_unused_layers").unwrap_or(false),
            clearance: child_num(item, "clearance").or(frame.clearance),
        };
        let _ = (&frame.value, &frame.footprint);
        self.out.pads.push(pad);
        Ok(())
    }

    fn primitives(&mut self, node: &Sexp, where_: &str) -> Result<Vec<Primitive>, ArtworkError> {
        let mut out = Vec::new();
        for p in node.items().iter().skip(1) {
            let head = p.head().unwrap_or("");
            let width = stroke_width(p);
            let pt = |name: &str| -> Result<Pt, ArtworkError> {
                child_point(p, name, &format!("a {head} in {where_}"))?.ok_or_else(|| {
                    ArtworkError::Malformed(format!("a {head} in {where_} has no ({name}"))
                })
            };
            out.push(match head {
                "gr_line" => Primitive::Line {
                    a: pt("start")?,
                    b: pt("end")?,
                    width,
                },
                "gr_arc" => {
                    let (s, m, e) = (pt("start")?, pt("mid")?, pt("end")?);
                    match Arc::through(s, m, e) {
                        Some(arc) => Primitive::Arc { arc, width },
                        None => Primitive::Line { a: s, b: e, width },
                    }
                }
                "gr_circle" => {
                    let c = pt("center")?;
                    Primitive::Circle {
                        centre: c,
                        radius: c.dist(pt("end")?),
                        width,
                        fill: is_filled(p),
                    }
                }
                "gr_rect" => {
                    let (a, b) = (pt("start")?, pt("end")?);
                    Primitive::Poly {
                        points: vec![a, Pt::new(b.x, a.y), b, Pt::new(a.x, b.y)],
                        width,
                        fill: is_filled(p),
                    }
                }
                "gr_poly" => Primitive::Poly {
                    points: points_of(p, &format!("a gr_poly in {where_}"))?,
                    width,
                    fill: is_filled(p),
                },
                other => {
                    return Err(ArtworkError::Unsupported(format!(
                        "{where_} is a custom pad with a `{other}` primitive, which is not supported"
                    )));
                }
            });
        }
        Ok(out)
    }

    /// `*.Cu` is every copper layer of the board, `*.Mask` both masks, `F&B.Cu` the outer
    /// two; named layers stay.
    fn expand_layers(&self, names: Vec<&str>) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut push = |l: String| {
            if !out.contains(&l) {
                out.push(l);
            }
        };
        for n in names {
            match n {
                "*.Cu" => self.copper.iter().cloned().for_each(&mut push),
                "F&B.Cu" => {
                    if let Some(f) = self.copper.first() {
                        push(f.clone());
                    }
                    if let Some(b) = self.copper.last() {
                        push(b.clone());
                    }
                }
                other if other.starts_with("*.") => {
                    push(format!("F.{}", &other[2..]));
                    push(format!("B.{}", &other[2..]));
                }
                other => push(other.to_string()),
            }
        }
        out
    }
}

/// A footprint's text variables put in: `${REFERENCE}`, `${VALUE}`, `${FOOTPRINT_NAME}`.
fn expand(text: &str, f: &Frame) -> String {
    text.replace("${REFERENCE}", &f.part)
        .replace("${VALUE}", &f.value)
        .replace(
            "${FOOTPRINT_NAME}",
            f.footprint.rsplit(':').next().unwrap_or(&f.footprint),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOARD: &str = r#"(kicad_pcb
	(layers (0 "F.Cu" signal) (31 "B.Cu" signal) (25 "Edge.Cuts" user))
	(setup (aux_axis_origin 100 200) (pad_to_mask_clearance 0.05) (tenting front back))
	(net 0 "") (net 1 "GND")
	(footprint "L:R" (layer "F.Cu") (at 110 190 90)
		(property "Reference" "R1" (at 0 -2 0) (layer "F.SilkS") (effects (font (size 1 1) (thickness 0.15))))
		(property "Value" "10k" (at 0 2 0) (layer "F.Fab") (effects (font (size 1 1))))
		(fp_line (start -1 0) (end 1 0) (stroke (width 0.12) (type solid)) (layer "F.SilkS"))
		(fp_text user "${REFERENCE}" (at 0 0 90) (layer "F.Fab") (effects (font (size 0.4 0.4) (thickness 0.06))))
		(pad "1" smd roundrect (at -1 0 90) (size 1 2) (layers "F.Cu" "F.Mask" "F.Paste") (roundrect_rratio 0.25) (net 1 "GND"))
		(pad "2" thru_hole oval (at 1 0 90) (size 1.5 2) (drill oval 0.8 1.4) (layers "*.Cu" "*.Mask")))
	(segment (start 1 2) (end 3 4) (width 0.2) (layer "B.Cu") (net 1))
	(via (at 5 6) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))
	(gr_rect (start 0 0) (end 10 5) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
)"#;

    #[test]
    fn a_kicad_10_board_names_its_nets_with_no_numbers() {
        let a = Artwork::parse(
            r#"(kicad_pcb (version 20260206) (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
            (footprint "L:R" (layer "F.Cu") (at 0 0)
              (property "Reference" "R1" (at 0 0 0) (layer "F.Fab"))
              (pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu") (net "GND")))
            (segment (start 0 0) (end 2 0) (width 0.2) (layer "F.Cu") (net "GND"))
            (via (at 2 0) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net "+3V3")))"#,
        )
        .unwrap();
        assert_eq!(a.pads[0].net.as_deref(), Some("GND"));
        assert_eq!(a.tracks[0].net.as_deref(), Some("GND"));
        assert_eq!(a.vias[0].net.as_deref(), Some("+3V3"));
    }

    #[test]
    fn a_footprints_own_margins_and_clearance_are_its_pads_unless_a_pad_says_its_own() {
        let a = Artwork::parse(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
            (footprint "L:F" (layer "F.Cu") (at 0 0)
              (property "Reference" "U1" (at 0 0 0) (layer "F.Fab"))
              (solder_mask_margin 0.08) (solder_paste_ratio -0.1) (clearance 0.1) (zone_connect 2)
              (pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu" "F.Mask" "F.Paste"))
              (pad "2" smd rect (at 2 0) (size 1 1) (layers "F.Cu" "F.Mask" "F.Paste")
                (solder_mask_margin 0.02) (clearance 0.3) (zone_connect 0))))"#,
        )
        .unwrap();
        let (p1, p2) = (&a.pads[0], &a.pads[1]);
        assert_eq!(
            (
                p1.mask_margin,
                p1.paste_ratio,
                p1.clearance,
                p1.zone_connect
            ),
            (Some(0.08), Some(-0.1), Some(0.1), Some(Connect::Solid))
        );
        assert_eq!(
            (
                p2.mask_margin,
                p2.paste_ratio,
                p2.clearance,
                p2.zone_connect
            ),
            (Some(0.02), Some(-0.1), Some(0.3), Some(Connect::None))
        );
    }

    #[test]
    fn a_pad_with_no_turn_is_not_turned_whatever_its_footprint_is() {
        let a = Artwork::parse(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
            (footprint "L:F" (layer "F.Cu") (at 10 10 90)
              (property "Reference" "U1" (at 0 0 0) (layer "F.Fab"))
              (pad "1" smd rect (at 0 0) (size 1 2) (layers "F.Cu"))
              (pad "2" smd rect (at 3 0 90) (size 1 2) (layers "F.Cu"))))"#,
        )
        .unwrap();
        assert_eq!((a.pads[0].angle, a.pads[1].angle), (0.0, 90.0));
    }

    #[test]
    fn a_footprint_is_placed_by_its_position_and_turn() {
        let a = Artwork::parse(BOARD).unwrap();
        assert_eq!(a.copper, ["F.Cu", "B.Cu"]);
        assert_eq!(a.origin, Pt::new(100.0, 200.0));
        assert_eq!(a.mask_margin, 0.05);
        assert!(a.tent_front && a.tent_back);
        // The footprint is at (110, 190) turned 90: its local (-1, 0) is 1 mm below.
        let p1 = &a.pads[0];
        assert_eq!((p1.at.x, p1.at.y), (110.0, 191.0));
        assert_eq!(p1.angle, 90.0);
        assert_eq!(p1.net.as_deref(), Some("GND"));
        assert_eq!(p1.layers, ["F.Cu", "F.Mask", "F.Paste"]);
        let p2 = &a.pads[1];
        assert_eq!((p2.at.x, p2.at.y), (110.0, 189.0));
        assert_eq!(p2.layers, ["F.Cu", "B.Cu", "F.Mask", "B.Mask"]);
        let drill = p2.drill.unwrap();
        assert!(drill.is_slot());
        assert_eq!((drill.width, drill.length), (0.8, 1.4));
    }

    #[test]
    fn text_and_drawings_come_with_their_footprint_variables_and_places() {
        let a = Artwork::parse(BOARD).unwrap();
        let texts: Vec<_> = a
            .texts
            .iter()
            .map(|t| (t.layer.as_str(), t.text.as_str()))
            .collect();
        assert_eq!(
            texts,
            [("F.SilkS", "R1"), ("F.Fab", "10k"), ("F.Fab", "R1")]
        );
        let line = a.graphics.iter().find(|g| g.layer == "F.SilkS").unwrap();
        assert_eq!(
            line.shape,
            Shape::Line(Pt::new(110.0, 191.0), Pt::new(110.0, 189.0))
        );
        assert_eq!(line.width, 0.12);
        assert!(a.graphics.iter().any(|g| g.layer == "Edge.Cuts"));
        assert_eq!(a.tracks.len(), 1);
        assert_eq!(a.vias[0].net.as_deref(), Some("GND"));
        assert_eq!(a.vias[0].tented, (true, true));
    }

    #[test]
    fn a_blind_via_is_refused_with_its_place() {
        let src = BOARD.replace(
            "(via (at 5 6) (size 0.6) (drill 0.3) (layers \"F.Cu\" \"B.Cu\")",
            "(via blind (at 5 6) (size 0.6) (drill 0.3) (layers \"F.Cu\" \"In1.Cu\")",
        );
        let err = Artwork::parse(&src).unwrap_err().to_string();
        assert!(
            err.contains("via at (5, 6)") && err.contains("F.Cu to In1.Cu"),
            "{err}"
        );
    }

    #[test]
    fn a_pad_with_a_shape_that_is_not_known_is_named() {
        let src = BOARD.replace("smd roundrect", "smd hexagon");
        let err = Artwork::parse(&src).unwrap_err().to_string();
        assert!(err.contains("pad R1.1") && err.contains("hexagon"), "{err}");
    }

    #[test]
    fn a_padstack_and_a_hatched_zone_are_refused_with_their_names() {
        let src = BOARD.replace("(roundrect_rratio 0.25)", "(padstack (mode custom))");
        let err = Artwork::parse(&src).unwrap_err().to_string();
        assert!(err.contains("pad R1.1 has a padstack"), "{err}");
        let src = BOARD.replace(
            "(via (at 5 6)",
            "(zone (net 1) (net_name \"GND\") (layer \"F.Cu\") (name \"gnd\") (fill yes (mode hatch)) (polygon (pts (xy 0 0) (xy 1 0) (xy 1 1))))\n\t(via (at 5 6)",
        );
        let err = Artwork::parse(&src).unwrap_err().to_string();
        assert!(
            err.contains("zone gnd on F.Cu is filled in `hatch` mode"),
            "{err}"
        );
    }

    #[test]
    fn a_roundrect_with_no_ratio_is_read_as_kicad_reads_it() {
        let src = BOARD.replace(" (roundrect_rratio 0.25)", "");
        assert_eq!(Artwork::parse(&src).unwrap().pads[0].roundrect_ratio, 0.25);
    }

    #[test]
    fn a_hole_with_no_copper_round_it_has_no_copper_on_any_layer() {
        let src = BOARD.replace(
            "(pad \"2\" thru_hole oval",
            "(pad \"\" np_thru_hole circle (at 3 0) (size 1 1) (drill 1) (layers \"*.Cu\" \"*.Mask\"))\n\t\t(pad \"2\" thru_hole oval",
        );
        let a = Artwork::parse(&src).unwrap();
        let hole = a.pads.iter().find(|p| p.kind == PadKind::Npth).unwrap();
        assert!(!a.pad_copper_on(hole, "F.Cu") && !a.pad_copper_on(hole, "B.Cu"));
        assert!(a.pad_copper_on(&a.pads[0], "F.Cu"));
        assert!(!a.pad_copper_on(&a.pads[0], "B.Cu"));
    }
}
