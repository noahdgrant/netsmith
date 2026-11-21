//! Ripping up lines for a line that does not fit: the rip-ups, the chain and the history the
//! negotiation (`negotiation.rs`) and the rounds learn.

use super::*;

impl<'a> Router<'a> {
    /// Count a soft path in the crowding map: once for each map cell it passes through.
    pub(super) fn count_crowding(&self, path: &[Spot]) {
        if !self.counting.get() {
            return;
        }
        let mut crowding = self.crowding.borrow_mut();
        let mut cells: Vec<usize> = path
            .iter()
            .map(|&(i, j, _)| crowding.index_of(i, j))
            .collect();
        cells.sort_unstable();
        cells.dedup();
        for c in cells {
            crowding.counts[c] = crowding.counts[c].saturating_add(1);
        }
    }

    /// Why each line left in `state` is left: the stage it last failed in (`failed_in`, of the
    /// try that gave the state), and the nets whose copper stands in its way, from one soft
    /// search on the state at the least room; when that search finds no path, what walls the line
    /// in ([`Self::walls`]). With `late` (the time limit has passed, `route_with` lifted it) only
    /// the first [`WHY_LATE`] lines are looked at, and the rest name nothing.
    pub(super) fn why(&self, state: &State, failed_in: &[Stage], late: bool) -> Vec<LineReport> {
        self.counting.set(false);
        let mut failed = state.failed.clone();
        failed.sort_unstable();
        failed.dedup();
        let name = |n: usize| self.board.nets[n].name.clone();
        failed
            .into_iter()
            .enumerate()
            .map(|(k, line)| {
                let looked = !late || k < WHY_LATE;
                let (nets, walls) = match looked.then(|| self.contested(line, state, ROOM_TIGHT)) {
                    Some(Some((_, nets))) => (nets, walls::Walls::default()),
                    Some(None) if !self.out_of_time() => {
                        let walls = self.walls(line, state);
                        (walls.nets.clone(), walls)
                    }
                    _ => (Vec::new(), walls::Walls::default()),
                };
                LineReport {
                    line: self.lines[line].clone(),
                    stage: failed_in.get(line).copied().unwrap_or(Stage::FirstPass),
                    blockers: nets.into_iter().map(name).collect(),
                    pads: walls.pads,
                    vias: walls.vias,
                    edge: walls.edge,
                }
            })
            .collect()
    }

    /// Where a line would go if the copper of other nets could be crossed, at a price per
    /// cell ([`SOFT`]), and the nets whose copper that path comes near: the nets least in the
    /// line's way, not all that lie between its ends. `None` if even that has no path.
    pub(super) fn contested(
        &self,
        line: usize,
        state: &State,
        room: f64,
    ) -> Option<(Vec<Spot>, Vec<usize>)> {
        self.contested_locked(line, state, room, &[])
    }

    /// [`Self::contested`], with the copper of the lines in `locked` hard too: it is not to be
    /// crossed, and their nets are not named.
    pub(super) fn contested_locked(
        &self,
        line: usize,
        state: &State,
        room: f64,
        locked: &[usize],
    ) -> Option<(Vec<Spot>, Vec<usize>)> {
        let net = self.nets[line];
        // A protected line's copper is no way through, and its net is not ripped up for this
        // line ([`Class::protected`]).
        let mut protected = self.protected(line, state);
        protected.extend_from_slice(locked);
        let (_, path) = self.try_line_locked(line, state, &[], room, true, &protected)?;
        self.count_crowding(&path);
        let reach = self.clearance_of(line) + self.widest_of(line) + PITCH;
        let mut found = Vec::new();
        let protected_nets: Vec<usize> = protected.iter().map(|&l| self.nets[l]).collect();
        for r in state
            .routed
            .iter()
            .filter(|r| self.nets[r.line] != net && !protected_nets.contains(&self.nets[r.line]))
        {
            let forms = r
                .tracks
                .iter()
                .map(|t| Form::Segment(t.from, t.to, t.width / 2.0))
                .chain(r.vias.iter().map(|v| Form::Circle(v.at, v.diameter / 2.0)));
            let near = forms.into_iter().any(|f| {
                path.iter()
                    .any(|&(i, j, _)| f.distance(self.grid.centre(i, j)) < reach)
            });
            let n = self.nets[r.line];
            if near && !found.contains(&n) {
                found.push(n);
            }
        }
        found.sort_unstable();
        Some((path, found))
    }

    /// [`Self::contested`] by line: where the line would go if the copper of other nets could
    /// be crossed, except the copper of the lines in `locked`, and for each other line how far
    /// its copper is from being crossed by that path, in mm (negative when the path crosses
    /// it, which is when the masks would keep the path from the cell), nearest first.
    pub(super) fn contested_lines(
        &self,
        line: usize,
        state: &State,
        room: f64,
        locked: &[usize],
    ) -> Option<Vec<(usize, f64)>> {
        let net = self.nets[line];
        let mut locked = locked.to_vec();
        locked.extend(self.protected(line, state));
        let locked = &locked[..];
        let (_, path) = self.try_line_locked(line, state, &[], room, true, locked)?;
        self.count_crowding(&path);
        let rules = &self.board.rules;
        // The cells the masks keep a track, or a via, from, for copper of this width.
        let track_reach = self.clearance_of(line) + self.widest_of(line) / 2.0 + room;
        let via_reach = self.clearance_of(line) + rules.via_diameter / 2.0 + room;
        // Where the path puts a via: the cell where it changes layer.
        let via_at: Vec<(usize, usize)> = path
            .windows(2)
            .filter(|w| w[0].2 != w[1].2)
            .map(|w| (w[0].0, w[0].1))
            .collect();
        let mut found: Vec<(usize, f64)> = Vec::new();
        for r in state
            .routed
            .iter()
            .filter(|r| self.nets[r.line] != net && !locked.contains(&r.line))
        {
            let tracks = r.tracks.iter().map(|t| {
                (
                    Form::Segment(t.from, t.to, t.width / 2.0),
                    self.stack.layers_of(t.layer),
                )
            });
            let vias = r.vias.iter().map(|v| {
                (
                    Form::Circle(v.at, v.diameter / 2.0),
                    self.stack.via_layers(v),
                )
            });
            let mut least = f64::MAX;
            for (f, layers) in tracks.chain(vias) {
                for &(i, j, layer) in &path {
                    if layers[layer] {
                        least = least.min(f.distance(self.grid.centre(i, j)) - track_reach);
                    }
                }
                for &(i, j) in &via_at {
                    least = least.min(f.distance(self.grid.centre(i, j)) - via_reach);
                }
            }
            if least < CHAIN_NEAR.last().copied().unwrap_or(0.0) {
                found.push((r.line, least));
            }
        }
        found.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        Some(found)
    }

    /// The last resort for a line that is still left: route it, ripping up the single lines
    /// (not whole nets) in its way, and route those again, each ripping up the lines in its
    /// way in turn, down to `CHAIN_DEPTH` levels, never touching a line that is routed for
    /// the one above. The whole attempt costs at most `CHAIN_SEARCHES` searches and changes
    /// `state` only if every line ends up routed. Nothing is random: the same board gives
    /// the same copper.
    pub(super) fn rip_up_chain(
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
        if depth >= CHAIN_DEPTH {
            return false;
        }
        *budget -= 1;
        let Some(blockers) = self.contested_lines(line, state, room, locked) else {
            return false;
        };
        // First the lines the path crosses, then those it comes near, as one set each, and
        // then each line it crosses alone.
        let mut options: Vec<Vec<usize>> = Vec::new();
        for &near in &CHAIN_NEAR {
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
                if self.rip_up_chain(l, &mut trial, room, depth + 1, &lock, budget) {
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

    /// [`Self::rip_up_chain`] for every line that is left, as far as the searches go.
    pub(super) fn finish(&self, state: &mut State, room: f64, attempt: usize, attempts: usize) {
        let mut total = CHAIN_TOTAL;
        for line in state.failed.clone() {
            if self.out_of_time() || total < 2 {
                return;
            }
            self.report(false, Some(&*state), || {
                format!(
                    "routing, try {attempt} of {attempts}: moving lines for {}, {} not routed",
                    self.lines[line],
                    state.failed.len()
                )
            });
            let mut trial = state.clone();
            let mut budget = CHAIN_SEARCHES.min(total);
            let start = budget;
            let done = self.rip_up_chain(line, &mut trial, room, 0, &[], &mut budget);
            total -= start - budget;
            if done {
                trial.failed.retain(|&f| f != line);
                *state = trial;
            }
        }
    }

    /// The nets whose copper stands in the way of a line.
    pub(super) fn blockers(&self, line: usize, state: &State, room: f64) -> Vec<usize> {
        self.contested(line, state, room)
            .map_or_else(Vec::new, |(_, nets)| nets)
    }

    /// Copy what the negotiation has learnt about the cells of the window into the masks.
    pub(super) fn add_history(&self, masks: &mut Masks) {
        let history = self.history.borrow();
        if history[FRONT].is_empty() {
            return;
        }
        let window = masks.window;
        for layer in 0..history.len() {
            let mut cells = vec![0u8; window.cells()];
            for j in window.j0..window.j0 + window.ny {
                let row = j * self.grid.nx;
                for i in window.i0..window.i0 + window.nx {
                    cells[window.index(i, j)] = history[layer][row + i];
                }
            }
            masks.history[layer] = cells;
        }
    }

    /// Make the cells of a path dearer to every line that comes after.
    pub(super) fn learn(&self, path: &[Spot]) {
        let mut history = self.history.borrow_mut();
        for &(i, j, layer) in path {
            let h = &mut history[layer][j * self.grid.nx + i];
            *h = (*h + 1).min(HISTORY_MOST);
        }
    }

    /// Try to get a failed line routed by ripping up the nets in its way, routing it, and
    /// routing what was ripped up again. If that does not work out, nothing changes.
    pub(super) fn rip_up_for(&self, line: usize, state: &mut State, room: f64) {
        let blockers = self.blockers(line, state, room);
        let mut attempts: Vec<Vec<usize>> = vec![blockers.clone()];
        if blockers.len() > 1 {
            attempts.extend(blockers.iter().map(|&n| vec![n]));
        }
        for ripped in attempts.into_iter().filter(|a| !a.is_empty()) {
            if self.out_of_time() {
                return;
            }
            let mut trial = state.clone();
            let (gone, kept): (Vec<Routed>, Vec<Routed>) = trial
                .routed
                .drain(..)
                .partition(|r| ripped.contains(&self.nets[r.line]));
            trial.routed = kept;
            let mut again: Vec<usize> = gone.iter().map(|r| r.line).collect();
            again.sort_by(|&a, &b| self.lines[a].length().total_cmp(&self.lines[b].length()));

            let Some((first, _)) = self.try_line(line, &trial, &[], room, false) else {
                continue;
            };
            trial.routed.push(first);
            let mut ok = true;
            for l in again {
                match self.try_line(l, &trial, &[], room, false) {
                    Some((r, _)) => trial.routed.push(r),
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                trial.failed.retain(|&f| f != line);
                *state = trial;
                return;
            }
        }
    }
}

impl<'a> Router<'a> {
    /// The routed lines whose copper is not ripped up for `line` on this try: those of a
    /// protected class before `line`'s ([`Class::protected`]). None on a try without classes.
    pub(super) fn protected(&self, line: usize, state: &State) -> Vec<usize> {
        if !self.classed.get() {
            return Vec::new();
        }
        state
            .routed
            .iter()
            .map(|r| r.line)
            .filter(|&l| self.class[l] < self.class[line] && self.class[l].protected())
            .collect()
    }
}
