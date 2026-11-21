//! The connection of a net through its copper (issue #1487): the tracks that take its pads to
//! each other, which [`crate::NetTable`] measures a net's length on, and the copper that is not
//! part of it.
//!
//! The net's tracks are a graph: a track is split where another track of the net ends on it (a
//! tee) and where a via of the net stands on it, and the pieces are joined where their ends meet
//! on one layer, at a via (on the layers it spans) and in a pad (on any layer: the length is no
//! check of connectivity, which the design rules check makes). The
//! **connection** is, in each part of the graph that joins two pads or more, the shortest way
//! from its first pad to each of the others, all those ways together: for two pads the path
//! from pad to pad, for more a tree of the branches the pads need, and the copper that lies
//! wholly inside one of those pads (the short track a hand-routed board draws to a pad's centre
//! is the way into the pad, no stub to remove: issue #1511). The rest of the copper is **not
//! counted**, each piece in one run, so the runs add up to the copper less the length:
//!
//! - a **stub** leads nowhere: what is left when the ends that touch no pad are cut back, again
//!   and again, and every track of a part that joins fewer than two pads;
//! - a **second way** is the rest: copper that joins pads the connection already joins by a
//!   shorter way, as a duplicate track does.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use netsmith_geom::{Artwork, ArtworkPad, ArtworkVia, Pt};

use crate::net_table::Seg;

/// Points closer than this are one point, in mm.
const SAME: f64 = 1e-3;

/// What a piece of copper not counted is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Uncounted {
    /// Copper that leads to no pad.
    Stub,
    /// Copper that joins pads the connection already joins by a shorter way.
    SecondWay,
}

impl Uncounted {
    /// The words for a person: `a dead stub`, `a second way`.
    pub fn words(self) -> &'static str {
        match self {
            Uncounted::Stub => "a dead stub",
            Uncounted::SecondWay => "a second way",
        }
    }
}

/// A run of copper of a net that its length does not count: joined pieces of one kind on one
/// layer.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct NotCounted {
    pub kind: Uncounted,
    /// The copper layer, as the board file names it.
    pub layer: String,
    /// Its length, in mm.
    pub length: f64,
    /// A point at one end of it, in mm.
    pub at: (f64, f64),
    /// The pieces of track it is made of, so a view can draw it (issue #1495).
    pub pieces: Vec<Piece>,
}

/// One straight piece of a run not counted: a track, or the part of one between a tee or a via
/// and its end, in mm.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Piece {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub width: f64,
}

/// The connection of one net: the pieces of track it counts, and the runs it does not.
#[derive(Debug, Clone, Default)]
pub(crate) struct Connection {
    pub counted: Vec<Seg>,
    pub not_counted: Vec<NotCounted>,
}

/// A small union of sets.
struct Union(Vec<usize>);

impl Union {
    fn new(n: usize) -> Self {
        Union((0..n).collect())
    }
    fn find(&mut self, k: usize) -> usize {
        let mut r = k;
        while self.0[r] != r {
            r = self.0[r];
        }
        let mut k = k;
        while self.0[k] != r {
            let next = self.0[k];
            self.0[k] = r;
            k = next;
        }
        r
    }
    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a.max(b)] = a.min(b);
        }
    }
}

/// The distance from `p` to the segment `a`-`b`, and where along it, 0 to 1.
fn along(p: Pt, a: Pt, b: Pt) -> (f64, f64) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    };
    (p.dist(Pt::new(a.x + dx * t, a.y + dy * t)), t)
}

/// Whether `p` is in the copper of `pad` (its rectangle, turned with it).
fn in_pad(pad: &ArtworkPad, p: Pt) -> bool {
    let local = (p - pad.at).rotate(-pad.angle);
    local.x.abs() <= pad.size.0 / 2.0 + SAME && local.y.abs() <= pad.size.1 / 2.0 + SAME
}

/// The connection of each net of `segs` with a pad on the board, by name. A net with no pad has
/// none: its length is all its copper.
pub(crate) fn of(art: &Artwork, segs: &[Seg]) -> BTreeMap<String, Connection> {
    let mut by_net: BTreeMap<&str, Vec<&Seg>> = BTreeMap::new();
    for s in segs {
        by_net.entry(s.0.as_str()).or_default().push(s);
    }
    by_net
        .into_iter()
        .filter_map(|(net, segs)| {
            let pads: Vec<&ArtworkPad> = art
                .pads
                .iter()
                .filter(|p| p.net.as_deref() == Some(net))
                .collect();
            (!pads.is_empty()).then(|| (net.to_string(), connect(art, net, &segs, &pads)))
        })
        .collect()
}

fn connect(art: &Artwork, net: &str, segs: &[&Seg], pads: &[&ArtworkPad]) -> Connection {
    let vias: Vec<&ArtworkVia> = art
        .vias
        .iter()
        .filter(|v| v.net.as_deref() == Some(net))
        .collect();
    let layer_index = |layer: &str| art.copper.iter().position(|c| c == layer);
    // Split each track where another track's end or a via of the net stands on it, at the point
    // of the track nearest it, so the pieces add up to the track. An end that touches the track
    // away from its centre line is joined to that point (a tee): cutting at the end itself would
    // draw a piece from the track to it, which copies a short track that ends there (issue #1511).
    let mut pieces: Vec<Seg> = Vec::new();
    let mut tees: Vec<(Pt, Pt, &str)> = Vec::new();
    for (i, s) in segs.iter().enumerate() {
        let (_, layer, a, b, w) = s;
        let mut cuts: Vec<(f64, Pt)> = Vec::new();
        let mut cut_at = |p: Pt, reach: f64, tee: bool| {
            let (d, t) = along(p, *a, *b);
            if d > reach {
                return;
            }
            let q = Pt::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
            if q.dist(*a) > SAME && q.dist(*b) > SAME {
                cuts.push((t, q));
            }
            if tee && p.dist(q) > SAME {
                tees.push((q, p, layer.as_str()));
            }
        };
        for (j, o) in segs.iter().enumerate() {
            if j != i && o.1 == *layer {
                cut_at(o.2, w / 2.0, true);
                cut_at(o.3, w / 2.0, true);
            }
        }
        for v in &vias {
            cut_at(v.at, w / 2.0, false);
        }
        cuts.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut from = *a;
        for (_, p) in cuts.into_iter().chain([(1.0, *b)]) {
            if p.dist(from) > SAME || p == *b {
                pieces.push((s.0.clone(), layer.clone(), from, p, *w));
                from = p;
            }
        }
    }
    // The points: two ends of each piece, then each via, then each pad.
    let ends = 2 * pieces.len();
    let mut union = Union::new(ends + vias.len() + pads.len());
    let end = |k: usize| -> (Pt, &str) {
        let s = &pieces[k / 2];
        (if k.is_multiple_of(2) { s.2 } else { s.3 }, s.1.as_str())
    };
    for k in 0..ends {
        let (p, layer) = end(k);
        for m in k + 1..ends {
            let (q, other) = end(m);
            if layer == other && p.dist(q) <= SAME {
                union.join(k, m);
            }
        }
        for (v, via) in vias.iter().enumerate() {
            let spans = match (via.span, layer_index(layer)) {
                (Some((top, bottom)), Some(l)) => (top..=bottom).contains(&l),
                _ => true,
            };
            if spans && p.dist(via.at) <= via.diameter / 2.0 + SAME {
                union.join(k, ends + v);
            }
        }
        for (d, pad) in pads.iter().enumerate() {
            if in_pad(pad, p) {
                union.join(k, ends + vias.len() + d);
            }
        }
    }
    // A track's end that touches another track is joined to the point of it nearest the end.
    for (q, p, layer) in &tees {
        let at = |pt: Pt| -> Vec<usize> {
            (0..ends)
                .filter(|&k| {
                    let (e, l) = end(k);
                    l == *layer && e.dist(pt) <= SAME
                })
                .collect()
        };
        for k in at(*q) {
            for m in at(*p) {
                union.join(k, m);
            }
        }
    }
    // Pads on one point are one.
    for (d, pad) in pads.iter().enumerate() {
        if let Some(first) = pads.iter().position(|q| q.at.dist(pad.at) <= SAME) {
            union.join(ends + vias.len() + d, ends + vias.len() + first);
        }
    }
    let node: Vec<usize> = (0..union.0.len()).map(|k| union.find(k)).collect();
    let edge = |e: usize| (node[2 * e], node[2 * e + 1]);
    let mut adjacent: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for e in 0..pieces.len() {
        let (u, v) = edge(e);
        if u != v {
            adjacent.entry(u).or_default().push(e);
            adjacent.entry(v).or_default().push(e);
        }
    }
    let mut terminals: Vec<usize> = Vec::new();
    for d in 0..pads.len() {
        let t = node[ends + vias.len() + d];
        if !terminals.contains(&t) {
            terminals.push(t);
        }
    }
    // The parts of the graph, and the pads of each.
    let mut part: BTreeMap<usize, usize> = BTreeMap::new();
    let mut parts = 0;
    for &start in adjacent.keys() {
        if part.contains_key(&start) {
            continue;
        }
        let mut stack = vec![start];
        part.insert(start, parts);
        while let Some(u) = stack.pop() {
            for &e in &adjacent[&u] {
                let (a, b) = edge(e);
                let w = if a == u { b } else { a };
                if part.insert(w, parts).is_none() {
                    stack.push(w);
                }
            }
        }
        parts += 1;
    }
    let mut counted = vec![false; pieces.len()];
    for p in 0..parts {
        let pads_here: Vec<usize> = terminals
            .iter()
            .copied()
            .filter(|t| part.get(t) == Some(&p))
            .collect();
        if pads_here.len() < 2 {
            continue;
        }
        // The shortest way from the first pad to every point, then back from each other pad.
        let mut best: BTreeMap<usize, f64> = BTreeMap::new();
        let mut came: BTreeMap<usize, usize> = BTreeMap::new();
        let mut heap = BinaryHeap::new();
        best.insert(pads_here[0], 0.0);
        heap.push(Reverse((0u64, pads_here[0])));
        while let Some(Reverse((bits, u))) = heap.pop() {
            let d = f64::from_bits(bits);
            if d > best[&u] {
                continue;
            }
            for &e in &adjacent[&u] {
                let (a, b) = edge(e);
                let w = if a == u { b } else { a };
                let next = d + pieces[e].2.dist(pieces[e].3);
                if best.get(&w).is_none_or(|&old| next < old) {
                    best.insert(w, next);
                    came.insert(w, e);
                    heap.push(Reverse((next.to_bits(), w)));
                }
            }
        }
        for &t in &pads_here[1..] {
            let mut at = t;
            while let Some(&e) = came.get(&at) {
                if counted[e] {
                    break;
                }
                counted[e] = true;
                let (a, b) = edge(e);
                at = if a == at { b } else { a };
            }
        }
    }
    // Copper wholly inside a pad the connection reaches is counted: it is the way into the pad
    // (the short track a hand-routed board draws to the pad's centre), not copper to remove.
    for (e, counted) in counted.iter_mut().enumerate() {
        let (u, v) = edge(e);
        let reached = part
            .get(&u)
            .is_some_and(|p| terminals.iter().filter(|t| part.get(t) == Some(p)).count() >= 2);
        if u == v && terminals.contains(&u) && reached {
            *counted = true;
        }
    }
    // Cut back the ends that touch no pad: what goes is a stub; what stays, a second way.
    let mut kind: Vec<Option<Uncounted>> = (0..pieces.len())
        .map(|e| (!counted[e]).then_some(Uncounted::SecondWay))
        .collect();
    let mut degree: BTreeMap<usize, usize> =
        adjacent.iter().map(|(&u, es)| (u, es.len())).collect();
    let mut live = vec![true; pieces.len()];
    for e in 0..pieces.len() {
        let (u, v) = edge(e);
        let lonely = part
            .get(&u)
            .is_none_or(|p| terminals.iter().filter(|t| part.get(t) == Some(p)).count() < 2);
        if (u == v && !counted[e]) || lonely {
            kind[e] = Some(Uncounted::Stub);
            live[e] = false;
        }
    }
    let mut leaves: Vec<usize> = degree
        .iter()
        .filter(|(u, d)| **d == 1 && !terminals.contains(u))
        .map(|(&u, _)| u)
        .collect();
    while let Some(u) = leaves.pop() {
        let Some(&e) = adjacent[&u].iter().find(|&&e| live[e] && !counted[e]) else {
            continue;
        };
        live[e] = false;
        kind[e] = Some(Uncounted::Stub);
        let (a, b) = edge(e);
        for w in [a, b] {
            let d = degree.get_mut(&w).expect("a point of an edge");
            *d -= 1;
            if *d == 1 && w != u && !terminals.contains(&w) {
                leaves.push(w);
            }
        }
    }
    // Join the pieces not counted into runs of one kind on one layer.
    let mut runs = Union::new(pieces.len());
    for es in adjacent.values() {
        for (x, &e) in es.iter().enumerate() {
            for &f in &es[x + 1..] {
                if kind[e].is_some() && kind[e] == kind[f] && pieces[e].1 == pieces[f].1 {
                    runs.join(e, f);
                }
            }
        }
    }
    let mut by_run: BTreeMap<usize, NotCounted> = BTreeMap::new();
    for (e, k) in kind.iter().enumerate() {
        let Some(k) = *k else { continue };
        let r = runs.find(e);
        let s = &pieces[e];
        let run = by_run.entry(r).or_insert_with(|| NotCounted {
            kind: k,
            layer: s.1.clone(),
            length: 0.0,
            at: (s.2.x, s.2.y),
            pieces: Vec::new(),
        });
        run.length += s.2.dist(s.3);
        run.pieces.push(Piece {
            from: (s.2.x, s.2.y),
            to: (s.3.x, s.3.y),
            width: s.4,
        });
    }
    let mut not_counted: Vec<NotCounted> =
        by_run.into_values().filter(|r| r.length > SAME).collect();
    not_counted.sort_by(|a, b| b.length.total_cmp(&a.length).then(a.kind.cmp(&b.kind)));
    Connection {
        counted: pieces
            .into_iter()
            .zip(counted)
            .filter_map(|(s, c)| c.then_some(s))
            .collect(),
        not_counted,
    }
}
