//! The reroute pass (issue #1538): the worst nets laid again with the nets in their way, the
//! worst first.
//!
//! The router lays the lines of a try in one order. A net routed after its neighbour may find
//! the neighbour's copper across its straight way and go round it, a long way for a short hop,
//! where the neighbour, laid after it, would have gone round at little cost. The polish pass
//! ([`Router::polish`]) does not find that: it routes one line again against all the others,
//! and the neighbour is still there.
//!
//! **What it does.** On a try that routed every line, after the polish pass, and only when
//! [`RouteOptions::weights`] are stated: the signal nets that are not of a diff pair are ranked
//! by their detour, the length of their copper over the straight length of their lines, the
//! worst first, and those over [`REROUTE_DETOUR`] are tried, at most [`Reroute::nets`] in a
//! round. For each, the nets in its way are the other such nets with a track within
//! [`CORRIDOR`] of the straight way of one of its lines (a quarter of that line's length, at
//! least 1 mm), at most [`NEIGHBOURS`] of them, the nearest first. Their copper and the net's
//! are taken away and laid again ([`Router::route_again`]): the net first, then the others,
//! each at the usual room and failing that at the least. The result is kept only when every
//! line is routed again, their total by the weights ([`Weights::measure`]: length, vias and
//! layer changes) is shorter by more than [`REROUTE_GAIN`], and it lays no more track on
//! another net's pour; otherwise the copper is as it was. A round that keeps nothing ends the
//! pass, and it runs at most [`Reroute::rounds`] rounds; a net tried and not kept is not tried
//! again. So the pass never leaves a line unrouted, and it makes at most `nets * rounds` tries.
//!
//! **Reported** as [`Routing::rerouted`] (each net kept, the nets laid after it, the totals
//! before and after) and [`Routing::reroute_tried`].
//!
//! **Measured** (2026-11-21, `tests/reroute.rs`'s `measured`, the router's own weights stated:
//! the length of all track without the pass and with it, the nets tried and kept):
//!
//! | board | without | with | tried | kept |
//! |---|---|---|---|---|
//! | crossing (the test's) | 37.1 | 31.3 | 2 | 1 |
//! | generated 1 | 100.2 | 100.2 | 0 | 0 |
//! | generated 2 | 75.1 | 75.1 | 0 | 0 |
//! | generated 3 | 85.6 | 85.6 | 1 | 0 |
//!
//! On the generated boards the polish pass leaves no net past the detour the pass tries, or
//! laying it first gains nothing: the pass changes a board only where the order cost length.

use super::*;

/// The least detour (routed length over straight length) of a net the pass tries.
pub(super) const REROUTE_DETOUR: f64 = 1.25;

/// How much shorter, in mm of track by the weights, the total must get to be kept.
const REROUTE_GAIN: f64 = 0.5;

/// How near a straight way of the net another net's track must be to be in its way: this share
/// of the line's straight length, at least [`CORRIDOR_LEAST`] mm.
const CORRIDOR: f64 = 0.25;
const CORRIDOR_LEAST: f64 = 1.0;

/// The most nets laid again after a net.
const NEIGHBOURS: usize = 3;

impl<'a> Router<'a> {
    /// The nets the pass may move: signal nets with no diff pair partner, and lines routed.
    fn movable(&self, state: &State) -> Vec<usize> {
        let mut nets: Vec<usize> = state
            .routed
            .iter()
            .filter(|r| self.is_signal(r.line) && self.partners[self.nets[r.line]].is_none())
            .map(|r| self.nets[r.line])
            .collect();
        nets.sort_unstable();
        nets.dedup();
        // A net with a line not routed by this try's search (or not a signal line) stays.
        nets.retain(|&n| {
            state
                .routed
                .iter()
                .filter(|r| self.nets[r.line] == n)
                .all(|r| self.is_signal(r.line))
        });
        nets
    }

    /// The detour of `net` on `state`: the length of its copper over the straight length of
    /// its lines.
    fn detour(&self, state: &State, net: usize) -> f64 {
        let (mut routed, mut straight) = (0.0, 0.0);
        for r in state.routed.iter().filter(|r| self.nets[r.line] == net) {
            routed += r.tracks.iter().map(|t| t.from.distance(t.to)).sum::<f64>();
            let l = &self.lines[r.line];
            straight += l.from_at.distance(l.to_at);
        }
        routed / straight.max(0.1)
    }

    /// The nets of `movable` in the way of `net`: those with a track within the corridor of one
    /// of its lines' straight ways, the nearest first, at most [`NEIGHBOURS`].
    fn in_the_way_of(&self, state: &State, net: usize, movable: &[usize]) -> Vec<usize> {
        let ways: Vec<(Pt, Pt, f64)> = state
            .routed
            .iter()
            .filter(|r| self.nets[r.line] == net)
            .map(|r| {
                let l = &self.lines[r.line];
                let reach = (CORRIDOR * l.from_at.distance(l.to_at)).max(CORRIDOR_LEAST);
                (l.from_at, l.to_at, reach)
            })
            .collect();
        let mut near: Vec<(f64, usize)> = Vec::new();
        for r in &state.routed {
            let other = self.nets[r.line];
            if other == net || !movable.contains(&other) {
                continue;
            }
            let gap = r
                .tracks
                .iter()
                .flat_map(|t| {
                    ways.iter().filter_map(move |&(a, b, reach)| {
                        let g = line_gap(t.from, t.to, a, b);
                        (g <= reach).then_some(g)
                    })
                })
                .min_by(f64::total_cmp);
            if let Some(g) = gap {
                match near.iter_mut().find(|(_, n)| *n == other) {
                    Some(e) => e.0 = e.0.min(g),
                    None => near.push((g, other)),
                }
            }
        }
        near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        near.into_iter().take(NEIGHBOURS).map(|(_, n)| n).collect()
    }

    /// What the copper of `nets` costs on `state` by `w`, and its length on other nets' pours.
    fn total(&self, w: &Weights, state: &State, nets: &[usize]) -> (f64, f64) {
        state
            .routed
            .iter()
            .filter(|r| nets.contains(&self.nets[r.line]))
            .fold((0.0, 0.0), |(cost, pour), r| {
                let length: f64 = r.tracks.iter().map(|t| t.from.distance(t.to)).sum();
                (
                    cost + w.measure(length, r.vias.len()),
                    pour + self.on_pour(r),
                )
            })
    }

    /// The reroute pass (the module's header). Runs only when weights are stated; records the
    /// nets it kept in [`State::rerouted`] and the nets it tried in [`State::reroute_tried`].
    pub(super) fn reroute(&self, state: &mut State, attempt: usize, attempts: usize) {
        let Some(w) = self.weights else { return };
        if self.reroute.nets == 0 {
            return;
        }
        let stage = self.stage.get();
        let failed_in = self.failed_in.borrow().clone();
        let counting = self.counting.replace(false);
        let fine = self.fine.replace(true);
        let mut given_up: Vec<usize> = Vec::new();
        for round in 1..=self.reroute.rounds {
            let movable = self.movable(state);
            let mut worst: Vec<(f64, usize)> = movable
                .iter()
                .filter(|n| !given_up.contains(n))
                .map(|&n| (self.detour(state, n), n))
                .filter(|&(d, _)| d > REROUTE_DETOUR)
                .collect();
            worst.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
            worst.truncate(self.reroute.nets);
            let mut kept = false;
            for (k, &(_, net)) in worst.iter().enumerate() {
                if self.out_of_time() {
                    break;
                }
                self.report(false, Some(&*state), || {
                    format!(
                        "routing, try {attempt} of {attempts}: reroute round {round}, {k} of {} nets",
                        worst.len()
                    )
                });
                state.reroute_tried += 1;
                let movable = self.movable(state);
                let with = self.in_the_way_of(state, net, &movable);
                let mut order = vec![net];
                order.extend(&with);
                let before = self.total(&w, state, &order);
                let better = self.route_again(state, &order).filter(|trial| {
                    let after = self.total(&w, trial, &order);
                    trial.routed.len() == state.routed.len()
                        && after.0 < before.0 - REROUTE_GAIN
                        && after.1 <= before.1 + 1e-6
                });
                match better {
                    Some(trial) => {
                        let after = self.total(&w, &trial, &order).0;
                        let name = |n: usize| self.board.nets[n].name.clone();
                        // The trial is a clone of the state, its counts with it.
                        *state = trial;
                        state.rerouted.push(Rerouted {
                            net: name(net),
                            with: with.iter().map(|&n| name(n)).collect(),
                            before: before.0,
                            after,
                        });
                        kept = true;
                    }
                    None => given_up.push(net),
                }
            }
            if !kept {
                break;
            }
        }
        self.fine.set(fine);
        self.counting.set(counting);
        *self.failed_in.borrow_mut() = failed_in;
        self.stage.set(stage);
    }
}
