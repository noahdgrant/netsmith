//! Building the block of one main part: its satellites placed round it.

use super::*;

/// The satellites of one anchor placed round it, each as near the pin it serves as it can be
/// without covering anything.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_block(
    ctx: &Ctx,
    placed: &[PlacedPart],
    units: &[Unit],
    shapes: &[Shape],
    anchor: usize,
    sats: &[&SatPlan],
    tied: &mut [bool],
    tagged: &[bool],
    rooms: &[Rect],
    spine: Option<&crate::layout::spine::Spine>,
) -> Block {
    let a = &placed[anchor];
    // What the anchor covers: its body and pins, and each of its texts (not the one box
    // round them all, which would keep a satellite out of the strip beside the texts).
    let mut obstacles: Vec<Rect> = vec![inflate(body_of(&a.glyph, a.orient), 0.5)];
    for t in std::iter::once(&a.reference).chain(a.value.as_ref()) {
        obstacles.push(inflate(t.extent(), 0.5));
    }
    let mut members: Vec<(usize, Pt, Option<Dir>)> = vec![(anchor, Pt::new(0.0, 0.0), None)];
    let mut rect = a.extent;
    // The names and flags at the anchor's pins keep their room; but at a pin that has parts
    // beside it the wire goes on past them (as a person draws it) and the name is beyond:
    // that room is kept once they are placed.
    let served: BTreeSet<usize> = sats
        .iter()
        .filter(|x| x.anchor == anchor)
        .map(|x| x.pin)
        .collect();
    for i in 0..a.pins.len() {
        if served.contains(&i) {
            continue;
        }
        if let Some(r) = tag_rect_of(ctx, a, i, tagged) {
            obstacles.push(inflate(r, 0.5));
            rect.union(r);
        }
    }
    // The entries, spine and writing of the buses at this part.
    for r in rooms {
        obstacles.push(inflate(*r, 0.5));
        rect.union(*r);
    }
    // The corridors of the rows of a spine are kept free of the satellites placed before it.
    let lanes: Vec<Rect> = spine.map_or_else(Vec::new, |s| spine::lanes(ctx, placed, s));
    obstacles.extend(lanes.iter().copied());
    // The satellites of the anchor's own pins first, from the top down; then those that stand
    // beside a satellite, in the order they were found.
    let mut sorted: Vec<&&SatPlan> = sats.iter().filter(|x| x.anchor == anchor).collect();
    // Beside a spine the lower pin's satellite goes nearest: the wire of a pin above passes over
    // it, and nothing is crossed.
    let by = |bottom_first: bool| {
        move |x: &&&SatPlan, y: &&&SatPlan| {
            let (px, py) = (&a.pins[x.pin], &a.pins[y.pin]);
            let by_y = if bottom_first {
                py.at.y.total_cmp(&px.at.y)
            } else {
                px.at.y.total_cmp(&py.at.y)
            };
            (px.escape as u8)
                .cmp(&(py.escape as u8))
                .then(by_y)
                .then(px.at.x.total_cmp(&py.at.x))
                .then(x.unit.cmp(&y.unit))
        }
    };
    if spine.is_some() {
        sorted.sort_by(by(true));
    } else {
        // The choice of `Tuning::bottom_first` matters here only when the two orders differ,
        // which is told to the context (`Ctx::touch`).
        let mut other = sorted.clone();
        sorted.sort_by(by(false));
        other.sort_by(by(true));
        if sorted
            .iter()
            .zip(&other)
            .any(|(x, y)| !std::ptr::eq(**x, **y))
        {
            ctx.touch(Tuning::BOTTOM_FIRST_BIT);
        }
        if ctx.tuning.bottom_first {
            sorted = other;
        }
    }
    sorted.extend(sats.iter().filter(|x| x.anchor != anchor));
    // The satellites across two of the anchor's pins stand beside both, nearest first.
    let mut orients: Vec<(usize, Orient)> = Vec::new();
    let mut pairs: Vec<pair::PairRoute> = Vec::new();
    let mut paired: BTreeSet<usize> = BTreeSet::new();
    let mut foots: Vec<Rect> = Vec::new();
    for plan in sorted
        .iter()
        .filter(|x| x.anchor == anchor && x.pair.is_some())
    {
        let Some(pp) = plan.pair else { continue };
        let part = units[plan.unit].parts[0];
        let Some(at) = pair::place(ctx, placed, &pp, part, &obstacles) else {
            continue;
        };
        tied[plan.unit] = true;
        paired.insert(plan.unit);
        members.push((part, at.off, Some(at.side)));
        orients.push((part, at.orient));
        pairs.push(pair::PairRoute {
            part,
            anchor,
            links: pp.links,
            order: at.order,
        });
        obstacles.push(at.foot);
        rect.union(at.foot);
        foots.push(at.foot);
        for lane in at.lanes {
            obstacles.push(lane);
            rect.union(lane);
        }
    }
    sorted.retain(|x| !paired.contains(&x.unit));
    // The capacitors that decouple a supply pin stand in a row, as far from the part as each
    // other, the same distance apart, level, hanging from the supply rail above and standing
    // on the ground rail below.
    let is_row = |x: &&SatPlan| {
        x.anchor == anchor
            && ctx.power[x.net] == Some(PowerKind::Supply)
            && !a.pins[x.pin].escape.is_vertical()
    };
    let mut done_rows: Vec<&SatPlan> = Vec::new();
    // (`foots`, above, are the boxes the satellites were placed in.)
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
                foots.push(foot);
                done_rows.push(x);
            }
            break;
        }
    }
    sorted.retain(|x| !done_rows.iter().any(|d| std::ptr::eq(*d, **x)));
    for plan in sorted {
        netsmith_core::job::checkpoint();
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
        const DYS: [f64; 15] = [
            0.0, 5.08, -5.08, 10.16, -10.16, 15.24, -15.24, 20.32, -20.32, 25.4, -25.4, 30.48,
            -30.48, 38.1, -38.1,
        ];
        let mut order: Vec<(f64, i32)> = Vec::new();
        if pin.escape.is_vertical() && ctx.power[plan.net].is_some() {
            // A supply pin on the top or the bottom edge has the body of its part just beside
            // it, so level with it is clear only far out: a place a little off the line is
            // tried first, nearest first.
            let near = |k: i32| 7.62 + 5.08 * f64::from(k) <= policy::LEVEL_MM;
            order.extend((0..20).filter(|&k| near(k)).map(|k| (0.0, k)));
            let mut off_level: Vec<(f64, i32)> = Vec::new();
            for dy in DYS.into_iter().skip(1) {
                off_level.extend((0..20).map(|k| (dy, k)));
            }
            // The cells out and the cells across (a stable sort keeps the order above among
            // equals).
            off_level.sort_by(|p, q| {
                let cost = |&(dy, k): &(f64, i32)| 7.62 + 5.08 * f64::from(k) + dy.abs();
                cost(p).total_cmp(&cost(q))
            });
            order.extend(off_level);
            order.extend((0..20).filter(|&k| !near(k)).map(|k| (0.0, k)));
        } else {
            for dy in DYS {
                order.extend((0..20).map(|k| (dy, k)));
            }
        }
        'search: for (dy, k) in order {
            {
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
                    // The wire from the pin to the part does not cross a row of the spine.
                    if !lanes.is_empty() {
                        let join = Pt::new(plan.r.x + off.x, plan.r.y + off.y);
                        let mut along = Rect::around(Pt::new(e.x.min(join.x), e.y - 0.3));
                        along.include(Pt::new(e.x.max(join.x), e.y + 0.3));
                        let mut down = Rect::around(Pt::new(join.x - 0.3, e.y.min(join.y)));
                        down.include(Pt::new(join.x + 0.3, e.y.max(join.y)));
                        if lanes
                            .iter()
                            .any(|l| overlaps(&along, l) || overlaps(&down, l))
                        {
                            continue;
                        }
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
            foots.push(foot);
        }
    }
    // The spine: its parts are placed now, the rows starting beyond the satellites of the pins.
    let mut level: Option<f64> = None;
    let mut spine_nets: Vec<(usize, i32)> = Vec::new();
    let mut spine_lanes: Vec<Rect> = Vec::new();
    if let Some(s) = spine {
        obstacles.retain(|r| !lanes.contains(r));
        let pin_nets: Vec<Vec<Option<usize>>> = (0..ctx.design.parts.len())
            .map(|p| {
                ctx.glyphs[p]
                    .pins
                    .iter()
                    .map(|pin| net_of(ctx, p, &pin.id))
                    .collect()
            })
            .collect();
        let plan = spine::place(ctx, placed, &pin_nets, s, &foots);
        for &(p, off, side) in &plan.members {
            tied[unit_index(units, p)] = true;
            members.push((p, off, side));
        }
        orients.extend(plan.orients.iter().copied());
        obstacles.extend(plan.obstacles.iter().map(|r| inflate(*r, 0.0)));
        rect.union(plan.rect);
        level = plan.level;
        spine_nets = plan.nets;
        spine_lanes = plan.lanes;
    }
    // The name at a pin with parts beside it: beyond the farthest of them.
    for &i in &served {
        let Some(own) = tag_rect_of(ctx, a, i, tagged) else {
            continue;
        };
        let pin = &a.pins[i];
        let (dx, _) = pin.escape.vector();
        if dx == 0.0 {
            // Up or down: the room is the stub's own.
            rect.union(own);
            continue;
        }
        let beyond = foots
            .iter()
            .filter(|f| f.min.y < pin.at.y + 2.0 && f.max.y > pin.at.y - 2.0)
            .map(|f| if dx > 0.0 { f.max.x } else { f.min.x })
            .fold(if dx > 0.0 { own.min.x } else { own.max.x }, |m, x| {
                if dx > 0.0 { m.max(x) } else { m.min(x) }
            });
        let shift = beyond - if dx > 0.0 { own.min.x } else { own.max.x };
        rect.union(own.translated(Pt::new(shift + dx * 2.54, 0.0)));
    }
    Block {
        anchor,
        members,
        rect: inflate(rect, 0.0),
        orients,
        pairs,
        level,
        spine_nets,
        spine_lanes,
    }
}

// ---------------------------------------------------------------------------------------
// Wires

/// A part, the lines of its pins and all its texts are in the way of wires.
pub(super) fn block_part(grid: &mut Grid, part: &PlacedPart) {
    if let Some(b) = part.glyph.body() {
        let corners = [
            part.orient.apply((b.min.x, b.min.y)),
            part.orient.apply((b.max.x, b.max.y)),
            part.orient.apply((b.min.x, b.max.y)),
            part.orient.apply((b.max.x, b.min.y)),
        ];
        let mut r = Rect::around(Pt::new(corners[0].x + part.at.x, corners[0].y + part.at.y));
        for c in &corners {
            r.include(Pt::new(c.x + part.at.x, c.y + part.at.y));
        }
        // Room round the body, so that no wire runs along its outline (a body
        // thinner than a grid step has the nodes beside it within reach of its
        // lines). On a side a pin leaves by the room is kept off the columns of the
        // pins: their wires start there.
        let by = crate::sheetview::HUG;
        let leaves = |d: Dir| part.pins.iter().any(|q| q.escape == d);
        let wide = Rect {
            min: Pt::new(
                r.min.x - if leaves(Dir::Left) { 0.0 } else { by },
                r.min.y - if leaves(Dir::Up) { 0.0 } else { by },
            ),
            max: Pt::new(
                r.max.x + if leaves(Dir::Right) { 0.0 } else { by },
                r.max.y + if leaves(Dir::Down) { 0.0 } else { by },
            ),
        };
        grid.block_body(wide);
        for side in [Dir::Up, Dir::Down, Dir::Left, Dir::Right] {
            let along: Vec<f64> = part
                .pins
                .iter()
                .filter(|q| q.escape == side)
                .map(|q| if side.is_vertical() { q.at.x } else { q.at.y })
                .collect();
            if along.is_empty() {
                continue;
            }
            for strip in side_strips(r, side, &along, by) {
                grid.block_rect(strip);
            }
        }
        grid.block_body(r);
    }
    for (pin, placed) in part.glyph.pins.iter().zip(&part.pins) {
        if pin.hidden {
            continue;
        }
        let toward = part.orient.dir(pin.toward_body);
        // The pin's line, but not the point a wire starts at.
        let from = grid.node(placed.at);
        let (dx, dy) = toward.vector();
        for k in 1..=((pin.length / GRID).round() as i32) {
            grid.block((from.0 + dx as i32 * k, from.1 + dy as i32 * k));
        }
        let (number, name) = part.glyph.pin_texts(pin, placed.at, toward);
        for t in number.into_iter().chain(name) {
            grid.block_rect(inflate(t.extent_at(part.glyph.pin_text), 0.2));
        }
    }
    for t in std::iter::once(&part.reference).chain(part.value.as_ref()) {
        grid.block_rect(inflate(t.extent(), 0.9));
    }
}

/// The straight stub out of a pin at `node`, as far as it goes free, up to [`STUB_CELLS`]:
/// claimed for net `id`. Gives its end.
pub(super) fn lay_stub(grid: &mut Grid, node: Node, escape: Dir, id: u32) -> Node {
    grid.unblock(node);
    let (dx, dy) = escape.vector();
    let step = |k: i32| (node.0 + dx as i32 * k, node.1 + dy as i32 * k);
    let mut end = node;
    for k in 1..=STUB_CELLS {
        if grid.is_free(step(k)) && !grid.step_is_cut(step(k - 1), escape) {
            end = step(k);
        } else {
            break;
        }
    }
    grid.claim_wire(node, end, id);
    end
}
