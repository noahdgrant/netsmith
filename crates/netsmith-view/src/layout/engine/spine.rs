//! The drawing of a spine ([`crate::layout::spine`]): the series parts of the power path on one
//! row, left to right, each turned along it; the parts to ground and the feedback divider hanging
//! from the node they serve, each with a stub and a symbol of its own; the IC on the row with its
//! input on the left and its switching pin on the right.
//!
//! Everything is placed from the IC's origin (the IC is at the origin of `placed` when its block
//! is built). The row of the input side is the line of the IC's first input pin, the row of the
//! output side the line of its switching pin; they are the same line when the symbol has the two
//! pins level, and else the IC stands between two rows, as it does on a hand drawing. No wire is
//! laid here: the parts are placed so that the router's wires are straight, and the corridor of
//! each row is kept for them.

use super::*;
use crate::layout::spine::{Side, Spine, Tap, value_of};

/// From the row to the tip of the pin of a part that hangs from it.
const DROP: f64 = 2.0 * GRID;
/// Between a node's wire and the first part that hangs from it, and between the last and the
/// next series part, in millimetres.
const NODE_GAP: f64 = 2.0 * GRID;
/// Between the two pins of a divider's resistors, the wire of the middle node.
const TAP_WIRE: f64 = 6.0 * GRID;
/// How far a corridor reaches past the end of a row, so that nothing stands in its way.
const FAR: f64 = 400.0;
/// How far below a row the parts that hang from it reach, at most, in millimetres.
const BAND: f64 = 30.0;

/// What the spine adds to the block of its IC.
pub(super) struct Plan {
    pub members: Vec<(usize, Pt, Option<Dir>)>,
    pub orients: Vec<(usize, Orient)>,
    /// What other parts keep out of: the feet of the parts and the corridors of the rows.
    pub obstacles: Vec<Rect>,
    /// What the line covers, with room for its symbols.
    pub rect: Rect,
    /// The row of the input side, from the IC's origin: the line a row of pieces is level on.
    pub level: Option<f64>,
    /// The nets of the line and the way their symbol goes.
    pub nets: Vec<(usize, i32)>,
    /// The corridor of each row, from the IC's stub to the end of the row.
    pub lanes: Vec<Rect>,
}

/// The two corridors of the rows, from the stubs of the IC's pins outward, long: kept free of the
/// satellites that are placed before the line is.
pub(super) fn lanes(ctx: &Ctx, placed: &[PlacedPart], spine: &Spine) -> Vec<Rect> {
    let a = &placed[spine.ic];
    let mut out = Vec::new();
    for (side, d) in [(&spine.input, -1.0), (&spine.output, 1.0)] {
        let Some(pin) = row_pin(ctx, a, side.nodes[0], d) else {
            continue;
        };
        let start = pin.at.x + d * (STUB + 0.7);
        let (lo, hi) = if d > 0.0 {
            (start, start + FAR)
        } else {
            (start - FAR, start)
        };
        out.push(Rect {
            min: Pt::new(lo, pin.at.y - 0.6),
            max: Pt::new(hi, pin.at.y + 0.6),
        });
    }
    out
}

/// The pin the row of a side runs from: the top one of the IC's pins on `net` that leaves the
/// way `d` goes, else the top one on the net.
fn row_pin<'a>(ctx: &Ctx, a: &'a PlacedPart, net: usize, d: f64) -> Option<&'a PlacedPin> {
    let want = if d > 0.0 { Dir::Right } else { Dir::Left };
    let on_net = |i: usize, q: &PlacedPin| q.net == Some(net) && !ctx.glyphs[a.part].pins[i].hidden;
    let top = |same_way: bool| {
        a.pins
            .iter()
            .enumerate()
            .filter(|&(i, q)| on_net(i, q) && (!same_way || q.escape == want))
            .map(|(_, q)| q)
            .min_by(|p, q| p.at.y.total_cmp(&q.at.y).then(p.at.x.total_cmp(&q.at.x)))
    };
    top(true).or_else(|| top(false))
}

/// `p` turned so that its pin on `net` leaves `dir` and the other leaves the opposite way, with
/// no two names over each other where a turn can avoid it.
fn turned(
    ctx: &Ctx,
    p: usize,
    pin_nets: &[Option<usize>],
    net: usize,
    dir: Dir,
) -> (PlacedPart, Orient) {
    let g = &ctx.glyphs[p];
    let fits = |o: Orient| {
        g.pins.iter().all(|pin| {
            let escape = o.dir((-pin.toward_body.0, -pin.toward_body.1));
            if net_of(ctx, p, &pin.id) == Some(net) {
                escape == dir
            } else {
                escape == dir.opposite()
            }
        })
    };
    let all: Vec<Orient> = Orient::ALL.into_iter().filter(|&o| fits(o)).collect();
    let readable: Vec<Orient> = all
        .iter()
        .copied()
        .filter(|&o| !g.texts_collide(o))
        .collect();
    let o = readable
        .first()
        .or(all.first())
        .copied()
        .unwrap_or(Orient::IDENTITY);
    (place_part(ctx, p, o, pin_nets), o)
}

fn pin_on(part: &PlacedPart, net: usize) -> Pt {
    part.pins
        .iter()
        .find(|q| q.net == Some(net))
        .map_or(Pt::new(0.0, 0.0), |q| q.at)
}

fn pin_off(part: &PlacedPart, net: usize) -> Pt {
    part.pins
        .iter()
        .find(|q| q.net != Some(net))
        .map_or(Pt::new(0.0, 0.0), |q| q.at)
}

/// What a part covers with its names on `text`, and the room a ground symbol below a pin of it
/// needs.
fn boxed(ctx: &Ctx, part: &PlacedPart, text: Dir) -> Rect {
    let mut p = part.clone();
    set_text_side(ctx, &mut p, text);
    let mut r = p.extent;
    for pin in &p.pins {
        if let Some(n) = pin.net
            && ctx.power[n] == Some(PowerKind::Ground)
        {
            r.include(Pt::new(pin.at.x - 2.54, pin.at.y));
            r.include(Pt::new(pin.at.x + 2.54, pin.at.y + SYMBOL_DOWN));
        }
    }
    r
}

/// Parts that hang from a node together: where each is from the item's reference point (the tip
/// of the first part's pin on the node, `DROP` below the row), and what they cover.
struct Item {
    parts: Vec<(usize, Pt, Orient)>,
    foot: Rect,
}

fn translated(r: Rect, by: Pt) -> Rect {
    r.translated(by)
}

fn grounded(ctx: &Ctx, pin_nets: &[Vec<Option<usize>>], p: usize, node: usize, text: Dir) -> Item {
    let (part, o) = turned(ctx, p, &pin_nets[p], node, Dir::Up);
    let r = pin_on(&part, node);
    let by = Pt::new(-r.x, -r.y);
    let mut foot = inflate(translated(boxed(ctx, &part, text), by), GRID);
    foot.include(Pt::new(0.0, -DROP));
    Item {
        parts: vec![(p, by, o)],
        foot,
    }
}

/// The divider that hangs from a node: the parts across the node and the net of the IC's pin
/// side by side, with their far pins level, and the parts from that net to ground under the
/// first.
fn tapped(ctx: &Ctx, pin_nets: &[Vec<Option<usize>>], node: usize, tap: &Tap, text: Dir) -> Item {
    let mut parts: Vec<(usize, Pt, Orient)> = Vec::new();
    let mut foot: Option<Rect> = None;
    // The part with the greatest value first (the divider's resistor, then a capacitor across it):
    // the parts from the middle net to ground stand under it.
    let mut order: Vec<usize> = tap.tops.clone();
    order.sort_by(|&a, &b| {
        value_of(&ctx.design.parts[b].value)
            .total_cmp(&value_of(&ctx.design.parts[a].value))
            .then(a.cmp(&b))
    });
    let tops: Vec<(usize, PlacedPart, Orient)> = order
        .iter()
        .map(|&p| {
            let (part, o) = turned(ctx, p, &pin_nets[p], node, Dir::Up);
            (p, part, o)
        })
        .collect();
    // The depth of the level the far pins are on, from the node pin's depth.
    let lens: Vec<f64> = tops
        .iter()
        .map(|(_, part, _)| pin_off(part, node).y - pin_on(part, node).y)
        .collect();
    let mid = lens.iter().copied().fold(0.0, f64::max);
    let mut x = 0.0;
    for ((p, part, o), len) in tops.iter().zip(&lens) {
        let (t, b) = (pin_on(part, node), boxed(ctx, part, text));
        let by = Pt::new(x - t.x, (mid - len) - t.y);
        parts.push((*p, by, *o));
        let r = translated(b, by);
        foot = Some(foot.map_or(r, |mut f| {
            f.union(r);
            f
        }));
        x = snap_up(r.max.x + 2.0 * GRID, GRID);
    }
    // The parts from the middle net to ground, in the column of the first top.
    let mut bx = 0.0;
    for &p in &tap.bottoms {
        let (part, o) = turned(ctx, p, &pin_nets[p], tap.net, Dir::Up);
        let t = pin_on(&part, tap.net);
        let by = Pt::new(bx - t.x, mid + TAP_WIRE - t.y);
        parts.push((p, by, o));
        let r = translated(boxed(ctx, &part, text), by);
        foot = Some(foot.map_or(r, |mut f| {
            f.union(r);
            f
        }));
        bx = snap_up(r.max.x + 2.0 * GRID, GRID);
        bx = bx.max(x);
    }
    let mut foot = inflate(foot.expect("a tap has a part"), GRID);
    foot.include(Pt::new(0.0, -DROP));
    Item { parts, foot }
}

struct Draft {
    members: Vec<(usize, Pt, Option<Dir>)>,
    orients: Vec<(usize, Orient)>,
    obstacles: Vec<Rect>,
    rect: Rect,
}

impl Draft {
    fn cover(&mut self, r: Rect) {
        self.rect.union(r);
    }
}

/// Lays one stretch of the line along the row `y`, outward from `x0` the way `d` goes (+1
/// right, -1 left), `toward` the way the inner pin of a series part faces (to the IC). Gives the
/// end of the row.
#[allow(clippy::too_many_arguments)]
fn lay(
    ctx: &Ctx,
    pin_nets: &[Vec<Option<usize>>],
    side: &Side,
    d: f64,
    y: f64,
    x0: f64,
    toward: Dir,
    draft: &mut Draft,
) -> f64 {
    // The names stand on the outer side of what hangs, away from the IC.
    let text = if d > 0.0 { Dir::Right } else { Dir::Left };
    let mut x = x0;
    for (i, &node) in side.nodes.iter().enumerate() {
        let mut items: Vec<Item> = Vec::new();
        if let Some(tap) = &side.taps[i] {
            items.push(tapped(ctx, pin_nets, node, tap, text));
        }
        for &p in &side.grounds[i] {
            items.push(grounded(ctx, pin_nets, p, node, text));
        }
        for item in items {
            let foot = item.foot;
            let at = if d > 0.0 {
                snap_up(x + NODE_GAP - foot.min.x, GRID)
            } else {
                -snap_up(-(x - NODE_GAP - foot.max.x), GRID)
            };
            let origin = Pt::new(at, y + DROP);
            for (p, by, o) in &item.parts {
                draft
                    .members
                    .push((*p, Pt::new(origin.x + by.x, origin.y + by.y), Some(text)));
                draft.orients.push((*p, *o));
            }
            let covered = foot.translated(origin);
            draft.obstacles.push(covered);
            draft.cover(covered);
            x = if d > 0.0 {
                covered.max.x
            } else {
                covered.min.x
            };
        }
        let Some(&p) = side.parts.get(i) else {
            continue;
        };
        // The series part: its pin on this node faces the way the IC is.
        let (part, o) = turned(ctx, p, &pin_nets[p], node, toward);
        let inner = pin_on(&part, node);
        let outer = pin_off(&part, node);
        let len = (outer.x - inner.x).abs();
        let gap = if side.grounds[i].is_empty() && side.taps[i].is_none() {
            4.0 * GRID
        } else {
            NODE_GAP + GRID
        };
        let pin_x = if d > 0.0 {
            snap_up(x + gap, GRID)
        } else {
            -snap_up(-(x - gap), GRID)
        };
        let off = Pt::new(snap_grid(pin_x - inner.x), snap_grid(y - inner.y));
        draft.members.push((p, off, Some(Dir::Up)));
        draft.orients.push((p, o));
        let mut r = boxed(ctx, &part, Dir::Up).translated(off);
        r.include(Pt::new(pin_x, y));
        let foot = inflate(r, GRID);
        draft.obstacles.push(foot);
        draft.cover(foot);
        // The next node starts at the end of the part's outer stub.
        x = pin_x + d * (len + STUB);
    }
    x
}

/// The line through the IC `spine.ic` placed at the origin: the plan for its block.
/// `from_pairs` are the boxes the pair satellites of the IC took, which the rows start beyond.
pub(super) fn place(
    ctx: &Ctx,
    placed: &[PlacedPart],
    pin_nets: &[Vec<Option<usize>>],
    spine: &Spine,
    from_pairs: &[Rect],
) -> Plan {
    let a = &placed[spine.ic];
    let mut draft = Draft {
        members: Vec::new(),
        orients: Vec::new(),
        obstacles: Vec::new(),
        rect: a.extent,
    };
    let mut level: Option<f64> = None;
    let mut lanes: Vec<Rect> = Vec::new();
    for (side, d, toward) in [
        (&spine.input, -1.0, Dir::Right),
        (&spine.output, 1.0, Dir::Left),
    ] {
        let Some(pin) = row_pin(ctx, a, side.nodes[0], d) else {
            continue;
        };
        let y = pin.at.y;
        if d < 0.0 {
            level = Some(y);
        }
        let mut x0 = pin.at.x + d * STUB;
        // Beyond what the satellites of the IC's pins took on that side, where the parts that
        // hang from the row will be.
        for r in from_pairs {
            if r.max.y < y - 3.0 || r.min.y > y + BAND {
                continue;
            }
            if d > 0.0 && r.max.x > x0 && r.min.x < x0 + FAR {
                x0 = x0.max(r.max.x);
            }
            if d < 0.0 && r.min.x < x0 && r.max.x > x0 - FAR {
                x0 = x0.min(r.min.x);
            }
        }
        let mut end = lay(ctx, pin_nets, side, d, y, x0, toward, &mut draft);
        // The free chains continue the input side, a gap between: the symbols of the two nets
        // stand there.
        if d < 0.0 {
            for free in &spine.free {
                let names = |n: usize| text_width(&ellipsize(&ctx.design.nets[n].name));
                let gap = 8.0 * GRID
                    + names(free.nodes[0]).max(names(*spine.input.nodes.last().expect("a node")));
                let start = end - gap;
                end = lay(ctx, pin_nets, free, d, y, start, Dir::Right, &mut draft);
            }
        }
        let (lo, hi) = if d > 0.0 { (x0, end) } else { (end, x0) };
        let mut lane = Rect {
            min: Pt::new(lo, y - 0.6),
            max: Pt::new(hi, y + 0.6),
        };
        // Room for the symbol at the outer end.
        lane.include(Pt::new(end, y - SYMBOL_UP));
        draft.cover(lane);
        let corridor = Rect {
            min: Pt::new(lo, y - 0.6),
            max: Pt::new(hi, y + 0.6),
        };
        draft.obstacles.push(corridor);
        lanes.push(corridor);
    }
    Plan {
        members: draft.members,
        orients: draft.orients,
        obstacles: draft.obstacles,
        rect: draft.rect,
        level,
        nets: spine.nets(),
        lanes,
    }
}
