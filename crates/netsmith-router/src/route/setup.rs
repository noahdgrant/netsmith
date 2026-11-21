//! The router: its setup and the helpers that read what it holds.

use super::*;

impl<'a> Router<'a> {
    pub(super) fn new(board: &'a Board, options: &RouteOptions) -> Self {
        let grid = Grid::over(&board.outline);
        let stack = Stack::of(board);
        let outer_only: Vec<bool> = board
            .nets
            .iter()
            .map(|n| board.keeps_outside(&n.name))
            .collect();
        let net_index: HashMap<&str, usize> = board
            .nets
            .iter()
            .enumerate()
            .map(|(i, n)| (n.name.as_str(), i))
            .collect();
        let one_layer = one_layer_of(board);
        // A net a routes file locks gets no copper (`netsmith_pcb::Routes`).
        let open = |l: &Ratline| !board.fixed.is_locked(&l.net);
        // Every pad of a plane net gets its own way to the plane first, whatever a pour of its
        // net might join (`Ratsnest::without_pour_fills`): the other copper is not routed yet.
        let mut lines = crate::decouple::lines(board, Ratsnest::without_pour_fills(board).lines);
        lines.retain(open);
        // The lines of the ratsnest, then the decoupling capacitors' vias to the pours, which
        // are routed after a complete try only ([`Router::to_the_pours`]), then the lines
        // between two pads of a plane net that may share a via ([`Router::share_vias`]).
        let routed_lines = lines.len();
        lines.extend(
            crate::decouple::return_lines(board)
                .into_iter()
                .filter(open),
        );
        let shared_from = lines.len();
        let (shares, share_lines): (Vec<(usize, usize)>, Vec<Ratline>) =
            share::share_lines(board, &lines, routed_lines)
                .into_iter()
                .unzip();
        lines.extend(share_lines);
        let nets = lines.iter().map(|l| net_index[l.net.as_str()]).collect();
        let ends = lines
            .iter()
            .map(|l| {
                let layers = if outer_only[net_index[l.net.as_str()]] {
                    2
                } else {
                    stack.len()
                };
                let to = if l.to_plane {
                    Vec::new()
                } else {
                    pad_spots(board, &l.to, l.to_at, &grid, layers)
                };
                let from = pad_spots(board, &l.from, l.from_at, &grid, layers);
                // A net on one layer starts and ends on that layer only (issue #1274).
                match one_layer[net_index[l.net.as_str()]] {
                    Some(k) => {
                        let on = |s: Vec<Spot>| s.into_iter().filter(|s| s.2 == k).collect();
                        (on(from), on(to))
                    }
                    None => (from, to),
                }
            })
            .collect();
        // A net whose current one via does not carry changes layer only where its cluster fits
        // (`room_for_vias`), and the others of a cluster never stand in a pad
        // (`crate::parallel`): it gets no via in a pad, which would stand alone.
        let in_pads = lines
            .iter()
            .map(|l| {
                // A net on one layer has no via, in a pad or anywhere (issue #1274).
                if board.line_vias(l) > 1 || one_layer[net_index[l.net.as_str()]].is_some() {
                    return (None, None);
                }
                // A via in a pad leads to the far outer layer and to every signal layer the
                // line may use (issue #833).
                let layers = if outer_only[net_index[l.net.as_str()]] {
                    2
                } else {
                    stack.len()
                };
                let to = if l.to_plane {
                    None
                } else {
                    in_pad(board, &stack, &l.to, l.to_at, &grid, layers)
                };
                (in_pad(board, &stack, &l.from, l.from_at, &grid, layers), to)
            })
            .collect();
        let necks = lines
            .iter()
            .map(|l| {
                [
                    board.neck_box(&l.from.refdes),
                    (!l.to_plane)
                        .then(|| board.neck_box(&l.to.refdes))
                        .flatten(),
                ]
            })
            .collect();
        let depth: Vec<f64> = lines
            .iter()
            .map(|l| {
                let to = if l.to_plane {
                    0.0
                } else {
                    pad_depth(board, &l.to, l.to_at)
                };
                pad_depth(board, &l.from, l.from_at).max(to)
            })
            .collect();
        let class = classes(board, &lines, &depth, true);
        let mut fixed = initial_obstacles(board, &stack);
        fixed_copper(board, &stack, &net_index, &mut fixed);
        let mut fixed_by_net = vec![Vec::new(); board.nets.len()];
        for (k, o) in fixed.list.iter().enumerate() {
            if let Some(n) = o.net {
                fixed_by_net[n].push(k);
            }
        }
        let crowding = Crowding::over(&grid);
        let lanes = escape_lanes(board, &grid);
        // The farthest any copper reaches into a mask or a check: half the widest track or
        // a via, its space and the room, or a via's hole and its space, with a millimetre over.
        let rules = &board.rules;
        let widest = board
            .nets
            .iter()
            .flat_map(|n| {
                stack
                    .copper
                    .iter()
                    .map(|&l| board.track_width_on(&n.name, l))
            })
            .chain(
                lines
                    .iter()
                    .filter(|l| board.line_branch(l).is_some())
                    .flat_map(|l| stack.copper.iter().map(|&c| board.line_width_on(l, c))),
            )
            .fold(rules.via_diameter, f64::max);
        let print_reach = 1.0
            + widest
            + board.widest_clearance()
            + MARGIN
            + (rules.via_hole_to_hole + rules.via_drill).max(rules.hole_clearance);
        Router {
            board,
            fixed,
            fixed_by_net,
            bases: RefCell::new(HashMap::new()),
            grid,
            lines,
            routed_lines,
            shared_from,
            shares,
            share: options.share_vias,
            nets,
            ends,
            in_pads,
            necks,
            depth,
            class,
            class_use: options.classes,
            classed: Cell::new(false),
            negotiate: options.negotiate,
            history: RefCell::new(vec![Vec::new(); stack.len()]),
            stack,
            outer_only,
            one_layer,
            fine: Cell::new(true),
            deadline: options.time_limit.map(|t| Instant::now() + t),
            stopped: Cell::new(false),
            progress: options.progress,
            cancel: options.cancel,
            last_report: Cell::new(Instant::now()),
            last_glimpse: Cell::new(None),
            costs: options.costs,
            weights: options.weights,
            power_groups: options.power_groups,
            stage: Cell::new(Stage::FirstPass),
            failed_in: RefCell::new(Vec::new()),
            crowding: RefCell::new(crowding),
            counting: Cell::new(true),
            snapshots: options.snapshots.then(|| RefCell::new(Vec::new())),
            lanes,
            escaping: Cell::new(false),
            plan: None,
            planned: Cell::new(false),
            pouring: Cell::new(false),
            via_price: Cell::new(0),
            plan_use: options.plan,
            plan_cost: options.plan_cost.unwrap_or(OFF_PLAN),
            memo: RefCell::new(HashMap::new()),
            print_reach,
            partners: diff_pair_partners(board),
            pairing: Cell::new(false),
            most_rip_rounds: if options.rip_up {
                options.rip_rounds
            } else {
                0
            },
            ripped: RefCell::new(RipRounds::default()),
        }
    }

    /// The masks of a window for a net: see [`masks_for`].
    pub(super) fn masks(
        &self,
        window: Window,
        net: usize,
        (routed, keepouts): (&[&Obstacle], &[(Form, f64)]),
        (soft, soft_keepouts): (&[&Obstacle], &[(Form, f64)]),
        reach: Reach,
    ) -> Masks {
        let key = reach.key();
        let base = self
            .bases
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                std::rc::Rc::new(Base::new(
                    self.board,
                    &self.grid,
                    &self.fixed,
                    reach,
                    self.stack.len(),
                ))
            })
            .clone();
        let own: Vec<&Obstacle> = self.fixed_by_net[net]
            .iter()
            .map(|&k| &self.fixed.list[k])
            .collect();
        let mut m = masks_for(
            &self.grid, window, self.board, &base, &own, routed, keepouts, net, reach,
        );
        m.turn = self.costs.turn;
        m.spans = self.stack.spans.clone();
        if !soft.is_empty() || !soft_keepouts.is_empty() {
            // The cells only the soft copper keeps tracks and vias from.
            let mut counts = Counts::new(window, self.stack.len());
            for o in soft.iter().filter(|o| o.net != Some(net)) {
                counts.stamp(&self.grid, &self.board.rules, o, reach, true);
            }
            for (form, keep) in soft_keepouts {
                counts.stamp_keepout(&self.grid, form, *keep);
            }
            m.soft = Some(Soft {
                track: (0..self.stack.len())
                    .map(|l| {
                        (0..window.cells())
                            .map(|c| counts.track[l][c] > 0 && !m.track[l][c])
                            .collect()
                    })
                    .collect(),
                via: (0..window.cells())
                    .map(|c| counts.via[c] > 0 && !m.via[c])
                    .collect(),
            });
        }
        // A net on one layer (issue #1274): no via, and no track on any other layer.
        if let Some(k) = self.one_layer[net] {
            m.via.fill(true);
            for (l, track) in m.track.iter_mut().enumerate() {
                if l != k {
                    track.fill(true);
                }
            }
            if let Some(soft) = &mut m.soft {
                soft.via.fill(false);
                for (l, track) in soft.track.iter_mut().enumerate() {
                    if l != k {
                        track.fill(false);
                    }
                }
            }
        }
        m
    }

    /// The space a line's copper keeps from copper of other nets, with the router's margin:
    /// its net's clearance (issue #758).
    pub(super) fn clearance_of(&self, line: usize) -> f64 {
        self.board.clearance(&self.lines[line].net) + MARGIN
    }

    /// The width a line's tracks are drawn at on the outer layers: its net's (ADR 0020), or its
    /// branch's when it carries the current of a pin that states one (issue #1113).
    pub(super) fn width_of(&self, line: usize) -> f64 {
        self.width_on(line, FRONT)
    }

    /// The width a line's tracks are drawn at on a layer of the stack: on an inner signal
    /// layer, a net with a known current is drawn for the inner copper (issue #833).
    pub(super) fn width_on(&self, line: usize, layer: usize) -> f64 {
        self.board
            .line_width_on(&self.lines[line], self.stack.layer(layer))
    }

    /// The widest a line's tracks are drawn on any layer of the stack.
    pub(super) fn widest_of(&self, line: usize) -> f64 {
        (0..self.stack.len())
            .map(|l| self.width_on(line, l))
            .fold(self.width_of(line), f64::max)
    }

    /// The narrowest a line's track may be necked down to away from its pads on the outer
    /// layers: the width its net's current needs (issue #431), and no wider than the track
    /// itself.
    pub(super) fn neck_width(&self, line: usize) -> f64 {
        self.neck_width_on(line, FRONT)
    }

    /// The same on a layer of the stack: on an inner signal layer, the width the current needs
    /// in the inner copper (issue #833).
    pub(super) fn neck_width_on(&self, line: usize, layer: usize) -> f64 {
        self.board
            .line_least_width_on(&self.lines[line], self.stack.layer(layer))
            .min(self.width_on(line, layer))
    }

    /// For a net whose current one via does not carry, leave a cell open for a via only where
    /// the others fit beside it (issue #431, [`crate::parallel`]): enough cells a cluster's step
    /// away, on the four straight or the four diagonal directions, open for a via too.
    pub(super) fn room_for_vias(&self, line: usize, masks: &mut Masks) {
        if self.vias_needed(line) <= 1 {
            return;
        }
        let w = masks.window;
        let keep: Vec<bool> = (0..w.cells())
            .map(|c| {
                masks.via[c]
                    || self
                        .cluster_cells(line, masks, w.i0 + c % w.nx, w.j0 + c / w.nx)
                        .is_none()
            })
            .collect();
        masks.via_alone = Some(std::mem::replace(&mut masks.via, keep));
    }

    /// The cells inside a pad of the net, on each layer the pad has copper on.
    pub(super) fn mark_own_pads(&self, net: usize, masks: &mut Masks) {
        let w = masks.window;
        let mut free = vec![vec![false; w.cells()]; self.stack.len()];
        // The box round a pad of any shape is more than its copper: a round pad fills only the
        // square inside it, so only that is counted, for every pad.
        for (r, layers) in self.own_pad_squares(net) {
            for (i, j) in self.grid.cells_near(&w, &r, 0.0) {
                let p = self.grid.centre(i, j);
                if r.contains(&Rect::new(p, p)) {
                    for (layer, free) in free.iter_mut().enumerate() {
                        if layers[layer] {
                            free[w.index(i, j)] = true;
                        }
                    }
                }
            }
        }
        masks.free_turn = free;
    }

    /// Whether `net` is a net of a diff pair ([`netsmith_pcb::Board::diff_pairs`]).
    pub(super) fn paired(&self, net: usize) -> bool {
        self.partners.get(net).is_some_and(Option::is_some)
    }

    /// The gap, edge to edge, a net of a diff pair keeps from its partner where they run
    /// together (issue #1223): the pair's stated spacing (a `PairImpedance`, which the net's
    /// clearance holds, issue #1025), or for a pair with none (two nets the names or a
    /// `DiffPair` make a pair) the net's clearance, the nearest the rules allow, so that every
    /// pair runs at one pitch: its two widths' halves and this gap.
    pub(super) fn pair_gap(&self, net: usize) -> f64 {
        self.board.clearance(&self.board.nets[net].name)
    }

    /// The centre distances from a track of a pair's partner at which a track of `net` runs
    /// beside it, for tracks `width` and `partner_width` wide: from half of each width and the
    /// pair's gap ([`Router::pair_gap`]: a stated spacing, which the net's clearance holds,
    /// [`netsmith_pcb::Board::clearance`], or the clearance) out to a cell past the nearest a
    /// track searched at [`ROOM_TIGHT`] comes ([`MARGIN`] and that room past the gap), where the
    /// drawn gap is the pair's gap rounded up to the grid (issue #1054). Since issue #1223 every
    /// pair keeps one pitch: before it, a pair with no stated spacing counted anything within
    /// the scorecard's coupled distance (0.5 mm, centre to centre) as beside its partner, and on a
    /// board the lines of a CAN bus ran 0.9 mm apart for 60 mm.
    pub(super) fn pair_band(&self, net: usize, width: f64, partner_width: f64) -> (f64, f64) {
        let half = width / 2.0 + partner_width / 2.0;
        let lo = half + self.pair_gap(net);
        (lo, lo + MARGIN + ROOM_TIGHT + PITCH)
    }

    /// The diff pairs of the board as the router routes them, each once: the names of its two
    /// nets, the lower index first.
    pub(super) fn pair_names(&self) -> Vec<(String, String)> {
        (0..self.partners.len())
            .filter_map(|n| self.partners[n].filter(|&p| p > n).map(|p| (n, p)))
            .map(|(n, p)| {
                (
                    self.board.nets[n].name.clone(),
                    self.board.nets[p].name.clone(),
                )
            })
            .collect()
    }

    /// Route every diff pair as two separate lines from here on (issue #1295): no net has a
    /// partner, so no pass runs it beside one, and its lines take the class they would have
    /// with no pair (not routed early as a pair, and ripped up for other lines as any line
    /// is). The memo of searches is emptied, since the classes say which copper a search may
    /// rip. The board's pairs are still matched in length by the meanders, which read the
    /// board.
    pub(super) fn unpair(&mut self) {
        self.partners = vec![None; self.partners.len()];
        self.class = classes(self.board, &self.lines, &self.depth, false);
        self.memo.borrow_mut().clear();
    }

    /// The track of `partner` that a net of its pair runs beside: the router's own copper of it
    /// in `routed`, and the copper the board had of it (a routes file's fixed copper, which
    /// `netsmith routes route` of one net of a pair leaves its partner as, issue #1295).
    pub(super) fn partner_tracks<'s>(
        &'s self,
        routed: impl IntoIterator<Item = &'s Routed>,
        partner: usize,
    ) -> Vec<&'s Track> {
        let name = &self.board.nets[partner].name;
        routed
            .into_iter()
            .filter(|r| self.nets[r.line] == partner)
            .flat_map(|r| r.tracks.iter())
            .chain(self.board.tracks.iter().filter(|t| &t.net == name))
            .collect()
    }

    /// Whether `net` has no line of its own on this board (its copper is all fixed, or it has
    /// none): its partner in a pair runs beside the copper it has.
    pub(super) fn has_no_lines(&self, net: usize) -> bool {
        !(0..self.lines.len()).any(|l| self.nets[l] == net)
    }

    /// [`Masks::follow`] for a line of `net`: the cells of [`Router::pair_band`] round the
    /// centre line of the partner's routed tracks, on their layers, for the width of `line`
    /// (on each layer for a pair with an impedance target, else the outer one); empty when the
    /// net has no routed partner.
    pub(super) fn follow_cells(
        &self,
        net: usize,
        routed: &[&Routed],
        w: Window,
        line: usize,
    ) -> Vec<Vec<bool>> {
        let layers = self.stack.len();
        let Some(partner) = self.partners[net] else {
            return vec![Vec::new(); layers];
        };
        let tracks = self.partner_tracks(routed.iter().copied(), partner);
        if tracks.is_empty() {
            return vec![Vec::new(); layers];
        }
        let mut cells = vec![vec![false; w.cells()]; layers];
        for t in tracks {
            let Some(layer) = self.stack.index(t.layer) else {
                continue;
            };
            // A pair with a target is drawn at its width on each layer; any other at the outer
            // width, as before (issue #1025).
            let width = if self.paired(net) {
                self.width_on(line, layer)
            } else {
                self.width_of(line)
            };
            let (lo, hi) = self.pair_band(net, width, t.width);
            let form = Form::Segment(t.from, t.to, 0.0);
            for (i, j) in self.grid.cells_near(&w, &form.bounds(), hi) {
                let d = point_to_line(self.grid.centre(i, j), t.from, t.to);
                if d >= lo - PITCH * 0.75 && d <= hi {
                    cells[layer][w.index(i, j)] = true;
                }
            }
        }
        cells
    }

    /// How many vias side by side the line's net needs where it changes layer.
    pub(super) fn vias_needed(&self, line: usize) -> usize {
        self.board.line_vias(&self.lines[line])
    }

    /// Where the other vias of the line's cluster stand round a via at cell (i, j): a cluster's
    /// step from its centre (their copper overlaps, their holes keep the via-to-via space), on
    /// the four straight directions or else the four diagonal ones, the first whose nearest
    /// cells the masks leave open for a via, in that order. `None` if too few are open.
    pub(super) fn cluster_cells(
        &self,
        line: usize,
        masks: &Masks,
        i: usize,
        j: usize,
    ) -> Option<Vec<Pt>> {
        let want = self.vias_needed(line).saturating_sub(1).min(4);
        let rules = &self.board.rules;
        let step = rules.via_hole_to_hole + rules.via_drill + crate::parallel::MARGIN;
        let w = masks.window;
        let centre = self.grid.centre(i, j);
        let open = |p: Pt| {
            let (a, b) = (
                ((p.x - self.grid.origin.x) / PITCH).round(),
                ((p.y - self.grid.origin.y) / PITCH).round(),
            );
            if a >= 0.0 && b >= 0.0 && w.contains(a as usize, b as usize) {
                let alone = masks.via_alone.as_ref().unwrap_or(&masks.via);
                !alone[w.index(a as usize, b as usize)]
            } else {
                false
            }
        };
        let r = std::f64::consts::FRAC_1_SQRT_2;
        for set in [
            [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)],
            [(r, r), (-r, r), (-r, -r), (r, -r)],
        ] {
            let found: Vec<Pt> = set
                .iter()
                .map(|(a, b)| Pt::new(centre.x + a * step, centre.y + b * step))
                .filter(|&p| open(p))
                .take(want)
                .collect();
            if found.len() >= want {
                return Some(found);
            }
        }
        None
    }

    /// Whether a point is within `reach` of the pads of a part an end of the line is on.
    pub(super) fn near_end_by(&self, line: usize, p: Pt, reach: f64) -> bool {
        self.necks[line]
            .iter()
            .flatten()
            .any(|b| b.expanded(reach).contains(&Rect::new(p, p)))
    }

    /// Whether a point is within [`NECK_REACH`] of the pads of a part an end of the line is on.
    pub(super) fn near_end(&self, line: usize, p: Pt) -> bool {
        self.near_end_by(line, p, NECK_REACH)
    }

    /// For each cell of the window, whether it is within [`NECK_REACH`] of the line's pads.
    pub(super) fn near_ends(&self, line: usize, window: Window) -> Vec<bool> {
        (0..window.cells())
            .map(|c| {
                let (i, j) = (window.i0 + c % window.nx, window.j0 + c / window.nx);
                // A cell a step inside the reach, so that the step out of the last pinched cell,
                // which is drawn at the pinched width, stays within it.
                self.near_end_by(line, self.grid.centre(i, j), NECK_REACH - PITCH * 1.5)
            })
            .collect()
    }

    /// Whether the time limit has passed, or the caller asked for the router to stop.
    /// Whether the router was told to stop (`RouteOptions::cancel`), which the time limit is not.
    pub(super) fn cancelled(&self) -> bool {
        self.cancel.is_some_and(|c| c.load(Ordering::Relaxed))
    }

    pub(super) fn out_of_time(&self) -> bool {
        // A job that was cancelled (`fab`) ends here; the flag below stops the router and
        // keeps what it routed, which is what the PCB view's "stop" asks for.
        netsmith_core::job::checkpoint();
        if !self.stopped.get()
            && (self.deadline.is_some_and(|d| Instant::now() >= d)
                || self.cancel.is_some_and(|c| c.load(Ordering::Relaxed)))
        {
            self.stopped.set(true);
        }
        self.stopped.get()
    }

    /// Tell the progress function something, if there is one, and the job's sink, if one
    /// listens (`netsmith_core::job::report_detail`, issue #764): `connections routed` of the
    /// lines the tries route, with the message and, given the try's `state`, the tracks and vias
    /// it has so far. Messages with `now` set are always sent, the others at most once in
    /// [`REPORT_EVERY`]. It only reads: the routing is the same with or without a listener.
    pub(super) fn report(
        &self,
        now: bool,
        state: Option<&State>,
        message: impl FnOnce() -> String,
    ) {
        if let Some(state) = state {
            self.glimpse(now, state);
        }
        let job = netsmith_core::job::listening();
        if self.progress.is_none() && !job {
            return;
        }
        if !(now || self.last_report.get().elapsed() >= REPORT_EVERY) {
            return;
        }
        self.last_report.set(Instant::now());
        let message = message();
        if let Some(progress) = self.progress {
            progress(&message);
        }
        if job {
            let routed = state.map_or(0, |s| {
                s.routed
                    .iter()
                    .filter(|r| r.line < self.routed_lines)
                    .count()
            });
            netsmith_core::job::report_detail(
                "connections routed",
                routed as u64,
                Some(self.routed_lines as u64),
                || {
                    let what = message
                        .strip_prefix("routing, ")
                        .or_else(|| message.strip_prefix("routing "))
                        .unwrap_or(&message);
                    match state {
                        Some(s) => {
                            let tracks: usize = s.routed.iter().map(|r| r.tracks.len()).sum();
                            let vias: usize = s.routed.iter().map(|r| r.vias.len()).sum();
                            format!("{what}; {tracks} tracks and {vias} vias so far")
                        }
                        None => what.to_string(),
                    }
                },
            );
        }
    }

    /// Show the copper the try has so far to a view that watches (`netsmith_pcb::show_glimpse`,
    /// issue #764): always with `now` (the end of a try), else at most once in
    /// [`GLIMPSE_EVERY`]. Only reads.
    fn glimpse(&self, now: bool, state: &State) {
        if !netsmith_pcb::glimpses_watched() {
            return;
        }
        if !now
            && self
                .last_glimpse
                .get()
                .is_some_and(|at| at.elapsed() < GLIMPSE_EVERY)
        {
            return;
        }
        self.last_glimpse.set(Some(Instant::now()));
        netsmith_pcb::show_glimpse(|| netsmith_pcb::Glimpse::Routing {
            tracks: state
                .routed
                .iter()
                .flat_map(|r| r.tracks.iter().cloned())
                .collect(),
            vias: state
                .routed
                .iter()
                .flat_map(|r| r.vias.iter().cloned())
                .collect(),
        });
    }

    /// What a try ended with, for [`Self::report`]: `routing, try 1 of 3: ended with 38 of 40
    /// connections routed, 2 left: net GND: U1.3 to C4.1, net SDA: U1.7 to J2.1`, the first
    /// [`TRY_ENDED_NAMES`] lines left by name.
    pub(super) fn try_ended(&self, state: &State, attempt: usize, attempts: usize) -> String {
        let routed = state
            .routed
            .iter()
            .filter(|r| r.line < self.routed_lines)
            .count();
        let mut text = format!(
            "routing, try {attempt} of {attempts}: ended with {routed} of {} connections routed",
            self.routed_lines
        );
        if !state.failed.is_empty() {
            let names: Vec<String> = state
                .failed
                .iter()
                .take(TRY_ENDED_NAMES)
                .map(|&l| self.lines[l].to_string())
                .collect();
            text.push_str(&format!(
                ", {} left: {}{}",
                state.failed.len(),
                names.join(", "),
                if state.failed.len() > TRY_ENDED_NAMES {
                    ", ..."
                } else {
                    ""
                }
            ));
        }
        text
    }

    /// The obstacles that routed copper makes, and the places it keeps vias from.
    pub(super) fn copper_of(&self, routed: &[&Routed]) -> (Vec<Obstacle>, Vec<(Form, f64)>) {
        let rules = &self.board.rules;
        let mut list = Vec::new();
        let mut keepouts = Vec::new();
        for r in routed {
            let net = Some(self.nets[r.line]);
            let clearance = self.clearance_of(r.line);
            for t in &r.tracks {
                list.push(Obstacle {
                    net,
                    layers: self.stack.layers_of(t.layer),
                    form: Form::Segment(t.from, t.to, t.width / 2.0),
                    clearance,
                    copper: true,
                });
            }
            for v in &r.vias {
                list.push(Obstacle {
                    net,
                    layers: self.stack.via_layers(v),
                    form: Form::Circle(v.at, v.diameter / 2.0),
                    clearance,
                    copper: true,
                });
                // Another via's hole keeps its distance, whatever the net.
                keepouts.push((
                    Form::Circle(v.at, 0.0),
                    rules.via_hole_to_hole + rules.via_drill + MARGIN,
                ));
            }
        }
        (list, keepouts)
    }
}

/// For each net of the board, the one layer of the stack it is routed on with no via (issue
/// #1274, [`Board::one_layer_nets`]): the front when every pad of the net has copper there,
/// else the back when every pad has copper there, else `None` (a net with pads on both faces
/// and no hole has no one layer, and is routed as any net; the check reports it).
fn one_layer_of(board: &Board) -> Vec<Option<usize>> {
    let wanted = board.one_layer_nets();
    board
        .nets
        .iter()
        .map(|net| {
            if !wanted.contains(&net.name) {
                return None;
            }
            let mut faces = (true, true);
            for pad in &net.pads {
                let Some(part) = board.part(&pad.refdes) else {
                    continue;
                };
                for found in part.footprint.pads_numbered(&pad.number) {
                    let (front, back) = part.pad_layers(found);
                    faces = (faces.0 && front, faces.1 && back);
                }
            }
            match faces {
                (true, _) => Some(FRONT),
                (false, true) => Some(BACK),
                (false, false) => None,
            }
        })
        .collect()
}
