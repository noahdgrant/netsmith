//! The copper of a branch (issue #1113): the current a track or a via cluster carries when it
//! only feeds the pins of parts that state the current of their branch.
//!
//! A pin that states `.current` (a converter's bias pin, a feedback divider's top) says that the
//! copper between its pads and the rest of its net carries at most that, and the router draws
//! that copper for it, narrower than the net's and with fewer vias. The `track_current` and
//! `via_current` rules hold such copper to the current it carries: take it out of its net's
//! copper, and the rest falls apart into pieces. When at most one piece holds a pad of no branch
//! or a pour (the trunk, which may carry the net's whole current), what flows through the copper
//! taken out is what the other pieces' branches draw, each pin counted once however many pads
//! it has. With no such piece (a net of branches only) it is all of them but the largest piece's.
//! Copper whose removal leaves one piece (it closes a loop) or two pieces of trunk is the net's,
//! and is held to the net's current as before.

use std::collections::HashSet;

use super::connect::net_graph;
use super::model::{Kind, Model};

/// The most the copper of `removed` (indices into the model's items, all of `net`) carries, in
/// amperes, when it only feeds branches, as the header says; `None` when it may carry the net's
/// current.
pub(crate) fn current_through(m: &Model<'_>, net: &str, removed: &[usize]) -> Option<f64> {
    let branches: Vec<&super::Branch> = m.extras.branches.iter().filter(|b| b.net == net).collect();
    if branches.is_empty() {
        return None;
    }
    let graph = net_graph(m, net);
    let gone: HashSet<usize> = graph
        .items
        .iter()
        .enumerate()
        .filter(|(_, i)| removed.contains(i))
        .map(|(k, _)| k)
        .collect();
    if gone.len() != removed.len() {
        return None;
    }
    // The pieces round the removed copper: what its neighbours reach without it.
    let mut piece_of: Vec<Option<usize>> = vec![None; graph.adj.len()];
    let mut pieces: Vec<Vec<usize>> = Vec::new();
    for &g in &gone {
        for &start in &graph.adj[g] {
            if gone.contains(&start) || piece_of[start].is_some() {
                continue;
            }
            let id = pieces.len();
            let mut members = vec![start];
            piece_of[start] = Some(id);
            let mut k = 0;
            while k < members.len() {
                for &next in &graph.adj[members[k]] {
                    if !gone.contains(&next) && piece_of[next].is_none() {
                        piece_of[next] = Some(id);
                        members.push(next);
                    }
                }
                k += 1;
            }
            pieces.push(members);
        }
    }
    if pieces.len() < 2 {
        return None;
    }
    // Each piece: whether it is trunk, and what its branches draw.
    let mut trunks = 0;
    let mut drawn: Vec<f64> = Vec::new();
    for piece in &pieces {
        let mut trunk = false;
        let mut pins: HashSet<usize> = HashSet::new();
        for &node in piece {
            let Some(&item) = graph.items.get(node) else {
                // A pour of the net.
                trunk = true;
                continue;
            };
            let Kind::Pad(p) = m.items[item].kind else {
                continue;
            };
            let pad = &m.art.pads[p];
            match branches
                .iter()
                .position(|b| b.part == pad.part && b.pads.contains(&pad.number))
            {
                Some(b) => {
                    pins.insert(b);
                }
                None => trunk = true,
            }
        }
        if trunk {
            trunks += 1;
            drawn.push(f64::INFINITY);
        } else {
            drawn.push(pins.iter().map(|&b| branches[b].current).sum());
        }
    }
    match trunks {
        0 => {
            let largest = drawn.iter().copied().fold(0.0, f64::max);
            Some(drawn.iter().sum::<f64>() - largest)
        }
        1 => Some(drawn.iter().filter(|d| d.is_finite()).sum()),
        _ => None,
    }
}
