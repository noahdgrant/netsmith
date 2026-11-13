//! Pin protrusion: a flat part over the cut pins of a through-hole part (ADR 0031 amendment,
//! ADR 0045 section 15; owner decisions of 2026-11-04).
//!
//! A through-hole part takes room on both faces (`Placed::room_on`). The board's physical
//! file may state how far its pins protrude out of the far side of the board
//! (`.pin_protrusion`, a length), whether they are cut after soldering or bought short. Then
//! a part on the face the pins come out of may stand over them, if
//!
//! 1. the part is fitted by hand, after the pins are cut (the board house fits its parts
//!    before any pin is soldered, so a part it assembles cannot be put over cut pins);
//! 2. its height is known;
//! 3. the stack fits: the pins protrude `t` out of the face, the part rests on them, so `t`
//!    plus the part's height must not exceed the limit that a height zone puts on that face
//!    where the part's body is. No zone there: nothing to exceed.
//!
//! What is excused is the overlap of the part's body and courtyard with the pads' room. The
//! copper clearance between their pads is not: the covering part's own pads keep the
//! process's clearance from the pads of the through-hole part.
//!
//! This is the only place that judges it. `Placed::rooms_overlap` (so `Board::check_placement`
//! and the placer's pre-check), the annealer's `Engine::room_pairs`, `could_fit`, the
//! conflict explanation and the design rules check's courtyard rule all ask it.
//!
//! **Owner decisions of 2026-11-04** (ADR 0045 section 15, issue #99). The member is
//! `.pin_protrusion`, per instance, stated in the board's physical file as `.height` is (a part
//! kind may bound it, `.pin_protrusion <= 3mm`, or leave it unknown); unknown is not zero and
//! keeps the old rule, so no board changes unless it states a protrusion. The covering part is
//! fitted by hand, after the pins are cut. The stack reading: the pins protrude `t`, the
//! covering part rests on them, so `t` plus its height must fit the limit a height zone puts on
//! that face where the part's body is. A covering part with no height over pins that protrude
//! is an error, since a missing fact would loosen a hard rule (it is not the warning of an
//! unknown height under a limit). The options were compared in
//! `docs/research/placement-language-options.md` section 1.
//!
//! **Failure modes.** F1 too tall: `C5 (`cell`) lies over the pins of J2 (`radio.row_a`), whose
//! pins protrude 0.40 mm (file:line), and that is not allowed: C5 stands 3.40 mm tall on pins
//! that protrude 0.40 mm: 3.80 mm from the bottom face, and the keepout `back_room` allows 3.60
//! mm there (0.20 mm too much); cut the pins to protrude 0.20 mm or less, or use a lower part`.
//! F2 no height and F5 assembled by the board house are findings of the same kind (`OverPins`).
//! F3, a part over pads with no stated protrusion, is the overlap of before with one more
//! sentence that names the member to state, and says that cutting the pins is a hand step to
//! state only if it is done; the hint is shown for any plated through-hole part whose pins come
//! out under another part with no stated protrusion. F4, that the pins are not cut in reality, is
//! not visible to software: `fab` prints a warning for every stated protrusion, and
//! `fab-notes.txt` and `order.txt` carry a `trim:` line beside `hand-solder:` (`J1 (row): after
//! soldering, cut the pins to protrude 0.20 mm from the bottom face, before U1 (cell) is fitted
//! (widget.physical.nsm:41); measure one protruding pin`), for every part that states a
//! protrusion, naming the parts that lie over it or saying that nothing does; the skill
//! `netsmith-prepare-fab-order` lists it as a step to verify. F6, no height zone over the
//! covering part, grants the excuse and `fab` warns that no limit was compared. F7, a
//! protrusion above what the part kind allows, is the ordinary refused claim of the solver,
//! which gives both numbers and points at the line of the bound.
//!
//! **Readings of the decisions** (the simplest reading of them, word for word from ADR 0045
//! section 15; accepted as built by the owner, 2026-11-11):
//! - "A protrusion is one value (`&=`). A bounded range (a kind that says at most 3 mm, a board
//!   that says nothing) is unknown."
//! - "A covering part with holes of its own is judged by the old rule."
//! - "The check measures the covering part's body against the zones; the placer measures the
//!   body grown by the placer's margin (section 11), so a clear result for the placer passes
//!   the check."
//! - "`.assembly` `hand` or `none` counts as fitted by hand."
//! - "The `trim:` line is written for every part that states a protrusion, naming the parts that
//!   lie over it, or saying that nothing does. `order.txt` shows it among each board's notes
//!   under "before you order", not in the board's summary line."
//! - "The hint of F3 is shown for any plated through-hole part whose pins come out under another
//!   part with no stated protrusion."
//! - "The cross-check against KiCad's own courtyard check was not run: `crosscheck` uses default
//!   extras, so KiCad and netsmith both report the overlap and agree."
//! - "The PCB view's drawing of the protruding pins and the covering part is not built; the
//!   editor lists the findings." Not planned until a board needs it.
//!
//! ADR 0048 (decision 12) adds that `.pin_protrusion`, like `.height`, may also be narrowed in
//! the board's circuit source: both places stay, as built.

use crate::board::{Placed, Side};
use crate::geometry::Rect;
use crate::placement::HeightZone;

/// How much over a limit a stack may be before it is refused: half a micrometre, as the
/// other height checks measure.
pub const TOLERANCE: f64 = 0.0005;

/// How far the pins of a through-hole part protrude out of the far side of the board, as
/// the design states it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Protrusion {
    /// How far they protrude out of the face opposite the part, in mm.
    pub length: f64,
    /// The lines of the physical file that state it (`widget.physical.nsm:41`).
    pub source: String,
}

/// Why a part over protruding pins is not excused.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    /// The board house assembles the part, so it is on the board before the pins are cut.
    HouseFitted,
    /// The part states no height.
    NoHeight,
    /// The protruding pins and the part together are taller than the zone allows. `stack` and `limit`
    /// are in mm.
    TooTall {
        stack: f64,
        limit: f64,
        zone: String,
    },
}

/// What the rule says of one part over the trimmed pins of another.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// The overlap is excused. `limit` is the tightest height limit over the part, with its
    /// zone's name, if any zone limits that face there; none means no limit was checked.
    Excused {
        limit: Option<(String, f64)>,
    },
    Refused(Refusal),
}

/// The facts about a covering part that the rule needs, as the judge has them.
#[derive(Debug, Clone, Copy)]
pub struct Cover {
    pub height: Option<f64>,
    pub hand_fitted: bool,
    /// Its body where it is.
    pub body: Rect,
    /// The face it is on.
    pub side: Side,
}

/// The rule, on plain facts: pins that protrude `protrusion` mm, covered by `cover`, under
/// `zones`.
pub fn judge(protrusion: f64, cover: &Cover, zones: &[HeightZone]) -> Verdict {
    if !cover.hand_fitted {
        return Verdict::Refused(Refusal::HouseFitted);
    }
    let Some(height) = cover.height else {
        return Verdict::Refused(Refusal::NoHeight);
    };
    let stack = protrusion + height;
    let mut tightest: Option<(String, f64)> = None;
    let mut worst: Option<(f64, Refusal)> = None;
    for z in zones.iter().filter(|z| {
        (if cover.side == Side::Back {
            z.back
        } else {
            z.front
        }) && z.rect.overlaps(&cover.body)
    }) {
        if tightest.as_ref().is_none_or(|(_, l)| z.limit < *l) {
            tightest = Some((z.name.clone(), z.limit));
        }
        let over = stack - z.limit;
        if over > TOLERANCE && worst.as_ref().is_none_or(|(w, _)| over > *w) {
            worst = Some((
                over,
                Refusal::TooTall {
                    stack,
                    limit: z.limit,
                    zone: z.name.clone(),
                },
            ));
        }
    }
    match worst {
        Some((_, refusal)) => Verdict::Refused(refusal),
        None => Verdict::Excused { limit: tightest },
    }
}

/// Whether `cover` can stand over the pins of `pin_part`: the part with the pins has stated
/// how far they protrude and has holes, the other is on the face the pins come out of and has no
/// holes of its own (a through-hole cover is judged by the rule without a protrusion). Returns how far they protrude.
pub fn pins_under(pin_part: &Placed, cover: &Placed) -> Option<f64> {
    let t = pin_part.protrusion.as_ref()?.length;
    (pin_part.has_hole() && !cover.has_hole() && cover.side != pin_part.side).then_some(t)
}

/// The part whose pins stand under the other, when how far they protrude is not stated (so that a
/// message can say how to state them): it has holes, the other has none and is on the face
/// the pins come out of.
pub fn unstated_under<'a>(a: &'a Placed, b: &'a Placed) -> Option<&'a Placed> {
    let under = |t: &'a Placed, c: &Placed| {
        let plated = t
            .footprint
            .pads
            .iter()
            .any(|p| p.drill.is_some() && p.copper_layers() != (false, false));
        (t.protrusion.is_none() && plated && !c.has_hole() && c.side != t.side).then_some(t)
    };
    under(a, b).or_else(|| under(b, a))
}

/// The facts of `p` as a covering part, at the place it has.
pub fn cover_of(p: &Placed) -> Cover {
    Cover {
        height: p.height,
        hand_fitted: p.hand_fitted,
        body: p.body(),
        side: p.side,
    }
}

/// The relation between two parts that the rule is about: whether the first has the pins
/// and the other covers them (`true`), or the other way round (`false`), and the verdict.
/// `None` if neither stands over the other's pins.
pub fn relation(a: &Placed, b: &Placed, zones: &[HeightZone]) -> Option<(bool, Verdict)> {
    if let Some(t) = pins_under(a, b) {
        return Some((true, judge(t, &cover_of(b), zones)));
    }
    pins_under(b, a).map(|t| (false, judge(t, &cover_of(a), zones)))
}

/// The pair `(pins first?)` when one stands excused over the other's pins.
pub fn excused(a: &Placed, b: &Placed, zones: &[HeightZone]) -> Option<bool> {
    match relation(a, b, zones)? {
        (first, Verdict::Excused { .. }) => Some(first),
        _ => None,
    }
}

/// The rooms two parts take on the face the pins come out of when the cover is excused: the
/// pads of the through-hole part, each grown by `clearance`, against the pads of the cover.
pub fn excused_rooms(pin_part: &Placed, cover: &Placed, clearance: f64) -> (Vec<Rect>, Vec<Rect>) {
    let protrusions = pin_part
        .footprint
        .pads
        .iter()
        .map(|p| pin_part.pad_bounds(p).expanded(clearance))
        .collect();
    let pads = cover
        .footprint
        .pads
        .iter()
        .map(|p| cover.pad_bounds(p))
        .collect();
    (protrusions, pads)
}

/// Whether a part with a stated protrusion may be stood over on `face`: what it takes there is not
/// certain, so a count of what is certainly taken leaves it out.
pub fn may_be_covered(p: &Placed, face: Side) -> bool {
    p.protrusion.is_some() && p.has_hole() && face != p.side
}

impl Refusal {
    /// The reason, as the end of a finding. `protrusion` is how far the pins protrude in mm, and
    /// `cover` and `path` name the covering part.
    pub fn because(&self, protrusion: f64, cover: &str, path: &str, face: Side) -> String {
        let cover = &crate::board::member(path, cover);
        match self {
            Refusal::HouseFitted => format!(
                "{cover} is assembled by the board house, which fits it before the pins are soldered and cut to protrude {} mm, and {cover} is to be fitted after that; make {cover} `hand` (`.{path}.assembly &= \"hand\"` in the circuit source), or move it off the pins",
                mm(protrusion)
            ),
            Refusal::NoHeight => format!(
                "{cover} states no height, so the stack on the {} mm the pins protrude cannot be checked; state `.{path}.height` with the real number, or move it off the pins",
                mm(protrusion)
            ),
            Refusal::TooTall { stack, limit, zone } => {
                let height = stack - protrusion;
                let room = limit - height;
                let advice = if room < 0.0 {
                    "no pin protrusion is short enough, so use a lower part".to_string()
                } else {
                    format!(
                        "cut the pins to protrude {} mm or less, or use a lower part",
                        mm(room)
                    )
                };
                format!(
                    "{cover} stands {} mm tall on pins that protrude {} mm: {} mm from the {} face, and the keepout `{zone}` allows {} mm there ({} mm too much); {advice}",
                    mm(height),
                    mm(protrusion),
                    mm(*stack),
                    face.word(),
                    mm(*limit),
                    mm(stack - limit)
                )
            }
        }
    }
}

/// A length in mm to two decimals, as the other height messages print it.
pub fn mm(v: f64) -> String {
    format!("{v:.2}")
}
