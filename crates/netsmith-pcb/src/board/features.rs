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

    /// Its place from the top on a board of `layers` copper layers: 0 for the top, `layers - 1`
    /// for the bottom, `n` for `In{n}.Cu`.
    pub fn position(self, layers: u32) -> usize {
        match self {
            Self::Front => 0,
            Self::Back => layers.max(2) as usize - 1,
            Self::Inner(n) => usize::from(n),
        }
    }

    /// The layer at a place from the top, on a board of `layers` copper layers.
    pub fn at_position(position: usize, layers: u32) -> CopperLayer {
        match position {
            0 => Self::Front,
            p if p + 1 >= layers as usize => Self::Back,
            p => Self::Inner(p as u8),
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
    /// The space from copper of other nets, in mm, if the design states one;
    /// [`crate::Rules::plane_clearance`] otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clearance: Option<f64>,
    /// The narrowest copper the fill keeps (KiCad's `min_thickness`), in mm, if the design
    /// states one; 0.25 mm otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<f64>,
    /// How it joins its net's pads, if the design states it; a thermal relief otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pads: Option<ZonePads>,
}

/// How a zone joins the pads of its net: the design's [`netsmith_core::PadConnection`], which a
/// pour states too (KiCad's `connect_pads`).
pub use netsmith_core::PadConnection as ZonePads;

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
/// the copper. One of the net of the design's pour or plane on its layer restates it, which is
/// then not drawn ([`Board::replaced`]); one of another net has the area it covers, and the pour
/// is drawn round it (issue #1314, [`Board::zone_lift`]).
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
    /// Whether `layers` name the faces as they are when the part that carries it is on the
    /// top, and turn over with the part: `top` is then the face the part is on, wherever the
    /// placer puts it. The area another board of a system covers over a mated connector is one
    /// (issue 1331); a keepout of the design names its faces as they are, and is not.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub on_carrier_face: bool,
    /// The corners of its area, in the frame of `at`, when it is a polygon and not the
    /// rectangle of `at` and `size` (which are then the box round it): a part's
    /// `.underside_keepout` (issue #1335).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polygon: Option<Vec<(f64, f64)>>,
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
    /// Its corners on the sheet when it is a polygon (issue #1335); `rect` is then the box round
    /// it.
    pub shape: Option<Vec<Pt>>,
}

impl KeepoutArea {
    /// Whether a box is in its area: in `rect`, and, for a polygon, in the polygon (it overlaps
    /// it with some area, as two boxes overlap).
    pub fn covers(&self, r: &Rect) -> bool {
        if !r.overlaps(&self.rect) {
            return false;
        }
        match &self.shape {
            None => true,
            Some(corners) => polygon_overlaps(corners, r),
        }
    }

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

/// A plated hole that joins copper layers: every layer, or the layers of its `span`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Via {
    pub net: String,
    pub at: Pt,
    pub diameter: f64,
    pub drill: f64,
    /// The layers a blind or a buried via joins (issue #1015); `None` for a through via, which
    /// joins every layer, and then not written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<ViaSpan>,
}

/// The copper layers a blind or a buried via joins, the upper first: its copper is on them and
/// on the layers between, and its hole goes no further (issue #1015).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ViaSpan {
    pub from: CopperLayer,
    pub to: CopperLayer,
}

impl ViaSpan {
    /// The span from `a` to `b`, the upper first, on a board of `layers` copper layers.
    pub fn between(a: CopperLayer, b: CopperLayer, layers: u32) -> ViaSpan {
        if a.position(layers) <= b.position(layers) {
            ViaSpan { from: a, to: b }
        } else {
            ViaSpan { from: b, to: a }
        }
    }

    /// Whether it joins `layer`, or passes through it, on a board of `layers` copper layers.
    pub fn holds(&self, layer: CopperLayer, layers: u32) -> bool {
        let at = layer.position(layers);
        self.from.position(layers) <= at && at <= self.to.position(layers)
    }

    /// Whether it ends on an outer layer: a blind via; else it is buried.
    pub fn is_blind(&self) -> bool {
        self.from.is_outer() || self.to.is_outer()
    }

    /// The layers' places from the top, 0 the top layer: its first and its last.
    pub fn positions(&self, layers: u32) -> (usize, usize) {
        (self.from.position(layers), self.to.position(layers))
    }
}

impl Via {
    /// Whether its copper is on `layer` of a board of `layers` copper layers: every layer for a
    /// through via, else the layers of its span.
    pub fn on(&self, layer: CopperLayer, layers: u32) -> bool {
        self.span.is_none_or(|s| s.holds(layer, layers))
    }

    /// Whether it and `other` share a layer, so that their holes meet in the board.
    pub fn meets(&self, other: &Via, layers: u32) -> bool {
        match (self.span, other.span) {
            (Some(a), Some(b)) => {
                let (a0, a1) = a.positions(layers);
                let (b0, b1) = b.positions(layers);
                a0 <= b1 && b0 <= a1
            }
            _ => true,
        }
    }
}

/// Whether a polygon and a box overlap with some area: a corner of either is strictly inside the
/// other, or an edge of the polygon crosses the inside of the box.
fn polygon_overlaps(corners: &[Pt], r: &Rect) -> bool {
    const E: f64 = 1e-9;
    let inside_box =
        |p: &Pt| p.x > r.min.x + E && p.x < r.max.x - E && p.y > r.min.y + E && p.y < r.max.y - E;
    if corners.iter().any(inside_box) {
        return true;
    }
    let inside_polygon = |p: Pt| {
        let mut odd = false;
        for (i, a) in corners.iter().enumerate() {
            let b = corners[(i + 1) % corners.len()];
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
                odd = !odd;
            }
        }
        odd
    };
    // The box's centre, and points just inside its corners.
    let centre = Pt::new((r.min.x + r.max.x) / 2.0, (r.min.y + r.max.y) / 2.0);
    let inset = [
        Pt::new(r.min.x + E * 10.0, r.min.y + E * 10.0),
        Pt::new(r.max.x - E * 10.0, r.min.y + E * 10.0),
        Pt::new(r.max.x - E * 10.0, r.max.y - E * 10.0),
        Pt::new(r.min.x + E * 10.0, r.max.y - E * 10.0),
    ];
    if inside_polygon(centre) || inset.into_iter().any(inside_polygon) {
        return true;
    }
    // An edge of the polygon through the inside of the box: clip it to the box.
    corners.iter().enumerate().any(|(i, a)| {
        let b = corners[(i + 1) % corners.len()];
        let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        for (p, q) in [
            (-dx, a.x - (r.min.x + E)),
            (dx, (r.max.x - E) - a.x),
            (-dy, a.y - (r.min.y + E)),
            (dy, (r.max.y - E) - a.y),
        ] {
            if p == 0.0 {
                if q < 0.0 {
                    return false;
                }
            } else {
                let t = q / p;
                if p < 0.0 {
                    t0 = t0.max(t);
                } else {
                    t1 = t1.min(t);
                }
            }
        }
        t0 < t1
    })
}

#[cfg(test)]
mod polygon_tests {
    use super::*;

    fn tri() -> Vec<Pt> {
        // A right triangle with its long edge from (10, 0) to (0, 10).
        vec![Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), Pt::new(0.0, 10.0)]
    }

    #[test]
    fn a_box_inside_the_triangle_overlaps_it_and_one_past_its_long_edge_does_not() {
        let inside = Rect::new(Pt::new(1.0, 1.0), Pt::new(3.0, 3.0));
        let past = Rect::new(Pt::new(7.0, 7.0), Pt::new(9.0, 9.0));
        assert!(polygon_overlaps(&tri(), &inside));
        assert!(!polygon_overlaps(&tri(), &past));
    }

    #[test]
    fn a_box_round_the_triangle_or_across_an_edge_overlaps_it() {
        let round = Rect::new(Pt::new(-1.0, -1.0), Pt::new(11.0, 11.0));
        let across = Rect::new(Pt::new(4.0, 4.0), Pt::new(6.0, 6.0));
        assert!(polygon_overlaps(&tri(), &round));
        assert!(polygon_overlaps(&tri(), &across));
    }

    #[test]
    fn a_box_that_only_touches_an_edge_does_not_overlap() {
        let below = Rect::new(Pt::new(2.0, -3.0), Pt::new(4.0, 0.0));
        assert!(!polygon_overlaps(&tri(), &below));
    }
}
