//! The stages of a try: the order the lines are routed in, and what follows.

use super::*;

impl<'a> Router<'a> {
    /// Route the lines in this order, then rip up for any that failed. `attempt` and
    /// `attempts` say which try of the board this is, for the progress messages. If the time
    /// runs out, the lines not yet tried count as failed.
    pub(super) fn run(
        &self,
        order: &[usize],
        rip_up: bool,
        attempt: usize,
        attempts: usize,
    ) -> State {
        self.escaping.set(attempt > 1);
        self.classed.set(match self.class_use {
            LineClasses::Off => false,
            LineClasses::EveryTry => true,
            LineClasses::FromSecondTry => attempt > 1,
        });
        self.planned.set(
            self.plan.is_some()
                && match self.plan_use {
                    CoarsePlan::Off => false,
                    CoarsePlan::EveryTry => true,
                    CoarsePlan::FromSecondTry => attempt > 1,
                },
        );
        *self.failed_in.borrow_mut() = vec![Stage::FirstPass; self.lines.len()];
        *self.ripped.borrow_mut() = RipRounds::default();
        if let Some(snapshots) = &self.snapshots {
            snapshots.borrow_mut().clear();
        }
        let mut state = self.run_stages(order, rip_up, attempt, attempts);
        if state.failed.is_empty() {
            // The pairs first: the pass off the pours leaves their nets alone, so it cannot undo
            // their coupling.
            self.side_by_side(&mut state);
            self.off_the_pours(&mut state, attempt, attempts);
            self.to_the_pours(&mut state);
            self.fewer_vias(&mut state, attempt, attempts);
            self.share_vias(&mut state);
        }
        self.snapshot(SnapshotStage::Final, &state);
        state
    }

    /// The track length of the lines of `net`, their vias, and the share of that length beside
    /// a track of `partner` on the same layer: within the far edge of their
    /// [`Router::pair_band`] (a cell past the pair's gap).
    pub(super) fn pair_measure(
        &self,
        state: &State,
        net: usize,
        partner: usize,
    ) -> (f64, usize, f64) {
        let theirs: Vec<&Track> = state
            .routed
            .iter()
            .filter(|r| self.nets[r.line] == partner)
            .flat_map(|r| r.tracks.iter())
            .collect();
        let (mut length, mut near, mut vias) = (0.0, 0.0, 0);
        for r in state.routed.iter().filter(|r| self.nets[r.line] == net) {
            vias += r.vias.len();
            for t in &r.tracks {
                let l = t.from.distance(t.to);
                length += l;
                let n = (l / 0.05).ceil().max(1.0) as usize;
                for k in 0..n {
                    let f = (k as f64 + 0.5) / n as f64;
                    let p = Pt::new(
                        t.from.x + (t.to.x - t.from.x) * f,
                        t.from.y + (t.to.y - t.from.y) * f,
                    );
                    if theirs.iter().any(|o| {
                        o.layer == t.layer
                            && point_to_line(p, o.from, o.to)
                                <= self.pair_band(net, t.width, o.width).1 + 1e-9
                    }) {
                        near += l / n as f64;
                    }
                }
            }
        }
        (length, vias, if length > 0.0 { near / length } else { 0.0 })
    }

    /// Run each net of a diff pair beside its partner on a try that routed every line (issue
    /// #431, S9): the lines of one net are routed again against all the other copper, a step
    /// beside the partner's track costing [`FOLLOW`] of its length, and the new copper is kept
    /// only if the share that runs coupled grows and the net is no longer and has no more vias.
    /// Then both nets are routed again together, one and then the other beside it, each net
    /// first in turn, and the copper is kept on the same terms for the pair as a whole
    /// ([`keeps_both`], issue #1010): one net's way can be what keeps the other from running
    /// beside it, and a net may take the vias its partner already has. A line is never left unrouted, and a board with no pair is
    /// unchanged.
    pub(super) fn side_by_side(&self, state: &mut State) {
        let failed_in = self.failed_in.borrow().clone();
        self.pairing.set(true);
        self.fine.set(true);
        for n in 0..self.partners.len() {
            let Some(p) = self.partners[n] else { continue };
            if p < n {
                continue;
            }
            for (x, y) in [(p, n), (n, p)] {
                if self.out_of_time() {
                    break;
                }
                let (len0, vias0, c0) = self.pair_measure(state, x, y);
                if len0 == 0.0 || self.pair_measure(state, y, x).0 == 0.0 {
                    continue;
                }
                let Some(trial) = self.route_again(state, &[x]) else {
                    continue;
                };
                let (len1, vias1, c1) = self.pair_measure(&trial, x, y);
                if keeps_pair(
                    (len0, vias0, c0),
                    (len1, vias1, c1),
                    self.funnel_slack(x, y),
                ) {
                    *state = trial;
                }
            }
            let before = self.both_measure(state, n, p);
            if before.0 == 0.0 {
                continue;
            }
            let mut best: Option<(State, (f64, usize, f64))> = None;
            for first in [(n, p), (p, n)] {
                if self.out_of_time() {
                    break;
                }
                let Some(trial) = self.route_again(state, &[first.0, first.1]) else {
                    continue;
                };
                let after = self.both_measure(&trial, n, p);
                let better = best.as_ref().is_none_or(|(_, b)| after.2 > b.2);
                let slack = self.funnel_slack(n, p).max(self.funnel_slack(p, n));
                if keeps_both(before, after, slack) && better {
                    best = Some((trial, after));
                }
            }
            if let Some((trial, _)) = best {
                *state = trial;
            }
        }
        *self.failed_in.borrow_mut() = failed_in;
        self.fine.set(false);
        self.pairing.set(false);
    }

    /// How much longer a net of a pair may become in [`Self::side_by_side`]: [`PAIR_SLACK`],
    /// and also what the jogs of 45 degrees from its pads to the pair's gap add (issue #1054, for
    /// every pair since issue #1223, [`Router::pair_gap`]), [`JOG`] for each mm by which each end of the net's lines is
    /// farther from the nearest end of the partner's than the pair's pitch (its widths and the
    /// spacing), as if the net alone moved over to its partner.
    fn funnel_slack(&self, net: usize, partner: usize) -> f64 {
        if !self.paired(net) {
            return PAIR_SLACK;
        }
        let spacing = self.pair_gap(net);
        let ends = |n: usize| -> Vec<Pt> {
            (0..self.lines.len())
                .filter(|&l| self.nets[l] == n && !self.lines[l].to_plane)
                .flat_map(|l| [self.lines[l].from_at, self.lines[l].to_at])
                .collect()
        };
        let theirs = ends(partner);
        let width = |n: usize| self.board.track_width(&self.board.nets[n].name);
        let pitch = width(net) / 2.0 + width(partner) / 2.0 + spacing;
        let far: f64 = ends(net)
            .iter()
            .filter_map(|p| theirs.iter().map(|q| p.distance(*q)).min_by(f64::total_cmp))
            .map(|d| (d - pitch).max(0.0))
            .sum();
        PAIR_SLACK + JOG * far
    }

    /// `state` with the lines of `nets` routed again against all the other copper, the lines of
    /// each net in turn, shortest first, at the usual room and failing that at the least:
    /// `None` if a line finds no way.
    fn route_again(&self, state: &State, nets: &[usize]) -> Option<State> {
        let mut trial = state.clone();
        let (gone, kept): (Vec<Routed>, Vec<Routed>) = trial
            .routed
            .drain(..)
            .partition(|r| nets.contains(&self.nets[r.line]));
        trial.routed = kept;
        for &net in nets {
            let mut again: Vec<usize> = gone
                .iter()
                .filter(|r| self.nets[r.line] == net)
                .map(|r| r.line)
                .collect();
            again.sort_by(|&a, &b| self.lines[a].length().total_cmp(&self.lines[b].length()));
            // A net of a pair with a stated spacing is searched for at the least room first:
            // at the usual room, its partner's track keeps it a cell past the spacing.
            let rooms = if self.paired(net) {
                [ROOM_TIGHT, ROOM_LOOSE]
            } else {
                [ROOM_LOOSE, ROOM_TIGHT]
            };
            for l in again {
                let (r, _) = self
                    .try_line(l, &trial, &[], rooms[0], false)
                    .or_else(|| self.try_line(l, &trial, &[], rooms[1], false))?;
                trial.routed.push(r);
            }
        }
        Some(trial)
    }

    /// [`Self::pair_measure`] of the two nets of a pair together: the length of the longer
    /// net, the vias of the net with more, and the share of their length that runs coupled.
    /// A length of 0 if either has no track.
    fn both_measure(&self, state: &State, a: usize, b: usize) -> (f64, usize, f64) {
        let (la, va, ca) = self.pair_measure(state, a, b);
        let (lb, vb, cb) = self.pair_measure(state, b, a);
        if la == 0.0 || lb == 0.0 {
            return (0.0, 0, 0.0);
        }
        (la.max(lb), va.max(vb), (ca * la + cb * lb) / (la + lb))
    }
}

impl<'a> Router<'a> {
    /// The stages of [`Self::run`].
    pub(super) fn run_stages(
        &self,
        order: &[usize],
        rip_up: bool,
        attempt: usize,
        attempts: usize,
    ) -> State {
        let mut state = State::default();
        self.stage.set(Stage::FirstPass);
        // The first pass, which gives most lines the copper they keep, searches in eight
        // directions; the repairs after it search in four, with every right angle cut, which is
        // a fifth of the work (issue #415).
        self.fine.set(true);
        for (k, &line) in order.iter().enumerate() {
            if self.out_of_time() {
                state.failed.extend_from_slice(&order[k..]);
                let mut failed_in = self.failed_in.borrow_mut();
                for &line in &order[k..] {
                    failed_in[line] = Stage::OutOfTime;
                }
                return state;
            }
            self.report(false, Some(&state), || {
                format!(
                    "routing, try {attempt} of {attempts}: {k} of {} connections done, {} not routed, now {}",
                    order.len(),
                    state.failed.len(),
                    self.lines[line]
                )
            });
            match self.try_line(line, &state, &[], ROOM_LOOSE, false) {
                Some((routed, _)) => state.routed.push(routed),
                None => state.failed.push(line),
            }
        }
        self.fine.set(false);
        self.snapshot(SnapshotStage::FirstPass, &state);
        if !rip_up {
            return state;
        }
        // First with the room the masks usually keep; what is still left is tried with the
        // least room that is safe, which opens the gaps between fine-pitch pads.
        for room in [ROOM_LOOSE, ROOM_TIGHT] {
            if room == ROOM_TIGHT {
                self.stage.set(Stage::LessRoom);
                for line in state.failed.clone() {
                    if self.out_of_time() {
                        return state;
                    }
                    self.report(false, Some(&state), || {
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
            self.stage.set(Stage::RipUp);
            for pass in 1..=RIP_UP_PASSES {
                let before = state.failed.len();
                for line in state.failed.clone() {
                    if self.out_of_time() {
                        return state;
                    }
                    self.report(false, Some(&state), || {
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
        self.snapshot(SnapshotStage::RipUp, &state);
        if self.negotiate && !state.failed.is_empty() {
            self.stage.set(Stage::Negotiation);
            self.negotiate(&mut state, ROOM_TIGHT, attempt, attempts);
            self.snapshot(SnapshotStage::Negotiation, &state);
            if !state.failed.is_empty() {
                self.stage.set(Stage::Chain);
                self.finish(&mut state, ROOM_TIGHT, attempt, attempts);
            }
        }
        // Whole nets ripped up for the lines still left, and routed again after them (issue
        // #1088): a board the stages above route is not touched.
        if self.most_rip_rounds > 0 && !state.failed.is_empty() && !self.out_of_time() {
            self.stage.set(Stage::RipRounds);
            self.rip_rounds(&mut state, attempt, attempts);
        }
        state
    }

    /// Keep what a stage of this try routed, if snapshots are asked for.
    pub(super) fn snapshot(&self, after: SnapshotStage, state: &State) {
        let Some(snapshots) = &self.snapshots else {
            return;
        };
        let mut routed: Vec<&Routed> = state.routed.iter().collect();
        routed.sort_by_key(|r| r.line);
        let mut failed = state.failed.clone();
        failed.sort_unstable();
        snapshots.borrow_mut().push(Snapshot {
            after,
            tracks: routed
                .iter()
                .flat_map(|r| r.tracks.iter().cloned())
                .collect(),
            vias: routed.iter().flat_map(|r| r.vias.iter().cloned()).collect(),
            left: failed.iter().map(|&l| self.lines[l].clone()).collect(),
        });
    }
}
