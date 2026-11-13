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
//! **The corridor** ([`corridor`]) of a line is the tiles of its plan, on both layers, grown
//! by one tile each way; a line wider than the narrowest has none. The detailed search pays a cost for each step outside it; it is not a
//! wall, so a line the plan got wrong still finds its way.
//!
//! **When** ([`crate::route::CoarsePlan`], `RouteOptions::plan`): off by default. `EveryTry`
//! follows the plan on every try; `FromSecondTry` routes the first order again with it as the
//! second try, as the escape lanes do, so a board that routes on its first try is unchanged.
//! `netsmith route --plan every|later` turns it on for experiments.
//!
//! # Measured (issue #431, S10, 2026-11-12)
//!
//! Main at 1852adb (#550), placements saved by main's `fab`, one try with no time limit at the
//! least track (`netsmith route <placed> --tries 1 --route-timeout 0 --margin 1 --turn T`). Lines
//! left at T = 24 / 25 / 26 / 27, and vias at 25:
//!
//! | | RP2350 | Orbit | EasyDuino |
//! |---|---|---|---|
//! | main | 0 / 0 / 0 / 1, 137 vias | 0 / 0 / 0 / 0, 91 | 0 / 0 / 0 / 0, 105 |
//! | the plan on every try (built) | **0 / 0 / 0 / 0**, 209 vias | 0 / 0 / 0 / 0, 109 | 0 / 0 / 0 / 0, 115 |
//!
//! At 25, routed length over the spanning tree: RP2350 1.43x against 1.40x, Orbit 1.26x against
//! 1.27x, EasyDuino 1.18x against 1.21x; other nets' track on RP2350's GND pour 706 mm against
//! 647, Orbit's 407 against 464; EasyDuino's pair coupled 22 % against 23 %, skew 0.21 mm
//! against 0.75. A try of RP2350 takes 76 to 80 s against 54 to 57 (Orbit 26 to 29 against 19).
//! The plan makes RP2350 route at every turn cost and makes its result nearly the same at each
//! (a sign that it no longer depends on luck), but costs a third more vias on RP2350 and a
//! tenth on the others, and a third more time. From the second try, RP2350 with two tries
//! leaves no line at any turn cost, as main does (143 against 149 vias at 27, where the second
//! try runs; the others route on their first try and are byte for byte main's). So neither
//! form meets the bar of no more vias and no more time, and the plan is off.
//!
//! **Why the vias.** On a board of two layers a plan's two lines that cross in a tile do so on
//! different layers, and the plan does not count crossings. Without a plan the detailed search
//! goes round a track that is in its way; inside a corridor three tiles wide that detour costs,
//! so it crosses by a via, and a power net's crossing becomes a cluster of vias. The pictures of
//! the first pass are alike; the vias come in the repairs.
//!
//! **Tried and not kept** (same boards and runs; lines left at 25 and 27, vias at 25):
//!
//! - The capacity of an edge counting pads as closed, and paths of tiles with no pull toward the
//!   straight line: 100 to 190 tracks over capacity after ten rounds, L-shaped corridors, RP2350
//!   2 and 4 lines left, EasyDuino 130 vias. Counting a pad's copper as a way across (each pad is
//!   its own line's way out) and the pull ([`DEVIATION`]) bring it to 0 to 5 over capacity.
//! - A step outside the corridor at 2 or 4 (against 10): RP2350 177 to 197 vias, one line left
//!   at 27 with 2. Corridors grown by two tiles: 150 to 180 vias. Tiles of 2 mm: 175 to 181.
//!   The plan in the first pass only (the repairs free): 175 to 202 vias.
//! - A cost only in tiles the plan fills (where its lines take every track of an edge, and the
//!   tile is not the line's): almost no tile is full (0 to 8 of 1300 to 2800), so it is main.
//!
//! **What would make it pass**, in my view: a plan that knows the order of tracks across an
//! edge, so two lines that cross are planned as a crossing with a via (a cost the plan pays and
//! the capacity counts), or the corridor as a cost on the layer the plan chose, with the plan's
//! via places; and a way to keep the detailed search's free detours where the plan has room
//! (a cost that grows with the tile's use, not a flat one outside the corridor).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// How many grid cells a side of a tile has: 10, a tile of 1 mm.
pub(crate) const TILE_CELLS: usize = 10;

/// What a step to the next tile costs before congestion, and a via.
const STEP_COST: u64 = 100;
const VIA_COST: u64 = 150;
/// What entering a tile costs for each tile it is from the straight line between a line's ends.
const DEVIATION: u64 = 30;

/// The rounds of negotiation, and the present-use factor of the first (in hundredths; it
/// doubles each round).
const ROUNDS: usize = 10;
const PRESENT_START: u64 = 50;

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
}

/// The plan: for each line, the tiles its global route passes, in order, from one end to the
/// other (`None` for a line the plan was not asked about), and what is left over capacity.
pub(crate) struct Plan {
    pub(crate) paths: Vec<Option<Vec<Node>>>,
    /// The sum over every edge and tile of how far its use passes its capacity.
    pub(crate) overuse: u32,
    /// The rounds negotiated.
    pub(crate) rounds: usize,
}

/// Route every line of `ends` on the tiles by negotiated congestion (see the module's header),
/// in `order`.
pub(crate) fn negotiate(tiles: &Tiles, ends: &[Option<Ends>], order: &[usize]) -> Plan {
    let n = tiles.uses();
    let mut history = vec![0u64; n];
    let mut paths: Vec<Option<Vec<Node>>> = vec![None; ends.len()];
    let mut best: Option<Plan> = None;
    let mut present = PRESENT_START;
    for round in 1..=ROUNDS {
        let mut occupied = vec![0u32; n];
        for &line in order {
            let Some(e) = &ends[line] else { continue };
            let path = search(tiles, e, &occupied, &history, present);
            for w in path.windows(2) {
                occupied[tiles.index(tiles.between(w[0], w[1]).0)] += 1;
            }
            paths[line] = Some(path);
        }
        let mut overuse = 0;
        for u in all_uses(tiles) {
            let k = tiles.index(u);
            let over = occupied[k].saturating_sub(tiles.capacity(u));
            overuse += over;
            history[k] += u64::from(over);
        }
        if best.as_ref().is_none_or(|b| overuse < b.overuse) {
            best = Some(Plan {
                paths: paths.clone(),
                overuse,
                rounds: round,
            });
        }
        if overuse == 0 {
            break;
        }
        present = (present * 2).min(1 << 20);
    }
    let mut plan = best.unwrap_or(Plan {
        paths,
        overuse: 0,
        rounds: 0,
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

/// The cheapest path of tiles between a line's ends, given what the lines routed this round
/// occupy and the history: A* with the Manhattan distance in steps.
fn search(
    tiles: &Tiles,
    ends: &Ends,
    occupied: &[u32],
    history: &[u64],
    present: u64,
) -> Vec<Node> {
    let (nx, ny) = (tiles.nx, tiles.ny);
    let id = |(a, b, l): Node| (l * ny + b) * nx + a;
    let node = |s: usize| (s % nx, (s / nx) % ny, s / (nx * ny));
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
    let cost_of = |u: Use, base: u64| -> u64 {
        let k = tiles.index(u);
        let over = u64::from((occupied[k] + 1).saturating_sub(tiles.capacity(u)));
        base * (1 + history[k]) * (100 + present * over) / 100
    };
    let states = 2 * nx * ny;
    let mut cost = vec![u64::MAX; states];
    let mut came = vec![usize::MAX; states];
    let mut queue = BinaryHeap::new();
    let mut sequence = 0u64;
    for &s in &ends.from {
        let k = id(s);
        if cost[k] != 0 {
            cost[k] = 0;
            queue.push(Reverse((heuristic(s), sequence, k)));
            sequence += 1;
        }
    }
    while let Some(Reverse((_, _, k))) = queue.pop() {
        let here = node(k);
        if ends.to.contains(&here) {
            let mut path = vec![here];
            let mut at = k;
            while came[at] != usize::MAX {
                at = came[at];
                path.push(node(at));
            }
            path.reverse();
            return path;
        }
        let (a, b, l) = here;
        let mut next = Vec::with_capacity(5);
        if a + 1 < nx {
            next.push((a + 1, b, l));
        }
        if b + 1 < ny {
            next.push((a, b + 1, l));
        }
        if a > 0 {
            next.push((a - 1, b, l));
        }
        if b > 0 {
            next.push((a, b - 1, l));
        }
        next.push((a, b, 1 - l));
        for to in next {
            let (u, base) = tiles.between(here, to);
            let c = cost[k] + cost_of(u, base) + off_line(to);
            let t = id(to);
            if c < cost[t] {
                cost[t] = c;
                came[t] = k;
                queue.push(Reverse((c + heuristic(to), sequence, t)));
                sequence += 1;
            }
        }
    }
    // The graph is connected, so this is not reached; an end with no way is planned as itself.
    ends.from.iter().take(1).copied().collect()
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
