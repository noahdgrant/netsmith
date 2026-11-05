//! Trimmed pin tails: a flat part over the cut pins of a through-hole part (ADR 0031
//! amendment, ADR 0045 section 14, PROVISIONAL; owner decisions of 2026-11-04).
//!
//! A through-hole part takes room on both faces (`Placed::room_on`). The board's physical
//! file may state that its pins are cut short after soldering (`.tail_height`, a length).
//! Then a part on the face the pins come out of may stand over them, if
//!
//! 1. the part is fitted by hand, after the pins are cut (the board house fits its parts
//!    before any pin is soldered, so a part it assembles cannot be put over cut pins);
//! 2. its height is known;
//! 3. the stack fits: the tails stand `t` out of the face, the part rests on them, so `t`
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

use crate::board::{Placed, Side};
use crate::geometry::Rect;
use crate::place::HeightZone;

/// How much over a limit a stack may be before it is refused: half a micrometre, as the
/// other height checks measure.
pub const TOLERANCE: f64 = 0.0005;

/// The pins of a through-hole part, cut short after soldering, as the design states it.
#[derive(Debug, Clone, PartialEq)]
pub struct Tail {
    /// How far they stand out of the face opposite the part, in mm.
    pub length: f64,
    /// The lines of the physical file that state it (`widget.physical.nsm:41`).
    pub source: String,
}

/// Why a part over trimmed pins is not excused.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    /// The board house assembles the part, so it is on the board before the pins are cut.
    HouseFitted,
    /// The part states no height.
    NoHeight,
    /// The tails and the part together are taller than the zone allows. `stack` and `limit`
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
    Excused { limit: Option<(String, f64)> },
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

/// The rule, on plain facts: pins cut to `tail` mm, covered by `cover`, under `zones`.
pub fn judge(tail: f64, cover: &Cover, zones: &[HeightZone]) -> Verdict {
    if !cover.hand_fitted {
        return Verdict::Refused(Refusal::HouseFitted);
    }
    let Some(height) = cover.height else {
        return Verdict::Refused(Refusal::NoHeight);
    };
    let stack = tail + height;
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

/// Whether `cover` can stand over the tails of `tail_part`: the part with the tails has
/// stated them and has holes, the other is on the face the pins come out of and has no
/// holes of its own (a through-hole cover is judged by today's rule). Returns the tail.
pub fn tail_under(tail_part: &Placed, cover: &Placed) -> Option<f64> {
    let t = tail_part.tail.as_ref()?.length;
    (tail_part.has_hole() && !cover.has_hole() && cover.side != tail_part.side).then_some(t)
}

/// The part whose pins stand under the other, when its tails are not stated (so that a
/// message can say how to state them): it has holes, the other has none and is on the face
/// the pins come out of.
pub fn untrimmed_under<'a>(a: &'a Placed, b: &'a Placed) -> Option<&'a Placed> {
    let under = |t: &'a Placed, c: &Placed| {
        let plated = t
            .footprint
            .pads
            .iter()
            .any(|p| p.drill.is_some() && p.copper_layers() != (false, false));
        (t.tail.is_none() && plated && !c.has_hole() && c.side != t.side).then_some(t)
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

/// The relation between two parts that the rule is about: whether the first has the tails
/// and the other covers them (`true`), or the other way round (`false`), and the verdict.
/// `None` if neither stands over the other's pins.
pub fn relation(a: &Placed, b: &Placed, zones: &[HeightZone]) -> Option<(bool, Verdict)> {
    if let Some(t) = tail_under(a, b) {
        return Some((true, judge(t, &cover_of(b), zones)));
    }
    tail_under(b, a).map(|t| (false, judge(t, &cover_of(a), zones)))
}

/// The pair `(tail part first?)` when one stands excused over the other's pins.
pub fn excused(a: &Placed, b: &Placed, zones: &[HeightZone]) -> Option<bool> {
    match relation(a, b, zones)? {
        (first, Verdict::Excused { .. }) => Some(first),
        _ => None,
    }
}

/// The rooms two parts take on the face the pins come out of when the cover is excused: the
/// pads of the through-hole part, each grown by `clearance`, against the pads of the cover.
pub fn excused_rooms(tail_part: &Placed, cover: &Placed, clearance: f64) -> (Vec<Rect>, Vec<Rect>) {
    let tails = tail_part
        .footprint
        .pads
        .iter()
        .map(|p| tail_part.pad_bounds(p).expanded(clearance))
        .collect();
    let pads = cover
        .footprint
        .pads
        .iter()
        .map(|p| cover.pad_bounds(p))
        .collect();
    (tails, pads)
}

/// Whether a part with stated tails may be stood over on `face`: what it takes there is not
/// certain, so a count of what is certainly taken leaves it out.
pub fn may_be_covered(p: &Placed, face: Side) -> bool {
    p.tail.is_some() && p.has_hole() && face != p.side
}

impl Refusal {
    /// The reason, as the end of a finding. `tail` is the length of the tails in mm, and
    /// `cover` and `path` name the covering part.
    pub fn because(&self, tail: f64, cover: &str, path: &str, face: Side) -> String {
        match self {
            Refusal::HouseFitted => format!(
                "{cover} is assembled by the board house, which fits it before any pin is soldered or cut, and the pins are cut after soldering, before {cover} is fitted; make {cover} `hand` (`.{path}.assembly &= \"hand\"` in the circuit source), or move it off the pins"
            ),
            Refusal::NoHeight => format!(
                "{cover} states no height, so the stack on the tails cannot be checked; state `.{path}.height` with the real number, or move it off the pins"
            ),
            Refusal::TooTall { stack, limit, zone } => {
                let height = stack - tail;
                let room = limit - height;
                let advice = if room < 0.0 {
                    "no tail is short enough, so use a lower part".to_string()
                } else {
                    format!("trim the pins to {} mm or less, or use a lower part", mm(room))
                };
                format!(
                    "{cover} stands {} mm tall on tails of {} mm: {} mm from the {} face, and the keepout `{zone}` allows {} mm there ({} mm too much); {advice}",
                    mm(height),
                    mm(tail),
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
