//! The defects of the router's own copper (issue #1486), counted on routed boards: a connection
//! drawn twice (a loop in a net's copper), a track in one place (a track with both ends in one pad or via of
//! its net), a stair step (a straight piece shorter than [`TINY`] that no pad or via holds), a
//! join of two collinear pieces left in two, and a T-join beside a via (a track that ends on
//! another of its net within [`NEAR_VIA`] of a via of the net but not in it). The router must
//! make none of them on the examples and on a generated board.

use netsmith_pcb::{Board, CopperLayer, Pt, Rect, Track};
use netsmith_router::route;

use crate::common;

/// A straight piece shorter than this, outside a pad or via, is a stair step.
const TINY: f64 = 0.14;
/// How near a via a T-join may be and still be one beside it.
const NEAR_VIA: f64 = 1.0;
const EPS: f64 = 1e-4;

/// The copper a track end may join: a pad (its bounds, its layers) or a via (round, all layers).
enum Anchor {
    Pad(Rect, bool, bool),
    Via(Pt, f64),
}

impl Anchor {
    fn holds(&self, p: Pt, layer: CopperLayer) -> bool {
        match self {
            Anchor::Pad(r, front, back) => {
                let on = match layer {
                    CopperLayer::Front => *front,
                    CopperLayer::Back => *back,
                    CopperLayer::Inner(_) => *front && *back,
                };
                on && p.x >= r.min.x - EPS
                    && p.x <= r.max.x + EPS
                    && p.y >= r.min.y - EPS
                    && p.y <= r.max.y + EPS
            }
            Anchor::Via(c, r) => c.distance(p) <= r + EPS,
        }
    }
}

/// Each kind of defect, counted over a board.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Defects {
    /// Loops in the copper of a net: each is a connection drawn a second way.
    pub loops: usize,
    pub in_one_place: usize,
    pub stair_steps: usize,
    pub unmerged: usize,
    pub t_joins_beside_vias: usize,
    /// Where each one is, for a person to look at.
    pub places: Vec<String>,
}

fn anchors(board: &Board, net: &str) -> Vec<Anchor> {
    let mut out = Vec::new();
    for part in &board.parts {
        for pad in &part.footprint.pads {
            if board
                .net_of(&part.refdes, &pad.number)
                .is_none_or(|n| n.name != net)
            {
                continue;
            }
            let (front, back) = part.pad_layers(pad);
            out.push(Anchor::Pad(part.pad_bounds(pad), front, back));
        }
    }
    for v in board.vias.iter().filter(|v| v.net == net) {
        out.push(Anchor::Via(v.at, v.diameter / 2.0));
    }
    out
}

fn on_interior(p: Pt, t: &Track) -> bool {
    if t.mid.is_some() {
        return false;
    }
    let (dx, dy) = (t.to.x - t.from.x, t.to.y - t.from.y);
    let len = dx.hypot(dy);
    if len < EPS {
        return false;
    }
    let along = ((p.x - t.from.x) * dx + (p.y - t.from.y) * dy) / len;
    let across = ((p.x - t.from.x) * dy - (p.y - t.from.y) * dx).abs() / len;
    across < 1e-3 && along > 1e-3 && along < len - 1e-3
}

fn find(parent: &mut [usize], mut a: usize) -> usize {
    while parent[a] != a {
        parent[a] = parent[parent[a]];
        a = parent[a];
    }
    a
}

/// Counts the defects of `board`'s copper.
pub fn count(board: &Board) -> Defects {
    let mut d = Defects::default();
    let mut nets: Vec<&str> = board.tracks.iter().map(|t| t.net.as_str()).collect();
    nets.sort_unstable();
    nets.dedup();
    for net in nets {
        let anchors = anchors(board, net);
        let tracks: Vec<&Track> = board.tracks.iter().filter(|t| t.net == net).collect();
        let anchor_of = |p: Pt, layer: CopperLayer| anchors.iter().position(|a| a.holds(p, layer));
        // The nodes: the anchors, then the free ends of each layer.
        let mut free: Vec<(CopperLayer, Pt)> = Vec::new();
        let node = |p: Pt, layer: CopperLayer, free: &mut Vec<(CopperLayer, Pt)>| -> usize {
            if let Some(a) = anchor_of(p, layer) {
                return a;
            }
            let k = free
                .iter()
                .position(|(l, q)| *l == layer && q.distance(p) < 1e-3)
                .unwrap_or_else(|| {
                    free.push((layer, p));
                    free.len() - 1
                });
            anchors.len() + k
        };
        let mut edges: Vec<(usize, usize, Pt, Pt)> = Vec::new();
        for (i, t) in tracks.iter().enumerate() {
            let len = t.from.distance(t.to);
            let a = anchor_of(t.from, t.layer);
            let b = anchor_of(t.to, t.layer);
            if t.mid.is_none() {
                if a.is_some() && a == b {
                    d.in_one_place += 1;
                    d.places.push(format!("in one place {net} {:?} {:?}->{:?}", t.layer, t.from, t.to));
                    continue;
                }
                // A lone short piece between two straight runs is one cut corner, not a stair.
                let lone_corner = [t.from, t.to].iter().all(|&end| {
                    let at: Vec<&&Track> = tracks
                        .iter()
                        .enumerate()
                        .filter(|(j, o)| {
                            *j != i
                                && o.layer == t.layer
                                && (o.from.distance(end) < 1e-3 || o.to.distance(end) < 1e-3)
                        })
                        .map(|(_, o)| o)
                        .collect();
                    matches!(at[..], [o] if o.mid.is_none() && o.from.distance(o.to) >= TINY)
                });
                if len < TINY && a.is_none() && b.is_none() && !lone_corner {
                    d.stair_steps += 1;
                    d.places.push(format!("step {net} {:?} {:?}->{:?}", t.layer, t.from, t.to));
                }
            }
            // The points of the other tracks that end on this one's interior split it.
            let mut cuts: Vec<Pt> = tracks
                .iter()
                .enumerate()
                .filter(|(j, o)| *j != i && o.layer == t.layer)
                .flat_map(|(_, o)| [o.from, o.to])
                .filter(|&p| on_interior(p, t))
                .collect();
            cuts.sort_by(|p, q| {
                t.from
                    .distance(*p)
                    .partial_cmp(&t.from.distance(*q))
                    .unwrap()
            });
            cuts.dedup_by(|p, q| p.distance(*q) < 1e-3);
            for &c in &cuts {
                if anchor_of(c, t.layer).is_none()
                    && board
                        .vias
                        .iter()
                        .any(|v| v.net == net && v.at.distance(c) < NEAR_VIA)
                {
                    d.t_joins_beside_vias += 1;
                }
            }
            let mut points = vec![t.from];
            points.extend(cuts);
            points.push(t.to);
            for w in points.windows(2) {
                let x = node(w[0], t.layer, &mut free);
                let y = node(w[1], t.layer, &mut free);
                if x != y {
                    edges.push((x, y, w[0], w[1]));
                }
            }
        }
        // Two collinear straight pieces of one width that meet at a free end and nowhere else.
        for (k, &(layer, p)) in free.iter().enumerate() {
            let at: Vec<&&Track> = tracks
                .iter()
                .filter(|t| t.layer == layer && (t.from.distance(p) < 1e-3 || t.to.distance(p) < 1e-3))
                .collect();
            let _ = k;
            if at.len() == 2 && at.iter().all(|t| t.mid.is_none()) && at[0].width == at[1].width {
                let dir = |t: &Track| {
                    let (a, b) = if t.from.distance(p) < 1e-3 { (t.from, t.to) } else { (t.to, t.from) };
                    let l = a.distance(b).max(EPS);
                    ((b.x - a.x) / l, (b.y - a.y) / l)
                };
                let (u, v) = (dir(at[0]), dir(at[1]));
                if u.0 * v.0 + u.1 * v.1 < -0.9999 {
                    d.unmerged += 1;
                }
            }
        }
        let n = anchors.len() + free.len();
        let mut parent: Vec<usize> = (0..n).collect();
        for (x, y, p, q) in edges {
            let (a, b) = (find(&mut parent, x), find(&mut parent, y));
            if a == b {
                d.loops += 1;
                d.places.push(format!("loop {net} {p:?}->{q:?} (nodes {x}/{y})"));
            } else {
                parent[a] = b;
            }
        }
    }
    d
}

fn show(name: &str, board: &Board) {
    let d = count(board);
    let length: f64 = board.tracks.iter().map(|t| t.length()).sum();
    println!(
        "{name}: {:?} left {} vias {} length {length:.2} tracks {}",
        Defects { places: Vec::new(), ..d.clone() },
        netsmith_pcb::Ratsnest::of(board).len(),
        board.vias.len(),
        board.tracks.len()
    );
    if std::env::var_os("DEFECTS_DUMP").is_some() {
        for p in &d.places {
            println!("  {p}");
            let net = p.split(' ').nth(1).unwrap_or("");
            for t in board.tracks.iter().filter(|t| t.net == net) {
                println!("    {:?} {:.4},{:.4} -> {:.4},{:.4} w{} {}", t.layer, t.from.x, t.from.y, t.to.x, t.to.y, t.width, if t.mid.is_some() {"arc"} else {""});
            }
            for v in board.vias.iter().filter(|v| v.net == net) {
                println!("    via {:.4},{:.4}", v.at.x, v.at.y);
            }
        }
    }
}

fn routed(mut board: Board) -> Board {
    route(&mut board);
    board
}

/// The examples and generated boards: none has a defect.
#[test]
fn the_router_leaves_no_defect_on_the_examples_and_generated_boards() {
    let mut boards: Vec<(String, Board)> = ["led", "status_indicator", "linear_regulator", "four_layer", "six_layer"]
        .into_iter()
        .map(|name| (name.to_string(), common::example_board(name, &common::fixtures())))
        .collect();
    boards.extend((1..=3).map(|seed| {
        (
            format!("generated board {seed}"),
            common::synthetic_board(seed, 10, 3, 8, 10, 4),
        )
    }));
    for (name, board) in boards {
        let board = routed(board);
        check_none(&name, &board, true);
    }
    // The seeded boards of crossed nets: two lines of a net may lie one on the other where taking
    // either off would leave an acute join, which `track_angle` refuses; that loop stays.
    for (k, board) in common::hard_boards(4).into_iter().enumerate() {
        check_none(&format!("crossed nets {k}"), &routed(board), false);
    }
}

/// That `board` is routed and has no defect, of loops too where `loops`.
fn check_none(name: &str, board: &Board, loops: bool) {
    assert!(netsmith_pcb::Ratsnest::of(board).is_empty(), "{name}");
    let d = count(board);
    assert_eq!(
        Defects {
            places: Vec::new(),
            loops: if loops { d.loops } else { 0 },
            ..d.clone()
        },
        Defects::default(),
        "{name}: {:#?}",
        d.places
    );
}

/// The counts on the examples and the generated boards, with the copper's length and vias,
/// and on a board of seventy parts; `DEFECTS_DUMP=1` prints each defect's net and copper.
#[test]
#[ignore = "prints the defects of the routed boards"]
fn measured() {
    for name in ["led", "status_indicator", "linear_regulator", "four_layer", "six_layer"] {
        let board = routed(common::example_board(name, &common::fixtures()));
        show(name, &board);
    }
    for (k, b) in common::hard_boards(4).into_iter().enumerate() {
        show(&format!("hard {k}"), &routed(b));
    }
    show("seventy", &routed(common::synthetic_board(7, 52, 17, 20, 72, 36)));
    for seed in 1..=3 {
        let b = common::synthetic_board(seed, 10, 3, 8, 10, 4);
        show(&format!("synthetic {seed}"), &routed(b));
    }
}

