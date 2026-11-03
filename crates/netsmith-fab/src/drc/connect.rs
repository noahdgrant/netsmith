//! Connectivity: which copper of a net is joined to which, and so which connections are
//! missing, which tracks and vias lead nowhere, and which pours join what.
//!
//! Two items of one net are joined if they share a layer and touch. A pour joins every pad,
//! via and track of its net that touches its outline on its layer (the pour's fill is not
//! computed, so copper the fill would cut off is not seen: ADR 0037). Each net's separate
//! pieces are then joined, in the finding, by the shortest connections between their
//! anchors, which is how KiCad's ratsnest picks them.

use super::checks::{EPS, Out};
use super::model::{Copper, Kind, Model};
use super::shape::{Shape, gap, near_pairs};
use super::{Finding, Rule};
use crate::artwork::Connect;
use crate::geom::{Bounds, Pt};
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

fn touches(a: &Copper, b: &Copper) -> bool {
    a.shapes.iter().any(|sa| {
        b.shapes
            .iter()
            .any(|sb| sa.bounds_touch(sb) && gap(sa, sb).d == 0.0)
    })
}

impl Shape {
    fn bounds_touch(&self, o: &Shape) -> bool {
        self.bounds.min.x <= o.bounds.max.x
            && o.bounds.min.x <= self.bounds.max.x
            && self.bounds.min.y <= o.bounds.max.y
            && o.bounds.min.y <= self.bounds.max.y
    }
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
    for (i, j) in near_pairs(&boxes, 0.0) {
        let (a, b) = (&m.items[i], &m.items[j]);
        if a.net.is_none() || a.net != b.net || !a.layers.iter().any(|l| b.layers.contains(l)) {
            continue;
        }
        if touches(a, b) {
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
            out.push(
                Rule::UnconnectedItems,
                format!(
                    "net {net}: {} is not connected to {}, {d:.3} mm away",
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
                    let at = Shape::disc(end, 0.0);
                    let held = neighbours[i].iter().any(|&j| {
                        let o = &m.items[j];
                        o.layers.contains(&layer) && o.shapes.iter().any(|s| gap(s, &at).d == 0.0)
                    }) || poured[i].iter().any(|&p| {
                        m.pours[p].layer == layer
                            && m.pours[p].polygons.iter().any(|poly| {
                                poly.len() >= 3
                                    && gap(&Shape::polygon(poly.clone(), 0.0), &at).d == 0.0
                            })
                    });
                    if !held {
                        loose = Some(end);
                        break;
                    }
                }
                if let Some(end) = loose {
                    out.push(
                        Rule::TrackDangling,
                        format!(
                            "{} has an end at ({:.3}, {:.3}) that touches nothing of net {}",
                            c.item.what,
                            end.x,
                            end.y,
                            c.net.as_deref().unwrap_or("")
                        ),
                        end,
                        Some(&track.layer),
                        vec![c.item.clone()],
                    );
                }
            }
            Kind::Via(_) if c.net.is_some() => {
                let mut layers: Vec<usize> = neighbours[i]
                    .iter()
                    .flat_map(|&j| m.items[j].layers.iter().copied())
                    .collect();
                layers.extend(poured[i].iter().map(|&p| m.pours[p].layer));
                layers.sort_unstable();
                layers.dedup();
                if layers.len() < 2 {
                    out.push(
                        Rule::ViaDangling,
                        format!(
                            "{} is joined to copper of its net on {} layer(s); a via needs at least two",
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
