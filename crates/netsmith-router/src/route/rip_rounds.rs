//! Rounds of rip-up and re-route of whole nets, for the lines a try still leaves after the
//! negotiation (issue #1088).
//!
//! The stages before it each keep a line's neighbours where they are unless the line and all it
//! moved fit at once ([`Router::rip_up_for`], [`Router::make_way`]), or stop when a round
//! spreads the lines left ([`Router::negotiate`]). On a dense board placed by hand that leaves a
//! few lines that have no free path unless some other nets move. This stage lets them move:
//!
//! 1. For each line left, the cheapest path that may cross other nets' routed copper at a price
//!    ([`SOFT`] a cell, [`SOFT_VIA`] a via: [`Router::contested_locked`]) names the nets whose
//!    copper it crosses, the fewest the path can meet.
//! 2. Those nets' copper is removed whole, every line of each, and the line is routed through
//!    the room they left. If it still finds no way the nets are put back and nothing is counted.
//! 3. The nets are routed again after it, shortest line first, at the usual room and failing that
//!    at the least. A line that finds no way is left, and is one of the lines of the next round.
//!
//! The cells each path fought over cost more from then on (the negotiation's history), so a net
//! ripped up does not come back to the same place. **Bounds, all counts** (the same answer on
//! every machine; the time limit stops it only as a hang guard): at most
//! [`RouteOptions::rip_rounds`] rounds ([`RIP_ROUNDS`] by default), [`RIP_MOST`] rips of one net
//! (after that its copper is hard to these searches, as a protected line's is), and
//! [`RIP_SEARCHES`] searches in all. The best state the rounds see, the fewest lines left, is
//! kept, and the state the stage began with when no round did better: a try the rounds cannot
//! improve keeps its copper. A try the rounds improve is a fallback (`route_with`): the router
//! goes on with its other orders, and the first try that routes the board without the rounds wins,
//! so a board that routed before keeps its copper; a try the rounds completed is kept only when no
//! other try completes, and otherwise the try with the fewest lines left.
//!
//! **Measured** (2026-11-17, main at db86ed84 against this; the agent's choice of the bounds).
//! The three ported boards (`netsmith board fab`, and `netsmith dev route --tries 1 --route-timeout 0
//! --turn T` at T = 24 to 27) route before the rounds and write byte for byte the same board
//! files. One try of the invented boards (`NETSMITH_TRIES=1`, `tight::measured` seeds 0 to 10,
//! `mixed::measured` seeds 0 to 8): tight leaves 5 lines with the rounds and without; mixed 7
//! without and 6 with (seed 4 routes, in one round of 60 searches and 7 nets ripped). The other
//! lines left are on a fine-pitch part, where the line and a net ripped for it take turns in one
//! gap; a round there makes 500 to 650 searches and adds 8 to 12 s to the try. Tried and not
//! kept: the nets of a line the rounds routed kept hard for the rest of them (the lock of a line making its way),
//! and 20 rounds with 6 rips a net: no seed more. With every try (the scorecard's way), the
//! boards that route keep their copper (mixed seed 4 routes on its second try with the same 226
//! vias); taking a try the rounds complete as final gave it 242 vias, which is why it is only a
//! fallback.
//!
//! **Never ripped.** Copper the router did not lay this run: the routes file's, locked or not
//! (ADR 0030), and what `carry` kept. A net with `lock = true` has no copper of the router's, so
//! it is never ripped. The copper of a protected class ([`Class::protected`]) is hard here too.

use super::*;

impl<'a> Router<'a> {
    /// The stage of the module's header, on `state` at the end of a try's other stages.
    pub(super) fn rip_rounds(&self, state: &mut State, attempt: usize, attempts: usize) {
        let cells = self.grid.nx * self.grid.ny;
        *self.history.borrow_mut() = vec![vec![0; cells]; self.stack.len()];
        let mut ripped = vec![0usize; self.board.nets.len()];
        let mut searches = 0usize;
        let mut rounds = 0usize;
        let left_before = state.failed.len();
        let mut best = state.clone();
        for round in 1..=self.most_rip_rounds {
            if state.failed.is_empty() || searches >= RIP_SEARCHES || self.out_of_time() {
                break;
            }
            rounds = round;
            let mut moved = false;
            for line in state.failed.clone() {
                if searches >= RIP_SEARCHES || self.out_of_time() {
                    break;
                }
                self.report(false, Some(&*state), || {
                    format!(
                        "routing, try {attempt} of {attempts}: ripping up whole nets for {} (round {round} of {}), {} not routed",
                        self.lines[line],
                        self.most_rip_rounds,
                        state.failed.len()
                    )
                });
                // The nets ripped up as often as they may be keep their copper.
                let tired: Vec<usize> = state
                    .routed
                    .iter()
                    .filter(|r| ripped[self.nets[r.line]] >= RIP_MOST)
                    .map(|r| r.line)
                    .collect();
                searches += 1;
                let Some((path, nets)) = self.contested_locked(line, state, ROOM_TIGHT, &tired)
                else {
                    continue;
                };
                self.learn(&path);
                let (gone, kept): (Vec<Routed>, Vec<Routed>) = state
                    .routed
                    .drain(..)
                    .partition(|r| nets.contains(&self.nets[r.line]));
                state.routed = kept;
                // The line first, through the room the nets left.
                let Some(first) = self.place_again(line, state, &mut searches) else {
                    state.routed.extend(gone);
                    continue;
                };
                state.routed.push(first);
                state.failed.retain(|&f| f != line);
                moved = true;
                for &n in &nets {
                    ripped[n] += 1;
                }
                let mut again: Vec<usize> = gone.iter().map(|r| r.line).collect();
                again.sort_by(|&a, &b| {
                    self.lines[a]
                        .length()
                        .total_cmp(&self.lines[b].length())
                        .then(a.cmp(&b))
                });
                for l in again {
                    match self.place_again(l, state, &mut searches) {
                        Some(r) => state.routed.push(r),
                        None => state.failed.push(l),
                    }
                }
            }
            if state.failed.len() < best.failed.len() {
                best = state.clone();
            }
            if !moved {
                break;
            }
        }
        let left_after = best.failed.len();
        *state = best;
        *self.history.borrow_mut() = vec![Vec::new(); self.stack.len()];
        *self.ripped.borrow_mut() = RipRounds {
            rounds,
            searches,
            ripped: ripped
                .iter()
                .enumerate()
                .filter(|&(_, &n)| n > 0)
                .map(|(net, &n)| (self.board.nets[net].name.clone(), n))
                .collect(),
            left_before,
            left_after,
        };
    }

    /// A line routed against the copper of `state`, at the usual room and failing that at the
    /// least, counting each search in `searches`.
    fn place_again(&self, line: usize, state: &State, searches: &mut usize) -> Option<Routed> {
        for room in [ROOM_LOOSE, ROOM_TIGHT] {
            if *searches >= RIP_SEARCHES {
                // Out of searches: the line is not tried, and so failed here.
                if let Some(s) = self.failed_in.borrow_mut().get_mut(line) {
                    *s = Stage::RipRounds;
                }
                return None;
            }
            *searches += 1;
            if let Some((routed, _)) = self.try_line(line, state, &[], room, false) {
                return Some(routed);
            }
        }
        None
    }
}
