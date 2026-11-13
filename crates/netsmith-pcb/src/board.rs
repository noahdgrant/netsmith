//! The board: a design's parts as placed footprints inside an outline, and its nets as the
//! pads they join.

use std::collections::{BTreeMap, HashMap};

use netsmith_core::{Design, DesignError, Process};
use netsmith_kicad::{MapError, Mapping};

use crate::footprint::{Footprint, FootprintError, FootprintLibrary, Layer, Mount, Pad};
use crate::geometry::{Pt, Rect};
use crate::overhang::{self, Edge, Overhang};
use crate::placement::{DoesNotFit, ORIGIN, PlaceOptions};
use crate::rules::Rules;
use crate::shape::RoundRect;

mod check;
mod features;
mod load;
mod placed;
mod problems;
mod process;
mod queries;
mod reports;
#[cfg(test)]
pub(crate) mod tests;
mod unplaced;

pub use features::*;
pub use placed::*;
pub use problems::*;
pub use unplaced::*;

#[derive(Debug)]
pub enum BoardError {
    Design(DesignError),
    /// The board says nothing about how it is to be made.
    NoProcess {
        board: String,
    },
    /// The process cannot make this board: it has layers the router cannot draw, or a
    /// thickness the board has not chosen.
    Process(String),
    /// No footprint is known for this part.
    Map {
        refdes: String,
        error: MapError,
    },
    Footprint {
        refdes: String,
        error: FootprintError,
    },
    /// The placement names a part that the design does not have.
    NoPart {
        refdes: String,
    },
    /// The parts do not fit the size the board was given.
    DoesNotFit(DoesNotFit),
    /// The design's polygon outline is not a board edge: it does not close, or it crosses
    /// itself (issue #57).
    Outline(String),
    /// The design's requirements on where parts go (ADR 0045) cannot all be met: each entry
    /// names what conflicts and the numbers.
    Constraints(Vec<String>),
    /// The requirements of the physical file, the circuit and the layout file provably cannot
    /// all hold, or a courtyard is past an edge nobody allowed: found before the placer ran
    /// (ADR 0045 section 14), so nothing was placed.
    Unplaceable(Box<crate::explain::Precheck>),
    /// A net names a pad that its part's footprint does not have.
    NoPad {
        net: String,
        refdes: String,
        pad: String,
        footprint: String,
        available: Vec<String>,
    },
}

impl std::fmt::Display for BoardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Design(e) => write!(f, "invalid design: {e}"),
            Self::NoProcess { board } => write!(
                f,
                "`{board}` does not say how it is made, and a PCB needs design rules: add `import std::mfg::jlcpcb::TwoLayer` and `.process: TwoLayer` to the board"
            ),
            Self::Process(message) => write!(f, "{message}"),
            Self::Outline(message) => write!(f, "{message}"),
            Self::Map { refdes, error } => write!(f, "{refdes}: {error}"),
            Self::Footprint { refdes, error } => write!(f, "{refdes}: {error}"),
            Self::NoPart { refdes } => write!(
                f,
                "the placement puts {refdes} on a face of the board, but the design has no part {refdes}"
            ),
            Self::DoesNotFit(e) => write!(f, "{e}"),
            Self::Constraints(errors) => write!(
                f,
                "the requirements on where parts go cannot all be met:\n  - {}",
                errors.join("\n  - ")
            ),
            Self::Unplaceable(found) => write!(f, "{}", found.errors().join("\n")),
            Self::NoPad {
                net,
                refdes,
                pad,
                footprint,
                available,
            } => write!(
                f,
                "net {net} joins pad {pad} of {refdes}, but footprint {footprint} has only pads {}",
                available.join(", ")
            ),
        }
    }
}

impl std::error::Error for BoardError {}

/// The box of the outline the design states, from the top left corner of the sheet, as the
/// most it allows (a range's top; infinite where a side is left open), and whether it is
/// known on both sides, and whether its size is a range (the real outline may be smaller).
pub fn outline_box(design: &Design) -> (Rect, bool, bool) {
    let Some(o) = &design.features.outline else {
        return (
            Rect::new(ORIGIN, Pt::new(f64::INFINITY, f64::INFINITY)),
            false,
            false,
        );
    };
    let top = |(_, hi): (f64, f64)| if hi.is_finite() { hi } else { f64::INFINITY };
    let (w, h) = (top(o.width), top(o.height));
    let limit = o.width.0 < o.width.1 - 1e-9 || o.height.0 < o.height.1 - 1e-9;
    (
        Rect::new(ORIGIN, Pt::new(ORIGIN.x + w, ORIGIN.y + h)),
        w.is_finite() && h.is_finite(),
        limit,
    )
}

/// What [`Board::load`] found: everything [`Board::unplaced`] needs before it turns the
/// requirements into places.
pub struct Loaded {
    pub process: Process,
    pub rules: Rules,
    /// The options with the design's keepouts, cutouts and height limits added.
    pub placement: PlaceOptions,
    /// The footprints, on the faces the options gave them, not yet placed.
    pub parts: Vec<Placed>,
}

impl From<DesignError> for BoardError {
    fn from(e: DesignError) -> Self {
        Self::Design(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BoardNet {
    pub name: String,
    pub pads: Vec<PadRef>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Board {
    pub name: String,
    /// The board edge, in mm: a rectangle, with corners rounded by `corner_radius`. On a
    /// board with a `polygon` outline, the box that holds the polygon.
    pub outline: Rect,
    /// The radius of the outline's four corners, 0 for square ones (and for a polygon).
    pub corner_radius: f64,
    /// The board edge when it is not a rectangle with four equal corners, on the sheet
    /// (issue #57). [`Board::edge`] is the edge whichever shape it is.
    pub polygon: Option<crate::polygon::Polygon>,
    /// Rectangular holes inside the outline.
    pub cutouts: Vec<BoardCutout>,
    /// The outer layers that are poured, top first. Every net with a pour is a net of the
    /// board.
    pub pours: Vec<BoardPour>,
    /// Areas where tracks, vias, copper or parts are not allowed.
    pub keepouts: Vec<BoardKeepout>,
    pub parts: Vec<Placed>,
    pub nets: Vec<BoardNet>,
    /// What the board is to be made to, read from its process.
    pub rules: Rules,
    /// The process it is made with, which the writers take the stackup from.
    pub process: Process,
    /// The inner layers, from the top. Empty for a two-layer board. Every net with a plane
    /// is a net of the board.
    pub planes: Vec<Plane>,
    /// Routed copper. Empty until the board is routed.
    pub tracks: Vec<Track>,
    pub vias: Vec<Via>,
    /// The teardrops where tracks meet pads and vias; empty until the board is routed, and
    /// after it when they are switched off.
    #[serde(default)]
    pub teardrops: Vec<Teardrop>,
    /// The narrowest track of the nets whose rails ask for one (ADR 0020), by net name.
    /// Every other net is routed at the rules' track width.
    pub net_widths: Vec<(String, f64)>,
    /// The current each net carries where it is known, and the least width that carries it
    /// (issue #431): no track of the net is narrower, away from its pads.
    pub net_currents: Vec<netsmith_core::NetCurrent>,
    /// The parts the person said may hang over the board edge, by reference designator
    /// (ADR 0045, "Built: overhang"). [`Board::check_placement`] does not report the
    /// courtyard of such a part past an edge it may hang over.
    pub overhang: BTreeMap<String, Overhang>,
    /// The parts whose place the design's requirements pin, by reference designator (ADR
    /// 0045): the placer did not move them, whatever its seed.
    pub pinned: std::collections::BTreeSet<String>,
}

/// The gap between two boxes, zero if they touch or overlap.
pub fn rect_distance(a: &Rect, b: &Rect) -> f64 {
    let dx = (a.min.x - b.max.x).max(b.min.x - a.max.x).max(0.0);
    let dy = (a.min.y - b.max.y).max(b.min.y - a.max.y).max(0.0);
    dx.hypot(dy)
}

/// The least distance between two segments that do not cross.
fn segment_distance(a: (Pt, Pt), b: (Pt, Pt)) -> f64 {
    let to_segment = |p: Pt, (s, e): (Pt, Pt)| {
        let d = e - s;
        let len2 = d.x * d.x + d.y * d.y;
        let t = if len2 == 0.0 {
            0.0
        } else {
            (((p.x - s.x) * d.x + (p.y - s.y) * d.y) / len2).clamp(0.0, 1.0)
        };
        p.distance(Pt::new(s.x + d.x * t, s.y + d.y * t))
    };
    [
        to_segment(a.0, b),
        to_segment(a.1, b),
        to_segment(b.0, a),
        to_segment(b.1, a),
    ]
    .into_iter()
    .fold(f64::MAX, f64::min)
}

/// The least distance from a segment to a box, zero if it touches it.
fn segment_to_rect((p, q): (Pt, Pt), r: &Rect) -> f64 {
    let steps = ((p.distance(q) / 0.05).ceil() as usize).max(1);
    (0..=steps)
        .map(|k| {
            let t = k as f64 / steps as f64;
            let at = Pt::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t);
            let dx = (r.min.x - at.x).max(at.x - r.max.x).max(0.0);
            let dy = (r.min.y - at.y).max(at.y - r.max.y).max(0.0);
            dx.hypot(dy)
        })
        .fold(f64::MAX, f64::min)
}
