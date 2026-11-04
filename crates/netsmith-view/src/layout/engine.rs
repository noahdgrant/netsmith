//! The engine: places the parts as a person would (the main parts in a row, the small parts
//! that serve a pin beside that pin), then wires every net on a grid, then draws the rails
//! as power symbols and the nets that leave the sheet as flags.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::grid::{self, Grid, Node};
use super::*;

/// Clear space round a part and what it needs beside it.
const MARGIN: f64 = 2.54;
/// Between two blocks of parts in the row.
const BLOCK_GAP: f64 = 15.24;
/// Between the parts that stand alone.
const FREE_GAP: f64 = 7.62;
/// Pins of one rail within this many cells of each other share a symbol.
const POWER_JOIN: i32 = 30;
/// Room above a supply pin for its wire and symbol, and below a ground pin.
const SYMBOL_UP: f64 = 11.43;
const SYMBOL_DOWN: f64 = 8.89;
/// A wire straight out of a pin, in cells, before it may turn.
const STUB_CELLS: i32 = 2;
/// The wire from a rail's wires to its symbol, in cells.
const STEM_CELLS: i32 = 2;

fn snap_grid(v: f64) -> f64 {
    snap(v, GRID)
}

fn inflate(r: Rect, d: f64) -> Rect {
    Rect {
        min: Pt::new(r.min.x - d, r.min.y - d),
        max: Pt::new(r.max.x + d, r.max.y + d),
    }
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.min.x < b.max.x - 1e-6
        && b.min.x < a.max.x - 1e-6
        && a.min.y < b.max.y - 1e-6
        && b.min.y < a.max.y - 1e-6
}

fn translate_part(part: &mut PlacedPart, by: Pt) {
    part.at = Pt::new(part.at.x + by.x, part.at.y + by.y);
    part.extent = part.extent.translated(by);
    for pin in &mut part.pins {
        pin.at = Pt::new(pin.at.x + by.x, pin.at.y + by.y);
    }
    for t in std::iter::once(&mut part.reference).chain(part.value.as_mut()) {
        t.at = Pt::new(t.at.x + by.x, t.at.y + by.y);
    }
}

/// Moves the reference and the value of a part (at the origin) to `side` of its body.
fn set_text_side(ctx: &Ctx, part: &mut PlacedPart, side: Dir) {
    let body = body_of(&part.glyph, part.orient);
    let (reference, value) = part_texts(&ctx.design.parts[part.part], body, side);
    let mut extent = body;
    extent.union(reference.extent());
    if let Some(v) = &value {
        extent.union(v.extent());
    }
    for pin in &part.pins {
        extent.include(pin.at);
    }
    part.reference = reference;
    part.value = value;
    part.extent = extent;
}

fn same_point(a: Pt, b: Pt) -> bool {
    (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
}

#[derive(Clone)]
struct Shape {
    /// Where each part of the unit is, from the first part's origin.
    offsets: Vec<Pt>,
}

fn unit_offsets(placed: &[PlacedPart], unit: &Unit) -> Shape {
    let mut offsets = vec![Pt::new(0.0, 0.0)];
    for w in 1..unit.parts.len() {
        let (prev, next) = (unit.parts[w - 1], unit.parts[w]);
        let down = pin_on(&placed[prev], &unit.up_pads[w - 1], true);
        let up = pin_on(&placed[next], &unit.up_pads[w], false);
        let before = offsets[w - 1];
        offsets.push(Pt::new(
            before.x + down.x - up.x,
            before.y + down.y + CHAIN_WIRE - up.y,
        ));
    }
    Shape { offsets }
}

/// The box of a unit of parts with its parts' texts on `side` (when told), placed by
/// `offsets`, and what its power pins need above and below.
fn unit_rect(
    ctx: &Ctx,
    placed: &[PlacedPart],
    unit: &Unit,
    shape: &Shape,
    side: Option<Dir>,
) -> Rect {
    let mut out: Option<Rect> = None;
    for (w, &p) in unit.parts.iter().enumerate() {
        let mut part = placed[p].clone();
        if let Some(side) = side {
            set_text_side(ctx, &mut part, side);
        }
        let by = shape.offsets[w];
        union_into(&mut out, part.extent.translated(by));
        for pin in &part.pins {
            if let Some(n) = pin.net
                && let Some(kind) = ctx.power[n]
            {
                let at = Pt::new(pin.at.x + by.x, pin.at.y + by.y);
                let mut r = Rect::around(Pt::new(at.x - 2.54, at.y));
                r.include(Pt::new(at.x + 2.54, at.y));
                match kind {
                    PowerKind::Supply => r.include(Pt::new(at.x, at.y - SYMBOL_UP)),
                    PowerKind::Ground => r.include(Pt::new(at.x, at.y + SYMBOL_DOWN)),
                }
                union_into(&mut out, r);
            }
        }
    }
    out.expect("a unit has parts")
}

/// A net a satellite is on, and where to bring a wire to, from the unit's first part's origin.
struct Tie {
    net: usize,
    r: Pt,
}

fn ties(placed: &[PlacedPart], unit: &Unit, shape: &Shape) -> Vec<Tie> {
    let mut out = Vec::new();
    let terminal = |pin: &PlacedPin, by: Pt| Tie {
        net: pin.net.unwrap_or(usize::MAX),
        r: Pt::new(pin.at.x + by.x, pin.at.y + by.y).step(pin.escape, STUB),
    };
    if unit.up_pads.is_empty() {
        let part = unit.parts[0];
        for pin in &placed[part].pins {
            if pin.net.is_some() {
                out.push(terminal(pin, Pt::new(0.0, 0.0)));
            }
        }
        return out;
    }
    // The nodes between the parts come first: what is on them is what the stack serves.
    for w in 0..unit.parts.len() - 1 {
        let part = unit.parts[w];
        let down = placed[part]
            .pins
            .iter()
            .find(|q| q.id != unit.up_pads[w])
            .expect("a stacked part has two pins");
        let by = shape.offsets[w];
        if let Some(net) = down.net {
            let a = Pt::new(down.at.x + by.x, down.at.y + by.y);
            out.push(Tie {
                net,
                r: Pt::new(a.x, a.y + CHAIN_WIRE / 2.0),
            });
        }
    }
    let last = unit.parts.len() - 1;
    for (w, up) in [(0, true), (last, false)] {
        let part = unit.parts[w];
        let pin = placed[part]
            .pins
            .iter()
            .find(|q| (q.id == unit.up_pads[w]) == up)
            .expect("a stacked part has two pins");
        if pin.net.is_some() {
            out.push(terminal(pin, shape.offsets[w]));
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Anchor,
    Satellite,
    Free,
}

struct SatPlan {
    unit: usize,
    /// The part whose pin it stands beside: an anchor, or a satellite placed before it.
    anchor: usize,
    /// The anchor of the block it is in.
    root: usize,
    pin: usize,
    r: Pt,
    /// The net the satellite is tied to the anchor's pin by.
    net: usize,
}

struct Block {
    anchor: usize,
    /// Each part and where it is from the anchor's origin, with the side its texts go on.
    members: Vec<(usize, Pt, Option<Dir>)>,
    rect: Rect,
}

pub(super) fn run(
    ctx: &Ctx,
    units: &[Unit],
    order: &[usize],
    orient: &[Orient],
    marks: &Marks,
) -> Layout {
    let n = ctx.design.parts.len();
    let pin_nets: Vec<Vec<Option<usize>>> = (0..n)
        .map(|p| {
            ctx.glyphs[p]
                .pins
                .iter()
                .map(|pin| net_of(ctx, p, &pin.id))
                .collect()
        })
        .collect();
    let mut placed: Vec<PlacedPart> = (0..n)
        .map(|p| place_part(ctx, p, orient[p], &pin_nets[p]))
        .collect();
    let shapes: Vec<Shape> = units.iter().map(|u| unit_offsets(&placed, u)).collect();

    let role: Vec<Role> = units
        .iter()
        .map(|u| {
            if u.parts.iter().all(|&p| ctx.part_nets[p].is_empty()) {
                Role::Free
            } else if u.parts.len() == 1 && !is_stackable(ctx, u.parts[0]) {
                Role::Anchor
            } else {
                Role::Satellite
            }
        })
        .collect();
    let anchors: Vec<usize> = order
        .iter()
        .copied()
        .filter(|&u| role[u] == Role::Anchor)
        .collect();
    let anchor_pos: BTreeMap<usize, usize> =
        anchors.iter().enumerate().map(|(i, &u)| (u, i)).collect();

    // Which pins of the anchors are on each net.
    let mut anchor_pins: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
    for &u in &anchors {
        let p = units[u].parts[0];
        for (i, pin) in placed[p].pins.iter().enumerate() {
            if ctx.glyphs[p].pins[i].hidden {
                continue;
            }
            if let Some(net) = pin.net {
                anchor_pins.entry(net).or_default().push((p, i));
            }
        }
    }

    // Each satellite is tied to the pin it serves.
    let mut plans: Vec<SatPlan> = Vec::new();
    let mut chained: Vec<SatPlan> = Vec::new();
    let mut supply_use: BTreeMap<usize, usize> = BTreeMap::new();
    for u in 0..units.len() {
        if role[u] != Role::Satellite {
            continue;
        }
        let ties = ties(&placed, &units[u], &shapes[u]);
        let mut chosen: Option<SatPlan> = None;
        for t in ties
            .iter()
            .filter(|t| t.net != usize::MAX && ctx.power[t.net].is_none())
        {
            if let Some(pins) = anchor_pins.get(&t.net) {
                let &(p, i) = pins
                    .iter()
                    .min_by_key(|&&(p, i)| (anchor_pos[&unit_index(units, p)], i))
                    .expect("a pin");
                chosen = Some(SatPlan {
                    unit: u,
                    anchor: p,
                    root: p,
                    pin: i,
                    r: t.r,
                    net: t.net,
                });
                break;
            }
        }
        if chosen.is_none() {
            // A part between two rails stands beside the supply pin it decouples.
            for t in ties
                .iter()
                .filter(|t| t.net != usize::MAX && ctx.power[t.net] == Some(PowerKind::Supply))
            {
                if let Some(pins) = anchor_pins.get(&t.net) {
                    let used = supply_use.entry(t.net).or_default();
                    let (p, i) = pins[*used % pins.len()];
                    *used += 1;
                    chosen = Some(SatPlan {
                        unit: u,
                        anchor: p,
                        root: p,
                        pin: i,
                        r: t.r,
                        net: t.net,
                    });
                    break;
                }
            }
        }
        if let Some(mut c) = chosen {
            // A part in series (an inductor, a resistor between two signals) lies along the
            // pin it hangs on, with its end that is on the pin's net toward the part.
            let part = units[u].parts[0];
            let series = units[u].parts.len() == 1
                && (matches!(
                    ctx.design.parts[part].kind.as_str(),
                    "Inductor" | "FerriteBead" | "Fuse"
                ) || placed[part]
                    .pins
                    .iter()
                    .all(|q| q.net.is_none_or(|n| ctx.power[n].is_none())));
            let side = placed[c.anchor].pins[c.pin].escape;
            if series && !side.is_vertical() {
                let want = side.opposite();
                let found = Orient::ALL.into_iter().find(|&o| {
                    let g = &ctx.glyphs[part];
                    g.pins.len() == 2
                        && g.pins.iter().all(|pin| {
                            !o.dir((-pin.toward_body.0, -pin.toward_body.1))
                                .is_vertical()
                        })
                        && g.pins.iter().any(|pin| {
                            net_of(ctx, part, &pin.id) == Some(c.net)
                                && o.dir((-pin.toward_body.0, -pin.toward_body.1)) == want
                        })
                });
                if let Some(o) = found {
                    placed[part] = place_part(ctx, part, o, &pin_nets[part]);
                    if let Some(pin) = placed[part].pins.iter().find(|q| q.net == Some(c.net)) {
                        c.r = Pt::new(pin.at.x, pin.at.y).step(pin.escape, STUB);
                    }
                }
            }
            plans.push(c);
        }
    }

    // A part between two rails whose supply no anchor pin is on stands beside a satellite that
    // is on it (the output capacitor beside the inductor).
    for u in 0..units.len() {
        if role[u] != Role::Satellite || plans.iter().any(|c| c.unit == u) {
            continue;
        }
        let ties = ties(&placed, &units[u], &shapes[u]);
        'tie: for t in ties
            .iter()
            .filter(|t| t.net != usize::MAX && ctx.power[t.net] == Some(PowerKind::Supply))
        {
            for parent in plans.iter() {
                let part = units[parent.unit].parts[0];
                if units[parent.unit].parts.len() != 1 {
                    continue;
                }
                let found = placed[part]
                    .pins
                    .iter()
                    .enumerate()
                    .find(|(i, q)| q.net == Some(t.net) && !ctx.glyphs[part].pins[*i].hidden);
                if let Some((i, _)) = found {
                    let root = parent.root;
                    let unit_u = u;
                    let r = t.r;
                    let net = t.net;
                    let _ = unit_u;
                    chained.push(SatPlan {
                        unit: u,
                        anchor: part,
                        root,
                        pin: i,
                        r,
                        net,
                    });
                    break 'tie;
                }
            }
        }
    }
    plans.append(&mut chained);

    // A block for each anchor: the anchor with its satellites round it.
    let mut blocks: Vec<Block> = Vec::new();
    let mut tied = vec![false; units.len()];
    for &u in &anchors {
        let a = units[u].parts[0];
        let mine: Vec<&SatPlan> = plans.iter().filter(|s| s.root == a).collect();
        let block = build_block(ctx, &placed, units, &shapes, a, &mine, &mut tied);
        blocks.push(block);
    }

    // Absolute places.
    let mut at: Vec<Pt> = vec![Pt::new(0.0, 0.0); n];
    let mut text_side: Vec<Option<Dir>> = vec![None; n];
    let mut x = 0.0;
    let mut area: Option<Rect> = None;
    for b in &blocks {
        let ext = placed[b.anchor].extent;
        let by = Pt::new(
            snap_up(x - b.rect.min.x, GRID),
            -snap_grid((ext.min.y + ext.max.y) / 2.0),
        );
        for &(p, off, side) in &b.members {
            at[p] = Pt::new(by.x + off.x, by.y + off.y);
            text_side[p] = side;
        }
        union_into(&mut area, b.rect.translated(by));
        x = by.x + b.rect.max.x + BLOCK_GAP;
    }
    // The parts that stand alone, in rows under the blocks.
    let free: Vec<usize> = (0..units.len())
        .filter(|&u| role[u] == Role::Free || (role[u] == Role::Satellite && !tied[u]))
        .collect();
    let mut rects: Vec<(usize, Rect)> = Vec::new();
    for &u in &free {
        let r = inflate(
            unit_rect(ctx, &placed, &units[u], &shapes[u], None),
            FREE_GAP / 2.0,
        );
        rects.push((u, r));
    }
    let total: f64 = rects.iter().map(|(_, r)| r.width() * r.height()).sum();
    let target = match area {
        Some(a) => a.width().max(80.0),
        None => (total * 1.6).sqrt().max(80.0),
    };
    let top = area.map_or(0.0, |a| a.max.y + 12.7);
    let (mut fx, mut fy, mut row) = (0.0, top, 0.0f64);
    for (u, r) in &rects {
        if fx > 0.0 && fx + r.width() > target {
            fx = 0.0;
            fy += row;
            row = 0.0;
        }
        let by = Pt::new(snap_up(fx - r.min.x, GRID), snap_up(fy - r.min.y, GRID));
        for (w, &p) in units[*u].parts.iter().enumerate() {
            at[p] = Pt::new(
                by.x + shapes[*u].offsets[w].x,
                by.y + shapes[*u].offsets[w].y,
            );
        }
        fx += r.width();
        row = row.max(r.height());
    }
    for p in 0..n {
        if let Some(side) = text_side[p] {
            set_text_side(ctx, &mut placed[p], side);
        }
        translate_part(&mut placed[p], at[p]);
    }

    Scene::new(ctx, placed, units, marks).finish()
}

/// The name inside a flag whose point is at `tip` and that points `dir`.
fn port_label(name: &str, tip: Pt, dir: Dir) -> Text {
    let start = tip.step(dir, 1.6);
    let anchor = if dir == Dir::Left {
        Anchor::End
    } else {
        Anchor::Start
    };
    Text::new(name, Pt::new(start.x, start.y + 0.45), anchor)
}

fn unit_index(units: &[Unit], part: usize) -> usize {
    units
        .iter()
        .position(|u| u.parts.contains(&part))
        .expect("every part is in a unit")
}

/// The satellites of one anchor placed round it, each as near the pin it serves as it can be
/// without covering anything.
fn build_block(
    ctx: &Ctx,
    placed: &[PlacedPart],
    units: &[Unit],
    shapes: &[Shape],
    anchor: usize,
    sats: &[&SatPlan],
    tied: &mut [bool],
) -> Block {
    let a = &placed[anchor];
    let mut obstacles: Vec<Rect> = vec![inflate(a.extent, 0.5)];
    let mut members: Vec<(usize, Pt, Option<Dir>)> = vec![(anchor, Pt::new(0.0, 0.0), None)];
    let mut rect = a.extent;
    // The satellites of the anchor's own pins first, from the top down; then those that stand
    // beside a satellite, in the order they were found.
    let mut sorted: Vec<&&SatPlan> = sats.iter().filter(|x| x.anchor == anchor).collect();
    sorted.sort_by(|x, y| {
        let (px, py) = (&a.pins[x.pin], &a.pins[y.pin]);
        (px.escape as u8)
            .cmp(&(py.escape as u8))
            .then(px.at.y.total_cmp(&py.at.y))
            .then(px.at.x.total_cmp(&py.at.x))
            .then(x.unit.cmp(&y.unit))
    });
    sorted.extend(sats.iter().filter(|x| x.anchor != anchor));
    for plan in sorted {
        let (pin_at, pin_escape) = if plan.anchor == anchor {
            (a.pins[plan.pin].at, a.pins[plan.pin].escape)
        } else {
            let Some(&(_, off, _)) = members.iter().find(|m| m.0 == plan.anchor) else {
                continue;
            };
            let q = &placed[plan.anchor].pins[plan.pin];
            (Pt::new(q.at.x + off.x, q.at.y + off.y), q.escape)
        };
        let pin = PlacedPin {
            id: String::new(),
            at: pin_at,
            escape: pin_escape,
            net: None,
        };
        let e = pin.at.step(pin.escape, STUB);
        let sides: Vec<Dir> = if pin.escape.is_vertical() {
            vec![Dir::Right, Dir::Left]
        } else {
            vec![pin.escape]
        };
        let unit = &units[plan.unit];
        let shape = &shapes[plan.unit];
        let mut found: Option<(Pt, Dir, Rect)> = None;
        'search: for k in 0..14 {
            let dx = 7.62 + 5.08 * f64::from(k);
            for &sd in &sides {
                let base = unit_rect(ctx, placed, unit, shape, Some(sd));
                for dy in [0.0, 5.08, -5.08, 10.16, -10.16, 15.24, -15.24] {
                    let target = Pt::new(e.x + sd.vector().0 * dx, e.y + dy);
                    let off = Pt::new(
                        snap_grid(target.x - plan.r.x),
                        snap_grid(target.y - plan.r.y),
                    );
                    let foot = inflate(base.translated(off), MARGIN);
                    if obstacles.iter().any(|o| overlaps(&foot, o)) {
                        continue;
                    }
                    if dy == 0.0 {
                        let mut lane = Rect::around(Pt::new(e.x.min(target.x), e.y - 0.6));
                        lane.include(Pt::new(e.x.max(target.x), e.y + 0.6));
                        if obstacles.iter().any(|o| overlaps(&lane, o)) {
                            continue;
                        }
                    }
                    found = Some((off, sd, foot));
                    break 'search;
                }
            }
        }
        if let Some((off, sd, foot)) = found {
            tied[plan.unit] = true;
            // The wire from the pin to the part is not for another part to stand on.
            let target = Pt::new(plan.r.x + off.x, plan.r.y + off.y);
            let mut lane = Rect::around(Pt::new(e.x.min(target.x), e.y.min(target.y) - 0.6));
            lane.include(Pt::new(e.x.max(target.x), e.y.max(target.y) + 0.6));
            obstacles.push(lane);
            for (w, &p) in unit.parts.iter().enumerate() {
                members.push((
                    p,
                    Pt::new(off.x + shape.offsets[w].x, off.y + shape.offsets[w].y),
                    Some(sd),
                ));
            }
            obstacles.push(foot);
            rect.union(foot);
        }
    }
    Block {
        anchor,
        members,
        rect: inflate(rect, 0.0),
    }
}

// ---------------------------------------------------------------------------------------
// Wires

struct PinInfo {
    part: usize,
    idx: usize,
    net: usize,
    node: Node,
    escape: Dir,
}

struct Stub {
    pin: Node,
    end: Node,
    heading: Dir,
}

struct Scene<'a> {
    ctx: &'a Ctx<'a>,
    parts: Vec<PlacedPart>,
    grid: Grid,
    wires: Vec<Wire>,
    symbols: Vec<PowerSymbol>,
    ports: Vec<Port>,
    pins: Vec<PinInfo>,
    stubs: BTreeMap<(usize, usize), Stub>,
    /// Nodes each net's wires already cover, for the nets a stack joined.
    trees: BTreeMap<usize, BTreeSet<Node>>,
    joined: BTreeSet<(usize, usize)>,
    local: Vec<usize>,
    marks: &'a Marks,
    units: &'a [Unit],
}

impl<'a> Scene<'a> {
    fn new(ctx: &'a Ctx<'a>, parts: Vec<PlacedPart>, units: &'a [Unit], marks: &'a Marks) -> Self {
        let mut bounds = parts[0..]
            .first()
            .map_or(Rect::around(Pt::new(0.0, 0.0)), |p| p.extent);
        for p in &parts {
            bounds.union(p.extent);
        }
        let bounds = inflate(bounds, 40.0);
        let grid = Grid::new(bounds, GRID);
        let mut scene = Scene {
            ctx,
            parts,
            grid,
            wires: Vec::new(),
            symbols: Vec::new(),
            ports: Vec::new(),
            pins: Vec::new(),
            stubs: BTreeMap::new(),
            trees: BTreeMap::new(),
            joined: BTreeSet::new(),
            local: vec![0; ctx.design.nets.len()],
            marks,
            units,
        };
        scene.block_parts();
        scene.collect_pins();
        scene
    }

    fn net_id(net: usize) -> u32 {
        net as u32
    }

    /// Parts, the lines of their pins and all their texts are in the way of wires.
    fn block_parts(&mut self) {
        for part in &self.parts {
            if let Some(b) = part.glyph.body() {
                let corners = [
                    part.orient.apply((b.min.x, b.min.y)),
                    part.orient.apply((b.max.x, b.max.y)),
                    part.orient.apply((b.min.x, b.max.y)),
                    part.orient.apply((b.max.x, b.min.y)),
                ];
                let mut r =
                    Rect::around(Pt::new(corners[0].x + part.at.x, corners[0].y + part.at.y));
                for c in &corners {
                    r.include(Pt::new(c.x + part.at.x, c.y + part.at.y));
                }
                self.grid.block_rect(r);
            }
            for (pin, placed) in part.glyph.pins.iter().zip(&part.pins) {
                if pin.hidden {
                    continue;
                }
                let toward = part.orient.dir(pin.toward_body);
                // The pin's line, but not the point a wire starts at.
                let from = self.grid.node(placed.at);
                let (dx, dy) = toward.vector();
                for k in 1..=((pin.length / GRID).round() as i32) {
                    self.grid
                        .block((from.0 + dx as i32 * k, from.1 + dy as i32 * k));
                }
                let (number, name) = part.glyph.pin_texts(pin, placed.at, toward);
                for t in number.into_iter().chain(name) {
                    self.grid
                        .block_rect(inflate(t.extent_at(part.glyph.pin_text), 0.2));
                }
            }
            for t in std::iter::once(&part.reference).chain(part.value.as_ref()) {
                self.grid.block_rect(inflate(t.extent(), 0.3));
            }
        }
    }

    fn collect_pins(&mut self) {
        let mut seen: BTreeSet<Node> = BTreeSet::new();
        for (p, part) in self.parts.iter().enumerate() {
            for (i, pin) in part.pins.iter().enumerate() {
                if part.glyph.pins[i].hidden {
                    continue;
                }
                let node = self.grid.node(pin.at);
                if let Some(net) = pin.net
                    && seen.insert(node)
                {
                    self.pins.push(PinInfo {
                        part: p,
                        idx: i,
                        net,
                        node,
                        escape: pin.escape,
                    });
                    self.local[net] += 1;
                }
            }
        }
    }

    /// Whether the net has to be drawn here: two of its pins, a rail, or a way on.
    fn active(&self, net: usize) -> bool {
        self.local[net] >= 2 || self.ctx.external[net] || self.ctx.power[net].is_some()
    }

    fn poly(&self, nodes: &[Node]) -> Vec<Pt> {
        grid::corners(nodes)
            .into_iter()
            .map(|n| self.grid.pt(n))
            .collect()
    }

    fn push_wire(&mut self, net: usize, nodes: &[Node]) {
        if nodes.len() >= 2 {
            let points = self.poly(nodes);
            self.wires.push(Wire { net, points });
        }
    }

    /// The stack's own wires, and the straight stubs out of the pins that are not in one.
    fn join_and_stub(&mut self) {
        let units = self.units;
        for unit in units.iter().filter(|u| u.parts.len() > 1) {
            for w in 1..unit.parts.len() {
                let (prev, next) = (unit.parts[w - 1], unit.parts[w]);
                let down = self.parts[prev]
                    .pins
                    .iter()
                    .position(|q| q.id != unit.up_pads[w - 1])
                    .expect("two pins");
                let up = self.parts[next]
                    .pins
                    .iter()
                    .position(|q| q.id == unit.up_pads[w])
                    .expect("two pins");
                let (a, b) = (
                    self.grid.node(self.parts[prev].pins[down].at),
                    self.grid.node(self.parts[next].pins[up].at),
                );
                let Some(net) = self.parts[prev].pins[down].net else {
                    continue;
                };
                self.grid.claim_wire(a, b, Self::net_id(net));
                let mut nodes = vec![a];
                let (dx, dy) = ((b.0 - a.0).signum(), (b.1 - a.1).signum());
                let mut k = a;
                while k != b {
                    k = (k.0 + dx, k.1 + dy);
                    nodes.push(k);
                }
                self.push_wire(net, &[a, b]);
                self.trees.entry(net).or_default().extend(nodes);
                self.joined.insert((prev, down));
                self.joined.insert((next, up));
            }
        }
        let pins: Vec<(usize, usize, usize, Node, Dir)> = self
            .pins
            .iter()
            .map(|p| (p.part, p.idx, p.net, p.node, p.escape))
            .collect();
        for (part, idx, net, node, escape) in pins {
            if self.joined.contains(&(part, idx)) || !self.active(net) {
                continue;
            }
            let id = Self::net_id(net);
            self.grid.unblock(node);
            let (dx, dy) = escape.vector();
            let step = |k: i32| (node.0 + dx as i32 * k, node.1 + dy as i32 * k);
            let mut end = node;
            for k in 1..=STUB_CELLS {
                if self.grid.is_free(step(k)) {
                    end = step(k);
                } else {
                    break;
                }
            }
            self.grid.claim_wire(node, end, id);
            self.push_wire(net, &[node, end]);
            self.stubs.insert(
                (part, idx),
                Stub {
                    pin: node,
                    end,
                    heading: escape,
                },
            );
        }
    }

    fn manhattan(a: Node, b: Node) -> i32 {
        (a.0 - b.0).abs() + (a.1 - b.1).abs()
    }

    /// Joins the stubs of `pins` (and what the net already has) into one tree of wires, and
    /// returns the nodes it covers.
    fn route_tree(
        &mut self,
        net: usize,
        pins: &[(usize, usize)],
        mut tree: BTreeSet<Node>,
    ) -> BTreeSet<Node> {
        let id = Self::net_id(net);
        let mut todo: Vec<(usize, usize)> = pins
            .iter()
            .copied()
            .filter(|k| self.stubs.contains_key(k))
            .collect();
        todo.sort_by_key(|k| (self.stubs[k].pin.0, self.stubs[k].pin.1));
        let add_stub = |tree: &mut BTreeSet<Node>, s: &Stub| {
            let (dx, dy) = ((s.end.0 - s.pin.0).signum(), (s.end.1 - s.pin.1).signum());
            let mut k = s.pin;
            tree.insert(k);
            while k != s.end {
                k = (k.0 + dx, k.1 + dy);
                tree.insert(k);
            }
        };
        if tree.is_empty() && !todo.is_empty() {
            let first = todo.remove(0);
            add_stub(&mut tree, &self.stubs[&first]);
        }
        while !todo.is_empty() {
            // The pin nearest the tree goes next.
            let (at, _) = todo
                .iter()
                .enumerate()
                .map(|(i, k)| {
                    let end = self.stubs[k].end;
                    let d = tree
                        .iter()
                        .map(|&t| Self::manhattan(end, t))
                        .min()
                        .unwrap_or(0);
                    (i, (d, k.0, k.1))
                })
                .min_by_key(|&(_, key)| key)
                .expect("a pin");
            let key = todo.remove(at);
            let (end, heading, pin) = {
                let s = &self.stubs[&key];
                (s.end, s.heading, s.pin)
            };
            let targets: HashSet<Node> = tree.iter().copied().collect();
            match self.grid.route(id, end, heading, &targets) {
                Some(path) => {
                    self.grid.commit(&path, id);
                    self.push_wire(net, &path);
                    tree.extend(path.iter().copied());
                }
                None => {
                    // Nothing clear: an L of wire, so that the net is joined all the same.
                    let &to = tree
                        .iter()
                        .min_by_key(|&&t| Self::manhattan(end, t))
                        .expect("a tree");
                    let path = vec![end, (end.0, to.1), to];
                    self.push_wire(net, &path);
                    self.grid.commit(&path, id);
                    tree.extend(path.iter().copied());
                }
            }
            add_stub(&mut tree, &self.stubs[&key]);
            tree.insert(pin);
        }
        tree
    }

    fn pins_of(&self, net: usize) -> Vec<(usize, usize)> {
        self.pins
            .iter()
            .filter(|p| p.net == net && !self.joined.contains(&(p.part, p.idx)))
            .map(|p| (p.part, p.idx))
            .collect()
    }

    fn route_signals(&mut self) {
        let mut nets: Vec<(i32, usize)> = (0..self.ctx.design.nets.len())
            .filter(|&n| self.ctx.power[n].is_none() && self.active(n))
            .map(|n| {
                let nodes: Vec<Node> = self
                    .pins
                    .iter()
                    .filter(|p| p.net == n)
                    .map(|p| p.node)
                    .collect();
                let (lo, hi) = nodes
                    .iter()
                    .fold(((i32::MAX, i32::MAX), (i32::MIN, i32::MIN)), |(l, h), q| {
                        ((l.0.min(q.0), l.1.min(q.1)), (h.0.max(q.0), h.1.max(q.1)))
                    });
                (
                    if nodes.is_empty() {
                        0
                    } else {
                        (hi.0 - lo.0) + (hi.1 - lo.1)
                    },
                    n,
                )
            })
            .collect();
        nets.sort();
        for (_, net) in nets {
            let pins = self.pins_of(net);
            let tree = self.trees.remove(&net).unwrap_or_default();
            let tree = self.route_tree(net, &pins, tree);
            self.trees.insert(net, tree);
        }
    }

    fn rect_is_free(&self, r: Rect) -> bool {
        let r = inflate(r, 0.2);
        let lo = self.grid.node(Pt::new(r.min.x, r.min.y));
        let hi = self.grid.node(Pt::new(r.max.x, r.max.y));
        for j in lo.1..=hi.1 {
            for i in lo.0..=hi.0 {
                if !self.grid.is_free((i, j)) {
                    return false;
                }
            }
        }
        true
    }

    fn route_power(&mut self) {
        for net in 0..self.ctx.design.nets.len() {
            let Some(kind) = self.ctx.power[net] else {
                continue;
            };
            let pins = self.pins_of(net);
            let mut keys: Vec<(usize, usize)> = pins
                .into_iter()
                .filter(|k| self.stubs.contains_key(k))
                .collect();
            keys.sort_by_key(|k| (self.stubs[k].pin.0, self.stubs[k].pin.1));
            // Pins near each other share a symbol.
            let mut clusters: Vec<Vec<(usize, usize)>> = Vec::new();
            for k in keys {
                let end = self.stubs[&k].end;
                let home = clusters.iter().position(|c| {
                    c.iter()
                        .any(|q| Self::manhattan(self.stubs[q].end, end) <= POWER_JOIN)
                });
                match home {
                    Some(i) => clusters[i].push(k),
                    None => clusters.push(vec![k]),
                }
            }
            for cluster in clusters {
                let tree = self.route_tree(net, &cluster, BTreeSet::new());
                self.add_symbol(net, kind, &tree);
            }
        }
    }

    fn add_symbol(&mut self, net: usize, kind: PowerKind, tree: &BTreeSet<Node>) {
        let id = Self::net_id(net);
        let dir = if kind == PowerKind::Supply {
            Dir::Up
        } else {
            Dir::Down
        };
        let mut cands: Vec<Node> = tree.iter().copied().collect();
        cands.sort_by_key(|n| (if kind == PowerKind::Supply { n.1 } else { -n.1 }, n.0));
        let name = ellipsize(&self.ctx.design.nets[net].name);
        let named = kind == PowerKind::Supply || self.ctx.label_grounds;
        let (dx, dy) = dir.vector();
        let at_of = |q: Node, grid: &Grid| {
            grid.pt((q.0 + dx as i32 * STEM_CELLS, q.1 + dy as i32 * STEM_CELLS))
        };
        let mut pick: Option<(Node, Option<Text>)> = None;
        for &q in &cands {
            if self.grid.owner(q, 1) != grid::NONE && self.grid.owner(q, 1) != id {
                continue;
            }
            if !self.grid.run_is_free(q, dir, STEM_CELLS + 2) {
                continue;
            }
            let at = at_of(q, &self.grid);
            let glyph = PowerSymbol {
                net,
                kind,
                at,
                dir,
                label: None,
            }
            .glyph_extent();
            if !self.rect_is_free(glyph) {
                continue;
            }
            let label = if named {
                [Side::Right, Side::Left, Side::Beyond]
                    .into_iter()
                    .map(|s| label_beside(at, dir, &name, s))
                    .find(|t| self.rect_is_free(t.extent()))
            } else {
                None
            };
            if named && label.is_none() {
                continue;
            }
            pick = Some((q, label));
            break;
        }
        let (q, label) = pick.unwrap_or_else(|| {
            // Nowhere is clear: past the highest (or lowest) of the wires, by crossing.
            let q = cands[0];
            let at = at_of(q, &self.grid);
            let label = named.then(|| label_beside(at, dir, &name, Side::Right));
            (q, label)
        });
        let end = (q.0 + dx as i32 * STEM_CELLS, q.1 + dy as i32 * STEM_CELLS);
        self.grid.claim_wire(q, end, id);
        self.push_wire(net, &[q, end]);
        let at = self.grid.pt(end);
        let symbol = PowerSymbol {
            net,
            kind,
            at,
            dir,
            label,
        };
        self.grid.block_rect(inflate(symbol.extent(), 0.1));
        self.symbols.push(symbol);
    }

    /// A flag at the end of a short wire for each net that goes on to another sheet.
    fn route_ports(&mut self) {
        for net in 0..self.ctx.design.nets.len() {
            if !self.ctx.external[net] || self.ctx.power[net].is_some() || self.local[net] == 0 {
                continue;
            }
            let name = self.ctx.design.nets[net].name.clone();
            let label = is_named(&name).then(|| ellipsize(&name));
            let length = label.as_ref().map_or(3.0, |l| text_width(l) + 3.6);
            let tree: BTreeSet<Node> = self.trees.get(&net).cloned().unwrap_or_else(|| {
                self.pins
                    .iter()
                    .filter(|p| p.net == net)
                    .flat_map(|p| {
                        self.stubs
                            .get(&(p.part, p.idx))
                            .map(|s| [s.pin, s.end])
                            .into_iter()
                            .flatten()
                    })
                    .collect()
            });
            // Out of the pins' own stubs first (the way they point, if that is sideways), then
            // out of the ends of the wires. A flag points left or right, so its name reads.
            let mut cands: Vec<(Node, Dir)> = Vec::new();
            for p in self.pins.iter().filter(|p| p.net == net) {
                if let Some(s) = self.stubs.get(&(p.part, p.idx)) {
                    if s.heading.is_vertical() {
                        cands.push((s.end, Dir::Right));
                        cands.push((s.end, Dir::Left));
                    } else {
                        cands.push((s.end, s.heading));
                    }
                }
            }
            for dir in [Dir::Left, Dir::Right] {
                let pick = match dir {
                    Dir::Left => tree.iter().min_by_key(|n| (n.0, n.1)),
                    _ => tree.iter().max_by_key(|n| (n.0, -n.1)),
                };
                if let Some(&n) = pick {
                    cands.push((n, dir));
                }
            }
            // Anywhere on the wires, if the ends are crowded.
            let mut rest: Vec<Node> = tree.iter().copied().collect();
            rest.sort_by_key(|n| (n.1, n.0));
            for &n in rest.iter().take(300) {
                cands.push((n, Dir::Right));
                cands.push((n, Dir::Left));
            }
            let cells = (length / GRID).ceil() as i32 + 1;
            let mut done = false;
            'find: for extra in 0..=30 {
                for &(q, dir) in &cands {
                    let (dx, dy) = dir.vector();
                    let tip = (q.0 + dx as i32 * (1 + extra), q.1 + dy as i32 * (1 + extra));
                    if !self.grid.run_is_free(q, dir, 1 + extra + cells) {
                        continue;
                    }
                    let tip_pt = self.grid.pt(tip);
                    let port = Port {
                        net,
                        at: tip_pt,
                        dir,
                        length,
                        label: label.as_ref().map(|l| port_label(l, tip_pt, dir)),
                    };
                    if !self.rect_is_free(port.extent()) {
                        continue;
                    }
                    let id = Self::net_id(net);
                    self.grid.claim_wire(q, tip, id);
                    self.push_wire(net, &[q, tip]);
                    self.grid.block_rect(inflate(port.extent(), 0.1));
                    self.ports.push(port);
                    done = true;
                    break 'find;
                }
            }
            // Or up or down from a wire, a short way, and then the flag.
            if !done {
                let mut nodes: Vec<Node> = cands.iter().map(|c| c.0).collect();
                nodes.dedup();
                'vertical: for stem in 3..=10 {
                    for &q in &nodes {
                        for vdir in [Dir::Up, Dir::Down] {
                            if !self.grid.run_is_free(q, vdir, stem) {
                                continue;
                            }
                            let (dx, dy) = vdir.vector();
                            let tip = (q.0 + dx as i32 * stem, q.1 + dy as i32 * stem);
                            for fdir in [Dir::Right, Dir::Left] {
                                if !self.grid.run_is_free(tip, fdir, cells) {
                                    continue;
                                }
                                let tip_pt = self.grid.pt(tip);
                                let port = Port {
                                    net,
                                    at: tip_pt,
                                    dir: fdir,
                                    length,
                                    label: label.as_ref().map(|l| port_label(l, tip_pt, fdir)),
                                };
                                if !self.rect_is_free(port.extent()) {
                                    continue;
                                }
                                self.grid.claim_wire(q, tip, Self::net_id(net));
                                self.push_wire(net, &[q, tip]);
                                self.grid.block_rect(inflate(port.extent(), 0.1));
                                self.ports.push(port);
                                done = true;
                                break 'vertical;
                            }
                        }
                    }
                }
            }
            if !done && let Some(&(q, dir)) = cands.first() {
                let (dx, dy) = dir.vector();
                let tip_node = (q.0 + dx as i32, q.1 + dy as i32);
                let tip = self.grid.pt(tip_node);
                self.grid.claim_wire(q, tip_node, Self::net_id(net));
                self.push_wire(net, &[q, tip_node]);
                self.ports.push(Port {
                    net,
                    at: tip,
                    dir,
                    length,
                    label: label.as_ref().map(|l| port_label(l, tip, dir)),
                });
            }
        }
    }

    fn pin_marks(&self) -> Vec<PinMark> {
        let mut out = Vec::new();
        for (p, part) in self.parts.iter().enumerate() {
            let refdes = &self.ctx.design.parts[p].refdes;
            for (i, (pin, g)) in part.pins.iter().zip(&part.glyph.pins).enumerate() {
                let _ = i;
                if g.hidden {
                    continue;
                }
                let must = self.marks.must.get(&(refdes.clone(), g.id.clone()));
                let open = match pin.net {
                    None => true,
                    Some(n) => {
                        self.local[n] <= 1 && !self.ctx.external[n] && self.ctx.power[n].is_none()
                    }
                };
                if !open && must.is_none() {
                    continue;
                }
                out.push(PinMark {
                    part: p,
                    pad: g.id.clone(),
                    at: pin.at,
                    kind: match must {
                        Some(m) => MarkKind::Must(m.clone()),
                        None => MarkKind::Open,
                    },
                    pin_name: g.name.clone(),
                    pin_kind: g.kind.to_string(),
                });
            }
        }
        out
    }

    fn finish(mut self) -> Layout {
        self.join_and_stub();
        self.route_signals();
        self.route_power();
        self.route_ports();
        let marks = self.pin_marks();
        let junctions = junctions(&self.wires);

        let mut bounds = self
            .parts
            .first()
            .map_or(Rect::around(Pt::new(0.0, 0.0)), |p| p.extent);
        for p in &self.parts {
            bounds.union(p.extent);
        }
        for w in &self.wires {
            for &pt in &w.points {
                bounds.include(pt);
            }
        }
        for s in &self.symbols {
            bounds.union(s.extent());
        }
        for p in &self.ports {
            bounds.union(p.extent());
            if let Some(l) = &p.label {
                bounds.union(l.extent());
            }
        }
        bounds.min = Pt::new(
            snap(bounds.min.x - BORDER, GRID),
            snap(bounds.min.y - BORDER, GRID),
        );
        bounds.max = Pt::new(
            snap(bounds.max.x + BORDER, GRID),
            snap(bounds.max.y + BORDER, GRID),
        );
        Layout {
            parts: self.parts,
            symbols: self.symbols,
            ports: self.ports,
            marks,
            sheets: Vec::new(),
            wires: self.wires,
            junctions,
            labels: Vec::new(),
            bounds,
        }
    }
}
