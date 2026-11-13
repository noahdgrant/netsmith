//! Settling a found path: the memo of searches, and the corners and joins that are cleaned up.

use super::*;

impl<'a> Router<'a> {
    /// What a search with this key found before, if it is remembered.
    pub(super) fn recall(&self, key: &MemoKey) -> Option<Memo> {
        self.memo.borrow().get(key).cloned()
    }

    /// Remember what a search found. The memo is emptied when it grows past [`MEMO_MOST`],
    /// which only costs the searches again.
    pub(super) fn remember(&self, key: MemoKey, found: Memo) {
        let mut memo = self.memo.borrow_mut();
        if memo.len() >= MEMO_MOST {
            memo.clear();
        }
        memo.insert(key, found);
    }

    /// The line's copper as it will be drawn, checked against the copper of other nets, the
    /// pads, the holes and the board edge as they are, not as cells: a track that is nearer
    /// than the rules allow is drawn at the narrowest width the process makes instead (a stub
    /// from a fine-pitch pad, or a run from a cell the path was given at its end, for a wide
    /// net); `None` if some copper is too near whatever its width. This is what makes
    /// "routed" mean that netsmith's own design rules check finds the copper clear.
    pub(super) fn settled(
        &self,
        mut routed: Routed,
        net: usize,
        obstacles: &[&Obstacle],
        cut: bool,
    ) -> Result<Routed, Unsettled> {
        let rules = &self.board.rules;
        let floor = self.neck_width(routed.line);
        let line = routed.line;
        let others: Vec<&&Obstacle> = obstacles.iter().filter(|o| o.net != Some(net)).collect();
        // The space the rules ask for: an obstacle's clearance has the router's margin on it.
        let apart = |o: &Obstacle| o.clearance - MARGIN - EXACT_SLACK;
        for track in &mut routed.tracks {
            let layer = usize::from(track.layer == CopperLayer::Back);
            let clear = |width: f64| {
                let reach = Rect::new(
                    Pt::new(track.from.x.min(track.to.x), track.from.y.min(track.to.y)),
                    Pt::new(track.from.x.max(track.to.x), track.from.y.max(track.to.y)),
                );
                others.iter().filter(|o| o.layers[layer]).all(|o| {
                    let near = reach.expanded(width / 2.0 + o.clearance);
                    !near.overlaps(&o.form.bounds())
                        || o.form.distance_to_line(track.from, track.to) - width / 2.0 >= apart(o)
                })
            };
            if !clear(track.width) {
                // Narrower, but not below what the current needs, except near the line's pads.
                let steps = ((track.from.distance(track.to) / 0.05).ceil() as usize).max(1);
                let near = (0..=steps).all(|k| {
                    let t = k as f64 / steps as f64;
                    let p = Pt::new(
                        track.from.x + (track.to.x - track.from.x) * t,
                        track.from.y + (track.to.y - track.from.y) * t,
                    );
                    self.near_end(line, p)
                });
                let narrowest = if near { rules.min_track_width } else { floor };
                let width = [floor, narrowest]
                    .into_iter()
                    .filter(|&w| w < track.width - 1e-9)
                    .find(|&w| clear(w))
                    .ok_or(Unsettled::Clearance)?;
                track.width = width;
            }
        }
        for via in &routed.vias {
            let radius = via.diameter / 2.0;
            if !others
                .iter()
                .all(|o| o.form.distance(via.at) - radius >= apart(o))
            {
                return Err(Unsettled::Clearance);
            }
        }
        if cut {
            self.cut_corners(&mut routed, net, &others, &apart)?;
        }
        Ok(routed)
    }

    /// `routed` if every place where its tracks end meets the other tracks of its net there as
    /// `fab`'s `track_angle` check asks: two directions bend by 45 degrees at most, and three or
    /// more meet at right angles or wider, except inside a pad or a via of the net.
    /// `Err(Unsettled::Corner)` otherwise, for the line to be searched for again in the next way
    /// (issue #476): lines of one net may run over each other and cross at 45 degrees.
    pub(super) fn joins_cleanly(
        &self,
        routed: Routed,
        own: &[&Routed],
        check: bool,
    ) -> Result<Routed, Unsettled> {
        if !check || own.is_empty() {
            return Ok(routed);
        }
        let net = self.nets[routed.line];
        let pads = self.own_pad_squares(net);
        let vias: Vec<&Via> = own
            .iter()
            .flat_map(|r| r.vias.iter())
            .chain(routed.vias.iter())
            .collect();
        let tracks: Vec<&Track> = own.iter().flat_map(|r| r.tracks.iter()).collect();
        let same = |p: Pt, q: Pt| p.distance(q) < 1e-3;
        for t in &routed.tracks {
            let layer = usize::from(t.layer == CopperLayer::Back);
            for at in [t.from, t.to] {
                if pads
                    .iter()
                    .any(|(r, layers)| layers[layer] && r.contains(&Rect::new(at, at)))
                    || vias.iter().any(|v| v.at.distance(at) <= v.diameter / 2.0)
                {
                    continue;
                }
                // The directions out of `at` of every track of the net that ends there.
                let mut dirs: Vec<(f64, f64)> = Vec::new();
                let mut met = false;
                for (s, theirs) in routed
                    .tracks
                    .iter()
                    .map(|s| (s, false))
                    .chain(tracks.iter().map(|s| (*s, true)))
                {
                    if s.layer != t.layer {
                        continue;
                    }
                    for (end, far) in [(s.from, s.to), (s.to, s.from)] {
                        let d = far - end;
                        let n = d.x.hypot(d.y);
                        if same(end, at) && n > 1e-6 {
                            met |= theirs;
                            let u = (d.x / n, d.y / n);
                            if !dirs
                                .iter()
                                .any(|v| (v.0 - u.0).abs() < 1e-3 && (v.1 - u.1).abs() < 1e-3)
                            {
                                dirs.push(u);
                            }
                        }
                    }
                }
                if !met || dirs.len() < 2 {
                    continue;
                }
                let least = (0..dirs.len())
                    .flat_map(|i| (i + 1..dirs.len()).map(move |j| (i, j)))
                    .map(|(i, j)| {
                        let dot = dirs[i].0 * dirs[j].0 + dirs[i].1 * dirs[j].1;
                        dot.clamp(-1.0, 1.0).acos().to_degrees()
                    })
                    .fold(180.0, f64::min);
                let limit = if dirs.len() == 2 { 134.0 } else { 89.0 };
                if least < limit {
                    return Err(Unsettled::Corner);
                }
            }
        }
        Ok(routed)
    }

    /// Cut each right angle between two tracks of the line on one layer into two bends of 45
    /// degrees (issue #415): the corner is replaced by a diagonal from a point on one track to a
    /// point on the other, as far from the corner as the shorter of them allows, up to
    /// [`CHAMFERS`]' first, checked exactly against the other nets' copper. A corner inside a
    /// pad of the net is the pad's copper and stays. `Err(Unsettled::Corner)` when a corner has
    /// no cut that keeps the rules' space, for the line to be searched for with no right angle.
    pub(super) fn cut_corners(
        &self,
        routed: &mut Routed,
        net: usize,
        others: &[&&Obstacle],
        apart: &dyn Fn(&Obstacle) -> f64,
    ) -> Result<(), Unsettled> {
        let pads = self.own_pad_squares(net);
        let in_pad = |p: Pt, layer: usize| {
            pads.iter()
                .any(|(r, layers)| layers[layer] && r.contains(&Rect::new(p, p)))
        };
        let clear = |a: Pt, b: Pt, width: f64, layer: usize| {
            let reach = Rect::new(
                Pt::new(a.x.min(b.x), a.y.min(b.y)),
                Pt::new(a.x.max(b.x), a.y.max(b.y)),
            );
            others.iter().filter(|o| o.layers[layer]).all(|o| {
                let near = reach.expanded(width / 2.0 + o.clearance);
                !near.overlaps(&o.form.bounds())
                    || o.form.distance_to_line(a, b) - width / 2.0 >= apart(o)
            })
        };
        let same = |p: Pt, q: Pt| p.distance(q) < 1e-6;
        let mut k = 0;
        while k < routed.tracks.len() {
            let mut cut = false;
            for m in 0..routed.tracks.len() {
                if m == k || routed.tracks[m].layer != routed.tracks[k].layer {
                    continue;
                }
                let (a, b) = (&routed.tracks[k], &routed.tracks[m]);
                // The corner: an end of `a` that is an end of `b`.
                let Some((corner, a_far, b_far)) = [(a.to, a.from), (a.from, a.to)]
                    .into_iter()
                    .find_map(|(c, af)| {
                        if same(c, b.from) {
                            Some((c, af, b.to))
                        } else if same(c, b.to) {
                            Some((c, af, b.from))
                        } else {
                            None
                        }
                    })
                else {
                    continue;
                };
                let (u, v) = (a_far - corner, b_far - corner);
                let (la, lb) = (u.x.hypot(u.y), v.x.hypot(v.y));
                if la < 1e-6 || lb < 1e-6 || ((u.x * v.x + u.y * v.y) / (la * lb)).abs() > 1e-3 {
                    continue;
                }
                let layer = usize::from(a.layer == CopperLayer::Back);
                if in_pad(corner, layer) {
                    continue;
                }
                let width = a.width.min(b.width);
                let found = CHAMFERS.iter().copied().find_map(|c| {
                    if c >= la - 1e-6 || c >= lb - 1e-6 {
                        return None;
                    }
                    let p = Pt::new(corner.x + u.x / la * c, corner.y + u.y / la * c);
                    let q = Pt::new(corner.x + v.x / lb * c, corner.y + v.y / lb * c);
                    clear(p, q, width, layer).then_some((p, q))
                });
                let Some((p, q)) = found else {
                    return Err(Unsettled::Corner);
                };
                let net_name = routed.tracks[k].net.clone();
                let layer_of_track = routed.tracks[k].layer;
                for (index, to) in [(k, p), (m, q)] {
                    let t = &mut routed.tracks[index];
                    if same(t.from, corner) {
                        t.from = to;
                    } else {
                        t.to = to;
                    }
                }
                routed.tracks.push(Track {
                    net: net_name,
                    layer: layer_of_track,
                    from: p,
                    to: q,
                    width,
                    mid: None,
                });
                cut = true;
                break;
            }
            if !cut {
                k += 1;
            }
        }
        Ok(())
    }
}
