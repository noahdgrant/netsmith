//! Putting parts at poses, and what a pose is limited by.

use super::*;

impl<'a> Engine<'a> {
    /// Move a part to a pose.
    pub(super) fn put(&mut self, i: usize, pose: Pose) {
        let turned = self.poses[i].turn != pose.turn;
        self.poses[i] = pose;
        // A turn changes where the body is, so the parts that follow it (or that it follows)
        // are moved off the grid by another amount.
        if turned && !self.links_of[i].is_empty() {
            self.realign();
        }
        self.reach_abs[i] = self.shape(i).reach.translated(self.pos(i));
    }

    pub(super) fn put_all(&mut self, poses: Vec<Pose>) {
        for (i, pose) in poses.into_iter().enumerate() {
            self.put(i, pose);
        }
    }

    pub(super) fn pos(&self, i: usize) -> Pt {
        let pose = self.poses[i];
        let sub = self.sub[i];
        let mut at = Pt::new(pose.x as f64 * UNIT + sub.x, pose.y as f64 * UNIT + sub.y);
        // An axis the requirements pin is at its exact value, not at the grid's nearest.
        if let Some(turn) = self.limits(i, pose.turn) {
            at.x = turn.pin_x.unwrap_or(at.x);
            at.y = turn.pin_y.unwrap_or(at.y);
        }
        at
    }

    /// Where along its axis the measure `field` of part `i`'s body is, on the sheet.
    pub(super) fn measure_of(&self, i: usize, field: PlaceField, axis: usize) -> f64 {
        let at = self.pos(i);
        (if axis == 0 { at.x } else { at.y }) + offset(field, self.shape(i).body)
    }

    /// The measure of `b` minus that of `a` for relation `k`.
    pub(super) fn link_value(&self, k: usize) -> f64 {
        let l = &self.links[k];
        self.measure_of(l.b, l.fb, l.axis) - self.measure_of(l.a, l.fa, l.axis)
    }

    /// How far relation `k` is from its window, in mm; 0 when it holds.
    pub(super) fn link_miss(&self, k: usize) -> f64 {
        self.links[k].miss(self.link_value(k))
    }

    /// The cost of a relation that does not hold: a hard rule, as a wall is.
    pub(super) fn link_cost(&self, k: usize) -> f64 {
        let miss = self.link_miss(k);
        if miss > LINK_EPS {
            W_HARD * (miss + 0.05)
        } else {
            0.0
        }
    }

    /// Put every part that follows another on the grid shifted by what makes the relation
    /// hold exactly at the middle of its window, given where its leader is and how each is
    /// turned. A part with no leader is on the grid.
    pub(super) fn realign(&mut self) {
        for k in 0..self.order.len() {
            let (f, ax) = self.order[k];
            let Some(link) = self.leader_of[f][ax] else {
                continue;
            };
            let l = &self.links[link];
            let t = (l.lo + l.hi) / 2.0;
            let (leader, wanted) = if l.b == f {
                // b = a + t, in the measures: origin_b + off_b = origin_a + off_a + t.
                let a = l.a;
                let at = self.measure_of(a, l.fa, ax);
                (a, at + t - offset(l.fb, self.shape(f).body))
            } else {
                let b = l.b;
                let at = self.measure_of(b, l.fb, ax);
                (b, at - t - offset(l.fa, self.shape(f).body))
            };
            debug_assert_ne!(leader, f);
            let grid = if ax == 0 {
                self.poses[f].x
            } else {
                self.poses[f].y
            } as f64
                * UNIT;
            let off = wanted - grid;
            let shift = off - UNIT * (off / UNIT).round();
            if ax == 0 {
                self.sub[f].x = shift;
            } else {
                self.sub[f].y = shift;
            }
            self.reach_abs[f] = self.shape(f).reach.translated(self.pos(f));
        }
    }

    /// Part `j` moved along the axis of relation `k` to where the relation holds, as near as
    /// it is now. `None` if `j` stays or the requirements pin it along that axis.
    pub(super) fn repair_pose(&self, k: usize, j: usize) -> Option<Pose> {
        let l = &self.links[k];
        let ax = l.axis;
        let now = self.poses[j];
        if self.models[j].fixed
            || self.limits(j, now.turn).is_some_and(|t| {
                if ax == 0 {
                    t.pin_x.is_some()
                } else {
                    t.pin_y.is_some()
                }
            })
        {
            return None;
        }
        let (lo, hi) = (l.lo, l.hi);
        if !lo.is_finite() && !hi.is_finite() {
            return None;
        }
        // Inside the window by the half of a grid step, so that rounding to the grid stays in.
        let margin = ((hi - lo) / 2.0).min(UNIT / 2.0 + 1e-9);
        let t = self.link_value(k).clamp(lo + margin, hi - margin);
        let (other, f_other, f_j, sign) = if l.b == j {
            (l.a, l.fa, l.fb, 1.0)
        } else {
            (l.b, l.fb, l.fa, -1.0)
        };
        let target = self.measure_of(other, f_other, ax) + sign * t;
        let origin = target - offset(f_j, self.shape(j).body);
        let sub = if ax == 0 {
            self.sub[j].x
        } else {
            self.sub[j].y
        };
        let g = ((origin - sub) / UNIT).round() as i64;
        let mut pose = now;
        if ax == 0 {
            pose.x = g;
        } else {
            pose.y = g;
        }
        Some(pose)
    }

    /// `change` with every part that must move with it to keep the relations it is in: a
    /// part whose relation to a moved part no longer holds is moved to where it does, and so
    /// on from it. The parts are left as they were.
    pub(super) fn expand(&mut self, change: Vec<(usize, Pose)>) -> Vec<(usize, Pose)> {
        if self.links.is_empty() || change.iter().all(|&(i, _)| self.links_of[i].is_empty()) {
            return change;
        }
        let mut undo: Vec<(usize, Pose)> =
            change.iter().map(|&(i, _)| (i, self.poses[i])).collect();
        let mut result = change.clone();
        for &(i, pose) in &change {
            self.put(i, pose);
        }
        let mut at = 0;
        while at < result.len() {
            let i = result[at].0;
            at += 1;
            for n in 0..self.links_of[i].len() {
                let k = self.links_of[i][n];
                if self.link_miss(k) <= LINK_EPS {
                    continue;
                }
                let l = &self.links[k];
                let j = if l.a == i { l.b } else { l.a };
                if result.iter().any(|&(m, _)| m == j) {
                    continue;
                }
                let Some(pose) = self.repair_pose(k, j) else {
                    continue;
                };
                let pose = self.clamp(j, pose);
                undo.push((j, self.poses[j]));
                self.put(j, pose);
                result.push((j, pose));
            }
        }
        for &(i, pose) in undo.iter().rev() {
            self.put(i, pose);
        }
        result
    }

    /// What the requirements allow part `i` in turn `turn`, if it has any.
    pub(super) fn limits(&self, i: usize, turn: u8) -> Option<Turn> {
        self.models[i].want.as_ref()?.turns[usize::from(turn)]
    }

    /// The pose nearest `pose` that meets the design's requirements on the part: the next
    /// turn they allow if this one is ruled out, and the position pushed into the window.
    pub(super) fn clamp(&self, i: usize, pose: Pose) -> Pose {
        let Some(want) = &self.models[i].want else {
            return pose;
        };
        let turn = (0..4)
            .map(|k| (pose.turn + k) % 4)
            .find(|&t| want.turns[usize::from(t)].is_some())
            .unwrap_or(pose.turn);
        match want.turns[usize::from(turn)] {
            Some(t) => Pose {
                x: pose.x.clamp(t.x.0, t.x.1),
                y: pose.y.clamp(t.y.0, t.y.1),
                turn,
                text: pose.text,
            },
            None => pose,
        }
    }

    pub(super) fn shape(&self, i: usize) -> &Shape {
        &self.models[i].shapes[self.shape_index(i)]
    }

    /// Which of the part's shapes (and carried boxes) it has in its pose.
    pub(super) fn shape_index(&self, i: usize) -> usize {
        let pose = self.poses[i];
        variant(pose.turn, pose.text).min(self.models[i].shapes.len() - 1)
    }
}
