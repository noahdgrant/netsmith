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
//! Lines are routed shortest first. A line that cannot be routed is then tried again by
//! ripping up the nets that stand in its way, routing it, and routing those again; and if
//! lines are still left, the whole board is routed again with the lines in another order
//! (longest first, then by net, then shortest last). The first complete result is kept, or
//! failing that the one that leaves fewest lines. There is no push-and-shove, so a board
//! can still end partly routed, and what is left is returned.
//!
//! The work is bounded. A line is searched for in a window round its pads first, and in a
//! larger one only if that finds nothing. Before a search, a cheap flood from both ends finds
//! out whether the ends are joined at all, so a pad that its neighbours wall in is given up on
//! at once instead of after a search of the whole board. The router makes a fixed number of
//! tries and rip-up passes, and stops at the time limit in [`RouteOptions`], returning what is
//! left.

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

/// How far, in mm, round the pads of a failed line the routed lines are ripped up when the
/// region is started again, how many times each failed line gets that, and how many
/// passes over the failed lines.
const REGION: f64 = 3.5;
const REGION_TRIES: usize = 6;
const REGION_PASSES: usize = 3;

/// What a move costs, in tenths of a millimetre of track.
const STEP: i64 = 10;
const TURN: i64 = 25;
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

const FRONT: usize = 0;
const BACK: usize = 1;
/// The four directions, then "none" for the first step of a path.
const MOVES: [(isize, isize); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
const NO_DIRECTION: usize = 4;

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
            history: [Vec::new(), Vec::new()],
            soft: None,
        }
    }

    fn at(&self, i: usize, j: usize) -> usize {
        self.window.index(i, j)
    }

    /// Whether a track on the cell has to be necked down.
    fn necked(&self, layer: usize, i: usize, j: usize) -> bool {
        self.wide.as_ref().is_some_and(|w| w[layer][self.at(i, j)])
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
    let states = window.cells() * 2 * 5;
    let state =
        |(i, j, layer): Spot, dir: usize| (layer * window.cells() + window.index(i, j)) * 5 + dir;
    let unpack = |s: usize| -> (Spot, usize) {
        let dir = s % 5;
        let cell = s / 5;
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
            Goal::Spots(to) => to
                .iter()
                .map(|&(ti, tj, _)| (i.abs_diff(ti) + j.abs_diff(tj)) as i64 * STEP)
                .min()
                .unwrap_or(0),
            Goal::Via => 0,
        }
    };

    let mut cost = vec![i64::MAX; states];
    let mut came_from = vec![usize::MAX; states];
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
        queue.push(Reverse((start + heuristic(spot), sequence, s)));
        sequence += 1;
    }
    // A state that stands on a cell reached through a via in a pad is not the end of the
    // path itself; it becomes so, at the via's cost, when it comes off the queue. Those
    // ends are queued as a state number past the real ones.
    let mut finished = vec![false; states];
    let end_of = |s: usize| s - states;
    let mut taken = 0u32;
    while let Some(Reverse((_, _, s))) = queue.pop() {
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
            queue.push(Reverse((cost[s] + VIA_IN_PAD, sequence, states + s)));
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
                queue.push(Reverse((cost[n] + heuristic(next), sequence, n)));
                sequence += 1;
            }
        };
        for (d, (di, dj)) in MOVES.iter().enumerate() {
            let (ni, nj) = (i as isize + di, j as isize + dj);
            if ni < 0 || nj < 0 || !window.contains(ni as usize, nj as usize) {
                continue;
            }
            let next = (ni as usize, nj as usize, layer);
            if masks.track[layer][masks.at(next.0, next.1)] && !open(next) {
                continue;
            }
            let turn = if dir != NO_DIRECTION && dir != d {
                TURN
            } else {
                0
            };
            let neck = if masks.necked(layer, next.0, next.1) {
                NECK
            } else {
                0
            };
            relax(
                next,
                d,
                STEP + turn
                    + neck
                    + masks.history_cost(layer, next.0, next.1)
                    + masks.soft_cost(layer, next.0, next.1),
            );
        }
        let other = 1 - layer;
        let at = masks.at(i, j);
        if !masks.via[at] && (!masks.track[other][at] || open((i, j, other))) {
            relax((i, j, other), NO_DIRECTION, VIA + masks.soft_via_cost(i, j));
        }
    }
    Err(Miss::Blocked)
}

/// Straight runs through a list of cells on one layer; a single cell has none.
#[cfg(test)]
fn runs(grid: &Grid, cells: &[(usize, usize)]) -> Vec<(Pt, Pt)> {
    let none = vec![false; cells.len()];
    runs_with(grid, cells, &none)
        .into_iter()
        .map(|(a, b, _)| (a, b))
        .collect()
}

/// [`runs`], where a step to or from a cell in `neck` is a run of its own, marked as
/// necked down, so that the track is drawn narrow there and wide elsewhere.
fn runs_with(grid: &Grid, cells: &[(usize, usize)], neck: &[bool]) -> Vec<(Pt, Pt, bool)> {
    let mut out = Vec::new();
    let mut start = 0;
    let narrow = |a: usize, b: usize| neck[a] || neck[b];
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
                    list.push(Obstacle {
                        net: None,
                        layers: [true, true],
                        form: Form::Segment(a, b, radius),
                        clearance: rules.clearance + MARGIN,
                    });
                    // A via's hole keeps `hole_to_hole` from this hole's edge.
                    via_keepouts.push((
                        Form::Segment(a, b, radius),
                        rules.hole_to_hole + rules.via_drill / 2.0 + MARGIN,
                    ));
                }
                continue;
            }
            let bounds = part.pad_bounds(pad);
            list.push(Obstacle {
                net: net_of
                    .get(&(part.refdes.as_str(), pad.number.as_str()))
                    .copied(),
                layers: [front, back],
                form: Form::Rect(bounds),
                clearance: rules.clearance + MARGIN,
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
    /// Whether lines left over are negotiated for, and what the negotiation has learnt.
    negotiate: bool,
    history: RefCell<[Vec<u8>; 2]>,
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

    /// Whether the time limit has passed, or the caller asked for the router to stop.
    fn out_of_time(&self) -> bool {
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
                    rules.hole_to_hole + rules.via_drill + MARGIN,
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
        let others: Vec<&Routed> = state
            .routed
            .iter()
            .filter(|r| !ignore.contains(&self.nets[r.line]))
            .collect();
        let (list, routed_keepouts) = self.copper_of(&others);
        // A soft search goes through the copper of other nets at a price, to find the nets
        // that are least in the way; what it finds is not copper that can be drawn.
        let obstacles: Vec<&Obstacle> = if soft {
            self.fixed.list.iter().collect()
        } else {
            self.fixed.list.iter().chain(list.iter()).collect()
        };
        let routed: Vec<&Obstacle> = list.iter().collect();
        let (hard, hard_keepouts): (&[&Obstacle], &[(Form, f64)]) = if soft {
            (&[], &[])
        } else {
            (&routed, &routed_keepouts)
        };
        let (soft_list, soft_keepouts): (&[&Obstacle], &[(Form, f64)]) = if soft {
            (&routed, &routed_keepouts)
        } else {
            (&[], &[])
        };
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
            // width may pass at the narrowest the process makes.
            if room == ROOM_TIGHT && track_width > rules.min_track_width + 1e-9 {
                let narrow = self.masks(
                    window,
                    net,
                    (hard, hard_keepouts),
                    (soft_list, soft_keepouts),
                    Reach {
                        room,
                        width: rules.min_track_width,
                    },
                );
                masks.wide = Some(std::mem::replace(&mut masks.track, narrow.track));
            }
            self.add_history(&mut masks);
            match link(&masks, from, goal, extras) {
                Link::Joined => {}
                Link::Closed => break,
                Link::Cramped if window.covers(&self.grid) => break,
                Link::Cramped => continue,
            }
            match search(&masks, from, goal, extras, &|| self.out_of_time()) {
                Ok(path) => {
                    let start = in_from.filter(|p| path.first() == Some(&p.spot));
                    let end = in_to.filter(|p| path.last() == Some(&p.spot));
                    let routed = self.realize(line, &path, start, end, &masks);
                    // What the grid let through is measured again on the copper itself: a
                    // line whose copper is nearer to another net's than the rules allow, even
                    // at the narrowest track, is not routed.
                    match self.settled(routed, net, &obstacles) {
                        Some(routed) => return Some((routed, path)),
                        None => break,
                    }
                }
                Err(Miss::Stopped) => return None,
                Err(Miss::Blocked) if window.covers(&self.grid) => break,
                Err(Miss::Blocked) => {}
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
    fn settled(&self, mut routed: Routed, net: usize, obstacles: &[&Obstacle]) -> Option<Routed> {
        let rules = &self.board.rules;
        let narrowest = rules.min_track_width;
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
                if track.width <= narrowest + 1e-9 || !clear(narrowest) {
                    return None;
                }
                track.width = narrowest;
            }
        }
        for via in &routed.vias {
            let radius = via.diameter / 2.0;
            if !others
                .iter()
                .all(|o| o.form.distance(via.at) - radius >= apart(o))
            {
                return None;
            }
        }
        Some(routed)
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
        // A track through a cell that is too tight for the full width is the narrowest.
        let width_of = |necked: bool| {
            if necked {
                rules.min_track_width
            } else {
                self.width_of(line)
            }
        };
        let necked = |&(i, j, layer): &Spot| masks.necked(layer, i, j);
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
            let neck: Vec<bool> = path[start..k].iter().map(necked).collect();
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
        }
        // LAB-BEGIN
        if std::env::var("LAB_REGION").is_ok_and(|v| v == "1") && !state.failed.is_empty() {
            self.restart_regions(&mut state, ROOM_TIGHT, attempt, attempts);
        }
        // LAB-END
        state
    }

    /// Where a line would go if the copper of other nets could be crossed, at a price per
    /// cell ([`SOFT`]), and the routed lines whose copper that path comes near: the lines least
    /// in the line's way, not all that lie between its ends, and not the other lines of their
    /// nets. `None` if even that has no path.
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
            if near && !found.contains(&r.line) {
                found.push(r.line);
            }
        }
        // LAB-BEGIN
        if std::env::var_os("LAB_NETRIP").is_some() {
            let nets: Vec<usize> = found.iter().map(|&l| self.nets[l]).collect();
            found = state
                .routed
                .iter()
                .filter(|r| nets.contains(&self.nets[r.line]))
                .map(|r| r.line)
                .collect();
        }
        // LAB-END
        found.sort_unstable();
        Some((path, found))
    }

    /// The routed lines whose copper stands in the way of a line.
    fn blockers(&self, line: usize, state: &State, room: f64) -> Vec<usize> {
        self.contested(line, state, room)
            .map_or_else(Vec::new, |(_, lines)| lines)
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
        let rounds = std::env::var("LAB_ROUNDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(NEGOTIATION_ROUNDS);
        for round in 1..=rounds {
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
                    .partition(|r| blockers.contains(&r.line));
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

    /// Lines the other methods left are given a fresh start together with their
    /// surroundings: every routed line whose copper is within [`REGION`] mm of a pad of a
    /// failed line is ripped up, and the failed lines and these are routed again, the failed
    /// first, in another order each time. What comes out is kept only if it leaves fewer
    /// lines unrouted than before.
    fn restart_regions(&self, state: &mut State, room: f64, attempt: usize, attempts: usize) {
        let rounds = std::env::var("LAB_REGION_PASSES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(REGION_PASSES);
        let tries = std::env::var("LAB_REGION_TRIES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(REGION_TRIES);
        let radius: f64 = std::env::var("LAB_REGION_MM")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(REGION);
        for pass in 0..rounds {
            let before = state.failed.len();
            for line in state.failed.clone() {
                if !state.failed.contains(&line) {
                    continue;
                }
                for k in 0..tries {
                    if self.out_of_time() {
                        return;
                    }
                    self.report(false, || {
                        format!(
                            "routing, try {attempt} of {attempts}: starting again round {} (pass {} of {rounds}), {} not routed",
                            self.lines[line], pass + 1, state.failed.len()
                        )
                    });
                    if let Some(better) = self.restart_region(line, state, room, radius, k) {
                        *state = better;
                        break;
                    }
                }
            }
            if state.failed.is_empty() || state.failed.len() >= before {
                break;
            }
        }
    }

    /// One start again of the region round a failed line; the new state if it is better.
    fn restart_region(
        &self,
        line: usize,
        state: &State,
        room: f64,
        radius: f64,
        variant: usize,
    ) -> Option<State> {
        let l = &self.lines[line];
        let ends: Vec<Pt> = if l.to_plane {
            vec![l.from_at]
        } else {
            vec![l.from_at, l.to_at]
        };
        let near = |p: Pt, q: Pt, half: f64| point_to_line(p, q, q) - half < radius;
        let _ = near;
        let touches = |r: &Routed| {
            r.tracks.iter().any(|t| {
                ends.iter()
                    .any(|&p| point_to_line(p, t.from, t.to) - t.width / 2.0 < radius)
            }) || r
                .vias
                .iter()
                .any(|v| ends.iter().any(|&p| v.at.distance(p) < radius))
        };
        let mut trial = state.clone();
        let (gone, kept): (Vec<Routed>, Vec<Routed>) = trial
            .routed
            .drain(..)
            .partition(|r| self.nets[r.line] != self.nets[line] && touches(r));
        trial.routed = kept;
        if gone.is_empty() {
            return None;
        }
        // The failed lines with an end in the region, and the lines ripped up.
        let in_region = |f: usize| {
            let fl = &self.lines[f];
            ends.iter().any(|&p| {
                p.distance(fl.from_at) < radius || (!fl.to_plane && p.distance(fl.to_at) < radius)
            })
        };
        let mut failed: Vec<usize> = state
            .failed
            .iter()
            .copied()
            .filter(|&f| f == line || in_region(f))
            .collect();
        trial.failed.retain(|f| !failed.contains(f));
        let mut ripped: Vec<usize> = gone.iter().map(|r| r.line).collect();
        let by_length =
            |a: &usize, b: &usize| self.lines[*a].length().total_cmp(&self.lines[*b].length());
        if variant == 0 {
            failed.sort_by(|a, b| {
                self.depth[*b]
                    .total_cmp(&self.depth[*a])
                    .then(by_length(a, b))
            });
            ripped.sort_by(by_length);
        } else {
            let mut x = (line as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ (variant as u64).wrapping_mul(0xD1B5_4A32_D192_ED03);
            let mut next = || {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x
            };
            // A random order, but the lines that ended on deep pads earlier on average.
            let mut key: HashMap<usize, f64> = HashMap::new();
            for &f in failed.iter().chain(&ripped) {
                key.insert(
                    f,
                    (next() % 10_000) as f64 / 10_000.0 - 0.5 * self.depth[f].min(4.0) / 4.0,
                );
            }
            failed.sort_by(|a, b| key[a].total_cmp(&key[b]));
            ripped.sort_by(|a, b| key[a].total_cmp(&key[b]));
        }
        for l in failed.into_iter().chain(ripped) {
            if self.out_of_time() {
                return None;
            }
            match self.try_line(l, &trial, &[], room, false) {
                Some((routed, _)) => trial.routed.push(routed),
                None => trial.failed.push(l),
            }
        }
        (trial.failed.len() < state.failed.len()).then_some(trial)
    }

    /// Try to get a failed line routed by ripping up the nets in its way, routing it, and
    /// routing what was ripped up again. If that does not work out, nothing changes.
    fn rip_up_for(&self, line: usize, state: &mut State, room: f64) {
        let blockers = self.blockers(line, state, room);
        // All the lines in the way at once, then the lines of each net on its own.
        let mut attempts: Vec<Vec<usize>> = vec![blockers.clone()];
        let mut nets: Vec<usize> = blockers.iter().map(|&l| self.nets[l]).collect();
        nets.sort_unstable();
        nets.dedup();
        if nets.len() > 1 {
            attempts.extend(nets.iter().map(|&n| {
                blockers
                    .iter()
                    .copied()
                    .filter(|&l| self.nets[l] == n)
                    .collect()
            }));
        }
        for ripped in attempts.into_iter().filter(|a| !a.is_empty()) {
            if self.out_of_time() {
                return;
            }
            let mut trial = state.clone();
            let (gone, kept): (Vec<Routed>, Vec<Routed>) = trial
                .routed
                .drain(..)
                .partition(|r| ripped.contains(&r.line));
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

    // LAB-BEGIN
    fn map(&self, line: usize, state: &State, room: f64) {
        let net = self.nets[line];
        let (copper, ro_keepouts) = self.copper_of(&state.routed.iter().collect::<Vec<_>>());
        let routed: Vec<&Obstacle> = copper.iter().collect();
        let width = self.width_of(line);
        let reach = Reach { room, width };
        for (end, at) in [
            (&self.ends[line].0, self.lines[line].from_at),
            (&self.ends[line].1, self.lines[line].to_at),
        ] {
            if end.is_empty() {
                continue;
            }
            let (ci, cj, _) = end[0];
            let w = Window::around(&self.grid, &[(ci, cj, 0)], 3.0);
            let all = self.masks(w, net, (&routed, &ro_keepouts), (&[], &[]), reach);
            let fixed = self.masks(w, net, (&[], &[]), (&[], &[]), reach);
            eprintln!(
                "MAP line {} end ({:.2},{:.2}) window {}x{} cells; per row: front | back | via   (. free, R routed copper of another net, F fixed obstacle, P pad cell, v free for via only)",
                self.lines[line], at.x, at.y, w.nx, w.ny
            );
            for j in w.j0..w.j0 + w.ny {
                let mut row = String::new();
                for l in 0..3 {
                    for i in w.i0..w.i0 + w.nx {
                        let k = all.at(i, j);
                        let (a, f) = match l {
                            0 => (all.track[FRONT][k], fixed.track[FRONT][k]),
                            1 => (all.track[BACK][k], fixed.track[BACK][k]),
                            _ => (all.via[k], fixed.via[k]),
                        };
                        let c = if end.contains(&(i, j, FRONT)) && l == 0
                            || end.contains(&(i, j, BACK)) && l == 1
                        {
                            'P'
                        } else if f {
                            'F'
                        } else if a {
                            'R'
                        } else {
                            '.'
                        };
                        row.push(c);
                    }
                    row.push_str(" | ");
                }
                eprintln!("MAP {row}");
            }
        }
    }
    fn deadline_off(&self) {}
    fn lab(&self, state: &State) {
        let all: Vec<usize> = (0..self.board.nets.len()).collect();
        for &line in &state.failed {
            eprintln!("LAB failed line {}", self.lines[line]);
            let net = self.nets[line];
            let pads = [self.lines[line].from_at, self.lines[line].to_at];
            if std::env::var_os("LAB_MAP").is_some() {
                self.map(line, state, ROOM_TIGHT);
                continue;
            }
            for room in [ROOM_LOOSE, ROOM_TIGHT] {
                let free = self.try_line(line, state, &all, room, false).is_some();
                eprintln!(
                    "LAB   room {room:.3}: routable with every other net's routed copper removed: {free}"
                );
                if free {
                    // Which single nets near the pads.
                    let mut near: Vec<usize> = Vec::new();
                    for r in state.routed.iter().filter(|r| self.nets[r.line] != net) {
                        let hit = r
                            .tracks
                            .iter()
                            .any(|t| pads.iter().any(|&p| point_to_line(p, t.from, t.to) < 3.0))
                            || r.vias
                                .iter()
                                .any(|v| pads.iter().any(|&p| v.at.distance(p) < 3.0));
                        if hit && !near.contains(&self.nets[r.line]) {
                            near.push(self.nets[r.line]);
                        }
                    }
                    let mut single = Vec::new();
                    for &n in &near {
                        if self.try_line(line, state, &[n], room, false).is_some() {
                            single.push(self.board.nets[n].name.clone());
                        }
                    }
                    eprintln!(
                        "LAB   room {room:.3}: {} nets within 3 mm; removing only one frees the line: {:?}",
                        near.len(),
                        single
                    );
                    for name in &single {
                        let n = self
                            .board
                            .nets
                            .iter()
                            .position(|x| &x.name == name)
                            .unwrap();
                        let mut trial = state.clone();
                        let (gone, kept): (Vec<Routed>, Vec<Routed>) =
                            trial.routed.drain(..).partition(|r| self.nets[r.line] == n);
                        trial.routed = kept;
                        let Some((first, _)) = self.try_line(line, &trial, &[], room, false) else {
                            continue;
                        };
                        trial.routed.push(first);
                        for g in &gone {
                            let ok = self.try_line(g.line, &trial, &[], room, false);
                            match ok {
                                Some((r, _)) => {
                                    trial.routed.push(r);
                                    eprintln!(
                                        "LAB     ripping {name}: line {} rerouted",
                                        self.lines[g.line]
                                    );
                                }
                                None => {
                                    let mut fr = Vec::new();
                                    let mut nn: Vec<usize> = Vec::new();
                                    for r in trial.routed.iter() {
                                        let m = self.nets[r.line];
                                        if m != self.nets[g.line] && !nn.contains(&m) {
                                            nn.push(m);
                                        }
                                    }
                                    let ends =
                                        [self.lines[g.line].from_at, self.lines[g.line].to_at];
                                    nn.retain(|&m| {
                                        trial.routed.iter().any(|r| {
                                            self.nets[r.line] == m
                                                && (r.tracks.iter().any(|t| {
                                                    ends.iter().any(|&p| {
                                                        point_to_line(p, t.from, t.to) < 2.0
                                                    })
                                                }) || r.vias.iter().any(|v| {
                                                    ends.iter().any(|&p| v.at.distance(p) < 2.0)
                                                }))
                                        })
                                    });
                                    for &m in &nn {
                                        if self
                                            .try_line(g.line, &trial, &[m], room, false)
                                            .is_some()
                                        {
                                            fr.push(self.board.nets[m].name.clone());
                                        }
                                    }
                                    eprintln!(
                                        "LAB     ripping {name}: line {} CANNOT be rerouted; single nets freeing it: {:?}",
                                        self.lines[g.line], fr
                                    );
                                }
                            }
                        }
                    }
                    // Pairs
                    let mut pairs = Vec::new();
                    if single.is_empty() {
                        for (a, &x) in near.iter().enumerate() {
                            for &y in &near[a + 1..] {
                                if self.try_line(line, state, &[x, y], room, false).is_some() {
                                    pairs.push((
                                        self.board.nets[x].name.clone(),
                                        self.board.nets[y].name.clone(),
                                    ));
                                }
                            }
                        }
                        eprintln!("LAB   room {room:.3}: pairs that free it: {pairs:?}");
                    }
                }
            }
        }
    }
    // LAB-END

    /// The orders to try the lines in.
    fn orders(&self, retry: bool) -> Vec<Vec<usize>> {
        let mut shortest: Vec<usize> = (0..self.lines.len()).collect();
        shortest.sort_by(|&a, &b| self.lines[a].length().total_cmp(&self.lines[b].length()));
        let mut orders = vec![shortest.clone()];
        if retry {
            let mut longest = shortest.clone();
            longest.reverse();
            let mut by_net = shortest.clone();
            by_net.sort_by_key(|&l| self.nets[l]);
            let mut reversed_nets = by_net.clone();
            reversed_nets.reverse();
            // LAB-BEGIN
            let mut deep = shortest.clone();
            deep.sort_by(|&a, &b| self.depth[b].total_cmp(&self.depth[a]));
            let mode = std::env::var("LAB_ORDERS").unwrap_or_default();
            if mode == "deep" {
                return vec![deep];
            }
            if mode == "short" {
                return orders;
            }
            if mode == "deep-first" {
                orders.insert(0, deep.clone());
            }
            if mode == "deep+" {
                orders.push(deep.clone());
            }
            // LAB-END
            for o in [longest, by_net, reversed_nets] {
                if !orders.contains(&o) {
                    orders.push(o);
                }
            }
        }
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
    /// over dearer, and route what was ripped up again, for a few rounds. Only used with
    /// `rip_up`.
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
            // LAB-BEGIN
            if std::env::var_os("LAB_TRIES").is_some() {
                eprintln!("LAB try {tries}: {} left", state.failed.len());
            }
            // LAB-END
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
        // LAB-BEGIN
        if std::env::var("LAB_REGION").is_ok_and(|v| v == "end")
            && let Some(b) = &mut best
            && !b.failed.is_empty()
        {
            router.restart_regions(b, ROOM_TIGHT, tries, tries);
        }
        // LAB-END
        let stopped = router.out_of_time();
        // LAB-BEGIN
        if std::env::var_os("NETSMITH_ROUTE_LAB").is_some()
            && let Some(b) = &best
        {
            router.deadline_off();
            router.lab(b);
        }
        // LAB-END
        (best.unwrap_or_default(), stopped, tries)
    };
    let mut routed = best.routed;
    routed.sort_by_key(|r| r.line);
    for r in routed {
        board.tracks.extend(r.tracks);
        board.vias.extend(r.vias);
    }
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
        let mut neck = vec![false; 10];
        for n in &mut neck[4..7] {
            *n = true;
        }
        let runs = runs_with(&grid, &cells, &neck);
        // Wide up to the first necked cell, narrow through it, wide again after.
        let widths: Vec<bool> = runs.iter().map(|r| r.2).collect();
        assert_eq!(widths, [false, true, false], "{runs:?}");
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
        let settled = router.settled(routed, net, &pad_only);
        (
            settled.map(|r| r.tracks[0].clone()),
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
        let path = find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])).unwrap();
        assert_eq!(path.len(), 31);
        assert!(path.contains(&(15, 5, FRONT)));
    }

    #[test]
    fn a_soft_search_goes_round_copper_by_a_gap_when_the_gap_is_not_far() {
        let (grid, mut masks) = open_grid();
        // The wall is open at rows 8 and 9, three rows from the line: the detour costs 160
        // (six steps and four turns), crossing the wall's two cells 200 (SOFT, 100 each).
        soft_wall(&grid, &mut masks, 15, 8..10);
        let path = find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])).unwrap();
        assert!(path.contains(&(15, 8, FRONT)), "{path:?}");
        assert!(!path.contains(&(15, 5, FRONT)));
    }

    #[test]
    fn a_soft_path_does_not_stop_at_a_gap_that_costs_more_than_the_wall() {
        let (grid, mut masks) = open_grid();
        // Open only at row 9, eight rows from the line, whose detour costs 260: the path
        // crosses the wall.
        soft_wall(&grid, &mut masks, 15, 9..10);
        let path = find(&masks, &[(0, 1, FRONT)], Goal::Spots(&[(30, 1, FRONT)])).unwrap();
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
