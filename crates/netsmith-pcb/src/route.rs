//! A small router: A* over a grid of two copper layers, with vias.
//!
//! On a four-layer board the router draws on the two outer layers only. The inner layers are
//! planes, so a pad on a plane's net is joined to it by a via dropped beside the pad and a
//! short track from the pad to the via; that is all such a pad needs.
//!
//! The grid is 0.1 mm. A cell is open to a track of a net only if the track's copper, and
//! the clearance round it, stays off every pad, track and via of other nets and off the
//! board edge. Every size and space comes from the board's [`Rules`].
//!
//! No track bends by more than 45 degrees (issue #415). The first pass of each try, which gives
//! most lines the copper they keep, searches in eight directions: a state remembers the
//! direction it came in, a turn of 45 degrees costs [`TURN`] and a right angle [`TURN_RIGHT`],
//! dearer than two bends, and an acute angle or a step back is no move. A diagonal step costs
//! [`DIAGONAL`] against [`STEP`] (14 against 10, the length), passes between the two cells
//! beside it, which must both be open, and the heuristic is the octile distance. The repairs
//! after it (rip-ups, negotiation, the chain) make four fifths of the searches on a dense board
//! and search in four directions, as the router did before. The exact pass over the copper
//! ([`Router::settled`]) then cuts every right angle into two bends of 45 degrees, a diagonal
//! from 0.3 mm down to 0.05 mm from the corner ([`CHAMFERS`]), the farthest that keeps the
//! rules' space; where none does, the line is searched for again in eight directions with
//! right angles that are cut, and last in eight with no right angle at all, and a line that
//! still has none is left, never drawn with the corner. Two places turn more sharply, where
//! the copper shows no corner: inside a pad of the line's own net (the square a round pad of
//! that size holds; also where corners are not cut), and the short stub from a pad's centre to
//! its cell. A via is a state of its own, from which the path may not take a via back at once,
//! so two vias at one place cannot hide a corner. The search queue breaks ties of cost toward
//! the state that has come farthest. The design rules check's `track_angle` (netsmith's own)
//! refuses a corner of more than 45 degrees outside a pad or a via of the net.
//!
//! Measured (2026-11-11): eight directions in every search left RP2350's 61 thousand searches
//! popping 3 billion states, against 853 million in four, and hit the route timeout; with eight
//! in the first pass only, RP2350 routes in 92 s, Orbit in 11 s and the Easyduino in 5 s, all
//! completely, with no right angle (they had 270, 171 and 154), and 1.49, 1.28 and 1.15 times
//! their spanning length against 1.55, 1.41 and 1.45 before.
//!
//! Lines are routed shortest first, except that a line that ends on a pad deep in a
//! fine-pitch grid of pads, which has the fewest ways out, comes before the others (see
//! [`pad_depth`]). A line that cannot be routed is then tried again by ripping up the nets
//! that stand in its way, routing it, and routing those again; lines still left are negotiated
//! for, and then each is given a bounded chain of rip-ups of single lines, each of which may
//! rip up the lines in its own way ([`Router::rip_up_chain`]). If lines are still left, the
//! whole board is routed again with the lines in another order (shortest first, longest
//! first, by net, in reverse net order), at most [`MOST_TRIES`] times in all. The first
//! complete result is kept, or failing that the one that leaves fewest lines. There is no
//! push-and-shove, so a board can still end partly routed, and what is left is returned.
//!
//! The work is bounded. A line is searched for in a window round its pads first, and in a
//! larger one only if that finds nothing. Before a search, a cheap flood from both ends finds
//! out whether the ends are joined at all, so a pad that its neighbours wall in is given up on
//! at once instead of after a search of the whole board. The router makes a fixed number of
//! tries and rip-up passes, and stops at the time limit in [`RouteOptions`], returning what is
//! left.
//!
//! # Why the router is shaped so
//!
//! This is ADR 0031, accepted after the owner's review of 2026-11-04. The parts that were
//! provisional (the rip-up chain, below) are accepted as built by the owner, 2026-11-11.
//!
//! A real board has a module whose pads are 0.6 mm squares on a 1.1 mm pitch, and parts on both
//! faces. The first router left the pads inside the module unrouted: it kept half a cell and a
//! margin beyond the rules from every obstacle, which closed the 0.5 mm between two pads, and a
//! track with its two spaces needs 0.3 mm. A line is routed in these steps, each paid for only
//! by the lines the step before left:
//!
//! 1. It is searched for with the old room (the rules, [`MARGIN`] of 0.05 mm and half a 0.1 mm
//!    cell).
//! 2. A line left is tried again with 0.02 mm beyond the rules ([`ROOM_TIGHT`]), which a track
//!    between two open cells and a stub from a pad's centre to the nearest cell stay inside
//!    of. A board that routes at the old room routes as it did.
//! 3. A pad still walled in is reached by a **via in the pad**, if the process allows it
//!    (`.via_in_pad`, the largest via the fabricator fills with epoxy and caps with plating).
//!    The via is at the pad's centre, no wider than the pad's smaller side, and is checked at
//!    that exact place against all other copper and holes. A path that starts or ends on a
//!    pad's far layer pays 60 mm of track for it, so it is the last resort. A pad on a plane
//!    net that cannot get out gets the via and no track.
//! 4. Where the board draws a track wider than the process's least, the tight try also looks
//!    for cells where a track of the least width fits, and a track through them is **necked
//!    down** to the least width there and full width elsewhere.
//! 5. Lines still left are **negotiated** for (PathFinder in a small form): each is routed
//!    where it would go if no other net were in the way, the nets it comes near are ripped up,
//!    the cells they fought over cost more from then on, and what was ripped up is routed
//!    again. Six rounds, and the best state is kept. The earlier rip-up put the nets back if
//!    the line did not fit; this one lets lines fail for a round so the others can move.
//!
//! There is no fan-out stage that puts a via beside each pad of a dense footprint before
//! routing: the search already puts a via on any free cell next to a pad, and a pad with no
//! free cell beside it is the case a via in the pad is for. A fan-out stage was tried on a
//! branch (PR #85, not merged; it was `RouteOptions::fan_out`, off by default):
//! each pad of a fine-pitch part (at least 8 surface-mount pads, its
//! box narrower than a via with the process's space on each side, another pad nearer than
//! that) got a fixed track to the nearest cell where a via may stand, deepest pads first. On
//! the invented tight board below it was worse, so it was not kept: 89 lines left over 11
//! placed seeds against 41 with no stage, and 53 for the inner ring only.
//! Each fixed escape is a choice the router no longer makes: the nearest cell for a via is not
//! the end that suits the line. Not tried: an escape chosen with the line it serves, a
//! reservation the negotiation may take back, and push and shove.
//!
//! What came after, each measured on invented boards (nothing here is a real design):
//!
//! - **The nets least in the way.** The search for the nets to rip up ([`Router::contested`])
//!   crosses other nets' copper at a price ([`SOFT`], 100 a cell, a via 300) and not at none,
//!   so only the nets on the path with the fewest crossings are ripped up. Before, the
//!   negotiation ripped up every net its ideal path came near, 12 to 20 nets for a line
//!   crossing the module's neighbourhood.
//! - **Fixed copper counted once** (`Base`). The masks stamped every pad, edge and keepout for
//!   each of 30 000 to 40 000 searches of a hard board; the fixed obstacles are counted once
//!   for each room and track width and a net's own pads are taken away again for that net.
//! - **Lines that end deep in a fine-pitch grid go first** ([`pad_depth`]: at least eight
//!   surface-mount pads, a gap under a via and two spaces, counted in pitches from the outside
//!   of the part's pads). Such a pad has few ways out and a short line of a part beside the
//!   grid can take the gap it needs. A board with no such grid has the old first order.
//! - **A last chain of rip-ups of single lines** ([`Router::rip_up_chain`]), after the
//!   negotiation of each try, for each line still left. A net's other lines stay where they
//!   are, and lines routed for a line above are locked below it (a lock, not a reservation:
//!   this ends a cycle of two lines). Bounded by [`CHAIN_SEARCHES`] 600 for a line and
//!   [`CHAIN_TOTAL`] 3000 for a try of the board, depth 4, whatever the time limit. Nothing in
//!   it is random, ties go by line number, and the same board gives the same copper.
//! - **`.kicad_pro` gives the Default net class a `priority`.** Without it KiCad 9 ignored the
//!   Default class's clearance and checked every board against its own 0.2 mm (measured: a
//!   0.1 mm project reported "clearance 0.2000 mm"). One added line in every `.kicad_pro`.
//! - **`fab` with connections left** writes the board file, the project file, the Gerbers, the
//!   bill of materials, the placement file and `fab-notes.txt` with an `ERROR: INCOMPLETE`
//!   line at its head and one `ERROR: not routed` line for each connection, and
//!   `routing-failure.txt` (each connection with its net, its two pads and their places, and
//!   the parts most of them end on). The exit status is an error as before.
//!
//! The boards. `tight_board` (`tests/common/mod.rs`): 99 parts (80 0402 resistors and
//! capacitors, 18 SOT-23 regulators, half of each on the back) round one module of 80 pads,
//! 0.6 mm square on a 1.1 mm pitch in two rings round a hollow centre, four layers, 106 nets,
//! squeezed by the placer into 27 x 25 mm; at 50 x 42 mm it routes completely in under a
//! second. `mixed_board` (`tests/mixed.rs`): 101 parts on 35 x 60 mm, a QFN of 48 pads and one
//! of 32 at 0.5 mm pitch with exposed pads, parts at most 4 mm high, both faces. Both are
//! measured by `tight::measured` and `mixed::measured` (`--ignored --nocapture`, with
//! `NETSMITH_AREA` and `NETSMITH_SEEDS`). Lines left, seeds 0 to 11, release build:
//!
//! | | tight, 27 x 25 mm | mixed, 35 x 60 mm |
//! |---|---|---|
//! | before ADR 0031 | 2 of 11 seeds done, 46 lines left | 0 of 12, 96 left |
//! | the nets least in the way | 2 of 11, 41 left | |
//! | the placer keeps the room two busy parts need (`anneal.rs`) | unchanged | 1 of 12, 28 left |
//! | the chain of single-line rip-ups | 8 of 12, 8 left | 12 of 12, 0 left (24 seeds, 12 not used to choose anything) |
//!
//! On the invented board a change of one or two lines on one seed is no signal (another order
//! of lines changes a seed by plus or minus two or three); 89 against 41 over eleven seeds is.
//! Every board passes the design rules check on what it routed.
//!
//! **Tried and not kept.** Starting the next try with the lines the last left: no seed
//! changed, the time doubled. A finer grid (0.05 mm cells, margin 0.02 mm): 3 seeds left 0, 2,
//! 2 against 1, 5, 2 for the coarse grid at twice the time, and it changes every existing
//! routed board. In the placer, a soft cost for a part on the other face under a fine-pitch
//! grid: 65 lines left against 46. More negotiation rounds (50 against 6) help by chance, not
//! by design, and the cost of a history cell (10 to 400) made no difference; forty rounds,
//! another restart radius (2.5, 5 mm), ripping up only the lines whose copper the cheapest
//! path comes near: no gain. Constants of the search (via 60 or 300 against 150, turn 8 or 60
//! against 25, soft crossing 40 or 300 against 100): none is a signal. A soft cost in front of
//! fine-pitch pads: 71 to 66 and 64 lines, since the cause is the room between the parts. A
//! first form of the chain that ripped up everything within 0.35 mm of the path, layer-blind,
//! left 3 lines; depth 5 and 1000 searches left 7 of a different form: the set-by-set order
//! (crossed lines, then within 0.1 mm, then 0.25 mm, then each alone) is not a measured optimum
//! but the form that did not depend on one threshold.
//!
//! **What is in the way** (read from cell maps, not proved). Every line left can be routed if
//! all other nets' copper is removed, and one or two nets near its pad are enough to free it,
//! but those nets then cannot be routed again (cycles of two or three nets). The gaps between
//! pads take one track each and no via fits in a gap, so a pad escapes by a track in a gap or
//! a via in the pad to the other face, where the space under the module is full. A pad of the
//! outer ring next to the board edge can be sealed in a pocket by two other nets' vias, one on
//! each side of the pad.
//!
//! **Choices** (the conservative one is what is built, accepted as built by the owner,
//! 2026-11-11):
//!
//! - Rip up single lines, not nets. A net's other lines stay where they are. The alternative,
//!   nets as before, keeps the connection of a net's pads together but ripped up 3 to 20 lines
//!   at a time here.
//! - Depth 4, 600 searches a line, 3000 a try. The numbers are tuned on the two invented boards
//!   over 12 seeds, and 12 more seeds of the first that were not used for tuning came out the
//!   same. A deeper chain finishes more of the tight board's last lines at more time; the
//!   bound exists so that time stays bounded whatever the board, and a lower one gives back
//!   some of the gain.
//! - A line of this chain is a lock, not a reservation. A push and shove router would move
//!   their copper a little and keep them routed: more code (a track moved must keep its
//!   clearance to everything beside it), and not needed on the board that measured it.
//!
//! **Not done** (issue #3 is open): the invented tight board in 27 x 25 mm still leaves 1
//! to 3 lines on the module on 4 of 12 seeds. A router that can move a track (push and shove),
//! or reserve a pad's way out before the others are routed without fixing where it goes, and
//! the placer weighing the capacity of the strip between a module and the edge, are not done.
//! A board of this density with a fine-pitch grid next to an edge is the case none of this
//! reaches.

use std::cell::{Cell, RefCell};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::board::{Board, CopperLayer, Track, Via};
use crate::geometry::{Pt, Rect};
use crate::ratsnest::{Ratline, Ratsnest};

/// The distance between the cells' centres, in mm.
const PITCH: f64 = 0.1;
/// How much nearer than the rules an exact measurement of finished copper may come, which is
/// only the rounding of the numbers.
const EXACT_SLACK: f64 = 1e-6;
/// Space kept over the rules, which covers the short stub from a pad's centre to the
/// nearest cell.
const MARGIN: f64 = 0.05;
/// How many times the nets in the way of a line are ripped up for it.
const RIP_UP_PASSES: usize = 3;
/// How much room a track keeps beyond the rules, as a mask is made: half a cell, which covers
/// a track running between two cells that are both open, and the margin on every obstacle.
const ROOM_LOOSE: f64 = PITCH / 2.0;
/// The least room that is still safe: a track between two open cells can come nearer an
/// obstacle than either cell by a few thousandths of a mm, and a stub from a pad's centre to
/// the nearest cell is inside the pad. A line that cannot be routed with the room above is
/// tried again with this, which opens the gaps between fine-pitch pads (a 0.6 mm pad on a
/// 1.1 mm pitch leaves 0.5 mm, and a track with its spaces needs 0.3 mm).
const ROOM_TIGHT: f64 = 0.02 - MARGIN;

/// How far round the ends of a line, in mm, its first searches are confined to; then the
/// whole board.
const WINDOW_MARGINS: [f64; 2] = [4.0, 16.0];
/// How often progress is reported while routing, at most.
const REPORT_EVERY: Duration = Duration::from_secs(2);

/// How many rounds of negotiation the lines left over get, and what a cell the lines fought
/// over costs for each time they did, and the most it counts.
const NEGOTIATION_ROUNDS: usize = 6;

const HISTORY_COST: i64 = 40;
const HISTORY_MOST: u8 = 25;

/// The last resort for a line that is still left ([`Router::rip_up_chain`]): how many levels of
/// lines ripped up for lines ripped up it goes down, how many searches it may spend on one
/// line and on all lines of one try of the board, and how far from the path (mm beyond the
/// least the masks keep) the copper of a line may be for the line to be ripped up: first the
/// lines the path crosses, then those within 0.1 mm of it, then within 0.25 mm.
const CHAIN_DEPTH: usize = 4;
const CHAIN_SEARCHES: usize = 600;
const CHAIN_TOTAL: usize = 3000;
const CHAIN_NEAR: [f64; 3] = [0.0, 0.1, 0.25];

/// The most times the whole board is routed, each with the lines in another order: the lines
/// of a fine-pitch grid first, shortest first, longest first, by net and in reverse net order.
/// A board that routes on the first order is routed once; the fifth order (reverse net order)
/// was dropped while there were four, and it finishes the invented mixed board on seed 2 since
/// the via became 0.2 in 0.45 mm (issue #431), so it costs time only on a board that the other
/// four leave unfinished.
const MOST_TRIES: usize = 5;

/// What a move costs, in tenths of a millimetre of track, straight and diagonal.
const STEP: i64 = 10;
const DIAGONAL: i64 = 14;
const TURN: i64 = 25;
/// What a right-angle turn costs, which the router cuts into two bends of 45 degrees after the
/// search: dearer than two of them, so that a path bends by 45 degrees where it can.
const TURN_RIGHT: i64 = 60;
const VIA: i64 = 150;
/// A via standing in a pad costs a process step the fabricator charges for, so a path uses
/// one only when no path without is shorter than this much more: 60 mm of track.
const VIA_IN_PAD: i64 = 600;
/// What it costs to put a track, or a via, through a cell another net's copper keeps it
/// from, when the router looks for the nets least in the way of a line: a track crosses a
/// few cells of another track, so crossing one costs several times this.
const SOFT: i64 = 100;
const SOFT_VIA: i64 = 300;
/// A step through a cell where the track has to be narrower costs this much more: a necked
/// down track carries less and is harder to make, so it is kept short.
const NECK: i64 = 30;
/// How far from the pads of a part that one of a line's ends is on, in mm (from the box round
/// the centres of the part's pads), a track may be necked down to the process's least width
/// whatever its current: the escape from a fine-pitch part, whose pads are no wider than the
/// neck and carry the same current into it. Elsewhere a track is no narrower than its net's
/// current needs (issue #431).
pub const NECK_REACH: f64 = 1.0;

const FRONT: usize = 0;
const BACK: usize = 1;
/// The four directions a flood ([`link`]) spreads in.
const MOVES: [(isize, isize); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
/// The eight directions a path moves in, in turn round the compass (issue #431, #415): a step
/// from one to the next is a bend of 45 degrees, the only bend a path makes. Then "none", for
/// the first step of a path, and "through a via", for the first step on the other layer, from
/// which a path may not take a via back.
const MOVES8: [(isize, isize); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];
const NO_DIRECTION: usize = 8;
const AFTER_VIA: usize = 9;
/// The directions a search state remembers.
const DIRECTIONS: usize = 10;

fn layer_of(index: usize) -> CopperLayer {
    if index == FRONT {
        CopperLayer::Front
    } else {
        CopperLayer::Back
    }
}

/// Something that copper must keep clear of.
#[derive(Clone, Copy)]
enum Form {
    Rect(Rect),
    /// A track: its two ends, and half its width.
    Segment(Pt, Pt, f64),
    Circle(Pt, f64),
}

impl Form {
    /// How far a point is from the form's edge, zero inside it.
    fn distance(&self, p: Pt) -> f64 {
        match self {
            Form::Rect(r) => {
                let dx = (r.min.x - p.x).max(p.x - r.max.x).max(0.0);
                let dy = (r.min.y - p.y).max(p.y - r.max.y).max(0.0);
                dx.hypot(dy)
            }
            Form::Segment(a, b, half) => {
                let ab = *b - *a;
                let len2 = ab.x * ab.x + ab.y * ab.y;
                let t = if len2 == 0.0 {
                    0.0
                } else {
                    (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0)
                };
                let nearest = Pt::new(a.x + ab.x * t, a.y + ab.y * t);
                (p.distance(nearest) - half).max(0.0)
            }
            Form::Circle(c, radius) => (p.distance(*c) - radius).max(0.0),
        }
    }

    /// How far a track's centre line from `a` to `b` is from the form's edge, zero if it
    /// touches or crosses the form. Exact, not on the grid.
    fn distance_to_line(&self, a: Pt, b: Pt) -> f64 {
        match self {
            Form::Rect(r) => {
                let inside =
                    |p: Pt| r.min.x <= p.x && p.x <= r.max.x && r.min.y <= p.y && p.y <= r.max.y;
                if inside(a) || inside(b) {
                    return 0.0;
                }
                let corners = [
                    r.min,
                    Pt::new(r.max.x, r.min.y),
                    r.max,
                    Pt::new(r.min.x, r.max.y),
                ];
                (0..4)
                    .map(|k| line_gap(a, b, corners[k], corners[(k + 1) % 4]))
                    .fold(f64::MAX, f64::min)
            }
            Form::Segment(c, d, half) => (line_gap(a, b, *c, *d) - half).max(0.0),
            Form::Circle(c, radius) => (point_to_line(*c, a, b) - radius).max(0.0),
        }
    }

    fn bounds(&self) -> Rect {
        match self {
            Form::Rect(r) => *r,
            Form::Segment(a, b, half) => Rect::new(
                Pt::new(a.x.min(b.x), a.y.min(b.y)),
                Pt::new(a.x.max(b.x), a.y.max(b.y)),
            )
            .expanded(*half),
            Form::Circle(c, r) => Rect::new(*c, *c).expanded(*r),
        }
    }
}

/// How far a point is from the segment from `a` to `b`.
fn point_to_line(p: Pt, a: Pt, b: Pt) -> f64 {
    let ab = b - a;
    let len2 = ab.x * ab.x + ab.y * ab.y;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0)
    };
    p.distance(Pt::new(a.x + ab.x * t, a.y + ab.y * t))
}

/// The least distance between the segments `a`-`b` and `c`-`d`, zero if they cross.
fn line_gap(a: Pt, b: Pt, c: Pt, d: Pt) -> f64 {
    let side = |p: Pt, q: Pt, r: Pt| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2) = (side(a, b, c), side(a, b, d));
    let (d3, d4) = (side(c, d, a), side(c, d, b));
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
        return 0.0;
    }
    [
        point_to_line(a, c, d),
        point_to_line(b, c, d),
        point_to_line(c, a, b),
        point_to_line(d, a, b),
    ]
    .into_iter()
    .fold(f64::MAX, f64::min)
}

struct Obstacle {
    /// The net it carries, as an index into the board's nets.
    net: Option<usize>,
    layers: [bool; 2],
    form: Form,
    clearance: f64,
}

struct Grid {
    origin: Pt,
    nx: usize,
    ny: usize,
}

impl Grid {
    fn over(outline: &Rect) -> Grid {
        Grid {
            origin: outline.min,
            nx: (outline.width() / PITCH).floor() as usize + 1,
            ny: (outline.height() / PITCH).floor() as usize + 1,
        }
    }

    fn centre(&self, i: usize, j: usize) -> Pt {
        Pt::new(
            self.origin.x + i as f64 * PITCH,
            self.origin.y + j as f64 * PITCH,
        )
    }

    /// The cell whose centre is nearest.
    fn cell(&self, p: Pt) -> (usize, usize) {
        let i = ((p.x - self.origin.x) / PITCH).round().max(0.0) as usize;
        let j = ((p.y - self.origin.y) / PITCH).round().max(0.0) as usize;
        (i.min(self.nx - 1), j.min(self.ny - 1))
    }

    /// The cells of `window` within `reach` of the box.
    fn cells_near(
        &self,
        window: &Window,
        bounds: &Rect,
        reach: f64,
    ) -> impl Iterator<Item = (usize, usize)> + use<> {
        let (i0, j0) = self.cell(bounds.min.add_scalar(-reach));
        let (i1, j1) = self.cell(bounds.max.add_scalar(reach));
        let (i0, j0) = (i0.max(window.i0), j0.max(window.j0));
        let (i1, j1) = (
            i1.min(window.i0 + window.nx - 1),
            j1.min(window.j0 + window.ny - 1),
        );
        (j0..=j1).flat_map(move |j| (i0..=i1).map(move |i| (i, j)))
    }
}

/// A rectangle of cells: the part of the grid a search is confined to. Most lines join pads
/// that are near each other, so most are routed in a small window; one that cannot be is
/// tried again in a larger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Window {
    i0: usize,
    j0: usize,
    nx: usize,
    ny: usize,
}

impl Window {
    fn full(grid: &Grid) -> Window {
        Window {
            i0: 0,
            j0: 0,
            nx: grid.nx,
            ny: grid.ny,
        }
    }

    /// The cells within `margin` mm of the box that spans these spots.
    fn around(grid: &Grid, spots: &[Spot], margin: f64) -> Window {
        let cells = (margin / PITCH).ceil() as usize;
        let (mut i0, mut j0, mut i1, mut j1) = (usize::MAX, usize::MAX, 0, 0);
        for &(i, j, _) in spots {
            (i0, j0, i1, j1) = (i0.min(i), j0.min(j), i1.max(i), j1.max(j));
        }
        let (i0, j0) = (i0.saturating_sub(cells), j0.saturating_sub(cells));
        let (i1, j1) = ((i1 + cells).min(grid.nx - 1), (j1 + cells).min(grid.ny - 1));
        Window {
            i0,
            j0,
            nx: i1 - i0 + 1,
            ny: j1 - j0 + 1,
        }
    }

    fn cells(&self) -> usize {
        self.nx * self.ny
    }

    fn contains(&self, i: usize, j: usize) -> bool {
        (self.i0..self.i0 + self.nx).contains(&i) && (self.j0..self.j0 + self.ny).contains(&j)
    }

    /// The cell's place in a vector over the window.
    fn index(&self, i: usize, j: usize) -> usize {
        (j - self.j0) * self.nx + (i - self.i0)
    }

    fn covers(&self, grid: &Grid) -> bool {
        *self == Window::full(grid)
    }
}

impl Pt {
    fn add_scalar(self, by: f64) -> Pt {
        Pt::new(self.x + by, self.y + by)
    }
}

/// Which cells of a window a track, or a via, of one net may not use.
struct Masks {
    window: Window,
    track: [Vec<bool>; 2],
    via: Vec<bool>,
    /// The cells a track at its full width may not use, when `track` is for the narrowest
    /// track the process makes: a track through a cell of this is necked down there. `None`
    /// when `track` is for the full width.
    wide: Option<[Vec<bool>; 2]>,
    /// Of the necked cells, those near the line's own pads where only the process's least track
    /// fits: elsewhere a necked track is the width its current needs. `None` when the two are
    /// one width.
    pinch: Option<[Vec<bool>; 2]>,
    /// Where a via could stand before [`Router::room_for_vias`] kept only the cells with room
    /// for a cluster: where the others of a cluster stand. `None` when it did not run.
    via_alone: Option<Vec<bool>>,
    /// The cells inside a pad of the line's own net, by layer, where a path may turn by more
    /// than 45 degrees: the corner is inside the pad's copper (issue #415). Empty when unset.
    free_turn: [Vec<bool>; 2],
    /// How often lines have fought over each cell while the router negotiated, which makes
    /// a track through it dearer. Empty when it is not negotiating.
    history: [Vec<u8>; 2],
    /// Cells that other nets' routed copper keeps tracks and vias from, which a search for
    /// the nets least in the way may cross at a price.
    soft: Option<Soft>,
}

struct Soft {
    track: [Vec<bool>; 2],
    via: Vec<bool>,
}

impl Masks {
    /// Nothing in the way anywhere in the window.
    fn open(window: Window) -> Masks {
        Masks {
            window,
            track: [vec![false; window.cells()], vec![false; window.cells()]],
            via: vec![false; window.cells()],
            wide: None,
            pinch: None,
            via_alone: None,
            free_turn: [Vec::new(), Vec::new()],
            history: [Vec::new(), Vec::new()],
            soft: None,
        }
    }

    fn at(&self, i: usize, j: usize) -> usize {
        self.window.index(i, j)
    }

    /// Whether a path may turn by more than 45 degrees on the cell: it is inside a pad of the
    /// line's net.
    fn turns_freely(&self, layer: usize, i: usize, j: usize) -> bool {
        self.free_turn[layer]
            .get(self.at(i, j))
            .copied()
            .unwrap_or(false)
    }

    /// Whether a track on the cell has to be necked down.
    fn necked(&self, layer: usize, i: usize, j: usize) -> bool {
        self.wide.as_ref().is_some_and(|w| w[layer][self.at(i, j)])
    }

    /// How narrow a track on the cell is: 0 at full width, 1 necked to the width its current
    /// needs, 2 pinched to the process's least.
    fn neck(&self, layer: usize, i: usize, j: usize) -> u8 {
        if self.pinch.as_ref().is_some_and(|p| p[layer][self.at(i, j)]) {
            2
        } else {
            u8::from(self.necked(layer, i, j))
        }
    }

    /// What it costs, beyond a step, to put a track on a cell that other nets' routed copper
    /// keeps tracks from.
    fn soft_cost(&self, layer: usize, i: usize, j: usize) -> i64 {
        match &self.soft {
            Some(soft) if soft.track[layer][self.at(i, j)] => SOFT,
            _ => 0,
        }
    }

    /// The same for a via.
    fn soft_via_cost(&self, i: usize, j: usize) -> i64 {
        match &self.soft {
            Some(soft) if soft.via[self.at(i, j)] => SOFT_VIA,
            _ => 0,
        }
    }

    /// What it costs, beyond a step, to put a track on the cell.
    fn history_cost(&self, layer: usize, i: usize, j: usize) -> i64 {
        self.history[layer]
            .get(self.at(i, j))
            .map_or(0, |&h| i64::from(h) * HISTORY_COST)
    }
}

/// How far from an obstacle a mask keeps a track and a via: the room beyond the rules, and
/// the width the track is drawn at.
#[derive(Clone, Copy)]
struct Reach {
    room: f64,
    width: f64,
}

/// Everything copper must keep clear of, apart from what has been routed.
struct Obstacles {
    list: Vec<Obstacle>,
    /// Where no via may go whatever its net, and how far from the box: a via does not sit
    /// on a pad, and keeps further from the holes of through-hole pads.
    via_keepouts: Vec<(Form, f64)>,
}

/// How many obstacles keep each cell of a window from tracks on each layer, and from vias.
/// A cell is open when its count is zero. Counting is what lets one net's own pads be taken
/// away again from counts made for all of them.
struct Counts {
    window: Window,
    track: [Vec<u16>; 2],
    via: Vec<u16>,
}

impl Counts {
    fn new(window: Window) -> Counts {
        Counts {
            window,
            track: [vec![0; window.cells()], vec![0; window.cells()]],
            via: vec![0; window.cells()],
        }
    }

    /// Add one (`up`) or take away one obstacle's reach: the cells within the rules' space
    /// of it, plus the room, for a track of `width` and for a via.
    fn stamp(
        &mut self,
        grid: &Grid,
        rules: &crate::rules::Rules,
        o: &Obstacle,
        reach: Reach,
        up: bool,
    ) {
        let Reach { room, width } = reach;
        let window = self.window;
        let track_reach = o.clearance + width / 2.0 + room;
        let via_reach = o.clearance + rules.via_diameter / 2.0 + room;
        let bounds = o.form.bounds();
        let far = track_reach.max(via_reach);
        // Most obstacles are nowhere near a small window.
        let corner = |i: usize, j: usize| grid.centre(i, j);
        let seen = Rect::new(
            corner(window.i0, window.j0),
            corner(window.i0 + window.nx - 1, window.j0 + window.ny - 1),
        );
        if !seen.expanded(PITCH).overlaps(&bounds.expanded(far)) {
            return;
        }
        // Look as far as either goes: a track wider than a via reaches further than it.
        for (i, j) in grid.cells_near(&window, &bounds, far) {
            let d = o.form.distance(grid.centre(i, j));
            let at = window.index(i, j);
            let bump = |c: &mut u16| {
                if up {
                    *c += 1;
                } else {
                    *c -= 1;
                }
            };
            for layer in [FRONT, BACK] {
                if o.layers[layer] && d < track_reach {
                    bump(&mut self.track[layer][at]);
                }
            }
            if d < via_reach {
                bump(&mut self.via[at]);
            }
        }
    }

    fn stamp_keepout(&mut self, grid: &Grid, form: &Form, reach: f64) {
        for (i, j) in grid.cells_near(&self.window, &form.bounds(), reach) {
            if form.distance(grid.centre(i, j)) < reach {
                let at = self.window.index(i, j);
                self.via[at] += 1;
            }
        }
    }
}

/// What the copper that never moves keeps cells from, over the whole board, for one room
/// and track width: the board's edge, the pads, the holes and the keepouts.
struct Base {
    counts: Counts,
}

impl Base {
    fn new(board: &Board, grid: &Grid, fixed: &Obstacles, reach: Reach) -> Base {
        let mut counts = Counts::new(Window::full(grid));
        let (outline, rules) = (&board.outline, &board.rules);
        for j in 0..grid.ny {
            for i in 0..grid.nx {
                let p = grid.centre(i, j);
                if !outline.contains(&Rect::new(p, p)) {
                    let at = counts.window.index(i, j);
                    counts.track[FRONT][at] += 1;
                    counts.track[BACK][at] += 1;
                    counts.via[at] += 1;
                }
            }
        }
        for o in &fixed.list {
            counts.stamp(grid, rules, o, reach, true);
        }
        for (form, keep) in &fixed.via_keepouts {
            counts.stamp_keepout(grid, form, *keep);
        }
        Base { counts }
    }
}

/// The cells of `window` that a track, or a via, of `net` may not use: what never moves
/// (`base`, less the net's own pads, `own`), the copper routed so far of other nets
/// (`routed`), and the keep-outs of the holes of routed vias (`keepouts`).
#[allow(clippy::too_many_arguments)]
fn masks_for(
    grid: &Grid,
    window: Window,
    board: &Board,
    base: &Base,
    own: &[&Obstacle],
    routed: &[&Obstacle],
    keepouts: &[(Form, f64)],
    net: usize,
    reach: Reach,
) -> Masks {
    let rules = &board.rules;
    let mut counts = Counts::new(window);
    let from = &base.counts;
    for layer in [FRONT, BACK] {
        copy_window(&from.track[layer], &mut counts.track[layer], grid, window);
    }
    copy_window(&from.via, &mut counts.via, grid, window);
    for o in own {
        counts.stamp(grid, rules, o, reach, false);
    }
    for o in routed.iter().filter(|o| o.net != Some(net)) {
        counts.stamp(grid, rules, o, reach, true);
    }
    for (form, keep) in keepouts {
        counts.stamp_keepout(grid, form, *keep);
    }
    let mut m = Masks::open(window);
    for layer in [FRONT, BACK] {
        m.track[layer] = counts.track[layer].iter().map(|&c| c > 0).collect();
    }
    m.via = counts.via.iter().map(|&c| c > 0).collect();
    m
}

/// The rows of a window out of counts over the whole grid.
fn copy_window(whole: &[u16], out: &mut [u16], grid: &Grid, window: Window) {
    for j in 0..window.ny {
        let from = (window.j0 + j) * grid.nx + window.i0;
        out[j * window.nx..(j + 1) * window.nx].copy_from_slice(&whole[from..from + window.nx]);
    }
}

/// A cell on a layer: where a path starts or ends, and so is open whatever the masks say.
type Spot = (usize, usize, usize);

/// Cells a path may also start or end on, by way of a via in the pad: the cell of the pad's
/// other layer. Reaching one costs [`VIA_IN_PAD`] more.
#[derive(Clone, Copy, Default)]
struct Extras<'a> {
    from: &'a [Spot],
    to: &'a [Spot],
    /// Whether the path may turn by a right angle, at [`TURN_RIGHT`], for the clean-up of
    /// [`Router::settled`] to cut into two bends of 45 degrees (issue #415).
    right_angles: bool,
    /// Four directions and free right angles, as a search that only finds which nets are in a
    /// line's way does: its path is never drawn, and eight directions cost it three times the
    /// work.
    coarse: bool,
}

/// Where a path is to end.
#[derive(Clone, Copy)]
enum Goal<'a> {
    /// Any of these cells.
    Spots(&'a [Spot]),
    /// The nearest cell where a via may go: the path is to a plane, which a via reaches.
    Via,
}

/// How a search ended without a path.
#[derive(Debug, PartialEq, Eq)]
enum Miss {
    /// Every cell the search could reach was tried.
    Blocked,
    /// The search was stopped, at the time limit.
    Stopped,
}

/// Whether a path can exist at all.
#[derive(Debug, PartialEq, Eq)]
enum Link {
    /// The ends are joined by free cells, so a search will find a path.
    Joined,
    /// One end is walled in: the cells it can reach, which touch no edge of the window, do
    /// not reach the other end or a place for a via, and a larger window would not change
    /// that.
    Closed,
    /// The cells one end can reach run out, but only at the window's edge.
    Cramped,
}

/// Find out, much more cheaply than a search can, whether the ends of a line are joined by
/// cells a track may use. A search that fails has looked at everything it could reach, which
/// on a big board with one end walled in by fine-pitch pads is most of the board; this
/// looks at the walled-in end only, because it spreads from both ends a cell at a time and
/// stops when either runs out.
///
/// A cell is passable if the masks leave it open, or it is an end of the line. A move
/// between passable cells is possible both ways, so it does not matter which end is which.
fn link(masks: &Masks, from: &[Spot], goal: Goal, extras: Extras) -> Link {
    let window = masks.window;
    let cells = window.cells();
    let from_all: Vec<Spot> = from.iter().chain(extras.from).copied().collect();
    let to_all: Vec<Spot> = match goal {
        Goal::Spots(to) => to.iter().chain(extras.to).copied().collect(),
        Goal::Via => Vec::new(),
    };
    let from = &from_all[..];
    let goal = match goal {
        Goal::Spots(_) => Goal::Spots(&to_all),
        Goal::Via => Goal::Via,
    };
    let ends: Vec<Spot> = match goal {
        Goal::Spots(to) => from.iter().chain(to).copied().collect(),
        Goal::Via => from.to_vec(),
    };
    let passable =
        |(i, j, layer): Spot| !masks.track[layer][masks.at(i, j)] || ends.contains(&(i, j, layer));
    let on_edge = |(i, j, _): Spot| {
        i == window.i0
            || j == window.j0
            || i + 1 == window.i0 + window.nx
            || j + 1 == window.j0 + window.ny
    };
    // Each side: the cells it has reached (by whom: 1 for the first side, 2 for the second),
    // its queue, and whether it has reached an edge of the window.
    let mut seen = vec![0u8; cells * 2];
    let mark = |(i, j, layer): Spot| layer * cells + window.index(i, j);
    let sides: &[&[Spot]] = match goal {
        Goal::Spots(to) => &[from, to],
        Goal::Via => &[from],
    };
    let mut queues: Vec<std::collections::VecDeque<Spot>> = Vec::new();
    let mut edge = vec![false; sides.len()];
    for (side, spots) in sides.iter().enumerate() {
        let mut queue = std::collections::VecDeque::new();
        for &spot in spots.iter().filter(|s| window.contains(s.0, s.1)) {
            seen[mark(spot)] = side as u8 + 1;
            queue.push_back(spot);
        }
        queues.push(queue);
    }
    loop {
        for side in 0..sides.len() {
            let Some(spot) = queues[side].pop_front() else {
                return if edge[side] {
                    Link::Cramped
                } else {
                    Link::Closed
                };
            };
            let (i, j, layer) = spot;
            edge[side] |= on_edge(spot);
            if matches!(goal, Goal::Via) && !masks.via[masks.at(i, j)] {
                return Link::Joined;
            }
            let mut next = |n: Spot, queues: &mut Vec<std::collections::VecDeque<Spot>>| {
                let m = mark(n);
                if seen[m] == 0 {
                    seen[m] = side as u8 + 1;
                    queues[side].push_back(n);
                    false
                } else {
                    seen[m] != side as u8 + 1
                }
            };
            for (di, dj) in MOVES {
                let (ni, nj) = (i as isize + di, j as isize + dj);
                if ni < 0 || nj < 0 || !window.contains(ni as usize, nj as usize) {
                    continue;
                }
                let n = (ni as usize, nj as usize, layer);
                if passable(n) && next(n, &mut queues) {
                    return Link::Joined;
                }
            }
            let other = (i, j, 1 - layer);
            if !masks.via[masks.at(i, j)] && passable(other) && next(other, &mut queues) {
                return Link::Joined;
            }
        }
    }
}

/// How often a search looks at the clock, in cells taken from the queue.
const CLOCK_EVERY: u32 = 1 << 14;

/// The cheapest path from any of `from` to the goal, as cells with their layers, within the
/// masks' window. `stop` is asked now and then whether to give up.
fn search(
    masks: &Masks,
    from: &[Spot],
    goal: Goal,
    extras: Extras,
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Spot>, Miss> {
    let window = masks.window;
    let states = window.cells() * 2 * DIRECTIONS;
    let state = |(i, j, layer): Spot, dir: usize| {
        (layer * window.cells() + window.index(i, j)) * DIRECTIONS + dir
    };
    let unpack = |s: usize| -> (Spot, usize) {
        let dir = s % DIRECTIONS;
        let cell = s / DIRECTIONS;
        let (layer, rest) = (cell / window.cells(), cell % window.cells());
        (
            (
                window.i0 + rest % window.nx,
                window.j0 + rest / window.nx,
                layer,
            ),
            dir,
        )
    };
    let open = |spot: Spot| {
        from.contains(&spot)
            || extras.from.contains(&spot)
            || extras.to.contains(&spot)
            || matches!(goal, Goal::Spots(to) if to.contains(&spot))
    };
    let reached = |spot: Spot| match goal {
        Goal::Spots(to) => to.contains(&spot),
        Goal::Via => !masks.via[masks.at(spot.0, spot.1)],
    };
    let heuristic = |(i, j, _): Spot| -> i64 {
        match goal {
            // The octile distance: as far diagonally as both axes go, then straight.
            Goal::Spots(to) => to
                .iter()
                .map(|&(ti, tj, _)| {
                    let (a, b) = (i.abs_diff(ti) as i64, j.abs_diff(tj) as i64);
                    if extras.coarse {
                        (a + b) * STEP
                    } else {
                        a.min(b) * DIAGONAL + (a - b).abs() * STEP
                    }
                })
                .min()
                .unwrap_or(0),
            Goal::Via => 0,
        }
    };

    let mut cost = vec![i64::MAX; states];
    let mut came_from = vec![usize::MAX; states];
    // The queue orders states by their estimate, then by how far they have come, the farthest
    // first, then by when they were queued: among the many paths of one cost that eight
    // directions give, the search follows one to its end before it widens, instead of spreading
    // over all of them; the path found is still a cheapest one, and the same every time.
    let mut queue = BinaryHeap::new();
    let mut sequence = 0u64;
    for (&spot, start) in from
        .iter()
        .map(|s| (s, 0))
        .chain(extras.from.iter().map(|s| (s, VIA_IN_PAD)))
        .filter(|(s, _)| window.contains(s.0, s.1))
    {
        let s = state(spot, NO_DIRECTION);
        cost[s] = start;
        queue.push(Reverse((start + heuristic(spot), -start, sequence, s)));
        sequence += 1;
    }
    // A state that stands on a cell reached through a via in a pad is not the end of the
    // path itself; it becomes so, at the via's cost, when it comes off the queue. Those
    // ends are queued as a state number past the real ones.
    let mut finished = vec![false; states];
    let end_of = |s: usize| s - states;
    let mut taken = 0u32;
    while let Some(Reverse((_, _, _, s))) = queue.pop() {
        taken += 1;
        if taken.is_multiple_of(CLOCK_EVERY) && stop() {
            return Err(Miss::Stopped);
        }
        if s >= states {
            let mut path = Vec::new();
            let mut at = end_of(s);
            path.push(unpack(at).0);
            while came_from[at] != usize::MAX {
                at = came_from[at];
                path.push(unpack(at).0);
            }
            path.reverse();
            return Ok(path);
        }
        let (spot, dir) = unpack(s);
        if matches!(goal, Goal::Spots(_)) && extras.to.contains(&spot) && !finished[s] {
            finished[s] = true;
            queue.push(Reverse((
                cost[s] + VIA_IN_PAD,
                -(cost[s] + VIA_IN_PAD),
                sequence,
                states + s,
            )));
            sequence += 1;
        }
        if reached(spot) {
            let mut path = vec![spot];
            let mut at = s;
            while came_from[at] != usize::MAX {
                at = came_from[at];
                path.push(unpack(at).0);
            }
            path.reverse();
            return Ok(path);
        }
        let here = cost[s];
        let (i, j, layer) = spot;
        let mut relax = |next: Spot, next_dir: usize, step: i64| {
            let n = state(next, next_dir);
            if here + step < cost[n] {
                cost[n] = here + step;
                came_from[n] = s;
                queue.push(Reverse((cost[n] + heuristic(next), -cost[n], sequence, n)));
                sequence += 1;
            }
        };
        let passable = |(ci, cj): (isize, isize)| {
            ci >= 0
                && cj >= 0
                && window.contains(ci as usize, cj as usize)
                && (!masks.track[layer][masks.at(ci as usize, cj as usize)]
                    || open((ci as usize, cj as usize, layer)))
        };
        for (d, (di, dj)) in MOVES8.iter().enumerate() {
            if extras.coarse && d % 2 == 1 {
                continue;
            }
            // No bend sharper than 45 degrees: a right angle, an acute one, or back.
            let bend = if dir < 8 {
                let k = (d + 8 - dir) % 8;
                k.min(8 - k)
            } else {
                0
            };
            // Inside a pad of the net the corner is in the pad's copper, and any turn but back
            // is free.
            let free = bend < 4 && masks.turns_freely(layer, i, j);
            if bend > 1 && !free && !(bend == 2 && (extras.right_angles || extras.coarse)) {
                continue;
            }
            let (ni, nj) = (i as isize + di, j as isize + dj);
            if !passable((ni, nj)) {
                continue;
            }
            // A diagonal step passes between the two cells beside it, which must be open too.
            let diagonal = *di != 0 && *dj != 0;
            if diagonal
                && !(passable((i as isize + di, j as isize))
                    && passable((i as isize, j as isize + dj)))
            {
                continue;
            }
            let next = (ni as usize, nj as usize, layer);
            let turn = match bend {
                0 => 0,
                _ if extras.coarse || free => TURN,
                1 => TURN,
                _ => TURN_RIGHT,
            };
            let neck = if masks.necked(layer, next.0, next.1) {
                NECK
            } else {
                0
            };
            relax(
                next,
                d,
                if diagonal { DIAGONAL } else { STEP }
                    + turn
                    + neck
                    + masks.history_cost(layer, next.0, next.1)
                    + masks.soft_cost(layer, next.0, next.1),
            );
        }
        let other = 1 - layer;
        let at = masks.at(i, j);
        if dir != AFTER_VIA && !masks.via[at] && (!masks.track[other][at] || open((i, j, other))) {
            relax((i, j, other), AFTER_VIA, VIA + masks.soft_via_cost(i, j));
        }
    }
    Err(Miss::Blocked)
}

/// Straight runs through a list of cells on one layer; a single cell has none.
#[cfg(test)]
fn runs(grid: &Grid, cells: &[(usize, usize)]) -> Vec<(Pt, Pt)> {
    let none = vec![0u8; cells.len()];
    runs_with(grid, cells, &none)
        .into_iter()
        .map(|(a, b, _)| (a, b))
        .collect()
}

/// [`runs`], where a step to or from a cell of `neck` above 0 is a run of its own, marked with
/// the narrower of its two cells, so that the track is drawn narrow there and wide elsewhere.
fn runs_with(grid: &Grid, cells: &[(usize, usize)], neck: &[u8]) -> Vec<(Pt, Pt, u8)> {
    let mut out = Vec::new();
    let mut start = 0;
    let narrow = |a: usize, b: usize| neck[a].max(neck[b]);
    for k in 1..cells.len() {
        let step = |a: usize, b: usize| {
            (
                cells[b].0 as isize - cells[a].0 as isize,
                cells[b].1 as isize - cells[a].1 as isize,
                narrow(a, b),
            )
        };
        let ends = k + 1 == cells.len() || step(k - 1, k) != step(k, k + 1);
        if ends {
            out.push((
                grid.centre(cells[start].0, cells[start].1),
                grid.centre(cells[k].0, cells[k].1),
                narrow(k - 1, k),
            ));
            start = k;
        }
    }
    out
}

/// The pad a ratsnest line ends on, and the part it is of.
fn pad_of<'a>(
    board: &'a Board,
    pad: &'a crate::board::PadRef,
    at: Pt,
) -> (&'a crate::board::Placed, &'a crate::footprint::Pad) {
    let part = board
        .part(&pad.refdes)
        .expect("a ratsnest line names a part");
    let found = part
        .footprint
        .pads_numbered(&pad.number)
        .find(|p| part.pad_centre(p) == at)
        .expect("a ratsnest line ends on a pad centre");
    (part, found)
}

/// The copper layers a pad of this footprint has, and its centre.
fn pad_spots(board: &Board, pad: &crate::board::PadRef, at: Pt, grid: &Grid) -> Vec<Spot> {
    let (part, found) = pad_of(board, pad, at);
    let (front, back) = part.pad_layers(found);
    let (i, j) = grid.cell(at);
    [(front, FRONT), (back, BACK)]
        .iter()
        .filter(|(has, _)| *has)
        .map(|&(_, layer)| (i, j, layer))
        .collect()
}

/// A pad a via may stand in: a surface-mount pad of a plain shape that is wide enough for
/// the via's copper, when the process fills and caps vias that size.
#[derive(Clone, Copy, Debug)]
struct InPad {
    /// Where the via goes: the pad's centre.
    at: Pt,
    /// The pad's box, which the via stays off all the keep-outs of other pads.
    bounds: Rect,
    /// The cell of the pad's other layer, where a path through the via goes on.
    spot: Spot,
}

fn in_pad(board: &Board, pad: &crate::board::PadRef, at: Pt, grid: &Grid) -> Option<InPad> {
    let rules = &board.rules;
    if !rules.via_in_pad {
        return None;
    }
    let (part, found) = pad_of(board, pad, at);
    let (front, back) = part.pad_layers(found);
    if front == back || found.extent.is_some() || found.drill.is_some() {
        return None;
    }
    if found.width.min(found.height) < rules.via_diameter {
        return None;
    }
    let (i, j) = grid.cell(at);
    Some(InPad {
        at,
        bounds: part.pad_bounds(found),
        spot: (i, j, if front { BACK } else { FRONT }),
    })
}

/// How deep a pad stands in a fine-pitch grid of pads, in pitches from the outside of the
/// part's pads: 0 for a pad on the outer ring, and for every pad of a part that is not a
/// fine-pitch grid (fewer than eight surface-mount pads, or none within the room a via and the
/// rules' spaces need between pads). Such a pad can leave only through the gaps between its
/// neighbours, and the deeper it is the fewer ways out it has.
fn pad_depth(board: &Board, pad: &crate::board::PadRef, at: Pt) -> f64 {
    let rules = &board.rules;
    let (part, found) = pad_of(board, pad, at);
    let (front, back) = part.pad_layers(found);
    let on_face = |p: &crate::footprint::Pad| {
        let (f, b) = part.pad_layers(p);
        (front && f) || (back && b)
    };
    let pads: Vec<&crate::footprint::Pad> = part
        .footprint
        .pads
        .iter()
        .filter(|p| p.drill.is_none() && on_face(p))
        .collect();
    if pads.len() < 8 || found.drill.is_some() {
        return 0.0;
    }
    let here = part.pad_bounds(found);
    let gap = |a: &Rect, b: &Rect| {
        let dx = (a.min.x - b.max.x).max(b.min.x - a.max.x).max(0.0);
        let dy = (a.min.y - b.max.y).max(b.min.y - a.max.y).max(0.0);
        dx.hypot(dy)
    };
    let nearest = pads
        .iter()
        .filter(|p| part.pad_centre(p) != at)
        .map(|p| {
            (
                gap(&here, &part.pad_bounds(p)),
                part.pad_centre(p).distance(at),
            )
        })
        .fold((f64::MAX, f64::MAX), |m, g| (m.0.min(g.0), m.1.min(g.1)));
    if nearest.0 >= rules.via_diameter + 2.0 * rules.clearance {
        return 0.0;
    }
    let mut span = here;
    for p in &pads {
        let b = part.pad_bounds(p);
        span = Rect::new(
            Pt::new(span.min.x.min(b.min.x), span.min.y.min(b.min.y)),
            Pt::new(span.max.x.max(b.max.x), span.max.y.max(b.max.y)),
        );
    }
    let inside = (at.x - span.min.x)
        .min(span.max.x - at.x)
        .min(at.y - span.min.y)
        .min(span.max.y - at.y);
    (inside / nearest.1).round().max(0.0)
}

fn initial_obstacles(board: &Board) -> Obstacles {
    let rules = &board.rules;
    let net_of: HashMap<(&str, &str), usize> = board
        .nets
        .iter()
        .enumerate()
        .flat_map(|(n, net)| {
            net.pads
                .iter()
                .map(move |p| ((p.refdes.as_str(), p.number.as_str()), n))
        })
        .collect();
    let mut list = Vec::new();
    let mut via_keepouts = Vec::new();
    for part in &board.parts {
        for pad in &part.footprint.pads {
            let (front, back) = part.pad_layers(pad);
            if !front && !back {
                // A hole with no copper (a mounting hole, a pogo-pin pad) still keeps
                // copper and other holes away from its edge.
                if let Some((a, b, radius)) = part.hole(pad) {
                    // The page's "NPTH to track" (issue #431).
                    list.push(Obstacle {
                        net: None,
                        layers: [true, true],
                        form: Form::Segment(a, b, radius),
                        clearance: rules.clearance.max(rules.hole_clearance) + MARGIN,
                    });
                    // A via's hole keeps `hole_to_hole` from this hole's edge.
                    via_keepouts.push((
                        Form::Segment(a, b, radius),
                        rules.hole_to_hole + rules.via_drill / 2.0 + MARGIN,
                    ));
                }
                // Its opening in the solder mask, which may be wider than the hole, exposes
                // copper: a track keeps the process's space from it (issue #431).
                let mask = mask_sides(part, pad);
                if mask != [false, false] && pad.width > 0.0 && pad.height > 0.0 {
                    let form = match pad.shape {
                        crate::footprint::PadShape::Circle => {
                            Form::Circle(part.pad_centre(pad), pad.width.min(pad.height) / 2.0)
                        }
                        _ => Form::Rect(part.pad_bounds(pad)),
                    };
                    list.push(Obstacle {
                        net: None,
                        layers: mask,
                        form,
                        clearance: rules.mask_to_track + MARGIN,
                    });
                }
                continue;
            }
            let bounds = part.pad_bounds(pad);
            let pad_net = net_of
                .get(&(part.refdes.as_str(), pad.number.as_str()))
                .copied();
            // A track keeps the page's "PTH to track" from a plated pad's hole, and "NPTH to
            // track" from a hole with copper round it that is not plated (issue #431); the pad's
            // copper and its clearance usually keep more.
            if let Some((a, b, radius)) = part.hole(pad) {
                let keep = if pad.kind == crate::footprint::PadKind::NpThruHole {
                    rules.hole_clearance
                } else {
                    rules.plated_hole_clearance
                };
                list.push(Obstacle {
                    net: pad_net,
                    layers: [true, true],
                    form: Form::Segment(a, b, radius),
                    clearance: keep + MARGIN,
                });
            }
            list.push(Obstacle {
                net: pad_net,
                layers: [front, back],
                form: Form::Rect(bounds),
                // The larger of the board's clearance and the pad's own, as the design
                // rules check applies it.
                clearance: rules.clearance.max(pad.clearance.unwrap_or(0.0)) + MARGIN,
            });
            // A via keeps off every pad, and keeps its hole clear of a pad's hole.
            let reach = match pad.drill {
                Some(drill) => {
                    // The via's hole keeps `hole_to_hole` from the pad's hole, measured from
                    // the pad's centre, which is at least half the shorter side from its edge.
                    let apart = drill.width.max(drill.height) / 2.0
                        + rules.hole_to_hole
                        + rules.via_drill / 2.0;
                    let inset = bounds.width().min(bounds.height()) / 2.0;
                    (apart - inset).max(rules.via_diameter / 2.0) + MARGIN
                }
                None => rules.via_diameter / 2.0 + PITCH / 2.0,
            };
            via_keepouts.push((Form::Rect(bounds), reach));
        }
    }
    // An opening the footprint draws in the solder mask exposes any copper in it, so a via
    // (whose own hole the mask may not cover) keeps the mask's least web from it.
    for part in &board.parts {
        for opening in part.mask_openings() {
            via_keepouts.push((
                Form::Rect(opening),
                rules.via_diameter / 2.0 + rules.mask_bridge + MARGIN,
            ));
        }
    }
    // Keepouts: tracks keep out of the areas that forbid them on a layer (and, because a
    // track's cells also bound vias here, vias keep out of them too), and vias of the
    // areas that forbid them.
    for k in board.keepout_areas() {
        let layers = [k.on(CopperLayer::Front), k.on(CopperLayer::Back)];
        if k.tracks && layers != [false, false] {
            list.push(Obstacle {
                net: None,
                layers,
                form: Form::Rect(k.rect),
                clearance: MARGIN,
            });
        }
        if k.vias {
            via_keepouts.push((Form::Rect(k.rect), rules.via_diameter / 2.0 + PITCH / 2.0));
        }
    }
    // The board edge: the outline, with its rounded corners, and the edge of each cutout.
    let edges = std::iter::once(board.outline_shape())
        .chain(board.cutout_shapes().into_iter().map(|(_, shape)| shape));
    for (from, to) in edges.flat_map(|shape| shape.segments()) {
        list.push(Obstacle {
            net: None,
            layers: [true, true],
            form: Form::Segment(from, to, 0.0),
            clearance: rules.copper_to_edge + MARGIN,
        });
    }
    Obstacles { list, via_keepouts }
}

/// Why a line's copper was not kept: it comes too near other copper, or a right angle has no
/// cut into two bends of 45 degrees that keeps the rules' space.
enum Unsettled {
    Clearance,
    Corner,
}

/// How far from a right angle, in mm, the clean-up tries to cut it, the farthest first.
const CHAMFERS: [f64; 4] = [0.3, 0.2, 0.1, 0.05];

/// The faces whose solder mask a pad opens, on the board: front first.
fn mask_sides(part: &crate::board::Placed, pad: &crate::footprint::Pad) -> [bool; 2] {
    let has = |name: &str| pad.layers.iter().any(|l| l == name || l == "*.Mask");
    let (front, back) = (has("F.Mask"), has("B.Mask"));
    if part.mirrored() {
        [back, front]
    } else {
        [front, back]
    }
}

/// What one ratsnest line became.
#[derive(Clone)]
struct Routed {
    line: usize,
    tracks: Vec<Track>,
    vias: Vec<Via>,
}

/// Which lines are routed and how, and which are not.
#[derive(Clone, Default)]
struct State {
    routed: Vec<Routed>,
    failed: Vec<usize>,
}

struct Router<'a> {
    board: &'a Board,
    grid: Grid,
    fixed: Obstacles,
    /// The obstacles of `fixed` that each net's pads make, as indices into its list.
    fixed_by_net: Vec<Vec<usize>>,
    /// What the fixed obstacles keep cells from, for each room and track width asked for.
    bases: RefCell<HashMap<(u64, u64), std::rc::Rc<Base>>>,
    lines: Vec<Ratline>,
    /// The net of each line, as an index into the board's nets.
    nets: Vec<usize>,
    /// The cells each line starts and ends on.
    ends: Vec<(Vec<Spot>, Vec<Spot>)>,
    /// The pads at each end of a line that a via may stand in.
    in_pads: Vec<(Option<InPad>, Option<InPad>)>,
    /// How deep in a fine-pitch grid of pads the deeper end of each line is ([`pad_depth`]).
    depth: Vec<f64>,
    /// Whether lines left over are negotiated for and given a chain of rip-ups, and what the
    /// negotiation has learnt.
    negotiate: bool,
    history: RefCell<[Vec<u8>; 2]>,
    /// Whether the router is in the first pass of a try, which searches in eight directions.
    fine: Cell<bool>,
    /// When to stop, if there is a limit.
    deadline: Option<Instant>,
    /// Whether the deadline has passed; once it has, every search gives up at once.
    stopped: Cell<bool>,
    progress: Option<fn(&str)>,
    cancel: Option<&'static AtomicBool>,
    last_report: Cell<Instant>,
}

impl<'a> Router<'a> {
    fn new(board: &'a Board, options: &RouteOptions) -> Self {
        let grid = Grid::over(&board.outline);
        let net_index: HashMap<&str, usize> = board
            .nets
            .iter()
            .enumerate()
            .map(|(i, n)| (n.name.as_str(), i))
            .collect();
        let lines = Ratsnest::of(board).lines;
        let nets = lines.iter().map(|l| net_index[l.net.as_str()]).collect();
        let ends = lines
            .iter()
            .map(|l| {
                let to = if l.to_plane {
                    Vec::new()
                } else {
                    pad_spots(board, &l.to, l.to_at, &grid)
                };
                (pad_spots(board, &l.from, l.from_at, &grid), to)
            })
            .collect();
        let in_pads = lines
            .iter()
            .map(|l| {
                let to = if l.to_plane {
                    None
                } else {
                    in_pad(board, &l.to, l.to_at, &grid)
                };
                (in_pad(board, &l.from, l.from_at, &grid), to)
            })
            .collect();
        let depth = lines
            .iter()
            .map(|l| {
                let to = if l.to_plane {
                    0.0
                } else {
                    pad_depth(board, &l.to, l.to_at)
                };
                pad_depth(board, &l.from, l.from_at).max(to)
            })
            .collect();
        let fixed = initial_obstacles(board);
        let mut fixed_by_net = vec![Vec::new(); board.nets.len()];
        for (k, o) in fixed.list.iter().enumerate() {
            if let Some(n) = o.net {
                fixed_by_net[n].push(k);
            }
        }
        Router {
            board,
            fixed,
            fixed_by_net,
            bases: RefCell::new(HashMap::new()),
            grid,
            lines,
            nets,
            ends,
            in_pads,
            depth,
            negotiate: options.negotiate,
            history: RefCell::new([Vec::new(), Vec::new()]),
            fine: Cell::new(true),
            deadline: options.time_limit.map(|t| Instant::now() + t),
            stopped: Cell::new(false),
            progress: options.progress,
            cancel: options.cancel,
            last_report: Cell::new(Instant::now()),
        }
    }

    /// The masks of a window for a net: see [`masks_for`].
    fn masks(
        &self,
        window: Window,
        net: usize,
        (routed, keepouts): (&[&Obstacle], &[(Form, f64)]),
        (soft, soft_keepouts): (&[&Obstacle], &[(Form, f64)]),
        reach: Reach,
    ) -> Masks {
        let key = (reach.room.to_bits(), reach.width.to_bits());
        let base = self
            .bases
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                std::rc::Rc::new(Base::new(self.board, &self.grid, &self.fixed, reach))
            })
            .clone();
        let own: Vec<&Obstacle> = self.fixed_by_net[net]
            .iter()
            .map(|&k| &self.fixed.list[k])
            .collect();
        let mut m = masks_for(
            &self.grid, window, self.board, &base, &own, routed, keepouts, net, reach,
        );
        if !soft.is_empty() || !soft_keepouts.is_empty() {
            // The cells only the soft copper keeps tracks and vias from.
            let mut counts = Counts::new(window);
            for o in soft.iter().filter(|o| o.net != Some(net)) {
                counts.stamp(&self.grid, &self.board.rules, o, reach, true);
            }
            for (form, keep) in soft_keepouts {
                counts.stamp_keepout(&self.grid, form, *keep);
            }
            m.soft = Some(Soft {
                track: [FRONT, BACK].map(|l| {
                    (0..window.cells())
                        .map(|c| counts.track[l][c] > 0 && !m.track[l][c])
                        .collect()
                }),
                via: (0..window.cells())
                    .map(|c| counts.via[c] > 0 && !m.via[c])
                    .collect(),
            });
        }
        m
    }

    /// The width a line's tracks are drawn at: its net's (ADR 0020).
    fn width_of(&self, line: usize) -> f64 {
        self.board.track_width(&self.lines[line].net)
    }

    /// The narrowest a line's track may be necked down to away from its pads: the width its
    /// net's current needs (issue #431), and no wider than the track itself.
    fn neck_width(&self, line: usize) -> f64 {
        self.board
            .least_width(&self.lines[line].net)
            .min(self.width_of(line))
    }

    /// For a net whose current one via does not carry, leave a cell open for a via only where
    /// the others fit beside it (issue #431, [`crate::parallel`]): enough cells a cluster's step
    /// away, on the four straight or the four diagonal directions, open for a via too.
    fn room_for_vias(&self, line: usize, masks: &mut Masks) {
        if self.vias_needed(line) <= 1 {
            return;
        }
        let w = masks.window;
        let keep: Vec<bool> = (0..w.cells())
            .map(|c| {
                masks.via[c]
                    || self
                        .cluster_cells(line, masks, w.i0 + c % w.nx, w.j0 + c / w.nx)
                        .is_none()
            })
            .collect();
        masks.via_alone = Some(std::mem::replace(&mut masks.via, keep));
    }

    /// The cells inside a pad of the net, on each layer the pad has copper on.
    fn mark_own_pads(&self, net: usize, masks: &mut Masks) {
        let w = masks.window;
        let mut free = [vec![false; w.cells()], vec![false; w.cells()]];
        // The box round a pad of any shape is more than its copper: a round pad fills only the
        // square inside it, so only that is counted, for every pad.
        for (r, layers) in self.own_pad_squares(net) {
            for (i, j) in self.grid.cells_near(&w, &r, 0.0) {
                let p = self.grid.centre(i, j);
                if r.contains(&Rect::new(p, p)) {
                    for layer in [FRONT, BACK] {
                        if layers[layer] {
                            free[layer][w.index(i, j)] = true;
                        }
                    }
                }
            }
        }
        masks.free_turn = free;
    }

    /// How many vias side by side the line's net needs where it changes layer.
    fn vias_needed(&self, line: usize) -> usize {
        let net = &self.lines[line].net;
        self.board
            .net_currents
            .iter()
            .find(|c| &c.net == net)
            .map_or(1, |c| c.vias)
    }

    /// Where the other vias of the line's cluster stand round a via at cell (i, j): a cluster's
    /// step from its centre (their copper overlaps, their holes keep the via-to-via space), on
    /// the four straight directions or else the four diagonal ones, the first whose nearest
    /// cells the masks leave open for a via, in that order. `None` if too few are open.
    fn cluster_cells(&self, line: usize, masks: &Masks, i: usize, j: usize) -> Option<Vec<Pt>> {
        let want = self.vias_needed(line).saturating_sub(1).min(4);
        let rules = &self.board.rules;
        let step = rules.via_hole_to_hole + rules.via_drill + crate::parallel::MARGIN;
        let w = masks.window;
        let centre = self.grid.centre(i, j);
        let open = |p: Pt| {
            let (a, b) = (
                ((p.x - self.grid.origin.x) / PITCH).round(),
                ((p.y - self.grid.origin.y) / PITCH).round(),
            );
            if a >= 0.0 && b >= 0.0 && w.contains(a as usize, b as usize) {
                let alone = masks.via_alone.as_ref().unwrap_or(&masks.via);
                !alone[w.index(a as usize, b as usize)]
            } else {
                false
            }
        };
        let r = std::f64::consts::FRAC_1_SQRT_2;
        for set in [
            [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)],
            [(r, r), (-r, r), (-r, -r), (r, -r)],
        ] {
            let found: Vec<Pt> = set
                .iter()
                .map(|(a, b)| Pt::new(centre.x + a * step, centre.y + b * step))
                .filter(|&p| open(p))
                .take(want)
                .collect();
            if found.len() >= want {
                return Some(found);
            }
        }
        None
    }

    /// Whether a point is within `reach` of the pads of a part an end of the line is on.
    fn near_end_by(&self, line: usize, p: Pt, reach: f64) -> bool {
        let l = &self.lines[line];
        let near = |refdes: &str| {
            self.board
                .neck_box(refdes)
                .is_some_and(|b| b.expanded(reach).contains(&Rect::new(p, p)))
        };
        near(&l.from.refdes) || (!l.to_plane && near(&l.to.refdes))
    }

    /// Whether a point is within [`NECK_REACH`] of the pads of a part an end of the line is on.
    fn near_end(&self, line: usize, p: Pt) -> bool {
        self.near_end_by(line, p, NECK_REACH)
    }

    /// For each cell of the window, whether it is within [`NECK_REACH`] of the line's pads.
    fn near_ends(&self, line: usize, window: Window) -> Vec<bool> {
        (0..window.cells())
            .map(|c| {
                let (i, j) = (window.i0 + c % window.nx, window.j0 + c / window.nx);
                // A cell a step inside the reach, so that the step out of the last pinched cell,
                // which is drawn at the pinched width, stays within it.
                self.near_end_by(line, self.grid.centre(i, j), NECK_REACH - PITCH * 1.5)
            })
            .collect()
    }

    /// Whether the time limit has passed, or the caller asked for the router to stop.
    fn out_of_time(&self) -> bool {
        // A job that was cancelled (`fab`) ends here; the flag below stops the router and
        // keeps what it routed, which is what the PCB view's "stop" asks for.
        netsmith_core::job::checkpoint();
        if !self.stopped.get()
            && (self.deadline.is_some_and(|d| Instant::now() >= d)
                || self.cancel.is_some_and(|c| c.load(Ordering::Relaxed)))
        {
            self.stopped.set(true);
        }
        self.stopped.get()
    }

    /// Tell the progress function something, if there is one. Messages with `now` set are
    /// always sent, the others at most once a second.
    fn report(&self, now: bool, message: impl FnOnce() -> String) {
        let Some(progress) = self.progress else {
            return;
        };
        if now || self.last_report.get().elapsed() >= REPORT_EVERY {
            self.last_report.set(Instant::now());
            progress(&message());
        }
    }

    /// The obstacles that routed copper makes, and the places it keeps vias from.
    fn copper_of(&self, routed: &[&Routed]) -> (Vec<Obstacle>, Vec<(Form, f64)>) {
        let rules = &self.board.rules;
        let mut list = Vec::new();
        let mut keepouts = Vec::new();
        for r in routed {
            let net = Some(self.nets[r.line]);
            for t in &r.tracks {
                list.push(Obstacle {
                    net,
                    layers: [t.layer == CopperLayer::Front, t.layer == CopperLayer::Back],
                    form: Form::Segment(t.from, t.to, t.width / 2.0),
                    clearance: rules.clearance + MARGIN,
                });
            }
            for v in &r.vias {
                list.push(Obstacle {
                    net,
                    layers: [true, true],
                    form: Form::Circle(v.at, v.diameter / 2.0),
                    clearance: rules.clearance + MARGIN,
                });
                // Another via's hole keeps its distance, whatever the net.
                keepouts.push((
                    Form::Circle(v.at, 0.0),
                    rules.via_hole_to_hole + rules.via_drill + MARGIN,
                ));
            }
        }
        (list, keepouts)
    }

    /// Try to route one line, given the lines routed so far, but leaving out the copper of
    /// the nets in `ignore`, which is as if they were ripped up.
    ///
    /// The line is searched for in a window round its ends first, which is cheap, and in a
    /// larger one only if that finds nothing.
    fn try_line(
        &self,
        line: usize,
        state: &State,
        ignore: &[usize],
        room: f64,
        soft: bool,
    ) -> Option<(Routed, Vec<Spot>)> {
        self.try_line_locked(line, state, ignore, room, soft, &[])
    }

    /// [`Self::try_line`], where the lines in `locked` keep their copper hard even in a soft
    /// search: they are not to be ripped up.
    fn try_line_locked(
        &self,
        line: usize,
        state: &State,
        ignore: &[usize],
        room: f64,
        soft: bool,
        locked: &[usize],
    ) -> Option<(Routed, Vec<Spot>)> {
        let others: Vec<&Routed> = state
            .routed
            .iter()
            .filter(|r| !ignore.contains(&self.nets[r.line]))
            .collect();
        // In a soft search the locked lines' copper stays hard; the rest is crossed at a price.
        let (fixed_others, free_others): (Vec<&Routed>, Vec<&Routed>) = if soft {
            others.iter().partition(|r| locked.contains(&r.line))
        } else {
            (others.clone(), Vec::new())
        };
        let (list, routed_keepouts) = self.copper_of(&fixed_others);
        let (soft_copper, soft_copper_keepouts) = self.copper_of(&free_others);
        // A soft search goes through the copper of other nets at a price, to find the nets
        // that are least in the way; what it finds is not copper that can be drawn.
        let obstacles: Vec<&Obstacle> = self.fixed.list.iter().chain(list.iter()).collect();
        let routed: Vec<&Obstacle> = list.iter().collect();
        let (hard, hard_keepouts): (&[&Obstacle], &[(Form, f64)]) = (&routed, &routed_keepouts);
        let soft_routed: Vec<&Obstacle> = soft_copper.iter().collect();
        let (soft_list, soft_keepouts): (&[&Obstacle], &[(Form, f64)]) =
            (&soft_routed, &soft_copper_keepouts);
        let mut keepouts = hard_keepouts.to_vec();
        keepouts.extend(self.fixed.via_keepouts.iter().copied());
        let (from, to) = &self.ends[line];
        let goal = if self.lines[line].to_plane {
            Goal::Via
        } else {
            Goal::Spots(to)
        };
        // The pads a via may stand in at the ends of the line, where the via would keep the
        // rules' space from the copper there is now.
        let net = self.nets[line];
        let usable =
            |pad: Option<InPad>| pad.filter(|p| self.via_fits(p, net, &obstacles, &keepouts));
        let (in_from, in_to) = (usable(self.in_pads[line].0), usable(self.in_pads[line].1));
        let from_extra: Vec<Spot> = in_from.iter().map(|p| p.spot).collect();
        let to_extra: Vec<Spot> = in_to.iter().map(|p| p.spot).collect();
        let extras = Extras {
            from: &from_extra,
            to: &to_extra,
            right_angles: true,
            coarse: false,
        };
        let ends: Vec<Spot> = from.iter().chain(to).copied().collect();
        // The width this line's net is drawn at.
        let track_width = self.width_of(line);
        let mut tried: Option<Window> = None;
        for margin in WINDOW_MARGINS.iter().map(|&m| Some(m)).chain([None]) {
            let window = match margin {
                Some(m) => Window::around(&self.grid, &ends, m),
                None => Window::full(&self.grid),
            };
            if tried == Some(window) {
                continue;
            }
            tried = Some(window);
            let rules = &self.board.rules;
            let mut masks = self.masks(
                window,
                net,
                (hard, hard_keepouts),
                (soft_list, soft_keepouts),
                Reach {
                    room,
                    width: track_width,
                },
            );
            // With the least room, a track that cannot pass between two pads at its full
            // width may pass necked down: to the width its current needs anywhere, and to the
            // narrowest the process makes within `NECK_REACH` of the line's own pads.
            let floor = self.neck_width(line);
            if room == ROOM_TIGHT && track_width > rules.min_track_width + 1e-9 {
                let reach = |width| Reach { room, width };
                let mut narrow = self
                    .masks(
                        window,
                        net,
                        (hard, hard_keepouts),
                        (soft_list, soft_keepouts),
                        reach(floor),
                    )
                    .track;
                if floor > rules.min_track_width + 1e-9 {
                    let thin = self
                        .masks(
                            window,
                            net,
                            (hard, hard_keepouts),
                            (soft_list, soft_keepouts),
                            reach(rules.min_track_width),
                        )
                        .track;
                    let near = self.near_ends(line, window);
                    let pinch = [FRONT, BACK].map(|l| {
                        (0..window.cells())
                            .map(|c| narrow[l][c] && !thin[l][c] && near[c])
                            .collect::<Vec<bool>>()
                    });
                    for l in [FRONT, BACK] {
                        for c in 0..window.cells() {
                            if pinch[l][c] {
                                narrow[l][c] = false;
                            }
                        }
                    }
                    masks.pinch = Some(pinch);
                }
                if floor < track_width - 1e-9 || masks.pinch.is_some() {
                    masks.wide = Some(std::mem::replace(&mut masks.track, narrow));
                }
            }
            self.room_for_vias(line, &mut masks);
            self.mark_own_pads(net, &mut masks);
            self.add_history(&mut masks);
            match link(&masks, from, goal, extras) {
                Link::Joined => {}
                Link::Closed => break,
                Link::Cramped if window.covers(&self.grid) => break,
                Link::Cramped => continue,
            }
            // First with right angles, which the clean-up cuts into two bends of 45 degrees;
            // where a cut would come too near other copper, the line is searched for again with
            // no right angle at all (issue #415).
            let mut give_up = false;
            // A repair searches first in four directions, then (where a cut does not fit) in
            // eight, both with right angles that are cut; and last in eight with none. The first
            // pass starts at eight.
            let modes: &[(bool, bool)] = if soft {
                &[(true, true)]
            } else if self.fine.get() {
                &[(true, false), (false, false)]
            } else {
                &[(true, true), (true, false), (false, false)]
            };
            for (n, &(right_angles, coarse)) in modes.iter().enumerate() {
                let last = n + 1 == modes.len();
                let extras = Extras {
                    right_angles,
                    coarse,
                    ..extras
                };
                match search(&masks, from, goal, extras, &|| self.out_of_time()) {
                    Ok(path) => {
                        let start = in_from.filter(|p| path.first() == Some(&p.spot));
                        let end = in_to.filter(|p| path.last() == Some(&p.spot));
                        let routed = self.realize(line, &path, start, end, &masks);
                        // What the grid let through is measured again on the copper itself: a
                        // line whose copper is nearer to another net's than the rules allow,
                        // even at the narrowest track, is not routed.
                        // A path that only finds the nets in the way is never drawn, so its
                        // corners are not cut.
                        match self.settled(routed, net, &obstacles, !soft) {
                            Ok(routed) => return Some((routed, path)),
                            Err(Unsettled::Corner) if !last => continue,
                            Err(_) => {
                                give_up = true;
                                break;
                            }
                        }
                    }
                    Err(Miss::Stopped) => return None,
                    Err(Miss::Blocked) if window.covers(&self.grid) => {
                        give_up = true;
                        break;
                    }
                    Err(Miss::Blocked) => break,
                }
            }
            if give_up {
                break;
            }
        }
        // A pad on a plane whose way out is blocked is joined by a via in the pad.
        if self.lines[line].to_plane
            && !self.out_of_time()
            && let Some(pad) = in_from
        {
            let rules = &self.board.rules;
            let via = Via {
                net: self.lines[line].net.clone(),
                at: pad.at,
                diameter: rules.via_diameter,
                drill: rules.via_drill,
            };
            return Some((
                Routed {
                    line,
                    tracks: Vec::new(),
                    vias: vec![via],
                },
                Vec::new(),
            ));
        }
        None
    }

    /// The line's copper as it will be drawn, checked against the copper of other nets, the
    /// pads, the holes and the board edge as they are, not as cells: a track that is nearer
    /// than the rules allow is drawn at the narrowest width the process makes instead (a stub
    /// from a fine-pitch pad, or a run from a cell the path was given at its end, for a wide
    /// net); `None` if some copper is too near whatever its width. This is what makes
    /// "routed" mean that netsmith's own design rules check finds the copper clear.
    fn settled(
        &self,
        mut routed: Routed,
        net: usize,
        obstacles: &[&Obstacle],
        cut: bool,
    ) -> Result<Routed, Unsettled> {
        let rules = &self.board.rules;
        let floor = self.neck_width(routed.line);
        let line = routed.line;
        let others: Vec<&&Obstacle> = obstacles.iter().filter(|o| o.net != Some(net)).collect();
        // The space the rules ask for: an obstacle's clearance has the router's margin on it.
        let apart = |o: &Obstacle| o.clearance - MARGIN - EXACT_SLACK;
        for track in &mut routed.tracks {
            let layer = usize::from(track.layer == CopperLayer::Back);
            let clear = |width: f64| {
                let reach = Rect::new(
                    Pt::new(track.from.x.min(track.to.x), track.from.y.min(track.to.y)),
                    Pt::new(track.from.x.max(track.to.x), track.from.y.max(track.to.y)),
                );
                others.iter().filter(|o| o.layers[layer]).all(|o| {
                    let near = reach.expanded(width / 2.0 + o.clearance);
                    !near.overlaps(&o.form.bounds())
                        || o.form.distance_to_line(track.from, track.to) - width / 2.0 >= apart(o)
                })
            };
            if !clear(track.width) {
                // Narrower, but not below what the current needs, except near the line's pads.
                let steps = ((track.from.distance(track.to) / 0.05).ceil() as usize).max(1);
                let near = (0..=steps).all(|k| {
                    let t = k as f64 / steps as f64;
                    let p = Pt::new(
                        track.from.x + (track.to.x - track.from.x) * t,
                        track.from.y + (track.to.y - track.from.y) * t,
                    );
                    self.near_end(line, p)
                });
                let narrowest = if near { rules.min_track_width } else { floor };
                let width = [floor, narrowest]
                    .into_iter()
                    .filter(|&w| w < track.width - 1e-9)
                    .find(|&w| clear(w))
                    .ok_or(Unsettled::Clearance)?;
                track.width = width;
            }
        }
        for via in &routed.vias {
            let radius = via.diameter / 2.0;
            if !others
                .iter()
                .all(|o| o.form.distance(via.at) - radius >= apart(o))
            {
                return Err(Unsettled::Clearance);
            }
        }
        if cut {
            self.cut_corners(&mut routed, net, &others, &apart)?;
        }
        Ok(routed)
    }

    /// Cut each right angle between two tracks of the line on one layer into two bends of 45
    /// degrees (issue #415): the corner is replaced by a diagonal from a point on one track to a
    /// point on the other, as far from the corner as the shorter of them allows, up to
    /// [`CHAMFERS`]' first, checked exactly against the other nets' copper. A corner inside a
    /// pad of the net is the pad's copper and stays. `Err(Unsettled::Corner)` when a corner has
    /// no cut that keeps the rules' space, for the line to be searched for with no right angle.
    fn cut_corners(
        &self,
        routed: &mut Routed,
        net: usize,
        others: &[&&Obstacle],
        apart: &dyn Fn(&Obstacle) -> f64,
    ) -> Result<(), Unsettled> {
        let pads = self.own_pad_squares(net);
        let in_pad = |p: Pt, layer: usize| {
            pads.iter()
                .any(|(r, layers)| layers[layer] && r.contains(&Rect::new(p, p)))
        };
        let clear = |a: Pt, b: Pt, width: f64, layer: usize| {
            let reach = Rect::new(
                Pt::new(a.x.min(b.x), a.y.min(b.y)),
                Pt::new(a.x.max(b.x), a.y.max(b.y)),
            );
            others.iter().filter(|o| o.layers[layer]).all(|o| {
                let near = reach.expanded(width / 2.0 + o.clearance);
                !near.overlaps(&o.form.bounds())
                    || o.form.distance_to_line(a, b) - width / 2.0 >= apart(o)
            })
        };
        let same = |p: Pt, q: Pt| p.distance(q) < 1e-6;
        let mut k = 0;
        while k < routed.tracks.len() {
            let mut cut = false;
            for m in 0..routed.tracks.len() {
                if m == k || routed.tracks[m].layer != routed.tracks[k].layer {
                    continue;
                }
                let (a, b) = (&routed.tracks[k], &routed.tracks[m]);
                // The corner: an end of `a` that is an end of `b`.
                let Some((corner, a_far, b_far)) = [(a.to, a.from), (a.from, a.to)]
                    .into_iter()
                    .find_map(|(c, af)| {
                        if same(c, b.from) {
                            Some((c, af, b.to))
                        } else if same(c, b.to) {
                            Some((c, af, b.from))
                        } else {
                            None
                        }
                    })
                else {
                    continue;
                };
                let (u, v) = (a_far - corner, b_far - corner);
                let (la, lb) = (u.x.hypot(u.y), v.x.hypot(v.y));
                if la < 1e-6 || lb < 1e-6 || ((u.x * v.x + u.y * v.y) / (la * lb)).abs() > 1e-3 {
                    continue;
                }
                let layer = usize::from(a.layer == CopperLayer::Back);
                if in_pad(corner, layer) {
                    continue;
                }
                let width = a.width.min(b.width);
                let found = CHAMFERS.iter().copied().find_map(|c| {
                    if c >= la - 1e-6 || c >= lb - 1e-6 {
                        return None;
                    }
                    let p = Pt::new(corner.x + u.x / la * c, corner.y + u.y / la * c);
                    let q = Pt::new(corner.x + v.x / lb * c, corner.y + v.y / lb * c);
                    clear(p, q, width, layer).then_some((p, q))
                });
                let Some((p, q)) = found else {
                    return Err(Unsettled::Corner);
                };
                let net_name = routed.tracks[k].net.clone();
                let layer_of_track = routed.tracks[k].layer;
                for (index, to) in [(k, p), (m, q)] {
                    let t = &mut routed.tracks[index];
                    if same(t.from, corner) {
                        t.from = to;
                    } else {
                        t.to = to;
                    }
                }
                routed.tracks.push(Track {
                    net: net_name,
                    layer: layer_of_track,
                    from: p,
                    to: q,
                    width,
                });
                cut = true;
                break;
            }
            if !cut {
                k += 1;
            }
        }
        Ok(())
    }

    /// The pads of a net, each as the square a round pad of its size holds (where a corner is
    /// inside the pad's copper whatever its shape), with the layers it has copper on.
    fn own_pad_squares(&self, net: usize) -> Vec<(Rect, [bool; 2])> {
        self.fixed_by_net[net]
            .iter()
            .filter_map(|&k| {
                let o = &self.fixed.list[k];
                let Form::Rect(r) = o.form else { return None };
                let inset =
                    r.width().min(r.height()) / 2.0 * (1.0 - std::f64::consts::FRAC_1_SQRT_2);
                let r = r.expanded(-inset);
                (r.width() > 0.0 && r.height() > 0.0).then_some((r, o.layers))
            })
            .collect()
    }

    /// Whether a via in the pad would keep the rules' space from all other copper, and its
    /// hole from other holes. The via is at a known place, so this is measured there and
    /// not on the grid.
    fn via_fits(
        &self,
        pad: &InPad,
        net: usize,
        obstacles: &[&Obstacle],
        keepouts: &[(Form, f64)],
    ) -> bool {
        let radius = self.board.rules.via_diameter / 2.0;
        obstacles
            .iter()
            .filter(|o| o.net != Some(net))
            .all(|o| o.form.distance(pad.at) >= o.clearance - MARGIN + radius + 1e-3)
            && keepouts.iter().all(|(form, reach)| match form {
                // The pad's own keep-out is what the via is allowed to stand in.
                Form::Rect(b) if *b == pad.bounds => true,
                _ => form.distance(pad.at) >= *reach,
            })
    }

    /// The tracks and vias that follow a path. A path to a plane ends in a via, which is the
    /// join to the plane.
    fn realize(
        &self,
        line: usize,
        path: &[Spot],
        in_from: Option<InPad>,
        in_to: Option<InPad>,
        masks: &Masks,
    ) -> Routed {
        let rules = &self.board.rules;
        // A track through a cell that is too tight for the full width is necked down: to the
        // width its current needs, or near its pads to the narrowest.
        let width_of = |neck: u8| match neck {
            0 => self.width_of(line),
            1 => self.neck_width(line),
            _ => rules.min_track_width,
        };
        let necked = |&(i, j, layer): &Spot| masks.neck(layer, i, j);
        let l = &self.lines[line];
        let mut tracks: Vec<Track> = Vec::new();
        let mut vias: Vec<Via> = Vec::new();
        let mut start = 0;
        for k in 1..=path.len() {
            if k < path.len() && path[k].2 == path[k - 1].2 {
                continue;
            }
            let layer = path[start].2;
            let cells: Vec<(usize, usize)> = path[start..k].iter().map(|s| (s.0, s.1)).collect();
            let neck: Vec<u8> = path[start..k].iter().map(necked).collect();
            for (a, b, narrow) in runs_with(&self.grid, &cells, &neck) {
                tracks.push(Track {
                    net: l.net.clone(),
                    layer: layer_of(layer),
                    from: a,
                    to: b,
                    width: width_of(narrow),
                });
            }
            if k < path.len() {
                vias.push(Via {
                    net: l.net.clone(),
                    at: self.grid.centre(path[k].0, path[k].1),
                    diameter: rules.via_diameter,
                    drill: rules.via_drill,
                });
                // The others of its cluster, where the masks left room (issue #431).
                for at in self
                    .cluster_cells(line, masks, path[k].0, path[k].1)
                    .unwrap_or_default()
                {
                    vias.push(Via {
                        net: l.net.clone(),
                        at,
                        diameter: rules.via_diameter,
                        drill: rules.via_drill,
                    });
                }
            }
            start = k;
        }
        let last = path[path.len() - 1];
        let mut pads = vec![(l.from_at, path[0])];
        if l.to_plane {
            vias.push(Via {
                net: l.net.clone(),
                at: self.grid.centre(last.0, last.1),
                diameter: rules.via_diameter,
                drill: rules.via_drill,
            });
        } else {
            pads.push((l.to_at, last));
        }
        // A path that begins or ends on the far layer of a pad goes through a via in the pad.
        for pad in in_from.iter().chain(&in_to) {
            vias.push(Via {
                net: l.net.clone(),
                at: pad.at,
                diameter: rules.via_diameter,
                drill: rules.via_drill,
            });
        }
        // A pad's centre is rarely on the grid, so join it to the first and last cell.
        for (pad_at, spot) in pads {
            let cell = self.grid.centre(spot.0, spot.1);
            if pad_at != cell {
                tracks.push(Track {
                    net: l.net.clone(),
                    layer: layer_of(spot.2),
                    from: pad_at,
                    to: cell,
                    width: width_of(necked(&spot)),
                });
            }
        }
        Routed { line, tracks, vias }
    }

    /// Route the lines in this order, then rip up for any that failed. `attempt` and
    /// `attempts` say which try of the board this is, for the progress messages. If the time
    /// runs out, the lines not yet tried count as failed.
    fn run(&self, order: &[usize], rip_up: bool, attempt: usize, attempts: usize) -> State {
        let mut state = State::default();
        // The first pass, which gives most lines the copper they keep, searches in eight
        // directions; the repairs after it search in four, with every right angle cut, which is
        // a fifth of the work (issue #415).
        self.fine.set(true);
        for (k, &line) in order.iter().enumerate() {
            if self.out_of_time() {
                state.failed.extend_from_slice(&order[k..]);
                return state;
            }
            self.report(false, || {
                format!(
                    "routing, try {attempt} of {attempts}: {k} of {} connections done, {} not routed",
                    order.len(),
                    state.failed.len()
                )
            });
            match self.try_line(line, &state, &[], ROOM_LOOSE, false) {
                Some((routed, _)) => state.routed.push(routed),
                None => state.failed.push(line),
            }
        }
        self.fine.set(false);
        if !rip_up {
            return state;
        }
        // First with the room the masks usually keep; what is still left is tried with the
        // least room that is safe, which opens the gaps between fine-pitch pads.
        for room in [ROOM_LOOSE, ROOM_TIGHT] {
            if room == ROOM_TIGHT {
                for line in state.failed.clone() {
                    if self.out_of_time() {
                        return state;
                    }
                    self.report(false, || {
                        format!(
                            "routing, try {attempt} of {attempts}: {} again with less room, {} not routed",
                            self.lines[line],
                            state.failed.len()
                        )
                    });
                    if let Some((routed, _)) = self.try_line(line, &state, &[], room, false) {
                        state.routed.push(routed);
                        state.failed.retain(|&f| f != line);
                    }
                }
            }
            for pass in 1..=RIP_UP_PASSES {
                let before = state.failed.len();
                for line in state.failed.clone() {
                    if self.out_of_time() {
                        return state;
                    }
                    self.report(false, || {
                        format!(
                            "routing, try {attempt} of {attempts}: ripping up for {} (pass {pass} of {RIP_UP_PASSES}), {} not routed",
                            self.lines[line], state.failed.len()
                        )
                    });
                    self.rip_up_for(line, &mut state, room);
                }
                if state.failed.is_empty() || state.failed.len() == before {
                    break;
                }
            }
            if state.failed.is_empty() {
                break;
            }
        }
        if self.negotiate && !state.failed.is_empty() {
            self.negotiate(&mut state, ROOM_TIGHT, attempt, attempts);
            if !state.failed.is_empty() {
                self.finish(&mut state, ROOM_TIGHT, attempt, attempts);
            }
        }
        state
    }

    /// Where a line would go if the copper of other nets could be crossed, at a price per
    /// cell ([`SOFT`]), and the nets whose copper that path comes near: the nets least in the
    /// line's way, not all that lie between its ends. `None` if even that has no path.
    fn contested(&self, line: usize, state: &State, room: f64) -> Option<(Vec<Spot>, Vec<usize>)> {
        let net = self.nets[line];
        let (_, path) = self.try_line(line, state, &[], room, true)?;
        let rules = &self.board.rules;
        let reach = rules.clearance + MARGIN + self.width_of(line) + PITCH;
        let mut found = Vec::new();
        for r in state.routed.iter().filter(|r| self.nets[r.line] != net) {
            let forms = r
                .tracks
                .iter()
                .map(|t| Form::Segment(t.from, t.to, t.width / 2.0))
                .chain(r.vias.iter().map(|v| Form::Circle(v.at, v.diameter / 2.0)));
            let near = forms.into_iter().any(|f| {
                path.iter()
                    .any(|&(i, j, _)| f.distance(self.grid.centre(i, j)) < reach)
            });
            let n = self.nets[r.line];
            if near && !found.contains(&n) {
                found.push(n);
            }
        }
        found.sort_unstable();
        Some((path, found))
    }

    /// [`Self::contested`] by line: where the line would go if the copper of other nets could
    /// be crossed, except the copper of the lines in `locked`, and for each other line how far
    /// its copper is from being crossed by that path, in mm (negative when the path crosses
    /// it, which is when the masks would keep the path from the cell), nearest first.
    fn contested_lines(
        &self,
        line: usize,
        state: &State,
        room: f64,
        locked: &[usize],
    ) -> Option<Vec<(usize, f64)>> {
        let net = self.nets[line];
        let (_, path) = self.try_line_locked(line, state, &[], room, true, locked)?;
        let rules = &self.board.rules;
        // The cells the masks keep a track, or a via, from, for copper of this width.
        let track_reach = rules.clearance + MARGIN + self.width_of(line) / 2.0 + room;
        let via_reach = rules.clearance + MARGIN + rules.via_diameter / 2.0 + room;
        // Where the path puts a via: the cell where it changes layer.
        let via_at: Vec<(usize, usize)> = path
            .windows(2)
            .filter(|w| w[0].2 != w[1].2)
            .map(|w| (w[0].0, w[0].1))
            .collect();
        let mut found: Vec<(usize, f64)> = Vec::new();
        for r in state
            .routed
            .iter()
            .filter(|r| self.nets[r.line] != net && !locked.contains(&r.line))
        {
            let tracks = r.tracks.iter().map(|t| {
                (
                    Form::Segment(t.from, t.to, t.width / 2.0),
                    [t.layer == CopperLayer::Front, t.layer == CopperLayer::Back],
                )
            });
            let vias = r
                .vias
                .iter()
                .map(|v| (Form::Circle(v.at, v.diameter / 2.0), [true, true]));
            let mut least = f64::MAX;
            for (f, layers) in tracks.chain(vias) {
                for &(i, j, layer) in &path {
                    if layers[layer] {
                        least = least.min(f.distance(self.grid.centre(i, j)) - track_reach);
                    }
                }
                for &(i, j) in &via_at {
                    least = least.min(f.distance(self.grid.centre(i, j)) - via_reach);
                }
            }
            if least < CHAIN_NEAR.last().copied().unwrap_or(0.0) {
                found.push((r.line, least));
            }
        }
        found.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        Some(found)
    }

    /// The last resort for a line that is still left: route it, ripping up the single lines
    /// (not whole nets) in its way, and route those again, each ripping up the lines in its
    /// way in turn, down to `CHAIN_DEPTH` levels, never touching a line that is routed for
    /// the one above. The whole attempt costs at most `CHAIN_SEARCHES` searches and changes
    /// `state` only if every line ends up routed. Nothing is random: the same board gives
    /// the same copper.
    fn rip_up_chain(
        &self,
        line: usize,
        state: &mut State,
        room: f64,
        depth: usize,
        locked: &[usize],
        budget: &mut usize,
    ) -> bool {
        if *budget < 2 || self.out_of_time() {
            return false;
        }
        *budget -= 1;
        if let Some((routed, _)) = self.try_line(line, state, &[], room, false) {
            state.routed.push(routed);
            return true;
        }
        if depth >= CHAIN_DEPTH {
            return false;
        }
        *budget -= 1;
        let Some(blockers) = self.contested_lines(line, state, room, locked) else {
            return false;
        };
        // First the lines the path crosses, then those it comes near, as one set each, and
        // then each line it crosses alone.
        let mut options: Vec<Vec<usize>> = Vec::new();
        for &near in &CHAIN_NEAR {
            let mut set: Vec<usize> = blockers
                .iter()
                .filter(|b| b.1 < near)
                .map(|b| b.0)
                .collect();
            set.sort_unstable();
            if !set.is_empty() && !options.contains(&set) {
                options.push(set);
            }
        }
        let crossed: Vec<usize> = blockers.iter().filter(|b| b.1 < 0.0).map(|b| b.0).collect();
        if crossed.len() > 1 {
            options.extend(crossed.iter().map(|&b| vec![b]));
        }
        for ripped in options.into_iter().filter(|o| !o.is_empty()) {
            if *budget == 0 || self.out_of_time() {
                return false;
            }
            let mut trial = state.clone();
            let (gone, kept): (Vec<Routed>, Vec<Routed>) = trial
                .routed
                .drain(..)
                .partition(|r| ripped.contains(&r.line));
            trial.routed = kept;
            let mut again: Vec<usize> = gone.iter().map(|r| r.line).collect();
            again.sort_by(|&a, &b| self.lines[a].length().total_cmp(&self.lines[b].length()));
            *budget -= 1;
            let Some((first, _)) = self.try_line(line, &trial, &[], room, false) else {
                continue;
            };
            trial.routed.push(first);
            let mut lock = locked.to_vec();
            lock.push(line);
            let mut ok = true;
            for l in again {
                if self.rip_up_chain(l, &mut trial, room, depth + 1, &lock, budget) {
                    lock.push(l);
                } else {
                    ok = false;
                    break;
                }
            }
            if ok {
                *state = trial;
                return true;
            }
        }
        false
    }

    /// [`Self::rip_up_chain`] for every line that is left, as far as the searches go.
    fn finish(&self, state: &mut State, room: f64, attempt: usize, attempts: usize) {
        let mut total = CHAIN_TOTAL;
        for line in state.failed.clone() {
            if self.out_of_time() || total < 2 {
                return;
            }
            self.report(false, || {
                format!(
                    "routing, try {attempt} of {attempts}: moving lines for {}, {} not routed",
                    self.lines[line],
                    state.failed.len()
                )
            });
            let mut trial = state.clone();
            let mut budget = CHAIN_SEARCHES.min(total);
            let start = budget;
            let done = self.rip_up_chain(line, &mut trial, room, 0, &[], &mut budget);
            total -= start - budget;
            if done {
                trial.failed.retain(|&f| f != line);
                *state = trial;
            }
        }
    }

    /// The nets whose copper stands in the way of a line.
    fn blockers(&self, line: usize, state: &State, room: f64) -> Vec<usize> {
        self.contested(line, state, room)
            .map_or_else(Vec::new, |(_, nets)| nets)
    }

    /// Copy what the negotiation has learnt about the cells of the window into the masks.
    fn add_history(&self, masks: &mut Masks) {
        let history = self.history.borrow();
        if history[FRONT].is_empty() {
            return;
        }
        let window = masks.window;
        for layer in [FRONT, BACK] {
            let mut cells = vec![0u8; window.cells()];
            for j in window.j0..window.j0 + window.ny {
                let row = j * self.grid.nx;
                for i in window.i0..window.i0 + window.nx {
                    cells[window.index(i, j)] = history[layer][row + i];
                }
            }
            masks.history[layer] = cells;
        }
    }

    /// Make the cells of a path dearer to every line that comes after.
    fn learn(&self, path: &[Spot]) {
        let mut history = self.history.borrow_mut();
        for &(i, j, layer) in path {
            let h = &mut history[layer][j * self.grid.nx + i];
            *h = (*h + 1).min(HISTORY_MOST);
        }
    }

    /// Lines the router could not place are negotiated for: each is routed where it would go
    /// if no other net were in the way, and the nets it then comes near are ripped up. The
    /// cells they fought over are made dearer for all, so the lines ripped up find another
    /// way round, and the ones that find none are the next to be negotiated for. This goes
    /// on for a few rounds, and the best state seen is kept.
    fn negotiate(&self, state: &mut State, room: f64, attempt: usize, attempts: usize) {
        let cells = self.grid.nx * self.grid.ny;
        *self.history.borrow_mut() = [vec![0; cells], vec![0; cells]];
        let mut best = state.clone();
        for round in 1..=NEGOTIATION_ROUNDS {
            if state.failed.is_empty() || self.out_of_time() {
                break;
            }
            for line in state.failed.clone() {
                if self.out_of_time() {
                    break;
                }
                self.report(false, || {
                    format!(
                        "routing, try {attempt} of {attempts}: negotiating for {} (round {round} of {NEGOTIATION_ROUNDS}), {} not routed",
                        self.lines[line],
                        state.failed.len()
                    )
                });
                let Some((path, blockers)) = self.contested(line, state, room) else {
                    continue;
                };
                self.learn(&path);
                let (gone, kept): (Vec<Routed>, Vec<Routed>) = state
                    .routed
                    .drain(..)
                    .partition(|r| blockers.contains(&self.nets[r.line]));
                state.routed = kept;
                let mut again: Vec<usize> = gone.iter().map(|r| r.line).collect();
                again.sort_by(|&a, &b| self.lines[a].length().total_cmp(&self.lines[b].length()));
                // The line first, and then what was ripped up for it.
                match self.try_line(line, state, &[], room, false) {
                    Some((routed, _)) => {
                        state.routed.push(routed);
                        state.failed.retain(|&f| f != line);
                    }
                    None => {
                        // Nothing came of it: put back what was taken up.
                        state.routed.extend(gone);
                        continue;
                    }
                }
                for l in again {
                    match self.try_line(l, state, &[], room, false) {
                        Some((routed, _)) => state.routed.push(routed),
                        None => state.failed.push(l),
                    }
                }
            }
            if state.failed.len() < best.failed.len() {
                best = state.clone();
            }
        }
        *state = best;
        *self.history.borrow_mut() = [Vec::new(), Vec::new()];
    }

    /// Try to get a failed line routed by ripping up the nets in its way, routing it, and
    /// routing what was ripped up again. If that does not work out, nothing changes.
    fn rip_up_for(&self, line: usize, state: &mut State, room: f64) {
        let blockers = self.blockers(line, state, room);
        let mut attempts: Vec<Vec<usize>> = vec![blockers.clone()];
        if blockers.len() > 1 {
            attempts.extend(blockers.iter().map(|&n| vec![n]));
        }
        for ripped in attempts.into_iter().filter(|a| !a.is_empty()) {
            if self.out_of_time() {
                return;
            }
            let mut trial = state.clone();
            let (gone, kept): (Vec<Routed>, Vec<Routed>) = trial
                .routed
                .drain(..)
                .partition(|r| ripped.contains(&self.nets[r.line]));
            trial.routed = kept;
            let mut again: Vec<usize> = gone.iter().map(|r| r.line).collect();
            again.sort_by(|&a, &b| self.lines[a].length().total_cmp(&self.lines[b].length()));

            let Some((first, _)) = self.try_line(line, &trial, &[], room, false) else {
                continue;
            };
            trial.routed.push(first);
            let mut ok = true;
            for l in again {
                match self.try_line(l, &trial, &[], room, false) {
                    Some((r, _)) => trial.routed.push(r),
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                trial.failed.retain(|&f| f != line);
                *state = trial;
                return;
            }
        }
    }

    /// The orders to try the lines in.
    fn orders(&self, retry: bool) -> Vec<Vec<usize>> {
        let mut shortest: Vec<usize> = (0..self.lines.len()).collect();
        shortest.sort_by(|&a, &b| self.lines[a].length().total_cmp(&self.lines[b].length()));
        // Lines that end deep in a fine-pitch grid of pads first, the deepest first, and the
        // others shortest first. It differs from `shortest` only on a board that has such a
        // grid.
        let mut constrained = shortest.clone();
        constrained.sort_by(|&a, &b| self.depth[b].total_cmp(&self.depth[a]));
        let mut orders = vec![constrained];
        if !orders.contains(&shortest) {
            orders.push(shortest.clone());
        }
        if retry {
            let mut longest = shortest.clone();
            longest.reverse();
            let mut by_net = shortest.clone();
            by_net.sort_by_key(|&l| self.nets[l]);
            let mut reversed_nets = by_net.clone();
            reversed_nets.reverse();
            for o in [longest, by_net, reversed_nets] {
                if !orders.contains(&o) {
                    orders.push(o);
                }
            }
        }
        orders.truncate(MOST_TRIES);
        orders
    }
}

/// How hard to try.
#[derive(Debug, Clone, Copy)]
pub struct RouteOptions {
    /// Route the board again with the lines in other orders if some are left.
    pub retry_orders: bool,
    /// Rip up the nets in the way of a line that failed, and route it first.
    pub rip_up: bool,
    /// Negotiate for lines that are still left after that: route each where it would go
    /// if nothing stood in its way, rip up what it comes near, make the cells they fought
    /// over dearer, and route what was ripped up again, for a few rounds; and then give each
    /// line still left a bounded chain of rip-ups of single lines ([`Router::rip_up_chain`]).
    /// Only used with `rip_up`.
    pub negotiate: bool,
    /// Give up after this long, leaving the lines not yet routed in the ratsnest. `None` is
    /// no limit.
    pub time_limit: Option<Duration>,
    /// Told what the router is doing, now and then, so that a long run is not silent.
    pub progress: Option<fn(&str)>,
    /// Stop as at the time limit when this is set, from another thread.
    pub cancel: Option<&'static AtomicBool>,
}

impl Default for RouteOptions {
    fn default() -> Self {
        Self {
            retry_orders: true,
            rip_up: true,
            negotiate: true,
            time_limit: None,
            progress: None,
            cancel: None,
        }
    }
}

/// What the router did.
#[derive(Debug, Clone, PartialEq)]
pub struct Routing {
    /// What is still unrouted, which is empty when the board is done.
    pub left: Ratsnest,
    /// The time limit passed before the router had tried everything it would have.
    pub stopped: bool,
    /// How many times the whole board was routed, each with the lines in another order.
    pub tries: usize,
}

/// Route as much of the ratsnest as the router can, adding tracks and vias to the board.
/// Returns what is still unrouted, which is empty when the board is done.
pub fn route(board: &mut Board) -> Ratsnest {
    route_with(board, RouteOptions::default()).left
}

/// [`route`], with a choice of how hard to try.
pub fn route_with(board: &mut Board, options: RouteOptions) -> Routing {
    let (best, stopped, tries) = {
        let router = Router::new(board, &options);
        let orders = router.orders(options.retry_orders);
        router.report(true, || {
            format!(
                "routing {} connections on a {} by {} mm board",
                router.lines.len(),
                board.outline.width(),
                board.outline.height()
            )
        });
        let mut best: Option<State> = None;
        let mut tries = 0;
        for order in &orders {
            tries += 1;
            let state = router.run(order, options.rip_up, tries, orders.len());
            let done = state.failed.is_empty();
            if best
                .as_ref()
                .is_none_or(|b| state.failed.len() < b.failed.len())
            {
                best = Some(state);
            }
            if done || router.out_of_time() {
                break;
            }
        }
        let stopped = router.out_of_time();
        (best.unwrap_or_default(), stopped, tries)
    };
    let mut routed = best.routed;
    routed.sort_by_key(|r| r.line);
    for r in routed {
        board.tracks.extend(r.tracks);
        board.vias.extend(r.vias);
    }
    // A via that does not carry its net's current gets others beside it (issue #431).
    crate::parallel::parallel_vias(board);
    // Pours are stitched to the other layers of their nets in what the routing left free.
    crate::stitch::stitch(board);
    Routing {
        left: Ratsnest::of(board),
        stopped,
        tries,
    }
}

#[cfg(test)]
mod tests {
    use crate::board::tests::{fixtures, four_layer_design};

    use super::*;

    /// A grid of 3 mm by 1 mm with nothing in the way.
    fn open_grid() -> (Grid, Masks) {
        let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(3.0, 1.0)));
        let masks = Masks::open(Window::full(&grid));
        (grid, masks)
    }

    /// A search that is never stopped.
    fn find(masks: &Masks, from: &[Spot], goal: Goal) -> Option<Vec<Spot>> {
        search(masks, from, goal, Extras::default(), &|| false).ok()
    }

    /// A search for the nets in a line's way, as the router makes one: in four directions.
    fn find_soft(masks: &Masks, from: &[Spot], goal: Goal) -> Option<Vec<Spot>> {
        let extras = Extras {
            coarse: true,
            ..Extras::default()
        };
        search(masks, from, goal, extras, &|| false).ok()
    }

    #[test]
    fn a_straight_path_stays_on_its_layer() {
        let (grid, masks) = open_grid();
        let path = find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])).unwrap();
        assert_eq!(path.len(), 31);
        assert!(path.iter().all(|&(_, j, layer)| j == 5 && layer == FRONT));
        let runs = runs(&grid, &path.iter().map(|s| (s.0, s.1)).collect::<Vec<_>>());
        assert_eq!(runs.len(), 1, "one straight track");
        assert_eq!(runs[0], (Pt::new(0.0, 0.5), Pt::new(3.0, 0.5)));
    }

    #[test]
    fn a_wall_on_the_front_sends_the_path_through_the_back() {
        let (grid, mut masks) = open_grid();
        for j in 0..grid.ny {
            let at = masks.at(15, j);
            masks.track[FRONT][at] = true;
        }
        let path = find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])).unwrap();
        assert!(path.iter().any(|s| s.2 == BACK));
        let changes = path.windows(2).filter(|w| w[0].2 != w[1].2).count();
        assert_eq!(changes, 2, "down to the back and up again");
    }

    #[test]
    fn a_wall_on_both_layers_cannot_be_crossed() {
        let (grid, mut masks) = open_grid();
        for j in 0..grid.ny {
            let at = masks.at(15, j);
            masks.track[FRONT][at] = true;
            let at = masks.at(15, j);
            masks.track[BACK][at] = true;
        }
        assert_eq!(
            find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])),
            None
        );
    }

    #[test]
    fn a_via_is_not_put_where_the_mask_forbids_one() {
        let (grid, mut masks) = open_grid();
        for j in 0..grid.ny {
            let at = masks.at(15, j);
            masks.track[FRONT][at] = true;
        }
        for i in 0..grid.nx {
            for j in 0..grid.ny {
                let at = masks.at(i, j);
                masks.via[at] = true;
            }
        }
        assert_eq!(
            find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])),
            None
        );
    }

    #[test]
    fn a_run_through_cells_that_need_a_narrow_track_is_a_run_of_its_own() {
        let (grid, _) = open_grid();
        let cells: Vec<(usize, usize)> = (0..10).map(|i| (i, 5)).collect();
        let mut neck = vec![0u8; 10];
        for n in &mut neck[4..7] {
            *n = 1;
        }
        let runs = runs_with(&grid, &cells, &neck);
        // Wide up to the first necked cell, narrow through it, wide again after.
        let widths: Vec<u8> = runs.iter().map(|r| r.2).collect();
        assert_eq!(widths, [0, 1, 0], "{runs:?}");
        assert_eq!(runs[0].1, runs[1].0);
        assert_eq!(runs[1].1, runs[2].0);
        assert_eq!(runs[0].0, Pt::new(0.0, 0.5));
        assert_eq!(runs[2].1, Pt::new(0.9, 0.5));
    }

    #[test]
    fn turns_cost_more_than_straight_runs() {
        let (grid, masks) = open_grid();
        // Same cost in steps either way round, so the path with one corner wins.
        let path = find(&masks, &[(0, 0, FRONT)], Goal::Spots(&[(20, 8, FRONT)])).unwrap();
        let cells: Vec<_> = path.iter().map(|s| (s.0, s.1)).collect();
        assert_eq!(runs(&grid, &cells).len(), 2);
    }

    #[test]
    fn a_path_to_a_plane_ends_at_the_nearest_cell_a_via_may_use() {
        let (grid, mut masks) = open_grid();
        // No via within three cells of the start, along either layer.
        for i in 0..3 {
            for j in 0..grid.ny {
                let at = masks.at(i, j);
                masks.via[at] = true;
            }
        }
        let path = find(&masks, &[(0, 5, FRONT)], Goal::Via).unwrap();
        assert_eq!(path.last(), Some(&(3, 5, FRONT)));
        // With a via nowhere, there is no way to the plane.
        masks.via.iter_mut().for_each(|v| *v = true);
        assert_eq!(find(&masks, &[(0, 5, FRONT)], Goal::Via), None);
    }

    #[test]
    fn a_window_round_the_ends_of_a_line_is_clipped_to_the_grid() {
        let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(10.0, 5.0)));
        let w = Window::around(&grid, &[(20, 25, FRONT), (30, 25, BACK)], 1.0);
        assert_eq!(
            w,
            Window {
                i0: 10,
                j0: 15,
                nx: 31,
                ny: 21
            }
        );
        assert!(w.contains(10, 15) && w.contains(40, 35) && !w.contains(41, 25));
        let edge = Window::around(&grid, &[(1, 1, FRONT)], 2.0);
        assert_eq!((edge.i0, edge.j0), (0, 0));
        let all = Window::around(&grid, &[(50, 25, FRONT)], 100.0);
        assert!(all.covers(&grid));
    }

    #[test]
    fn a_search_in_a_window_stays_inside_it() {
        let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(3.0, 1.0)));
        let mut masks = Masks::open(Window {
            i0: 10,
            j0: 0,
            nx: 15,
            ny: grid.ny,
        });
        // A wall on both layers across the window, with a gap only outside it is no gap.
        for j in 0..grid.ny {
            let at = masks.at(20, j);
            masks.track[FRONT][at] = true;
            masks.track[BACK][at] = true;
        }
        assert_eq!(
            find(&masks, &[(10, 5, FRONT)], Goal::Spots(&[(24, 5, FRONT)])),
            None
        );
    }

    #[test]
    fn a_pad_walled_in_is_closed_and_one_hemmed_in_by_the_window_is_cramped() {
        let (_, mut masks) = open_grid();
        let ring = |masks: &mut Masks, (ci, cj): (usize, usize)| {
            for (i, j) in [(ci - 1, cj), (ci + 1, cj), (ci, cj - 1), (ci, cj + 1)] {
                let at = masks.at(i, j);
                masks.track[FRONT][at] = true;
                masks.track[BACK][at] = true;
            }
        };
        ring(&mut masks, (15, 5));
        let from = [(15, 5, FRONT)];
        let to = [(28, 5, FRONT)];
        assert_eq!(
            link(&masks, &from, Goal::Spots(&to), Extras::default()),
            Link::Closed
        );
        // From the other end the answer is the same.
        assert_eq!(
            link(&masks, &to, Goal::Spots(&from), Extras::default()),
            Link::Closed
        );
        // Open it again and the ends are joined.
        let (_, open) = open_grid();
        assert_eq!(
            link(&open, &from, Goal::Spots(&to), Extras::default()),
            Link::Joined
        );

        // A window with a wall in it, from edge to edge: each side is hemmed in by the wall
        // and the window, which a larger window might not.
        let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(3.0, 1.0)));
        let mut cut = Masks::open(Window {
            i0: 5,
            j0: 0,
            nx: 20,
            ny: grid.ny,
        });
        for j in 0..grid.ny {
            let at = cut.at(15, j);
            cut.track[FRONT][at] = true;
            cut.track[BACK][at] = true;
        }
        assert_eq!(
            link(
                &cut,
                &[(10, 5, FRONT)],
                Goal::Spots(&[(20, 5, FRONT)]),
                Extras::default()
            ),
            Link::Cramped
        );
    }

    #[test]
    fn a_link_to_a_plane_needs_a_place_for_a_via() {
        let (_, mut masks) = open_grid();
        assert_eq!(
            link(&masks, &[(0, 5, FRONT)], Goal::Via, Extras::default()),
            Link::Joined
        );
        masks.via.iter_mut().for_each(|v| *v = true);
        assert_ne!(
            link(&masks, &[(0, 5, FRONT)], Goal::Via, Extras::default()),
            Link::Joined
        );
    }

    /// The open grid with the cell `(ci, cj)` walled in on the front: its four neighbours
    /// are closed to tracks there, and no via may be put anywhere (the way out of a pad
    /// that has no room beside it).
    fn walled_front(ci: usize, cj: usize) -> Masks {
        let (_, mut masks) = open_grid();
        masks.via.iter_mut().for_each(|v| *v = true);
        for (i, j) in [(ci - 1, cj), (ci + 1, cj), (ci, cj - 1), (ci, cj + 1)] {
            let at = masks.at(i, j);
            masks.track[FRONT][at] = true;
        }
        masks
    }

    fn search_with(masks: &Masks, from: &[Spot], to: &[Spot], extras: Extras) -> Option<Vec<Spot>> {
        search(masks, from, Goal::Spots(to), extras, &|| false).ok()
    }

    #[test]
    fn a_pad_walled_in_on_its_layer_is_left_through_a_via_in_the_pad_if_it_may_have_one() {
        let masks = walled_front(10, 5);
        let from = [(10, 5, FRONT)];
        let to = [(28, 5, BACK)];
        assert_eq!(search_with(&masks, &from, &to, Extras::default()), None);
        let extras = Extras {
            from: &[(10, 5, BACK)],
            to: &[],
            ..Extras::default()
        };
        let path = search_with(&masks, &from, &to, extras).unwrap();
        // The path starts on the back, in the pad's cell, and comes up at the far end.
        assert_eq!(path.first(), Some(&(10, 5, BACK)));
        assert_eq!(path.last(), Some(&(28, 5, BACK)));
        assert_eq!(link(&masks, &from, Goal::Spots(&to), extras), Link::Joined);
        assert_eq!(
            link(&masks, &from, Goal::Spots(&to), Extras::default()),
            Link::Closed
        );
    }

    #[test]
    fn the_far_end_of_a_path_may_be_a_via_in_a_pad_too() {
        let masks = walled_front(20, 5);
        let from = [(2, 5, BACK)];
        let to = [(20, 5, FRONT)];
        assert_eq!(search_with(&masks, &from, &to, Extras::default()), None);
        let extras = Extras {
            from: &[],
            to: &[(20, 5, BACK)],
            ..Extras::default()
        };
        let path = search_with(&masks, &from, &to, extras).unwrap();
        assert_eq!(path.last(), Some(&(20, 5, BACK)));
        assert_eq!(path.first(), Some(&(2, 5, BACK)));
    }

    #[test]
    fn a_via_in_a_pad_is_used_only_when_nothing_cheaper_reaches_the_pad() {
        // Nothing in the way: the straight path on the front costs 180 and a via in the pad
        // costs far more, so the path is not through it.
        let (_, masks) = open_grid();
        let extras = Extras {
            from: &[(2, 5, BACK)],
            to: &[(28, 5, BACK)],
            ..Extras::default()
        };
        let path = search_with(&masks, &[(2, 5, FRONT)], &[(28, 5, FRONT)], extras).unwrap();
        assert!(path.iter().all(|s| s.2 == FRONT), "{path:?}");
    }

    #[test]
    fn a_search_that_is_told_to_stop_gives_up_and_says_so() {
        // No place for a via anywhere, and so much to look at that the search looks at the
        // clock before it has seen it all.
        let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(40.0, 40.0)));
        let mut all_via = Masks::open(Window::full(&grid));
        all_via.via.iter_mut().for_each(|v| *v = true);
        assert_eq!(
            search(
                &all_via,
                &[(0, 0, FRONT)],
                Goal::Via,
                Extras::default(),
                &|| true
            ),
            Err(Miss::Stopped)
        );
        assert_eq!(
            search(
                &all_via,
                &[(0, 0, FRONT)],
                Goal::Via,
                Extras::default(),
                &|| false
            ),
            Err(Miss::Blocked)
        );
    }

    #[test]
    fn the_four_layer_board_is_routed_with_a_via_to_the_plane_beside_each_surface_mount_pad() {
        let mut board = Board::from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let left = route(&mut board);
        assert!(left.is_empty(), "{:?}", left.lines);
        // Tracks are on the two outer layers only: the type has no other.
        for (refdes, pad, net) in [("D1", "1", "GND"), ("R1", "1", "3V3")] {
            let part = board.part(refdes).unwrap();
            let at = part.pad_centre(part.footprint.pads_numbered(pad).next().unwrap());
            let via = board
                .vias
                .iter()
                .find(|v| v.net == net && v.at.distance(at) < 2.0)
                .unwrap_or_else(|| panic!("no {net} via beside {refdes}.{pad}"));
            // The via is not on the pad, and a track on the pad's layer joins them.
            assert!(via.at.distance(at) > 0.2, "{refdes}.{pad}");
            let touches = |p: Pt| {
                board.tracks.iter().any(|t| {
                    t.net == net
                        && t.layer == CopperLayer::Front
                        && (t.from.distance(p) < 1e-6 || t.to.distance(p) < 1e-6)
                })
            };
            assert!(touches(at) && touches(via.at), "{refdes}.{pad}");
        }
        // The through-hole pads are on the planes already, so nothing else has a via.
        assert_eq!(board.vias.iter().filter(|v| v.net == "GND").count(), 1);
        assert_eq!(board.vias.iter().filter(|v| v.net == "3V3").count(), 1);
    }

    /// A track of the net `3V3` over the first pad of the net `GND`, `gap` mm from its top
    /// edge, `width` wide, settled against that pad alone.
    fn settle_beside_a_foreign_pad(gap: f64, width: f64) -> (Option<Track>, f64) {
        let board = Board::from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let router = Router::new(&board, &RouteOptions::default());
        let part = board.part("D1").unwrap();
        let pad = part.footprint.pads_numbered("1").next().unwrap();
        let bounds = part.pad_bounds(pad);
        let net = board.nets.iter().position(|n| n.name == "3V3").unwrap();
        let pad_only: Vec<&Obstacle> = router
            .fixed
            .list
            .iter()
            .filter(|o| matches!(o.form, Form::Rect(r) if r == bounds))
            .collect();
        assert_eq!(pad_only.len(), 1, "the pad is one obstacle");
        let y = bounds.min.y - gap - width / 2.0;
        let track = Track {
            net: "3V3".into(),
            layer: CopperLayer::Front,
            from: Pt::new(bounds.min.x, y),
            to: Pt::new(bounds.max.x, y),
            width,
        };
        let routed = Routed {
            line: 0,
            tracks: vec![track],
            vias: Vec::new(),
        };
        let settled = router.settled(routed, net, &pad_only, true);
        (
            settled.ok().map(|r| r.tracks[0].clone()),
            board.rules.min_track_width,
        )
    }

    #[test]
    fn a_wide_track_nearer_a_pad_than_the_rules_allow_is_drawn_at_the_narrowest_width() {
        // 0.05 mm from the pad at 0.6 mm; at the narrowest width the same centre line is
        // far enough away.
        let (track, narrowest) = settle_beside_a_foreign_pad(0.05, 0.6);
        let track = track.expect("a narrow track fits");
        assert_eq!(track.width, narrowest);
    }

    #[test]
    fn a_wide_track_that_keeps_the_rules_clear_is_left_wide() {
        let (track, _) = settle_beside_a_foreign_pad(0.3, 0.6);
        assert_eq!(track.expect("it fits").width, 0.6);
    }

    #[test]
    fn a_track_over_a_pad_of_another_net_is_refused_whatever_its_width() {
        let (track, _) = settle_beside_a_foreign_pad(-0.4, 0.6);
        assert_eq!(track, None);
    }

    #[test]
    fn a_via_to_a_plane_keeps_the_rules_clear_of_other_copper() {
        let mut board = Board::from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        route(&mut board);
        let rules = board.rules.clone();
        for via in &board.vias {
            // Off every pad of another net by the clearance.
            for part in &board.parts {
                for pad in &part.footprint.pads {
                    let own = board
                        .net_of(&part.refdes, &pad.number)
                        .is_some_and(|n| n.name == via.net);
                    if own {
                        continue;
                    }
                    let b = part.pad_bounds(pad);
                    let dx = (b.min.x - via.at.x).max(via.at.x - b.max.x).max(0.0);
                    let dy = (b.min.y - via.at.y).max(via.at.y - b.max.y).max(0.0);
                    assert!(
                        dx.hypot(dy) - via.diameter / 2.0 >= rules.clearance - 1e-9,
                        "{} via at {:?} too near {}.{}",
                        via.net,
                        via.at,
                        part.refdes,
                        pad.number
                    );
                }
            }
        }
    }

    #[test]
    fn a_via_keeps_the_masks_least_web_from_an_opening_the_footprint_draws_in_the_mask() {
        let mut board = Board::from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        // An opening in the mask the library draws beside R1, where its plane via would go.
        board
            .parts
            .iter_mut()
            .find(|p| p.refdes == "R1")
            .unwrap()
            .rotation = 0.0;
        let r1 = board.part("R1").unwrap().courtyard();
        let opening = Rect::new(
            Pt::new(r1.max.x + 0.2, r1.min.y - 1.0),
            Pt::new(r1.max.x + 2.2, r1.max.y + 1.0),
        );
        let r1 = board.parts.iter_mut().find(|p| p.refdes == "R1").unwrap();
        let local = opening.translated(Pt::new(-r1.at.x, -r1.at.y));
        r1.footprint.mask_openings.push(local);
        route(&mut board);
        let rules = board.rules.clone();
        let reach = rules.via_diameter / 2.0 + rules.mask_bridge;
        for via in &board.vias {
            let form = Form::Rect(opening);
            assert!(
                form.distance(via.at) >= reach - 1e-9,
                "{} via at {:?} is {:.3} mm from the opening, under {reach}",
                via.net,
                via.at,
                form.distance(via.at)
            );
        }
        assert!(!board.vias.is_empty());
    }

    /// Copper of another net across the whole grid on both layers: a wall two cells thick at
    /// columns `i` and `i + 1`, except for the rows in `gap`.
    fn soft_wall(grid: &Grid, masks: &mut Masks, i: usize, gap: std::ops::Range<usize>) {
        let cells = masks.window.cells();
        let mut soft = Soft {
            track: [vec![false; cells], vec![false; cells]],
            via: vec![false; cells],
        };
        for j in (0..grid.ny).filter(|j| !gap.contains(j)) {
            for i in [i, i + 1] {
                let at = masks.at(i, j);
                soft.track[FRONT][at] = true;
                soft.track[BACK][at] = true;
                soft.via[at] = true;
            }
        }
        masks.soft = Some(soft);
    }

    #[test]
    fn a_soft_search_crosses_copper_that_a_hard_one_cannot() {
        let (grid, mut masks) = open_grid();
        soft_wall(&grid, &mut masks, 15, 0..0);
        // Hard masks do not see the wall; the soft ones price it, and the path goes through
        // because no way round exists.
        let path = find_soft(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])).unwrap();
        assert_eq!(path.len(), 31);
        assert!(path.contains(&(15, 5, FRONT)));
    }

    #[test]
    fn a_soft_search_goes_round_copper_by_a_gap_when_the_gap_is_not_far() {
        let (grid, mut masks) = open_grid();
        // The wall is open at rows 8 and 9, three rows from the line: the detour costs 160
        // (six steps and four turns), crossing the wall's two cells 200 (SOFT, 100 each).
        soft_wall(&grid, &mut masks, 15, 8..10);
        let path = find_soft(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])).unwrap();
        assert!(path.contains(&(15, 8, FRONT)), "{path:?}");
        assert!(!path.contains(&(15, 5, FRONT)));
    }

    #[test]
    fn a_soft_path_does_not_stop_at_a_gap_that_costs_more_than_the_wall() {
        let (grid, mut masks) = open_grid();
        // Open only at row 9, eight rows from the line, whose detour costs 260: the path
        // crosses the wall.
        soft_wall(&grid, &mut masks, 15, 9..10);
        let path = find_soft(&masks, &[(0, 1, FRONT)], Goal::Spots(&[(30, 1, FRONT)])).unwrap();
        assert!(path.contains(&(15, 1, FRONT)), "{path:?}");
    }

    #[test]
    fn masks_made_from_the_cached_fixed_copper_leave_a_nets_own_pads_open() {
        let board = Board::from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let router = Router::new(&board, &RouteOptions::default());
        let window = Window::full(&router.grid);
        let reach = Reach {
            room: ROOM_LOOSE,
            width: board.rules.track_width,
        };
        // For every net, a cell on its own pad is open to it, and shut to every other net.
        for (n, net) in board.nets.iter().enumerate() {
            let own = net.pads.first().expect("a net has pads");
            let part = board.part(&own.refdes).unwrap();
            let pad = part.footprint.pads_numbered(&own.number).next().unwrap();
            let (i, j) = router.grid.cell(part.pad_centre(pad));
            let (front, _) = part.pad_layers(pad);
            let layer = if front { FRONT } else { BACK };
            let mine = router.masks(window, n, (&[], &[]), (&[], &[]), reach);
            assert!(
                !mine.track[layer][mine.at(i, j)],
                "{}: its own pad {own:?} is shut to it",
                net.name
            );
            let other = (n + 1) % board.nets.len();
            {
                let theirs = router.masks(window, other, (&[], &[]), (&[], &[]), reach);
                assert!(
                    theirs.track[layer][theirs.at(i, j)],
                    "{}: the pad of {} is open to {}",
                    net.name,
                    net.name,
                    board.nets[other].name
                );
            }
        }
    }

    #[test]
    fn distances_are_to_the_edge_of_the_form() {
        let rect = Form::Rect(Rect::new(Pt::new(0.0, 0.0), Pt::new(2.0, 1.0)));
        assert_eq!(rect.distance(Pt::new(1.0, 0.5)), 0.0);
        assert_eq!(rect.distance(Pt::new(3.0, 0.5)), 1.0);
        assert_eq!(rect.distance(Pt::new(5.0, 5.0)), 5.0);
        let track = Form::Segment(Pt::new(0.0, 0.0), Pt::new(4.0, 0.0), 0.1);
        assert!((track.distance(Pt::new(2.0, 1.0)) - 0.9).abs() < 1e-12);
        assert!((track.distance(Pt::new(6.0, 0.0)) - 1.9).abs() < 1e-12);
        let via = Form::Circle(Pt::new(0.0, 0.0), 0.3);
        assert!((via.distance(Pt::new(1.0, 0.0)) - 0.7).abs() < 1e-12);
    }
}
