//! The rooms a part has on a face and what it carries.

use super::*;

impl<'a> Engine<'a> {
    /// How far part `j` is in the keepouts part `c` carries, or nearer than `gap` to them:
    /// for each, its name, its box where it is, and the depth. A keepout that forbids parts
    /// is measured against the part's room (its courtyard), as the check does; a height
    /// limit the part is too tall for, against its body grown by a margin
    /// ([`Engine::zone_box`]). The part that carries a keepout is not in it.
    pub(super) fn carried_depths(&self, c: usize, j: usize, gap: f64) -> Vec<(&'a str, Rect, f64)> {
        let mut found = Vec::new();
        if c == j || self.models[c].carries.is_empty() {
            return found;
        }
        let at = self.shape_index(c);
        let model = &self.models[j];
        for carry in &self.models[c].carries {
            let zone = &self.p.carried[carry.zone];
            let rect = carry.boxes[at.min(carry.boxes.len() - 1)].translated(self.pos(c));
            let mut depth = 0.0f64;
            if zone.parts {
                depth = depth.max(penetration(&self.room(j), &rect, gap));
            }
            if zone.limits(model.height, model.side) {
                depth = depth.max(penetration(&self.zone_box(j), &rect, gap));
            }
            if depth > 0.0 {
                found.push((zone.name.as_str(), rect, depth));
            }
        }
        found
    }

    /// What the keepouts part `c` carries cost part `j`: soft near them, hard in them, as a
    /// keepout for parts does.
    pub(super) fn carried_cost(&self, c: usize, j: usize) -> Cost {
        let mut cost = Cost::default();
        for (_, _, depth) in self.carried_depths(c, j, KEEPOUT_GAP) {
            cost.soft += W_GAP * depth;
        }
        for (_, _, depth) in self.carried_depths(c, j, 0.0) {
            if depth > 1e-6 {
                cost.hard += W_HARD * (depth + 0.05);
            }
        }
        cost
    }

    /// The boxes part `i` takes on `face`, as `Placed::room_on` finds them, and whether they
    /// are its pads.
    pub(super) fn rooms_on(&self, i: usize, face: Side) -> Vec<(Rect, bool)> {
        let m = &self.models[i];
        if !m.hole {
            return if face == m.side {
                vec![(self.room(i), false)]
            } else {
                Vec::new()
            };
        }
        let (own, opposite) = m.court_faces;
        let (here, there) = if face == m.side {
            (own, opposite)
        } else {
            (opposite, own)
        };
        if here || !there {
            return vec![(self.room(i), false)];
        }
        let pos = self.pos(i);
        self.shape(i)
            .rooms
            .iter()
            .map(|r| (r.translated(pos), true))
            .collect()
    }

    /// The pairs of boxes in which parts `i` and `j` must not overlap, one for each face
    /// they both take room on, and whether either box is a part's pads. Where one stands
    /// excused over the other's protruding pins (`netsmith_pcb::judge_protrusion`), the pair on that face is the
    /// pads of both.
    pub(super) fn room_pairs(&self, i: usize, j: usize) -> Vec<(Rect, Rect, bool)> {
        let mut pairs: Vec<(Rect, Rect, bool)> = Vec::new();
        let excused = self
            .excused(i, j)
            .or_else(|| self.excused(j, i).map(|_| false));
        for face in [Side::Front, Side::Back] {
            if let Some(pins_first) = excused {
                let (t, c) = if pins_first { (i, j) } else { (j, i) };
                if face == self.models[c].side {
                    let protrusions: Vec<Rect> = self
                        .shape(t)
                        .rooms
                        .iter()
                        .map(|r| r.translated(self.pos(t)))
                        .collect();
                    let pads: Vec<Rect> = self
                        .shape(c)
                        .pad_boxes
                        .iter()
                        .map(|r| r.translated(self.pos(c)))
                        .collect();
                    for &x in &protrusions {
                        for &y in &pads {
                            let pair = if pins_first {
                                (x, y, true)
                            } else {
                                (y, x, true)
                            };
                            if !pairs.contains(&pair) {
                                pairs.push(pair);
                            }
                        }
                    }
                    continue;
                }
            }
            let (ra, rb) = (self.rooms_on(i, face), self.rooms_on(j, face));
            for &(x, px) in &ra {
                for &(y, py) in &rb {
                    let pair = (x, y, px || py);
                    if !pairs.contains(&pair) {
                        pairs.push(pair);
                    }
                }
            }
        }
        pairs
    }

    /// Whether part `c` stands excused over the protruding pins of part `t` where they are now
    /// (`netsmith_pcb::judge_protrusion`): `Some(true)` for `t` first. Asked as `excused(t, c)`.
    pub(super) fn excused(&self, t: usize, c: usize) -> Option<bool> {
        let (mt, mc) = (&self.models[t], &self.models[c]);
        let length = mt.protrusion?;
        if !mt.hole || mc.hole || mt.side == mc.side {
            return None;
        }
        let cover = netsmith_pcb::Cover {
            height: mc.height,
            hand_fitted: mc.hand_fitted,
            body: self.zone_box(c),
            side: mc.side,
        };
        matches!(
            netsmith_pcb::judge_protrusion(length, &cover, self.p.zones),
            netsmith_pcb::Verdict::Excused { .. }
        )
        .then_some(true)
    }

    pub(super) fn court(&self, i: usize) -> Rect {
        self.shape(i).court.translated(self.pos(i))
    }

    /// What part `i` takes against a height-limited zone: its body, the same box the check
    /// measures (`Board::check_placement`), grown by [`HEIGHT_ZONE_MARGIN`], so that a clear
    /// result passes the check. A footprint that draws no fabrication outline has only its
    /// courtyard, which stays as it is (ADR 0045 section 11).
    pub(super) fn zone_box(&self, i: usize) -> Rect {
        if self.models[i].body_is_courtyard {
            return self.court(i);
        }
        self.shape(i)
            .body
            .translated(self.pos(i))
            .expanded(HEIGHT_ZONE_MARGIN)
    }

    /// The room part `i` takes against other courtyards: its courtyard, clipped to the
    /// outline if the design says it may hang over an edge (as `Board::check_placement`
    /// counts it, so that two parts that hang over the same edge may overlap beyond it).
    pub(super) fn room(&self, i: usize) -> Rect {
        let court = self.court(i);
        if self.models[i].may.any() {
            netsmith_pcb::clip_to_outline(&court, &self.p.outline)
        } else {
            court
        }
    }

    pub(super) fn pad_at(&self, part: usize, pad: usize) -> Pt {
        self.pos(part) + self.shape(part).pads[pad]
    }
}
