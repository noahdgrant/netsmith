//! The cost of one part and of a pair of parts, and of the whole.

use super::*;

impl<'a> Engine<'a> {
    /// What part `j` costs for being inside the box of a module instance it is not part of.
    pub(super) fn intrusion(&self, b: &Rect, j: usize) -> f64 {
        let c = self.court(j).centre();
        let depth = (c.x - b.min.x)
            .min(b.max.x - c.x)
            .min(c.y - b.min.y)
            .min(b.max.y - c.y);
        if depth > 0.0 {
            W_INTRUDER * (depth + 1.0)
        } else {
            0.0
        }
    }

    /// How far inside the outline a point is, negative outside it.
    pub(super) fn depth(&self, p: Pt) -> f64 {
        let o = &self.p.region;
        let straight = (p.x - o.min.x)
            .min(p.y - o.min.y)
            .min(o.max.x - p.x)
            .min(o.max.y - p.y);
        let r = self.p.corner_radius.min(o.width().min(o.height()) / 2.0);
        if r <= 0.0 {
            return straight;
        }
        let centre = Pt::new(
            if p.x < o.min.x + r {
                o.min.x + r
            } else if p.x > o.max.x - r {
                o.max.x - r
            } else {
                return straight;
            },
            if p.y < o.min.y + r {
                o.min.y + r
            } else if p.y > o.max.y - r {
                o.max.y - r
            } else {
                return straight;
            },
        );
        straight.min(r - dist(p, centre))
    }

    /// [`Engine::depth`], not counting the sides along an axis the design decides.
    pub(super) fn depth_without(&self, p: Pt, skip_x: bool, skip_y: bool) -> f64 {
        if !skip_x && !skip_y {
            return self.depth(p);
        }
        let o = &self.p.region;
        let mut depth = f64::INFINITY;
        if !skip_x {
            depth = depth.min((p.x - o.min.x).min(o.max.x - p.x));
        }
        if !skip_y {
            depth = depth.min((p.y - o.min.y).min(o.max.y - p.y));
        }
        depth
    }

    /// What a part costs on its own: walls, keepouts, its pull to an edge, its turn.
    pub(super) fn single(&self, i: usize) -> Cost {
        let mut c = Cost::default();
        let court = self.court(i);
        let model = &self.models[i];
        // The design says where a part goes along an axis it holds: the walls are not the
        // placer's to enforce there.
        let (skip_x, skip_y) = model
            .want
            .as_ref()
            .map_or((false, false), |w| (w.holds_x, w.holds_y));
        let wall = self.p.rules.edge_wall();
        for corner in court.corners() {
            c.hard += W_HARD * (wall - self.depth_without(corner, skip_x, skip_y)).max(0.0);
        }
        if let Some(t) = self.shape(i).text {
            for corner in t.translated(self.pos(i)).corners() {
                c.hard +=
                    W_HARD * (TEXT_EDGE - self.depth_without(corner, skip_x, skip_y)).max(0.0);
            }
        }
        // A pad keeps the copper to edge space from the outline, as the placement check asks.
        // The courtyard's wall does that unless a pad reaches past the courtyard or the wall
        // is thin; a part that hangs over the edge has no wall along the axes the design
        // holds, but its copper stays inside the outline and the process's space from its edge.
        let kept = self.p.rules.copper_to_edge + PAD_SLACK;
        if model.overhangs || wall + self.shape(i).pad_inset < kept {
            let pos = self.pos(i);
            for pad in &self.shape(i).pad_boxes {
                for corner in pad.translated(pos).corners() {
                    c.hard += W_HARD * (kept - self.depth(corner)).max(0.0);
                }
            }
        }
        // A pad keeps the process's distance from the edge of a cutout, as `check_placement`
        // asks: the courtyard may be smaller than the copper, or not drawn.
        if !self.p.cutouts.is_empty() {
            let pos = self.pos(i);
            for pad in &self.shape(i).pad_boxes {
                let pad = pad.translated(pos);
                for cut in self.p.cutouts {
                    let near = rect_distance(&pad, cut);
                    c.hard += W_HARD * (self.p.rules.copper_to_edge + PAD_SLACK - near).max(0.0);
                }
            }
        }
        let tall = self.zone_box(i);
        for (k, part) in self
            .p
            .keepouts
            .iter()
            .map(|k| (k, &court))
            .chain(model.no_go.iter().map(|k| (k, &tall)))
        {
            let depth = penetration(part, k, KEEPOUT_GAP);
            if depth > 0.0 {
                c.soft += W_GAP * depth;
                if penetration(part, k, 0.0) > 1e-6 {
                    c.hard += W_HARD * (penetration(part, k, 0.0) + 0.05);
                }
            }
        }
        let r = &self.p.region;
        let to_x = (court.min.x - r.min.x).min(r.max.x - court.max.x) - self.p.rules.edge_wall();
        let to_y = (court.min.y - r.min.y).min(r.max.y - court.max.y) - self.p.rules.edge_wall();
        match model.role {
            Role::Connector => c.soft += W_EDGE * to_x.min(to_y).max(0.0),
            Role::Mechanical => c.soft += W_EDGE * (to_x.max(0.0) + to_y.max(0.0)),
            Role::Plain => {}
        }
        if self.poses[i].turn != 0
            && (model.angles[0] + 90.0 * f64::from(self.poses[i].turn)) % 360.0 != 0.0
        {
            c.soft += W_TURN;
        }
        if self.poses[i].text != 0 && !model.fixed {
            c.soft += W_TURN;
        }
        c
    }

    /// What two parts cost together: courtyards, reference text and silkscreen, and the
    /// keepouts either carries against the other.
    pub(super) fn pair(&self, i: usize, j: usize) -> Cost {
        // A keepout a part carries reaches past the part: it is not in the part's reach.
        let mut c = self.carried_cost(i, j);
        c += self.carried_cost(j, i);
        c += self.pair_of_parts(i, j);
        c
    }

    /// What two parts cost together by their own boxes: courtyards, reference text and
    /// silkscreen.
    pub(super) fn pair_of_parts(&self, i: usize, j: usize) -> Cost {
        let mut c = Cost::default();
        if !self.reach_abs[i].overlaps(&self.reach_abs[j]) {
            return c;
        }
        let (a, b) = (&self.models[i], &self.models[j]);
        let (sa, sb) = (self.shape(i), self.shape(j));
        let (pa, pb) = (self.pos(i), self.pos(j));
        // Parts on opposite faces share area unless one has a hole through the board.
        if a.side != b.side && !a.hole && !b.hole {
            return c;
        }
        let (ca, cb) = (self.room(i), self.room(j));
        // Each part's pads leave through the space round it. Where a busy part (16 to 66 pads)
        // faces another the space is the same, so the escape spaces of the two add up and each
        // mm short of them costs more than a plain gap. Other pairs keep the larger of the
        // two spaces at the plain weight.
        let busiest = a.escape.max(b.escape);
        let busy = (BUSY_ESCAPE..=BUSY_MOST).contains(&busiest);
        let escape = if self.near_ok[i * self.models.len() + j] {
            0.0
        } else if busy {
            a.escape + b.escape
        } else {
            a.escape.max(b.escape)
        };
        let gap = if a.hole || b.hole {
            self.hole_gap
        } else {
            self.gap
        }
        .max(self.gap + escape);
        if !a.hole && !b.hole {
            let extra = if busy {
                W_ESCAPE * (escape / (2.0 * MAX_ESCAPE)).min(1.0)
            } else {
                0.0
            };
            c += Self::court_cost_escape(&ca, &cb, self.gap, escape, extra);
        } else {
            // A part with a hole takes room on each face by `Placed::room_on`: its courtyard,
            // or on a face its footprint draws no courtyard for, its pads. Pads are only kept
            // apart, not spaced.
            for (x, y, pads) in self.room_pairs(i, j) {
                c += Self::court_cost(&x, &y, if pads { 0.0 } else { gap });
            }
        }
        c += self.silk_cost(sa, pa, sb, pb, &cb);
        c += self.silk_cost(sb, pb, sa, pa, &ca);
        c
    }

    /// [`Engine::court_cost`] where `escape` of the space asked for beyond `gap` is the room
    /// the parts' pads need to leave, each mm of which costs `extra` more than the plain gap
    /// ([`W_ESCAPE`]).
    pub(super) fn court_cost_escape(a: &Rect, b: &Rect, gap: f64, escape: f64, extra: f64) -> Cost {
        let mut c = Cost::default();
        let full = penetration(a, b, gap + escape);
        if full > 0.0 {
            let plain = penetration(a, b, gap);
            c.soft += W_GAP * plain + (W_GAP + extra) * (full - plain);
            let over = penetration(a, b, 0.0);
            if over > 1e-6 {
                c.hard += W_HARD * (over + 0.05);
            }
        }
        c
    }

    /// What two boxes cost: soft within `gap` of each other, hard when they overlap.
    pub(super) fn court_cost(a: &Rect, b: &Rect, gap: f64) -> Cost {
        let mut c = Cost::default();
        let near = penetration(a, b, gap);
        if near > 0.0 {
            c.soft += W_GAP * near;
            let over = penetration(a, b, 0.0);
            if over > 1e-6 {
                c.hard += W_HARD * (over + 0.05);
            }
        }
        c
    }

    /// What the reference text and silk strokes of one part cost against another part: the
    /// text over its silkscreen, text, pads and mask openings, and the strokes over its
    /// pads and mask openings, all of which KiCad's check refuses; and, a little, text over
    /// the part's courtyard.
    pub(super) fn silk_cost(
        &self,
        from: &Shape,
        at_from: Pt,
        to: &Shape,
        at_to: Pt,
        court: &Rect,
    ) -> Cost {
        let to_pad = self.p.rules.silk_to_pad;
        let mut soft = 0.0;
        let mut refuse = |depth: f64| {
            if depth > 0.0 {
                soft += self.text_weight * (depth + 0.05);
            }
        };
        let mut over_court = 0.0;
        if let Some(t) = from.text {
            let t = t.translated(at_from);
            for r in &to.silk {
                refuse(penetration(&t, &r.translated(at_to), SILK_CLEAR));
            }
            for r in &to.copper {
                refuse(penetration(
                    &t,
                    &r.translated(at_to),
                    to_pad.max(SILK_CLEAR),
                ));
            }
            // Two reference texts apart, counted once (by the lower of the two parts).
            if let Some(other) = to
                .text
                .filter(|_| (at_from.x, at_from.y) < (at_to.x, at_to.y))
            {
                refuse(penetration(&t, &other.translated(at_to), SILK_CLEAR));
            }
            let over = penetration(&t, court, 0.0);
            if over > 0.0 {
                over_court = W_TEXT * (over + 0.05);
            }
        }
        for stroke in &from.silk {
            let stroke = stroke.translated(at_from);
            for r in &to.copper {
                refuse(penetration(&stroke, &r.translated(at_to), to_pad));
            }
        }
        Cost {
            soft: soft + over_court,
            hard: 0.0,
        }
    }

    /// The box round every courtyard, in half perimeter.
    pub(super) fn area_cost(&self) -> f64 {
        let mut lo = Pt::new(f64::MAX, f64::MAX);
        let mut hi = Pt::new(f64::MIN, f64::MIN);
        for i in 0..self.models.len() {
            let c = self.court(i);
            lo = Pt::new(lo.x.min(c.min.x), lo.y.min(c.min.y));
            hi = Pt::new(hi.x.max(c.max.x), hi.y.max(c.max.y));
        }
        if lo.x > hi.x {
            return 0.0;
        }
        W_AREA * ((hi.x - lo.x) + (hi.y - lo.y))
    }

    /// The cost of everything that involves a part of `set`, counting each net, pair of
    /// parts, module and decoupling pair once. The difference between this before and after
    /// moving the parts of `set` is the change in the whole cost.
    pub(super) fn eval(&mut self, set: &[usize]) -> Cost {
        self.stamp += 1;
        let stamp = self.stamp;
        for &i in set {
            self.part_mark[i] = stamp;
        }
        let mut c = Cost::default();
        let mut touched: Vec<usize> = Vec::new();
        for &i in set {
            for k in 0..self.nets_of[i].len() {
                let n = self.nets_of[i][k];
                if self.net_mark[n] != stamp {
                    self.net_mark[n] = stamp;
                    c.soft += self.net_cost(n);
                    if self.nets[n].lines {
                        touched.push(n);
                    }
                }
            }
            for k in 0..self.decs_of[i].len() {
                let d = self.decs_of[i][k];
                if self.dec_mark[d] != stamp {
                    self.dec_mark[d] = stamp;
                    c.soft += self.dec_cost(d);
                }
            }
            for k in 0..self.groups_of[i].len() {
                let g = self.groups_of[i][k];
                if self.group_mark[g] != stamp {
                    self.group_mark[g] = stamp;
                    c.soft += self.group_cost(g);
                }
            }
            if !self.models[i].fixed {
                c += self.single(i);
            }
            for j in 0..self.models.len() {
                if j == i || (self.part_mark[j] == stamp && j < i) {
                    continue;
                }
                if self.models[i].fixed && self.models[j].fixed {
                    continue;
                }
                c += self.pair(i, j);
            }
        }
        // The parts of the set inside the box of a module instance they are not part of
        // (those of an instance that has a part in the set were counted with it).
        for &i in set {
            for g in 0..self.groups.len() {
                if self.group_mark[g] != stamp && !self.in_group[g][i] {
                    c.soft += self.intrusion(&self.group_box(g), i);
                }
            }
        }
        for &i in set {
            for n in 0..self.links_of[i].len() {
                let k = self.links_of[i][n];
                if self.link_mark[k] != stamp {
                    self.link_mark[k] = stamp;
                    c.hard += self.link_cost(k);
                }
            }
        }
        c.soft += W_CROSSING * self.crossings(&touched, stamp) as f64;
        c.soft += self.area_cost();
        c
    }
}
