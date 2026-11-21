//! The router's copper made clean once the board is routed (issue #1486): each line's ends drawn
//! to its pads' centres, and the short pieces of a stair left by the grid joined into the runs
//! beside them. It runs on the best try's lines, one after another, each checked against the
//! copper of the other nets as it is then, so the searches, and what they route, are as before.
//!
//! **Ends.** A path begins and ends on the cell that holds a pad's centre, which is rarely on
//! the grid, so [`Router::realize`] joins the centre to the cell with a stub, and the path may
//! turn inside the pad too: a track that begins and ends in one pad (or the via in it) is no
//! join at the centre, nor is one from a via's centre to a bend inside the via. Each is folded
//! into the run it leads to, keeping every bend's angle: where the run's far end is a via or
//! the line's other end (a joint inside copper of the net, where any angle is allowed), the run is drawn from the pad's centre to there; where the run goes on in line,
//! it is drawn from the centre; where it ends at a bend, the run is moved across to pass the
//! pad's centre, along its own direction, and the bend moves along the next run to meet it.
//! Where that comes too near other copper, the stub is drawn on from the centre to meet the run
//! just outside the pad, at a bend of 45 degrees or less, which `track_angle` allows.
//!
//! **Steps.** The grid's paths step a cell at a time, and the cut of a right angle
//! ([`Router::cut_corners`]) on a run of a cell or two leaves pieces of 0.05 to 0.14 mm. A
//! piece shorter than [`shortest`] (0.14 mm or two widths) between two other pieces of the line is taken out: where the
//! runs on either side bend by 45 degrees or less, they are drawn on to meet; where they bend by
//! a right angle, they meet at a cut corner of [`CHAMFERS`]; where they run in parallel, they
//! meet at one bend of 45 degrees each way; where the piece after it runs on in parallel, nearer
//! than that, it goes too and the runs on either side meet. A change is kept only when it leaves fewer short
//! pieces, or as many and fewer pieces, so the clean-up ends, and moves no bend into a pad or a
//! via of the line.
//!
//! Every change is checked against the copper of other nets, the holes and the edge as the
//! settled copper is ([`Router::settled`]), and a change that would come too near is not made:
//! the line keeps that stub or step. The line's vias do not move.

use super::*;

/// A straight piece shorter than this, or than [`SHORT_WIDTHS`] of its widths, between two
/// others of its line, is a step (issue #1486): a diagonal of one cell of the grid is just
/// longer.
pub(crate) const SHORT: f64 = 0.14;

/// A straight piece shorter than this many of its widths is short: the quality report's measure
/// (`netsmith_fabricate::routes_tidy::TINY`, issue #1481).
pub(crate) const SHORT_WIDTHS: f64 = 2.0;

/// The least length of a straight piece of `width` that is not short.
pub(crate) fn shortest(width: f64) -> f64 {
    SHORT.max(SHORT_WIDTHS * width)
}

fn is_short(t: &Track) -> bool {
    t.from.distance(t.to) < shortest(t.width)
}

/// Ends nearer than this are one place.
const SAME: f64 = 1e-6;

/// How far round a line's copper other copper is looked at: farther than a change moves it.
const NEAR: f64 = 2.0;

/// The most short pieces in a row that one change takes out.
const RUN: usize = 3;

/// How far along a run a stub is drawn on at a time, to find where the run leaves the pad.
const GENTLE_STEP: f64 = 0.01;

/// The least length a piece keeps when a clean-up moves its ends.
const LEAST: f64 = 1e-3;

/// Whether a segment of a width on a layer keeps the rules' space from the other nets' copper.
pub(super) type Clear<'c> = dyn Fn(Pt, Pt, f64, usize) -> bool + 'c;

/// The unit direction from `a` to `b`, `None` for a piece of no length.
fn way(a: Pt, b: Pt) -> Option<(f64, f64)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let n = dx.hypot(dy);
    (n > SAME).then_some((dx / n, dy / n))
}

/// Where the line through `p` along `u` meets the line through `q` along `v`, `None` for
/// lines in parallel.
fn meet(p: Pt, u: (f64, f64), q: Pt, v: (f64, f64)) -> Option<Pt> {
    let cross = u.0 * v.1 - u.1 * v.0;
    if cross.abs() < 1e-6 {
        return None;
    }
    let t = ((q.x - p.x) * v.1 - (q.y - p.y) * v.0) / cross;
    Some(Pt::new(p.x + u.0 * t, p.y + u.1 * t))
}

/// How far `z` is from `a` along `d`.
fn ahead(a: Pt, z: Pt, d: (f64, f64)) -> f64 {
    (z.x - a.x) * d.0 + (z.y - a.y) * d.1
}

fn same(p: Pt, q: Pt) -> bool {
    p.distance(q) < SAME
}

/// The end of `t` that is not `near`.
fn far(t: &Track, near: Pt) -> Pt {
    if same(t.from, near) { t.to } else { t.from }
}

/// The tracks of `tracks` on `layer` with an end at `at`, but those in `but`.
fn touching(tracks: &[Track], layer: CopperLayer, at: Pt, but: &[usize]) -> Vec<usize> {
    (0..tracks.len())
        .filter(|k| {
            !but.contains(k)
                && tracks[*k].layer == layer
                && (same(tracks[*k].from, at) || same(tracks[*k].to, at))
        })
        .collect()
}

/// How many pieces of `tracks` are short, and how many there are.
fn measure(tracks: &[Track]) -> (usize, usize) {
    let short = tracks.iter().filter(|t| is_short(t)).count();
    (short, tracks.len())
}

/// A change to a line's tracks: pieces given new ends, pieces added, pieces taken out.
#[derive(Default)]
struct Change {
    moved: Vec<(usize, Pt, Pt)>,
    added: Vec<Track>,
    gone: Vec<usize>,
}

impl Change {
    /// The tracks with the change made.
    fn apply(&self, tracks: &[Track]) -> Vec<Track> {
        let mut out = tracks.to_vec();
        for &(k, from, to) in &self.moved {
            out[k].from = from;
            out[k].to = to;
        }
        let mut gone = self.gone.clone();
        gone.sort_unstable_by(|a, b| b.cmp(a));
        for k in gone {
            out.remove(k);
        }
        out.extend(self.added.iter().cloned());
        out
    }

    /// Whether every piece it draws keeps the rules' space.
    fn clear(&self, tracks: &[Track], stack: &Stack, clear: &Clear<'_>) -> bool {
        let layer = |t: &Track| stack.index(t.layer).unwrap_or(FRONT);
        self.moved
            .iter()
            .all(|&(k, from, to)| clear(from, to, tracks[k].width, layer(&tracks[k])))
            && self
                .added
                .iter()
                .all(|t| clear(t.from, t.to, t.width, layer(t)))
    }
}

impl<'a> Router<'a> {
    /// Each line of `state` made clean (the module's header).
    pub(super) fn clean(&self, state: &mut State) {
        let mut copper: Vec<Vec<Obstacle>> = state
            .routed
            .iter()
            .map(|r| self.copper_of(&[r]).0)
            .collect();
        for i in 0..state.routed.len() {
            if self.out_of_time() {
                return;
            }
            let line = state.routed[i].line;
            let net = self.nets[line];
            // The copper near the line: a change moves a piece by less than a pad.
            let Some(reach) = state.routed[i]
                .tracks
                .iter()
                .map(|t| Rect::new(t.from, t.from).union(&Rect::new(t.to, t.to)))
                .reduce(|a, b| a.union(&b))
                .map(|r| r.expanded(NEAR))
            else {
                continue;
            };
            let others: Vec<&Obstacle> = self
                .fixed
                .list
                .iter()
                .chain(
                    (0..state.routed.len())
                        .filter(|&k| self.nets[state.routed[k].line] != net)
                        .flat_map(|k| copper[k].iter()),
                )
                .filter(|o| o.net != Some(net) && reach.overlaps(&o.form.bounds()))
                .collect();
            let own = self.clearance_of(line);
            let apart = |o: &Obstacle| o.space(own) - MARGIN - EXACT_SLACK;
            let clear = |a: Pt, b: Pt, width: f64, layer: usize| {
                let span = Rect::new(a, a).union(&Rect::new(b, b));
                others.iter().filter(|o| o.layers[layer]).all(|o| {
                    let near = span.expanded(width / 2.0 + apart(o) + MARGIN);
                    !near.overlaps(&o.form.bounds())
                        || o.form.distance_to_line(a, b) - width / 2.0 >= apart(o)
                })
            };
            let mut routed = state.routed[i].clone();
            self.snap_ends(&mut routed, net, &clear);
            self.smooth(&mut routed, &clear);
            // The line's joins with the rest of its net, judged as `track_angle` judges them: a
            // line whose clean copper bends there more is kept as it was.
            let own: Vec<&Routed> = state
                .routed
                .iter()
                .enumerate()
                .filter(|&(k, r)| k != i && self.nets[r.line] == net)
                .map(|(_, r)| r)
                .collect();
            if routed.tracks != state.routed[i].tracks && self.bends_well(&routed, &own, true) {
                copper[i] = self.copper_of(&[&routed]).0;
                state.routed[i] = routed;
            }
        }
    }

    /// The places of a line where any angle is allowed: its ends (pad centres) and its vias.
    fn joints(&self, routed: &Routed) -> Vec<Pt> {
        let l = &self.lines[routed.line];
        let mut out = vec![l.from_at];
        if !l.to_plane {
            out.push(l.to_at);
        }
        out.extend(routed.vias.iter().map(|v| v.at));
        out
    }

    /// Whether `q` is in the copper of the net at the line end `p` on `layer`: the pad's box,
    /// or a via there.
    fn in_end(&self, net: usize, routed: &Routed, p: Pt, q: Pt, layer: usize) -> bool {
        let in_pad = self.fixed_by_net[net].iter().any(|&k| {
            let o = &self.fixed.list[k];
            let Form::Rect(r) = o.form else { return false };
            o.copper
                && o.layers[layer]
                && r.contains(&Rect::new(p, p))
                && r.contains(&Rect::new(q, q))
        });
        in_pad
            || routed
                .vias
                .iter()
                .any(|v| same(v.at, p) && v.at.distance(q) <= v.diameter / 2.0)
    }

    /// The line's ends drawn to its pads' centres (the module's header).
    pub(super) fn snap_ends(&self, routed: &mut Routed, net: usize, clear: &Clear<'_>) {
        // The line's ends, and its vias: a track from a via's centre to a bend in the via is a
        // track to clean up as well.
        let joints = self.joints(routed);
        let mut ends = joints.clone();
        ends.dedup_by(|a, b| same(*a, *b));
        let at_joint = |p: Pt| joints.iter().any(|&j| same(j, p));
        let at_via = |p: Pt| routed.vias.iter().any(|v| same(v.at, p));
        for p in ends {
            // At most as many folds as the line has pieces.
            for _ in 0..routed.tracks.len() {
                let tracks = &routed.tracks;
                let Some(s) = (0..tracks.len()).find(|&k| {
                    let t = &tracks[k];
                    (same(t.from, p) || same(t.to, p))
                        && !same(t.from, t.to)
                        && self.in_end(
                            net,
                            routed,
                            p,
                            far(t, p),
                            self.stack.index(t.layer).unwrap_or(FRONT),
                        )
                }) else {
                    break;
                };
                let layer = tracks[s].layer;
                let c = far(&tracks[s], p);
                if at_via(c) {
                    break;
                }
                let [r] = touching(tracks, layer, c, &[s])[..] else {
                    break;
                };
                let e = far(&tracks[r], c);
                let (Some(u), Some(w)) = (way(c, e), way(p, c)) else {
                    break;
                };
                let li = self.stack.index(layer).unwrap_or(FRONT);
                let mut changes: Vec<Change> = Vec::new();
                if at_joint(e) || (u.0 * w.1 - u.1 * w.0).abs() < 1e-9 {
                    // The run ends inside copper of the net, or goes on in line with the stub.
                    changes.push(Change {
                        moved: vec![(r, p, e)],
                        gone: vec![s],
                        ..Change::default()
                    });
                } else if let [n] = touching(tracks, layer, e, &[r])[..] {
                    let f = far(&tracks[n], e);
                    if at_joint(f) && self.in_end(net, routed, f, e, li) {
                        // The next piece is the stub of the line's other end: one piece
                        // between the two centres.
                        changes.push(Change {
                            moved: vec![(r, p, f)],
                            gone: vec![s, n],
                            ..Change::default()
                        });
                    } else if let Some(v) = way(e, f)
                        && let Some(b) = meet(p, u, e, v)
                        && ahead(p, b, u) >= LEAST
                        && ahead(b, f, v) >= LEAST
                    {
                        // The run moved across to pass the centre, the bend along the next.
                        changes.push(Change {
                            moved: vec![(r, p, b), (n, b, f)],
                            gone: vec![s],
                            ..Change::default()
                        });
                    }
                }
                // Else the stub drawn on to meet the run just outside the pad, at a bend of 45
                // degrees or less.
                let foot = ahead(c, p, u);
                let q0 = Pt::new(c.x + u.0 * foot, c.y + u.1 * foot);
                let h = q0.distance(p);
                let length = ahead(q0, e, u);
                let mut tau = h;
                while tau < length - LEAST {
                    let c2 = Pt::new(q0.x + u.0 * tau, q0.y + u.1 * tau);
                    if !self.in_end(net, routed, p, c2, li) {
                        let c2 = Pt::new(
                            c2.x + u.0 * GENTLE_STEP / 2.0,
                            c2.y + u.1 * GENTLE_STEP / 2.0,
                        );
                        if ahead(c2, e, u) >= LEAST {
                            changes.push(Change {
                                moved: vec![(s, p, c2), (r, c2, e)],
                                ..Change::default()
                            });
                        }
                        break;
                    }
                    tau += GENTLE_STEP;
                }
                let Some(change) = changes
                    .into_iter()
                    .find(|c| c.clear(tracks, &self.stack, clear))
                else {
                    break;
                };
                routed.tracks = change.apply(tracks);
            }
        }
    }

    /// The short pieces of the line taken out (the module's header).
    pub(super) fn smooth(&self, routed: &mut Routed, clear: &Clear<'_>) {
        let joints = self.joints(routed);
        let at_joint = |p: Pt| joints.iter().any(|&j| same(j, p));
        let net = self.nets[routed.line];
        // The line's vias, which no change moves.
        let vias = Routed {
            line: routed.line,
            tracks: Vec::new(),
            vias: routed.vias.clone(),
        };
        // A bend the change moves into the copper at a joint would leave a track in it.
        let into_joint = |change: &Change, tracks: &[Track]| {
            let moved = change
                .moved
                .iter()
                .map(|&(k, from, to)| (from, to, tracks[k].layer));
            let added = change.added.iter().map(|t| (t.from, t.to, t.layer));
            moved.chain(added).any(|(from, to, layer)| {
                let layer = self.stack.index(layer).unwrap_or(FRONT);
                [from, to]
                    .into_iter()
                    .filter(|&q| !at_joint(q))
                    .any(|q| joints.iter().any(|&j| self.in_end(net, &vias, j, q, layer)))
            })
        };
        // Each change kept leaves fewer short pieces or fewer pieces, so this ends.
        loop {
            let tracks = &routed.tracks;
            let before = measure(tracks);
            let short = |k: usize| is_short(&tracks[k]);
            // The one other piece at a place of the line that is no joint, if there is one.
            let next = |k: usize, at: Pt| -> Option<usize> {
                if at_joint(at) {
                    return None;
                }
                match touching(tracks, tracks[k].layer, at, &[k])[..] {
                    [m] => Some(m),
                    _ => None,
                }
            };
            let found = (0..tracks.len()).filter(|&k| short(k)).find_map(|k| {
                let t = &tracks[k];
                // The short pieces from `k` on, up to [`RUN`], each way along the line.
                [(t.from, t.to), (t.to, t.from)]
                    .into_iter()
                    .find_map(|(p, q0)| {
                        let a = next(k, p)?;
                        let mut run = vec![k];
                        let mut q = q0;
                        loop {
                            let b = next(*run.last().expect("a piece"), q)?;
                            if let Some(change) =
                                (a != b && !run.contains(&b) && tracks[a].width == tracks[b].width)
                                    .then(|| self.join_across(tracks, &run, a, b, p, q))
                                    .flatten()
                                    .and_then(|changes| {
                                        changes.into_iter().find_map(|change| {
                                            let after = change.apply(tracks);
                                            (measure(&after) < before
                                                && !into_joint(&change, tracks)
                                                && change.clear(tracks, &self.stack, clear))
                                            .then_some(after)
                                        })
                                    })
                            {
                                return Some(change);
                            }
                            if run.len() == RUN || run.contains(&b) {
                                return None;
                            }
                            // A longer piece goes too only where it runs on in parallel with
                            // `a`, nearer its line than a short piece is long: a jog too small
                            // for a bend of 45 degrees, which the next piece takes up.
                            if !short(b) {
                                let a0 = far(&tracks[a], p);
                                let (Some(da), Some(db)) = (way(a0, p), way(q, far(&tracks[b], q)))
                                else {
                                    return None;
                                };
                                let across = (da.0 * (q.y - p.y) - da.1 * (q.x - p.x)).abs();
                                let parallel = (da.0 * db.1 - da.1 * db.0).abs() < 1e-9
                                    && da.0 * db.0 + da.1 * db.1 > 0.0;
                                if !parallel || across >= shortest(tracks[b].width) {
                                    return None;
                                }
                            }
                            q = far(&tracks[b], q);
                            run.push(b);
                        }
                    })
            });
            match found {
                Some(after) => routed.tracks = after,
                None => break,
            }
        }
    }

    /// The changes that take out the short pieces of `run`, from `p` to `q`, joining the piece
    /// `a` (at `p`) and `b` (at `q`) across them with every bend 45 degrees at most, the best
    /// first.
    fn join_across(
        &self,
        tracks: &[Track],
        run: &[usize],
        a: usize,
        b: usize,
        p: Pt,
        q: Pt,
    ) -> Option<Vec<Change>> {
        let (a0, b1) = (far(&tracks[a], p), far(&tracks[b], q));
        let (da, db) = (way(a0, p)?, way(q, b1)?);
        let dot = da.0 * db.0 + da.1 * db.1;
        let cross = da.0 * db.1 - da.1 * db.0;
        let piece = |from: Pt, to: Pt| Track {
            from,
            to,
            ..tracks[a].clone()
        };
        if cross.abs() < 1e-9 && dot > 0.0 {
            // In parallel: one piece where they are in line, else a bend of 45 degrees each way.
            let d = Pt::new(q.x - p.x, q.y - p.y);
            let along = d.x * da.0 + d.y * da.1;
            let across = da.0 * d.y - da.1 * d.x;
            if across.abs() < SAME {
                return Some(vec![Change {
                    moved: vec![(a, a0, b1)],
                    gone: [run, &[b]].concat(),
                    ..Change::default()
                }]);
            }
            let n = (-da.1, da.0);
            let x = Pt::new(
                p.x + (along - across.abs()) / 2.0 * da.0,
                p.y + (along - across.abs()) / 2.0 * da.1,
            );
            let y = Pt::new(
                x.x + across.abs() * da.0 + across * n.0,
                x.y + across.abs() * da.1 + across * n.1,
            );
            if ahead(a0, x, da) < LEAST || ahead(y, b1, da) < LEAST {
                return None;
            }
            return Some(vec![Change {
                moved: vec![(a, a0, x), (b, y, b1)],
                added: vec![piece(x, y)],
                gone: run.to_vec(),
            }]);
        }
        let c = meet(a0, da, b1, db)?;
        if dot >= std::f64::consts::FRAC_1_SQRT_2 - 1e-9 {
            // A bend of 45 degrees or less: the two runs drawn on to meet.
            if ahead(a0, c, da) < LEAST || ahead(c, b1, db) < LEAST {
                return None;
            }
            return Some(vec![Change {
                moved: vec![(a, a0, c), (b, c, b1)],
                gone: run.to_vec(),
                ..Change::default()
            }]);
        }
        if dot.abs() < 1e-9 {
            // A right angle: the two runs meet at a cut corner, the largest that fits.
            return Some(
                CHAMFERS
                    .iter()
                    .filter_map(|&cut| {
                        let x = Pt::new(c.x - da.0 * cut, c.y - da.1 * cut);
                        let y = Pt::new(c.x + db.0 * cut, c.y + db.1 * cut);
                        (ahead(a0, x, da) >= LEAST && ahead(y, b1, db) >= LEAST).then(|| Change {
                            moved: vec![(a, a0, x), (b, y, b1)],
                            added: vec![piece(x, y)],
                            gone: run.to_vec(),
                        })
                    })
                    .collect(),
            );
        }
        None
    }
}
