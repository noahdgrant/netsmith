//! Connectivity: which copper of a net is joined to which, and so which connections are
//! missing, which tracks and vias lead nowhere, and which pours join what.
//!
//! Two items of one net are joined if they share a layer and their copper runs into each other
//! by more than [`JOIN`] (issue #1334): a track whose end lies on a pad's edge, or whose round end
//! runs into it, is joined to the pad, as KiCad's connectivity joins a track's end to what lies
//! nearer than half its width; a round end that only touches the edge is not, in KiCad as here.
//! A track's end is held when its round end runs into copper of its net, so such a track is not
//! dangling either. Copper that only touches, or misses by less than [`NEAR_MISS`], is said so in
//! the finding, so a near miss is never silent. An arc track is the exception (issue #1542):
//! KiCad's hit test of an arc counts a point on its edge as on it, so a track's end whose round
//! end only touches an arc's copper is joined to it, and the distance is the arc's own, not its
//! chords' ([`ARC_TOUCH`]). Tracks of different widths are joined by the same rule as any two:
//! an end joins the other track when it lies nearer to its centre line than the two half widths
//! together (the round ends run into each other), as KiCad 9 has it. A pour joins every pad,
//! via and track of its net that touches its outline on its layer (the pour's fill is not
//! computed, so copper the fill would cut off is not seen: ADR 0037). Each net's separate
//! pieces are then joined, in the finding, by the shortest connections between their
//! anchors, which is how KiCad's ratsnest picks them.

use super::checks::{EPS, Out};
use super::model::{Copper, Kind, Model};
use super::shape::{ARC_TOUCH, JOIN, Shape, gap, joined, near_pairs};
use super::{Finding, Rule};
use crate::artwork::{Connect, TrackShape};
use crate::geom::{ARC_ERROR, Arc, Bounds, Pt};
use std::collections::BTreeMap;

struct Sets(Vec<usize>);

impl Sets {
    fn new(n: usize) -> Sets {
        Sets((0..n).collect())
    }

    fn find(&mut self, mut a: usize) -> usize {
        while self.0[a] != a {
            self.0[a] = self.0[self.0[a]];
            a = self.0[a];
        }
        a
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[b] = a;
        }
    }
}

/// Copper of one net this near other copper of it, and not joined, is a near miss: the finding
/// that the two are not connected says how far apart they are (issue #1334).
pub(crate) const NEAR_MISS: f64 = 0.01;

/// What a finding adds of copper that misses other copper of its net by `gap` mm.
fn near_miss(gap: f64, what: &str) -> String {
    if gap >= NEAR_MISS {
        String::new()
    } else if gap < 1e-6 {
        format!(
            "; it only touches {what}, and copper that touches without running into it is not joined (as in KiCad): run it into it"
        )
    } else {
        format!("; it misses {what} by {gap:.4} mm: run it into it")
    }
}

/// The item's arc and half width, when it is an arc track.
fn arc_of<'a>(m: &'a Model<'_>, c: &Copper) -> Option<(&'a Arc, f64)> {
    match c.kind {
        Kind::Track(t) => match &m.art.tracks[t].shape {
            TrackShape::Arc(arc) => Some((arc, m.art.tracks[t].width / 2.0)),
            TrackShape::Line(..) => None,
        },
        _ => None,
    }
}

/// Whether the round end of a track at `end`, of half width `half`, is joined to copper made of
/// `shapes` (a pad's, a via's, a straight track's): it runs into them by more than [`JOIN`]. The
/// rule the check joins a track's end by, and the one `board nets` traces a net's length by
/// (issue #1582), so the two never disagree on what a track's end reaches.
pub fn end_joins(end: Pt, half: f64, shapes: &[Shape]) -> bool {
    let at = Shape::disc(end, half);
    shapes.iter().any(|s| joined(s, &at))
}

/// Whether two pieces of copper, each made of its shapes, are joined: some shape of one runs
/// into some shape of the other by more than [`JOIN`] (two pads that overlap, a via in a pad).
pub fn copper_joins(a: &[Shape], b: &[Shape]) -> bool {
    a.iter().any(|sa| b.iter().any(|sb| joined(sa, sb)))
}

/// The copper of a pad, as the check joins it: its outline's shapes.
pub fn pad_copper(pad: &crate::artwork::Pad) -> Vec<Shape> {
    super::model::outline_shapes(&crate::pads::outline(pad))
}

/// Whether the round end of a track at `end`, of half width `h`, is joined to the item `o`.
fn end_on(m: &Model<'_>, o: &Copper, end: Pt, h: f64) -> bool {
    match arc_of(m, o) {
        Some((arc, r)) => arc.distance_to(end) <= h + r + ARC_TOUCH,
        None => end_joins(end, h, &o.shapes),
    }
}

fn touches(m: &Model<'_>, a: &Copper, b: &Copper) -> bool {
    let shapes = |depth: f64| {
        a.shapes.iter().any(|sa| {
            b.shapes.iter().any(|sb| {
                if depth <= JOIN {
                    return copper_joins(std::slice::from_ref(sa), std::slice::from_ref(sb));
                }
                let g = gap(sa, sb);
                g.d == 0.0 && g.depth > depth
            })
        })
    };
    let track = |c: &Copper| matches!(c.kind, Kind::Track(_));
    if !(track(a) && track(b)) || (arc_of(m, a).is_none() && arc_of(m, b).is_none()) {
        return shapes(JOIN);
    }
    // A track and an arc: KiCad joins them where an end of either lies on the other (the arc's
    // edge included), and where their copper runs into each other further than the arc's
    // chords can be wrong (the paths cross, or overlap along their middles).
    let half = |c: &Copper| match c.kind {
        Kind::Track(t) => m.art.tracks[t].width / 2.0,
        _ => 0.0,
    };
    let ends_on = |x: &Copper, y: &Copper| x.anchors.iter().any(|&e| end_on(m, y, e, half(x)));
    ends_on(a, b) || ends_on(b, a) || shapes(2.0 * ARC_ERROR)
}

/// The least gap between the copper of two items.
fn copper_gap(a: &Copper, b: &Copper) -> f64 {
    a.shapes
        .iter()
        .flat_map(|sa| b.shapes.iter().map(move |sb| gap(sa, sb).d))
        .fold(f64::MAX, f64::min)
}

/// Whether a pour reaches an item: the item touches the pour's outline, and for a pad the
/// pour and the pad both allow the connection.
fn pour_reaches(m: &Model<'_>, pour: &super::model::Pour, c: &Copper) -> bool {
    if c.net != pour.net || !c.layers.contains(&pour.layer) {
        return false;
    }
    if let Kind::Pad(i) = c.kind {
        let pad = &m.art.pads[i];
        let allowed = match pad.zone_connect {
            Some(Connect::None) => false,
            Some(_) => true,
            None => pour.joins_pads,
        };
        if !allowed {
            return false;
        }
    }
    pour.polygons.iter().any(|poly| {
        poly.len() >= 3 && {
            let area = Shape::polygon(poly.clone(), 0.0);
            c.shapes.iter().any(|s| gap(s, &area).d == 0.0)
        }
    })
}

pub(crate) fn run(m: &Model<'_>, findings: &mut Vec<Finding>) {
    let n = m.items.len();
    let mut sets = Sets::new(n + m.pours.len());
    // Items of one net that touch on a layer they share.
    let boxes: Vec<Bounds> = m.items.iter().map(|c| c.bounds).collect();
    let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); n];
    // An arc's box is its chords', which may fall short of the arc by the chords' error.
    for (i, j) in near_pairs(&boxes, 2.0 * ARC_ERROR) {
        let (a, b) = (&m.items[i], &m.items[j]);
        if a.net.is_none() || a.net != b.net || !a.layers.iter().any(|l| b.layers.contains(l)) {
            continue;
        }
        if touches(m, a, b) {
            sets.join(i, j);
            neighbours[i].push(j);
            neighbours[j].push(i);
        }
    }
    // Pours join what they reach.
    let mut poured: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (p, pour) in m.pours.iter().enumerate() {
        for (i, c) in m.items.iter().enumerate() {
            if pour_reaches(m, pour, c) {
                sets.join(n + p, i);
                poured[i].push(p);
            }
        }
        for (q, other) in m.pours.iter().enumerate().skip(p + 1) {
            if other.net == pour.net && other.layer == pour.layer && polygons_touch(pour, other) {
                sets.join(n + p, n + q);
            }
        }
    }

    let mut out = Out { m, findings };
    missing_connections(&mut out, &mut sets);
    dangling(&mut out, &neighbours, &poured);
}

/// The copper of one net as a graph (issue #1113): its items (indices into the model's), then
/// its pours, joined as [`run`] joins them.
pub(crate) struct NetGraph {
    pub items: Vec<usize>,
    /// The neighbours of each node: `0..items.len()` are the items, the rest the pours.
    pub adj: Vec<Vec<usize>>,
}

/// The graph of `net`'s copper.
pub(crate) fn net_graph(m: &Model<'_>, net: &str) -> NetGraph {
    let items: Vec<usize> = (0..m.items.len())
        .filter(|&i| m.items[i].net.as_deref() == Some(net))
        .collect();
    let pours: Vec<usize> = (0..m.pours.len())
        .filter(|&p| m.pours[p].net.as_deref() == Some(net))
        .collect();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); items.len() + pours.len()];
    let boxes: Vec<Bounds> = items.iter().map(|&i| m.items[i].bounds).collect();
    for (a, b) in near_pairs(&boxes, 2.0 * ARC_ERROR) {
        let (x, y) = (&m.items[items[a]], &m.items[items[b]]);
        if x.layers.iter().any(|l| y.layers.contains(l)) && touches(m, x, y) {
            adj[a].push(b);
            adj[b].push(a);
        }
    }
    for (k, &p) in pours.iter().enumerate() {
        let node = items.len() + k;
        for (a, &i) in items.iter().enumerate() {
            if pour_reaches(m, &m.pours[p], &m.items[i]) {
                adj[node].push(a);
                adj[a].push(node);
            }
        }
        for (j, &q) in pours.iter().enumerate().skip(k + 1) {
            if m.pours[q].layer == m.pours[p].layer && polygons_touch(&m.pours[p], &m.pours[q]) {
                adj[node].push(items.len() + j);
                adj[items.len() + j].push(node);
            }
        }
    }
    NetGraph { items, adj }
}

/// Whether the copper of its net joins the pad `number` of `part` (any pad of that number) to a
/// pour of its net, by the graph of [`net_graph`]; `None` when there is no such pad on a net.
pub(crate) fn pad_reaches_pour(m: &Model<'_>, part: &str, number: &str) -> Option<bool> {
    let is_pad = |c: &Copper| {
        matches!(c.kind, Kind::Pad(_))
            && c.item.part.as_deref() == Some(part)
            && c.item.pad.as_deref() == Some(number)
    };
    let net = m.items.iter().find(|c| is_pad(c))?.net.clone()?;
    let graph = net_graph(m, &net);
    let mut seen = vec![false; graph.adj.len()];
    let mut stack: Vec<usize> = (0..graph.items.len())
        .filter(|&k| is_pad(&m.items[graph.items[k]]))
        .collect();
    while let Some(k) = stack.pop() {
        if std::mem::replace(&mut seen[k], true) {
            continue;
        }
        if k >= graph.items.len() {
            return Some(true);
        }
        stack.extend(graph.adj[k].iter().copied().filter(|&j| !seen[j]));
    }
    Some(false)
}

fn polygons_touch(a: &super::model::Pour, b: &super::model::Pour) -> bool {
    a.polygons.iter().any(|pa| {
        b.polygons.iter().any(|pb| {
            pa.len() >= 3
                && pb.len() >= 3
                && gap(
                    &Shape::polygon(pa.clone(), 0.0),
                    &Shape::polygon(pb.clone(), 0.0),
                )
                .d == 0.0
        })
    })
}

/// A piece of a net: items that are joined, and where they can be joined to another.
struct Piece {
    items: Vec<usize>,
}

fn missing_connections(out: &mut Out<'_>, sets: &mut Sets) {
    let m = out.m;
    let mut nets: BTreeMap<&str, BTreeMap<usize, Piece>> = BTreeMap::new();
    for (i, c) in m.items.iter().enumerate() {
        let Some(net) = c.net.as_deref() else {
            continue;
        };
        let root = sets.find(i);
        nets.entry(net)
            .or_default()
            .entry(root)
            .or_insert_with(|| Piece { items: Vec::new() })
            .items
            .push(i);
    }
    for (net, pieces) in nets {
        if pieces.len() < 2 {
            continue;
        }
        let pieces: Vec<Piece> = pieces.into_values().collect();
        let anchors: Vec<Vec<(Pt, usize)>> = pieces
            .iter()
            .map(|p| {
                p.items
                    .iter()
                    .flat_map(|&i| m.items[i].anchors.iter().map(move |q| (*q, i)))
                    .collect()
            })
            .collect();
        // The shortest joins between pieces: Prim's tree on the distance between anchors.
        // `near[b]` is the nearest anchor pair from the tree to piece `b`.
        let mut in_tree = vec![false; pieces.len()];
        let mut near: Vec<Option<(f64, usize, usize)>> = vec![None; pieces.len()];
        let add = |x: usize,
                   in_tree: &mut Vec<bool>,
                   near: &mut Vec<Option<(f64, usize, usize)>>| {
            in_tree[x] = true;
            for b in (0..pieces.len()).filter(|b| !in_tree[*b]) {
                for &(qa, ia) in &anchors[x] {
                    for &(qb, ib) in &anchors[b] {
                        let d = qa.dist(qb);
                        // At equal distances KiCad takes the item on a layer the other
                        // has, then a track, then the longer.
                        let score = |i: usize, j: usize| {
                            let (a, b) = (&m.items[i], &m.items[j]);
                            let shared = a.layers.iter().any(|l| b.layers.contains(l));
                            let tracks = [i, j]
                                .iter()
                                .filter(|k| matches!(m.items[**k].kind, Kind::Track(_)))
                                .count();
                            (shared, tracks, (a.length + b.length).to_bits())
                        };
                        let better = near[b].is_none_or(|n| {
                            d < n.0 - 1e-9 || (d < n.0 + 1e-9 && score(ia, ib) > score(n.1, n.2))
                        });
                        if better {
                            near[b] = Some((d, ia, ib));
                        }
                    }
                }
            }
        };
        add(0, &mut in_tree, &mut near);
        for _ in 1..pieces.len() {
            let next = (0..pieces.len())
                .filter(|b| !in_tree[*b])
                .filter_map(|b| near[b].map(|n| (b, n)))
                .min_by(|x, y| x.1.0.total_cmp(&y.1.0));
            let Some((b, (d, ia, ib))) = next else { break };
            let (ca, cb) = (&m.items[ia], &m.items[ib]);
            let (qa, qb) = nearest_anchors(ca, cb);
            add(b, &mut in_tree, &mut near);
            // KiCad draws no line between points that coincide (two layers of copper laid
            // one over the other with no via between them).
            if d < 1e-6 {
                continue;
            }
            let miss = near_miss(copper_gap(ca, cb), &cb.item.what);
            out.push(
                Rule::UnconnectedItems,
                format!(
                    "net {net}: {} is not connected to {}, {d:.3} mm away{miss}",
                    ca.item.what, cb.item.what
                ),
                Pt::new((qa.x + qb.x) / 2.0, (qa.y + qb.y) / 2.0),
                None,
                vec![ca.item.clone(), cb.item.clone()],
            );
        }
    }
}

fn nearest_anchors(a: &Copper, b: &Copper) -> (Pt, Pt) {
    let mut best = (f64::MAX, a.anchors[0], b.anchors[0]);
    for &qa in &a.anchors {
        for &qb in &b.anchors {
            let d = qa.dist(qb);
            if d < best.0 {
                best = (d, qa, qb);
            }
        }
    }
    (best.1, best.2)
}

/// Tracks with an end that touches nothing and vias that join fewer than two layers.
fn dangling(out: &mut Out<'_>, neighbours: &[Vec<usize>], poured: &[Vec<usize>]) {
    let m = out.m;
    for (i, c) in m.items.iter().enumerate() {
        match c.kind {
            Kind::Track(t) if c.net.is_some() => {
                let track = &m.art.tracks[t];
                let layer = c.layers[0];
                // Each end must lie on something else of the net.
                let ends = [c.anchors[0], c.anchors[1]];
                let mut loose = None;
                for end in ends {
                    // The round end of the track: KiCad joins what lies within half its width
                    // of the end (issue #1334).
                    let at = Shape::disc(end, track.width / 2.0);
                    let held = neighbours[i].iter().any(|&j| {
                        let o = &m.items[j];
                        o.layers.contains(&layer) && end_on(m, o, end, track.width / 2.0)
                    }) || poured[i].iter().any(|&p| {
                        m.pours[p].layer == layer
                            && m.pours[p].polygons.iter().any(|poly| {
                                poly.len() >= 3
                                    && gap(&Shape::polygon(poly.clone(), 0.0), &at).d == 0.0
                            })
                    });
                    if !held {
                        loose = Some((end, at));
                        break;
                    }
                }
                if let Some((end, at)) = loose {
                    // The copper of the net the end only just misses.
                    let at = &at;
                    let miss = m
                        .items
                        .iter()
                        .enumerate()
                        .filter(|&(j, o)| j != i && o.net == c.net && o.layers.contains(&layer))
                        .filter(|(_, o)| o.bounds.expanded(NEAR_MISS).overlaps(&at.bounds))
                        .flat_map(|(_, o)| o.shapes.iter().map(move |s| (gap(s, at).d, o)))
                        .filter(|(g, _)| *g < NEAR_MISS)
                        .min_by(|x, y| x.0.total_cmp(&y.0))
                        .map(|(g, o)| near_miss(g, &o.item.what))
                        .unwrap_or_default();
                    out.push(
                        Rule::TrackDangling,
                        format!(
                            "{} has an end that touches nothing of net {}{miss}",
                            c.item.what,
                            c.net.as_deref().unwrap_or("")
                        ),
                        end,
                        Some(&track.layer),
                        vec![c.item.clone()],
                    );
                }
            }
            Kind::Via(_) if c.net.is_some() => {
                // The vias whose copper touches this one's, through others that touch, change
                // layer together (a cluster side by side, issue #431): the group is joined on
                // the layers where copper of the net that is no via of it touches one of them,
                // a pour's fill among it (issue #1185).
                let is_via = |j: usize| matches!(m.items[j].kind, Kind::Via(_));
                let mut group = vec![i];
                let mut k = 0;
                while k < group.len() {
                    for &j in &neighbours[group[k]] {
                        if is_via(j) && !group.contains(&j) {
                            group.push(j);
                        }
                    }
                    k += 1;
                }
                let mut layers: Vec<usize> = group
                    .iter()
                    .flat_map(|&g| {
                        neighbours[g]
                            .iter()
                            .filter(|&&j| !is_via(j))
                            .flat_map(|&j| m.items[j].layers.iter().copied())
                            .chain(poured[g].iter().map(|&p| m.pours[p].layer))
                    })
                    .collect();
                layers.sort_unstable();
                layers.dedup();
                if layers.len() < 2 {
                    let beside = if group.len() > 1 {
                        format!(" with the {} via(s) its copper touches", group.len() - 1)
                    } else {
                        String::new()
                    };
                    out.push(
                        Rule::ViaDangling,
                        format!(
                            "{}{beside} is joined to copper of its net on {} layer(s); a via needs at least two",
                            c.item.what,
                            layers.len()
                        ),
                        c.item.at,
                        None,
                        vec![c.item.clone()],
                    );
                }
            }
            _ => {}
        }
    }
    let _ = EPS;
}
