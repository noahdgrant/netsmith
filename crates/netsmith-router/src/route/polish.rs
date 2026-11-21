//! The quality objective's weights and the polish pass (issue #1517).
//!
//! **Weights.** With [`RouteOptions::weights`] stated, the search for a line of a signal net
//! ([`Router::weighs`]) pays the length weight for each step of track and the via and layer
//! change weights for each via, in millimetres of track, instead of the router's own costs (a
//! share of 1 and 15 mm). The search's estimate takes the same share of the octile distance, so
//! it never says more than a path costs. Other nets, and every line when none are stated, route
//! at the router's own costs: a board routed without weights is routed as before.
//!
//! **Polish.** On a try that routed every line, after the pass that takes vias out, each line of
//! a signal net that is not of a diff pair is routed again against all the other copper in eight
//! directions, at the usual room and then the least, the dearest by the weights for its straight
//! length first; its new copper is kept only when it costs less by the weights and lays no more
//! track on another net's pour, and it passed the exact checks every search's copper passes. A
//! line routed by a repair (the rip-ups, the negotiation and the chain search in four
//! directions, the lines in their way routed earlier) often finds a shorter way once the board is
//! done. At most [`POLISH_SWEEPS`] sweeps, each while the last gave a line cheaper copper; a line
//! is never left unrouted.
//!
//! **Measured** (2026-11-21, `netsmith-fabricate`'s `router_weights::measured`: vias, layer
//! changes and the detour, routed over straight length, of the nets of `board nets`' table):
//!
//! | board | own | own weights, polished | via 45 mm |
//! |---|---|---|---|
//! | six_layer | 0, 0, 1.131 | 0, 0, 1.079 | 0, 0, 1.079 |
//! | four_layer | 0, 0, 1.071 | 0, 0, 1.066 | 0, 0, 1.066 |
//! | generated 1 | 6, 6, 1.086 | 6, 6, 1.080 | 6, 6, 1.105 |
//! | generated 2 | 4, 4, 1.072 | 4, 4, 1.044 | 4, 4, 1.044 |
//! | generated 3 | 8, 8, 1.115 | 8, 8, 1.104 | 4, 4, 1.239 |
//!
//! The polish alone shortens the copper; a dear via trades length for vias (generated 3 halves
//! its vias for 11 % more track), and a layer change of 15 mm more cuts the changes further
//! (generated 1: 6 to 3).

use super::*;

/// How many times at most the polish pass goes over the lines.
pub(super) const POLISH_SWEEPS: usize = 2;

/// How much less, in mm of track by the weights, new copper must cost to be kept: what keeps
/// two ways of one cost from being swapped for each other.
const POLISH_GAIN: f64 = 0.05;

impl<'a> Router<'a> {
    /// Whether a line is of a signal net ([`Weights`]): a ratsnest line between two pads (not to
    /// a plane) of a net that no plane, pour or zone fills and whose current needs no group of
    /// vias side by side.
    pub(super) fn is_signal(&self, line: usize) -> bool {
        let net = &self.lines[line].net;
        line < self.routed_lines
            && !self.lines[line].to_plane
            && self.vias_needed(line) <= 1
            && !self.board.pours.iter().any(|p| &p.net == net)
            && !self.board.planes.iter().any(|p| &p.net == net)
            && !self.board.zones.iter().any(|z| &z.net == net)
    }

    /// The weights the search for `line` pays, when they are stated and it is of a signal net.
    pub(super) fn weighs(&self, line: usize) -> Option<Weights> {
        self.weights.filter(|_| self.is_signal(line))
    }

    /// What a line's copper costs by the weights, in mm of track.
    fn weighed(&self, w: &Weights, r: &Routed) -> f64 {
        let length: f64 = r.tracks.iter().map(|t| t.from.distance(t.to)).sum();
        w.measure(length, r.vias.len())
    }

    /// The polish pass (the module's header): each line of a signal net, the dearest for its
    /// length first, routed again against all the other copper and kept when cheaper by the
    /// weights. Runs only when weights are stated; counts the lines it changed in
    /// [`State::polished`].
    pub(super) fn polish(&self, state: &mut State, attempt: usize, attempts: usize) {
        let Some(w) = self.weights else { return };
        let stage = self.stage.get();
        let failed_in = self.failed_in.borrow().clone();
        let counting = self.counting.replace(false);
        let fine = self.fine.replace(true);
        for _ in 0..POLISH_SWEEPS {
            let mut todo: Vec<(usize, f64)> = state
                .routed
                .iter()
                .filter(|r| self.is_signal(r.line) && self.partners[self.nets[r.line]].is_none())
                .map(|r| {
                    let l = &self.lines[r.line];
                    let straight = l.from_at.distance(l.to_at).max(0.1);
                    (r.line, self.weighed(&w, r) / straight)
                })
                .collect();
            todo.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            let total = todo.len();
            let mut gained = false;
            for (k, (line, _)) in todo.into_iter().enumerate() {
                if self.out_of_time() {
                    break;
                }
                self.report(false, Some(&*state), || {
                    format!("routing, try {attempt} of {attempts}: polish, {k} of {total} lines")
                });
                let Some(at) = state.routed.iter().position(|r| r.line == line) else {
                    continue;
                };
                let old = state.routed.remove(at);
                let before = (self.weighed(&w, &old), self.on_pour(&old));
                let keeps = |new: &Routed| {
                    self.weighed(&w, new) < before.0 - POLISH_GAIN
                        && self.on_pour(new) <= before.1 + 1e-6
                };
                let kept = [ROOM_LOOSE, ROOM_TIGHT].into_iter().find_map(|room| {
                    self.try_line(line, state, &[], room, false)
                        .map(|(new, _)| new)
                        .filter(keeps)
                });
                if kept.is_some() {
                    gained = true;
                    state.polished += 1;
                }
                state.routed.insert(at, kept.unwrap_or(old));
            }
            if !gained {
                break;
            }
        }
        self.fine.set(fine);
        self.counting.set(counting);
        *self.failed_in.borrow_mut() = failed_in;
        self.stage.set(stage);
    }
}
