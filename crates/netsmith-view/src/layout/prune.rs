//! The last step of drawing a sheet's wires: take out what no pin needs.
//!
//! Routing a net adds wire in steps (a rail from its symbol to the farthest pin it may serve,
//! a path to the tree built so far, a stub lengthened to make room for a name), and a later
//! step can leave an earlier one without a use: a rail that goes on past the last wire that
//! hangs from it, a path that ends where the tree no longer is, two wires laid along each
//! other. This takes them out, by net, on the grid's own edges:
//!
//! 1. A wire edge that another wire of the net already has is dropped (a double line).
//! 2. A loop of wire is opened: where two ways join the same two places, the longer is taken
//!    out (a net is drawn as a tree, and a small loop is a hook).
//! 3. A wire end that touches nothing is dropped, edge by edge, until every end is at an
//!    anchor (a pin, a power symbol, a flag, a label of the net) or on another wire of the
//!    net. What is left of a wire that is cut is as many wires as it has runs.
//! 4. A dot is put where three or more arms of a net meet: a T, a crossing of its own wires,
//!    three ends, two wires and a pin.
//!
//! What stays connected stays so: only ends that touch nothing are taken, never a wire between
//! two things.

use std::collections::{BTreeMap, BTreeSet};

use super::grid::{self, Grid, Node};
use super::{Junction, Wire};

type Edge = (Node, Node);

fn edge(a: Node, b: Node) -> Edge {
    if a <= b { (a, b) } else { (b, a) }
}

/// The nodes along a wire, one grid step apart. A wire that is not along the grid's rows and
/// columns is kept whole by the caller.
fn walk(grid: &Grid, w: &Wire) -> Option<Vec<Node>> {
    let nodes: Vec<Node> = w.points.iter().map(|&p| grid.node(p)).collect();
    let mut out: Vec<Node> = Vec::new();
    for pair in nodes.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if a.0 != b.0 && a.1 != b.1 {
            return None;
        }
        let (dx, dy) = ((b.0 - a.0).signum(), (b.1 - a.1).signum());
        let mut k = a;
        if out.last() != Some(&a) {
            out.push(a);
        }
        while k != b {
            k = (k.0 + dx, k.1 + dy);
            out.push(k);
        }
    }
    Some(out)
}

/// `wires` without the double lines and the ends that touch nothing, and the junction dots
/// of what is left. `anchors` are the `(net, node)` places where a wire may end: the pins of
/// the net, its power symbols, flags and labels.
pub(super) fn prune(
    grid: &Grid,
    wires: Vec<Wire>,
    anchors: &BTreeSet<(usize, Node)>,
) -> (Vec<Wire>, Vec<Junction>) {
    // Each wire as its nodes; a wire off the grid is passed through as it is.
    let walked: Vec<Option<Vec<Node>>> = wires.iter().map(|w| walk(grid, w)).collect();

    // 1. Each edge of a net belongs to the first wire that has it.
    let mut owner: BTreeMap<(usize, Edge), usize> = BTreeMap::new();
    for (i, (w, nodes)) in wires.iter().zip(&walked).enumerate() {
        if let Some(nodes) = nodes {
            for pair in nodes.windows(2) {
                owner.entry((w.net, edge(pair[0], pair[1]))).or_insert(i);
            }
        }
    }

    // 2. Open the loops, then take the ends that touch nothing, edge by edge.
    let mut near: BTreeMap<usize, BTreeMap<Node, BTreeSet<Node>>> = BTreeMap::new();
    for &(net, (a, b)) in owner.keys() {
        let n = near.entry(net).or_default();
        n.entry(a).or_default().insert(b);
        n.entry(b).or_default().insert(a);
    }
    let mut gone: BTreeSet<(usize, Edge)> = BTreeSet::new();
    for (&net, graph) in &mut near {
        open_loops(net, graph, anchors, &mut gone);
        let mut loose: Vec<Node> = graph
            .iter()
            .filter(|(n, to)| to.len() == 1 && !anchors.contains(&(net, **n)))
            .map(|(n, _)| *n)
            .collect();
        while let Some(n) = loose.pop() {
            let Some(to) = graph.remove(&n) else { continue };
            for m in to {
                gone.insert((net, edge(n, m)));
                if let Some(back) = graph.get_mut(&m) {
                    back.remove(&n);
                    if back.len() == 1 && !anchors.contains(&(net, m)) {
                        loose.push(m);
                    }
                }
            }
        }
    }

    // What each wire keeps: its runs of edges that are its own and not gone.
    let mut out: Vec<Wire> = Vec::new();
    for (i, (w, nodes)) in wires.into_iter().zip(&walked).enumerate() {
        let Some(nodes) = nodes else {
            out.push(w);
            continue;
        };
        let mut run: Vec<Node> = Vec::new();
        let flush = |run: &mut Vec<Node>, out: &mut Vec<Wire>| {
            if run.len() >= 2 {
                out.push(Wire {
                    net: w.net,
                    points: grid::corners(run).into_iter().map(|n| grid.pt(n)).collect(),
                });
            }
            run.clear();
        };
        for pair in nodes.windows(2) {
            let key = (w.net, edge(pair[0], pair[1]));
            if owner.get(&key) == Some(&i) && !gone.contains(&key) {
                if run.is_empty() {
                    run.push(pair[0]);
                }
                run.push(pair[1]);
            } else {
                flush(&mut run, &mut out);
            }
        }
        flush(&mut run, &mut out);
    }

    // 3. A dot where three or more arms of a net meet.
    // The graph is ordered by net and node, so the dots are too.
    let mut junctions = Vec::new();
    for (&net, graph) in &near {
        for (&n, to) in graph {
            if to.len() + usize::from(anchors.contains(&(net, n))) >= 3 {
                junctions.push(Junction {
                    net,
                    at: grid.pt(n),
                });
            }
        }
    }
    (out, junctions)
}

/// A run of wire between two nodes where something meets (a pin, a symbol, a flag, a label, a
/// branch or an end), through nodes that only continue it.
struct Chain {
    nodes: Vec<Node>,
}

impl Chain {
    fn ends(&self) -> (Node, Node) {
        (self.nodes[0], self.nodes[self.nodes.len() - 1])
    }

    fn bends(&self) -> usize {
        self.nodes
            .windows(3)
            .filter(|w| (w[1].0 - w[0].0, w[1].1 - w[0].1) != (w[2].0 - w[1].0, w[2].1 - w[1].1))
            .count()
    }
}

fn chains(
    net: usize,
    graph: &BTreeMap<Node, BTreeSet<Node>>,
    anchors: &BTreeSet<(usize, Node)>,
) -> Vec<Chain> {
    let key = |n: Node| graph[&n].len() != 2 || anchors.contains(&(net, n));
    let mut used: BTreeSet<Edge> = BTreeSet::new();
    let mut out = Vec::new();
    for (&start, near) in graph {
        if !key(start) {
            continue;
        }
        for &first in near {
            if used.contains(&edge(start, first)) {
                continue;
            }
            let mut nodes = vec![start, first];
            used.insert(edge(start, first));
            while !key(*nodes.last().expect("a node")) {
                let at = nodes[nodes.len() - 1];
                let before = nodes[nodes.len() - 2];
                let next = *graph[&at].iter().find(|&&m| m != before).expect("a way on");
                used.insert(edge(at, next));
                nodes.push(next);
            }
            out.push(Chain { nodes });
        }
    }
    out
}

/// Takes out, chain by chain, the longest run of wire that is on a loop, until there is none.
fn open_loops(
    net: usize,
    graph: &mut BTreeMap<Node, BTreeSet<Node>>,
    anchors: &BTreeSet<(usize, Node)>,
    gone: &mut BTreeSet<(usize, Edge)>,
) {
    loop {
        let all = chains(net, graph, anchors);
        // A chain is on a loop when its ends are still joined without it.
        let on_loop = |i: usize| {
            let (a, b) = all[i].ends();
            let mut seen: BTreeSet<Node> = BTreeSet::from([a]);
            let mut todo = vec![a];
            while let Some(n) = todo.pop() {
                for (j, c) in all.iter().enumerate() {
                    if j == i {
                        continue;
                    }
                    let (p, q) = c.ends();
                    let other = if p == n {
                        q
                    } else if q == n {
                        p
                    } else {
                        continue;
                    };
                    if seen.insert(other) {
                        todo.push(other);
                    }
                }
            }
            seen.contains(&b)
        };
        let worst = (0..all.len()).filter(|&i| on_loop(i)).max_by_key(|&i| {
            (
                all[i].nodes.len(),
                all[i].bends(),
                std::cmp::Reverse(all[i].nodes[0]),
            )
        });
        let Some(i) = worst else { return };
        for pair in all[i].nodes.windows(2) {
            gone.insert((net, edge(pair[0], pair[1])));
            if let Some(a) = graph.get_mut(&pair[0]) {
                a.remove(&pair[1]);
            }
            if let Some(b) = graph.get_mut(&pair[1]) {
                b.remove(&pair[0]);
            }
        }
        graph.retain(|_, near| !near.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{Pt, Rect};

    fn grid() -> Grid {
        Grid::new(
            Rect {
                min: Pt::new(0.0, 0.0),
                max: Pt::new(50.0, 50.0),
            },
            1.27,
        )
    }

    fn wire(g: &Grid, net: usize, nodes: &[Node]) -> Wire {
        Wire {
            net,
            points: nodes.iter().map(|&n| g.pt(n)).collect(),
        }
    }

    fn anchors(list: &[(usize, Node)]) -> BTreeSet<(usize, Node)> {
        list.iter().copied().collect()
    }

    /// The nodes the wires of `net` cover, as the unit edges they are made of.
    fn edges(g: &Grid, wires: &[Wire], net: usize) -> BTreeSet<Edge> {
        let mut out = BTreeSet::new();
        for w in wires.iter().filter(|w| w.net == net) {
            let nodes = walk(g, w).expect("on the grid");
            for pair in nodes.windows(2) {
                out.insert(edge(pair[0], pair[1]));
            }
        }
        out
    }

    #[test]
    fn a_rail_that_goes_on_past_its_last_tap_is_cut_back_to_it() {
        // A rail from x=2 to x=12 on row 5, with its symbol at x=2 and a drop at x=8 to a
        // pin at (8, 9): the part of the rail past x=8 reaches nothing.
        let g = grid();
        let wires = vec![
            wire(&g, 0, &[(2, 5), (12, 5)]),
            wire(&g, 0, &[(8, 5), (8, 9)]),
        ];
        let (out, dots) = prune(&g, wires, &anchors(&[(0, (2, 5)), (0, (8, 9))]));
        let covered = edges(&g, &out, 0);
        assert!(!covered.contains(&edge((9, 5), (10, 5))), "{covered:?}");
        assert!(covered.contains(&edge((7, 5), (8, 5))));
        assert!(covered.contains(&edge((8, 8), (8, 9))));
        // The rail now ends where the drop leaves it: a corner, which has no dot.
        assert!(dots.is_empty(), "{dots:?}");
    }

    #[test]
    fn a_wire_laid_twice_is_laid_once() {
        let g = grid();
        let wires = vec![
            wire(&g, 0, &[(2, 5), (8, 5)]),
            wire(&g, 0, &[(4, 5), (8, 5)]),
        ];
        let (out, _) = prune(&g, wires, &anchors(&[(0, (2, 5)), (0, (8, 5))]));
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(edges(&g, &out, 0).len(), 6);
    }

    #[test]
    fn a_loop_is_opened_at_its_longest_way_round() {
        // Pins at (2, 5) and (6, 5), joined along row 5 and again by a way round through
        // row 9: the long way goes.
        let g = grid();
        let wires = vec![
            wire(&g, 0, &[(2, 5), (6, 5)]),
            wire(&g, 0, &[(2, 5), (2, 9), (6, 9), (6, 5)]),
        ];
        let (out, dots) = prune(&g, wires, &anchors(&[(0, (2, 5)), (0, (6, 5))]));
        let covered = edges(&g, &out, 0);
        assert_eq!(covered.len(), 4, "{covered:?}");
        assert!(covered.contains(&edge((3, 5), (4, 5))));
        assert!(dots.is_empty(), "{dots:?}");
    }

    #[test]
    fn a_small_loop_beside_a_bus_is_a_hook_and_goes() {
        // The bus along row 5 from x=2 to x=10; a pin at (4, 9) comes up to it at x=4 and a
        // second way, from (4, 7), goes right, up and joins the bus at x=5: a loop of one
        // step. One side of it is taken out, and nothing is left hanging.
        let g = grid();
        let wires = vec![
            wire(&g, 0, &[(2, 5), (10, 5)]),
            wire(&g, 0, &[(4, 9), (4, 5)]),
            wire(&g, 0, &[(4, 7), (5, 7), (5, 5)]),
        ];
        let (out, _) = prune(
            &g,
            wires,
            &anchors(&[(0, (2, 5)), (0, (10, 5)), (0, (4, 9))]),
        );
        let covered = edges(&g, &out, 0);
        // Every anchor is still joined: walk from (4, 9).
        let mut seen: BTreeSet<Node> = BTreeSet::from([(4, 9)]);
        let mut todo = vec![(4, 9)];
        while let Some(n) = todo.pop() {
            for &(a, b) in &covered {
                let other = if a == n {
                    b
                } else if b == n {
                    a
                } else {
                    continue;
                };
                if seen.insert(other) {
                    todo.push(other);
                }
            }
        }
        assert!(
            seen.contains(&(2, 5)) && seen.contains(&(10, 5)),
            "{covered:?}"
        );
        // As a tree: one edge fewer than nodes.
        let nodes: BTreeSet<Node> = covered.iter().flat_map(|&(a, b)| [a, b]).collect();
        assert_eq!(covered.len() + 1, nodes.len(), "{covered:?}");
    }

    #[test]
    fn two_wires_and_a_pin_at_one_point_have_a_dot_and_two_wires_alone_do_not() {
        let g = grid();
        let wires = vec![
            wire(&g, 0, &[(2, 5), (6, 5)]),
            wire(&g, 0, &[(6, 5), (6, 9)]),
        ];
        let (_, corner) = prune(&g, wires.clone(), &anchors(&[(0, (2, 5)), (0, (6, 9))]));
        assert!(corner.is_empty(), "a corner: {corner:?}");
        let wires = vec![
            wire(&g, 0, &[(2, 5), (6, 5)]),
            wire(&g, 0, &[(6, 5), (6, 9)]),
        ];
        let (_, at_pin) = prune(
            &g,
            wires,
            &anchors(&[(0, (2, 5)), (0, (6, 9)), (0, (6, 5))]),
        );
        assert_eq!(at_pin.len(), 1, "{at_pin:?}");
    }

    #[test]
    fn a_wire_that_touches_nothing_goes_whole() {
        let g = grid();
        let wires = vec![wire(&g, 1, &[(2, 5), (6, 5), (6, 9)])];
        let (out, dots) = prune(&g, wires, &anchors(&[]));
        assert!(out.is_empty() && dots.is_empty());
    }

    #[test]
    fn the_nets_are_pruned_each_on_its_own() {
        // Net 1 crosses net 0 at (4, 5); net 0's dead end at (6, 5) is cut, net 1's wire
        // stays between its two pins.
        let g = grid();
        let wires = vec![
            wire(&g, 0, &[(2, 5), (6, 5)]),
            wire(&g, 1, &[(4, 2), (4, 8)]),
        ];
        let (out, _) = prune(
            &g,
            wires,
            &anchors(&[(0, (2, 5)), (1, (4, 2)), (1, (4, 8))]),
        );
        assert!(edges(&g, &out, 0).is_empty());
        assert_eq!(edges(&g, &out, 1).len(), 6);
    }
}
