//! Legalising a placement and proposing moves.

use super::*;

impl<'a> Engine<'a> {
    /// The hard cost of one part against the walls, keepouts and every other part.
    pub(super) fn hard_of(&self, i: usize) -> f64 {
        let mut hard = self.single(i).hard;
        for &k in &self.links_of[i] {
            hard += self.link_cost(k);
        }
        // Only the parts that can cost something with it: the others add nothing.
        for j in self.near(i) {
            if j != i {
                hard += self.pair(i, j).hard;
            }
        }
        hard
    }

    /// Move each part that overlaps another, or a wall or a keepout, to the nearest place
    /// round it where it does not. Returns whether all of them found one.
    pub(super) fn legalise(&mut self) -> bool {
        let reach =
            ((self.p.region.width().max(self.p.region.height())) / (10.0 * UNIT)).ceil() as i64;
        self.legalise_by(10, reach)
    }

    /// [`Engine::legalise`] in steps of `step` grid units, as far as `reach` steps. The last
    /// pass of a run uses one unit (0.05 mm) and 40 steps: a part left a hair past a line or
    /// over a neighbour moves by the least that clears it, where the coarse pass, which is in
    /// half millimetres, would have gone further or found nothing (B5, PROVISIONAL).
    ///
    /// On a crowded board most parts start over others and each searches rings far out
    /// (issue #766). A pose is costed only against the parts near it ([`Engine::near`]), and the poses
    /// whose courtyard cannot be inside the walls are not tried ([`Engine::inside_walls`]);
    /// neither changes which pose is found first. The pass says how many parts it has been through
    /// (`parts legalised`), a cancel stops it at each ring, and a limit whose time is up (issue #764) ends
    /// it, the parts not yet reached left where they are.
    pub(super) fn legalise_by(&mut self, step: i64, reach: i64) -> bool {
        let mut all = true;
        let total = self.free.len() as u64;
        for k in 0..self.free.len() {
            netsmith_core::job::report("parts legalised", k as u64, Some(total));
            let i = self.free[k];
            if self.hard_of(i) <= HARD_EPS {
                continue;
            }
            if netsmith_core::job::time_up() {
                all = false;
                break;
            }
            let start = self.poses[i];
            let inside = self.inside_walls(i, start, step);
            let mut found = false;
            'rings: for ring in 1..=reach {
                // A ring is up to `8 * ring` poses, each a pass over the parts near it.
                netsmith_core::job::checkpoint();
                if let Some(b) = inside
                    && (b.0 > ring || b.2 < -ring || b.1 > ring || b.3 < -ring)
                {
                    // No pose of this ring has the courtyard inside the walls.
                    continue;
                }
                // The ring's poses row by row, each row from left to right.
                for dy in -ring..=ring {
                    let edge = dy.abs() == ring;
                    let mut dx = -ring;
                    while dx <= ring {
                        let here = (dx, dy);
                        dx = if edge || dx == ring { dx + 1 } else { ring };
                        let (dx, dy) = here;
                        if let Some(b) = inside
                            && !((b.0..=b.2).contains(&dx) && (b.1..=b.3).contains(&dy))
                        {
                            continue;
                        }
                        self.poses_tried += 1;
                        if inside.is_some()
                            && self.overlaps_at(i, start.x + dx * step, start.y + dy * step)
                        {
                            continue;
                        }
                        let pose = self.clamp(
                            i,
                            Pose {
                                x: start.x + dx * step,
                                y: start.y + dy * step,
                                turn: start.turn,
                                text: start.text,
                            },
                        );
                        // The parts it is related to go with it.
                        let change = self.expand(vec![(i, pose)]);
                        let undo: Vec<(usize, Pose)> =
                            change.iter().map(|&(j, _)| (j, self.poses[j])).collect();
                        for &(j, q) in &change {
                            self.put(j, q);
                        }
                        if change.iter().all(|&(j, _)| self.hard_of(j) <= HARD_EPS) {
                            found = true;
                            break 'rings;
                        }
                        for &(j, q) in undo.iter().rev() {
                            self.put(j, q);
                        }
                    }
                }
            }
            if !found {
                self.put(i, start);
                all = false;
            }
        }
        netsmith_core::job::report("parts legalised", total, Some(total));
        self.refresh_all();
        all
    }

    /// The steps `(dx, dy)` of `step` units from `start` that can put part `i`'s courtyard
    /// inside the walls, as a box `(dx_lo, dy_lo, dx_hi, dy_hi)`: a pose outside it costs
    /// more than [`HARD_EPS`] by the walls alone ([`Engine::single`]), so trying it is no use.
    /// The box is a little larger than what holds, never smaller. `None` for a part whose
    /// poses the requirements limit or that relations move others with (any pose is tried).
    ///
    /// On a crowded fixed outline most parts start in the rows far outside it (issue #766):
    /// each tried every pose of every ring before, all outside the walls.
    pub(super) fn inside_walls(
        &self,
        i: usize,
        start: Pose,
        step: i64,
    ) -> Option<(i64, i64, i64, i64)> {
        if self.models[i].want.is_some() || !self.links_of[i].is_empty() {
            return None;
        }
        let court = self.shape(i).court;
        let sub = self.sub[i];
        let r = &self.p.region;
        // A corner nearer the outline than the wall less this costs more than `HARD_EPS`.
        let wall = self.p.rules.edge_wall() - HARD_EPS / W_HARD - 1e-6;
        let unit = UNIT * step as f64;
        // The origin is `start * UNIT + d * unit + sub`.
        let at = |lo: f64, origin: f64| (lo - origin) / unit;
        let (ox, oy) = (start.x as f64 * UNIT + sub.x, start.y as f64 * UNIT + sub.y);
        let x_lo = at(r.min.x + wall - court.min.x, ox).floor();
        let x_hi = at(r.max.x - wall - court.max.x, ox).ceil();
        let y_lo = at(r.min.y + wall - court.min.y, oy).floor();
        let y_hi = at(r.max.y - wall - court.max.y, oy).ceil();
        if ![x_lo, x_hi, y_lo, y_hi].iter().all(|v| v.is_finite()) {
            return None;
        }
        let bound = |v: f64| v.clamp(-1e15, 1e15) as i64;
        Some((bound(x_lo), bound(y_lo), bound(x_hi), bound(y_hi)))
    }

    /// Whether part `i` with its origin at `(x, y)` units, as it is turned now, would overlap
    /// the courtyard of a part near it on its face, neither with a hole through the board: a
    /// pair that costs more than [`HARD_EPS`] ([`Engine::court_cost_escape`]), so the pose is
    /// no use. It measures what [`Engine::pair_of_parts`] would with the part put there,
    /// without putting it. Only for a part that [`Engine::inside_walls`] bounds (no
    /// requirements on its place and no relations, so it would be put exactly there).
    pub(super) fn overlaps_at(&self, i: usize, x: i64, y: i64) -> bool {
        let a = &self.models[i];
        if a.hole {
            return false;
        }
        let sub = self.sub[i];
        let at = Pt::new(x as f64 * UNIT + sub.x, y as f64 * UNIT + sub.y);
        let shape = self.shape(i);
        let clip = a.may.any();
        let mine: Vec<Rect> = shape
            .courts
            .iter()
            .map(|r| {
                let court = r.translated(at);
                if clip {
                    netsmith_pcb::clip_to_outline(&court, &self.p.outline)
                } else {
                    court
                }
            })
            .collect();
        self.grid
            .near(i, &shape.reach.translated(at))
            .into_iter()
            .filter(|&j| j != i && self.models[j].side == a.side && !self.models[j].hole)
            .any(|j| {
                let theirs = self.rooms(j);
                mine.iter()
                    .any(|m| theirs.iter().any(|t| penetration(m, t, 0.0) > 1e-6))
            })
    }

    /// Where `i` goes to be beside `j`, on the side of `j`'s courtyard nearest `towards`.
    pub(super) fn beside(&self, i: usize, j: usize, towards: Pt, rng: &mut Rng) -> Pose {
        let cj = self.court(j);
        let ci = self.shape(i).court;
        let (w, h) = (ci.width(), ci.height());
        let gap = self.gap + 0.1;
        // Distances from the point to each side of the courtyard.
        let sides = [
            (towards.x - cj.min.x, 0),
            (cj.max.x - towards.x, 1),
            (towards.y - cj.min.y, 2),
            (cj.max.y - towards.y, 3),
        ];
        let mut best = sides[0];
        for s in sides {
            if s.0 < best.0 {
                best = s;
            }
        }
        // Sometimes the second nearest side, so that a crowded side is not the only one.
        if rng.below(4) == 0 {
            let mut second = if best.1 == 0 { sides[1] } else { sides[0] };
            for s in sides {
                if s.1 != best.1 && s.0 < second.0 {
                    second = s;
                }
            }
            best = second;
        }
        let jitter = |rng: &mut Rng| rng.spread(6) as f64 * UNIT;
        let centre = match best.1 {
            0 => Pt::new(cj.min.x - gap - w / 2.0, towards.y + jitter(rng)),
            1 => Pt::new(cj.max.x + gap + w / 2.0, towards.y + jitter(rng)),
            2 => Pt::new(towards.x + jitter(rng), cj.min.y - gap - h / 2.0),
            _ => Pt::new(towards.x + jitter(rng), cj.max.y + gap + h / 2.0),
        };
        let origin = centre - ci.centre();
        Pose {
            x: (origin.x / UNIT).round() as i64,
            y: (origin.y / UNIT).round() as i64,
            turn: self.poses[i].turn,
            text: self.poses[i].text,
        }
    }

    /// The pose of `i` turned to `turn` about its courtyard's centre.
    pub(super) fn turned(&self, i: usize, turn: u8) -> Pose {
        let now = self.court(i).centre();
        let m = &self.models[i];
        let after = m.shapes[variant(turn, self.poses[i].text).min(m.shapes.len() - 1)]
            .court
            .centre();
        let origin = now - after;
        Pose {
            x: (origin.x / UNIT).round() as i64,
            y: (origin.y / UNIT).round() as i64,
            turn,
            text: self.poses[i].text,
        }
    }

    /// A random change: the parts and the poses they would take. Empty if nothing can move.
    /// A part with requirements on its place only goes where they allow.
    pub(super) fn propose(&mut self, rng: &mut Rng, reach: i64) -> Vec<(usize, Pose)> {
        // Only a run that weighs escape room pushes, so a run that does not draws the same
        // random numbers as before the push was built.
        let mut change = if self.w_escape > 0.0
            && !self.escapers.is_empty()
            && rng.below(PUSH_ONE_IN) == 0
        {
            self.push(rng, reach)
        } else {
            self.propose_free(rng, reach)
        };
        for (i, pose) in &mut change {
            *pose = self.clamp(*i, *pose);
        }
        // A part in a relation takes the parts it is related to along.
        self.expand(change)
    }

    /// A shift of a busy part that takes along the parts it lands on (issue #1035): busy part
    /// `b` moves by a step of at most [`PUSH_REACH`] units, and each free part that would then
    /// break a hard rule with a part moved (its courtyard, or the escape ring) moves by the
    /// same step, and so on from it, up to [`PUSH_MOST`] parts. A plain shift of a part as big
    /// as an 80-pad module lands on the parts of its face at once and is refused, so without
    /// this it never leaves the place the rows gave it. Empty if `b` stays where it is.
    pub(super) fn push(&mut self, rng: &mut Rng, reach: i64) -> Vec<(usize, Pose)> {
        let b = self.escapers[rng.below(self.escapers.len())];
        let r = reach.min(PUSH_REACH);
        let (dx, dy) = (rng.spread(r), rng.spread(r));
        if self.models[b].fixed || (dx, dy) == (0, 0) {
            return Vec::new();
        }
        let by = |p: Pose| Pose {
            x: p.x + dx,
            y: p.y + dy,
            ..p
        };
        let mut old = vec![(b, self.poses[b])];
        self.put(b, by(self.poses[b]));
        let mut k = 0;
        while k < old.len() && old.len() < PUSH_MOST {
            let m = old[k].0;
            k += 1;
            for j in self.near(m) {
                if j == m
                    || self.models[j].fixed
                    || old.iter().any(|&(o, _)| o == j)
                    || old.len() >= PUSH_MOST
                    || self.pair(m, j).hard <= HARD_EPS
                {
                    continue;
                }
                old.push((j, self.poses[j]));
                self.put(j, by(self.poses[j]));
            }
        }
        let change = old.iter().map(|&(j, _)| (j, self.poses[j])).collect();
        dbg_push::LAST.store(true, std::sync::atomic::Ordering::Relaxed);
        dbg_push::N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        dbg_push::LEN.fetch_add(old.len() as u64, std::sync::atomic::Ordering::Relaxed);
        for &(j, p) in old.iter().rev() {
            self.put(j, p);
        }
        change
    }

    pub(super) fn propose_free(&self, rng: &mut Rng, reach: i64) -> Vec<(usize, Pose)> {
        if self.free.is_empty() {
            return Vec::new();
        }
        let i = self.free[rng.below(self.free.len())];
        let now = self.poses[i];
        let roll = rng.below(100);
        if roll < 40 {
            let r = if rng.below(2) == 0 {
                reach
            } else {
                reach.min(20)
            };
            return vec![(
                i,
                Pose {
                    x: now.x + rng.spread(r),
                    y: now.y + rng.spread(r),
                    ..now
                },
            )];
        }
        if roll < 46 && self.models[i].turnable() {
            // The text to another side of the part.
            let text = (now.text + 1 + rng.below(3) as u8) % 4;
            return vec![(i, Pose { text, ..now })];
        }
        if roll < 52 {
            let m = &self.models[i];
            if !m.turnable() {
                return Vec::new();
            }
            // Any other turn the part may take: one of three, or of seven for a part that may
            // turn by 45 degrees.
            let n = m.turns();
            let turn = (now.turn + 1 + rng.below(usize::from(n) - 1) as u8) % n;
            return vec![(i, self.turned(i, turn))];
        }
        if roll < 66 {
            let j = self.free[rng.below(self.free.len())];
            if j == i || self.models[i].side != self.models[j].side {
                return Vec::new();
            }
            let (ci, cj) = (self.court(i).centre(), self.court(j).centre());
            let (si, sj) = (&self.shape(i).court, &self.shape(j).court);
            let (oi, oj) = (cj - si.centre(), ci - sj.centre());
            return vec![
                (
                    i,
                    Pose {
                        x: (oi.x / UNIT).round() as i64,
                        y: (oi.y / UNIT).round() as i64,
                        turn: now.turn,
                        text: now.text,
                    },
                ),
                (
                    j,
                    Pose {
                        x: (oj.x / UNIT).round() as i64,
                        y: (oj.y / UNIT).round() as i64,
                        turn: self.poses[j].turn,
                        text: self.poses[j].text,
                    },
                ),
            ];
        }
        if roll < 90 {
            let list = &self.neighbours[i];
            if list.is_empty() {
                return Vec::new();
            }
            let (j, k) = list[rng.below(list.len())];
            let mut pose = self.beside(i, j, self.pad_at(j, k), rng);
            if self.models[i].turnable() && rng.below(3) == 0 {
                let turn = rng.below(usize::from(self.models[i].turns())) as u8;
                let (now_turn, text) = (self.poses[i].turn, self.poses[i].text);
                let m = &self.models[i];
                let from = m.shapes[variant(now_turn, text)].court.centre();
                let to = m.shapes[variant(turn, text)].court.centre();
                let shift = from - to;
                pose = Pose {
                    x: pose.x + (shift.x / UNIT).round() as i64,
                    y: pose.y + (shift.y / UNIT).round() as i64,
                    turn,
                    text,
                };
            }
            return vec![(i, pose)];
        }
        // A whole module moves together.
        let list = &self.groups_of[i];
        if list.is_empty() {
            return Vec::new();
        }
        let g = list[rng.below(list.len())];
        let (dx, dy) = (rng.spread(reach), rng.spread(reach));
        self.groups[g]
            .iter()
            .filter(|&&m| !self.models[m].fixed)
            .map(|&m| {
                let p = self.poses[m];
                (
                    m,
                    Pose {
                        x: p.x + dx,
                        y: p.y + dy,
                        turn: p.turn,
                        text: p.text,
                    },
                )
            })
            .collect()
    }

    pub(super) fn all(&self) -> Vec<usize> {
        (0..self.models.len()).collect()
    }
}

pub(super) mod dbg_push {
    use std::sync::atomic::{AtomicBool, AtomicU64};
    pub static LAST: AtomicBool = AtomicBool::new(false);
    pub static N: AtomicU64 = AtomicU64::new(0);
    pub static LEN: AtomicU64 = AtomicU64::new(0);
    pub static OK: AtomicU64 = AtomicU64::new(0);
    pub static HARD: AtomicU64 = AtomicU64::new(0);
}
