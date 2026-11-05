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
/// Room above and below a group of parts for its rails and their symbols.
const RAIL_ROOM: f64 = 20.32;
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
    let gap = text_gap(&part.pins, side);
    let (reference, value) = part_texts(&ctx.design.parts[part.part], body, side, gap);
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
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut satellites: BTreeSet<usize> = BTreeSet::new();
    groups.push(Vec::new());
    for b in &blocks {
        let ext = placed[b.anchor].extent;
        let by = Pt::new(
            snap_up(x - b.rect.min.x, GRID),
            -snap_grid((ext.min.y + ext.max.y) / 2.0),
        );
        for &(p, off, side) in &b.members {
            at[p] = Pt::new(by.x + off.x, by.y + off.y);
            text_side[p] = side;
            groups[0].push(p);
            if p != b.anchor {
                satellites.insert(p);
            }
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
    let top = area.map_or(0.0, |a| a.max.y + 12.7 + 2.0 * RAIL_ROOM);
    let (mut fx, mut fy, mut row) = (0.0, top, 0.0f64);
    groups.push(Vec::new());
    for (u, r) in &rects {
        if fx > 0.0 && fx + r.width() > target {
            fx = 0.0;
            fy += row + 2.0 * RAIL_ROOM;
            row = 0.0;
            groups.push(Vec::new());
        }
        let by = Pt::new(snap_up(fx - r.min.x, GRID), snap_up(fy - r.min.y, GRID));
        for (w, &p) in units[*u].parts.iter().enumerate() {
            at[p] = Pt::new(
                by.x + shapes[*u].offsets[w].x,
                by.y + shapes[*u].offsets[w].y,
            );
            groups.last_mut().expect("a group").push(p);
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

    // The flags of the whole sheet stand in two columns beyond everything, so none can be
    // over a part, a text or another flag.
    let regions: Vec<Vec<usize>> = vec![(0..n).collect()];
    groups.retain(|g| !g.is_empty());
    Scene::new(ctx, placed, units, marks, groups, regions, satellites).finish()
}

/// The name inside a flag whose point is at `tip` and that points `dir`.
fn port_label(name: &str, tip: Pt, dir: Dir) -> Text {
    let start = tip.step(dir, 1.1);
    let anchor = if dir == Dir::Left {
        Anchor::End
    } else {
        Anchor::Start
    };
    Text::new(name, Pt::new(start.x, start.y + 0.3), anchor)
}

/// How long a flag is for `name`, from its point.
fn port_length(name: Option<&str>) -> f64 {
    name.map_or(3.0, |n| {
        n.chars().count() as f64 * Port::TEXT * CHAR_WIDTH + 2.2
    })
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
    // The capacitors that decouple a supply pin stand in a row, as far from the part as each
    // other, the same distance apart, level, hanging from the supply rail above and standing
    // on the ground rail below.
    let is_row = |x: &&SatPlan| {
        x.anchor == anchor
            && ctx.power[x.net] == Some(PowerKind::Supply)
            && !a.pins[x.pin].escape.is_vertical()
    };
    let mut done_rows: Vec<&SatPlan> = Vec::new();
    for side in [Dir::Left, Dir::Right] {
        let row: Vec<&SatPlan> = sorted
            .iter()
            .map(|x| **x)
            .filter(|x| is_row(x) && a.pins[x.pin].escape == side)
            .collect();
        if row.is_empty() {
            continue;
        }
        let first = row
            .iter()
            .map(|x| a.pins[x.pin].at)
            .min_by(|p, q| p.y.total_cmp(&q.y))
            .expect("a pin");
        let e0 = first.step(side, STUB);
        let bases: Vec<Rect> = row
            .iter()
            .map(|x| unit_rect(ctx, placed, &units[x.unit], &shapes[x.unit], Some(side)))
            .collect();
        let widest = bases.iter().map(Rect::width).fold(0.0, f64::max);
        let pitch = snap_up(widest + 3.81, GRID).max(7.62);
        'rows: for k in 0..16 {
            let dx0 = 10.16 + 5.08 * f64::from(k);
            let mut places: Vec<(Pt, Rect)> = Vec::new();
            for (i, (x, base)) in row.iter().zip(&bases).enumerate() {
                let target = Pt::new(e0.x + side.vector().0 * (dx0 + i as f64 * pitch), e0.y);
                let off = Pt::new(snap_grid(target.x - x.r.x), snap_grid(target.y - x.r.y));
                let foot = inflate(base.translated(off), MARGIN);
                if obstacles.iter().any(|o| overlaps(&foot, o)) {
                    continue 'rows;
                }
                places.push((off, foot));
            }
            for (x, (off, foot)) in row.iter().zip(places) {
                let unit = &units[x.unit];
                let shape = &shapes[x.unit];
                tied[x.unit] = true;
                for (w, &p) in unit.parts.iter().enumerate() {
                    members.push((
                        p,
                        Pt::new(off.x + shape.offsets[w].x, off.y + shape.offsets[w].y),
                        Some(side),
                    ));
                }
                obstacles.push(foot);
                rect.union(foot);
                done_rows.push(x);
            }
            break;
        }
    }
    sorted.retain(|x| !done_rows.iter().any(|d| std::ptr::eq(*d, **x)));
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
        // Level with the pin first, as far out as it takes; only then a row above or below.
        'search: for dy in [
            0.0, 5.08, -5.08, 10.16, -10.16, 15.24, -15.24, 20.32, -20.32, 25.4, -25.4, 30.48,
            -30.48, 38.1, -38.1,
        ] {
            for k in 0..20 {
                let dx = 7.62 + 5.08 * f64::from(k);
                for &sd in &sides {
                    let base = unit_rect(ctx, placed, unit, shape, Some(sd));
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
    /// The parts that are drawn together: the row of blocks, then each row of loose parts.
    /// A rail is one wire across the group, with one symbol.
    groups: Vec<Vec<usize>>,
    /// The parts of each block (an anchor and its satellites), then of each row of loose parts:
    /// where the flags of a part's nets go, beside its own block.
    regions: Vec<Vec<usize>>,
    /// The parts that stand beside a pin of another part.
    satellites: BTreeSet<usize>,
}

impl<'a> Scene<'a> {
    fn new(
        ctx: &'a Ctx<'a>,
        parts: Vec<PlacedPart>,
        units: &'a [Unit],
        marks: &'a Marks,
        groups: Vec<Vec<usize>>,
        regions: Vec<Vec<usize>>,
        satellites: BTreeSet<usize>,
    ) -> Self {
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
            groups,
            regions,
            satellites,
        };
        scene.block_parts();
        scene.collect_pins();
        scene.block_open_pins();
        scene
    }

    /// An X is at the end of every pin with nothing on it, and nothing else is near it.
    fn block_open_pins(&mut self) {
        let mut rects = Vec::new();
        for (p, part) in self.parts.iter().enumerate() {
            for (pin, g) in part.pins.iter().zip(&part.glyph.pins) {
                if g.hidden {
                    continue;
                }
                let open = match pin.net {
                    None => true,
                    Some(n) => !self.active(n),
                };
                let _ = p;
                if open {
                    rects.push(inflate(Rect::around(pin.at), 1.5));
                }
            }
        }
        for r in rects {
            self.grid.block_rect(r);
        }
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
                self.grid.block_rect(inflate(t.extent(), 0.9));
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

    /// The nodes of the stub ends of a part's pins on one side, with what is between.
    fn along(node: Node, escape: Dir) -> i32 {
        if escape.is_vertical() { node.0 } else { node.1 }
    }

    /// Each rail is one wire across its group, at the top (a supply) or the bottom (a ground)
    /// beyond every part, with one symbol; the pins hang from it by straight wires. Pins next
    /// to each other on one side share a bus first.
    fn route_rails(&mut self) {
        for g in 0..self.groups.len() {
            let mut rect: Option<Rect> = None;
            for &p in &self.groups[g] {
                union_into(&mut rect, self.parts[p].extent);
            }
            let Some(rect) = rect else {
                continue;
            };
            let in_group: BTreeSet<usize> = self.groups[g].iter().copied().collect();
            let mut nets: Vec<usize> = self
                .pins
                .iter()
                .filter(|p| {
                    in_group.contains(&p.part)
                        && self.ctx.power[p.net].is_some()
                        && self.stubs.contains_key(&(p.part, p.idx))
                })
                .map(|p| p.net)
                .collect();
            nets.sort_unstable();
            nets.dedup();
            let (mut supplies, mut grounds) = (0, 0);
            for net in nets {
                let kind = self.ctx.power[net].expect("a rail");
                let k = if kind == PowerKind::Supply {
                    supplies += 1;
                    supplies - 1
                } else {
                    grounds += 1;
                    grounds - 1
                };
                self.rail(net, kind, k, rect, &in_group);
            }
        }
    }

    fn rail(
        &mut self,
        net: usize,
        kind: PowerKind,
        k: i32,
        rect: Rect,
        in_group: &BTreeSet<usize>,
    ) {
        let id = Self::net_id(net);
        let natural = if kind == PowerKind::Supply {
            Dir::Up
        } else {
            Dir::Down
        };
        let mut keys: Vec<(usize, usize)> = self
            .pins
            .iter()
            .filter(|p| p.net == net && in_group.contains(&p.part))
            .map(|p| (p.part, p.idx))
            .filter(|key| self.stubs.contains_key(key))
            .collect();
        keys.sort_by_key(|key| {
            let s = &self.stubs[key];
            (key.0, s.heading as u8, Self::along(s.end, s.heading))
        });
        // Runs: neighbours on one side of one part, with no other pin between them.
        let mut runs: Vec<Vec<(usize, usize)>> = Vec::new();
        for key in keys {
            let s = &self.stubs[&key];
            let (end, heading) = (s.end, s.heading);
            let joins = runs.last().is_some_and(|run| {
                let last = &self.stubs[run.last().expect("a run has a pin")];
                let (a, b) = (Self::along(last.end, heading), Self::along(end, heading));
                last.heading == heading
                    && run[0].0 == key.0
                    && b - a <= 2
                    && !self.parts[key.0].pins.iter().any(|q| {
                        let node = self.grid.node(q.at);
                        let c = Self::along(node, heading);
                        q.escape == heading && c > a && c < b
                    })
            });
            if joins {
                runs.last_mut().expect("a run to join").push(key);
            } else {
                runs.push(vec![key]);
            }
        }
        struct Drop {
            node: Node,
            heading: Dir,
            part: usize,
        }
        let mut drops: Vec<Drop> = Vec::new();
        for run in &runs {
            let ends: Vec<Node> = run.iter().map(|key| self.stubs[key].end).collect();
            let heading = self.stubs[&run[0]].heading;
            if ends.len() > 1 {
                let (first, last) = (ends[0], ends[ends.len() - 1]);
                self.grid.claim_wire(first, last, id);
                self.push_wire(net, &[first, last]);
            }
            let (node, h) = if heading.is_vertical() {
                let mean = ends.iter().map(|n| n.0).sum::<i32>() as f64 / ends.len() as f64;
                let hub = *ends
                    .iter()
                    .min_by(|a, b| {
                        (f64::from(a.0) - mean)
                            .abs()
                            .total_cmp(&(f64::from(b.0) - mean).abs())
                    })
                    .expect("a pin");
                (hub, heading)
            } else if ends.len() > 1 {
                let pick = if kind == PowerKind::Supply {
                    ends.iter().min_by_key(|n| n.1)
                } else {
                    ends.iter().max_by_key(|n| n.1)
                };
                (*pick.expect("a pin"), natural)
            } else {
                (ends[0], heading)
            };
            drops.push(Drop {
                node,
                heading: h,
                part: run[0].0,
            });
        }
        // The rail's row, and what it spans.
        let top = self.grid.node(Pt::new(0.0, rect.min.y)).1;
        let bottom = self.grid.node(Pt::new(0.0, rect.max.y)).1;
        let row = if kind == PowerKind::Supply {
            top - (4 + 3 * k)
        } else {
            bottom + (4 + 3 * k)
        };
        let xs: Vec<i32> = drops.iter().map(|d| d.node.0).collect();
        let (mut lo, mut hi) = (
            *xs.iter().min().expect("a drop"),
            *xs.iter().max().expect("a drop"),
        );
        // Where the symbol hangs: in line with the outermost satellite on the rail, the part's
        // own centre when there is one part, else the left end.
        let centre = self
            .grid
            .node(Pt::new((rect.min.x + rect.max.x) / 2.0, 0.0))
            .0;
        let parts_on: BTreeSet<usize> = drops.iter().map(|d| d.part).collect();
        let sat_xs: Vec<i32> = drops
            .iter()
            .filter(|d| self.satellites.contains(&d.part))
            .map(|d| d.node.0)
            .collect();
        let preferred = if let Some(&x) = sat_xs.iter().max_by_key(|&&x| ((x - centre).abs(), -x)) {
            x
        } else if parts_on.len() == 1 {
            (lo + hi) / 2
        } else {
            lo
        };
        // The symbol's place decides how far the rail goes: if it is outside the drops, the
        // rail reaches it.
        let mut order: Vec<i32> = vec![preferred];
        order.extend(xs.iter().copied());
        for d in 1..=14 {
            order.push(lo - d);
            order.push(hi + d);
        }
        order.dedup();
        let mut chosen: Option<(i32, Option<Text>)> = None;
        for &x in &order {
            if let Some(label) = self.symbol_fits(net, kind, (x, row)) {
                chosen = Some((x, label));
                break;
            }
        }
        let sym_x = chosen.as_ref().map_or(preferred, |c| c.0);
        lo = lo.min(sym_x);
        hi = hi.max(sym_x);
        self.grid.claim_wire((lo, row), (hi, row), id);
        if hi > lo {
            self.push_wire(net, &[(lo, row), (hi, row)]);
        }
        let targets: HashSet<Node> = (lo..=hi).map(|x| (x, row)).collect();
        for d in &drops {
            match self.grid.route(id, d.node, d.heading, &targets) {
                Some(path) => {
                    self.grid.commit(&path, id);
                    self.push_wire(net, &path);
                }
                None => {
                    let to = (d.node.0, row);
                    self.push_wire(net, &[d.node, to]);
                }
            }
        }
        // The symbol, at the end of a stem from the rail.
        let (dx, dy) = natural.vector();
        let q = (sym_x, row);
        let end = (q.0 + dx as i32 * STEM_CELLS, q.1 + dy as i32 * STEM_CELLS);
        self.grid.claim_wire(q, end, id);
        self.push_wire(net, &[q, end]);
        let at = self.grid.pt(end);
        let name = ellipsize(&self.ctx.design.nets[net].name);
        let named = kind == PowerKind::Supply || self.ctx.label_grounds;
        let label = match chosen {
            Some((_, l)) => l,
            None => named.then(|| label_beside(at, natural, &name, Side::Right)),
        };
        let symbol = PowerSymbol {
            net,
            kind,
            at,
            dir: natural,
            label,
        };
        self.grid.block_rect(inflate(symbol.extent(), 0.9));
        self.symbols.push(symbol);
    }

    /// Whether a symbol can hang at `q` of a rail: the stem and the drawing are clear of
    /// everything, and its name has room. Gives the name's place.
    fn symbol_fits(&self, net: usize, kind: PowerKind, q: Node) -> Option<Option<Text>> {
        let dir = if kind == PowerKind::Supply {
            Dir::Up
        } else {
            Dir::Down
        };
        let (dx, dy) = dir.vector();
        let end = (q.0 + dx as i32 * STEM_CELLS, q.1 + dy as i32 * STEM_CELLS);
        if !self.grid.run_is_free(q, dir, STEM_CELLS + 2) && !self.grid.is_free(q) {
            // The rail's own row is free before it is claimed; only the stem is tested.
        }
        let stem_free = (1..=STEM_CELLS + 2).all(|k| {
            self.grid
                .is_free((q.0 + dx as i32 * k, q.1 + dy as i32 * k))
        });
        if !stem_free || self.grid.is_blocked(q) {
            return None;
        }
        let at = self.grid.pt(end);
        let glyph = PowerSymbol {
            net,
            kind,
            at,
            dir,
            label: None,
        }
        .glyph_extent();
        if !self.rect_is_free(inflate(glyph, 0.3)) {
            return None;
        }
        let named = kind == PowerKind::Supply || self.ctx.label_grounds;
        if !named {
            return Some(None);
        }
        let name = ellipsize(&self.ctx.design.nets[net].name);
        [Side::Right, Side::Left, Side::Beyond]
            .into_iter()
            .map(|s| label_beside(at, dir, &name, s))
            .find(|t| self.rect_is_free(inflate(t.extent(), 0.3)))
            .map(Some)
    }

    /// A flag for each net that goes on to another sheet, in a column of its own outside the
    /// parts, each on a row of its own, one pin pitch apart, joined to the net's wires.
    fn route_ports(&mut self) {
        let mut group_rect: Vec<Option<Rect>> = Vec::new();
        for g in &self.regions {
            let mut r: Option<Rect> = None;
            for &p in g {
                union_into(&mut r, self.parts[p].extent);
            }
            group_rect.push(r);
        }
        struct Want {
            net: usize,
            group: usize,
            side: Dir,
            row: i32,
            start: Node,
            heading: Dir,
            label: Option<String>,
            length: f64,
        }
        let mut wants: Vec<Want> = Vec::new();
        for net in 0..self.ctx.design.nets.len() {
            if !self.ctx.external[net] || self.ctx.power[net].is_some() || self.local[net] == 0 {
                continue;
            }
            // The pin the flag is for: of the part with the most pins, in pin order.
            let mut pins: Vec<&PinInfo> = self.pins.iter().filter(|p| p.net == net).collect();
            pins.sort_by_key(|p| {
                (
                    std::cmp::Reverse(self.parts[p.part].pins.len()),
                    p.part,
                    p.idx,
                )
            });
            let Some(pin) = pins.first() else {
                continue;
            };
            let Some(stub) = self.stubs.get(&(pin.part, pin.idx)) else {
                continue;
            };
            let group = self
                .regions
                .iter()
                .position(|g| g.contains(&pin.part))
                .unwrap_or(0);
            let name = &self.ctx.design.nets[net].name;
            let label = is_named(name).then(|| ellipsize(name));
            wants.push(Want {
                net,
                group,
                side: if pin.escape == Dir::Left {
                    Dir::Left
                } else {
                    Dir::Right
                },
                row: if pin.escape.is_vertical() {
                    i32::MAX
                } else {
                    stub.end.1
                },
                start: stub.end,
                heading: stub.heading,
                length: port_length(label.as_deref()),
                label,
            });
        }
        let mut keys: Vec<(usize, Dir)> = wants.iter().map(|w| (w.group, w.side)).collect();
        keys.sort_by_key(|(g, d)| (*g, *d as u8));
        keys.dedup();
        for (group, side) in keys {
            let Some(rect) = group_rect[group] else {
                continue;
            };
            let mut idxs: Vec<usize> = (0..wants.len())
                .filter(|&i| wants[i].group == group && wants[i].side == side)
                .collect();
            idxs.sort_by_key(|&i| (wants[i].row, wants[i].net));
            // Rows are the pins' own rows, two apart at least (one pin pitch); a pin that
            // leaves up or down gets a row below the group, after the others.
            let below = self.grid.node(Pt::new(0.0, rect.max.y)).1 + 8;
            let mut last = i32::MIN / 2;
            for &i in &idxs {
                let want = if wants[i].row == i32::MAX {
                    below.max(last + 2)
                } else {
                    wants[i].row
                };
                let row = want.max(last + 2);
                wants[i].row = row;
                last = row;
            }
            // One column for all of them: as near as every flag fits.
            let dir = side;
            let (sx, _) = dir.vector();
            let edge = if dir == Dir::Left {
                rect.min.x
            } else {
                rect.max.x
            };
            let base = self.grid.node(Pt::new(edge, 0.0)).0 + sx as i32 * 6;
            let mut column = base;
            'column: for extra in 0..=90 {
                let x = base + sx as i32 * extra * 2;
                let all = idxs.iter().all(|&i| {
                    let w = &wants[i];
                    let tip = self.grid.pt((x, w.row));
                    let port = Port {
                        net: w.net,
                        at: tip,
                        dir,
                        length: w.length,
                        label: None,
                    };
                    self.rect_is_free(inflate(port.extent(), 0.3))
                });
                if all {
                    column = x;
                    break 'column;
                }
            }
            for &i in &idxs {
                let w = &wants[i];
                let tip_node = (column, w.row);
                let tip = self.grid.pt(tip_node);
                let port = Port {
                    net: w.net,
                    at: tip,
                    dir,
                    length: w.length,
                    label: w.label.as_ref().map(|l| port_label(l, tip, dir)),
                };
                let id = Self::net_id(w.net);
                // The flag's body is in the way of its own wire: the wire reaches its point.
                self.grid.block_rect(inflate(port.extent(), 0.9));
                self.grid.unblock(tip_node);
                let targets: HashSet<Node> = HashSet::from([tip_node]);
                match self.grid.route(id, w.start, w.heading, &targets) {
                    Some(path) => {
                        self.grid.commit(&path, id);
                        self.push_wire(w.net, &path);
                    }
                    None => {
                        let path = vec![w.start, (w.start.0, tip_node.1), tip_node];
                        self.push_wire(w.net, &path);
                    }
                }
                self.grid.block_rect(inflate(port.extent(), 0.9));
                self.ports.push(port);
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
        self.route_rails();
        self.route_signals();
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
