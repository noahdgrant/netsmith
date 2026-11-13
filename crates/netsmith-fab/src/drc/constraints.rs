//! The requirements a design puts on where its parts go (ADR 0045), measured on the board
//! file: for each one, whether it is satisfied, and by how much.
//!
//! A requirement names the footprints it is about (by reference designator; a module's lists
//! all of its parts) and a field of the body's box. The body is the box of the footprint's
//! drawing on the fabrication layer (`F.Fab`, or `B.Fab` on the back), or its courtyard if
//! it draws none, or its pads and [`BARE_COURTYARD_MARGIN`] if it has neither, found in the
//! footprint's own frame and turned with it, as the placer finds it; a finding says which was
//! used. Positions
//! are in the frame of the layout file: the origin is the top left corner of the box round
//! `Edge.Cuts`, x grows to the right and y downward. `rotation` is the footprint's turn as
//! `.place.rotation` says it, in degrees counterclockwise on screen ([`Footprint::turn`]: the
//! file's angle, less a half turn on the back), and `side` is the face it is on.
//!
//! [`Footprint::turn`]: crate::artwork::Footprint::turn
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

/// One end of a [`Relation`]: a field of one part's body.
#[derive(Debug, Clone, PartialEq)]
pub struct RelationEnd {
    /// The reference designator of the footprint.
    pub part: String,
    /// The part's path in the design, which the message names beside the designator.
    pub path: String,
    /// What is measured: along x (`X`, `Left`, `Right`) or along y (`Y`, `Top`, `Bottom`).
    pub measure: Measure,
}

/// A requirement on two parts together (ADR 0045, section 3): the measure of `b` minus the
/// measure of `a`, along one axis, is in `lo..=hi`. The two are measured on the body of each
/// footprint, so no outline is needed.
#[derive(Debug, Clone, PartialEq)]
pub struct Relation {
    /// What the design calls it: `radio.row_b.place.x - radio.row_a.place.x`.
    pub id: String,
    /// Where the design states it: `widget.physical.nsm:14`; empty if not known.
    pub source: String,
    pub a: RelationEnd,
    pub b: RelationEnd,
    pub lo: f64,
    pub hi: f64,
}

/// Every requirement of a board, and how a violation counts.
#[derive(Debug, Clone, PartialEq)]
pub struct Constraints {
    pub list: Vec<Constraint>,
    pub sides: Vec<SideRule>,
    /// Requirements on two parts together.
    pub relations: Vec<Relation>,
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

/// How far the courtyard of a footprint that draws none goes past its pads, in mm: the
/// placer's courtyard for it (`netsmith_pcb::Footprint::courtyard`) and the body the check
/// measures are its pads and this margin (#457).
pub const BARE_COURTYARD_MARGIN: f64 = 0.25;

/// The box of a footprint's body and what it is made from: the drawing on the fabrication
/// layer, else on the courtyard layer, else the pads and [`BARE_COURTYARD_MARGIN`]; a box
/// made from a courtyard takes in every hole that reaches past it. It is found in the
/// footprint's own frame and then turned onto the board, as the placer finds it
/// (`netsmith_pcb::Placed::body`), so that the two measure one box (#457).
pub fn body_of(art: &Artwork, part: &str, back: bool) -> Option<(Bounds, String)> {
    let (at, angle) = art
        .footprints
        .iter()
        .find(|f| f.part == part)
        .map_or((Pt::default(), 0.0), |f| (f.at, f.angle));
    let local = |p: Pt| (p - at).rotate(-angle);
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
        Bounds::of(points.into_iter().map(local))
    };
    let pads: Vec<&crate::artwork::Pad> = art.pads.iter().filter(|p| p.part == part).collect();
    let on_board = |b: Bounds| {
        Bounds::of(
            [
                b.min,
                Pt::new(b.max.x, b.min.y),
                b.max,
                Pt::new(b.min.x, b.max.y),
            ]
            .map(|c| at + c.rotate(angle)),
        )
        .expect("a box has corners")
    };
    let with_holes = |mut b: Bounds| {
        for p in &pads {
            if let Some(d) = &p.drill {
                let (c, reach) = (local(p.at), d.width.max(d.length) / 2.0);
                b.add(Pt::new(c.x - reach, c.y - reach));
                b.add(Pt::new(c.x + reach, c.y + reach));
            }
        }
        b
    };
    let (fab, yard) = if back {
        ("B.Fab", "B.CrtYd")
    } else {
        ("F.Fab", "F.CrtYd")
    };
    if let Some(b) = draws(fab) {
        return Some((on_board(b), fab.to_string()));
    }
    if let Some(b) = draws(yard) {
        return Some((on_board(with_holes(b)), yard.to_string()));
    }
    let boxes = pads.iter().flat_map(|p| {
        let mut turned = (*p).clone();
        turned.at = local(p.at);
        turned.angle = p.angle - angle;
        let b = crate::pads::outline(&turned).bounds();
        [b.min, b.max]
    });
    let b = Bounds::of(boxes)?;
    let grown = Bounds {
        min: Pt::new(
            b.min.x - BARE_COURTYARD_MARGIN,
            b.min.y - BARE_COURTYARD_MARGIN,
        ),
        max: Pt::new(
            b.max.x + BARE_COURTYARD_MARGIN,
            b.max.y + BARE_COURTYARD_MARGIN,
        ),
    };
    Some((
        on_board(with_holes(grown)),
        format!("pads and {BARE_COURTYARD_MARGIN} mm"),
    ))
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
        for r in &self.relations {
            out.push(relation_of(art, r));
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
            let r = fp.turn();
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

/// What a body's box measures for `measure`, along its axis, in the frame of the sheet.
fn along(body: &Bounds, measure: Measure) -> f64 {
    match measure {
        Measure::X => (body.min.x + body.max.x) / 2.0,
        Measure::Y => (body.min.y + body.max.y) / 2.0,
        Measure::Left => body.min.x,
        Measure::Right => body.max.x,
        Measure::Top => body.min.y,
        Measure::Bottom => body.max.y,
        Measure::Rotation => 0.0,
    }
}

fn relation_of(art: &Artwork, r: &Relation) -> Outcome {
    let name = if r.source.is_empty() {
        r.id.clone()
    } else {
        format!("{} ({})", r.id, r.source)
    };
    let parts = vec![r.b.part.clone(), r.a.part.clone()];
    let unmeasurable = |why: String| Outcome {
        id: r.id.clone(),
        source: r.source.clone(),
        satisfied: false,
        parts: parts.clone(),
        at: Pt::default(),
        measured: None,
        allowed: (r.lo, r.hi),
        over: f64::INFINITY,
        margin: 0.0,
        body: String::new(),
        message: why,
    };
    let find = |end: &RelationEnd| {
        let back = art
            .footprints
            .iter()
            .find(|f| f.part == end.part)
            .is_some_and(|f| f.back);
        body_of(art, &end.part, back)
    };
    let (Some((body_a, layer_a)), Some((body_b, layer_b))) = (find(&r.a), find(&r.b)) else {
        let missing: Vec<&str> = [&r.a, &r.b]
            .into_iter()
            .filter(|e| find(e).is_none())
            .map(|e| e.part.as_str())
            .collect();
        return unmeasurable(format!(
            "constraint {name} is violated: {} is not on the board",
            missing.join(" and ")
        ));
    };
    let value = along(&body_b, r.b.measure) - along(&body_a, r.a.measure);
    let over = (r.lo - value).max(value - r.hi).max(0.0);
    let satisfied = over <= super::checks::EPS;
    let margin = if satisfied {
        (value - r.lo).min(r.hi - value)
    } else {
        0.0
    };
    let who = |end: &RelationEnd, layer: &str| {
        format!(
            "{} of the body of {} (`{}`, {layer})",
            end.measure.what(),
            end.part,
            end.path
        )
    };
    let axis = if matches!(r.b.measure, Measure::X | Measure::Left | Measure::Right) {
        "x"
    } else {
        "y"
    };
    let centre = |b: &Bounds| Pt::new((b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0);
    let (ca, cb) = (centre(&body_a), centre(&body_b));
    Outcome {
        id: r.id.clone(),
        source: r.source.clone(),
        satisfied,
        parts,
        at: Pt::new((ca.x + cb.x) / 2.0, (ca.y + cb.y) / 2.0),
        measured: Some(value),
        allowed: (r.lo, r.hi),
        over,
        margin,
        body: format!("{layer_b}, {layer_a}"),
        message: format!(
            "constraint {name} is {}: {} minus {} is {} mm along {axis}, and it allows {}{}",
            if satisfied { "satisfied" } else { "violated" },
            who(&r.b, &layer_b),
            who(&r.a, &layer_a),
            n(value),
            range_text(r.lo, r.hi, "mm"),
            if satisfied {
                String::new()
            } else {
                format!(" ({} mm over)", n(over))
            }
        ),
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
