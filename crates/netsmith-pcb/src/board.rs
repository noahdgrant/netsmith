//! The board: a design's parts as placed footprints inside an outline, and its nets as the
//! pads they join.

use std::collections::{BTreeMap, HashMap};

use netsmith_core::{Design, DesignError, Process};
use netsmith_kicad::{MapError, Mapping};

use crate::footprint::{Footprint, FootprintError, FootprintLibrary, Layer, Mount, Pad};
use crate::geometry::{Pt, Rect};
use crate::overhang::{self, Edge, Overhang};
use crate::place::{self, DoesNotFit, ORIGIN, PlaceOptions};
use crate::rules::Rules;
use crate::shape::RoundRect;

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

/// The refusal with the requirements of the physical file that are not met where the placer
/// ended with the parts, and the lines that cannot all hold (ADR 0045).
/// `base` is what the layout file and the caller said, before the requirements were turned
/// into places.
fn explained(
    mut error: DoesNotFit,
    design: &Design,
    parts: &[Placed],
    base: &PlaceOptions,
    rules: &Rules,
) -> DoesNotFit {
    // The parts as the refusal lists them: where the placer ended, not where they started.
    let ended: Vec<Placed> = parts
        .iter()
        .map(|p| {
            let mut q = p.clone();
            if let Some(state) = error.parts.iter().find(|s| s.part == p.refdes) {
                q.at = ORIGIN + Pt::new(state.at.0, state.at.1);
                q.rotation = state.rotation;
                q.side = state.side;
            }
            q
        })
        .collect();
    let outline = Rect::new(
        ORIGIN,
        Pt::new(ORIGIN.x + error.outline.0, ORIGIN.y + error.outline.1),
    );
    error.requirements =
        crate::explain::explain(design, &ended, outline, base, rules).map(Box::new);
    error
}

/// The box of the outline the design states, from the top left corner of the sheet, as the
/// most it allows (a range's top; infinite where a side is left open), and whether it is
/// known on both sides, and whether its size is a range (the real outline may be smaller).
fn outline_box(design: &Design) -> (Rect, bool, bool) {
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

/// What [`Board::load`] found: everything [`Board::from_design_placed`] needs before it turns
/// the requirements into places.
struct Loaded {
    process: Process,
    rules: Rules,
    /// The options with the design's keepouts, cutouts and height limits added.
    placement: PlaceOptions,
    /// The footprints, on the faces the options gave them, not yet placed.
    parts: Vec<Placed>,
}

impl From<DesignError> for BoardError {
    fn from(e: DesignError) -> Self {
        Self::Design(e)
    }
}

/// Which face of the board a part is on.
///
/// A part on the back is the front footprint seen in a mirror along its y axis (its x is
/// negated) and then turned by its rotation, as KiCad flips a footprint to the other side:
/// its pads are on `B.Cu` and its silkscreen and courtyard on the `B.` layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Side {
    #[default]
    Front,
    Back,
}

impl Side {
    /// The face's word in messages and the layout file: `top` or `bottom`.
    pub fn word(self) -> &'static str {
        match self {
            Side::Front => "top",
            Side::Back => "bottom",
        }
    }

    /// The outer copper layer of this face.
    pub fn copper(self) -> CopperLayer {
        match self {
            Side::Front => CopperLayer::Front,
            Side::Back => CopperLayer::Back,
        }
    }
}

/// Which side of a part its reference text is on, as the part lies on the board: the text is
/// upright whatever the part's turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ReferenceSide {
    /// Where libraries put it.
    #[default]
    Above = 0,
    Below = 1,
    Left = 2,
    Right = 3,
}

impl ReferenceSide {
    pub const ALL: [ReferenceSide; 4] = [Self::Above, Self::Below, Self::Left, Self::Right];
}

/// A footprint on the board.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub refdes: String,
    /// The part's path in the design (`stage.output_cap`), which a layout file keys it by.
    pub path: String,
    pub kind: String,
    /// The generics the part's definition provides (ADR 0068), by name, as the design has
    /// them: a provider of `Capacitor` is a capacitor to the placer whatever its own name.
    pub provides: Vec<String>,
    pub value: String,
    /// `Resistor_SMD:R_0402_1005Metric`
    pub library_id: String,
    pub footprint: Footprint,
    /// The numbers of the pads that belong to pins the part's pin table calls `power`
    /// (ADR 0016). The placer reads them to find which capacitor decouples which part.
    pub power_pads: Vec<String>,
    /// Where the footprint's origin is on the board, in mm.
    pub at: Pt,
    /// Degrees, counterclockwise on screen.
    pub rotation: f64,
    pub side: Side,
    /// Which side of the part its reference text is on.
    pub reference_side: ReferenceSide,
    /// How tall the body stands above its face of the board, in mm, as the design states it
    /// (`.height`, ADR 0045 "Height limits"). `None` is unknown, never zero.
    pub height: Option<f64>,
    /// How far its pins protrude out of the opposite face
    /// (`.pin_protrusion`, ADR 0045 section 15). `None` is unknown: the pads are an obstacle.
    pub protrusion: Option<crate::protrusion::Protrusion>,
    /// Whether the board house leaves it to be fitted by hand (or not at all), so that it
    /// can be put over cut pins. False when the design does not say.
    pub hand_fitted: bool,
}

impl Placed {
    /// The height of the reference text on the board, in mm: what the library draws, or the
    /// process's smallest if that is more.
    pub(crate) fn text_height(&self, rules: &Rules) -> f64 {
        self.footprint.reference_size.max(rules.min_text_height)
    }

    /// The width of the reference text, in mm: about one text height for each letter.
    pub(crate) fn text_width(&self, rules: &Rules) -> f64 {
        self.refdes.chars().count() as f64 * self.text_height(rules) * 0.95
            + rules.min_text_thickness
    }

    /// Whether the footprint has a drilled hole, which takes room on both faces.
    pub fn has_hole(&self) -> bool {
        self.footprint.pads.iter().any(|p| p.drill.is_some())
    }

    /// What face `face` of the board the part takes room on, and whether that room is its
    /// pads (ADR 0031): a surface-mount part takes its courtyard on its own face
    /// only; a part with a hole takes its courtyard on both, except that a footprint which
    /// draws its courtyard on only one face's layer is judged on the other face by its pads,
    /// each grown by `clearance`, so a part on that face may stand over the rest of its
    /// body. A footprint that draws both courtyard layers, or none, counts on both as ever.
    /// `court` is the courtyard to use where it counts (the board's clip of it, if the part
    /// may hang over an edge). The boxes are empty where the part takes no room.
    pub fn room_on(&self, face: Side, clearance: f64, court: Rect) -> Vec<Rect> {
        if !self.has_hole() {
            return if face == self.side {
                vec![court]
            } else {
                Vec::new()
            };
        }
        // The footprint's front layer is the part's own face, whichever it is on.
        let (own, opposite) = self.footprint.courtyard_faces;
        let (here, there) = if face == self.side {
            (own, opposite)
        } else {
            (opposite, own)
        };
        if here || !there {
            return vec![court];
        }
        self.footprint
            .pads
            .iter()
            .map(|p| self.pad_bounds(p).expanded(clearance))
            .collect()
    }

    /// Whether two parts take room in the same place on either face of the board: `court_a`
    /// and `court_b` are the courtyards to count ([`Placed::room_on`]). A part that stands
    /// excused over the other's protruding pins ([`crate::protrusion`]) takes only its pads there,
    /// against the pads of the other; `zones` are the height limits that rule judges by.
    pub fn rooms_overlap(
        &self,
        court_a: Rect,
        other: &Placed,
        court_b: Rect,
        clearance: f64,
        zones: &[crate::place::HeightZone],
    ) -> bool {
        let excused = crate::protrusion::excused(self, other, zones);
        [Side::Front, Side::Back].into_iter().any(|face| {
            let (mine, theirs) = match excused {
                Some(pins_first) => {
                    let (protrusion, cover) = if pins_first {
                        (self, other)
                    } else {
                        (other, self)
                    };
                    if face == cover.side {
                        let (t, c) = crate::protrusion::excused_rooms(protrusion, cover, clearance);
                        if pins_first { (t, c) } else { (c, t) }
                    } else {
                        (
                            self.room_on(face, clearance, court_a),
                            other.room_on(face, clearance, court_b),
                        )
                    }
                }
                None => (
                    self.room_on(face, clearance, court_a),
                    other.room_on(face, clearance, court_b),
                ),
            };
            mine.iter().any(|a| theirs.iter().any(|b| a.overlaps(b)))
        })
    }

    /// Whether the part is surface mount or through hole.
    pub fn mount(&self) -> Mount {
        self.footprint.mount
    }

    /// Where the centre of the reference text goes, in the footprint's own frame. It is on
    /// the part's [`ReferenceSide`], clear of the footprint's silkscreen and courtyard by the
    /// process's silk to pad space. Above, it is where the library puts it, or further out if
    /// that is nearer than the rules allow. The text is written upright on the board
    /// whatever the part's turn, so its place is found for the part as it lies on the board
    /// and turned back to the footprint's frame.
    pub fn reference_at(&self, rules: &Rules) -> Pt {
        let fp = &self.footprint;
        // A point of the footprint's frame as it lies on the board, round the origin.
        let lie = |p: Pt| {
            let p = if self.mirrored() {
                Pt::new(-p.x, p.y)
            } else {
                p
            };
            p.rotate(self.rotation)
        };
        // The box of the courtyard and the silkscreen as the part lies.
        let court = fp.courtyard.rotated_for(self.mirrored(), self.rotation);
        let silk = fp
            .graphics
            .iter()
            .filter(|g| g.layer == Layer::Silk)
            .flat_map(|g| {
                g.shape
                    .extent_points()
                    .into_iter()
                    .map(move |p| Rect::new(p, p).expanded(g.stroke / 2.0))
            })
            .fold(court, |a, r| {
                let r = Rect::new(lie(r.min), lie(r.max));
                let r = Rect::new(
                    Pt::new(r.min.x.min(r.max.x), r.min.y.min(r.max.y)),
                    Pt::new(r.min.x.max(r.max.x), r.min.y.max(r.max.y)),
                );
                a.union(&r)
            });
        let height = self.text_height(rules);
        let away = rules.silk_to_pad + rules.min_text_thickness;
        let library = lie(fp.reference_at);
        let on_board = match self.reference_side {
            ReferenceSide::Above => {
                let y = silk.min.y - away - height / 2.0;
                Pt::new(library.x, library.y.min(y))
            }
            ReferenceSide::Below => Pt::new(library.x, silk.max.y + away + height / 2.0),
            ReferenceSide::Left => Pt::new(
                silk.min.x - away - self.text_width(rules) / 2.0,
                silk.centre().y,
            ),
            ReferenceSide::Right => Pt::new(
                silk.max.x + away + self.text_width(rules) / 2.0,
                silk.centre().y,
            ),
        };
        let local = on_board.rotate(-self.rotation);
        if self.mirrored() {
            Pt::new(-local.x, local.y)
        } else {
            local
        }
    }

    /// Whether the part is on the back, which mirrors its footprint.
    pub fn mirrored(&self) -> bool {
        self.side == Side::Back
    }

    /// A point of the footprint's own frame, on the board.
    pub fn to_board(&self, local: Pt) -> Pt {
        let local = if self.mirrored() {
            Pt::new(-local.x, local.y)
        } else {
            local
        };
        self.at + local.rotate(self.rotation)
    }

    /// A box of the footprint's own frame, on the board.
    pub(crate) fn rect_to_board(&self, r: Rect) -> Rect {
        let r = if self.mirrored() { r.mirrored_x() } else { r };
        r.rotated(self.rotation).translated(self.at)
    }

    /// Whether the pad has copper on the front and on the back of the board. A through-hole
    /// pad has both; a surface-mount pad has the face its part is on.
    pub fn pad_layers(&self, pad: &Pad) -> (bool, bool) {
        let (front, back) = pad.copper_layers();
        if self.mirrored() {
            (back, front)
        } else {
            (front, back)
        }
    }

    pub fn pad_centre(&self, pad: &Pad) -> Pt {
        self.to_board(pad.at)
    }

    /// The box the pad's copper spans on the board.
    pub fn pad_bounds(&self, pad: &Pad) -> Rect {
        self.rect_to_board(pad.bounds())
    }

    /// The pad's hole on the board as a capsule: the two ends of its axis and its radius.
    /// A round hole has both ends at its centre. `None` for a pad with no hole.
    pub fn hole(&self, pad: &Pad) -> Option<(Pt, Pt, f64)> {
        let drill = pad.drill?;
        let (long, short) = (drill.width.max(drill.height), drill.width.min(drill.height));
        let reach = (long - short) / 2.0;
        // A slot lies along the pad's x axis when it is wider than it is high.
        let axis = if drill.width >= drill.height {
            Pt::new(reach, 0.0)
        } else {
            Pt::new(0.0, reach)
        };
        let turn = self.pad_angle(pad);
        let centre = self.pad_centre(pad);
        let end = axis.rotate(turn);
        Some((centre - end, centre + end, short / 2.0))
    }

    /// The pad's total turn on the board.
    pub fn pad_angle(&self, pad: &Pad) -> f64 {
        if self.mirrored() {
            self.rotation - pad.angle
        } else {
            pad.angle + self.rotation
        }
    }

    /// The boxes of the openings the footprint draws in the solder mask, on the board.
    pub fn mask_openings(&self) -> Vec<Rect> {
        self.footprint
            .mask_openings
            .iter()
            .map(|r| self.rect_to_board(*r))
            .collect()
    }

    /// The courtyard's box on the board.
    pub fn courtyard(&self) -> Rect {
        self.rect_to_board(self.footprint.courtyard)
    }

    /// The body's box on the board: what a constraint on a position measures (ADR 0045).
    pub fn body(&self) -> Rect {
        self.rect_to_board(self.footprint.body)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PadRef {
    pub refdes: String,
    /// The pad number its footprint uses. Every pad of the footprint with this number is
    /// meant.
    pub number: String,
}

impl std::fmt::Display for PadRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.refdes, self.number)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardNet {
    pub name: String,
    pub pads: Vec<PadRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlacementProblem {
    /// Two courtyards share area. `pins` is the path of a through-hole part whose pins are
    /// under the other part and are not stated as cut short, if that is what the overlap is
    /// (ADR 0045 section 15), so that the message can say how to state it.
    Overlap {
        a: String,
        b: String,
        pins: Option<String>,
    },
    /// A part lies over the pins of a through-hole part that the design states are cut
    /// short, and the rule that excuses it ([`crate::protrusion`]) refuses.
    OverPins {
        holder: String,
        pin_path: String,
        /// How far the pins protrude, in mm, and the lines that state it.
        length: f64,
        source: String,
        cover: String,
        cover_path: String,
        side: Side,
        why: crate::protrusion::Refusal,
    },
    /// A courtyard is not wholly inside the outline, past an edge the design does not say
    /// the part may hang over (`past`: how far it is past each such edge, in mm; empty if the
    /// box is inside and a corner of it is outside a rounded corner of the outline).
    OutsideOutline {
        refdes: String,
        /// The part's path, for the line that would allow the overhang.
        path: String,
        past: Vec<(Edge, f64)>,
        /// The edges the design does say the part may hang over, and by what.
        stated: Vec<Edge>,
        by: String,
    },
    /// A pad is nearer the board edge than the rules allow, or outside it (a negative
    /// distance).
    PadNearEdge { refdes: String, distance: f64 },
    /// A part's courtyard overlaps a cutout.
    InCutout { refdes: String, cutout: String },
    /// A pad is nearer the edge of a cutout than the rules allow.
    PadNearCutout {
        refdes: String,
        cutout: String,
        distance: f64,
    },
    /// A part's courtyard overlaps a keepout that forbids parts.
    InKeepout { refdes: String, keepout: String },
    /// A pad is inside a keepout that forbids tracks, so no track can reach it.
    PadInKeepout { refdes: String, keepout: String },
    /// A part is taller than the height limit of a keepout it stands under (ADR 0045,
    /// "Height limits").
    TooTall {
        refdes: String,
        path: String,
        keepout: String,
        /// The part's height and the keepout's limit, in mm.
        height: f64,
        limit: f64,
        /// The face of the board the part is on.
        side: Side,
        /// The centre of the part's body, in mm from the outline's top left corner.
        at: (f64, f64),
    },
    /// A cutout is not wholly inside the outline.
    CutoutOutsideOutline { cutout: String },
    /// A cutout or a keepout is nowhere: the design does not say where, and the layout file
    /// has no entry for it.
    NoPosition { what: &'static str, name: String },
    /// A hole of `refdes` is nearer the copper or the hole of `other`, on `to`, than the
    /// process allows (`least`, for copper its clearance and for a hole its hole to hole).
    HoleTooNear {
        refdes: String,
        /// The number of the pad whose hole it is, and of the pad of `other` it is near.
        pad: String,
        other: String,
        other_pad: String,
        to: HoleNeighbour,
        distance: f64,
        least: f64,
    },
}

impl PlacementProblem {
    /// The finding for two parts whose rooms overlap: the plain overlap, with a hint when
    /// the pins of a through-hole part come out under the other and are not stated as cut
    /// short, or, when they are, why the trim does not excuse it (ADR 0045 section 15).
    pub(crate) fn overlap_of(
        a: &Placed,
        b: &Placed,
        zones: &[crate::place::HeightZone],
    ) -> PlacementProblem {
        match crate::protrusion::relation(a, b, zones) {
            Some((first, crate::protrusion::Verdict::Refused(why))) => {
                let (protrusion, cover) = if first { (a, b) } else { (b, a) };
                let stated =
                    protrusion
                        .protrusion
                        .clone()
                        .unwrap_or(crate::protrusion::Protrusion {
                            length: 0.0,
                            source: String::new(),
                        });
                PlacementProblem::OverPins {
                    holder: protrusion.refdes.clone(),
                    pin_path: protrusion.path.clone(),
                    length: stated.length,
                    source: stated.source,
                    cover: cover.refdes.clone(),
                    cover_path: cover.path.clone(),
                    side: cover.side,
                    why,
                }
            }
            _ => PlacementProblem::Overlap {
                a: a.refdes.clone(),
                b: b.refdes.clone(),
                pins: crate::protrusion::unstated_under(a, b).map(|p| p.path.clone()),
            },
        }
    }
}

/// ` (pad 3)`, or nothing for a pad with no number (a mounting hole's).
fn pad_text(number: &str) -> String {
    if number.is_empty() {
        String::new()
    } else {
        format!(" (pad {number})")
    }
}

/// How much taller than a limit a part may be before it is reported: half a micrometre, as
/// the rest of the checks measure.
const HEIGHT_TOLERANCE: f64 = 0.0005;

/// A part under a height limit that states no height (ADR 0045, "Height limits"): whether it
/// fits is unknown, which is not a pass.
#[derive(Debug, Clone, PartialEq)]
pub struct HeightUnchecked {
    pub refdes: String,
    pub path: String,
    pub keepout: String,
    /// The limit, in mm.
    pub limit: f64,
    pub side: Side,
}

impl std::fmt::Display for HeightUnchecked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "height of {} (`{}`) is unchecked: the keepout `{}` limits parts on the {} face to {:.2} mm and the part states no height (say `.{}.height &= 8mm`, with the real number)",
            self.refdes,
            self.path,
            self.keepout,
            self.side.word(),
            self.limit,
            self.path
        )
    }
}

/// The pins of a through-hole part whose protrusion the design states, to be cut short after
/// soldering, and the parts that lie over them (ADR 0045 section 15).
#[derive(Debug, Clone, PartialEq)]
pub struct Trim {
    pub refdes: String,
    pub path: String,
    /// How far the pins protrude out of the board, in mm.
    pub length: f64,
    /// The lines that state it.
    pub source: String,
    /// The face the pins stand out of.
    pub face: Side,
    /// The parts whose courtyards lie over them: designator and path.
    pub covers: Vec<(String, String)>,
    /// Those of `covers` that no height limit covers where they are, so that the stack on the
    /// protrusions was compared with none.
    pub unlimited: Vec<(String, String)>,
}

impl Trim {
    /// The line of `fab-notes.txt` and `order.txt` (after `trim: `): a step for a person.
    pub fn note(&self) -> String {
        let parts = |list: &[(String, String)]| {
            list.iter()
                .map(|(r, p)| format!("{r} ({p})"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let when = if self.covers.is_empty() {
            "nothing lies over them".to_string()
        } else {
            format!(
                "before {} {} fitted",
                parts(&self.covers),
                if self.covers.len() == 1 { "is" } else { "are" }
            )
        };
        format!(
            "{} ({}): after soldering, cut the pins to protrude {} mm from the {} face, {when}{}; measure one protruding pin",
            self.refdes,
            self.path,
            crate::protrusion::mm(self.length),
            self.face.word(),
            if self.source.is_empty() {
                String::new()
            } else {
                format!(" ({})", self.source)
            }
        )
    }

    /// What was not checked: a covering part with no height limit over it.
    pub fn unchecked(&self) -> Vec<String> {
        self.unlimited
            .iter()
            .map(|(r, p)| {
                format!(
                    "no height limit covers {r} (`{p}`) on the {} face where it lies over the pins of {} (`{}`) that protrude {} mm, so the stack of that protrusion and its height was compared with none",
                    self.face.word(),
                    self.refdes,
                    self.path,
                    crate::protrusion::mm(self.length)
                )
            })
            .collect()
    }
}

/// A part's courtyard past an edge of the outline, which the design says it may be.
#[derive(Debug, Clone, PartialEq)]
pub struct Hang {
    pub refdes: String,
    pub edge: Edge,
    /// How far the courtyard is past the edge, in mm.
    pub by: f64,
    /// Who said so: the requirement, or the lock in the layout file.
    pub stated: String,
}

impl std::fmt::Display for Hang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} hangs over the {} edge: its courtyard is {:.2} mm past it ({})",
            self.refdes,
            self.edge.word(),
            self.by,
            self.stated
        )
    }
}

/// What a hole of one part is too near, on another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoleNeighbour {
    Copper,
    Hole,
}

/// How a courtyard that is past an edge nobody stated is told: the part, each edge and how
/// far, what the design says about overhang, and what would allow it. The
/// distances for the edges in `inexact` are the least over every place the part's
/// requirements leave it (the check before placing). The hint to require the body to reach the edge is left out, and the
/// reason given in its place, when `body_inside`: the body stays inside the
/// outline there, and no statement allows only the courtyard past an edge (ADR 0045
/// section 10; issue #99 has the options).
pub(crate) fn outside_text(
    refdes: &str,
    path: &str,
    past: &[(Edge, f64)],
    stated: &[Edge],
    by: &str,
    inexact: &[Edge],
    body_inside: bool,
) -> String {
    let mut out = format!("{refdes} is not wholly inside the board outline");
    let Some(&(edge, _)) = past.first() else {
        return format!(
            "{out}: a corner of its courtyard is outside a rounded corner of the outline"
        );
    };
    let list: Vec<String> = past
        .iter()
        .map(|(e, by)| {
            format!(
                "{}{by:.2} mm past the {} edge",
                if inexact.contains(e) { "at least " } else { "" },
                e.word()
            )
        })
        .collect();
    out.push_str(&format!(": its courtyard is {}", list.join(" and ")));
    if !inexact.is_empty() {
        out.push_str(" wherever its requirements put it");
    }
    if stated.is_empty() {
        out.push_str(", and the design does not say it may hang over an edge");
    } else {
        let words: Vec<&str> = stated.iter().map(|e| e.word()).collect();
        out.push_str(&format!(
            ", and the design says it may hang over only the {} edge ({by})",
            words.join(" and ")
        ));
    }
    if body_inside {
        out.push_str(&format!(
            "; its body stays inside the outline there, and no statement allows only the courtyard past the {} edge: an overhang is stated by the body's own edge field reaching the edge (ADR 0045 section 10), which these requirements do not do; move the part, give it a smaller courtyard, or see the options note on issue #99",
            edge.word()
        ));
        return out;
    }
    let line = match edge {
        Edge::Left | Edge::Top => format!("{path}.place.{} <= 0mm", edge.word()),
        Edge::Right => format!("{path}.place.right >= outline.width"),
        Edge::Bottom => format!("{path}.place.bottom >= outline.height"),
    };
    out.push_str(&format!(
        "; to allow it past the {} edge, require the body to reach it, for example `.{line}`",
        edge.word()
    ));
    out
}

impl std::fmt::Display for PlacementProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overlap { a, b, pins } => {
                write!(f, "the courtyards of {a} and {b} overlap")?;
                match pins {
                    Some(path) => write!(
                        f,
                        "; the pins of the through-hole part `{path}` come out under the other; if they are cut short after soldering, the physical file can say how far they protrude (`.{path}.pin_protrusion &= 0.2mm`, with the real length; cutting them is a step someone does by hand, so state it only if it is done)"
                    ),
                    None => Ok(()),
                }
            }
            Self::OverPins {
                holder,
                pin_path,
                length,
                source,
                cover,
                cover_path,
                side,
                why,
            } => write!(
                f,
                "{cover} (`{cover_path}`) lies over the pins of {holder} (`{pin_path}`), whose pins protrude {} mm{}, and that is not allowed: {}",
                crate::protrusion::mm(*length),
                if source.is_empty() {
                    String::new()
                } else {
                    format!(" ({source})")
                },
                why.because(*length, cover, cover_path, *side)
            ),
            Self::OutsideOutline {
                refdes,
                path,
                past,
                stated,
                by,
            } => write!(
                f,
                "{}",
                outside_text(refdes, path, past, stated, by, &[], false)
            ),
            Self::InKeepout { refdes, keepout } => write!(
                f,
                "{refdes} overlaps the keepout `{keepout}`, which forbids parts"
            ),
            Self::TooTall {
                refdes,
                path,
                keepout,
                height,
                limit,
                side,
                at,
            } => write!(
                f,
                "{refdes} (`{path}`) stands {height:.2} mm tall on the {} face, and the keepout `{keepout}` allows {limit:.2} mm there ({:.2} mm too tall), where its body is, around ({:.1}, {:.1}) mm from the board's top left corner",
                side.word(),
                height - limit,
                at.0,
                at.1
            ),
            Self::PadInKeepout { refdes, keepout } => write!(
                f,
                "a pad of {refdes} is inside the keepout `{keepout}`, which forbids tracks, so none can reach it"
            ),
            Self::InCutout { refdes, cutout } => {
                write!(f, "{refdes} overlaps the cutout `{cutout}`")
            }
            Self::PadNearCutout {
                refdes,
                cutout,
                distance,
            } => write!(
                f,
                "a pad of {refdes} is {distance:.2} mm from the edge of the cutout `{cutout}`"
            ),
            Self::CutoutOutsideOutline { cutout } => {
                write!(
                    f,
                    "the cutout `{cutout}` is not wholly inside the board outline"
                )
            }
            Self::NoPosition { what, name } => write!(
                f,
                "the {what} `{name}` has no position: say `.x` and `.y` in the design, or give it one in the layout file"
            ),
            Self::PadNearEdge { refdes, distance } if *distance < 0.0 => {
                write!(
                    f,
                    "a pad of {refdes} is {:.2} mm outside the board edge",
                    -distance
                )
            }
            Self::PadNearEdge { refdes, distance } => {
                write!(
                    f,
                    "a pad of {refdes} is {distance:.2} mm from the board edge"
                )
            }
            Self::HoleTooNear {
                refdes,
                pad,
                other,
                other_pad,
                to,
                distance,
                least,
            } => {
                let (what, rule) = match to {
                    HoleNeighbour::Copper => ("the copper", "clearance"),
                    HoleNeighbour::Hole => ("the hole", "hole to hole space"),
                };
                write!(
                    f,
                    "a hole of {refdes}{} is {distance:.2} mm from {what} of {other}{}, under the \
                     process's {rule} of {least} mm",
                    pad_text(pad),
                    pad_text(other_pad)
                )
            }
        }
    }
}

/// An outer copper layer, the two a track can be on. Inner layers are planes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// An inner copper layer poured as one sheet on a net.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plane {
    /// KiCad's name for the layer: `In1.Cu`.
    pub layer: String,
    pub net: String,
}

/// An outer copper layer poured on a net (ADR 0020).
#[derive(Debug, Clone, PartialEq)]
pub struct BoardPour {
    pub layer: CopperLayer,
    pub net: String,
    /// The most space between the vias that stitch it to the other layers of its net.
    pub stitch_pitch: Option<f64>,
}

/// An area where tracks, vias, copper or parts are not allowed (ADR 0029).
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct BoardCutout {
    pub name: String,
    /// Its corner nearest the board's origin, in the board's frame (the outline's top left),
    /// or `None` while neither the design nor the layout file says where it is.
    pub at: Option<Pt>,
    pub size: (f64, f64),
    pub radius: f64,
}

/// A straight run of copper.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub net: String,
    pub layer: CopperLayer,
    pub from: Pt,
    pub to: Pt,
    pub width: f64,
}

/// A plated hole that joins the two copper layers.
#[derive(Debug, Clone, PartialEq)]
pub struct Via {
    pub net: String,
    pub at: Pt,
    pub diameter: f64,
    pub drill: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub name: String,
    /// The board edge, in mm: a rectangle, with corners rounded by `corner_radius`.
    pub outline: Rect,
    /// The radius of the outline's four corners, 0 for square ones.
    pub corner_radius: f64,
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
    /// The narrowest track of the nets whose rails ask for one (ADR 0020), by net name.
    /// Every other net is routed at the rules' track width.
    pub net_widths: Vec<(String, f64)>,
    /// The parts the person said may hang over the board edge, by reference designator
    /// (ADR 0045, "Built: overhang"). [`Board::check_placement`] does not report the
    /// courtyard of such a part past an edge it may hang over.
    pub overhang: BTreeMap<String, Overhang>,
}

impl Board {
    /// Choose a footprint for each part, place them, and keep the nets. The same design
    /// always gives the same board. The design must say how it is made (its `process`), and
    /// the board must have chosen a thickness for it. The mapping says which footprint each
    /// part has and the library holds it.
    pub fn from_design(
        design: &Design,
        mapping: &Mapping,
        library: &FootprintLibrary,
    ) -> Result<Board, BoardError> {
        Self::from_design_placed(design, mapping, library, &PlaceOptions::default())
    }

    /// The part of [`Board::from_design_placed`] before the requirements are turned into
    /// places: the process and its rules, the options with the keepouts, cutouts and height
    /// limits of the design added, and the parts as footprints on the faces they were given.
    fn load(
        design: &Design,
        mapping: &Mapping,
        library: &FootprintLibrary,
        placement: &PlaceOptions,
    ) -> Result<Loaded, BoardError> {
        design.validate()?;
        if let Some(refdes) = placement
            .sides
            .keys()
            .find(|r| !design.parts.iter().any(|p| &p.refdes == *r))
        {
            return Err(BoardError::NoPart {
                refdes: refdes.clone(),
            });
        }
        let process = design.process.clone().ok_or(BoardError::NoProcess {
            board: design.name.clone(),
        })?;
        let rules = Rules::for_process(&process).map_err(BoardError::Process)?;
        if process.stackup.chosen_thickness().is_none() {
            let (lo, hi) = process.stackup.thickness;
            return Err(BoardError::Process(format!(
                "`{}` has not chosen a thickness: the process `{}` offers {lo} mm to {hi} mm, so say which with `.process.stackup.thickness &= 1.6mm`",
                design.name, process.name
            )));
        }

        // The placer keeps clear of the cutouts and the keepouts for parts that the source
        // places, whether or not the caller said so.
        let mut placement = placement.clone();
        placement.wall = Some(rules.edge_wall());
        for rect in crate::layout::clear_of(design, None) {
            if !placement.keepouts.contains(&rect) {
                placement.keepouts.push(rect);
            }
        }
        for rect in crate::layout::cutouts_of(design, None) {
            if !placement.cutouts.contains(&rect) {
                placement.cutouts.push(rect);
            }
        }
        for zone in crate::layout::height_zones(design, None) {
            if !placement.height_zones.contains(&zone) {
                placement.height_zones.push(zone);
            }
        }
        for zone in crate::layout::carried_zones(design) {
            if !placement.carried.contains(&zone) {
                placement.carried.push(zone);
            }
        }
        let placement = placement.clone();

        let mut loaded: HashMap<String, Footprint> = HashMap::new();
        let mut parts = Vec::with_capacity(design.parts.len());
        for part in &design.parts {
            // A label is text, not a footprint of a library.
            if part.kind == "Label" {
                let height = design
                    .features
                    .labels
                    .iter()
                    .find(|l| l.path == part.path)
                    .and_then(|l| l.text_height)
                    .unwrap_or(rules.min_text_height);
                parts.push(Placed {
                    refdes: part.refdes.clone(),
                    path: part.path.clone(),
                    kind: part.kind.clone(),
                    provides: part.provides.clone(),
                    value: part.value.clone(),
                    footprint: Footprint::label(&part.value, height, rules.min_text_thickness),
                    power_pads: Vec::new(),
                    library_id: "Label".to_string(),
                    at: Pt::default(),
                    rotation: 0.0,
                    side: placement
                        .sides
                        .get(&part.refdes)
                        .copied()
                        .unwrap_or_default(),
                    reference_side: ReferenceSide::default(),
                    height: None,
                    protrusion: None,
                    hand_fitted: true,
                });
                continue;
            }
            let mapped = mapping
                .map(&part.kind, part.package.as_deref())
                .map_err(|error| BoardError::Map {
                    refdes: part.refdes.clone(),
                    error,
                })?;
            if !loaded.contains_key(&mapped.footprint) {
                let footprint =
                    library
                        .load(&mapped.footprint)
                        .map_err(|error| BoardError::Footprint {
                            refdes: part.refdes.clone(),
                            error,
                        })?;
                loaded.insert(mapped.footprint.clone(), footprint);
            }
            parts.push(Placed {
                refdes: part.refdes.clone(),
                path: part.path.clone(),
                kind: part.kind.clone(),
                provides: part.provides.clone(),
                value: part.value.clone(),
                footprint: loaded[&mapped.footprint].clone(),
                power_pads: part
                    .pins
                    .iter()
                    .filter(|pin| pin.kind == netsmith_core::PinKind::Power)
                    .flat_map(|pin| pin.pads.iter().cloned())
                    .collect(),
                library_id: mapped.footprint,
                at: Pt::default(),
                rotation: 0.0,
                side: placement
                    .sides
                    .get(&part.refdes)
                    .copied()
                    .unwrap_or_default(),
                reference_side: ReferenceSide::default(),
                height: design
                    .features
                    .heights
                    .iter()
                    .find(|h| h.path == part.path)
                    .map(|h| h.height),
                protrusion: design
                    .features
                    .protrusions
                    .iter()
                    .find(|t| t.path == part.path)
                    .map(|t| crate::protrusion::Protrusion {
                        length: t.length,
                        source: t.source.clone(),
                    }),
                hand_fitted: design.features.hand_fitted.contains(&part.path),
            });
        }
        Ok(Loaded {
            process,
            rules,
            placement,
            parts,
        })
    }

    /// What the requirements of `design` prove about where its parts can go, before any
    /// placing: the same proofs [`Board::from_design_placed`] makes first (and fails with
    /// [`BoardError::Unplaceable`]), with the warnings for what is only suspicious. It takes
    /// milliseconds. `Err` is a design the placer could not start on either.
    pub fn precheck(
        design: &Design,
        mapping: &Mapping,
        library: &FootprintLibrary,
        placement: &PlaceOptions,
    ) -> Result<crate::explain::Precheck, BoardError> {
        let Loaded {
            rules,
            placement,
            parts,
            ..
        } = Self::load(design, mapping, library, placement)?;
        let plan = crate::constraints::plan(design, &parts, &placement, rules.clearance)
            .map_err(BoardError::Constraints)?;
        let found = crate::explain::before_placing(
            design,
            &parts,
            &placement,
            &plan,
            &rules,
            outline_box(design),
        );
        Ok(if placement.lines_warn {
            found.lines_as_warnings()
        } else {
            found
        })
    }

    /// [`Board::from_design`], with what is known about where the parts go: the face each
    /// is on, and the size the board must be.
    pub fn from_design_placed(
        design: &Design,
        mapping: &Mapping,
        library: &FootprintLibrary,
        placement: &PlaceOptions,
    ) -> Result<Board, BoardError> {
        let Loaded {
            process,
            rules,
            placement,
            mut parts,
        } = Self::load(design, mapping, library, placement)?;

        // What the design requires of where the parts go (ADR 0045): pinned parts are fixed
        // like locked ones, the others get a want the placer keeps to, and every constrained
        // part keeps the face it was given.
        let base = placement.clone();
        let mut placement = placement.clone();
        let plan = crate::constraints::plan(design, &parts, &placement, rules.clearance)
            .map_err(BoardError::Constraints)?;
        // What the lines prove before any placing, in milliseconds: lines that cannot all
        // hold, a courtyard past an edge nobody stated, a part bigger than the outline. The
        // placer is not started for a board that cannot be placed.
        let pre = crate::explain::before_placing(
            design,
            &parts,
            &base,
            &plan,
            &rules,
            outline_box(design),
        );
        let pre = if base.lines_warn {
            pre.lines_as_warnings()
        } else {
            pre
        };
        if pre.is_error() {
            return Err(BoardError::Unplaceable(Box::new(pre)));
        }
        // A part may hang over an edge only where its own requirements say so; a lock in the
        // layout file does not (owner decision, 2026-11-03, ADR 0045 section 10).
        let mut overhang = placement.overhang.clone();
        overhang.extend(plan.overhang);
        placement.overhang = overhang.clone();
        placement.fixed.extend(plan.fixed);
        placement.wants = plan.wants;
        placement.offsets = plan.offsets;
        for part in &mut parts {
            if let Some(&side) = plan.sides.get(&part.refdes) {
                part.side = side;
                placement.sides.insert(part.refdes.clone(), side);
            }
        }
        let placement = &placement;

        let mut nets = Vec::with_capacity(design.nets.len());
        for net in &design.nets {
            let mut pads = Vec::with_capacity(net.pins.len());
            for pin in &net.pins {
                let part = parts
                    .iter()
                    .find(|p| p.refdes == pin.refdes)
                    .expect("the design was validated, so every pin names a part");
                if part.footprint.pads_numbered(&pin.pin).next().is_none() {
                    return Err(BoardError::NoPad {
                        net: net.name.clone(),
                        refdes: pin.refdes.clone(),
                        pad: pin.pin.clone(),
                        footprint: part.library_id.clone(),
                        available: part
                            .footprint
                            .pad_numbers()
                            .into_iter()
                            .map(str::to_string)
                            .collect(),
                    });
                }
                pads.push(PadRef {
                    refdes: pin.refdes.clone(),
                    number: pin.pin.clone(),
                });
            }
            nets.push(BoardNet {
                name: net.name.clone(),
                pads,
            });
        }

        let planes: Vec<Plane> = process
            .stackup
            .inner
            .iter()
            .flat_map(|inner| {
                inner.planes.iter().enumerate().map(|(i, net)| Plane {
                    layer: format!("In{}.Cu", i + 1),
                    net: net.clone(),
                })
            })
            .collect();
        if let Some(plane) = planes
            .iter()
            .find(|p| !nets.iter().any(|n| n.name == p.net))
        {
            return Err(BoardError::Process(format!(
                "the inner layer {} is a plane on the net `{}`, but `{}` has no net of that name",
                plane.layer, plane.net, design.name
            )));
        }

        // A board that states no outline is as big as its parts need, as ever.
        let plane_nets: Vec<String> = planes.iter().map(|p| p.net.clone()).collect();
        let (outline, corner_radius) = match &design.features.outline {
            None => (
                place::arrange_best(
                    &mut parts,
                    &nets,
                    placement,
                    &place::Context {
                        rules: &rules,
                        planes: &plane_nets,
                        corner_radius: 0.0,
                        pours: !design.features.pours.is_empty(),
                    },
                )
                .map_err(|e| BoardError::DoesNotFit(explained(e, design, &parts, &base, &rules)))?,
                0.0,
            ),
            Some(wanted) => (
                crate::fit::fit(
                    &mut parts,
                    &nets,
                    wanted,
                    &rules,
                    placement,
                    &plane_nets,
                    !design.features.pours.is_empty(),
                )
                .map_err(|e| BoardError::DoesNotFit(explained(e, design, &parts, &base, &rules)))?,
                wanted.corner_radius,
            ),
        };
        let cutouts = design
            .features
            .cutouts
            .iter()
            .map(|c| BoardCutout {
                name: c.name.clone(),
                at: c.at.map(|(x, y)| Pt::new(x, y)),
                size: c.size,
                radius: c.corner_radius,
            })
            .collect();
        let pours = design
            .features
            .pours
            .iter()
            .map(|p| BoardPour {
                layer: if p.layer == "top" {
                    CopperLayer::Front
                } else {
                    CopperLayer::Back
                },
                net: p.net.clone(),
                stitch_pitch: p.stitch_pitch,
            })
            .collect();
        Ok(Board {
            name: design.name.clone(),
            outline,
            corner_radius,
            cutouts,
            pours,
            keepouts: design
                .features
                .keepouts
                .iter()
                .map(|k| BoardKeepout {
                    name: k.name.clone(),
                    part: k.part.clone(),
                    at: k.at.map(|(x, y)| Pt::new(x, y)),
                    size: k.size,
                    layers: k.layers.clone(),
                    tracks: k.tracks,
                    vias: k.vias,
                    copper: k.copper,
                    parts: k.parts,
                    max_height: k.max_height,
                    drawn: false,
                })
                .collect(),
            parts,
            nets,
            rules,
            process,
            planes,
            tracks: Vec::new(),
            vias: Vec::new(),
            net_widths: design.features.net_widths.clone(),
            overhang,
        })
    }

    /// The width the router draws a net's tracks at: the widest its rails ask for, and never
    /// less than the rules' track width.
    pub fn track_width(&self, net: &str) -> f64 {
        self.net_widths
            .iter()
            .find(|(n, _)| n == net)
            .map_or(self.rules.track_width, |(_, w)| {
                w.max(self.rules.track_width)
            })
    }

    /// The nets that are routed wider than the rules' track width, grouped by width, from
    /// the narrowest: the net classes of the project file.
    pub fn net_classes(&self) -> Vec<(f64, Vec<String>)> {
        let mut classes: Vec<(f64, Vec<String>)> = Vec::new();
        for net in &self.nets {
            let width = self.track_width(&net.name);
            if width <= self.rules.track_width {
                continue;
            }
            match classes.iter_mut().find(|(w, _)| *w == width) {
                Some((_, nets)) => nets.push(net.name.clone()),
                None => classes.push((width, vec![net.name.clone()])),
            }
        }
        classes.sort_by(|a, b| a.0.total_cmp(&b.0));
        classes
    }

    /// The process the board was routed and checked to, on one line for the fabrication
    /// notes: who makes it, where its numbers are published, and the rules and stackup the
    /// board ended up with (the library's, after the board narrowed them).
    pub fn process_note(&self) -> String {
        let (p, r) = (&self.process, &self.rules);
        let thickness = p
            .stackup
            .chosen_thickness()
            .map_or_else(|| "unchosen".to_string(), |t| t.to_string());
        let planes = if self.planes.is_empty() && self.pours.is_empty() {
            String::new()
        } else {
            let list: Vec<String> = self
                .planes
                .iter()
                .map(|pl| format!("{} is a plane on {}", pl.layer, pl.net))
                .chain(
                    self.pours
                        .iter()
                        .map(|po| format!("{} is a pour on {}", po.layer.name(), po.net)),
                )
                .collect();
            format!(" ({})", list.join(", "))
        };
        format!(
            "{} {} (numbers from {}); track {} mm, space {} mm, drill {} mm, via {} mm, annular ring {} mm, hole to hole {} mm, copper to edge {} mm, silkscreen {} mm line and {} mm text, mask bridge {} mm; {} layers{}, {} mm thick, {} oz copper; order it with these",
            p.fabricator,
            p.name,
            p.source,
            r.min_track_width,
            r.clearance,
            r.via_drill,
            r.via_diameter,
            r.min_annular_ring,
            r.hole_to_hole,
            r.copper_to_edge,
            r.min_text_thickness,
            r.min_text_height,
            p.min_mask_bridge,
            p.stackup.layers,
            planes,
            thickness,
            p.stackup.copper_weight,
        )
    }

    /// Each via that stands in a surface-mount pad (its centre is on the pad's copper): the
    /// via's place in [`Board::vias`] and the pad. A via in a pad has to be filled and capped
    /// by the fabricator, or the solder runs down the hole.
    pub fn vias_in_pads(&self) -> Vec<(usize, PadRef)> {
        let mut found = Vec::new();
        for (k, via) in self.vias.iter().enumerate() {
            let at = Rect::new(via.at, via.at);
            for part in &self.parts {
                for pad in &part.footprint.pads {
                    let surface = pad.drill.is_none() && part.pad_layers(pad) != (false, false);
                    if surface && part.pad_bounds(pad).contains(&at) {
                        found.push((
                            k,
                            PadRef {
                                refdes: part.refdes.clone(),
                                number: pad.number.clone(),
                            },
                        ));
                    }
                }
            }
        }
        found
    }

    /// What the board needs of the fabricator beyond its standard process, one line each for
    /// the fabrication notes; empty when the standard process makes it.
    pub fn process_extras(&self) -> Vec<String> {
        let mut extras = Vec::new();
        let in_pad = self.vias_in_pads();
        if !in_pad.is_empty() {
            let mut pads: Vec<String> = in_pad.iter().map(|(_, p)| p.to_string()).collect();
            pads.dedup();
            extras.push(format!(
                "vias in pads: {} via(s) stand in pads ({}); order them as epoxy filled and capped (plated over) vias, which {} makes up to {} mm wide at extra cost, because an open via in a pad draws the solder away; this is on the order form and the Gerbers do not say it",
                in_pad.len(),
                pads.join(", "),
                self.process.fabricator,
                self.process.via_in_pad
            ));
        }
        extras
    }

    pub fn part(&self, refdes: &str) -> Option<&Placed> {
        self.parts.iter().find(|p| p.refdes == refdes)
    }

    /// The board edge as a shape: the outline with its rounded corners.
    pub fn outline_shape(&self) -> RoundRect {
        RoundRect::new(self.outline, self.corner_radius)
    }

    /// The cutouts that have a place, as shapes on the sheet.
    pub fn cutout_shapes(&self) -> Vec<(&BoardCutout, RoundRect)> {
        self.cutouts
            .iter()
            .filter_map(|c| {
                let min = self.outline.min + c.at?;
                Some((
                    c,
                    RoundRect::new(
                        Rect::new(min, Pt::new(min.x + c.size.0, min.y + c.size.1)),
                        c.radius,
                    ),
                ))
            })
            .collect()
    }

    /// The keepouts that have a place, on the sheet. One a part carries moves and turns with
    /// the part (a turn that is not a right angle gives the box that holds it).
    pub fn keepout_areas(&self) -> Vec<KeepoutArea> {
        self.keepouts
            .iter()
            .filter_map(|k| {
                let at = k.at?;
                let local = Rect::new(at, Pt::new(at.x + k.size.0, at.y + k.size.1));
                let rect = match &k.part {
                    None => local.translated(self.outline.min),
                    Some(path) => {
                        let part = self.parts.iter().find(|p| &p.path == path)?;
                        Rect::around(local.corners().map(|c| part.to_board(c)))?
                    }
                };
                Some(KeepoutArea {
                    name: k.name.clone(),
                    carrier: k.part.clone(),
                    rect,
                    layers: k.layers.clone(),
                    tracks: k.tracks,
                    vias: k.vias,
                    copper: k.copper,
                    parts: k.parts,
                    max_height: k.max_height,
                })
            })
            .collect()
    }

    /// The areas that limit how tall a part may stand, for the placer.
    pub(crate) fn height_zones(&self) -> Vec<crate::place::HeightZone> {
        self.keepout_areas()
            .into_iter()
            .filter(|k| k.carrier.is_none())
            .filter_map(|k| {
                Some(crate::place::HeightZone {
                    front: k.limits_height_on(Side::Front),
                    back: k.limits_height_on(Side::Back),
                    limit: k.max_height?,
                    name: k.name,
                    rect: k.rect,
                })
            })
            .collect()
    }

    /// The keepouts that parts carry, each in its part's own frame, for the placer.
    pub(crate) fn carried_zones(&self) -> Vec<crate::place::CarriedZone> {
        self.keepouts
            .iter()
            .filter_map(|k| {
                let at = k.at?;
                let on = |layer: &str| k.layers.iter().any(|l| l == layer);
                Some(crate::place::CarriedZone {
                    name: k.name.clone(),
                    part: k.part.clone()?,
                    local: Rect::new(at, Pt::new(at.x + k.size.0, at.y + k.size.1)),
                    parts: k.parts,
                    front: on("top"),
                    back: on("bottom"),
                    limit: k.max_height,
                })
            })
            .collect()
    }

    /// How far a box is from the board edge: negative if any of it is outside.
    pub fn edge_distance(&self, r: &Rect) -> f64 {
        let o = &self.outline;
        let straight = (r.min.x - o.min.x)
            .min(r.min.y - o.min.y)
            .min(o.max.x - r.max.x)
            .min(o.max.y - r.max.y);
        if self.corner_radius == 0.0 {
            return straight;
        }
        // A box inside a convex shape is nearest the edge at one of its corners.
        let shape = self.outline_shape();
        r.corners()
            .iter()
            .map(|&c| shape.depth(c))
            .fold(straight, f64::min)
    }

    /// How far a pad's copper is from the board edge: negative if any of it is outside. A round
    /// pad is measured as the circle it is, so that a plated mounting hole near a rounded
    /// corner is not taken for a pad whose box corner nearly touches the edge; any other pad is
    /// measured by its box.
    fn pad_edge_distance(&self, a: &Placed, pad: &Pad) -> f64 {
        let round = pad.shape == crate::footprint::PadShape::Circle
            && pad.extent.is_none()
            && (pad.width - pad.height).abs() < 1e-9;
        if round {
            return self.outline_shape().depth(a.pad_centre(pad)) - pad.width / 2.0;
        }
        self.edge_distance(&a.pad_bounds(pad))
    }

    /// The net a pad is on, if any.
    pub fn net_of(&self, refdes: &str, number: &str) -> Option<&BoardNet> {
        self.nets.iter().find(|n| {
            n.pads
                .iter()
                .any(|p| p.refdes == refdes && p.number == number)
        })
    }

    /// The holes of `a` that are nearer a hole or the copper of `b` than the rules allow.
    ///
    /// `holes` says whether the distance between two holes is measured: it is the same from
    /// either part, so a pair is checked from one of them only; the distance from a hole to
    /// the other part's copper is not the same both ways.
    fn check_holes(
        &self,
        a: &Placed,
        b: &Placed,
        holes: bool,
        problems: &mut Vec<PlacementProblem>,
    ) {
        for pad in &a.footprint.pads {
            let Some((p, q, radius)) = a.hole(pad) else {
                continue;
            };
            let mut report = |to, other_pad: &str, distance: f64, least: f64| {
                if distance < least - 1e-9 {
                    problems.push(PlacementProblem::HoleTooNear {
                        refdes: a.refdes.clone(),
                        pad: pad.number.clone(),
                        other: b.refdes.clone(),
                        other_pad: other_pad.to_string(),
                        to,
                        distance,
                        least,
                    });
                }
            };
            for other in &b.footprint.pads {
                if let (true, Some((r, s, other_radius))) = (holes, b.hole(other)) {
                    let apart = segment_distance((p, q), (r, s)) - radius - other_radius;
                    report(
                        HoleNeighbour::Hole,
                        &other.number,
                        apart,
                        self.rules.hole_to_hole,
                    );
                }
                if b.pad_layers(other) != (false, false) {
                    let apart = segment_to_rect((p, q), &b.pad_bounds(other)) - radius;
                    report(
                        HoleNeighbour::Copper,
                        &other.number,
                        apart,
                        self.rules.clearance,
                    );
                }
            }
        }
    }

    /// The requirements of `design`'s physical file that the parts are not at, and the lines
    /// that cannot all hold where that is proved (ADR 0045): what to say beside
    /// the faults [`Board::check_placement`] finds when the placer could not meet them.
    /// `options` is what the layout file and the caller said ([`place_options`]). `None` when
    /// the design states no requirement or all are met.
    pub fn explain_requirements(
        &self,
        design: &Design,
        options: &PlaceOptions,
    ) -> Option<crate::explain::RequirementReport> {
        crate::explain::explain(design, &self.parts, self.outline, options, &self.rules)
    }

    /// The courtyards that are past an edge of the outline, where the design says the part may
    /// hang over it, with how far: what [`Board::check_placement`] leaves out for a stated
    /// overhang, so that a person or a fabrication note can still read it.
    pub fn hangs(&self) -> Vec<Hang> {
        let mut found = Vec::new();
        for part in &self.parts {
            let Some(stated) = self.overhang.get(&part.refdes) else {
                continue;
            };
            let may = stated.edges();
            for (edge, by) in overhang::past(&part.courtyard(), &self.outline) {
                if may.has(edge) {
                    found.push(Hang {
                        refdes: part.refdes.clone(),
                        edge,
                        by,
                        stated: stated.by(),
                    });
                }
            }
        }
        found
    }

    /// The through-hole parts whose pin protrusion the design states
    /// (ADR 0045 section 15), each with the parts that lie over them. The
    /// manufacturing step they stand for is the person's to do.
    pub fn trims(&self) -> Vec<Trim> {
        let zones = self.height_zones();
        let mut found = Vec::new();
        for t in &self.parts {
            let Some(protrusion) = &t.protrusion else {
                continue;
            };
            if !t.has_hole() {
                continue;
            }
            let reach: Vec<Rect> = t
                .footprint
                .pads
                .iter()
                .map(|p| t.pad_bounds(p).expanded(self.rules.clearance))
                .collect();
            let mut covers = Vec::new();
            let mut unlimited = Vec::new();
            for c in &self.parts {
                if crate::protrusion::pins_under(t, c).is_none() {
                    continue;
                }
                let court = c.courtyard();
                if !reach.iter().any(|r| r.overlaps(&court)) {
                    continue;
                }
                covers.push((c.refdes.clone(), c.path.clone()));
                if let (_, crate::protrusion::Verdict::Excused { limit: None }) = (
                    (),
                    crate::protrusion::judge(
                        protrusion.length,
                        &crate::protrusion::cover_of(c),
                        &zones,
                    ),
                ) {
                    unlimited.push((c.refdes.clone(), c.path.clone()));
                }
            }
            found.push(Trim {
                refdes: t.refdes.clone(),
                path: t.path.clone(),
                length: protrusion.length,
                source: protrusion.source.clone(),
                face: if t.side == Side::Front {
                    Side::Back
                } else {
                    Side::Front
                },
                covers,
                unlimited,
            });
        }
        found
    }

    /// The pairs of parts, by designator, whose courtyards overlap only because one stands
    /// over the other's protruding pins and the rule of [`crate::protrusion`] excuses it, for the
    /// design rules check's courtyard rule.
    pub fn excused_courtyards(&self) -> Vec<(String, String)> {
        let zones = self.height_zones();
        let mut pairs = Vec::new();
        for trim in self.trims() {
            let Some(t) = self.part(&trim.refdes) else {
                continue;
            };
            for (cover, _) in &trim.covers {
                let Some(c) = self.part(cover) else { continue };
                if crate::protrusion::excused(t, c, &zones).is_some() {
                    pairs.push((trim.refdes.clone(), cover.clone()));
                }
            }
        }
        pairs
    }

    /// The parts that stand under a height limit and state no height, so that whether they
    /// fit is not known (ADR 0045, "Height limits"). Not a pass: each is named,
    /// with the keepout and its limit. A label has no body and is left out.
    pub fn heights_unchecked(&self) -> Vec<HeightUnchecked> {
        let keepouts = self.keepout_areas();
        let mut found = Vec::new();
        for a in self
            .parts
            .iter()
            .filter(|a| a.height.is_none() && a.kind != "Label")
        {
            let body = a.body();
            for k in keepouts
                .iter()
                .filter(|k| k.carrier.as_deref() != Some(a.path.as_str()))
            {
                if let Some(limit) = k.max_height
                    && k.limits_height_on(a.side)
                    && body.overlaps(&k.rect)
                {
                    found.push(HeightUnchecked {
                        refdes: a.refdes.clone(),
                        path: a.path.clone(),
                        keepout: k.name.clone(),
                        limit,
                        side: a.side,
                    });
                }
            }
        }
        found
    }

    /// Everything wrong with where the parts are: courtyards that overlap, courtyards that
    /// stick out of the outline, pads nearer the edge than `rules.copper_to_edge`, and holes
    /// nearer the holes and copper of other parts than the process allows. Empty when the
    /// placement is valid.
    ///
    /// A part the design says may hang over an edge ([`Board::overhang`]) is not reported
    /// past that edge, and takes only the room its courtyard has inside the outline: that is
    /// the part of it that overlaps other courtyards, keepouts and cutouts. Its pads are
    /// checked as ever.
    pub fn check_placement(&self) -> Vec<PlacementProblem> {
        let courtyards: Vec<Rect> = self.parts.iter().map(Placed::courtyard).collect();
        // Where a part's room is: its courtyard, clipped to the outline if it may hang over.
        let may: Vec<crate::overhang::Edges> = self
            .parts
            .iter()
            .map(|p| {
                self.overhang
                    .get(&p.refdes)
                    .map(Overhang::edges)
                    .unwrap_or_default()
            })
            .collect();
        let rooms: Vec<Rect> = courtyards
            .iter()
            .zip(&may)
            .map(|(c, m)| {
                if m.any() {
                    overhang::clip(c, &self.outline)
                } else {
                    *c
                }
            })
            .collect();
        let cutouts = self.cutout_shapes();
        let zones = self.height_zones();
        let mut problems = Vec::new();
        for c in self.cutouts.iter().filter(|c| c.at.is_none()) {
            problems.push(PlacementProblem::NoPosition {
                what: "cutout",
                name: c.name.clone(),
            });
        }
        for k in self.keepouts.iter().filter(|k| k.at.is_none()) {
            problems.push(PlacementProblem::NoPosition {
                what: "keepout",
                name: k.name.clone(),
            });
        }
        let keepouts = self.keepout_areas();
        for (cutout, hole) in &cutouts {
            if !self.outline.contains(&hole.rect) {
                problems.push(PlacementProblem::CutoutOutsideOutline {
                    cutout: cutout.name.clone(),
                });
            }
        }
        for (i, a) in self.parts.iter().enumerate() {
            let shape = self.outline_shape();
            let declared = may[i];
            let past: Vec<(Edge, f64)> = overhang::past(&courtyards[i], &self.outline)
                .into_iter()
                .filter(|&(edge, _)| !declared.has(edge))
                .collect();
            // The corners of the room, but not those on an edge the part may hang over, which
            // are the board's edge and not its rounded corners.
            let o = &self.outline;
            let in_corner = rooms[i].corners().iter().any(|&c| {
                let on_edge = (declared.left && c.x <= o.min.x + 1e-6)
                    || (declared.right && c.x >= o.max.x - 1e-6)
                    || (declared.top && c.y <= o.min.y + 1e-6)
                    || (declared.bottom && c.y >= o.max.y - 1e-6);
                !on_edge && !shape.contains(c)
            });
            if !past.is_empty() || in_corner {
                let stated = self.overhang.get(&a.refdes);
                problems.push(PlacementProblem::OutsideOutline {
                    refdes: a.refdes.clone(),
                    path: a.path.clone(),
                    past,
                    stated: declared.list(),
                    by: stated.map(Overhang::by).unwrap_or_default(),
                });
            }
            for k in keepouts
                .iter()
                .filter(|k| k.carrier.as_deref() != Some(a.path.as_str()))
            {
                if k.parts && rooms[i].overlaps(&k.rect) {
                    problems.push(PlacementProblem::InKeepout {
                        refdes: a.refdes.clone(),
                        keepout: k.name.clone(),
                    });
                }
                let trapped = k.tracks
                    && a.footprint.pads.iter().any(|p| {
                        let (front, back) = p.copper_layers();
                        ((front && k.on(CopperLayer::Front)) || (back && k.on(CopperLayer::Back)))
                            && a.pad_bounds(p).overlaps(&k.rect)
                    });
                if trapped {
                    problems.push(PlacementProblem::PadInKeepout {
                        refdes: a.refdes.clone(),
                        keepout: k.name.clone(),
                    });
                }
            }
            // A part taller than the limit of a keepout it stands under, on its face. The body
            // is what stands there (the footprint's fabrication outline, else its courtyard).
            if let Some(height) = a.height {
                let body = a.body();
                for k in keepouts
                    .iter()
                    .filter(|k| k.carrier.as_deref() != Some(a.path.as_str()))
                {
                    let Some(limit) = k.max_height else { continue };
                    if k.limits_height_on(a.side)
                        && body.overlaps(&k.rect)
                        && height > limit + HEIGHT_TOLERANCE
                    {
                        let centre = body.centre();
                        problems.push(PlacementProblem::TooTall {
                            refdes: a.refdes.clone(),
                            path: a.path.clone(),
                            keepout: k.name.clone(),
                            height,
                            limit,
                            side: a.side,
                            at: (centre.x - self.outline.min.x, centre.y - self.outline.min.y),
                        });
                    }
                }
            }
            for (cutout, hole) in &cutouts {
                if rooms[i].overlaps(&hole.rect) {
                    problems.push(PlacementProblem::InCutout {
                        refdes: a.refdes.clone(),
                        cutout: cutout.name.clone(),
                    });
                }
                let nearest = a
                    .footprint
                    .pads
                    .iter()
                    .filter(|p| p.copper_layers() != (false, false))
                    .map(|p| rect_distance(&a.pad_bounds(p), &hole.rect))
                    .fold(f64::MAX, f64::min);
                if nearest < self.rules.copper_to_edge - 1e-9 && !rooms[i].overlaps(&hole.rect) {
                    problems.push(PlacementProblem::PadNearCutout {
                        refdes: a.refdes.clone(),
                        cutout: cutout.name.clone(),
                        distance: nearest,
                    });
                }
            }
            let nearest = a
                .footprint
                .pads
                .iter()
                .filter(|p| a.pad_layers(p) != (false, false))
                .map(|p| self.pad_edge_distance(a, p))
                .fold(f64::MAX, f64::min);
            if nearest < self.rules.copper_to_edge - 1e-9 {
                problems.push(PlacementProblem::PadNearEdge {
                    refdes: a.refdes.clone(),
                    distance: nearest,
                });
            }
            for (j, b) in self.parts.iter().enumerate().skip(i + 1) {
                // Parts on opposite faces may share the board's area, unless one has a hole
                // through the board that takes room there (`Placed::room_on`).
                if a.rooms_overlap(rooms[i], b, rooms[j], self.rules.clearance, &zones) {
                    problems.push(PlacementProblem::overlap_of(a, b, &zones));
                }
                self.check_holes(a, b, true, &mut problems);
                self.check_holes(b, a, false, &mut problems);
            }
        }
        problems
    }
}

/// The gap between two boxes, zero if they touch or overlap.
pub(crate) fn rect_distance(a: &Rect, b: &Rect) -> f64 {
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

#[cfg(test)]
pub(crate) mod tests {
    use netsmith_core::{Net, Part, PinRef};

    use super::*;

    pub fn fixtures() -> FootprintLibrary {
        FootprintLibrary::at(netsmith_kicad::testlib::sample_footprints())
    }

    fn part(refdes: &str, kind: &str, value: &str, package: Option<&str>) -> Part {
        Part {
            // What the design stage records for a standard connector (ADR 0068).
            provides: if kind.starts_with("Header") || kind.starts_with("Socket") {
                vec![netsmith_core::CONNECTOR_GENERIC.to_string()]
            } else {
                Vec::new()
            },
            refdes: refdes.into(),
            kind: kind.into(),
            value: value.into(),
            package: package.map(str::to_string),
            path: String::new(),
            pins: Vec::new(),
        }
    }

    fn net(name: &str, pins: &[(&str, &str)]) -> Net {
        Net {
            name: name.into(),
            pins: pins
                .iter()
                .map(|(r, p)| PinRef {
                    refdes: (*r).into(),
                    pin: (*p).into(),
                })
                .collect(),
        }
    }

    /// The same circuit as `examples/led.nsm`.
    pub fn led_design() -> Design {
        Design {
            name: "LedExample".into(),
            process: Some(crate::rules::test_process()),
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts: vec![
                part("D1", "Led", "LED", None),
                part("J1", "Header1x2", "Conn_01x02", None),
                part("R1", "Resistor", "300", Some("0402")),
            ],
            nets: vec![
                net("GND", &[("D1", "1"), ("J1", "2")]),
                net("VCC", &[("J1", "1"), ("R1", "1")]),
                net("net-led.anode", &[("D1", "2"), ("R1", "2")]),
            ],
        }
    }

    /// The LED board on four layers: ground and 3.3 V planes, and the supply net called `3V3`.
    pub fn four_layer_design() -> Design {
        let mut design = led_design();
        design.process = Some(crate::rules::four_layer_process(&[]));
        design.nets[1].name = "3V3".into();
        design
    }

    #[test]
    fn the_inner_layers_of_a_four_layer_board_are_planes_on_the_nets_the_board_named() {
        let board = Board::from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let planes: Vec<(&str, &str)> = board
            .planes
            .iter()
            .map(|p| (p.layer.as_str(), p.net.as_str()))
            .collect();
        assert_eq!(planes, [("In1.Cu", "GND"), ("In2.Cu", "3V3")]);
        assert!(board.rules.plane_clearance >= board.rules.clearance);
        let two = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        assert_eq!(two.planes, vec![]);
    }

    #[test]
    fn a_plane_on_a_net_the_design_does_not_have_names_the_layer_and_the_net() {
        // The design's supply net is called VCC, not 3V3.
        let mut design = four_layer_design();
        design.nets[1].name = "VCC".into();
        let text = Board::from_design(&design, &netsmith_kicad::Mapping::standard(), &fixtures())
            .unwrap_err()
            .to_string();
        assert!(
            text.contains("In2.Cu is a plane on the net `3V3`")
                && text.contains("`LedExample` has no net of that name"),
            "{text}"
        );
    }

    #[test]
    fn the_process_note_says_which_net_each_plane_is() {
        let board = Board::from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let note = board.process_note();
        assert!(
            note.contains(
                "4 layers (In1.Cu is a plane on GND, In2.Cu is a plane on 3V3), 1.6 mm thick"
            ),
            "{note}"
        );
        assert!(note.contains("track 0.09 mm"), "{note}");
        let two = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        assert!(two.process_note().contains("; 2 layers, 1.6 mm thick"));
    }

    #[test]
    fn a_design_with_no_process_is_refused_with_what_to_add() {
        let mut design = led_design();
        design.process = None;
        let text = Board::from_design(&design, &netsmith_kicad::Mapping::standard(), &fixtures())
            .unwrap_err()
            .to_string();
        assert!(
            text.contains("`LedExample` does not say how it is made"),
            "{text}"
        );
        assert!(text.contains(".process: TwoLayer"), "{text}");
        assert!(text.contains("import std::mfg::jlcpcb::TwoLayer"), "{text}");
    }

    #[test]
    fn a_board_that_has_not_chosen_a_thickness_says_the_range() {
        let mut design = led_design();
        design.process.as_mut().unwrap().stackup.thickness = (0.4, 2.0);
        let text = Board::from_design(&design, &netsmith_kicad::Mapping::standard(), &fixtures())
            .unwrap_err()
            .to_string();
        assert!(text.contains("offers 0.4 mm to 2 mm"), "{text}");
        assert!(
            text.contains(".process.stackup.thickness &= 1.6mm"),
            "{text}"
        );
    }

    #[test]
    fn builds_a_board_from_a_design() {
        let board = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        assert_eq!(board.name, "LedExample");
        let refs: Vec<&str> = board.parts.iter().map(|p| p.refdes.as_str()).collect();
        assert_eq!(refs, ["D1", "J1", "R1"]);
        assert_eq!(
            board.part("D1").unwrap().library_id,
            "LED_SMD:LED_0603_1608Metric"
        );
        assert_eq!(board.part("R1").unwrap().value, "300");
        assert_eq!(board.nets.len(), 3);
        assert_eq!(board.net_of("R1", "2").unwrap().name, "net-led.anode");
        assert!(board.net_of("R1", "9").is_none());
    }

    #[test]
    fn placement_is_valid_and_the_same_every_time() {
        let board = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        assert_eq!(board.check_placement(), vec![]);
        assert_eq!(
            board,
            Board::from_design(
                &led_design(),
                &netsmith_kicad::Mapping::standard(),
                &fixtures()
            )
            .unwrap()
        );
    }

    #[test]
    fn a_part_on_the_back_is_the_footprint_in_a_mirror_with_its_pads_on_the_back_layer() {
        let mut board = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let mut front = board.part("R1").unwrap().clone();
        front.rotation = 0.0;
        let pad2 = front.footprint.pads_numbered("2").next().unwrap().clone();
        let r1 = board.parts.iter_mut().find(|p| p.refdes == "R1").unwrap();
        r1.at = Pt::new(10.0, 10.0);
        r1.rotation = 0.0;
        let before = r1.pad_centre(&pad2);
        r1.side = Side::Back;
        let after = r1.pad_centre(&pad2);
        // Pad 2 is on the right of the footprint; on the back it is on the left.
        assert!((before.x - 10.0) > 0.0 && (after.x - 10.0) < 0.0);
        assert!((before.x - 10.0 + (after.x - 10.0)).abs() < 1e-9 && before.y == after.y);
        assert_eq!(r1.pad_layers(&pad2), (false, true));
        assert_eq!(front.pad_layers(&pad2), (true, false));
        assert_eq!(r1.pad_bounds(&pad2).centre().x, after.x);
        assert_eq!(r1.courtyard().width(), front.courtyard().width());
        // Turned by 90 degrees, the mirrored pad is where the mirror and the turn put it.
        r1.rotation = 90.0;
        let turned = r1.pad_centre(&pad2);
        assert!(close(
            turned,
            Pt::new(10.0 + (after.y - 10.0), 10.0 - (after.x - 10.0))
        ));
        assert_eq!(r1.pad_angle(&pad2), 90.0 - pad2.angle);
    }

    #[test]
    fn courtyards_of_opposite_faces_may_overlap_unless_one_part_has_a_hole() {
        let mut board = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let d1 = board.part("D1").unwrap().at;
        {
            let r1 = board.parts.iter_mut().find(|p| p.refdes == "R1").unwrap();
            r1.at = d1;
            r1.side = Side::Back;
        }
        assert_eq!(board.check_placement(), vec![]);
        // The same place on the same face is an overlap.
        board
            .parts
            .iter_mut()
            .find(|p| p.refdes == "R1")
            .unwrap()
            .side = Side::Front;
        assert!(
            board
                .check_placement()
                .contains(&PlacementProblem::Overlap {
                    a: "D1".into(),
                    b: "R1".into(),
                    pins: None,
                })
        );
        // A header has holes through the board, so a part on the other face under it clashes.
        let j1 = board.part("J1").unwrap().at;
        let r1 = board.parts.iter_mut().find(|p| p.refdes == "R1").unwrap();
        r1.at = j1;
        r1.side = Side::Back;
        assert!(board.check_placement().iter().any(|p| matches!(
            p, PlacementProblem::Overlap { a, b, .. } if (a == "J1" && b == "R1") || (a == "R1" && b == "J1")
        )));
    }

    #[test]
    fn the_check_finds_overlaps_and_parts_off_the_board() {
        let mut board = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let r1 = board.part("R1").unwrap().at;
        let d1 = board.parts.iter_mut().find(|p| p.refdes == "D1").unwrap();
        d1.at = r1 + Pt::new(0.5, 0.0);
        let j1 = board.parts.iter_mut().find(|p| p.refdes == "J1").unwrap();
        j1.at = Pt::new(1000.0, 0.0);
        let problems = board.check_placement();
        assert!(problems.contains(&PlacementProblem::Overlap {
            a: "D1".into(),
            b: "R1".into(),
            pins: None,
        }));
        assert!(problems.iter().any(|p| matches!(
            p,
            PlacementProblem::OutsideOutline { refdes, .. } if refdes == "J1"
        )));
        assert!(matches!(
            problems.last(),
            Some(PlacementProblem::PadNearEdge { refdes, .. }) if refdes == "J1"
        ));
        assert_eq!(problems.len(), 3);
    }

    /// The led board with `refdes` moved so that its courtyard's right side is `past` mm past
    /// the right edge (negative: inside it), and what `check_placement` says of that part.
    fn moved_past_the_right_edge(refdes: &str, past: f64) -> Board {
        let mut board = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let edge = board.outline.max.x;
        let part = board.parts.iter_mut().find(|p| p.refdes == refdes).unwrap();
        let right = part.courtyard().max.x;
        part.at.x += edge + past - right;
        board
    }

    fn about(board: &Board, refdes: &str) -> Vec<PlacementProblem> {
        board
            .check_placement()
            .into_iter()
            .filter(|p| match p {
                PlacementProblem::OutsideOutline { refdes: r, .. }
                | PlacementProblem::PadNearEdge { refdes: r, .. } => r == refdes,
                _ => false,
            })
            .collect()
    }

    fn stated(edge: Edge) -> Overhang {
        let mut edges = crate::overhang::Edges::default();
        edges.set(edge);
        Overhang::Stated {
            edges,
            says: "`j1.place.right (a.nsm:3)` allows at least 36 mm".into(),
        }
    }

    #[test]
    fn a_courtyard_past_an_edge_nobody_stated_is_outside_and_says_how_far() {
        let board = moved_past_the_right_edge("J1", 0.5);
        let found = about(&board, "J1");
        assert_eq!(found.len(), 1, "{found:?}");
        let text = found[0].to_string();
        let path = &board.part("J1").unwrap().path;
        assert_eq!(
            text,
            format!(
                "J1 is not wholly inside the board outline: its courtyard is 0.50 mm past the right edge, and the design does not say it may hang over an edge; to allow it past the right edge, require the body to reach it, for example `.{path}.place.right >= outline.width`"
            )
        );
    }

    #[test]
    fn a_courtyard_past_the_edge_the_design_states_is_not_reported_and_one_past_another_is() {
        let mut board = moved_past_the_right_edge("J1", 0.5);
        board.overhang.insert("J1".into(), stated(Edge::Right));
        assert_eq!(
            about(&board, "J1"),
            vec![],
            "a stated overhang is not a finding"
        );
        assert_eq!(board.hangs().len(), 1);
        assert!((board.hangs()[0].by - 0.5).abs() < 1e-6);
        // Stating another edge does not help.
        board.overhang.insert("J1".into(), stated(Edge::Bottom));
        let found = about(&board, "J1");
        assert_eq!(found.len(), 1, "{found:?}");
        let text = found[0].to_string();
        assert!(
            text.contains("its courtyard is 0.50 mm past the right edge")
                && text.contains("the design says it may hang over only the bottom edge (`j1.place.right (a.nsm:3)` allows at least 36 mm)"),
            "{text}"
        );
    }

    #[test]
    fn a_pad_outside_is_a_finding_even_where_the_overhang_is_stated() {
        // The header's pads are 0.95 mm inside its courtyard's right side.
        let mut board = moved_past_the_right_edge("J1", 2.0);
        board.overhang.insert("J1".into(), stated(Edge::Right));
        let found = about(&board, "J1");
        assert_eq!(found.len(), 1, "{found:?}");
        let text = found[0].to_string();
        assert!(
            text.starts_with("a pad of J1 is ") && text.ends_with(" mm outside the board edge"),
            "{text}"
        );
        // Just far enough in that the copper keeps the process's space from the edge.
        let mut board = moved_past_the_right_edge("J1", 0.95 - board.rules.copper_to_edge);
        board.overhang.insert("J1".into(), stated(Edge::Right));
        assert_eq!(about(&board, "J1"), vec![]);
    }

    #[test]
    fn a_stated_overhang_overlaps_other_parts_only_inside_the_outline() {
        let mut board = moved_past_the_right_edge("R1", 1.0);
        board.overhang.insert("R1".into(), stated(Edge::Right));
        board.overhang.insert("D1".into(), stated(Edge::Right));
        let overlaps = |board: &Board| {
            board
                .check_placement()
                .into_iter()
                .filter(|p| matches!(p, PlacementProblem::Overlap { .. }))
                .count()
        };
        // D1 is put on R1, so that their courtyards share the area beyond the edge only.
        let edge = board.outline.max.x;
        let r1 = board.part("R1").unwrap().courtyard();
        let d1 = board.parts.iter_mut().find(|p| p.refdes == "D1").unwrap();
        let c = d1.courtyard();
        d1.at = Pt::new(d1.at.x + edge - c.min.x, d1.at.y + r1.min.y - c.min.y);
        assert_eq!(overlaps(&board), 0, "the shared area is beyond the edge");
        // One that reaches into the board overlaps there.
        let d1 = board.parts.iter_mut().find(|p| p.refdes == "D1").unwrap();
        d1.at.x -= 0.4;
        assert_eq!(overlaps(&board), 1);
    }

    #[test]
    fn a_pad_too_near_the_edge_is_found() {
        let near_edge = |board: &Board| -> Vec<PlacementProblem> {
            board
                .check_placement()
                .into_iter()
                .filter(|p| matches!(p, PlacementProblem::PadNearEdge { .. }))
                .collect()
        };
        let mut board = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        assert_eq!(board.check_placement(), vec![]);
        // The leftmost pad of the board, wherever the placer put it.
        let (leftmost, left) = board
            .parts
            .iter()
            .flat_map(|p| {
                p.footprint
                    .pads
                    .iter()
                    .map(move |pad| (p.refdes.clone(), p.pad_bounds(pad).min.x))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        board.outline.min.x = left - 0.1;
        // Every part whose leftmost pad is as far left as that one is too near the edge.
        let problems = near_edge(&board);
        assert!(!problems.is_empty());
        let mut found = false;
        for problem in &problems {
            match problem {
                PlacementProblem::PadNearEdge { refdes, distance } => {
                    assert!(
                        (distance - 0.1).abs() < 1e-9,
                        "{refdes} is {distance} mm off"
                    );
                    found |= *refdes == leftmost;
                }
                other => panic!("{other}"),
            }
        }
        assert!(found, "{leftmost} is not named: {problems:?}");
        board.outline.min.x = left - board.rules.copper_to_edge;
        assert_eq!(near_edge(&board), vec![], "exactly the rule is allowed");
    }

    /// A round pad of a mounting hole in a rounded corner is measured as the circle it is: its
    /// box corner is nearly at the arc (0.05 mm), the copper is 1.09 mm from it.
    #[test]
    fn a_round_pad_near_a_rounded_corner_is_measured_as_a_circle() {
        let mut board = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        board.outline = Rect {
            min: Pt::new(0.0, 0.0),
            max: Pt::new(50.0, 50.0),
        };
        board.corner_radius = 5.0;
        let mut part = board.parts[0].clone();
        let mut pad = part.footprint.pads[0].clone();
        pad.shape = crate::footprint::PadShape::Circle;
        pad.width = 5.0;
        pad.height = 5.0;
        pad.at = Pt::new(0.0, 0.0);
        pad.extent = None;
        part.at = Pt::new(4.0, 4.0);
        part.rotation = 0.0;
        let circle = board.pad_edge_distance(&part, &pad);
        let by_box = board.edge_distance(&part.pad_bounds(&pad));
        let expected = 5.0 - 2f64.sqrt() - 2.5;
        assert!((circle - expected).abs() < 1e-9, "{circle}");
        assert!(by_box < 0.06, "{by_box}");
        // A pad that is not round is still measured by its box.
        pad.shape = crate::footprint::PadShape::Rect;
        assert!((board.pad_edge_distance(&part, &pad) - by_box).abs() < 1e-9);
    }

    fn close(a: Pt, b: Pt) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn a_rotated_part_moves_its_pads_and_courtyard() {
        let board = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let mut r1 = board.part("R1").unwrap().clone();
        r1.at = Pt::new(10.0, 10.0);
        r1.rotation = 0.0;
        let pad2 = r1.footprint.pads[1].clone();
        assert!(close(r1.pad_centre(&pad2), Pt::new(10.51, 10.0)));
        assert!((r1.courtyard().width() - 1.86).abs() < 1e-9);
        r1.rotation = 90.0;
        assert!(close(r1.pad_centre(&pad2), Pt::new(10.0, 9.49)));
        assert_eq!(r1.pad_angle(&pad2), 90.0);
        assert!((r1.courtyard().height() - 1.86).abs() < 1e-9);
    }

    #[test]
    fn a_net_that_names_a_missing_pad_says_which() {
        let mut design = led_design();
        design.nets[0].pins[0].pin = "7".into();
        let err = Board::from_design(&design, &netsmith_kicad::Mapping::standard(), &fixtures())
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "net GND joins pad 7 of D1, but footprint LED_SMD:LED_0603_1608Metric has only pads 1, 2"
        );
    }

    #[test]
    fn a_part_with_no_footprint_is_named() {
        let mut design = led_design();
        design.parts[2].kind = "Gizmo".into();
        let err = Board::from_design(&design, &netsmith_kicad::Mapping::standard(), &fixtures())
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "R1: there is no KiCad symbol for a `Gizmo` yet"
        );
    }

    #[test]
    fn a_missing_library_file_names_the_footprint_and_path() {
        let empty = FootprintLibrary::at("/nonexistent/footprints");
        let text = Board::from_design(&led_design(), &netsmith_kicad::Mapping::standard(), &empty)
            .unwrap_err()
            .to_string();
        assert!(text.starts_with("D1: footprint `LED_SMD:LED_0603_1608Metric` not found"));
        assert!(
            text.contains("/nonexistent/footprints/LED_SMD.pretty/LED_0603_1608Metric.kicad_mod")
        );
    }
}
