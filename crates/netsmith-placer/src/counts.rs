//! What the placement objective for routing (issue #1534, the header of `anneal/routing.rs`)
//! measures on a placed board, counted: the tests assert these, and the measured test prints
//! them beside the router's objective.

use netsmith_pcb::{Board, Rect, Side};

use crate::anneal::fine_pitch;
use crate::anneal::routing::{chain_nets, find_buses};

/// How far the escape ring of a fine-pitch part reaches past its pads, in mm (`RING` of the
/// annealer).
const RING: f64 = 1.5;

/// A placed board by the measures of the objective for routing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoutingCounts {
    /// The parts on the other face from a fine-pitch part whose courtyard reaches under its
    /// escape ring (its pads grown by 1.5 mm), each fine-pitch part counted on its own; a
    /// capacitor that decouples the part and a part with a hole are not counted.
    pub under_escape: usize,
    /// The buses: two parts that share four or more nets of two or three pads.
    pub buses: usize,
    /// The parts in a bus's corridor (the box round the two parts' pads on its nets) on the
    /// satellite's face, each bus counted on its own; a part on a net of the bus and a
    /// capacitor that decouples either part are not counted.
    pub in_corridors: usize,
    /// The half perimeters of the power chain's nets added up, in mm.
    pub chain_length: f64,
    /// How many faces the parts on the power chain's nets are on: 0 with no chain.
    pub chain_faces: usize,
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.max.x.min(b.max.x) - a.min.x.max(b.min.x) > 1e-9
        && a.max.y.min(b.max.y) - a.min.y.max(b.min.y) > 1e-9
}

fn has_hole(part: &netsmith_pcb::Placed) -> bool {
    part.footprint.pads.iter().any(|p| p.drill.is_some())
}

/// The objective for routing's counts of `board` as it is placed.
pub fn routing_counts(board: &Board) -> RoutingCounts {
    let parts = &board.parts;
    let n = parts.len();
    let index = |refdes: &str| parts.iter().position(|p| p.refdes == refdes);
    // The pairs a decoupling capacitor and its part make, which neither measure counts.
    let mut near_ok = vec![false; n * n];
    for pair in netsmith_pcb::decoupling_pairs(parts, &board.nets) {
        for net in &pair.nets {
            for (i, _) in &net.pins {
                near_ok[pair.cap * n + i] = true;
                near_ok[i * n + pair.cap] = true;
            }
        }
    }
    let mut counts = RoutingCounts::default();

    for (c, part) in parts.iter().enumerate() {
        if !fine_pitch(part) {
            continue;
        }
        let Some(ring) = part
            .footprint
            .pads
            .iter()
            .filter(|p| p.drill.is_none())
            .map(|p| part.pad_bounds(p))
            .reduce(|a, b| a.union(&b))
            .map(|r| r.expanded(RING))
        else {
            continue;
        };
        counts.under_escape += parts
            .iter()
            .enumerate()
            .filter(|&(j, other)| {
                j != c
                    && other.side != part.side
                    && !has_hole(other)
                    && !near_ok[c * n + j]
                    && other
                        .courtyards()
                        .iter()
                        .any(|court| overlaps(court, &ring))
            })
            .count();
    }

    // Each net as the bus finder sees it: whether it is a plane, and its pads by part.
    let planes: Vec<String> = board.planes.iter().map(|p| p.net.clone()).collect();
    let pads: Vec<(bool, Vec<(usize, usize)>)> = board
        .nets
        .iter()
        .map(|net| {
            let mut found = Vec::new();
            for pad in &net.pads {
                if let Some(i) = index(&pad.refdes) {
                    for (k, q) in parts[i].footprint.pads.iter().enumerate() {
                        if q.number == pad.number {
                            found.push((i, k));
                        }
                    }
                }
            }
            (planes.contains(&net.name), found)
        })
        .collect();
    let pads_of: Vec<usize> = parts.iter().map(|p| p.footprint.pads.len()).collect();
    for (part, host, ids) in find_buses(&pads, &pads_of) {
        counts.buses += 1;
        if parts[part].side != parts[host].side {
            continue;
        }
        let mut on = vec![false; n];
        let mut corridor: Option<Rect> = None;
        for &id in &ids {
            for &(i, k) in &pads[id].1 {
                on[i] = true;
                if i == part || i == host {
                    let at = parts[i].pad_centre(&parts[i].footprint.pads[k]);
                    let dot = Rect::new(at, at);
                    corridor = Some(corridor.map_or(dot, |r| r.union(&dot)));
                }
            }
        }
        let Some(corridor) = corridor else { continue };
        counts.in_corridors += parts
            .iter()
            .enumerate()
            .filter(|&(j, other)| {
                !on[j]
                    && !near_ok[part * n + j]
                    && !near_ok[host * n + j]
                    && (other.side == parts[part].side || has_hole(other))
                    && other
                        .courtyards()
                        .iter()
                        .any(|court| overlaps(court, &corridor))
            })
            .count();
    }

    let currents: Vec<(String, f64)> = board
        .net_currents
        .iter()
        .map(|c| (c.net.clone(), c.current))
        .collect();
    let chain = chain_nets(&board.nets, &currents, &planes);
    let mut faces = [false; 2];
    for k in chain {
        let mut box_: Option<Rect> = None;
        for &(i, q) in &pads[k].1 {
            let at = parts[i].pad_centre(&parts[i].footprint.pads[q]);
            let dot = Rect::new(at, at);
            box_ = Some(box_.map_or(dot, |r| r.union(&dot)));
            faces[usize::from(parts[i].side == Side::Back)] = true;
        }
        if let Some(r) = box_ {
            counts.chain_length += r.width() + r.height();
        }
    }
    counts.chain_faces = faces.iter().filter(|f| **f).count();
    counts
}
