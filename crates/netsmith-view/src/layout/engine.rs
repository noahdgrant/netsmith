//! The engine: places the parts as a person would (the main parts in rows, the small parts
//! that serve a pin beside that pin), then draws the rails as power symbols, the nets that leave
//! the sheet as flags and the nets [`super::policy`] chose as names at the end of stubs, and
//! wires every other net on a grid.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::grid::{self, Grid, Node};
use super::prune;
use super::*;

mod bus;
mod pair;
mod spine;

/// Clear space round a part and what it needs beside it.
const MARGIN: f64 = 2.54;
/// Between two blocks of parts in the row.
const BLOCK_GAP: f64 = 15.24;
/// Between the parts that stand alone.
const FREE_GAP: f64 = 7.62;
/// Pins of one rail within this many cells of each other share a symbol.
/// Room above a supply pin for its wire and symbol, and below a ground pin.
const SYMBOL_UP: f64 = 11.43;
const SYMBOL_DOWN: f64 = 8.89;
/// A wire straight out of a pin, in cells, before it may turn.
const STUB_CELLS: i32 = 2;
/// A sheet is cut into rows when it would be wider than this (width over height) in one.
const MAX_ASPECT: f64 = 1.8;
/// A sheet whose parts are in a row narrower than this is not cut into rows.
const MIN_WRAP_WIDTH: f64 = 160.0;
/// The shape the rows aim for.
const TARGET_ASPECT: f64 = 1.4;
/// The width of a letter in a flag, in its height: bold capitals are wider than the
/// [`CHAR_WIDTH`] the texts are measured with.
const FLAG_CHAR_WIDTH: f64 = 0.72;
/// A name is in line with its stub, this far past its end.
const NAME_GAP: f64 = 0.6;
/// How far a stub may be lengthened to find room for the name at its end, in cells.
const STUB_EXTRA_CELLS: i32 = 24;
/// Room kept round a name or a flag, so that no wire runs along it.
const TAG_CLEAR: f64 = 1.0;
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

/// The strips along one side of the body `r`, `by` deep, that are not in line with a pin
/// that leaves that side (`along` is where those pins are, across the side): a wire that
/// ran there would run along the outline.
fn side_strips(r: Rect, side: Dir, along: &[f64], by: f64) -> Vec<Rect> {
    // Half the width kept clear of a pin's own column.
    const COLUMN: f64 = 0.6;
    let (lo, hi) = if side.is_vertical() {
        (r.min.x - by, r.max.x + by)
    } else {
        (r.min.y - by, r.max.y + by)
    };
    let mut cuts = along.to_vec();
    cuts.sort_by(f64::total_cmp);
    let mut spans: Vec<(f64, f64)> = Vec::new();
    let mut from = lo;
    for c in cuts {
        if c - COLUMN > from {
            spans.push((from, c - COLUMN));
        }
        from = from.max(c + COLUMN);
    }
    if hi > from {
        spans.push((from, hi));
    }
    spans
        .into_iter()
        .map(|(a, b)| match side {
            Dir::Up => Rect {
                min: Pt::new(a, r.min.y - by),
                max: Pt::new(b, r.min.y),
            },
            Dir::Down => Rect {
                min: Pt::new(a, r.max.y),
                max: Pt::new(b, r.max.y + by),
            },
            Dir::Left => Rect {
                min: Pt::new(r.min.x - by, a),
                max: Pt::new(r.min.x, b),
            },
            Dir::Right => Rect {
                min: Pt::new(r.max.x, a),
                max: Pt::new(r.max.x + by, b),
            },
        })
        .collect()
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
    let (reference, value) = part_texts(&ctx.design.parts[part.part], body, side, gap, &part.pins);
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
    /// Set for a satellite across two pins of its anchor (a crystal): it is placed beside
    /// both, by [`pair`].
    pair: Option<pair::PairPlan>,
}

struct Block {
    anchor: usize,
    /// Each part and where it is from the anchor's origin, with the side its texts go on.
    members: Vec<(usize, Pt, Option<Dir>)>,
    rect: Rect,
    /// The parts of the block that were turned when they were placed (pair satellites).
    orients: Vec<(usize, Orient)>,
    /// The pair satellites placed beside two pins, whose wires are drawn first.
    pairs: Vec<pair::PairRoute>,
    /// The row the block is level on, when it has a spine: its input row, from the anchor's origin.
    level: Option<f64>,
    /// The nets of the spine and the way the symbol of each goes.
    spine_nets: Vec<(usize, i32)>,
    /// The corridors of its rows, from the anchor's origin.
    spine_lanes: Vec<Rect>,
}

/// A sheet drawn, and what the choice of names needs to know of it.
pub(super) struct Drawn {
    pub layout: Layout,
    /// For each net, how many groups of parts (a main part and what serves its pins, or a part
    /// that stands alone) it has pins in, on this sheet.
    pub clusters: Vec<usize>,
    /// The nets that are drawn as members of a bus: not for the policy to name.
    pub bus_nets: Vec<bool>,
    /// The nets of a pair satellite (a crystal across two pins): never named.
    pub pair_nets: Vec<bool>,
}

/// One piece of the sheet that is placed as a whole: a block (a main part with the parts that
/// serve its pins), or a unit of parts that stand alone.
struct Piece {
    /// Each part and where it is from the piece's origin, with the side its texts go on.
    members: Vec<(usize, Pt, Option<Dir>)>,
    /// The main part of a block; none for parts that stand alone.
    anchor: Option<usize>,
    /// What the piece covers, with the room its names and flags need.
    rect: Rect,
    /// The row the piece's own middle is on, from the origin: pieces of a row are level there.
    middle: f64,
    /// The supplies and the grounds the piece's pins are on: a row needs room for each rail.
    supplies: BTreeSet<usize>,
    grounds: BTreeSet<usize>,
    /// The corridors of the rows of a spine, from the origin.
    lanes: Vec<Rect>,
}

/// Room above a row for its supply rails and their symbols, and below it for its grounds':
/// each rail is a few grid steps beyond the one before.
fn rail_room(rails: usize) -> f64 {
    if rails == 0 {
        0.0
    } else {
        11.5 + 3.8 * (rails - 1) as f64
    }
}

/// Clear space between two rows that have no rails between them.
const ROW_MARGIN: f64 = 6.35;
/// The strip a sheet's title takes above its drawing (`sheets`).
const TITLE_ROOM: f64 = 8.89;

pub(super) fn run(
    ctx: &Ctx,
    units: &[Unit],
    order: &[usize],
    orient: &[Orient],
    marks: &Marks,
    labelled: &[bool],
) -> Drawn {
    // The nets that are drawn as a name at the end of a stub: the nets that leave the sheet
    // (a flag) and the nets chosen by the policy.
    let retag: Vec<bool> = (0..ctx.design.nets.len())
        .map(|net| (ctx.external[net] || labelled[net]) && ctx.power[net].is_none())
        .collect();
    let mut tagged = retag.clone();
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
    // The parts of a spine are placed with their IC: no satellite of a pin.
    let claimed: Vec<bool> = units
        .iter()
        .map(|u| u.parts.iter().any(|&p| ctx.spine_part[p]))
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

    // The interfaces drawn as buses: their nets have no flag or name of their own, and their
    // parts keep room for the entries, the spine and the writing.
    let is_anchor = |p: usize| role[unit_index(units, p)] == Role::Anchor;
    let bus_plans = bus::plan(ctx, &placed, &is_anchor);
    let mut bus_net = vec![false; ctx.design.nets.len()];
    for plan in &bus_plans {
        for &net in &plan.nets {
            tagged[net] = false;
            bus_net[net] = true;
        }
    }

    // The satellites across two pins of one anchor (a crystal): their nets are wires, never
    // names, and the satellite is placed beside both pins.
    let mut pair_of: BTreeMap<usize, pair::PairPlan> = BTreeMap::new();
    let mut pair_nets = vec![false; ctx.design.nets.len()];
    for u in 0..units.len() {
        if role[u] != Role::Satellite || units[u].parts.len() != 1 || claimed[u] {
            continue;
        }
        let part = units[u].parts[0];
        if let Some(plan) = pair::detect(ctx, &placed, part, &anchor_pins, &bus_net) {
            for pin in &ctx.glyphs[part].pins {
                if let Some(net) = net_of(ctx, part, &pin.id) {
                    tagged[net] = false;
                    pair_nets[net] = true;
                }
            }
            pair_of.insert(u, plan);
        }
    }
    let bus_rooms = bus::rooms(ctx, &placed, &bus_plans, &tagged);

    // Each satellite is tied to the pin it serves.
    let mut plans: Vec<SatPlan> = Vec::new();
    let mut chained: Vec<SatPlan> = Vec::new();
    let mut supply_use: BTreeMap<usize, usize> = BTreeMap::new();
    for u in 0..units.len() {
        if role[u] != Role::Satellite || claimed[u] {
            continue;
        }
        let ties = ties(&placed, &units[u], &shapes[u]);
        let mut chosen: Option<SatPlan> = None;
        if let Some(plan) = pair_of.get(&u) {
            // Across two pins: tied to the first, and placed beside both when it can be.
            let (_, ia) = plan.links[0];
            let pin = &placed[plan.anchor].pins[ia];
            if let Some(t) = ties.iter().find(|t| Some(t.net) == pin.net) {
                plans.push(SatPlan {
                    unit: u,
                    anchor: plan.anchor,
                    root: plan.anchor,
                    pin: ia,
                    r: t.r,
                    net: t.net,
                    pair: Some(*plan),
                });
                continue;
            }
        }
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
                    pair: None,
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
                        pair: None,
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
        if role[u] != Role::Satellite || claimed[u] || plans.iter().any(|c| c.unit == u) {
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
                        pair: None,
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
        let rooms = bus_rooms.get(&a).map_or(&[][..], Vec::as_slice);
        let spine = ctx.spines.iter().find(|s| s.ic == a);
        let block = build_block(
            ctx, &placed, units, &shapes, a, &mine, &mut tied, &tagged, rooms, spine,
        );
        blocks.push(block);
    }

    // The parts that were turned as they were placed (pair satellites), and the wires that
    // are drawn first.
    let mut pair_routes: Vec<pair::PairRoute> = Vec::new();
    for b in &blocks {
        for &(p, o) in &b.orients {
            placed[p] = place_part(ctx, p, o, &pin_nets[p]);
        }
        pair_routes.extend(b.pairs.iter().copied());
    }

    // The pieces: the blocks in the order of the signal, then the parts that stand alone.
    let rails_of = |members: &[(usize, Pt, Option<Dir>)]| {
        let (mut supplies, mut grounds) = (BTreeSet::new(), BTreeSet::new());
        for &(p, _, _) in members {
            for pin in &placed[p].pins {
                match pin.net.and_then(|net| ctx.power[net].map(|k| (net, k))) {
                    Some((net, PowerKind::Supply)) => {
                        supplies.insert(net);
                    }
                    Some((net, PowerKind::Ground)) => {
                        grounds.insert(net);
                    }
                    None => {}
                }
            }
        }
        (supplies, grounds)
    };
    let mut pieces: Vec<Piece> = Vec::new();
    let mut spine_nets: Vec<(usize, i32)> = Vec::new();
    for b in blocks {
        let ext = placed[b.anchor].extent;
        let (supplies, grounds) = rails_of(&b.members);
        spine_nets.extend(b.spine_nets.iter().copied());
        pieces.push(Piece {
            anchor: Some(b.anchor),
            members: b.members,
            rect: b.rect,
            middle: b.level.unwrap_or((ext.min.y + ext.max.y) / 2.0),
            supplies,
            grounds,
            lanes: b.spine_lanes,
        });
    }
    let free: Vec<usize> = (0..units.len())
        .filter(|&u| role[u] == Role::Free || (role[u] == Role::Satellite && !tied[u]))
        .collect();
    for &u in &free {
        let mut rect = inflate(
            unit_rect(ctx, &placed, &units[u], &shapes[u], None),
            FREE_GAP / 2.0,
        );
        let mut members = Vec::new();
        for (w, &p) in units[u].parts.iter().enumerate() {
            let off = shapes[u].offsets[w];
            for r in tag_rects(ctx, &placed[p], &tagged) {
                rect.union(r.translated(off));
            }
            members.push((p, off, None));
        }
        let (supplies, grounds) = rails_of(&members);
        pieces.push(Piece {
            anchor: None,
            members,
            middle: (rect.min.y + rect.max.y) / 2.0,
            rect,
            supplies,
            grounds,
            lanes: Vec::new(),
        });
    }

    // Absolute places: rows of pieces, each row level on the middles of its pieces.
    let rows = pack(&pieces);
    let mut at: Vec<Pt> = vec![Pt::new(0.0, 0.0); n];
    let mut text_side: Vec<Option<Dir>> = vec![None; n];
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut satellites: BTreeSet<usize> = BTreeSet::new();
    let mut block_of: Vec<usize> = (0..n).collect();
    let mut anchored = vec![false; n];
    let mut y = 0.0;
    let mut previous: Option<(f64, &[usize])> = None;
    for row in &rows {
        let (above, below) = row_height(&pieces, row);
        if let Some((bottom, before)) = previous {
            y = bottom + row_gap(&pieces, before, row) + above;
        }
        previous = Some((y + below, row));
        let mut x = 0.0;
        let mut group = Vec::new();
        for &i in row {
            let piece = &pieces[i];
            let by = Pt::new(
                snap_up(x - piece.rect.min.x, GRID),
                snap_grid(y - piece.middle),
            );
            let first = piece.members[0].0;
            for &(p, off, side) in &piece.members {
                at[p] = Pt::new(by.x + off.x, by.y + off.y);
                text_side[p] = side;
                block_of[p] = piece.anchor.unwrap_or(first);
                anchored[p] = piece.anchor.is_some();
                group.push(p);
                if Some(p) != piece.anchor && piece.anchor.is_some() {
                    satellites.insert(p);
                }
            }
            x = by.x + piece.rect.max.x + BLOCK_GAP;
        }
        groups.push(group);
    }
    for p in 0..n {
        if let Some(side) = text_side[p] {
            set_text_side(ctx, &mut placed[p], side);
        }
        translate_part(&mut placed[p], at[p]);
    }

    groups.retain(|g| !g.is_empty());
    Scene::new(
        ctx,
        placed,
        units,
        marks,
        groups,
        satellites,
        tagged,
        block_of,
        anchored,
        bus_plans,
        retag,
        pair_routes,
        pair_nets,
        spine_nets,
    )
    .finish()
}

/// How far a row reaches above and below the line its pieces are level on.
fn row_height(pieces: &[Piece], row: &[usize]) -> (f64, f64) {
    let above = row
        .iter()
        .map(|&i| pieces[i].middle - pieces[i].rect.min.y)
        .fold(0.0, f64::max);
    let below = row
        .iter()
        .map(|&i| pieces[i].rect.max.y - pieces[i].middle)
        .fold(0.0, f64::max);
    (above, below)
}

/// The space between a row and the one under it: the grounds' rails of the first and the
/// supplies' of the second need room, and so do their symbols.
fn row_gap(pieces: &[Piece], upper: &[usize], lower: &[usize]) -> f64 {
    let rails = |row: &[usize], ground: bool| -> usize {
        row.iter()
            .flat_map(|&i| {
                if ground {
                    pieces[i].grounds.iter()
                } else {
                    pieces[i].supplies.iter()
                }
            })
            .collect::<BTreeSet<_>>()
            .len()
    };
    ROW_MARGIN + rail_room(rails(upper, true)) + rail_room(rails(lower, false))
}

/// The pieces in rows, as indices into `pieces`: one row if the sheet comes out no wider than
/// [`MAX_ASPECT`] that way, else as many rows as bring it nearest [`TARGET_ASPECT`]; the
/// pieces keep their order, a row is filled from the left.
fn pack(pieces: &[Piece]) -> Vec<Vec<usize>> {
    if pieces.is_empty() {
        return Vec::new();
    }
    let width = |row: &[usize]| -> f64 {
        row.iter().map(|&i| pieces[i].rect.width()).sum::<f64>()
            + BLOCK_GAP * (row.len() - 1) as f64
    };
    let rails = |row: &[usize], ground: bool| -> usize {
        row.iter()
            .flat_map(|&i| {
                if ground {
                    pieces[i].grounds.iter()
                } else {
                    pieces[i].supplies.iter()
                }
            })
            .collect::<BTreeSet<_>>()
            .len()
    };
    let shape = |rows: &[Vec<usize>]| -> f64 {
        // The frame has a border round the drawing and a strip for the title above it, and
        // the first row's supplies and the last row's grounds need room too.
        let w = rows.iter().map(|r| width(r)).fold(0.0, f64::max) + 2.0 * BORDER;
        let mut h = TITLE_ROOM
            + 2.0 * BORDER
            + rail_room(rails(&rows[0], false))
            + rail_room(rails(&rows[rows.len() - 1], true));
        for (k, row) in rows.iter().enumerate() {
            let (above, below) = row_height(pieces, row);
            h += above + below;
            if k > 0 {
                h += row_gap(pieces, &rows[k - 1], row);
            }
        }
        w / h.max(1.0)
    };
    let all: Vec<usize> = (0..pieces.len()).collect();
    let one = vec![all.clone()];
    // A small sheet stays as it is: a few parts are not worth a second row. So does a sheet of
    // a spine and a few more pieces: the spine is the row, and a connector that sets the power
    // going in is at its left end, not above it.
    let spine = pieces.len() <= 3 && pieces.iter().any(|p| !p.lanes.is_empty());
    if shape(&one) <= MAX_ASPECT || width(&all) < MIN_WRAP_WIDTH || spine {
        return one;
    }
    let total = width(&all);
    let mut best = ((shape(&one) / TARGET_ASPECT).ln().abs(), one);
    for count in 2..=pieces.len().min(12) {
        for slack in [0.9, 1.0, 1.1, 1.25, 1.4] {
            let target = total / count as f64 * slack;
            let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
            for &i in &all {
                let row = rows.last().expect("a row");
                let mut with = row.clone();
                with.push(i);
                if !row.is_empty() && width(&with) > target {
                    rows.push(vec![i]);
                } else {
                    rows.last_mut().expect("a row").push(i);
                }
            }
            let miss = (shape(&rows) / TARGET_ASPECT).ln().abs();
            if miss < best.0 - 1e-9 {
                best = (miss, rows);
            }
        }
    }
    best.1
}

/// The name inside a flag whose point is at `tip` and that points `dir`: across for a flag
/// that points left or right, upward for one that points up or down.
fn port_label(name: &str, tip: Pt, dir: Dir) -> Text {
    let start = tip.step(dir, 1.1);
    match dir {
        Dir::Left => Text::new(name, Pt::new(start.x, start.y + 0.3), Anchor::End),
        Dir::Right => Text::new(name, Pt::new(start.x, start.y + 0.3), Anchor::Start),
        Dir::Up => Text::new(name, Pt::new(start.x + 0.3, start.y), Anchor::Start).turned(),
        Dir::Down => Text::new(name, Pt::new(start.x + 0.3, start.y), Anchor::End).turned(),
    }
}

/// How long a flag is for `name`, from its point.
fn port_length(name: Option<&str>) -> f64 {
    name.map_or(3.0, |n| {
        n.chars().count() as f64 * Port::TEXT * FLAG_CHAR_WIDTH + 2.2
    })
}

/// The name of a net written at `e`, the end of a stub that points `dir`, in line with the
/// stub: across for a stub that points left or right, upward for one that points up or down.
fn name_text(name: &str, e: Pt, dir: Dir) -> Text {
    match dir {
        Dir::Right => Text::new(name, Pt::new(e.x + NAME_GAP, e.y + 0.45), Anchor::Start),
        Dir::Left => Text::new(name, Pt::new(e.x - NAME_GAP, e.y + 0.45), Anchor::End),
        Dir::Up => Text::new(name, Pt::new(e.x + 0.45, e.y - NAME_GAP), Anchor::Start).turned(),
        Dir::Down => Text::new(name, Pt::new(e.x + 0.45, e.y + NAME_GAP), Anchor::End).turned(),
    }
}

/// What is drawn at the end of a stub: a flag for a net that leaves the sheet, else the net's
/// name. `rect` is what it covers.
struct Tag {
    port: Option<Port>,
    label: Option<NetLabel>,
    rect: Rect,
}

/// What a label says for `net`: its name, or for a net with a generated name the first pin
/// of the part with the most pins (`J2.3`), as KiCad names such a net by its pin.
fn shown_name(ctx: &Ctx, net: usize) -> String {
    let name = &ctx.design.nets[net].name;
    if is_named(name) {
        return name.clone();
    }
    let part = |refdes: &str| ctx.design.parts.iter().position(|p| p.refdes == refdes);
    let main = ctx.design.nets[net].pins.iter().min_by_key(|pin| {
        let p = part(&pin.refdes);
        (
            std::cmp::Reverse(p.map_or(0, |p| ctx.glyphs[p].pins.len())),
            p.map(|p| ctx.design.parts[p].path.clone()),
            pin.pin.clone(),
        )
    });
    main.map_or_else(|| name.clone(), |pin| format!("{}.{}", pin.refdes, pin.pin))
}

fn tag_at(ctx: &Ctx, net: usize, e: Pt, dir: Dir) -> Tag {
    let name = &ctx.design.nets[net].name;
    if ctx.external[net] {
        let shown = is_named(name).then(|| ellipsize(name));
        let port = Port {
            net,
            at: e,
            dir,
            length: port_length(shown.as_deref()),
            label: shown.as_deref().map(|l| port_label(l, e, dir)),
        };
        let mut rect = port.extent();
        if let Some(t) = &port.label {
            rect.union(t.extent_at(Port::TEXT));
        }
        Tag {
            port: Some(port),
            label: None,
            rect,
        }
    } else {
        let text = name_text(&ellipsize(&shown_name(ctx, net)), e, dir);
        // Bold letters run wider than they are measured: keep a little room past the end.
        let mut rect = text.extent();
        let more = 0.15 * text.text.chars().count() as f64 * TEXT_SIZE * CHAR_WIDTH;
        match dir {
            Dir::Right => rect.max.x += more,
            Dir::Left => rect.min.x -= more,
            Dir::Up => rect.min.y -= more,
            Dir::Down => rect.max.y += more,
        }
        Tag {
            port: None,
            label: Some(NetLabel {
                net,
                text,
                at: e,
                dir,
            }),
            rect,
        }
    }
}

/// The box the name or flag at pin `i` of a part will cover, stub included, if the pin ends in
/// one: room to keep when the part is placed. The part may be at the origin or placed.
fn tag_rect_of(ctx: &Ctx, part: &PlacedPart, i: usize, tagged: &[bool]) -> Option<Rect> {
    let (pin, g) = (&part.pins[i], &part.glyph.pins[i]);
    let net = pin.net?;
    if g.hidden || !tagged[net] {
        return None;
    }
    let e = pin.at.step(pin.escape, STUB);
    let mut rect = tag_at(ctx, net, e, pin.escape).rect;
    rect.include(pin.at);
    Some(rect)
}

/// The boxes of every name and flag at a part's pins.
fn tag_rects(ctx: &Ctx, part: &PlacedPart, tagged: &[bool]) -> Vec<Rect> {
    (0..part.pins.len())
        .filter_map(|i| tag_rect_of(ctx, part, i, tagged))
        .collect()
}

fn unit_index(units: &[Unit], part: usize) -> usize {
    units
        .iter()
        .position(|u| u.parts.contains(&part))
        .expect("every part is in a unit")
}

/// The satellites of one anchor placed round it, each as near the pin it serves as it can be
/// without covering anything.
#[allow(clippy::too_many_arguments)]
fn build_block(
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
    let bottom_first = spine.is_some();
    sorted.sort_by(|x, y| {
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
    });
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
fn block_part(grid: &mut Grid, part: &PlacedPart) {
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
fn lay_stub(grid: &mut Grid, node: Node, escape: Dir, id: u32) -> Node {
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

/// A pin of a placed part: the part and the index of the pin.
type PinKey = (usize, usize);

/// The rail pins of satellites whose stubs end on one line: the net and the line, and the pins
/// along it with where each stub ends.
type RailRow = ((usize, i32), Vec<(i32, PinKey)>);

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
    /// The parts that stand beside a pin of another part.
    satellites: BTreeSet<usize>,
    /// The nets drawn as a name at the end of a stub (flags and labels), not as one wire.
    tagged: Vec<bool>,
    /// The block each part is in: its main part, or the first part of a unit that stands
    /// alone.
    block_of: Vec<usize>,
    /// Whether each part is in a block with a main part (and not one of the parts that stand
    /// alone).
    anchored: Vec<bool>,
    /// How many blocks with a main part each net has pins in.
    clusters: Vec<usize>,
    /// The pins of each tagged net, by block.
    tag_clusters: BTreeMap<usize, Vec<Vec<(usize, usize)>>>,
    /// What the names and flags cover, with the part each is at.
    tag_boxes: Vec<(usize, Rect)>,
    /// The pins whose stub ends in a name or a flag: a wire to them joins the stub half way,
    /// not at its end, where the name is.
    tag_keys: BTreeSet<(usize, usize)>,
    labels: Vec<NetLabel>,
    /// The buses planned, until they are drawn.
    bus_plans: Vec<bus::BusPlan>,
    /// Whether each net is one of a bus that is planned or drawn: it has no stub of its own
    /// and no wire to route.
    bus_skip: Vec<bool>,
    /// Whether each net is one of a bus that was drawn.
    bus_done: Vec<bool>,
    /// What each net's tag was before the buses took their nets' tags away.
    retag: Vec<bool>,
    buses: Vec<Bus>,
    /// The pair satellites placed beside two pins: their wires are drawn first.
    pair_routes: Vec<pair::PairRoute>,
    /// The pins of those satellites: a wire leaves the tip in any direction, with no stub.
    bare: BTreeSet<(usize, usize)>,
    /// The nets of pair satellites, for the policy that names nets.
    pair_nets: Vec<bool>,
    /// The rail pins of satellites that have a symbol of their own, or share one with the
    /// satellites beside them: the rail of the group does not reach them.
    local_done: BTreeSet<(usize, usize)>,
    /// The nets of the spines and the way the symbol of each goes (-1 left, 1 right).
    spine_pref: Vec<(usize, i32)>,
}

impl<'a> Scene<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        ctx: &'a Ctx<'a>,
        parts: Vec<PlacedPart>,
        units: &'a [Unit],
        marks: &'a Marks,
        groups: Vec<Vec<usize>>,
        satellites: BTreeSet<usize>,
        tagged: Vec<bool>,
        block_of: Vec<usize>,
        anchored: Vec<bool>,
        bus_plans: Vec<bus::BusPlan>,
        retag: Vec<bool>,
        pair_routes: Vec<pair::PairRoute>,
        pair_nets: Vec<bool>,
        spine_pref: Vec<(usize, i32)>,
    ) -> Self {
        let mut bounds = parts[0..]
            .first()
            .map_or(Rect::around(Pt::new(0.0, 0.0)), |p| p.extent);
        for p in &parts {
            bounds.union(p.extent);
        }
        // Room for the rails, their symbols and the names at the pins at the edge.
        // A bus has its spine, its trunk and its writing out beyond the parts as well.
        let bounds = inflate(bounds, if bus_plans.is_empty() { 40.0 } else { 70.0 });
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
            satellites,
            clusters: vec![0; ctx.design.nets.len()],
            tagged,
            block_of,
            anchored,
            tag_clusters: BTreeMap::new(),
            tag_boxes: Vec::new(),
            tag_keys: BTreeSet::new(),
            labels: Vec::new(),
            bus_skip: {
                let mut skip = vec![false; ctx.design.nets.len()];
                for plan in &bus_plans {
                    for &net in &plan.nets {
                        skip[net] = true;
                    }
                }
                skip
            },
            bus_done: vec![false; ctx.design.nets.len()],
            bus_plans,
            retag,
            buses: Vec::new(),
            bare: pair_routes
                .iter()
                .flat_map(|r| r.links.iter().map(|&(pin, _)| (r.part, pin)))
                .collect(),
            pair_routes,
            pair_nets,
            local_done: BTreeSet::new(),
            spine_pref,
        };
        scene.block_parts();
        scene.collect_pins();
        scene.count_clusters();
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
            block_part(&mut self.grid, part);
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

    /// How many blocks with a main part each net has pins in: parts that stand alone are
    /// reached by a bus wire and do not make a net one to name.
    fn count_clusters(&mut self) {
        for net in 0..self.clusters.len() {
            let blocks: BTreeSet<usize> = self
                .pins
                .iter()
                .filter(|p| p.net == net && self.anchored[p.part])
                .map(|p| self.block_of[p.part])
                .collect();
            self.clusters[net] = blocks.len();
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
            if self.joined.contains(&(part, idx)) || !self.active(net) || self.bus_skip[net] {
                continue;
            }
            self.stub_pin(part, idx, net, node, escape);
        }
    }

    /// The straight stub out of a pin, as far as it goes free, up to [`STUB_CELLS`].
    fn stub_pin(&mut self, part: usize, idx: usize, net: usize, node: Node, escape: Dir) {
        let end = if self.bare.contains(&(part, idx)) {
            // A pair satellite's pin has no stub: the wire leaves the tip as it likes.
            self.grid.unblock(node);
            self.grid.claim_wire(node, node, Self::net_id(net));
            node
        } else {
            lay_stub(&mut self.grid, node, escape, Self::net_id(net))
        };
        if end != node {
            self.push_wire(net, &[node, end]);
        }
        self.stubs.insert(
            (part, idx),
            Stub {
                pin: node,
                end,
                heading: escape,
            },
        );
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
            // A stub that ends in a name is joined half way along: the name is in the way
            // at its end.
            let start = if self.tag_keys.contains(&key) && Self::manhattan(end, pin) >= 2 {
                let (dx, dy) = heading.vector();
                (end.0 - dx as i32, end.1 - dy as i32)
            } else {
                end
            };
            let targets: HashSet<Node> = tree.iter().copied().collect();
            let routed = if self.bare.contains(&key) {
                self.grid
                    .route_free(id, start, &targets)
                    .map(|(_, path)| path)
            } else {
                self.grid.route(id, start, heading, &targets)
            };
            match routed {
                Some(path) => {
                    self.grid.commit(&path, id);
                    self.push_wire(net, &path);
                    tree.extend(path.iter().copied());
                }
                None => {
                    // Nothing clear: the pin is left unjoined, and `check` says so. A wire
                    // laid through whatever is in the way would join it by drawing a short
                    // or a wire along another net's.
                    continue;
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

    /// The wires of the pair satellites, before anything else is routed: each joins the stub of
    /// the main part's pin on its net, in the order the placement found. What else is on the
    /// net is joined to them afterwards.
    fn route_pairs(&mut self) {
        for route in self.pair_routes.clone() {
            for &k in &route.order {
                let (sat_pin, anchor_pin) = route.links[k];
                let Some(net) = self.parts[route.part].pins[sat_pin].net else {
                    continue;
                };
                let Some(stub) = self.stubs.get(&(route.anchor, anchor_pin)) else {
                    continue;
                };
                let mut tree: BTreeSet<Node> = BTreeSet::new();
                let (dx, dy) = (
                    (stub.end.0 - stub.pin.0).signum(),
                    (stub.end.1 - stub.pin.1).signum(),
                );
                let mut at = stub.pin;
                tree.insert(at);
                while at != stub.end {
                    at = (at.0 + dx, at.1 + dy);
                    tree.insert(at);
                }
                // The row of a spine through the net is joined where the wire meets it.
                tree.extend(self.trees.get(&net).cloned().unwrap_or_default());
                let mut tree = self.route_tree(net, &[(route.part, sat_pin)], tree);
                tree.extend(self.trees.remove(&net).unwrap_or_default());
                self.trees.insert(net, tree);
            }
        }
    }

    /// The nets of the spines first (`spine`), then every other signal.
    fn route_signals(&mut self, spine: bool) {
        let mut nets: Vec<(i32, usize)> = (0..self.ctx.design.nets.len())
            .filter(|&n| {
                (self.ctx.power[n].is_none() || self.ctx.spine_net[n])
                    && self.ctx.spine_net[n] == spine
                    && self.active(n)
                    && !self.bus_skip[n]
            })
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
            if let Some(clusters) = self.tag_clusters.get(&net).cloned() {
                // A net with a name at each of its blocks: each block's pins are wired to
                // that block's name, and nothing joins the blocks.
                let mut stacked = self.trees.remove(&net).unwrap_or_default();
                for cluster in clusters {
                    let has_joined = cluster.iter().any(|k| self.joined.contains(k));
                    let pins: Vec<(usize, usize)> = cluster
                        .iter()
                        .copied()
                        .filter(|k| !self.joined.contains(k))
                        .collect();
                    let tree = if has_joined {
                        std::mem::take(&mut stacked)
                    } else {
                        BTreeSet::new()
                    };
                    self.route_tree(net, &pins, tree);
                }
                continue;
            }
            let mut pins = self.pins_of(net);
            if spine {
                // The pair satellites' wires are drawn next and join the row.
                pins.retain(|k| !self.bare.contains(k));
            }
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
        self.local_rails();
        for g in 0..self.groups.len() {
            let mut rect: Option<Rect> = None;
            for &p in &self.groups[g] {
                union_into(&mut rect, self.parts[p].extent);
            }
            // What is written at the pins is part of the group: a rail goes beyond it.
            for (p, r) in &self.tag_boxes {
                if self.groups[g].contains(p) {
                    union_into(&mut rect, *r);
                }
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
                        && !self.ctx.spine_net[p.net]
                        && self.stubs.contains_key(&(p.part, p.idx))
                        && !self.local_done.contains(&(p.part, p.idx))
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
            .filter(|key| self.stubs.contains_key(key) && !self.local_done.contains(key))
            .collect();
        keys.sort_by_key(|key| {
            let s = &self.stubs[key];
            (key.0, s.heading as u8, Self::along(s.end, s.heading))
        });
        // The rail's wire is beyond the parts that have a pin on it (and what is written at their
        // pins), not beyond every part of the group: the supply of a unit that stands apart from
        // a tall one has its symbol just above it.
        let rect = {
            let mut own: Option<Rect> = None;
            for key in &keys {
                union_into(&mut own, self.parts[key.0].extent);
            }
            for (p, r) in &self.tag_boxes {
                if keys.iter().any(|k| k.0 == *p) {
                    union_into(&mut own, *r);
                }
            }
            own.unwrap_or(rect)
        };
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
        // The rail goes further out while a text or a part is on its row.
        let outward = if kind == PowerKind::Supply { -1 } else { 1 };
        let mut row = row;
        for _ in 0..40 {
            if (lo..=hi).all(|x| self.grid.free_for((x, row), id)) {
                break;
            }
            row += outward;
        }
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
            // Nothing clear: the pin is left unjoined, and `check` says so.
            if let Some(path) = self.grid.route(id, d.node, d.heading, &targets) {
                self.grid.commit(&path, id);
                self.push_wire(net, &path);
            }
        }
        // The symbol, at the end of a stem from the rail.
        self.place_symbol(net, kind, (sym_x, row), chosen.map(|c| c.1));
    }

    /// The symbol of rail `net` at the end of a stem from `q`, a node of the rail's wire.
    /// `label` is the place its name was found for by [`symbol_fits`](Self::symbol_fits); none
    /// when no place was found, and the name goes beside it as it is.
    fn place_symbol(&mut self, net: usize, kind: PowerKind, q: Node, label: Option<Option<Text>>) {
        let id = Self::net_id(net);
        let natural = if kind == PowerKind::Supply {
            Dir::Up
        } else {
            Dir::Down
        };
        let (dx, dy) = natural.vector();
        let end = (q.0 + dx as i32 * STEM_CELLS, q.1 + dy as i32 * STEM_CELLS);
        self.grid.claim_wire(q, end, id);
        self.push_wire(net, &[q, end]);
        let at = self.grid.pt(end);
        let name = ellipsize(&self.ctx.design.nets[net].name);
        let named = kind == PowerKind::Supply || self.ctx.label_grounds;
        let label = match label {
            Some(l) => l,
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

    /// A satellite whose pin is on a rail has a stub and a symbol of its own, short and local,
    /// instead of a drop to the one wire across the group (a decoupling capacitor's supply
    /// and ground would run the length of the sheet and across other wires): the stub of its
    /// pin goes on, a few cells at most, to a symbol that points away from the part (a supply
    /// up, a ground down). Satellites standing side by side in a row, whose pins end on one
    /// line, share a wire and a symbol when the wire is short and nothing is on its way
    /// ([`policy::shares_rail`]). A pin that has no room for a symbol, or whose stub does not
    /// point away from the part, stays with the group's rail, as does every pin of a main
    /// part.
    fn local_rails(&mut self) {
        // The rail pins, by net and by the line their stubs end on.
        let mut rows: BTreeMap<(usize, i32), Vec<(i32, PinKey)>> = BTreeMap::new();
        for p in &self.pins {
            let Some(kind) = self.ctx.power[p.net] else {
                continue;
            };
            if !self.satellites.contains(&p.part) || self.ctx.spine_net[p.net] {
                continue;
            }
            let key = (p.part, p.idx);
            let Some(stub) = self.stubs.get(&key) else {
                continue;
            };
            let natural = if kind == PowerKind::Supply {
                Dir::Up
            } else {
                Dir::Down
            };
            if stub.heading != natural {
                continue;
            }
            rows.entry((p.net, stub.end.1))
                .or_default()
                .push((stub.end.0, key));
        }
        // The wires that carry a symbol so far, by net: a satellite near one of them is joined to it
        // instead of having a symbol of its own (see `bridge`). A supply's rows are taken from the
        // top down and a ground's from the bottom up, so that a symbol is always the farthest out.
        let mut hubs: Vec<(usize, Vec<Node>)> = Vec::new();
        let mut rows: Vec<RailRow> = rows.into_iter().collect();
        rows.sort_by_key(|&((net, row), _)| {
            let ground = self.ctx.power[net] == Some(PowerKind::Ground);
            (net, if ground { -row } else { row })
        });
        for ((net, row), mut pins) in rows {
            pins.sort();
            let kind = self.ctx.power[net].expect("a rail");
            let id = Self::net_id(net);
            // Runs of pins whose stubs end near each other on the line.
            let mut runs: Vec<Vec<(i32, PinKey)>> = Vec::new();
            for pin in pins {
                match runs.last_mut() {
                    Some(run)
                        if f64::from(pin.0 - run[run.len() - 1].0)
                            <= policy::RAIL_ROW_GAP_STEPS =>
                    {
                        run.push(pin)
                    }
                    _ => runs.push(vec![pin]),
                }
            }
            for run in runs {
                let (first, last) = (run[0].0, run[run.len() - 1].0);
                let mut shared = false;
                if run.len() >= 2
                    && policy::shares_rail(f64::from(last - first))
                    && (first..=last).all(|x| self.grid.free_for((x, row), id))
                {
                    // The symbol hangs from the end of the row, or else from another stub.
                    let mut order: Vec<i32> = run.iter().map(|p| p.0).collect();
                    order.reverse();
                    let found = order.into_iter().find_map(|x| {
                        self.symbol_fits(net, kind, (x, row))
                            .map(|label| (x, label))
                    });
                    if let Some((x, label)) = found {
                        self.grid.claim_wire((first, row), (last, row), id);
                        self.push_wire(net, &[(first, row), (last, row)]);
                        self.place_symbol(net, kind, (x, row), Some(label));
                        self.local_done.extend(run.iter().map(|p| p.1));
                        hubs.push((net, (first..=last).map(|x| (x, row)).collect()));
                        shared = true;
                    }
                }
                if shared {
                    continue;
                }
                for &(x, key) in &run {
                    if self.bridge(net, kind, (x, row), key, &hubs) {
                        continue;
                    }
                    self.local_symbol(net, kind, (x, row), key);
                    if self.local_done.contains(&key) {
                        hubs.push((net, vec![(x, row)]));
                    }
                }
            }
        }
    }

    /// A satellite pin whose stub ends at `end` joins a wire of its rail that carries a symbol
    /// (`hubs`), where that is near and the way is free: pull-ups side by side, standing a row
    /// apart, share one wire and one symbol, as a hand drawing has them, and not a symbol
    /// each. The wire goes out along the stub's way to the row of the hub's wire and across to
    /// it, through free nodes only (so it crosses nothing, and does not run along anything).
    /// Whether it was drawn.
    fn bridge(
        &mut self,
        net: usize,
        kind: PowerKind,
        end: Node,
        key: (usize, usize),
        hubs: &[(usize, Vec<Node>)],
    ) -> bool {
        let id = Self::net_id(net);
        let natural = if kind == PowerKind::Supply {
            Dir::Up
        } else {
            Dir::Down
        };
        let (_, dy) = natural.vector();
        let dy = dy as i32;
        // The nearest node of a hub of this net that is level with the stub or beyond it.
        let mut best: Option<(i32, Node)> = None;
        for (n, nodes) in hubs {
            if *n != net {
                continue;
            }
            for &t in nodes {
                let out = (t.1 - end.1) * dy;
                let cost = (t.0 - end.0).abs() + out;
                if out >= 0
                    && cost <= policy::BRIDGE_CELLS
                    && best.is_none_or(|(c, b)| (cost, t) < (c, b))
                {
                    best = Some((cost, t));
                }
            }
        }
        let Some((_, to)) = best else {
            return false;
        };
        // Out along the stub's way, then across.
        let mut path: Vec<Node> = vec![end];
        let mut at = end;
        while at.1 != to.1 {
            let next = (at.0, at.1 + dy);
            if !self.grid.free_for(next, id) || self.grid.step_is_cut(at, natural) {
                return false;
            }
            path.push(next);
            at = next;
        }
        let across = if to.0 > at.0 { Dir::Right } else { Dir::Left };
        while at.0 != to.0 {
            let next = (at.0 + across.vector().0 as i32, at.1);
            if !self.grid.free_for(next, id) || self.grid.step_is_cut(at, across) {
                return false;
            }
            path.push(next);
            at = next;
        }
        if path.len() < 2 {
            return false;
        }
        let corner = (end.0, to.1);
        self.grid.claim_wire(end, corner, id);
        self.grid.claim_wire(corner, to, id);
        self.push_wire(net, &path);
        self.local_done.insert(key);
        true
    }

    /// The symbol of one satellite pin whose stub ends at `end`: at the end of the stub, or a
    /// few cells beyond it if the symbol or its name has no room there.
    fn local_symbol(&mut self, net: usize, kind: PowerKind, end: Node, key: (usize, usize)) {
        let id = Self::net_id(net);
        let natural = if kind == PowerKind::Supply {
            Dir::Up
        } else {
            Dir::Down
        };
        let (dx, dy) = natural.vector();
        for extra in 0..=policy::LOCAL_STEM_EXTRA {
            let q = (end.0 + dx as i32 * extra, end.1 + dy as i32 * extra);
            if extra > 0 {
                let before = (q.0 - dx as i32, q.1 - dy as i32);
                if !self.grid.free_for(q, id) || self.grid.step_is_cut(before, natural) {
                    return;
                }
            }
            let Some(label) = self.symbol_fits(net, kind, q) else {
                continue;
            };
            if extra > 0 {
                self.grid.claim_wire(end, q, id);
                self.push_wire(net, &[end, q]);
            }
            self.place_symbol(net, kind, q, Some(label));
            self.local_done.insert(key);
            return;
        }
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

    /// Whether every node of the boxes `r` (and a little round it) is free to a wire of `net`:
    /// not in the way of a part or text, and not another net's.
    fn rect_is_free_for(&self, r: Rect, net: usize) -> bool {
        let r = inflate(r, 0.2);
        let lo = self.grid.node(Pt::new(r.min.x, r.min.y));
        let hi = self.grid.node(Pt::new(r.max.x, r.max.y));
        // Only the nodes that lie in the box, not those a rounding of its corners reaches.
        (lo.1 - 1..=hi.1 + 1).all(|j| {
            (lo.0 - 1..=hi.0 + 1).all(|i| {
                let p = self.grid.pt((i, j));
                let inside = p.x >= r.min.x && p.x <= r.max.x && p.y >= r.min.y && p.y <= r.max.y;
                !inside || self.grid.free_for((i, j), Self::net_id(net))
            })
        })
    }

    /// A name at the end of the stub of every block's pins of each net that is tagged: a flag
    /// for a net that goes on to another sheet, a label for a net the policy named. The pin is
    /// the one of the part with the most pins (the main part), in the order of the parts'
    /// paths, and its stub is lengthened until the name has room. A net whose names cannot be
    /// placed is left as wires.
    fn plan_tags(&mut self) {
        self.plan_tags_of((0..self.ctx.design.nets.len()).collect());
    }

    /// [`plan_tags`](Self::plan_tags) for the nets of `nets`.
    fn plan_tags_of(&mut self, nets: Vec<usize>) {
        let ctx = self.ctx;
        for net in nets {
            if !self.tagged[net] || self.local[net] == 0 {
                continue;
            }
            let mut by_block: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
            for p in self.pins.iter().filter(|p| p.net == net) {
                by_block
                    .entry(self.block_of[p.part])
                    .or_default()
                    .push((p.part, p.idx));
            }
            let mut clusters: Vec<Vec<(usize, usize)>> = by_block.into_values().collect();
            // A block with no pin that has a stub (all in a stack) has nowhere for a name.
            let has_stub = |c: &Vec<(usize, usize)>| c.iter().any(|k| self.stubs.contains_key(k));
            if clusters.iter().any(|c| !has_stub(c)) {
                self.tagged[net] = false;
                continue;
            }
            let path = |k: &(usize, usize)| ctx.design.parts[k.0].path.clone();
            for c in &mut clusters {
                c.sort_by_key(|k| (std::cmp::Reverse(self.parts[k.0].pins.len()), path(k), k.1));
            }
            clusters.sort_by_key(|c| (path(&c[0]), c[0].1));
            for cluster in &clusters {
                let keys: Vec<(usize, usize)> = cluster
                    .iter()
                    .copied()
                    .filter(|k| self.stubs.contains_key(k))
                    .collect();
                let placed = keys.iter().any(|&key| {
                    let past = self.cells_past(key, cluster);
                    self.try_tag(net, key, past, true)
                });
                let placed = placed || keys.iter().any(|&key| self.try_spur(net, key));
                if !placed {
                    // Nowhere with room: the name goes at the first stub as it is.
                    self.try_tag(net, keys[0], 0, false);
                }
            }
            self.tag_clusters.insert(net, clusters);
        }
    }

    /// Puts the name of `net` at the end of the stub of `key`, lengthening the stub while the
    /// name does not fit and until it is past `past` cells (the parts beside the pin join the
    /// wire before the name, as a person draws it); with `must_fit` false it is put at the end
    /// of the stub as it is whatever is there. Whether it was put.
    fn try_tag(&mut self, net: usize, key: (usize, usize), past: i32, must_fit: bool) -> bool {
        let id = Self::net_id(net);
        let stub = &self.stubs[&key];
        let (start, heading) = (stub.end, stub.heading);
        let (dx, dy) = heading.vector();
        let reach = if must_fit {
            STUB_EXTRA_CELLS.max(past + 8)
        } else {
            0
        };
        let mut end = start;
        for k in 0..=reach {
            if k > 0 {
                // The stub goes on over free nodes only.
                let before = end;
                end = (start.0 + dx as i32 * k, start.1 + dy as i32 * k);
                if !self.grid.free_for(end, id) || self.grid.step_is_cut(before, heading) {
                    return false;
                }
            }
            if k < past {
                continue;
            }
            let tag = tag_at(self.ctx, net, self.grid.pt(end), heading);
            if must_fit && !self.rect_is_free_for(tag.rect, net) {
                continue;
            }
            if k > 0 {
                self.grid.claim_wire(start, end, id);
                self.push_wire(net, &[start, end]);
                if let Some(stub) = self.stubs.get_mut(&key) {
                    stub.end = end;
                }
            }
            self.grid.block_rect(inflate(tag.rect, TAG_CLEAR));
            self.grid.unblock(end);
            self.tag_keys.insert(key);
            self.tag_boxes.push((key.0, tag.rect));
            self.ports.extend(tag.port);
            self.labels.extend(tag.label);
            return true;
        }
        false
    }

    /// Puts the name of `net` at the end of a short spur that leaves the stub of `key` at right
    /// angles, half way along it, where nothing is room for it in line (another pin of its
    /// block is in the way). Whether it was put.
    fn try_spur(&mut self, net: usize, key: (usize, usize)) -> bool {
        let id = Self::net_id(net);
        let stub = &self.stubs[&key];
        let (end, heading, pin) = (stub.end, stub.heading, stub.pin);
        let (hx, hy) = heading.vector();
        let mid = if Self::manhattan(end, pin) >= 2 {
            (end.0 - hx as i32, end.1 - hy as i32)
        } else {
            end
        };
        let sides = if heading.is_vertical() {
            [Dir::Left, Dir::Right]
        } else {
            [Dir::Up, Dir::Down]
        };
        for side in sides {
            let (dx, dy) = side.vector();
            for len in 2..=4 {
                let tip = (mid.0 + dx as i32 * len, mid.1 + dy as i32 * len);
                let clear = (1..=len).all(|k| {
                    let n = (mid.0 + dx as i32 * k, mid.1 + dy as i32 * k);
                    self.grid.free_for(n, id)
                        && !self
                            .grid
                            .step_is_cut((n.0 - dx as i32, n.1 - dy as i32), side)
                });
                if !clear {
                    break;
                }
                let tag = tag_at(self.ctx, net, self.grid.pt(tip), side);
                if !self.rect_is_free_for(tag.rect, net) {
                    continue;
                }
                self.grid.claim_wire(mid, tip, id);
                self.push_wire(net, &[mid, tip]);
                self.grid.block_rect(inflate(tag.rect, TAG_CLEAR));
                self.grid.unblock(tip);
                self.tag_boxes.push((key.0, tag.rect));
                self.ports.extend(tag.port);
                self.labels.extend(tag.label);
                return true;
            }
        }
        false
    }

    /// How many cells past the end of the stub of `key` the other pins of its block are, along
    /// the stub: the name goes beyond them.
    fn cells_past(&self, key: (usize, usize), cluster: &[(usize, usize)]) -> i32 {
        let stub = &self.stubs[&key];
        let (dx, dy) = stub.heading.vector();
        cluster
            .iter()
            .filter(|&&k| k != key)
            .map(|k| {
                let n = self.stubs.get(k).map_or_else(
                    || {
                        self.pins
                            .iter()
                            .find(|p| (p.part, p.idx) == *k)
                            .map_or(stub.end, |p| p.node)
                    },
                    |s| s.end,
                );
                (n.0 - stub.end.0) * dx as i32 + (n.1 - stub.end.1) * dy as i32
            })
            .filter(|&d| d > 0)
            .map(|d| d + 2)
            .max()
            .unwrap_or(0)
    }

    /// A rail on a spine is wired along the line like any net and has one symbol, at the outer
    /// end of its wire: up from the node that is farthest the way the net's stretch of the line
    /// goes (the left end of the input side, the right end of the output), as a person draws the
    /// supply where the power comes in or goes out.
    fn spine_symbols(&mut self) {
        for (net, pref) in self.spine_pref.clone() {
            let Some(kind) = self.ctx.power[net] else {
                continue;
            };
            let Some(tree) = self.trees.get(&net) else {
                continue;
            };
            let mut nodes: Vec<Node> = tree.iter().copied().collect();
            nodes.sort_by_key(|n| (-pref * n.0, n.1));
            let first = nodes.first().copied();
            let found = nodes
                .into_iter()
                .take(80)
                .find_map(|q| self.symbol_fits(net, kind, q).map(|label| (q, label)));
            if let Some((q, label)) = found {
                self.place_symbol(net, kind, q, Some(label));
                continue;
            }
            // No room by the wire (a long name above the part): the wire goes on outward until
            // a symbol has.
            let Some(q0) = first else {
                continue;
            };
            let id = Self::net_id(net);
            for k in 1..=40 {
                let q = (q0.0 + pref * k, q0.1);
                let before = (q.0 - pref, q.1);
                let dir = if pref > 0 { Dir::Right } else { Dir::Left };
                if !self.grid.free_for(q, id) || self.grid.step_is_cut(before, dir) {
                    break;
                }
                if let Some(label) = self.symbol_fits(net, kind, q) {
                    self.grid.claim_wire(q0, q, id);
                    self.push_wire(net, &[q0, q]);
                    self.place_symbol(net, kind, q, Some(label));
                    break;
                }
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

    /// Where a wire of a net may end: at one of its pins, power symbols, flags or labels.
    fn anchors(&self) -> BTreeSet<(usize, Node)> {
        let mut out = BTreeSet::new();
        for part in &self.parts {
            for pin in &part.pins {
                if let Some(net) = pin.net {
                    out.insert((net, self.grid.node(pin.at)));
                }
            }
        }
        for s in &self.symbols {
            out.insert((s.net, self.grid.node(s.at)));
        }
        for p in &self.ports {
            out.insert((p.net, self.grid.node(p.at)));
        }
        for l in &self.labels {
            out.insert((l.net, self.grid.node(l.at)));
        }
        for b in &self.buses {
            for e in &b.entries {
                out.insert((e.net, self.grid.node(e.at)));
            }
        }
        out
    }

    fn finish(mut self) -> Drawn {
        self.join_and_stub();
        self.plan_tags();
        self.plan_buses();
        // The rows of the spines first: the wires of the pair satellites join them where they
        // meet, and nothing else is wired along them.
        self.route_signals(true);
        self.route_pairs();
        self.route_rails();
        self.route_signals(false);
        self.spine_symbols();
        let marks = self.pin_marks();
        let anchors = self.anchors();
        // Take out what no pin needs, and put a dot where three arms meet.
        let (wires, junctions) =
            prune::prune(&self.grid, std::mem::take(&mut self.wires), &anchors);
        self.wires = wires;

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
        for l in &self.labels {
            bounds.union(l.text.extent());
        }
        for b in &self.buses {
            for line in &b.lines {
                for &pt in line {
                    bounds.include(pt);
                }
            }
            for t in b.texts() {
                bounds.union(t.extent());
            }
            if let Some(f) = &b.flag {
                bounds.union(f.extent());
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
        Drawn {
            layout: Layout {
                parts: self.parts,
                symbols: self.symbols,
                ports: self.ports,
                marks,
                sheets: Vec::new(),
                wires: self.wires,
                junctions,
                labels: self.labels,
                buses: self.buses,
                spines: Vec::new(),
                bounds,
            },
            clusters: self.clusters,
            bus_nets: self.bus_done,
            pair_nets: self.pair_nets,
        }
    }
}
