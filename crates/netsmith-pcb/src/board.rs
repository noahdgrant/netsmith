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

/// Which face of the board a part is on.
///
/// A part on the back is the front footprint seen in a mirror along its y axis (its x is
/// negated) and then turned by its rotation, as KiCad flips a footprint to the other side:
/// its pads are on `B.Cu` and its silkscreen and courtyard on the `B.` layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// Where its symbol is in the schematic the build writes ([`netsmith_kicad::Hierarchy`]),
    /// so that KiCad's update of the board from the schematic finds the footprint of each
    /// symbol. `None` for a label, which has no symbol.
    #[serde(default)]
    pub symbol: Option<netsmith_kicad::SymbolLink>,
}

impl Placed {
    /// The _Standard height_ of the part's footprint (ADR 0106,
    /// [`crate::standard_height`]), when the part has no height of its own: `None` when the
    /// design or the catalog row gives one, which always wins, and for a footprint the table has
    /// no row for.
    pub fn standard_height(&self) -> Option<&'static crate::standard_height::StandardHeight> {
        if self.height.is_some() || self.kind == "Label" {
            return None;
        }
        crate::standard_height::of(&self.library_id)
    }

    /// The height of the reference text on the board, in mm: what the library draws, or the
    /// process's smallest if that is more.
    pub fn text_height(&self, rules: &Rules) -> f64 {
        self.footprint.reference_size.max(rules.min_text_height)
    }

    /// The width of the reference text, in mm: about one text height for each letter.
    pub fn text_width(&self, rules: &Rules) -> f64 {
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
        zones: &[crate::placement::HeightZone],
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

    /// The largest area two parts share on one face, in mm2, with their rooms as
    /// [`Self::rooms_overlap`] counts them (the courtyards as they lie; the pads grown by
    /// `clearance` where a footprint draws its courtyard on one face only): what the
    /// placement check gives with an overlap, for its size.
    pub fn shared_area(
        &self,
        other: &Placed,
        clearance: f64,
        zones: &[crate::placement::HeightZone],
    ) -> f64 {
        let (court_a, court_b) = (self.courtyard(), other.courtyard());
        let excused = crate::protrusion::excused(self, other, zones);
        let share = |a: &Rect, b: &Rect| {
            (a.max.x.min(b.max.x) - a.min.x.max(b.min.x)).max(0.0)
                * (a.max.y.min(b.max.y) - a.min.y.max(b.min.y)).max(0.0)
        };
        [Side::Front, Side::Back]
            .into_iter()
            .map(|face| {
                let plain = || {
                    (
                        self.room_on(face, clearance, court_a),
                        other.room_on(face, clearance, court_b),
                    )
                };
                let (mine, theirs) = match excused {
                    Some(pins_first) => {
                        let (protrusion, cover) = if pins_first {
                            (self, other)
                        } else {
                            (other, self)
                        };
                        if face == cover.side {
                            let (t, c) =
                                crate::protrusion::excused_rooms(protrusion, cover, clearance);
                            if pins_first { (t, c) } else { (c, t) }
                        } else {
                            plain()
                        }
                    }
                    None => plain(),
                };
                mine.iter()
                    .flat_map(|a| theirs.iter().map(move |b| share(a, b)))
                    .fold(0.0, f64::max)
            })
            .fold(0.0, f64::max)
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
    pub fn rect_to_board(&self, r: Rect) -> Rect {
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

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
        /// The members' paths in the design (empty for a part that has none), which the
        /// message names them by, with the designator beside.
        a_path: String,
        b_path: String,
        /// The largest area the two share on one face, in mm2 (their rooms as the check
        /// counts them: the courtyards, or the pads where a footprint draws its courtyard on
        /// one face only). Zero where they only touch within the clearance.
        area: f64,
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
    /// box is inside and a corner of it is outside a rounded corner of the outline, or, on a
    /// polygon outline, outside its edge between the sides of its bounding box).
    OutsideOutline {
        refdes: String,
        /// The part's path, for the line that would allow the overhang.
        path: String,
        past: Vec<(Edge, f64)>,
        /// The edges the design does say the part may hang over, and by what.
        stated: Vec<Edge>,
        by: String,
        /// On a polygon outline, how far the courtyard reaches past its edge where it is not
        /// past a side of the bounding box, in mm (issue #57).
        cut: Option<f64>,
    },
    /// A pad is nearer the board edge than the rules allow, or outside it (a negative
    /// distance).
    PadNearEdge {
        refdes: String,
        path: String,
        distance: f64,
    },
    /// A part's courtyard overlaps a cutout.
    InCutout {
        refdes: String,
        path: String,
        cutout: String,
    },
    /// A pad is nearer the edge of a cutout than the rules allow.
    PadNearCutout {
        refdes: String,
        path: String,
        cutout: String,
        distance: f64,
    },
    /// A part's courtyard overlaps a keepout that forbids parts.
    InKeepout {
        refdes: String,
        path: String,
        keepout: String,
    },
    /// A pad is inside a keepout that forbids tracks, so no track can reach it.
    PadInKeepout {
        refdes: String,
        path: String,
        keepout: String,
    },
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
        /// The paths of the two parts in the design.
        path: String,
        other_path: String,
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
    /// The designators of the parts the problem is about (none for a cutout or keepout of
    /// its own).
    pub fn parts(&self) -> Vec<&str> {
        match self {
            Self::Overlap { a, b, .. } => vec![a, b],
            Self::OverPins { holder, cover, .. } => vec![holder, cover],
            Self::HoleTooNear { refdes, other, .. } => vec![refdes, other],
            Self::OutsideOutline { refdes, .. }
            | Self::PadNearEdge { refdes, .. }
            | Self::InCutout { refdes, .. }
            | Self::PadNearCutout { refdes, .. }
            | Self::InKeepout { refdes, .. }
            | Self::PadInKeepout { refdes, .. }
            | Self::TooTall { refdes, .. } => vec![refdes],
            Self::CutoutOutsideOutline { .. } | Self::NoPosition { .. } => Vec::new(),
        }
    }

    /// By how much, where the problem has one measure to compare tries by: the area two
    /// courtyards share, how far a courtyard is past an edge, how much too tall a part is.
    /// Zero otherwise.
    pub fn amount(&self) -> f64 {
        match self {
            Self::Overlap { area, .. } => *area,
            Self::OutsideOutline { past, .. } => past.iter().map(|(_, by)| *by).sum(),
            Self::TooTall { height, limit, .. } => height - limit,
            Self::HoleTooNear {
                distance, least, ..
            } => (least - distance).max(0.0),
            _ => 0.0,
        }
    }

    /// The finding for two parts whose rooms overlap: the plain overlap, with a hint when
    /// the pins of a through-hole part come out under the other and are not stated as cut
    /// short, or, when they are, why the trim does not excuse it (ADR 0045 section 15).
    pub(crate) fn overlap_of(
        a: &Placed,
        b: &Placed,
        clearance: f64,
        zones: &[crate::placement::HeightZone],
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
                a_path: a.path.clone(),
                b_path: b.path.clone(),
                area: a.shared_area(b, clearance, zones),
                pins: crate::protrusion::unstated_under(a, b).map(|p| p.path.clone()),
            },
        }
    }
}

/// A member of the design as a message names it: its path, with the reference designator
/// beside it (`` `stage.c37` (C37) ``), or the designator alone for a part with no path.
pub fn member(path: &str, refdes: &str) -> String {
    if path.is_empty() || path == refdes {
        refdes.to_string()
    } else {
        format!("`{path}` ({refdes})")
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
    /// The package standard's height, where the footprint has one and it is over the limit
    /// (ADR 0106): the part in the package may be lower, so whether it fits is still unknown.
    pub standard: Option<crate::standard_height::StandardHeight>,
}

impl std::fmt::Display for HeightUnchecked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.standard {
            None => write!(
                f,
                "height of {} is unchecked: the keepout `{}` limits parts on the {} face to {:.2} mm and the part states no height (say `.{}.height &= 8mm`, with the real number)",
                member(&self.path, &self.refdes),
                self.keepout,
                self.side.word(),
                self.limit,
                self.path
            ),
            Some(s) => write!(
                f,
                "height of {} is unchecked: the keepout `{}` limits parts on the {} face to {:.2} mm, the part states no height, and its package standard ({}) allows up to {:.2} mm, so a part in that package may or may not fit (say `.{}.height &= 1mm`, with the real number from its datasheet)",
                member(&self.path, &self.refdes),
                self.keepout,
                self.side.word(),
                self.limit,
                s.standard,
                s.max,
                self.path
            ),
        }
    }
}

/// A part under a height limit that fits it only by its package standard's height (ADR
/// 0106): a pass, but on the standard's maximum and not on a figure of the part.
#[derive(Debug, Clone, PartialEq)]
pub struct StandardPass {
    pub refdes: String,
    pub path: String,
    pub keepout: String,
    /// The limit, in mm.
    pub limit: f64,
    pub side: Side,
    pub standard: crate::standard_height::StandardHeight,
}

impl std::fmt::Display for StandardPass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} fits the {:.2} mm the keepout `{}` allows on the {} face by its package standard ({}, at most {:.2} mm), not by a height of its own: the design states none and its catalog row has none",
            member(&self.path, &self.refdes),
            self.limit,
            self.keepout,
            self.side.word(),
            self.standard.cited(),
            self.standard.max
        )
    }
}

/// A part under a height limit, for [`Board::heights_unchecked`] and
/// [`Board::standard_passes`].
struct UnderLimit {
    refdes: String,
    path: String,
    keepout: String,
    side: Side,
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
                    "no height limit covers {} on the {} face where it lies over the pins of {} that protrude {} mm, so the stack of that protrusion and its height was compared with none",
                    member(p, r),
                    self.face.word(),
                    member(&self.path, &self.refdes),
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
    /// The part's path in the design.
    pub path: String,
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
            member(&self.path, &self.refdes),
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
    let mut out = format!(
        "{} is not wholly inside the board outline",
        member(path, refdes)
    );
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
            Self::Overlap {
                a,
                b,
                a_path,
                b_path,
                area,
                pins,
            } => {
                write!(
                    f,
                    "the courtyards of {} and {} overlap",
                    member(a_path, a),
                    member(b_path, b)
                )?;
                if *area > 0.005 {
                    write!(f, " by {area:.2} mm2")?;
                }
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
                "{} lies over the pins of {}, whose pins protrude {} mm{}, and that is not allowed: {}",
                member(cover_path, cover),
                member(pin_path, holder),
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
                cut,
            } => {
                let text = outside_text(refdes, path, past, stated, by, &[], false);
                match (past.is_empty(), cut) {
                    (true, Some(cut)) => write!(
                        f,
                        "{} is not wholly inside the board outline: its courtyard is {cut:.2} mm past the outline's edge",
                        member(path, refdes)
                    ),
                    _ => write!(f, "{text}"),
                }
            }
            Self::InKeepout {
                refdes,
                path,
                keepout,
            } => write!(
                f,
                "{} overlaps the keepout `{keepout}`, which forbids parts",
                member(path, refdes)
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
                "{} stands {height:.2} mm tall on the {} face, and the keepout `{keepout}` allows {limit:.2} mm there ({:.2} mm too tall), where its body is, around ({:.1}, {:.1}) mm from the board's top left corner",
                member(path, refdes),
                side.word(),
                height - limit,
                at.0,
                at.1
            ),
            Self::PadInKeepout {
                refdes,
                path,
                keepout,
            } => write!(
                f,
                "a pad of {} is inside the keepout `{keepout}`, which forbids tracks, so none can reach it",
                member(path, refdes)
            ),
            Self::InCutout {
                refdes,
                path,
                cutout,
            } => {
                write!(f, "{} overlaps the cutout `{cutout}`", member(path, refdes))
            }
            Self::PadNearCutout {
                refdes,
                path,
                cutout,
                distance,
            } => write!(
                f,
                "a pad of {} is {distance:.2} mm from the edge of the cutout `{cutout}`",
                member(path, refdes)
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
            Self::PadNearEdge {
                refdes,
                path,
                distance,
            } if *distance < 0.0 => {
                write!(
                    f,
                    "a pad of {} is {:.2} mm outside the board edge",
                    member(path, refdes),
                    -distance
                )
            }
            Self::PadNearEdge {
                refdes,
                path,
                distance,
            } => {
                write!(
                    f,
                    "a pad of {} is {distance:.2} mm from the board edge",
                    member(path, refdes)
                )
            }
            Self::HoleTooNear {
                refdes,
                path,
                other_path,
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
                    "a hole of {}{} is {distance:.2} mm from {what} of {}{}, under the \
                     process's {rule} of {least} mm",
                    member(path, refdes),
                    pad_text(pad),
                    member(other_path, other),
                    pad_text(other_pad)
                )
            }
        }
    }
}

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
fn inner_number(layer: &str) -> u32 {
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

impl Board {
    /// The part of [`Board::unplaced`] before the requirements are turned into
    /// places: the process and its rules, the options with the keepouts, cutouts and height
    /// limits of the design added, and the parts as footprints on the faces they were given.
    pub fn load(
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
        let links = netsmith_kicad::Hierarchy::of(design).links(design);
        for (part, symbol) in design.parts.iter().zip(links) {
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
                    symbol,
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
                symbol,
            });
        }
        Ok(Loaded {
            process,
            rules,
            placement,
            parts,
        })
    }

    /// The board of `design` before its parts are placed, with what is known about where they
    /// go (the face each is on, the size the board must be): the footprints on the faces they
    /// were given, the requirements of the physical file turned into what the placer is told
    /// (ADR 0045), the nets and the planes. What is wrong with the design is refused here,
    /// before any placing. The placer (`netsmith-placer`) places the parts and makes the
    /// board with [`Unplaced::into_board`].
    pub fn unplaced(
        design: &Design,
        mapping: &Mapping,
        library: &FootprintLibrary,
        placement: &PlaceOptions,
    ) -> Result<Unplaced, BoardError> {
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
        let pinned: std::collections::BTreeSet<String> = plan.fixed.keys().cloned().collect();
        placement.fixed.extend(plan.fixed);
        placement.wants = plan.wants;
        placement.offsets = plan.offsets;
        for part in &mut parts {
            if let Some(&side) = plan.sides.get(&part.refdes) {
                part.side = side;
                placement.sides.insert(part.refdes.clone(), side);
            }
        }

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
                inner.planes().map(|(n, net)| Plane {
                    layer: format!("In{n}.Cu"),
                    net: net.to_string(),
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

        // A polygon outline (issue #57) is checked here, before any placing.
        let polygon = match &design.features.outline {
            Some(o) => crate::polygon::of_outline(o)
                .map_err(|e| BoardError::Outline(format!("{}: {e}", design.name)))?
                .map(|p| p.translated(ORIGIN)),
            None => None,
        };

        Ok(Unplaced {
            process,
            rules,
            base,
            placement,
            parts,
            nets,
            planes,
            overhang,
            pinned,
            polygon,
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

    /// The narrowest a track of `net` may be away from its pads: the least width its current
    /// needs, and never less than the process's least track (issue #431).
    pub fn least_width(&self, net: &str) -> f64 {
        self.net_currents
            .iter()
            .find(|c| c.net == net)
            .map_or(0.0, |c| c.least_width)
            .max(self.rules.min_track_width)
    }

    /// The box round the centres of a part's pads, which a track of a net that carries current
    /// may be necked down near (issue #431).
    pub fn neck_box(&self, refdes: &str) -> Option<Rect> {
        let part = self.part(refdes)?;
        let mut pts = part.footprint.pads.iter().map(|p| part.pad_centre(p));
        let first = pts.next()?;
        Some(pts.fold(Rect::new(first, first), |r, p| {
            Rect::new(
                Pt::new(r.min.x.min(p.x), r.min.y.min(p.y)),
                Pt::new(r.max.x.max(p.x), r.max.y.max(p.y)),
            )
        }))
    }

    /// The nets that need more than one via side by side where they change layer, and how
    /// many (issue #431).
    pub fn via_counts(&self) -> Vec<(String, usize)> {
        self.net_currents
            .iter()
            .filter(|c| c.vias > 1)
            .map(|c| (c.net.clone(), c.vias))
            .collect()
    }

    /// The nets whose current needs more than the process's least track, with the least width
    /// it needs: what the design rules check holds their routed copper to (issue #431).
    pub fn least_widths(&self) -> Vec<(String, f64)> {
        self.net_currents
            .iter()
            .filter(|c| c.least_width > self.rules.min_track_width + 1e-9)
            .map(|c| (c.net.clone(), c.least_width))
            .collect()
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
        let signals: Vec<String> = self
            .inner_layers()
            .into_iter()
            .filter(|l| !self.planes.iter().any(|pl| pl.layer == *l))
            .collect();
        let planes = if self.planes.is_empty() && self.pours.is_empty() && signals.is_empty() {
            String::new()
        } else {
            let mut inner: Vec<(String, String)> = self
                .planes
                .iter()
                .map(|pl| {
                    (
                        pl.layer.clone(),
                        format!("{} is a plane on {}", pl.layer, pl.net),
                    )
                })
                .chain(signals.iter().map(|l| {
                    (
                        l.clone(),
                        format!("{l} is a signal layer, with no tracks yet"),
                    )
                }))
                .collect();
            inner.sort_by_key(|(l, _)| inner_number(l));
            let list: Vec<String> = inner
                .into_iter()
                .map(|(_, text)| text)
                .chain(
                    self.pours
                        .iter()
                        .map(|po| format!("{} is a pour on {}", po.layer.name(), po.net)),
                )
                .collect();
            format!(" ({})", list.join(", "))
        };
        format!(
            "{} {} (numbers from {}); track {} mm, space {} mm, drill {} mm, via {} mm, annular ring {} mm, hole to hole {} mm, copper to edge {} mm, silkscreen {} mm line and {} mm text, mask bridge {} mm; {} layers{}, {} mm thick, {} oz copper{}; {}",
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
            p.stackup
                .inner
                .as_ref()
                .map_or_else(String::new, |i| format!(", build-up {}", i.build)),
            if p.is_fictional() {
                "a fictional process: this package is not for ordering"
            } else {
                "order it with these"
            },
        )
    }

    /// KiCad's names of the inner copper layers, from the top: `In1.Cu`, `In2.Cu`... Each is a
    /// plane (in [`Board::planes`]) or a signal layer (ADR 0111).
    pub fn inner_layers(&self) -> Vec<String> {
        (1..=self.process.stackup.layers.saturating_sub(2))
            .map(|n| format!("In{n}.Cu"))
            .collect()
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
    /// the fabrication notes; empty when the standard process makes it. A board of a fictional
    /// fabricator (ADR 0112) says first that it cannot be ordered.
    pub fn process_extras(&self) -> Vec<String> {
        let mut extras = Vec::new();
        if self.process.is_fictional() {
            extras.push(format!(
                "fictional fabricator: {} does not exist; its numbers were chosen, not sourced, and stand for no real fabricator's capability, so this package cannot be ordered (`fab --order` refuses it, ADR 0112)",
                self.process.fabricator
            ));
        }
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

    /// The outline's box with its rounded corners: the board edge, unless the board has a
    /// polygon outline, whose bounding box it then is. What measures the edge uses
    /// [`Board::edge`].
    pub fn outline_shape(&self) -> RoundRect {
        RoundRect::new(self.outline, self.corner_radius)
    }

    /// The board edge, whichever shape it is.
    pub fn edge(&self) -> crate::polygon::BoardEdge {
        match &self.polygon {
            Some(p) => crate::polygon::BoardEdge::Polygon(p.clone()),
            None => crate::polygon::BoardEdge::Round(self.outline_shape()),
        }
    }

    /// The part of the outline's box that is outside a polygon outline, as boxes the placer
    /// keeps courtyards out of. Empty for a rounded rectangle.
    pub fn outside_boxes(&self) -> Vec<Rect> {
        self.polygon
            .as_ref()
            .map(crate::polygon::Polygon::outside_boxes)
            .unwrap_or_default()
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
    pub fn height_zones(&self) -> Vec<crate::placement::HeightZone> {
        self.keepout_areas()
            .into_iter()
            .filter(|k| k.carrier.is_none())
            .filter_map(|k| {
                Some(crate::placement::HeightZone {
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
    pub fn carried_zones(&self) -> Vec<crate::placement::CarriedZone> {
        self.keepouts
            .iter()
            .filter_map(|k| {
                let at = k.at?;
                let on = |layer: &str| k.layers.iter().any(|l| l == layer);
                Some(crate::placement::CarriedZone {
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
        if let Some(p) = &self.polygon {
            return p.box_depth(r);
        }
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
            return self.edge().depth(a.pad_centre(pad)) - pad.width / 2.0;
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
                        path: a.path.clone(),
                        other_path: b.path.clone(),
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
                        path: part.path.clone(),
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
    ///
    /// A part whose footprint has a _Standard height_ (ADR 0106) is unchecked only where the
    /// standard's maximum is over the limit; where it is not, the part is a [`StandardPass`]
    /// ([`Board::standard_passes`]).
    pub fn heights_unchecked(&self) -> Vec<HeightUnchecked> {
        self.under_limits_without_height()
            .into_iter()
            .filter(|(_, limit, standard)| {
                standard.is_none_or(|s| s.max > limit + HEIGHT_TOLERANCE)
            })
            .map(|(u, limit, standard)| HeightUnchecked {
                refdes: u.refdes.clone(),
                path: u.path.clone(),
                keepout: u.keepout.clone(),
                limit,
                side: u.side,
                standard: standard.copied(),
            })
            .collect()
    }

    /// The parts under a height limit that state no height and fit it by their package
    /// standard's maximum (ADR 0106). Each is a pass to be reported as one, not a silent pass.
    pub fn standard_passes(&self) -> Vec<StandardPass> {
        self.under_limits_without_height()
            .into_iter()
            .filter_map(|(u, limit, standard)| {
                let s = standard.filter(|s| s.max <= limit + HEIGHT_TOLERANCE)?;
                Some(StandardPass {
                    refdes: u.refdes.clone(),
                    path: u.path.clone(),
                    keepout: u.keepout.clone(),
                    limit,
                    side: u.side,
                    standard: *s,
                })
            })
            .collect()
    }

    /// Each pair of a part with no height of its own and a height limit over its body on its
    /// face, with the limit and the part's standard height, if any. A label is left out.
    fn under_limits_without_height(
        &self,
    ) -> Vec<(
        UnderLimit,
        f64,
        Option<&'static crate::standard_height::StandardHeight>,
    )> {
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
                    found.push((
                        UnderLimit {
                            refdes: a.refdes.clone(),
                            path: a.path.clone(),
                            keepout: k.name.clone(),
                            side: a.side,
                        },
                        limit,
                        a.standard_height(),
                    ));
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
        let edge = self.edge();
        for (cutout, hole) in &cutouts {
            if edge.box_depth(&hole.rect) < -1e-6 {
                problems.push(PlacementProblem::CutoutOutsideOutline {
                    cutout: cutout.name.clone(),
                });
            }
        }
        for (i, a) in self.parts.iter().enumerate() {
            let declared = may[i];
            let past: Vec<(Edge, f64)> = overhang::past(&courtyards[i], &self.outline)
                .into_iter()
                .filter(|&(edge, _)| !declared.has(edge))
                .collect();
            // The corners of the room, but not those on an edge the part may hang over, which
            // are the board's edge and not its rounded corners.
            let o = &self.outline;
            let (in_corner, cut) = match &self.polygon {
                // A polygon's edge is measured along all of it, not only at the room's
                // corners, which may all be inside an L while its inner corner is in the room.
                // Past a side of the box the design allows, the room is clipped to the box.
                Some(polygon) => {
                    let depth = polygon.box_depth(&rooms[i]);
                    (depth < -1e-6, (depth < -1e-6).then_some(-depth))
                }
                None => {
                    let shape = self.outline_shape();
                    let in_corner = rooms[i].corners().iter().any(|&c| {
                        let on_edge = (declared.left && c.x <= o.min.x + 1e-6)
                            || (declared.right && c.x >= o.max.x - 1e-6)
                            || (declared.top && c.y <= o.min.y + 1e-6)
                            || (declared.bottom && c.y >= o.max.y - 1e-6);
                        !on_edge && !shape.contains(c)
                    });
                    (in_corner, None)
                }
            };
            if !past.is_empty() || in_corner {
                let stated = self.overhang.get(&a.refdes);
                problems.push(PlacementProblem::OutsideOutline {
                    refdes: a.refdes.clone(),
                    path: a.path.clone(),
                    past,
                    stated: declared.list(),
                    by: stated.map(Overhang::by).unwrap_or_default(),
                    cut,
                });
            }
            for k in keepouts
                .iter()
                .filter(|k| k.carrier.as_deref() != Some(a.path.as_str()))
            {
                if k.parts && rooms[i].overlaps(&k.rect) {
                    problems.push(PlacementProblem::InKeepout {
                        refdes: a.refdes.clone(),
                        path: a.path.clone(),
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
                        path: a.path.clone(),
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
                        path: a.path.clone(),
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
                        path: a.path.clone(),
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
                    path: a.path.clone(),
                    distance: nearest,
                });
            }
            for (j, b) in self.parts.iter().enumerate().skip(i + 1) {
                // Parts on opposite faces may share the board's area, unless one has a hole
                // through the board that takes room there (`Placed::room_on`).
                if a.rooms_overlap(rooms[i], b, rooms[j], self.rules.clearance, &zones) {
                    problems.push(PlacementProblem::overlap_of(
                        a,
                        b,
                        self.rules.clearance,
                        &zones,
                    ));
                }
                self.check_holes(a, b, true, &mut problems);
                self.check_holes(b, a, false, &mut problems);
            }
        }
        problems
    }
}

/// A board whose parts are not placed yet: what [`Board::unplaced`] makes of a design, and what
/// the placer places.
#[derive(Debug, Clone)]
pub struct Unplaced {
    pub process: Process,
    pub rules: Rules,
    /// What the layout file and the caller said, before the requirements were turned into
    /// places.
    pub base: PlaceOptions,
    /// `base` with the requirements' fixed parts, wants, offsets, faces and overhangs: what the
    /// placer keeps to.
    pub placement: PlaceOptions,
    /// The footprints, on the faces they were given, where the placer is to place them.
    pub parts: Vec<Placed>,
    pub nets: Vec<BoardNet>,
    pub planes: Vec<Plane>,
    /// The parts that may hang over an edge, as [`Board::overhang`].
    pub overhang: BTreeMap<String, Overhang>,
    /// The parts the requirements pin, as [`Board::pinned`].
    pub pinned: std::collections::BTreeSet<String>,
    /// The design's polygon outline on the sheet, as [`Board::polygon`]: the placer keeps
    /// the parts inside it.
    pub polygon: Option<crate::polygon::Polygon>,
}

impl Unplaced {
    /// The board, with the parts where the placer left them in an outline of `outline`.
    pub fn into_board(self, design: &Design, outline: Rect, corner_radius: f64) -> Board {
        let Unplaced {
            process,
            rules,
            parts,
            nets,
            planes,
            overhang,
            pinned,
            polygon,
            ..
        } = self;
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
        Board {
            name: design.name.clone(),
            outline,
            corner_radius,
            polygon,
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
            teardrops: Vec::new(),
            net_widths: design.features.net_widths.clone(),
            net_currents: design.features.net_currents.clone(),
            overhang,
            pinned,
        }
    }
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

#[cfg(test)]
pub(crate) mod tests {
    use netsmith_core::{Net, Part, PinRef};

    use super::*;

    pub fn fixtures() -> FootprintLibrary {
        FootprintLibrary::at(netsmith_testkit::sample_footprints())
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

    /// The board `json` holds, a placed `Board` from `tests/fixtures`, which the placer
    /// wrote (`netsmith-placer`'s `pcb_fixtures` test keeps them in step with it, and says how
    /// to write them again). The tests of this crate start from a placed board without
    /// depending on the placer (issue #608).
    fn fixture(json: &str) -> Board {
        serde_json::from_str(json).expect("a fixture is the JSON of a Board")
    }

    /// `led_design` placed: the board most tests of this crate check.
    pub fn led_board() -> Board {
        fixture(include_str!("../tests/fixtures/led.board.json"))
    }

    /// `four_layer_design` placed.
    pub fn four_layer_board() -> Board {
        fixture(include_str!("../tests/fixtures/four_layer.board.json"))
    }

    /// The LED board placed on six layers.
    pub fn six_layer_board() -> Board {
        fixture(include_str!("../tests/fixtures/six_layer.board.json"))
    }

    /// The LED board placed on ACME's ten layers.
    pub fn ten_layer_board() -> Board {
        fixture(include_str!("../tests/fixtures/ten_layer.board.json"))
    }

    /// The board of `design` with its parts where [`Board::unplaced`] leaves them, in an outline
    /// of 60 by 40 mm: for the tests that put the parts where they want them.
    pub fn unplaced_board(design: &Design) -> Board {
        Board::unplaced(
            design,
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
            &PlaceOptions::default(),
        )
        .unwrap_or_else(|e| panic!("{e}"))
        .into_board(design, Rect::new(ORIGIN, ORIGIN + Pt::new(60.0, 40.0)), 0.0)
    }

    /// The same circuit as `examples/led.nsm`.
    pub fn led_design() -> Design {
        Design {
            modules: Vec::new(),
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
    fn a_board_of_a_fictional_fabricator_says_it_is_not_for_ordering() {
        let board = ten_layer_board();
        assert_eq!(board.inner_layers().len(), 8);
        let note = board.process_note();
        assert!(
            note.starts_with("ACME TenLayer (numbers from std/mfg/acme.nsm: chosen, not sourced; ACME is a fictional fabricator)")
                && note.contains("10 layers (In1.Cu is a plane on GND, In2.Cu is a signal layer")
                && note.contains("In8.Cu is a plane on 3V3), 1.6 mm thick, 1 oz copper, build-up ACME-10L-1.6")
                && note.ends_with("; a fictional process: this package is not for ordering")
                && !note.contains("order it with these"),
            "{note}"
        );
        let extras = board.process_extras();
        assert!(
            extras.iter().any(|e| e.starts_with(
                "fictional fabricator: ACME does not exist; its numbers were chosen, not sourced, and stand for no real fabricator's capability, so this package cannot be ordered"
            )),
            "{extras:?}"
        );
        // A real fabricator's board says neither.
        let six = six_layer_board();
        assert!(six.process_note().ends_with("order it with these"));
        assert!(!six.process_extras().iter().any(|e| e.contains("fictional")));
    }

    #[test]
    fn a_six_layer_board_has_planes_on_the_named_layers_and_says_what_each_inner_layer_is() {
        let board = six_layer_board();
        let planes: Vec<(&str, &str)> = board
            .planes
            .iter()
            .map(|p| (p.layer.as_str(), p.net.as_str()))
            .collect();
        assert_eq!(planes, [("In1.Cu", "GND"), ("In4.Cu", "3V3")]);
        assert_eq!(
            board.inner_layers(),
            ["In1.Cu", "In2.Cu", "In3.Cu", "In4.Cu"]
        );
        let note = board.process_note();
        assert!(
            note.contains(
                "6 layers (In1.Cu is a plane on GND, In2.Cu is a signal layer, with no tracks yet, In3.Cu is a signal layer, with no tracks yet, In4.Cu is a plane on 3V3), 1.6 mm thick, 1 oz copper, build-up JLC06161H-3313"
            ),
            "{note}"
        );
    }

    #[test]
    fn the_inner_layers_of_a_four_layer_board_are_planes_on_the_nets_the_board_named() {
        let board = four_layer_board();
        let planes: Vec<(&str, &str)> = board
            .planes
            .iter()
            .map(|p| (p.layer.as_str(), p.net.as_str()))
            .collect();
        assert_eq!(planes, [("In1.Cu", "GND"), ("In2.Cu", "3V3")]);
        assert!(board.rules.plane_clearance >= board.rules.clearance);
        let two = led_board();
        assert_eq!(two.planes, vec![]);
    }

    #[test]
    fn a_plane_on_a_net_the_design_does_not_have_names_the_layer_and_the_net() {
        // The design's supply net is called VCC, not 3V3.
        let mut design = four_layer_design();
        design.nets[1].name = "VCC".into();
        let text = Board::unplaced(
            &design,
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
            &PlaceOptions::default(),
        )
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
        let board = four_layer_board();
        let note = board.process_note();
        assert!(
            note.contains(
                "4 layers (In1.Cu is a plane on GND, In2.Cu is a plane on 3V3), 1.6 mm thick"
            ),
            "{note}"
        );
        assert!(note.contains("track 0.09 mm"), "{note}");
        let two = led_board();
        assert!(two.process_note().contains("; 2 layers, 1.6 mm thick"));
    }

    #[test]
    fn a_design_with_no_process_is_refused_with_what_to_add() {
        let mut design = led_design();
        design.process = None;
        let text = Board::unplaced(
            &design,
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
            &PlaceOptions::default(),
        )
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
        let text = Board::unplaced(
            &design,
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
            &PlaceOptions::default(),
        )
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
        let board = led_board();
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
    fn the_placed_led_board_is_a_valid_placement() {
        assert_eq!(led_board().check_placement(), vec![]);
    }

    #[test]
    fn a_part_on_the_back_is_the_footprint_in_a_mirror_with_its_pads_on_the_back_layer() {
        let mut board = led_board();
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
        let mut board = led_board();
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
        assert!(board.check_placement().iter().any(|p| matches!(
            p,
            PlacementProblem::Overlap { a, b, pins: None, .. } if a == "D1" && b == "R1"
        )));
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
    fn an_overlap_names_both_members_by_path_with_the_designator_and_by_how_much() {
        let mut board = led_board();
        for p in &mut board.parts {
            p.path = match p.refdes.as_str() {
                "D1" => "status.led".into(),
                "R1" => "status.limit".into(),
                _ => String::new(),
            };
        }
        let r1 = board.part("R1").unwrap().at;
        let d1 = board.parts.iter_mut().find(|p| p.refdes == "D1").unwrap();
        d1.at = r1;
        let (a, b) = (board.part("D1").unwrap(), board.part("R1").unwrap());
        let (ca, cb) = (a.courtyard(), b.courtyard());
        let shared = (ca.max.x.min(cb.max.x) - ca.min.x.max(cb.min.x))
            * (ca.max.y.min(cb.max.y) - ca.min.y.max(cb.min.y));
        let text: Vec<String> = board
            .check_placement()
            .iter()
            .filter(|p| matches!(p, PlacementProblem::Overlap { .. }))
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            text,
            vec![format!(
                "the courtyards of `status.led` (D1) and `status.limit` (R1) overlap by {shared:.2} mm2"
            )]
        );
        // A part with no path is named by its designator alone.
        assert_eq!(member("", "J1"), "J1");
        assert_eq!(member("conn", "J1"), "`conn` (J1)");
    }

    #[test]
    fn every_placement_check_message_names_the_member_by_path_with_the_designator() {
        let text = |p: PlacementProblem| p.to_string();
        let (r, path) = (|| "R3".to_string(), || "stage.r3".to_string());
        assert_eq!(
            text(PlacementProblem::InKeepout {
                refdes: r(),
                path: path(),
                keepout: "lid".into()
            }),
            "`stage.r3` (R3) overlaps the keepout `lid`, which forbids parts"
        );
        assert_eq!(
            text(PlacementProblem::PadInKeepout {
                refdes: r(),
                path: path(),
                keepout: "lid".into()
            }),
            "a pad of `stage.r3` (R3) is inside the keepout `lid`, which forbids tracks, so none can reach it"
        );
        assert_eq!(
            text(PlacementProblem::InCutout {
                refdes: r(),
                path: path(),
                cutout: "slot".into()
            }),
            "`stage.r3` (R3) overlaps the cutout `slot`"
        );
        assert_eq!(
            text(PlacementProblem::PadNearCutout {
                refdes: r(),
                path: path(),
                cutout: "slot".into(),
                distance: 0.12
            }),
            "a pad of `stage.r3` (R3) is 0.12 mm from the edge of the cutout `slot`"
        );
        assert_eq!(
            text(PlacementProblem::PadNearEdge {
                refdes: r(),
                path: path(),
                distance: 0.12
            }),
            "a pad of `stage.r3` (R3) is 0.12 mm from the board edge"
        );
        assert_eq!(
            text(PlacementProblem::PadNearEdge {
                refdes: r(),
                path: path(),
                distance: -0.5
            }),
            "a pad of `stage.r3` (R3) is 0.50 mm outside the board edge"
        );
        assert_eq!(
            text(PlacementProblem::HoleTooNear {
                refdes: "J1".into(),
                path: "conn".into(),
                other_path: path(),
                pad: "1".into(),
                other: r(),
                other_pad: "2".into(),
                to: HoleNeighbour::Copper,
                distance: 0.1,
                least: 0.2
            }),
            "a hole of `conn` (J1) (pad 1) is 0.10 mm from the copper of `stage.r3` (R3) (pad 2), under the process's clearance of 0.2 mm"
        );
        assert_eq!(
            text(PlacementProblem::TooTall {
                refdes: r(),
                path: path(),
                keepout: "lid".into(),
                height: 3.0,
                limit: 2.0,
                side: Side::Front,
                at: (1.0, 2.0)
            }),
            "`stage.r3` (R3) stands 3.00 mm tall on the top face, and the keepout `lid` allows 2.00 mm there (1.00 mm too tall), where its body is, around (1.0, 2.0) mm from the board's top left corner"
        );
        assert!(
            text(PlacementProblem::OutsideOutline {
                refdes: r(),
                path: path(),
                past: vec![(Edge::Left, 0.5)],
                stated: Vec::new(),
                by: String::new(),
                cut: None,
            })
            .starts_with("`stage.r3` (R3) is not wholly inside the board outline: its courtyard is 0.50 mm past the left edge")
        );
        assert_eq!(
            HeightUnchecked {
                refdes: r(),
                path: path(),
                keepout: "lid".into(),
                limit: 2.0,
                side: Side::Front,
                standard: None,
            }
            .to_string(),
            "height of `stage.r3` (R3) is unchecked: the keepout `lid` limits parts on the top face to 2.00 mm and the part states no height (say `.stage.r3.height &= 8mm`, with the real number)"
        );
        assert!(
            Hang {
                refdes: r(),
                path: path(),
                edge: Edge::Left,
                by: 0.5,
                stated: "the requirement".into()
            }
            .to_string()
            .starts_with(
                "`stage.r3` (R3) hangs over the left edge: its courtyard is 0.50 mm past it"
            )
        );
    }

    #[test]
    fn the_check_finds_overlaps_and_parts_off_the_board() {
        let mut board = led_board();
        let r1 = board.part("R1").unwrap().at;
        let d1 = board.parts.iter_mut().find(|p| p.refdes == "D1").unwrap();
        d1.at = r1 + Pt::new(0.5, 0.0);
        let j1 = board.parts.iter_mut().find(|p| p.refdes == "J1").unwrap();
        j1.at = Pt::new(1000.0, 0.0);
        let problems = board.check_placement();
        assert!(problems.iter().any(|p| matches!(
            p,
            PlacementProblem::Overlap { a, b, pins: None, .. } if a == "D1" && b == "R1"
        )));
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
        let mut board = led_board();
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
        let mut board = led_board();
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
                PlacementProblem::PadNearEdge {
                    refdes, distance, ..
                } => {
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
        let mut board = led_board();
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
        let board = led_board();
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
    fn the_placer_and_the_fab_check_measure_one_body_for_a_footprint_with_no_courtyard() {
        // #457: a footprint that draws neither a courtyard nor a fabrication layer, on the
        // back and turned, with pads off its origin. The placer's body (`Placed::body`) and
        // the one the fab check measures on the written board are one box, at a right angle
        // and at any other.
        use netsmith_geom::drc::constraints::{Constraint, Constraints, Measure};
        let mut board = led_board();
        let bare = crate::footprint::Footprint::parse(
            "T:Fingers",
            r#"(footprint "Fingers"
                (property "Reference" "REF**" (at 0 -3 0) (layer "F.SilkS"))
                (pad "1" connect rect (at 0 0.55) (size 0.7 3.2) (layers "F.Cu" "F.Mask"))
                (pad "2" connect rect (at -1 0) (size 0.7 4.3) (layers "F.Cu" "F.Mask")))"#,
        )
        .unwrap();
        assert!(bare.body_is_courtyard);
        for rotation in [90.0, 30.0] {
            let r1 = board.parts.iter_mut().find(|p| p.refdes == "R1").unwrap();
            r1.footprint = bare.clone();
            r1.side = Side::Back;
            r1.at = Pt::new(15.0, 12.0);
            r1.rotation = rotation;
            let placer = r1.body();
            let o = board.outline.min;
            let want = |measure| Constraint {
                id: "r1".into(),
                source: "t.nsm:1".into(),
                parts: vec!["R1".into()],
                measure,
                lo: f64::NEG_INFINITY,
                hi: f64::INFINITY,
            };
            let constraints = Constraints {
                list: vec![
                    want(Measure::Left),
                    want(Measure::Right),
                    want(Measure::Top),
                    want(Measure::Bottom),
                ],
                sides: Vec::new(),
                relations: Vec::new(),
                severity: netsmith_geom::drc::Severity::Error,
            };
            let art =
                netsmith_geom::artwork::Artwork::parse(&crate::export::kicad_pcb(&board)).unwrap();
            let checked: Vec<f64> = constraints
                .measure(&art)
                .iter()
                .map(|m| m.measured.unwrap_or_else(|| panic!("{}", m.message)))
                .collect();
            let expected = [
                placer.min.x - o.x,
                placer.max.x - o.x,
                placer.min.y - o.y,
                placer.max.y - o.y,
            ];
            for (c, e) in checked.iter().zip(expected) {
                assert!(
                    (c - e).abs() < 1e-6,
                    "turned {rotation}: the check measures {checked:?} and the placer {expected:?}"
                );
            }
        }
    }

    #[test]
    fn a_net_that_names_a_missing_pad_says_which() {
        let mut design = led_design();
        design.nets[0].pins[0].pin = "7".into();
        let err = Board::unplaced(
            &design,
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
            &PlaceOptions::default(),
        )
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
        let err = Board::unplaced(
            &design,
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
            &PlaceOptions::default(),
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "R1: there is no KiCad symbol for a `Gizmo` yet"
        );
    }

    #[test]
    fn a_missing_library_file_names_the_footprint_and_path() {
        let empty = FootprintLibrary::at("/nonexistent/footprints");
        let text = Board::unplaced(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &empty,
            &PlaceOptions::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(text.starts_with("D1: footprint `LED_SMD:LED_0603_1608Metric` not found"));
        assert!(
            text.contains("/nonexistent/footprints/LED_SMD.pretty/LED_0603_1608Metric.kicad_mod")
        );
    }
}
