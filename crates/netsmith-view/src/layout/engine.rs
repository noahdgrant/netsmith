//! The engine: places the parts as a person would (the main parts in rows, the small parts
//! that serve a pin beside that pin), then draws the rails as power symbols, the nets that leave
//! the sheet as flags and the nets [`super::policy`] chose as names at the end of stubs, and
//! wires every other net on a grid.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::grid::{self, Grid, Node};
use super::prune;
use super::*;

mod block;
mod bus;
mod pack;
mod pair;
mod scene;
mod spine;
mod tag;

pub(super) use pair::Cache as PairCache;

use block::*;
use pack::*;
use scene::*;
use tag::*;

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
/// The width of a row is a multiple of this (a coarse step, so that a small edit leaves it).
const ROW_QUANTUM: f64 = 50.8;
/// A piece counts as a multiple of this wide when pieces are cut into rows.
const SLOT_X: f64 = 12.7;
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
/// A supply's pins are close when their stubs end within this many grid cells (about 20 mm) of each other, and there are at
/// most [`CLOSE_MAX_PINS`] of them: then it is wired, with its name on a stem, and has no symbol.
const CLOSE_CELLS: i32 = 16;
const CLOSE_MAX_PINS: usize = 4;

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
    /// How far the layout file moved each box from where the engine put it; empty when it
    /// moved none.
    pub shifts: Vec<Pt>,
    /// What the engine could not honour of the file's moves.
    pub findings: Vec<crate::hints::HintFinding>,
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
    /// Set for the source of a link: the next piece is its load, and the two stay in one row.
    joined: bool,
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
        netsmith_core::job::checkpoint();
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
        netsmith_core::job::checkpoint();
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
                && (super::is_series_kind(&ctx.design.parts[part])
                    || placed[part]
                        .pins
                        .iter()
                        .all(|q| q.net.is_none_or(|n| ctx.power[n].is_none())));
            let side = placed[c.anchor].pins[c.pin].escape;
            if series && !side.is_vertical() && !ctx.hints.turned(part) {
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
        netsmith_core::job::checkpoint();
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
        netsmith_core::job::checkpoint();
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
            // A turn the file asks for stands.
            if ctx.hints.turned(p) {
                continue;
            }
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
        netsmith_core::job::checkpoint();
        let ext = placed[b.anchor].extent;
        let (supplies, grounds) = rails_of(&b.members);
        spine_nets.extend(b.spine_nets.iter().copied());
        // The load of a link is level with the row that runs into it: on its pin.
        let load_row = ctx
            .spines
            .iter()
            .find(|s| s.load == Some(b.anchor))
            .and_then(|s| {
                let net = *s.output.nodes.last()?;
                placed[b.anchor]
                    .pins
                    .iter()
                    .filter(|q| q.net == Some(net))
                    .map(|q| q.at.y)
                    .min_by(f64::total_cmp)
            });
        pieces.push(Piece {
            anchor: Some(b.anchor),
            members: b.members,
            rect: b.rect,
            middle: b
                .level
                .or(load_row)
                .unwrap_or((ext.min.y + ext.max.y) / 2.0),
            supplies,
            grounds,
            lanes: b.spine_lanes,
            joined: ctx
                .spines
                .iter()
                .any(|s| s.ic == b.anchor && s.load.is_some()),
        });
    }
    let free: Vec<usize> = (0..units.len())
        .filter(|&u| role[u] == Role::Free || (role[u] == Role::Satellite && !tied[u]))
        .collect();
    for &u in &free {
        netsmith_core::job::checkpoint();
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
            joined: false,
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
    // The piece of the sheet's reference part (`origin`): the main part with most pins, then the
    // largest symbol, then the first by name.
    let ref_piece = {
        let key = |a: usize| {
            let r = placed[a].glyph.bounds();
            (
                placed[a].pins.len(),
                ((r.max.x - r.min.x) * (r.max.y - r.min.y) * 100.0).round() as i64,
            )
        };
        (0..pieces.len())
            .filter(|&i| pieces[i].anchor.is_some())
            .max_by(|&i, &j| {
                let (a, b) = (
                    pieces[i].anchor.expect("an anchor"),
                    pieces[j].anchor.expect("an anchor"),
                );
                let name = |p: usize| canon::part_name(&ctx.design.parts[p]);
                key(a)
                    .cmp(&key(b))
                    .then_with(|| canon::natural_cmp(name(b), name(a)))
            })
    };
    for row in &rows {
        let (above, below) = row_height(&pieces, row);
        if let Some((bottom, before)) = previous {
            y = bottom + row_gap(&pieces, before, row) + above;
        }
        previous = Some((y + below, row));
        // The row is laid out outward from one piece: the one with the sheet's reference part if
        // it is in this row, else the first. Its main part is at x = 0 (not its left edge: what
        // it grows to the left of its part does not move the part), the pieces after it follow to
        // the right and those before it to the left, so that a block that grows moves only the
        // pieces beyond it, and what is on the far side of the reference part stays.
        let lead = |i: usize| {
            pieces[i].anchor.map_or(0.0, |a| {
                snap_grid(placed[a].extent.min.x - pieces[i].rect.min.x)
            })
        };
        let start = ref_piece
            .and_then(|r| row.iter().position(|&i| i == r))
            .unwrap_or(0);
        let mut by_x = vec![0.0; row.len()];
        let mut x = -lead(row[start]);
        for k in start..row.len() {
            let piece = &pieces[row[k]];
            by_x[k] = snap_up(x - piece.rect.min.x, GRID);
            x = by_x[k] + piece.rect.max.x + BLOCK_GAP;
        }
        let mut edge = by_x[start] + pieces[row[start]].rect.min.x;
        for k in (0..start).rev() {
            let piece = &pieces[row[k]];
            let right = edge - BLOCK_GAP - piece.rect.max.x;
            by_x[k] = (right / GRID + 1e-9).floor() * GRID;
            edge = by_x[k] + piece.rect.min.x;
        }
        let mut group = Vec::new();
        for (k, &i) in row.iter().enumerate() {
            let piece = &pieces[i];
            let by = Pt::new(by_x[k], snap_grid(y - piece.middle));
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
        }
        groups.push(group);
    }
    for p in 0..n {
        if let Some(side) = text_side[p] {
            set_text_side(ctx, &mut placed[p], side);
        }
    }
    // The layout file's moves: every block is in its place and nothing is routed yet, so the
    // wires go round the result and are never moved after (ADR 0061).
    let mut shifts: Vec<Pt> = Vec::new();
    let mut hint_findings = Vec::new();
    if !ctx.hints.moves.is_empty() {
        let names: Vec<String> = (0..n)
            .map(|p| canon::part_name(&ctx.design.parts[p]).to_string())
            .collect();
        let rects: Vec<Rect> = placed.iter().map(|p| p.extent).collect();
        // The block a move carries: a main part's block (the satellites that serve it), or the
        // unit of parts the part stands in (a satellite, or parts that stand alone).
        let carried = |p: usize| -> (Vec<usize>, bool) {
            match pieces.iter().find(|piece| piece.anchor == Some(p)) {
                Some(piece) => (piece.members.iter().map(|m| m.0).collect(), true),
                None => (units[unit_index(units, p)].parts.clone(), false),
            }
        };
        let (by, found) = hinting::resolve(
            &ctx.hints,
            &hinting::Frame {
                at: &at,
                rect: &rects,
                names: &names,
                carried: &carried,
            },
        );
        for p in 0..n {
            at[p] = Pt::new(at[p].x + by[p].x, at[p].y + by[p].y);
        }
        shifts = by;
        hint_findings = found;
    }
    for p in 0..n {
        translate_part(&mut placed[p], at[p]);
    }

    groups.retain(|g| !g.is_empty());
    let row_count = rows.len();
    let mut drawn = Scene::new(
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
    .finish();
    drawn.layout.rows = row_count;
    drawn.shifts = shifts;
    drawn.findings = hint_findings;
    drawn
}
