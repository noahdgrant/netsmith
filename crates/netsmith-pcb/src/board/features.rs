//! What is on a board besides its parts: layers, planes, pours, keepouts, cutouts, tracks and vias.

use super::*;

/// A copper layer a track can be on: an outer layer, or an inner signal layer (ADR 0111, issue
/// #833). An inner plane holds no track; the router draws on the inner layers the process
/// declares signal layers only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CopperLayer {
    Front,
    Back,
    /// The inner layer `In{n}.Cu`, counted from the top from 1.
    Inner(u8),
}

/// KiCad's names of the inner layers, `In1.Cu` to `In30.Cu`: KiCad has at most 30.
const INNER_NAMES: [&str; 30] = [
    "In1.Cu", "In2.Cu", "In3.Cu", "In4.Cu", "In5.Cu", "In6.Cu", "In7.Cu", "In8.Cu", "In9.Cu",
    "In10.Cu", "In11.Cu", "In12.Cu", "In13.Cu", "In14.Cu", "In15.Cu", "In16.Cu", "In17.Cu",
    "In18.Cu", "In19.Cu", "In20.Cu", "In21.Cu", "In22.Cu", "In23.Cu", "In24.Cu", "In25.Cu",
    "In26.Cu", "In27.Cu", "In28.Cu", "In29.Cu", "In30.Cu",
];

/// The layout file's names of the inner layers, `inner1` to `inner30`.
const INNER_WORDS: [&str; 30] = [
    "inner1", "inner2", "inner3", "inner4", "inner5", "inner6", "inner7", "inner8", "inner9",
    "inner10", "inner11", "inner12", "inner13", "inner14", "inner15", "inner16", "inner17",
    "inner18", "inner19", "inner20", "inner21", "inner22", "inner23", "inner24", "inner25",
    "inner26", "inner27", "inner28", "inner29", "inner30",
];

impl CopperLayer {
    /// KiCad's name for the layer.
    pub fn name(self) -> &'static str {
        match self {
            Self::Front => "F.Cu",
            Self::Back => "B.Cu",
            Self::Inner(n) => INNER_NAMES[usize::from(n.clamp(1, 30)) - 1],
        }
    }

    /// The layer's name in the layout, routes and physical files: `top`, `inner2`, `bottom`.
    pub fn word(self) -> &'static str {
        match self {
            Self::Front => "top",
            Self::Back => "bottom",
            Self::Inner(n) => INNER_WORDS[usize::from(n.clamp(1, 30)) - 1],
        }
    }

    /// The layer a [`CopperLayer::word`] names, if it names one.
    pub fn of_word(word: &str) -> Option<CopperLayer> {
        match word {
            "top" => Some(Self::Front),
            "bottom" => Some(Self::Back),
            _ => {
                let n: u8 = word.strip_prefix("inner")?.parse().ok()?;
                (1..=30).contains(&n).then_some(Self::Inner(n))
            }
        }
    }

    /// The layer KiCad names `name` (`F.Cu`, `In2.Cu`, `B.Cu`), if it is a copper layer.
    pub fn of_name(name: &str) -> Option<CopperLayer> {
        match name {
            "F.Cu" => Some(Self::Front),
            "B.Cu" => Some(Self::Back),
            _ => {
                let n = inner_number(name);
                (1..=30).contains(&n).then_some(Self::Inner(n as u8))
            }
        }
    }

    /// Whether it is an outer layer, where parts stand.
    pub fn is_outer(self) -> bool {
        !matches!(self, Self::Inner(_))
    }
}

/// The number of an inner layer's KiCad name, `In3.Cu` -> 3, and 0 for any other.
pub(super) fn inner_number(layer: &str) -> u32 {
    layer
        .strip_prefix("In")
        .and_then(|n| n.strip_suffix(".Cu"))
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// An inner copper layer poured as one sheet on a net.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Plane {
    /// KiCad's name for the layer: `In1.Cu`.
    pub layer: String,
    pub net: String,
}

/// An outer copper layer poured on a net (ADR 0020).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BoardPour {
    pub layer: CopperLayer,
    pub net: String,
    /// The most space between the vias that stitch it to the other layers of its net.
    pub stitch_pitch: Option<f64>,
    /// The gap of a thermal relief, in mm, if the design states one; KiCad's default otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermal_gap: Option<f64>,
    /// The width of the spokes of a thermal relief, in mm, likewise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spoke_width: Option<f64>,
}

/// How a zone joins the pads of its net: KiCad's `connect_pads` (`yes`, nothing, `no`,
/// `thru_hole_only`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZonePads {
    /// Solid copper up to the pad.
    Solid,
    /// A thermal relief: a gap round the pad, crossed by four spokes.
    #[default]
    Thermal,
    /// Not joined: the pad keeps the zone's clearance, as another net's would.
    None,
    /// A thermal relief round a through-hole pad, solid copper up to a surface pad.
    ThruHoleOnly,
}

/// Which pieces of a zone's fill that join nothing KiCad removes: KiCad's
/// `island_removal_mode` 0, 1 and 2. netsmith's own fill removes none (`netsmith_geom::fill`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneIslands {
    #[default]
    Remove,
    Keep,
    /// Removes those smaller than [`BoardZone::island_min_area`].
    RemoveBelow,
}

/// A zone a routes file fixed ([`crate::routes`]): copper filled inside `outline`, less its
/// holes, on each of `layers`, joined to `net`, with the settings of KiCad's zone that change
/// the copper. A layer that has one takes its zones from the routes file alone: the design's
/// pour or plane on it is not drawn ([`Board::zoned`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BoardZone {
    pub net: String,
    /// KiCad's names of its copper layers: `F.Cu`, `In1.Cu`, `B.Cu`.
    pub layers: Vec<String>,
    /// On the sheet, as the board's other copper.
    pub outline: Vec<Pt>,
    pub holes: Vec<Vec<Pt>>,
    /// Where two zones of a layer overlap, the higher one has the area (KiCad's rule).
    pub priority: u32,
    /// The space it keeps from copper of other nets, in mm.
    pub clearance: f64,
    /// KiCad's `min_thickness`: the narrowest copper KiCad's fill keeps, in mm.
    pub min_width: f64,
    pub pads: ZonePads,
    /// The gap of a thermal relief and the width of its spokes, in mm.
    pub thermal_gap: f64,
    pub spoke_width: f64,
    pub islands: ZoneIslands,
    /// For [`ZoneIslands::RemoveBelow`], in mm².
    pub island_min_area: Option<f64>,
}

/// An area where tracks, vias, copper or parts are not allowed (ADR 0029).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BoardKeepout {
    /// The member's path in the design.
    pub name: String,
    /// The path of the part that carries it, if one does. Then `at` is in the part's own
    /// frame (its footprint's, turned with the part).
    pub part: Option<String>,
    /// Its corner nearest the origin: of the board (the outline's top left), or of the part.
    /// `None` while neither the design nor the layout file says where it is.
    pub at: Option<Pt>,
    pub size: (f64, f64),
    /// The copper layers it applies to: `top`, `bottom`, `inner1`, `inner2`.
    pub layers: Vec<String>,
    pub tracks: bool,
    pub vias: bool,
    pub copper: bool,
    pub parts: bool,
    /// The tallest a part may stand in it, in mm, on the faces in `layers` (ADR 0045,
    /// "Height limits").
    pub max_height: Option<f64>,
}

/// A keepout where it is on the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct KeepoutArea {
    pub name: String,
    /// The path of the part that carries it, if one does.
    pub carrier: Option<String>,
    pub rect: Rect,
    pub layers: Vec<String>,
    pub tracks: bool,
    pub vias: bool,
    pub copper: bool,
    pub parts: bool,
    pub max_height: Option<f64>,
}

impl KeepoutArea {
    /// Whether it forbids any of tracks, vias, copper and parts: a keepout that only limits
    /// height does not, and is no rule area for KiCad.
    pub fn forbids_anything(&self) -> bool {
        self.tracks || self.vias || self.copper || self.parts
    }

    /// Whether its height limit applies to parts on a face: it does when its layers name
    /// that face's outer layer.
    pub fn limits_height_on(&self, side: Side) -> bool {
        self.max_height.is_some() && self.on(side.copper())
    }

    /// Whether it keeps the parts on a face out: it forbids parts and its layers name that
    /// face's outer layer (issue #767). A part stands on a face, so a keepout whose layers
    /// are all inner ones keeps no part out.
    pub fn forbids_parts_on(&self, side: Side) -> bool {
        self.parts && self.on(side.copper())
    }

    /// Whether it applies to a copper layer, `top`, `inner2` or `bottom`.
    pub fn on(&self, layer: CopperLayer) -> bool {
        let name = layer.word();
        self.layers.iter().any(|l| l == name)
    }
}

/// A rectangular hole cut through the board (ADR 0020).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BoardCutout {
    pub name: String,
    /// Its corner nearest the board's origin, in the board's frame (the outline's top left),
    /// or `None` while neither the design nor the layout file says where it is.
    pub at: Option<Pt>,
    pub size: (f64, f64),
    pub radius: f64,
}

/// A run of copper: straight from `from` to `to`, or an arc of a circle through `mid`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Track {
    pub net: String,
    pub layer: CopperLayer,
    pub from: Pt,
    pub to: Pt,
    pub width: f64,
    /// For an arc, a point on it halfway from `from` to `to`, as KiCad's `(arc (start) (mid)
    /// (end))` stores one. The router draws arcs only where a track bends (the header of
    /// netsmith-router's `arcs.rs`); `None` is a straight track.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mid: Option<Pt>,
}

impl Track {
    /// A straight track.
    pub fn straight(net: &str, layer: CopperLayer, from: Pt, to: Pt, width: f64) -> Track {
        Track {
            net: net.to_string(),
            layer,
            from,
            to,
            width,
            mid: None,
        }
    }

    /// The track's arc, if it is one whose three points are not on a line.
    pub fn arc(&self) -> Option<netsmith_geom::Arc> {
        let g = |p: Pt| netsmith_geom::Pt::new(p.x, p.y);
        netsmith_geom::Arc::through(g(self.from), g(self.mid?), g(self.to))
    }

    /// The track's centre line from `from` to `to`: its two ends, or for an arc points no
    /// further than 2 µm from it (`netsmith_geom::ARC_ERROR`).
    pub fn centre_line(&self) -> Vec<Pt> {
        match self.arc() {
            Some(arc) => arc
                .points()
                .into_iter()
                .map(|p| Pt::new(p.x, p.y))
                .collect(),
            None => vec![self.from, self.to],
        }
    }

    /// How far `p` is from the track's centre line, in mm.
    pub fn distance_to(&self, p: Pt) -> f64 {
        let to_piece = |a: Pt, b: Pt| {
            let ab = b - a;
            let len2 = ab.x * ab.x + ab.y * ab.y;
            let t = if len2 == 0.0 {
                0.0
            } else {
                (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0)
            };
            p.distance(Pt::new(a.x + ab.x * t, a.y + ab.y * t))
        };
        self.centre_line()
            .windows(2)
            .map(|w| to_piece(w[0], w[1]))
            .fold(f64::MAX, f64::min)
    }

    /// How long the track is along its centre line, in mm.
    pub fn length(&self) -> f64 {
        match self.arc() {
            Some(arc) => arc.radius * arc.sweep().abs(),
            None => self.from.distance(self.to),
        }
    }
}

/// Copper that widens a track where it meets a pad or a via (issue #431, S11): a polygon on
/// the track's layer and net, drawn by `netsmith_router::teardrop::add` once the board is routed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Teardrop {
    pub net: String,
    pub layer: CopperLayer,
    pub points: Vec<Pt>,
}

/// A plated hole that joins the two copper layers.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Via {
    pub net: String,
    pub at: Pt,
    pub diameter: f64,
    pub drill: f64,
}
