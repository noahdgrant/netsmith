//! Where each part goes and how each net is drawn.
//!
//! **Design goal: a schematic is for a human to read.** It is the end customer of this view
//! (an agent reads the netsmith source better than any drawing), so every decision here is
//! "what would a careful person draw on paper", and when a rule conflicts with readability,
//! readability wins.
//!
//! What a person draws, and what this does:
//!
//! 1. The main parts (an IC, a connector) are in rows that read left to right along the
//!    signal; a sheet with enough parts is set in as many rows as make it a page (about 1.4 to
//!    1.8 wide for each high), not a strip. Each part is turned so its pins point toward what
//!    it connects to.
//! 2. A two-terminal part that serves one pin (a decoupling capacitor, a pull-up, a
//!    feedback divider, a bootstrap capacitor) goes next to that pin, on the pin's side, and
//!    is wired to it, always. Parts in series (a divider) stand in one stack, and what is on
//!    the node between them is wired out sideways. Supplies are at the top of a part, grounds
//!    at the bottom. A two-terminal part across **two** pins of one main part (a crystal across
//!    `XL1` and `XL2`, a resonator, a series element) is a **pair satellite**: it is placed
//!    beside both pins, in the orientation and at the place whose two wires are cheapest by
//!    length, bends and crossings (every candidate is tried, in a fixed order, and routed on a
//!    scratch copy of the grid), the wires are drawn before any other, they leave the part's
//!    pins in any direction and cross nothing, and their nets are never named (`engine/pair.rs`).
//! 3. A net is drawn as wires when it is local and short, and as a **name at each of its
//!    places** when wires would be long or tangled. A wired net is orthogonal wires on a
//!    grid, round parts and texts, crossing other wires only at right angles ([`grid`]). A
//!    named net is a short stub out of a pin with the net's name at its end (a [`NetLabel`],
//!    plain text in line with the stub) at each group of its pins (a main part and the parts
//!    that serve its pins), and the pins of a group are wired to that one stub. Which nets
//!    are named is [`policy`]'s rule, decided from a first drawing of the sheet: a net that
//!    joins more than one group, and whose wire is longer than 48 grid steps (61 mm), or
//!    that fans out to three groups over 30 steps, or crosses more than one other wire, is
//!    named, the worst crossing first. A small sheet, whose wires are short, has none. A net
//!    that goes on to another sheet is always named, by a flag (the shape of KiCad's
//!    hierarchical label) at the end of the stub of each group of its pins. A rail is drawn
//!    with a power symbol at the end of a short wire (never on a wire, never named). The net
//!    of a part that serves a pin is only wires. A generated name is shown on a label as the
//!    first pin of the net's main part (`J2.3`), as KiCad names such a net, and on a flag not
//!    at all. The rails of a main part's own pins are drawn as before (a run of neighbouring
//!    pins shares a bus and a symbol, and the rail is one wire across the part's group); the
//!    rail pin of a small part that stands beside a main part has a **stub of its own** of a
//!    few cells and a symbol at its end (supply up, ground down) instead of a drop to that wire,
//!    which would run the length of the sheet and across other wires. Small parts side by side
//!    in a row whose pins end on one line share a short wire and one symbol when nothing is on
//!    its way ([`policy`]). Satellites on one rail that are not in one row (two pull-ups of a bus,
//!    one a few steps above the other) share one wire and one symbol too, where the stubs end
//!    within 20 cells of each other and a path of free nodes joins them: the lower one is wired
//!    up to the row of the other's stub, across, and has no symbol of its own (`engine::bridge`).
//!    The wire of a main part's rail is beyond the parts that have a pin on it (and what is written
//!    at their pins), not beyond every part drawn in the same row: the supply of a power unit
//!    that stands beside a taller signal unit is just above it.
//! 4. A wire never crosses a text. A pin that connects to nothing has an X.
//! 5. The members of one interface instance (the design carries them, [`netsmith_core::Design::interfaces`])
//!    are a **bus** when three or more of them run between the same two main parts, or leave the
//!    sheet from one part, each with one pin on each part and all on one side of it. The pins stay
//!    one by one on the part's edge; each member's wire is a short entry that meets one thick
//!    line, a spine beside the pins and a trunk that runs to the other part's spine or, where the
//!    members go on to another sheet, ends in **one** flag with the interface's name. The line is
//!    named once, by the instance, with the list of members and a slash with their count, and it
//!    joins nothing: each entry joins the net of its name at the other end, as a label does. The
//!    members have no flag or name of their own. See `engine/bus.rs` and ADR 0049.
//!
//! 6. The power path through a regulator is a **spine**, one row from left to right, as it is
//!    on a hand drawing (`spine.rs`, `engine/spine.rs`; found from the netlist, the pin kinds and
//!    the rails when they are known, never from what a part is called). On the row: the series
//!    parts (a fuse, a diode, a ferrite bead, the inductor: two pins on two different nets, neither
//!    a ground, not across two pins of the IC), each turned along it, from where the power comes in
//!    to the IC's input pins on its left, and from its switching pin on its right out to the output
//!    node; the sheet's own input protection, whose far end goes on to another sheet, is the first
//!    stretch of it. The capacitors to ground hang from the node they serve, two steps under the
//!    row, the smallest nearest the IC, each with a stub and a ground symbol of its own (side by
//!    side they share one, as above), and the feedback divider hangs from the output node with the
//!    feedback pin wired to its middle. A rail on the row is a wire along it with one symbol at
//!    its outer end. The row of each side is the line of the IC's first pin on it, and a row is
//!    kept free: the satellites of the IC's other pins are placed first beside it, the row's parts
//!    beyond them, and the wires of a pair satellite (a bootstrap capacitor) join the row where
//!    they meet it and cross nothing. The sheet of a spine and a few more pieces is not cut into
//!    rows. `NETSMITH_SPINE=off` draws without it, to compare (the metrics `spine_bends` and
//!    `detached_chain` still find the chains).
//!
//! 7. A part whose library symbol has several **units** is drawn as one box for each, as the
//!    library draws the unit, upright, with the part's reference and the unit's letter (`U5A`
//!    for the supplies and the twenty-five grounds of a processor module, `U5B` for its signals;
//!    ADR 0050, `units.rs`). The decision is the library's (the unit of each pin of the symbol),
//!    never a name or a pin kind. Each box is a main part to the engine: the capacitors that
//!    decouple the supply stand beside the power unit, its grounds are one column with one wire
//!    and one symbol, and the signal unit has no power row. `layout.parts[i]` is part `i` of the
//!    design (a part of several units: its box with most pins) and the boxes of the further units
//!    follow them; [`PlacedPart::part`] says which part a box draws. The checker still rebuilds
//!    the nets pin by pin and has one rule more: a pad is drawn in exactly one box.
//!
//! Nothing here knows how a layout is drawn: [`crate::render_svg`] reads the result.

mod engine;
mod grid;
mod policy;
mod prune;
mod spine;
mod units;

pub(crate) use units::unit_letter;

use std::collections::{BTreeSet, HashMap, VecDeque};

use netsmith_core::{Design, DesignError};
use netsmith_kicad::{Mapping, Pinout, SymbolLib};

use crate::geom::{Dir, Orient, Pt, Rect, snap, snap_up};
use crate::glyph::{CHAR_WIDTH, Flow, Glyph, used_pads};

const GRID: f64 = 1.27;
/// Distance from a pin to where its wire may first turn.
const STUB: f64 = 2.54;
/// The wire between two parts stacked in one column.
const CHAIN_WIRE: f64 = 5.08;
pub const TEXT_SIZE: f64 = 1.27;
const BORDER: f64 = 5.08;
/// A net name longer than this is cut, with an ellipsis; the drawing keeps the whole name in
/// the tooltip.
pub const MAX_LABEL_CHARS: usize = 24;

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
    /// Index into `design.parts`: the part this box draws. A part whose symbol has several
    /// units has a box for each, with the same `part`; the first `design.parts.len()` boxes of
    /// a layout are the parts' own (a part with units: its unit with most pins) and the other
    /// units follow them (`units.rs`).
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

/// The name of a net at the end of a short stub, where a net is drawn as a name and not as a
/// wire: plain text, in line with the stub that leaves a pin ([`policy`]). Same-name labels
/// on a sheet are one net, as a flag's are.
#[derive(Debug, Clone, PartialEq)]
pub struct NetLabel {
    pub net: usize,
    pub text: Text,
    /// Where the label meets its wire: the end of the stub.
    pub at: Pt,
    /// The way the text runs away from `at`, the way the stub points.
    pub dir: Dir,
}

/// One member of a [`Bus`]: the net it is on and what the interface calls it.
#[derive(Debug, Clone, PartialEq)]
pub struct BusMember {
    /// Index into `design.nets`.
    pub net: usize,
    /// The member's name in the bus's interface (`tx`).
    pub name: String,
}

/// Where the wire of one member meets a bus's thick line: the end of the member's wire.
#[derive(Debug, Clone, PartialEq)]
pub struct BusEntry {
    /// The member's net. A member's entry joins the net of that name at the other end of the
    /// bus, as a label does; the bus itself joins nothing.
    pub net: usize,
    /// Which end of the bus it is at: an index into [`Bus::ends`].
    pub end: usize,
    /// Where the wire meets the thick line.
    pub at: Pt,
}

/// The flag that ends a bus whose members go on to another sheet: one for the whole bus, with
/// the interface's name in it, in the shape of a net's flag.
#[derive(Debug, Clone, PartialEq)]
pub struct BusFlag {
    /// Where the thick line ends and the point of the flag is.
    pub at: Pt,
    /// The way the flag points away from the line.
    pub dir: Dir,
    pub length: f64,
    pub label: Text,
}

impl BusFlag {
    /// The box the flag and its name cover.
    pub fn extent(&self) -> Rect {
        let port = Port {
            net: 0,
            at: self.at,
            dir: self.dir,
            length: self.length,
            label: None,
        };
        let mut r = port.extent();
        r.union(self.label.extent_at(Port::TEXT));
        r
    }
}

/// The members of one interface instance drawn as one bus: each member's wire is a short
/// entry that meets a thick line (the spine at an end, the trunk between the ends), which runs
/// to the other part, or ends in one flag where the members go on to another sheet. The thick
/// line carries no net; each entry carries its member's net and joins, by name, the net of
/// that name at the other end. The bus is named once, by the interface instance, with the
/// list of its members and their count.
#[derive(Debug, Clone, PartialEq)]
pub struct Bus {
    /// The path of the interface instance that names it (`mcu.uart0`): what the bus is
    /// keyed by.
    pub path: String,
    /// The interface's definition (`Uart`).
    pub type_name: String,
    /// What the label says: the instance's own name (`uart0`).
    pub name: String,
    /// The members, in the order of the pins at the first end.
    pub members: Vec<BusMember>,
    /// The thick line, as runs of corners: a spine at each end that meets the part, and the
    /// trunk between them or out to the flag.
    pub lines: Vec<Vec<Pt>>,
    /// The members' entries, `members.len()` at each end.
    pub entries: Vec<BusEntry>,
    /// Indices into `design.parts` of the part at each end: two, or one when the bus ends in a
    /// flag.
    pub ends: Vec<usize>,
    /// The flag, where the members go on to another sheet.
    pub flag: Option<BusFlag>,
    /// The name of the interface instance, and under it the list of its members.
    pub label: Text,
    pub list: Text,
    /// What the other end calls the interface, when it is not the same (`uart: {rx, tx}`).
    pub other: Option<Text>,
    /// The slash across the thick line, and the count of members beside it.
    pub tick: (Pt, Pt),
    pub count: Text,
}

impl Bus {
    /// Every straight run of the thick line.
    pub fn runs(&self) -> Vec<(Pt, Pt)> {
        let mut out = Vec::new();
        for line in &self.lines {
            for w in line.windows(2) {
                out.push((w[0], w[1]));
            }
        }
        out
    }

    /// The texts of the bus, with the flag's.
    pub fn texts(&self) -> Vec<&Text> {
        let mut out: Vec<&Text> = vec![&self.label, &self.list, &self.count];
        out.extend(self.other.as_ref());
        out.extend(self.flag.as_ref().map(|f| &f.label));
        out
    }
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
    /// Half the height of a flag, and the height of the name in it: flags of neighbouring pins
    /// are one pin pitch apart, with a grid step less a little between them.
    pub const HALF: f64 = 0.64;
    pub const TEXT: f64 = 1.0;

    pub fn extent(&self) -> Rect {
        let tip = self.at;
        let end = tip.step(self.dir, self.length);
        // Half a flag's height to each side of the line the flag runs along.
        let (px, py) = if self.dir.is_vertical() {
            (Self::HALF, 0.0)
        } else {
            (0.0, Self::HALF)
        };
        let mut r = Rect::around(Pt::new(tip.x - px, tip.y - py));
        r.include(Pt::new(tip.x + px, tip.y + py));
        r.include(Pt::new(end.x - px, end.y - py));
        r.include(Pt::new(end.x + px, end.y + py));
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

/// The power path through an IC as the layout found it ([`spine`]): the chains of series parts
/// that lead to it, each from the IC outward. The metrics `spine_bends` and `detached_chain`
/// read the drawing against it.
#[derive(Debug, Clone, PartialEq)]
pub struct SpineMark {
    /// Index into `design.parts` of the IC.
    pub ic: usize,
    /// Indices into `design.parts`: the input side's chain, the output side's, then the free
    /// chains (series parts of the sheet that join neither side), the nearest first. A side with
    /// no series part has no chain here.
    pub chains: Vec<Vec<usize>>,
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
    /// The interfaces drawn as one thick line, by the path of their instance.
    pub buses: Vec<Bus>,
    /// The power paths found through ICs, drawn as a spine or not.
    pub spines: Vec<SpineMark>,
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
    /// The power paths through ICs ([`spine`]), found when the rails are known.
    spines: Vec<spine::Spine>,
    /// Whether each part belongs to one: it is placed with its IC and stands in no stack.
    spine_part: Vec<bool>,
    /// Whether each net is on one: it is wired along the line, a rail on it has one symbol.
    spine_net: Vec<bool>,
    /// The choices the engine is run with: the first drawing uses [`Tuning::default`], and a
    /// sheet that has wires crossing is drawn again with the others (see [`layout_with`]).
    tuning: Tuning,
    /// Which choices of [`Tuning`] the drawing made last has a place where the choice makes a
    /// difference (see [`Ctx::touch`]).
    touched: std::cell::Cell<u8>,
    /// Where the pair satellites were placed, for the drawings of this sheet to share.
    pair_places: engine::PairCache,
}

impl Ctx<'_> {
    /// Says that the drawing being made came to a place where the choice of [`Tuning`] with the
    /// bit `bit` (`Tuning::*_BIT`) would have drawn something else, whether the choice is on
    /// or off now. A drawing is made by steps that do not read the choices anywhere else, so
    /// two drawings that agree on the choices told here are the same drawing, and a sheet
    /// need not be drawn again for a choice that was never told (see [`layout_with_tuning`]).
    fn touch(&self, bit: u8) {
        self.touched.set(self.touched.get() | bit);
    }
}

/// Choices of the engine that are not right for every sheet. A sheet is drawn with the
/// default of each; when that leaves wires crossing, it is drawn again with each of
/// [`Tuning::SINGLE`], and the drawing with the fewest crossings is kept (the first of equals,
/// so a sheet that does not cross never changes). Nothing here is chosen by a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tuning {
    /// A main part's rail pins (at most [`Tuning::LOCAL_MAIN_PINS`] on one net) have a stub
    /// and a symbol of their own, short and local, as a satellite's have, instead of a wire to
    /// the rail of the group: the ground of an IC's pin is a symbol just beyond the pin.
    pub local_main: bool,
    /// The satellites of a part's pins are placed from the bottom pin up, not from the top
    /// down (the lower pin's satellite is nearest, and the wire of a pin above passes over it).
    pub bottom_first: bool,
    /// A part across two supplies is a pair satellite, standing beside the two pins of its chip
    /// that the supplies are on.
    pub pair_rails: bool,
}

impl Tuning {
    /// The most pins of one rail on a main part that [`Tuning::local_main`] gives a symbol each.
    pub const LOCAL_MAIN_PINS: usize = 3;
    /// The bit of each choice in [`Ctx::touch`] and [`Tuning::bits`].
    pub(crate) const LOCAL_MAIN_BIT: u8 = 1;
    pub(crate) const BOTTOM_FIRST_BIT: u8 = 2;
    pub(crate) const PAIR_RAILS_BIT: u8 = 4;

    /// The choices as bits.
    fn bits(self) -> u8 {
        let bit = |on: bool, bit: u8| if on { bit } else { 0 };
        bit(self.local_main, Self::LOCAL_MAIN_BIT)
            | bit(self.bottom_first, Self::BOTTOM_FIRST_BIT)
            | bit(self.pair_rails, Self::PAIR_RAILS_BIT)
    }

    /// What a sheet that crosses is drawn with besides the default: each choice alone (and
    /// then those that helped, together).
    pub const SINGLE: [Tuning; 3] = [
        Tuning {
            local_main: true,
            ..Tuning::DEFAULT
        },
        Tuning {
            bottom_first: true,
            ..Tuning::DEFAULT
        },
        Tuning {
            pair_rails: true,
            ..Tuning::DEFAULT
        },
    ];

    /// The choices that any of `these` makes.
    fn union(these: &[Tuning]) -> Tuning {
        Tuning {
            local_main: these.iter().any(|t| t.local_main),
            bottom_first: these.iter().any(|t| t.bottom_first),
            pair_rails: these.iter().any(|t| t.pair_rails),
        }
    }

    const DEFAULT: Tuning = Tuning {
        local_main: false,
        bottom_first: false,
        pair_rails: false,
    };
}

impl Default for Tuning {
    fn default() -> Self {
        Self::DEFAULT
    }
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

/// [`layout_rails`] drawn once with the given [`Tuning`], without drawing again with the others
/// when wires cross: to see what a choice does (the tests do), not for a tool.
pub fn layout_tuned(
    design: &Design,
    rails: &Rails,
    mapping: &Mapping,
    lib: &SymbolLib,
    tuning: Tuning,
) -> Result<Layout, DesignError> {
    layout_tuned_touched(design, rails, mapping, lib, tuning).map(|(layout, _)| layout)
}

/// [`layout_tuned`], and the choices that made a difference somewhere in the drawing: each of
/// them was on or off at a place where the other value would have drawn something else. A
/// drawing with the choices of `tuning` is the same drawing with any other choices that agree
/// with `tuning` on those (it is how [`layout_rails`] leaves out a drawing it would only
/// repeat; the tests check it).
pub fn layout_tuned_touched(
    design: &Design,
    rails: &Rails,
    mapping: &Mapping,
    lib: &SymbolLib,
    tuning: Tuning,
) -> Result<(Layout, Tuning), DesignError> {
    let (layout, touched) = layout_with_tuning(
        design,
        mapping,
        lib,
        &vec![false; design.nets.len()],
        rails,
        &Marks::default(),
        rails.ground.len() > 1,
        Some(tuning),
    )?;
    Ok((
        layout,
        Tuning {
            local_main: touched & Tuning::LOCAL_MAIN_BIT != 0,
            bottom_first: touched & Tuning::BOTTOM_FIRST_BIT != 0,
            pair_rails: touched & Tuning::PAIR_RAILS_BIT != 0,
        },
    ))
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
    layout_with_tuning(
        design,
        mapping,
        lib,
        external,
        rails,
        marks,
        label_grounds,
        None,
    )
    .map(|(layout, _)| layout)
}

/// [`layout_with`], drawn only with `only` if it is given; and the choices of [`Tuning`] (as
/// bits) that made a difference in the first drawing.
#[allow(clippy::too_many_arguments)]
fn layout_with_tuning(
    design: &Design,
    mapping: &Mapping,
    lib: &SymbolLib,
    external: &[bool],
    rails: &Rails,
    marks: &Marks,
    label_grounds: bool,
    only: Option<Tuning>,
) -> Result<(Layout, u8), DesignError> {
    design.validate()?;
    let used = used_pads(design);
    // A part whose symbol has several units is drawn as one box for each: the engine draws a
    // derived design in which each unit is a part, and the boxes are told back to the design's
    // parts at the end (`units.rs`).
    let split = units::split(design, &used, marks, mapping, lib);
    let (design, marks, glyphs): (&Design, &Marks, Vec<Glyph>) = match &split {
        Some(s) => (&s.design, &s.marks, s.glyphs.clone()),
        None => (
            design,
            marks,
            design
                .parts
                .iter()
                .zip(&used)
                .map(|(p, u)| Glyph::for_part(p, u, Pinout::of(p).as_ref(), mapping, lib))
                .collect(),
        ),
    };

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
    let mut ctx = Ctx {
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
        spines: Vec::new(),
        spine_part: vec![false; design.parts.len()],
        spine_net: vec![false; design.nets.len()],
        tuning: Tuning::default(),
        touched: std::cell::Cell::new(0),
        pair_places: engine::PairCache::default(),
    };
    // `NETSMITH_SPINE=off` draws without the spines (to compare), the paths are still found.
    let found = spine::find(&ctx);
    let spine_marks: Vec<SpineMark> = found
        .iter()
        .map(|sp| SpineMark {
            ic: sp.ic,
            chains: sp
                .sides()
                .filter(|side| !side.parts.is_empty())
                .map(|side| side.parts.clone())
                .collect(),
        })
        .collect();
    if std::env::var_os("NETSMITH_SPINE").is_none_or(|v| v != "off") {
        ctx.spines = found;
    }
    for sp in &ctx.spines {
        for p in sp.parts() {
            ctx.spine_part[p] = true;
        }
        for (n, _) in sp.nets() {
            ctx.spine_net[n] = true;
        }
    }

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
    // Draw the sheet, see which nets are better as names, and draw it again with them: a name
    // takes room, and a net named changes what crosses what. Nets are only ever added, so
    // this ends; it stops after a few passes in any case.
    let draw = |ctx: &Ctx| {
        let mut labelled = vec![false; design.nets.len()];
        ctx.touched.set(0);
        let mut drawn = engine::run(ctx, &units, &order, &orient, marks, &labelled);
        for _ in 0..policy::PASSES {
            let next = policy::choose(ctx, &drawn, &labelled);
            if next == labelled {
                break;
            }
            labelled = next;
            ctx.touched.set(0);
            drawn = engine::run(ctx, &units, &order, &orient, marks, &labelled);
        }
        (drawn, labelled)
    };
    if let Some(tuning) = only {
        ctx.tuning = tuning;
    }
    let (first, labelled) = draw(&ctx);
    let mut best = Quality::of(&first.layout);
    let touched_first = ctx.touched.get();
    // The drawings made so far, the first at index 0, and which of them is kept.
    let mut runs = vec![Run {
        tuning: ctx.tuning,
        touched: ctx.touched.get(),
        quality: best,
        drawn: first,
    }];
    let mut kept_run = 0;
    if only.is_none() && best.crossings > 0 {
        // A sheet whose wires cross is drawn again, once for each of the choices of `Tuning`
        // alone, with the nets named as in the first drawing, and then once with all the choices
        // that were better together. A drawing that crosses less, or as much with at least 5 mm
        // less wire, and breaks no more rules replaces the one drawn so far. The rules broken are
        // counted only for such a drawing (the check is the dearest part), and once for the one
        // it would replace.
        //
        // A choice is only drawn when it can make a difference: the engine tells where it came
        // to a place where a choice matters (`Ctx::touch`), so a drawing that agrees with one
        // made before on every choice that one touched is that drawing, and it is not drawn
        // again (a sheet with no part across two supplies has nothing for `pair_rails`).
        let mut best_broken: Option<usize> = None;
        // The choices in the drawing kept so far.
        let mut kept = Tuning::default();
        let mut attempt = |ctx: &mut Ctx, tuning: Tuning| -> bool {
            let run = match runs
                .iter()
                .position(|r| (r.tuning.bits() ^ tuning.bits()) & r.touched == 0)
            {
                Some(run) => run,
                None => {
                    ctx.tuning = tuning;
                    ctx.touched.set(0);
                    let other = engine::run(ctx, &units, &order, &orient, marks, &labelled);
                    runs.push(Run {
                        tuning,
                        touched: ctx.touched.get(),
                        quality: Quality::of(&other.layout),
                        drawn: other,
                    });
                    runs.len() - 1
                }
            };
            let q = runs[run].quality;
            if !q.better_than(&best) {
                return false;
            }
            let held = *best_broken.get_or_insert_with(|| {
                crate::check::check(design, &runs[kept_run].drawn.layout).len()
            });
            let broken = crate::check::check(design, &runs[run].drawn.layout).len();
            if broken > held {
                return false;
            }
            best = q;
            best_broken = Some(broken);
            kept_run = run;
            true
        };
        // Each choice alone, then each that did not help added to those that did.
        let mut helped: Vec<Tuning> = Vec::new();
        for single in Tuning::SINGLE {
            if attempt(&mut ctx, single) {
                helped.push(single);
                kept = Tuning::union(&[kept, single]);
            }
        }
        for single in Tuning::SINGLE {
            if helped.contains(&single) || helped.is_empty() {
                continue;
            }
            let with = Tuning::union(&[kept, single]);
            if attempt(&mut ctx, with) {
                kept = with;
            }
        }
        ctx.tuning = Tuning::default();
    }
    let mut drawn = runs.swap_remove(kept_run).drawn;
    drawn.layout.spines = spine_marks;
    if let Some(s) = &split {
        for (placed, &part) in drawn.layout.parts.iter_mut().zip(&s.origin) {
            placed.part = part;
        }
        for m in &mut drawn.layout.marks {
            m.part = s.origin[m.part];
        }
    }
    Ok((drawn.layout, touched_first))
}

/// A sheet drawn with some choices of [`Tuning`], and where in it those choices mattered.
struct Run {
    tuning: Tuning,
    /// The bits of the choices that made a difference somewhere in it (`Ctx::touch`).
    touched: u8,
    quality: Quality,
    drawn: engine::Drawn,
}

/// What a drawing of a sheet is judged by when it is drawn again with another [`Tuning`].
#[derive(Clone, Copy)]
struct Quality {
    crossings: usize,
    symbols: usize,
    /// The wire in millimetres.
    length: f64,
}

impl Quality {
    fn of(layout: &Layout) -> Self {
        use crate::sheetview::{Rel, Seg, relate, simplify};
        let mut segs: Vec<Seg> = Vec::new();
        for w in &layout.wires {
            for pair in simplify(&w.points).windows(2) {
                segs.push(Seg {
                    net: w.net,
                    a: pair[0],
                    b: pair[1],
                });
            }
        }
        let length = segs.iter().map(Seg::length).sum();
        let mut crossings = 0;
        for (i, s) in segs.iter().enumerate() {
            for t in &segs[i + 1..] {
                if s.net != t.net && matches!(relate(s, t), Rel::Cross(_)) {
                    crossings += 1;
                }
            }
        }
        Self {
            crossings,
            symbols: layout.symbols.len(),
            length,
        }
    }

    /// Fewer crossings, or as many with at least 5 mm less wire, and not many more power
    /// symbols than the drawing it would replace (a symbol for every pin is not a better
    /// drawing). That it breaks no more rules is asked separately.
    fn better_than(&self, other: &Self) -> bool {
        self.symbols <= other.symbols + 2
            && (self.crossings < other.crossings
                || (self.crossings == other.crossings && self.length + 5.0 < other.length))
    }
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
    if g.symbol.is_none() || g.pins.len() != 2 || g.unit.is_some() || ctx.connector[p] {
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

/// Whether the far pins of two parts joined at `a` and `b` are on one rail (a pull-down and a
/// capacitor on one node, both to ground): they are two parts hung on the node, side by side,
/// not a stack with a wire round from the top of it to the bottom.
fn same_rail_beyond(ctx: &Ctx, a: (usize, &str), b: (usize, &str)) -> bool {
    let far = |(p, pad): (usize, &str)| net_of(ctx, p, other_pad(ctx, p, pad));
    match (far(a), far(b)) {
        (Some(x), Some(y)) => x == y && (ctx.power[x].is_some() || ctx.ground[x]),
        _ => false,
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
            .filter(|&(p, _)| is_stackable(ctx, p) && !ctx.spine_part[p])
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
            && !same_rail_beyond(ctx, (a, pa), (b, pb))
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
        let single = parts.len() == 1;
        let rank = |n: usize| match ctx.power[n] {
            // A part on its own (a pull-up, a pull-down) has its supply above and its ground
            // below, however far each net is from ground; in a stack the levels decide.
            Some(PowerKind::Supply) if single => usize::MAX / 2,
            Some(PowerKind::Ground) if single => 0,
            None if single && ctx.ground[n] => 0,
            _ => ctx.level[n] + 1,
        };
        let top = net_of(ctx, parts[0], &up_pads[0]).map_or(0, rank);
        let last = parts.len() - 1;
        let bottom_pad = other_pad(ctx, parts[last], &up_pads[last]);
        let bottom = net_of(ctx, parts[last], bottom_pad).map_or(0, rank);
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
            orient[p] = if ctx.glyphs[p].dense || ctx.glyphs[p].unit.is_some() {
                // A table of pins stays upright: supplies at the top, grounds at the bottom. A
                // unit of a symbol is drawn as the library draws it.
                Orient::IDENTITY
            } else if let Some(sp) = ctx.spines.iter().find(|s| s.ic == p) {
                // The input of a regulator on the left, its switching pin on the right.
                spine::orientation(ctx, sp)
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
    // A part with all its pins on one side (a connector), or with one pin (a test point), is
    // turned so that what is written at its pins reads across.
    let first = glyph.pins.first().map(|q| q.toward_body);
    let one_sided = glyph.pins.iter().all(|q| Some(q.toward_body) == first);
    let simple = one_sided || connected.len() == 1;
    let mut best = (f64::NEG_INFINITY, candidates[0]);
    for orient in candidates {
        let mut score = 0.0;
        for (pin, n) in &connected {
            let escape = orient.dir((-pin.toward_body.0, -pin.toward_body.1));
            if simple && ctx.external[*n] && ctx.power[*n].is_none() {
                score += match escape {
                    Dir::Left if pin.flow.takes() => 10.5,
                    Dir::Right if pin.flow.gives() => 10.5,
                    Dir::Left | Dir::Right => 10.0,
                    _ => 0.0,
                };
            }
            if connected.len() == 1 {
                // A pin on its own on a rail points at the rail's symbol.
                score += match (ctx.power[*n], escape) {
                    (Some(PowerKind::Supply), Dir::Up) | (Some(PowerKind::Ground), Dir::Down) => {
                        5.0
                    }
                    _ => 0.0,
                };
            }
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
fn part_texts(
    part: &netsmith_core::Part,
    body: Rect,
    side: Dir,
    gap: f64,
    pins: &[PlacedPin],
) -> (Text, Option<Text>) {
    let (cx, cy) = (
        (body.min.x + body.max.x) / 2.0,
        (body.min.y + body.max.y) / 2.0,
    );
    let line = TEXT_SIZE * 1.4;
    let (reference, value) = match side {
        Dir::Right | Dir::Left => {
            let (x, anchor) = if side == Dir::Right {
                (body.max.x + gap, Anchor::Start)
            } else {
                (body.min.x - gap, Anchor::End)
            };
            (
                text(&part.refdes, Pt::new(x, cy - 0.3), anchor),
                text(&part.value, Pt::new(x, cy - 0.3 + line), anchor),
            )
        }
        Dir::Up => {
            let (x, anchor) = text_column(part, pins, side, cx);
            (
                text(&part.refdes, Pt::new(x, body.min.y - gap - line), anchor),
                text(&part.value, Pt::new(x, body.min.y - gap), anchor),
            )
        }
        Dir::Down => {
            let (x, anchor) = text_column(part, pins, side, cx);
            (
                text(
                    &part.refdes,
                    Pt::new(x, body.max.y + gap + TEXT_SIZE),
                    anchor,
                ),
                text(
                    &part.value,
                    Pt::new(x, body.max.y + gap + TEXT_SIZE + line),
                    anchor,
                ),
            )
        }
    };
    let value = (!part.value.is_empty()).then_some(value);
    (reference, value)
}

/// Where the reference and the value go along a top or bottom side: centred on the body, or
/// else the nearest place to the centre where no wire that leaves that side runs through them
/// (a text is not written over a wire, and a wire does not turn aside for a text).
fn text_column(
    part: &netsmith_core::Part,
    pins: &[PlacedPin],
    side: Dir,
    centre: f64,
) -> (f64, Anchor) {
    // Clear space between a wire and a text.
    const CLEAR: f64 = 2.0;
    // As wide as the layout measures it and as wide as it is drawn, whichever is more.
    let em = |t: &str| crate::glyph::text_em(t).max(t.chars().count() as f64 * CHAR_WIDTH);
    let half = em(&part.refdes).max(em(&part.value)) * TEXT_SIZE / 2.0;
    let wires: Vec<f64> = pins
        .iter()
        .filter(|q| q.escape == side)
        .map(|q| q.at.x)
        .collect();
    let free = |c: f64| wires.iter().all(|&x| (x - c).abs() >= half + CLEAR - 1e-9);
    if free(centre) {
        return (centre, Anchor::Middle);
    }
    wires
        .iter()
        .flat_map(|&x| [x + CLEAR + half, x - CLEAR - half])
        .filter(|&c| free(c))
        .min_by(|a, b| (a - centre).abs().total_cmp(&(b - centre).abs()))
        .map_or((centre, Anchor::Middle), |c| (c, Anchor::Middle))
}

/// How far from the body the texts go on `side`: beyond the wires and the X of the pins that
/// leave that way.
fn text_gap(pins: &[PlacedPin], side: Dir) -> f64 {
    if pins.iter().any(|q| q.escape == side) {
        GRID + STUB + 2.0
    } else {
        // Past the number of a pin at the edge, which is written above its line.
        GRID + 0.6
    }
}

/// The side of a part's body its texts go on: one no pin leaves by, else the one fewest do.
fn text_side(pins: &[PlacedPin], body: Rect) -> Dir {
    let candidates = if body.height() > body.width() {
        [Dir::Right, Dir::Left, Dir::Up, Dir::Down]
    } else {
        [Dir::Up, Dir::Down, Dir::Right, Dir::Left]
    };
    let count = |d: Dir| pins.iter().filter(|q| q.escape == d).count();
    candidates
        .into_iter()
        .min_by_key(|&d| count(d))
        .unwrap_or(Dir::Right)
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
    let side = text_side(&pins, body);
    let (reference, value) = part_texts(part, body, side, text_gap(&pins, side), &pins);

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
