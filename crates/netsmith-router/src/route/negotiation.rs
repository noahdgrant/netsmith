//! Negotiated routing for the lines a try leaves after its rip-ups (issue #1533, step 3 of
//! #1517): PathFinder on the router's grid, bounded by counts.
//!
//! The rip-ups before it ([`Router::rip_up_for`]) put a line's neighbours back unless the line
//! and all it moved fit at once, and the chain after it ([`Router::rip_up_chain`]) locks what it
//! moved, depth by depth: each takes whatever way is free. The negotiation instead lets the lines
//! fight over the cells for a few rounds and makes the cells they fight over dearer, so that each
//! round they spread out a little more. A round:
//!
//! 1. Each line left is searched for **allowing overlaps at a price**: the other nets' routed
//!    copper may be crossed, a track's cell at [`SOFT`] and a via at [`SOFT_VIA`] times the
//!    round's **present price** ([`present_price`]: 100 % in the first round, [`PRESENT_GROWTH`]
//!    times more each round after), on top of what the line's copper costs anyway.
//! 2. The cells where that path **overlaps** other copper (a cell of the path the masks would
//!    keep the path from, for that copper alone) get one more unit of **history**
//!    ([`HISTORY_COST`] a unit for every later search of the try, soft or for copper, up to
//!    [`HISTORY_MOST`]), and every cell of the path one unit: the cells fought over learn twice
//!    as fast as the rest of the path. (Learning only the cells fought over, a line whose path
//!    overlaps little or nothing, one whose copper could not be drawn where the search ran
//!    because its corners do not fit, was searched for on the same path round after round:
//!    issue #1572.)
//! 3. The lines whose copper it overlaps (single lines, not whole nets: a net's other lines stay)
//!    are ripped up, the line is routed through the room they left, and they are **routed
//!    again**, shortest first, each paying the history. A line that finds no way is left, and is
//!    one of the overlapping lines of the next round; when the line in hand finds no way, what was
//!    ripped up is put back.
//!
//! It stops when no line is left, after [`Negotiation::rounds`] rounds, when its searches reach
//! [`Negotiation::searches`] (each soft search and each search for copper counts one, so the
//! count is the same on every machine; the time limit stops it only as a hang guard), or after a
//! round that spreads the lines left ([`spreads`]). The best state it sees, the fewest lines left,
//! is kept, and the state it began with when no round did better: a try the negotiation cannot
//! improve keeps its copper. When it did better, the chain runs from both its end and the state
//! it began with, and the try keeps the end with fewer lines left (of two that leave as many, the
//! one cheaper by the weights, [`Router::state_cost`]): a try never routes less than it would
//! without the negotiation. A board the rip-ups route never gets here, so its copper is as it was.
//! What it did is [`Routing::negotiation`] ([`Negotiated`]). The chain then has the lines it left,
//! and after it the rounds of whole nets (`rip_rounds.rs`).
//!
//! **The chain stays** (issue #1566, measured 2026-11-21 with `measured`'s `no chain` setting,
//! [`RouteOptions::chain`]; the same 16 boards, one try). The negotiation does not close what
//! the chain does: it searches one path for a line and rips up what that path overlaps, where
//! the chain tries several sets (what the path crosses, what it comes near) and goes down four
//! levels, keeping a set only when every line it moved is routed again.
//!
//! | | lines left | vias |
//! |---|---|---|
//! | negotiation and the chain from both states (built) | **5** | 1883 |
//! | the chain from the negotiation's end only | 6 | 1942 |
//! | no chain | 30 | 1955 |
//! | no chain, history halved between rounds | 26 | 1982 |
//! | no chain, the lines routed in a round locked for the rest of it | 32 | 1975 |
//! | no chain, 20 rounds, 12000 searches, no stop on a spread | 17 | 1990 |
//! | the same with the lock and the halving | 15 | 1958 |
//!
//! The second run is tight 5 again (1 line left without it, none with it). These rows were
//! measured before the history learnt the whole path (issue #1572); with it, the built form
//! leaves 2 lines (1864 vias) and no chain 19 (1992 vias).
//!
//! The form before this one (2026-11-04 to 2026-11-21) ripped up the whole nets near the line's
//! path, at one price in every round, and learnt every cell of the path; it had no bound but
//! the time limit.
//!
//! **Measured** (2026-11-21, main at 9b9dc6ce against this, `tests/negotiation.rs`'s `measured`:
//! one try of each board, release build). The six examples the router's tests read route before
//! the negotiation and are the same. On the invented tight and mixed boards, seeds 0 to 7 (16
//! boards), main ended every board exactly as it does with no negotiation at all: its rounds
//! never kept a state. This form, against main:
//!
//! | | lines left | vias | layer changes | nets over twice their straight length | mean detour |
//! |---|---|---|---|---|---|
//! | main (and no negotiation) | 9 | 1945 | 1913 | 190 | 1.535 |
//! | the first form of this, learning the cells fought over only | 5 | 1883 | 1850 | 177 | 1.502 |
//! | **this, learning the whole path and the cells fought over twice** | **2** | **1864** | **1831** | **177** | **1.487** |
//!
//! It finishes seven boards main leaves lines of (mixed 0, 1, 2, 5 and 7, tight 6) and leaves
//! none that main finishes; tight 4 and 7 keep their one line. Learning the cells fought over
//! only, it finished tight 4 too but none of mixed 0, 2 and 5, and it lost a land-grid board
//! that main finished (`synthetic::a_walled_in_pad_on_a_plane_net_gets_a_via_in_the_pad`, issue
//! #1572): a line whose overlapping path crossed no copper learnt no cell and was searched for
//! on the same path, which could not be drawn, round after round. Before the chain ran from both
//! states, one board (tight 5) ended with a line that main's chain closed: the chain from the
//! negotiation's end, 2 lines, left 1, where from the 4 lines it began with it left none.

use super::*;

impl<'a> Router<'a> {
    /// What the copper of `state` costs by the weights ([`RouteOptions::weights`], else the
    /// router's own, [`Weights::DEFAULT`]), in mm of track: its length and its vias.
    pub(super) fn state_cost(&self, state: &State) -> f64 {
        let w = self.weights.unwrap_or(Weights::DEFAULT);
        let length: f64 = state
            .routed
            .iter()
            .flat_map(|r| &r.tracks)
            .map(|t| t.from.distance(t.to))
            .sum();
        let vias: usize = state.routed.iter().map(|r| r.vias.len()).sum();
        w.measure(length, vias)
    }

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
                // A line takes a soft search and a search for its copper: both must fit.
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
