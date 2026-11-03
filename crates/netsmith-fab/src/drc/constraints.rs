//! The requirements a design puts on where its parts go (ADR 0045), measured on the board
//! file: for each one, whether it is satisfied, and by how much.
//!
//! A requirement names the footprints it is about (by reference designator; a module's lists
//! all of its parts) and a field of the body's box. The body is the box of the footprint's
//! drawing on the fabrication layer (`F.Fab`, or `B.Fab` on the back), or its courtyard if
//! it draws none, or its pads if it has neither; a finding says which was used. Positions
//! are in the frame of the layout file: the origin is the top left corner of the box round
//! `Edge.Cuts`, x grows to the right and y downward. `rotation` is the footprint's angle in
//! degrees, counterclockwise on screen, and `side` is the face it is on.
//!
//! This is netsmith's own rule (`constraint`): KiCad has none. The requirements are not in
//! the board file, so the caller that made the board says them ([`Constraints`]).

use super::{Finding, Item, Rule, Severity};
use crate::artwork::{Artwork, Shape};
use crate::geom::{Bounds, Pt};

/// What a requirement measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Measure {
    /// The centre of the body, along x or y.
    X,
    Y,
    /// An edge of the body's box.
    Left,
    Right,
    Top,
    Bottom,
    /// The footprint's turn in degrees.
    Rotation,
}

impl Measure {
    pub fn word(self) -> &'static str {
        match self {
            Measure::X => "x",
            Measure::Y => "y",
            Measure::Left => "left",
            Measure::Right => "right",
            Measure::Top => "top",
            Measure::Bottom => "bottom",
            Measure::Rotation => "rotation",
        }
    }

    fn unit(self) -> &'static str {
        if self == Measure::Rotation {
            "degrees"
        } else {
            "mm"
        }
    }

    /// What is measured, as a person says it.
    fn what(self) -> &'static str {
        match self {
            Measure::X | Measure::Y => "the centre",
            Measure::Left => "the left edge",
            Measure::Right => "the right edge",
            Measure::Top => "the top edge",
            Measure::Bottom => "the bottom edge",
            Measure::Rotation => "the turn",
        }
    }
}

/// One requirement. `lo` and `hi` are the range the field may take (infinite when open).
#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    /// What the design calls it: `usb.place.left`.
    pub id: String,
    /// Where the design states it: `widget.nsm:14`; empty if not known.
    pub source: String,
    /// The footprints it is about, by reference designator. A module's has all its parts and
    /// is measured on the box round all of their bodies.
    pub parts: Vec<String>,
    pub measure: Measure,
    pub lo: f64,
    pub hi: f64,
}

/// A requirement on the face a footprint is on.
#[derive(Debug, Clone, PartialEq)]
pub struct SideRule {
    pub id: String,
    pub source: String,
    pub part: String,
    /// `top` or `bottom`.
    pub side: String,
}

/// Every requirement of a board, and how a violation counts.
#[derive(Debug, Clone, PartialEq)]
pub struct Constraints {
    pub list: Vec<Constraint>,
    pub sides: Vec<SideRule>,
    /// What a violation is: an error stops `fab`, a warning is listed (`constraints = "warn"`).
    pub severity: Severity,
}

/// One requirement measured.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub id: String,
    pub source: String,
    pub satisfied: bool,
    /// The reference designators it is about, and where the body is now (its centre, on the
    /// sheet, for zooming to it).
    pub parts: Vec<String>,
    pub at: Pt,
    /// What was measured (mm or degrees); not a number for a face, or a part that is not on
    /// the board.
    pub measured: Option<f64>,
    pub allowed: (f64, f64),
    /// By how much it misses (0 if satisfied), and, if satisfied, the room left to the
    /// nearer end of the range.
    pub over: f64,
    pub margin: f64,
    /// Which drawing was used for the body: `F.Fab`, `F.CrtYd` or `pads`.
    pub body: String,
    /// The sentence: what was measured and what is allowed, with the numbers.
    pub message: String,
}

fn n(v: f64) -> String {
    let r = (v * 1e4).round() / 1e4;
    format!("{}", if r == 0.0 { 0.0 } else { r })
}

/// A range as a person says it.
pub fn range_text(lo: f64, hi: f64, unit: &str) -> String {
    match (lo.is_finite(), hi.is_finite()) {
        (true, true) if (hi - lo).abs() < 1e-9 => format!("{} {unit}", n(lo)),
        (true, true) => format!("{} to {} {unit}", n(lo), n(hi)),
        (true, false) => format!("at least {} {unit}", n(lo)),
        (false, true) => format!("at most {} {unit}", n(hi)),
        (false, false) => format!("any {unit}"),
    }
}

/// The box of a footprint's body and the layer it is drawn on.
fn body_of(art: &Artwork, part: &str, back: bool) -> Option<(Bounds, String)> {
    let draws = |layer: &str| -> Option<Bounds> {
        let mut points: Vec<Pt> = Vec::new();
        for g in art
            .graphics
            .iter()
            .filter(|g| g.layer == layer && g.owner == part)
        {
            match &g.shape {
                Shape::Line(a, b) => points.extend([*a, *b]),
                Shape::Arc(arc) => points.extend(arc.extent_points()),
                Shape::Circle { centre, radius } => points.extend([
                    Pt::new(centre.x - radius, centre.y - radius),
                    Pt::new(centre.x + radius, centre.y + radius),
                ]),
                Shape::Poly(p) => points.extend(p.iter().copied()),
            }
        }
        Bounds::of(points)
    };
    let (fab, yard) = if back {
        ("B.Fab", "B.CrtYd")
    } else {
        ("F.Fab", "F.CrtYd")
    };
    if let Some(b) = draws(fab) {
        return Some((b, fab.to_string()));
    }
    if let Some(b) = draws(yard) {
        return Some((b, yard.to_string()));
    }
    let pads = art.pads.iter().filter(|p| p.part == part).flat_map(|p| {
        [
            Pt::new(p.at.x - p.size.0 / 2.0, p.at.y - p.size.1 / 2.0),
            Pt::new(p.at.x + p.size.0 / 2.0, p.at.y + p.size.1 / 2.0),
        ]
    });
    Bounds::of(pads).map(|b| (b, "pads".to_string()))
}

/// The box round `Edge.Cuts`: the frame the requirements are measured in.
fn outline_of(art: &Artwork) -> Option<Bounds> {
    let mut points: Vec<Pt> = Vec::new();
    for g in art
        .graphics
        .iter()
        .filter(|g| g.layer == "Edge.Cuts" && g.owner.is_empty())
    {
        match &g.shape {
            Shape::Line(a, b) => points.extend([*a, *b]),
            Shape::Arc(arc) => points.extend(arc.extent_points()),
            Shape::Circle { centre, radius } => points.extend([
                Pt::new(centre.x - radius, centre.y - radius),
                Pt::new(centre.x + radius, centre.y + radius),
            ]),
            Shape::Poly(p) => points.extend(p.iter().copied()),
        }
    }
    Bounds::of(points)
}

impl Constraints {
    /// Measure every requirement on the board. A requirement whose footprint is not on the
    /// board, or on a board with no outline, is not satisfied.
    pub fn measure(&self, art: &Artwork) -> Vec<Outcome> {
        let outline = outline_of(art);
        let mut out = Vec::new();
        for c in &self.list {
            out.push(self.one(art, outline, c));
        }
        for s in &self.sides {
            out.push(side_of(art, s));
        }
        out
    }

    fn one(&self, art: &Artwork, outline: Option<Bounds>, c: &Constraint) -> Outcome {
        let unmeasurable = |why: String| Outcome {
            id: c.id.clone(),
            source: c.source.clone(),
            satisfied: false,
            parts: c.parts.clone(),
            at: Pt::default(),
            measured: None,
            allowed: (c.lo, c.hi),
            over: f64::INFINITY,
            margin: 0.0,
            body: String::new(),
            message: why,
        };
        let name = |s: &str| {
            if c.source.is_empty() {
                s.to_string()
            } else {
                format!("{s} ({})", c.source)
            }
        };
        if c.measure == Measure::Rotation {
            let Some(fp) = c
                .parts
                .first()
                .and_then(|p| art.footprints.iter().find(|f| &f.part == p))
            else {
                return unmeasurable(format!(
                    "constraint {} is violated: {} is not on the board",
                    name(&c.id),
                    c.parts.join(", ")
                ));
            };
            let r = fp.angle.rem_euclid(360.0);
            // The turn nearest the range, counting whole turns.
            let over = [r - 360.0, r, r + 360.0]
                .into_iter()
                .map(|t| (c.lo - t).max(t - c.hi).max(0.0))
                .fold(f64::INFINITY, f64::min);
            let satisfied = over <= super::checks::EPS;
            let margin = if satisfied {
                [r - 360.0, r, r + 360.0]
                    .into_iter()
                    .map(|t| (t - c.lo).min(c.hi - t))
                    .fold(f64::NEG_INFINITY, f64::max)
            } else {
                0.0
            };
            return Outcome {
                id: c.id.clone(),
                source: c.source.clone(),
                satisfied,
                parts: c.parts.clone(),
                at: fp.at,
                measured: Some(r),
                allowed: (c.lo, c.hi),
                over,
                margin,
                body: String::new(),
                message: format!(
                    "constraint {} is {}: {} is turned {} degrees, and it allows {}{}",
                    name(&c.id),
                    if satisfied { "satisfied" } else { "violated" },
                    c.parts[0],
                    n(r),
                    range_text(c.lo, c.hi, "degrees"),
                    if satisfied {
                        String::new()
                    } else {
                        format!(" ({} degrees over)", n(over))
                    }
                ),
            };
        }
        let Some(outline) = outline else {
            return unmeasurable(format!(
                "constraint {} cannot be measured: the board has no outline (Edge.Cuts) to measure from",
                name(&c.id)
            ));
        };
        let mut boxes: Vec<(Bounds, String)> = Vec::new();
        for p in &c.parts {
            let back = art
                .footprints
                .iter()
                .find(|f| &f.part == p)
                .is_some_and(|f| f.back);
            match body_of(art, p, back) {
                Some(b) => boxes.push(b),
                None => {
                    return unmeasurable(format!(
                        "constraint {} is violated: {p} is not on the board",
                        name(&c.id)
                    ));
                }
            }
        }
        let Some(mut body) = boxes.first().map(|b| b.0) else {
            return unmeasurable(format!(
                "constraint {} is about a module with no part on the board",
                name(&c.id)
            ));
        };
        for (b, _) in &boxes[1..] {
            body.add(b.min);
            body.add(b.max);
        }
        let drawn: Vec<&str> = {
            let mut layers: Vec<&str> = boxes.iter().map(|b| b.1.as_str()).collect();
            layers.dedup();
            layers
        };
        let layer = drawn.join(", ");
        let value = match c.measure {
            Measure::X => (body.min.x + body.max.x) / 2.0 - outline.min.x,
            Measure::Y => (body.min.y + body.max.y) / 2.0 - outline.min.y,
            Measure::Left => body.min.x - outline.min.x,
            Measure::Right => body.max.x - outline.min.x,
            Measure::Top => body.min.y - outline.min.y,
            Measure::Bottom => body.max.y - outline.min.y,
            Measure::Rotation => unreachable!("handled above"),
        };
        let over = (c.lo - value).max(value - c.hi).max(0.0);
        let satisfied = over <= super::checks::EPS;
        let margin = if satisfied {
            (value - c.lo).min(c.hi - value)
        } else {
            0.0
        };
        let who = if c.parts.len() == 1 {
            format!("the body of {} ({layer})", c.parts[0])
        } else {
            format!("the box round the bodies of {}", c.parts.join(", "))
        };
        let message = format!(
            "constraint {} is {}: {} of {who} is {} mm {}, and it allows {}{}",
            name(&c.id),
            if satisfied { "satisfied" } else { "violated" },
            c.measure.what(),
            n(value),
            match c.measure {
                Measure::X | Measure::Left | Measure::Right => "from the left edge of the outline",
                _ => "from the top edge of the outline",
            },
            range_text(c.lo, c.hi, c.measure.unit()),
            if satisfied {
                String::new()
            } else {
                format!(" ({} mm over)", n(over))
            }
        );
        Outcome {
            id: c.id.clone(),
            source: c.source.clone(),
            satisfied,
            parts: c.parts.clone(),
            at: Pt::new(
                (body.min.x + body.max.x) / 2.0,
                (body.min.y + body.max.y) / 2.0,
            ),
            measured: Some(value),
            allowed: (c.lo, c.hi),
            over,
            margin,
            body: layer,
            message,
        }
    }

    /// The findings for the requirements that are not satisfied, errors or warnings as the
    /// caller chose, in the order of the requirements.
    pub fn findings(&self, art: &Artwork) -> Vec<Finding> {
        outcome_findings(&self.measure(art), self.severity)
    }
}

fn side_of(art: &Artwork, s: &SideRule) -> Outcome {
    let found = art.footprints.iter().find(|f| f.part == s.part);
    let (satisfied, now, at) = match found {
        Some(f) => (
            (s.side == "bottom") == f.back,
            if f.back { "bottom" } else { "top" },
            f.at,
        ),
        None => (false, "not on the board", Pt::default()),
    };
    let name = if s.source.is_empty() {
        s.id.clone()
    } else {
        format!("{} ({})", s.id, s.source)
    };
    Outcome {
        id: s.id.clone(),
        source: s.source.clone(),
        satisfied,
        parts: vec![s.part.clone()],
        at,
        measured: None,
        allowed: (0.0, 0.0),
        over: if satisfied { 0.0 } else { f64::INFINITY },
        margin: 0.0,
        body: String::new(),
        message: format!(
            "constraint {name} is {}: {} is on the {now}, and it must be on the {}",
            if satisfied { "satisfied" } else { "violated" },
            s.part,
            s.side
        ),
    }
}

/// The findings for the outcomes that are violated.
pub fn outcome_findings(outcomes: &[Outcome], severity: Severity) -> Vec<Finding> {
    if severity == Severity::Ignore {
        return Vec::new();
    }
    outcomes
        .iter()
        .filter(|o| !o.satisfied)
        .map(|o| Finding {
            rule: Rule::Constraint,
            severity,
            message: o.message.clone(),
            at: o.at,
            layer: None,
            items: o
                .parts
                .iter()
                .map(|p| Item {
                    what: format!("Footprint {p}"),
                    at: o.at,
                    part: Some(p.clone()),
                    pad: None,
                    net: None,
                })
                .collect(),
        })
        .collect()
}
