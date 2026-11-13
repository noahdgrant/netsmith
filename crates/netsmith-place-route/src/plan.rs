//! The coarse plan of the router (issue #431, S10): a global route of every line over tiles of
//! the board, made before the detailed search, so that a line knows where the board is crowded
//! before it fails there.
//!
//! **Tiles.** The router's grid of 0.1 mm cells is cut into square tiles of [`TILE_CELLS`]
//! cells a side. Each edge between two tiles on a layer has a capacity: how many tracks at the
//! board's signal width and space cross it, counted on the cells along the edge that the fixed
//! copper (pads, holes, keepouts, the outline) leaves open to a track's centre. A run of `r`
//! open cells holds `(r - 1) * 0.1 / (width + space) + 1` tracks, rounded down. Each tile has a
//! via capacity too: the cells open to a via, over the area a via and its space take.
//!
//! **The plan** ([`negotiate`]). Every line is routed on the graph of tiles (a step to a
//! neighbour on its layer, or a via to the other layer in the same tile) by negotiated
//! congestion (PathFinder, McMurchie and Ebeling 1995): an edge or a tile's vias cost their
//! base times `1 + history` times `1 + present * overuse`, where `overuse` is how far one more
//! line would take it past its capacity. After each round every edge still over its capacity
//! adds its overuse to its history and `present` doubles, and every line is routed again, in
//! the same order. Nothing is random: the order is fixed, the costs are integers, and the
//! search queue breaks ties by when a state was queued. The round with the least overuse is
//! kept (the first of equals).
//!
//! **The corridor** ([`corridor`]) of a line is the tiles of its plan, on both layers. The
//! detailed search pays for a step outside it by how crowded the plan leaves that tile on that
//! layer ([`Plan::fill`], the fullest of its four edges): nothing below 70 %, the whole
//! crowding cost (`OFF_PLAN` in `route.rs`, 20, two steps) from 100 %, in proportion between
//! ([`crowding_cost`]). A free detour stays free where the plan has room. It is a cost, not a
//! wall, so a line the plan got wrong still finds its way. The plan steers only lines of the
//! narrowest track (the capacities are counted in it); a line to a plane, a net its current
//! widens, a pour's net (the pour joins it anyway) and a diff pair's nets (which run beside
//! each other, S9, and which any cost moves apart) have no corridor.
//!
//! **When** ([`crate::route::CoarsePlan`], `RouteOptions::plan`): off by default. `EveryTry`
//! follows the plan on every try; `FromSecondTry` routes the first order again with it as the
//! second try, as the escape lanes do, so a board that routes on its first try is unchanged.
//! `netsmith route --plan every|later` turns it on for experiments.
//!
//! # Measured (issue #431, S10, 2026-11-12)
//!
//! Main at d07d6e8 (#557), placements saved by main's `fab`. The robustness runs are one try
//! with no time limit: `netsmith route <placed> --tries 1 --route-timeout 0 --turn T`, at the
//! least track (`--margin 1`) and at the board's own margin. Lines left and vias at
//! T = 24 / 25 / 26 / 27:
//!
//! | | RP2350 | Orbit | EasyDuino |
//! |---|---|---|---|
//! | main, least track | 0/0/0/1, 137/137/151/162 | 0/0/0/0, 93/91/93/93 | 0/0/0/0, 106/105/105/105 |
//! | plan, least track | **0/0/0/0**, 159/159/159/166 | 0/0/0/0, 100/100/106/106 | 0/0/0/0, 103/103/103/103 |
//! | main, own margin | 0/0/0/1, 137/137/151/162 | 0/0/0/0, 87/87/90/96 | 0/0/0/0, 106/105/105/105 |
//! | plan, own margin | **0/0/0/0**, 147/**130**/159/166 | 0/0/0/0, 100/100/106/106 | 0/0/0/0, 113/109/109/109 |
//!
//! At T = 25, routed length over the spanning tree, least track: RP2350 1.34x against 1.40x,
//! Orbit 1.24x against 1.27x, EasyDuino 1.22x against 1.21x; own margin: 1.42x against 1.40x,
//! 1.27x against 1.28x, 1.18x against 1.22x. At its own margin RP2350 routes completely in the
//! margin pass at 24 and 25 (signals at 0.15 mm), which main never does. Other nets' track on
//! the GND pour at the own margin: RP2350 731 mm against 646, Orbit 418 against 466,
//! EasyDuino F 429 against 504, B 399 against 352. Pair coupling: RP2350's mcu.usb 75 %
//! against 69 %, Orbit's usb 91 % against 0 %, EasyDuino's bridge 20 % against 54 %.
//!
//! As `fab` routes (`netsmith route <placed>`, fab's tries and time, one at a time on one
//! machine, main and the plan in turn, twice):
//!
//! | | RP2350 | Orbit | EasyDuino |
//! |---|---|---|---|
//! | main | 121 and 123 s, 137 vias, 1.40x, 0.1 mm | 11 and 13 s, 87, 1.28x | 2.4 and 2.3 s, 105, 1.22x |
//! | plan | **73 s, 130 vias, 1.42x, 0.15 mm; then 152 s, 159, 1.34x, 0.1 mm** | 13 and 13 s, 100, 1.27x | 1.8 and 1.6 s, 109, 1.18x |
//!
//! With the plan, one try of RP2350 at its margin takes 73 s of the margin pass's 75: the first
//! run finished it, the second ran out and routed again at the least track. So `fab`'s result
//! with the plan would hang on the machine's speed, which main's does not.
//!
//! So the plan makes every board route at every turn cost and can take RP2350 through its
//! margin pass, but not reliably within its time, and costs Orbit 10 to 18 % more vias in every form tried, RP2350 16 % at the least
//! track and EasyDuino 4 to 8 % at its margin: it misses the bar (no more than main's vias and
//! 5 %), and is off.
//!
//! **Why the vias.** On two layers the plan does not count crossings, and a cost that differs
//! by layer makes a line change to the emptier one: Orbit's extra vias are about two on each of
//! a dozen signal nets, one layer change each, and six on GND. A cost on both layers (the
//! fuller or the mean of the two) does not help: the mean is almost never 70 % (main's copper),
//! the fuller leaves a line on RP2350 and Orbit.
//!
//! **Tried and not kept** (RP2350 at T = 25 and the least track unless said; main 137 vias,
//! 1.40x):
//!
//! - The plan of #556: a flat cost of 10 on every step outside a corridor grown by one tile:
//!   209 vias (Orbit 109, EasyDuino 115). Costs of 2 or 4, corridors grown by two, tiles of
//!   2 mm and the plan in the first pass only: 150 to 202. Pads counted as closed and no pull
//!   toward the straight line: L-shaped corridors and worse on every board.
//! - The corridor on the plan's layer only, grown by one there (both layers where it changes
//!   layer): 163 to 165 vias, but 1.30x and only 291 to 323 mm of other nets' track on the
//!   GND pour against 646, the best length and pour of anything tried; grown by two, 206 vias.
//!   A lead for a plan that counts crossings.
//! - The crowding cost at 10: 154 to 157 vias; at 30 or 50: 161; from 30 % or 50 % full:
//!   160 to 170; charged once where a step enters a tile instead of on every cell (50 to
//!   200, as `fab` routes): 147 to 177 vias, and 140 s against 66 at 50. The thresholds 60 % to 95 % give the same
//!   copper at 20 (a tile's fullness takes few values).
//! - Corridors grown by one, with the pour's net steered: 145 to 147 vias at every turn cost
//!   at the least track (the best RP2350 there), but Orbit 108 to 110 and EasyDuino 121 at
//!   its margin. With a diff pair steered too, Orbit leaves its USB pair unrouted at every
//!   turn cost.
//! - No corridor for a net whose current needs a cluster of vias: no change (at the least
//!   track every net needs one). The plan's via cost at 300 or 600 against 150: no better.
//! - From the second try: the margin pass has a quarter of the route time and one try of
//!   RP2350 takes most of it, so the second try never finishes there and `fab` is main's.
//!
//! **What to try next.** A plan that counts crossings: the order of tracks across each tile
//! edge, so that two lines that must cross are planned as a crossing with a via that the plan
//! pays for and the tile's vias count, and then the corridor on the plan's layer (above, the
//! best length and pour). And a measure of more than three boards before a choice: one
//! placement and one variant moves a board's vias by 10 % either way.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// How many grid cells a side of a tile has: 10, a tile of 1 mm.
pub(crate) const TILE_CELLS: usize = 10;

/// How many tiles a line's corridor reaches past its plan: none, the plan's own tiles.
pub(crate) const CORRIDOR_GROW: usize = 0;

/// What a step to the next tile costs before congestion, and a via.
const STEP_COST: u64 = 100;
const VIA_COST: u64 = 150;
/// What entering a tile costs for each tile it is from the straight line between a line's ends.
const DEVIATION: u64 = 30;

/// How full a tile's fullest edge must be (planned tracks over capacity) before a step there
/// outside a line's corridor costs anything, and how full it is when it costs the whole of the
/// crowding cost.
const CROWDED_FROM: f64 = 0.7;
const CROWDED: f64 = 1.0;

/// The rounds of negotiation, and the present-use factor of the first (in hundredths; it
/// doubles each round).
const ROUNDS: usize = 10;
const PRESENT_START: u64 = 50;

/// What a line pays, before congestion, for each line of another net it crosses in a tile on
/// a layer, or whose place on a tile's edge it takes: more than the two vias that take it under
/// the other line and back.
const CROSS_COST: u64 = 400;

/// The most places a tile's edge is cut into (see [`Tiles::slots`]).
const MOST_SLOTS: usize = 12;

/// The sides of a tile a line enters or leaves it by: toward the lower and the higher column,
/// the lower and the higher row.
const LOW_X: usize = 0;
const HIGH_X: usize = 1;
const LOW_Y: usize = 2;
const HIGH_Y: usize = 3;

/// A tile on a layer: its column, its row and the layer.
pub(crate) type Node = (usize, usize, usize);

/// The tiles of a board and what crosses each of their edges.
pub(crate) struct Tiles {
    /// How many grid cells a side of a tile has.
    pub(crate) cells: usize,
    pub(crate) nx: usize,
    pub(crate) ny: usize,
    /// How many tracks cross each edge to the next tile in x, by layer: the edge of tile
    /// `(a, b)` and `(a + 1, b)` at `b * nx + a`.
    pub(crate) across_x: [Vec<u32>; 2],
    /// The same in y: the edge of `(a, b)` and `(a, b + 1)` at `b * nx + a`.
    pub(crate) across_y: [Vec<u32>; 2],
    /// How many vias each tile holds.
    pub(crate) vias: Vec<u32>,
}

/// One resource the lines share: an edge on a layer or a tile's vias.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Use {
    X(usize, usize),
    Y(usize, usize),
    Via(usize),
}

impl Tiles {
    /// The tiles over a grid of `cells_x` by `cells_y` cells (row by row, x fastest), where
    /// `track_open[layer]` says which cells a track's centre may use and `via_open` which a
    /// via's, as the fixed copper leaves them; `track_pitch` and `via_pitch` are the centre to
    /// centre distances of two tracks and of two vias, in cells.
    pub(crate) fn measure(
        k: usize,
        (cells_x, cells_y): (usize, usize),
        track_open: [&[bool]; 2],
        via_open: &[bool],
        track_pitch: f64,
        via_pitch: f64,
    ) -> Tiles {
        let (nx, ny) = (cells_x.div_ceil(k), cells_y.div_ceil(k));
        // How many tracks a run of open cells holds, along a line of cells.
        let tracks = |open: &mut dyn Iterator<Item = bool>| -> u32 {
            let mut total = 0;
            let mut run = 0u32;
            let close = |run: u32| {
                if run > 0 {
                    ((f64::from(run - 1) / track_pitch).floor() as u32) + 1
                } else {
                    0
                }
            };
            for o in open {
                if o {
                    run += 1;
                } else {
                    total += close(run);
                    run = 0;
                }
            }
            total + close(run)
        };
        let mut across_x = [vec![0; nx * ny], vec![0; nx * ny]];
        let mut across_y = [vec![0; nx * ny], vec![0; nx * ny]];
        for layer in [0, 1] {
            let open = track_open[layer];
            for b in 0..ny {
                for a in 0..nx {
                    // The first column of the next tile in x, and the first row of the next in y.
                    let i = (a + 1) * k;
                    if a + 1 < nx && i < cells_x {
                        let rows = b * k..((b + 1) * k).min(cells_y);
                        across_x[layer][b * nx + a] =
                            tracks(&mut rows.map(|j| open[j * cells_x + i]));
                    }
                    let j = (b + 1) * k;
                    if b + 1 < ny && j < cells_y {
                        let cols = a * k..((a + 1) * k).min(cells_x);
                        across_y[layer][b * nx + a] =
                            tracks(&mut cols.map(|i| open[j * cells_x + i]));
                    }
                }
            }
        }
        let mut vias = vec![0; nx * ny];
        for b in 0..ny {
            for a in 0..nx {
                let mut open = 0u32;
                for j in b * k..((b + 1) * k).min(cells_y) {
                    for i in a * k..((a + 1) * k).min(cells_x) {
                        open += u32::from(via_open[j * cells_x + i]);
                    }
                }
                vias[b * nx + a] = if open == 0 {
                    0
                } else {
                    ((f64::from(open) / (via_pitch * via_pitch)).floor() as u32).max(1)
                };
            }
        }
        Tiles {
            cells: k,
            nx,
            ny,
            across_x,
            across_y,
            vias,
        }
    }

    /// The tile a cell is in.
    pub(crate) fn of(&self, i: usize, j: usize) -> (usize, usize) {
        (
            (i / self.cells).min(self.nx - 1),
            (j / self.cells).min(self.ny - 1),
        )
    }

    fn index(&self, u: Use) -> usize {
        let n = self.nx * self.ny;
        match u {
            Use::X(layer, e) => layer * n + e,
            Use::Y(layer, e) => (2 + layer) * n + e,
            Use::Via(t) => 4 * n + t,
        }
    }

    fn capacity(&self, u: Use) -> u32 {
        match u {
            Use::X(layer, e) => self.across_x[layer][e],
            Use::Y(layer, e) => self.across_y[layer][e],
            Use::Via(t) => self.vias[t],
        }
    }

    /// How many places a line may take on the edge `u` (its capacity, at most [`MOST_SLOTS`]):
    /// the tracks across an edge are in the order of their places along it, the same from the
    /// tiles on both sides, which is what makes a crossing in a tile something to count.
    fn slots(&self, u: Use) -> usize {
        (self.capacity(u) as usize).min(MOST_SLOTS)
    }

    /// The edge of tile `(a, b)` on `layer` on `side`, if the board goes on past it.
    fn edge(&self, (a, b, layer): Node, side: usize) -> Option<Use> {
        let t = b * self.nx + a;
        match side {
            LOW_X => (a > 0).then(|| Use::X(layer, t - 1)),
            HIGH_X => (a + 1 < self.nx).then_some(Use::X(layer, t)),
            LOW_Y => (b > 0).then(|| Use::Y(layer, t - self.nx)),
            _ => (b + 1 < self.ny).then_some(Use::Y(layer, t)),
        }
    }

    fn uses(&self) -> usize {
        5 * self.nx * self.ny
    }

    /// The resource a move from one node to a neighbour takes, and what it costs before
    /// congestion.
    fn between(&self, from: Node, to: Node) -> (Use, u64) {
        let (a, b, l) = from;
        let (c, d, m) = to;
        if l != m {
            (Use::Via(b * self.nx + a), VIA_COST)
        } else if b == d {
            (Use::X(l, b * self.nx + a.min(c)), STEP_COST)
        } else {
            (Use::Y(l, b.min(d) * self.nx + a), STEP_COST)
        }
    }
}

/// What a line asks of the plan: the tiles (with layers) its two ends are in.
#[derive(Clone, Debug)]
pub(crate) struct Ends {
    pub(crate) from: Vec<Node>,
    pub(crate) to: Vec<Node>,
    /// The line's net: lines of one net may cross and share a place on an edge.
    pub(crate) net: usize,
}

/// The plan: for each line, the tiles its global route passes, in order, from one end to the
/// other (`None` for a line the plan was not asked about), and what is left over capacity.
pub(crate) struct Plan {
    pub(crate) paths: Vec<Option<Vec<Node>>>,
    /// The sum over every edge and tile of how far its use passes its capacity.
    pub(crate) overuse: u32,
    /// How many pairs of lines cross in a tile on a layer: one straight across it each way.
    pub(crate) crossings: u32,
    /// The rounds negotiated.
    pub(crate) rounds: usize,
    /// How many planned tracks or vias use each resource, by [`Tiles::index`].
    occupied: Vec<u32>,
}

impl Plan {
    /// How full each tile is on each layer: the most of its four edges' planned tracks over
    /// their capacity (an edge no track crosses is empty, and one with no room that a track
    /// crosses counts as twice full), row by row.
    pub(crate) fn fill(&self, tiles: &Tiles) -> [Vec<f64>; 2] {
        let (nx, ny) = (tiles.nx, tiles.ny);
        let ratio = |u: Use| -> f64 {
            let used = self.occupied[tiles.index(u)];
            match (used, tiles.capacity(u)) {
                (0, _) => 0.0,
                (_, 0) => 2.0,
                (used, cap) => f64::from(used) / f64::from(cap),
            }
        };
        let layer = |l: usize| -> Vec<f64> {
            (0..nx * ny)
                .map(|t| {
                    let (a, b) = (t % nx, t / nx);
                    let mut edges = Vec::with_capacity(4);
                    if a + 1 < nx {
                        edges.push(Use::X(l, t));
                    }
                    if a > 0 {
                        edges.push(Use::X(l, t - 1));
                    }
                    if b + 1 < ny {
                        edges.push(Use::Y(l, t));
                    }
                    if b > 0 {
                        edges.push(Use::Y(l, t - nx));
                    }
                    edges.into_iter().map(ratio).fold(0.0, f64::max)
                })
                .collect()
        };
        [layer(0), layer(1)]
    }
}

/// What a step costs, beyond the step, on a tile and layer this full outside a line's corridor:
/// nothing below [`CROWDED_FROM`], `cost` from [`CROWDED`], and in proportion between.
pub(crate) fn crowding_cost(fill: f64, cost: i64) -> i64 {
    let share = ((fill - CROWDED_FROM) / (CROWDED - CROWDED_FROM)).clamp(0.0, 1.0);
    (share * cost as f64).round() as i64
}

/// Where a line meets the rim of a tile: inside it (the line ends there or changes layer), or
/// at a place on one of its sides.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rim {
    Inside,
    At { side: usize, slot: usize },
}

impl Rim {
    /// How far round the rim a place is, counterclockwise from the corner of the lowest column
    /// and row. The places on an edge are in one order, seen from either tile, so two lines
    /// through a tile cross when their places alternate round its rim.
    fn around(self) -> Option<u32> {
        match self {
            Rim::Inside => None,
            Rim::At { side, slot } => {
                let slot = slot as u32;
                Some(match side {
                    LOW_Y => slot,
                    HIGH_X => 100 + slot,
                    HIGH_Y => 300 - slot,
                    _ => 400 - slot,
                })
            }
        }
    }

    /// The state's number of this place, from 0.
    fn index(self) -> usize {
        match self {
            Rim::Inside => 0,
            Rim::At { side, slot } => 1 + side * MOST_SLOTS + slot,
        }
    }

    fn from_index(k: usize) -> Rim {
        if k == 0 {
            Rim::Inside
        } else {
            Rim::At {
                side: (k - 1) / MOST_SLOTS,
                slot: (k - 1) % MOST_SLOTS,
            }
        }
    }
}

/// How many places a state of the search tells apart in a tile: inside, or one of the places
/// on its four sides.
const RIMS: usize = 1 + 4 * MOST_SLOTS;

/// The side across from a side: a line leaving a tile by one enters the next by the other.
fn facing(side: usize) -> usize {
    match side {
        LOW_X => HIGH_X,
        HIGH_X => LOW_X,
        LOW_Y => HIGH_Y,
        _ => LOW_Y,
    }
}

/// A line's way through a tile on a layer: the places it enters and leaves by round the rim
/// ([`Rim::around`]), `None` for inside, and its net.
#[derive(Clone, Copy)]
struct Chord {
    from: Option<u32>,
    to: Option<u32>,
    net: usize,
}

impl Chord {
    /// Whether two ways through one tile must cross: both go rim to rim, and their places
    /// alternate round it. A way that ends inside the tile can always go round the other.
    fn crosses(&self, other: &Chord) -> bool {
        let (Some(p), Some(q), Some(r), Some(s)) = (self.from, self.to, other.from, other.to)
        else {
            return false;
        };
        let (lo, hi) = (p.min(q), p.max(q));
        let inside = |x: u32| lo < x && x < hi;
        let ends = [p, q];
        if ends.contains(&r) || ends.contains(&s) {
            return false;
        }
        inside(r) != inside(s)
    }
}

/// What the lines routed so far in a round leave for the next, and the history of the rounds
/// before.
struct Congestion<'a> {
    occupied: &'a [u32],
    history: &'a [u64],
    /// The nets on each place of each edge, at `index(edge) * MOST_SLOTS + slot`.
    taken: &'a [Vec<usize>],
    /// The ways lines take through each tile on each layer, at `layer * tiles + tile`.
    chords: &'a [Vec<Chord>],
    /// How often lines of different nets crossed, or shared a place, in each tile on each
    /// layer, over the rounds before.
    cross_history: &'a [u64],
    present: u64,
}

/// A planned line: each tile it passes, and where it enters that tile.
type Steps = Vec<(Node, Rim)>;

/// Count a planned line in what it occupies: the edges and vias it uses, the places it takes on
/// edges and its ways through tiles.
fn occupy(
    tiles: &Tiles,
    steps: &Steps,
    net: usize,
    occupied: &mut [u32],
    taken: &mut [Vec<usize>],
    chords: &mut [Vec<Chord>],
) {
    let n = tiles.nx * tiles.ny;
    for (i, &(here, enter)) in steps.iter().enumerate() {
        let leave = match steps.get(i + 1) {
            Some(&(next, Rim::At { side, slot })) => {
                let (u, _) = tiles.between(here, next);
                occupied[tiles.index(u)] += 1;
                taken[tiles.index(u) * MOST_SLOTS + slot].push(net);
                Rim::At {
                    side: facing(side),
                    slot,
                }
            }
            Some(&(next, Rim::Inside)) => {
                let (u, _) = tiles.between(here, next);
                occupied[tiles.index(u)] += 1;
                Rim::Inside
            }
            None => Rim::Inside,
        };
        chords[here.2 * n + here.1 * tiles.nx + here.0].push(Chord {
            from: enter.around(),
            to: leave.around(),
            net,
        });
    }
}

/// Route every line of `ends` on the tiles by negotiated congestion (see the module's header),
/// in `order`.
pub(crate) fn negotiate(tiles: &Tiles, ends: &[Option<Ends>], order: &[usize]) -> Plan {
    let n = tiles.uses();
    let tiles_n = tiles.nx * tiles.ny;
    let mut history = vec![0u64; n];
    let mut cross_history = vec![0u64; 2 * tiles_n];
    let mut paths: Vec<Option<Vec<Node>>> = vec![None; ends.len()];
    let mut best: Option<Plan> = None;
    let mut present = PRESENT_START;
    for round in 1..=ROUNDS {
        let mut occupied = vec![0u32; n];
        let mut taken: Vec<Vec<usize>> = vec![Vec::new(); n * MOST_SLOTS];
        let mut chords: Vec<Vec<Chord>> = vec![Vec::new(); 2 * tiles_n];
        for &line in order {
            let Some(e) = &ends[line] else { continue };
            let congestion = Congestion {
                occupied: &occupied,
                history: &history,
                taken: &taken,
                chords: &chords,
                cross_history: &cross_history,
                present,
            };
            let steps = search(tiles, e, &congestion);
            occupy(tiles, &steps, e.net, &mut occupied, &mut taken, &mut chords);
            paths[line] = Some(steps.into_iter().map(|(node, _)| node).collect());
        }
        let mut overuse = 0;
        for u in all_uses(tiles) {
            let k = tiles.index(u);
            let over = occupied[k].saturating_sub(tiles.capacity(u));
            overuse += over;
            history[k] += u64::from(over);
        }
        let mut crossings = 0;
        for (t, ways) in chords.iter().enumerate() {
            let mut here = 0;
            for (i, c) in ways.iter().enumerate() {
                here += ways[i + 1..]
                    .iter()
                    .filter(|d| d.net != c.net && c.crosses(d))
                    .count() as u32;
            }
            crossings += here;
            cross_history[t] += u64::from(here);
        }
        for u in all_uses(tiles) {
            let (Use::X(layer, e) | Use::Y(layer, e)) = u else {
                continue;
            };
            for slot in 0..MOST_SLOTS {
                let nets = &taken[tiles.index(u) * MOST_SLOTS + slot];
                let mut shared = 0;
                for (i, a) in nets.iter().enumerate() {
                    shared += nets[i + 1..].iter().filter(|b| *b != a).count() as u32;
                }
                crossings += shared;
                cross_history[layer * tiles_n + e] += u64::from(shared);
            }
        }
        if best
            .as_ref()
            .is_none_or(|b| overuse + crossings < b.overuse + b.crossings)
        {
            best = Some(Plan {
                paths: paths.clone(),
                overuse,
                crossings,
                rounds: round,
                occupied: occupied.clone(),
            });
        }
        if overuse == 0 && crossings == 0 {
            break;
        }
        present = (present * 2).min(1 << 20);
    }
    let mut plan = best.unwrap_or(Plan {
        paths,
        overuse: 0,
        crossings: 0,
        rounds: 0,
        occupied: vec![0; n],
    });
    plan.rounds = plan.rounds.max(1);
    plan
}

fn all_uses(tiles: &Tiles) -> impl Iterator<Item = Use> + '_ {
    let n = tiles.nx * tiles.ny;
    (0..2)
        .flat_map(move |l| (0..n).map(move |e| Use::X(l, e)))
        .chain((0..2).flat_map(move |l| (0..n).map(move |e| Use::Y(l, e))))
        .chain((0..n).map(Use::Via))
}

/// The cheapest path of tiles between a line's ends, given the congestion: A* with the
/// Manhattan distance in steps, over states of a tile, a layer and the place the line entered
/// the tile by, so that a line pays for each line of another net it crosses in a tile and for
/// a place on an edge another net has taken.
fn search(tiles: &Tiles, ends: &Ends, congestion: &Congestion) -> Steps {
    let (nx, ny) = (tiles.nx, tiles.ny);
    let tiles_n = nx * ny;
    let id = |(a, b, l): Node, rim: Rim| ((l * ny + b) * nx + a) * RIMS + rim.index();
    let state = |s: usize| {
        let (k, rim) = (s / RIMS, Rim::from_index(s % RIMS));
        ((k % nx, (k / nx) % ny, k / tiles_n), rim)
    };
    let heuristic = |(a, b, _): Node| -> u64 {
        ends.to
            .iter()
            .map(|&(c, d, _)| (a.abs_diff(c) + b.abs_diff(d)) as u64 * STEP_COST)
            .min()
            .unwrap_or(0)
    };
    // The straight line between the middles of the two ends, in tiles: a tile off it costs
    // more, so that of the many paths of one length the plan takes the one a track would.
    let middle = |nodes: &[Node]| {
        let n = nodes.len().max(1) as f64;
        (
            nodes.iter().map(|t| t.0 as f64).sum::<f64>() / n,
            nodes.iter().map(|t| t.1 as f64).sum::<f64>() / n,
        )
    };
    let (p, q) = (middle(&ends.from), middle(&ends.to));
    let off_line = |(a, b, _): Node| -> u64 {
        let (x, y) = (a as f64, b as f64);
        let (dx, dy) = (q.0 - p.0, q.1 - p.1);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 {
            (((x - p.0) * dx + (y - p.1) * dy) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let d = ((x - p.0 - t * dx).powi(2) + (y - p.1 - t * dy).powi(2)).sqrt();
        (d * DEVIATION as f64).round() as u64
    };
    let present = congestion.present;
    let cost_of = |u: Use, base: u64| -> u64 {
        let k = tiles.index(u);
        let over = u64::from((congestion.occupied[k] + 1).saturating_sub(tiles.capacity(u)));
        base * (1 + congestion.history[k]) * (100 + present * over) / 100
    };
    // What crossing `n` lines of other nets, or sharing a place with them, costs in a tile.
    let disorder = |(a, b, l): Node, n: usize| -> u64 {
        if n == 0 {
            return 0;
        }
        let t = l * tiles_n + b * nx + a;
        CROSS_COST * n as u64 * (1 + congestion.cross_history[t]) * (100 + present) / 100
    };
    let crossed = |here: Node, enter: Rim, leave: Rim| -> usize {
        let way = Chord {
            from: enter.around(),
            to: leave.around(),
            net: ends.net,
        };
        if way.from.is_none() || way.to.is_none() {
            return 0;
        }
        congestion.chords[here.2 * tiles_n + here.1 * nx + here.0]
            .iter()
            .filter(|c| c.net != ends.net && way.crosses(c))
            .count()
    };
    let states = 2 * tiles_n * RIMS;
    let mut cost = vec![u64::MAX; states];
    let mut came = vec![usize::MAX; states];
    let mut done = vec![false; states];
    let mut queue = BinaryHeap::new();
    let mut sequence = 0u64;
    for &s in &ends.from {
        let k = id(s, Rim::Inside);
        if cost[k] != 0 {
            cost[k] = 0;
            queue.push(Reverse((heuristic(s), sequence, k)));
            sequence += 1;
        }
    }
    while let Some(Reverse((_, _, k))) = queue.pop() {
        if done[k] {
            continue;
        }
        done[k] = true;
        let (here, entered) = state(k);
        if ends.to.contains(&here) {
            let mut steps = vec![(here, entered)];
            let mut at = k;
            while came[at] != usize::MAX {
                at = came[at];
                steps.push(state(at));
            }
            steps.reverse();
            return steps;
        }
        let (a, b, l) = here;
        let so_far = cost[k];
        let mut relax = |to: Node, rim: Rim, c: u64, queue: &mut BinaryHeap<_>| {
            let t = id(to, rim);
            if c < cost[t] {
                cost[t] = c;
                came[t] = k;
                queue.push(Reverse((c + heuristic(to), sequence, t)));
                sequence += 1;
            }
        };
        for side in [HIGH_X, HIGH_Y, LOW_X, LOW_Y] {
            let Some(u) = tiles.edge(here, side) else {
                continue;
            };
            let to = match side {
                LOW_X => (a - 1, b, l),
                HIGH_X => (a + 1, b, l),
                LOW_Y => (a, b - 1, l),
                _ => (a, b + 1, l),
            };
            let step = so_far + cost_of(u, STEP_COST) + off_line(to);
            for slot in 0..tiles.slots(u).max(1) {
                let others = congestion.taken[tiles.index(u) * MOST_SLOTS + slot]
                    .iter()
                    .filter(|&&net| net != ends.net)
                    .count();
                let leave = Rim::At { side, slot };
                let c = step + disorder(here, others + crossed(here, entered, leave));
                let enter = Rim::At {
                    side: facing(side),
                    slot,
                };
                relax(to, enter, c, &mut queue);
            }
        }
        let to = (a, b, 1 - l);
        let c = so_far + cost_of(Use::Via(b * nx + a), VIA_COST) + off_line(to);
        relax(to, Rim::Inside, c, &mut queue);
    }
    // The graph is connected, so this is not reached; an end with no way is planned as itself.
    ends.from
        .iter()
        .take(1)
        .map(|&node| (node, Rim::Inside))
        .collect()
}

/// The tiles of a line's corridor: those of its plan, on either layer, grown by `grow` tiles
/// each way (see the module's header), row by row.
pub(crate) fn corridor(tiles: &Tiles, path: &[Node], grow: usize) -> Vec<bool> {
    let mut inside = vec![false; tiles.nx * tiles.ny];
    for &(a, b, _) in path {
        for d in b.saturating_sub(grow)..=(b + grow).min(tiles.ny - 1) {
            for c in a.saturating_sub(grow)..=(a + grow).min(tiles.nx - 1) {
                inside[d * tiles.nx + c] = true;
            }
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Open cells everywhere on a grid of `x` by `y` cells.
    fn open(x: usize, y: usize) -> Vec<bool> {
        vec![true; x * y]
    }

    #[test]
    fn an_edge_holds_as_many_tracks_as_fit_in_its_open_cells() {
        // Two tiles side by side, 20 by 10 cells. Track pitch 2 cells (0.2 mm): an open edge of
        // 10 cells holds (10 - 1) / 2 + 1 = 5 tracks.
        let all = open(20, 10);
        let t = Tiles::measure(10, (20, 10), [&all, &all], &all, 2.0, 5.0);
        assert_eq!((t.nx, t.ny), (2, 1));
        assert_eq!(t.across_x[0][0], 5);
        // 100 cells for vias at 5 cells a via: 4 vias.
        assert_eq!(t.vias, vec![4, 4]);

        // A pad over rows 3 to 6 of the edge's column on the front leaves runs of 3 and 3:
        // 2 + 2 tracks. The back is unchanged.
        let mut front = open(20, 10);
        for j in 3..7 {
            front[j * 20 + 10] = false;
        }
        let t = Tiles::measure(10, (20, 10), [&front, &all], &all, 2.0, 5.0);
        assert_eq!(t.across_x[0][0], 4);
        assert_eq!(t.across_x[1][0], 5);

        // A wall closes the edge.
        let mut wall = open(20, 10);
        for j in 0..10 {
            wall[j * 20 + 10] = false;
        }
        let t = Tiles::measure(10, (20, 10), [&wall, &wall], &all, 2.0, 5.0);
        assert_eq!(t.across_x, [vec![0, 0], vec![0, 0]]);
    }

    #[test]
    fn two_lines_through_a_tile_edge_of_capacity_one_are_planned_apart() {
        // Three columns of tiles and two rows; the edges between columns 0 and 1 hold one track,
        // and the back layer is closed, so one of two lines from column 0 to column 2 must use
        // the other row.
        let tiles = Tiles {
            cells: 10,
            nx: 3,
            ny: 2,
            across_x: [vec![1, 1, 0, 1, 1, 0], vec![0; 6]],
            across_y: [vec![5, 5, 5, 0, 0, 0], vec![0; 6]],
            vias: vec![0; 6],
        };
        let line = || {
            Some(Ends {
                from: vec![(0, 0, 0)],
                to: vec![(2, 0, 0)],
                net: 0,
            })
        };
        let plan = negotiate(&tiles, &[line(), line()], &[0, 1]);
        assert_eq!(plan.overuse, 0);
        let rows = |p: &Option<Vec<Node>>| {
            p.as_ref()
                .unwrap()
                .iter()
                .filter(|n| n.0 == 1)
                .map(|n| n.1)
                .collect::<Vec<_>>()
        };
        assert_ne!(rows(&plan.paths[0]), rows(&plan.paths[1]));
        // The same plan every time.
        let again = negotiate(&tiles, &[line(), line()], &[0, 1]);
        assert_eq!(plan.paths, again.paths);
    }

    /// A grid of `n` by `n` tiles with room for many tracks on every edge and many vias.
    fn roomy(n: usize) -> Tiles {
        Tiles {
            cells: 10,
            nx: n,
            ny: n,
            across_x: [vec![8; n * n], vec![8; n * n]],
            across_y: [vec![8; n * n], vec![8; n * n]],
            vias: vec![8; n * n],
        }
    }

    /// The vias of a planned path: its moves to the other layer in the same tile.
    fn vias_of(path: &Option<Vec<Node>>) -> Vec<(usize, usize)> {
        path.as_ref()
            .unwrap()
            .windows(2)
            .filter(|w| w[0].2 != w[1].2)
            .map(|w| (w[0].0, w[0].1))
            .collect()
    }

    #[test]
    fn two_lines_that_must_cross_in_a_tile_are_planned_with_a_via() {
        // On a grid of 5 by 5 tiles, a line runs across the middle row on the front and another
        // down the middle column, from the front to a pad on both layers. They must cross in
        // the middle tile (a way round the first line's end is four tiles longer), so the plan
        // gives one of them one via, and no tile on a layer has a line straight across it each
        // way.
        let tiles = roomy(5);
        let across = Some(Ends {
            from: vec![(0, 2, 0)],
            to: vec![(4, 2, 0)],
            net: 0,
        });
        let down = Some(Ends {
            from: vec![(2, 0, 0)],
            to: vec![(2, 4, 0), (2, 4, 1)],
            net: 1,
        });
        let plan = negotiate(&tiles, &[across.clone(), down], &[0, 1]);
        assert_eq!(plan.crossings, 0);
        assert_eq!(
            vias_of(&plan.paths[0]).len() + vias_of(&plan.paths[1]).len(),
            1
        );
        // With both ends of both lines on the front, the line that changes layer comes back:
        // two vias, one on each side of the crossing.
        let down_front = Some(Ends {
            from: vec![(2, 0, 0)],
            to: vec![(2, 4, 0)],
            net: 1,
        });
        let plan = negotiate(&tiles, &[across, down_front], &[0, 1]);
        assert_eq!(plan.crossings, 0);
        assert_eq!(
            vias_of(&plan.paths[0]).len() + vias_of(&plan.paths[1]).len(),
            2
        );
    }

    #[test]
    fn lines_that_need_not_cross_are_planned_without_a_via() {
        // Two lines side by side across the grid, and two that turn away from each other in
        // the middle tile (one in from the west and up, one in from the east and down): no
        // line crosses another, so none changes layer.
        let tiles = roomy(3);
        let line = |from: Node, to: Node, net: usize| {
            Some(Ends {
                from: vec![from],
                to: vec![to],
                net,
            })
        };
        let plan = negotiate(
            &tiles,
            &[line((0, 1, 0), (2, 1, 0), 0), line((0, 0, 0), (2, 0, 0), 1)],
            &[0, 1],
        );
        assert_eq!(plan.crossings, 0);
        assert!(plan.paths.iter().all(|p| vias_of(p).is_empty()));
        let plan = negotiate(
            &tiles,
            &[line((0, 1, 0), (1, 0, 0), 0), line((2, 1, 0), (1, 2, 0), 1)],
            &[0, 1],
        );
        assert_eq!(plan.crossings, 0);
        assert!(plan.paths.iter().all(|p| vias_of(p).is_empty()));
    }

    #[test]
    fn a_tile_is_as_full_as_its_fullest_edge_on_its_layer() {
        // Two tiles side by side whose edge holds two tracks on the front and none on the back;
        // two lines cross it on the front.
        let tiles = Tiles {
            cells: 10,
            nx: 2,
            ny: 1,
            across_x: [vec![2, 0], vec![0, 0]],
            across_y: [vec![0, 0], vec![0, 0]],
            vias: vec![0, 0],
        };
        let line = || {
            Some(Ends {
                from: vec![(0, 0, 0)],
                to: vec![(1, 0, 0)],
                net: 0,
            })
        };
        let plan = negotiate(&tiles, &[line(), line()], &[0, 1]);
        assert_eq!(plan.overuse, 0);
        assert_eq!(plan.fill(&tiles), [vec![1.0, 1.0], vec![0.0, 0.0]]);
        // One line fills the edge halfway.
        let plan = negotiate(&tiles, &[line()], &[0]);
        assert_eq!(plan.fill(&tiles)[0], vec![0.5, 0.5]);
    }

    #[test]
    fn a_step_costs_nothing_where_the_plan_leaves_room_and_the_whole_cost_where_it_is_full() {
        assert_eq!(crowding_cost(0.0, 20), 0);
        assert_eq!(crowding_cost(0.5, 20), 0);
        assert_eq!(crowding_cost(0.7, 20), 0);
        assert_eq!(crowding_cost(0.85, 20), 10);
        assert_eq!(crowding_cost(1.0, 20), 20);
        assert_eq!(crowding_cost(2.0, 20), 20);
    }

    #[test]
    fn a_corridor_is_the_plan_grown_by_one_tile() {
        let tiles = Tiles {
            cells: 10,
            nx: 5,
            ny: 3,
            across_x: [vec![1; 15], vec![1; 15]],
            across_y: [vec![1; 15], vec![1; 15]],
            vias: vec![1; 15],
        };
        let inside = corridor(&tiles, &[(0, 0, 0), (1, 0, 1)], 1);
        let row = |b: usize| inside[b * 5..b * 5 + 5].to_vec();
        assert_eq!(row(0), vec![true, true, true, false, false]);
        assert_eq!(row(1), vec![true, true, true, false, false]);
        assert_eq!(row(2), vec![false; 5]);
    }
}
