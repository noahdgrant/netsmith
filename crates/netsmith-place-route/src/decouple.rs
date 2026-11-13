//! The lines of a decoupled power pin (issue #431, S7): the pin runs to its capacitor first.
//!
//! The ratsnest joins a net's pads by the shortest spanning tree, so an IC's power pin may be
//! joined to the rail by a line to a neighbour pin or another part, and its capacitor hang off
//! somewhere else: the current the pin draws then does not pass the capacitor. For each
//! decoupling pair the placer derives ([`crate::quality::decoupling_pairs`]), the router takes
//! the capacitor's supply pad and the nearest power pin it decouples and makes the line between
//! them part of the tree, and the pin a leaf: no other line of the net ends on it, so the rest
//! of the net joins at the capacitor (pin, capacitor, rail). On a net poured as a plane, the
//! pin's line to the plane becomes the line to the capacitor, whose own via is the pin's way to
//! the plane. Only the lines change, not the search.
//!
//! Of the pair's two nets, the return is left as the ratsnest has it: the net with a pour or a
//! plane, and if both or neither has one, the net with more pads (ground is the net most pads
//! are on). The capacitor's pad on the return gets its own short via to the pour
//! ([`return_pads`], [`crate::route`]'s pass after a complete try). A net with copper on it
//! already is left as it is.

use std::collections::{HashMap, HashSet};

use netsmith_pcb::board::{Board, PadRef};
use netsmith_pcb::ratsnest::{Node, Ratline, nodes_of};

use crate::quality::{DecouplingPair, decoupling_pairs};

/// The farthest a capacitor's pad is from the power pin it decouples for the line between them
/// to be made first, in mm: a capacitor farther off is a bulk capacitor of the rail, and its
/// line would be longer than the via or the neighbour it replaces.
const NEAR: f64 = 5.0;

/// Sets that merge.
struct Sets(Vec<usize>);

impl Sets {
    fn find(&mut self, mut i: usize) -> usize {
        while self.0[i] != i {
            self.0[i] = self.0[self.0[i]];
            i = self.0[i];
        }
        i
    }

    fn join(&mut self, a: usize, b: usize) -> bool {
        let (a, b) = (self.find(a), self.find(b));
        if a == b {
            return false;
        }
        self.0[a.max(b)] = a.min(b);
        true
    }
}

/// Whether a net is joined by a plane or poured on an outer layer.
fn poured(board: &Board, net: &str) -> bool {
    board.planes.iter().any(|p| p.net == net) || board.pours.iter().any(|p| p.net == net)
}

/// Which of a pair's two nets is its return: see the module header.
fn return_side(board: &Board, nets: [usize; 2]) -> usize {
    let [a, b] = nets.map(|n| &board.nets[n]);
    match (poured(board, &a.name), poured(board, &b.name)) {
        (true, false) => 0,
        (false, true) => 1,
        _ => usize::from(b.pads.len() > a.pads.len()),
    }
}

/// The net index of each pad of the board, by (refdes, number).
fn net_of_pads(board: &Board) -> HashMap<(&str, &str), usize> {
    let mut net_of = HashMap::new();
    for (n, net) in board.nets.iter().enumerate() {
        for pad in &net.pads {
            net_of.insert((pad.refdes.as_str(), pad.number.as_str()), n);
        }
    }
    net_of
}

/// Whether a line joins a decoupling capacitor's pad to a power pin it decouples, on either of
/// the pair's nets: a line of the router's decoupling class ([`crate::route`]'s `Class`).
pub(crate) fn decouples(board: &Board, pairs: &[DecouplingPair], l: &Ratline) -> bool {
    pairs.iter().any(|pair| {
        let cap = &board.parts[pair.cap].refdes;
        pair.nets.iter().any(|n| {
            let ends = |a: &PadRef, b: &PadRef| {
                &a.refdes == cap
                    && a.number == n.cap_pad
                    && n.pins.iter().any(|(part, pad)| {
                        board.parts[*part].refdes == b.refdes && *pad == b.number
                    })
            };
            ends(&l.from, &l.to) || ends(&l.to, &l.from)
        })
    })
}

/// The capacitor's pad on the return net of each decoupling pair, with that net's name.
pub(crate) fn return_pads(board: &Board) -> Vec<(PadRef, String)> {
    let net_of = net_of_pads(board);
    decoupling_pairs(&board.parts, &board.nets)
        .into_iter()
        .filter_map(|pair| {
            let refdes = board.parts[pair.cap].refdes.as_str();
            let nets = pair
                .nets
                .clone()
                .map(|side| net_of.get(&(refdes, side.cap_pad.as_str())).copied());
            let [Some(a), Some(b)] = nets else {
                return None;
            };
            let r = return_side(board, [a, b]);
            Some((
                PadRef {
                    refdes: refdes.to_string(),
                    number: pair.nets[r].cap_pad.clone(),
                },
                board.nets[[a, b][r]].name.clone(),
            ))
        })
        .collect()
}

/// A line to the pour from each decoupling capacitor's pad on its return net, where that net
/// is poured on an outer layer the pad is not on and has no plane: on a two-layer board with
/// ground poured on the back, a front capacitor's ground pad reaches the pour through a via of
/// its own beside it. The router routes these after a complete try only, and keeps one only if
/// it is short ([`crate::route`]'s `to_the_pours`); they are never in its orders.
pub(crate) fn return_lines(board: &Board) -> Vec<Ratline> {
    let mut out = Vec::new();
    for (pad, net) in return_pads(board) {
        if board.planes.iter().any(|p| p.net == net) {
            continue;
        }
        let pours: Vec<_> = board.pours.iter().filter(|p| p.net == net).collect();
        if pours.is_empty() {
            continue;
        }
        let Some(n) = board.nets.iter().find(|n| n.name == net) else {
            continue;
        };
        for node in nodes_of(&board.parts, n).iter().filter(|n| n.pad == pad) {
            let on_pour = pours.iter().any(|p| match p.layer {
                netsmith_pcb::board::CopperLayer::Front => node.front,
                netsmith_pcb::board::CopperLayer::Back => node.back,
            });
            if on_pour {
                continue;
            }
            out.push(Ratline {
                net: net.clone(),
                from: node.pad.clone(),
                to: node.pad.clone(),
                from_at: node.centre,
                to_at: node.centre,
                to_plane: true,
            });
        }
    }
    out
}

/// The lines the router routes: `lines` (the board's ratsnest, net by net in board order) with
/// the lines of each net that has a decoupled power pin made again so that the pin runs to its
/// capacitor first (see the module header).
pub(crate) fn lines(board: &Board, lines: Vec<Ratline>) -> Vec<Ratline> {
    let net_of = net_of_pads(board);
    // For each supply net, the (capacitor pad, power pins) of each pair on it.
    let mut wanted: HashMap<usize, Vec<(PadRef, Vec<PadRef>)>> = HashMap::new();
    for pair in decoupling_pairs(&board.parts, &board.nets) {
        let refdes = &board.parts[pair.cap].refdes;
        let nets = pair.nets.clone().map(|side| {
            net_of
                .get(&(refdes.as_str(), side.cap_pad.as_str()))
                .copied()
        });
        let [Some(a), Some(b)] = nets else {
            continue;
        };
        let supply = 1 - return_side(board, [a, b]);
        let side = &pair.nets[supply];
        let pins = side
            .pins
            .iter()
            .map(|(i, number)| PadRef {
                refdes: board.parts[*i].refdes.clone(),
                number: number.clone(),
            })
            .collect();
        let cap = PadRef {
            refdes: refdes.clone(),
            number: side.cap_pad.clone(),
        };
        wanted.entry([a, b][supply]).or_default().push((cap, pins));
    }
    if wanted.is_empty() {
        return lines;
    }
    let mut remade: HashMap<&str, Vec<Ratline>> = HashMap::new();
    for (&n, caps) in &wanted {
        let net = &board.nets[n];
        let copper = board.tracks.iter().any(|t| t.net == net.name)
            || board.vias.iter().any(|v| v.net == net.name);
        if copper {
            continue;
        }
        let plane = board.planes.iter().any(|p| p.net == net.name);
        let own: Vec<Ratline> = lines
            .iter()
            .filter(|l| l.net == net.name)
            .cloned()
            .collect();
        remade.insert(
            net.name.as_str(),
            tree(&net.name, &nodes_of(&board.parts, net), caps, plane, own),
        );
    }
    // Each remade net's lines take the place of its old ones, in board order.
    let mut out = Vec::with_capacity(lines.len());
    let mut done: HashSet<String> = HashSet::new();
    for line in lines {
        match remade.get(line.net.as_str()) {
            None => out.push(line),
            Some(new) => {
                if done.insert(line.net.clone()) {
                    out.extend(new.iter().cloned());
                }
            }
        }
    }
    out
}

fn line(net: &str, a: &Node, b: &Node) -> Ratline {
    Ratline {
        net: net.to_string(),
        from: a.pad.clone(),
        to: b.pad.clone(),
        from_at: a.centre,
        to_at: b.centre,
        to_plane: false,
    }
}

/// One net's lines with each capacitor's pad joined to its nearest power pin first, then the
/// shortest spanning tree over the rest. `own` is what the ratsnest gave the net, kept for a
/// plane net's lines to the plane.
fn tree(
    net: &str,
    nodes: &[Node],
    caps: &[(PadRef, Vec<PadRef>)],
    plane: bool,
    own: Vec<Ratline>,
) -> Vec<Ratline> {
    // The forced lines, as (pin node, capacitor node).
    let mut forced: Vec<(usize, usize)> = Vec::new();
    for (cap, pins) in caps {
        let Some(c) = nodes.iter().position(|n| &n.pad == cap) else {
            continue;
        };
        let at = nodes[c].centre;
        let nearest = (0..nodes.len())
            .filter(|&k| pins.contains(&nodes[k].pad))
            .min_by(|&a, &b| {
                at.distance(nodes[a].centre)
                    .total_cmp(&at.distance(nodes[b].centre))
                    .then(a.cmp(&b))
            });
        // On a plane net a through-hole pin is on the plane where it stands.
        if let Some(p) = nearest
            .filter(|&p| !(plane && nodes[p].front && nodes[p].back))
            .filter(|&p| at.distance(nodes[p].centre) <= NEAR)
        {
            forced.push((p, c));
        }
    }
    if forced.is_empty() {
        return own;
    }
    let mut sets = Sets((0..nodes.len()).collect());
    if plane {
        // Each pin's line to the plane becomes its line to the capacitor; the capacitor's own
        // line to the plane (or its through-hole pad) joins them both.
        let pins: HashSet<usize> = forced.iter().map(|&(p, _)| p).collect();
        let mut out: Vec<Ratline> = own
            .into_iter()
            .filter(|l| !(l.to_plane && pins.iter().any(|&p| l.from == nodes[p].pad)))
            .collect();
        for &(p, c) in &forced {
            if sets.join(p, c) {
                out.push(line(net, &nodes[p], &nodes[c]));
            }
        }
        return out;
    }
    let mut out = Vec::new();
    for &(p, c) in &forced {
        if sets.join(p, c) {
            out.push(line(net, &nodes[p], &nodes[c]));
        }
    }
    // Kruskal over the rest, as the ratsnest does it.
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for i in 0..nodes.len() {
        for j in i + 1..nodes.len() {
            pairs.push((nodes[i].centre.distance(nodes[j].centre), i, j));
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    for (_, i, j) in pairs {
        if sets.join(i, j) {
            out.push(line(net, &nodes[i], &nodes[j]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use netsmith_core::{Design, Net, Part, PartPin, PinKind, PinRef};
    use netsmith_pcb::board::{Board, BoardPour, CopperLayer};
    use netsmith_pcb::geometry::Pt;
    use netsmith_pcb::ratsnest::Ratsnest;

    use super::*;

    fn part(refdes: &str, kind: &str, package: &str, pins: Vec<PartPin>) -> Part {
        Part {
            provides: Vec::new(),
            refdes: refdes.into(),
            kind: kind.into(),
            value: "1".into(),
            package: Some(package.into()),
            path: format!("ldo.{refdes}"),
            pins,
        }
    }

    fn power(name: &str, pad: &str) -> PartPin {
        PartPin {
            name: name.into(),
            datasheet: name.to_uppercase(),
            kind: PinKind::Power,
            pads: vec![pad.into()],
            function: None,
            power_flow: None,
        }
    }

    fn net(name: &str, pins: &[(&str, &str)]) -> Net {
        Net {
            name: name.into(),
            pins: pins
                .iter()
                .map(|(r, p)| PinRef {
                    refdes: (*r).into(),
                    pin: (*p).into(),
                })
                .collect(),
        }
    }

    /// A regulator U1 whose input pin (pad 3) is decoupled by C1, and the rail `IN` going on
    /// to R1, which stands nearer the pin than the capacitor does; ground (`GND`, poured on the
    /// back) joins U1, C1 and R1 too.
    pub(crate) fn decoupled_board() -> Board {
        let design = Design {
            modules: Vec::new(),
            name: "T".into(),
            process: Some(crate::testlib::test_process()),
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts: vec![
                part(
                    "U1",
                    "LinearRegulator",
                    "SOT-23",
                    vec![power("vin", "3"), power("gnd", "1")],
                ),
                part("C1", "Capacitor", "0603", Vec::new()),
                part("R1", "Resistor", "0603", Vec::new()),
                part("R2", "Resistor", "0603", Vec::new()),
            ],
            nets: vec![
                net("IN", &[("U1", "3"), ("C1", "1"), ("R1", "1")]),
                net("GND", &[("U1", "1"), ("C1", "2"), ("R1", "2"), ("R2", "2")]),
                net("OUT", &[("U1", "2"), ("R2", "1")]),
            ],
        };
        let mut board = crate::board_from_design(
            &design,
            &netsmith_kicad::Mapping::standard(),
            &crate::testlib::fixtures(),
        )
        .unwrap();
        let place = |board: &mut Board, refdes: &str, at: Pt, rotation: f64| {
            let p = board.parts.iter_mut().find(|p| p.refdes == refdes).unwrap();
            p.at = at;
            p.rotation = rotation;
        };
        let o = board.outline.min;
        board.outline = netsmith_pcb::geometry::Rect::new(o, Pt::new(o.x + 24.0, o.y + 20.0));
        place(&mut board, "U1", Pt::new(o.x + 10.0, o.y + 10.0), 0.0);
        place(&mut board, "R1", Pt::new(o.x + 10.0, o.y + 7.0), 0.0);
        place(&mut board, "C1", Pt::new(o.x + 14.0, o.y + 10.0), 90.0);
        place(&mut board, "R2", Pt::new(o.x + 10.0, o.y + 14.0), 0.0);
        board.pours.push(BoardPour {
            layer: CopperLayer::Back,
            net: "GND".into(),
            stitch_pitch: None,
        });
        board
    }

    fn ends(l: &Ratline) -> [String; 2] {
        [&l.from, &l.to].map(|p| format!("{}.{}", p.refdes, p.number))
    }

    #[test]
    fn a_decoupled_power_pin_has_a_line_to_its_capacitor_even_with_a_nearer_pad_on_the_rail() {
        let board = decoupled_board();
        let of_in = |lines: &[Ratline]| -> Vec<[String; 2]> {
            lines.iter().filter(|l| l.net == "IN").map(ends).collect()
        };
        // The spanning tree alone joins the pin to the resistor, which is nearer.
        let plain = Ratsnest::of(&board).lines;
        assert!(
            of_in(&plain)
                .iter()
                .any(|e| e.contains(&"U1.3".to_string()) && e.contains(&"R1.1".to_string())),
            "{:?}",
            of_in(&plain)
        );
        let lines = lines(&board, plain.clone());
        let mine = of_in(&lines);
        assert_eq!(mine.len(), 2, "{mine:?}");
        assert!(
            mine.contains(&["U1.3".to_string(), "C1.1".to_string()]),
            "{mine:?}"
        );
        // The return net, which is poured, keeps the ratsnest's lines.
        let gnd = |l: &[Ratline]| -> Vec<[String; 2]> {
            l.iter().filter(|l| l.net == "GND").map(ends).collect()
        };
        assert_eq!(gnd(&lines), gnd(&plain));
        assert_eq!(
            return_pads(&board),
            [(
                PadRef {
                    refdes: "C1".into(),
                    number: "2".into()
                },
                "GND".to_string()
            )]
        );
    }

    #[test]
    fn the_capacitor_is_routed_from_its_pin_and_its_ground_pad_gets_a_short_via_to_the_pour() {
        let mut board = decoupled_board();
        let routing = crate::route::route_with(&mut board, crate::route::RouteOptions::default());
        let left = routing.left;
        assert!(left.is_empty(), "{:?}", left.lines);
        let centre = |refdes: &str, pad: &str| {
            let part = board.part(refdes).unwrap();
            part.pad_centre(part.footprint.pads_numbered(pad).next().unwrap())
        };
        let touches = |net: &str, p: Pt| {
            board.tracks.iter().any(|t| {
                t.net == net
                    && t.layer == CopperLayer::Front
                    && (t.from.distance(p) < 1e-6 || t.to.distance(p) < 1e-6)
            })
        };
        // The pin's track starts at the pin and the capacitor's pad.
        assert!(touches("IN", centre("U1", "3")) && touches("IN", centre("C1", "1")));
        // The capacitor's ground pad has a via of its own beside it, joined by a short track.
        let pad = centre("C1", "2");
        let via = board
            .vias
            .iter()
            .find(|v| v.net == "GND" && v.at.distance(pad) <= RETURN_NEAR)
            .unwrap_or_else(|| panic!("no GND via beside C1.2: {:?}", board.vias));
        assert!(touches("GND", via.at), "{via:?}");
    }

    /// How near the test wants the capacitor's via: the router's bound with a cell over.
    const RETURN_NEAR: f64 = 1.6;
}
