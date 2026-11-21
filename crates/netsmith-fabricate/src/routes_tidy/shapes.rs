//! The passes of `routes tidy` that find their defects in the shapes of the copper, where the
//! design rules check finds nothing (issue #1485): a routed board passes the check with all of
//! them and still carries copper a person removes by hand.
//!
//! - **tracks inside a pad**: a straight track from the centre of a pad or a via of its net to
//!   a point still inside that pad or via is removed: the copper there is the pad's or the
//!   via's already. A pad is held to the circle of its shorter side round its centre, which
//!   lies inside every shape of pad however it is turned; a via to its round copper;
//! - **duplicate ways**: the copper of a net is taken as ways, each a run of tracks, and the
//!   vias it changes layer by, between two stops: a pad, or a place where other than two tracks
//!   and vias meet. Two ways between the same two stops join what one of them joins, and no pad
//!   is on the middle of either, so the longer is removed (of two as long, the later). A net
//!   with a track that ends on the middle of another (a tee) is left as it is, since its ways
//!   are not seen at their ends alone;
//! - **overlapping copper**: a straight track whose copper lies wholly inside the copper of
//!   another straight track of its net and layer is removed, and two straight tracks of one
//!   width on one line that overlap along it become one, from end to end;
//! - **runs of tiny tracks**: [`TINY_RUN`] or more straight tracks in a row, each shorter than
//!   [`TINY`] of its widths, of one net, layer, width and owner, joined end to end where nothing
//!   else meets them (no pad, via or third track), become one track from the run's first end to
//!   its last; when that is refused, two that meet where the lines of the tracks on either side
//!   of the run cross (one corner). A form that would bend by more than 45 degrees at an end of
//!   the run or at its corner is not tried, so a chamfer made of two short tracks is left.
//!
//! Each is a change as the others are: made only when the check of the board with it finds
//! nothing new and the connections left to route are the same (the rule of every change, in
//! the header of [`super`]).

use std::collections::BTreeMap;

use netsmith_pcb::{CopperLayer, Pt, Rect, Track};

use super::{Candidate, Key, Kind, NEAR, Work, dot, length, segment_distance, unit};

/// A track shorter than this many of its widths is tiny.
pub const TINY: f64 = 2.0;

/// The fewest tiny tracks in a row that make a run.
pub const TINY_RUN: usize = 2;

/// The most a form of a run may bend at a joint, in degrees: the check's `track_angle` allows a
/// bend of 45 degrees, to the degree.
const BEND: f64 = 45.5;

/// A pad of a net, as these passes see it.
#[derive(Debug, Clone)]
pub(super) struct Spot {
    centre: Pt,
    /// Half the pad's shorter side: the circle round the centre that is copper.
    inner: f64,
    bounds: Rect,
    front: bool,
    back: bool,
}

impl Spot {
    fn on(&self, layer: CopperLayer) -> bool {
        match layer {
            CopperLayer::Front => self.front,
            CopperLayer::Back => self.back,
            CopperLayer::Inner(_) => self.front && self.back,
        }
    }
}

/// The pads of each net of `board`.
pub(super) fn spots(board: &netsmith_pcb::Board) -> BTreeMap<String, Vec<Spot>> {
    let mut out: BTreeMap<String, Vec<Spot>> = BTreeMap::new();
    for net in &board.nets {
        for r in &net.pads {
            let Some(part) = board.parts.iter().find(|p| p.refdes == r.refdes) else {
                continue;
            };
            for pad in part.footprint.pads_numbered(&r.number) {
                let (front, back) = part.pad_layers(pad);
                out.entry(net.name.clone()).or_default().push(Spot {
                    centre: part.pad_centre(pad),
                    inner: pad.width.min(pad.height) / 2.0,
                    bounds: part.pad_bounds(pad),
                    front,
                    back,
                });
            }
        }
    }
    out
}

/// A place of the copper by net, layer and micrometre.
type Place = (String, &'static str, (i64, i64));

fn q(p: Pt) -> (i64, i64) {
    ((p.x * 1e3).round() as i64, (p.y * 1e3).round() as i64)
}

/// The bend between going along `a` and then along `b`, in degrees.
fn turn(a: Pt, b: Pt) -> f64 {
    let (u, v) = (unit(a), unit(b));
    dot(u, v).clamp(-1.0, 1.0).acos().to_degrees()
}

impl Work<'_> {
    fn spots_of(&self, net: &str) -> &[Spot] {
        self.spots.get(net).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The live tracks by the places where they end.
    fn ends(&self) -> BTreeMap<Place, Vec<usize>> {
        let mut ends: BTreeMap<Place, Vec<usize>> = BTreeMap::new();
        for (i, w) in self.copper.wires.iter().enumerate() {
            if !w.alive {
                continue;
            }
            for p in [w.track.from, w.track.to] {
                ends.entry((w.track.net.clone(), w.track.layer.word(), q(p)))
                    .or_default()
                    .push(i);
            }
        }
        ends
    }

    /// Whether a live via of `net` stands at `p`.
    fn via_at(&self, net: &str, p: Pt) -> Option<usize> {
        (0..self.copper.holes.len()).find(|&i| {
            let h = &self.copper.holes[i];
            h.alive && h.via.net == net && h.via.at.distance(p) < NEAR
        })
    }

    /// A track of `net` on `layer` other than `but` whose middle (not an end) `p` is on.
    fn tee_at(&self, net: &str, layer: CopperLayer, p: Pt, but: &[usize]) -> bool {
        self.copper.wires.iter().enumerate().any(|(j, o)| {
            o.alive
                && !but.contains(&j)
                && o.track.net == net
                && o.track.layer == layer
                && o.track.from.distance(p) >= NEAR
                && o.track.to.distance(p) >= NEAR
                && segment_distance(p, o.track.from, o.track.to) < NEAR
        })
    }

    fn removal(&self, kind: Kind, i: usize, said: String, holes: Vec<usize>) -> Candidate {
        let w = &self.copper.wires[i];
        let t = &w.track;
        let at = Pt::new((t.from.x + t.to.x) / 2.0, (t.from.y + t.to.y) / 2.0);
        Candidate {
            key: self.key(kind, &t.net, Some(t.layer), at),
            change: self.change(kind, &t.net, Some(t.layer), at, w.owner, said),
            remove_wires: vec![i],
            remove_holes: holes,
            add: Vec::new(),
        }
    }

    /// Each track inside a pad: a track from the centre of a pad or a via of its net to a point still in it.
    pub(super) fn in_pad_tracks(&self) -> Vec<Candidate> {
        let mut out = Vec::new();
        for i in 0..self.copper.wires.len() {
            if !self.editable(i) {
                continue;
            }
            let t = &self.copper.wires[i].track;
            let mut said = None;
            for (centre, far) in [(t.from, t.to), (t.to, t.from)] {
                let pad = self.spots_of(&t.net).iter().find(|s| {
                    s.on(t.layer)
                        && s.centre.distance(centre) < NEAR
                        && s.centre.distance(far) <= s.inner
                });
                if pad.is_some() {
                    said = Some("removed a track from a pad's centre that ends inside the pad");
                    break;
                }
                let via = self.copper.holes.iter().any(|h| {
                    h.alive
                        && h.via.net == t.net
                        && h.via.at.distance(centre) < NEAR
                        && h.via.at.distance(far) <= h.via.diameter / 2.0
                });
                if via {
                    said = Some("removed a track from a via's centre that ends inside the via");
                    break;
                }
            }
            if let Some(said) = said {
                out.push(self.removal(
                    Kind::InPad,
                    i,
                    format!("{said} ({:.3} mm)", length(t)),
                    Vec::new(),
                ));
            }
        }
        out
    }

    /// Each track whose copper lies inside another's, and each pair of collinear tracks of one
    /// width that overlap along their line. A track is compared with the tracks of its net and
    /// layer that the board's grid lists near it, so the work grows with the neighbourhood and
    /// not with the length of the net (issue 1492).
    pub(super) fn overlaps(&self) -> Vec<Candidate> {
        let wires = &self.copper.wires;
        let mut by: BTreeMap<(String, &'static str), Vec<usize>> = BTreeMap::new();
        for (i, w) in wires.iter().enumerate() {
            if w.alive && w.track.mid.is_none() {
                by.entry((w.track.net.clone(), w.track.layer.word()))
                    .or_default()
                    .push(i);
            }
        }
        let mut out = Vec::new();
        let mut gone = std::collections::BTreeSet::new();
        for list in by.values() {
            for &a in list {
                let ta = &wires[a].track;
                let hood = super::near::track_box(ta).expanded(NEAR);
                // (The grid lists a track by the box it was drawn in, to the micrometre.)
                let mut others: Vec<usize> = self
                    .grid
                    .near(&hood)
                    .into_iter()
                    .filter_map(|item| match item {
                        super::near::Item::Wire(b) if b > a && b < wires.len() => Some(b),
                        _ => None,
                    })
                    .filter(|&b| {
                        let tb = &wires[b].track;
                        wires[b].alive
                            && tb.mid.is_none()
                            && tb.net == ta.net
                            && tb.layer == ta.layer
                    })
                    .collect();
                others.sort_unstable();
                for b in others {
                    if gone.contains(&a) || gone.contains(&b) {
                        continue;
                    }
                    self.compared.set(self.compared.get() + 1);
                    let (ta, tb) = (&wires[a].track, &wires[b].track);
                    if !super::near::meets(&super::near::track_box(ta), &super::near::track_box(tb))
                    {
                        continue;
                    }
                    let inside = |x: &Track, y: &Track| {
                        [x.from, x.to].iter().all(|&p| {
                            segment_distance(p, y.from, y.to) + x.width / 2.0
                                <= y.width / 2.0 + 1e-6
                        })
                    };
                    // The later of two the same is the one inside.
                    let covered = if inside(tb, ta) {
                        Some((b, a))
                    } else if inside(ta, tb) {
                        Some((a, b))
                    } else {
                        None
                    };
                    if let Some((x, y)) = covered {
                        if self.editable(x) {
                            gone.insert(x);
                            out.push(self.removal(
                                Kind::Overlap,
                                x,
                                format!(
                                    "removed a track of {:.3} mm whose copper lies inside a track of {:.3} mm",
                                    length(&self.copper.wires[x].track),
                                    length(&self.copper.wires[y].track)
                                ),
                                Vec::new(),
                            ));
                        }
                        continue;
                    }
                    if let Some(c) = self.joined(a, b) {
                        gone.insert(a);
                        gone.insert(b);
                        out.push(c);
                    }
                }
            }
        }
        out
    }

    /// Tracks `a` and `b`, of one width and owner on one line and overlapping along it, as one.
    fn joined(&self, a: usize, b: usize) -> Option<Candidate> {
        if !self.editable(a) || !self.editable(b) {
            return None;
        }
        let (wa, wb) = (&self.copper.wires[a], &self.copper.wires[b]);
        let (ta, tb) = (&wa.track, &wb.track);
        if wa.owner != wb.owner || (ta.width - tb.width).abs() > 1e-9 {
            return None;
        }
        let along = unit(ta.to - ta.from);
        if length(ta) < NEAR || length(tb) < NEAR {
            return None;
        }
        let off = |p: Pt| {
            let d = p - ta.from;
            (d.x * along.y - d.y * along.x).abs()
        };
        if off(tb.from) > NEAR || off(tb.to) > NEAR {
            return None;
        }
        let s = |p: Pt| dot(p - ta.from, along);
        let (a0, a1) = (0.0, length(ta));
        let (b0, b1) = {
            let (x, y) = (s(tb.from), s(tb.to));
            (x.min(y), x.max(y))
        };
        // They overlap: more than touching end to end.
        if b0.max(a0) > b1.min(a1) - NEAR {
            return None;
        }
        let (lo, hi) = (a0.min(b0), a1.max(b1));
        let at = |v: f64| ta.from + super::scale(along, v);
        let mut one = ta.clone();
        one.from = self.snap(at(lo));
        one.to = self.snap(at(hi));
        let mid = at((b0.max(a0) + b1.min(a1)) / 2.0);
        Some(Candidate {
            key: self.key(Kind::Overlap, &ta.net, Some(ta.layer), mid),
            change: self.change(
                Kind::Overlap,
                &ta.net,
                Some(ta.layer),
                mid,
                wa.owner,
                format!(
                    "made one track of {:.3} mm of two that overlap by {:.3} mm",
                    hi - lo,
                    b1.min(a1) - b0.max(a0)
                ),
            ),
            remove_wires: vec![a, b],
            remove_holes: Vec::new(),
            add: vec![one],
        })
    }

    /// `p` to the micrometre of the routes file's frame.
    fn snap(&self, p: Pt) -> Pt {
        let min = self.board.outline.min;
        let d = p - min;
        min + Pt::new(super::to_um(d.x), super::to_um(d.y))
    }

    /// Each way of a net that another, shorter, way between the same two stops makes needless.
    pub(super) fn duplicates(&self) -> Vec<Candidate> {
        // A node of a net's copper: a pad, a via, or a place on a layer.
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
        enum Node {
            Pad(usize),
            Via(usize),
            At(&'static str, (i64, i64)),
        }
        let mut nets: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (i, w) in self.copper.wires.iter().enumerate() {
            if w.alive {
                nets.entry(w.track.net.as_str()).or_default().push(i);
            }
        }
        let mut out = Vec::new();
        for (net, wires) in nets {
            let tee = wires.iter().any(|&i| {
                let t = &self.copper.wires[i].track;
                [t.from, t.to]
                    .iter()
                    .any(|&p| self.tee_at(net, t.layer, p, &[i]))
            });
            if tee {
                continue;
            }
            let pads = self.spots_of(net);
            let node = |p: Pt, layer: CopperLayer| -> Node {
                if let Some(k) = pads
                    .iter()
                    .position(|s| s.on(layer) && super::near::holds(&s.bounds, p))
                {
                    return Node::Pad(k);
                }
                match self.via_at(net, p) {
                    Some(v) => Node::Via(v),
                    None => Node::At(layer.word(), q(p)),
                }
            };
            // Each track's two nodes, and the tracks at each node.
            let mut at: BTreeMap<Node, Vec<usize>> = BTreeMap::new();
            let mut ends: BTreeMap<usize, (Node, Node)> = BTreeMap::new();
            for &i in &wires {
                let t = &self.copper.wires[i].track;
                let (a, b) = (node(t.from, t.layer), node(t.to, t.layer));
                at.entry(a.clone()).or_default().push(i);
                at.entry(b.clone()).or_default().push(i);
                ends.insert(i, (a, b));
            }
            let stop = |n: &Node| matches!(n, Node::Pad(_)) || at.get(n).map_or(0, Vec::len) != 2;
            // The ways: from each stop along each of its tracks to the next stop.
            let mut seen = std::collections::BTreeSet::new();
            // Each way by its two stops: its length, its tracks and the vias on its middle.
            type Way = (f64, Vec<usize>, Vec<usize>);
            let mut ways: BTreeMap<(Node, Node), Vec<Way>> = BTreeMap::new();
            for (start, list) in &at {
                if !stop(start) {
                    continue;
                }
                for &first in list {
                    if seen.contains(&first) {
                        continue;
                    }
                    let mut tracks = Vec::new();
                    let mut vias = Vec::new();
                    let mut here = start.clone();
                    let mut track = first;
                    let end = loop {
                        tracks.push(track);
                        let (a, b) = &ends[&track];
                        let next = if *a == here { b.clone() } else { a.clone() };
                        if stop(&next) || tracks.len() > wires.len() {
                            break next;
                        }
                        if let Node::Via(v) = next {
                            vias.push(v);
                        }
                        let Some(&t) = at[&next].iter().find(|&&t| t != track) else {
                            break next;
                        };
                        here = next;
                        track = t;
                    };
                    seen.extend(tracks.iter().copied());
                    let len: f64 = tracks
                        .iter()
                        .map(|&i| length(&self.copper.wires[i].track))
                        .sum();
                    let key = if *start <= end {
                        (start.clone(), end)
                    } else {
                        (end, start.clone())
                    };
                    ways.entry(key).or_default().push((len, tracks, vias));
                }
            }
            for (_, mut group) in ways {
                if group.len() < 2 {
                    continue;
                }
                // The shortest is kept; of two as long, the first.
                group.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                let kept = group[0].0;
                for (len, tracks, vias) in group.into_iter().skip(1) {
                    if !tracks.iter().all(|&i| self.editable(i))
                        || !vias.iter().all(|&v| !self.copper.holes[v].fixed)
                    {
                        continue;
                    }
                    let middle = tracks[tracks.len() / 2];
                    let mut c = self.removal(
                        Kind::Duplicate,
                        middle,
                        format!(
                            "removed a way of {} track(s) and {} via(s), {len:.3} mm, beside a way of {kept:.3} mm between the same two places",
                            tracks.len(),
                            vias.len()
                        ),
                        vias,
                    );
                    c.remove_wires = tracks;
                    out.push(c);
                }
            }
        }
        out
    }

    /// The copper no pad needs that ends in nothing (issue #1481): a track with an end that
    /// touches no pad of its net on its layer, no via of its net and no other copper of its net
    /// on its layer, and a via whose copper meets its net (its tracks and a pad it stands in) on
    /// fewer than two layers, each taken away and the rest looked at again, so that a dead
    /// branch of several tracks is all of it. Each is one change, of [`Kind::Stub`] or
    /// [`Kind::Via`]. Only the quality report counts by it: `routes tidy` removes a stub and a
    /// dangling via from the check's findings, when the net is complete.
    pub(super) fn dead_copper(&self) -> Vec<Candidate> {
        let wires = &self.copper.wires;
        let holes = &self.copper.holes;
        let mut wire_alive: Vec<bool> = wires.iter().map(|w| w.alive).collect();
        let mut hole_alive: Vec<bool> = holes.iter().map(|h| h.alive).collect();
        let mut out = Vec::new();
        loop {
            let mut dead_wires = Vec::new();
            for (i, w) in wires.iter().enumerate() {
                if !wire_alive[i] {
                    continue;
                }
                let t = &w.track;
                let joined = |p: Pt| {
                    self.spots_of(&t.net)
                        .iter()
                        .any(|s| s.on(t.layer) && super::near::holds(&s.bounds, p))
                        || holes.iter().enumerate().any(|(k, h)| {
                            hole_alive[k]
                                && h.via.net == t.net
                                && h.via.at.distance(p) <= h.via.diameter / 2.0 + NEAR
                        })
                        || wires.iter().enumerate().any(|(j, o)| {
                            j != i
                                && wire_alive[j]
                                && o.track.net == t.net
                                && o.track.layer == t.layer
                                && segment_distance(p, o.track.from, o.track.to)
                                    <= o.track.width / 2.0 + NEAR
                        })
                };
                if !joined(t.from) || !joined(t.to) {
                    dead_wires.push(i);
                }
            }
            let mut dead_holes = Vec::new();
            for (k, h) in holes.iter().enumerate() {
                if !hole_alive[k] {
                    continue;
                }
                let v = &h.via;
                let mut layers: Vec<&'static str> = Vec::new();
                for (i, w) in wires.iter().enumerate() {
                    let t = &w.track;
                    if wire_alive[i]
                        && t.net == v.net
                        && segment_distance(v.at, t.from, t.to) <= (v.diameter + t.width) / 2.0
                        && !layers.contains(&t.layer.word())
                    {
                        layers.push(t.layer.word());
                    }
                }
                for s in self.spots_of(&v.net) {
                    if super::near::holds(&s.bounds, v.at) {
                        for (on, word) in
                            [(s.front, CopperLayer::Front), (s.back, CopperLayer::Back)]
                        {
                            if on && !layers.contains(&word.word()) {
                                layers.push(word.word());
                            }
                        }
                    }
                }
                if layers.len() < 2 {
                    dead_holes.push(k);
                }
            }
            if dead_wires.is_empty() && dead_holes.is_empty() {
                return out;
            }
            for i in dead_wires {
                wire_alive[i] = false;
                let t = &wires[i].track;
                out.push(self.removal(
                    Kind::Stub,
                    i,
                    format!("a track of {:.3} mm that ends in nothing", length(t)),
                    Vec::new(),
                ));
            }
            for k in dead_holes {
                hole_alive[k] = false;
                let v = &holes[k].via;
                out.push(Candidate {
                    key: self.key(Kind::Via, &v.net, None, v.at),
                    change: self.change(
                        Kind::Via,
                        &v.net,
                        None,
                        v.at,
                        holes[k].owner,
                        "a via joined to its net on fewer than two layers".to_string(),
                    ),
                    remove_wires: Vec::new(),
                    remove_holes: vec![k],
                    add: Vec::new(),
                });
            }
        }
    }

    /// Each run of tiny tracks, in the form after those `attempt` says were refused.
    pub(super) fn tiny_runs(&self, attempt: &dyn Fn(&Key) -> usize) -> Vec<Candidate> {
        let ends = self.ends();
        let tiny = |i: usize| {
            let t = &self.copper.wires[i].track;
            self.editable(i) && length(t) < TINY * t.width
        };
        // A joint the run may pass: two tracks alike and tiny end there, and nothing else.
        let plain = |place: &Place, list: &[usize]| -> Option<(usize, usize)> {
            let [a, b] = list[..] else { return None };
            if a == b || !tiny(a) || !tiny(b) {
                return None;
            }
            let (wa, wb) = (&self.copper.wires[a], &self.copper.wires[b]);
            if wa.owner != wb.owner || (wa.track.width - wb.track.width).abs() > 1e-9 {
                return None;
            }
            let p = if q(wa.track.from) == place.2 {
                wa.track.from
            } else {
                wa.track.to
            };
            let net = &place.0;
            let layer = wa.track.layer;
            if self.via_at(net, p).is_some()
                || self.tee_at(net, layer, p, &[a, b])
                || self
                    .spots_of(net)
                    .iter()
                    .any(|s| s.on(layer) && super::near::holds(&s.bounds, p))
            {
                return None;
            }
            Some((a, b))
        };
        // The tiny tracks joined through plain joints.
        let mut next: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (place, list) in &ends {
            if let Some((a, b)) = plain(place, list) {
                next.entry(a).or_default().push(b);
                next.entry(b).or_default().push(a);
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for (&start, links) in &next {
            if seen.contains(&start) || links.len() != 1 {
                continue;
            }
            // From one end of a run along it to the other.
            let mut run = vec![start];
            let mut prev = usize::MAX;
            let mut here = start;
            while let Some(&n) = next[&here].iter().find(|&&n| n != prev) {
                if run.contains(&n) {
                    break;
                }
                run.push(n);
                prev = here;
                here = n;
            }
            seen.extend(run.iter().copied());
            if run.len() < TINY_RUN {
                continue;
            }
            if let Some(c) = self.tiny_run(&run, &ends, attempt) {
                out.push(c);
            }
        }
        out
    }

    /// The run `run` (tracks in order) replaced by its next form.
    fn tiny_run(
        &self,
        run: &[usize],
        ends: &BTreeMap<Place, Vec<usize>>,
        attempt: &dyn Fn(&Key) -> usize,
    ) -> Option<Candidate> {
        let track = |i: usize| &self.copper.wires[i].track;
        let first = track(run[0]);
        let second = track(run[1]);
        let shared = |a: &Track, b: &Track| {
            [a.from, a.to]
                .into_iter()
                .find(|&p| b.from.distance(p) < NEAR || b.to.distance(p) < NEAR)
        };
        let other = |t: &Track, p: Pt| {
            if t.from.distance(p) < NEAR {
                t.to
            } else {
                t.from
            }
        };
        let start = other(first, shared(first, second)?);
        let mut points = vec![start];
        for &i in run {
            let last = *points.last().expect("a point");
            points.push(other(track(i), last));
        }
        let end = *points.last().expect("a point");
        if start.distance(end) < NEAR {
            return None;
        }
        // The way in to the run at `p` and the way out of it, from the one other track there.
        let leg = |p: Pt, mine: usize| -> Option<Pt> {
            let t = track(mine);
            let list = ends.get(&(t.net.clone(), t.layer.word(), q(p)))?;
            let [o] = list
                .iter()
                .copied()
                .filter(|&o| o != mine)
                .collect::<Vec<_>>()[..]
            else {
                return None;
            };
            let o = track(o);
            if o.mid.is_some() {
                return None;
            }
            Some(other(o, p))
        };
        let into = leg(start, run[0]).map(|far| start - far);
        let out_of = leg(end, *run.last().expect("a track")).map(|far| far - end);
        let fits = |path: &[Pt]| {
            let mut ways: Vec<Pt> = Vec::new();
            ways.extend(into);
            ways.extend(path.windows(2).map(|w| w[1] - w[0]));
            ways.extend(out_of);
            ways.windows(2).all(|w| turn(w[0], w[1]) <= BEND)
        };
        let t0 = first;
        let net = t0.net.clone();
        let layer = t0.layer;
        let mid = points[points.len() / 2];
        let key = self.key(Kind::TinyRun, &net, Some(layer), mid);
        let mut forms: Vec<Vec<Pt>> = Vec::new();
        forms.push(vec![start, end]);
        if let (Some(a), Some(b)) = (into, out_of) {
            // Where the lines in and out cross.
            let d = a.x * b.y - a.y * b.x;
            if d.abs() > 1e-9 {
                let w = end - start;
                let s = (w.x * b.y - w.y * b.x) / d;
                let x = start + super::scale(a, s);
                if s > 0.0 && dot(end - x, b) > 0.0 {
                    forms.push(vec![start, x, end]);
                }
            }
        }
        let tried = attempt(&key);
        let path = forms.into_iter().filter(|f| fits(f)).nth(tried)?;
        let owner = self.copper.wires[run[0]].owner;
        let add = self.polyline(&path, t0, t0.width);
        if add.iter().any(|t| length(t) < NEAR) {
            return None;
        }
        let said = if path.len() == 2 {
            format!("made one track of {} tiny tracks", run.len())
        } else {
            format!("made one corner of {} tiny tracks", run.len())
        };
        Some(Candidate {
            key,
            change: self.change(Kind::TinyRun, &net, Some(layer), mid, owner, said),
            remove_wires: run.to_vec(),
            remove_holes: Vec::new(),
            add,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{findings, led, lines, path, routes_of, tidied};
    use super::super::{Counts, tidy};
    use netsmith_pcb::{CopperLayer, RouteTrack, Routes};

    const TOP: CopperLayer = CopperLayer::Front;
    /// The net from R1's pad 2 at (9.61, 4.4) to D1's pad 2 at (13.3875, 4.2), on top.
    const ANODE: &str = "net-led.anode";

    /// The anode's way along the top: out of R1's pad 2, a 45 degree bend, along y = 3, a 45
    /// degree bend and into D1's pad 2, with `along` in place of the straight track between
    /// the bends, (10.16, 3) to (12.8875, 3).
    fn anode(start: &[[f64; 2]], along: Vec<RouteTrack>) -> Vec<RouteTrack> {
        let mut t = path(TOP, start);
        t.extend(path(TOP, &[[9.61, 3.55], [10.16, 3.0]]));
        t.extend(along);
        t.extend(path(TOP, &[[12.8875, 3.0], [13.3875, 3.5], [13.3875, 4.2]]));
        t
    }

    /// `tracks` of the anode tidied: each change kept adds no finding and keeps the
    /// connections, and a second tidy has nothing to do.
    fn tidy_anode(tracks: Vec<RouteTrack>) -> (Counts, Vec<RouteTrack>, Vec<String>) {
        let board = led();
        let routes = routes_of(&board, vec![(ANODE, false, tracks, Vec::new())]);
        let before = findings(&board, &routes);
        let tidied = tidied(&board, &routes, None);
        let said = tidied.text();
        let after = findings(&board, &tidied.routes);
        let mut old = before.clone();
        for f in &after {
            let i = old.iter().position(|o| o == f);
            assert!(i.is_some(), "a new finding: {f}\n{said}");
            old.remove(i.unwrap());
        }
        assert_eq!(lines(&board, &tidied.routes), lines(&board, &routes));
        assert!(
            lines(&board, &routes).iter().all(|l| !l.contains(ANODE)),
            "{:?}",
            lines(&board, &routes)
        );
        let again = tidy(&board, &tidied.routes, None).unwrap();
        assert_eq!(again.changes, Vec::new(), "{}", again.text());
        let tracks = |r: &Routes| r.boards[&board.name].nets[ANODE].tracks.clone();
        (tidied.counts, tracks(&tidied.routes), vec![said])
    }

    /// Whether two places of the routes file are one, to a nanometre.
    fn same(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
    }

    fn only(counts: Counts) -> Counts {
        Counts {
            corners: 0,
            merges: 0,
            vias: 0,
            stubs: 0,
            acid_traps: 0,
            widened: 0,
            ..counts
        }
    }

    #[test]
    fn a_track_inside_a_pad_is_removed() {
        // From R1's pad 2's centre a step to (9.66, 4.35), still in the pad, then out of it.
        let (counts, tracks, said) = tidy_anode(path(
            TOP,
            &[
                [9.61, 4.4],
                [9.66, 4.35],
                [9.66, 3.5],
                [10.16, 3.0],
                [12.8875, 3.0],
                [13.3875, 3.5],
                [13.3875, 4.2],
            ],
        ));
        assert_eq!(
            only(counts),
            Counts {
                in_pad_tracks: 1,
                ..Counts::default()
            },
            "{said:?}"
        );
        assert!(
            !tracks
                .iter()
                .any(|t| t.from == [9.61, 4.4] || t.to == [9.61, 4.4]),
            "{tracks:#?}"
        );
    }

    #[test]
    fn the_longer_of_two_ways_between_two_places_is_removed() {
        let mut along = path(TOP, &[[10.16, 3.0], [12.8875, 3.0]]);
        // A second way from (10.16, 3) to (12.8875, 3), above the first.
        along.extend(path(
            TOP,
            &[[10.16, 3.0], [10.66, 2.5], [12.3875, 2.5], [12.8875, 3.0]],
        ));
        let (counts, tracks, said) = tidy_anode(anode(&[[9.61, 4.4], [9.61, 3.55]], along));
        assert_eq!(
            only(counts),
            Counts {
                duplicates: 1,
                ..Counts::default()
            },
            "{said:?}"
        );
        assert!(
            !tracks.iter().any(|t| t.from[1] < 2.9 || t.to[1] < 2.9),
            "{tracks:#?}"
        );
        assert!(
            tracks
                .iter()
                .any(|t| t.from == [10.16, 3.0] && t.to == [12.8875, 3.0]),
            "{tracks:#?}"
        );
    }

    #[test]
    fn overlapping_tracks_on_one_line_become_one_and_a_track_inside_another_goes() {
        // Two tracks that overlap from x = 11 to 12, and a short one inside the first.
        let along = path(TOP, &[[10.16, 3.0], [12.0, 3.0]])
            .into_iter()
            .chain(path(TOP, &[[11.0, 3.0], [12.8875, 3.0]]))
            .chain(path(TOP, &[[10.5, 3.0], [10.9, 3.0]]))
            .collect();
        let (counts, tracks, said) = tidy_anode(anode(&[[9.61, 4.4], [9.61, 3.55]], along));
        assert_eq!(
            only(counts),
            Counts {
                overlaps: 2,
                ..Counts::default()
            },
            "{said:?}"
        );
        let on_the_line: Vec<_> = tracks
            .iter()
            .filter(|t| (t.from[1] - 3.0).abs() < 1e-6 && (t.to[1] - 3.0).abs() < 1e-6)
            .collect();
        assert_eq!(on_the_line.len(), 1, "{tracks:#?}");
        let t = on_the_line[0];
        assert!(
            (same(t.from, [10.16, 3.0]) && same(t.to, [12.8875, 3.0]))
                || (same(t.to, [10.16, 3.0]) && same(t.from, [12.8875, 3.0])),
            "{tracks:#?}"
        );
    }

    #[test]
    fn a_run_of_tiny_tracks_at_a_corner_becomes_one_track() {
        let points = [
            [9.61, 4.4],
            [9.61, 3.55],
            [10.16, 3.0],
            [12.8, 3.0],
            // Six tracks of less than two widths each, round the corner to (13.3875, 3.5875).
            [12.98, 3.02],
            [13.13, 3.07],
            [13.25, 3.15],
            [13.33, 3.27],
            [13.37, 3.42],
            [13.3875, 3.5875],
            [13.3875, 4.2],
        ];
        let (counts, tracks, said) = tidy_anode(path(TOP, &points));
        assert_eq!(
            only(counts),
            Counts {
                tiny_runs: 1,
                ..Counts::default()
            },
            "{said:?}"
        );
        assert_eq!(tracks.len(), 5, "{tracks:#?}");
        assert!(
            tracks
                .iter()
                .any(|t| same(t.from, [12.8, 3.0]) && same(t.to, [13.3875, 3.5875])),
            "{tracks:#?}"
        );
    }

    /// The quality of the anode (issue #1481) of the board with `routes` on it.
    fn quality(routes: &Routes) -> crate::net_quality::Counts {
        let board = crate::routes::with_routes(&led(), routes).unwrap().0;
        let q = crate::net_quality::Quality::of_board(&board);
        q.nets
            .iter()
            .find(|n| n.net == ANODE)
            .map(|n| n.counts)
            .unwrap_or_default()
    }

    #[test]
    fn the_quality_report_counts_what_tidy_removes_and_nothing_once_tidied() {
        use crate::net_quality::Counts as Q;
        let duplicate = {
            let mut along = path(TOP, &[[10.16, 3.0], [12.8875, 3.0]]);
            along.extend(path(
                TOP,
                &[[10.16, 3.0], [10.66, 2.5], [12.3875, 2.5], [12.8875, 3.0]],
            ));
            anode(&[[9.61, 4.4], [9.61, 3.55]], along)
        };
        let nudge = path(
            TOP,
            &[
                [9.61, 4.4],
                [9.66, 4.35],
                [9.66, 3.5],
                [10.16, 3.0],
                [12.8875, 3.0],
                [13.3875, 3.5],
                [13.3875, 4.2],
            ],
        );
        // A dead branch of two tracks off the middle of the way along y = 3.
        let mut dead = anode(
            &[[9.61, 4.4], [9.61, 3.55]],
            path(TOP, &[[10.16, 3.0], [11.5, 3.0], [12.8875, 3.0]]),
        );
        dead.extend(path(TOP, &[[11.5, 3.0], [11.5, 2.0], [12.0, 1.5]]));
        // The way along y = 3 crossed by a loop of its own net, down and back up through it.
        let mut crossing = anode(
            &[[9.61, 4.4], [9.61, 3.55]],
            path(TOP, &[[10.16, 3.0], [11.0, 3.0], [12.8875, 3.0]]),
        );
        crossing.extend(path(
            TOP,
            &[
                [11.0, 3.0],
                [11.5, 2.5],
                [12.0, 2.5],
                [12.0, 3.5],
                [11.0, 3.0],
            ],
        ));
        for (name, tracks, want) in [
            (
                "duplicate",
                duplicate,
                Q {
                    duplicates: 1,
                    ..Q::default()
                },
            ),
            (
                "nudge",
                nudge,
                Q {
                    unneeded: 1,
                    ..Q::default()
                },
            ),
            (
                "dead",
                dead,
                Q {
                    unneeded: 2,
                    ..Q::default()
                },
            ),
            (
                "crossing",
                crossing,
                Q {
                    crossings: 1,
                    ..Q::default()
                },
            ),
        ] {
            let board = led();
            let routes = routes_of(&board, vec![(ANODE, false, tracks, Vec::new())]);
            assert_eq!(quality(&routes), want, "{name}");
            if name == "duplicate" || name == "nudge" {
                let tidied = tidy(&board, &routes, None).unwrap();
                assert_eq!(
                    quality(&tidied.routes),
                    Q::default(),
                    "{name}: {}",
                    tidied.text()
                );
            }
        }
    }
}
