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
//!
//! Copper joins where it touches: a track's end on a pad or a track over a pad, two tracks of one layer whose copper
//! meets (end to end, or an end on the other's middle, as a hand-routed board has them), and a
//! via on a pad or on a track. A net a routes file locks ([`crate::routes`]) is joined by its
//! pours on the outer layers too: the router adds nothing to it, so the pour is how its pads
//! meet, and a pour joins the pads, tracks and vias of its net on its layer, as the design rules
//! check takes it (its fill is not computed). Other nets are not: the router routes them to
//! every pad as before.

use crate::board::{
    Board, BoardNet, BoardPour, CopperLayer, FixedCopper, PadRef, Placed, Plane, Track, Via,
};
use crate::geometry::{Pt, Rect};

/// Copper this close to a pad, or to other copper, touches it.
const TOUCH: f64 = 1e-6;

/// One line still to be routed.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
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
pub struct Node {
    pub pad: PadRef,
    pub centre: Pt,
    pub bounds: Rect,
    pub front: bool,
    pub back: bool,
}

/// The pads a net reaches. A net that names a pad number reaches every pad of that
/// footprint with the number, and KiCad does not count them as joined to each other, so
/// each is a node of its own.
pub fn nodes_of(parts: &[Placed], net: &BoardNet) -> Vec<Node> {
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

/// Whether the copper of two tracks of one layer meets: an end of one lies on the other's
/// copper (a track that ends on another's middle), or their centre lines cross.
fn tracks_touch(a: &Track, b: &Track) -> bool {
    let reach = (a.width + b.width) / 2.0 + TOUCH;
    let ends_on = |x: &Track, y: &Track| [x.from, x.to].iter().any(|&e| y.distance_to(e) <= reach);
    if ends_on(a, b) || ends_on(b, a) {
        return true;
    }
    let (pa, pb) = (a.centre_line(), b.centre_line());
    pa.windows(2)
        .any(|p| pb.windows(2).any(|q| crosses(p[0], p[1], q[0], q[1])))
}

/// Whether segment `a`-`b` crosses segment `c`-`d` (their insides meet at one point).
fn crosses(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    let side = |p: Pt, q: Pt, r: Pt| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2) = (side(a, b, c), side(a, b, d));
    let (d3, d4) = (side(c, d, a), side(c, d, b));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

fn on_pad(node: &Node, at: Pt, layer: Option<CopperLayer>) -> bool {
    let on_layer = match layer {
        Some(CopperLayer::Front) => node.front,
        Some(CopperLayer::Back) => node.back,
        None => node.front || node.back,
    };
    on_layer && node.bounds.expanded(TOUCH).contains(&Rect::new(at, at))
}

/// Whether the track's centre line runs through the pad (its box) on the track's layer: a
/// track drawn over a pad of its net, as a hand-routed board has it, joins the pad.
fn runs_over(node: &Node, track: &Track) -> bool {
    let on_layer = match track.layer {
        CopperLayer::Front => node.front,
        CopperLayer::Back => node.back,
    };
    let b = node.bounds.expanded(TOUCH);
    on_layer
        && track
            .centre_line()
            .windows(2)
            .any(|w| segment_meets_box(w[0], w[1], &b))
}

/// Whether segment `a`-`b` has a point inside box `r` (Liang and Barsky's clipping).
fn segment_meets_box(a: Pt, b: Pt, r: &Rect) -> bool {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-dx, a.x - r.min.x),
        (dx, r.max.x - a.x),
        (-dy, a.y - r.min.y),
        (dy, r.max.y - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                lo = lo.max(t);
            } else {
                hi = hi.min(t);
            }
        }
    }
    lo <= hi
}

impl Ratsnest {
    /// The ratsnest of a board as it is placed and routed. Nets are taken in board order.
    pub fn of(board: &Board) -> Ratsnest {
        Self::from_parts(
            &board.parts,
            &board.nets,
            (&board.planes, &board.pours),
            &board.tracks,
            &board.vias,
            &board.fixed,
        )
    }

    pub(crate) fn from_parts(
        parts: &[Placed],
        nets: &[BoardNet],
        (planes, pours): (&[Plane], &[BoardPour]),
        tracks: &[Track],
        vias: &[Via],
        fixed: &FixedCopper,
    ) -> Ratsnest {
        let mut lines = Vec::new();
        for net in nets {
            let nodes = nodes_of(parts, net);
            let tracks: Vec<&Track> = tracks.iter().filter(|t| t.net == net.name).collect();
            let vias: Vec<&Via> = vias.iter().filter(|v| v.net == net.name).collect();
            let plane = planes.iter().any(|p| p.net == net.name);
            // A locked net's pours join it (the module's header).
            let poured: Vec<CopperLayer> = if fixed.is_locked(&net.name) {
                pours
                    .iter()
                    .filter(|p| p.net == net.name)
                    .map(|p| p.layer)
                    .collect()
            } else {
                Vec::new()
            };
            lines.extend(spanning_tree(
                &net.name, &nodes, &tracks, &vias, plane, &poured,
            ));
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
    poured: &[CopperLayer],
) -> Vec<Ratline> {
    let (n, t) = (nodes.len(), tracks.len());
    // Items 0..n are pads, then tracks, then vias, then the net's plane, then its pours on the
    // front and the back.
    let plane_item = n + t + vias.len();
    let pour_item = |layer: CopperLayer| plane_item + 1 + usize::from(layer == CopperLayer::Back);
    let mut sets = Sets::new(plane_item + 3);
    for (ti, track) in tracks.iter().enumerate() {
        for (ni, node) in nodes.iter().enumerate() {
            let touches = [track.from, track.to]
                .iter()
                .any(|&end| on_pad(node, end, Some(track.layer)))
                || runs_over(node, track);
            if touches {
                sets.join(ni, n + ti);
            }
        }
        for (tj, other) in tracks.iter().enumerate().skip(ti + 1) {
            let meet = track.layer == other.layer
                && ([track.from, track.to]
                    .iter()
                    .any(|&a| [other.from, other.to].iter().any(|&b| near(a, b)))
                    || tracks_touch(track, other));
            if meet {
                sets.join(n + ti, n + tj);
            }
        }
        if poured.contains(&track.layer) {
            sets.join(n + ti, pour_item(track.layer));
        }
    }
    for (vi, via) in vias.iter().enumerate() {
        for (ni, node) in nodes.iter().enumerate() {
            if on_pad(node, via.at, None) {
                sets.join(ni, n + t + vi);
            }
        }
        for (ti, track) in tracks.iter().enumerate() {
            if near(via.at, track.from)
                || near(via.at, track.to)
                || track.distance_to(via.at) <= (track.width + via.diameter) / 2.0 + TOUCH
            {
                sets.join(n + t + vi, n + ti);
            }
        }
        for &layer in poured {
            sets.join(n + t + vi, pour_item(layer));
        }
    }
    for (ni, node) in nodes.iter().enumerate() {
        for &layer in poured {
            let on = match layer {
                CopperLayer::Front => node.front,
                CopperLayer::Back => node.back,
            };
            if on {
                sets.join(ni, pour_item(layer));
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

    use crate::board::{CopperLayer, Track, Via};
    use crate::geometry::Pt;

    use super::*;

    #[test]
    fn the_led_board_has_one_line_per_two_pad_net() {
        let board = crate::board::tests::led_board();
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
            modules: Vec::new(),
            name: "T".into(),
            process: Some(crate::rules::test_process()),
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts: (1..=4)
                .map(|i| Part {
                    provides: Vec::new(),
                    declared_kind: netsmith_testkit::std_kind("Resistor")
                        .and_then(netsmith_core::PartKind::from_word),
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
        let mut board = crate::board::tests::unplaced_board(&design);
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
            modules: Vec::new(),
            name: "T".into(),
            process: Some(crate::rules::test_process()),
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts: vec![
                Part {
                    provides: Vec::new(),
                    declared_kind: netsmith_testkit::std_kind("LinearRegulator")
                        .and_then(netsmith_core::PartKind::from_word),
                    refdes: "U1".into(),
                    kind: "LinearRegulator".into(),
                    value: "LDO".into(),
                    package: Some("SOT-223".into()),
                    path: String::new(),
                    pins: Vec::new(),
                },
                Part {
                    provides: Vec::new(),
                    declared_kind: netsmith_testkit::std_kind("Resistor")
                        .and_then(netsmith_core::PartKind::from_word),
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
        let mut board = crate::board::tests::unplaced_board(&design);
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
        let mut board = crate::board::tests::led_board();
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
            mid: None,
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
        let mut board = crate::board::tests::led_board();
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
            mid: None,
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
        let board = crate::board::tests::four_layer_board();
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
        assert_eq!(Ratsnest::of(&crate::board::tests::led_board()).len(), 3);
    }

    #[test]
    fn a_via_beside_the_pad_with_a_track_to_it_joins_the_pad_to_the_plane() {
        let mut board = crate::board::tests::four_layer_board();
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
            mid: None,
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
        let board = crate::board::tests::four_layer_board();
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
        let mut board = crate::board::tests::four_layer_board();
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
            modules: Vec::new(),
            name: "T".into(),
            process: Some(crate::rules::test_process()),
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts: vec![Part {
                provides: Vec::new(),
                declared_kind: netsmith_testkit::std_kind("Resistor")
                    .and_then(netsmith_core::PartKind::from_word),
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
        let board = crate::board::tests::unplaced_board(&design);
        assert!(Ratsnest::of(&board).is_empty());
    }

    #[test]
    fn the_ratsnest_is_the_same_every_time() {
        let board = crate::board::tests::led_board();
        assert_eq!(Ratsnest::of(&board), Ratsnest::of(&board));
    }

    /// The two pads' centres of the LED board's net `net`.
    fn pad_centres(board: &Board, net: &str) -> (Pt, Pt) {
        let n = board.nets.iter().find(|n| n.name == net).unwrap();
        let nodes = nodes_of(&board.parts, n);
        (nodes[0].centre, nodes[1].centre)
    }

    #[test]
    fn a_track_that_ends_on_the_middle_of_another_joins_it() {
        let mut board = crate::board::tests::led_board();
        let (a, b) = pad_centres(&board, "VCC");
        // From the first pad to a point beyond the second, off it, and from the second pad to
        // the middle of that track.
        let off = Pt::new(b.x, b.y + 3.0);
        board
            .tracks
            .push(Track::straight("VCC", CopperLayer::Front, a, off, 0.2));
        let middle = Pt::new((a.x + off.x) / 2.0, (a.y + off.y) / 2.0);
        board
            .tracks
            .push(Track::straight("VCC", CopperLayer::Front, b, middle, 0.2));
        assert!(Ratsnest::of(&board).lines.iter().all(|l| l.net != "VCC"));
        // On the other layer it does not.
        board.tracks[1].layer = CopperLayer::Back;
        assert!(Ratsnest::of(&board).lines.iter().any(|l| l.net == "VCC"));
    }

    #[test]
    fn a_track_that_runs_over_a_pad_of_its_net_joins_it() {
        let mut board = crate::board::tests::led_board();
        let (a, b) = pad_centres(&board, "VCC");
        // From beyond the first pad, through it, to the second: KiCad counts the pad joined,
        // as a hand-routed board has it.
        let before = Pt::new(a.x - (b.x - a.x), a.y - (b.y - a.y));
        board
            .tracks
            .push(Track::straight("VCC", CopperLayer::Front, before, b, 0.2));
        assert!(Ratsnest::of(&board).lines.iter().all(|l| l.net != "VCC"));
        // On the other layer it does not.
        board.tracks[0].layer = CopperLayer::Back;
        assert!(Ratsnest::of(&board).lines.iter().any(|l| l.net == "VCC"));
    }

    #[test]
    fn a_locked_nets_pour_joins_its_pads_on_the_pours_layer() {
        let mut board = crate::board::tests::led_board();
        board.pours.push(BoardPour {
            layer: CopperLayer::Front,
            net: "GND".into(),
            stitch_pitch: None,
        });
        // Not locked: the router routes it, as before.
        assert!(Ratsnest::of(&board).lines.iter().any(|l| l.net == "GND"));
        board.fixed.locked.insert("GND".into());
        assert!(Ratsnest::of(&board).lines.iter().all(|l| l.net != "GND"));
        // A pour on the back does not reach the pads on the front.
        board.pours[0].layer = CopperLayer::Back;
        assert!(Ratsnest::of(&board).lines.iter().any(|l| l.net == "GND"));
    }
}
