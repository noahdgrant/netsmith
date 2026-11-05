//! Pair satellites: a two-terminal part whose two pins are on two different pins of one main
//! part, such as a crystal across `XL1` and `XL2`, a resonator, or a series element across two
//! pins. A person draws it beside the two pins it serves, turned so that its pins face them,
//! with two short wires of one bend each that cross nothing, and never writes a name on them.
//!
//! [`detect`] finds such parts from the netlist and where the pins are. [`place`] then tries
//! every orientation of the part at every place near the two pins, in a fixed order, and keeps
//! the one whose two wires are cheapest by the router's own cost (grid steps, bends, crossings)
//! on a scratch copy of the grid: the same router that draws the wires afterwards, so the
//! drawing keeps what the search found. A wire leaves a pin's tip in any direction (there is no
//! stub on the part's pins), so a wire that drops straight from a tip and turns once into the
//! other part's stub has one bend, as in a hand drawing. A place where the wires would cross
//! each other, or anything else, is not a candidate. Nothing is chosen by the first free spot.

use std::collections::{BTreeMap, HashSet};

use super::*;

/// Which pins of the main part a satellite serves: for each of the satellite's two pins (by
/// index in its glyph) the pin of the main part on the same net.
#[derive(Clone, Copy, Debug)]
pub(super) struct PairPlan {
    pub anchor: usize,
    pub links: [(usize, usize); 2],
}

/// A pair satellite whose place was found: what [`build_block`] and the scene need.
#[derive(Clone, Copy, Debug)]
pub(super) struct PairRoute {
    pub part: usize,
    pub anchor: usize,
    pub links: [(usize, usize); 2],
    /// The order the two wires are drawn in, as indices into `links`.
    pub order: [usize; 2],
}

pub(super) struct Placement {
    /// Where the satellite's origin is, from the main part's.
    pub off: Pt,
    pub orient: Orient,
    /// The side of its body that its reference and value are written on.
    pub side: Dir,
    /// What it covers, with its texts and a margin.
    pub foot: Rect,
    /// The strips its two wires run in: other parts keep out of them.
    pub lanes: Vec<Rect>,
    pub order: [usize; 2],
}

/// Whether `part` is a pair satellite of one of the anchors: two pins that face opposite ways,
/// on two different nets that are neither a rail nor leave the sheet, and a pin of one same
/// anchor on each net, on one side of it and no more than [`policy::PAIR_REACH_STEPS`] steps
/// apart. `anchor_pins` are the pins of the anchors by net, as `(part, pin index)`.
pub(super) fn detect(
    ctx: &Ctx,
    placed: &[PlacedPart],
    part: usize,
    anchor_pins: &BTreeMap<usize, Vec<(usize, usize)>>,
    skip: &[bool],
) -> Option<PairPlan> {
    let g = &ctx.glyphs[part];
    if g.symbol.is_none() || g.pins.len() != 2 || g.pins.iter().any(|p| p.hidden) {
        return None;
    }
    let nets: Vec<usize> = g
        .pins
        .iter()
        .map(|pin| net_of(ctx, part, &pin.id))
        .collect::<Option<_>>()?;
    if nets[0] == nets[1] {
        return None;
    }
    for &n in &nets {
        if ctx.power[n].is_some() || ctx.ground[n] || ctx.external[n] || skip[n] {
            return None;
        }
    }
    let mut best: Option<((i64, usize, usize, usize), PairPlan)> = None;
    for &(pa, ia) in anchor_pins.get(&nets[0])? {
        for &(pb, ib) in anchor_pins.get(&nets[1])? {
            if pa != pb || pa == part {
                continue;
            }
            let (qa, qb) = (&placed[pa].pins[ia], &placed[pb].pins[ib]);
            if qa.escape != qb.escape {
                continue;
            }
            let steps = ((qa.at.x - qb.at.x).abs() + (qa.at.y - qb.at.y).abs()) / GRID;
            if steps > policy::PAIR_REACH_STEPS {
                continue;
            }
            let key = ((steps * 100.0).round() as i64, pa, ia, ib);
            if best.as_ref().is_none_or(|(k, _)| key < *k) {
                best = Some((
                    key,
                    PairPlan {
                        anchor: pa,
                        links: [(0, ia), (1, ib)],
                    },
                ));
            }
        }
    }
    best.map(|(_, plan)| plan)
}

/// Cells the search looks, each way, from the middle of the two stubs.
const WINDOW_X: i32 = 40;
const WINDOW_Y: i32 = 34;
/// Room of the scratch grid past the window.
const SCRATCH_MARGIN: i32 = 16;
/// Candidates whose wires are routed, at most.
const ATTEMPTS: usize = 400;
/// What a bend of a pair satellite's wire costs on top of the router's own price.
const BEND_EXTRA: u32 = 60;
/// What a wire that meets another is charged, in the router's units: more than any detour.
const CLASH: u32 = 5000;

struct Candidate {
    lower: u32,
    orient: usize,
    cell: (i32, i32),
    side: Dir,
    foot: Rect,
}

fn nodes_of(from: Node, to: Node) -> Vec<Node> {
    let (dx, dy) = ((to.0 - from.0).signum(), (to.1 - from.1).signum());
    let mut out = vec![from];
    let mut k = from;
    while k != to {
        k = (k.0 + dx, k.1 + dy);
        out.push(k);
    }
    out
}

/// The cheapest place and turn for the pair satellite `part` of `plan` beside the main part
/// `anchor`, which stands at the origin of `placed`, with `obstacles` (boxes the satellite and
/// its texts must keep clear of) already there. `None` when no place has two wires that reach.
pub(super) fn place(
    ctx: &Ctx,
    placed: &[PlacedPart],
    plan: &PairPlan,
    part: usize,
    obstacles: &[Rect],
) -> Option<Placement> {
    let a = &placed[plan.anchor];
    let nets: [usize; 2] = [
        net_of(ctx, part, &ctx.glyphs[part].pins[0].id)?,
        net_of(ctx, part, &ctx.glyphs[part].pins[1].id)?,
    ];
    let pin_nets = [Some(nets[0]), Some(nets[1])];
    let (ia, ib) = (plan.links[0].1, plan.links[1].1);
    let side = a.pins[ia].escape;
    let stub_end = |i: usize| a.pins[i].at.step(side, STUB);
    let mid = Pt::new(
        snap_grid((stub_end(ia).x + stub_end(ib).x) / 2.0),
        snap_grid((stub_end(ia).y + stub_end(ib).y) / 2.0),
    );

    // Every orientation that draws the part differently, with its texts on each side.
    let mut turned: Vec<Vec<PlacedPart>> = Vec::new();
    let mut seen: Vec<Vec<(i64, i64, u8)>> = Vec::new();
    for o in Orient::ALL {
        let base = place_part(ctx, part, o, &pin_nets);
        let key: Vec<(i64, i64, u8)> = base
            .pins
            .iter()
            .map(|q| {
                (
                    (q.at.x * 100.0).round() as i64,
                    (q.at.y * 100.0).round() as i64,
                    q.escape as u8,
                )
            })
            .collect();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let mut sides = Vec::new();
        for d in [Dir::Up, Dir::Down, Dir::Left, Dir::Right] {
            let mut p = base.clone();
            set_text_side(ctx, &mut p, d);
            sides.push(p);
        }
        turned.push(sides);
    }
    let side_index = |d: Dir| {
        [Dir::Up, Dir::Down, Dir::Left, Dir::Right]
            .iter()
            .position(|&x| x == d)
            .unwrap_or(0)
    };

    // The scratch grid: the obstacles, and the stubs and crosses of the main part's other pins.
    let cells = |c: i32| f64::from(c) * GRID;
    let bounds = Rect {
        min: Pt::new(
            mid.x - cells(WINDOW_X + SCRATCH_MARGIN),
            mid.y - cells(WINDOW_Y + SCRATCH_MARGIN),
        ),
        max: Pt::new(
            mid.x + cells(WINDOW_X + SCRATCH_MARGIN),
            mid.y + cells(WINDOW_Y + SCRATCH_MARGIN),
        ),
    };
    let mut base = Grid::new(bounds, GRID);
    for r in obstacles {
        base.block_rect(*r);
    }
    for (i, (pin, g)) in a.pins.iter().zip(&a.glyph.pins).enumerate() {
        if g.hidden || i == ia || i == ib {
            continue;
        }
        let node = base.node(pin.at);
        if pin.net.is_none() {
            base.block_rect(inflate(Rect::around(pin.at), 1.5));
            continue;
        }
        let (dx, dy) = pin.escape.vector();
        for k in 0..=STUB_CELLS {
            base.block((node.0 + dx as i32 * k, node.1 + dy as i32 * k));
        }
    }
    // The stubs of the two pins, as they will be drawn.
    let mut stub_nodes: [Vec<Node>; 2] = [Vec::new(), Vec::new()];
    for (k, &(_, i)) in plan.links.iter().enumerate() {
        let node = base.node(a.pins[i].at);
        let end = lay_stub(&mut base, node, side, Scene::net_id(nets[k]));
        stub_nodes[k] = nodes_of(node, end);
    }
    // Where the stubs are is no place for the part.
    let own: Vec<Rect> = (0..2)
        .map(|k| {
            let mut r = Rect::around(base.pt(stub_nodes[k][0]));
            r.include(base.pt(*stub_nodes[k].last().expect("a stub")));
            inflate(r, 0.6)
        })
        .collect();
    // The wire may join a stub anywhere along it but at the pin's tip.
    let reach: [Vec<Pt>; 2] = [0, 1].map(|k| {
        stub_nodes[k]
            .iter()
            .skip(1)
            .map(|&n| base.pt(n))
            .collect::<Vec<Pt>>()
    });

    // Every place the part fits, nearest to its pins first.
    let mut found: Vec<Candidate> = Vec::new();
    for (oi, sides) in turned.iter().enumerate() {
        for ty in -WINDOW_Y..=WINDOW_Y {
            for tx in -WINDOW_X..=WINDOW_X {
                let origin = Pt::new(mid.x + cells(tx), mid.y + cells(ty));
                let pins = &sides[0].pins;
                let lower: f64 = (0..2)
                    .map(|k| {
                        let tip = Pt::new(pins[k].at.x + origin.x, pins[k].at.y + origin.y);
                        reach[k]
                            .iter()
                            .map(|q| ((tip.x - q.x).abs() + (tip.y - q.y).abs()) / GRID)
                            .fold(f64::INFINITY, f64::min)
                    })
                    .sum();
                if !lower.is_finite() {
                    continue;
                }
                // Sides, the one that leads away from the pins first.
                let (tx_, ty_) = (mid.x - origin.x, mid.y - origin.y);
                let mut order = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];
                order.sort_by(|p, q| {
                    let dot = |d: &Dir| d.vector().0 * tx_ + d.vector().1 * ty_;
                    dot(p).total_cmp(&dot(q))
                });
                let mut chosen: Option<(Dir, Rect)> = None;
                for d in order {
                    let foot = inflate(sides[side_index(d)].extent.translated(origin), MARGIN);
                    if obstacles.iter().any(|o| overlaps(&foot, o))
                        || own.iter().any(|o| overlaps(&foot, o))
                    {
                        continue;
                    }
                    chosen = Some((d, foot));
                    break;
                }
                if let Some((d, foot)) = chosen {
                    found.push(Candidate {
                        lower: (lower * f64::from(grid::STEP_COST)).round() as u32,
                        orient: oi,
                        cell: (tx, ty),
                        side: d,
                        foot,
                    });
                }
            }
        }
    }
    found.sort_by_key(|c| {
        (
            c.lower,
            c.cell.0.abs() + c.cell.1.abs(),
            c.orient,
            c.cell.1,
            c.cell.0,
        )
    });

    let mut best: Option<(u32, Placement)> = None;
    for cand in found.iter().take(ATTEMPTS) {
        if best.as_ref().is_some_and(|(cost, _)| cand.lower >= *cost) {
            break;
        }
        let origin = Pt::new(mid.x + cells(cand.cell.0), mid.y + cells(cand.cell.1));
        let mut sat = turned[cand.orient][side_index(cand.side)].clone();
        translate_part(&mut sat, origin);
        let mut grid = base.clone();
        block_part(&mut grid, &sat);
        let pin_nodes: [Node; 2] = [grid.node(sat.pins[0].at), grid.node(sat.pins[1].at)];
        for k in 0..2 {
            grid.unblock(pin_nodes[k]);
            grid.claim_wire(pin_nodes[k], pin_nodes[k], Scene::net_id(nets[k]));
        }
        for order in [[0, 1], [1, 0]] {
            let mut scratch = grid.clone();
            let mut cost = 0u32;
            let mut paths: Vec<Vec<Node>> = Vec::new();
            let mut ok = true;
            for &k in &order {
                // The wire joins the main part's stub at any node but the pin's own tip.
                let targets: HashSet<Node> = stub_nodes[k].iter().skip(1).copied().collect();
                let targets = if targets.is_empty() {
                    stub_nodes[k].iter().copied().collect()
                } else {
                    targets
                };
                let id = Scene::net_id(nets[k]);
                match scratch.route_free(id, pin_nodes[k], &targets) {
                    Some((c, path)) => {
                        cost += c;
                        scratch.commit(&path, id);
                        paths.push(path);
                    }
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            // A bend is worth more here than the router's own price for it: two short wires of
            // one bend each are what is wanted.
            cost += BEND_EXTRA
                * paths
                    .iter()
                    .map(|p| grid::corners(p).len().saturating_sub(2) as u32)
                    .sum::<u32>();
            // The wires keep clear of each other and of the other net's stub.
            for (k, path) in paths.iter().enumerate() {
                let other_net = 1 - order[k];
                if path.iter().any(|n| stub_nodes[other_net].contains(n)) {
                    cost += CLASH;
                }
            }
            if paths[0].iter().any(|n| paths[1].contains(n)) {
                cost += CLASH;
            }
            if best.as_ref().is_none_or(|(c, _)| cost < *c) {
                let mut lanes = Vec::new();
                for path in &paths {
                    for w in grid::corners(path).windows(2) {
                        let mut r = Rect::around(grid.pt(w[0]));
                        r.include(grid.pt(w[1]));
                        lanes.push(inflate(r, 0.6));
                    }
                }
                best = Some((
                    cost,
                    Placement {
                        off: origin,
                        orient: turned[cand.orient][0].orient,
                        side: cand.side,
                        foot: cand.foot,
                        lanes,
                        order,
                    },
                ));
            }
        }
    }
    best.filter(|(cost, _)| *cost < CLASH)
        .map(|(_, placement)| placement)
}
