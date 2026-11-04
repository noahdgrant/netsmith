//! What a part looks like: the drawing shapes and pins of its symbol, in the symbol's own
//! coordinates (millimetres, y up), read from the symbol library's S-expressions. A part
//! with no usable symbol gets a labelled box with its pins, so the viewer never fails on
//! a part it cannot draw.

use netsmith_core::{Design, Part};
use netsmith_kicad::sexp::Sexp;
use netsmith_kicad::{Mapping, PartPin, PinKind, PinType, Pinout, Symbol, SymbolLib};

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
        let mapped = match mapping.map(&part.kind, part.package.as_deref()) {
            Ok(m) => m,
            Err(e) => return Glyph::fallback(part, used, pinout, e.to_string()),
        };
        let symbol = match lib.symbol(&mapped.lib_id) {
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
            return Glyph::grouped(part, symbol);
        }
        if symbol.units() > 1 {
            return Glyph::merged(part, symbol);
        }
        let glyph = Glyph::from_symbol(symbol);
        if glyph.texts_collide(Orient::IDENTITY) {
            Glyph::merged(part, symbol)
        } else {
            glyph
        }
    }

    /// The symbol's own drawing: its first unit, then the others each below the one before.
    /// The drawing shared by all units (unit 0) goes with the first.
    fn from_symbol(symbol: &Symbol) -> Glyph {
        let def = symbol.definition();
        let mut shapes = Vec::new();
        let mut pins: Vec<GlyphPin> = Vec::new();
        let mut hidden_in_symbol: Vec<bool> = Vec::new();
        let mut floor: Option<f64> = None;
        for unit in 1..=symbol.units() {
            let mut unit_shapes = Vec::new();
            for part in def.find_all("symbol") {
                let Some((number, style)) =
                    unit_of(part.args().first().and_then(Sexp::text).unwrap_or(""))
                else {
                    continue;
                };
                if style > 1 || !(number == unit || (number == 0 && unit == 1)) {
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
                .filter(|p| p.unit == unit || (p.unit == 0 && unit == 1))
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
                    .filter(|p| p.unit == unit || (p.unit == 0 && unit == 1))
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
        }
    }

    /// A box for a part with no usable symbol. With the part's pinout it has every pin of
    /// the package, named from the datasheet: supplies on top, grounds below, inputs on
    /// the left, outputs on the right and the no-connect pins last. Without, it has one
    /// pin for each pad the design uses, split between its left and right sides.
    fn fallback(part: &Part, used: &[String], pinout: Option<&Pinout>, why: String) -> Glyph {
        let mut sides = Sides::default();
        match pinout {
            Some(pinout) => {
                for pin in &pinout.pins {
                    sides.put(pin);
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
        }
    }

    /// The number and the name written at `pin`, whose connection point is `at` and whose
    /// line runs toward `toward` (the way to the body), as they are drawn.
    pub fn pin_texts(&self, pin: &GlyphPin, at: Pt, toward: Dir) -> (Option<Text>, Option<Text>) {
        let inner = at.step(toward, pin.length);
        let number = self.show_numbers.then(|| {
            let mid = Pt::new((at.x + inner.x) / 2.0, (at.y + inner.y) / 2.0);
            if toward.is_vertical() {
                Text::new(&pin.id, Pt::new(mid.x - 0.4, mid.y + 0.35), Anchor::End)
            } else {
                Text::new(&pin.id, Pt::new(mid.x, mid.y - 0.4), Anchor::Middle)
            }
        });
        let name = (self.show_names && !pin.name.is_empty()).then(|| {
            let offset = self.name_offset;
            match toward {
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

    /// Whether, with the part turned by `orient`, two of the texts at its pins would be
    /// written over each other.
    pub fn texts_collide(&self, orient: Orient) -> bool {
        let mut boxes: Vec<Rect> = Vec::new();
        for pin in self.pins.iter().filter(|p| !p.hidden) {
            let at = orient.apply(pin.at);
            let toward = orient.dir(pin.toward_body);
            let (number, name) = self.pin_texts(pin, at, toward);
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
fn symbol_entries(symbol: &Symbol) -> Vec<SymEntry> {
    let mut out: Vec<SymEntry> = Vec::new();
    for p in symbol.pins() {
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
    fn merged(part: &Part, symbol: &Symbol) -> Glyph {
        let mut entries = symbol_entries(symbol);
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
    fn grouped(part: &Part, symbol: &Symbol) -> Glyph {
        let entries = symbol_entries(symbol);
        let total = entries.len();
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
        let mut all: Vec<(String, Vec<Entry>)> = Vec::new();
        all.push(("Power".into(), power));
        all.push(("Ground".into(), ground));
        all.extend(groups);
        all.push(("Other".into(), other));
        all.push(("Not connected".into(), open));
        all.retain(|(_, list)| !list.is_empty());

        let mut rows: Vec<Row> = Vec::new();
        for (key, list) in all {
            rows.push(Row::Heading(format!("{key} ({})", list.len())));
            rows.extend(list.into_iter().map(Row::Pin));
        }
        // Cut after the row nearest the middle that is the end of a group; inside the group
        // when it is longer than the half.
        let half = rows.len().div_ceil(2);
        let boundaries: Vec<usize> = (1..rows.len())
            .filter(|&i| matches!(rows[i], Row::Heading(_)))
            .collect();
        let cut = boundaries
            .iter()
            .copied()
            .min_by_key(|&i| i.abs_diff(half))
            .filter(|&i| i <= half * 3 / 2 + 1 && rows.len() - i <= half * 3 / 2 + 1)
            .unwrap_or(half);
        let mut right: Vec<Row> = rows.split_off(cut);
        let left = rows;
        if matches!(right.first(), Some(Row::Pin(_))) {
            right.insert(0, Row::Heading("(continued)".into()));
        }

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
        let title = format!("{}, {total} pins", part.kind);
        let title_width = title.chars().count() as f64 * 1.27 * CHAR_WIDTH;
        let half_width = snap_up(
            ((widest(&left) + widest(&right) + 2.0 * 2.54 + 2.54).max(title_width + 2.54)) / 2.0,
            1.27,
        )
        .max(7.62);
        let height_rows = left.len().max(right.len());
        // A band at the top for the title, then one row for each pin and heading.
        let half_height = snap_up((height_rows as f64 + 2.0) * PIN_PITCH / 2.0, 1.27);
        let first_row = half_height - 3.81;

        let mut pins = Vec::new();
        let mut shapes = vec![
            Shape::Rect {
                a: (-half_width, half_height),
                b: (half_width, -half_height),
                width: 0.254,
                fill: Fill::Background,
            },
            Shape::Text {
                at: (0.0, half_height - 1.9),
                text: title,
                size: 1.27,
            },
        ];
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
        }
    }
}

/// One pin of the fallback box: all its pads, the name printed beside it.
struct Entry {
    pads: Vec<String>,
    name: String,
    flow: Flow,
}

impl Entry {
    /// A pin known only by its pad.
    fn pad(pad: &str) -> Entry {
        Entry {
            pads: vec![pad.to_string()],
            name: pad.to_string(),
            flow: Flow::Unspecified,
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
                PinKind::Input | PinKind::Power => Flow::In,
                PinKind::Output => Flow::Out,
                PinKind::Bidirectional => Flow::Bidirectional,
                PinKind::Passive | PinKind::NoConnect => Flow::Unspecified,
            },
        }
    }
}

/// Whether a power pin is a ground, which goes below the box.
fn is_ground(pin: &PartPin) -> bool {
    let name = pin.datasheet.to_ascii_uppercase();
    pin.name.ends_with(".neg")
        || ["GND", "VSS", "GROUND", "AGND", "DGND", "PGND"]
            .iter()
            .any(|g| name.starts_with(g))
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
    fn put(&mut self, pin: &PartPin) {
        let entry = Entry::of(pin);
        match pin.kind {
            PinKind::Power if is_ground(pin) => self.bottom.push(entry),
            PinKind::Power => self.top.push(entry),
            PinKind::Input | PinKind::Passive => self.left.push(entry),
            PinKind::Output => self.right.push(entry),
            PinKind::Bidirectional => self.either.push(entry),
            PinKind::NoConnect => self.no_connect.push(entry),
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
            let first = -(entries.len().saturating_sub(1) as f64) * across / 2.0;
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
    let up = |p: P| (p.0, p.1 + dy);
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
    use netsmith_kicad::testlib::{self, TempDir};

    use super::*;

    fn part(kind: &str, package: Option<&str>) -> Part {
        Part {
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
            SymbolLib::at(testlib::sample_symbols()),
        )
    }

    /// The standard mapping plus a kind `Gadget` drawn by `acme:<symbol>`, with the test
    /// symbol library `acme`.
    fn acme(symbol: &str) -> (TempDir, Mapping, SymbolLib) {
        let dir = TempDir::new("glyph");
        let file = dir.path().join("acme.kicad_sym");
        std::fs::write(&file, testlib::acme()).unwrap();
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
            let g = Glyph::for_part(
                &part(kind, Some(package)),
                &pads(pad_count),
                None,
                &mapping,
                &lib,
            );
            assert!(g.symbol.is_some(), "{kind}: {:?}", g.missing);
            assert_eq!(g.pins.len(), pad_count as usize);
            assert!(!g.shapes.is_empty());
        }
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
