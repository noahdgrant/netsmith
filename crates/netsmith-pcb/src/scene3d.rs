//! The board's _Scene_ (ADR 0105): one three-dimensional description of a placed board, built
//! from netsmith's own data, that `netsmith board3d` reports as numbers and that the editor's 3D
//! view, the `.glb` writer and the rasterizer will draw. [`Scene::of`] is a pure function of the
//! [`Board`] and the parts' _Height provenance_; it reads no file and needs no KiCad.
//!
//! # The frame
//!
//! Millimetres, right-handed, Z up:
//!
//! - the origin is the outline's bottom-left corner (as the layout file and the editor draw the
//!   board, with its top edge up), on the board's **top face**;
//! - x runs right along the outline's bottom edge, y up toward its top edge, z up out of the top
//!   face, so the top face is `z = 0` and the bottom face `z = -thickness`;
//! - a point of the layout file (mm from the outline's top-left corner, y down) is
//!   `(x, outline height - y)` here; a turn is the same number of degrees, counterclockwise
//!   seen from above, as the layout file has it.
//!
//! A part on the top face stands from `z = 0` up to its height; a part on the bottom face hangs
//! from `z = -thickness` down by its height. A face is the outside of its solder mask, which is
//! what a height is measured from (the glossary's _Catalog height_).
//!
//! # What it holds
//!
//! - The board solid: the rounded outline, the cutouts, every drilled hole (pads and vias,
//!   plated or not) and the layers of [`crate::stackup::layers`], the list the KiCad board file's
//!   stackup is written from, each at its Z.
//! - One [`Body`] per part with a body (a label has none): the body box (the footprint's
//!   `F.Fab` drawing, else its courtyard, as a placement requirement and the height check measure
//!   it, ADR 0045) turned and mirrored with the part, from its face to its height. A part whose
//!   height is unknown has none: it is never given zero or a guess (ADR 0094, ADR 0105).
//! - The pins of a through-hole part below the far face, where the design states their
//!   protrusion (ADR 0045 section 15); unknown otherwise.
//! - The height limits ([`HeightLimit`]): every placed keepout with `.max_height`, per face it
//!   limits, with the tallest part under it and the margin.
//! - [`Findings`] that are 3D facts: the parts too tall for a limit (the placement check's own
//!   `TooTall`, not measured again here), the parts of unknown height under a limit (the board's
//!   `heights_unchecked`), and bodies that overlap in 3D (two bodies on one face whose turned
//!   boxes share area in plan and whose heights both start at the face).
//!
//! Copper (pads, tracks, vias as rings and barrels, planes and pours, each with its net) is
//! added by the next phase of #430 as a field of [`Scene`] beside `board`; the ids it will carry
//! are the net names of [`Board::nets`].
//!
//! Everything is in a fixed order: bodies by part path, holes by part path then pad order then
//! vias in the board's order, limits by name, findings by path.

use std::collections::BTreeMap;

use crate::board::{Board, PlacementProblem, Side};
use crate::footprint::PadKind;
use crate::geometry::{Pt, Rect};
use crate::stackup::{LayerKind, StackLayer};

/// Where a part's height came from (the glossary's _Height provenance_), strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HeightProvenance {
    /// The source or the physical file states `.height`.
    Stated,
    /// The `height` cell of the catalog row the part was picked as (ADR 0094).
    Catalog,
    /// Neither: the part has no height.
    Unknown,
}

impl HeightProvenance {
    /// `stated`, `catalog` or `unknown`.
    pub fn word(self) -> &'static str {
        match self {
            HeightProvenance::Stated => "stated",
            HeightProvenance::Catalog => "catalog",
            HeightProvenance::Unknown => "unknown",
        }
    }
}

/// A point in the plan of the scene: x right, y up, in mm from the outline's bottom-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub x: f64,
    pub y: f64,
}

/// An axis-aligned box in plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanBox {
    pub min: Plan,
    pub max: Plan,
}

/// The board as its stackup makes it.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardSolid {
    pub name: String,
    /// The outline's size, in plan from `(0, 0)` to `(width, height)`.
    pub width: f64,
    pub height: f64,
    pub corner_radius: f64,
    /// The thickness the board chose, which the board house is asked for: the bottom face is at
    /// `-thickness`.
    pub thickness: f64,
    /// The sum of the layers' thicknesses, the process's own figures: it may differ from
    /// `thickness` by the process's rounding (a four-layer board).
    pub stack_thickness: f64,
    /// From the top face down.
    pub layers: Vec<SceneLayer>,
    pub cutouts: Vec<SceneCutout>,
    pub holes: Vec<Hole>,
}

/// A layer of the stackup at its height.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneLayer {
    pub name: String,
    pub kind: LayerKind,
    pub thickness: f64,
    pub z_top: f64,
    pub z_bottom: f64,
}

/// A rectangular hole through the board, with rounded corners.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneCutout {
    pub name: String,
    pub at: PlanBox,
    pub radius: f64,
}

/// What a drilled hole belongs to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum HoleOwner {
    /// A pad of a part: its path, designator and pad number (empty for a mounting hole's).
    Pad {
        path: String,
        refdes: String,
        pad: String,
    },
    /// A via, by its net.
    Via { net: String },
}

/// A drilled hole, through the whole board: a round hole has both ends at its centre, a slot
/// has the two ends of its axis.
#[derive(Debug, Clone, PartialEq)]
pub struct Hole {
    pub owner: HoleOwner,
    pub from: Plan,
    pub to: Plan,
    pub diameter: f64,
    pub plated: bool,
}

/// The pins of a through-hole part where they come out of the far face.
#[derive(Debug, Clone, PartialEq)]
pub enum Pins {
    /// The part has no plated hole.
    None,
    /// The design states no protrusion: how far the pins stand out is unknown, not zero.
    Unknown { at: PlanBox },
    /// They stand out by `length` (the lines in `source` state it), from `z_top` to `z_bottom`.
    Known {
        at: PlanBox,
        length: f64,
        source: String,
        z_top: f64,
        z_bottom: f64,
    },
}

/// A part's body in the scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    /// The part's path in the design: the id a layout file, a finding and the scene share.
    pub path: String,
    pub refdes: String,
    pub kind: String,
    pub side: Side,
    /// The footprint's origin and turn as the layout file writes them (from the outline's
    /// top-left corner, y down), to find the part in it.
    pub layout_at: (f64, f64),
    pub rotation: f64,
    /// The body box's four corners in plan, turned with the part.
    pub corners: [Plan; 4],
    /// The box that holds them, which the height check measures.
    pub bounds: PlanBox,
    /// Whether the box is the courtyard, because the footprint draws no fabrication outline.
    pub body_is_courtyard: bool,
    /// The face it stands on: `0` for the top face, `-thickness` for the bottom.
    pub z_face: f64,
    /// The far end of the body: above the top face or below the bottom. `None` when the height
    /// is unknown.
    pub z_far: Option<f64>,
    pub height: Option<f64>,
    pub provenance: HeightProvenance,
    pub pins: Pins,
}

impl Body {
    /// The lowest and the highest Z of the body, each `None` where the unknown height leaves it.
    pub fn z_range(&self) -> (Option<f64>, Option<f64>) {
        match self.side {
            Side::Back => (self.z_far, Some(self.z_face)),
            _ => (Some(self.z_face), self.z_far),
        }
    }
}

/// The tallest part under a limit, or on a face.
#[derive(Debug, Clone, PartialEq)]
pub struct Tallest {
    pub path: String,
    pub refdes: String,
    pub height: f64,
}

/// One face that a height limit limits.
#[derive(Debug, Clone, PartialEq)]
pub struct LimitFace {
    pub side: Side,
    /// The volume parts may fill: from the face out to the limit.
    pub z_bottom: f64,
    pub z_top: f64,
    /// The tallest part of known height whose body is under the limit on this face.
    pub tallest: Option<Tallest>,
    /// The limit less that part's height: negative when it is too tall.
    pub margin: Option<f64>,
    /// The paths of the parts under it on this face whose height is unknown.
    pub unknown: Vec<String>,
}

/// A keepout with a height limit, as a volume over each face it limits.
#[derive(Debug, Clone, PartialEq)]
pub struct HeightLimit {
    pub name: String,
    /// The path of the part that carries it, if one does (its zone is not measured against
    /// that part).
    pub carrier: Option<String>,
    pub at: PlanBox,
    pub max_height: f64,
    /// Top first. Empty for a limit on inner layers only, which limits no face.
    pub faces: Vec<LimitFace>,
}

/// A part taller than a limit it stands under: the placement check's `TooTall`.
#[derive(Debug, Clone, PartialEq)]
pub struct TooTall {
    pub path: String,
    pub refdes: String,
    pub keepout: String,
    pub height: f64,
    pub limit: f64,
    pub side: Side,
}

/// A part of unknown height under a limit: the board's `heights_unchecked`.
#[derive(Debug, Clone, PartialEq)]
pub struct Unchecked {
    pub path: String,
    pub refdes: String,
    pub keepout: String,
    pub limit: f64,
    pub side: Side,
}

/// Two bodies on one face that fill the same space.
#[derive(Debug, Clone, PartialEq)]
pub struct Overlap {
    /// The two paths, the smaller first.
    pub a: String,
    pub b: String,
    pub a_refdes: String,
    pub b_refdes: String,
    pub side: Side,
    /// The Z range both fill: from the face out to the shorter body's height.
    pub z_bottom: f64,
    pub z_top: f64,
}

/// What the scene shows that is wrong or unknown in 3D.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Findings {
    pub too_tall: Vec<TooTall>,
    pub unchecked: Vec<Unchecked>,
    pub overlaps: Vec<Overlap>,
}

/// How many bodies have a known height (the glossary's _Height coverage_).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coverage {
    pub known: usize,
    pub total: usize,
    pub stated: usize,
    pub catalog: usize,
}

/// The scene of one placed board.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub board: BoardSolid,
    // Copper goes here (P3 of #430): pads, tracks, vias and pours, each with its net.
    /// By path.
    pub bodies: Vec<Body>,
    /// By name.
    pub limits: Vec<HeightLimit>,
    pub findings: Findings,
}

/// How far two boxes must share before they count as overlapping, in mm: what the placement
/// check's `Rect::overlaps` allows.
const TOLERANCE: f64 = 1e-6;

impl Scene {
    /// The scene of `board`. `provenance` says where each part's height came from, by path
    /// (`netsmith::heights::provenance`); a part it does not name is `stated` if it has a height
    /// and `unknown` if not. A part with no height is `unknown` whatever it says.
    pub fn of(board: &Board, provenance: &BTreeMap<String, HeightProvenance>) -> Scene {
        let frame = Frame::of(board);
        let thickness = board
            .process
            .stackup
            .chosen_thickness()
            .expect("a board is only made once its thickness is chosen");
        let solid = solid(board, &frame, thickness);
        let mut parts: Vec<&crate::board::Placed> =
            board.parts.iter().filter(|p| p.kind != "Label").collect();
        parts.sort_by(|a, b| a.path.cmp(&b.path));
        let bodies: Vec<Body> = parts
            .iter()
            .map(|p| body(p, &frame, thickness, provenance))
            .collect();
        let limits = limits(board, &frame, thickness, &bodies);
        let findings = findings(board, &bodies);
        Scene {
            board: solid,
            bodies,
            limits,
            findings,
        }
    }

    pub fn body(&self, path: &str) -> Option<&Body> {
        self.bodies.iter().find(|b| b.path == path)
    }

    /// How many bodies have a known height, and where it came from.
    pub fn coverage(&self) -> Coverage {
        let count = |p| self.bodies.iter().filter(|b| b.provenance == p).count();
        let (stated, catalog) = (
            count(HeightProvenance::Stated),
            count(HeightProvenance::Catalog),
        );
        Coverage {
            known: stated + catalog,
            total: self.bodies.len(),
            stated,
            catalog,
        }
    }

    /// The tallest part of known height on a face, the first by path among equals.
    pub fn tallest(&self, side: Side) -> Option<Tallest> {
        tallest_of(self.bodies.iter().filter(|b| b.side == side))
    }
}

fn tallest_of<'a>(bodies: impl Iterator<Item = &'a Body>) -> Option<Tallest> {
    let mut best: Option<&Body> = None;
    for b in bodies {
        if let Some(h) = b.height
            && best.is_none_or(|t| h > t.height.unwrap_or(f64::MIN))
        {
            best = Some(b);
        }
    }
    best.map(|b| Tallest {
        path: b.path.clone(),
        refdes: b.refdes.clone(),
        height: b.height.unwrap_or_default(),
    })
}

/// The board's sheet frame (y down, origin at the sheet's corner) to the scene's plan.
struct Frame {
    left: f64,
    top: f64,
    bottom: f64,
}

impl Frame {
    fn of(board: &Board) -> Frame {
        Frame {
            left: board.outline.min.x,
            top: board.outline.min.y,
            bottom: board.outline.max.y,
        }
    }

    fn point(&self, p: Pt) -> Plan {
        Plan {
            x: clean(p.x - self.left),
            y: clean(self.bottom - p.y),
        }
    }

    fn rect(&self, r: Rect) -> PlanBox {
        let (a, b) = (self.point(r.min), self.point(r.max));
        PlanBox {
            min: Plan {
                x: a.x.min(b.x),
                y: a.y.min(b.y),
            },
            max: Plan {
                x: a.x.max(b.x),
                y: a.y.max(b.y),
            },
        }
    }
}

/// A length to a nanometre, with no negative zero, so a report prints the same bytes on every
/// machine.
fn clean(v: f64) -> f64 {
    let r = crate::stackup::round(v);
    if r == 0.0 { 0.0 } else { r }
}

fn solid(board: &Board, frame: &Frame, thickness: f64) -> BoardSolid {
    let stack: Vec<StackLayer> = crate::stackup::layers(board);
    let mut z = 0.0;
    let layers = stack
        .into_iter()
        .map(|l| {
            let top = z;
            z = clean(z - l.thickness);
            SceneLayer {
                name: l.name,
                kind: l.kind,
                thickness: l.thickness,
                z_top: top,
                z_bottom: z,
            }
        })
        .collect();
    let cutouts = board
        .cutout_shapes()
        .into_iter()
        .map(|(c, shape)| SceneCutout {
            name: c.name.clone(),
            at: frame.rect(shape.rect),
            radius: c.radius,
        })
        .collect::<Vec<_>>();
    let mut parts: Vec<&crate::board::Placed> = board.parts.iter().collect();
    parts.sort_by(|a, b| a.path.cmp(&b.path));
    let mut holes = Vec::new();
    for p in parts {
        for pad in &p.footprint.pads {
            let Some((from, to, radius)) = p.hole(pad) else {
                continue;
            };
            holes.push(Hole {
                owner: HoleOwner::Pad {
                    path: p.path.clone(),
                    refdes: p.refdes.clone(),
                    pad: pad.number.clone(),
                },
                from: frame.point(from),
                to: frame.point(to),
                diameter: clean(2.0 * radius),
                plated: pad.kind == PadKind::ThruHole,
            });
        }
    }
    holes.extend(board.vias.iter().map(|v| Hole {
        owner: HoleOwner::Via { net: v.net.clone() },
        from: frame.point(v.at),
        to: frame.point(v.at),
        diameter: v.drill,
        plated: true,
    }));
    BoardSolid {
        name: board.name.clone(),
        width: clean(board.outline.width()),
        height: clean(board.outline.height()),
        corner_radius: board.corner_radius,
        thickness,
        stack_thickness: clean(-z),
        layers,
        cutouts,
        holes,
    }
}

fn body(
    p: &crate::board::Placed,
    frame: &Frame,
    thickness: f64,
    provenance: &BTreeMap<String, HeightProvenance>,
) -> Body {
    let local = p.footprint.body;
    let corners = [
        local.min,
        Pt::new(local.max.x, local.min.y),
        local.max,
        Pt::new(local.min.x, local.max.y),
    ]
    .map(|c| frame.point(p.to_board(c)));
    let provenance = match p.height {
        None => HeightProvenance::Unknown,
        Some(_) => match provenance.get(&p.path) {
            Some(HeightProvenance::Catalog) => HeightProvenance::Catalog,
            _ => HeightProvenance::Stated,
        },
    };
    let (z_face, out) = match p.side {
        Side::Back => (-thickness, -1.0),
        _ => (0.0, 1.0),
    };
    let plated: Vec<Rect> = p
        .footprint
        .pads
        .iter()
        .filter(|pad| pad.kind == PadKind::ThruHole)
        .map(|pad| p.pad_bounds(pad))
        .collect();
    let pins = match plated.iter().copied().reduce(|a, b| a.union(&b)) {
        None => Pins::None,
        Some(r) => {
            let at = frame.rect(r);
            match &p.protrusion {
                None => Pins::Unknown { at },
                Some(t) => {
                    // The pins come out of the face opposite the part.
                    let (near, toward) = match p.side {
                        Side::Back => (0.0, 1.0),
                        _ => (-thickness, -1.0),
                    };
                    let far = clean(near + toward * t.length);
                    Pins::Known {
                        at,
                        length: t.length,
                        source: t.source.clone(),
                        z_top: near.max(far),
                        z_bottom: near.min(far),
                    }
                }
            }
        }
    };
    Body {
        path: p.path.clone(),
        refdes: p.refdes.clone(),
        kind: p.kind.clone(),
        side: p.side,
        layout_at: (clean(p.at.x - frame.left), clean(p.at.y - frame.top)),
        rotation: p.rotation,
        corners,
        bounds: frame.rect(p.body()),
        body_is_courtyard: p.footprint.body_is_courtyard,
        z_face,
        z_far: p.height.map(|h| clean(z_face + out * h)),
        height: p.height,
        provenance,
        pins,
    }
}

fn limits(board: &Board, frame: &Frame, thickness: f64, bodies: &[Body]) -> Vec<HeightLimit> {
    let parts: BTreeMap<&str, &crate::board::Placed> =
        board.parts.iter().map(|p| (p.path.as_str(), p)).collect();
    let mut limits: Vec<HeightLimit> = board
        .keepout_areas()
        .into_iter()
        .filter_map(|k| {
            let max_height = k.max_height?;
            let faces = [Side::Front, Side::Back]
                .into_iter()
                .filter(|side| k.limits_height_on(*side))
                .map(|side| {
                    // The parts under it on this face, as the height check finds them.
                    let under: Vec<&Body> = bodies
                        .iter()
                        .filter(|b| b.side == side && k.carrier.as_deref() != Some(&b.path))
                        .filter(|b| parts[b.path.as_str()].body().overlaps(&k.rect))
                        .collect();
                    let tallest = tallest_of(under.iter().copied());
                    let (z_bottom, z_top) = match side {
                        Side::Back => (clean(-thickness - max_height), -thickness),
                        _ => (0.0, max_height),
                    };
                    LimitFace {
                        side,
                        z_bottom,
                        z_top,
                        margin: tallest.as_ref().map(|t| clean(max_height - t.height)),
                        tallest,
                        unknown: under
                            .iter()
                            .filter(|b| b.height.is_none())
                            .map(|b| b.path.clone())
                            .collect(),
                    }
                })
                .collect();
            Some(HeightLimit {
                name: k.name,
                carrier: k.carrier,
                at: frame.rect(k.rect),
                max_height,
                faces,
            })
        })
        .collect();
    limits.sort_by(|a, b| a.name.cmp(&b.name));
    limits
}

fn findings(board: &Board, bodies: &[Body]) -> Findings {
    let mut too_tall: Vec<TooTall> = board
        .check_placement()
        .into_iter()
        .filter_map(|p| match p {
            PlacementProblem::TooTall {
                refdes,
                path,
                keepout,
                height,
                limit,
                side,
                ..
            } => Some(TooTall {
                path,
                refdes,
                keepout,
                height,
                limit,
                side,
            }),
            _ => None,
        })
        .collect();
    too_tall.sort_by(|a, b| (&a.path, &a.keepout).cmp(&(&b.path, &b.keepout)));
    let mut unchecked: Vec<Unchecked> = board
        .heights_unchecked()
        .into_iter()
        .map(|u| Unchecked {
            path: u.path,
            refdes: u.refdes,
            keepout: u.keepout,
            limit: u.limit,
            side: u.side,
        })
        .collect();
    unchecked.sort_by(|a, b| (&a.path, &a.keepout).cmp(&(&b.path, &b.keepout)));
    Findings {
        too_tall,
        unchecked,
        overlaps: overlaps(bodies),
    }
}

/// The pairs of bodies on one face, both of known height above zero, whose turned boxes share
/// area in plan. Both stand from the face, so they share the space up to the shorter one.
fn overlaps(bodies: &[Body]) -> Vec<Overlap> {
    let mut found = Vec::new();
    for (i, a) in bodies.iter().enumerate() {
        for b in &bodies[i + 1..] {
            let (Some(ha), Some(hb)) = (a.height, b.height) else {
                continue;
            };
            if a.side != b.side || ha <= TOLERANCE || hb <= TOLERANCE {
                continue;
            }
            if !boxes_overlap(&a.bounds, &b.bounds) || !turned_overlap(&a.corners, &b.corners) {
                continue;
            }
            let reach = ha.min(hb);
            let (z_bottom, z_top) = match a.side {
                Side::Back => (clean(a.z_face - reach), a.z_face),
                _ => (a.z_face, clean(a.z_face + reach)),
            };
            found.push(Overlap {
                a: a.path.clone(),
                b: b.path.clone(),
                a_refdes: a.refdes.clone(),
                b_refdes: b.refdes.clone(),
                side: a.side,
                z_bottom,
                z_top,
            });
        }
    }
    found
}

fn boxes_overlap(a: &PlanBox, b: &PlanBox) -> bool {
    a.min.x < b.max.x - TOLERANCE
        && b.min.x < a.max.x - TOLERANCE
        && a.min.y < b.max.y - TOLERANCE
        && b.min.y < a.max.y - TOLERANCE
}

/// Whether two convex quadrilaterals share area: no edge of either separates them by more than
/// the tolerance (the separating axis test).
fn turned_overlap(a: &[Plan; 4], b: &[Plan; 4]) -> bool {
    let separated_along = |poly: &[Plan; 4]| {
        (0..4).any(|i| {
            let (p, q) = (poly[i], poly[(i + 1) % 4]);
            let (nx, ny) = (q.y - p.y, p.x - q.x);
            let len = (nx * nx + ny * ny).sqrt();
            if len == 0.0 {
                return false;
            }
            let project = |s: &[Plan; 4]| {
                s.iter()
                    .map(|c| (c.x * nx + c.y * ny) / len)
                    .fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
            };
            let ((alo, ahi), (blo, bhi)) = (project(a), project(b));
            ahi <= blo + TOLERANCE || bhi <= alo + TOLERANCE
        })
    };
    !separated_along(a) && !separated_along(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, size: f64) -> [Plan; 4] {
        [
            Plan { x, y },
            Plan { x: x + size, y },
            Plan {
                x: x + size,
                y: y + size,
            },
            Plan { x, y: y + size },
        ]
    }

    #[test]
    fn boxes_that_only_touch_do_not_overlap_and_boxes_that_share_area_do() {
        assert!(!turned_overlap(
            &square(0.0, 0.0, 1.0),
            &square(1.0, 0.0, 1.0)
        ));
        assert!(turned_overlap(
            &square(0.0, 0.0, 1.0),
            &square(0.5, 0.5, 1.0)
        ));
    }

    #[test]
    fn a_diamond_beside_a_square_corner_is_apart_though_their_bounds_meet() {
        // A square turned 45 degrees round (2, 2), reaching to (2 - 0.71, 2): its bounds overlap
        // the unit square at the origin's, but its edge does not reach the square's corner.
        let r = std::f64::consts::FRAC_1_SQRT_2;
        let diamond = [
            Plan { x: 2.0 - r, y: 2.0 },
            Plan { x: 2.0, y: 2.0 - r },
            Plan { x: 2.0 + r, y: 2.0 },
            Plan { x: 2.0, y: 2.0 + r },
        ];
        let square = square(0.0, 0.0, 1.5);
        assert!(!turned_overlap(&square, &diamond));
    }

    #[test]
    fn a_length_is_cleaned_of_negative_zero_and_float_noise() {
        assert_eq!(clean(-0.0).to_string(), "0");
        assert_eq!(clean(0.1 + 0.2), 0.3);
    }
}
