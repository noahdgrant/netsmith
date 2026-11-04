//! Where each part goes and how each net is drawn.
//!
//! **Design goal: a schematic is for a human to read.** It is the end customer of this view
//! (an agent reads the netsmith source better than any drawing), so every decision here is
//! "what would a careful person draw on paper", and when a rule conflicts with readability,
//! readability wins.
//!
//! What a person draws, and what this does:
//!
//! 1. The main parts (an IC, a connector) are in the middle, in a row that reads left to
//!    right along the signal. Each is turned so its pins point toward what it connects to.
//! 2. A two-terminal part that serves one pin (a decoupling capacitor, a pull-up, a
//!    feedback divider, a bootstrap capacitor) goes next to that pin, on the pin's side.
//!    Parts in series (a divider) stand in one stack, and what is on the node between them
//!    is wired out sideways. Supplies are at the top of a part, grounds at the bottom.
//! 3. Every net on a sheet is wired: orthogonal wires on a grid, round parts and texts,
//!    crossing other wires only at right angles ([`grid`]). A name is used only where a wire
//!    cannot do: a net that goes on to another sheet leaves through a flag, and a rail is
//!    drawn with a power symbol at the end of a short wire (never on a wire). A net with a
//!    generated name is always only wires.
//! 4. A wire never crosses a text. A pin that connects to nothing has an X.
//!
//! Nothing here knows how a layout is drawn: [`crate::render_svg`] reads the result.

mod engine;
mod grid;

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

/// A flag where a net leaves the sheet, shaped like KiCad's hierarchical label: a pentagon
/// whose point is where the wire ends.
#[derive(Debug, Clone, PartialEq)]
pub struct Port {
    pub net: usize,
    /// Where the wire ends and the point of the flag is.
    pub at: Pt,
    /// The way the flag points away from the wire.
    pub dir: Dir,
    /// How long the flag is, from its point.
    pub length: f64,
    /// The net's name, inside the flag; none for a net with a generated name.
    pub label: Option<Text>,
}

impl Port {
    pub fn extent(&self) -> Rect {
        let tip = self.at;
        let end = tip.step(self.dir, self.length);
        let mut r = Rect::around(Pt::new(tip.x, tip.y - 1.1));
        r.include(Pt::new(tip.x, tip.y + 1.1));
        r.include(Pt::new(end.x, end.y - 1.1));
        r.include(Pt::new(end.x, end.y + 1.1));
        r
    }
}

/// Why a pin has an X.
#[derive(Debug, Clone, PartialEq)]
pub enum MarkKind {
    /// Nothing is connected to it: left open on purpose or knowingly.
    Open,
    /// The checker says it must be connected and it is not; the message is the checker's.
    Must(String),
}

/// An X at the end of a pin that has nothing connected to it.
#[derive(Debug, Clone, PartialEq)]
pub struct PinMark {
    /// Index into `design.parts`.
    pub part: usize,
    pub pad: String,
    pub at: Pt,
    pub kind: MarkKind,
    /// The pin's name and kind, for the tooltip.
    pub pin_name: String,
    pub pin_kind: String,
}

/// What the checker knows about pins that the drawing shows: the pins that must be
/// connected, by refdes and pad, with the checker's message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marks {
    pub must: std::collections::BTreeMap<(String, String), String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// In the same order as `design.parts`.
    pub parts: Vec<PlacedPart>,
    pub symbols: Vec<PowerSymbol>,
    pub ports: Vec<Port>,
    pub marks: Vec<PinMark>,
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
    layout_marked(design, rails, &Marks::default(), mapping, lib)
}

/// [`layout_rails`] with the pins that must be connected told.
pub fn layout_marked(
    design: &Design,
    rails: &Rails,
    marks: &Marks,
    mapping: &Mapping,
    lib: &SymbolLib,
) -> Result<Layout, DesignError> {
    layout_with(
        design,
        mapping,
        lib,
        &vec![false; design.nets.len()],
        rails,
        marks,
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
    marks: &Marks,
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
            // A net with a generated name is an internal node, never a rail.
            if !is_named(&n.name) {
                None
            } else if rails.ground.contains(&n.name) {
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
    Ok(engine::run(&ctx, &units, &order, &orient, marks))
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
    // A net that has the pins of exactly two two-terminal parts joins them in a stack; what
    // else is on it (a tap, such as the pin a divider feeds) is wired out of the node.
    for (n, net) in ctx.design.nets.iter().enumerate() {
        if ctx.power[n].is_some() || ctx.ground[n] {
            continue;
        }
        let find = |r: &str| ctx.design.parts.iter().position(|p| p.refdes == r).unwrap();
        let stackable: Vec<(usize, &str)> = net
            .pins
            .iter()
            .map(|pin| (find(&pin.refdes), pin.pin.as_str()))
            .filter(|&(p, _)| is_stackable(ctx, p))
            .collect();
        // Parts that hang on a node with something else (an inductor, a fuse) are not in
        // series with each other, but each in series with what the node goes to.
        let in_series_kind = |p: usize| {
            matches!(
                ctx.design.parts[p].kind.as_str(),
                "Inductor" | "FerriteBead" | "Fuse"
            )
        };
        if let [(a, pa), (b, pb)] = stackable[..]
            && a != b
            && !(net.pins.len() > 2 && (in_series_kind(a) || in_series_kind(b)))
        {
            link.insert((a, pa.to_string()), (b, pb.to_string()));
            link.insert((b, pb.to_string()), (a, pa.to_string()));
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
        let walk = pull_together(ctx, unit_of, walk);
        let mut walk = walk;
        if signal_runs_backwards(ctx, &walk, units, unit_of) {
            walk.reverse();
        }
        order.extend(walk);
    }
    order
}

/// The walk with neighbours swapped while that shortens the nets: each net that is not a
/// power net pulls the units it joins together, a narrow net harder than a wide one. The
/// first unit stays first. Passes go in index order, so the result is the same each time.
fn pull_together(ctx: &Ctx, unit_of: &HashMap<usize, usize>, mut walk: Vec<usize>) -> Vec<usize> {
    let nets: Vec<Vec<usize>> = ctx
        .design
        .nets
        .iter()
        .enumerate()
        .filter(|(n, _)| ctx.power[*n].is_none() && !ctx.ground[*n])
        .map(|(_, net)| {
            let mut us: Vec<usize> = net
                .pins
                .iter()
                .map(|pin| unit_of[&part_index(ctx, &pin.refdes)])
                .collect();
            us.sort_unstable();
            us.dedup();
            us
        })
        .filter(|us| us.len() >= 2)
        .collect();
    let cost = |walk: &[usize]| -> f64 {
        let place: HashMap<usize, usize> = walk.iter().enumerate().map(|(i, &u)| (u, i)).collect();
        nets.iter()
            .map(|us| {
                let at: Vec<usize> = us.iter().filter_map(|u| place.get(u).copied()).collect();
                if at.len() < 2 {
                    return 0.0;
                }
                let span = at.iter().max().unwrap() - at.iter().min().unwrap();
                span as f64 / (at.len() - 1) as f64
            })
            .sum()
    };
    for _ in 0..walk.len() {
        let mut changed = false;
        for i in 1..walk.len().saturating_sub(1) {
            let before = cost(&walk);
            walk.swap(i, i + 1);
            if cost(&walk) < before - 1e-9 {
                changed = true;
            } else {
                walk.swap(i, i + 1);
            }
        }
        if !changed {
            break;
        }
    }
    walk
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
            orient[p] = if ctx.glyphs[p].dense {
                // A table of pins stays upright: supplies at the top, grounds at the bottom.
                Orient::IDENTITY
            } else if unit.up_pads.is_empty() {
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
// Placement

/// The box of the symbol's body and pins once turned.
fn body_of(glyph: &Glyph, orient: Orient) -> Rect {
    let b = glyph.bounds();
    let mut body = Rect::around(orient.apply((b.min.x, b.min.y)));
    body.include(orient.apply((b.max.x, b.max.y)));
    body.include(orient.apply((b.min.x, b.max.y)));
    body.include(orient.apply((b.max.x, b.min.y)));
    body
}

/// The reference and the value of a part, written on `side` of its body.
fn part_texts(part: &netsmith_core::Part, body: Rect, side: Dir) -> (Text, Option<Text>) {
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
    (reference, value)
}

/// Turn a part's glyph into a placed part at the origin and work out its texts.
fn place_part(ctx: &Ctx, p: usize, orient: Orient, nets: &[Option<usize>]) -> PlacedPart {
    let glyph = ctx.glyphs[p].clone();
    let part = &ctx.design.parts[p];
    let body = body_of(&glyph, orient);

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
    let (reference, value) = part_texts(part, body, side);

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

/// The pin of a placed part, relative to its origin, that faces down (`down`) or up.
fn pin_on(part: &PlacedPart, up_pad: &str, down: bool) -> Pt {
    part.pins
        .iter()
        .find(|q| (q.id == up_pad) != down)
        .expect("a stacked part has two pins")
        .at
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
