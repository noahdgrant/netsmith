//! Why a placement cannot be met, as data (`--format json`, `board3d --json`, issue #779): each
//! set of lines of the physical file and locks of the layout file that cannot all hold
//! ([`Conflict`], what [`explain`](crate::explain) proves), and each fault the placement check
//! finds on a placed board ([`PlacementFault`], what [`Board::check_placement`] finds). The
//! sentence a person reads says the same; this is for a program that reads it, so that an agent
//! that moves parts to clear a conflict reads the parts, the boxes and the lines, not the text.
//!
//! The figures are in mm (mm2 for an area), to four decimals, in the outline's frame: from its
//! top left corner, y down, as the layout file has them.

use serde::Serialize;

use crate::board::{Board, HoleNeighbour, PlacementProblem, Side};
use crate::fit_report::{PartRef, Size};
use crate::geometry::{Pt, Rect};
use crate::miss::finite;
use crate::placement::ORIGIN;

/// A box in the outline's frame: from its top left corner, y down, in mm.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Span {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
}

impl Span {
    /// The box of `r`, a box on the sheet, with `origin` the outline's top left corner.
    pub(crate) fn of(r: &Rect, origin: Pt) -> Span {
        let at = |v: f64| finite(v).unwrap_or_default();
        Span {
            x_min: at(r.min.x - origin.x),
            x_max: at(r.max.x - origin.x),
            y_min: at(r.min.y - origin.y),
            y_max: at(r.max.y - origin.y),
        }
    }
}

/// A part that a conflict or a fault is about, with the box its courtyard takes: for a fault,
/// where the courtyard is; for a conflict, the box the courtyard lies within wherever the lines
/// put it. None where the finding is not about where the part is (two relations with no value
/// in common).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartBox {
    #[serde(flatten)]
    pub part: PartRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub courtyard: Option<Span>,
}

/// One line that takes part in a conflict: a line of the physical file, or the layout file's
/// lock of a part.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConflictLine {
    /// `requirement` (a line of the physical file) or `lock` (an entry of the layout file with
    /// `lock = true`).
    pub kind: &'static str,
    /// What the design calls the requirement (`r1.place.x`, `.r2.place.x - .r1.place.x`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Where the line is (`main.physical.nsm:13`), when the design knows it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The text of the line, when the design knows it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// For a lock: the part, and where the file locks its footprint's origin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<PartRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
}

/// A keepout or a cutout that a part's courtyard overlaps wherever its lines put it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConflictArea {
    /// `keepout` or `cutout`.
    pub kind: &'static str,
    pub name: String,
    #[serde(flatten)]
    pub span: Span,
    /// The face whose parts it keeps out (`top` or `bottom`); none where it keeps out both.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face: Option<&'static str>,
}

/// The outline a conflict is proved inside: the lines alone leave a window, and the outline
/// with the margin the placer keeps clear of its edge bounds it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConflictOutline {
    pub width: f64,
    pub height: f64,
    /// The space kept between a courtyard and the edge, in mm.
    pub margin: f64,
    /// Whether the outline is the most a range of sizes allows, so that the proof holds for
    /// every size in it.
    pub at_most: bool,
}

/// A set of lines that cannot all hold, proved from what they leave (ADR 0045 sections 13 and
/// 14).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Conflict {
    /// What is proved:
    /// - `relations`: two relations between the same two parts with no value in common;
    /// - `relation`: a relation that the places its two parts' other lines leave cannot meet;
    /// - `overlap`: two courtyards that overlap wherever the lines put them;
    /// - `keepout` or `cutout`: a courtyard that overlaps one (`area`) wherever it is put;
    /// - `over_pins`: a courtyard over a pad of a through-hole part wherever it is put, which
    ///   the pins' protrusion does not excuse;
    /// - `outside`: no place the lines leave keeps the courtyard inside the outline;
    /// - `past_edge`: the lines put the courtyard past an edge (`past`) that nobody stated it
    ///   may hang over, wherever they put it;
    /// - `too_big`: the courtyard is bigger than the outline leaves in every turn the part may
    ///   take (`turns`, `outline`).
    pub kind: &'static str,
    /// The lines and locks that cannot all hold, each once, in the order the sentence names
    /// them.
    pub lines: Vec<ConflictLine>,
    /// The parts, each with the box its courtyard lies within wherever the lines put it.
    pub parts: Vec<PartBox>,
    /// The least overlap along x (`width`) and y (`height`), for `overlap`, `keepout`, `cutout`
    /// and `over_pins`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlap: Option<Size>,
    /// The keepout or cutout, for `keepout` and `cutout`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area: Option<ConflictArea>,
    /// How far apart, in mm: the gap between the two windows of `relations`, or how far out of
    /// its window a `relation` is at best.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<f64>,
    /// The outline, where the proof needs it (the lines alone would leave a place).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline: Option<ConflictOutline>,
    /// How far the courtyard is past each edge nobody stated it may hang over, at best
    /// (`past_edge`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub past: Vec<EdgePast>,
    /// The size of the part's courtyard in each turn tried (`too_big`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub turns: Vec<Turn>,
    /// The sentence a person reads for it. Not in the JSON: the finding carries the message.
    #[serde(skip)]
    pub text: String,
}

/// The size of a courtyard turned by `angle` degrees.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Turn {
    pub angle: f64,
    pub width: f64,
    pub height: f64,
}

/// One fault the placement check finds on a placed board.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlacementFault {
    /// The check's name for it, as the editor's findings have it: `overlap`, `over_pins`,
    /// `outside_outline`, `pad_near_edge`, `in_cutout`, `pad_near_cutout`, `in_keepout`,
    /// `pad_in_keepout`, `too_tall`, `cutout_outside_outline`, `no_position` or
    /// `hole_too_near`.
    pub kind: &'static str,
    /// The parts it is about, each with its courtyard where it stands.
    pub parts: Vec<PartBox>,
    /// The area two courtyards share, in mm2 (`overlap`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area: Option<f64>,
    /// The box the two courtyards share (`overlap`, `over_pins`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared: Option<Span>,
    /// How far the courtyard is past each edge nobody stated it may hang over
    /// (`outside_outline`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub past: Vec<EdgePast>,
    /// A distance in mm: a pad from the edge or a cutout (negative when outside it), a hole from
    /// its neighbour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance: Option<f64>,
    /// The least the rules allow for `distance` (`hole_too_near`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub least: Option<f64>,
    /// The pads, by number, of `hole_too_near`: the hole's, then its neighbour's.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pads: Vec<String>,
    /// What the hole is near (`copper` or `hole`), for `hole_too_near`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub near: Option<&'static str>,
    /// The cutout or keepout it is about, by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cutout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keepout: Option<String>,
    /// The part's height and the limit, in mm (`too_tall`), and the face (`too_tall`,
    /// `over_pins`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face: Option<&'static str>,
    /// The sentence a person reads for it. Not in the JSON: the finding carries the message.
    #[serde(skip)]
    pub text: String,
}

/// How far a courtyard is past one edge of the outline.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EdgePast {
    /// `left`, `right`, `top` or `bottom`.
    pub edge: &'static str,
    pub by: f64,
}

pub(crate) fn face_word(side: Side) -> &'static str {
    if side == Side::Back { "bottom" } else { "top" }
}

impl PlacementProblem {
    /// The check's name for the fault, as the editor's findings and [`PlacementFault::kind`]
    /// have it.
    pub fn rule(&self) -> &'static str {
        match self {
            Self::Overlap { .. } => "overlap",
            Self::OverPins { .. } => "over_pins",
            Self::OutsideOutline { .. } => "outside_outline",
            Self::PadNearEdge { .. } => "pad_near_edge",
            Self::InCutout { .. } => "in_cutout",
            Self::PadNearCutout { .. } => "pad_near_cutout",
            Self::InKeepout { .. } => "in_keepout",
            Self::PadInKeepout { .. } => "pad_in_keepout",
            Self::TooTall { .. } => "too_tall",
            Self::CutoutOutsideOutline { .. } => "cutout_outside_outline",
            Self::NoPosition { .. } => "no_position",
            Self::HoleTooNear { .. } => "hole_too_near",
        }
    }
}

impl Board {
    /// The placement check's faults ([`Board::check_placement`]) as data, each with its
    /// sentence in `text`.
    pub fn placement_faults(&self) -> Vec<PlacementFault> {
        self.check_placement()
            .iter()
            .map(|p| self.fault_of(p))
            .collect()
    }

    /// One problem of the placement check as data.
    pub fn fault_of(&self, problem: &PlacementProblem) -> PlacementFault {
        let num = |v: f64| finite(v).unwrap_or_default();
        let origin = self.outline.min;
        let part = |refdes: &str| {
            let placed = self.part(refdes);
            PartBox {
                part: PartRef {
                    refdes: refdes.to_string(),
                    path: placed.map(|p| p.path.clone()).unwrap_or_default(),
                },
                courtyard: placed.map(|p| Span::of(&p.courtyard(), origin)),
            }
        };
        let shared = |a: &str, b: &str| {
            let (a, b) = (self.part(a)?.courtyard(), self.part(b)?.courtyard());
            let min = Pt::new(a.min.x.max(b.min.x), a.min.y.max(b.min.y));
            let max = Pt::new(a.max.x.min(b.max.x), a.max.y.min(b.max.y));
            (min.x <= max.x && min.y <= max.y).then(|| Span::of(&Rect::new(min, max), origin))
        };
        let mut fault = PlacementFault {
            kind: problem.rule(),
            parts: problem.parts().into_iter().map(part).collect(),
            area: None,
            shared: None,
            past: Vec::new(),
            distance: None,
            least: None,
            pads: Vec::new(),
            near: None,
            cutout: None,
            keepout: None,
            height: None,
            limit: None,
            face: None,
            text: problem.to_string(),
        };
        match problem {
            PlacementProblem::Overlap { a, b, area, .. } => {
                fault.area = Some(num(*area));
                fault.shared = shared(a, b);
            }
            PlacementProblem::OverPins {
                holder,
                cover,
                side,
                ..
            } => {
                fault.shared = shared(holder, cover);
                fault.face = Some(face_word(*side));
            }
            PlacementProblem::OutsideOutline { past, .. } => {
                fault.past = past
                    .iter()
                    .map(|(edge, by)| EdgePast {
                        edge: edge.word(),
                        by: num(*by),
                    })
                    .collect();
            }
            PlacementProblem::PadNearEdge { distance, .. } => fault.distance = Some(num(*distance)),
            PlacementProblem::InCutout { cutout, .. }
            | PlacementProblem::CutoutOutsideOutline { cutout } => {
                fault.cutout = Some(cutout.clone());
            }
            PlacementProblem::PadNearCutout {
                cutout, distance, ..
            } => {
                fault.cutout = Some(cutout.clone());
                fault.distance = Some(num(*distance));
            }
            PlacementProblem::InKeepout { keepout, .. }
            | PlacementProblem::PadInKeepout { keepout, .. } => {
                fault.keepout = Some(keepout.clone());
            }
            PlacementProblem::TooTall {
                keepout,
                height,
                limit,
                side,
                ..
            } => {
                fault.keepout = Some(keepout.clone());
                fault.height = Some(num(*height));
                fault.limit = Some(num(*limit));
                fault.face = Some(face_word(*side));
            }
            PlacementProblem::NoPosition { what, name } => {
                if *what == "cutout" {
                    fault.cutout = Some(name.clone());
                } else {
                    fault.keepout = Some(name.clone());
                }
            }
            PlacementProblem::HoleTooNear {
                pad,
                other_pad,
                to,
                distance,
                least,
                ..
            } => {
                fault.pads = vec![pad.clone(), other_pad.clone()];
                fault.near = Some(match to {
                    HoleNeighbour::Copper => "copper",
                    HoleNeighbour::Hole => "hole",
                });
                fault.distance = Some(num(*distance));
                fault.least = Some(num(*least));
            }
        }
        fault
    }
}

/// The box `r` on the sheet in the outline's frame of the requirements, whose top left corner is
/// [`ORIGIN`].
pub(crate) fn span_from_origin(r: &Rect) -> Span {
    Span::of(r, ORIGIN)
}
