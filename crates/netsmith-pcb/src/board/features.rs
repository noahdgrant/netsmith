//! What is on a board besides its parts: layers, planes, pours, keepouts, cutouts, tracks and vias.

use super::*;

/// An outer copper layer, the two a track can be on. Inner layers are planes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CopperLayer {
    Front,
    Back,
}

impl CopperLayer {
    /// KiCad's name for the layer.
    pub fn name(self) -> &'static str {
        match self {
            Self::Front => "F.Cu",
            Self::Back => "B.Cu",
        }
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
    /// Drawn in the editor and kept in the layout file, not stated in the source (ADR 0029).
    pub drawn: bool,
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

    /// Whether it applies to a copper layer, `top` or `bottom`.
    pub fn on(&self, layer: CopperLayer) -> bool {
        let name = match layer {
            CopperLayer::Front => "top",
            CopperLayer::Back => "bottom",
        };
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
    pub fn arc(&self) -> Option<netsmith_geom::geom::Arc> {
        let g = |p: Pt| netsmith_geom::geom::Pt::new(p.x, p.y);
        netsmith_geom::geom::Arc::through(g(self.from), g(self.mid?), g(self.to))
    }

    /// The track's centre line from `from` to `to`: its two ends, or for an arc points no
    /// further than 2 µm from it (`netsmith_geom::geom::ARC_ERROR`).
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
