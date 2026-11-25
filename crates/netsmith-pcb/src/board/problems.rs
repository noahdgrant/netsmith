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
        /// Where that is, in mm from the board's top left corner, and the side or corner of
        /// the outline's box it is near, if it is near one (issue #783).
        at: Option<(f64, f64)>,
        near: Option<&'static str>,
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
        /// The height checked against the limit (the part's `.height` plus its
        /// `.height_growth`) and the keepout's limit, in mm.
        height: f64,
        limit: f64,
        /// The part's `.height_growth`, in mm, included in `height` (issue #1263); zero for a
        /// part that states none.
        growth: f64,
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

/// What a height finding says of a part's `.height_growth` (issue #1263): ` when grown (its
/// 6.00 mm height with its 0.50 mm ...)`, or nothing for a part that states none. `height` is
/// the grown height.
pub(super) fn grown_word(height: f64, growth: f64) -> String {
    if growth > 0.0 {
        format!(
            " when grown (its {:.2} mm height with its {growth:.2} mm `.height_growth` added)",
            height - growth
        )
    } else {
        String::new()
    }
}

/// What follows a standard's height for a part that states a `.height_growth` (` with its
/// 0.50 mm ... added`), or nothing for a part that states no growth (issue #1263).
fn growth_added(growth: f64) -> String {
    if growth > 0.0 {
        format!(" with its {growth:.2} mm `.height_growth` added")
    } else {
        String::new()
    }
}

/// What a message calls the thing that limits a height: a keepout by its name, the standoff
/// of a body (`standoff of J1`, [`Placed::STANDOFF_ZONE`](super::Placed::STANDOFF_ZONE)), or the
/// area under a part's body (`m1.underside_keepout`, issue #1335, or one of several,
/// `m1.underside_keepout_sim`, issue #1726), by the part it is under.
pub(super) fn limit_word(name: &str) -> String {
    if let Some(holder) = name.strip_prefix(Placed::STANDOFF_ZONE) {
        return format!("the standoff of {holder}");
    }
    match netsmith_core::underside_of(name) {
        Some((part, netsmith_core::UNDERSIDE_KEEPOUT)) => {
            format!("the underside keepout of `{part}`")
        }
        Some((part, member)) => format!("the underside keepout `.{member}` of `{part}`"),
        None => format!("the keepout `{name}`"),
    }
}

/// What a message calls the keepout a part is in: by its name, or, for the room a mounting
/// hole keeps for its screw head (`.head_diameter`), the head of that hole.
fn keepout_word(name: &str) -> String {
    match name.strip_prefix(netsmith_core::HEAD_ZONE) {
        Some(hole) => format!("the screw head of `{hole}`"),
        None => format!("the keepout `{name}`"),
    }
}

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
    /// The part's `.height_growth`, in mm (issue #1263), added to the standard's height; zero
    /// for a part that states none.
    pub growth: f64,
}

impl std::fmt::Display for HeightUnchecked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.standard {
            None => write!(
                f,
                "height of {} is unchecked: {} limits parts on the {} face to {:.2} mm and the part states no height (say `.{}.height &= 8mm`, with the real number)",
                member(&self.path, &self.refdes),
                limit_word(&self.keepout),
                self.side.word(),
                self.limit,
                self.path
            ),
            Some(s) => write!(
                f,
                "height of {} is unchecked: {} limits parts on the {} face to {:.2} mm, the part states no height, and its package standard ({}) allows up to {:.2} mm{}, so a part in that package may or may not fit (say `.{}.height &= 1mm`, with the real number from its datasheet)",
                member(&self.path, &self.refdes),
                limit_word(&self.keepout),
                self.side.word(),
                self.limit,
                s.standard,
                s.max,
                growth_added(self.growth),
                self.path
            ),
        }
    }
}

/// What a part of unknown height stands under, for [`HeightUnknown`] (issue #1117).
#[derive(Debug, Clone, PartialEq)]
pub enum UnknownUnder {
    /// A height limit over it on its face: a keepout's `.max_height` or the standoff of a body
    /// that states one (`standoff of J1`).
    Limit { keepout: String, limit: f64 },
    /// The body of another part of known height on its face that states no `.standoff`: the
    /// body is taken to stand on the board, so no limit is over the part, though a part under a
    /// module's body is under a limit that a standoff of the module would state.
    Body {
        refdes: String,
        path: String,
        /// The body's height, in mm.
        height: f64,
    },
}

/// A part of unknown height that stands under a height limit or under another part's body, so
/// that the height check cannot say whether it fits (issue #1117). Not a pass: see
/// [`Board::heights_unknown`] and [`heights_unknown_warning`].
#[derive(Debug, Clone, PartialEq)]
pub struct HeightUnknown {
    pub refdes: String,
    pub path: String,
    pub side: Side,
    /// Each limit or body it stands under, the limits first (the tightest first), in a fixed
    /// order.
    pub under: Vec<UnknownUnder>,
    /// What the tools treat the part as, which ranks the parts that stand under no limit
    /// ([`HeightUnknown::class_rank`]).
    pub kind: Option<netsmith_core::PartKind>,
    /// The body's area in mm², the tie-break of the package class (the larger body first).
    pub body_area: f64,
}

impl HeightUnknown {
    /// The lowest limit over the part, in mm: how tight the room above it is, the clearance a
    /// part of unknown height would have to fit in. `None` when no limit is over it (it stands
    /// only under a body that states no `.standoff`).
    pub fn tightest_limit(&self) -> Option<f64> {
        self.under
            .iter()
            .filter_map(|u| match u {
                UnknownUnder::Limit { limit, .. } => Some(*limit),
                UnknownUnder::Body { .. } => None,
            })
            .min_by(f64::total_cmp)
    }

    /// The package class of the part, larger for a class whose parts stand taller: a connector
    /// 5, an inductor or a crystal 4, an integrated circuit or a capacitor 3, a light emitting
    /// diode, a diode, a transistor or a fuse 2, a ferrite bead or a resistor 1, anything else 0.
    /// It ranks the parts that stand under no limit, where there is no clearance to rank by.
    pub fn class_rank(&self) -> u8 {
        use netsmith_core::PartKind as K;
        match self.kind {
            Some(K::Connector) => 5,
            Some(K::Inductor | K::Crystal) => 4,
            Some(K::Ic | K::Capacitor) => 3,
            Some(K::Led | K::Diode | K::Transistor | K::Fuse) => 2,
            Some(K::FerriteBead | K::Resistor) => 1,
            _ => 0,
        }
    }

    /// The order the warning lists the parts in (issue #1590): the parts under a limit first,
    /// the lowest limit (the least room) first; then the parts under a body alone, by package
    /// class (the taller class first) and then by the larger body; the path last, so the order
    /// is fixed.
    pub fn rank_cmp(&self, other: &Self) -> std::cmp::Ordering {
        let by_limit = match (self.tightest_limit(), other.tightest_limit()) {
            (Some(a), Some(b)) => a.total_cmp(&b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => other
                .class_rank()
                .cmp(&self.class_rank())
                .then(other.body_area.total_cmp(&self.body_area)),
        };
        by_limit.then_with(|| self.path.cmp(&other.path))
    }
}

impl std::fmt::Display for HeightUnknown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} on the {} face",
            member(&self.path, &self.refdes),
            self.side.word()
        )?;
        let mut sep = " under ";
        for u in &self.under {
            f.write_str(sep)?;
            sep = " and ";
            match u {
                UnknownUnder::Limit { keepout, limit } => {
                    write!(f, "a {limit:.2} mm limit ({})", limit_word(keepout))?
                }
                UnknownUnder::Body {
                    refdes,
                    path,
                    height,
                } => write!(
                    f,
                    "the body of {} ({height:.2} mm tall), which states no `.standoff` (the limit that a standoff would put over it)",
                    member(path, refdes)
                )?,
            }
        }
        Ok(())
    }
}

/// How many parts the warning `height_unknown` lists one by one; the rest are one count line.
const HEIGHT_UNKNOWN_LISTED: usize = 5;

/// The one warning `height_unknown` of `fab` and `dev board3d` (issue #1117): every part of
/// unknown height that stands under a height limit or under another part's body, with the limit
/// that would apply, so that a pass of the height check does not read as "fine". `None` when no
/// part is in that case. It is a warning always: `fab` still passes, whatever the constraints
/// are (the per-part `height_unchecked` is the error of strict constraints).
///
/// The parts are ranked (issue #1590, [`HeightUnknown::rank_cmp`]): the one with the least room
/// under its limit first, each naming the limit that makes it tight (`under a 1.24 mm limit`),
/// then the parts under a body alone by package class. The first `HEIGHT_UNKNOWN_LISTED`
/// are listed and a count line says what the rest stand under; `parts` need not be sorted.
pub fn heights_unknown_warning(parts: &[HeightUnknown]) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    let mut ranked: Vec<&HeightUnknown> = parts.iter().collect();
    ranked.sort_by(|a, b| a.rank_cmp(b));
    let (shown, rest) = ranked.split_at(HEIGHT_UNKNOWN_LISTED.min(ranked.len()));
    let mut list: Vec<String> = shown.iter().map(|p| p.to_string()).collect();
    if !rest.is_empty() {
        let limits: Vec<f64> = rest.iter().filter_map(|p| p.tightest_limit()).collect();
        let mut kinds: Vec<String> = Vec::new();
        if let Some(least) = limits.iter().copied().min_by(f64::total_cmp) {
            kinds.push(format!(
                "{} under a limit of {least:.2} mm or more",
                limits.len()
            ));
        }
        let bodies = rest.len() - limits.len();
        if bodies > 0 {
            kinds.push(format!("{bodies} under another part's body alone"));
        }
        list.push(format!(
            "and {} more, {} (all of them are in `dev board3d --format json`)",
            rest.len(),
            kinds.join(" and ")
        ));
    }
    let first = ranked[0];
    Some(format!(
        "height_unknown: the height check cannot say whether {} part{} of unknown height fit{}, so a pass does not cover {}; the tightest first: {}; state each part's `.height` (for example `.{}.height &= 8mm`, with the real number from its datasheet)",
        parts.len(),
        if parts.len() == 1 { "" } else { "s" },
        if parts.len() == 1 { "s" } else { "" },
        if parts.len() == 1 { "it" } else { "them" },
        list.join("; "),
        first.path
    ))
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
    /// The part's `.height_growth`, in mm (issue #1263), added to the standard's height; zero
    /// for a part that states none.
    pub growth: f64,
}

impl std::fmt::Display for StandardPass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} fits the {:.2} mm {} allows on the {} face by its package standard ({}, at most {:.2} mm{}), not by a height of its own: the design states none and its catalog row has none",
            member(&self.path, &self.refdes),
            self.limit,
            limit_word(&self.keepout),
            self.side.word(),
            self.standard.cited(),
            self.standard.max,
            growth_added(self.growth)
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
    /// The part's `.height_growth`, in mm.
    pub(super) growth: f64,
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
                at,
                near,
            } => {
                let text = outside_text(refdes, path, past, stated, by, &[], false);
                match (past.is_empty(), cut, at) {
                    (true, Some(cut), Some((x, y))) => {
                        let place = match near {
                            Some(near) => format!("by the {near} of the outline's box, at"),
                            None => "at".to_string(),
                        };
                        write!(
                            f,
                            "{} is not wholly inside the board outline: its courtyard is {cut:.2} mm past the outline's edge {place} ({x:.1}, {y:.1}) {tail}",
                            member(path, refdes),
                            tail = crate::prose::BOARD_TAIL
                        )
                    }
                    _ => write!(f, "{text}"),
                }
            }
            Self::InKeepout {
                refdes,
                path,
                keepout,
            } => write!(
                f,
                "{} overlaps {}, which forbids parts",
                member(path, refdes),
                keepout_word(keepout)
            ),
            Self::TooTall {
                refdes,
                path,
                keepout,
                height,
                limit,
                growth,
                side,
                at,
            } => write!(
                f,
                "{} stands {height:.2} mm tall on the {} face{}, and {} allows {limit:.2} mm there ({:.2} mm too tall), where its body is, around ({:.1}, {:.1}) {}",
                member(path, refdes),
                side.word(),
                grown_word(*height, *growth),
                limit_word(keepout),
                height - limit,
                at.0,
                at.1,
                crate::prose::BOARD_TAIL
            ),
            Self::PadInKeepout {
                refdes,
                path,
                keepout,
            } => write!(
                f,
                "a pad of {} is inside {}, which forbids tracks, so none can reach it",
                member(path, refdes),
                keepout_word(keepout)
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
                    "the cutout `{cutout}` is not wholly inside the board outline; a notch in the board's edge is a polygon outline, not a cutout (language guide, chapter 23, \"A polygon outline\")"
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
