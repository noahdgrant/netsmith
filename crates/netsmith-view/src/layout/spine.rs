//! The **spine** of a regulator: the power path through an IC, found from the netlist, the pin
//! kinds and the rails alone (never from what a part is called).
//!
//! A person draws a buck converter, an LDO or a protected input as one line from left to right:
//! the input protection (a fuse, a diode, a ferrite bead: parts in series), the input capacitors
//! hanging from it to ground, the IC's input pins, the IC, its switching pin, the inductor in
//! series, the output node with the output capacitors and the feedback divider hanging from it,
//! and the output. This module finds that line:
//!
//! - the **input side**: the net of the IC's input pins (power pins that take current, most pins
//!   first) and the chain of **series parts** that leads from it outward to where the power
//!   comes from;
//! - the **output side**: the net of a pin that gives (a switching pin, an output) and the chain
//!   that leads outward to a supply rail;
//! - the **free chains**: series parts with a supply rail on them that join neither side on this
//!   sheet (the sheet's own input protection, whose far end goes on to another sheet): they are
//!   the first stretch of the line, left of the input side;
//! - on each node of the line the **shunts**: parts to ground (capacitors, a TVS) and, where a
//!   part joins the node to a net of the IC's other pins (the feedback pin), the **tap**: the
//!   divider that hangs there.
//!
//! A **series part** is a two-terminal part whose two pins are on two different nets, neither a
//! ground, that is not across two pins of the IC (that is a pair satellite), and whose far net
//! does not touch the IC again (the one exception is an inductor between the switching node
//! and an output rail the IC senses with another pin). A chain goes on through a net only while
//! exactly one more series part is there and no main part (a connector, an IC) has a pin on it.
//!
//! The IC qualifies when it has both sides and the rails are known. `engine/spine.rs` draws it.
//!
//! A **link** is the same line with no IC: a chain of series parts that runs from one main part
//! (a connector the power comes in at) to another (what it goes to), a supply rail at each end.
//! The part at the end with no parts to ground on it is the **source**: it is the `ic` of the
//! spine, its input side is empty and its output side is the chain; the other end is the
//! **load**, which the sheet puts next to the end of the row and level with it.

use super::*;

use std::collections::{BTreeMap, BTreeSet};

/// Parts and nets of one stretch of the line, from where it meets the IC (or, for a free
/// chain, the IC's side of it) outward.
#[derive(Debug, Clone, Default)]
pub(crate) struct Side {
    /// The series parts, in order outward.
    pub parts: Vec<usize>,
    /// `nodes[0]` is the net next to the IC; `nodes[i + 1]` is the net beyond `parts[i]`.
    pub nodes: Vec<usize>,
    /// The parts to ground on each node, the smallest capacitor first.
    pub grounds: Vec<Vec<usize>>,
    /// The divider that hangs on each node.
    pub taps: Vec<Option<Tap>>,
}

impl Side {
    fn new(parts: Vec<usize>, nodes: Vec<usize>) -> Side {
        let len = nodes.len();
        Side {
            parts,
            nodes,
            grounds: vec![Vec::new(); len],
            taps: vec![None; len],
        }
    }
}

/// A net of the IC's own pins (the feedback pin) that a part joins to a node: the parts across
/// the two (`tops`) and the parts from it to ground (`bottoms`).
#[derive(Debug, Clone)]
pub(crate) struct Tap {
    pub net: usize,
    pub tops: Vec<usize>,
    pub bottoms: Vec<usize>,
}

/// The line through one IC.
#[derive(Debug, Clone)]
pub(crate) struct Spine {
    pub ic: usize,
    pub input: Side,
    pub output: Side,
    /// Chains to the left of the input side, the nearest first.
    pub free: Vec<Side>,
    /// For a link: the main part at the far end of the output chain (the `ic` is the source and
    /// the input side is empty).
    pub load: Option<usize>,
}

/// The longest chain of series parts on one side.
const MAX_PARTS: usize = 6;

impl Spine {
    /// Every part the line owns, the IC left out.
    pub fn parts(&self) -> Vec<usize> {
        let mut out = Vec::new();
        for side in self.sides() {
            out.extend(side.parts.iter().copied());
            for g in &side.grounds {
                out.extend(g.iter().copied());
            }
            for t in side.taps.iter().flatten() {
                out.extend(t.tops.iter().chain(&t.bottoms).copied());
            }
        }
        out
    }

    /// Every net on the line, with the way its rail's symbol goes: toward the left (-1) or the
    /// right (1) end of the drawing.
    pub fn nets(&self) -> Vec<(usize, i32)> {
        let mut out = Vec::new();
        for n in &self.input.nodes {
            out.push((*n, -1));
        }
        for (i, n) in self.output.nodes.iter().enumerate() {
            // A link's source end is wired straight to its part: no symbol (a symbol that joins
            // nothing else on the sheet reads as a port to somewhere); its far end, the rail the
            // load takes, has the one symbol.
            let pref = if self.load.is_some() && i + 1 < self.output.nodes.len() {
                0
            } else {
                1
            };
            out.push((*n, pref));
        }
        for side in &self.free {
            let last = side.nodes.len() - 1;
            for (i, n) in side.nodes.iter().enumerate() {
                out.push((*n, if i == 0 && last > 0 { 1 } else { -1 }));
            }
        }
        out
    }

    pub fn sides(&self) -> impl Iterator<Item = &Side> {
        [&self.input, &self.output].into_iter().chain(&self.free)
    }
}

fn parts_on(ctx: &Ctx, net: usize) -> Vec<usize> {
    let mut out: Vec<usize> = ctx.design.nets[net]
        .pins
        .iter()
        .map(|pin| part_index(ctx, &pin.refdes))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The two nets of a part that can be in series: two pins facing opposite ways, on two nets,
/// neither of them a ground.
fn series_nets(ctx: &Ctx, p: usize) -> Option<[usize; 2]> {
    if !is_stackable(ctx, p) || ctx.glyphs[p].pins.iter().any(|q| q.hidden) {
        return None;
    }
    let g = &ctx.glyphs[p];
    let a = net_of(ctx, p, &g.pins[0].id)?;
    let b = net_of(ctx, p, &g.pins[1].id)?;
    (a != b && !ctx.ground[a] && !ctx.ground[b]).then_some([a, b])
}

/// The far net of a part to ground on `net`, if the part is one.
fn to_ground(ctx: &Ctx, p: usize, net: usize) -> bool {
    if !is_stackable(ctx, p) || ctx.glyphs[p].pins.iter().any(|q| q.hidden) {
        return false;
    }
    let g = &ctx.glyphs[p];
    let (Some(a), Some(b)) = (net_of(ctx, p, &g.pins[0].id), net_of(ctx, p, &g.pins[1].id)) else {
        return false;
    };
    (a == net && ctx.ground[b]) || (b == net && ctx.ground[a])
}

fn touches(ctx: &Ctx, net: usize, ic: usize) -> bool {
    ctx.design.nets[net]
        .pins
        .iter()
        .any(|pin| part_index(ctx, &pin.refdes) == ic)
}

/// Whether a net has a pin of a main part (not a small part to stand beside a pin) other than
/// `ic`: a connector, another IC.
fn has_other_main(ctx: &Ctx, net: usize, ic: usize) -> bool {
    parts_on(ctx, net)
        .into_iter()
        .any(|p| p != ic && !is_stackable(ctx, p))
}

/// A part across two nets that are neither rails nor leave the sheet, each with a pin of `ic`:
/// a pair satellite, whatever it is (a bootstrap capacitor, a crystal).
fn pair_like(ctx: &Ctx, ic: usize, nets: [usize; 2]) -> bool {
    nets.iter()
        .all(|&n| ctx.power[n].is_none() && !ctx.external[n] && touches(ctx, n, ic))
}

/// The capacitance (or any number with an SI suffix) a value says, for ordering parts; a value
/// that is not a number is last.
pub(crate) fn value_of(text: &str) -> f64 {
    let t = text.trim();
    let end = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(t.len());
    let Ok(n) = t[..end].parse::<f64>() else {
        return f64::MAX;
    };
    let scale = match t[end..].chars().next() {
        Some('p') => 1e-12,
        Some('n') => 1e-9,
        Some('u' | 'µ') => 1e-6,
        Some('m') => 1e-3,
        Some('k') => 1e3,
        Some('M') => 1e6,
        _ => 1.0,
    };
    n * scale
}

/// The series parts on `net` that continue a chain: `(part, far net)`, the parts of `taken`
/// and the nets of `nodes` left out. `first` is the first step out of the IC's own net.
fn candidates(
    ctx: &Ctx,
    ic: usize,
    net: usize,
    taken: &[bool],
    nodes: &[usize],
    first: bool,
) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for p in parts_on(ctx, net) {
        if taken[p] || p == ic {
            continue;
        }
        let Some(nets) = series_nets(ctx, p) else {
            continue;
        };
        let far = if nets[0] == net {
            nets[1]
        } else if nets[1] == net {
            nets[0]
        } else {
            continue;
        };
        if nodes.contains(&far) {
            continue;
        }
        if pair_like(ctx, ic, nets) {
            continue;
        }
        if touches(ctx, far, ic) {
            // An inductor between the switching node and the rail the IC senses with another
            // pin: the one way a series part ends on the IC's own net.
            let sensed = first
                && ctx.power[far] == Some(PowerKind::Supply)
                && ctx.power[net] != Some(PowerKind::Supply);
            if !sensed {
                continue;
            }
        }
        out.push((p, far));
    }
    out
}

/// The chain that leaves `net` outward: its parts and nodes (the first node is `net`).
fn walk(ctx: &Ctx, ic: usize, net: usize, taken: &mut [bool]) -> (Vec<usize>, Vec<usize>) {
    let mut parts: Vec<usize> = Vec::new();
    let mut nodes = vec![net];
    while parts.len() < MAX_PARTS {
        let at = *nodes.last().expect("a node");
        let mut found = candidates(ctx, ic, at, taken, &nodes, parts.is_empty());
        if !parts.is_empty() {
            // A node that goes on only if one series part is there and nothing else main.
            if found.len() != 1 || has_other_main(ctx, at, ic) {
                break;
            }
        }
        // The one that leads to a supply rail first, then the first by index.
        found.sort_by_key(|&(p, far)| (ctx.power[far] != Some(PowerKind::Supply), p));
        let Some(&(p, far)) = found.first() else {
            break;
        };
        taken[p] = true;
        parts.push(p);
        nodes.push(far);
    }
    (parts, nodes)
}

/// Fills in the shunts of every node of `side`: the parts to ground, and the tap.
fn hang(ctx: &Ctx, ic: usize, side: &mut Side, taken: &mut [bool], with_taps: bool) {
    for i in 0..side.nodes.len() {
        let node = side.nodes[i];
        let mut grounds: Vec<usize> = parts_on(ctx, node)
            .into_iter()
            .filter(|&p| !taken[p] && p != ic && to_ground(ctx, p, node))
            .collect();
        grounds.sort_by(|&a, &b| {
            value_of(&ctx.design.parts[a].value)
                .total_cmp(&value_of(&ctx.design.parts[b].value))
                .then(a.cmp(&b))
        });
        for &p in &grounds {
            taken[p] = true;
        }
        side.grounds[i] = grounds;
        if !with_taps {
            continue;
        }
        // A part from the node to a net that has a pin of the IC (the feedback pin).
        let mut tops: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for p in parts_on(ctx, node) {
            if taken[p] || p == ic {
                continue;
            }
            let Some(nets) = series_nets(ctx, p) else {
                continue;
            };
            let far = if nets[0] == node { nets[1] } else { nets[0] };
            if nets[0] != node && nets[1] != node {
                continue;
            }
            if ctx.power[far].is_none()
                && !ctx.external[far]
                && touches(ctx, far, ic)
                && !pair_like(ctx, ic, nets)
            {
                tops.entry(far).or_default().push(p);
            }
        }
        let Some((&net, parts)) = tops.iter().next() else {
            continue;
        };
        let bottoms: Vec<usize> = parts_on(ctx, net)
            .into_iter()
            .filter(|&p| !taken[p] && !parts.contains(&p) && to_ground(ctx, p, net))
            .collect();
        for &p in parts.iter().chain(&bottoms) {
            taken[p] = true;
        }
        side.taps[i] = Some(Tap {
            net,
            tops: parts.clone(),
            bottoms,
        });
    }
}

/// The line through every IC of the sheet that has one. Nothing is found when the rails are
/// not known: the line is a power path, and a rail is what says so.
pub(crate) fn find(ctx: &Ctx) -> Vec<Spine> {
    if !ctx.power.contains(&Some(PowerKind::Supply)) {
        return Vec::new();
    }
    let mut taken = vec![false; ctx.design.parts.len()];
    let mut out = Vec::new();
    for ic in 0..ctx.design.parts.len() {
        if let Some(spine) = spine_of(ctx, ic, &mut taken) {
            out.push(spine);
        }
    }
    for a in 0..ctx.design.parts.len() {
        if let Some(link) = link_of(ctx, a, &mut taken) {
            out.push(link);
        }
    }
    out
}

/// Whether `p` can stand at the end of a link: a main part whose turn is the line's to choose
/// (not a table of pins, not a unit of a symbol).
fn is_end(ctx: &Ctx, p: usize) -> bool {
    let g = &ctx.glyphs[p];
    g.symbol.is_some()
        && !g.dense
        && g.unit.is_none()
        && !is_stackable(ctx, p)
        && g.pins.iter().filter(|q| !q.hidden).count() >= 2
}

/// The number of parts to ground on `net`, other than those taken.
fn shunts(ctx: &Ctx, net: usize, taken: &[bool]) -> usize {
    parts_on(ctx, net)
        .into_iter()
        .filter(|&p| !taken[p] && to_ground(ctx, p, net))
        .count()
}

/// Whether a series part that no chain has taken is on `net`.
fn in_series_elsewhere(ctx: &Ctx, net: usize, taken: &[bool]) -> bool {
    parts_on(ctx, net)
        .into_iter()
        .any(|p| !taken[p] && series_nets(ctx, p).is_some())
}

/// The link that leaves main part `a` by one of its nets: a chain of series parts, each net of it
/// with one series part on it besides the one before (a rail that feeds several branches is no
/// chain), that ends on a net with a pin of another main part, with a supply rail somewhere on it.
fn link_of(ctx: &Ctx, a: usize, taken: &mut [bool]) -> Option<Spine> {
    if taken[a] || !is_end(ctx, a) {
        return None;
    }
    let mut nets: Vec<usize> = (0..ctx.design.nets.len())
        .filter(|&n| !ctx.ground[n] && touches(ctx, n, a))
        .collect();
    nets.sort_unstable();
    for start in nets {
        let mut scratch = taken.to_vec();
        scratch[a] = true;
        // The first step must be the only way out of the net.
        if candidates(ctx, a, start, &scratch, &[start], false).len() != 1 {
            continue;
        }
        let (parts, nodes) = walk(ctx, a, start, &mut scratch);
        let end = *nodes.last().expect("a node");
        if parts.is_empty() || end == start {
            continue;
        }
        let Some(b) = parts_on(ctx, end)
            .into_iter()
            .find(|&p| p != a && !taken[p] && is_end(ctx, p))
        else {
            continue;
        };
        // Both ends are supply rails: the chain carries power from one to the other. A resistor
        // from a rail to a signal net (a pull-up) is no link.
        if [start, end]
            .iter()
            .any(|&n| ctx.power[n] != Some(PowerKind::Supply))
        {
            continue;
        }
        // Neither end is a rail that feeds other series parts too (the pull-ups of a rail, the
        // branches of a supply): that is a net with parts beside its pins, not the end of a chain.
        for &p in &parts {
            scratch[p] = true;
        }
        if [start, end]
            .iter()
            .any(|&n| in_series_elsewhere(ctx, n, &scratch))
        {
            continue;
        }
        // The end with parts to ground on it is the one the load is on; with none, or as many on
        // each, the chain stays as it was found.
        let (mut ic, mut load) = (a, b);
        let (mut parts, mut nodes) = (parts, nodes);
        scratch = taken.to_vec();
        scratch[a] = true;
        scratch[b] = true;
        for &p in &parts {
            scratch[p] = true;
        }
        if shunts(ctx, start, &scratch) > shunts(ctx, end, &scratch) {
            std::mem::swap(&mut ic, &mut load);
            parts.reverse();
            nodes.reverse();
        }
        taken[ic] = true;
        for &p in &parts {
            taken[p] = true;
        }
        let mut output = Side::new(parts, nodes);
        hang(ctx, ic, &mut output, taken, false);
        return Some(Spine {
            ic,
            input: Side::default(),
            output,
            free: Vec::new(),
            load: Some(load),
        });
    }
    None
}

fn is_main(ctx: &Ctx, ic: usize) -> bool {
    let g = &ctx.glyphs[ic];
    g.symbol.is_some()
        && !g.dense
        && g.unit.is_none()
        && !ctx.connector[ic]
        && !is_stackable(ctx, ic)
        && g.pins.iter().filter(|p| !p.hidden).count() >= 3
}

/// The best output chain so far: how good it is, its parts and its nodes.
struct Found {
    key: (usize, usize),
    parts: Vec<usize>,
    nodes: Vec<usize>,
}

fn spine_of(ctx: &Ctx, ic: usize, taken: &mut [bool]) -> Option<Spine> {
    if taken[ic] || !is_main(ctx, ic) {
        return None;
    }
    let g = &ctx.glyphs[ic];
    // The input: the net with the most power pins that take current.
    let mut taking: BTreeMap<usize, usize> = BTreeMap::new();
    for pin in g.pins.iter().filter(|p| !p.hidden) {
        let Some(n) = net_of(ctx, ic, &pin.id) else {
            continue;
        };
        if !ctx.ground[n] && pin.flow.takes() && pin.kind.starts_with("power") {
            *taking.entry(n).or_default() += 1;
        }
    }
    let input_net = taking
        .iter()
        .max_by_key(|&(&n, &count)| (count, std::cmp::Reverse(n)))
        .map(|(&n, _)| n)?;

    // The output: a net of the IC's pins whose chain reaches a supply rail, or a supply rail
    // an output pin of the IC is on.
    let mut best: Option<Found> = None;
    let mut nets: Vec<usize> = (0..ctx.design.nets.len())
        .filter(|&n| n != input_net && !ctx.ground[n] && touches(ctx, n, ic))
        .collect();
    nets.sort_unstable();
    for net in nets {
        let mut scratch = taken.to_vec();
        scratch[ic] = true;
        let (parts, nodes) = walk(ctx, ic, net, &mut scratch);
        let end = *nodes.last().expect("a node");
        let gives = g
            .pins
            .iter()
            .any(|pin| !pin.hidden && pin.flow.gives() && net_of(ctx, ic, &pin.id) == Some(net));
        let rail_end = ctx.power[end] == Some(PowerKind::Supply);
        // A pin that takes (an input, a sense pin) starts no output: a divider on an ADC pin
        // is not the way out of a regulator.
        let takes = g
            .pins
            .iter()
            .any(|pin| !pin.hidden && pin.flow.takes() && net_of(ctx, ic, &pin.id) == Some(net));
        let valid = if parts.is_empty() {
            gives && rail_end
        } else {
            rail_end && !takes && !nodes.contains(&input_net)
        };
        // The output is a node with a capacitor (or another part) to ground on it: a pull-up to
        // a rail from an open-drain pin is no way out of a regulator.
        let loaded = parts_on(ctx, end)
            .into_iter()
            .any(|p| p != ic && !scratch[p] && to_ground(ctx, p, end));
        if !valid || !loaded {
            continue;
        }
        let key = (usize::from(gives) + parts.len(), usize::MAX - net);
        if best.as_ref().is_none_or(|b| key > b.key) {
            best = Some(Found { key, parts, nodes });
        }
    }
    let Found {
        parts: out_parts,
        nodes: out_nodes,
        ..
    } = best?;

    taken[ic] = true;
    // The output chain first (its parts are taken), then the input chain.
    let mut output = Side::new(out_parts, out_nodes);
    for &p in &output.parts {
        taken[p] = true;
    }
    let mut scratch = taken.to_vec();
    let (in_parts, in_nodes) = walk(ctx, ic, input_net, &mut scratch);
    if in_nodes.iter().any(|n| output.nodes.contains(n)) {
        taken[ic] = false;
        for &p in &output.parts {
            taken[p] = false;
        }
        return None;
    }
    for &p in &in_parts {
        taken[p] = true;
    }
    let mut input = Side::new(in_parts, in_nodes);
    // The shunts: the input's first (its capacitors), then the output's with the divider.
    hang(ctx, ic, &mut input, taken, false);
    hang(ctx, ic, &mut output, taken, true);
    let free = free_chains(ctx, ic, taken);
    Some(Spine {
        ic,
        input,
        output,
        free,
        load: None,
    })
}

/// Series parts with a supply rail on them that touch neither side of the line, as chains whose
/// first node is the one with more parts to ground (the end the load is on).
fn free_chains(ctx: &Ctx, ic: usize, taken: &mut [bool]) -> Vec<Side> {
    let mut out: Vec<Side> = Vec::new();
    for start in 0..ctx.design.parts.len() {
        if taken[start] || out.len() >= 2 {
            continue;
        }
        let Some(nets) = series_nets(ctx, start) else {
            continue;
        };
        if pair_like(ctx, ic, nets) || nets.iter().any(|&n| touches(ctx, n, ic)) {
            continue;
        }
        // Grow both ways from the part.
        let mut scratch = taken.to_vec();
        scratch[start] = true;
        let (right_parts, right_nodes) = walk(ctx, ic, nets[1], &mut scratch);
        let (left_parts, left_nodes) = walk(ctx, ic, nets[0], &mut scratch);
        // From the left end to the right end.
        let mut parts: Vec<usize> = left_parts.iter().rev().copied().collect();
        parts.push(start);
        parts.extend(right_parts.iter().copied());
        let mut nodes: Vec<usize> = left_nodes.iter().rev().copied().collect();
        nodes.extend(right_nodes.iter().copied());
        if !nodes
            .iter()
            .any(|&n| ctx.power[n] == Some(PowerKind::Supply))
            || nodes.iter().any(|&n| touches(ctx, n, ic))
        {
            continue;
        }
        for &p in &parts {
            taken[p] = true;
        }
        // The end with more parts to ground is the one the load is on: it goes next to the
        // line.
        let shunts = |n: usize| {
            parts_on(ctx, n)
                .into_iter()
                .filter(|&p| !taken[p] && to_ground(ctx, p, n))
                .count()
        };
        let (first, last) = (nodes[0], nodes[nodes.len() - 1]);
        if (shunts(first), first) > (shunts(last), last) {
            parts.reverse();
            nodes.reverse();
        }
        // `nodes` runs left to right; the side runs outward, that is right to left.
        parts.reverse();
        nodes.reverse();
        let mut side = Side::new(parts, nodes);
        hang(ctx, ic, &mut side, taken, false);
        out.push(side);
    }
    out
}

/// The turn of the IC that puts the input of the line on its left and the switching pin and the
/// output on its right, the pins of the feedback divider on the output's side, the grounds below;
/// the two rows level where it can. Ties keep the library's drawing.
pub(crate) fn orientation(ctx: &Ctx, spine: &Spine) -> Orient {
    let glyph = &ctx.glyphs[spine.ic];
    let left: BTreeSet<usize> = spine.input.nodes.iter().copied().collect();
    let mut right: BTreeSet<usize> = spine.output.nodes.iter().copied().collect();
    for tap in spine.output.taps.iter().flatten() {
        right.insert(tap.net);
    }
    let readable: Vec<Orient> = Orient::ALL
        .into_iter()
        .filter(|&o| !glyph.texts_collide(o))
        .collect();
    let candidates = if readable.is_empty() {
        Orient::ALL.to_vec()
    } else {
        readable
    };
    let mut best = (f64::NEG_INFINITY, candidates[0]);
    for orient in candidates {
        let mut score = 0.0;
        let (mut y_in, mut y_out): (Option<f64>, Option<f64>) = (None, None);
        for pin in glyph.pins.iter().filter(|p| !p.hidden) {
            let Some(n) = net_of(ctx, spine.ic, &pin.id) else {
                continue;
            };
            let escape = orient.dir((-pin.toward_body.0, -pin.toward_body.1));
            let y = orient.apply(pin.at).y;
            let want = if left.contains(&n) {
                Some(Dir::Left)
            } else if right.contains(&n) {
                Some(Dir::Right)
            } else {
                None
            };
            if let Some(want) = want {
                score += if escape == want {
                    10.0
                } else if escape == want.opposite() {
                    -10.0
                } else {
                    -3.0
                };
                if escape == want && n == spine.input.nodes[0] {
                    y_in = Some(y_in.map_or(y, |v: f64| v.min(y)));
                }
                if escape == want && n == spine.output.nodes[0] {
                    y_out = Some(y_out.map_or(y, |v: f64| v.min(y)));
                }
            }
            if ctx.ground[n] {
                score += match escape {
                    Dir::Down => 2.0,
                    Dir::Up => -2.0,
                    _ => 0.0,
                };
            } else if want.is_none() && ctx.power[n] == Some(PowerKind::Supply) {
                score += match escape {
                    Dir::Up => 1.0,
                    Dir::Down => -1.0,
                    _ => 0.0,
                };
            }
        }
        if let (Some(a), Some(b)) = (y_in, y_out) {
            score -= 0.1 * ((a - b).abs() / GRID).min(20.0);
        }
        if score > best.0 + 1e-9 {
            best = (score, orient);
        }
    }
    best.1
}

/// The turn of an end of a link: the pins on the net of the chain leave toward it (the source's to
/// the right, the load's to the left), grounds below and other supplies above, and the library's
/// drawing when two turns tie.
pub(crate) fn link_orientation(ctx: &Ctx, spine: &Spine, p: usize) -> Orient {
    let (net, want) = if p == spine.ic {
        (spine.output.nodes[0], Dir::Right)
    } else {
        (*spine.output.nodes.last().expect("a node"), Dir::Left)
    };
    let glyph = &ctx.glyphs[p];
    let readable: Vec<Orient> = Orient::ALL
        .into_iter()
        .filter(|&o| !glyph.texts_collide(o))
        .collect();
    let candidates = if readable.is_empty() {
        Orient::ALL.to_vec()
    } else {
        readable
    };
    let mut best = (f64::NEG_INFINITY, candidates[0]);
    for orient in candidates {
        let mut score = 0.0;
        for pin in glyph.pins.iter().filter(|q| !q.hidden) {
            let Some(n) = net_of(ctx, p, &pin.id) else {
                continue;
            };
            let escape = orient.dir((-pin.toward_body.0, -pin.toward_body.1));
            if n == net {
                score += if escape == want {
                    10.0
                } else if escape == want.opposite() {
                    -10.0
                } else {
                    -3.0
                };
            } else if ctx.ground[n] {
                score += match escape {
                    Dir::Down => 2.0,
                    Dir::Up => -2.0,
                    _ => 0.0,
                };
            } else if ctx.power[n] == Some(PowerKind::Supply) {
                score += match escape {
                    Dir::Up => 1.0,
                    Dir::Down => -1.0,
                    _ => 0.0,
                };
            }
        }
        if score > best.0 + 1e-9 {
            best = (score, orient);
        }
    }
    best.1
}
