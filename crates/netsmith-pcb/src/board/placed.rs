//! The sides of a board, and a part placed on it: its footprint, face, place and turn.

use super::*;

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
    /// What the tools treat the part as, as the design has it (ADR 0125): a part of the kind
    /// `capacitor` is a capacitor to the placer whatever its own name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_kind: Option<netsmith_core::PartKind>,
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
