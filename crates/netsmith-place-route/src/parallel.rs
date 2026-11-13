//! Vias side by side where one does not carry a net's current (issue #431).
//!
//! The design stage rates the process's via for current (`netsmith_design::guidelines::
//! via_current`) and says how many of them a net needs where a track changes layer
//! ([`netsmith_core::NetCurrent::vias`]). The router draws one via at each change of layer; this
//! pass, after routing, puts more beside it until the cluster has as many. A via of the cluster
//! stands so near the one before that their copper overlaps on both layers (their holes keep the
//! process's via-to-via space, `via_hole_to_hole`), so the vias are joined with no track. A place
//! is tried in a fixed order of eight directions round each via of the cluster, and taken only
//! where the via keeps the rules' space from every other net's copper, from every hole, from the
//! board edge, the cutouts and the keepouts that forbid vias; where none is free the cluster
//! stays short, and the design rules check (`via_current`) says so.
//!
//! The count is for the net's whole current, which a via on a branch of the net carries only part
//! of: the pessimism of ADR 0036's sum, kept rather than guessing the net's topology.

use netsmith_pcb::board::{Board, Via};
use netsmith_pcb::geometry::{Pt, Rect};

/// Space kept beyond the rules, for rounding.
pub(crate) const MARGIN: f64 = 0.02;

/// The distance from a point to a box, zero inside it.
fn to_rect(p: Pt, r: &Rect) -> f64 {
    let dx = (r.min.x - p.x).max(p.x - r.max.x).max(0.0);
    let dy = (r.min.y - p.y).max(p.y - r.max.y).max(0.0);
    dx.hypot(dy)
}

/// The distance from a point to a segment.
fn to_segment(p: Pt, a: Pt, b: Pt) -> f64 {
    let d = b - a;
    let len2 = d.x * d.x + d.y * d.y;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
    };
    p.distance(Pt::new(a.x + d.x * t, a.y + d.y * t))
}

/// Whether a via of `net` may stand at `p`.
fn allows(board: &Board, net: &str, p: Pt) -> bool {
    let r = &board.rules;
    let radius = r.via_diameter / 2.0;
    let edge = r.copper_to_edge + radius + MARGIN;
    if board.edge().depth(p) < edge
        || board
            .cutout_shapes()
            .iter()
            .any(|(_, c)| to_rect(p, &c.rect) < edge)
    {
        return false;
    }
    if board
        .keepout_areas()
        .iter()
        .any(|k| k.vias && to_rect(p, &k.rect) < radius + MARGIN)
    {
        return false;
    }
    let net_of = |refdes: &str, number: &str| {
        board
            .nets
            .iter()
            .find(|n| {
                n.pads
                    .iter()
                    .any(|q| q.refdes == refdes && q.number == number)
            })
            .map(|n| n.name.as_str())
    };
    for part in &board.parts {
        for pad in &part.footprint.pads {
            // No via stands in a pad, of any net: its hole would take the solder.
            if pad.copper_layers() != (false, false) {
                let keep = if net_of(&part.refdes, &pad.number) == Some(net) {
                    radius + MARGIN
                } else {
                    r.clearance + radius + MARGIN
                };
                if to_rect(p, &part.pad_bounds(pad)) < keep {
                    return false;
                }
            }
            if let Some((a, b, hole)) = part.hole(pad)
                && to_segment(p, a, b) - hole < r.hole_to_hole + r.via_drill / 2.0 + MARGIN
            {
                return false;
            }
        }
    }
    if board.tracks.iter().any(|t| {
        t.net != net && t.distance_to(p) - t.width / 2.0 < r.clearance + radius + MARGIN
    }) {
        return false;
    }
    board.vias.iter().all(|v| {
        let apart = p.distance(v.at);
        let holes = r.via_hole_to_hole + r.via_drill + MARGIN;
        if v.net == net {
            apart >= holes
        } else {
            apart >= holes.max(v.diameter / 2.0 + radius + r.clearance + MARGIN)
        }
    })
}

/// The vias of `net` whose copper touches the via at `index`, through others that touch,
/// as indices into the board's vias.
fn cluster(board: &Board, index: usize) -> Vec<usize> {
    let net = &board.vias[index].net;
    let mut found = vec![index];
    let mut k = 0;
    while k < found.len() {
        let (at, size) = (board.vias[found[k]].at, board.vias[found[k]].diameter);
        for (j, v) in board.vias.iter().enumerate() {
            if &v.net == net
                && !found.contains(&j)
                && v.at.distance(at) <= (v.diameter + size) / 2.0 + 1e-9
            {
                found.push(j);
            }
        }
        k += 1;
    }
    found
}

/// Add vias beside each via of a net whose current one via does not carry.
pub fn parallel_vias(board: &mut Board) {
    let r = board.rules.clone();
    // Near enough that the copper overlaps, as far as the holes' space allows.
    let step = (r.via_hole_to_hole + r.via_drill + MARGIN).max(0.0);
    if step > r.via_diameter {
        return;
    }
    let mut index = 0;
    while index < board.vias.len() {
        let net = board.vias[index].net.clone();
        let needs = board
            .net_currents
            .iter()
            .find(|c| c.net == net)
            .map_or(1, |c| c.vias);
        let mut members = cluster(board, index);
        // A cluster is grown from its first via only.
        if members.iter().any(|&m| m < index) {
            index += 1;
            continue;
        }
        'grow: while members.len() < needs {
            for &m in &members.clone() {
                let at = board.vias[m].at;
                for k in 0..8 {
                    let angle = std::f64::consts::FRAC_PI_4 * k as f64;
                    let p = Pt::new(at.x + step * angle.cos(), at.y + step * angle.sin());
                    if allows(board, &net, p) {
                        board.vias.push(Via {
                            net: net.clone(),
                            at: p,
                            diameter: r.via_diameter,
                            drill: r.via_drill,
                        });
                        members.push(board.vias.len() - 1);
                        continue 'grow;
                    }
                }
            }
            break;
        }
        index += 1;
    }
}

/// How many vias of each net stand in a cluster smaller than the net needs, as (net, the
/// cluster's size, what the net needs, where).
pub fn short_clusters(board: &Board) -> Vec<(String, usize, usize, Pt)> {
    let mut out = Vec::new();
    for (i, v) in board.vias.iter().enumerate() {
        let needs = board
            .net_currents
            .iter()
            .find(|c| c.net == v.net)
            .map_or(1, |c| c.vias);
        let members = cluster(board, i);
        if members.iter().all(|&m| m >= i) && members.len() < needs {
            out.push((v.net.clone(), members.len(), needs, v.at));
        }
    }
    out
}
