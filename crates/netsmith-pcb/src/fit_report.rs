//! The parts that do not fit, as data (`--format json`, issue #817): every pair of courtyards
//! that overlap, every part outside what is kept clear, every part that is bigger than the
//! outline, and every part's final place, where the message names the ten worst in text and
//! `placement-failure.txt` lists every part. [`DoesNotFit::report`] makes it.
//!
//! The figures are in mm (mm2 for an area), to four decimals, in the outline's frame: from its
//! top left corner, y down. A part is its reference designator and its member path.

use serde::Serialize;

use crate::board::Side;
use crate::miss::finite;
use crate::placement::DoesNotFit;

/// A part: its reference designator and the member path in the design.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartRef {
    pub refdes: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
}

/// A width and a height in mm.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// A part bigger than the outline in every turn it may take.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OversizePart {
    #[serde(flatten)]
    pub part: PartRef,
    pub kind: String,
    /// Whether the turn is the part's own to choose; if not, a layout file or a requirement
    /// fixed it, and only the turns it leaves were tried.
    pub free: bool,
    /// Each turn tried, in degrees, with the courtyard's size in it.
    pub turns: Vec<Turn>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Turn {
    pub rotation: f64,
    #[serde(flatten)]
    pub courtyard: Size,
}

/// Two courtyards that overlap where the placer ended.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Overlap {
    pub a: PartRef,
    pub b: PartRef,
    /// The area they share, in mm2.
    pub area: f64,
    /// The overlap's size along x and along y.
    #[serde(flatten)]
    pub size: Size,
    /// Where the overlap is.
    pub x: f64,
    pub y: f64,
    /// The side of the outline it is nearest (`left`, `right`, `top` or `bottom`) and how far.
    pub nearest_edge: &'static str,
    pub nearest_distance: f64,
}

/// A part outside the outline, inside its margin, or in a keepout where the placer ended.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Outside {
    #[serde(flatten)]
    pub part: PartRef,
    /// What is wrong with each direction, as a sentence.
    pub why: Vec<String>,
    /// The worst of them in mm; zero where nothing is past a line (the reference text, or a
    /// requirement).
    pub worst: f64,
}

/// One part where the placer ended with it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartPlace {
    #[serde(flatten)]
    pub part: PartRef,
    pub kind: String,
    /// `top` or `bottom`.
    pub side: &'static str,
    /// The footprint's origin in mm from the outline's top left corner.
    pub x: f64,
    pub y: f64,
    /// Degrees, counterclockwise on screen.
    pub rotation: f64,
    /// The courtyard as it lies.
    pub courtyard: Size,
    /// Whether a layout file or a requirement pins it, so the placer did not move it.
    pub fixed: bool,
    /// What is wrong with it, as sentences; empty if it is where it should be.
    pub state: Vec<String>,
}

/// What a refusal because the parts do not fit says, as data.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FitReport {
    pub outline: Size,
    /// What the rows of courtyards need, with the margin; where the refusal is certain, so
    /// that this is a measure of by how much, and also where it is not (then it is only where
    /// the rows start, and `certain` is false).
    pub needs: Size,
    /// The space kept between a courtyard and the board edge, in mm.
    pub margin: f64,
    /// The sum of the courtyards' areas and what both faces of the outline hold inside the
    /// margin, in mm2.
    pub courtyard_area: f64,
    pub available_area: f64,
    /// Whether the courtyards alone are more than the faces hold, which no arrangement can
    /// fix.
    pub certain: bool,
    /// How many times the placer tried; absent where it did not try.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tries: Option<usize>,
    /// Parts bigger than the outline in every turn.
    pub oversize: Vec<OversizePart>,
    /// Every pair of courtyards that overlap, the most overlapped first.
    pub overlapping: Vec<Overlap>,
    /// Every part outside what is kept clear, the furthest first.
    pub outside: Vec<Outside>,
    /// The relations between two parts not held where the placer ended, as sentences.
    pub relations: Vec<String>,
    /// Sets of lines of the physical file that cannot all hold, as sentences.
    pub conflicts: Vec<String>,
    /// Every part where the placer ended with it, in the order of the design.
    pub parts: Vec<PartPlace>,
}

impl DoesNotFit {
    /// The refusal as data: what [`Display`](std::fmt::Display) and
    /// [`full_report`](DoesNotFit::full_report) say, without the sentences' cut at ten.
    pub fn report(&self) -> FitReport {
        let paths = self.ended.as_ref().map(|e| &e.paths);
        let part_ref = |refdes: &str, path: Option<&str>| PartRef {
            refdes: refdes.to_string(),
            path: path
                .map(str::to_string)
                .or_else(|| paths.and_then(|p| p.get(refdes).cloned()))
                .or_else(|| {
                    self.parts
                        .iter()
                        .find(|p| p.part == refdes)
                        .map(|p| p.path.clone())
                })
                .unwrap_or_default(),
        };
        let size = |(width, height): (f64, f64)| Size {
            width: finite(width).unwrap_or_default(),
            height: finite(height).unwrap_or_default(),
        };
        let num = |v: f64| finite(v).unwrap_or_default();
        FitReport {
            outline: size(self.outline),
            needs: size(self.needs),
            margin: num(self.wall),
            courtyard_area: num(self.area.0),
            available_area: num(self.area.1),
            certain: self.area.0 > self.area.1 + 1e-9,
            tries: self.ended.as_ref().map(|e| e.tries),
            oversize: self
                .oversize
                .iter()
                .map(|o| OversizePart {
                    part: part_ref(&o.part, Some(&o.path)),
                    kind: o.kind.clone(),
                    free: o.free,
                    turns: o
                        .tried
                        .iter()
                        .map(|&(rotation, courtyard)| Turn {
                            rotation: num(rotation),
                            courtyard: size(courtyard),
                        })
                        .collect(),
                })
                .collect(),
            overlapping: self
                .ended
                .iter()
                .flat_map(|e| &e.overlapping)
                .map(|o| Overlap {
                    a: part_ref(&o.a, None),
                    b: part_ref(&o.b, None),
                    area: num(o.area),
                    size: size(o.size),
                    x: num(o.at.0),
                    y: num(o.at.1),
                    nearest_edge: o.nearest.0,
                    nearest_distance: num(o.nearest.1),
                })
                .collect(),
            outside: self
                .ended
                .iter()
                .flat_map(|e| &e.outside_parts)
                .map(|o| Outside {
                    part: part_ref(&o.part, None),
                    why: o.why.clone(),
                    worst: num(o.worst),
                })
                .collect(),
            relations: self
                .ended
                .iter()
                .flat_map(|e| e.relations.iter().cloned())
                .collect(),
            conflicts: self
                .requirements
                .iter()
                .flat_map(|r| r.conflicts.iter().cloned())
                .collect(),
            parts: self
                .parts
                .iter()
                .map(|p| PartPlace {
                    part: part_ref(&p.part, Some(&p.path)),
                    kind: p.kind.clone(),
                    side: if p.side == Side::Back {
                        "bottom"
                    } else {
                        "top"
                    },
                    x: num(p.at.0),
                    y: num(p.at.1),
                    rotation: num(p.rotation),
                    courtyard: size(p.size),
                    fixed: p.fixed,
                    state: p.state.clone(),
                })
                .collect(),
        }
    }
}
