//! Shoving the unlocked copper of the routes file out of the way of a track being drawn (issue
//! 726, the third slice, after the walkaround of [`crate::walk`]).
//!
//! The question: a track `D` is drawn through some corners, on a layer, of a net. Which of the
//! copper of the routes file that may move (the caller's [`Piece`]s: its unlocked tracks and
//! vias) keeps the design rules check's spaces from `D` only if it moves, and where does it go?
//! [`Ground::shove`] answers with the pieces' new places, or says which piece cannot get out of
//! the way. The ground it is asked on holds every copper the caller does *not* offer to move
//! (pads, locked copper, zones, keepouts, the board edge, the copper of nets that are locked):
//! none of that moves, and the pieces keep their spaces from it.
//!
//! **One clearance model.** The spaces are those of [`netsmith_geom::Room`], as in
//! [`crate::walk`]: [`Room::fits_with`] says whether a piece of copper, in a place, keeps every
//! space from the ground and from the other pieces where they are now. The work is counted
//! ([`Shoved::checked`], at most [`MOST_CHECKED`]), never timed.
//!
//! **What a push is.** A piece that does not keep its spaces (round one: those near `D`) is pushed
//! once, away from what it is too near. A *track* is moved sideways, at right angles to itself,
//! as little as is needed (a step of [`STEP`] and then a bisection). An end of it that meets
//! another track of the net slides along that track, so that the track keeps its direction and
//! the corner keeps its angle (a track that meets it at less than 20 degrees is moved at right
//! angles instead, and turns by a little); every other piece that meets the end (a via, a
//! third track) follows. An end that is on a pad, on copper that does not move, on the body of
//! another track (a tee), or on a piece pushed before, stays: the track then turns about it. A
//! track with no end that can move is stuck. A *via* moves, with the track ends that meet it,
//! straight away from what it is too near.
//!
//! **A cascade.** A piece pushed into the space of another makes that one a piece that does not
//! keep its spaces, and it is pushed in its turn (a piece is pushed once: it then stays where it
//! is, and the pieces it meets give way to it). At most [`MOST_PIECES`] pieces are pushed, each
//! no further than [`MOST_PUSH`] mm from where it was.
//!
//! **What is kept.** The width of every track; the angle of every corner, as the check's
//! `track_angle` asks (two tracks of a net that meet leave each other at 134 degrees or more, three
//! at 89 degrees or more; a corner that was sharper before is no sharper); the ends joined (they
//! move together or not at all); no track turned back on itself or made no length. A push that
//! cannot keep all of this is not made, and the piece is stuck. The check of the whole board when
//! the edit is made has the last word, as it has for a drawn track.
//!
//! **Not done:** a piece is never pushed across the board edge or a pad (it is stuck instead);
//! arcs and the bodies of tees never move; a piece that already broke a space before the track
//! was there is left alone; there is no layer change.

use std::collections::HashMap;

use netsmith_geom::{Bounds, DrcShape, Room, TrackShape, shape_gap};
use netsmith_pcb::{CopperLayer, Pt, Track, Via};

use crate::walk::{Ground, angle_between, fpt, segment};

/// How far a piece may be moved from where it was, in mm.
pub const MOST_PUSH: f64 = 2.0;
/// The first try of a push and the step of the search for the least one that clears, in mm.
pub const STEP: f64 = 0.05;
/// The most pieces a shove pushes.
pub const MOST_PIECES: usize = 12;
/// The most questions to the room ([`Room::fits_with`]) a shove asks before it gives up.
pub const MOST_CHECKED: usize = 60_000;
/// How far round a piece, in mm, other copper can matter to it: more than any space of the rules.
const NEAR: f64 = 2.0;
/// The sine of the least angle at which an end slides along the track it meets: less, it moves at
/// right angles instead.
const MIN_SLIDE: f64 = 0.34;
/// Two places that are one, in mm.
const SAME: f64 = 5e-4;
/// How near a track's body a joint must be to be on it, in mm.
const ON: f64 = 1e-3;

/// A piece of copper of the routes file that may be moved: the caller's `id` comes back in the
/// answer.
#[derive(Debug, Clone)]
pub struct Piece {
    pub id: usize,
    pub what: What,
}

/// A track (an arc is never moved) or a via.
#[derive(Debug, Clone)]
pub enum What {
    Track(Track),
    Via(Via),
}

/// The track being drawn: its net, layer and width, and its corners from the first to the last.
#[derive(Debug, Clone, Copy)]
pub struct Drawn<'a> {
    pub net: &'a str,
    pub layer: CopperLayer,
    pub width: f64,
    pub corners: &'a [Pt],
}

/// A piece in its new place.
#[derive(Debug, Clone, PartialEq)]
pub enum Moved {
    Track { id: usize, from: Pt, to: Pt },
    Via { id: usize, at: Pt },
}

impl Moved {
    pub fn id(&self) -> usize {
        match self {
            Moved::Track { id, .. } | Moved::Via { id, .. } => *id,
        }
    }
}

/// What a shove did: the pieces that moved (and only those) and the work it took.
#[derive(Debug, Clone, PartialEq)]
pub struct Shoved {
    pub moved: Vec<Moved>,
    /// How many pieces were pushed (more can have moved: the ends that follow).
    pub pushed: usize,
    /// The questions asked of the room.
    pub checked: usize,
}

/// Why no shove clears the track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stuck {
    /// This piece (`None`: copper that is not a piece, or the layer is not the board's) does not keep
    /// its spaces and cannot be moved to where it does.
    Blocked { by: Option<usize>, checked: usize },
    /// The cascade went past [`MOST_PIECES`] pieces or [`MOST_CHECKED`] questions.
    TooMany { checked: usize },
}

impl Stuck {
    /// The piece that blocks, if one does.
    pub fn by(&self) -> Option<usize> {
        match self {
            Stuck::Blocked { by, .. } => *by,
            Stuck::TooMany { .. } => None,
        }
    }

    /// The work done.
    pub fn checked(&self) -> usize {
        match self {
            Stuck::Blocked { checked, .. } | Stuck::TooMany { checked } => *checked,
        }
    }
}

#[derive(Clone)]
struct Item {
    id: usize,
    net: String,
    layer: CopperLayer,
    layer_ix: usize,
    width: f64,
    is_via: bool,
    a: Pt,
    b: Pt,
    orig: (Pt, Pt),
    /// An arc, never moved.
    arc: Option<Track>,
    pinned: bool,
    shape: DrcShape,
}

impl Item {
    fn layers(&self, all: usize) -> std::ops::Range<usize> {
        if self.is_via {
            0..all
        } else {
            self.layer_ix..self.layer_ix + 1
        }
    }

    fn reshape(&mut self) {
        self.shape = if self.is_via {
            DrcShape::disc(fpt(self.a), self.width / 2.0)
        } else if let Some(t) = &self.arc {
            DrcShape::stroke(
                t.centre_line().into_iter().map(fpt).collect(),
                self.width / 2.0,
            )
        } else {
            segment(self.a, self.b, self.width)
        };
    }

    fn end(&self, e: End) -> Pt {
        match e {
            End::A => self.a,
            End::B => self.b,
        }
    }

    fn set_end(&mut self, e: End, p: Pt) {
        match e {
            End::A => self.a = p,
            End::B => self.b = p,
        }
        if self.is_via {
            self.b = self.a;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum End {
    A,
    B,
}

struct Joint {
    at: Pt,
    net: String,
    members: Vec<(usize, End)>,
    /// On a pad, on copper that does not move, or on the body of another track: it stays.
    anchored: bool,
    /// On a pad or a via: no corner for the check's `track_angle`.
    excused: bool,
    /// The way the copper that does not move leaves it, with the layer's name.
    fixed: Vec<(String, Pt)>,
    /// The sharpest angle of the corner at the start, in degrees.
    first: f64,
}

fn sub(a: Pt, b: Pt) -> Pt {
    Pt::new(a.x - b.x, a.y - b.y)
}

fn add(a: Pt, b: Pt) -> Pt {
    Pt::new(a.x + b.x, a.y + b.y)
}

fn scale(a: Pt, k: f64) -> Pt {
    Pt::new(a.x * k, a.y * k)
}

fn dot(a: Pt, b: Pt) -> f64 {
    a.x * b.x + a.y * b.y
}

fn norm(a: Pt) -> f64 {
    a.x.hypot(a.y)
}

fn unit(a: Pt) -> Pt {
    let n = norm(a).max(1e-18);
    Pt::new(a.x / n, a.y / n)
}

fn round6(p: Pt) -> Pt {
    let r = |v: f64| (v * 1e6).round() / 1e6;
    Pt::new(r(p.x), r(p.y))
}

/// How far `p` is from the segment `a` to `b`.
fn to_segment(p: Pt, a: Pt, b: Pt) -> f64 {
    let ab = sub(b, a);
    let len2 = dot(ab, ab);
    let t = if len2 < 1e-18 {
        0.0
    } else {
        (dot(sub(p, a), ab) / len2).clamp(0.0, 1.0)
    };
    norm(sub(p, add(a, scale(ab, t))))
}

fn key(p: Pt) -> (i64, i64) {
    ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64)
}

fn bounds_near(b: &Bounds, d: f64) -> Bounds {
    b.expanded(d)
}

struct Search<'a> {
    room: Room<'a>,
    layers: usize,
    items: Vec<Item>,
    joints: Vec<Joint>,
    /// The joint of each end of each item.
    at: Vec<[usize; 2]>,
    settled: Vec<bool>,
    /// What the drawn track is made of: its net, layer and the shape of each segment.
    drawn: (String, usize, Vec<DrcShape>),
    drawn_start: Pt,
    /// Items that broke a space before the track was there: left alone while they stay.
    broken: Vec<bool>,
    checked: usize,
}

/// Which copper an item is asked to keep its spaces from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// The ground, the drawn track and every other piece.
    All,
    /// What a piece being pushed may not move into: the ground, the drawn track, and the pieces
    /// that stay (pinned) or were pushed already. The pieces still to be pushed give way to it.
    Pusher,
    /// The ground and the pieces that stay or were pushed: what a piece may not move through.
    Hard,
}

impl<'a> Search<'a> {
    fn fits(
        &mut self,
        shape: &DrcShape,
        net: &str,
        layers: std::ops::Range<usize>,
        skip: usize,
        scope: Scope,
    ) -> bool {
        self.checked += 1;
        let near = bounds_near(&shape.bounds, NEAR);
        let mut extra: Vec<(&str, usize, &DrcShape)> = Vec::new();
        for (k, it) in self.items.iter().enumerate() {
            let counts = scope == Scope::All || self.settled[k] || it.pinned;
            if k == skip || !counts || !it.shape.bounds.overlaps(&near) {
                continue;
            }
            for l in it.layers(self.layers) {
                extra.push((it.net.as_str(), l, &it.shape));
            }
        }
        for s in &self.drawn.2 {
            if scope != Scope::Hard && s.bounds.overlaps(&near) {
                extra.push((self.drawn.0.as_str(), self.drawn.1, s));
            }
        }
        layers
            .clone()
            .all(|l| self.room.fits_with(shape, net, l, false, &extra))
    }

    fn item_fits(&mut self, i: usize, scope: Scope) -> bool {
        let (shape, net, layers) = {
            let it = &self.items[i];
            (it.shape.clone(), it.net.clone(), it.layers(self.layers))
        };
        self.fits(&shape, &net, layers, i, scope)
    }

    /// The joint is free to move: nothing on it stays or has been pushed.
    fn joint_free(&self, j: usize) -> bool {
        let joint = &self.joints[j];
        !joint.anchored
            && joint
                .members
                .iter()
                .all(|&(k, _)| !self.items[k].pinned && !self.settled[k])
    }

    /// The direction a track leaves a joint by, from its current place.
    fn leaves(&self, k: usize, e: End) -> Pt {
        let it = &self.items[k];
        match e {
            End::A => sub(it.b, it.a),
            End::B => sub(it.a, it.b),
        }
    }

    /// The sharpest corner at a joint now: its angle and the least the check allows, or `None`
    /// where there is no corner for the check.
    fn corner(&self, j: usize) -> Option<(f64, f64)> {
        let joint = &self.joints[j];
        if joint.excused {
            return None;
        }
        let mut by_layer: HashMap<&str, Vec<Pt>> = HashMap::new();
        for &(k, e) in &joint.members {
            let it = &self.items[k];
            if it.is_via {
                return None;
            }
            let d = self.leaves(k, e);
            if norm(d) > 1e-6 {
                by_layer.entry(it.layer.name()).or_default().push(d);
            }
        }
        for (layer, d) in &joint.fixed {
            by_layer.entry(layer.as_str()).or_default().push(*d);
        }
        let mut worst: Option<(f64, f64)> = None;
        for dirs in by_layer.values() {
            let mut dirs: Vec<Pt> = dirs.iter().map(|&d| unit(d)).collect();
            dirs.sort_by(|p, q| p.x.total_cmp(&q.x).then(p.y.total_cmp(&q.y)));
            dirs.dedup_by(|p, q| norm(sub(*p, *q)) < 1e-4);
            if dirs.len() < 2 {
                continue;
            }
            let mut least = 180.0f64;
            for a in 0..dirs.len() {
                for b in a + 1..dirs.len() {
                    least = least.min(angle_between(dirs[a], dirs[b]));
                }
            }
            let limit = if dirs.len() == 2 { 134.0 } else { 89.0 };
            if worst.is_none_or(|(l, m)| least - limit < l - m) {
                worst = Some((least, limit));
            }
        }
        worst
    }

    fn corner_ok(&self, j: usize) -> bool {
        match self.corner(j) {
            None => true,
            Some((least, limit)) => least >= limit || least >= self.joints[j].first - 1e-6,
        }
    }

    /// Where the free end at joint `j` of item `i`, at `p`, goes for a push of `d` along `n`.
    fn target(&self, j: usize, i: usize, p: Pt, n: Pt, d: f64) -> Pt {
        let others: Vec<(usize, End)> = self.joints[j]
            .members
            .iter()
            .copied()
            .filter(|(k, _)| *k != i)
            .collect();
        if let [(k, e)] = others[..] {
            let other = &self.items[k];
            if !other.is_via && other.layer == self.items[i].layer {
                let u = unit(self.leaves(k, e));
                let along = dot(u, n);
                if along.abs() >= MIN_SLIDE {
                    return add(p, scale(u, d / along));
                }
            }
        }
        add(p, scale(n, d))
    }

    /// Move joint `j` to `to`, every piece that meets there following. Returns what each was.
    fn drag(&mut self, j: usize, to: Pt, saved: &mut Vec<(usize, Item)>) {
        self.joints[j].at = to;
        let members = self.joints[j].members.clone();
        for (k, e) in members {
            if !saved.iter().any(|(s, _)| *s == k) {
                saved.push((k, self.items[k].clone()));
            }
            self.items[k].set_end(e, to);
            self.items[k].reshape();
        }
    }

    /// Whether the pieces that changed (`saved`) are as they must be: no further than
    /// [`MOST_PUSH`] from where they were, no track made no length or turned back, every corner
    /// they are in as the check allows.
    fn sound(&self, saved: &[(usize, Item)]) -> bool {
        for (k, was) in saved {
            let it = &self.items[*k];
            if norm(sub(it.a, it.orig.0)) > MOST_PUSH + 1e-9
                || norm(sub(it.b, it.orig.1)) > MOST_PUSH + 1e-9
            {
                return false;
            }
            if !it.is_via {
                let (now, then) = (sub(it.b, it.a), sub(was.b, was.a));
                if norm(now) < 0.01 || dot(unit(now), unit(then)) < 0.2 {
                    return false;
                }
            }
            for e in [End::A, End::B] {
                let j = self.at[*k][e as usize];
                if !self.corner_ok(j) {
                    return false;
                }
            }
        }
        true
    }

    /// The nearest thing the item `i` keeps too little space from, if it is a piece or the drawn
    /// track: the point of the item that is nearest, the point of the other, whether they touch,
    /// and where the other begins.
    fn offender(&self, i: usize) -> Option<(Pt, Pt, bool, Pt)> {
        let it = &self.items[i];
        let to_pt = |p: netsmith_geom::Pt| Pt::new(p.x, p.y);
        let mut worst: Option<(f64, (Pt, Pt, bool, Pt))> = None;
        let mut consider = |shape: &DrcShape, net: &str, layers: &[usize], start: Pt| {
            if net == it.net || !it.layers(self.layers).any(|l| layers.contains(&l)) {
                return;
            }
            let need = self.room.clearance(&it.net).max(self.room.clearance(net));
            let g = shape_gap(&it.shape, shape);
            if g.d < need + 1e-3 {
                let slack = g.d - need;
                if worst.as_ref().is_none_or(|(w, _)| slack < *w) {
                    worst = Some((slack, (to_pt(g.a), to_pt(g.b), g.d <= 1e-9, start)));
                }
            }
        };
        for (k, o) in self.items.iter().enumerate() {
            if k != i
                && o.shape
                    .bounds
                    .overlaps(&bounds_near(&it.shape.bounds, NEAR))
            {
                let layers: Vec<usize> = o.layers(self.layers).collect();
                consider(&o.shape, &o.net, &layers, to_pt(o.shape.pts[0]));
            }
        }
        for s in &self.drawn.2 {
            consider(s, &self.drawn.0, &[self.drawn.1], self.drawn_start);
        }
        worst.map(|(_, w)| w)
    }

    /// The directions to try pushing item `i` in, away from what it is too near: for a track the
    /// two sides of it (the one away first, alone when it is known), for a via the one way.
    fn ways(&self, i: usize) -> Vec<Pt> {
        let it = &self.items[i];
        let found = self.offender(i);
        if it.is_via {
            let Some((_, theirs, touching, start)) = found else {
                return Vec::new();
            };
            let from = if touching { start } else { theirs };
            let away = sub(it.a, from);
            return if norm(away) < 1e-9 {
                Vec::new()
            } else {
                vec![unit(away)]
            };
        }
        let u = unit(sub(it.b, it.a));
        let n = Pt::new(-u.y, u.x);
        let middle = scale(add(it.a, it.b), 0.5);
        if let Some((mine, theirs, touching, start)) = found {
            // Away from the other: from its nearest point, or from where it begins when they cross.
            let away = if touching {
                sub(middle, start)
            } else {
                sub(mine, theirs)
            };
            let side = dot(n, away);
            if side.abs() > 1e-9 {
                return vec![scale(n, side.signum())];
            }
        }
        vec![n, scale(n, -1.0)]
    }

    /// Push item `i` by `d` along `n`: the pieces are then where that puts them, and what to put
    /// back is returned with how it went, unless it did not clear or was unsound (nothing is
    /// changed then). With `guard` the push is also refused where it would be through copper that
    /// stays, which the piece could not have been moved through.
    fn push(&mut self, i: usize, n: Pt, d: f64, guard: bool) -> Pushed {
        let mut saved: Vec<(usize, Item)> = Vec::new();
        let mut was_at: Vec<(usize, Pt)> = Vec::new();
        let is_via = self.items[i].is_via;
        let ends: &[End] = if is_via { &[End::A] } else { &[End::A, End::B] };
        for &e in ends {
            let j = self.at[i][e as usize];
            if !self.joint_free(j) {
                continue;
            }
            let p = self.items[i].end(e);
            let to = if is_via {
                add(p, scale(n, d))
            } else {
                self.target(j, i, p, n, d)
            };
            was_at.push((j, self.joints[j].at));
            self.drag(j, to, &mut saved);
        }
        if saved.is_empty() {
            return Pushed::Blocked;
        }
        let undo = Undo { saved, was_at };
        if !self.sound(&undo.saved) {
            self.undo(undo);
            return Pushed::Missed;
        }
        if guard && !self.item_fits(i, Scope::Hard) {
            self.undo(undo);
            return Pushed::Blocked;
        }
        if self.item_fits(i, Scope::Pusher) {
            Pushed::Fits(undo)
        } else {
            self.undo(undo);
            Pushed::Missed
        }
    }

    fn undo(&mut self, undo: Undo) {
        for (k, it) in undo.saved {
            self.items[k] = it;
        }
        for (j, at) in undo.was_at {
            self.joints[j].at = at;
        }
    }

    /// Push item `i` as little as clears: the least multiple of [`STEP`] along the way (the
    /// ways) that does, then a bisection between that and the one before. A way ends where the
    /// piece would go through copper that stays. Whether it was pushed.
    fn clear(&mut self, i: usize) -> bool {
        let ways = self.ways(i);
        let mut open = vec![true; ways.len()];
        // A piece already in the way of copper that stays is not stopped by it.
        let guard = self.item_fits(i, Scope::Hard);
        let steps = (MOST_PUSH / STEP).round() as usize;
        for k in 1..=steps {
            for (w, n) in ways.iter().enumerate() {
                if self.checked > MOST_CHECKED {
                    return false;
                }
                if !open[w] {
                    continue;
                }
                let d = k as f64 * STEP;
                match self.push(i, *n, d, guard) {
                    Pushed::Missed => {}
                    Pushed::Blocked => open[w] = false,
                    Pushed::Fits(undo) => {
                        self.undo(undo);
                        let (mut low, mut high) = ((k - 1) as f64 * STEP, d);
                        for _ in 0..6 {
                            let mid = (low + high) / 2.0;
                            match self.push(i, *n, mid, false) {
                                Pushed::Fits(undo) => {
                                    self.undo(undo);
                                    high = mid;
                                }
                                _ => low = mid,
                            }
                        }
                        return matches!(self.push(i, *n, high, false), Pushed::Fits(_));
                    }
                }
            }
            if !open.iter().any(|&o| o) {
                return false;
            }
        }
        false
    }
}

enum Pushed {
    /// It cleared; this puts it back.
    Fits(Undo),
    /// It did not clear.
    Missed,
    /// The piece cannot go this way: nothing of it can move, or it would pass through copper that
    /// stays.
    Blocked,
}

/// What a push changed, to put back.
struct Undo {
    saved: Vec<(usize, Item)>,
    was_at: Vec<(usize, Pt)>,
}

/// Where item `a` and `b` of a segment are: the part of the track's own shape.
fn item_of(p: &Piece, layers: &dyn Fn(CopperLayer) -> Option<usize>) -> Option<Item> {
    match &p.what {
        What::Track(t) => {
            let layer_ix = layers(t.layer)?;
            let mut it = Item {
                id: p.id,
                net: t.net.clone(),
                layer: t.layer,
                layer_ix,
                width: t.width,
                is_via: false,
                a: t.from,
                b: t.to,
                orig: (t.from, t.to),
                arc: t.mid.is_some().then(|| t.clone()),
                pinned: t.mid.is_some(),
                shape: DrcShape::disc(fpt(t.from), 0.0),
            };
            it.reshape();
            Some(it)
        }
        What::Via(v) => {
            let mut it = Item {
                id: p.id,
                net: v.net.clone(),
                layer: CopperLayer::Front,
                layer_ix: 0,
                width: v.diameter,
                is_via: true,
                a: v.at,
                b: v.at,
                orig: (v.at, v.at),
                arc: None,
                pinned: false,
                shape: DrcShape::disc(fpt(v.at), 0.0),
            };
            it.reshape();
            Some(it)
        }
    }
}

impl Ground {
    /// Move the unlocked copper `pieces` out of the way of `drawn`, or say which of it cannot
    /// move. `self` must hold everything that does not move, and not the pieces. See the
    /// module's header for what a push is and keeps. Writes nothing; the pieces that stay where
    /// they are are not in the answer.
    pub fn shove(&self, drawn: &Drawn<'_>, pieces: &[Piece]) -> Result<Shoved, Stuck> {
        let room = Room::new(&self.art, &self.rules, &self.extras);
        let Some(drawn_ix) = room.layer(drawn.layer.name()) else {
            return Err(Stuck::Blocked {
                by: None,
                checked: 0,
            });
        };
        let layers = room.layers();
        let index = |l: CopperLayer| room.layer(l.name());
        let mut items: Vec<Item> = pieces.iter().filter_map(|p| item_of(p, &index)).collect();
        let shapes: Vec<DrcShape> = match drawn.corners {
            [] => Vec::new(),
            [one] => vec![segment(*one, *one, drawn.width)],
            many => many
                .windows(2)
                .map(|w| segment(w[0], w[1], drawn.width))
                .collect(),
        };
        let start = drawn.corners.first().copied().unwrap_or_default();
        // Tees: a track whose body has the end of another track of the net on it stays where it is.
        self.pin_tees(&mut items);
        let (joints, at) = self.joints_of(&items);
        let count = items.len();
        let mut s = Search {
            room,
            layers,
            items,
            joints,
            at,
            settled: vec![false; count],
            drawn: (drawn.net.to_string(), drawn_ix, shapes),
            drawn_start: start,
            broken: vec![false; count],
            checked: 0,
        };
        for j in 0..s.joints.len() {
            s.joints[j].first = s.corner(j).map_or(180.0, |(least, _)| least);
        }
        // What broke a space before the track was drawn is left alone.
        let drawn_shapes = std::mem::take(&mut s.drawn.2);
        for i in 0..count {
            let b = s.items[i].shape.bounds;
            if drawn_shapes
                .iter()
                .any(|d| d.bounds.overlaps(&bounds_near(&b, NEAR)))
            {
                s.broken[i] = !s.item_fits(i, Scope::All);
            }
        }
        s.drawn.2 = drawn_shapes;
        let mut dirty: Vec<usize> = Vec::new();
        let mut pushed = 0usize;
        loop {
            // The items near the track, or near what moved last, that do not keep their spaces.
            let mut sources: Vec<Bounds> = s.drawn.2.iter().map(|d| d.bounds).collect();
            sources.extend(dirty.iter().map(|&k| s.items[k].shape.bounds));
            let mut candidates: Vec<usize> = (0..count)
                .filter(|&k| {
                    let b = s.items[k].shape.bounds;
                    sources
                        .iter()
                        .any(|src| bounds_near(src, NEAR).overlaps(&b))
                        || dirty.contains(&k)
                })
                .collect();
            candidates.sort_unstable();
            let mut next: Option<usize> = None;
            let mut stuck: Option<usize> = None;
            for k in candidates {
                if s.checked > MOST_CHECKED {
                    return Err(Stuck::TooMany { checked: s.checked });
                }
                let moved = s.items[k].a != s.items[k].orig.0 || s.items[k].b != s.items[k].orig.1;
                if s.broken[k] && !moved {
                    continue;
                }
                if s.item_fits(k, Scope::All) {
                    continue;
                }
                if s.settled[k] || s.items[k].pinned {
                    stuck.get_or_insert(k);
                } else if next.is_none() {
                    next = Some(k);
                }
            }
            let Some(k) = next else {
                if let Some(k) = stuck {
                    return Err(Stuck::Blocked {
                        by: Some(s.items[k].id),
                        checked: s.checked,
                    });
                }
                break;
            };
            if pushed >= MOST_PIECES {
                return Err(Stuck::TooMany { checked: s.checked });
            }
            let before: Vec<(Pt, Pt)> = s.items.iter().map(|it| (it.a, it.b)).collect();
            if !s.clear(k) {
                if s.checked > MOST_CHECKED {
                    return Err(Stuck::TooMany { checked: s.checked });
                }
                return Err(Stuck::Blocked {
                    by: Some(s.items[k].id),
                    checked: s.checked,
                });
            }
            s.settled[k] = true;
            pushed += 1;
            dirty = (0..count)
                .filter(|&m| (s.items[m].a, s.items[m].b) != before[m])
                .collect();
        }
        let moved = s
            .items
            .iter()
            .filter(|it| it.a != it.orig.0 || it.b != it.orig.1)
            .map(|it| {
                if it.is_via {
                    Moved::Via {
                        id: it.id,
                        at: round6(it.a),
                    }
                } else {
                    Moved::Track {
                        id: it.id,
                        from: round6(it.a),
                        to: round6(it.b),
                    }
                }
            })
            .collect();
        Ok(Shoved {
            moved,
            pushed,
            checked: s.checked,
        })
    }

    /// Pin the tracks that have the end of another track of their net on their body.
    fn pin_tees(&self, items: &mut [Item]) {
        let mut ends: Vec<(String, Pt, usize)> = Vec::new();
        for (k, it) in items.iter().enumerate() {
            ends.push((it.net.clone(), it.a, k));
            if !it.is_via {
                ends.push((it.net.clone(), it.b, k));
            }
        }
        let mut pin: Vec<usize> = Vec::new();
        for (k, it) in items.iter().enumerate() {
            if it.is_via || it.arc.is_some() {
                continue;
            }
            let on = ends.iter().any(|(net, p, owner)| {
                *owner != k
                    && *net == it.net
                    && norm(sub(*p, it.a)) > ON
                    && norm(sub(*p, it.b)) > ON
                    && to_segment(*p, it.a, it.b) <= ON
            });
            if on {
                pin.push(k);
            }
        }
        for k in pin {
            items[k].pinned = true;
        }
    }

    /// The joints of the pieces: where their ends meet, by net, and what holds each in place.
    fn joints_of(&self, items: &[Item]) -> (Vec<Joint>, Vec<[usize; 2]>) {
        let mut joints: Vec<Joint> = Vec::new();
        let mut index: HashMap<(String, (i64, i64)), usize> = HashMap::new();
        let mut at = vec![[0usize; 2]; items.len()];
        for (k, it) in items.iter().enumerate() {
            for e in [End::A, End::B] {
                if it.is_via && e == End::B {
                    at[k][1] = at[k][0];
                    continue;
                }
                let p = it.end(e);
                let j = *index.entry((it.net.clone(), key(p))).or_insert_with(|| {
                    joints.push(Joint {
                        at: p,
                        net: it.net.clone(),
                        members: Vec::new(),
                        anchored: false,
                        excused: false,
                        fixed: Vec::new(),
                        first: 180.0,
                    });
                    joints.len() - 1
                });
                joints[j].members.push((k, e));
                at[k][e as usize] = j;
            }
        }
        for j in 0..joints.len() {
            let (net, p) = (joints[j].net.clone(), joints[j].at);
            let mut anchored = false;
            let mut excused = false;
            let mut fixed: Vec<(String, Pt)> = Vec::new();
            for pad in self
                .art
                .pads
                .iter()
                .filter(|pad| pad.net.as_deref() == Some(net.as_str()))
            {
                let reach = pad.size.0.hypot(pad.size.1) / 2.0 + 1e-6;
                if norm(sub(p, Pt::new(pad.at.x, pad.at.y))) <= reach {
                    anchored = true;
                    excused = true;
                }
            }
            for via in self
                .art
                .vias
                .iter()
                .filter(|v| v.net.as_deref() == Some(net.as_str()))
            {
                if norm(sub(p, Pt::new(via.at.x, via.at.y))) <= via.diameter / 2.0 + 1e-6 {
                    anchored = true;
                    excused = true;
                }
            }
            for t in self
                .art
                .tracks
                .iter()
                .filter(|t| t.net.as_deref() == Some(net.as_str()))
            {
                let (a, b, first, last) = match &t.shape {
                    TrackShape::Line(a, b) => {
                        let (a, b) = (Pt::new(a.x, a.y), Pt::new(b.x, b.y));
                        (a, b, sub(b, a), sub(a, b))
                    }
                    TrackShape::Arc(arc) => {
                        let (f, l) = arc.tangents();
                        (
                            Pt::new(arc.start.x, arc.start.y),
                            Pt::new(arc.end.x, arc.end.y),
                            Pt::new(f.x, f.y),
                            Pt::new(l.x, l.y),
                        )
                    }
                };
                if norm(sub(p, a)) <= ON {
                    anchored = true;
                    fixed.push((t.layer.clone(), first));
                } else if norm(sub(p, b)) <= ON {
                    anchored = true;
                    fixed.push((t.layer.clone(), last));
                } else if matches!(t.shape, TrackShape::Line(..)) && to_segment(p, a, b) <= ON {
                    anchored = true;
                }
            }
            // A piece that stays holds the joints at its ends, or has the joint on its body.
            for it in items.iter().filter(|it| it.pinned && it.net == net) {
                let held = [it.a, it.b].iter().any(|q| norm(sub(p, *q)) <= SAME)
                    || (!it.is_via && it.arc.is_none() && to_segment(p, it.a, it.b) <= ON);
                if held {
                    anchored = true;
                }
            }
            let joint = &mut joints[j];
            joint.anchored = anchored;
            joint.excused = excused;
            joint.fixed = fixed;
        }
        // A joint on the body of another piece of its net (a tee) stays too.
        for j in 0..joints.len() {
            let (net, p) = (joints[j].net.clone(), joints[j].at);
            let on_body = items.iter().enumerate().any(|(k, it)| {
                !it.is_via
                    && it.arc.is_none()
                    && it.net == net
                    && !joints[j].members.iter().any(|&(m, _)| m == k)
                    && to_segment(p, it.a, it.b) <= ON
            });
            if on_body {
                joints[j].anchored = true;
            }
        }
        (joints, at)
    }
}
