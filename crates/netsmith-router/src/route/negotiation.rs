//! Negotiated routing for the lines a try leaves after its rip-ups (issue #1533, step 3 of
//! #1517): PathFinder on the router's grid, bounded by counts.
//!
//! The rip-ups before it ([`Router::rip_up_for`]) put a line's neighbours back unless the line
//! and all it moved fit at once. The negotiation lets the lines fight over the cells for a few
//! rounds and makes the cells they fight over dearer, so that each round they spread out a little
//! more. A round, for each line left:
//!
//! 1. **The line makes its way** ([`Router::make_way`]): it is routed, ripping up a set of the
//!    single lines in its way, and those are routed again, each making its way in turn, down to
//!    [`WAY_DEPTH`] levels, a line routed for the one above locked below it (a lock, not a
//!    reservation: this ends a cycle of two lines). The sets come from one search that may
//!    cross other nets' copper at the round's price ([`Router::contested_lines`]): the lines
//!    its path crosses, then those within 0.1 mm of it, then within 0.25 mm ([`WAY_NEAR`]), then
//!    each line it crosses alone. A set is kept only when every line it moved is routed again,
//!    so this step never leaves more lines than before; it spends at most [`WAY_SEARCHES`]
//!    searches for a line. Every search pays the history the rounds before learnt.
//! 2. When it finds no way, the line is searched for **allowing overlaps at a price**: the other
//!    nets' routed copper may be crossed, a track's cell at [`SOFT`] and a via at [`SOFT_VIA`]
//!    times the round's **present price** ([`present_price`]: 100 % in the first round,
//!    [`PRESENT_GROWTH`] times more each round after), on top of what the line's copper costs
//!    anyway.
//! 3. The cells where that path **overlaps** other copper (a cell of the path the masks would
//!    keep the path from, for that copper alone) get one more unit of **history**
//!    ([`HISTORY_COST`] a unit for every later search of the try, soft or for copper, up to
//!    [`HISTORY_MOST`]), and every cell of the path one unit: the cells fought over learn twice
//!    as fast as the rest of the path. (Learning only the cells fought over, a line whose path
//!    overlaps little or nothing, one whose copper could not be drawn where the search ran
//!    because its corners do not fit, was searched for on the same path round after round:
//!    issue #1572.)
//! 4. The lines whose copper it overlaps (single lines, not whole nets: a net's other lines stay)
//!    are ripped up, the line is routed through the room they left, and they are **routed
//!    again**, shortest first, each paying the history. A line that finds no way is left, and
//!    makes its way, or is one of the overlapping lines, in the next round; when the line in
//!    hand finds no way, what was ripped up is put back.
//!
//! It stops when no line is left, after [`Negotiation::rounds`] rounds, when its searches reach
//! [`Negotiation::searches`] (each search, soft or for copper, of every step counts one, so the
//! count is the same on every machine; the time limit stops it only as a hang guard), or after a
//! round that spreads the lines left ([`spreads`]). The best state it sees, the fewest lines left,
//! is kept, and the state it began with when no round did better: a try the negotiation cannot
//! improve keeps its copper. A board the rip-ups route never gets here, so its copper is as it
//! was. What it did is [`Routing::negotiation`] ([`Negotiated`]). After it come the rounds of
//! whole nets (`rip_rounds.rs`).
//!
//! **The chain is gone** (issue #1570, measured 2026-11-21 with `measured`, `NETSMITH_SEEDS=0..8`:
//! the six examples the router's tests read and the invented tight and mixed boards, 16 of them,
//! one try, release build). Until then a chain of rip-ups of single lines (step 1 alone, with its
//! own 3000 searches a try) ran after the negotiation, from its end and again from the state it
//! began with, and the try kept the better end. The negotiation searched one path for a line and
//! ripped up what that path overlapped; the chain tried several sets and went down four levels,
//! keeping a set only when every line it moved was routed again, and the negotiation alone left
//! many more lines (issue #1566). Step 1 is that chain's search, run in each round with the
//! history the rounds learn:
//!
//! | | lines left | vias | layer changes | nets over twice their straight length |
//! |---|---|---|---|---|
//! | no negotiation (the chain alone, before) | 9 | 1945 | 1913 | 190 |
//! | the negotiation and the chain from both its ends (before) | 2 | 1864 | 1831 | 177 |
//! | the negotiation without the chain (before) | 19 | 1992 | 1959 | 183 |
//! | **this: step 1 in each round, no chain** | **0** | **1884** | **1855** | **188** |
//! | the same, step 1 at the first round's price in every round | 0 | 1880 | 1851 | 190 |
//! | the same, 9000 searches | 0 | 1884 | 1855 | 188 |
//! | no negotiation now (`none`: the rip-ups and the rounds of whole nets) | 51 | 1925 | 1892 | 187 |
//!
//! It finishes every board, tight 4 and 7 too, which kept a line with the chain; the examples
//! route before the negotiation and are the same. No board spent more than 1700 of its 3000
//! searches, so the larger bound changed nothing. A board has a few more vias than with the chain
//! from both ends, which kept the cheaper of two ends by the weights; the reroute pass is where
//! the vias are worked on (issue #1538).
//!
//! **Before** (2026-11-21, main at 9b9dc6ce, the same boards less the examples, with the chain):
//! main's rounds never kept a state, 9 lines; ripping up single lines and learning the cells
//! fought over only, 5; learning the whole path and the cells fought over twice, 2 (issue #1572:
//! a line whose overlapping path crossed no copper learnt no cell and was searched for on the
//! same path, which could not be drawn, round after round). The form before that (2026-11-04 to
//! 2026-11-21) ripped up the whole nets near the line's path, at one price in every round, and
//! learnt every cell of the path; it had no bound but the time limit. Tried without the chain
//! and not kept: the history halved between rounds (26 lines, before #1572), the lines routed in
//! a round locked for the rest of it (32), and 20 rounds of 12000 searches with no stop on a
//! spread (17; 15 with the lock and the halving).

use super::*;

impl<'a> Router<'a> {
    /// The stage of the module's header, on `state` after the rip-ups of a try, at `room`.
    pub(super) fn negotiate(&self, state: &mut State, room: f64, attempt: usize, attempts: usize) {
        let bounds = self.negotiation;
        let mut done = Negotiated {
            left_before: state.failed.len(),
            ..Negotiated::default()
        };
        let cells = self.grid.nx * self.grid.ny;
        *self.history.borrow_mut() = vec![vec![0; cells]; self.stack.len()];
        let mut best = state.clone();
        let spent = |done: &Negotiated| done.searches >= bounds.searches;
        'rounds: for round in 1..=bounds.rounds {
            if state.failed.is_empty() || spent(&done) || self.out_of_time() {
                break;
            }
            done.rounds = round;
            self.soft_price.set(present_price(round));
            for line in state.failed.clone() {
                // Each step takes a soft search and a search for its copper: both must fit.
                if done.searches + 2 > bounds.searches || self.out_of_time() {
                    break 'rounds;
                }
                self.report(false, Some(&*state), || {
                    format!(
                        "routing, try {attempt} of {attempts}: negotiating for {} (round {round} of {}), {} not routed",
                        self.lines[line],
                        bounds.rounds,
                        state.failed.len()
                    )
                });
                // 1. The line makes its way, on a copy kept only when every line is routed.
                let mut trial = state.clone();
                let mut budget = WAY_SEARCHES.min(bounds.searches - done.searches);
                let start = budget;
                let made = self.make_way(line, &mut trial, room, 0, &[], &mut budget);
                done.searches += start - budget;
                if made {
                    trial.failed.retain(|&f| f != line);
                    *state = trial;
                    continue;
                }
                if done.searches + 2 > bounds.searches || self.out_of_time() {
                    break 'rounds;
                }
                // 2. to 4. The overlaps, the history and the lines routed again.
                done.searches += 1;
                let Some((crossed, fought)) = self.overlaps(line, state, room) else {
                    continue;
                };
                self.learn(&fought);
                let (gone, kept): (Vec<Routed>, Vec<Routed>) = state
                    .routed
                    .drain(..)
                    .partition(|r| crossed.contains(&r.line));
                state.routed = kept;
                // The line first, through the room the lines it overlaps left.
                done.searches += 1;
                match self.try_line(line, state, &[], room, false) {
                    Some((routed, _)) => {
                        state.routed.push(routed);
                        state.failed.retain(|&f| f != line);
                    }
                    None => {
                        state.routed.extend(gone);
                        continue;
                    }
                }
                done.ripped += gone.len();
                let mut again: Vec<usize> = gone.iter().map(|r| r.line).collect();
                again.sort_by(|&a, &b| {
                    self.lines[a]
                        .length()
                        .total_cmp(&self.lines[b].length())
                        .then(a.cmp(&b))
                });
                for (k, &l) in again.iter().enumerate() {
                    if spent(&done) || self.out_of_time() {
                        // The bound is spent: the lines not routed again are left, and the
                        // best state seen is what is kept.
                        state.failed.extend_from_slice(&again[k..]);
                        break 'rounds;
                    }
                    done.searches += 1;
                    match self.try_line(l, state, &[], room, false) {
                        Some((routed, _)) => state.routed.push(routed),
                        None => state.failed.push(l),
                    }
                }
            }
            if state.failed.len() < best.failed.len() {
                best = state.clone();
            }
            if spreads(best.failed.len(), state.failed.len()) {
                break;
            }
        }
        if state.failed.len() < best.failed.len() {
            best = state.clone();
        }
        self.soft_price.set(100);
        *self.history.borrow_mut() = vec![Vec::new(); self.stack.len()];
        done.left_after = best.failed.len();
        done.kept = done.left_after < done.left_before;
        *state = best;
        state.negotiated = done;
    }

    /// The first step of the negotiation for a line (step 1 of the module's header): route it,
    /// ripping up a set of the single lines (not whole nets) in its way, and route those again,
    /// each making its way in turn, down to [`WAY_DEPTH`] levels, never touching a line that
    /// is routed for the one above. It costs at most `budget` searches and changes `state` only
    /// if every line ends up routed. Nothing is random: the same board gives the same copper.
    pub(super) fn make_way(
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
        if depth >= WAY_DEPTH {
            return false;
        }
        *budget -= 1;
        let Some(blockers) = self.contested_lines(line, state, room, locked) else {
            return false;
        };
        // First the lines the path crosses, then those it comes near, as one set each, and
        // then each line it crosses alone.
        let mut options: Vec<Vec<usize>> = Vec::new();
        for &near in &WAY_NEAR {
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
                if self.make_way(l, &mut trial, room, depth + 1, &lock, budget) {
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

    /// Where `line` goes when other nets' copper may be crossed at the present price, as
    /// [`Self::contested`] finds it (the copper of a protected line stays hard), and what that
    /// path overlaps: the routed lines of other nets whose copper the masks would keep the path
    /// from, in line order, and the cells to learn: those where it does, each once, and then
    /// every cell of the path. `None` if even
    /// that search finds no path.
    pub(super) fn overlaps(
        &self,
        line: usize,
        state: &State,
        room: f64,
    ) -> Option<(Vec<usize>, Vec<Spot>)> {
        let net = self.nets[line];
        let protected = self.protected(line, state);
        let (_, path) = self.try_line_locked(line, state, &[], room, true, &protected)?;
        self.count_crowding(&path);
        let rules = &self.board.rules;
        let track_reach = self.clearance_of(line) + self.widest_of(line) / 2.0 + room;
        let via_reach = self.clearance_of(line) + rules.via_diameter / 2.0 + room;
        // A via of the path stands where it changes layer, on both layers.
        let vias: Vec<(Spot, Spot)> = path
            .windows(2)
            .filter(|w| w[0].2 != w[1].2)
            .map(|w| (w[0], w[1]))
            .collect();
        let mut crossed = Vec::new();
        let mut fought: Vec<Spot> = Vec::new();
        for r in state
            .routed
            .iter()
            .filter(|r| self.nets[r.line] != net && !protected.contains(&r.line))
        {
            let tracks = r.tracks.iter().map(|t| {
                (
                    Form::Segment(t.from, t.to, t.width / 2.0),
                    self.stack.layers_of(t.layer),
                )
            });
            let via_forms = r.vias.iter().map(|v| {
                (
                    Form::Circle(v.at, v.diameter / 2.0),
                    self.stack.via_layers(v),
                )
            });
            let mut hit = false;
            for (f, layers) in tracks.chain(via_forms) {
                for &spot in &path {
                    let (i, j, layer) = spot;
                    if layers[layer] && f.distance(self.grid.centre(i, j)) < track_reach {
                        fought.push(spot);
                        hit = true;
                    }
                }
                for &(a, b) in &vias {
                    if f.distance(self.grid.centre(a.0, a.1)) < via_reach {
                        fought.extend([a, b]);
                        hit = true;
                    }
                }
            }
            if hit {
                crossed.push(r.line);
            }
        }
        crossed.sort_unstable();
        crossed.dedup();
        // Every cell of the path is learnt once, and each cell where it overlaps once more: a
        // line whose path overlaps little failed as much because its copper could not be drawn
        // where the search ran (its corners do not fit) as because other copper was in the way,
        // and learning only the overlap left the next search on the same path (issue #1572).
        fought.sort_unstable();
        fought.dedup();
        fought.extend(path);
        Some((crossed, fought))
    }
}
