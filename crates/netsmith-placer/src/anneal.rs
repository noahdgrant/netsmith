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
//!   ([`netsmith_pcb::decoupling_pairs`]).
//! - **Edge**: a connector toward the nearest edge, a hole or fiducial toward a corner.
//! - **Area**: the half perimeter of the box round everything, so the board is no larger
//!   than it must be; and a small cost on every turned part, so a tie goes to the upright
//!   one.
//!
//! The run is a function of the parts, the nets and the seed only: positions are whole
//! units, the random numbers come from a generator written here, and nothing that a
//! platform's maths library may round differently (`exp`, `powf`, `hypot`) is used, so the
//! same input gives the same board on any machine.
//!
//! # What the annealer does, why, and what it measured
//!
//! This is ADR 0033 (accepted after the owner's review of 2026-11-04) and the placer part of
//! ADR 0031. The parts that were provisional (busy parts, below) are accepted as built by the
//! owner, 2026-11-11. It replaced
//! the row layout (connectors in two columns, the rest in rows of 30 mm, nothing turned, the
//! order improved by up to 8 passes of pair swaps) as the placer. The rows are kept as the
//! annealer's start and as the first test of whether the parts fit a size that was asked for
//! ([`crate::place::PlaceOptions::fit`]); the pair swaps are gone. P3 of ADR 0030, the
//! `layout` block of the language, is language and is not built.
//!
//! - **State.** For each part: its origin on a grid of 0.05 mm, its turn (0, 90, 180 or 270
//!   degrees from the angle it began at) and which side of it the reference text is on. The
//!   face is not part of the state (it comes from `PlaceOptions::sides`).
//! - **Start.** The rows arrangement, parts in the order of their instance paths, inside the
//!   outline the rows made (or the size asked for).
//! - **Moves.** Shift one part (a step that shrinks as the temperature falls), turn it about
//!   its courtyard's centre, move its reference text to another side, swap two parts, put a
//!   part beside a part it shares a net with (on the side of that part's courtyard nearest the
//!   connected pad, sometimes turned), and shift a whole module instance. The cost of a move
//!   is the difference of the terms that involve the parts moved, so a move costs about the
//!   same on a board of 70 parts as on one of 700.
//! - **Schedule.** The first temperature is the median of the uphill changes a sample of 300
//!   moves would make (at most 20 mm of wire), the last is 0.03, and it falls by 0.93 after
//!   each stage. The number of moves is 3000 for each free part, at least 2000 and at most
//!   300 000. It is a count, not a clock.
//! - **Result.** The best state of the last part of the run that breaks no hard rule. If there
//!   is none, the parts that overlap something are moved to the nearest free place round
//!   them; if that fails too, the rows arrangement stands and `check_placement` says what is
//!   wrong. Without a size the board is the arrangement's box plus the wall round the
//!   courtyards (and 0.5 mm round the reference text), on whole millimetres, and the
//!   arrangement is moved to the corner.
//! - **Determinism.** The same circuit, options and seed give the same board on any machine.
//!   The random numbers come from SplitMix64 written in this crate, `exp` is a series of
//!   additions and multiplications, distances use `sqrt` and not `hypot`, nothing iterates a
//!   hash map to decide anything. `PlaceOptions::seed` (the layout file's `seed`, 0 to
//!   4294967295) changes the run.
//!
//! **The weights** (the `W_*` constants) in distances of mm: wire 1 (a net of many pads counts
//! less per mm, down to 0.35, and a plane net 0.15 of that); a courtyard overlap is hard (200)
//! and nearer than 1.0 mm soft (4; 1.8 mm on a board with a pour on an outer layer, which needs
//! room round a pad for its spokes); the walls 200; cohesion 0.3 for the parts of a module
//! instance and 0.05 for an intruder; crossings 1 (pairs of ratsnest lines, the shortest tree
//! over each net's pads, for nets of at most 10 pads that are not planes); decoupling 6; edge
//! 0.4; area 0.03; turn 0.3; silkscreen a ramp and 3. A part with a hole through the board keeps
//! the process's clearance and hole to hole space instead of the soft gap. A part with many
//! pads asks 0.03 mm more for each pad, to 2.5 mm, so that its tracks can leave, and a
//! decoupling capacitor and the part it decouples are exempt from the extra. Parts on opposite
//! faces share area unless one has a hole, and then only where it takes room on the other face:
//! its courtyard, or its pad boxes (each grown by the process's clearance) if the footprint
//! draws its courtyard on one face's layer only (ADR 0031: another part's courtyard on that
//! face may cover the rest of its body, and a pad under a pad box is still an error).
//!
//! **The wall** (accepted by the owner, 2026-11-04). The placer kept a fixed 2.0 mm between
//! every courtyard and the outline, which costs about 364 mm2 of each face on a 35 x 60 mm board,
//! where a real layout had parts 0.55 mm from the edge, and nothing recorded why it was 2.0.
//! What it can protect, and what was found: the pad to edge rule is `copper_to_edge` (0.2 mm
//! for JLCPCB), which the router keeps itself, and a pad is inside its courtyard, so the wall
//! matters only for a pad that sticks out of it, which the annealer keeps off the edge with a
//! hard cost of its own. A routing channel round the edge is not needed, since tracks go inward:
//! several boards routed completely and passed netsmith's check and KiCad's with the wall at 0.5
//! and 0.9 mm. A V-cut asks for copper 0.4 mm from the edge (`VCutPanel.min_edge_clearance`),
//! which a board states by narrowing its own `min_edge_clearance`, so the wall follows it.
//! Reference text keeps its own 0.5 mm ([`TEXT_EDGE`]), and `advised_edge_clearance` (the
//! fabricator's advice for a V-cut, 1 mm) stays a note. So the wall is `Rules::edge_wall`: the
//! copper to edge space, the process's clearance and its least track, one track with its space
//! beside a part (0.4 mm for JLCPCB's, 0.75 mm for the test process), never less than the
//! copper to edge space. A placer given no rules keeps 2.0 mm (`Rules::DEFAULT_EDGE_WALL`). The
//! cost: less room for a track beside a part that close to the edge, which the router handles,
//! and a layout's silkscreen may come nearer the edge by chance (a warning, 0.07 mm silk to silk
//! on one seed).
//!
//! **Silkscreen** is a cost that grows during the run, from 2 per mm to 40 and then to a ban
//! for the last 30 % of the moves. A ban from the start makes the board 30 % longer in wire,
//! because every part must have a free place for its text before it has a place: on the
//! synthetic boards, four seeds, with no rule the big board's wire was 995 mm, with the text
//! banned over other parts from the first move 1412 mm, with the weight ramped 1303 mm. The
//! text sides (four, not one) recovered little of that, the ramp most. The text box is the
//! library's text size (or the process's, if more) by about one text height for each letter,
//! grown by [`TEXT_PAD`], and sits where the part's `ReferenceSide` says. The text is upright
//! whatever the part's turn, so `Placed::reference_at` finds the place above (or beside) the
//! part as it lies on the board and turns it back to the footprint's frame.
//!
//! **Crossings.** With no term for the lines that cross the wire was as short but the boards
//! were hard to route (the big board's seed 0 took 54 s and left two lines in a release
//! build); counting crossings halves them, and every board of every seed tried has routed
//! completely since. A pull of the parts of a module to their centre, squared, made the wire
//! longer than the box round them does.
//!
//! **Decoupling pairs** are derived ([`netsmith_pcb::decoupling_pairs`]), not written: a
//! two-pad capacitor (kind `Capacitor...`) whose pads are on two different nets, such that one
//! same other part has a `power` pin on each. The parts that share most of the capacitor's
//! instance path are kept, so a module's capacitor goes with the module's part. The cost is the
//! nearest power pin pad on each net. No number and no annotation is read.
//!
//! **Busy parts** (ADR 0031). A part with 16 to 66 pads ([`BUSY_ESCAPE`] to [`BUSY_MOST`] of
//! escape) asks, from its neighbour, the sum of the two escape spaces and not the larger (the
//! same channel serves both rows), and each mm short of it costs [`W_GAP`] plus up to
//! [`W_ESCAPE`] instead of [`W_GAP`] alone. Found on a 35 x 60 mm board with two QFN packages
//! that 14 nets join: the annealer put them 0.5 mm apart where the gap it asks for was 1.47 and
//! 0.99 mm, the 12 + 8 pads of two rows that face each other had to leave through a channel in
//! which three tracks fit, and the link test said "closed" at the pad end in more than half of
//! the failed attempts. It was the placement, not the order of the lines. Lines left over seeds
//! 0 to 11 went from 96 (0 of 12 seeds done) to 28 (1 of 12), and the courtyards of the two
//! packages are 3.5 mm apart (`two_busy_packages_*` keeps them at 2 mm or more). Pairs where
//! neither part is busy are costed as before. Tried and not kept: the larger of the two escape
//! spaces at a heavier weight (17, 16, 13 lines at 40, 100, 300, 1000), the same rule for every
//! pair with one busy part and no limit (it spaced every small part away from a module of 80
//! pads by up to 2.4 mm and left 64 lines on the tight board against 41), and no weight for a
//! QFN against a resistor (54).
//!
//! Choices (the conservative one is built, accepted as built by the owner, 2026-11-11): a limit
//! of 66 pads, or none (without one the mixed board gets the same 28 lines and the tight one
//! gets worse, 41 to 64; the alternatives are a limit stated in the process or a part's own
//! datasheet, or a ring that is a keepout the router can see); a soft weight, or a rule (the
//! escape space stays soft: a hard minimum would refuse boards that place today); 300 and 0.03
//! mm a pad (both tuned on two invented boards over 12 seeds, where seed to seed varies by two
//! to three lines); and the room round a part with more than 66 pads, which is still the plain
//! gap plus its escape space at the plain weight (the "escape rings" are not built; issue #58).
//!
//! **Measured** on the two synthetic boards of `crates/netsmith-router/tests/synthetic.rs` (the
//! only boards in this repository, so anyone can re-measure; `cargo test --release -p
//! netsmith-router --test main synthetic::the_placement_of_the_big_boards_is_measured -- --nocapture` prints the numbers): the big board (70 parts, an
//! 80-pad four-sided package, 106 nets, four layers) and the same with the structure of a
//! design in modules (78 parts, instance paths of eight parts, 8 decoupling capacitors between
//! 3V3 and GND, `power` pins on the package). Before is the row layout, after is this
//! placer, seed 0, release build:
//!
//! | | big board | with modules and power pins |
//! |---|---|---|
//! | board | 42 x 53 to 42 x 42 mm | 43 x 56 to 42 x 48 mm |
//! | wire length | 1805 to 1441 mm | 1770 to 1381 mm |
//! | ratsnest crossings | 456 to 178 | 364 to 182 |
//! | decoupling distance, mean / max | none paired | 31.8 / 42.9 mm to 5.0 / 10.3 mm |
//! | connections left, KiCad's check | 0 and 0 errors | 0 and 0 errors |
//! | route time | 3.4 s to 1.1 s | 2.3 s to 2.0 s |
//! | placement time | 0.4 s to 1.7 s | 0.4 s to 3.1 s |
//!
//! Over more seeds seed 0 is not the best: six seeds of the big board average 1291 mm and 150
//! crossings; four seeds of the one with modules 1442 mm, 181 crossings and 3.9 mm of
//! decoupling. A board of a real design with a module was also measured, but its data is not in
//! this repository, so no figure from it is kept.
//!
//! **Limits.** Routability is a proxy: the placer keeps a gap between parts, room round parts
//! with many pads and few ratsnest lines crossing, but it does not estimate congestion, and a
//! board packed harder than the router can untangle is not helped. The placer shares the faces
//! only when a size to fit needs it, and never flips a part for wire length. Hierarchical
//! placement (arrange each module, then place the blocks) is not built: cohesion is the cost
//! above. The weights were tuned on four boards (one not in this repository) and a handful of
//! seeds, and the variation between seeds on one board is large. A footprint's own silkscreen
//! text and strokes that clip each other or the mask are the library's and stay. KiCad's
//! `starved_thermal` (a pad in a pour with one spoke) appeared on some seeds of a real board
//! whose data is not in this repository, and neither check finds it (ADR 0037). Two latent faults
//! that the new placements showed were fixed (stitching vias in the gap between two pads, the
//! reference text of a turned part). The decoupling pull is a distance with no number from a
//! source.
//!
//! **For a person** (the owner's review of 2026-11-04 asked for it; checklist in issue #10): a
//! public reference board of a real design, with a module, whose wire, crossings and check results
//! are measured and kept, so the weights are tuned on something a reader can run. The `layout`
//! block of the language (P3 of ADR 0030) is not built (issues #58 and #59).

use netsmith_core::PlaceField;
use netsmith_pcb::{
    BoardNet, Placed, PlanOffset, ReferenceSide, Side, Want, constraint_axis_of, constraint_offset,
    constraint_quarter, rect_distance,
};

use netsmith_pcb::{DecouplingPair, decoupling_pairs};
use netsmith_pcb::{Edges, Ended, OutsideAt, OverlapAt, Pt, Rect, Rules, short_mm};

mod build;
mod cost;
mod explain;
mod moves;
mod nets;
mod pose;
mod rooms;
mod shapes;
#[cfg(test)]
mod tests;

pub(crate) use shapes::*;

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
/// How far a part's body is kept from a height-limited zone it is too tall for, in mm (ADR 0045
/// section 11, owner decision 2026-11-04). The placer's own small clearance for
/// keepouts, [`KEEPOUT_GAP`]. The process's copper clearance (0.1 to 0.2 mm) is smaller than
/// the 0.25 mm the placer already wants beside a keepout, and a height limit is not copper,
/// so the placer's constant is used and a process with more clearance changes nothing here.
pub(crate) const HEIGHT_ZONE_MARGIN: f64 = KEEPOUT_GAP;
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
/// A part whose escape space is at least [`BUSY_ESCAPE`] (16 pads or more) is busy, and up to
/// [`BUSY_MOST`] (66 pads): the escape spaces of it and of its neighbour add up, and the space
/// between them counts for more than a plain gap. A part with more pads than that (a module of
/// eighty pads is 2.4 mm) asks for a ring that a small board cannot give: it keeps the plain
/// weight, as measured on the invented tight board (ADR 0031).
const BUSY_ESCAPE: f64 = 16.0 * ESCAPE_PER_PAD;
const BUSY_MOST: f64 = 2.0;
/// What each mm short of the escape space between two parts costs, at the most (for two
/// parts of [`MAX_ESCAPE`] each), on top of [`W_GAP`]: much more than a plain gap, because the
/// pads of two parts that face each other have to leave through the space between them, and
/// a ratsnest that pulls the parts together outweighs a plain gap (issue #3).
const W_ESCAPE: f64 = 300.0;
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
/// What a hard cost may be and the state still counts as clear of every hard rule: 5 nanometres
/// of a wall, a thousandth of the grid, below the 1e-6 mm the placement check itself ignores.
const HARD_EPS: f64 = 1e-3;
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
    pub zones: &'a [netsmith_pcb::HeightZone],
    /// The keepouts that parts carry: each moves and turns with its part, and keeps the other
    /// parts out as a keepout for parts (`parts`) and as a height zone (`limit`) do.
    pub carried: &'a [netsmith_pcb::CarriedZone],
}

/// What a run ended with.
pub(crate) struct Outcome {
    /// No courtyards overlap, and all are inside the walls and out of the keepouts.
    pub feasible: bool,
    /// What the run ended with that breaks a hard rule, for the message when it is not
    /// feasible.
    pub ended: Ended,
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
    /// How far its pins protrude out of the opposite face, and the facts a part
    /// that stands over them is judged by (`netsmith_pcb::judge_protrusion`).
    protrusion: Option<f64>,
    height: Option<f64>,
    hand_fitted: bool,
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
    /// Whether the footprint draws no fabrication outline, so that its courtyard is its body.
    body_is_courtyard: bool,
    /// The keepouts the part carries, which keep the other parts out where the part is.
    carries: Vec<Carry>,
}

/// A keepout that a part carries, as the annealer measures it (ADR 0045 section 11).
struct Carry {
    /// Which of `Problem::carried` it is.
    zone: usize,
    /// Its box relative to the part's origin, for each of the part's shapes (the same
    /// indices as `Model::shapes`): the box that holds it turned, and mirrored on the back.
    boxes: Vec<Rect>,
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
            *slot = constraint_quarter(base + 90.0 * t as f64)
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

/// Place the parts that are not fixed so that the cost is least the search finds, and write
/// the result into `parts` (`at` and `rotation`). `effort` scales the number of moves tried.
pub(crate) fn anneal(parts: &mut [Placed], p: &Problem, effort: f64) -> Outcome {
    let names: Vec<String> = parts.iter().map(|part| part.refdes.clone()).collect();
    let shown: Vec<String> = parts
        .iter()
        .map(|part| netsmith_pcb::board_member(&part.path, &part.refdes))
        .collect();
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
                // A move is some microseconds, so this is no cost; a cancelled job (`fab`)
                // ends here, in the middle of the placing.
                netsmith_core::job::checkpoint();
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
                    if e.text_last && current.hard <= HARD_EPS && current.total() < best_cost {
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
                if e.text_last && current.hard <= HARD_EPS && current.total() < best_cost {
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
        // What is left is a part a hair over a line or a neighbour: move it by the least.
        if !feasible {
            feasible = e.legalise_by(1, 40);
        }
    }

    let ended = if feasible {
        Ended::default()
    } else {
        let mut ended = e.ended(&names, &shown);
        ended.paths = parts
            .iter()
            .map(|part| (part.refdes.clone(), part.path.clone()))
            .collect();
        ended
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
