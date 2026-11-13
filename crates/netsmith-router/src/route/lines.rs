//! Searching for one line, in a window round its ends first.

use super::*;

impl<'a> Router<'a> {
    /// Try to route one line, given the lines routed so far, but leaving out the copper of
    /// the nets in `ignore`, which is as if they were ripped up.
    ///
    /// The line is searched for in a window round its ends first, which is cheap, and in a
    /// larger one only if that finds nothing.
    pub(super) fn try_line(
        &self,
        line: usize,
        state: &State,
        ignore: &[usize],
        room: f64,
        soft: bool,
    ) -> Option<(Routed, Vec<Spot>)> {
        self.try_line_locked(line, state, ignore, room, soft, &[])
    }

    /// [`Self::try_line`], where the lines in `locked` keep their copper hard even in a soft
    /// search: they are not to be ripped up. A line that a search for copper (not a soft one)
    /// fails to route is marked as failed in the stage the router is in, or out of time.
    pub(super) fn try_line_locked(
        &self,
        line: usize,
        state: &State,
        ignore: &[usize],
        room: f64,
        soft: bool,
        locked: &[usize],
    ) -> Option<(Routed, Vec<Spot>)> {
        let found = self.search_line(line, state, ignore, room, soft, locked);
        if found.is_none() && !soft {
            let stage = if self.stopped.get() {
                Stage::OutOfTime
            } else {
                self.stage.get()
            };
            if let Some(s) = self.failed_in.borrow_mut().get_mut(line) {
                *s = stage;
            }
        }
        found
    }

    /// The search of [`Self::try_line_locked`].
    pub(super) fn search_line(
        &self,
        line: usize,
        state: &State,
        ignore: &[usize],
        room: f64,
        soft: bool,
        locked: &[usize],
    ) -> Option<(Routed, Vec<Spot>)> {
        let others: Vec<&Routed> = state
            .routed
            .iter()
            .filter(|r| !ignore.contains(&self.nets[r.line]))
            .collect();
        // In a soft search the locked lines' copper stays hard; the rest is crossed at a price.
        let (fixed_others, free_others): (Vec<&Routed>, Vec<&Routed>) = if soft {
            others.iter().partition(|r| locked.contains(&r.line))
        } else {
            (others.clone(), Vec::new())
        };
        let (list, routed_keepouts) = self.copper_of(&fixed_others);
        let (soft_copper, soft_copper_keepouts) = self.copper_of(&free_others);
        // A soft search goes through the copper of other nets at a price, to find the nets
        // that are least in the way; what it finds is not copper that can be drawn.
        let obstacles: Vec<&Obstacle> = self.fixed.list.iter().chain(list.iter()).collect();
        let routed: Vec<&Obstacle> = list.iter().collect();
        let (hard, hard_keepouts): (&[&Obstacle], &[(Form, f64)]) = (&routed, &routed_keepouts);
        let soft_routed: Vec<&Obstacle> = soft_copper.iter().collect();
        let (soft_list, soft_keepouts): (&[&Obstacle], &[(Form, f64)]) =
            (&soft_routed, &soft_copper_keepouts);
        let mut keepouts = hard_keepouts.to_vec();
        keepouts.extend(self.fixed.via_keepouts.iter().copied());
        let (from, to) = &self.ends[line];
        let goal = if self.lines[line].to_plane {
            Goal::Via
        } else {
            Goal::Spots(to)
        };
        // The pads a via may stand in at the ends of the line, where the via would keep the
        // rules' space from the copper there is now.
        let net = self.nets[line];
        let usable =
            |pad: Option<InPad>| pad.filter(|p| self.via_fits(p, net, &obstacles, &keepouts));
        let (in_from, in_to) = (usable(self.in_pads[line].0), usable(self.in_pads[line].1));
        let from_extra: Vec<Spot> = in_from.iter().map(|p| p.spot).collect();
        let to_extra: Vec<Spot> = in_to.iter().map(|p| p.spot).collect();
        let extras = Extras {
            from: &from_extra,
            to: &to_extra,
            right_angles: true,
            coarse: false,
        };
        let ends: Vec<Spot> = from.iter().chain(to).copied().collect();
        let mut tried: Option<Window> = None;
        for margin in WINDOW_MARGINS.iter().map(|&m| Some(m)).chain([None]) {
            let window = match margin {
                Some(m) => Window::around(&self.grid, &ends, m),
                None => Window::full(&self.grid),
            };
            if tried == Some(window) {
                continue;
            }
            tried = Some(window);
            // What a search in this window found before, if nothing it depends on has changed
            // since ([`Router::memo`]): the masks are made only when something is searched.
            let print = self.print(window, net, &others, soft, locked);
            let key = |step: u8| MemoKey {
                line,
                room: room.to_bits(),
                soft,
                fine: self.fine.get(),
                escaping: self.escaping.get(),
                planned: self.planned.get(),
                pairing: self.pairing.get(),
                pouring: self.pouring.get(),
                via_price: self.via_price.get(),
                window,
                print,
                step,
            };
            let mut built: Option<Masks> = None;
            let make = |built: &mut Option<Masks>| {
                if built.is_none() {
                    *built = Some(self.window_masks(
                        line,
                        window,
                        room,
                        (hard, hard_keepouts),
                        (soft_list, soft_keepouts),
                    ));
                    if self.pairing.get()
                        && let Some(masks) = built.as_mut()
                    {
                        masks.follow = self.follow_cells(net, &others, window, self.width_of(line));
                        // The estimate is cut to match, so that the search does go out of its
                        // way to run beside the partner; the pass keeps the new copper only if
                        // it is no longer.
                        if !masks.follow[FRONT].is_empty() {
                            masks.estimate = FOLLOW;
                        }
                    }
                }
            };
            let own: Vec<&Routed> = others
                .iter()
                .copied()
                .filter(|r| r.line != line && self.nets[r.line] == net)
                .collect();
            let linked = match self.recall(&key(0)) {
                Some(Memo::Link(linked)) => linked,
                _ => {
                    make(&mut built);
                    let linked = link(built.as_ref().expect("made"), from, goal, extras);
                    self.remember(key(0), Memo::Link(linked));
                    linked
                }
            };
            match linked {
                Link::Joined => {}
                Link::Closed => break,
                Link::Cramped if window.covers(&self.grid) => break,
                Link::Cramped => continue,
            }
            // First with right angles, which the clean-up cuts into two bends of 45 degrees;
            // where a cut would come too near other copper, the line is searched for again with
            // no right angle at all (issue #415).
            let mut give_up = false;
            // A repair searches first in four directions, then (where a cut does not fit) in
            // eight, both with right angles that are cut; and last in eight with none. The first
            // pass starts at eight.
            let modes: &[(bool, bool)] = if soft {
                &[(true, true)]
            } else if self.fine.get() {
                &[(true, false), (false, false)]
            } else {
                &[(true, true), (true, false), (false, false)]
            };
            for (n, &(right_angles, coarse)) in modes.iter().enumerate() {
                let last = n + 1 == modes.len();
                let extras = Extras {
                    right_angles,
                    coarse,
                    ..extras
                };
                let found = match self.recall(&key(n as u8 + 1)) {
                    Some(Memo::Search(found)) => found,
                    _ => {
                        make(&mut built);
                        let masks = built.as_ref().expect("made");
                        let stop = || self.out_of_time();
                        // Where no right angle is allowed, a line often has no path: a pad that
                        // is reached only round a corner. A flood says so for much less than
                        // a search that runs out of states.
                        let flood = (!extras.right_angles && !extras.coarse)
                            .then(|| reachable(masks, from, goal, extras, &stop));
                        let searched = match flood {
                            Some(Err(miss)) => Err(miss),
                            Some(Ok(false)) => Err(Miss::Blocked),
                            _ => search(masks, from, goal, extras, &stop),
                        };
                        let found = match searched {
                            Ok(path) => {
                                let start = in_from.filter(|p| path.first() == Some(&p.spot));
                                let end = in_to.filter(|p| path.last() == Some(&p.spot));
                                let routed = self.realize(line, &path, start, end, masks);
                                // What the grid let through is measured again on the copper
                                // itself: a line whose copper is nearer to another net's than
                                // the rules allow, even at the narrowest track, is not routed.
                                // A path that only finds the nets in the way is never drawn, so
                                // its corners are not cut.
                                Some(
                                    self.settled(routed, net, &obstacles, !soft)
                                        .map(|r| (r, path)),
                                )
                            }
                            Err(Miss::Stopped) => return None,
                            Err(Miss::Blocked) => None,
                        };
                        self.remember(key(n as u8 + 1), Memo::Search(found.clone()));
                        found
                    }
                };
                match found {
                    // The joins with the net's own tracks are checked on the copper there is
                    // now: the memo does not depend on them.
                    Some(settled) => match settled
                        .and_then(|(r, path)| Ok((self.joins_cleanly(r, &own, !soft)?, path)))
                    {
                        Ok(done) => return Some(done),
                        Err(Unsettled::Corner) if !last => continue,
                        Err(_) => {
                            give_up = true;
                            break;
                        }
                    },
                    None if window.covers(&self.grid) => {
                        give_up = true;
                        break;
                    }
                    None => break,
                }
            }
            if give_up {
                break;
            }
        }
        // A pad on a plane whose way out is blocked is joined by a via in the pad.
        if self.lines[line].to_plane
            && !self.out_of_time()
            && let Some(pad) = in_from
        {
            let rules = &self.board.rules;
            let via = Via {
                net: self.lines[line].net.clone(),
                at: pad.at,
                diameter: rules.via_diameter,
                drill: rules.via_drill,
            };
            return Some((
                Routed {
                    line,
                    tracks: Vec::new(),
                    vias: vec![via],
                },
                Vec::new(),
            ));
        }
        None
    }

    /// The masks a line is searched for in, in a window: the copper of other nets, kept at
    /// `room`, with the cells where its track is necked down, the room its vias need, the
    /// cells inside its own pads and what the negotiation has learnt.
    pub(super) fn window_masks(
        &self,
        line: usize,
        window: Window,
        room: f64,
        (hard, hard_keepouts): (&[&Obstacle], &[(Form, f64)]),
        (soft_list, soft_keepouts): (&[&Obstacle], &[(Form, f64)]),
    ) -> Masks {
        let net = self.nets[line];
        let rules = &self.board.rules;
        // The width this line's net is drawn at.
        let track_width = self.width_of(line);
        let mut masks = self.masks(
            window,
            net,
            (hard, hard_keepouts),
            (soft_list, soft_keepouts),
            Reach {
                room,
                width: track_width,
            },
        );
        // With the least room, a track that cannot pass between two pads at its full
        // width may pass necked down: to the width its current needs anywhere, and to the
        // narrowest the process makes within `NECK_REACH` of the line's own pads.
        let floor = self.neck_width(line);
        if room == ROOM_TIGHT && track_width > rules.min_track_width + 1e-9 {
            let reach = |width| Reach { room, width };
            let mut narrow = self
                .masks(
                    window,
                    net,
                    (hard, hard_keepouts),
                    (soft_list, soft_keepouts),
                    reach(floor),
                )
                .track;
            if floor > rules.min_track_width + 1e-9 {
                let thin = self
                    .masks(
                        window,
                        net,
                        (hard, hard_keepouts),
                        (soft_list, soft_keepouts),
                        reach(rules.min_track_width),
                    )
                    .track;
                let near = self.near_ends(line, window);
                let pinch = [FRONT, BACK].map(|l| {
                    (0..window.cells())
                        .map(|c| narrow[l][c] && !thin[l][c] && near[c])
                        .collect::<Vec<bool>>()
                });
                for l in [FRONT, BACK] {
                    for c in 0..window.cells() {
                        if pinch[l][c] {
                            narrow[l][c] = false;
                        }
                    }
                }
                masks.pinch = Some(pinch);
            }
            if floor < track_width - 1e-9 || masks.pinch.is_some() {
                masks.wide = Some(std::mem::replace(&mut masks.track, narrow));
            }
        }
        self.room_for_vias(line, &mut masks);
        self.mark_own_pads(net, &mut masks);
        if self.escaping.get() {
            masks.lane = self.lanes.as_ref().and_then(|l| l.charged(window, net));
        }
        if self.planned.get()
            && let Some(plan) = &self.plan
            && let Some(inside) = &plan.corridors[line]
        {
            let tiles = &plan.tiles;
            let cost = |layer: usize| -> Vec<i64> {
                (0..window.cells())
                    .map(|c| {
                        let (a, b) = tiles.of(window.i0 + c % window.nx, window.j0 + c / window.nx);
                        let t = b * tiles.nx + a;
                        if inside[layer][t] {
                            0
                        } else {
                            crate::plan::crowding_cost(plan.fill[layer][t], self.plan_cost)
                        }
                    })
                    .collect()
            };
            let off = [cost(FRONT), cost(BACK)];
            if off.iter().flatten().any(|&c| c > 0) {
                masks.off_plan = Some(off);
            }
        }
        if self.pouring.get() {
            masks.pour = (self.pour_costs(net), POUR_VIA);
        }
        masks.pour.1 += self.via_price.get();
        self.add_history(&mut masks);
        masks
    }

    /// A print of everything a search of a line of `net` in `window` depends on that changes
    /// while the router runs ([`Router::memo`]): the routed copper near the window, each piece
    /// marked hard or soft (in a soft search, the copper of the `locked` lines is hard), and
    /// what the negotiation has learnt about the window's cells. The net's own tracks are left
    /// out: the masks and the check of the copper do not see them, and the joins with them are
    /// checked on every search ([`Router::joins_cleanly`]). The net's own vias are in: their
    /// holes keep other vias away. The print does not depend on the order of the copper.
    pub(super) fn print(
        &self,
        window: Window,
        net: usize,
        others: &[&Routed],
        soft: bool,
        locked: &[usize],
    ) -> u128 {
        use std::hash::{Hash, Hasher};
        let seen = Rect::new(
            self.grid.centre(window.i0, window.j0),
            self.grid
                .centre(window.i0 + window.nx - 1, window.j0 + window.ny - 1),
        )
        .expanded(self.print_reach);
        let one = |piece: &dyn Fn(&mut std::hash::DefaultHasher)| {
            let mut h = std::hash::DefaultHasher::new();
            piece(&mut h);
            h.finish()
        };
        let mut pieces: Vec<u64> = Vec::new();
        for r in others {
            let own = self.nets[r.line] == net;
            let hard = !soft || locked.contains(&r.line);
            if !own {
                for t in &r.tracks {
                    if seen.overlaps(&Form::Segment(t.from, t.to, t.width / 2.0).bounds()) {
                        pieces.push(one(&|h| {
                            (0u8, hard, t.layer == CopperLayer::Front).hash(h);
                            [t.from.x, t.from.y, t.to.x, t.to.y, t.width]
                                .map(f64::to_bits)
                                .hash(h);
                        }));
                    }
                }
            }
            for v in &r.vias {
                if seen.overlaps(&Form::Circle(v.at, v.diameter / 2.0).bounds()) {
                    pieces.push(one(&|h| {
                        (1u8, hard, own).hash(h);
                        [v.at.x, v.at.y, v.diameter].map(f64::to_bits).hash(h);
                    }));
                }
            }
        }
        pieces.sort_unstable();
        let history = self.history.borrow();
        let halves = [0u8, 1].map(|seed| {
            let mut h = std::hash::DefaultHasher::new();
            seed.hash(&mut h);
            pieces.hash(&mut h);
            if !history[FRONT].is_empty() {
                for layer in [FRONT, BACK] {
                    for j in window.j0..window.j0 + window.ny {
                        let row = j * self.grid.nx + window.i0;
                        history[layer][row..row + window.nx].hash(&mut h);
                    }
                }
            }
            h.finish()
        });
        (u128::from(halves[0]) << 64) | u128::from(halves[1])
    }
}
