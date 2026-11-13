//! The stage that takes tracks off the pours of other nets, and puts them on their own net's.

use super::*;

impl<'a> Router<'a> {
    /// What a step of a line of `net` costs beyond its length on each layer in
    /// [`Self::off_the_pours`]: [`POUR`] on an outer layer another net is poured on, where the
    /// other outer layer is free of other nets' pours (where both are poured, either way cuts
    /// one).
    pub(super) fn pour_costs(&self, net: usize) -> [i64; 2] {
        let name = &self.board.nets[net].name;
        let poured = |layer: CopperLayer| {
            self.board
                .pours
                .iter()
                .any(|p| p.layer == layer && &p.net != name)
        };
        let (front, back) = (poured(CopperLayer::Front), poured(CopperLayer::Back));
        [
            if front && !back { POUR } else { 0 },
            if back && !front { POUR } else { 0 },
        ]
    }

    /// What a line's copper costs as the search counts it in [`Self::off_the_pours`]: its
    /// length, [`POUR`] more a step on another net's pour, and its vias at [`VIA`] and
    /// [`POUR_VIA`].
    pub(super) fn pour_cost(&self, r: &Routed) -> i64 {
        let cost = self.pour_costs(self.nets[r.line]);
        let steps: f64 = r
            .tracks
            .iter()
            .map(|t| {
                let layer = usize::from(t.layer == CopperLayer::Back);
                t.from.distance(t.to) / PITCH * (STEP + cost[layer]) as f64
            })
            .sum();
        steps.round() as i64 + (VIA + POUR_VIA) * r.vias.len() as i64
    }

    /// The length of `r`'s track on another net's pour, in mm.
    pub(super) fn on_pour(&self, r: &Routed) -> f64 {
        let cost = self.pour_costs(self.nets[r.line]);
        r.tracks
            .iter()
            .filter(|t| cost[usize::from(t.layer == CopperLayer::Back)] > 0)
            .map(|t| t.from.distance(t.to))
            .sum()
    }

    /// Take track off other nets' pours on a try that routed every line (issue #431, S7): each
    /// line with track on one, the most first, is searched for again against the copper of all
    /// the others with a step on the pour paying [`POUR`], at the usual room and failing that at
    /// the least, and its new copper is kept only where it costs less so counted
    /// ([`Self::pour_cost`]). A line is never left unrouted, and a board with no such pour is
    /// unchanged. The nets of a diff pair ([`diff_pair_partners`]) are left as they are: moving
    /// one alone pulls it off its partner.
    pub(super) fn off_the_pours(&self, state: &mut State, attempt: usize, attempts: usize) {
        if (0..self.board.nets.len()).all(|n| self.pour_costs(n) == [0, 0]) {
            return;
        }
        let stage = self.stage.get();
        let failed_in = self.failed_in.borrow().clone();
        self.pouring.set(true);
        self.fine.set(true);
        for _ in 0..POUR_SWEEPS {
            let mut todo: Vec<(usize, f64)> = state
                .routed
                .iter()
                .filter(|r| self.partners[self.nets[r.line]].is_none())
                // On a try with classes, a decoupling or clock line keeps the copper its class
                // gave it, as a pair's does (issue #416).
                .filter(|r| !(self.classed.get() && self.class[r.line].protected()))
                .map(|r| (r.line, self.on_pour(r)))
                .filter(|&(_, mm)| mm > 0.0)
                .collect();
            todo.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            let total = todo.len();
            for (k, (line, _)) in todo.into_iter().enumerate() {
                if self.out_of_time() {
                    break;
                }
                self.report(false, || {
                    format!(
                        "routing, try {attempt} of {attempts}: off the pours, {k} of {total} lines"
                    )
                });
                let Some(at) = state.routed.iter().position(|r| r.line == line) else {
                    continue;
                };
                let old = state.routed.remove(at);
                let kept = match self
                    .try_line(line, state, &[], ROOM_LOOSE, false)
                    .or_else(|| self.try_line(line, state, &[], ROOM_TIGHT, false))
                {
                    Some((new, _)) if self.pour_cost(&new) < self.pour_cost(&old) => new,
                    _ => old,
                };
                state.routed.insert(at, kept);
            }
        }
        self.fine.set(false);
        self.pouring.set(false);
        *self.failed_in.borrow_mut() = failed_in;
        self.stage.set(stage);
    }

    /// Give each decoupling capacitor's pad on a poured return net its own via to the pour, on
    /// a try that routed every line (issue #431, S7): each line past [`Self::routed_lines`] is
    /// searched for against all the other copper, as a pad's line to a plane is, and kept only
    /// if its track is at most [`RETURN_TRACK`] long and the net has no via within
    /// [`RETURN_TRACK`] of the pad already. Nothing else moves and no line is left.
    pub(super) fn to_the_pours(&self, state: &mut State) {
        if self.routed_lines == self.lines.len() {
            return;
        }
        let stage = self.stage.get();
        let failed_in = self.failed_in.borrow().clone();
        self.fine.set(true);
        for line in self.routed_lines..self.lines.len() {
            if self.out_of_time() {
                break;
            }
            let at = self.lines[line].from_at;
            let net = &self.lines[line].net;
            let near = state
                .routed
                .iter()
                .flat_map(|r| r.vias.iter())
                .any(|v| &v.net == net && v.at.distance(at) <= RETURN_TRACK);
            if near {
                continue;
            }
            if let Some((r, _)) = self.try_line(line, state, &[], ROOM_LOOSE, false) {
                let length: f64 = r.tracks.iter().map(|t| t.from.distance(t.to)).sum();
                if !r.tracks.is_empty() && length <= RETURN_TRACK {
                    state.routed.push(r);
                }
            }
        }
        self.fine.set(false);
        *self.failed_in.borrow_mut() = failed_in;
        self.stage.set(stage);
    }
}
