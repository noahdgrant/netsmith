//! The stage that takes out vias a line does not need.

use super::*;

impl<'a> Router<'a> {
    /// Whether [`Self::fewer_vias`] may take vias out of a line: not a decoupling capacitor's
    /// via to the pour ([`Self::to_the_pours`]), not a line to a plane, not a line of a net that
    /// is a plane or a pour (its vias join it), not a net whose current needs vias side by side,
    /// and not a net of a diff pair, whose copper the pairs' pass shaped.
    pub(super) fn may_lose_vias(&self, line: usize) -> bool {
        let net = &self.lines[line].net;
        line < self.routed_lines
            && !self.lines[line].to_plane
            && self.vias_needed(line) <= 1
            && self.partners[self.nets[line]].is_none()
            && !self.board.pours.iter().any(|p| &p.net == net)
            && !self.board.planes.iter().any(|p| &p.net == net)
    }

    /// Take vias out on a try that routed every line (issue #431, the "remove vias" of KiCad's
    /// optimizer): each line that [`Self::may_lose_vias`], the most vias first, is searched for
    /// again against all the other copper with a via costing [`DEAR_VIA`] more, at the usual
    /// room and failing that at the least, and its new copper is kept only if it has fewer vias
    /// and is not much longer nor more on another net's pour ([`keeps_fewer_vias`]); failing
    /// that, with the line in its way ripped up too ([`Self::fewer_vias_with_others`]). At most
    /// [`VIA_SWEEPS`] sweeps, each while the last took a via out. A line is never left unrouted,
    /// and another line moves only in a change that takes vias out of the two together.
    pub(super) fn fewer_vias(&self, state: &mut State, attempt: usize, attempts: usize) {
        let stage = self.stage.get();
        let failed_in = self.failed_in.borrow().clone();
        let counting = self.counting.replace(false);
        self.via_price.set(DEAR_VIA);
        self.fine.set(true);
        for _ in 0..VIA_SWEEPS {
            let mut todo: Vec<(usize, usize)> = state
                .routed
                .iter()
                .filter(|r| !r.vias.is_empty() && self.may_lose_vias(r.line))
                .map(|r| (r.line, r.vias.len()))
                .collect();
            todo.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            let total = todo.len();
            let mut gained = false;
            for (k, (line, _)) in todo.into_iter().enumerate() {
                if self.out_of_time() {
                    break;
                }
                self.report(false, || {
                    format!(
                        "routing, try {attempt} of {attempts}: fewer vias, {k} of {total} lines"
                    )
                });
                let Some(at) = state.routed.iter().position(|r| r.line == line) else {
                    continue;
                };
                // A neighbour ripped up for an earlier line may have taken this one's vias out.
                if state.routed[at].vias.is_empty() {
                    continue;
                }
                let old = state.routed.remove(at);
                let before = self.via_measure(&old);
                let keeps = |new: &Routed| keeps_fewer_vias(before, self.via_measure(new));
                let kept = [ROOM_LOOSE, ROOM_TIGHT].into_iter().find_map(|room| {
                    self.try_line(line, state, &[], room, false)
                        .map(|(new, _)| new)
                        .filter(keeps)
                });
                let alone = kept.is_some();
                state.routed.insert(at, kept.unwrap_or(old));
                gained |= alone || self.fewer_vias_with_others(line, state);
            }
            if !gained {
                break;
            }
        }
        self.fine.set(false);
        self.via_price.set(0);
        self.counting.set(counting);
        *self.failed_in.borrow_mut() = failed_in;
        self.stage.set(stage);
    }

    /// A line's track length in mm, its vias, and its track on another net's pour in mm, as
    /// [`keeps_fewer_vias`] weighs them.
    pub(super) fn via_measure(&self, r: &Routed) -> (f64, usize, f64) {
        let length = r.tracks.iter().map(|t| t.from.distance(t.to)).sum();
        (length, r.vias.len(), self.on_pour(r))
    }

    /// The same for the lines of `group` in `state`, together.
    pub(super) fn group_measure(&self, state: &State, group: &[usize]) -> (f64, usize, f64) {
        state
            .routed
            .iter()
            .filter(|r| group.contains(&r.line))
            .map(|r| self.via_measure(r))
            .fold((0.0, 0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2))
    }

    /// For a line that cannot give up a via against the other copper as it is: the lines whose
    /// copper its cheapest way with dear vias crosses ([`Self::contested_lines`]), at most
    /// [`VIA_NEIGHBOURS`] and each one [`Self::may_lose_vias`] would take vias out of, are
    /// ripped up with it, the line is routed first and they after it, all with dear vias, and
    /// the change is kept only if every line is routed and together they have fewer vias, as
    /// [`keeps_fewer_vias`] weighs them. Whether it was kept.
    pub(super) fn fewer_vias_with_others(&self, line: usize, state: &mut State) -> bool {
        let Some(found) = self.contested_lines(line, state, ROOM_LOOSE, &[]) else {
            return false;
        };
        let crossed: Vec<usize> = found
            .iter()
            .filter(|&&(_, d)| d < 0.0)
            .map(|&(l, _)| l)
            .collect();
        if crossed.is_empty()
            || crossed.len() > VIA_NEIGHBOURS
            || crossed.iter().any(|&l| !self.may_lose_vias(l))
        {
            return false;
        }
        let mut group = vec![line];
        group.extend(&crossed);
        let mut trial = state.clone();
        trial.routed.retain(|r| !group.contains(&r.line));
        for &l in &group {
            let Some((new, _)) = self
                .try_line(l, &trial, &[], ROOM_LOOSE, false)
                .or_else(|| self.try_line(l, &trial, &[], ROOM_TIGHT, false))
            else {
                return false;
            };
            trial.routed.push(new);
        }
        if !keeps_fewer_vias(
            self.group_measure(state, &group),
            self.group_measure(&trial, &group),
        ) {
            return false;
        }
        // Each line of the group takes its new copper in its old place in the list.
        for r in state.routed.iter_mut().filter(|r| group.contains(&r.line)) {
            if let Some(new) = trial.routed.iter().find(|n| n.line == r.line) {
                *r = new.clone();
            }
        }
        true
    }
}
