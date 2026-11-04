//! The annealer behind derived placement (ADR 0033).
//!
//! It moves footprints about on a grid of 0.05 mm, turns them by right angles, and keeps a
//! change when it lowers a cost, or, with a probability that falls as the run goes on, when
//! it raises it. The cost is the sum of the terms below; `W_*` are their weights.
//!
//! - **Wire**: the half perimeter of the box round each net's pad centres, weighted by the
//!   net (a plane counts little, a long net less than a short one).
//! - **Overlap**: courtyards that overlap (hard), and that are nearer than [`GAP`] (soft; more on a board with a pour),
//!   or [`Rules::hole_to_hole`] and the clearance if one has a hole through the board. Parts
//!   on opposite faces share area unless one has a hole.
//! - **Walls**: a courtyard nearer the outline than [`Rules::edge_wall`], a pad nearer than the
//!   copper to edge space, reference text nearer than
//!   [`TEXT_EDGE`], and a courtyard inside a keepout (all hard).
//! - **Silkscreen**: reference text, and the strokes of a footprint's silkscreen, within the
//!   process's silk to pad space of another part's pads and mask openings, or near its
//!   silkscreen or text; and, a little, text over another part's courtyard. The weight of
//!   these grows through a run.
//! - **Crossing**: pairs of ratsnest lines of two nets that cross, which the router has to untangle
//!   with vias and detours.
//! - **Cohesion**: the half perimeter of the box round the parts of each module instance
//!   (each prefix of the instance path), so a module stays together.
//! - **Decoupling**: the distance from a capacitor's pads to the power pins it decouples
//!   ([`crate::quality::decoupling_pairs`]).
//! - **Edge**: a connector toward the nearest edge, a hole or fiducial toward a corner.
//! - **Area**: the half perimeter of the box round everything, so the board is no larger
//!   than it must be; and a small cost on every turned part, so a tie goes to the upright
//!   one.
//!
//! The run is a function of the parts, the nets and the seed only: positions are whole
//! units, the random numbers come from a generator written here, and nothing that a
//! platform's maths library may round differently (`exp`, `powf`, `hypot`) is used, so the
//! same input gives the same board on any machine.

use crate::board::{BoardNet, Placed, ReferenceSide, Side, rect_distance};
use crate::constraints::{PlanOffset, Want, axis_of, offset, quarter};
use netsmith_core::PlaceField;

use crate::geometry::{Pt, Rect};
use crate::overhang::Edges;
use crate::quality::{DecouplingPair, decoupling_pairs};
use crate::rules::Rules;

/// The grid positions are on, in mm.
const UNIT: f64 = 0.05;
/// The least space wanted between courtyards, in mm.
const GAP: f64 = 1.0;
/// More space between parts on a board with a pour on an outer layer, which needs room round
/// a pad for the spokes that join it to the pour.
const POUR_GAP: f64 = 0.8;
/// The space kept between reference text and the board edge.
pub(crate) const TEXT_EDGE: f64 = 0.5;
/// The space wanted between a courtyard and a keepout.
const KEEPOUT_GAP: f64 = 0.25;
/// A pad is kept this much further than the rule from a cutout, so that the rounding of
/// positions on the grid never leaves it a hair nearer than the placement check allows.
const PAD_SLACK: f64 = 1e-6;
/// What the box of reference text is grown by on every side: the letters' strokes and the
/// slack of the estimate of their size, found by trying boards against KiCad's check.
const TEXT_PAD: f64 = 0.15;
/// The space wanted between reference text and the silkscreen or text of another part.
const SILK_CLEAR: f64 = 0.2;
/// Extra space round a part for each of its pads, to a most of [`MAX_ESCAPE`], in mm.
const ESCAPE_PER_PAD: f64 = 0.03;
const MAX_ESCAPE: f64 = 2.5;
/// The moves tried for each part that is free, at most [`MAX_MOVES`] and at least 2000.
const MOVES_PER_PART: f64 = 3000.0;
const MAX_MOVES: usize = 300_000;

/// The weight of a conflict of reference text with another part at the start of a run and
/// before its last part, and how far through the run the weight is that of a hard rule.
const TEXT_WEIGHT_FROM: f64 = 2.0;
const TEXT_WEIGHT_TO: f64 = 40.0;
const TEXT_FULL_AT: f64 = 0.7;
/// The temperature falls by this factor after each stage of moves.
const COOLING: f64 = 0.93;

/// The weights of the cost terms, in mm of wire: a net's half perimeter counts once.
const W_WIRE: f64 = 1.0;
/// A hard violation (overlap, a wall, a keepout), for each mm of it.
const W_HARD: f64 = 200.0;
/// Courtyards nearer than their gap, for each mm.
const W_GAP: f64 = 4.0;
/// Reference text over another part's courtyard, for each mm.
const W_TEXT: f64 = 3.0;
/// The half perimeter of the box round a module instance's parts, and each part of another
/// instance inside that box.
const W_COHESION: f64 = 0.3;
const W_INTRUDER: f64 = 0.05;
/// Each mm from a decoupling capacitor's pads to the power pins they belong with.
const W_DECOUPLING: f64 = 6.0;
/// Each mm a connector is from the nearest edge, a hole or fiducial from the nearest corner.
const W_EDGE: f64 = 0.4;
/// The half perimeter of the box round everything.
const W_AREA: f64 = 0.03;
/// Each pair of ratsnest lines of two nets that cross.
const W_CROSSING: f64 = 1.0;
/// Nets with more pads than this have no lines counted.
const MAX_LINE_PADS: usize = 10;
/// A part that is turned, or has its text off the top.
const W_TURN: f64 = 0.3;

/// Everything the annealer needs besides the parts.
pub(crate) struct Problem<'a> {
    pub nets: &'a [BoardNet],
    pub rules: &'a Rules,
    /// The nets poured as planes on inner layers: a pad reaches one with a via.
    pub planes: &'a [String],
    /// The board's outline: the parts are placed inside it.
    pub region: Rect,
    pub corner_radius: f64,
    /// For each part, whether it stays where it is.
    pub fixed: &'a [bool],
    /// Boxes no courtyard may touch.
    pub keepouts: &'a [Rect],
    /// The cutouts, which are also in `keepouts`: no copper of a part is nearer to one than
    /// `rules.copper_to_edge`, the distance the placement check asks for (`PadNearCutout`).
    pub cutouts: &'a [Rect],
    /// Whether the board has a pour on an outer layer.
    pub pours: bool,
    pub seed: u64,
    /// For each part, where the design's requirements let it be (ADR 0045); empty or `None`
    /// is anywhere. A part is only ever put inside its want.
    pub wants: &'a [Option<Want>],
    /// The relations between two parts that the design requires (ADR 0045, section 3). Each
    /// is a hard rule: the measure of one part's body minus the other's stays in a window,
    /// and the two parts are moved together toward it.
    pub offsets: &'a [PlanOffset],
    /// For each part, the edges of `outline` its requirements say it may hang over (ADR 0045,
    /// section 10); empty is none. Such a part takes only the room its courtyard has inside
    /// the outline against other courtyards, as `Board::check_placement` counts it.
    pub overhangs: &'a [Edges],
    /// The board's outline, which the courtyards of overhanging parts are clipped to.
    pub outline: Rect,
    /// The areas that limit how tall a part may stand (ADR 0045, "Height limits"): a part
    /// with a stated height over a zone's limit, on a face the zone limits, is kept out of it.
    pub zones: &'a [crate::place::HeightZone],
}

/// What a run ended with.
pub(crate) struct Outcome {
    /// No courtyards overlap, and all are inside the walls and out of the keepouts.
    pub feasible: bool,
    /// What the run ended with that breaks a hard rule, for the message when it is not
    /// feasible.
    pub ended: Ended,
}

/// How many hard rules the final state of a run breaks, and which parts break them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ended {
    /// Pairs of parts, on the same copper, whose courtyards overlap.
    pub overlaps: usize,
    /// Parts that are outside the outline or nearer its edge than the margin, or in a keepout.
    pub outside: usize,
    /// Pairs of parts whose reference text or silkscreen lie over the other's pads.
    pub silk: usize,
    /// The overlapping pairs by name, the most overlapped first.
    pub overlapping: Vec<OverlapAt>,
    /// The parts that are outside, by name, the furthest first.
    pub outside_parts: Vec<OutsideAt>,
    /// The relations between two parts that do not hold, each as a sentence with the numbers
    /// (ADR 0045, section 3).
    pub relations: Vec<String>,
}

/// Two courtyards that overlap when a run ended.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlapAt {
    pub a: String,
    pub b: String,
    /// The area they share, in mm2, and its size along x and along y.
    pub area: f64,
    pub size: (f64, f64),
    /// Where the overlap is, in the outline's frame (its top left corner is 0, 0), and the
    /// side of the outline it is nearest, with the distance to it.
    pub at: (f64, f64),
    pub nearest: (&'static str, f64),
}

/// A part that is outside the outline, inside its margin, or in a keepout when a run ended.
#[derive(Debug, Clone, PartialEq)]
pub struct OutsideAt {
    pub part: String,
    /// What is wrong with each direction, as a sentence: `0.80 mm outside the left edge`.
    pub why: Vec<String>,
    /// The worst of them in mm, for the order.
    pub worst: f64,
}

/// The side of `region` a point is nearest to, and how far from it.
fn nearest_edge(region: &Rect, p: Pt) -> (&'static str, f64) {
    [
        ("left", p.x - region.min.x),
        ("right", region.max.x - p.x),
        ("top", p.y - region.min.y),
        ("bottom", region.max.y - p.y),
    ]
    .into_iter()
    .fold(
        ("left", f64::MAX),
        |best, e| if e.1 < best.1 { e } else { best },
    )
}

/// A length in mm for a message: rounded to a tenth of a micrometre, no trailing zeros.
fn mm(v: f64) -> String {
    let r = (v * 1e4).round() / 1e4;
    format!("{}", if r == 0.0 { 0.0 } else { r })
}

/// A window as a person says it.
fn window_text(lo: f64, hi: f64) -> String {
    match (lo.is_finite(), hi.is_finite()) {
        (true, true) if (hi - lo).abs() < 1e-9 => format!("{} mm", mm(lo)),
        (true, true) => format!("{} to {} mm", mm(lo), mm(hi)),
        (true, false) => format!("at least {} mm", mm(lo)),
        (false, true) => format!("at most {} mm", mm(hi)),
        (false, false) => "any value".to_string(),
    }
}

/// A small random number generator (SplitMix64), so that nothing depends on a crate whose
/// output may change.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        let mut r = Rng(seed ^ 0x9E37_79B9_7F4A_7C15);
        r.next();
        r
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `n`, which is more than 0.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// A number in 0 up to but not including 1.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A whole number from `-reach` to `reach`.
    fn spread(&mut self, reach: i64) -> i64 {
        (self.next() % (2 * reach as u64 + 1)) as i64 - reach
    }
}

/// `e` to the minus `x` for `x` of 0 or more, from additions and multiplications only, so
/// that it is the same on every machine.
fn exp_neg(x: f64) -> f64 {
    if x > 40.0 {
        return 0.0;
    }
    let y = x / 1024.0;
    let mut r =
        1.0 - y + y * y / 2.0 - y * y * y / 6.0 + y * y * y * y / 24.0 - y * y * y * y * y / 120.0;
    for _ in 0..10 {
        r *= r;
    }
    r
}

/// The square of the distance between two points.
fn dist2(a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (a.x - b.x, a.y - b.y);
    dx * dx + dy * dy
}

/// The square root of the sum of the squares, which the standard `hypot` may round
/// differently on another platform.
fn dist(a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (a.x - b.x, a.y - b.y);
    (dx * dx + dy * dy).sqrt()
}

#[derive(Clone, Copy, Debug, Default)]
struct Cost {
    soft: f64,
    hard: f64,
}

impl Cost {
    fn total(self) -> f64 {
        self.soft + self.hard
    }
}

impl std::ops::AddAssign for Cost {
    fn add_assign(&mut self, o: Cost) {
        self.soft += o.soft;
        self.hard += o.hard;
    }
}

/// Where a part is: its footprint origin in units, its turn in quarter turns from the angle
/// it began at, and which side of the part its reference text is on (a [`ReferenceSide`] as a
/// number).
#[derive(Clone, Copy, PartialEq)]
struct Pose {
    x: i64,
    y: i64,
    turn: u8,
    text: u8,
}

/// The shapes of a part: four turns, each with the text on each of four sides.
const VARIANTS: usize = 16;

fn variant(turn: u8, text: u8) -> usize {
    usize::from(turn) * 4 + usize::from(text)
}

/// The boxes of one part in one turn, relative to its origin.
struct Shape {
    court: Rect,
    /// The body (what a requirement on the place measures), relative to the origin.
    body: Rect,
    text: Option<Rect>,
    silk: Vec<Rect>,
    /// Where each pad of the footprint is.
    pads: Vec<Pt>,
    /// The copper of the pads and the mask openings the footprint draws.
    copper: Vec<Rect>,
    /// The copper of the pads alone.
    pad_boxes: Vec<Rect>,
    /// How far inside the courtyard the nearest pad box is, on its nearest side: negative if
    /// a pad reaches past it. The courtyard's wall keeps the pads off the edge by this much
    /// more.
    pad_inset: f64,
    /// Every pad's box, copper or not, grown by the clearance: what a part with a hole takes
    /// on the face its footprint draws no courtyard for (`Placed::room_on`).
    rooms: Vec<Rect>,
    /// Everything of the part that other parts must keep clear of, grown by half the
    /// widest space asked for: two parts whose reaches do not overlap do not interact.
    reach: Rect,
}

#[derive(PartialEq)]
enum Role {
    Plain,
    Connector,
    Mechanical,
}

struct Model {
    shapes: Vec<Shape>,
    angles: [f64; 4],
    side: Side,
    hole: bool,
    /// Which layers the footprint draws its courtyard on, `(F.CrtYd, B.CrtYd)`.
    court_faces: (bool, bool),
    role: Role,
    fixed: bool,
    /// Extra space wanted round the part so that its pads can be reached: the more pads, the
    /// more tracks leave it.
    escape: f64,
    /// Where the design's requirements let the part be, if they say.
    want: Option<Limits>,
    /// Whether the requirements let the part hang over the board edge (ADR 0045): its walls
    /// are the design's along the axes it holds, and its pads stay inside the outline.
    overhangs: bool,
    /// The edges the part may hang over: its courtyard counts clipped to the outline against
    /// other courtyards.
    may: Edges,
    /// The boxes the part is too tall to stand in, on the face it is on.
    no_go: Vec<Rect>,
}

/// A [`Want`] in the engine's terms, for each of the part's four turns (as the annealer
/// numbers them, from the angle the part began at).
struct Limits {
    /// For each turn: the allowed origins in units of the grid, `None` if the turn is ruled
    /// out, and the exact origin of an axis the requirements pin.
    turns: [Option<Turn>; 4],
    holds_x: bool,
    holds_y: bool,
}

#[derive(Clone, Copy)]
struct Turn {
    x: (i64, i64),
    y: (i64, i64),
    pin_x: Option<f64>,
    pin_y: Option<f64>,
}

impl Limits {
    fn of(want: &Want, base: f64) -> Limits {
        let units = |(lo, hi): (f64, f64)| {
            let first = if lo.is_finite() {
                ((lo - 1e-9) / UNIT).ceil() as i64
            } else {
                i64::MIN / 4
            };
            let last = if hi.is_finite() {
                ((hi + 1e-9) / UNIT).floor() as i64
            } else {
                i64::MAX / 4
            };
            (first, last.max(first))
        };
        let pin = |(lo, hi): (f64, f64)| (lo == hi && lo.is_finite()).then_some(lo);
        let at = |range: (f64, f64)| match pin(range) {
            Some(v) => {
                let unit = (v / UNIT).round() as i64;
                (unit, unit)
            }
            None => units(range),
        };
        let mut turns = [None; 4];
        for (t, slot) in turns.iter_mut().enumerate() {
            *slot = quarter(base + 90.0 * t as f64)
                .and_then(|q| want.reach[q])
                .map(|r| Turn {
                    x: at(r.x),
                    y: at(r.y),
                    pin_x: pin(r.x),
                    pin_y: pin(r.y),
                });
        }
        Limits {
            turns,
            holds_x: want.holds_x,
            holds_y: want.holds_y,
        }
    }
}

/// The pads of a decoupling pair on one net: the capacitor's, and the power pins' (part, pad).
type PairPads = (Vec<usize>, Vec<(usize, usize)>);

/// A decoupling pair with its pads found.
struct Dec {
    cap: usize,
    nets: [PairPads; 2],
}

/// The straight lines of a net's ratsnest.
type Lines = Vec<(Pt, Pt)>;

/// A relation between the places of two parts (ADR 0045, section 3) as the annealer keeps it:
/// the measure of the field `fb` of `b`'s body minus that of the field `fa` of `a`'s, along
/// `axis` (0 for x, 1 for y), stays in `lo..=hi`.
struct Link {
    a: usize,
    b: usize,
    fa: PlaceField,
    fb: PlaceField,
    axis: usize,
    lo: f64,
    hi: f64,
    /// `radio.row_b.place.x - radio.row_a.place.x (radio.physical.nsm:12)`.
    label: String,
}

impl Link {
    fn miss(&self, value: f64) -> f64 {
        (self.lo - value).max(value - self.hi).max(0.0)
    }

    /// The window is too narrow for the grid to be sure of a position in it.
    fn needs_alignment(&self) -> bool {
        self.lo.is_finite() && self.hi.is_finite() && self.hi - self.lo < 2.0 * UNIT
    }
}

/// How far a relation may be from its window and still count as met, in mm: the check's own
/// tolerance (half a micrometre) is far above it.
const LINK_EPS: f64 = 1e-6;

/// A net as the annealer sees it.
struct NetModel {
    pads: Vec<(usize, usize)>,
    weight: f64,
    /// Whether the net's ratsnest lines are counted for crossings: a net that is not a plane
    /// and has few pads.
    lines: bool,
}

struct Engine<'a> {
    p: &'a Problem<'a>,
    models: Vec<Model>,
    poses: Vec<Pose>,
    nets: Vec<NetModel>,
    nets_of: Vec<Vec<usize>>,
    decs: Vec<Dec>,
    decs_of: Vec<Vec<usize>>,
    groups: Vec<Vec<usize>>,
    groups_of: Vec<Vec<usize>>,
    /// For each module instance, whether each part is one of its parts.
    in_group: Vec<Vec<bool>>,
    neighbours: Vec<Vec<(usize, usize)>>,
    free: Vec<usize>,
    /// The relations between two parts, and for each part the relations it is in.
    links: Vec<Link>,
    links_of: Vec<Vec<usize>>,
    link_mark: Vec<u32>,
    /// For each part and axis, the relation through which the part follows another's place
    /// on that axis, if one does: its origin is on the grid shifted by `sub`, so that a window
    /// narrower than the grid can be met exactly. A part follows at most one other on an
    /// axis, and nothing follows itself.
    leader_of: Vec<[Option<usize>; 2]>,
    /// The parts that follow, with their axis, leaders before the parts that follow them.
    order: Vec<(usize, usize)>,
    /// How far each part's origin is from the grid, kept up to date by [`Engine::realign`].
    sub: Vec<Pt>,
    /// Marks that a net, a pair or a group was counted in the evaluation being made.
    /// The ratsnest lines of each net that has them, as the parts are now, and the box round
    /// them; kept up to date by [`Engine::refresh`].
    lines: Vec<Vec<(Pt, Pt)>>,
    line_box: Vec<Option<Rect>>,
    net_mark: Vec<u32>,
    dec_mark: Vec<u32>,
    group_mark: Vec<u32>,
    part_mark: Vec<u32>,
    stamp: u32,
    gap: f64,
    hole_gap: f64,
    /// Pairs of parts (index `i * n + j`) that need no escape space between them: a
    /// decoupling capacitor and the part it decouples.
    near_ok: Vec<bool>,
    /// Each part's reach on the board, kept up to date by [`Engine::put`].
    reach_abs: Vec<Rect>,
    /// What a conflict of reference text or silkscreen with another part costs while it is
    /// as heavy as a hard rule: it grows through a run, from a hint to a rule (`text_last`).
    text_weight: f64,
    text_last: bool,
}

/// The box the reference text takes, on the board, or `None` for a part that prints none.
/// The text is upright whatever the part's turn: its place turns with the part, and its
/// letters do not (KiCad takes the angle of a footprint's text as the board's).
pub(crate) fn text_box(part: &Placed, rules: &Rules) -> Option<Rect> {
    if part.kind == "Label" {
        return None;
    }
    let (height, width) = (part.text_height(rules), part.text_width(rules));
    let c = part.to_board(part.reference_at(rules));
    Some(
        Rect::new(
            Pt::new(c.x - width / 2.0, c.y - height / 2.0),
            Pt::new(c.x + width / 2.0, c.y + height / 2.0),
        )
        .expanded(TEXT_PAD),
    )
}

fn shape_of(
    part: &mut Placed,
    angle: f64,
    text_side: ReferenceSide,
    rules: &Rules,
    reach_pad: f64,
) -> Shape {
    let (at, rotation, side) = (part.at, part.rotation, part.reference_side);
    part.at = Pt::default();
    part.rotation = angle;
    part.reference_side = text_side;
    let court = part.courtyard();
    let body = part.body();
    let text = text_box(part, rules);
    // The strokes of the silkscreen, each as a box (a rectangle as its four sides).
    let silk: Vec<Rect> = part
        .footprint
        .graphics
        .iter()
        .filter(|g| g.layer == crate::footprint::Layer::Silk)
        .flat_map(|g| {
            let pieces: Vec<Vec<Pt>> = match &g.shape {
                crate::footprint::Shape::Line { a, b } => vec![vec![*a, *b]],
                crate::footprint::Shape::Rect { a, b } => {
                    let (c, d) = (Pt::new(b.x, a.y), Pt::new(a.x, b.y));
                    vec![vec![*a, c], vec![c, *b], vec![*b, d], vec![d, *a]]
                }
                crate::footprint::Shape::Poly { points } => {
                    points.windows(2).map(|w| vec![w[0], w[1]]).collect()
                }
                other => vec![other.extent_points()],
            };
            let stroke = g.stroke;
            pieces
                .into_iter()
                .filter_map(move |pts| Rect::around(pts).map(|r| r.expanded(stroke / 2.0)))
        })
        .map(|r| part.rect_to_board(r))
        .collect();
    let pads: Vec<Pt> = part
        .footprint
        .pads
        .iter()
        .map(|p| part.pad_centre(p))
        .collect();
    let mut copper: Vec<Rect> = part
        .footprint
        .pads
        .iter()
        .filter(|p| part.pad_layers(p) != (false, false))
        .map(|p| part.pad_bounds(p))
        .collect();
    let pad_boxes = copper.clone();
    let pad_inset = pad_boxes
        .iter()
        .map(|b| {
            (b.min.x - court.min.x)
                .min(court.max.x - b.max.x)
                .min(b.min.y - court.min.y)
                .min(court.max.y - b.max.y)
        })
        .fold(f64::MAX, f64::min);
    let rooms: Vec<Rect> = part
        .footprint
        .pads
        .iter()
        .map(|p| part.pad_bounds(p).expanded(rules.clearance))
        .collect();
    copper.extend(part.mask_openings());
    part.at = at;
    part.rotation = rotation;
    part.reference_side = side;
    let mut reach = court;
    for r in text.iter().chain(silk.iter()).chain(copper.iter()) {
        reach = reach.union(r);
    }
    Shape {
        court,
        body,
        text,
        silk,
        pads,
        copper,
        pad_boxes,
        pad_inset,
        rooms,
        reach: reach.expanded(reach_pad),
    }
}

/// How far two boxes overlap, as the least of the overlaps along the axes after growing the
/// first by `gap`: positive when they are nearer than `gap` (and overlapping if more than
/// `gap`).
fn penetration(a: &Rect, b: &Rect, gap: f64) -> f64 {
    let dx = a.max.x.min(b.max.x) - a.min.x.max(b.min.x);
    let dy = a.max.y.min(b.max.y) - a.min.y.max(b.min.y);
    if dx > -gap && dy > -gap {
        dx.min(dy) + gap
    } else {
        0.0
    }
}

impl<'a> Engine<'a> {
    fn new(parts: &mut [Placed], p: &'a Problem<'a>) -> Engine<'a> {
        let n = parts.len();
        let rules = p.rules;
        let gap = GAP + if p.pours { POUR_GAP } else { 0.0 };
        let hole_gap = rules.clearance.max(rules.hole_to_hole).max(gap);
        let reach_pad = (hole_gap.max(rules.silk_to_pad)).max(gap + MAX_ESCAPE) / 2.0 + 0.01;
        let models: Vec<Model> = (0..n)
            .map(|i| {
                let base = parts[i].rotation;
                let angles = [base, base + 90.0, base + 180.0, base + 270.0];
                // A part that stays has the one shape it has; any other, every turn with the
                // text on every side.
                let mut shapes: Vec<Shape> = Vec::new();
                if p.fixed[i] {
                    let side = parts[i].reference_side;
                    shapes.push(shape_of(&mut parts[i], base, side, rules, reach_pad));
                } else {
                    for &a in &angles {
                        for text in ReferenceSide::ALL {
                            shapes.push(shape_of(&mut parts[i], a, text, rules, reach_pad));
                        }
                    }
                }
                let kind = parts[i].kind.as_str();
                Model {
                    shapes,
                    angles,
                    side: parts[i].side,
                    hole: parts[i].has_hole(),
                    court_faces: parts[i].footprint.courtyard_faces,
                    role: if crate::place::is_connector(kind) {
                        Role::Connector
                    } else if crate::fit::is_mechanical(kind) {
                        Role::Mechanical
                    } else {
                        Role::Plain
                    },
                    fixed: p.fixed[i],
                    escape: (parts[i].footprint.pads.len() as f64 * ESCAPE_PER_PAD).min(MAX_ESCAPE),
                    want: p
                        .wants
                        .get(i)
                        .and_then(Option::as_ref)
                        .filter(|_| !p.fixed[i])
                        .map(|w| Limits::of(w, base)),
                    overhangs: p
                        .wants
                        .get(i)
                        .and_then(Option::as_ref)
                        .is_some_and(|w| w.overhangs),
                    may: p.overhangs.get(i).copied().unwrap_or_default(),
                    no_go: match parts[i].height {
                        Some(h) => p
                            .zones
                            .iter()
                            .filter(|z| z.rules_out(h, parts[i].side))
                            .map(|z| z.rect)
                            .collect(),
                        None => Vec::new(),
                    },
                }
            })
            .collect();
        let poses: Vec<Pose> = parts
            .iter()
            .map(|q| Pose {
                x: (q.at.x / UNIT).round() as i64,
                y: (q.at.y / UNIT).round() as i64,
                turn: 0,
                text: q.reference_side as u8,
            })
            .collect();

        // Nets, with the pads found once.
        let index: std::collections::HashMap<&str, usize> = parts
            .iter()
            .enumerate()
            .map(|(i, q)| (q.refdes.as_str(), i))
            .collect();
        let mut nets = Vec::new();
        let mut nets_of: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut neighbours: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
        for net in p.nets {
            let mut pads = Vec::new();
            for pad in &net.pads {
                let Some(&i) = index.get(pad.refdes.as_str()) else {
                    continue;
                };
                for (k, q) in parts[i].footprint.pads.iter().enumerate() {
                    if q.number == pad.number {
                        pads.push((i, k));
                    }
                }
            }
            if pads.len() < 2 {
                continue;
            }
            let plane = p.planes.contains(&net.name);
            let k = pads.len() as f64;
            let mut weight = (1.0 / (1.0 + 0.04 * (k - 2.0))).max(0.35);
            if plane {
                weight *= 0.15;
            }
            let id = nets.len();
            for &(i, _) in &pads {
                if !nets_of[i].contains(&id) {
                    nets_of[i].push(id);
                }
            }
            if !plane && pads.len() <= 8 {
                for &(a, _) in &pads {
                    for &(b, pb) in &pads {
                        if a != b {
                            neighbours[a].push((b, pb));
                        }
                    }
                }
            }
            let lines = !plane && pads.len() <= MAX_LINE_PADS;
            nets.push(NetModel {
                pads,
                weight,
                lines,
            });
        }

        // Decoupling pairs, with their pads.
        let pad_ids = |part: usize, number: &str| -> Vec<usize> {
            parts[part]
                .footprint
                .pads
                .iter()
                .enumerate()
                .filter(|(_, q)| q.number == number)
                .map(|(k, _)| k)
                .collect()
        };
        let decs: Vec<Dec> = decoupling_pairs(parts, p.nets)
            .iter()
            .map(|pair: &DecouplingPair| {
                let side = |s: &crate::quality::PairNet| {
                    (
                        pad_ids(pair.cap, &s.cap_pad),
                        s.pins
                            .iter()
                            .flat_map(|(i, number)| {
                                pad_ids(*i, number).into_iter().map(move |k| (*i, k))
                            })
                            .collect::<Vec<_>>(),
                    )
                };
                Dec {
                    cap: pair.cap,
                    nets: [side(&pair.nets[0]), side(&pair.nets[1])],
                }
            })
            .collect();
        let mut decs_of: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (d, dec) in decs.iter().enumerate() {
            let mut involved = vec![dec.cap];
            involved.extend(
                dec.nets
                    .iter()
                    .flat_map(|(_, pins)| pins.iter().map(|&(i, _)| i)),
            );
            involved.sort_unstable();
            involved.dedup();
            for i in involved {
                decs_of[i].push(d);
            }
        }
        // Neighbours of a decoupled capacitor: the pins it belongs to.
        for dec in &decs {
            for (_, pins) in &dec.nets {
                for &(i, k) in pins {
                    neighbours[dec.cap].push((i, k));
                }
            }
        }

        // Module instances: each prefix of an instance path shared by two or more parts.
        let mut by_prefix: std::collections::BTreeMap<String, Vec<usize>> = Default::default();
        for (i, q) in parts.iter().enumerate() {
            let Some((parent, _)) = q.path.rsplit_once('.') else {
                continue;
            };
            let mut prefix = String::new();
            for segment in parent.split('.') {
                if !prefix.is_empty() {
                    prefix.push('.');
                }
                prefix.push_str(segment);
                by_prefix.entry(prefix.clone()).or_default().push(i);
            }
        }
        let groups: Vec<Vec<usize>> = by_prefix
            .into_values()
            .filter(|g| g.len() >= 2 && g.len() < n)
            .collect();
        let mut groups_of: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (g, members) in groups.iter().enumerate() {
            for &i in members {
                groups_of[i].push(g);
            }
        }
        let mut near_ok = vec![false; n * n];
        for dec in &decs {
            for (_, pins) in &dec.nets {
                for &(i, _) in pins {
                    near_ok[dec.cap * n + i] = true;
                    near_ok[i * n + dec.cap] = true;
                }
            }
        }
        let in_group: Vec<Vec<bool>> = groups
            .iter()
            .map(|members| (0..n).map(|i| members.contains(&i)).collect())
            .collect();
        let free: Vec<usize> = (0..n).filter(|&i| !p.fixed[i]).collect();

        // The relations between two parts: a relation whose part is not on the board has
        // nothing to keep.
        let mut links: Vec<Link> = Vec::new();
        for o in p.offsets {
            let (Some(&a), Some(&b)) = (index.get(o.a.as_str()), index.get(o.b.as_str())) else {
                continue;
            };
            let Some(axis) = axis_of(o.fa) else { continue };
            links.push(Link {
                a,
                b,
                fa: o.fa,
                fb: o.fb,
                axis,
                lo: o.lo,
                hi: o.hi,
                label: o.label.clone(),
            });
        }
        let mut links_of: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (k, l) in links.iter().enumerate() {
            links_of[l.a].push(k);
            links_of[l.b].push(k);
        }
        // Which part follows which, per axis (a forest: a part has one leader, and the
        // relations that would close a loop only cost). The parts that stay, and the ones the
        // requirements pin along the axis, never follow.
        let pinned = |i: usize, axis: usize| {
            models[i].want.as_ref().is_some_and(|w| {
                w.turns.iter().flatten().any(|t| {
                    if axis == 0 {
                        t.pin_x.is_some()
                    } else {
                        t.pin_y.is_some()
                    }
                })
            })
        };
        let mut leader_of: Vec<[Option<usize>; 2]> = vec![[None; 2]; n];
        let mut root: [Vec<usize>; 2] = [(0..n).collect(), (0..n).collect()];
        fn find(root: &mut [usize], mut i: usize) -> usize {
            while root[i] != i {
                root[i] = root[root[i]];
                i = root[i];
            }
            i
        }
        for (k, l) in links.iter().enumerate() {
            if !l.needs_alignment() {
                continue;
            }
            let ax = l.axis;
            let (ra, rb) = (find(&mut root[ax], l.a), find(&mut root[ax], l.b));
            if ra == rb {
                continue;
            }
            let can = |i: usize| !p.fixed[i] && !pinned(i, ax) && leader_of[i][ax].is_none();
            let child = if can(l.b) {
                l.b
            } else if can(l.a) {
                l.a
            } else {
                continue;
            };
            leader_of[child][ax] = Some(k);
            let (rc, ro) = if child == l.b { (rb, ra) } else { (ra, rb) };
            root[ax][rc] = ro;
        }
        let mut order: Vec<(usize, usize, usize)> = Vec::new();
        for i in 0..n {
            for ax in 0..2 {
                if leader_of[i][ax].is_none() {
                    continue;
                }
                let mut depth = 0;
                let mut at = i;
                while let Some(k) = leader_of[at][ax] {
                    depth += 1;
                    at = if links[k].a == at {
                        links[k].b
                    } else {
                        links[k].a
                    };
                }
                order.push((depth, i, ax));
            }
        }
        order.sort_unstable();
        let order: Vec<(usize, usize)> = order.into_iter().map(|(_, i, ax)| (i, ax)).collect();
        // A part that stays is where it is, exactly, though its pose is on the grid.
        let mut sub = vec![Pt::default(); n];
        for (i, q) in parts.iter().enumerate() {
            if p.fixed[i] && !links_of[i].is_empty() {
                sub[i] = Pt::new(
                    q.at.x - poses[i].x as f64 * UNIT,
                    q.at.y - poses[i].y as f64 * UNIT,
                );
            }
        }
        let link_mark = vec![0; links.len()];
        let (nn, nd, ng) = (nets.len(), decs.len(), groups.len());
        let mut engine = Engine {
            p,
            models,
            poses,
            nets,
            nets_of,
            decs,
            decs_of,
            groups,
            groups_of,
            in_group,
            neighbours,
            free,
            links,
            links_of,
            link_mark,
            leader_of,
            order,
            sub,
            lines: vec![Vec::new(); nn],
            line_box: vec![None; nn],
            net_mark: vec![0; nn],
            dec_mark: vec![0; nd],
            group_mark: vec![0; ng],
            part_mark: vec![0; n],
            stamp: 0,
            gap,
            hole_gap,
            near_ok,
            reach_abs: vec![Rect::new(Pt::default(), Pt::default()); n],
            text_weight: TEXT_WEIGHT_FROM,
            text_last: false,
        };
        for i in 0..n {
            let pose = engine.clamp(i, engine.poses[i]);
            engine.put(i, pose);
        }
        // Parts that stand in a relation begin where it holds, as far as it can be made to.
        if !engine.links.is_empty() {
            engine.realign();
            for i in 0..n {
                let change = engine.expand(vec![(i, engine.poses[i])]);
                for (j, pose) in change {
                    engine.put(j, pose);
                }
            }
        }
        engine.refresh_all();
        engine
    }

    /// Move a part to a pose.
    fn put(&mut self, i: usize, pose: Pose) {
        let turned = self.poses[i].turn != pose.turn;
        self.poses[i] = pose;
        // A turn changes where the body is, so the parts that follow it (or that it follows)
        // are moved off the grid by another amount.
        if turned && !self.links_of[i].is_empty() {
            self.realign();
        }
        self.reach_abs[i] = self.shape(i).reach.translated(self.pos(i));
    }

    fn put_all(&mut self, poses: Vec<Pose>) {
        for (i, pose) in poses.into_iter().enumerate() {
            self.put(i, pose);
        }
    }

    fn pos(&self, i: usize) -> Pt {
        let pose = self.poses[i];
        let sub = self.sub[i];
        let mut at = Pt::new(pose.x as f64 * UNIT + sub.x, pose.y as f64 * UNIT + sub.y);
        // An axis the requirements pin is at its exact value, not at the grid's nearest.
        if let Some(turn) = self.limits(i, pose.turn) {
            at.x = turn.pin_x.unwrap_or(at.x);
            at.y = turn.pin_y.unwrap_or(at.y);
        }
        at
    }

    /// Where along its axis the measure `field` of part `i`'s body is, on the sheet.
    fn measure_of(&self, i: usize, field: PlaceField, axis: usize) -> f64 {
        let at = self.pos(i);
        (if axis == 0 { at.x } else { at.y }) + offset(field, self.shape(i).body)
    }

    /// The measure of `b` minus that of `a` for relation `k`.
    fn link_value(&self, k: usize) -> f64 {
        let l = &self.links[k];
        self.measure_of(l.b, l.fb, l.axis) - self.measure_of(l.a, l.fa, l.axis)
    }

    /// How far relation `k` is from its window, in mm; 0 when it holds.
    fn link_miss(&self, k: usize) -> f64 {
        self.links[k].miss(self.link_value(k))
    }

    /// The cost of a relation that does not hold: a hard rule, as a wall is.
    fn link_cost(&self, k: usize) -> f64 {
        let miss = self.link_miss(k);
        if miss > LINK_EPS {
            W_HARD * (miss + 0.05)
        } else {
            0.0
        }
    }

    /// Put every part that follows another on the grid shifted by what makes the relation
    /// hold exactly at the middle of its window, given where its leader is and how each is
    /// turned. A part with no leader is on the grid.
    fn realign(&mut self) {
        for k in 0..self.order.len() {
            let (f, ax) = self.order[k];
            let Some(link) = self.leader_of[f][ax] else {
                continue;
            };
            let l = &self.links[link];
            let t = (l.lo + l.hi) / 2.0;
            let (leader, wanted) = if l.b == f {
                // b = a + t, in the measures: origin_b + off_b = origin_a + off_a + t.
                let a = l.a;
                let at = self.measure_of(a, l.fa, ax);
                (a, at + t - offset(l.fb, self.shape(f).body))
            } else {
                let b = l.b;
                let at = self.measure_of(b, l.fb, ax);
                (b, at - t - offset(l.fa, self.shape(f).body))
            };
            debug_assert_ne!(leader, f);
            let grid = if ax == 0 {
                self.poses[f].x
            } else {
                self.poses[f].y
            } as f64
                * UNIT;
            let off = wanted - grid;
            let shift = off - UNIT * (off / UNIT).round();
            if ax == 0 {
                self.sub[f].x = shift;
            } else {
                self.sub[f].y = shift;
            }
            self.reach_abs[f] = self.shape(f).reach.translated(self.pos(f));
        }
    }

    /// Part `j` moved along the axis of relation `k` to where the relation holds, as near as
    /// it is now. `None` if `j` stays or the requirements pin it along that axis.
    fn repair_pose(&self, k: usize, j: usize) -> Option<Pose> {
        let l = &self.links[k];
        let ax = l.axis;
        let now = self.poses[j];
        if self.models[j].fixed
            || self.limits(j, now.turn).is_some_and(|t| {
                if ax == 0 {
                    t.pin_x.is_some()
                } else {
                    t.pin_y.is_some()
                }
            })
        {
            return None;
        }
        let (lo, hi) = (l.lo, l.hi);
        if !lo.is_finite() && !hi.is_finite() {
            return None;
        }
        // Inside the window by the half of a grid step, so that rounding to the grid stays in.
        let margin = ((hi - lo) / 2.0).min(UNIT / 2.0 + 1e-9);
        let t = self.link_value(k).clamp(lo + margin, hi - margin);
        let (other, f_other, f_j, sign) = if l.b == j {
            (l.a, l.fa, l.fb, 1.0)
        } else {
            (l.b, l.fb, l.fa, -1.0)
        };
        let target = self.measure_of(other, f_other, ax) + sign * t;
        let origin = target - offset(f_j, self.shape(j).body);
        let sub = if ax == 0 {
            self.sub[j].x
        } else {
            self.sub[j].y
        };
        let g = ((origin - sub) / UNIT).round() as i64;
        let mut pose = now;
        if ax == 0 {
            pose.x = g;
        } else {
            pose.y = g;
        }
        Some(pose)
    }

    /// `change` with every part that must move with it to keep the relations it is in: a
    /// part whose relation to a moved part no longer holds is moved to where it does, and so
    /// on from it. The parts are left as they were.
    fn expand(&mut self, change: Vec<(usize, Pose)>) -> Vec<(usize, Pose)> {
        if self.links.is_empty() || change.iter().all(|&(i, _)| self.links_of[i].is_empty()) {
            return change;
        }
        let mut undo: Vec<(usize, Pose)> =
            change.iter().map(|&(i, _)| (i, self.poses[i])).collect();
        let mut result = change.clone();
        for &(i, pose) in &change {
            self.put(i, pose);
        }
        let mut at = 0;
        while at < result.len() {
            let i = result[at].0;
            at += 1;
            for n in 0..self.links_of[i].len() {
                let k = self.links_of[i][n];
                if self.link_miss(k) <= LINK_EPS {
                    continue;
                }
                let l = &self.links[k];
                let j = if l.a == i { l.b } else { l.a };
                if result.iter().any(|&(m, _)| m == j) {
                    continue;
                }
                let Some(pose) = self.repair_pose(k, j) else {
                    continue;
                };
                let pose = self.clamp(j, pose);
                undo.push((j, self.poses[j]));
                self.put(j, pose);
                result.push((j, pose));
            }
        }
        for &(i, pose) in undo.iter().rev() {
            self.put(i, pose);
        }
        result
    }

    /// What the requirements allow part `i` in turn `turn`, if it has any.
    fn limits(&self, i: usize, turn: u8) -> Option<Turn> {
        self.models[i].want.as_ref()?.turns[usize::from(turn)]
    }

    /// The pose nearest `pose` that meets the design's requirements on the part: the next
    /// turn they allow if this one is ruled out, and the position pushed into the window.
    fn clamp(&self, i: usize, pose: Pose) -> Pose {
        let Some(want) = &self.models[i].want else {
            return pose;
        };
        let turn = (0..4)
            .map(|k| (pose.turn + k) % 4)
            .find(|&t| want.turns[usize::from(t)].is_some())
            .unwrap_or(pose.turn);
        match want.turns[usize::from(turn)] {
            Some(t) => Pose {
                x: pose.x.clamp(t.x.0, t.x.1),
                y: pose.y.clamp(t.y.0, t.y.1),
                turn,
                text: pose.text,
            },
            None => pose,
        }
    }

    fn shape(&self, i: usize) -> &Shape {
        let m = &self.models[i];
        let pose = self.poses[i];
        &m.shapes[variant(pose.turn, pose.text).min(m.shapes.len() - 1)]
    }

    /// The boxes part `i` takes on `face`, as `Placed::room_on` finds them, and whether they
    /// are its pads.
    fn rooms_on(&self, i: usize, face: Side) -> Vec<(Rect, bool)> {
        let m = &self.models[i];
        if !m.hole {
            return if face == m.side {
                vec![(self.room(i), false)]
            } else {
                Vec::new()
            };
        }
        let (own, opposite) = m.court_faces;
        let (here, there) = if face == m.side {
            (own, opposite)
        } else {
            (opposite, own)
        };
        if here || !there {
            return vec![(self.room(i), false)];
        }
        let pos = self.pos(i);
        self.shape(i)
            .rooms
            .iter()
            .map(|r| (r.translated(pos), true))
            .collect()
    }

    /// The pairs of boxes in which parts `i` and `j` must not overlap, one for each face
    /// they both take room on, and whether either box is a part's pads.
    fn room_pairs(&self, i: usize, j: usize) -> Vec<(Rect, Rect, bool)> {
        let mut pairs: Vec<(Rect, Rect, bool)> = Vec::new();
        for face in [Side::Front, Side::Back] {
            let (ra, rb) = (self.rooms_on(i, face), self.rooms_on(j, face));
            for &(x, px) in &ra {
                for &(y, py) in &rb {
                    let pair = (x, y, px || py);
                    if !pairs.contains(&pair) {
                        pairs.push(pair);
                    }
                }
            }
        }
        pairs
    }

    fn court(&self, i: usize) -> Rect {
        self.shape(i).court.translated(self.pos(i))
    }

    /// The room part `i` takes against other courtyards: its courtyard, clipped to the
    /// outline if the design says it may hang over an edge (as `Board::check_placement`
    /// counts it, so that two parts that hang over the same edge may overlap beyond it).
    fn room(&self, i: usize) -> Rect {
        let court = self.court(i);
        if self.models[i].may.any() {
            crate::overhang::clip(&court, &self.p.outline)
        } else {
            court
        }
    }

    fn pad_at(&self, part: usize, pad: usize) -> Pt {
        self.pos(part) + self.shape(part).pads[pad]
    }

    fn net_cost(&self, n: usize) -> f64 {
        let net = &self.nets[n];
        let (mut lo, mut hi) = (Pt::new(f64::MAX, f64::MAX), Pt::new(f64::MIN, f64::MIN));
        for &(i, k) in &net.pads {
            let at = self.pad_at(i, k);
            lo = Pt::new(lo.x.min(at.x), lo.y.min(at.y));
            hi = Pt::new(hi.x.max(at.x), hi.y.max(at.y));
        }
        W_WIRE * net.weight * ((hi.x - lo.x) + (hi.y - lo.y))
    }

    /// The ratsnest lines of a net as the parts are now: the shortest tree over its pad
    /// centres (Prim's algorithm, on squared distances so that nothing is rounded).
    fn net_lines(&self, n: usize) -> Vec<(Pt, Pt)> {
        let pts: Vec<Pt> = self.nets[n]
            .pads
            .iter()
            .map(|&(i, k)| self.pad_at(i, k))
            .collect();
        let mut lines = Vec::with_capacity(pts.len().saturating_sub(1));
        let mut reach: Vec<(f64, usize)> = pts[1.min(pts.len())..]
            .iter()
            .map(|p| (dist2(*p, pts[0]), 0))
            .collect();
        let mut left: Vec<usize> = (1..pts.len()).collect();
        while !left.is_empty() {
            let mut best = 0;
            for k in 1..left.len() {
                if reach[k].0 < reach[best].0 {
                    best = k;
                }
            }
            let (_, from) = reach.swap_remove(best);
            let added = left.swap_remove(best);
            lines.push((pts[from], pts[added]));
            for (k, &other) in left.iter().enumerate() {
                let d = dist2(pts[other], pts[added]);
                if d < reach[k].0 {
                    reach[k] = (d, added);
                }
            }
        }
        lines
    }

    /// Bring the lines kept for a net up to date.
    fn refresh_net(&mut self, n: usize) {
        if !self.nets[n].lines {
            return;
        }
        let lines = self.net_lines(n);
        self.line_box[n] = Rect::around(lines.iter().flat_map(|&(a, b)| [a, b]));
        self.lines[n] = lines;
    }

    fn refresh(&mut self, set: &[usize]) {
        for &i in set {
            for k in 0..self.nets_of[i].len() {
                let n = self.nets_of[i][k];
                self.refresh_net(n);
            }
        }
    }

    fn refresh_all(&mut self) {
        for n in 0..self.nets.len() {
            self.refresh_net(n);
        }
    }

    /// How many lines of the nets in `touched` (as the parts are now) cross a line of another
    /// net, the lines of the nets not in `touched` being those kept. A net's lines are
    /// counted against each other net's once.
    fn crossings(&self, touched: &[usize], stamp: u32) -> usize {
        let news: Vec<(Lines, Option<Rect>)> = touched
            .iter()
            .map(|&n| {
                let lines = self.net_lines(n);
                let b = Rect::around(lines.iter().flat_map(|&(a, b)| [a, b]));
                (lines, b)
            })
            .collect();
        let mut count = 0;
        let across = |a: &[(Pt, Pt)], b: &[(Pt, Pt)]| {
            let mut n = 0;
            for &(p, q) in a {
                for &(r, s) in b {
                    if crate::quality::segments_cross(p, q, r, s) {
                        n += 1;
                    }
                }
            }
            n
        };
        for (x, (lines, bx)) in news.iter().enumerate() {
            let Some(bx) = bx else { continue };
            for u in 0..self.nets.len() {
                if !self.nets[u].lines || self.net_mark[u] == stamp {
                    continue;
                }
                if self.line_box[u].is_some_and(|bu| bu.overlaps(bx)) {
                    count += across(lines, &self.lines[u]);
                }
            }
            for (other, by) in &news[x + 1..] {
                if by.is_some_and(|by| by.overlaps(bx)) {
                    count += across(lines, other);
                }
            }
        }
        count
    }

    fn dec_cost(&self, d: usize) -> f64 {
        let dec = &self.decs[d];
        let mut total = 0.0;
        for (cap_pads, pins) in &dec.nets {
            let mut nearest = f64::MAX;
            for &c in cap_pads {
                let at = self.pad_at(dec.cap, c);
                for &(i, k) in pins {
                    nearest = nearest.min(dist(at, self.pad_at(i, k)));
                }
            }
            if nearest < f64::MAX {
                total += nearest;
            }
        }
        W_DECOUPLING * total
    }

    /// The box round the centres of a module instance's parts.
    fn group_box(&self, g: usize) -> Rect {
        let (mut lo, mut hi) = (Pt::new(f64::MAX, f64::MAX), Pt::new(f64::MIN, f64::MIN));
        for &i in &self.groups[g] {
            let c = self.court(i).centre();
            lo = Pt::new(lo.x.min(c.x), lo.y.min(c.y));
            hi = Pt::new(hi.x.max(c.x), hi.y.max(c.y));
        }
        Rect::new(lo, hi)
    }

    /// What a module instance costs: the half perimeter of its box, and each part of another
    /// instance whose centre is inside the box.
    fn group_cost(&self, g: usize) -> f64 {
        let b = self.group_box(g);
        let mut cost = W_COHESION * (b.width() + b.height());
        for j in 0..self.models.len() {
            if !self.in_group[g][j] {
                cost += self.intrusion(&b, j);
            }
        }
        cost
    }

    /// What part `j` costs for being inside the box of a module instance it is not part of.
    fn intrusion(&self, b: &Rect, j: usize) -> f64 {
        let c = self.court(j).centre();
        let depth = (c.x - b.min.x)
            .min(b.max.x - c.x)
            .min(c.y - b.min.y)
            .min(b.max.y - c.y);
        if depth > 0.0 {
            W_INTRUDER * (depth + 1.0)
        } else {
            0.0
        }
    }

    /// How far inside the outline a point is, negative outside it.
    fn depth(&self, p: Pt) -> f64 {
        let o = &self.p.region;
        let straight = (p.x - o.min.x)
            .min(p.y - o.min.y)
            .min(o.max.x - p.x)
            .min(o.max.y - p.y);
        let r = self.p.corner_radius.min(o.width().min(o.height()) / 2.0);
        if r <= 0.0 {
            return straight;
        }
        let centre = Pt::new(
            if p.x < o.min.x + r {
                o.min.x + r
            } else if p.x > o.max.x - r {
                o.max.x - r
            } else {
                return straight;
            },
            if p.y < o.min.y + r {
                o.min.y + r
            } else if p.y > o.max.y - r {
                o.max.y - r
            } else {
                return straight;
            },
        );
        straight.min(r - dist(p, centre))
    }

    /// [`Engine::depth`], not counting the sides along an axis the design decides.
    fn depth_without(&self, p: Pt, skip_x: bool, skip_y: bool) -> f64 {
        if !skip_x && !skip_y {
            return self.depth(p);
        }
        let o = &self.p.region;
        let mut depth = f64::INFINITY;
        if !skip_x {
            depth = depth.min((p.x - o.min.x).min(o.max.x - p.x));
        }
        if !skip_y {
            depth = depth.min((p.y - o.min.y).min(o.max.y - p.y));
        }
        depth
    }

    /// What a part costs on its own: walls, keepouts, its pull to an edge, its turn.
    fn single(&self, i: usize) -> Cost {
        let mut c = Cost::default();
        let court = self.court(i);
        let model = &self.models[i];
        // The design says where a part goes along an axis it holds: the walls are not the
        // placer's to enforce there.
        let (skip_x, skip_y) = model
            .want
            .as_ref()
            .map_or((false, false), |w| (w.holds_x, w.holds_y));
        let wall = self.p.rules.edge_wall();
        for corner in court.corners() {
            c.hard += W_HARD * (wall - self.depth_without(corner, skip_x, skip_y)).max(0.0);
        }
        if let Some(t) = self.shape(i).text {
            for corner in t.translated(self.pos(i)).corners() {
                c.hard +=
                    W_HARD * (TEXT_EDGE - self.depth_without(corner, skip_x, skip_y)).max(0.0);
            }
        }
        // A pad keeps the copper to edge space from the outline, as the placement check asks.
        // The courtyard's wall does that unless a pad reaches past the courtyard or the wall
        // is thin; a part that hangs over the edge has no wall along the axes the design
        // holds, but its copper stays inside the outline and the process's space from its edge.
        let kept = self.p.rules.copper_to_edge + PAD_SLACK;
        if model.overhangs || wall + self.shape(i).pad_inset < kept {
            let pos = self.pos(i);
            for pad in &self.shape(i).pad_boxes {
                for corner in pad.translated(pos).corners() {
                    c.hard += W_HARD * (kept - self.depth(corner)).max(0.0);
                }
            }
        }
        // A pad keeps the process's distance from the edge of a cutout, as `check_placement`
        // asks: the courtyard may be smaller than the copper, or not drawn.
        if !self.p.cutouts.is_empty() {
            let pos = self.pos(i);
            for pad in &self.shape(i).pad_boxes {
                let pad = pad.translated(pos);
                for cut in self.p.cutouts {
                    let near = rect_distance(&pad, cut);
                    c.hard += W_HARD * (self.p.rules.copper_to_edge + PAD_SLACK - near).max(0.0);
                }
            }
        }
        for k in self.p.keepouts.iter().chain(&model.no_go) {
            let depth = penetration(&court, k, KEEPOUT_GAP);
            if depth > 0.0 {
                c.soft += W_GAP * depth;
                if penetration(&court, k, 0.0) > 1e-6 {
                    c.hard += W_HARD * (penetration(&court, k, 0.0) + 0.05);
                }
            }
        }
        let r = &self.p.region;
        let to_x = (court.min.x - r.min.x).min(r.max.x - court.max.x) - self.p.rules.edge_wall();
        let to_y = (court.min.y - r.min.y).min(r.max.y - court.max.y) - self.p.rules.edge_wall();
        match model.role {
            Role::Connector => c.soft += W_EDGE * to_x.min(to_y).max(0.0),
            Role::Mechanical => c.soft += W_EDGE * (to_x.max(0.0) + to_y.max(0.0)),
            Role::Plain => {}
        }
        if self.poses[i].turn != 0
            && (model.angles[0] + 90.0 * f64::from(self.poses[i].turn)) % 360.0 != 0.0
        {
            c.soft += W_TURN;
        }
        if self.poses[i].text != 0 && !model.fixed {
            c.soft += W_TURN;
        }
        c
    }

    /// What two parts cost together: courtyards, reference text and silkscreen.
    fn pair(&self, i: usize, j: usize) -> Cost {
        let mut c = Cost::default();
        if !self.reach_abs[i].overlaps(&self.reach_abs[j]) {
            return c;
        }
        let (a, b) = (&self.models[i], &self.models[j]);
        let (sa, sb) = (self.shape(i), self.shape(j));
        let (pa, pb) = (self.pos(i), self.pos(j));
        // Parts on opposite faces share area unless one has a hole through the board.
        if a.side != b.side && !a.hole && !b.hole {
            return c;
        }
        let (ca, cb) = (self.room(i), self.room(j));
        let escape = if self.near_ok[i * self.models.len() + j] {
            0.0
        } else {
            a.escape.max(b.escape)
        };
        let gap = if a.hole || b.hole {
            self.hole_gap
        } else {
            self.gap
        }
        .max(self.gap + escape);
        if !a.hole && !b.hole {
            c += Self::court_cost(&ca, &cb, gap);
        } else {
            // A part with a hole takes room on each face by `Placed::room_on`: its courtyard,
            // or on a face its footprint draws no courtyard for, its pads. Pads are only kept
            // apart, not spaced.
            for (x, y, pads) in self.room_pairs(i, j) {
                c += Self::court_cost(&x, &y, if pads { 0.0 } else { gap });
            }
        }
        c += self.silk_cost(sa, pa, sb, pb, &cb);
        c += self.silk_cost(sb, pb, sa, pa, &ca);
        c
    }

    /// What two boxes cost: soft within `gap` of each other, hard when they overlap.
    fn court_cost(a: &Rect, b: &Rect, gap: f64) -> Cost {
        let mut c = Cost::default();
        let near = penetration(a, b, gap);
        if near > 0.0 {
            c.soft += W_GAP * near;
            let over = penetration(a, b, 0.0);
            if over > 1e-6 {
                c.hard += W_HARD * (over + 0.05);
            }
        }
        c
    }

    /// What the reference text and silk strokes of one part cost against another part: the
    /// text over its silkscreen, text, pads and mask openings, and the strokes over its
    /// pads and mask openings, all of which KiCad's check refuses; and, a little, text over
    /// the part's courtyard.
    fn silk_cost(&self, from: &Shape, at_from: Pt, to: &Shape, at_to: Pt, court: &Rect) -> Cost {
        let to_pad = self.p.rules.silk_to_pad;
        let mut soft = 0.0;
        let mut refuse = |depth: f64| {
            if depth > 0.0 {
                soft += self.text_weight * (depth + 0.05);
            }
        };
        let mut over_court = 0.0;
        if let Some(t) = from.text {
            let t = t.translated(at_from);
            for r in &to.silk {
                refuse(penetration(&t, &r.translated(at_to), SILK_CLEAR));
            }
            for r in &to.copper {
                refuse(penetration(
                    &t,
                    &r.translated(at_to),
                    to_pad.max(SILK_CLEAR),
                ));
            }
            // Two reference texts apart, counted once (by the lower of the two parts).
            if let Some(other) = to
                .text
                .filter(|_| (at_from.x, at_from.y) < (at_to.x, at_to.y))
            {
                refuse(penetration(&t, &other.translated(at_to), SILK_CLEAR));
            }
            let over = penetration(&t, court, 0.0);
            if over > 0.0 {
                over_court = W_TEXT * (over + 0.05);
            }
        }
        for stroke in &from.silk {
            let stroke = stroke.translated(at_from);
            for r in &to.copper {
                refuse(penetration(&stroke, &r.translated(at_to), to_pad));
            }
        }
        Cost {
            soft: soft + over_court,
            hard: 0.0,
        }
    }

    /// The box round every courtyard, in half perimeter.
    fn area_cost(&self) -> f64 {
        let mut lo = Pt::new(f64::MAX, f64::MAX);
        let mut hi = Pt::new(f64::MIN, f64::MIN);
        for i in 0..self.models.len() {
            let c = self.court(i);
            lo = Pt::new(lo.x.min(c.min.x), lo.y.min(c.min.y));
            hi = Pt::new(hi.x.max(c.max.x), hi.y.max(c.max.y));
        }
        if lo.x > hi.x {
            return 0.0;
        }
        W_AREA * ((hi.x - lo.x) + (hi.y - lo.y))
    }

    /// The cost of everything that involves a part of `set`, counting each net, pair of
    /// parts, module and decoupling pair once. The difference between this before and after
    /// moving the parts of `set` is the change in the whole cost.
    fn eval(&mut self, set: &[usize]) -> Cost {
        self.stamp += 1;
        let stamp = self.stamp;
        for &i in set {
            self.part_mark[i] = stamp;
        }
        let mut c = Cost::default();
        let mut touched: Vec<usize> = Vec::new();
        for &i in set {
            for k in 0..self.nets_of[i].len() {
                let n = self.nets_of[i][k];
                if self.net_mark[n] != stamp {
                    self.net_mark[n] = stamp;
                    c.soft += self.net_cost(n);
                    if self.nets[n].lines {
                        touched.push(n);
                    }
                }
            }
            for k in 0..self.decs_of[i].len() {
                let d = self.decs_of[i][k];
                if self.dec_mark[d] != stamp {
                    self.dec_mark[d] = stamp;
                    c.soft += self.dec_cost(d);
                }
            }
            for k in 0..self.groups_of[i].len() {
                let g = self.groups_of[i][k];
                if self.group_mark[g] != stamp {
                    self.group_mark[g] = stamp;
                    c.soft += self.group_cost(g);
                }
            }
            if !self.models[i].fixed {
                c += self.single(i);
            }
            for j in 0..self.models.len() {
                if j == i || (self.part_mark[j] == stamp && j < i) {
                    continue;
                }
                if self.models[i].fixed && self.models[j].fixed {
                    continue;
                }
                c += self.pair(i, j);
            }
        }
        // The parts of the set inside the box of a module instance they are not part of
        // (those of an instance that has a part in the set were counted with it).
        for &i in set {
            for g in 0..self.groups.len() {
                if self.group_mark[g] != stamp && !self.in_group[g][i] {
                    c.soft += self.intrusion(&self.group_box(g), i);
                }
            }
        }
        for &i in set {
            for n in 0..self.links_of[i].len() {
                let k = self.links_of[i][n];
                if self.link_mark[k] != stamp {
                    self.link_mark[k] = stamp;
                    c.hard += self.link_cost(k);
                }
            }
        }
        c.soft += W_CROSSING * self.crossings(&touched, stamp) as f64;
        c.soft += self.area_cost();
        c
    }

    /// Which hard rules the parts break as they are now, counted for the message of a run
    /// that found no arrangement that breaks none.
    fn ended(&self, names: &[String]) -> Ended {
        let mut ended = Ended::default();
        for i in 0..self.models.len() {
            if !self.models[i].fixed && self.single(i).hard > 1e-9 {
                ended.outside += 1;
                ended.outside_parts.push(self.outside_at(i, &names[i]));
            }
            for j in i + 1..self.models.len() {
                if self.models[i].fixed && self.models[j].fixed {
                    continue;
                }
                let (a, b) = (&self.models[i], &self.models[j]);
                if a.side != b.side && !a.hole && !b.hole {
                    continue;
                }
                let over = self
                    .room_pairs(i, j)
                    .into_iter()
                    .find(|(x, y, _)| penetration(x, y, 0.0) > 1e-6);
                if let Some((ca, cb, _)) = over {
                    ended.overlaps += 1;
                    let size = (
                        (ca.max.x.min(cb.max.x) - ca.min.x.max(cb.min.x)).max(0.0),
                        (ca.max.y.min(cb.max.y) - ca.min.y.max(cb.min.y)).max(0.0),
                    );
                    let centre = Pt::new(
                        (ca.max.x.min(cb.max.x) + ca.min.x.max(cb.min.x)) / 2.0,
                        (ca.max.y.min(cb.max.y) + ca.min.y.max(cb.min.y)) / 2.0,
                    );
                    let region = &self.p.region;
                    ended.overlapping.push(OverlapAt {
                        a: names[i].clone(),
                        b: names[j].clone(),
                        area: size.0 * size.1,
                        size,
                        at: (centre.x - region.min.x, centre.y - region.min.y),
                        nearest: nearest_edge(region, centre),
                    });
                } else if self.pair(i, j).hard > 1e-9 {
                    ended.silk += 1;
                }
            }
        }
        for (k, l) in self.links.iter().enumerate() {
            let miss = self.link_miss(k);
            if miss > LINK_EPS {
                ended.relations.push(format!(
                    "{}: {} minus {} is {} mm along {}, and it allows {} ({} mm over)",
                    l.label,
                    names[l.b],
                    names[l.a],
                    mm(self.link_value(k)),
                    if l.axis == 0 { "x" } else { "y" },
                    window_text(l.lo, l.hi),
                    mm(miss)
                ));
            }
        }
        ended
            .overlapping
            .sort_by(|a, b| b.area.total_cmp(&a.area).then_with(|| a.a.cmp(&b.a)));
        ended.outside_parts.sort_by(|a, b| {
            b.worst
                .total_cmp(&a.worst)
                .then_with(|| a.part.cmp(&b.part))
        });
        ended
    }

    /// In what way part `i` is outside, inside the margin or in a keepout.
    fn outside_at(&self, i: usize, name: &str) -> OutsideAt {
        let (r, court) = (&self.p.region, self.court(i));
        let wall = self.p.rules.edge_wall();
        let (skip_x, skip_y) = self.models[i]
            .want
            .as_ref()
            .map_or((false, false), |w| (w.holds_x, w.holds_y));
        let sides = [
            ("left", r.min.x - court.min.x, skip_x),
            ("right", court.max.x - r.max.x, skip_x),
            ("top", r.min.y - court.min.y, skip_y),
            ("bottom", court.max.y - r.max.y, skip_y),
        ];
        let (mut why, mut worst) = (Vec::new(), 0.0f64);
        for (side, past, skip) in sides {
            if skip {
                continue;
            }
            if past > 1e-6 {
                why.push(format!("{past:.2} mm outside the {side} edge"));
                worst = worst.max(past + wall);
            } else if past + wall > 1e-6 {
                why.push(format!(
                    "{:.2} mm inside the {} mm margin at the {side} edge",
                    past + wall,
                    (wall * 100.0).round() / 100.0
                ));
                worst = worst.max(past + wall);
            }
        }
        for k in self.p.keepouts.iter().chain(&self.models[i].no_go) {
            let depth = penetration(&court, k, 0.0);
            if depth > 1e-6 {
                let at = k.centre();
                why.push(format!(
                    "{depth:.2} mm into a keepout, cutout or height limit at ({:.1}, {:.1})",
                    at.x - r.min.x,
                    at.y - r.min.y
                ));
                worst = worst.max(depth);
            }
        }
        let pos = self.pos(i);
        let wanted = self.p.rules.copper_to_edge;
        if self.models[i].overhangs {
            let nearest = self
                .shape(i)
                .pad_boxes
                .iter()
                .flat_map(|pad| pad.translated(pos).corners())
                .map(|corner| (self.depth(corner), corner))
                .fold(
                    (f64::MAX, Pt::new(0.0, 0.0)),
                    |a, b| if b.0 < a.0 { b } else { a },
                );
            if nearest.0 < wanted - 1e-6 {
                let (side, _) = nearest_edge(r, nearest.1);
                why.push(if nearest.0 < 0.0 {
                    format!("a pad is {:.2} mm outside the {side} edge", -nearest.0)
                } else {
                    format!(
                        "a pad is {:.2} mm from the {side} edge and the process asks {wanted:.2} mm",
                        nearest.0
                    )
                });
                worst = worst.max(wanted - nearest.0);
            }
        }
        for cut in self.p.cutouts {
            let gap = self
                .shape(i)
                .pad_boxes
                .iter()
                .map(|pad| rect_distance(&pad.translated(pos), cut))
                .fold(f64::MAX, f64::min);
            if gap < wanted - 1e-6 {
                let at = cut.centre();
                why.push(format!(
                    "a pad is {gap:.2} mm from the cutout at ({:.1}, {:.1}) and the process asks {wanted:.2} mm",
                    at.x - r.min.x,
                    at.y - r.min.y
                ));
                worst = worst.max(wanted - gap);
            }
        }
        if why.is_empty() {
            why.push("reference text too near the edge, or a requirement".to_string());
        }
        OutsideAt {
            part: name.to_string(),
            why,
            worst,
        }
    }

    /// The hard cost of one part against the walls, keepouts and every other part.
    fn hard_of(&self, i: usize) -> f64 {
        let mut hard = self.single(i).hard;
        for &k in &self.links_of[i] {
            hard += self.link_cost(k);
        }
        for j in 0..self.models.len() {
            if j != i {
                hard += self.pair(i, j).hard;
            }
        }
        hard
    }

    /// Move each part that overlaps another, or a wall or a keepout, to the nearest place
    /// round it where it does not. Returns whether all of them found one.
    fn legalise(&mut self) -> bool {
        let reach =
            ((self.p.region.width().max(self.p.region.height())) / (10.0 * UNIT)).ceil() as i64;
        let mut all = true;
        for k in 0..self.free.len() {
            let i = self.free[k];
            if self.hard_of(i) <= 1e-9 {
                continue;
            }
            let start = self.poses[i];
            let mut found = false;
            'rings: for ring in 1..=reach {
                for dy in -ring..=ring {
                    for dx in -ring..=ring {
                        if dx.abs().max(dy.abs()) != ring {
                            continue;
                        }
                        let pose = self.clamp(
                            i,
                            Pose {
                                x: start.x + dx * 10,
                                y: start.y + dy * 10,
                                turn: start.turn,
                                text: start.text,
                            },
                        );
                        // The parts it is related to go with it.
                        let change = self.expand(vec![(i, pose)]);
                        let undo: Vec<(usize, Pose)> =
                            change.iter().map(|&(j, _)| (j, self.poses[j])).collect();
                        for &(j, q) in &change {
                            self.put(j, q);
                        }
                        if change.iter().all(|&(j, _)| self.hard_of(j) <= 1e-9) {
                            found = true;
                            break 'rings;
                        }
                        for &(j, q) in undo.iter().rev() {
                            self.put(j, q);
                        }
                    }
                }
            }
            if !found {
                self.put(i, start);
                all = false;
            }
        }
        self.refresh_all();
        all
    }

    /// Where `i` goes to be beside `j`, on the side of `j`'s courtyard nearest `towards`.
    fn beside(&self, i: usize, j: usize, towards: Pt, rng: &mut Rng) -> Pose {
        let cj = self.court(j);
        let ci = self.shape(i).court;
        let (w, h) = (ci.width(), ci.height());
        let gap = self.gap + 0.1;
        // Distances from the point to each side of the courtyard.
        let sides = [
            (towards.x - cj.min.x, 0),
            (cj.max.x - towards.x, 1),
            (towards.y - cj.min.y, 2),
            (cj.max.y - towards.y, 3),
        ];
        let mut best = sides[0];
        for s in sides {
            if s.0 < best.0 {
                best = s;
            }
        }
        // Sometimes the second nearest side, so that a crowded side is not the only one.
        if rng.below(4) == 0 {
            let mut second = if best.1 == 0 { sides[1] } else { sides[0] };
            for s in sides {
                if s.1 != best.1 && s.0 < second.0 {
                    second = s;
                }
            }
            best = second;
        }
        let jitter = |rng: &mut Rng| rng.spread(6) as f64 * UNIT;
        let centre = match best.1 {
            0 => Pt::new(cj.min.x - gap - w / 2.0, towards.y + jitter(rng)),
            1 => Pt::new(cj.max.x + gap + w / 2.0, towards.y + jitter(rng)),
            2 => Pt::new(towards.x + jitter(rng), cj.min.y - gap - h / 2.0),
            _ => Pt::new(towards.x + jitter(rng), cj.max.y + gap + h / 2.0),
        };
        let origin = centre - ci.centre();
        Pose {
            x: (origin.x / UNIT).round() as i64,
            y: (origin.y / UNIT).round() as i64,
            turn: self.poses[i].turn,
            text: self.poses[i].text,
        }
    }

    /// The pose of `i` turned to `turn` about its courtyard's centre.
    fn turned(&self, i: usize, turn: u8) -> Pose {
        let now = self.court(i).centre();
        let m = &self.models[i];
        let after = m.shapes[variant(turn, self.poses[i].text).min(m.shapes.len() - 1)]
            .court
            .centre();
        let origin = now - after;
        Pose {
            x: (origin.x / UNIT).round() as i64,
            y: (origin.y / UNIT).round() as i64,
            turn,
            text: self.poses[i].text,
        }
    }

    /// A random change: the parts and the poses they would take. Empty if nothing can move.
    /// A part with requirements on its place only goes where they allow.
    fn propose(&mut self, rng: &mut Rng, reach: i64) -> Vec<(usize, Pose)> {
        let mut change = self.propose_free(rng, reach);
        for (i, pose) in &mut change {
            *pose = self.clamp(*i, *pose);
        }
        // A part in a relation takes the parts it is related to along.
        self.expand(change)
    }

    fn propose_free(&self, rng: &mut Rng, reach: i64) -> Vec<(usize, Pose)> {
        if self.free.is_empty() {
            return Vec::new();
        }
        let i = self.free[rng.below(self.free.len())];
        let now = self.poses[i];
        let roll = rng.below(100);
        if roll < 40 {
            let r = if rng.below(2) == 0 {
                reach
            } else {
                reach.min(20)
            };
            return vec![(
                i,
                Pose {
                    x: now.x + rng.spread(r),
                    y: now.y + rng.spread(r),
                    ..now
                },
            )];
        }
        if roll < 46 && self.models[i].shapes.len() == VARIANTS {
            // The text to another side of the part.
            let text = (now.text + 1 + rng.below(3) as u8) % 4;
            return vec![(i, Pose { text, ..now })];
        }
        if roll < 52 {
            if self.models[i].shapes.len() < VARIANTS {
                return Vec::new();
            }
            let turn = (now.turn + 1 + rng.below(3) as u8) % 4;
            return vec![(i, self.turned(i, turn))];
        }
        if roll < 66 {
            let j = self.free[rng.below(self.free.len())];
            if j == i || self.models[i].side != self.models[j].side {
                return Vec::new();
            }
            let (ci, cj) = (self.court(i).centre(), self.court(j).centre());
            let (si, sj) = (&self.shape(i).court, &self.shape(j).court);
            let (oi, oj) = (cj - si.centre(), ci - sj.centre());
            return vec![
                (
                    i,
                    Pose {
                        x: (oi.x / UNIT).round() as i64,
                        y: (oi.y / UNIT).round() as i64,
                        turn: now.turn,
                        text: now.text,
                    },
                ),
                (
                    j,
                    Pose {
                        x: (oj.x / UNIT).round() as i64,
                        y: (oj.y / UNIT).round() as i64,
                        turn: self.poses[j].turn,
                        text: self.poses[j].text,
                    },
                ),
            ];
        }
        if roll < 90 {
            let list = &self.neighbours[i];
            if list.is_empty() {
                return Vec::new();
            }
            let (j, k) = list[rng.below(list.len())];
            let mut pose = self.beside(i, j, self.pad_at(j, k), rng);
            if self.models[i].shapes.len() == VARIANTS && rng.below(3) == 0 {
                let turn = rng.below(4) as u8;
                let (now_turn, text) = (self.poses[i].turn, self.poses[i].text);
                let m = &self.models[i];
                let from = m.shapes[variant(now_turn, text)].court.centre();
                let to = m.shapes[variant(turn, text)].court.centre();
                let shift = from - to;
                pose = Pose {
                    x: pose.x + (shift.x / UNIT).round() as i64,
                    y: pose.y + (shift.y / UNIT).round() as i64,
                    turn,
                    text,
                };
            }
            return vec![(i, pose)];
        }
        // A whole module moves together.
        let list = &self.groups_of[i];
        if list.is_empty() {
            return Vec::new();
        }
        let g = list[rng.below(list.len())];
        let (dx, dy) = (rng.spread(reach), rng.spread(reach));
        self.groups[g]
            .iter()
            .filter(|&&m| !self.models[m].fixed)
            .map(|&m| {
                let p = self.poses[m];
                (
                    m,
                    Pose {
                        x: p.x + dx,
                        y: p.y + dy,
                        turn: p.turn,
                        text: p.text,
                    },
                )
            })
            .collect()
    }

    fn all(&self) -> Vec<usize> {
        (0..self.models.len()).collect()
    }
}

/// Place the parts that are not fixed so that the cost is least the search finds, and write
/// the result into `parts` (`at` and `rotation`). `effort` scales the number of moves tried.
pub(crate) fn anneal(parts: &mut [Placed], p: &Problem, effort: f64) -> Outcome {
    let names: Vec<String> = parts.iter().map(|part| part.refdes.clone()).collect();
    let mut e = Engine::new(parts, p);
    let mut rng = Rng::new(p.seed);
    let all = e.all();
    let mut feasible = e.legalise();
    let free = e.free.len();
    if free > 0 {
        let moves = ((MOVES_PER_PART * effort * free as f64) as usize).clamp(2000, MAX_MOVES);
        let region_reach =
            (e.p.region.width().max(e.p.region.height()) / UNIT / 2.0).max(40.0) as i64;

        // The first temperature: most uphill moves of a sample are accepted at it.
        let mut uphill = Vec::new();
        let mut current = e.eval(&all);
        let mut probe = Rng::new(p.seed ^ 0xD1B5_4A32_D192_ED03);
        for _ in 0..300 {
            let change = e.propose(&mut probe, region_reach / 4);
            if change.is_empty() {
                continue;
            }
            let set: Vec<usize> = change.iter().map(|&(i, _)| i).collect();
            let before = e.eval(&set).total();
            let old: Vec<Pose> = set.iter().map(|&i| e.poses[i]).collect();
            for &(i, pose) in &change {
                e.put(i, pose);
            }
            let after = e.eval(&set).total();
            for (&i, &pose) in set.iter().zip(&old) {
                e.put(i, pose);
            }
            if after > before {
                uphill.push(after - before);
            }
        }
        uphill.sort_by(f64::total_cmp);
        let median = uphill.get(uphill.len() / 2).copied().unwrap_or(1.0);
        let (start, end) = (median.clamp(1.0, 20.0), 0.03);

        let mut stages = 0usize;
        let mut t = start;
        while t > end {
            t *= COOLING;
            stages += 1;
        }
        let per_stage = (moves / stages.max(1)).max(1);
        // The best arrangement the run itself finds, not the one it began with: another seed
        // is another arrangement even when the start was a good one.
        let mut best_poses: Option<Vec<Pose>> = None;
        let mut best_cost = f64::MAX;
        let mut t = start;
        let mut since_sync = 0usize;
        for stage in 0..stages {
            // The cost of reference text over other parts grows through the run, from a hint
            // to as much as a hard rule, so that parts find their places first and the text
            // is cleared of them last. The best state is one of the last part, when the
            // weight is full.
            let progress = stage as f64 / stages as f64;
            let last = progress >= TEXT_FULL_AT;
            let weight = if last {
                W_HARD
            } else {
                TEXT_WEIGHT_FROM + (TEXT_WEIGHT_TO - TEXT_WEIGHT_FROM) * progress / TEXT_FULL_AT
            };
            if last != e.text_last || (!last && weight != e.text_weight) {
                e.text_last = last;
                e.text_weight = weight;
                current = e.eval(&all);
                since_sync = 0;
                if last {
                    best_poses = None;
                    best_cost = f64::MAX;
                }
            }
            // Moves shrink with the temperature, from half the board down to a few units.
            let reach = ((region_reach as f64 * t / start) as i64).clamp(8, region_reach);
            for _ in 0..per_stage {
                let change = e.propose(&mut rng, reach);
                if change.is_empty() {
                    continue;
                }
                let set: Vec<usize> = change.iter().map(|&(i, _)| i).collect();
                let before = e.eval(&set);
                let old: Vec<Pose> = set.iter().map(|&i| e.poses[i]).collect();
                for &(i, pose) in &change {
                    e.put(i, pose);
                }
                let after = e.eval(&set);
                let delta = after.total() - before.total();
                if delta <= 0.0 || rng.unit() < exp_neg(delta / t) {
                    e.refresh(&set);
                    current.soft += after.soft - before.soft;
                    current.hard += after.hard - before.hard;
                    since_sync += 1;
                    if e.text_last && current.hard <= 1e-9 && current.total() < best_cost {
                        best_cost = current.total();
                        best_poses = Some(e.poses.clone());
                    }
                } else {
                    for (&i, &pose) in set.iter().zip(&old) {
                        e.put(i, pose);
                    }
                }
            }
            if since_sync > 2000 || stage + 1 == stages {
                current = e.eval(&all);
                since_sync = 0;
                if e.text_last && current.hard <= 1e-9 && current.total() < best_cost {
                    best_cost = current.total();
                    best_poses = Some(e.poses.clone());
                }
            }
            t *= COOLING;
        }
        match best_poses {
            Some(poses) => {
                e.put_all(poses);
                e.refresh_all();
                feasible = true;
            }
            None => feasible = e.legalise(),
        }
        if !feasible {
            feasible = e.legalise();
        }
    }

    let ended = if feasible {
        Ended::default()
    } else {
        e.ended(&names)
    };
    // Write the poses back.
    for (i, part) in parts.iter_mut().enumerate() {
        let pose = e.poses[i];
        if e.models[i].fixed {
            continue;
        }
        let exact = |v: f64| (v * 1e6).round() / 1e6;
        let pinned = e.limits(i, pose.turn);
        // A part in a relation is where the relation puts it, which may be off the grid.
        if !e.links_of[i].is_empty() {
            let at = e.pos(i);
            part.at = Pt::new(exact(at.x), exact(at.y));
            part.rotation = (e.models[i].angles[0] + 90.0 * f64::from(pose.turn)).rem_euclid(360.0);
            part.reference_side = ReferenceSide::ALL[usize::from(pose.text)];
            continue;
        }
        part.at = Pt::new(
            pinned
                .and_then(|t| t.pin_x)
                .map_or((pose.x as f64 * 5.0).round() / 100.0, exact),
            pinned
                .and_then(|t| t.pin_y)
                .map_or((pose.y as f64 * 5.0).round() / 100.0, exact),
        );
        part.rotation = (e.models[i].angles[0] + 90.0 * f64::from(pose.turn)).rem_euclid(360.0);
        part.reference_side = ReferenceSide::ALL[usize::from(pose.text)];
    }
    Outcome { feasible, ended }
}

#[cfg(test)]
mod tests {
    use netsmith_core::{Design, Net, Part, PinRef};

    use crate::board::Board;
    use crate::board::tests::fixtures;

    use super::*;

    #[test]
    fn the_random_numbers_and_the_exponential_are_the_same_on_every_machine() {
        // The generator is SplitMix64: the first outputs for a seed are fixed numbers.
        let mut r = Rng::new(0);
        let first = [r.next(), r.next(), r.next()];
        let mut again = Rng::new(0);
        assert_eq!(first, [again.next(), again.next(), again.next()]);
        assert_ne!(first[0], Rng::new(1).next());
        assert!((0..1000).all(|_| (0.0..1.0).contains(&r.unit())));
        assert!((0..1000).all(|_| r.below(7) < 7));
        assert!((0..1000).all(|_| r.spread(5).abs() <= 5));
        assert_eq!(exp_neg(0.0), 1.0);
        assert!((exp_neg(1.0) - (-1.0f64).exp()).abs() < 1e-9);
        assert!((exp_neg(7.3) - (-7.3f64).exp()).abs() < 1e-9);
        assert_eq!(exp_neg(41.0), 0.0);
    }

    fn resistors(paths: &[&str]) -> Board {
        let parts = paths
            .iter()
            .enumerate()
            .map(|(i, path)| Part {
                refdes: format!("R{}", i + 1),
                kind: "Resistor".into(),
                value: "1k".into(),
                package: Some("0603".into()),
                path: (*path).into(),
                pins: Vec::new(),
            })
            .collect();
        let design = Design {
            name: "T".into(),
            process: Some(crate::rules::test_process()),
            features: Default::default(),
            parts,
            nets: vec![Net {
                name: "N".into(),
                pins: vec![PinRef {
                    refdes: "R1".into(),
                    pin: "1".into(),
                }],
            }],
        };
        Board::from_design(&design, &netsmith_kicad::Mapping::standard(), &fixtures()).unwrap()
    }

    #[test]
    fn a_module_costs_more_the_further_apart_its_parts_are() {
        let mut board = resistors(&["a.x", "a.y", "b.x", "b.y"]);
        let fixed = [false; 4];
        let problem = Problem {
            nets: &board.nets.clone(),
            rules: &board.rules.clone(),
            planes: &[],
            region: board.outline,
            corner_radius: 0.0,
            fixed: &fixed,
            keepouts: &[],
            zones: &[],
            cutouts: &[],
            pours: false,
            seed: 0,
            wants: &[],
            offsets: &[],
            overhangs: &[],
            outline: board.outline,
        };
        let mut e = Engine::new(&mut board.parts, &problem);
        assert_eq!(e.groups.len(), 2, "one module instance for a and one for b");
        let at = |x: i64, y: i64| Pose {
            x,
            y,
            turn: 0,
            text: 0,
        };
        // a's parts side by side, b's far away, then a's parts apart.
        for (i, (x, y)) in [(0, 0), (60, 0), (800, 0), (860, 0)]
            .into_iter()
            .enumerate()
        {
            e.put(i, at(x, y));
        }
        let together = e.group_cost(0);
        e.put(1, at(600, 400));
        let apart = e.group_cost(0);
        assert!(apart > together + 10.0, "{together} and {apart}");
        // b's part inside the box of a costs, one outside does not.
        e.put(2, at(30, 0));
        let inside = e.group_cost(0);
        e.put(2, at(900, 0));
        e.put(1, at(60, 0));
        let outside = e.group_cost(0);
        assert!(inside > outside, "{inside} and {outside}");
    }

    #[test]
    fn parts_that_stay_are_never_moved_and_the_others_end_clear_of_them() {
        let mut board = resistors(&["", "", "", "", "", ""]);
        let before: Vec<Pt> = board.parts.iter().map(|p| p.at).collect();
        let fixed = [true, true, false, false, false, false];
        let problem = Problem {
            nets: &board.nets.clone(),
            rules: &board.rules.clone(),
            planes: &[],
            region: board.outline,
            corner_radius: 0.0,
            fixed: &fixed,
            keepouts: &[],
            zones: &[],
            cutouts: &[],
            pours: false,
            seed: 3,
            wants: &[],
            offsets: &[],
            overhangs: &[],
            outline: board.outline,
        };
        let outcome = anneal(&mut board.parts, &problem, 1.0);
        assert!(outcome.feasible);
        assert_eq!(board.parts[0].at, before[0]);
        assert_eq!(board.parts[1].at, before[1]);
        assert_eq!(board.check_placement(), vec![]);
    }

    #[test]
    fn two_parts_that_hang_over_the_same_edge_may_overlap_beyond_it() {
        // ADR 0045, section 10 (i): the placement check counts a part that may hang over an
        // edge as its courtyard clipped to the outline, so the annealer must too. R1 stands
        // 0.5 mm inside the bottom edge and R2 wholly below it, 0.2 mm under it; their whole
        // courtyards overlap, but R2 has no room on the board.
        let mut board = resistors(&["", ""]);
        let outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(20.0, 20.0));
        let fixed = [false; 2];
        let mut bottom = Edges::default();
        bottom.set(crate::overhang::Edge::Bottom);
        let run = |board: &mut Board, overhangs: &[Edges]| {
            let problem = Problem {
                nets: &board.nets.clone(),
                rules: &board.rules.clone(),
                planes: &[],
                region: outline,
                corner_radius: 0.0,
                fixed: &fixed,
                keepouts: &[],
                zones: &[],
                cutouts: &[],
                pours: false,
                seed: 0,
                wants: &[],
                offsets: &[],
                overhangs,
                outline,
            };
            let mut e = Engine::new(&mut board.parts, &problem);
            let court = e.shape(0).court;
            let pose = |top: f64| Pose {
                x: (10.0 / UNIT).round() as i64,
                y: ((top - court.min.y) / UNIT).round() as i64,
                turn: 0,
                text: 0,
            };
            e.put(0, pose(20.0 - 0.5));
            e.put(1, pose(20.0 + 0.2));
            let (a, b) = (e.court(0), e.court(1));
            assert!(a.overlaps(&b), "the whole courtyards overlap: {a:?} {b:?}");
            assert!(b.min.y >= 20.0, "R2 is below the edge: {b:?}");
            e.pair(0, 1)
        };
        let strict = run(&mut board, &[]);
        assert!(
            strict.hard > 0.0,
            "unstated, the overlap is hard: {strict:?}"
        );
        let stated = run(&mut board, &[bottom, bottom]);
        assert!(
            stated.hard == 0.0,
            "stated on both, the overlap beyond the edge is free: {stated:?}"
        );
        // One part stating it is enough for it to take only the room it has on the board.
        let one = run(&mut board, &[Edges::default(), bottom]);
        assert!(one.hard == 0.0, "{one:?}");
    }
}
