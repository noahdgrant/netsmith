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
//!
//! A pin need not state its current to be such a branch (issue #1333): a pin whose current the
//! model knows and is less than its net's (the design's `pin_currents`: what the rails and the
//! parts that pass a current through it carry, as a buck's feedback divider takes microamps
//! from its output) counts as one, with that current, and so does an `input` pin that nothing
//! carries a current through (an enable pin tied to a converter's input, issue #1504), with
//! none. A pad of a pin whose current is not
//! known (a capacitor's, a connector's that no rail names) is of the trunk, so the copper is
//! sized by the sum of the known currents of the pins past it and never by a guess.

use std::collections::HashSet;

use super::connect::net_graph;
use super::model::{Kind, Model};

/// What copper that only feeds branches carries (issue 1373): the most, in amperes, and the
/// pins of the branches it feeds, by their paths in the design, each once.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Through {
    pub current: f64,
    pub pins: Vec<String>,
}

/// The most the copper of `removed` (indices into the model's items, all of `net`) carries, in
/// amperes, when it only feeds branches, as the header says; `None` when it may carry the net's
/// current.
pub(crate) fn current_through(m: &Model<'_>, net: &str, removed: &[usize]) -> Option<Through> {
    // The stated branches first, so that a pad of a pin in both is the stated branch's.
    let branches: Vec<&super::Branch> = (m.extras.branches.iter())
        .chain(&m.extras.pin_currents)
        .filter(|b| b.net == net)
        .collect();
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
    let mut named: Vec<Vec<usize>> = Vec::new();
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
            named.push(Vec::new());
        } else {
            drawn.push(pins.iter().map(|&b| branches[b].current).sum());
            named.push(pins.iter().copied().collect());
        }
    }
    let (current, counted): (f64, Vec<usize>) = match trunks {
        0 => {
            let largest = drawn.iter().copied().fold(0.0, f64::max);
            let skip = drawn.iter().position(|&d| d == largest);
            (
                drawn.iter().sum::<f64>() - largest,
                (0..drawn.len()).filter(|&k| Some(k) != skip).collect(),
            )
        }
        1 => (
            drawn.iter().filter(|d| d.is_finite()).sum(),
            (0..drawn.len()).filter(|&k| drawn[k].is_finite()).collect(),
        ),
        _ => return None,
    };
    let mut pins: Vec<usize> = counted
        .into_iter()
        .flat_map(|k| named[k].iter().copied())
        .collect();
    pins.sort_unstable();
    pins.dedup();
    Some(Through {
        current,
        pins: pins.into_iter().map(|b| branches[b].path.clone()).collect(),
    })
}
