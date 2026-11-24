//! The connection of a net through its copper (issue #1487): the tracks that take its pads to
//! each other, which [`crate::NetTable`] measures a net's length on, and the copper that is not
//! part of it.
//!
//! The net's tracks are a graph: a track is split where another track of the net ends on it (a
//! tee), where a via of the net stands on it and where it passes through a pad of the net, and
//! the pieces are joined where their ends meet on one layer, at a via (on the layers it spans)
//! and in a pad (on any layer: the length is no check of a layer's connectivity, which the
//! design rules check makes). Where the design rules check joins copper the graph leaves apart,
//! a track's round end that runs into another track, a via or a pad of its net
//! ([`netsmith_geom::copper_end_joins`], the check's own rule, issue #1582), the graph joins it
//! too, nearest first and only where nothing else joins the two, so the length and the check
//! agree on what is joined and a stair of tracks shorter than their width is not cut short. The
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

use netsmith_geom::{
    Artwork, ArtworkPad, ArtworkVia, DrcShape, Pt, copper_end_joins as end_joins, copper_joins,
    pad_copper,
};

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

/// What a track's end reaches by its round end alone (issue #1582).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Target {
    Track(usize),
    Via(usize),
    Pad(usize),
}

/// A join only a track's round end makes: the end `end` (0 for the start, 1 for the end) of track
/// `from`, and what it runs into.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Reach {
    from: usize,
    end: usize,
    to: Target,
}

/// The graph of a net's tracks: the pieces, and the point each end of a piece, each via and each
/// pad is (`node[2 * e]` and `node[2 * e + 1]` the ends of piece `e`, then the vias, then the
/// pads).
struct Graph {
    pieces: Vec<Seg>,
    node: Vec<usize>,
}

/// The copper of a via, as the design rules check has it.
fn via_copper(v: &ArtworkVia) -> [DrcShape; 1] {
    [DrcShape::disc(v.at, v.diameter / 2.0)]
}

/// Whether the end `p` of a track is a tee on the track `s`: it lies on its copper, beside a
/// point of its centre line that is no end of it. An end that lies on the copper only near the
/// track's end is an end beside an end, which [`reaches`] joins where it must.
fn tee_on(p: Pt, s: &Seg) -> bool {
    let (d, t) = along(p, s.2, s.3);
    let q = Pt::new(s.2.x + (s.3.x - s.2.x) * t, s.2.y + (s.3.y - s.2.y) * t);
    d <= s.4 / 2.0 && q.dist(s.2) > SAME && q.dist(s.3) > SAME
}

/// Whether a via of `span` stands on the copper layer `layer` (an index; `None`, a layer the
/// board does not list, is on every via).
fn spans(span: Option<(usize, usize)>, layer: Option<usize>) -> bool {
    match (span, layer) {
        (Some((top, bottom)), Some(l)) => (top..=bottom).contains(&l),
        _ => true,
    }
}

/// Builds the graph: points that meet are joined (an end on an end, an end on a track's copper
/// beside its middle, an end in a via's or a pad's copper, pads and vias whose copper runs into
/// each other), and so are the joins of `reaches`.
fn graph(
    art: &Artwork,
    segs: &[&Seg],
    vias: &[&ArtworkVia],
    pads: &[&ArtworkPad],
    copper: &[Vec<DrcShape>],
    reaches: &[Reach],
) -> Graph {
    let layer_index = |layer: &str| art.copper.iter().position(|c| c == layer);
    let end_of = |r: &Reach| {
        let o = segs[r.from];
        if r.end == 0 { o.2 } else { o.3 }
    };
    // Split each track where another track's end or a via of the net stands on it, and where it
    // passes through a pad of the net, at the point of the track nearest it, so the pieces add up
    // to the track. An end that touches the track away from its centre line is joined to that
    // point (a tee): cutting at the end itself would draw a piece from the track to it, which
    // copies a short track that ends there (issue #1511).
    let mut pieces: Vec<Seg> = Vec::new();
    let mut tees: Vec<(Pt, Pt, &str)> = Vec::new();
    for (i, s) in segs.iter().enumerate() {
        let (_, layer, a, b, w) = s;
        let mut cuts: Vec<(f64, Pt)> = Vec::new();
        let mut cut_at = |p: Pt, tee: bool| {
            let (_, t) = along(p, *a, *b);
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
                for p in [o.2, o.3] {
                    if tee_on(p, s) {
                        cut_at(p, true);
                    }
                }
            }
        }
        for r in reaches.iter().filter(|r| r.to == Target::Track(i)) {
            cut_at(end_of(r), true);
        }
        for v in vias {
            if along(v.at, *a, *b).0 <= w / 2.0 {
                cut_at(v.at, false);
            }
        }
        for (d, pad) in pads.iter().enumerate() {
            let (_, t) = along(pad.at, *a, *b);
            let q = Pt::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
            if end_joins(q, SAME, &copper[d]) {
                cut_at(pad.at, false);
            }
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
    // The points: two ends of each piece, then each via, then each pad. An end in a pad is
    // joined to it on any layer: the length is no check of a layer's connectivity, which the
    // design rules check makes.
    let ends = 2 * pieces.len();
    let (via0, pad0) = (ends, ends + vias.len());
    let mut union = Union::new(ends + vias.len() + pads.len());
    let end = |k: usize| -> (Pt, &str) {
        let s = &pieces[k / 2];
        (if k.is_multiple_of(2) { s.2 } else { s.3 }, s.1.as_str())
    };
    let at = |pt: Pt, layer: &str| -> Vec<usize> {
        (0..ends)
            .filter(|&k| {
                let (e, l) = end(k);
                l == layer && e.dist(pt) <= SAME
            })
            .collect()
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
            if spans(via.span, layer_index(layer)) && end_joins(p, SAME, &via_copper(via)) {
                union.join(k, via0 + v);
            }
        }
        for (d, _) in pads.iter().enumerate() {
            if end_joins(p, SAME, &copper[d]) {
                union.join(k, pad0 + d);
            }
        }
    }
    // A track's end that touches another track is joined to the point of it nearest the end.
    for (q, p, layer) in &tees {
        for k in at(*q, layer) {
            for m in at(*p, layer) {
                union.join(k, m);
            }
        }
    }
    for r in reaches {
        let (p, layer) = (end_of(r), segs[r.from].1.as_str());
        let node = match r.to {
            Target::Track(_) => continue,
            Target::Via(v) => via0 + v,
            Target::Pad(d) => pad0 + d,
        };
        for k in at(p, layer) {
            union.join(k, node);
        }
    }
    // Pads on one point are one, and pads whose copper runs into each other on a layer, or a
    // via's, are joined, as the design rules check joins them.
    let pad_layers: Vec<Vec<usize>> = pads
        .iter()
        .map(|pad| {
            (0..art.copper.len())
                .filter(|&l| art.pad_copper_on(pad, &art.copper[l]))
                .collect()
        })
        .collect();
    for (d, pad) in pads.iter().enumerate() {
        for (e, other) in pads.iter().enumerate().skip(d + 1) {
            let share = pad_layers[d].iter().any(|l| pad_layers[e].contains(l));
            if other.at.dist(pad.at) <= SAME || (share && copper_joins(&copper[d], &copper[e])) {
                union.join(pad0 + d, pad0 + e);
            }
        }
        for (v, via) in vias.iter().enumerate() {
            let share = pad_layers[d].iter().any(|&l| via.on(l));
            if share && copper_joins(&copper[d], &via_copper(via)) {
                union.join(pad0 + d, via0 + v);
            }
        }
    }
    let node: Vec<usize> = (0..union.0.len()).map(|k| union.find(k)).collect();
    Graph { pieces, node }
}

/// The joins the graph needs that only a track's round end makes: an end whose round end runs
/// into another track of its layer, a via or a pad of the net, as the design rules check joins a
/// track's end (`netsmith_geom::copper_end_joins`, issue #1582), but whose point is not on that
/// copper (or is on a track's copper only beside its end), where the two are in parts of the
/// graph that nothing else joins, nearest first. Taking such a join where the copper is joined
/// already would cut short a stair of tracks shorter than their width, or a track into a via.
fn reaches(
    art: &Artwork,
    segs: &[&Seg],
    vias: &[&ArtworkVia],
    copper: &[Vec<DrcShape>],
    g: &Graph,
) -> Vec<Reach> {
    let ends = 2 * g.pieces.len();
    let (via0, pad0) = (ends, ends + vias.len());
    let mut parts = Union::new(g.node.len());
    for e in 0..g.pieces.len() {
        parts.join(g.node[2 * e], g.node[2 * e + 1]);
    }
    let point_of = |p: Pt, layer: &str| -> Option<usize> {
        (0..ends).find(|&k| {
            let s = &g.pieces[k / 2];
            s.1 == layer && (if k % 2 == 0 { s.2 } else { s.3 }).dist(p) <= SAME
        })
    };
    let layer_index = |layer: &str| art.copper.iter().position(|c| c == layer);
    let mut candidates: Vec<(f64, Reach, usize, usize)> = Vec::new();
    for (j, o) in segs.iter().enumerate() {
        let half = o.4 / 2.0;
        for (end, p) in [(0, o.2), (1, o.3)] {
            let Some(k) = point_of(p, &o.1) else {
                continue;
            };
            let from = g.node[k];
            let mut push = |d: f64, to: Target, node: usize| {
                candidates.push((d, Reach { from: j, end, to }, from, node));
            };
            for (i, s) in segs.iter().enumerate() {
                if i == j || s.1 != o.1 {
                    continue;
                }
                let d = along(p, s.2, s.3).0;
                let track = [DrcShape::capsule(s.2, s.3, s.4 / 2.0)];
                if d > SAME && !tee_on(p, s) && end_joins(p, half, &track) {
                    if let Some(on) = point_of(s.2, &s.1) {
                        push(d, Target::Track(i), g.node[on]);
                    }
                }
            }
            for (v, via) in vias.iter().enumerate() {
                let c = via_copper(via);
                if spans(via.span, layer_index(&o.1))
                    && !end_joins(p, SAME, &c)
                    && end_joins(p, half, &c)
                {
                    push(p.dist(via.at), Target::Via(v), g.node[via0 + v]);
                }
            }
            for (d, c) in copper.iter().enumerate() {
                if !end_joins(p, SAME, c) && end_joins(p, half, c) {
                    push(0.0, Target::Pad(d), g.node[pad0 + d]);
                }
            }
        }
    }
    candidates.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut out = Vec::new();
    for (_, r, a, b) in candidates {
        if parts.find(a) != parts.find(b) {
            parts.join(a, b);
            out.push(r);
        }
    }
    out
}

fn connect(art: &Artwork, net: &str, segs: &[&Seg], pads: &[&ArtworkPad]) -> Connection {
    let vias: Vec<&ArtworkVia> = art
        .vias
        .iter()
        .filter(|v| v.net.as_deref() == Some(net))
        .collect();
    let copper: Vec<Vec<DrcShape>> = pads.iter().map(|p| pad_copper(p)).collect();
    let mut g = graph(art, segs, &vias, pads, &copper, &[]);
    let needed = reaches(art, segs, &vias, &copper, &g);
    if !needed.is_empty() {
        g = graph(art, segs, &vias, pads, &copper, &needed);
    }
    let Graph { pieces, node } = g;
    let ends = 2 * pieces.len();
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
