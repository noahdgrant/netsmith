//! What a part looks like: the drawing shapes and pins of its symbol, in the symbol's own
//! coordinates (millimetres, y up), read from the symbol library's S-expressions. A part
//! with no usable symbol gets a labelled box with its pins, so the viewer never fails on
//! a part it cannot draw.

use netsmith_core::{Design, Part};
use netsmith_kicad::{
    Mapping, PartPin, PinKind, PinType, Pinout, Sexp, Side, Symbol, SymbolLib, part_gives_power,
    side_of,
};

use crate::geom::{Dir, Orient, Pt, Rect, snap_up};
use crate::layout::{Anchor, Text, is_ground_name};

/// KiCad's width for a line drawn with width 0.
const DEFAULT_WIDTH: f64 = 0.1524;
/// The pin name offset KiCad uses when the symbol does not say.
const DEFAULT_NAME_OFFSET: f64 = 0.508;
const PIN_LENGTH: f64 = 2.54;
const PIN_PITCH: f64 = 2.54;
/// The space between two units of one symbol, drawn one below the other.
const UNIT_GAP: f64 = 5.08;
/// Characters are drawn about this wide, as a fraction of the text height.
pub const CHAR_WIDTH: f64 = 0.6;
/// How wide `text` is drawn, in text heights, in the page's sans serif (Arial's letter widths
/// by class): the width that decides whether two texts beside each other collide. The
/// layout's flat [`CHAR_WIDTH`] is short for capitals and long for lower case.
pub fn text_em(text: &str) -> f64 {
    text.chars()
        .map(|c| match c {
            'i' | 'j' | 'l' | '|' | '!' => 0.25,
            'f' | 't' | 'r' | 'I' => 0.32,
            'm' | 'w' => 0.78,
            'M' | 'W' => 0.88,
            'a'..='z' => 0.54,
            'A'..='Z' => 0.7,
            '0'..='9' => 0.56,
            '.' | ',' | ':' | ';' | '/' | ' ' => 0.28,
            '-' | '(' | ')' | '~' | '{' | '}' => 0.34,
            '_' | '+' | '=' | '#' => 0.58,
            _ => 0.65,
        })
        .sum()
}
/// The height of pin names and numbers in a symbol of the library.
pub const PIN_TEXT: f64 = 1.0;
/// The height of pin names and numbers in a box this crate makes for a symbol of many pins.
const DENSE_TEXT: f64 = 1.27;
/// A symbol with more pads than this is drawn as a table of labelled groups.
pub const MANY_PINS: usize = 60;
/// A box of at least this many pins says how many it has.
const COUNTED_PINS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    None,
    /// Filled with the line colour.
    Outline,
    /// Filled with the body colour.
    Background,
}

pub type P = (f64, f64);

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Rect {
        a: P,
        b: P,
        width: f64,
        fill: Fill,
    },
    Poly {
        pts: Vec<P>,
        width: f64,
        fill: Fill,
    },
    Circle {
        center: P,
        radius: f64,
        width: f64,
        fill: Fill,
    },
    /// An arc through three points.
    Arc {
        start: P,
        mid: P,
        end: P,
        width: f64,
        fill: Fill,
    },
    Text {
        at: P,
        text: String,
        size: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlyphPin {
    /// The pad: the text a net refers to the pin by.
    pub id: String,
    /// Empty when the symbol leaves the pin unnamed (`~`).
    pub name: String,
    /// Where a wire attaches.
    pub at: P,
    /// Unit vector from the connection point toward the body, symbol coordinates (y up).
    pub toward_body: P,
    pub length: f64,
    /// Whether the symbol says the pin takes or gives power or a signal.
    pub flow: Flow,
    /// What the symbol or the datasheet says the pin is: `input`, `power_in`, `no_connect`...
    pub kind: &'static str,
    /// A hidden pin that repeats the point and name of another: stock symbols stack the
    /// pads of one supply this way. It resolves a pad but is not drawn.
    pub hidden: bool,
}

/// Which way a pin passes current or a signal, when the symbol says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    In,
    Out,
    /// Both takes and gives.
    Bidirectional,
    /// Gives, and can let go of the net.
    TriState,
    Unspecified,
}

impl Flow {
    /// What a symbol's electrical type says about the flow.
    fn of(kind: PinType) -> Flow {
        match kind {
            PinType::Input | PinType::PowerIn => Flow::In,
            PinType::Output | PinType::PowerOut | PinType::OpenCollector | PinType::OpenEmitter => {
                Flow::Out
            }
            PinType::Bidirectional => Flow::Bidirectional,
            PinType::TriState => Flow::TriState,
            _ => Flow::Unspecified,
        }
    }

    /// Whether the pin gives a signal.
    pub fn gives(self) -> bool {
        matches!(self, Flow::Out | Flow::TriState)
    }

    /// Whether the pin takes a signal.
    pub fn takes(self) -> bool {
        self == Flow::In
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Glyph {
    /// The symbol this was read from; `None` for the fallback box.
    pub symbol: Option<String>,
    /// Why there is no symbol, for a tooltip.
    pub missing: Option<String>,
    pub shapes: Vec<Shape>,
    pub pins: Vec<GlyphPin>,
    pub show_numbers: bool,
    pub show_names: bool,
    pub name_offset: f64,
    /// The height of the pin names and numbers.
    pub pin_text: f64,
    /// A box this crate made for a symbol of several units or many pins: it stays upright.
    pub dense: bool,
    /// Set when the glyph is one unit of a symbol that has several, drawn as a box of its own
    /// (the unit's number, 1 for `A`): it stays as the library draws it.
    pub unit: Option<u32>,
}

impl Glyph {
    /// The glyph for `part`: its symbol if the library has one with every pad the design
    /// uses, otherwise a box. `used` lists the pads the design's nets refer to. `pinout`
    /// is what the part says about its pins, if known; the box lists them all.
    pub fn for_part(
        part: &Part,
        used: &[String],
        pinout: Option<&Pinout>,
        mapping: &Mapping,
        lib: &SymbolLib,
    ) -> Glyph {
        if part.kind == "Label" {
            return Glyph::text_box(part);
        }
        let mapped = match mapping.map_part(&part.path, &part.kind, part.package.as_deref()) {
            Ok(m) => m,
            Err(e) => return Glyph::fallback(part, used, pinout, e.to_string()),
        };
        let symbol = match lib.symbol_of(part, &mapped) {
            Ok(s) => s,
            Err(e) => return Glyph::fallback(part, used, pinout, e.to_string()),
        };
        let glyph = Glyph::drawn(part, &symbol);
        match used
            .iter()
            .find(|id| !glyph.pins.iter().any(|p| &p.id == *id))
        {
            Some(missing) => Glyph::fallback(
                part,
                used,
                pinout,
                format!("the symbol `{}` has no pin `{missing}`", symbol.lib_id()),
            ),
            None => glyph,
        }
    }

    /// How a symbol is drawn, as one part: the library's own drawing when it has one unit,
    /// at most [`MANY_PINS`] pads and names that do not run into each other; otherwise one
    /// box with every pin of every unit, by side (a symbol of up to [`MANY_PINS`] pads) or in
    /// labelled groups (more).
    fn drawn(part: &Part, symbol: &Symbol) -> Glyph {
        if symbol.pins().len() > MANY_PINS {
            return Glyph::grouped(part, symbol, None);
        }
        if symbol.units() > 1 {
            return Glyph::merged(part, symbol, None);
        }
        let glyph = Glyph::from_symbol(symbol);
        if glyph.texts_collide(Orient::IDENTITY) {
            Glyph::merged(part, symbol, None)
        } else {
            glyph
        }
    }

    /// One unit of a symbol as a box of its own: the library's own drawing of the unit with
    /// the pins in the library's order, or, where its names would run into each other or it
    /// has more than [`MANY_PINS`] pins, the same boxes as for a whole symbol.
    fn drawn_unit(part: &Part, symbol: &Symbol, unit: u32, first: u32) -> Glyph {
        let only = Some((unit, first));
        if symbol_entries(symbol, only).len() > MANY_PINS {
            return Glyph::grouped(part, symbol, only);
        }
        let mut glyph = Glyph::from_units(symbol, &[unit], first, true);
        // A unit whose pins one move cannot put on the grid is drawn as a box made here.
        if !glyph.on_grid() || glyph.texts_collide(Orient::IDENTITY) {
            Glyph::merged(part, symbol, only)
        } else {
            glyph
        }
    }

    /// Moves the drawing so that its pins are on the sheet's grid, when one move does it, and
    /// says whether they are: a unit of a library symbol is drawn where the symbol put it, and
    /// the unit that was below another was moved by a distance that is not a grid step.
    fn on_grid(&mut self) -> bool {
        const STEP: f64 = 1.27;
        let Some(first) = self.pins.first() else {
            return true;
        };
        let snapped = |v: f64| (v / STEP).round() * STEP;
        let (dx, dy) = (
            snapped(first.at.0) - first.at.0,
            snapped(first.at.1) - first.at.1,
        );
        let aligned = self.pins.iter().all(|p| {
            (snapped(p.at.0 + dx) - (p.at.0 + dx)).abs() < 1e-3
                && (snapped(p.at.1 + dy) - (p.at.1 + dy)).abs() < 1e-3
        });
        if !aligned {
            return false;
        }
        if dx.abs() < 1e-6 && dy.abs() < 1e-6 {
            return true;
        }
        self.shapes = std::mem::take(&mut self.shapes)
            .into_iter()
            .map(|s| moved(s, dx, dy))
            .collect();
        for p in &mut self.pins {
            p.at = (p.at.0 + dx, p.at.1 + dy);
        }
        true
    }

    /// The boxes of a part whose symbol has several units: one for each unit that has a pin,
    /// in the unit's order, as `(unit number, glyph)`. A pin belongs to one box (a pin of unit
    /// 0, shared by all, to the first). None when the part is drawn as one box: its symbol has
    /// one unit with pins, or there is no symbol, or it lacks a pad the design uses.
    pub fn unit_glyphs(
        part: &Part,
        used: &[String],
        mapping: &Mapping,
        lib: &SymbolLib,
    ) -> Option<Vec<(u32, Glyph)>> {
        if part.kind == "Label" {
            return None;
        }
        let mapped = mapping.map_part(&part.path, &part.kind, part.package.as_deref()).ok()?;
        let symbol = lib.symbol_of(part, &mapped).ok()?;
        if symbol.units() < 2 {
            return None;
        }
        let has_pins = |unit: u32| symbol.pins().iter().any(|p| p.unit == unit);
        let mut units: Vec<u32> = (1..=symbol.units()).filter(|&u| has_pins(u)).collect();
        // Shared pins (unit 0) go with the first unit that has a pin; with none of its own, the
        // first unit.
        let first = units.first().copied().unwrap_or(1);
        if units.is_empty() && symbol.pins().iter().any(|p| p.unit == 0) {
            units.push(first);
        }
        if units.len() < 2 {
            return None;
        }
        let glyphs: Vec<(u32, Glyph)> = units
            .iter()
            .map(|&u| {
                let mut glyph = Glyph::drawn_unit(part, &symbol, u, first);
                glyph.unit = Some(u);
                (u, glyph)
            })
            .collect();
        used.iter()
            .all(|id| {
                glyphs
                    .iter()
                    .any(|(_, g)| g.pins.iter().any(|p| &p.id == id))
            })
            .then_some(glyphs)
    }

    /// The symbol's own drawing: its first unit, then the others each below the one before.
    /// The drawing shared by all units (unit 0) goes with the first.
    fn from_symbol(symbol: &Symbol) -> Glyph {
        let units: Vec<u32> = (1..=symbol.units()).collect();
        Glyph::from_units(symbol, &units, 1, false)
    }

    /// The drawing of `units` of a symbol, each below the one before. The pins of unit 0 (shared
    /// by all) are drawn with the unit `first`; the drawing of unit 0 is drawn with it too, or
    /// with every unit when `shared_drawing_everywhere` (a unit that stands alone).
    fn from_units(
        symbol: &Symbol,
        units: &[u32],
        first: u32,
        shared_drawing_everywhere: bool,
    ) -> Glyph {
        let def = symbol.definition();
        let mut shapes = Vec::new();
        let mut pins: Vec<GlyphPin> = Vec::new();
        let mut hidden_in_symbol: Vec<bool> = Vec::new();
        let mut floor: Option<f64> = None;
        for &unit in units {
            let mut unit_shapes = Vec::new();
            for part in def.find_all("symbol") {
                let Some((number, style)) =
                    unit_of(part.args().first().and_then(Sexp::text).unwrap_or(""))
                else {
                    continue;
                };
                let shared = number == 0 && (shared_drawing_everywhere || unit == first);
                if style > 1 || !(number == unit || shared) {
                    continue;
                }
                unit_shapes.extend(
                    part.args()
                        .iter()
                        .filter(|i| i.head() != Some("pin"))
                        .filter_map(read_shape),
                );
            }
            let unit_pins: Vec<GlyphPin> = symbol
                .pins()
                .iter()
                .filter(|p| p.unit == unit || (p.unit == 0 && unit == first))
                .map(|p| GlyphPin {
                    id: p.number.clone(),
                    name: if p.is_unnamed() {
                        String::new()
                    } else {
                        p.name.clone()
                    },
                    at: (p.x, p.y),
                    toward_body: {
                        let r = p.angle.to_radians();
                        (round_unit(r.cos()), round_unit(r.sin()))
                    },
                    length: p.length,
                    flow: Flow::of(p.kind),
                    kind: p.kind.name(),
                    hidden: false,
                })
                .collect();
            // Where this unit's top and bottom edges are, before it is moved.
            let (mut top, mut bottom) = (f64::MIN, f64::MAX);
            for y in unit_shapes
                .iter()
                .flat_map(shape_ys)
                .chain(unit_pins.iter().map(|p| p.at.1))
            {
                top = top.max(y);
                bottom = bottom.min(y);
            }
            if top < bottom {
                (top, bottom) = (0.0, 0.0);
            }
            let shift = floor.map_or(0.0, |f| f - UNIT_GAP - top);
            floor = Some(shift + bottom);
            shapes.extend(unit_shapes.into_iter().map(|s| shifted(s, shift)));
            hidden_in_symbol.extend(
                symbol
                    .pins()
                    .iter()
                    .filter(|p| p.unit == unit || (p.unit == 0 && unit == first))
                    .map(|p| p.hidden),
            );
            pins.extend(unit_pins.into_iter().map(|mut p| {
                p.at.1 += shift;
                p
            }));
        }
        mark_duplicates(&mut pins, &hidden_in_symbol);
        let flag = |list: &str| {
            def.find(list)
                .and_then(|l| l.find("hide"))
                .and_then(|h| h.args().first())
                .and_then(Sexp::text)
                != Some("yes")
        };
        let name_offset = def
            .find("pin_names")
            .and_then(|l| l.find("offset"))
            .and_then(|o| num(o.args().first()?))
            .unwrap_or(DEFAULT_NAME_OFFSET);
        Glyph {
            symbol: Some(symbol.lib_id().to_string()),
            missing: None,
            shapes,
            pins,
            show_numbers: flag("pin_numbers"),
            show_names: flag("pin_names"),
            name_offset,
            pin_text: PIN_TEXT,
            dense: false,
            unit: None,
        }
    }

    /// A box for a part with no usable symbol. With the part's pinout it has every pin of
    /// the package, named from the datasheet: supplies on top (a supply that takes power, of a
    /// part that also gives power, on the left), grounds below, inputs on the left, outputs on
    /// the right and the no-connect pins last. Without, it has one pin for each pad the design
    /// uses, split between its left and right sides.
    fn fallback(part: &Part, used: &[String], pinout: Option<&Pinout>, why: String) -> Glyph {
        let mut sides = Sides::default();
        match pinout {
            Some(pinout) => {
                let part_gives = part_gives_power(&pinout.pins);
                for pin in &pinout.pins {
                    sides.put(pin, part_gives);
                }
                // A pad the design uses that the pinout lacks still gets a pin.
                for pad in used {
                    if pinout.pin_on(pad).is_none() {
                        sides.extra.push(pad.clone());
                    }
                }
                sides.extra.sort_by(|a, b| natural_order(a, b));
                sides.extra.dedup();
            }
            None => {
                let mut pads: Vec<&String> = used.iter().collect();
                pads.sort_by(|a, b| natural_order(a, b));
                pads.dedup();
                let right = pads.len() / 2;
                for (i, pad) in pads.iter().enumerate() {
                    let entry = Entry::pad(pad);
                    if i < pads.len() - right {
                        sides.left.push(entry);
                    } else {
                        sides.right.push(entry);
                    }
                }
            }
        }
        sides.finish(part, Origin::Missing(why))
    }

    /// A `Label` is a line of silkscreen text with no pins and no KiCad symbol: a box that
    /// holds the text it prints, which is the part's value.
    fn text_box(part: &Part) -> Glyph {
        let half_width = snap_up(
            (part.value.chars().count() as f64 * 1.27 * CHAR_WIDTH + 2.54) / 2.0,
            1.27,
        )
        .max(3.81);
        Glyph {
            symbol: None,
            missing: None,
            shapes: vec![
                Shape::Rect {
                    a: (-half_width, 1.905),
                    b: (half_width, -1.905),
                    width: 0.254,
                    fill: Fill::Background,
                },
                Shape::Text {
                    at: (0.0, 0.0),
                    text: part.value.clone(),
                    size: 1.27,
                },
            ],
            pins: Vec::new(),
            show_numbers: false,
            show_names: false,
            name_offset: DEFAULT_NAME_OFFSET,
            pin_text: PIN_TEXT,
            dense: false,
            unit: None,
        }
    }

    /// The number and the name written at `pin`, whose connection point is `at` and whose
    /// line runs toward `toward` (the way to the body), as they are drawn.
    pub fn pin_texts(&self, pin: &GlyphPin, at: Pt, toward: Dir) -> (Option<Text>, Option<Text>) {
        self.pin_texts_in(pin, at, toward, true)
    }

    /// [`pin_texts`](Self::pin_texts) with every text across, none written upward: what an
    /// orientation is judged by (a part is turned so that no name has to be).
    fn pin_texts_in(
        &self,
        pin: &GlyphPin,
        at: Pt,
        toward: Dir,
        turn: bool,
    ) -> (Option<Text>, Option<Text>) {
        let inner = at.step(toward, pin.length);
        let number = self.show_numbers.then(|| {
            let mid = Pt::new((at.x + inner.x) / 2.0, (at.y + inner.y) / 2.0);
            if turn && toward.is_vertical() && self.crowded(pin, toward, |p| &p.id) {
                // Numbers wider than the pins are apart: written upward beside the line.
                Text::new(&pin.id, Pt::new(mid.x - 0.35, mid.y), Anchor::Middle).turned()
            } else if toward.is_vertical() {
                Text::new(&pin.id, Pt::new(mid.x - 0.4, mid.y + 0.35), Anchor::End)
            } else {
                // Clear of the X on an open pin, which reaches half a grid step either way.
                Text::new(&pin.id, Pt::new(mid.x, mid.y - 0.5), Anchor::Middle)
            }
        });
        let crowded = turn && self.crowded(pin, toward, |p| &p.name);
        let name = (self.show_names && !pin.name.is_empty()).then(|| {
            let offset = self.name_offset;
            // Names of pins side by side on a top or bottom edge, nearer than the names are
            // wide, are written upward inside the body, as KiCad draws them.
            let shift = self.pin_text * 0.35;
            match toward {
                Dir::Down if crowded => Text::new(
                    &pin.name,
                    Pt::new(inner.x + shift, inner.y + offset),
                    Anchor::End,
                )
                .turned(),
                Dir::Up if crowded => Text::new(
                    &pin.name,
                    Pt::new(inner.x + shift, inner.y - offset),
                    Anchor::Start,
                )
                .turned(),
                Dir::Right => Text::new(
                    &pin.name,
                    Pt::new(inner.x + offset, inner.y + 0.35),
                    Anchor::Start,
                ),
                Dir::Left => Text::new(
                    &pin.name,
                    Pt::new(inner.x - offset, inner.y + 0.35),
                    Anchor::End,
                ),
                Dir::Down => Text::new(
                    &pin.name,
                    Pt::new(inner.x, inner.y + offset + 1.0),
                    Anchor::Middle,
                ),
                Dir::Up => Text::new(
                    &pin.name,
                    Pt::new(inner.x, inner.y - offset - 0.2),
                    Anchor::Middle,
                ),
            }
        });
        (number, name)
    }

    /// Whether the text of `pin` (its name or its number), which will be written along a top
    /// or bottom edge (`toward` is vertical on the sheet), is nearer than the two texts are
    /// wide to the text of a pin beside it on the same edge. The distance along an edge does
    /// not change with the part's turn.
    fn crowded(&self, pin: &GlyphPin, toward: Dir, text: fn(&GlyphPin) -> &String) -> bool {
        if !toward.is_vertical() || text(pin).is_empty() {
            return false;
        }
        let width = |p: &GlyphPin| text_em(text(p)) * self.pin_text;
        // Along the edge, and how far out the edge is, in the symbol's own coordinates.
        let place = |p: &GlyphPin| {
            if p.toward_body.0 == 0.0 {
                (p.at.0, p.at.1 + p.toward_body.1 * p.length)
            } else {
                (p.at.1, p.at.0 + p.toward_body.0 * p.length)
            }
        };
        let (along, across) = place(pin);
        self.pins.iter().any(|q| {
            let (a, c) = place(q);
            !q.hidden
                && !text(q).is_empty()
                && !std::ptr::eq(q, pin)
                && q.toward_body == pin.toward_body
                && (c - across).abs() < 0.01
                && (a - along).abs() < (width(q) + width(pin)) / 2.0 + 0.3
        })
    }

    /// Whether, with the part turned by `orient`, two of the texts at its pins would be
    /// written over each other.
    pub fn texts_collide(&self, orient: Orient) -> bool {
        let mut boxes: Vec<Rect> = Vec::new();
        for pin in self.pins.iter().filter(|p| !p.hidden) {
            let at = orient.apply(pin.at);
            let toward = orient.dir(pin.toward_body);
            let (number, name) = self.pin_texts_in(pin, at, toward, false);
            boxes.extend(
                number
                    .into_iter()
                    .chain(name)
                    .map(|t| t.extent_at(self.pin_text)),
            );
        }
        boxes.iter().enumerate().any(|(i, a)| {
            boxes[i + 1..].iter().any(|b| {
                a.min.x < b.max.x - 0.05
                    && b.min.x < a.max.x - 0.05
                    && a.min.y < b.max.y - 0.05
                    && b.min.y < a.max.y - 0.05
            })
        })
    }

    /// The drawn body (the shapes, without the pins), in symbol coordinates, if it has any.
    pub fn body(&self) -> Option<Rect> {
        let mut out: Option<Rect> = None;
        let add = |r: &mut Option<Rect>, p: P| match r {
            Some(b) => b.include(Pt::new(p.0, p.1)),
            None => *r = Some(Rect::around(Pt::new(p.0, p.1))),
        };
        for s in &self.shapes {
            match s {
                Shape::Rect { a, b, .. } => {
                    add(&mut out, *a);
                    add(&mut out, *b);
                }
                Shape::Poly { pts, .. } => pts.iter().for_each(|p| add(&mut out, *p)),
                Shape::Circle { center, radius, .. } => {
                    add(&mut out, (center.0 - radius, center.1 - radius));
                    add(&mut out, (center.0 + radius, center.1 + radius));
                }
                Shape::Arc {
                    start, mid, end, ..
                } => {
                    add(&mut out, *start);
                    add(&mut out, *mid);
                    add(&mut out, *end);
                }
                Shape::Text { .. } => {}
            }
        }
        out
    }

    pub fn pin(&self, id: &str) -> Option<&GlyphPin> {
        self.pins.iter().find(|p| p.id == id)
    }

    /// The body and the pins, in symbol coordinates (y up), as a box. Never empty.
    pub fn bounds(&self) -> Rect {
        let mut r = Rect::around(Pt::new(0.0, 0.0));
        let mut add = |p: P| r.include(Pt::new(p.0, p.1));
        for s in &self.shapes {
            match s {
                Shape::Rect { a, b, .. } => {
                    add(*a);
                    add(*b);
                }
                Shape::Poly { pts, .. } => pts.iter().for_each(|p| add(*p)),
                Shape::Circle { center, radius, .. } => {
                    add((center.0 - radius, center.1 - radius));
                    add((center.0 + radius, center.1 + radius));
                }
                Shape::Arc {
                    start, mid, end, ..
                } => {
                    add(*start);
                    add(*mid);
                    add(*end);
                }
                Shape::Text { .. } => {}
            }
        }
        for p in &self.pins {
            add(p.at);
        }
        r
    }
}

/// Where a box made by this crate comes from.
enum Origin {
    /// There is no symbol; the string says why.
    Missing(String),
    /// A symbol of the library, redrawn as one box.
    Symbol(String),
}

/// One pin of a symbol, with the facts that place it in a box.
struct SymEntry {
    entry: Entry,
    /// The way the pin line runs toward the body, in degrees.
    angle: f64,
    unit: u32,
    x: f64,
    y: f64,
    kind: PinType,
}

/// The pins of a symbol, every unit, in the symbol's own order. A hidden pin that repeats a
/// shown pin's name (stock symbols stack the pads of one supply so) is one more pad of that
/// pin.
///
/// With `only` (a unit, and the unit that carries the pins shared by all), the pins of that
/// unit alone.
fn symbol_entries(symbol: &Symbol, only: Option<(u32, u32)>) -> Vec<SymEntry> {
    let mut out: Vec<SymEntry> = Vec::new();
    for p in symbol.pins().iter().filter(|p| {
        only.is_none_or(|(unit, first)| p.unit == unit || (p.unit == 0 && unit == first))
    }) {
        let name = if p.is_unnamed() {
            String::new()
        } else {
            p.name.clone()
        };
        if p.hidden
            && !name.is_empty()
            && let Some(twin) = out.iter_mut().find(|e| e.entry.name == name)
        {
            twin.entry.pads.push(p.number.clone());
            continue;
        }
        out.push(SymEntry {
            entry: Entry {
                pads: vec![p.number.clone()],
                name,
                flow: Flow::of(p.kind),
                kind: p.kind.name(),
            },
            angle: p.angle,
            unit: p.unit,
            x: p.x,
            y: p.y,
            kind: p.kind,
        });
    }
    out
}

/// What a pin of a big symbol is for, which decides its group.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Class {
    Power,
    Ground,
    NoConnect,
    Named(String),
}

fn classify(e: &SymEntry) -> Class {
    let name = e.entry.name.to_ascii_uppercase();
    if e.kind == PinType::NoConnect || name == "NC" || name == "DNC" {
        return Class::NoConnect;
    }
    if is_ground_name(&e.entry.name) || name.starts_with("AGND") || name.starts_with("DGND") {
        return Class::Ground;
    }
    let supply = ["VDD", "VCC", "VBUS", "VIN", "VBAT", "VOUT", "AVDD", "DVDD"]
        .iter()
        .any(|v| name.starts_with(v));
    if matches!(e.kind, PinType::PowerIn | PinType::PowerOut) || supply {
        return Class::Power;
    }
    Class::Named(group_key(&e.entry.name))
}

/// The group a pin name belongs to: `P0.16` is in `P0`, `GPIO12` in `GPIO`, `USB_DP` in
/// `USB`; `A/B` is taken as `A`.
fn group_key(name: &str) -> String {
    let first = name.split('/').next().unwrap_or(name);
    let key = match first.split_once('.') {
        Some((head, _)) => head,
        None => first.split('_').next().unwrap_or(first),
    };
    let trimmed = if first.contains('.') {
        key
    } else {
        key.trim_end_matches(|c: char| c.is_ascii_digit())
    };
    if trimmed.is_empty() {
        "Other".to_string()
    } else {
        trimmed.to_string()
    }
}

/// A row of a table of pins: a heading, or a pin.
enum Row {
    Heading(String),
    Pin(Entry),
}

impl Glyph {
    /// One box for a symbol of several units (or one whose texts would run together): every
    /// pin of every unit on the side its symbol puts it, in the symbol's own order (a unit
    /// after the one before, top to bottom, left to right), and the box as large as the
    /// names need.
    fn merged(part: &Part, symbol: &Symbol, only: Option<(u32, u32)>) -> Glyph {
        let mut entries = symbol_entries(symbol, only);
        entries.sort_by(|a, b| {
            let (ax, ay) = (a.x, a.y);
            let key = |e: &SymEntry, x: f64, y: f64| {
                let vertical = (e.angle.rem_euclid(180.0) - 90.0).abs() < 45.0;
                (e.unit, if vertical { x } else { -y })
            };
            let (ka, kb) = (key(a, ax, ay), key(b, b.x, b.y));
            ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
        });
        let mut sides = Sides::default();
        for e in entries {
            let a = e.angle.rem_euclid(360.0);
            let entry = e.entry;
            // The pin line runs toward the body: 0 degrees is a pin on the left.
            if !(45.0..315.0).contains(&a) {
                sides.left.push(entry);
            } else if a < 135.0 {
                sides.bottom.push(entry);
            } else if a < 225.0 {
                sides.right.push(entry);
            } else {
                sides.top.push(entry);
            }
        }
        sides.finish(part, Origin::Symbol(symbol.lib_id().to_string()))
    }

    /// A symbol of very many pins as one box with two columns of pins in labelled groups:
    /// the supplies, the grounds, then the others by the start of their names, the pins that
    /// connect to nothing last. The box says how many pins it has.
    fn grouped(part: &Part, symbol: &Symbol, only: Option<(u32, u32)>) -> Glyph {
        let entries = symbol_entries(symbol, only);
        let pin_count = entries.len();
        let mut power = Vec::new();
        let mut ground = Vec::new();
        let mut open = Vec::new();
        let mut named: Vec<(String, Vec<Entry>)> = Vec::new();
        for e in entries {
            match classify(&e) {
                Class::Power => power.push(e.entry),
                Class::Ground => ground.push(e.entry),
                Class::NoConnect => open.push(e.entry),
                Class::Named(key) => match named.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, list)) => list.push(e.entry),
                    None => named.push((key, vec![e.entry])),
                },
            }
        }
        // A group of one or two pins is not worth a heading.
        let mut other = Vec::new();
        let mut groups: Vec<(String, Vec<Entry>)> = Vec::new();
        for (key, list) in named {
            if list.len() < 3 {
                other.extend(list);
            } else {
                groups.push((key, list));
            }
        }
        // The supplies are in a row of pins on top and the grounds in one at the bottom, as a
        // person draws a big part; the signals are in two columns, the groups in order, the
        // first half in the left column and the rest, and the pins that connect to nothing,
        // in the right.
        let mut signals: Vec<(String, Vec<Entry>)> = groups;
        if !other.is_empty() {
            signals.push(("Other".into(), other));
        }
        let rows_of = |list: &Vec<Entry>| list.len() + 1;
        let total_rows = rows_of(&open) + signals.iter().map(|(_, l)| rows_of(l)).sum::<usize>();
        let half = total_rows.div_ceil(2);
        let mut left: Vec<Row> = Vec::new();
        let mut right: Vec<Row> = Vec::new();
        let put = |rows: &mut Vec<Row>, key: &str, list: Vec<Entry>| {
            if list.is_empty() {
                return;
            }
            rows.push(Row::Heading(format!("{key} ({})", list.len())));
            rows.extend(list.into_iter().map(Row::Pin));
        };
        let mut used = 0;
        let mut left_full = false;
        for (key, list) in signals {
            let need = rows_of(&list);
            if !left_full && (used + need <= half || used == 0) {
                used += need;
                put(&mut left, &key, list);
            } else {
                left_full = true;
                put(&mut right, &key, list);
            }
        }
        put(&mut right, "Not connected", open);

        let size = DENSE_TEXT;
        let char_width = size * CHAR_WIDTH;
        let widest = |rows: &[Row]| {
            rows.iter()
                .map(|r| match r {
                    Row::Heading(h) => h.chars().count(),
                    Row::Pin(e) => e.name.chars().count(),
                })
                .max()
                .unwrap_or(0) as f64
                * char_width
        };
        let title = format!("{}, {pin_count} pins", part.kind);
        let title_width = title.chars().count() as f64 * 1.27 * CHAR_WIDTH;
        // A row of supplies or grounds: the pins as far apart as their names need.
        let row_pitch = |list: &[Entry]| {
            let longest = list
                .iter()
                .map(|e| e.name.chars().count())
                .max()
                .unwrap_or(0);
            (((longest as f64 * char_width + 0.2) / 1.27 - 1e-6).ceil() * 1.27).max(2.54)
        };
        let (top_pitch, bottom_pitch) = (row_pitch(&power), row_pitch(&ground));
        let row_width = |list: &[Entry], pitch: f64| list.len() as f64 * pitch;
        let columns = widest(&left) + widest(&right) + 2.0 * 2.54 + 2.54;
        let half_width = snap_up(
            columns
                .max(title_width + 2.54)
                .max(row_width(&power, top_pitch) + 2.54)
                .max(row_width(&ground, bottom_pitch) + 2.54)
                / 2.0,
            1.27,
        )
        .max(7.62);
        let height_rows = left.len().max(right.len());
        // A band at the top for the title and the supplies' names, a row at the bottom for the
        // grounds' names, then one row for each pin and heading.
        let top_rows = usize::from(!power.is_empty());
        let bottom_rows = usize::from(!ground.is_empty());
        let half_height = snap_up(
            ((height_rows + 2 + top_rows + bottom_rows) as f64) * PIN_PITCH / 2.0,
            1.27,
        );
        let first_row = half_height - 3.81 - top_rows as f64 * PIN_PITCH;

        let mut pins = Vec::new();
        let mut shapes = vec![
            Shape::Rect {
                a: (-half_width, half_height),
                b: (half_width, -half_height),
                width: 0.254,
                fill: Fill::Background,
            },
            Shape::Text {
                at: (0.0, half_height - 1.9 - top_rows as f64 * PIN_PITCH),
                text: title,
                size: 1.27,
            },
        ];
        for (rows_of_pins, y, pitch, toward) in [
            (power, half_height + PIN_LENGTH, top_pitch, -1.0),
            (ground, -half_height - PIN_LENGTH, bottom_pitch, 1.0),
        ] {
            let count = rows_of_pins.len() as f64;
            for (i, e) in rows_of_pins.into_iter().enumerate() {
                let x = snap_up((i as f64 - (count - 1.0) / 2.0) * pitch, 1.27);
                for (k, pad) in e.pads.iter().enumerate() {
                    pins.push(GlyphPin {
                        id: pad.clone(),
                        name: e.name.clone(),
                        at: (x, y),
                        toward_body: (0.0, toward),
                        length: PIN_LENGTH,
                        flow: e.flow,
                        kind: e.kind,
                        hidden: k > 0,
                    });
                }
            }
        }
        for (side, rows) in [(-1.0, &left), (1.0, &right)] {
            for (i, row) in rows.iter().enumerate() {
                let y = first_row - i as f64 * PIN_PITCH;
                match row {
                    Row::Heading(h) => {
                        let w = h.chars().count() as f64 * char_width;
                        shapes.push(Shape::Text {
                            at: (side * (half_width - 1.0 - w / 2.0), y),
                            text: h.clone(),
                            size,
                        });
                    }
                    Row::Pin(e) => {
                        for (k, pad) in e.pads.iter().enumerate() {
                            pins.push(GlyphPin {
                                id: pad.clone(),
                                name: e.name.clone(),
                                at: (side * (half_width + PIN_LENGTH), y),
                                toward_body: (-side, 0.0),
                                length: PIN_LENGTH,
                                flow: e.flow,
                                kind: e.kind,
                                hidden: k > 0,
                            });
                        }
                    }
                }
            }
        }
        Glyph {
            symbol: Some(symbol.lib_id().to_string()),
            missing: None,
            shapes,
            pins,
            show_numbers: true,
            show_names: true,
            name_offset: DEFAULT_NAME_OFFSET,
            pin_text: DENSE_TEXT,
            dense: true,
            unit: None,
        }
    }
}

/// One pin of the fallback box: all its pads, the name printed beside it.
struct Entry {
    pads: Vec<String>,
    name: String,
    flow: Flow,
    kind: &'static str,
}

impl Entry {
    /// A pin known only by its pad.
    fn pad(pad: &str) -> Entry {
        Entry {
            pads: vec![pad.to_string()],
            name: pad.to_string(),
            flow: Flow::Unspecified,
            kind: "",
        }
    }

    /// A pin of the part: its datasheet name, and the function the board chose.
    fn of(pin: &PartPin) -> Entry {
        Entry {
            pads: pin.pads.clone(),
            name: match &pin.function {
                Some(function) => format!("{} {function}", pin.datasheet),
                None => pin.datasheet.clone(),
            },
            flow: match pin.kind {
                PinKind::Power if pin.gives_power() => Flow::Out,
                PinKind::Input | PinKind::Power => Flow::In,
                // As KiCad's `open_collector` reads (`Flow::of`).
                PinKind::Output | PinKind::OpenDrain => Flow::Out,
                PinKind::TriState => Flow::TriState,
                PinKind::Bidirectional => Flow::Bidirectional,
                PinKind::Passive | PinKind::NoConnect => Flow::Unspecified,
            },
            kind: match pin.kind {
                PinKind::Input => "input",
                PinKind::Output => "output",
                PinKind::Bidirectional => "bidirectional",
                PinKind::Power => "power",
                PinKind::Passive => "passive",
                PinKind::NoConnect => "no_connect",
                PinKind::TriState => "tri_state",
                PinKind::OpenDrain => "open_drain",
            },
        }
    }
}

/// The pins of the fallback box, by the side of the box they are on.
#[derive(Default)]
struct Sides {
    top: Vec<Entry>,
    bottom: Vec<Entry>,
    left: Vec<Entry>,
    right: Vec<Entry>,
    /// Bidirectional pins and no-connect pins go to whichever side is shorter, after the
    /// rest, the no-connect ones last.
    either: Vec<Entry>,
    no_connect: Vec<Entry>,
    /// Pads the design uses that the pinout does not list.
    extra: Vec<String>,
}

impl Sides {
    fn put(&mut self, pin: &PartPin, part_gives: bool) {
        let entry = Entry::of(pin);
        // The side is the rule of the symbol netsmith draws from a pin table (ADR 0097).
        match side_of(pin, part_gives) {
            Side::Bottom => self.bottom.push(entry),
            Side::Top => self.top.push(entry),
            Side::Left => self.left.push(entry),
            Side::Right => self.right.push(entry),
            Side::Either => self.either.push(entry),
            Side::NoConnect => self.no_connect.push(entry),
        }
    }

    fn finish(mut self, part: &Part, origin: Origin) -> Glyph {
        let extra: Vec<Entry> = self.extra.iter().map(|p| Entry::pad(p)).collect();
        let rest = std::mem::take(&mut self.either)
            .into_iter()
            .chain(extra)
            .chain(std::mem::take(&mut self.no_connect));
        for entry in rest.collect::<Vec<_>>() {
            if self.left.len() <= self.right.len() {
                self.left.push(entry);
            } else {
                self.right.push(entry);
            }
        }

        let chars = |side: &[Entry]| {
            side.iter()
                .map(|e| e.name.chars().count())
                .max()
                .unwrap_or(0) as f64
        };
        let char_width = 1.27 * CHAR_WIDTH;
        let total = self.left.len()
            + self.right.len()
            + self.top.len()
            + self.bottom.len()
            + self.either.len()
            + self.no_connect.len()
            + self.extra.len();
        let counted = matches!(origin, Origin::Symbol(_)) && total >= COUNTED_PINS;
        let title = if counted {
            format!("{}, {total} pins", part.kind)
        } else {
            part.kind.clone()
        };
        let label = format!("{} {}", title, part.value);
        let wide = label.chars().count() as f64 * char_width + 2.54;
        let names = (chars(&self.left) + chars(&self.right)) * char_width + 2.54 * 2.0;
        // Pins along the top and bottom are far enough apart for their names to sit side
        // by side.
        let across = snap_up(
            chars(&self.top).max(chars(&self.bottom)) * char_width + 1.27,
            1.27,
        )
        .max(PIN_PITCH);
        let columns = self.top.len().max(self.bottom.len()) as f64;
        let half_width = snap_up(
            wide.max(names).max(10.16).max(columns * across + 2.54) / 2.0,
            1.27,
        );
        let rows = self.left.len().max(self.right.len()).max(1);
        let top = rows as f64 * PIN_PITCH / 2.0;
        let vertical = !(self.top.is_empty() && self.bottom.is_empty());
        let half_height = if vertical {
            (top + 1.27).max(3.81)
        } else {
            top + 1.27
        };

        let mut pins = Vec::new();
        let mut add = |entry: &Entry, at: P, toward_body: P| {
            for (i, pad) in entry.pads.iter().enumerate() {
                pins.push(GlyphPin {
                    id: pad.clone(),
                    name: entry.name.clone(),
                    at,
                    toward_body,
                    length: PIN_LENGTH,
                    flow: entry.flow,
                    kind: entry.kind,
                    // The other pads of a pin sit on the first's point.
                    hidden: i > 0,
                });
            }
        };
        for (side, entries) in [(-1.0, &self.left), (1.0, &self.right)] {
            for (row, entry) in entries.iter().enumerate() {
                let at = (
                    side * (half_width + PIN_LENGTH),
                    top - 1.27 - row as f64 * PIN_PITCH,
                );
                add(entry, at, (-side, 0.0));
            }
        }
        for (edge, entries) in [(1.0, &self.top), (-1.0, &self.bottom)] {
            // On the grid: an even number of pins, a step apart, would be half a step off it.
            let first = snap_up(
                -(entries.len().saturating_sub(1) as f64) * across / 2.0,
                1.27,
            );
            for (i, entry) in entries.iter().enumerate() {
                let at = (first + i as f64 * across, edge * (half_height + PIN_LENGTH));
                add(entry, at, (0.0, -edge));
            }
        }
        let shapes = vec![
            Shape::Rect {
                a: (-half_width, half_height),
                b: (half_width, -half_height),
                width: 0.254,
                fill: Fill::Background,
            },
            Shape::Text {
                at: (0.0, if vertical { 0.0 } else { half_height - 1.9 }),
                text: title,
                size: 1.27,
            },
        ];
        let origin_kind_symbol = matches!(origin, Origin::Symbol(_));
        let (symbol, missing) = match origin {
            Origin::Missing(why) => (None, Some(why)),
            Origin::Symbol(id) => (Some(id), None),
        };
        Glyph {
            symbol,
            missing,
            shapes,
            pins,
            show_numbers: origin_kind_symbol,
            show_names: true,
            name_offset: DEFAULT_NAME_OFFSET,
            pin_text: if origin_kind_symbol {
                DENSE_TEXT
            } else {
                PIN_TEXT
            },
            dense: false,
            unit: None,
        }
    }
}

/// `R_0_1` and `R_1_1` hold the drawing: the unit (0 is shared by all) and the body style
/// (2 is the alternate drawing, which is not shown).
fn unit_of(name: &str) -> Option<(u32, u32)> {
    let mut tail = name.rsplit('_');
    let style = tail.next()?.parse().ok()?;
    let unit = tail.next()?.parse().ok()?;
    Some((unit, style))
}

/// The y coordinates a shape reaches.
fn shape_ys(shape: &Shape) -> Vec<f64> {
    match shape {
        Shape::Rect { a, b, .. } => vec![a.1, b.1],
        Shape::Poly { pts, .. } => pts.iter().map(|p| p.1).collect(),
        Shape::Circle { center, radius, .. } => vec![center.1 - radius, center.1 + radius],
        Shape::Arc {
            start, mid, end, ..
        } => vec![start.1, mid.1, end.1],
        Shape::Text { at, .. } => vec![at.1],
    }
}

/// The shape moved down by `-dy` (up by `dy`).
fn shifted(shape: Shape, dy: f64) -> Shape {
    moved(shape, 0.0, dy)
}

/// The shape moved by `(dx, dy)`.
fn moved(shape: Shape, dx: f64, dy: f64) -> Shape {
    let up = |p: P| (p.0 + dx, p.1 + dy);
    match shape {
        Shape::Rect { a, b, width, fill } => Shape::Rect {
            a: up(a),
            b: up(b),
            width,
            fill,
        },
        Shape::Poly { pts, width, fill } => Shape::Poly {
            pts: pts.into_iter().map(up).collect(),
            width,
            fill,
        },
        Shape::Circle {
            center,
            radius,
            width,
            fill,
        } => Shape::Circle {
            center: up(center),
            radius,
            width,
            fill,
        },
        Shape::Arc {
            start,
            mid,
            end,
            width,
            fill,
        } => Shape::Arc {
            start: up(start),
            mid: up(mid),
            end: up(end),
            width,
            fill,
        },
        Shape::Text { at, text, size } => Shape::Text {
            at: up(at),
            text,
            size,
        },
    }
}

/// Mark the pins the symbol hides that repeat the point and name of a pin it shows: stock
/// symbols stack the pads of one supply that way, and drawing them all would print the
/// name over itself. A hidden pin with no such twin is drawn.
fn mark_duplicates(pins: &mut [GlyphPin], hidden_in_symbol: &[bool]) {
    let shown: Vec<(P, String)> = pins
        .iter()
        .zip(hidden_in_symbol)
        .filter(|(_, hidden)| !**hidden)
        .map(|(p, _)| (p.at, p.name.clone()))
        .collect();
    for (pin, hidden) in pins.iter_mut().zip(hidden_in_symbol) {
        pin.hidden = *hidden
            && shown
                .iter()
                .any(|(at, name)| *at == pin.at && *name == pin.name);
    }
}

fn num(s: &Sexp) -> Option<f64> {
    s.text()?.parse().ok()
}

fn xy(list: &Sexp) -> Option<P> {
    let a = list.args();
    Some((num(a.first()?)?, num(a.get(1)?)?))
}

fn point(parent: &Sexp, name: &str) -> Option<P> {
    xy(parent.find(name)?)
}

fn read_fill(shape: &Sexp) -> Fill {
    let kind = shape
        .find("fill")
        .and_then(|f| f.find("type"))
        .and_then(|t| t.args().first())
        .and_then(Sexp::text);
    match kind {
        Some("outline") => Fill::Outline,
        Some("background") => Fill::Background,
        _ => Fill::None,
    }
}

fn read_width(shape: &Sexp) -> f64 {
    let w = shape
        .find("stroke")
        .and_then(|s| s.find("width"))
        .and_then(|w| num(w.args().first()?))
        .unwrap_or(0.0);
    if w > 0.0 { w } else { DEFAULT_WIDTH }
}

fn read_shape(item: &Sexp) -> Option<Shape> {
    let (width, fill) = (read_width(item), read_fill(item));
    match item.head()? {
        "rectangle" => Some(Shape::Rect {
            a: point(item, "start")?,
            b: point(item, "end")?,
            width,
            fill,
        }),
        "polyline" => {
            let pts: Vec<P> = item.find("pts")?.find_all("xy").filter_map(xy).collect();
            (pts.len() >= 2).then_some(Shape::Poly { pts, width, fill })
        }
        "circle" => Some(Shape::Circle {
            center: point(item, "center")?,
            radius: num(item.find("radius")?.args().first()?)?,
            width,
            fill,
        }),
        "arc" => Some(Shape::Arc {
            start: point(item, "start")?,
            mid: point(item, "mid")?,
            end: point(item, "end")?,
            width,
            fill,
        }),
        _ => None,
    }
}

fn round_unit(v: f64) -> f64 {
    if v.abs() < 1e-6 { 0.0 } else { v.round() }
}

/// `2` before `10`, and `A1` before `B1`.
pub fn natural_order(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        (Ok(_), Err(_)) => std::cmp::Ordering::Less,
        (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
        _ => a.cmp(b),
    }
}

/// The pads each part's nets mention, in net order, indexed like `design.parts`.
pub fn used_pads(design: &Design) -> Vec<Vec<String>> {
    let mut used = vec![Vec::new(); design.parts.len()];
    for net in &design.nets {
        for pin in &net.pins {
            if let Some(i) = design.parts.iter().position(|p| p.refdes == pin.refdes)
                && !used[i].contains(&pin.pin)
            {
                used[i].push(pin.pin.clone());
            }
        }
    }
    used
}

#[cfg(test)]
mod tests {
    use netsmith_kicad::PowerFlow;
    use netsmith_testkit::TempDir;

    use super::*;

    fn part(kind: &str, package: Option<&str>) -> Part {
        Part {
            // What the design stage records for a standard connector (ADR 0068).
            provides: if netsmith_testkit::std_kind(kind) == Some("connector") {
                vec!["Connector".to_string()]
            } else {
                Vec::new()
            },
            declared_kind: netsmith_testkit::std_kind(kind)
                .and_then(netsmith_core::PartKind::from_word),
            refdes: "X1".into(),
            kind: kind.into(),
            value: "v".into(),
            package: package.map(str::to_string),
            path: String::new(),
            pins: Vec::new(),
        }
    }

    fn sample() -> (Mapping, SymbolLib) {
        (
            Mapping::standard(),
            SymbolLib::at(netsmith_testkit::sample_symbols()),
        )
    }

    /// The standard mapping plus a kind `Gadget` drawn by `acme:<symbol>`, with the test
    /// symbol library `acme`.
    fn acme(symbol: &str) -> (TempDir, Mapping, SymbolLib) {
        let dir = TempDir::new("glyph");
        let file = dir.path().join("acme.kicad_sym");
        std::fs::write(&file, netsmith_testkit::acme()).unwrap();
        let rows = format!(
            "[[part]]\nkind = \"Gadget\"\npackage = \"P\"\nsymbol = \"acme:{symbol}\"\nfootprint = \"acme:F\"\n"
        );
        let mapping = Mapping::standard()
            .merged(Mapping::parse(&rows, "kicad.toml", dir.path()).unwrap())
            .unwrap();
        (
            dir,
            mapping,
            SymbolLib::at("/nonexistent").with_library("acme", file),
        )
    }

    fn pads(n: u32) -> Vec<String> {
        (1..=n).map(|n| n.to_string()).collect()
    }

    #[test]
    fn reads_the_resistor_symbol() {
        let (mapping, lib) = sample();
        let g = Glyph::for_part(
            &part("Resistor", Some("0402")),
            &["1".into()],
            None,
            &mapping,
            &lib,
        );
        assert_eq!(g.symbol.as_deref(), Some("Device:R"));
        assert_eq!(g.pins.len(), 2);
        assert_eq!(g.pin("1").unwrap().at, (0.0, 3.81));
        // The top pin's line runs down into the body.
        assert_eq!(g.pin("1").unwrap().toward_body, (0.0, -1.0));
        assert!(!g.show_numbers, "the resistor hides its pin numbers");
        assert!(matches!(g.shapes[0], Shape::Rect { .. }));
    }

    #[test]
    fn reads_every_standard_symbol() {
        let (mapping, lib) = sample();
        for (kind, package, pad_count) in [
            ("Led", "0603", 2),
            ("Capacitor", "0402", 2),
            ("Header1x2", "2.54mm", 2),
            ("LinearRegulator", "SOT-223", 3),
        ] {
            let mut part = part(kind, Some(package));
            // The regulator's row names no symbol: it is drawn from its pin table, which the
            // design stage makes (pad 3 the input, 2 the output, 1 the ground).
            if kind == "LinearRegulator" {
                part.pins = ["vin.pos", "vin.neg", "vout.pos"]
                    .iter()
                    .zip([("IN", "3"), ("GND", "1"), ("OUT", "2")])
                    .map(|(name, (datasheet, pad))| PartPin {
                        name: (*name).into(),
                        datasheet: datasheet.into(),
                        kind: PinKind::Power,
                        pads: vec![pad.into()],
                        function: None,
                        power_flow: None,
                    })
                    .collect();
            }
            let pinout = Pinout::of(&part);
            let g = Glyph::for_part(&part, &pads(pad_count), pinout.as_ref(), &mapping, &lib);
            assert!(g.symbol.is_some(), "{kind}: {:?}", g.missing);
            assert_eq!(g.pins.len(), pad_count as usize);
            assert!(!g.shapes.is_empty());
        }
    }

    #[test]
    fn an_ic_whose_row_names_no_symbol_is_drawn_from_its_pin_table() {
        let (mapping, lib) = sample();
        let pin = |name: &str, datasheet: &str, kind: PinKind, pad: &str| PartPin {
            name: name.into(),
            datasheet: datasheet.into(),
            kind,
            pads: vec![pad.into()],
            function: None,
            power_flow: None,
        };
        let mut ic = part("Lt8641", Some("UDC20(18)"));
        ic.pins = vec![
            pin("vin.pos", "VIN1", PinKind::Power, "3"),
            pin("vin.neg", "GND", PinKind::Power, "1"),
            pin("en", "EN/UV", PinKind::Input, "2"),
        ];
        let g = Glyph::for_part(&ic, &pads(3), Pinout::of(&ic).as_ref(), &mapping, &lib);
        assert_eq!(g.symbol.as_deref(), Some("netsmith:Lt8641_UDC20_18"));
        assert_eq!(g.pins.len(), 3);
        assert!(matches!(g.shapes[0], Shape::Rect { .. }));
        // A part of that row with no pin table is the editor's box, with the reason.
        let bare = Glyph::for_part(
            &part("Lt8641", Some("UDC20(18)")),
            &pads(3),
            None,
            &mapping,
            &lib,
        );
        assert!(bare.symbol.is_none());
        assert!(bare.missing.unwrap().contains("no pin table"));
    }

    #[test]
    fn a_taking_power_pin_of_a_part_that_gives_power_is_on_the_left_in_the_symbol_and_in_the_editors_box()
     {
        let (mapping, lib) = sample();
        let pin = |name: &str, datasheet: &str, pad: &str, flow| PartPin {
            name: name.into(),
            datasheet: datasheet.into(),
            kind: PinKind::Power,
            pads: vec![pad.into()],
            function: None,
            power_flow: flow,
        };
        let pins = vec![
            pin("vin.pos", "IN", "3", Some(PowerFlow::Takes)),
            pin("vin.neg", "GND", "1", None),
            pin("vout.pos", "OUT", "2", Some(PowerFlow::Gives)),
        ];
        // The symbol netsmith draws from the table, and the box for a part whose row names none
        // and has no symbol of its own, put the input on the left, the ground below and the
        // output on the right, which is where the layout reads an input and an output.
        for kind in ["Lt8641", "Gadget"] {
            let mut ic = part(kind, Some("UDC20(18)"));
            ic.pins = pins.clone();
            let g = Glyph::for_part(&ic, &pads(3), Pinout::of(&ic).as_ref(), &mapping, &lib);
            let at = |pad: &str| g.pin(pad).unwrap().at;
            assert!(at("3").0 < 0.0, "{kind}: the input is on the left");
            assert!(at("1").1 < 0.0, "{kind}: the ground is below");
            assert!(at("2").0 > 0.0, "{kind}: the output is on the right");
            assert_eq!(g.pin("2").unwrap().flow, Flow::Out, "{kind}");
            assert_eq!(g.pin("3").unwrap().flow, Flow::In, "{kind}");
        }
        // With no direction known for the output, the part gives no power as far as is known:
        // its supply stays above, as an input.
        let mut ic = part("Lt8641", Some("UDC20(18)"));
        ic.pins = pins;
        ic.pins[2].power_flow = None;
        let g = Glyph::for_part(&ic, &pads(3), Pinout::of(&ic).as_ref(), &mapping, &lib);
        assert!(g.pin("2").unwrap().at.1 > 0.0);
        assert_eq!(g.pin("2").unwrap().flow, Flow::In);
        assert!(g.pin("3").unwrap().at.1 > 0.0, "a part that gives nothing");
    }

    #[test]
    fn a_label_is_a_box_that_prints_its_text() {
        let (mapping, lib) = sample();
        let mut label = part("Label", None);
        label.value = "REV A".into();
        let g = Glyph::for_part(&label, &[], None, &mapping, &lib);
        assert!(
            g.missing.is_none(),
            "a label needs no symbol: {:?}",
            g.missing
        );
        assert!(g.pins.is_empty());
        let text: Vec<_> = g
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(text, ["REV A"]);
    }

    #[test]
    fn an_unknown_kind_becomes_a_box_with_its_pins() {
        let (mapping, lib) = sample();
        let used = vec!["1".to_string(), "2".to_string(), "10".to_string()];
        let g = Glyph::for_part(&part("Gadget", None), &used, None, &mapping, &lib);
        assert!(g.symbol.is_none());
        assert!(g.missing.as_deref().unwrap().contains("Gadget"));
        let ids: Vec<_> = g.pins.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["1", "2", "10"]);
        assert!(g.pins.iter().all(|p| p.name == p.id));
    }

    #[test]
    fn a_missing_library_gives_a_box_that_says_so() {
        let mapping = Mapping::standard();
        let lib = SymbolLib::at("/nonexistent/symbols");
        let g = Glyph::for_part(
            &part("Resistor", Some("0402")),
            &["1".into(), "2".into()],
            None,
            &mapping,
            &lib,
        );
        assert!(g.symbol.is_none());
        let why = g.missing.unwrap();
        assert!(why.contains("symbol library `Device` not found"), "{why}");
    }

    #[test]
    fn a_pad_the_symbol_lacks_also_falls_back_to_a_box() {
        let (mapping, lib) = sample();
        let g = Glyph::for_part(
            &part("Resistor", Some("0402")),
            &["1".into(), "9".into()],
            None,
            &mapping,
            &lib,
        );
        assert!(g.symbol.is_none());
        assert!(g.missing.unwrap().contains("no pin `9`"));
        assert_eq!(g.pins.len(), 2);
    }

    #[test]
    fn hidden_pins_that_repeat_a_shown_pin_resolve_a_pad_but_are_not_drawn() {
        let (_dir, mapping, lib) = acme("Stacked");
        let g = Glyph::for_part(&part("Gadget", Some("P")), &pads(5), None, &mapping, &lib);
        assert_eq!(g.symbol.as_deref(), Some("acme:Stacked"), "{:?}", g.missing);
        let hidden: Vec<_> = g
            .pins
            .iter()
            .filter(|p| p.hidden)
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(hidden, ["2", "3"]);
        // Pad 2 is still a pin, at pad 1's point.
        assert_eq!(g.pin("2").unwrap().at, g.pin("1").unwrap().at);
        assert!(!g.pin("1").unwrap().hidden);
    }

    #[test]
    fn a_hidden_pin_with_no_shown_twin_is_drawn() {
        let dir = TempDir::new("hidden-alone");
        let file = dir.path().join("a.kicad_sym");
        std::fs::write(
            &file,
            "(kicad_symbol_lib (symbol \"S\" (symbol \"S_1_1\" \
             (pin power_in line (at 0 0 0) (hide yes) (length 1) (name \"VDD\") (number \"1\")))))",
        )
        .unwrap();
        let mapping = Mapping::standard()
            .merged(
                Mapping::parse(
                    "[[part]]\nkind = \"G\"\npackage = \"P\"\nsymbol = \"a:S\"\nfootprint = \"a:F\"\n",
                    "kicad.toml",
                    dir.path(),
                )
                .unwrap(),
            )
            .unwrap();
        let lib = SymbolLib::at("/nonexistent").with_library("a", file);
        let g = Glyph::for_part(&part("G", Some("P")), &pads(1), None, &mapping, &lib);
        assert!(!g.pin("1").unwrap().hidden);
    }

    #[test]
    fn flow_follows_the_electrical_type() {
        let (_dir, mapping, lib) = acme("Types");
        let g = Glyph::for_part(&part("Gadget", Some("P")), &pads(12), None, &mapping, &lib);
        assert!(g.symbol.is_some(), "{:?}", g.missing);
        let flow = |pad: &str| g.pin(pad).unwrap().flow;
        assert_eq!(flow("1"), Flow::In);
        assert_eq!(flow("2"), Flow::Out);
        assert_eq!(flow("3"), Flow::Bidirectional);
        assert_eq!(flow("4"), Flow::TriState);
        assert_eq!(flow("5"), Flow::Unspecified);
        assert_eq!(flow("8"), Flow::In);
        assert_eq!(flow("9"), Flow::Out);
        assert_eq!(flow("10"), Flow::Out);
        assert!(Flow::TriState.gives() && !Flow::Bidirectional.gives());
        assert!(Flow::In.takes() && !Flow::Bidirectional.takes());
    }

    #[test]
    fn the_units_of_a_symbol_are_drawn_as_one_part() {
        let (_dir, mapping, lib) = acme("Dual");
        let used: Vec<String> = ["1", "2", "3", "4", "5", "6", "7", "8"]
            .map(String::from)
            .into();
        let g = Glyph::for_part(&part("Gadget", Some("P")), &used, None, &mapping, &lib);
        assert!(g.symbol.is_some(), "{:?}", g.missing);
        assert_eq!(g.pins.len(), 8);
        // One body for the three units, not one for each.
        let bodies = g
            .shapes
            .iter()
            .filter(|s| matches!(s, Shape::Rect { .. }))
            .count();
        assert_eq!(bodies, 1);
        // The pins sit on the side the symbol gave them: inputs left, outputs right, the
        // supply (unit 3) top and bottom, each side in the symbol's own order.
        let at = |pad: &str| g.pin(pad).unwrap().at;
        assert!(at("3").0 < 0.0 && at("2").0 < 0.0 && at("5").0 < 0.0 && at("6").0 < 0.0);
        assert!(at("1").0 > 0.0 && at("7").0 > 0.0);
        assert!(at("8").1 > at("1").1 && at("4").1 < at("1").1);
        // Unit 1's inputs are above unit 2's.
        assert!(at("3").1 > at("2").1 && at("2").1 > at("5").1 && at("5").1 > at("6").1);
        // No two pins are at one point, and no two names are written over each other.
        for (i, a) in g.pins.iter().enumerate() {
            assert!(g.pins[i + 1..].iter().all(|b| a.at != b.at), "{a:?}");
        }
        assert!(!g.texts_collide(Orient::IDENTITY));
    }

    #[test]
    fn a_pin_shared_by_every_unit_appears_once() {
        let (_dir, mapping, lib) = acme("Shared");
        let g = Glyph::for_part(&part("Gadget", Some("P")), &pads(4), None, &mapping, &lib);
        assert_eq!(g.pins.len(), 5, "{:?}", g.missing);
        assert_eq!(g.pins.iter().filter(|p| p.id == "9").count(), 1);
    }

    #[test]
    fn only_the_first_body_style_is_drawn() {
        let (_dir, mapping, lib) = acme("Gate");
        let g = Glyph::for_part(&part("Gadget", Some("P")), &pads(3), None, &mapping, &lib);
        assert_eq!(g.pins.len(), 3);
        assert_eq!(g.pin("1").unwrap().at, (-6.35, 1.27));
    }

    fn fact(path: &str, name: &str, kind: PinKind, pads: &[&str]) -> PartPin {
        PartPin {
            name: path.into(),
            datasheet: name.into(),
            kind,
            pads: pads.iter().map(|p| p.to_string()).collect(),
            function: None,
            power_flow: None,
        }
    }

    #[test]
    fn the_box_for_a_known_pinout_lists_every_pin_by_its_datasheet_name() {
        let (mapping, lib) = sample();
        let mut pa2 = fact("pa2", "PA2", PinKind::Bidirectional, &["12"]);
        pa2.function = Some("UART1_TX".into());
        let pinout = Pinout {
            pins: vec![
                fact("vdd.pos", "VDD", PinKind::Power, &["1", "17", "32"]),
                fact("vdd.neg", "VSS", PinKind::Power, &["8", "24"]),
                fact("nrst", "NRST", PinKind::Input, &["4"]),
                fact("boot0", "BOOT0", PinKind::Input, &["31"]),
                fact("txd", "TXD", PinKind::Output, &["13"]),
                fact("nc", "NC", PinKind::NoConnect, &["5"]),
                pa2,
            ],
        };
        // The design uses two of the pads; the box still has every pin.
        let used = vec!["1".to_string(), "12".to_string()];
        let g = Glyph::for_part(
            &part("Gadget", Some("P")),
            &used,
            Some(&pinout),
            &mapping,
            &lib,
        );
        assert!(g.symbol.is_none());
        let at = |pad: &str| g.pin(pad).unwrap().at;
        let name = |pad: &str| g.pin(pad).unwrap().name.clone();
        // Supplies on top, grounds below.
        assert!(at("1").1 > 0.0 && at("8").1 < 0.0);
        // Inputs left, outputs right.
        assert!(at("4").0 < 0.0 && at("31").0 < 0.0);
        assert!(at("13").0 > 0.0);
        // Each pad of a pin is a pin at one point, and only the first is drawn.
        assert_eq!(at("17"), at("1"));
        assert_eq!(at("32"), at("1"));
        let hidden: Vec<_> = g
            .pins
            .iter()
            .filter(|p| p.hidden)
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(hidden, ["17", "32", "24"]);
        // The chosen function is printed with the name.
        assert_eq!(name("12"), "PA2 UART1_TX");
        assert_eq!(name("4"), "NRST");
        // The no-connect pin is last on its side: below every other pin there.
        let side = |pad: &str| at(pad).0.signum();
        let same_side: Vec<f64> = g
            .pins
            .iter()
            .filter(|p| !p.hidden && p.at.0.signum() == side("5") && p.at.0 != 0.0 && p.id != "5")
            .map(|p| p.at.1)
            .collect();
        assert!(same_side.iter().all(|y| *y > at("5").1), "{same_side:?}");
        // No pad twice, and every pad of the pinout is there.
        assert_eq!(g.pins.len(), 3 + 2 + 1 + 1 + 1 + 1 + 1);
    }

    #[test]
    fn a_pad_the_pinout_lacks_still_gets_a_pin_in_the_box() {
        let (mapping, lib) = sample();
        let pinout = Pinout {
            pins: vec![fact("a", "A", PinKind::Input, &["1"])],
        };
        let g = Glyph::for_part(
            &part("Gadget", Some("P")),
            &["1".into(), "7".into()],
            Some(&pinout),
            &mapping,
            &lib,
        );
        assert!(g.pin("7").is_some());
    }

    #[test]
    fn natural_order_puts_two_before_ten() {
        let mut v = vec!["10", "2", "B1", "A1"];
        v.sort_by(|a, b| natural_order(a, b));
        assert_eq!(v, ["2", "10", "A1", "B1"]);
    }
}
