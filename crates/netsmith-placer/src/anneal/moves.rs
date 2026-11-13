//! Legalising a placement and proposing moves.

use super::*;

impl<'a> Engine<'a> {
    /// The hard cost of one part against the walls, keepouts and every other part.
    pub(super) fn hard_of(&self, i: usize) -> f64 {
        let mut hard = self.single(i).hard;
        for &k in &self.links_of[i] {
            hard += self.link_cost(k);
        }
        for j in 0..self.models.len() {
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
    pub(super) fn legalise_by(&mut self, step: i64, reach: i64) -> bool {
        let mut all = true;
        for k in 0..self.free.len() {
            let i = self.free[k];
            if self.hard_of(i) <= HARD_EPS {
                continue;
            }
            let start = self.poses[i];
            let mut found = false;
            'rings: for ring in 1..=reach {
                for dy in -ring..=ring {
                    for dx in -ring..=ring {
                        if dx.abs().max(dy.abs()) != ring {
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
        self.refresh_all();
        all
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
        let mut change = self.propose_free(rng, reach);
        for (i, pose) in &mut change {
            *pose = self.clamp(*i, *pose);
        }
        // A part in a relation takes the parts it is related to along.
        self.expand(change)
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
        if roll < 46 && self.models[i].shapes.len() == VARIANTS {
            // The text to another side of the part.
            let text = (now.text + 1 + rng.below(3) as u8) % 4;
            return vec![(i, Pose { text, ..now })];
        }
        if roll < 52 {
            if self.models[i].shapes.len() < VARIANTS {
                return Vec::new();
            }
            let turn = (now.turn + 1 + rng.below(3) as u8) % 4;
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
            if self.models[i].shapes.len() == VARIANTS && rng.below(3) == 0 {
                let turn = rng.below(4) as u8;
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
