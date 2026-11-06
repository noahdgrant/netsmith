//! How good a placement is, measured on the board and not on the placer's own cost, and the
//! decoupling pairs the placer derives from the netlist (ADR 0033).
//!
//! A decoupling pair is a capacitor whose two pads are on two different nets, both of which
//! carry a `power` pin of one same part: the part's supply and its return. Nothing in the
//! source says so; the netlist and the pin kinds do. The capacitor belongs next to those
//! pins.

use std::collections::HashMap;

use crate::board::{Board, BoardNet, Placed};
use crate::geometry::Pt;
use crate::ratsnest::{Ratline, Ratsnest};

/// One net of a decoupling pair: the capacitor's pad on it and the power pins it should be
/// near.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PairNet {
    /// The number of the capacitor's pad on this net.
    pub cap_pad: String,
    /// The pads of power pins on this net, as (part index, pad number): the candidates the
    /// capacitor is pulled to, the nearest one counting.
    pub pins: Vec<(usize, String)>,
}

/// A capacitor and the part it decouples.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DecouplingPair {
    /// The capacitor's index in the board's parts.
    pub cap: usize,
    pub nets: [PairNet; 2],
}

fn is_capacitor(part: &Placed) -> bool {
    is_capacitor_kind(&part.kind, &part.provides)
}

/// A capacitor by its kind, or because its definition provides the generic `Capacitor`
/// (ADR 0059), whatever its own name.
fn is_capacitor_kind(kind: &str, provides: &[String]) -> bool {
    kind.starts_with("Capacitor") || provides.iter().any(|g| g == "Capacitor")
}

/// The number of leading segments two instance paths share, not counting the part's own
/// name: `stage.cin` and `stage.reg` share one.
fn shared_segments(a: &str, b: &str) -> usize {
    let parent = |p: &str| {
        p.rsplit_once('.')
            .map_or("", |(parent, _)| parent)
            .to_string()
    };
    let (a, b) = (parent(a), parent(b));
    a.split('.')
        .zip(b.split('.'))
        .take_while(|(x, y)| !x.is_empty() && x == y)
        .count()
}

/// The decoupling pairs of a board: every two-pad capacitor between two nets that one same
/// other part has power pins on. Of several such parts, the ones that share the most of the
/// capacitor's instance path are kept (the capacitor of a module belongs to the module's
/// own part), then all of them. Parts are in board order, so the result does not depend on
/// anything else.
pub(crate) fn decoupling_pairs(parts: &[Placed], nets: &[BoardNet]) -> Vec<DecouplingPair> {
    let index: HashMap<&str, usize> = parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.refdes.as_str(), i))
        .collect();
    // The net of each pad, and the power pads of each net.
    let mut net_of: HashMap<(usize, &str), usize> = HashMap::new();
    let mut power: Vec<Vec<(usize, String)>> = vec![Vec::new(); nets.len()];
    for (n, net) in nets.iter().enumerate() {
        for pad in &net.pads {
            let Some(&i) = index.get(pad.refdes.as_str()) else {
                continue;
            };
            net_of.insert((i, pad.number.as_str()), n);
            if !is_capacitor(&parts[i]) && parts[i].power_pads.contains(&pad.number) {
                power[n].push((i, pad.number.clone()));
            }
        }
    }
    let mut pairs = Vec::new();
    for (c, cap) in parts.iter().enumerate() {
        if !is_capacitor(cap) {
            continue;
        }
        let numbers = cap.footprint.pad_numbers();
        let [first, second] = numbers.as_slice() else {
            continue;
        };
        let (first, second) = (*first, *second);
        let (Some(&n1), Some(&n2)) = (net_of.get(&(c, first)), net_of.get(&(c, second))) else {
            continue;
        };
        if n1 == n2 {
            continue;
        }
        // The parts with power pins on both nets.
        let mut targets: Vec<usize> = power[n1]
            .iter()
            .map(|&(i, _)| i)
            .filter(|i| power[n2].iter().any(|(j, _)| j == i))
            .collect();
        targets.sort_unstable();
        targets.dedup();
        if targets.is_empty() {
            continue;
        }
        let best = targets
            .iter()
            .map(|&i| shared_segments(&cap.path, &parts[i].path))
            .max()
            .unwrap_or(0);
        targets.retain(|&i| shared_segments(&cap.path, &parts[i].path) == best);
        let side = |pad: &str, net: usize| PairNet {
            cap_pad: pad.to_string(),
            pins: power[net]
                .iter()
                .filter(|(i, _)| targets.contains(i))
                .cloned()
                .collect(),
        };
        pairs.push(DecouplingPair {
            cap: c,
            nets: [side(first, n1), side(second, n2)],
        });
    }
    pairs
}

/// How far a decoupling capacitor is from the part it decouples.
#[derive(Debug, Clone, PartialEq)]
pub struct DecouplingDistance {
    /// The capacitor's reference designator.
    pub cap: String,
    /// The reference designator of the part with the nearest power pins.
    pub part: String,
    /// The mean, over the capacitor's two pads, of the distance from the pad's centre to the
    /// centre of the nearest power pin pad of the part on the same net, in mm.
    pub distance: f64,
}

/// Measures of a placed board, all from pad centres.
#[derive(Debug, Clone, PartialEq)]
pub struct Quality {
    /// The sum over the nets of the half perimeter of the box round the net's pads, in mm.
    pub hpwl: f64,
    /// The length of the ratsnest of the board as it is, in mm.
    pub ratsnest: f64,
    /// How many pairs of ratsnest lines of different nets cross.
    pub crossings: usize,
    /// One for each decoupling capacitor.
    pub decoupling: Vec<DecouplingDistance>,
}

impl Quality {
    pub fn of(board: &Board) -> Quality {
        let mut hpwl = 0.0;
        for net in &board.nets {
            let points: Vec<Pt> = crate::ratsnest::nodes_of(&board.parts, net)
                .iter()
                .map(|n| n.centre)
                .collect();
            if let Some(b) = crate::geometry::Rect::around(points) {
                hpwl += b.width() + b.height();
            }
        }
        let ratsnest = Ratsnest::of(board);
        let decoupling = decoupling_pairs(&board.parts, &board.nets)
            .iter()
            .map(|pair| {
                let cap = &board.parts[pair.cap];
                let mut total = 0.0;
                let mut part = String::new();
                let mut nearest_overall = f64::MAX;
                for side in &pair.nets {
                    let mut nearest = f64::MAX;
                    for pad in cap.footprint.pads_numbered(&side.cap_pad) {
                        let at = cap.pad_centre(pad);
                        for (i, number) in &side.pins {
                            let other = &board.parts[*i];
                            for q in other.footprint.pads_numbered(number) {
                                let d = at.distance(other.pad_centre(q));
                                nearest = nearest.min(d);
                                if d < nearest_overall {
                                    nearest_overall = d;
                                    part = other.refdes.clone();
                                }
                            }
                        }
                    }
                    total += nearest;
                }
                DecouplingDistance {
                    cap: cap.refdes.clone(),
                    part,
                    distance: total / 2.0,
                }
            })
            .collect();
        Quality {
            hpwl,
            ratsnest: ratsnest.total_length(),
            crossings: crossings(&ratsnest.lines),
            decoupling,
        }
    }

    /// The mean distance of the decoupling capacitors, 0 when there are none.
    pub fn decoupling_mean(&self) -> f64 {
        if self.decoupling.is_empty() {
            return 0.0;
        }
        self.decoupling.iter().map(|d| d.distance).sum::<f64>() / self.decoupling.len() as f64
    }

    /// The greatest distance of a decoupling capacitor, 0 when there are none.
    pub fn decoupling_max(&self) -> f64 {
        self.decoupling
            .iter()
            .map(|d| d.distance)
            .fold(0.0, f64::max)
    }
}

/// Whether the segments `p`-`q` and `r`-`s` cross at a point inside both.
pub(crate) fn segments_cross(p: Pt, q: Pt, r: Pt, s: Pt) -> bool {
    let side = |a: Pt, b: Pt, c: Pt| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let (d1, d2, d3, d4) = (side(p, q, r), side(p, q, s), side(r, s, p), side(r, s, q));
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

/// The number of pairs of lines of different nets that cross. A line to a plane is no line.
pub fn crossings(lines: &[Ratline]) -> usize {
    let real: Vec<&Ratline> = lines.iter().filter(|l| !l.to_plane).collect();
    let mut count = 0;
    for (k, a) in real.iter().enumerate() {
        for b in &real[k + 1..] {
            if a.net != b.net && segments_cross(a.from_at, a.to_at, b.from_at, b.to_at) {
                count += 1;
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_that_cross_inside_both_are_counted_and_touching_ones_are_not() {
        let p = |x, y| Pt::new(x, y);
        assert!(segments_cross(
            p(0.0, 0.0),
            p(2.0, 2.0),
            p(0.0, 2.0),
            p(2.0, 0.0)
        ));
        assert!(!segments_cross(
            p(0.0, 0.0),
            p(1.0, 1.0),
            p(0.0, 2.0),
            p(2.0, 0.0)
        ));
        // Sharing an end is not crossing.
        assert!(!segments_cross(
            p(0.0, 0.0),
            p(2.0, 2.0),
            p(2.0, 2.0),
            p(4.0, 0.0)
        ));
    }

    #[test]
    fn a_provider_of_capacitor_is_a_capacitor_whatever_its_name() {
        assert!(is_capacitor_kind("Capacitor", &[]));
        assert!(is_capacitor_kind("AcmeCeramic", &["Capacitor".to_string()]));
        assert!(!is_capacitor_kind("AcmeCeramic", &[]));
        assert!(!is_capacitor_kind("AcmeCeramic", &["Inductor".to_string()]));
    }

    #[test]
    fn instance_paths_share_their_parents() {
        assert_eq!(shared_segments("stage.cin", "stage.reg"), 1);
        assert_eq!(shared_segments("stage.cin", "other.reg"), 0);
        assert_eq!(shared_segments("a.b.c", "a.b.d"), 2);
        assert_eq!(shared_segments("c1", "u1"), 0);
    }
}
