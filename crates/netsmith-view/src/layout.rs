//! Where each part goes and how each net is drawn.
//!
//! The picture reads left to right along the signal and top to bottom along the supply:
//!
//! 1. Parts that sit in series between two nets are stacked into one column, so the net
//!    between them is a short straight wire. Everything else gets a column of its own.
//! 2. Columns run in the order a breadth-first walk meets them, starting from a
//!    connector, and turned around when the parts' pin directions say the signal runs
//!    the other way.
//! 3. Each part is turned or mirrored so that its pins on higher nets point up, its pins
//!    on lower nets point down, and its pins point toward the parts they connect to.
//!    "Lower" means closer to ground.
//! 4. Each net becomes a horizontal trunk in a channel above or below the parts, with a
//!    vertical drop to every pin, or a plain wire when only two neighbouring pins meet.
//!    A net too large to draw that way, or with pins that cannot reach its trunk, gets a
//!    short stub and a name at those pins instead.
//!
//! Nothing here knows how a layout is drawn: [`crate::render_svg`] reads the result.

use std::collections::{BTreeSet, HashMap, VecDeque};

use netsmith_core::{Design, DesignError};
use netsmith_kicad::{Mapping, Pinout, SymbolLib};

use crate::geom::{Dir, Orient, Pt, Rect, snap, snap_up};
use crate::glyph::{CHAR_WIDTH, Flow, Glyph, used_pads};

const GRID: f64 = 1.27;
/// Distance from a side-facing pin to its first vertical drop.
const STUB: f64 = 2.54;
/// Distance between the drops of one part's side pins.
const PITCH: f64 = 2.54;
const COLUMN_GAP: f64 = 5.08;
/// The wire between two parts stacked in one column.
const CHAIN_WIRE: f64 = 5.08;
/// Clear space between the outermost part and the first trunk.
const CHANNEL_MARGIN: f64 = 5.08;
const TRACK_PITCH: f64 = 5.08;
const LABEL_STUB: f64 = 2.54;
pub const TEXT_SIZE: f64 = 1.27;
const BORDER: f64 = 5.08;
/// A net name longer than this is cut, with an ellipsis; the drawing keeps the whole name in
/// the tooltip.
pub const MAX_LABEL_CHARS: usize = 24;
/// A wire from a pin to its power symbol, in the direction of the pin.
const STEM: f64 = 2.54;
/// Pins of one power net on one side of a part are joined by one bus and share one symbol
/// when no more than this lies between neighbours.
const MAX_RUN_GAP: f64 = 5.08;
/// A net with more pins than this is drawn with names, not wires.
const MAX_BUS_PINS: usize = 8;
/// A trunk longer than this is drawn with names instead.
const MAX_BUS_SPAN: f64 = 250.0;
/// How many columns apart two pins may be and still be joined by one Z-shaped wire.
const MAX_Z_COLUMNS: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

/// Text at a baseline point.
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub text: String,
    pub at: Pt,
    pub anchor: Anchor,
    /// Written upward (turned a quarter to the left about `at`) instead of across.
    pub vertical: bool,
}

impl Text {
    pub fn new(text: &str, at: Pt, anchor: Anchor) -> Text {
        Text {
            text: text.to_string(),
            at,
            anchor,
            vertical: false,
        }
    }

    /// The same text written upward.
    pub fn turned(mut self) -> Text {
        self.vertical = true;
        self
    }

    /// The box the text covers, at the height of net names.
    pub fn extent(&self) -> Rect {
        self.extent_at(TEXT_SIZE)
    }

    /// The box the text covers when it is `size` high.
    pub fn extent_at(&self, size: f64) -> Rect {
        let w = self.text.chars().count() as f64 * size * CHAR_WIDTH;
        let (x0, x1) = match self.anchor {
            Anchor::Start => (self.at.x, self.at.x + w),
            Anchor::Middle => (self.at.x - w / 2.0, self.at.x + w / 2.0),
            Anchor::End => (self.at.x - w, self.at.x),
        };
        if self.vertical {
            // Written upward: the text starts at the bottom, its letters rise to the left.
            return Rect {
                min: Pt::new(self.at.x - size, self.at.y - (x1 - self.at.x)),
                max: Pt::new(self.at.x + size * 0.3, self.at.y - (x0 - self.at.x)),
            };
        }
        Rect {
            min: Pt::new(x0, self.at.y - size),
            max: Pt::new(x1, self.at.y + size * 0.3),
        }
    }
}

pub fn text_width(text: &str) -> f64 {
    text.chars().count() as f64 * TEXT_SIZE * CHAR_WIDTH
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlacedPin {
    pub id: String,
    /// Where a wire attaches, on the sheet.
    pub at: Pt,
    /// The way a wire leaves the pin, away from the body.
    pub escape: Dir,
    /// Index into `design.nets`, if the pin is connected.
    pub net: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlacedPart {
    /// Index into `design.parts`.
    pub part: usize,
    pub glyph: Glyph,
    /// Where the symbol's origin lands on the sheet.
    pub at: Pt,
    pub orient: Orient,
    pub pins: Vec<PlacedPin>,
    pub reference: Text,
    pub value: Option<Text>,
    /// The body, pins and texts.
    pub extent: Rect,
}

/// A run of wire along one net.
#[derive(Debug, Clone, PartialEq)]
pub struct Wire {
    pub net: usize,
    pub points: Vec<Pt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Junction {
    pub net: usize,
    pub at: Pt,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NetLabel {
    pub net: usize,
    pub text: Text,
}

/// The nets of a design that are power nets, by name, as the checker's model has them: the
/// negative side of a rail (where the current returns) and the positive side. A net that is
/// in neither is an ordinary net, however it is named.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rails {
    pub positive: BTreeSet<String>,
    pub ground: BTreeSet<String>,
}

impl Rails {
    /// Whether nothing is known about rails: the drawing then has no power symbols.
    pub fn is_empty(&self) -> bool {
        self.positive.is_empty() && self.ground.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerKind {
    /// The negative side of a rail: the ground symbol, bars that shrink.
    Ground,
    /// The positive side: the supply symbol, a stem with an arrow head.
    Supply,
}

/// A power symbol: where a wire from a pin (or from several) ends, which way the symbol
/// points away from it, and the name beside it.
#[derive(Debug, Clone, PartialEq)]
pub struct PowerSymbol {
    pub net: usize,
    pub kind: PowerKind,
    pub at: Pt,
    pub dir: Dir,
    pub label: Option<Text>,
}

impl PowerSymbol {
    /// How far the drawn symbol reaches from `at`, along `dir`.
    pub const REACH: f64 = 2.0;

    /// The symbol without its name.
    pub fn glyph_extent(&self) -> Rect {
        let tip = self.at.step(self.dir, Self::REACH);
        let mut r = Rect::around(Pt::new(self.at.x - 1.6, self.at.y));
        r.include(Pt::new(self.at.x + 1.6, tip.y));
        r
    }

    /// The symbol and its name.
    pub fn extent(&self) -> Rect {
        let mut r = self.glyph_extent();
        if let Some(l) = &self.label {
            r.union(l.extent());
        }
        r
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// In the same order as `design.parts`.
    pub parts: Vec<PlacedPart>,
    pub symbols: Vec<PowerSymbol>,
    /// The boxes parts are grouped in, when the board was laid out by module
    /// ([`crate::sheets::layout_sheets`]); empty when the whole board is one drawing.
    pub sheets: Vec<crate::sheets::Sheet>,
    pub wires: Vec<Wire>,
    pub junctions: Vec<Junction>,
    pub labels: Vec<NetLabel>,
    pub bounds: Rect,
}

/// Whether the net has a name someone chose. An unnamed net is `net-` and a path.
pub fn is_named(net_name: &str) -> bool {
    !net_name.starts_with("net-")
}

/// A net name cut to [`MAX_LABEL_CHARS`] letters, with an ellipsis at the end.
pub fn ellipsize(name: &str) -> String {
    if name.chars().count() <= MAX_LABEL_CHARS {
        return name.to_string();
    }
    let head: String = name.chars().take(MAX_LABEL_CHARS - 1).collect();
    format!("{head}…")
}

pub(crate) fn is_ground_name(name: &str) -> bool {
    let upper = name.trim_start_matches('/').to_ascii_uppercase();
    upper.starts_with("GND")
        || upper.ends_with("GND")
        || matches!(upper.as_str(), "VSS" | "0V" | "GROUND")
}

/// Connectors are the way in and out of a board, so they start the left-to-right walk.
pub(crate) fn is_connector(refdes: &str) -> bool {
    let mut chars = refdes.chars();
    matches!(chars.next(), Some('J' | 'P')) && chars.next().is_some_and(|c| c.is_ascii_digit())
}

/// A pin of a net on a placed part, before the part is placed.
struct NetPin {
    part: usize,
    pad: String,
    /// Relative to the part's origin on the sheet.
    rel: Pt,
    escape: Dir,
}

enum Hook {
    /// A wire runs from the pin to the net's trunk.
    Drop,
    /// The pin leaves away from the trunk: its wire goes out, around the end of its column
    /// and to the trunk.
    Wrap,
    /// The pin gets a short stub and the net's name.
    Label,
}

enum Route {
    /// A power net: each of its pins, or each run of pins, has a wire to a power symbol.
    Power,
    /// Two pins joined by one wire.
    Direct,
    /// A trunk in the channel above or below the parts.
    Bus(Dir),
    Labels,
}

struct Plan {
    route: Route,
    hooks: Vec<Hook>,
}

/// Parts stacked in one column, top to bottom.
struct Unit {
    parts: Vec<usize>,
    /// For a stack of two-pin parts: the pad of each part that faces up.
    up_pads: Vec<String>,
}

struct Ctx<'a> {
    design: &'a Design,
    glyphs: Vec<Glyph>,
    pin_net: HashMap<(usize, String), usize>,
    /// Nets each part touches, in pin order, without repeats.
    part_nets: Vec<Vec<usize>>,
    ground: Vec<bool>,
    level: Vec<usize>,
    connector: Vec<bool>,
    /// Nets that leave this drawing: their pins get a stub and the net's name, never a trunk.
    external: Vec<bool>,
    /// What each net is, when the rails are known: ground, supply, or neither.
    power: Vec<Option<PowerKind>>,
    /// Whether ground symbols carry their net's name: when the design has more than one ground.
    label_grounds: bool,
}

/// Place the parts of `design` and route its nets.
pub fn layout(design: &Design, mapping: &Mapping, lib: &SymbolLib) -> Result<Layout, DesignError> {
    layout_rails(design, &Rails::default(), mapping, lib)
}

/// [`layout`] with the rails known: their pins get power symbols and no trunk.
pub fn layout_rails(
    design: &Design,
    rails: &Rails,
    mapping: &Mapping,
    lib: &SymbolLib,
) -> Result<Layout, DesignError> {
    layout_with(
        design,
        mapping,
        lib,
        &vec![false; design.nets.len()],
        rails,
        rails.ground.len() > 1,
    )
}

/// [`layout`] with `external[n]` set for each net of `design` that continues somewhere else
/// (another sheet): it is drawn as a stub and a name at each of its pins here.
pub(crate) fn layout_with(
    design: &Design,
    mapping: &Mapping,
    lib: &SymbolLib,
    external: &[bool],
    rails: &Rails,
    label_grounds: bool,
) -> Result<Layout, DesignError> {
    design.validate()?;
    let used = used_pads(design);
    let glyphs: Vec<Glyph> = design
        .parts
        .iter()
        .zip(&used)
        .map(|(p, u)| Glyph::for_part(p, u, Pinout::of(p).as_ref(), mapping, lib))
        .collect();

    let index: HashMap<&str, usize> = design
        .parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.refdes.as_str(), i))
        .collect();
    let mut pin_net = HashMap::new();
    let mut part_nets = vec![Vec::new(); design.parts.len()];
    for (n, net) in design.nets.iter().enumerate() {
        for pin in &net.pins {
            let p = index[pin.refdes.as_str()];
            pin_net.entry((p, pin.pin.clone())).or_insert(n);
            if !part_nets[p].contains(&n) {
                part_nets[p].push(n);
            }
        }
    }
    let power: Vec<Option<PowerKind>> = design
        .nets
        .iter()
        .map(|n| {
            if rails.ground.contains(&n.name) {
                Some(PowerKind::Ground)
            } else if rails.positive.contains(&n.name) {
                Some(PowerKind::Supply)
            } else {
                None
            }
        })
        .collect();
    let ground = if rails.is_empty() {
        find_ground(design)
    } else {
        power
            .iter()
            .map(|p| *p == Some(PowerKind::Ground))
            .collect()
    };
    let connector: Vec<bool> = design
        .parts
        .iter()
        .map(|p| is_connector(&p.refdes))
        .collect();
    let level = levels(&part_nets, &connector, &ground);
    let ctx = Ctx {
        design,
        glyphs,
        pin_net,
        part_nets,
        ground,
        level,
        connector,
        external: external.to_vec(),
        power,
        label_grounds,
    };

    let units = find_units(&ctx);
    let unit_of: HashMap<usize, usize> = units
        .iter()
        .enumerate()
        .flat_map(|(u, unit)| unit.parts.iter().map(move |&p| (p, u)))
        .collect();
    let order = column_order(&ctx, &units, &unit_of);
    let mut column_of = vec![0; design.parts.len()];
    for (col, &u) in order.iter().enumerate() {
        for &p in &units[u].parts {
            column_of[p] = col;
        }
    }

    let orient = orientations(&ctx, &units, &column_of);
    let net_pins = net_pins(&ctx, &orient);
    let plans = plan_nets(&ctx, &net_pins, &unit_of, &column_of);
    let parts = place(&ctx, &units, &order, &orient, &net_pins, &plans);
    Ok(route(&ctx, parts, &net_pins, &plans, &column_of))
}

// ---------------------------------------------------------------------------------------
// Which nets are ground, and how far each net is from it

fn find_ground(design: &Design) -> Vec<bool> {
    let mut ground: Vec<bool> = design
        .nets
        .iter()
        .map(|n| is_ground_name(&n.name))
        .collect();
    if !ground.contains(&true) {
        // No net says it is ground; the one net that joins the most pins is the best guess.
        let most = design.nets.iter().map(|n| n.pins.len()).max().unwrap_or(0);
        let biggest: Vec<usize> = (0..design.nets.len())
            .filter(|&i| design.nets[i].pins.len() == most)
            .collect();
        if most >= 3 && biggest.len() == 1 {
            ground[biggest[0]] = true;
        }
    }
    ground
}

/// Hops from ground, going through parts but not (at first) connectors, so a header that
/// carries two rails does not make them look equally far from ground.
fn levels(part_nets: &[Vec<usize>], connector: &[bool], ground: &[bool]) -> Vec<usize> {
    let mut level = vec![usize::MAX; ground.len()];
    let mut queue: VecDeque<usize> = (0..ground.len()).filter(|&n| ground[n]).collect();
    for &n in &queue {
        level[n] = 0;
    }
    for through_connectors in [false, true] {
        if through_connectors {
            let mut seeds: Vec<usize> = (0..level.len())
                .filter(|&n| level[n] != usize::MAX)
                .collect();
            seeds.sort_by_key(|&n| (level[n], n));
            queue = seeds.into();
        }
        while let Some(net) = queue.pop_front() {
            for (p, nets) in part_nets.iter().enumerate() {
                if (connector[p] && !through_connectors) || !nets.contains(&net) {
                    continue;
                }
                for &other in nets {
                    if level[other] == usize::MAX {
                        level[other] = level[net] + 1;
                        queue.push_back(other);
                    }
                }
            }
        }
    }
    level
        .into_iter()
        .map(|l| if l == usize::MAX { 0 } else { l })
        .collect()
}

// ---------------------------------------------------------------------------------------
// Columns

/// A part with two pins on one axis, facing opposite ways, can stand in a stack.
fn is_stackable(ctx: &Ctx, p: usize) -> bool {
    let g = &ctx.glyphs[p];
    if g.symbol.is_none() || g.pins.len() != 2 || ctx.connector[p] {
        return false;
    }
    let (a, b) = (&g.pins[0], &g.pins[1]);
    let opposite = a.toward_body.0 == -b.toward_body.0 && a.toward_body.1 == -b.toward_body.1;
    let along_x = a.toward_body.1 == 0.0 && a.at.1 == b.at.1;
    let along_y = a.toward_body.0 == 0.0 && a.at.0 == b.at.0;
    opposite && (along_x || along_y)
}

fn net_of(ctx: &Ctx, part: usize, pad: &str) -> Option<usize> {
    ctx.pin_net.get(&(part, pad.to_string())).copied()
}

fn other_pad<'a>(ctx: &'a Ctx, part: usize, pad: &str) -> &'a str {
    let pins = &ctx.glyphs[part].pins;
    if pins[0].id == pad {
        &pins[1].id
    } else {
        &pins[0].id
    }
}

fn find_units(ctx: &Ctx) -> Vec<Unit> {
    let n = ctx.design.parts.len();
    // link[(part, pad)] = (other part, other pad) for a net of exactly these two pins.
    let mut link: HashMap<(usize, String), (usize, String)> = HashMap::new();
    for net in &ctx.design.nets {
        if net.pins.len() != 2 {
            continue;
        }
        let find = |r: &str| ctx.design.parts.iter().position(|p| p.refdes == r).unwrap();
        let (a, b) = (find(&net.pins[0].refdes), find(&net.pins[1].refdes));
        if a != b && is_stackable(ctx, a) && is_stackable(ctx, b) {
            link.insert((a, net.pins[0].pin.clone()), (b, net.pins[1].pin.clone()));
            link.insert((b, net.pins[1].pin.clone()), (a, net.pins[0].pin.clone()));
        }
    }
    let linked = |p: usize| {
        ctx.glyphs[p]
            .pins
            .iter()
            .filter(|pin| link.contains_key(&(p, pin.id.clone())))
            .count()
    };

    let mut seen = vec![false; n];
    let mut units = Vec::new();
    // Ends of runs first, then whatever is left (a ring of parts).
    let starts: Vec<usize> = (0..n)
        .filter(|&p| is_stackable(ctx, p) && linked(p) <= 1)
        .chain((0..n).filter(|&p| is_stackable(ctx, p)))
        .collect();
    for start in starts {
        if seen[start] {
            continue;
        }
        // The first part faces up with the pin that is not linked on.
        let first_up = ctx.glyphs[start]
            .pins
            .iter()
            .find(|pin| !link.contains_key(&(start, pin.id.clone())))
            .unwrap_or(&ctx.glyphs[start].pins[0])
            .id
            .clone();
        let mut parts = vec![start];
        let mut up_pads = vec![first_up];
        seen[start] = true;
        loop {
            let last = *parts.last().unwrap();
            let down = other_pad(ctx, last, up_pads.last().unwrap()).to_string();
            match link.get(&(last, down)) {
                Some((next, next_pad)) if !seen[*next] => {
                    seen[*next] = true;
                    parts.push(*next);
                    up_pads.push(next_pad.clone());
                }
                _ => break,
            }
        }
        // Higher nets go on top.
        let top = net_of(ctx, parts[0], &up_pads[0]).map_or(0, |n| ctx.level[n]);
        let last = parts.len() - 1;
        let bottom_pad = other_pad(ctx, parts[last], &up_pads[last]);
        let bottom = net_of(ctx, parts[last], bottom_pad).map_or(0, |n| ctx.level[n]);
        if bottom > top {
            parts.reverse();
            up_pads = parts
                .iter()
                .zip(up_pads.iter().rev())
                .map(|(&p, up)| other_pad(ctx, p, up).to_string())
                .collect();
        }
        units.push(Unit { parts, up_pads });
    }
    for (p, _) in seen.iter().enumerate().filter(|(_, s)| !**s) {
        units.push(Unit {
            parts: vec![p],
            up_pads: Vec::new(),
        });
    }
    units.sort_by_key(|u| u.parts.iter().copied().min().unwrap_or(0));
    units
}

/// Unit indices left to right.
fn column_order(ctx: &Ctx, units: &[Unit], unit_of: &HashMap<usize, usize>) -> Vec<usize> {
    let mut adjacent: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); units.len()];
    for (n, net) in ctx.design.nets.iter().enumerate() {
        if ctx.ground[n] || ctx.power[n].is_some() {
            continue;
        }
        let touching: BTreeSet<usize> = net
            .pins
            .iter()
            .map(|pin| unit_of[&part_index(ctx, &pin.refdes)])
            .collect();
        for &a in &touching {
            adjacent[a].extend(touching.iter().filter(|&&b| b != a));
        }
    }

    let mut order = Vec::new();
    let mut seen = vec![false; units.len()];
    for first in 0..units.len() {
        if seen[first] {
            continue;
        }
        // The component this unit belongs to.
        let mut component = vec![first];
        seen[first] = true;
        let mut i = 0;
        while i < component.len() {
            for &next in &adjacent[component[i]] {
                if !seen[next] {
                    seen[next] = true;
                    component.push(next);
                }
            }
            i += 1;
        }
        component.sort_unstable();
        let is_connector_unit = |u: usize| units[u].parts.iter().any(|&p| ctx.connector[p]);
        let root = component
            .iter()
            .copied()
            .find(|&u| is_connector_unit(u))
            .unwrap_or(component[0]);

        let mut walk = vec![root];
        let mut visited: BTreeSet<usize> = BTreeSet::from([root]);
        let mut i = 0;
        while i < walk.len() {
            // Connectors are the way out, so they come last among the units met together.
            let mut next: Vec<usize> = adjacent[walk[i]].iter().copied().collect();
            next.sort_by_key(|&u| (is_connector_unit(u), u));
            for u in next {
                if visited.insert(u) {
                    walk.push(u);
                }
            }
            i += 1;
        }
        if signal_runs_backwards(ctx, &walk, units, unit_of) {
            walk.reverse();
        }
        order.extend(walk);
    }
    order
}

fn part_index(ctx: &Ctx, refdes: &str) -> usize {
    ctx.design
        .parts
        .iter()
        .position(|p| p.refdes == refdes)
        .expect("validate() guarantees the part exists")
}

/// Parts whose symbols say which pins take a signal and which give one vote on whether
/// the walk found the units in the order the signal travels.
fn signal_runs_backwards(
    ctx: &Ctx,
    walk: &[usize],
    units: &[Unit],
    unit_of: &HashMap<usize, usize>,
) -> bool {
    let position = |u: usize| walk.iter().position(|&w| w == u);
    let mean_position = |part: usize, flow: fn(Flow) -> bool| -> Option<f64> {
        let mut seen = Vec::new();
        for pin in ctx.glyphs[part].pins.iter().filter(|p| flow(p.flow)) {
            let Some(n) = net_of(ctx, part, &pin.id) else {
                continue;
            };
            if ctx.ground[n] {
                continue;
            }
            for other in &ctx.design.nets[n].pins {
                let q = part_index(ctx, &other.refdes);
                if q != part {
                    seen.extend(position(unit_of[&q]));
                }
            }
        }
        (!seen.is_empty()).then(|| seen.iter().sum::<usize>() as f64 / seen.len() as f64)
    };
    let (mut backwards, mut forwards) = (0, 0);
    for &u in walk {
        for &p in &units[u].parts {
            if let (Some(from), Some(to)) =
                (mean_position(p, Flow::takes), mean_position(p, Flow::gives))
            {
                if from > to {
                    backwards += 1;
                } else if from < to {
                    forwards += 1;
                }
            }
        }
    }
    backwards > forwards
}

// ---------------------------------------------------------------------------------------
// Orientation

fn sign(v: f64) -> f64 {
    if v > 1e-9 {
        1.0
    } else if v < -1e-9 {
        -1.0
    } else {
        0.0
    }
}

/// The orientation of every part, indexed like `design.parts`.
fn orientations(ctx: &Ctx, units: &[Unit], column_of: &[usize]) -> Vec<Orient> {
    let mut orient = vec![Orient::IDENTITY; ctx.design.parts.len()];
    for unit in units {
        for (i, &p) in unit.parts.iter().enumerate() {
            orient[p] = if unit.up_pads.is_empty() {
                best_orientation(ctx, p, column_of)
            } else {
                facing_up(&ctx.glyphs[p], &unit.up_pads[i])
            };
        }
    }
    orient
}

/// The first orientation that points `pad` upward.
fn facing_up(glyph: &Glyph, pad: &str) -> Orient {
    let pin = glyph.pin(pad).expect("the pad belongs to the part");
    let away = (-pin.toward_body.0, -pin.toward_body.1);
    Orient::ALL
        .into_iter()
        .find(|o| o.dir(away) == Dir::Up)
        .expect("some orientation points any axis-aligned pin up")
}

/// Score each orientation by how well it points high-net pins up, low-net pins down, and
/// pins toward the columns of the parts they connect to. Ties keep the library's drawing.
fn best_orientation(ctx: &Ctx, p: usize, column_of: &[usize]) -> Orient {
    let glyph = &ctx.glyphs[p];
    let nets: Vec<usize> = ctx.part_nets[p].clone();
    if nets.is_empty() {
        return Orient::IDENTITY;
    }
    let mean_level = nets.iter().map(|&n| ctx.level[n] as f64).sum::<f64>() / nets.len() as f64;
    let connected: Vec<_> = glyph
        .pins
        .iter()
        .filter_map(|pin| net_of(ctx, p, &pin.id).map(|n| (pin, n)))
        .collect();
    let mean_y = connected.iter().map(|(pin, _)| pin.at.1).sum::<f64>() / connected.len() as f64;

    // Turns that write two names over each other are out, unless every turn does.
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
        for (pin, n) in &connected {
            let escape = orient.dir((-pin.toward_body.0, -pin.toward_body.1));
            let want_up = sign(ctx.level[*n] as f64 - mean_level);
            let escapes_up = match escape {
                Dir::Up => 1.0,
                Dir::Down => -1.0,
                _ => 0.0,
            };
            // Where the pin sits relative to the part's other pins, once turned.
            let above = sign(-orient.apply(pin.at).y + orient.apply((0.0, mean_y)).y);
            score += 4.0 * want_up * (2.0 * escapes_up + above);

            // A ground trunk runs the whole width, so which way ground lies tells nothing.
            let others: Vec<f64> = ctx.design.nets[*n]
                .pins
                .iter()
                .filter(|_| !ctx.ground[*n])
                .map(|q| part_index(ctx, &q.refdes))
                .filter(|&q| column_of[q] != column_of[p])
                .map(|q| column_of[q] as f64)
                .collect();
            if !others.is_empty() {
                let toward =
                    sign(others.iter().sum::<f64>() / others.len() as f64 - column_of[p] as f64);
                score += toward * escape.vector().0;
            }
        }
        if score > best.0 + 1e-9 {
            best = (score, orient);
        }
    }
    best.1
}

// ---------------------------------------------------------------------------------------
// Net plans

fn net_pins(ctx: &Ctx, orient: &[Orient]) -> Vec<Vec<NetPin>> {
    ctx.design
        .nets
        .iter()
        .map(|net| {
            net.pins
                .iter()
                .map(|pin| {
                    let part = part_index(ctx, &pin.refdes);
                    let g = ctx.glyphs[part]
                        .pin(&pin.pin)
                        .expect("a glyph has every pad its nets use");
                    NetPin {
                        part,
                        pad: pin.pin.clone(),
                        rel: orient[part].apply(g.at),
                        escape: orient[part].dir((-g.toward_body.0, -g.toward_body.1)),
                    }
                })
                .collect()
        })
        .collect()
}

fn plan_nets(
    ctx: &Ctx,
    net_pins: &[Vec<NetPin>],
    unit_of: &HashMap<usize, usize>,
    column_of: &[usize],
) -> Vec<Plan> {
    // How far each side pin sits from the middle of its part's pins, so a side pin can
    // say whether it belongs toward the top or the bottom.
    let centre_y = |p: usize| -> f64 {
        let ys: Vec<f64> = net_pins
            .iter()
            .flatten()
            .filter(|q| q.part == p)
            .map(|q| q.rel.y)
            .collect();
        ys.iter().sum::<f64>() / ys.len().max(1) as f64
    };

    net_pins
        .iter()
        .enumerate()
        .map(|(n, pins)| {
            let labels = |pins: &[NetPin]| Plan {
                route: Route::Labels,
                hooks: pins.iter().map(|_| Hook::Label).collect(),
            };
            if ctx.power[n].is_some() {
                return Plan {
                    route: Route::Power,
                    hooks: Vec::new(),
                };
            }
            if pins.len() == 2 {
                let (a, b) = (&pins[0], &pins[1]);
                let stacked = a.part != b.part
                    && unit_of[&a.part] == unit_of[&b.part]
                    && a.escape.is_vertical()
                    && a.escape == b.escape.opposite();
                let (left, right) = if a.escape == Dir::Right {
                    (a, b)
                } else {
                    (b, a)
                };
                let facing = left.escape == Dir::Right
                    && right.escape == Dir::Left
                    && (1..=MAX_Z_COLUMNS)
                        .contains(&column_of[right.part].wrapping_sub(column_of[left.part]));
                if stacked || facing {
                    return Plan {
                        route: Route::Direct,
                        hooks: vec![Hook::Drop, Hook::Drop],
                    };
                }
            }
            if pins.len() < 2 || pins.len() > MAX_BUS_PINS {
                return labels(pins);
            }

            let mut up = 0;
            let mut down = 0;
            for q in pins {
                let wants = match q.escape {
                    Dir::Up => Dir::Up,
                    Dir::Down => Dir::Down,
                    _ if q.rel.y < centre_y(q.part) - 1e-9 => Dir::Up,
                    _ if q.rel.y > centre_y(q.part) + 1e-9 => Dir::Down,
                    _ => continue,
                };
                if wants == Dir::Up {
                    up += 1;
                } else {
                    down += 1;
                }
            }
            let side = if up > down {
                Dir::Up
            } else if down > up || ctx.ground[n] {
                Dir::Down
            } else {
                Dir::Up
            };
            let hooks: Vec<Hook> = pins
                .iter()
                .map(|q| {
                    if q.escape.is_vertical() && q.escape != side {
                        Hook::Wrap
                    } else {
                        Hook::Drop
                    }
                })
                .collect();
            if hooks.iter().filter(|h| !matches!(h, Hook::Label)).count() < 2 {
                return labels(pins);
            }
            Plan {
                route: Route::Bus(side),
                hooks,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------
// Placement

/// Turn a part's glyph into a placed part at the origin and work out its texts.
fn place_part(ctx: &Ctx, p: usize, orient: Orient, nets: &[Option<usize>]) -> PlacedPart {
    let glyph = ctx.glyphs[p].clone();
    let part = &ctx.design.parts[p];
    let b = glyph.bounds();
    let mut body = Rect::around(orient.apply((b.min.x, b.min.y)));
    body.include(orient.apply((b.max.x, b.max.y)));
    body.include(orient.apply((b.min.x, b.max.y)));
    body.include(orient.apply((b.max.x, b.min.y)));

    let pins: Vec<PlacedPin> = glyph
        .pins
        .iter()
        .zip(nets)
        .map(|(pin, &net)| PlacedPin {
            id: pin.id.clone(),
            at: orient.apply(pin.at),
            escape: orient.dir((-pin.toward_body.0, -pin.toward_body.1)),
            net,
        })
        .collect();

    // Beside a tall part, above or below a wide one, on a side no wire leaves by.
    let free = |d: Dir| !pins.iter().any(|q| q.escape == d);
    let candidates = if body.height() > body.width() {
        [Dir::Right, Dir::Left, Dir::Up, Dir::Down]
    } else {
        [Dir::Up, Dir::Down, Dir::Right, Dir::Left]
    };
    let side = candidates
        .into_iter()
        .find(|&d| free(d))
        .unwrap_or(Dir::Right);
    let (cx, cy) = (
        (body.min.x + body.max.x) / 2.0,
        (body.min.y + body.max.y) / 2.0,
    );
    let line = TEXT_SIZE * 1.4;
    let (reference, value) = match side {
        Dir::Right | Dir::Left => {
            let (x, anchor) = if side == Dir::Right {
                (body.max.x + GRID, Anchor::Start)
            } else {
                (body.min.x - GRID, Anchor::End)
            };
            (
                text(&part.refdes, Pt::new(x, cy - 0.3), anchor),
                text(&part.value, Pt::new(x, cy - 0.3 + line), anchor),
            )
        }
        Dir::Up => (
            text(
                &part.refdes,
                Pt::new(cx, body.min.y - GRID - line),
                Anchor::Middle,
            ),
            text(&part.value, Pt::new(cx, body.min.y - GRID), Anchor::Middle),
        ),
        Dir::Down => (
            text(
                &part.refdes,
                Pt::new(cx, body.max.y + GRID + TEXT_SIZE),
                Anchor::Middle,
            ),
            text(
                &part.value,
                Pt::new(cx, body.max.y + GRID + TEXT_SIZE + line),
                Anchor::Middle,
            ),
        ),
    };
    let value = (!part.value.is_empty()).then_some(value);

    let mut extent = body;
    extent.union(reference.extent());
    if let Some(v) = &value {
        extent.union(v.extent());
    }
    PlacedPart {
        part: p,
        glyph,
        at: Pt::new(0.0, 0.0),
        orient,
        pins,
        reference,
        value,
        extent,
    }
}

fn text(text: &str, at: Pt, anchor: Anchor) -> Text {
    Text::new(text, at, anchor)
}

/// How much sideways room a part needs outside its pins for wires that leave sideways.
fn side_reserve(
    ctx: &Ctx,
    part: usize,
    side: Dir,
    net_pins: &[Vec<NetPin>],
    plans: &[Plan],
) -> f64 {
    let mut drops = 0;
    let mut label_width: f64 = 0.0;
    for (n, pins) in net_pins.iter().enumerate() {
        for (q, hook) in pins.iter().zip(&plans[n].hooks) {
            if q.part != part || q.escape != side {
                continue;
            }
            match (hook, &plans[n].route) {
                (Hook::Drop, Route::Bus(_)) => drops += 1,
                (Hook::Wrap, _) => {}
                (Hook::Label, _) | (_, Route::Labels) => {
                    label_width =
                        label_width.max(LABEL_STUB + GRID + text_width(&ctx.design.nets[n].name));
                }
                _ => {}
            }
        }
    }
    let for_drops = if drops > 0 {
        STUB + drops as f64 * PITCH
    } else {
        0.0
    };
    for_drops.max(label_width)
}

fn place(
    ctx: &Ctx,
    units: &[Unit],
    order: &[usize],
    orient: &[Orient],
    net_pins: &[Vec<NetPin>],
    plans: &[Plan],
) -> Vec<PlacedPart> {
    let n = ctx.design.parts.len();
    let mut pin_nets: Vec<Vec<Option<usize>>> = Vec::with_capacity(n);
    for p in 0..n {
        pin_nets.push(
            ctx.glyphs[p]
                .pins
                .iter()
                .map(|pin| net_of(ctx, p, &pin.id))
                .collect(),
        );
    }
    let mut parts: Vec<PlacedPart> = (0..n)
        .map(|p| place_part(ctx, p, orient[p], &pin_nets[p]))
        .collect();
    let mut at = vec![Pt::new(0.0, 0.0); n];
    // What the power pins of each part draw, so a column leaves room for it.
    let reach: Vec<Option<Rect>> = parts
        .iter()
        .map(|p| power_art(ctx, &p.pins).extent())
        .collect();

    let mut cursor = 0.0;
    for &u in order {
        let unit = &units[u];
        // Stack the parts so each pair is joined by a short wire.
        let mut offsets = vec![Pt::new(0.0, 0.0)];
        for w in 1..unit.parts.len() {
            let (prev, next) = (unit.parts[w - 1], unit.parts[w]);
            let down = pin_on(&parts[prev], &unit.up_pads[w - 1], true);
            let up = pin_on(&parts[next], &unit.up_pads[w], false);
            let before = offsets[w - 1];
            offsets.push(Pt::new(
                before.x + down.x - up.x,
                before.y + down.y + CHAIN_WIRE - up.y,
            ));
        }
        let mut block = parts[unit.parts[0]].extent;
        for (w, &p) in unit.parts.iter().enumerate() {
            let mut e = parts[p].extent.translated(offsets[w]);
            if let Some(r) = reach[p] {
                e.union(r.translated(offsets[w]));
            }
            if w == 0 {
                block = e;
            } else {
                block.union(e);
            }
        }
        let reserve = |side: Dir| {
            unit.parts
                .iter()
                .map(|&p| side_reserve(ctx, p, side, net_pins, plans))
                .fold(0.0, f64::max)
        };
        let left = -block.min.x + reserve(Dir::Left);
        let x = snap_up(cursor + left, GRID);
        let y = snap(-(block.min.y + block.max.y) / 2.0, GRID);
        for (w, &p) in unit.parts.iter().enumerate() {
            at[p] = Pt::new(x + offsets[w].x, y + offsets[w].y);
        }
        let wraps = net_pins
            .iter()
            .zip(plans)
            .flat_map(|(pins, plan)| pins.iter().zip(&plan.hooks).map(move |(q, h)| (q, h, plan)))
            .filter(|(q, h, plan)| {
                unit.parts.contains(&q.part)
                    && matches!(h, Hook::Wrap)
                    && matches!(plan.route, Route::Bus(_))
            })
            .count();
        let wrap_room = if wraps > 0 {
            STUB + wraps as f64 * PITCH
        } else {
            0.0
        };
        cursor = x + block.max.x + reserve(Dir::Right) + wrap_room + COLUMN_GAP;
    }

    for p in 0..n {
        let by = at[p];
        parts[p].at = by;
        parts[p].extent = parts[p].extent.translated(by);
        for pin in &mut parts[p].pins {
            pin.at = Pt::new(pin.at.x + by.x, pin.at.y + by.y);
        }
        let part = &mut parts[p];
        for t in std::iter::once(&mut part.reference).chain(part.value.as_mut()) {
            t.at = Pt::new(t.at.x + by.x, t.at.y + by.y);
        }
    }
    parts
}

/// The pin of a placed part, relative to its origin, that faces down (`down`) or up.
fn pin_on(part: &PlacedPart, up_pad: &str, down: bool) -> Pt {
    part.pins
        .iter()
        .find(|q| (q.id == up_pad) != down)
        .expect("a stacked part has two pins")
        .at
}

// ---------------------------------------------------------------------------------------
// Power symbols

/// The wires and symbols that give the power pins of one part their rails.
#[derive(Default)]
struct PowerArt {
    wires: Vec<Wire>,
    symbols: Vec<PowerSymbol>,
}

impl PowerArt {
    /// Everything it draws, or `None` when it draws nothing.
    fn extent(&self) -> Option<Rect> {
        let mut out: Option<Rect> = None;
        let mut add = |r: Rect| match &mut out {
            Some(o) => o.union(r),
            None => out = Some(r),
        };
        for w in &self.wires {
            for &pt in &w.points {
                add(Rect::around(pt));
            }
        }
        for s in &self.symbols {
            add(s.extent());
        }
        out
    }
}

fn same_point(a: Pt, b: Pt) -> bool {
    (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
}

/// Where the power pins of a part (`pins`, in any frame) reach their rails. Pins on one
/// side that are next to each other and on one power net share a bus and one symbol. A
/// pin on the top or bottom of a part gets its symbol straight out; a pin on a side gets a
/// wire out and then up (a supply) or down (a ground), in a lane of its own.
fn power_art(ctx: &Ctx, pins: &[PlacedPin]) -> PowerArt {
    let mut art = PowerArt::default();
    // The distinct points: stacked pads of one pin are at one point.
    let mut points: Vec<(Pt, Dir, Option<usize>)> = Vec::new();
    for pin in pins {
        match points.iter_mut().find(|p| same_point(p.0, pin.at)) {
            Some(known) => {
                if known.2.is_none() {
                    known.2 = pin.net;
                }
            }
            None => points.push((pin.at, pin.escape, pin.net)),
        }
    }
    for dir in [Dir::Up, Dir::Down, Dir::Left, Dir::Right] {
        let along = |p: Pt| if dir.is_vertical() { p.x } else { p.y };
        let mut side: Vec<(Pt, Option<usize>)> = points
            .iter()
            .filter(|p| p.1 == dir)
            .map(|p| (p.0, p.2))
            .collect();
        side.sort_by(|a, b| along(a.0).total_cmp(&along(b.0)));
        let mut runs: Vec<(usize, Vec<Pt>)> = Vec::new();
        for (pt, net) in side {
            let power = net.filter(|&n| ctx.power[n].is_some());
            let joins = match (runs.last(), power) {
                (Some((n, pts)), Some(m)) => {
                    *n == m
                        && pts.last().is_some_and(|&last| {
                            along(pt) - along(last) <= MAX_RUN_GAP + 1e-6
                                && runs_adjacent(&points, dir, last, pt)
                        })
                }
                _ => false,
            };
            match (joins, power) {
                (true, _) => runs.last_mut().expect("a run to join").1.push(pt),
                (false, Some(m)) => runs.push((m, vec![pt])),
                (false, None) => {
                    // A pin that is not on a power net ends the run before it.
                    runs.push((usize::MAX, vec![pt]));
                }
            }
        }
        runs.retain(|(n, _)| *n != usize::MAX);
        if dir.is_vertical() {
            for (net, pts) in &runs {
                emit_run(ctx, &mut art, *net, dir, pts, 0);
            }
        } else {
            // Lanes: supplies from the top down, then grounds from the bottom up, so no
            // symbol's wire runs through another's.
            let kind_of = |n: usize| ctx.power[n];
            let mut order: Vec<usize> = (0..runs.len()).collect();
            order.sort_by(|&a, &b| {
                let (ka, kb) = (kind_of(runs[a].0), kind_of(runs[b].0));
                let rank = |k: Option<PowerKind>| u8::from(k != Some(PowerKind::Supply));
                let (ya, yb) = (runs[a].1[0].y, runs[b].1[0].y);
                rank(ka).cmp(&rank(kb)).then_with(|| {
                    if ka == Some(PowerKind::Supply) {
                        ya.total_cmp(&yb)
                    } else {
                        yb.total_cmp(&ya)
                    }
                })
            });
            for (slot, &i) in order.iter().enumerate() {
                emit_run(ctx, &mut art, runs[i].0, dir, &runs[i].1, slot);
            }
        }
    }
    art
}

/// Whether no other point of the part's side lies between `a` and `b`.
fn runs_adjacent(points: &[(Pt, Dir, Option<usize>)], dir: Dir, a: Pt, b: Pt) -> bool {
    let along = |p: Pt| if dir.is_vertical() { p.x } else { p.y };
    !points
        .iter()
        .any(|p| p.1 == dir && along(p.0) > along(a) + 1e-6 && along(p.0) < along(b) - 1e-6)
}

/// One run of pins on one side of a part: its wires and its symbol. `lane` counts the
/// runs on a side that leave sideways.
fn emit_run(ctx: &Ctx, art: &mut PowerArt, net: usize, escape: Dir, pts: &[Pt], lane: usize) {
    let kind = ctx.power[net].expect("a run is on a power net");
    let wire = |points: Vec<Pt>| Wire { net, points };
    let (at, dir);
    if escape.is_vertical() {
        dir = escape;
        if let [only] = pts {
            at = only.step(escape, STUB + STEM);
            art.wires.push(wire(vec![*only, at]));
        } else {
            let (lo, hi) = (pts[0].x, pts[pts.len() - 1].x);
            let mid = (lo + hi) / 2.0;
            let hub = pts
                .iter()
                .map(|p| p.x)
                .min_by(|a, b| (a - mid).abs().total_cmp(&(b - mid).abs()))
                .expect("a run has pins");
            let y = pts[0].step(escape, STUB).y;
            for p in pts {
                art.wires.push(wire(vec![*p, Pt::new(p.x, y)]));
            }
            art.wires.push(wire(vec![Pt::new(lo, y), Pt::new(hi, y)]));
            at = Pt::new(hub, y).step(escape, STEM);
            art.wires.push(wire(vec![Pt::new(hub, y), at]));
        }
    } else {
        dir = if kind == PowerKind::Supply {
            Dir::Up
        } else {
            Dir::Down
        };
        let x = pts[0].x + escape.vector().0 * (STUB + lane as f64 * PITCH);
        if let [only] = pts {
            at = Pt::new(x, only.y).step(dir, STEM);
            art.wires.push(wire(vec![*only, Pt::new(x, only.y), at]));
        } else {
            let (top, bottom) = (pts[0].y, pts[pts.len() - 1].y);
            for p in pts {
                art.wires.push(wire(vec![*p, Pt::new(x, p.y)]));
            }
            let (far, near) = if dir == Dir::Up {
                (bottom, top)
            } else {
                (top, bottom)
            };
            at = Pt::new(x, near).step(dir, STEM);
            art.wires
                .push(wire(vec![Pt::new(x, far), Pt::new(x, near), at]));
        }
    }
    let name = &ctx.design.nets[net].name;
    let named = kind == PowerKind::Supply || ctx.label_grounds;
    let label = named.then(|| label_beside(at, dir, &ellipsize(name), Side::Right));
    art.symbols.push(PowerSymbol {
        net,
        kind,
        at,
        dir,
        label,
    });
}

#[derive(Clone, Copy)]
enum Side {
    Right,
    Left,
    Beyond,
}

/// The name of a power symbol at `at` pointing `dir`: beside the symbol, or past its end.
fn label_beside(at: Pt, dir: Dir, name: &str, side: Side) -> Text {
    let dy = dir.vector().1;
    match side {
        Side::Right => Text::new(
            name,
            Pt::new(at.x + 2.2, at.y + dy * 1.4 + 0.45),
            Anchor::Start,
        ),
        Side::Left => Text::new(
            name,
            Pt::new(at.x - 2.2, at.y + dy * 1.4 + 0.45),
            Anchor::End,
        ),
        Side::Beyond => {
            let y = if dir == Dir::Up {
                at.y - PowerSymbol::REACH - 0.8
            } else {
                at.y + PowerSymbol::REACH + 0.8 + TEXT_SIZE
            };
            Text::new(name, Pt::new(at.x, y), Anchor::Middle)
        }
    }
}

/// Moves each symbol's name to the first of the places beside or past it that does not
/// cover a part, a wire, a name or another symbol.
fn settle_symbol_names(
    symbols: &mut [PowerSymbol],
    parts: &[PlacedPart],
    wires: &[Wire],
    labels: &[NetLabel],
) {
    let overlaps = |a: &Rect, b: &Rect| {
        a.min.x < b.max.x - 0.05
            && b.min.x < a.max.x - 0.05
            && a.min.y < b.max.y - 0.05
            && b.min.y < a.max.y - 0.05
    };
    let mut fixed: Vec<Rect> = parts.iter().map(|p| p.extent).collect();
    fixed.extend(labels.iter().map(|l| l.text.extent()));
    fixed.extend(symbols.iter().map(PowerSymbol::glyph_extent));
    for i in 0..symbols.len() {
        let Some(label) = symbols[i].label.clone() else {
            continue;
        };
        let own = symbols[i].at;
        let hits = |text: &Text, fixed: &[Rect]| {
            let r = text.extent();
            fixed.iter().any(|f| overlaps(&r, f))
                || wires.iter().any(|w| {
                    !w.points.iter().any(|&p| same_point(p, own))
                        && w.points.windows(2).any(|s| {
                            let mut seg = Rect::around(s[0]);
                            seg.include(s[1]);
                            seg.min = Pt::new(seg.min.x - 0.2, seg.min.y - 0.2);
                            seg.max = Pt::new(seg.max.x + 0.2, seg.max.y + 0.2);
                            overlaps(&r, &seg)
                        })
                })
        };
        let (at, dir) = (symbols[i].at, symbols[i].dir);
        let chosen = [Side::Right, Side::Left, Side::Beyond]
            .into_iter()
            .map(|side| label_beside(at, dir, &label.text, side))
            .find(|t| !hits(t, &fixed))
            .unwrap_or(label);
        fixed.push(chosen.extent());
        symbols[i].label = Some(chosen);
    }
}

// ---------------------------------------------------------------------------------------
// Routing

/// A trunk waiting for a track in its channel.
struct Trunk {
    net: usize,
    side: Dir,
    left: f64,
    right: f64,
}

fn route(
    ctx: &Ctx,
    parts: Vec<PlacedPart>,
    net_pins: &[Vec<NetPin>],
    plans: &[Plan],
    column_of: &[usize],
) -> Layout {
    let nets = &ctx.design.nets;
    let abs = |q: &NetPin| -> Pt {
        let part = &parts[q.part];
        part.pins
            .iter()
            .find(|pin| pin.id == q.pad)
            .expect("the pin was placed")
            .at
    };

    let mut power = PowerArt::default();
    let mut power_extents: Vec<Option<Rect>> = Vec::new();
    for part in &parts {
        let art = power_art(ctx, &part.pins);
        power_extents.push(art.extent());
        power.wires.extend(art.wires);
        power.symbols.extend(art.symbols);
    }
    let power_reach = power.extent();
    let mut top = parts
        .iter()
        .map(|p| p.extent.min.y)
        .chain(power_reach.map(|r| r.min.y))
        .fold(f64::INFINITY, f64::min);
    let mut bottom = parts
        .iter()
        .map(|p| p.extent.max.y)
        .chain(power_reach.map(|r| r.max.y))
        .fold(f64::NEG_INFINITY, f64::max);
    if parts.is_empty() {
        top = 0.0;
        bottom = 0.0;
    }

    // Where each drop leaves a side-facing pin: farther out for each pin on the same side
    // of the same part.
    let mut drop_x: HashMap<(usize, usize, String), f64> = HashMap::new();
    let mut side_pins: HashMap<(usize, Dir), Vec<(usize, usize)>> = HashMap::new();
    for (n, pins) in net_pins.iter().enumerate() {
        if !matches!(plans[n].route, Route::Bus(_)) {
            continue;
        }
        for (i, q) in pins.iter().enumerate() {
            if matches!(plans[n].hooks[i], Hook::Drop) && !q.escape.is_vertical() {
                side_pins
                    .entry((q.part, q.escape))
                    .or_default()
                    .push((n, i));
            }
        }
    }
    let mut keys: Vec<_> = side_pins.keys().copied().collect();
    keys.sort_by_key(|&(p, d)| (p, d as u8));
    for key in keys {
        let list = side_pins.remove(&key).unwrap();
        // Drops that go up leave top pin first and drops that go down leave bottom pin
        // first, so no drop crosses another's stub. An up and a down drop may share an x
        // unless the down drop starts above the up one and would run through it.
        let goes_up = |n: usize| matches!(plans[n].route, Route::Bus(Dir::Up));
        let y_of = |&(n, i): &(usize, usize)| abs(&net_pins[n][i]).y;
        let (mut ups, mut downs): (Vec<_>, Vec<_>) =
            list.into_iter().partition(|&(n, _)| goes_up(n));
        ups.sort_by(|a, b| y_of(a).total_cmp(&y_of(b)));
        downs.sort_by(|a, b| y_of(b).total_cmp(&y_of(a)));
        let clash = downs.iter().any(|d| ups.iter().any(|u| y_of(d) < y_of(u)));
        let shift = if clash { ups.len() } else { 0 };
        let ranked = ups
            .iter()
            .enumerate()
            .chain(downs.iter().enumerate().map(|(k, d)| (k + shift, d)));
        for (k, &(n, i)) in ranked {
            let q = &net_pins[n][i];
            let from = abs(q).step(q.escape, STUB + k as f64 * PITCH);
            drop_x.insert((n, q.part, q.pad.clone()), from.x);
        }
    }

    // Where a wrapped wire runs: past the right edge of its column, a lane for each.
    let edge_of = |column: usize| -> f64 {
        (0..parts.len())
            .filter(|&p| column_of[p] == column)
            .map(|p| {
                let right = power_extents[p].map_or(parts[p].extent.max.x, |r| {
                    r.max.x.max(parts[p].extent.max.x)
                });
                right + side_reserve(ctx, p, Dir::Right, net_pins, plans)
            })
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let mut lanes: HashMap<usize, usize> = HashMap::new();
    for (n, pins) in net_pins.iter().enumerate() {
        if !matches!(plans[n].route, Route::Bus(_)) {
            continue;
        }
        for (q, hook) in pins.iter().zip(&plans[n].hooks) {
            if matches!(hook, Hook::Wrap) {
                let lane = lanes.entry(column_of[q.part]).or_default();
                let x = edge_of(column_of[q.part]) + STUB + *lane as f64 * PITCH;
                *lane += 1;
                drop_x.insert((n, q.part, q.pad.clone()), x);
            }
        }
    }

    // The x a drop runs along, and where its pin is.
    let drop_at = |n: usize, q: &NetPin| -> (Pt, f64) {
        let pin = abs(q);
        let x = drop_x
            .get(&(n, q.part, q.pad.clone()))
            .copied()
            .unwrap_or(pin.x);
        (pin, x)
    };

    // Trunks that fit are kept; wide ones fall back to names.
    let mut trunks: Vec<Trunk> = Vec::new();
    let mut plan_is_labels: Vec<bool> = plans
        .iter()
        .map(|p| matches!(p.route, Route::Labels))
        .collect();
    for (n, pins) in net_pins.iter().enumerate() {
        if let Route::Bus(side) = plans[n].route {
            let xs: Vec<f64> = pins
                .iter()
                .zip(&plans[n].hooks)
                .filter(|(_, h)| !matches!(h, Hook::Label))
                .map(|(q, _)| drop_at(n, q).1)
                .collect();
            let left = xs.iter().copied().fold(f64::INFINITY, f64::min);
            let right = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if right - left > MAX_BUS_SPAN {
                plan_is_labels[n] = true;
            } else {
                trunks.push(Trunk {
                    net: n,
                    side,
                    left,
                    right,
                });
            }
        }
    }

    // The names of the pins that get a stub and a name, and what they reach: the trunks go
    // beyond them.
    let mut requests: Vec<StubRequest> = Vec::new();
    for (n, pins) in net_pins.iter().enumerate() {
        let wanted = pins.len() >= 2 || is_named(&nets[n].name);
        match plans[n].route {
            Route::Power | Route::Direct => {}
            Route::Bus(_) if !plan_is_labels[n] => {
                for (q, hook) in pins.iter().zip(&plans[n].hooks) {
                    if matches!(hook, Hook::Label) {
                        requests.push(StubRequest::new(n, q.part, abs(q), q.escape));
                    }
                }
            }
            _ => {
                if wanted {
                    requests.extend(
                        pins.iter()
                            .map(|q| StubRequest::new(n, q.part, abs(q), q.escape)),
                    );
                }
            }
        }
    }
    let stubs = draw_stubs(ctx, &parts, requests);
    if let Some(r) = stubs.extent {
        top = top.min(r.min.y);
        bottom = bottom.max(r.max.y);
    }
    let first_up = snap(top - CHANNEL_MARGIN, GRID);
    let first_down = snap_up(bottom + CHANNEL_MARGIN, GRID);

    // Narrow nets take the tracks nearest the parts; a track is shared by nets that do
    // not overlap.
    trunks.sort_by(|a, b| {
        (a.right - a.left, a.left, a.net)
            .partial_cmp(&(b.right - b.left, b.left, b.net))
            .unwrap()
    });
    let mut tracks: HashMap<(Dir, usize), Vec<(f64, f64)>> = HashMap::new();
    let mut track_of: HashMap<usize, usize> = HashMap::new();
    for t in &trunks {
        let mut k = 0;
        while tracks.get(&(t.side, k)).is_some_and(|used| {
            used.iter()
                .any(|&(l, r)| t.left < r + COLUMN_GAP && l < t.right + COLUMN_GAP)
        }) {
            k += 1;
        }
        tracks
            .entry((t.side, k))
            .or_default()
            .push((t.left, t.right));
        track_of.insert(t.net, k);
    }
    let track_y = |side: Dir, k: usize| -> f64 {
        if side == Dir::Up {
            first_up - k as f64 * TRACK_PITCH
        } else {
            first_down + k as f64 * TRACK_PITCH
        }
    };

    let mut wires: Vec<Wire> = stubs.wires;
    let mut labels: Vec<NetLabel> = stubs.labels;
    let mut trunk_labels: Vec<(usize, f64, f64, Dir, f64)> = Vec::new();
    let mut z_used: HashMap<(usize, usize), usize> = HashMap::new();

    for (n, pins) in net_pins.iter().enumerate() {
        let name = &nets[n].name;
        let plan = &plans[n];
        match plan.route {
            Route::Power => {}
            Route::Direct => {
                let (a, b) = (abs(&pins[0]), abs(&pins[1]));
                if pins[0].escape.is_vertical() {
                    wires.push(Wire {
                        net: n,
                        points: vec![a, b],
                    });
                    if is_named(name) || ctx.external[n] {
                        let mid = Pt::new(a.x + GRID, (a.y + b.y) / 2.0 + 0.4);
                        labels.push(NetLabel {
                            net: n,
                            text: text(name, mid, Anchor::Start),
                        });
                    }
                } else {
                    let (left, right) = if a.x <= b.x { (a, b) } else { (b, a) };
                    let (lc, rc) = if a.x <= b.x {
                        (column_of[pins[0].part], column_of[pins[1].part])
                    } else {
                        (column_of[pins[1].part], column_of[pins[0].part])
                    };
                    let nth = z_used.entry((lc, rc)).or_insert(0);
                    let mid = snap((left.x + right.x) / 2.0, GRID) + *nth as f64 * PITCH;
                    *nth += 1;
                    let points = if (left.y - right.y).abs() < 1e-9 {
                        vec![left, right]
                    } else {
                        vec![left, Pt::new(mid, left.y), Pt::new(mid, right.y), right]
                    };
                    wires.push(Wire { net: n, points });
                    if is_named(name) || ctx.external[n] {
                        let at = Pt::new(left.x + GRID, left.y - 0.6);
                        labels.push(NetLabel {
                            net: n,
                            text: text(name, at, Anchor::Start),
                        });
                    }
                }
            }
            Route::Bus(side) if !plan_is_labels[n] => {
                let y = track_y(side, track_of[&n]);
                let mut xs = Vec::new();
                for (q, hook) in pins.iter().zip(&plan.hooks) {
                    let (pin, x) = drop_at(n, q);
                    match hook {
                        Hook::Drop => {
                            let mut points = vec![pin];
                            if !q.escape.is_vertical() {
                                points.push(Pt::new(x, pin.y));
                            }
                            points.push(Pt::new(x, y));
                            wires.push(Wire { net: n, points });
                            xs.push(x);
                        }
                        Hook::Wrap => {
                            let out = pin.step(q.escape, STUB);
                            wires.push(Wire {
                                net: n,
                                points: vec![pin, out, Pt::new(x, out.y), Pt::new(x, y)],
                            });
                            xs.push(x);
                        }
                        Hook::Label => {}
                    }
                }
                let left = xs.iter().copied().fold(f64::INFINITY, f64::min);
                let right = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                if right > left {
                    wires.push(Wire {
                        net: n,
                        points: vec![Pt::new(left, y), Pt::new(right, y)],
                    });
                }
                if is_named(name) || ctx.external[n] {
                    trunk_labels.push((n, left, right, side, y));
                }
            }
            // Names instead of wires: the stubs are drawn by `draw_stubs`.
            _ => {}
        }
    }

    // Trunk names go on the outer side of the trunk, clear of any wire that passes by.
    let verticals: Vec<(f64, f64, f64)> = wires
        .iter()
        .flat_map(|w| {
            w.points
                .windows(2)
                .filter(|s| (s[0].x - s[1].x).abs() < 1e-9)
                .map(|s| (s[0].x, s[0].y.min(s[1].y), s[0].y.max(s[1].y)))
                .collect::<Vec<_>>()
        })
        .collect();
    for (n, left, right, side, y) in trunk_labels {
        let name = &ctx.design.nets[n].name;
        let width = text_width(name);
        let baseline = if side == Dir::Up {
            y - 0.8
        } else {
            y + 0.8 + TEXT_SIZE
        };
        let band = (baseline - TEXT_SIZE, baseline + 0.4);
        let clear = |x: f64| {
            !verticals.iter().any(|&(vx, y0, y1)| {
                vx > x - 0.4 && vx < x + width + 0.4 && y0 < band.1 && y1 > band.0
            })
        };
        let mut x = left + 0.6;
        let fits = loop {
            if x + width > right + 0.01 {
                break false;
            }
            if clear(x) {
                break true;
            }
            x += GRID;
        };
        let t = if fits {
            text(name, Pt::new(x, baseline), Anchor::Start)
        } else {
            // No clear stretch of the trunk: put the name past its left end.
            text(name, Pt::new(left - 0.6, baseline), Anchor::End)
        };
        labels.push(NetLabel { net: n, text: t });
    }

    let mut symbols = power.symbols;
    wires.extend(power.wires);
    settle_symbol_names(&mut symbols, &parts, &wires, &labels);
    let junctions = junctions(&wires);

    let mut bounds = parts
        .first()
        .map_or(Rect::around(Pt::new(0.0, 0.0)), |p| p.extent);
    for p in &parts {
        bounds.union(p.extent);
    }
    for w in &wires {
        for &pt in &w.points {
            bounds.include(pt);
        }
    }
    for l in &labels {
        bounds.union(l.text.extent());
    }
    for sym in &symbols {
        bounds.union(sym.extent());
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
        parts,
        symbols,
        sheets: Vec::new(),
        wires,
        junctions,
        labels,
        bounds,
    }
}

/// A pin that gets a short wire and the net's name at its end.
struct StubRequest {
    net: usize,
    part: usize,
    at: Pt,
    escape: Dir,
}

impl StubRequest {
    fn new(net: usize, part: usize, at: Pt, escape: Dir) -> Self {
        StubRequest {
            net,
            part,
            at,
            escape,
        }
    }
}

struct Stubs {
    wires: Vec<Wire>,
    labels: Vec<NetLabel>,
    /// What the wires and names reach.
    extent: Option<Rect>,
}

/// The stubs and names of the pins that are named instead of wired. A run of pins on one
/// side of a part, next to each other, on one net, share one bar and one name. A name that
/// would be written over another is turned to read upward. Names are cut at
/// [`MAX_LABEL_CHARS`].
fn draw_stubs(ctx: &Ctx, parts: &[PlacedPart], mut requests: Vec<StubRequest>) -> Stubs {
    let mut out = Stubs {
        wires: Vec::new(),
        labels: Vec::new(),
        extent: None,
    };
    let along = |r: &StubRequest| {
        if r.escape.is_vertical() {
            r.at.x
        } else {
            r.at.y
        }
    };
    requests.sort_by(|a, b| {
        (a.part, a.escape as u8, a.net)
            .cmp(&(b.part, b.escape as u8, b.net))
            .then(along(a).total_cmp(&along(b)))
    });
    requests.dedup_by(|b, a| {
        a.part == b.part && a.net == b.net && a.escape == b.escape && same_point(a.at, b.at)
    });
    // A run: a list of requests on one side of one part, one net, with no other pin between.
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for (i, r) in requests.iter().enumerate() {
        let joins = runs.last().is_some_and(|run| {
            let last = &requests[*run.last().expect("a run has a pin")];
            last.part == r.part
                && last.net == r.net
                && last.escape == r.escape
                && along(r) - along(last) <= MAX_RUN_GAP + 1e-6
                && !parts[r.part].pins.iter().any(|q| {
                    let a = if r.escape.is_vertical() {
                        q.at.x
                    } else {
                        q.at.y
                    };
                    q.escape == r.escape && a > along(last) + 1e-6 && a < along(r) - 1e-6
                })
        });
        if joins {
            runs.last_mut().expect("a run to join").push(i);
        } else {
            runs.push(vec![i]);
        }
    }
    // Where each run's name could go, first choice first.
    let mut placed: Vec<Rect> = Vec::new();
    let overlaps = |a: &Rect, b: &Rect| {
        a.min.x < b.max.x - 0.05
            && b.min.x < a.max.x - 0.05
            && a.min.y < b.max.y - 0.05
            && b.min.y < a.max.y - 0.05
    };
    for run in runs {
        let first = &requests[run[0]];
        let (net, escape) = (first.net, first.escape);
        let name = ellipsize(&ctx.design.nets[net].name);
        let ends: Vec<Pt> = run
            .iter()
            .map(|&i| requests[i].at.step(escape, LABEL_STUB))
            .collect();
        for (&i, end) in run.iter().zip(&ends) {
            out.wires.push(Wire {
                net,
                points: vec![requests[i].at, *end],
            });
        }
        let mid = if let [only] = ends.as_slice() {
            *only
        } else {
            let (a, b) = (ends[0], ends[ends.len() - 1]);
            out.wires.push(Wire {
                net,
                points: vec![a, b],
            });
            Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
        };
        let across = match escape {
            Dir::Right => text(&name, Pt::new(mid.x + 0.6, mid.y + 0.4), Anchor::Start),
            Dir::Left => text(&name, Pt::new(mid.x - 0.6, mid.y + 0.4), Anchor::End),
            Dir::Up => text(&name, Pt::new(mid.x, mid.y - 0.8), Anchor::Middle),
            Dir::Down => text(
                &name,
                Pt::new(mid.x, mid.y + 0.8 + TEXT_SIZE),
                Anchor::Middle,
            ),
        };
        let chosen = if escape.is_vertical() && placed.iter().any(|p| overlaps(&across.extent(), p))
        {
            // Read upward, from the end of the stub (downward stubs: toward it).
            let up = if escape == Dir::Up {
                Text::new(&name, Pt::new(mid.x + 0.45, mid.y - 0.6), Anchor::Start)
            } else {
                Text::new(&name, Pt::new(mid.x + 0.45, mid.y + 0.6), Anchor::End)
            };
            up.turned()
        } else {
            across
        };
        placed.push(chosen.extent());
        out.labels.push(NetLabel { net, text: chosen });
    }
    for w in &out.wires {
        for &pt in &w.points {
            union_into(&mut out.extent, Rect::around(pt));
        }
    }
    for l in &out.labels {
        union_into(&mut out.extent, l.text.extent());
    }
    out
}

fn union_into(into: &mut Option<Rect>, r: Rect) {
    match into {
        Some(o) => o.union(r),
        None => *into = Some(r),
    }
}

/// A dot where a wire meets the middle of another of the same net, or where three or more
/// wires end at one point.
fn junctions(wires: &[Wire]) -> Vec<Junction> {
    let key = |p: Pt| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64);
    let mut by_net: HashMap<usize, Vec<(Pt, Pt)>> = HashMap::new();
    for w in wires {
        for s in w.points.windows(2) {
            by_net.entry(w.net).or_default().push((s[0], s[1]));
        }
    }
    let mut nets: Vec<usize> = by_net.keys().copied().collect();
    nets.sort_unstable();

    let mut out = Vec::new();
    for net in nets {
        let segments = &by_net[&net];
        let mut seen = BTreeSet::new();
        for &(a, b) in segments {
            for p in [a, b] {
                if !seen.insert(key(p)) {
                    continue;
                }
                let (mut ends, mut through) = (0, 0);
                for &(s, e) in segments {
                    if key(s) == key(p) || key(e) == key(p) {
                        ends += 1;
                    } else if lies_inside(p, s, e) {
                        through += 1;
                    }
                }
                if ends >= 3 || (through >= 1 && ends + through >= 2) {
                    out.push(Junction { net, at: p });
                }
            }
        }
    }
    out
}

/// Whether `p` is strictly between the ends of an axis-aligned segment.
fn lies_inside(p: Pt, a: Pt, b: Pt) -> bool {
    const EPS: f64 = 1e-6;
    if (a.x - b.x).abs() < EPS {
        (p.x - a.x).abs() < EPS && p.y > a.y.min(b.y) + EPS && p.y < a.y.max(b.y) - EPS
    } else if (a.y - b.y).abs() < EPS {
        (p.y - a.y).abs() < EPS && p.x > a.x.min(b.x) + EPS && p.x < a.x.max(b.x) - EPS
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_turned_to_read_upward_covers_a_box_above_its_start() {
        let t = Text::new("ABCDE", Pt::new(10.0, 20.0), Anchor::Start).turned();
        let r = t.extent();
        let width = text_width("ABCDE");
        assert!((r.max.y - 20.0).abs() < 1e-9 && (r.min.y - (20.0 - width)).abs() < 1e-9);
        assert!(r.min.x < 10.0 && r.max.x > 10.0 && r.max.x < 10.0 + TEXT_SIZE);
        // Anchored at its end it reaches below its point.
        let r = Text::new("ABCDE", Pt::new(10.0, 20.0), Anchor::End)
            .turned()
            .extent();
        assert!((r.min.y - 20.0).abs() < 1e-9 && (r.max.y - (20.0 + width)).abs() < 1e-9);
    }

    #[test]
    fn a_long_name_is_cut_with_an_ellipsis_and_a_short_one_is_not() {
        assert_eq!(ellipsize("GND"), "GND");
        let exact = "x".repeat(MAX_LABEL_CHARS);
        assert_eq!(ellipsize(&exact), exact);
        let cut = ellipsize(&"y".repeat(MAX_LABEL_CHARS + 5));
        assert_eq!(cut.chars().count(), MAX_LABEL_CHARS);
        assert!(cut.ends_with('…'));
    }
}
