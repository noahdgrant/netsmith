//! The search: A* over the masks, the paths it finds and the runs a path is drawn as.

use super::*;

/// A cell on a layer: where a path starts or ends, and so is open whatever the masks say.
pub(super) type Spot = (usize, usize, usize);

/// Cells a path may also start or end on, by way of a via in the pad: the cell of the pad's
/// other layer. Reaching one costs [`VIA_IN_PAD`] more.
#[derive(Clone, Copy, Default)]
pub(super) struct Extras<'a> {
    pub(super) from: &'a [Spot],
    pub(super) to: &'a [Spot],
    /// Whether the path may turn by a right angle, at [`TURN_RIGHT`], for the clean-up of
    /// [`Router::settled`] to cut into two bends of 45 degrees (issue #415).
    pub(super) right_angles: bool,
    /// Four directions and free right angles, as a search that only finds which nets are in a
    /// line's way does: its path is never drawn, and eight directions cost it three times the
    /// work.
    pub(super) coarse: bool,
}

/// Where a path is to end.
#[derive(Clone, Copy)]
pub(super) enum Goal<'a> {
    /// Any of these cells.
    Spots(&'a [Spot]),
    /// The nearest cell where a via may go: the path is to a plane, which a via reaches.
    Via,
}

/// How a search ended without a path.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Miss {
    /// Every cell the search could reach was tried.
    Blocked,
    /// The search was stopped, at the time limit.
    Stopped,
}

/// Whether a path can exist at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Link {
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
pub(super) fn link(masks: &Masks, from: &[Spot], goal: Goal, extras: Extras) -> Link {
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
pub(super) const CLOCK_EVERY: u32 = 1 << 14;

/// The cheapest path from any of `from` to the goal, as cells with their layers, within the
/// masks' window. `stop` is asked now and then whether to give up.
pub(super) fn search(
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
                    let d = if extras.coarse {
                        (a + b) * STEP
                    } else {
                        a.min(b) * DIAGONAL + (a - b).abs() * STEP
                    };
                    d * masks.estimate.0 / masks.estimate.1
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
    while let Some(Reverse((_, g, _, s))) = queue.pop() {
        taken += 1;
        if taken.is_multiple_of(CLOCK_EVERY) && stop() {
            return Err(Miss::Stopped);
        }
        // A state queued again when a cheaper way to it was found is taken from the queue first
        // at that cost (its estimate is the same, the cost lower); the dearer entries left are
        // stale, and taking them again would relax nothing.
        if s < states && -g > cost[s] {
            continue;
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
        moves(masks, &open, extras, spot, dir, |next, next_dir, step| {
            let n = state(next, next_dir);
            if here + step < cost[n] {
                cost[n] = here + step;
                came_from[n] = s;
                queue.push(Reverse((cost[n] + heuristic(next), -cost[n], sequence, n)));
                sequence += 1;
            }
        });
    }
    Err(Miss::Blocked)
}

/// The moves of the search from a state, in the order the search queues them: each legal next
/// state of `spot`, come in by direction `dir`, with what the move costs, given to `relax`.
/// [`search`] and [`reachable`] share it, so they agree on which states can be reached.
pub(super) fn moves(
    masks: &Masks,
    open: &impl Fn(Spot) -> bool,
    extras: Extras,
    spot: Spot,
    dir: usize,
    mut relax: impl FnMut(Spot, usize, i64),
) {
    let window = masks.window;
    let (i, j, layer) = spot;
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
            && !(passable((i as isize + di, j as isize)) && passable((i as isize, j as isize + dj)))
        {
            continue;
        }
        let next = (ni as usize, nj as usize, layer);
        let turn = match bend {
            0 => 0,
            _ if extras.coarse || free => masks.turn,
            1 => masks.turn,
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
            masks.follow_cost(
                layer,
                next.0,
                next.1,
                if diagonal { DIAGONAL } else { STEP },
            ) + turn
                + neck
                + masks.history_cost(layer, next.0, next.1)
                + masks.soft_cost(layer, next.0, next.1)
                + masks.lane_cost(layer, next.0, next.1)
                + masks.off_plan_cost(layer, next.0, next.1)
                + masks.pour.0[layer],
        );
    }
    let other = 1 - layer;
    let at = masks.at(i, j);
    if dir != AFTER_VIA && !masks.via[at] && (!masks.track[other][at] || open((i, j, other))) {
        relax(
            (i, j, other),
            AFTER_VIA,
            VIA + masks.pour.1 + masks.soft_via_cost(i, j) + masks.lane_via_cost(i, j),
        );
    }
}

/// Whether a search with these masks, ends and moves can reach the goal at all: a flood over
/// the search's states (cell, layer and the direction come in by) with [`moves`], which costs
/// far less than a search that runs out of states, and says the same.
pub(super) fn reachable(
    masks: &Masks,
    from: &[Spot],
    goal: Goal,
    extras: Extras,
    stop: &dyn Fn() -> bool,
) -> Result<bool, Miss> {
    let window = masks.window;
    let cells = window.cells();
    let state =
        |(i, j, layer): Spot, dir: usize| (layer * cells + window.index(i, j)) * DIRECTIONS + dir;
    let open = |spot: Spot| {
        from.contains(&spot)
            || extras.from.contains(&spot)
            || extras.to.contains(&spot)
            || matches!(goal, Goal::Spots(to) if to.contains(&spot))
    };
    let done = |spot: Spot| match goal {
        Goal::Spots(to) => to.contains(&spot) || extras.to.contains(&spot),
        Goal::Via => !masks.via[masks.at(spot.0, spot.1)],
    };
    let mut seen = vec![false; cells * 2 * DIRECTIONS];
    let mut stack: Vec<(Spot, usize)> = Vec::new();
    for &spot in from
        .iter()
        .chain(extras.from)
        .filter(|s| window.contains(s.0, s.1))
    {
        let s = state(spot, NO_DIRECTION);
        if !seen[s] {
            seen[s] = true;
            stack.push((spot, NO_DIRECTION));
        }
    }
    let mut taken = 0u32;
    while let Some((spot, dir)) = stack.pop() {
        taken += 1;
        if taken.is_multiple_of(CLOCK_EVERY) && stop() {
            return Err(Miss::Stopped);
        }
        if done(spot) {
            return Ok(true);
        }
        moves(masks, &open, extras, spot, dir, |next, next_dir, _| {
            let n = state(next, next_dir);
            if !seen[n] {
                seen[n] = true;
                stack.push((next, next_dir));
            }
        });
    }
    Ok(false)
}

/// Straight runs through a list of cells on one layer; a single cell has none.
#[cfg(test)]
pub(super) fn runs(grid: &Grid, cells: &[(usize, usize)]) -> Vec<(Pt, Pt)> {
    let none = vec![0u8; cells.len()];
    runs_with(grid, cells, &none)
        .into_iter()
        .map(|(a, b, _)| (a, b))
        .collect()
}

/// [`runs`], where a step to or from a cell of `neck` above 0 is a run of its own, marked with
/// the narrower of its two cells, so that the track is drawn narrow there and wide elsewhere.
pub(super) fn runs_with(grid: &Grid, cells: &[(usize, usize)], neck: &[u8]) -> Vec<(Pt, Pt, u8)> {
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
