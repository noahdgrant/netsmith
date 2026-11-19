//! The sides of a board, and a part placed on it: its footprint, face, place and turn.

use super::*;
use crate::footprint::Shape;

/// How far a chord of a drawn arc may lie from the curve when the reference text is kept
/// clear of the silkscreen, in mm.
const ARC_SAGITTA: f64 = 0.01;

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

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// What a part's silkscreen prints, where the library draws its reference text (the
/// `.silk` and `.silk_text` of the physical file). The designator stays in the BOM, the
/// placement file and on the fabrication layer whatever is printed here.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Silk {
    /// The part's reference designator.
    #[default]
    Designator,
    /// Nothing: no text, no room for one and no finding about one.
    Nothing,
    /// This text, in the place and at the size the designator would have.
    Text(String),
}

impl Silk {
    /// Whether this is the default, which a serialized part leaves out.
    pub fn is_designator(&self) -> bool {
        *self == Silk::Designator
    }
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
    /// What the tools treat the part as, as the design has it (ADR 0125): a part of the kind
    /// `capacitor` is a capacitor to the placer whatever its own name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_kind: Option<netsmith_core::PartKind>,
    pub value: String,
    /// `Resistor_SMD:R_0402_1005Metric`
    pub library_id: String,
    pub footprint: Footprint,
    /// The numbers of the pads that belong to pins the part's pin table calls `power`
    /// (ADR 0016). The placer reads them to find which capacitor decouples which part, and which
    /// nets are supply nets; a part whose pins are not called `power` is found from the nets
    /// (`decoupling_pairs`).
    pub power_pads: Vec<String>,
    /// Where the footprint's origin is on the board, in mm.
    pub at: Pt,
    /// Degrees, counterclockwise on screen.
    pub rotation: f64,
    pub side: Side,
    /// Which side of the part its reference text is on.
    pub reference_side: ReferenceSide,
    /// What its silkscreen prints in place of its designator, if the design says.
    #[serde(default, skip_serializing_if = "Silk::is_designator")]
    pub silk: Silk,
    /// How tall the body stands above its face of the board, in mm, as the design states it
    /// (`.height`, ADR 0045 "Height limits"). `None` is unknown, never zero.
    pub height: Option<f64>,
    /// How high the underside of the body is above its face of the board, in mm, as the design
    /// states it (`.standoff`, issue #775): a body that stands on pins. Zero, as for every
    /// ordinary part, when the design states none. The body stands from here up to `height`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub standoff: f64,
    /// How much the body may grow in height over its life, in mm, as the design states it
    /// (`.height_growth`, issue #1263): a cell or a supercapacitor that swells. Zero when the
    /// design states none. A height limit is checked against `height` plus this
    /// ([`Placed::grown_height`]).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub height_growth: f64,
    /// The room for routing kept round its courtyard, in mm, as the design states it
    /// (`.routing_margin`, the part's own or the board's, issue #1313). Zero when the design
    /// states none. The placer and `layout fit` grow the courtyard by it on every side against
    /// other parts' courtyards ([`Placed::margin_short`]); the edge, keepouts and cutouts are
    /// measured against the courtyard itself, and `fab`'s placement check does not read it.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub routing_margin: f64,
    /// The distance other parts are asked to keep from its courtyard, in mm, as the physical
    /// file states it (`soft margin 3mm round .u1`, issue 1310). Zero when it states none. The
    /// placer pays for a courtyard inside it, and a courtyard that is still inside it on the
    /// placed board is a warning ([`Placed::soft_short`]), an error when strict.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub soft_margin: f64,
    /// The line that states `soft_margin` (`widget.physical.nsm:14`), for a message.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub soft_margin_source: String,
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
    /// The guidelines of the part's `layout` block, with the pads of the pins they name
    /// (ADR 0030, issues #829 and #994). Empty for a part whose definition states none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layout: Vec<crate::part_layout::PartGuideline>,
    /// Where the contacts of its pads are that are not at the pads' centres, by pad number: the
    /// offset from the centre of the copper, in the footprint's frame, in mm (a part's `pads`
    /// block, issue #1321). A pad not listed has its contact at its centre ([`Placed::contact`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contacts: Vec<(String, Pt)>,
}

impl Placed {
    /// Where the contact of `pad` is, in the footprint's frame: the centre of its copper moved by
    /// the offset the part states for its number, if any (issue #1321).
    pub fn contact(&self, pad: &crate::Pad) -> Pt {
        match self.contacts.iter().find(|(n, _)| *n == pad.number) {
            Some((_, d)) => Pt {
                x: pad.at.x + d.x,
                y: pad.at.y + d.y,
            },
            None => pad.at,
        }
    }

    /// The height a height limit is checked against, in mm: the part's `height` plus its
    /// `height_growth` (issue #1263). `None` when the height is unknown: a growth alone says
    /// nothing of how tall the part is.
    pub fn grown_height(&self) -> Option<f64> {
        self.height.map(|h| h + self.height_growth)
    }

    /// The _Standard height_ of the part's footprint (ADR 0106,
    /// [`crate::standard_height`]), when the part has no height of its own: `None` when the
    /// design or the catalog row gives one, which always wins, and for a footprint the table has
    /// no row for.
    pub fn standard_height(&self) -> Option<&'static crate::standard_height::StandardHeight> {
        if self.height.is_some() || netsmith_core::is_artwork(&self.kind) {
            return None;
        }
        crate::standard_height::of(&self.library_id)
    }

    /// The name of the limit a standoff makes (`standoff of J1`), and the word the messages use
    /// to say a limit is a body's and not a keepout's.
    pub const STANDOFF_ZONE: &'static str = "standoff of ";

    /// What the standoff of the part's body limits (issue #775): a part on the same face may
    /// stand under the body only as tall as the body stands off the board. A zone the part
    /// carries, in its footprint's frame, over its body, on its own face, that forbids
    /// nothing. `None` for a part on the board (a standoff of zero limits nothing: the courtyard
    /// does) and for a label.
    pub fn standoff_zone(&self) -> Option<crate::placement::CarriedZone> {
        if self.standoff <= 0.0 || netsmith_core::is_artwork(&self.kind) {
            return None;
        }
        let back = self.side == Side::Back;
        Some(crate::placement::CarriedZone {
            name: format!("{}{}", Self::STANDOFF_ZONE, self.refdes),
            part: self.path.clone(),
            local: self.footprint.body,
            parts: false,
            front: !back,
            back,
            limit: Some(self.standoff),
            on_carrier_face: true,
        })
    }

    /// The height of the text the silkscreen prints, in mm: what the library draws for the
    /// reference, or the process's smallest if that is more.
    pub fn text_height(&self, rules: &Rules) -> f64 {
        self.footprint.reference_size.max(rules.min_text_height)
    }

    /// The width of the text the silkscreen prints, in mm: about one text height for each
    /// letter.
    pub fn text_width(&self, rules: &Rules) -> f64 {
        self.silk_text().map_or(0, |t| t.chars().count()) as f64 * self.text_height(rules) * 0.95
            + rules.min_text_thickness
    }

    /// The text the silkscreen prints for the part: its designator, or the text the design
    /// gives in its place; `None` when it prints nothing.
    pub fn silk_text(&self) -> Option<&str> {
        match &self.silk {
            Silk::Designator => Some(&self.refdes),
            Silk::Nothing => None,
            Silk::Text(t) => Some(t),
        }
    }

    /// Whether the footprint has a drilled hole, which takes room on both faces.
    pub fn has_hole(&self) -> bool {
        self.footprint.pads.iter().any(|p| p.drill.is_some())
    }

    /// How near this part's courtyard is to `other`'s, and the space their routing margins ask
    /// for between them (`.routing_margin`, issue #1313): `Some((gap, need))`, in mm, when the
    /// courtyards are nearer than the sum of the two margins. The gap is the larger of the
    /// spaces along x and along y between the nearest two shapes of the courtyards, which a
    /// courtyard grown by its margin on every side measures. `None` when they keep the space,
    /// when neither has a margin, when they are on opposite faces and neither has a hole, and
    /// when the courtyards overlap (the placement check says that).
    pub fn margin_short(&self, other: &Placed) -> Option<(f64, f64)> {
        let need = self.routing_margin + other.routing_margin;
        if need <= 0.0 || (self.side != other.side && !self.has_hole() && !other.has_hole()) {
            return None;
        }
        let mut gap = f64::INFINITY;
        for a in self.courtyards() {
            for b in other.courtyards() {
                let dx = (a.min.x - b.max.x).max(b.min.x - a.max.x);
                let dy = (a.min.y - b.max.y).max(b.min.y - a.max.y);
                gap = gap.min(dx.max(dy));
            }
        }
        (gap > -1e-6 && gap < need - 1e-6).then_some((gap.max(0.0), need))
    }

    /// How near `other`'s courtyard is to this part's, when this part has a soft margin
    /// (`soft margin 3mm round .u1`, issue 1310) that it is nearer than: `Some((gap, margin))`,
    /// in mm. The gap is measured as [`Placed::margin_short`] measures it, the larger of the
    /// spaces along x and along y between the nearest two shapes of the courtyards. `None` when
    /// the part has no soft margin, when `other` keeps it, when they are on opposite faces and
    /// neither has a hole, and when the courtyards overlap (the placement check says that).
    /// Only this part's margin counts: a margin is asked round the part that states it.
    pub fn soft_short(&self, other: &Placed) -> Option<(f64, f64)> {
        let need = self.soft_margin;
        if need <= 0.0 || (self.side != other.side && !self.has_hole() && !other.has_hole()) {
            return None;
        }
        let mut gap = f64::INFINITY;
        for a in self.courtyards() {
            for b in other.courtyards() {
                let dx = (a.min.x - b.max.x).max(b.min.x - a.max.x);
                let dy = (a.min.y - b.max.y).max(b.min.y - a.max.y);
                gap = gap.min(dx.max(dy));
            }
        }
        (gap > -1e-6 && gap < need - 1e-6).then_some((gap.max(0.0), need))
    }

    /// What face `face` of the board the part takes room on, and whether that room is its
    /// pads (ADR 0031): a surface-mount part takes its courtyard on its own face
    /// only; a part with a hole takes its courtyard on both, except that a footprint which
    /// draws its courtyard on only one face's layer is judged on the other face by its pads,
    /// each grown by `clearance`, so a part on that face may stand over the rest of its
    /// body. A footprint that draws both courtyard layers, or none, counts on both as ever.
    /// `courts` are the courtyard's shapes to use where it counts, each its own room (the
    /// board's clip of them, if the part may hang over an edge): [`Placed::courtyards`]. The
    /// boxes are empty where the part takes no room.
    pub fn room_on(&self, face: Side, clearance: f64, courts: &[Rect]) -> Vec<Rect> {
        if !self.has_hole() {
            return if face == self.side {
                courts.to_vec()
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
            return courts.to_vec();
        }
        self.footprint
            .pads
            .iter()
            .map(|p| self.pad_bounds(p).expanded(clearance))
            .collect()
    }

    /// Whether two parts take room in the same place on either face of the board: `court_a`
    /// and `court_b` are the courtyard shapes to count ([`Placed::room_on`]). A part that stands
    /// excused over the other's protruding pins ([`crate::protrusion`]) takes only its pads there,
    /// against the pads of the other; `zones` are the height limits that rule judges by.
    pub fn rooms_overlap(
        &self,
        court_a: &[Rect],
        other: &Placed,
        court_b: &[Rect],
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
        let (court_a, court_b) = (self.courtyards(), other.courtyards());
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
                        self.room_on(face, clearance, &court_a),
                        other.room_on(face, clearance, &court_b),
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
                // An arc bulges past its three points: its chords follow the curve.
                let points = match g.shape {
                    Shape::Arc { .. } => g.shape.outline(ARC_SAGITTA),
                    _ => g.shape.extent_points(),
                };
                points
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
    /// A round hole has both ends at its centre, which is off the pad's centre by the
    /// drill's offset. A slot lies along the pad's own x or y axis and turns with the pad
    /// and the part, so it may lie at any angle on the board. `None` for a pad with no hole.
    pub fn hole(&self, pad: &Pad) -> Option<(Pt, Pt, f64)> {
        let drill = pad.drill?;
        let centre = self.to_board(pad.hole_at()?);
        let (long, short) = (drill.width.max(drill.height), drill.width.min(drill.height));
        let reach = (long - short) / 2.0;
        // A slot lies along the pad's x axis when it is wider than it is high.
        let axis = if drill.width >= drill.height {
            Pt::new(reach, 0.0)
        } else {
            Pt::new(0.0, reach)
        };
        let end = axis.rotate(self.pad_angle(pad));
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

    /// The box round the courtyard on the board.
    pub fn courtyard(&self) -> Rect {
        self.rect_to_board(self.footprint.courtyard)
    }

    /// The courtyard's shapes on the board, each its own room: what the placement check and
    /// the placer keep other parts out of ([`Footprint::courtyard_shapes`]). A part turned off
    /// the right angles has each shape as the strips that hold it turned
    /// ([`Rect::turned_cover`]), not the one box round it, so that it packs closer (issue #996).
    pub fn courtyards(&self) -> Vec<Rect> {
        self.footprint
            .courtyard_shapes
            .iter()
            .flat_map(|r| r.turned_cover(self.mirrored(), self.rotation))
            .map(|r| r.translated(self.at))
            .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::footprint::Graphic;

    #[test]
    fn the_reference_text_keeps_clear_of_the_bulge_of_an_arc_not_only_of_its_three_points() {
        // An arc of a circle of radius 2 from -20 to 20 degrees, through -10: its three points
        // reach x = 1.88 at the most, and the curve 2.0, at 0 degrees.
        let mut board = crate::board::tests::led_board();
        let rules = board.rules.clone();
        let part = &mut board.parts[0];
        part.at = Pt::default();
        part.rotation = 0.0;
        part.side = Side::Front;
        part.reference_side = ReferenceSide::Right;
        part.footprint.courtyard = Rect::new(Pt::default(), Pt::default());
        part.footprint.graphics.clear();
        let on_circle =
            |deg: f64| Pt::new(2.0 * deg.to_radians().cos(), 2.0 * deg.to_radians().sin());
        part.footprint.graphics.push(Graphic {
            layer: Layer::Silk,
            shape: Shape::Arc {
                start: on_circle(-20.0),
                mid: on_circle(-10.0),
                end: on_circle(20.0),
            },
            stroke: 0.12,
            filled: false,
        });
        let at = part.reference_at(&rules);
        let away = rules.silk_to_pad + rules.min_text_thickness;
        let clear = 2.0 + 0.06 + away + part.text_width(&rules) / 2.0;
        assert!(at.x > clear - 0.011, "{} against {clear}", at.x);
        assert!(at.x < clear + 1e-6, "{} against {clear}", at.x);
    }
}
