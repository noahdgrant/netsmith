//! What the placement check finds, and the words for each finding.

use super::*;

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
pub(super) fn pad_text(number: &str) -> String {
    if number.is_empty() {
        String::new()
    } else {
        format!(" (pad {number})")
    }
}

/// How much taller than a limit a part may be before it is reported: half a micrometre, as
/// the rest of the checks measure.
pub(super) const HEIGHT_TOLERANCE: f64 = 0.0005;

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
pub(super) struct UnderLimit {
    pub(super) refdes: String,
    pub(super) path: String,
    pub(super) keepout: String,
    pub(super) side: Side,
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
