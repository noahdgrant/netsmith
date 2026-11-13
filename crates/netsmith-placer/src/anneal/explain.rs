//! Where a part ended: the words for a finding that is left.

use super::*;

impl<'a> Engine<'a> {
    /// Which hard rules the parts break as they are now, counted for the message of a run
    /// that found no arrangement that breaks none.
    pub(super) fn ended(&self, names: &[String], shown: &[String]) -> Ended {
        let mut ended = Ended::default();
        for i in 0..self.models.len() {
            if !self.models[i].fixed && self.single(i).hard > HARD_EPS {
                ended.outside += 1;
                ended.outside_parts.push(self.outside_at(i, &names[i]));
            }
            for j in i + 1..self.models.len() {
                if self.models[i].fixed && self.models[j].fixed {
                    continue;
                }
                let (a, b) = (&self.models[i], &self.models[j]);
                if a.side != b.side && !a.hole && !b.hole {
                    continue;
                }
                let over = self
                    .room_pairs(i, j)
                    .into_iter()
                    .find(|(x, y, _)| penetration(x, y, 0.0) > 1e-6);
                if let Some((ca, cb, _)) = over {
                    ended.overlaps += 1;
                    let size = (
                        (ca.max.x.min(cb.max.x) - ca.min.x.max(cb.min.x)).max(0.0),
                        (ca.max.y.min(cb.max.y) - ca.min.y.max(cb.min.y)).max(0.0),
                    );
                    let centre = Pt::new(
                        (ca.max.x.min(cb.max.x) + ca.min.x.max(cb.min.x)) / 2.0,
                        (ca.max.y.min(cb.max.y) + ca.min.y.max(cb.min.y)) / 2.0,
                    );
                    let region = &self.p.region;
                    ended.overlapping.push(OverlapAt {
                        a: names[i].clone(),
                        b: names[j].clone(),
                        area: size.0 * size.1,
                        size,
                        at: (centre.x - region.min.x, centre.y - region.min.y),
                        nearest: nearest_edge(region, centre),
                    });
                } else if self.pair_of_parts(i, j).hard > HARD_EPS {
                    ended.silk += 1;
                }
            }
        }
        self.name_carried(names, shown, &mut ended);
        for (k, l) in self.links.iter().enumerate() {
            let miss = self.link_miss(k);
            if miss > LINK_EPS {
                ended.relations.push(format!(
                    "{}: {} minus {} is {} mm along {}, and it allows {} ({} mm over)",
                    l.label,
                    shown[l.b],
                    shown[l.a],
                    mm(self.link_value(k)),
                    if l.axis == 0 { "x" } else { "y" },
                    window_text(l.lo, l.hi),
                    mm(miss)
                ));
            }
        }
        ended
            .overlapping
            .sort_by(|a, b| b.area.total_cmp(&a.area).then_with(|| a.a.cmp(&b.a)));
        ended.outside_parts.sort_by(|a, b| {
            b.worst
                .total_cmp(&a.worst)
                .then_with(|| a.part.cmp(&b.part))
        });
        ended
    }

    /// The parts that stand in a keepout another part carries, added to the parts that are
    /// outside, each with the keepout, its carrier and where it is.
    // The indices name the models, the designators and the names shown together.
    #[allow(clippy::needless_range_loop)]
    pub(super) fn name_carried(&self, names: &[String], shown: &[String], ended: &mut Ended) {
        for c in 0..self.models.len() {
            for j in 0..self.models.len() {
                if self.models[c].fixed && self.models[j].fixed {
                    continue;
                }
                for (zone, rect, depth) in self.carried_depths(c, j, 0.0) {
                    if depth <= 1e-6 {
                        continue;
                    }
                    let region = &self.p.region;
                    let at = rect.centre();
                    let why = format!(
                        "{} mm into the keepout `{zone}` that {} carries, at ({:.1}, {:.1})",
                        short_mm(depth),
                        shown[c],
                        at.x - region.min.x,
                        at.y - region.min.y
                    );
                    match ended.outside_parts.iter_mut().find(|o| o.part == names[j]) {
                        Some(o) => {
                            o.why.push(why);
                            o.worst = o.worst.max(depth);
                        }
                        None => {
                            ended.outside += 1;
                            ended.outside_parts.push(OutsideAt {
                                part: names[j].clone(),
                                why: vec![why],
                                worst: depth,
                            });
                        }
                    }
                }
            }
        }
    }

    /// In what way part `i` is outside, inside the margin or in a keepout.
    pub(super) fn outside_at(&self, i: usize, name: &str) -> OutsideAt {
        let (r, court) = (&self.p.region, self.court(i));
        let wall = self.p.rules.edge_wall();
        let (skip_x, skip_y) = self.models[i]
            .want
            .as_ref()
            .map_or((false, false), |w| (w.holds_x, w.holds_y));
        let sides = [
            ("left", r.min.x - court.min.x, skip_x),
            ("right", court.max.x - r.max.x, skip_x),
            ("top", r.min.y - court.min.y, skip_y),
            ("bottom", court.max.y - r.max.y, skip_y),
        ];
        let (mut why, mut worst) = (Vec::new(), 0.0f64);
        for (side, past, skip) in sides {
            if skip {
                continue;
            }
            if past > 1e-6 {
                why.push(format!("{} mm outside the {side} edge", short_mm(past)));
                worst = worst.max(past + wall);
            } else if past + wall > 1e-6 {
                why.push(format!(
                    "{} mm inside the {} mm margin at the {side} edge",
                    short_mm(past + wall),
                    (wall * 100.0).round() / 100.0
                ));
                worst = worst.max(past + wall);
            }
        }
        let tall = self.zone_box(i);
        for (k, part) in self
            .p
            .keepouts
            .iter()
            .map(|k| (k, &court))
            .chain(self.models[i].no_go.iter().map(|k| (k, &tall)))
        {
            let depth = penetration(part, k, 0.0);
            if depth > 1e-6 {
                let at = k.centre();
                why.push(format!(
                    "{} mm into a keepout, cutout or height limit at ({:.1}, {:.1})",
                    short_mm(depth),
                    at.x - r.min.x,
                    at.y - r.min.y
                ));
                worst = worst.max(depth);
            }
        }
        let pos = self.pos(i);
        let wanted = self.p.rules.copper_to_edge;
        if self.models[i].overhangs {
            let nearest = self
                .shape(i)
                .pad_boxes
                .iter()
                .flat_map(|pad| pad.translated(pos).corners())
                .map(|corner| (self.depth(corner), corner))
                .fold(
                    (f64::MAX, Pt::new(0.0, 0.0)),
                    |a, b| if b.0 < a.0 { b } else { a },
                );
            if nearest.0 < wanted - 1e-6 {
                let (side, _) = nearest_edge(r, nearest.1);
                why.push(if nearest.0 < 0.0 {
                    format!(
                        "a pad is {} mm outside the {side} edge",
                        short_mm(-nearest.0)
                    )
                } else {
                    format!(
                        "a pad is {} mm from the {side} edge and the process asks {wanted:.2} mm",
                        short_mm(nearest.0)
                    )
                });
                worst = worst.max(wanted - nearest.0);
            }
        }
        for cut in self.p.cutouts {
            let gap = self
                .shape(i)
                .pad_boxes
                .iter()
                .map(|pad| rect_distance(&pad.translated(pos), cut))
                .fold(f64::MAX, f64::min);
            if gap < wanted - 1e-6 {
                let at = cut.centre();
                why.push(format!(
                    "a pad is {gap:.2} mm from the cutout at ({:.1}, {:.1}) and the process asks {wanted:.2} mm",
                    at.x - r.min.x,
                    at.y - r.min.y
                ));
                worst = worst.max(wanted - gap);
            }
        }
        if why.is_empty() {
            why.push("reference text too near the edge, or a requirement".to_string());
        }
        OutsideAt {
            part: name.to_string(),
            why,
            worst,
        }
    }
}
