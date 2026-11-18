//! How good a placement is, measured on the board and not on the placer's own cost, and the
//! decoupling pairs the placer derives from the netlist (ADR 0033).
//!
//! A decoupling pair is a capacitor whose two pads are on two different nets, both of which
//! carry a `power` pin of one same part: the part's supply and its return. Nothing in the
//! source says so; the netlist and the pin kinds do. The capacitor belongs next to those
//! pins.
//!
//! A part whose pin table does not call its supply pin `power` (a module, an imported part,
//! a connector with passive pins) is found from the nets too: a capacitor between two supply
//! nets (nets that a `power` pin of some part is on, whichever part) decouples an IC or a
//! connector that has a pin of any kind on the rail ([`decoupling_pairs`]).

use std::collections::HashMap;

use crate::board::{Board, BoardNet, Placed};
use crate::geometry::Pt;
use crate::ratsnest::{Ratline, Ratsnest};

/// One net of a decoupling pair: the capacitor's pad on it and the power pins it should be
/// near.
#[derive(Debug, Clone, PartialEq)]
pub struct PairNet {
    /// The number of the capacitor's pad on this net.
    pub cap_pad: String,
    /// The pads of the part's pins on this net, as (part index, pad number): the candidates
    /// the capacitor is pulled to, the nearest one counting. The `power` pins of a part that
    /// has them on both nets; the pins of any kind of a part that is found from the nets. A
    /// part found from the nets may have none on the return net.
    pub pins: Vec<(usize, String)>,
}

/// A capacitor and the part it decouples.
#[derive(Debug, Clone, PartialEq)]
pub struct DecouplingPair {
    /// The capacitor's index in the board's parts.
    pub cap: usize,
    pub nets: [PairNet; 2],
}

/// A capacitor by its kind (ADR 0125), whatever its own name.
fn is_capacitor(part: &Placed) -> bool {
    part.declared_kind == Some(netsmith_core::PartKind::Capacitor)
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

/// Whether a part that is none of the part kinds the placer knows to hang on a rail
/// (`None` is a part whose definitions state no kind) may be decoupled by a pin of any kind:
/// an IC or a connector. A resistor, a diode or an LED on a rail is not decoupled by its pin.
fn decoupled_by_any_pin(part: &Placed) -> bool {
    matches!(
        part.declared_kind,
        None | Some(netsmith_core::PartKind::Ic | netsmith_core::PartKind::Connector)
    )
}

/// The decoupling pairs of a board: every two-pad capacitor between two nets that one same
/// other part has power pins on, and, for a capacitor between two supply nets (nets with a
/// `power` pin of some part on them), every IC or connector (or part with no kind) that has a
/// pin of any kind on the capacitor's rail, the net of the two with fewer pads (the return is
/// the one with more, as the router takes it, the first on a tie). Of several such parts, the
/// ones that share the most of the capacitor's instance path are kept (the capacitor of a
/// module belongs to the module's own part), then all of them. Parts are in board order, so
/// the result does not depend on anything else.
pub fn decoupling_pairs(parts: &[Placed], nets: &[BoardNet]) -> Vec<DecouplingPair> {
    let index: HashMap<&str, usize> = parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.refdes.as_str(), i))
        .collect();
    // The net of each pad, the power pads of each net and the pads of every part but a
    // capacitor, whatever their kind.
    let mut net_of: HashMap<(usize, &str), usize> = HashMap::new();
    let mut power: Vec<Vec<(usize, String)>> = vec![Vec::new(); nets.len()];
    let mut any: Vec<Vec<(usize, String)>> = vec![Vec::new(); nets.len()];
    for (n, net) in nets.iter().enumerate() {
        for pad in &net.pads {
            let Some(&i) = index.get(pad.refdes.as_str()) else {
                continue;
            };
            net_of.insert((i, pad.number.as_str()), n);
            if is_capacitor(&parts[i]) {
                continue;
            }
            any[n].push((i, pad.number.clone()));
            if parts[i].power_pads.contains(&pad.number) {
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
        // The parts found from the nets: a pin of any kind on the rail of a capacitor between
        // two supply nets.
        let mut by_net: Vec<usize> = Vec::new();
        if !power[n1].is_empty() && !power[n2].is_empty() {
            let rail = if nets[n2].pads.len() > nets[n1].pads.len() {
                n1
            } else {
                n2
            };
            by_net = any[rail]
                .iter()
                .map(|&(i, _)| i)
                .filter(|&i| decoupled_by_any_pin(&parts[i]) && !targets.contains(&i))
                .collect();
            by_net.sort_unstable();
            by_net.dedup();
        }
        let mut all = targets.clone();
        all.extend(&by_net);
        if all.is_empty() {
            continue;
        }
        let best = all
            .iter()
            .map(|&i| shared_segments(&cap.path, &parts[i].path))
            .max()
            .unwrap_or(0);
        all.retain(|&i| shared_segments(&cap.path, &parts[i].path) == best);
        let side = |pad: &str, net: usize| PairNet {
            cap_pad: pad.to_string(),
            pins: power[net]
                .iter()
                .filter(|(i, _)| targets.contains(i) && all.contains(i))
                .chain(
                    any[net]
                        .iter()
                        .filter(|(i, _)| by_net.contains(i) && all.contains(i)),
                )
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

/// The terminations of a board's diff pairs (issue #1193), each as a [`DecouplingPair`] whose
/// `cap` is the termination: every part with two pads, that is no connector, one pad on each net
/// of a pair (`pairs`, [`Board::diff_pairs`]), with the pads of the connectors on those nets as
/// the pins it belongs next to. A termination matches the line's impedance at its end, where the
/// line leaves the board, so the placer pulls it to the connector as it pulls a decoupling
/// capacitor to its part (a `std::bus::CanTermination`'s resistor to the bus's connector). A
/// pair with no connector on both its nets has none. Parts are in board order.
pub fn termination_pairs(
    parts: &[Placed],
    nets: &[BoardNet],
    pairs: &[(String, String)],
) -> Vec<DecouplingPair> {
    if pairs.is_empty() {
        return Vec::new();
    }
    let connector = |part: &Placed| part.declared_kind == Some(netsmith_core::PartKind::Connector);
    let index: HashMap<&str, usize> = parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.refdes.as_str(), i))
        .collect();
    let net_index: HashMap<&str, usize> = nets
        .iter()
        .enumerate()
        .map(|(n, net)| (net.name.as_str(), n))
        .collect();
    let mut net_of: HashMap<(usize, &str), usize> = HashMap::new();
    let mut connectors: Vec<Vec<(usize, String)>> = vec![Vec::new(); nets.len()];
    for (n, net) in nets.iter().enumerate() {
        for pad in &net.pads {
            let Some(&i) = index.get(pad.refdes.as_str()) else {
                continue;
            };
            net_of.insert((i, pad.number.as_str()), n);
            if connector(&parts[i]) {
                connectors[n].push((i, pad.number.clone()));
            }
        }
    }
    let paired: Vec<(usize, usize)> = pairs
        .iter()
        .filter_map(|(p, n)| Some((*net_index.get(p.as_str())?, *net_index.get(n.as_str())?)))
        .collect();
    let mut found = Vec::new();
    for (t, part) in parts.iter().enumerate() {
        if connector(part) {
            continue;
        }
        let numbers = part.footprint.pad_numbers();
        let [first, second] = numbers.as_slice() else {
            continue;
        };
        let (Some(&n1), Some(&n2)) = (net_of.get(&(t, *first)), net_of.get(&(t, *second))) else {
            continue;
        };
        let across = paired
            .iter()
            .any(|&(a, b)| (a, b) == (n1, n2) || (b, a) == (n1, n2));
        // The connectors with a pad on each of the two nets.
        let both: Vec<usize> = connectors[n1]
            .iter()
            .map(|&(i, _)| i)
            .filter(|i| connectors[n2].iter().any(|(j, _)| j == i))
            .collect();
        if !across || both.is_empty() {
            continue;
        }
        let side = |pad: &str, net: usize| PairNet {
            cap_pad: pad.to_string(),
            pins: connectors[net]
                .iter()
                .filter(|(i, _)| both.contains(i))
                .cloned()
                .collect(),
        };
        found.push(DecouplingPair {
            cap: t,
            nets: [side(first, n1), side(second, n2)],
        });
    }
    found
}

/// How far a decoupling capacitor is from the part it decouples.
#[derive(Debug, Clone, PartialEq)]
pub struct DecouplingDistance {
    /// The capacitor's reference designator.
    pub cap: String,
    /// The reference designator of the part with the nearest power pins.
    pub part: String,
    /// The mean, over the capacitor's two pads, of the distance from the pad's centre to the
    /// centre of the nearest pin pad of the part on the same net, in mm (of the one pad that
    /// has a pin of the part on its net, when the other has none).
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
                let mut sides = 0.0;
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
                    // A part found from the nets may have no pin on the return net.
                    if nearest < f64::MAX {
                        total += nearest;
                        sides += 1.0;
                    }
                }
                DecouplingDistance {
                    cap: cap.refdes.clone(),
                    part,
                    distance: total / f64::max(sides, 1.0),
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
pub fn segments_cross(p: Pt, q: Pt, r: Pt, s: Pt) -> bool {
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
    fn instance_paths_share_their_parents() {
        assert_eq!(shared_segments("stage.cin", "stage.reg"), 1);
        assert_eq!(shared_segments("stage.cin", "other.reg"), 0);
        assert_eq!(shared_segments("a.b.c", "a.b.d"), 2);
        assert_eq!(shared_segments("c1", "u1"), 0);
    }
}
