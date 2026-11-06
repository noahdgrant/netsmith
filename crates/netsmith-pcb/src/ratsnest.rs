//! The ratsnest: the connections that are still to be routed.
//!
//! Each net's pads are joined by a minimum spanning tree of straight lines between pad
//! centres, which is the shortest set of lines that leaves no pad cut off. Pads that
//! routed copper already joins count as one, so a fully routed board has no ratsnest.
//!
//! A net that is poured as a plane on an inner layer is joined by the plane, so its pads
//! need no lines between them. A through-hole pad and a via of the net reach the plane
//! where they stand; any other pad needs a via to it, and has a line to the plane until
//! one is dropped.

use crate::board::{Board, BoardNet, CopperLayer, PadRef, Placed, Plane, Track, Via};
use crate::geometry::{Pt, Rect};

/// Copper this close to a pad, or to other copper, touches it.
const TOUCH: f64 = 1e-6;

/// One line still to be routed.
#[derive(Debug, Clone, PartialEq)]
pub struct Ratline {
    pub net: String,
    pub from: PadRef,
    pub to: PadRef,
    pub from_at: Pt,
    pub to_at: Pt,
    /// The line is from a pad to its net's plane, which is not a place on the board: `to`
    /// is `from` and the line has no length.
    pub to_plane: bool,
}

impl std::fmt::Display for Ratline {
    /// `net GND: D1.1 to J1.2`, or `net GND: D1.1 to the plane`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.to_plane {
            write!(f, "net {}: {} to the plane", self.net, self.from)
        } else {
            write!(f, "net {}: {} to {}", self.net, self.from, self.to)
        }
    }
}

impl Ratline {
    pub fn length(&self) -> f64 {
        self.from_at.distance(self.to_at)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ratsnest {
    pub lines: Vec<Ratline>,
}

/// One pad that a net reaches.
pub(crate) struct Node {
    pub pad: PadRef,
    pub centre: Pt,
    pub bounds: Rect,
    pub front: bool,
    pub back: bool,
}

/// The pads a net reaches. A net that names a pad number reaches every pad of that
/// footprint with the number, and KiCad does not count them as joined to each other, so
/// each is a node of its own.
pub(crate) fn nodes_of(parts: &[Placed], net: &BoardNet) -> Vec<Node> {
    let mut nodes = Vec::new();
    for pad in &net.pads {
        let Some(part) = parts.iter().find(|p| p.refdes == pad.refdes) else {
            continue;
        };
        nodes.extend(part.footprint.pads_numbered(&pad.number).map(|p| {
            let (front, back) = part.pad_layers(p);
            Node {
                pad: pad.clone(),
                centre: part.pad_centre(p),
                bounds: part.pad_bounds(p),
                front,
                back,
            }
        }));
    }
    nodes
}

/// Sets that merge, as a vector of parents.
struct Sets(Vec<usize>);

impl Sets {
    fn new(n: usize) -> Self {
        Self((0..n).collect())
    }

    fn find(&mut self, i: usize) -> usize {
        let mut root = i;
        while self.0[root] != root {
            root = self.0[root];
        }
        let mut at = i;
        while self.0[at] != root {
            at = std::mem::replace(&mut self.0[at], root);
        }
        root
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

fn near(a: Pt, b: Pt) -> bool {
    a.distance(b) < TOUCH
}

fn on_pad(node: &Node, at: Pt, layer: Option<CopperLayer>) -> bool {
    let on_layer = match layer {
        Some(CopperLayer::Front) => node.front,
        Some(CopperLayer::Back) => node.back,
        None => node.front || node.back,
    };
    on_layer && node.bounds.expanded(TOUCH).contains(&Rect::new(at, at))
}

impl Ratsnest {
    /// The ratsnest of a board as it is placed and routed. Nets are taken in board order.
    pub fn of(board: &Board) -> Ratsnest {
        Self::from_parts(
            &board.parts,
            &board.nets,
            &board.planes,
            &board.tracks,
            &board.vias,
        )
    }

    pub(crate) fn from_parts(
        parts: &[Placed],
        nets: &[BoardNet],
        planes: &[Plane],
        tracks: &[Track],
        vias: &[Via],
    ) -> Ratsnest {
        let mut lines = Vec::new();
        for net in nets {
            let nodes = nodes_of(parts, net);
            let tracks: Vec<&Track> = tracks.iter().filter(|t| t.net == net.name).collect();
            let vias: Vec<&Via> = vias.iter().filter(|v| v.net == net.name).collect();
            let plane = planes.iter().any(|p| p.net == net.name);
            lines.extend(spanning_tree(&net.name, &nodes, &tracks, &vias, plane));
        }
        Ratsnest { lines }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The sum of every line's length, in mm.
    pub fn total_length(&self) -> f64 {
        self.lines
            .iter()
            .map(Ratline::length)
            .fold(0.0, |a, b| a + b)
    }
}

/// Kruskal's algorithm, starting with everything the routed copper already joins. Of equal
/// lines, the one between the pads listed first wins.
fn spanning_tree(
    net: &str,
    nodes: &[Node],
    tracks: &[&Track],
    vias: &[&Via],
    plane: bool,
) -> Vec<Ratline> {
    let (n, t) = (nodes.len(), tracks.len());
    // Items 0..n are pads, then tracks, then vias, then the net's plane.
    let plane_item = n + t + vias.len();
    let mut sets = Sets::new(plane_item + 1);
    for (ti, track) in tracks.iter().enumerate() {
        for (ni, node) in nodes.iter().enumerate() {
            let touches = [track.from, track.to]
                .iter()
                .any(|&end| on_pad(node, end, Some(track.layer)));
            if touches {
                sets.join(ni, n + ti);
            }
        }
        for (tj, other) in tracks.iter().enumerate().skip(ti + 1) {
            let meet = track.layer == other.layer
                && [track.from, track.to]
                    .iter()
                    .any(|&a| [other.from, other.to].iter().any(|&b| near(a, b)));
            if meet {
                sets.join(n + ti, n + tj);
            }
        }
    }
    for (vi, via) in vias.iter().enumerate() {
        for (ni, node) in nodes.iter().enumerate() {
            if on_pad(node, via.at, None) {
                sets.join(ni, n + t + vi);
            }
        }
        for (ti, track) in tracks.iter().enumerate() {
            if near(via.at, track.from) || near(via.at, track.to) {
                sets.join(n + t + vi, n + ti);
            }
        }
    }

    if plane {
        // A through-hole pad has copper on every layer, and so has a via.
        for (ni, node) in nodes.iter().enumerate() {
            if node.front && node.back {
                sets.join(ni, plane_item);
            }
        }
        for vi in 0..vias.len() {
            sets.join(n + t + vi, plane_item);
        }
        // The plane joins every pad that reaches it, so the pads that do not need a line
        // each, and none to one another.
        return nodes
            .iter()
            .enumerate()
            .filter(|&(i, _)| sets.join(i, plane_item))
            .map(|(_, node)| Ratline {
                net: net.to_string(),
                from: node.pad.clone(),
                to: node.pad.clone(),
                from_at: node.centre,
                to_at: node.centre,
                to_plane: true,
            })
            .collect();
    }

    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            pairs.push((nodes[i].centre.distance(nodes[j].centre), i, j));
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut lines = Vec::new();
    for (_, i, j) in pairs {
        if sets.join(i, j) {
            lines.push(Ratline {
                net: net.to_string(),
                from: nodes[i].pad.clone(),
                to: nodes[j].pad.clone(),
                from_at: nodes[i].centre,
                to_at: nodes[j].centre,
                to_plane: false,
            });
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use netsmith_core::{Design, Net, Part, PinRef};

    use crate::board::tests::{fixtures, four_layer_design, led_design};
    use crate::board::{CopperLayer, Track, Via};
    use crate::geometry::Pt;

    use super::*;

    #[test]
    fn the_led_board_has_one_line_per_two_pad_net() {
        let board = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let rats = Ratsnest::of(&board);
        assert_eq!(rats.len(), 3);
        let nets: Vec<&str> = rats.lines.iter().map(|l| l.net.as_str()).collect();
        assert_eq!(nets, ["GND", "VCC", "net-led.anode"]);
        let direct: f64 = board
            .nets
            .iter()
            .map(|n| {
                let a = &n.pads[0];
                let b = &n.pads[1];
                let at = |p: &PadRef| {
                    let part = board.part(&p.refdes).unwrap();
                    part.pad_centre(part.footprint.pads_numbered(&p.number).next().unwrap())
                };
                at(a).distance(at(b))
            })
            .sum();
        assert!((rats.total_length() - direct).abs() < 1e-9);
        assert!(rats.total_length() > 0.0);
    }

    #[test]
    fn a_net_of_n_pads_needs_n_minus_one_lines_and_they_are_the_shortest() {
        // Four resistors in a row on one net: the tree is the chain of neighbours.
        let design = Design {
            name: "T".into(),
            process: Some(crate::rules::test_process()),
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts: (1..=4)
                .map(|i| Part {
                    refdes: format!("R{i}"),
                    kind: "Resistor".into(),
                    value: "1k".into(),
                    package: Some("0402".into()),
                    path: String::new(),
                    pins: Vec::new(),
                })
                .collect(),
            nets: vec![Net {
                name: "N".into(),
                pins: (1..=4)
                    .map(|i| PinRef {
                        refdes: format!("R{i}"),
                        pin: "1".into(),
                    })
                    .collect(),
            }],
        };
        let mut board =
            Board::from_design(&design, &netsmith_kicad::Mapping::standard(), &fixtures()).unwrap();
        for (i, p) in board.parts.iter_mut().enumerate() {
            p.at = Pt::new(i as f64 * 10.0, 0.0);
            p.rotation = 0.0;
        }
        let rats = Ratsnest::of(&board);
        assert_eq!(rats.len(), 3);
        assert!((rats.total_length() - 30.0).abs() < 1e-9);
    }

    #[test]
    fn pads_that_share_a_number_are_each_a_node() {
        // The regulator's tab and pin 2 are both pad "2". KiCad leaves them unjoined, so a
        // net of that pad number and one resistor needs two lines: the resistor to the
        // nearer of the two, and that one to the other.
        let design = Design {
            name: "T".into(),
            process: Some(crate::rules::test_process()),
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts: vec![
                Part {
                    refdes: "U1".into(),
                    kind: "LinearRegulator".into(),
                    value: "LDO".into(),
                    package: Some("SOT-223".into()),
                    path: String::new(),
                    pins: Vec::new(),
                },
                Part {
                    refdes: "R1".into(),
                    kind: "Resistor".into(),
                    value: "1k".into(),
                    package: Some("0402".into()),
                    path: String::new(),
                    pins: Vec::new(),
                },
            ],
            nets: vec![Net {
                name: "N".into(),
                pins: vec![
                    PinRef {
                        refdes: "U1".into(),
                        pin: "2".into(),
                    },
                    PinRef {
                        refdes: "R1".into(),
                        pin: "1".into(),
                    },
                ],
            }],
        };
        let mut board =
            Board::from_design(&design, &netsmith_kicad::Mapping::standard(), &fixtures()).unwrap();
        board.parts[0].at = Pt::new(0.0, 0.0);
        board.parts[1].at = Pt::new(10.0, 0.0);
        board.parts[0].rotation = 0.0;
        board.parts[1].rotation = 0.0;
        let rats = Ratsnest::of(&board);
        assert_eq!(rats.len(), 2);
        // The pad and the tab are 6.3 mm apart, and the tab is the nearer to R1.
        let expected = 6.3 + (10.0 - 0.51 - 3.15);
        assert!((rats.total_length() - expected).abs() < 1e-9);
    }

    #[test]
    fn copper_that_joins_two_pads_removes_their_line() {
        let mut board = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        assert_eq!(Ratsnest::of(&board).len(), 3);
        let ends = |board: &Board, refdes: &str, pad: &str| {
            let part = board.part(refdes).unwrap();
            part.pad_centre(part.footprint.pads_numbered(pad).next().unwrap())
        };
        // R1.1 to J1.1, on the front: the VCC net is done, the others are not.
        board.tracks.push(Track {
            net: "VCC".into(),
            layer: CopperLayer::Front,
            from: ends(&board, "R1", "1"),
            to: ends(&board, "J1", "1"),
            width: 0.2,
        });
        let rats = Ratsnest::of(&board);
        assert_eq!(rats.len(), 2);
        assert!(rats.lines.iter().all(|l| l.net != "VCC"));
        // The same copper on the back misses the surface mount pad.
        board.tracks[0].layer = CopperLayer::Back;
        assert_eq!(Ratsnest::of(&board).len(), 3);
    }

    #[test]
    fn a_via_joins_copper_on_both_layers() {
        let mut board = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let part = board.part("J1").unwrap();
        let j1_1 = part.pad_centre(part.footprint.pads_numbered("1").next().unwrap());
        let part = board.part("R1").unwrap();
        let r1_1 = part.pad_centre(part.footprint.pads_numbered("1").next().unwrap());
        let via_at = Pt::new(j1_1.x + 4.0, j1_1.y);
        let track = |layer, from, to| Track {
            net: "VCC".into(),
            layer,
            from,
            to,
            width: 0.2,
        };
        // Back copper from the header's through-hole pad to a via, then front copper on to
        // the resistor.
        board.tracks.push(track(CopperLayer::Back, j1_1, via_at));
        board.tracks.push(track(CopperLayer::Front, via_at, r1_1));
        assert_eq!(Ratsnest::of(&board).len(), 3, "no via yet, so no join");
        board.vias.push(Via {
            net: "VCC".into(),
            at: via_at,
            diameter: 0.6,
            drill: 0.3,
        });
        assert_eq!(Ratsnest::of(&board).len(), 2);
    }

    fn pad_at(board: &Board, refdes: &str, pad: &str) -> Pt {
        let part = board.part(refdes).unwrap();
        part.pad_centre(part.footprint.pads_numbered(pad).next().unwrap())
    }

    #[test]
    fn a_plane_joins_its_nets_through_hole_pads_and_leaves_each_surface_mount_pad_a_line() {
        // GND is D1.1 (surface mount) and J1.2 (through hole); 3V3 is J1.1 and R1.1.
        let board = Board::from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let rats = Ratsnest::of(&board);
        let summary: Vec<(&str, String, bool)> = rats
            .lines
            .iter()
            .map(|l| (l.net.as_str(), l.from.to_string(), l.to_plane))
            .collect();
        assert_eq!(
            summary,
            [
                ("GND", "D1.1".into(), true),
                ("3V3", "R1.1".into(), true),
                ("net-led.anode", "D1.2".into(), false),
            ]
        );
        let plane_line = &rats.lines[0];
        assert_eq!(plane_line.from, plane_line.to);
        assert_eq!(plane_line.length(), 0.0);
        // The same nets on two layers want a line between pads.
        assert_eq!(
            Ratsnest::of(
                &Board::from_design(
                    &led_design(),
                    &netsmith_kicad::Mapping::standard(),
                    &fixtures()
                )
                .unwrap()
            )
            .len(),
            3
        );
    }

    #[test]
    fn a_via_beside_the_pad_with_a_track_to_it_joins_the_pad_to_the_plane() {
        let mut board = Board::from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let d1 = pad_at(&board, "D1", "1");
        let via_at = Pt::new(d1.x, d1.y + 1.5);
        let via = |net: &str, at| Via {
            net: net.into(),
            at,
            diameter: 0.45,
            drill: 0.15,
        };
        // A via of the net alone is on the plane but not on the pad.
        board.vias.push(via("GND", via_at));
        assert_eq!(Ratsnest::of(&board).len(), 3);
        board.tracks.push(Track {
            net: "GND".into(),
            layer: CopperLayer::Front,
            from: d1,
            to: via_at,
            width: 0.09,
        });
        let rats = Ratsnest::of(&board);
        assert_eq!(rats.len(), 2);
        assert!(rats.lines.iter().all(|l| l.net != "GND"));
        // A via of another net does not reach this plane.
        board.tracks.clear();
        board.vias[0].net = "net-led.anode".into();
        assert_eq!(Ratsnest::of(&board).len(), 3);
    }

    #[test]
    fn a_line_says_what_it_joins() {
        let board = Board::from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let lines: Vec<String> = Ratsnest::of(&board)
            .lines
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            lines,
            [
                "net GND: D1.1 to the plane",
                "net 3V3: R1.1 to the plane",
                "net net-led.anode: D1.2 to R1.2"
            ]
        );
    }

    #[test]
    fn a_via_on_the_pad_joins_it_to_the_plane() {
        let mut board = Board::from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let r1 = pad_at(&board, "R1", "1");
        board.vias.push(Via {
            net: "3V3".into(),
            at: r1,
            diameter: 0.45,
            drill: 0.15,
        });
        assert!(Ratsnest::of(&board).lines.iter().all(|l| l.net != "3V3"));
    }

    #[test]
    fn a_net_of_one_pad_has_no_lines() {
        let design = Design {
            name: "T".into(),
            process: Some(crate::rules::test_process()),
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts: vec![Part {
                refdes: "R1".into(),
                kind: "Resistor".into(),
                value: "1k".into(),
                package: Some("0402".into()),
                path: String::new(),
                pins: Vec::new(),
            }],
            nets: vec![Net {
                name: "N".into(),
                pins: vec![PinRef {
                    refdes: "R1".into(),
                    pin: "1".into(),
                }],
            }],
        };
        let board =
            Board::from_design(&design, &netsmith_kicad::Mapping::standard(), &fixtures()).unwrap();
        assert!(Ratsnest::of(&board).is_empty());
    }

    #[test]
    fn the_ratsnest_is_the_same_every_time() {
        let board = Board::from_design(
            &led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        assert_eq!(Ratsnest::of(&board), Ratsnest::of(&board));
    }
}
