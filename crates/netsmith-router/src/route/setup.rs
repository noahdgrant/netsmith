//! The router: its setup and the helpers that read what it holds.

use super::*;

impl<'a> Router<'a> {
    pub(super) fn new(board: &'a Board, options: &RouteOptions) -> Self {
        let grid = Grid::over(&board.outline);
        let net_index: HashMap<&str, usize> = board
            .nets
            .iter()
            .enumerate()
            .map(|(i, n)| (n.name.as_str(), i))
            .collect();
        // A net a routes file locks gets no copper (`netsmith_pcb::Routes`).
        let open = |l: &Ratline| !board.fixed.is_locked(&l.net);
        let mut lines = crate::decouple::lines(board, Ratsnest::of(board).lines);
        lines.retain(open);
        // The lines of the ratsnest, then the decoupling capacitors' vias to the pours, which
        // are routed after a complete try only ([`Router::to_the_pours`]).
        let routed_lines = lines.len();
        lines.extend(
            crate::decouple::return_lines(board)
                .into_iter()
                .filter(open),
        );
        let nets = lines.iter().map(|l| net_index[l.net.as_str()]).collect();
        let ends = lines
            .iter()
            .map(|l| {
                let to = if l.to_plane {
                    Vec::new()
                } else {
                    pad_spots(board, &l.to, l.to_at, &grid)
                };
                (pad_spots(board, &l.from, l.from_at, &grid), to)
            })
            .collect();
        let in_pads = lines
            .iter()
            .map(|l| {
                let to = if l.to_plane {
                    None
                } else {
                    in_pad(board, &l.to, l.to_at, &grid)
                };
                (in_pad(board, &l.from, l.from_at, &grid), to)
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
        let class = classes(board, &lines, &depth);
        let mut fixed = initial_obstacles(board);
        fixed_copper(board, &net_index, &mut fixed);
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
            .map(|n| board.track_width(&n.name))
            .fold(rules.via_diameter, f64::max);
        let print_reach = 1.0
            + widest
            + rules.clearance
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
            nets,
            ends,
            in_pads,
            necks,
            depth,
            class,
            class_use: options.classes,
            classed: Cell::new(false),
            negotiate: options.negotiate,
            history: RefCell::new([Vec::new(), Vec::new()]),
            fine: Cell::new(true),
            deadline: options.time_limit.map(|t| Instant::now() + t),
            stopped: Cell::new(false),
            progress: options.progress,
            cancel: options.cancel,
            last_report: Cell::new(Instant::now()),
            costs: options.costs,
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
        let key = (reach.room.to_bits(), reach.width.to_bits());
        let base = self
            .bases
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                std::rc::Rc::new(Base::new(self.board, &self.grid, &self.fixed, reach))
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
        if !soft.is_empty() || !soft_keepouts.is_empty() {
            // The cells only the soft copper keeps tracks and vias from.
            let mut counts = Counts::new(window);
            for o in soft.iter().filter(|o| o.net != Some(net)) {
                counts.stamp(&self.grid, &self.board.rules, o, reach, true);
            }
            for (form, keep) in soft_keepouts {
                counts.stamp_keepout(&self.grid, form, *keep);
            }
            m.soft = Some(Soft {
                track: [FRONT, BACK].map(|l| {
                    (0..window.cells())
                        .map(|c| counts.track[l][c] > 0 && !m.track[l][c])
                        .collect()
                }),
                via: (0..window.cells())
                    .map(|c| counts.via[c] > 0 && !m.via[c])
                    .collect(),
            });
        }
        m
    }

    /// The width a line's tracks are drawn at: its net's (ADR 0020).
    pub(super) fn width_of(&self, line: usize) -> f64 {
        self.board.track_width(&self.lines[line].net)
    }

    /// The narrowest a line's track may be necked down to away from its pads: the width its
    /// net's current needs (issue #431), and no wider than the track itself.
    pub(super) fn neck_width(&self, line: usize) -> f64 {
        self.board
            .least_width(&self.lines[line].net)
            .min(self.width_of(line))
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
        let mut free = [vec![false; w.cells()], vec![false; w.cells()]];
        // The box round a pad of any shape is more than its copper: a round pad fills only the
        // square inside it, so only that is counted, for every pad.
        for (r, layers) in self.own_pad_squares(net) {
            for (i, j) in self.grid.cells_near(&w, &r, 0.0) {
                let p = self.grid.centre(i, j);
                if r.contains(&Rect::new(p, p)) {
                    for layer in [FRONT, BACK] {
                        if layers[layer] {
                            free[layer][w.index(i, j)] = true;
                        }
                    }
                }
            }
        }
        masks.free_turn = free;
    }

    /// [`Masks::follow`] for a line of `net`: the cells from the pair's spacing (half of each
    /// width and the clearance) out to [`PAIR_NEAR`] from the centre line of the partner's
    /// routed tracks, on their layers; empty when the net has no routed partner.
    pub(super) fn follow_cells(
        &self,
        net: usize,
        routed: &[&Routed],
        w: Window,
        width: f64,
    ) -> [Vec<bool>; 2] {
        let Some(partner) = self.partners[net] else {
            return [Vec::new(), Vec::new()];
        };
        let tracks: Vec<&Track> = routed
            .iter()
            .filter(|r| self.nets[r.line] == partner)
            .flat_map(|r| r.tracks.iter())
            .collect();
        if tracks.is_empty() {
            return [Vec::new(), Vec::new()];
        }
        let mut cells = [vec![false; w.cells()], vec![false; w.cells()]];
        for t in tracks {
            let spacing = t.width / 2.0 + width / 2.0 + self.board.rules.clearance;
            let layer = usize::from(t.layer == CopperLayer::Back);
            let form = Form::Segment(t.from, t.to, 0.0);
            for (i, j) in self.grid.cells_near(&w, &form.bounds(), PAIR_NEAR) {
                let d = point_to_line(self.grid.centre(i, j), t.from, t.to);
                if d >= spacing - PITCH * 0.75 && d <= PAIR_NEAR {
                    cells[layer][w.index(i, j)] = true;
                }
            }
        }
        cells
    }

    /// How many vias side by side the line's net needs where it changes layer.
    pub(super) fn vias_needed(&self, line: usize) -> usize {
        let net = &self.lines[line].net;
        self.board
            .net_currents
            .iter()
            .find(|c| &c.net == net)
            .map_or(1, |c| c.vias)
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

    /// Tell the progress function something, if there is one. Messages with `now` set are
    /// always sent, the others at most once a second.
    pub(super) fn report(&self, now: bool, message: impl FnOnce() -> String) {
        let Some(progress) = self.progress else {
            return;
        };
        if now || self.last_report.get().elapsed() >= REPORT_EVERY {
            self.last_report.set(Instant::now());
            progress(&message());
        }
    }

    /// The obstacles that routed copper makes, and the places it keeps vias from.
    pub(super) fn copper_of(&self, routed: &[&Routed]) -> (Vec<Obstacle>, Vec<(Form, f64)>) {
        let rules = &self.board.rules;
        let mut list = Vec::new();
        let mut keepouts = Vec::new();
        for r in routed {
            let net = Some(self.nets[r.line]);
            for t in &r.tracks {
                list.push(Obstacle {
                    net,
                    layers: [t.layer == CopperLayer::Front, t.layer == CopperLayer::Back],
                    form: Form::Segment(t.from, t.to, t.width / 2.0),
                    clearance: rules.clearance + MARGIN,
                });
            }
            for v in &r.vias {
                list.push(Obstacle {
                    net,
                    layers: [true, true],
                    form: Form::Circle(v.at, v.diameter / 2.0),
                    clearance: rules.clearance + MARGIN,
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
