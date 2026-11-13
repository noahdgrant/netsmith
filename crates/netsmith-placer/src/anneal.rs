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
//! **Decoupling pairs** are derived ([`crate::quality::decoupling_pairs`]), not written: a
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
use netsmith_pcb::board::{BoardNet, Placed, ReferenceSide, Side, rect_distance};
use netsmith_pcb::constraints::{PlanOffset, Want, axis_of, offset, quarter};

use crate::quality::{DecouplingPair, decoupling_pairs};
use netsmith_pcb::geometry::{Pt, Rect};
use netsmith_pcb::overhang::Edges;
use netsmith_pcb::placement::{Ended, OutsideAt, OverlapAt, short};
use netsmith_pcb::rules::Rules;

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
    pub zones: &'a [netsmith_pcb::placement::HeightZone],
    /// The keepouts that parts carry: each moves and turns with its part, and keeps the other
    /// parts out as a keepout for parts (`parts`) and as a height zone (`limit`) do.
    pub carried: &'a [netsmith_pcb::placement::CarriedZone],
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
    /// that stands over them is judged by (`netsmith_pcb::protrusion`).
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
        .filter(|g| g.layer == netsmith_pcb::footprint::Layer::Silk)
        .flat_map(|g| {
            let pieces: Vec<Vec<Pt>> = match &g.shape {
                netsmith_pcb::footprint::Shape::Line { a, b } => vec![vec![*a, *b]],
                netsmith_pcb::footprint::Shape::Rect { a, b } => {
                    let (c, d) = (Pt::new(b.x, a.y), Pt::new(a.x, b.y));
                    vec![vec![*a, c], vec![c, *b], vec![*b, d], vec![d, *a]]
                }
                netsmith_pcb::footprint::Shape::Poly { points } => {
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

/// The box of a keepout in a part's own frame, relative to the part's origin, with the part
/// turned to `angle`: the box that holds it, as `Board::keepout_areas` finds it on the board.
fn carried_box(part: &mut Placed, angle: f64, local: Rect) -> Rect {
    let (at, rotation) = (part.at, part.rotation);
    part.at = Pt::default();
    part.rotation = angle;
    let found = Rect::around(local.corners().map(|c| part.to_board(c))).unwrap_or(local);
    part.at = at;
    part.rotation = rotation;
    found
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
        let reach_pad = (hole_gap.max(rules.silk_to_pad)).max(gap + 2.0 * MAX_ESCAPE) / 2.0 + 0.01;
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
                let path = parts[i].path.clone();
                let carries: Vec<Carry> = p
                    .carried
                    .iter()
                    .enumerate()
                    .filter(|(_, z)| z.part == path)
                    .map(|(zone, z)| {
                        let mut boxes = Vec::new();
                        if p.fixed[i] {
                            boxes.push(carried_box(&mut parts[i], base, z.local));
                        } else {
                            for &a in &angles {
                                let at = carried_box(&mut parts[i], a, z.local);
                                boxes.extend(ReferenceSide::ALL.iter().map(|_| at));
                            }
                        }
                        Carry { zone, boxes }
                    })
                    .collect();
                let kind = parts[i].kind.as_str();
                Model {
                    shapes,
                    angles,
                    side: parts[i].side,
                    hole: parts[i].has_hole(),
                    protrusion: parts[i].protrusion.as_ref().map(|t| t.length),
                    height: parts[i].height,
                    hand_fitted: parts[i].hand_fitted,
                    court_faces: parts[i].footprint.courtyard_faces,
                    role: if crate::place::is_connector(&parts[i]) {
                        Role::Connector
                    } else if netsmith_pcb::placement::is_mechanical(kind) {
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
                    body_is_courtyard: parts[i].footprint.body_is_courtyard,
                    carries,
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
        for (i, leaders) in leader_of.iter().enumerate() {
            for ax in 0..2 {
                if leaders[ax].is_none() {
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
        // A part that stays is where it is, exactly, though its pose is on the grid: the
        // check measures it there, so its courtyard and pads are measured there too, whether
        // or not it is in a relation (#457).
        let mut sub = vec![Pt::default(); n];
        for (i, q) in parts.iter().enumerate() {
            if p.fixed[i] {
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
        &self.models[i].shapes[self.shape_index(i)]
    }

    /// Which of the part's shapes (and carried boxes) it has in its pose.
    fn shape_index(&self, i: usize) -> usize {
        let pose = self.poses[i];
        variant(pose.turn, pose.text).min(self.models[i].shapes.len() - 1)
    }

    /// How far part `j` is in the keepouts part `c` carries, or nearer than `gap` to them:
    /// for each, its name, its box where it is, and the depth. A keepout that forbids parts
    /// is measured against the part's room (its courtyard), as the check does; a height
    /// limit the part is too tall for, against its body grown by a margin
    /// ([`Engine::zone_box`]). The part that carries a keepout is not in it.
    fn carried_depths(&self, c: usize, j: usize, gap: f64) -> Vec<(&'a str, Rect, f64)> {
        let mut found = Vec::new();
        if c == j || self.models[c].carries.is_empty() {
            return found;
        }
        let at = self.shape_index(c);
        let model = &self.models[j];
        for carry in &self.models[c].carries {
            let zone = &self.p.carried[carry.zone];
            let rect = carry.boxes[at.min(carry.boxes.len() - 1)].translated(self.pos(c));
            let mut depth = 0.0f64;
            if zone.parts {
                depth = depth.max(penetration(&self.room(j), &rect, gap));
            }
            if zone.limits(model.height, model.side) {
                depth = depth.max(penetration(&self.zone_box(j), &rect, gap));
            }
            if depth > 0.0 {
                found.push((zone.name.as_str(), rect, depth));
            }
        }
        found
    }

    /// What the keepouts part `c` carries cost part `j`: soft near them, hard in them, as a
    /// keepout for parts does.
    fn carried_cost(&self, c: usize, j: usize) -> Cost {
        let mut cost = Cost::default();
        for (_, _, depth) in self.carried_depths(c, j, KEEPOUT_GAP) {
            cost.soft += W_GAP * depth;
        }
        for (_, _, depth) in self.carried_depths(c, j, 0.0) {
            if depth > 1e-6 {
                cost.hard += W_HARD * (depth + 0.05);
            }
        }
        cost
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
    /// they both take room on, and whether either box is a part's pads. Where one stands
    /// excused over the other's protruding pins (`netsmith_pcb::protrusion`), the pair on that face is the
    /// pads of both.
    fn room_pairs(&self, i: usize, j: usize) -> Vec<(Rect, Rect, bool)> {
        let mut pairs: Vec<(Rect, Rect, bool)> = Vec::new();
        let excused = self
            .excused(i, j)
            .or_else(|| self.excused(j, i).map(|_| false));
        for face in [Side::Front, Side::Back] {
            if let Some(pins_first) = excused {
                let (t, c) = if pins_first { (i, j) } else { (j, i) };
                if face == self.models[c].side {
                    let protrusions: Vec<Rect> = self
                        .shape(t)
                        .rooms
                        .iter()
                        .map(|r| r.translated(self.pos(t)))
                        .collect();
                    let pads: Vec<Rect> = self
                        .shape(c)
                        .pad_boxes
                        .iter()
                        .map(|r| r.translated(self.pos(c)))
                        .collect();
                    for &x in &protrusions {
                        for &y in &pads {
                            let pair = if pins_first {
                                (x, y, true)
                            } else {
                                (y, x, true)
                            };
                            if !pairs.contains(&pair) {
                                pairs.push(pair);
                            }
                        }
                    }
                    continue;
                }
            }
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

    /// Whether part `c` stands excused over the protruding pins of part `t` where they are now
    /// (`netsmith_pcb::protrusion::judge`): `Some(true)` for `t` first. Asked as `excused(t, c)`.
    fn excused(&self, t: usize, c: usize) -> Option<bool> {
        let (mt, mc) = (&self.models[t], &self.models[c]);
        let length = mt.protrusion?;
        if !mt.hole || mc.hole || mt.side == mc.side {
            return None;
        }
        let cover = netsmith_pcb::protrusion::Cover {
            height: mc.height,
            hand_fitted: mc.hand_fitted,
            body: self.zone_box(c),
            side: mc.side,
        };
        matches!(
            netsmith_pcb::protrusion::judge(length, &cover, self.p.zones),
            netsmith_pcb::protrusion::Verdict::Excused { .. }
        )
        .then_some(true)
    }

    fn court(&self, i: usize) -> Rect {
        self.shape(i).court.translated(self.pos(i))
    }

    /// What part `i` takes against a height-limited zone: its body, the same box the check
    /// measures (`Board::check_placement`), grown by [`HEIGHT_ZONE_MARGIN`], so that a clear
    /// result passes the check. A footprint that draws no fabrication outline has only its
    /// courtyard, which stays as it is (ADR 0045 section 11).
    fn zone_box(&self, i: usize) -> Rect {
        if self.models[i].body_is_courtyard {
            return self.court(i);
        }
        self.shape(i)
            .body
            .translated(self.pos(i))
            .expanded(HEIGHT_ZONE_MARGIN)
    }

    /// The room part `i` takes against other courtyards: its courtyard, clipped to the
    /// outline if the design says it may hang over an edge (as `Board::check_placement`
    /// counts it, so that two parts that hang over the same edge may overlap beyond it).
    fn room(&self, i: usize) -> Rect {
        let court = self.court(i);
        if self.models[i].may.any() {
            netsmith_pcb::overhang::clip(&court, &self.p.outline)
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
        let tall = self.zone_box(i);
        for (k, part) in self
            .p
            .keepouts
            .iter()
            .map(|k| (k, &court))
            .chain(model.no_go.iter().map(|k| (k, &tall)))
        {
            let depth = penetration(part, k, KEEPOUT_GAP);
            if depth > 0.0 {
                c.soft += W_GAP * depth;
                if penetration(part, k, 0.0) > 1e-6 {
                    c.hard += W_HARD * (penetration(part, k, 0.0) + 0.05);
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

    /// What two parts cost together: courtyards, reference text and silkscreen, and the
    /// keepouts either carries against the other.
    fn pair(&self, i: usize, j: usize) -> Cost {
        // A keepout a part carries reaches past the part: it is not in the part's reach.
        let mut c = self.carried_cost(i, j);
        c += self.carried_cost(j, i);
        c += self.pair_of_parts(i, j);
        c
    }

    /// What two parts cost together by their own boxes: courtyards, reference text and
    /// silkscreen.
    fn pair_of_parts(&self, i: usize, j: usize) -> Cost {
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
        // Each part's pads leave through the space round it. Where a busy part (16 to 66 pads)
        // faces another the space is the same, so the escape spaces of the two add up and each
        // mm short of them costs more than a plain gap. Other pairs keep the larger of the
        // two spaces at the plain weight.
        let busiest = a.escape.max(b.escape);
        let busy = (BUSY_ESCAPE..=BUSY_MOST).contains(&busiest);
        let escape = if self.near_ok[i * self.models.len() + j] {
            0.0
        } else if busy {
            a.escape + b.escape
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
            let extra = if busy {
                W_ESCAPE * (escape / (2.0 * MAX_ESCAPE)).min(1.0)
            } else {
                0.0
            };
            c += Self::court_cost_escape(&ca, &cb, self.gap, escape, extra);
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

    /// [`Engine::court_cost`] where `escape` of the space asked for beyond `gap` is the room
    /// the parts' pads need to leave, each mm of which costs `extra` more than the plain gap
    /// ([`W_ESCAPE`]).
    fn court_cost_escape(a: &Rect, b: &Rect, gap: f64, escape: f64, extra: f64) -> Cost {
        let mut c = Cost::default();
        let full = penetration(a, b, gap + escape);
        if full > 0.0 {
            let plain = penetration(a, b, gap);
            c.soft += W_GAP * plain + (W_GAP + extra) * (full - plain);
            let over = penetration(a, b, 0.0);
            if over > 1e-6 {
                c.hard += W_HARD * (over + 0.05);
            }
        }
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
    fn ended(&self, names: &[String], shown: &[String]) -> Ended {
        let mut ended = Ended::default();
        for i in 0..self.models.len() {
            if !self.models[i].fixed && self.single(i).hard > HARD_EPS {
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
                } else if self.pair_of_parts(i, j).hard > HARD_EPS {
                    ended.silk += 1;
                }
            }
        }
        self.name_carried(names, shown, &mut ended);
        for (k, l) in self.links.iter().enumerate() {
            let miss = self.link_miss(k);
            if miss > LINK_EPS {
                ended.relations.push(format!(
                    "{}: {} minus {} is {} mm along {}, and it allows {} ({} mm over)",
                    l.label,
                    shown[l.b],
                    shown[l.a],
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

    /// The parts that stand in a keepout another part carries, added to the parts that are
    /// outside, each with the keepout, its carrier and where it is.
    // The indices name the models, the designators and the names shown together.
    #[allow(clippy::needless_range_loop)]
    fn name_carried(&self, names: &[String], shown: &[String], ended: &mut Ended) {
        for c in 0..self.models.len() {
            for j in 0..self.models.len() {
                if self.models[c].fixed && self.models[j].fixed {
                    continue;
                }
                for (zone, rect, depth) in self.carried_depths(c, j, 0.0) {
                    if depth <= 1e-6 {
                        continue;
                    }
                    let region = &self.p.region;
                    let at = rect.centre();
                    let why = format!(
                        "{} mm into the keepout `{zone}` that {} carries, at ({:.1}, {:.1})",
                        short(depth),
                        shown[c],
                        at.x - region.min.x,
                        at.y - region.min.y
                    );
                    match ended.outside_parts.iter_mut().find(|o| o.part == names[j]) {
                        Some(o) => {
                            o.why.push(why);
                            o.worst = o.worst.max(depth);
                        }
                        None => {
                            ended.outside += 1;
                            ended.outside_parts.push(OutsideAt {
                                part: names[j].clone(),
                                why: vec![why],
                                worst: depth,
                            });
                        }
                    }
                }
            }
        }
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
                why.push(format!("{} mm outside the {side} edge", short(past)));
                worst = worst.max(past + wall);
            } else if past + wall > 1e-6 {
                why.push(format!(
                    "{} mm inside the {} mm margin at the {side} edge",
                    short(past + wall),
                    (wall * 100.0).round() / 100.0
                ));
                worst = worst.max(past + wall);
            }
        }
        let tall = self.zone_box(i);
        for (k, part) in self
            .p
            .keepouts
            .iter()
            .map(|k| (k, &court))
            .chain(self.models[i].no_go.iter().map(|k| (k, &tall)))
        {
            let depth = penetration(part, k, 0.0);
            if depth > 1e-6 {
                let at = k.centre();
                why.push(format!(
                    "{} mm into a keepout, cutout or height limit at ({:.1}, {:.1})",
                    short(depth),
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
                    format!("a pad is {} mm outside the {side} edge", short(-nearest.0))
                } else {
                    format!(
                        "a pad is {} mm from the {side} edge and the process asks {wanted:.2} mm",
                        short(nearest.0)
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
        self.legalise_by(10, reach)
    }

    /// [`Engine::legalise`] in steps of `step` grid units, as far as `reach` steps. The last
    /// pass of a run uses one unit (0.05 mm) and 40 steps: a part left a hair past a line or
    /// over a neighbour moves by the least that clears it, where the coarse pass, which is in
    /// half millimetres, would have gone further or found nothing (B5, PROVISIONAL).
    fn legalise_by(&mut self, step: i64, reach: i64) -> bool {
        let mut all = true;
        for k in 0..self.free.len() {
            let i = self.free[k];
            if self.hard_of(i) <= HARD_EPS {
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
                                x: start.x + dx * step,
                                y: start.y + dy * step,
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
                        if change.iter().all(|&(j, _)| self.hard_of(j) <= HARD_EPS) {
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
    let shown: Vec<String> = parts
        .iter()
        .map(|part| netsmith_pcb::board::member(&part.path, &part.refdes))
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

#[cfg(test)]
mod tests {
    use netsmith_core::{Design, Net, Part, PinRef};

    use crate::testlib::fixtures;
    use netsmith_pcb::board::Board;

    use super::*;

    #[test]
    fn a_shortfall_that_is_real_is_never_shown_as_zero() {
        assert_eq!(short(0.8), "0.80");
        assert_eq!(short(0.05), "0.05");
        assert_eq!(short(0.004), "0.004");
        assert_eq!(short(0.0004), "0.0004");
        // Below what the placement check itself ignores there is nothing to show.
        assert_eq!(short(2e-6), "<0.0001");
        for v in [0.0123, 0.0049, 0.00051, 0.00011] {
            assert!(
                short(v).parse::<f64>().is_ok_and(|shown| shown > 0.0),
                "{v} is shown as {}",
                short(v)
            );
        }
    }

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
                provides: Vec::new(),
                refdes: format!("R{}", i + 1),
                kind: "Resistor".into(),
                value: "1k".into(),
                package: Some("0603".into()),
                path: (*path).into(),
                pins: Vec::new(),
            })
            .collect();
        let design = Design {
            modules: Vec::new(),
            name: "T".into(),
            process: Some(crate::testlib::test_process()),
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts,
            nets: vec![Net {
                name: "N".into(),
                pins: vec![PinRef {
                    refdes: "R1".into(),
                    pin: "1".into(),
                }],
            }],
        };
        crate::board_from_design(&design, &netsmith_kicad::Mapping::standard(), &fixtures())
            .unwrap()
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
            carried: &[],
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
            carried: &[],
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
        bottom.set(netsmith_pcb::overhang::Edge::Bottom);
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
                carried: &[],
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
    #[test]
    fn a_part_that_stays_off_the_grid_is_measured_where_the_check_measures_it() {
        // #457: a part the requirements pin at x 97.405 mm (off the placer's 0.05 mm grid)
        // and in no relation was measured at the grid's nearest point, 0.005 mm off, so the
        // placer put a neighbour against a courtyard the check found 0.005 mm wider.
        let mut board = resistors(&["", ""]);
        board.parts[0].at = Pt::new(10.025, 7.0);
        board.parts[0].rotation = 90.0;
        let outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(20.0, 20.0));
        let fixed = [true, false];
        let problem = Problem {
            nets: &board.nets.clone(),
            rules: &board.rules.clone(),
            planes: &[],
            region: outline,
            corner_radius: 0.0,
            fixed: &fixed,
            keepouts: &[],
            zones: &[],
            carried: &[],
            cutouts: &[],
            pours: false,
            seed: 0,
            wants: &[],
            offsets: &[],
            overhangs: &[],
            outline,
        };
        let checked = board.parts[0].courtyard();
        let e = Engine::new(&mut board.parts, &problem);
        let placer = e.court(0);
        for (a, b) in [
            (placer.min.x, checked.min.x),
            (placer.max.x, checked.max.x),
            (placer.min.y, checked.min.y),
            (placer.max.y, checked.max.y),
        ] {
            assert!(
                (a - b).abs() < 1e-9,
                "the placer's courtyard {placer:?} is the check's {checked:?}"
            );
        }
    }

    /// A 30 by 10 mm board with a zone over its left 20 mm and a free strip 10 mm wide
    /// beside it. `R1` is 14 mm tall with a body 6 by 4 mm; its courtyard goes 4.5 mm past
    /// the body on one side (an edge connector's shell), so that the whole courtyard needs
    /// 10.5 mm and the strip, less the board's 2 mm wall, has 8 mm. `R2` to `R4` are small.
    /// Returns the board and the zone.
    fn lid_beside_a_strip() -> (Board, Rect) {
        let mut board = resistors(&["", "", "", ""]);
        let body = Rect::new(Pt::new(-3.0, -2.0), Pt::new(3.0, 2.0));
        let tall = &mut board.parts[0];
        tall.footprint.body = body;
        tall.footprint.body_is_courtyard = false;
        tall.footprint.courtyard = Rect::new(Pt::new(-7.5, -2.5), Pt::new(3.5, 2.5));
        tall.height = Some(14.0);
        for small in &mut board.parts[1..] {
            small.height = Some(1.0);
        }
        board.outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(30.0, 10.0));
        (board, Rect::new(Pt::new(0.0, 0.0), Pt::new(20.0, 10.0)))
    }

    fn run_beside_strip(
        board: &mut Board,
        zones: &[netsmith_pcb::placement::HeightZone],
        keepouts: &[Rect],
    ) -> bool {
        let fixed = [false; 4];
        let problem = Problem {
            nets: &board.nets.clone(),
            rules: &board.rules.clone(),
            planes: &[],
            region: board.outline,
            corner_radius: 0.0,
            fixed: &fixed,
            keepouts,
            zones,
            cutouts: &[],
            pours: false,
            seed: 0,
            wants: &[],
            offsets: &[],
            overhangs: &[],
            outline: board.outline,
            carried: &[],
        };
        anneal(&mut board.parts, &problem, 1.0).feasible
    }

    fn lid(rect: Rect) -> netsmith_pcb::placement::HeightZone {
        netsmith_pcb::placement::HeightZone {
            name: "lid".into(),
            rect,
            front: true,
            back: false,
            limit: 10.0,
        }
    }

    #[test]
    fn a_part_is_kept_out_of_a_height_limit_by_its_body_and_a_margin_not_by_its_courtyard() {
        // Owner decision 2026-11-04 (issue #29, ADR 0045 section 11). The courtyard is 10.5 mm
        // wide and the strip beside the zone has 8 mm inside the wall: the body fits, the
        // courtyard does not.
        let (mut board, zone) = lid_beside_a_strip();
        assert!(run_beside_strip(&mut board, &[lid(zone)], &[]));
        let tall = &board.parts[0];
        let body = tall.body();
        assert!(
            body.min.x >= zone.max.x + HEIGHT_ZONE_MARGIN - 1e-6,
            "the body is at x = {:.2} mm, {:.2} mm from the zone",
            body.min.x,
            body.min.x - zone.max.x
        );
        assert!(
            tall.courtyard().min.x < zone.max.x,
            "the courtyard goes into the zone, where only the body is kept out"
        );
        // What the check measures: no part of the body is in the zone.
        assert!(!body.overlaps(&zone));
        for small in &board.parts[1..] {
            assert!(!small.courtyard().overlaps(&tall.courtyard()));
        }
        // The same inputs give the same board.
        let (mut again, zone) = lid_beside_a_strip();
        assert!(run_beside_strip(&mut again, &[lid(zone)], &[]));
        let poses =
            |b: &Board| -> Vec<(Pt, f64)> { b.parts.iter().map(|p| (p.at, p.rotation)).collect() };
        assert_eq!(poses(&board), poses(&again));
    }

    #[test]
    fn a_part_that_draws_no_fabrication_outline_is_kept_out_by_its_courtyard() {
        // Its courtyard is its body, as the check reads it: no margin is added to that.
        let (mut board, zone) = lid_beside_a_strip();
        let court = board.parts[0].footprint.courtyard;
        board.parts[0].footprint.body = court;
        board.parts[0].footprint.body_is_courtyard = true;
        assert!(!run_beside_strip(&mut board, &[lid(zone)], &[]));
    }

    #[test]
    fn a_keepout_for_parts_still_keeps_the_whole_courtyard_out() {
        let (mut board, zone) = lid_beside_a_strip();
        assert!(!run_beside_strip(&mut board, &[], &[zone]));
    }

    /// Four resistors on a 26 by 12 mm board, the first (`a`) carrying a keepout of
    /// 12 by 10 mm in its own frame, from 2 mm right of its origin; the others are named `b`,
    /// `c` and `d`. Returns the board and the zone, as the annealer is told.
    fn carrier_with_a_zone(
        parts: bool,
        limit: Option<f64>,
    ) -> (Board, netsmith_pcb::placement::CarriedZone) {
        let mut board = resistors(&["a", "b", "c", "d"]);
        board.outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(26.0, 12.0));
        let zone = netsmith_pcb::placement::CarriedZone {
            name: "shadow".into(),
            part: "a".into(),
            local: Rect::new(Pt::new(2.0, -5.0), Pt::new(14.0, 5.0)),
            parts,
            front: true,
            back: true,
            limit,
        };
        board.keepouts.push(netsmith_pcb::board::BoardKeepout {
            name: zone.name.clone(),
            part: Some("a".into()),
            at: Some(zone.local.min),
            size: (12.0, 10.0),
            layers: vec!["top".into(), "bottom".into()],
            tracks: false,
            vias: false,
            copper: false,
            parts,
            max_height: limit,
            drawn: false,
        });
        (board, zone)
    }

    fn run_with_carried(
        board: &mut Board,
        carried: &[netsmith_pcb::placement::CarriedZone],
    ) -> bool {
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
            carried,
            cutouts: &[],
            pours: false,
            seed: 0,
            wants: &[],
            offsets: &[],
            overhangs: &[],
            outline: board.outline,
        };
        anneal(&mut board.parts, &problem, 1.0).feasible
    }

    #[test]
    fn a_keepout_for_parts_that_a_part_carries_keeps_the_other_parts_out_where_the_part_is() {
        // ADR 0045 section 11 (issue 28): the check measures a carried keepout against every
        // part but its carrier; the annealer must keep the parts out of it too, where the
        // carrier ends up and however it is turned.
        let (mut blind, _) = carrier_with_a_zone(true, None);
        assert!(run_with_carried(&mut blind, &[]));
        assert!(
            blind
                .check_placement()
                .iter()
                .any(|p| matches!(p, netsmith_pcb::board::PlacementProblem::InKeepout { .. })),
            "without the zone told to the placer a part stands in it, so the test says something"
        );
        let (mut board, zone) = carrier_with_a_zone(true, None);
        assert!(run_with_carried(&mut board, &[zone]));
        assert_eq!(board.check_placement(), vec![]);
    }

    #[test]
    fn a_keepout_that_a_part_on_the_back_carries_is_kept_clear_of_mirrored() {
        let (mut board, zone) = carrier_with_a_zone(true, None);
        board.parts[0].side = Side::Back;
        assert!(run_with_carried(&mut board, &[zone]));
        assert_eq!(board.check_placement(), vec![]);
    }

    #[test]
    fn a_height_limit_that_a_part_carries_keeps_out_the_parts_too_tall_for_it() {
        let tall_in_the_zone = |board: &Board| {
            board
                .check_placement()
                .iter()
                .any(|p| matches!(p, netsmith_pcb::board::PlacementProblem::TooTall { .. }))
        };
        let (mut blind, _) = carrier_with_a_zone(false, Some(3.0));
        for part in &mut blind.parts[1..] {
            part.height = Some(5.0);
        }
        assert!(run_with_carried(&mut blind, &[]));
        assert!(
            tall_in_the_zone(&blind),
            "without the zone told to the placer a 5 mm part stands in it"
        );
        let (mut board, zone) = carrier_with_a_zone(false, Some(3.0));
        for part in &mut board.parts[1..] {
            part.height = Some(5.0);
        }
        assert!(run_with_carried(&mut board, &[zone]));
        assert!(!tall_in_the_zone(&board), "{:?}", board.check_placement());
        let area = board.keepout_areas()[0].rect;
        for tall in &board.parts[1..] {
            assert!(!tall.body().overlaps(&area), "{} stands clear", tall.refdes);
        }
    }

    #[test]
    fn a_keepout_a_part_carries_is_no_reason_to_refuse_the_part_itself() {
        // The carrier is inside its own keepout and is not counted against it.
        let (mut board, zone) = carrier_with_a_zone(true, None);
        let fixed = [false; 4];
        let carried = [zone];
        let problem = Problem {
            nets: &board.nets.clone(),
            rules: &board.rules.clone(),
            planes: &[],
            region: board.outline,
            corner_radius: 0.0,
            fixed: &fixed,
            keepouts: &[],
            zones: &[],
            carried: &carried,
            cutouts: &[],
            pours: false,
            seed: 0,
            wants: &[],
            offsets: &[],
            overhangs: &[],
            outline: board.outline,
        };
        let e = Engine::new(&mut board.parts, &problem);
        assert!(e.carried_depths(0, 0, 0.0).is_empty());
        assert_eq!(e.carried_cost(0, 0).hard, 0.0);
    }
}
