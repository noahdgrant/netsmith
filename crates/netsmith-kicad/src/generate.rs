//! The symbol of an IC, drawn from its pin table (ADR 0097, step 1).
//!
//! A row of the mapping with no `symbol` names no library symbol: the part is drawn as a plain
//! box with its pins on the sides, and needs no KiCad install. The box is the one the editor
//! already draws for a part with no symbol, in KiCad's `.kicad_sym` form, with the same rule
//! for the side of each pin ([`side_of`]).
//!
//! A symbol is made of nothing but the pin table and the names of the part:
//!
//! - A box for each unit ([`draw`]; ADR 0050), a rectangle and pins and no other shape. A pin
//!   table does not say which unit a pin is in, so [`symbol`] draws one unit, and a part that
//!   has several (a library symbol's) is drawn as it always was.
//! - A pin for every pad of the table: the pin's first pad is the visible pin, the others lie
//!   on its point, hidden and `passive`, as KiCad's own symbols stack the pads of one pin.
//! - The pin's name is the datasheet's, and its electrical type follows its kind and, for a power
//!   pin, whether the solved design says it gives power ([`pin_type`]).
//! - Every pin and the box are on KiCad's 50 mil (1.27 mm) grid.
//!
//! The same table gives the same bytes: nothing here reads a clock, a map's order or the
//! machine.

use netsmith_core::{Part, PartPin, PinKind, PowerFlow};
use uuid::Uuid;

use crate::sexp::Sexp;
use crate::stdlib;
use crate::symbols::{PinType, Symbol};

/// KiCad's connection grid, 50 mil, in mm.
const GRID: f64 = 1.27;
/// Pin length and pitch, in grid steps (2.54 mm).
const PIN_LENGTH: i32 = 2;
const PIN_PITCH: i32 = 2;
/// The width of one character of a pin name, in grid steps (0.6 of the 1.27 mm text).
const CHAR_STEPS: f64 = 0.6;
/// The narrowest box, in grid steps (10.16 mm).
const MIN_WIDTH: i32 = 8;
/// The text of the symbol's fields, mm.
const TEXT: f64 = 1.27;

/// The side of the box a pin is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
    Left,
    Right,
    /// Bidirectional pins go to whichever side is shorter, after the others.
    Either,
    /// No-connect pins go to whichever side is shorter, after the bidirectional ones.
    NoConnect,
}

/// Whether a power pin is a ground, which goes below the box.
pub fn is_ground(pin: &PartPin) -> bool {
    let name = pin.datasheet.to_ascii_uppercase();
    pin.name.ends_with(".neg")
        || ["GND", "VSS", "GROUND", "AGND", "DGND", "PGND"]
            .iter()
            .any(|g| name.starts_with(g))
}

/// Whether one of `pins` is a power pin that gives power ([`PartPin::gives_power`]): the part is
/// a converter or a regulator, and [`side_of`] puts its supply on the left.
pub fn part_gives_power<'a>(pins: impl IntoIterator<Item = &'a PartPin>) -> bool {
    pins.into_iter()
        .any(|p| p.kind == PinKind::Power && p.gives_power())
}

/// Where a pin goes: the ground below, inputs and passive pins on the left, outputs on the
/// right. A power pin that gives power (an output of a regulator, [`PartPin::gives_power`])
/// is an output too, so it goes on the right, where the schematic layout reads an output. A
/// power pin that takes power ([`PartPin::power_flow`] is `Takes`) of a part that also has a
/// pin that gives power (`part_gives`, see [`part_gives_power`]) goes on the left, where the
/// layout reads the input of a converter. The supply of any other part, and a power pin whose
/// direction is not known, stays above. The editor's box for a part with no symbol uses this
/// too.
pub fn side_of(pin: &PartPin, part_gives: bool) -> Side {
    match pin.kind {
        PinKind::Power if is_ground(pin) => Side::Bottom,
        PinKind::Power if pin.gives_power() => Side::Right,
        PinKind::Power if part_gives && pin.power_flow == Some(PowerFlow::Takes) => Side::Left,
        PinKind::Power => Side::Top,
        PinKind::Input | PinKind::Passive => Side::Left,
        PinKind::Output | PinKind::TriState => Side::Right,
        PinKind::Bidirectional => Side::Either,
        PinKind::NoConnect => Side::NoConnect,
    }
}

/// The electrical type of a pin of the pin table in KiCad's symbol. The table says `power`
/// without saying whether the pin takes or gives power (that is the sign of a rail's current,
/// which is known after the solve): a power pin that the solved design says gives power is a
/// power output, and any other is a power input, so that a pin whose direction is not known
/// asks its net for a source and never claims one that may not be there.
pub fn pin_type(pin: &PartPin) -> PinType {
    match pin.kind {
        PinKind::Power if pin.gives_power() => PinType::PowerOut,
        PinKind::Power => PinType::PowerIn,
        PinKind::Input => PinType::Input,
        PinKind::Output => PinType::Output,
        PinKind::Bidirectional => PinType::Bidirectional,
        PinKind::Passive => PinType::Passive,
        PinKind::NoConnect => PinType::NoConnect,
        PinKind::TriState => PinType::TriState,
    }
}

/// The name of the symbol of a part of `kind` in `package`: `Lt8641_UDC20_18`. Only letters,
/// digits and `_ - . +` stay, so that the name is one KiCad takes in a file name and an id.
pub fn symbol_name(kind: &str, package: Option<&str>) -> String {
    symbol_name_for(kind, package, &[])
}

/// The name of the symbol of `part`: [`symbol_name`], and, for a part with pins that give
/// power, `_out` and the first pad of each (`LinearRegulator_SOT-223_out2`), so that two parts
/// of one kind and package whose pins differ in direction (an output that one board loads and
/// another leaves open) are two symbols with two names, as a library needs.
fn symbol_name_for(kind: &str, package: Option<&str>, pins: &[PartPin]) -> String {
    let mut raw = match package {
        Some(package) => format!("{kind}_{package}"),
        None => kind.to_string(),
    };
    let mut giving: Vec<&str> = pins
        .iter()
        .filter(|p| p.kind == PinKind::Power && p.gives_power())
        .filter_map(|p| p.pads.first().map(String::as_str))
        .collect();
    giving.sort_unstable();
    if !giving.is_empty() {
        raw = format!("{raw}_out{}", giving.join("-"));
    }
    let mut name: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "_-.+".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    while name.contains("__") {
        name = name.replace("__", "_");
    }
    name.trim_matches('_').to_string()
}

/// The letters of a reference designator, `U` for `U12`: the symbol's `Reference` field.
fn reference_prefix(refdes: &str) -> String {
    let letters: String = refdes.chars().take_while(|c| !c.is_ascii_digit()).collect();
    if letters.is_empty() {
        "U".to_string()
    } else {
        letters
    }
}

/// The symbol `netsmith:<name>` of `part`, drawn from its pin table, with `footprint` (the
/// mapping's id) in its `Footprint` field. `None` for a part with no pin table.
pub fn symbol(part: &Part, footprint: &str) -> Option<Symbol> {
    make(part, footprint, false)
}

/// The symbol of `part` as [`symbol`] draws it, under a name that ends in a short hash of the
/// drawing (`netsmith:Lt8641_UDC20_18_1a2b3c4d`), for a design in which the plain name is taken
/// by another drawing ([`crate::resolve`]): the name then says which of the drawings it is, and
/// is the same on every machine and in every order of the parts.
pub fn symbol_hashed(part: &Part, footprint: &str) -> Option<Symbol> {
    make(part, footprint, true)
}

fn make(part: &Part, footprint: &str, hashed: bool) -> Option<Symbol> {
    if part.pins.is_empty() {
        return None;
    }
    let plain = symbol_name_for(&part.kind, part.package.as_deref(), &part.pins);
    let reference = reference_prefix(&part.refdes);
    let drawn = |name: &str| draw(name, &reference, footprint, &[part.pins.iter().collect()]);
    let name = if hashed {
        // Everything the symbol is made of (the table, the reference letters, the footprint) is
        // in the drawing under the plain name.
        let text = drawn(&plain).to_kicad_string();
        let hash = Uuid::new_v5(&Uuid::NAMESPACE_OID, text.as_bytes()).simple();
        format!("{plain}_{}", &hash.to_string()[..8])
    } else {
        plain
    };
    let lib_id = format!("{}:{name}", stdlib::NAME);
    Some(
        Symbol::from_definition(&lib_id, &name, drawn(&name))
            .expect("a symbol drawn here is one that can be read"),
    )
}

/// One pin of a unit, as the box lays it out.
struct Entry<'a> {
    pin: &'a PartPin,
}

impl Entry<'_> {
    fn chars(&self) -> f64 {
        self.pin.datasheet.chars().count() as f64
    }
}

fn longest(entries: &[Entry]) -> f64 {
    entries.iter().map(Entry::chars).fold(0.0, f64::max)
}

/// The smallest whole number of grid steps that is at least `steps`.
fn ceil_steps(steps: f64) -> i32 {
    (steps - 1e-9).ceil() as i32
}

fn mm(steps: i32) -> f64 {
    f64::from(steps) * GRID
}

fn at(x: f64, y: f64, angle: f64) -> Sexp {
    Sexp::node("at", vec![Sexp::num(x), Sexp::num(y), Sexp::num(angle)])
}

fn effects(hide: bool) -> Sexp {
    let mut items = vec![Sexp::node(
        "font",
        vec![Sexp::node("size", vec![Sexp::num(TEXT), Sexp::num(TEXT)])],
    )];
    if hide {
        items.push(Sexp::node("hide", vec![Sexp::atom("yes")]));
    }
    Sexp::node("effects", items)
}

fn property(name: &str, value: &str, pos: (f64, f64), hide: bool) -> Sexp {
    Sexp::node(
        "property",
        vec![
            Sexp::string(name),
            Sexp::string(value),
            at(pos.0, pos.1, 0.0),
            effects(hide),
        ],
    )
}

fn text_field(tag: &str, text: &str) -> Sexp {
    Sexp::node(tag, vec![Sexp::string(text), effects(false)])
}

/// One `(pin ...)`: `hidden` for the second and later pads of a pin, which lie on its point.
fn pin_node(
    kind: PinType,
    position: (i32, i32),
    angle: f64,
    name: &str,
    number: &str,
    hidden: bool,
) -> Sexp {
    let mut items = vec![
        Sexp::atom(kind.name()),
        Sexp::atom("line"),
        at(mm(position.0), mm(position.1), angle),
        Sexp::node("length", vec![Sexp::num(mm(PIN_LENGTH))]),
    ];
    if hidden {
        items.push(Sexp::node("hide", vec![Sexp::atom("yes")]));
    }
    items.push(text_field("name", name));
    items.push(text_field("number", number));
    Sexp::node("pin", items)
}

/// The box of one unit and its pins, as the two `(symbol "<name>_<unit>_1" ...)` parts KiCad
/// wants: the rectangle and the pins.
fn unit_node(
    name: &str,
    unit: usize,
    pins: &[&PartPin],
    part_gives: bool,
) -> (Sexp, i32, bool, bool) {
    let mut top: Vec<Entry> = Vec::new();
    let mut bottom: Vec<Entry> = Vec::new();
    let mut left: Vec<Entry> = Vec::new();
    let mut right: Vec<Entry> = Vec::new();
    let mut either: Vec<Entry> = Vec::new();
    let mut no_connect: Vec<Entry> = Vec::new();
    for &pin in pins {
        let entry = Entry { pin };
        match side_of(pin, part_gives) {
            Side::Top => top.push(entry),
            Side::Bottom => bottom.push(entry),
            Side::Left => left.push(entry),
            Side::Right => right.push(entry),
            Side::Either => either.push(entry),
            Side::NoConnect => no_connect.push(entry),
        }
    }
    for entry in either.into_iter().chain(no_connect) {
        if left.len() <= right.len() {
            left.push(entry);
        } else {
            right.push(entry);
        }
    }

    // Everything below is in grid steps, so that the pins and the box are on the grid exactly.
    let names = ceil_steps((longest(&left) + longest(&right)) * CHAR_STEPS) + 2 * PIN_LENGTH;
    let across = ceil_steps(longest(&top).max(longest(&bottom)) * CHAR_STEPS + 1.0).max(PIN_PITCH);
    let columns = top.len().max(bottom.len()) as i32;
    let half_width = (MIN_WIDTH.max(names).max(columns * across + 2) + 1) / 2;
    let rows = left.len().max(right.len()).max(1) as i32;
    let vertical = !(top.is_empty() && bottom.is_empty());
    // The names of the pins along the top and bottom run down and up inside the box.
    let upright = ceil_steps((longest(&top) + longest(&bottom)) * CHAR_STEPS) + 2;
    let half_height = if vertical {
        (rows + 1).max(3).max((upright + 1) / 2)
    } else {
        rows + 1
    };

    let mut nodes: Vec<Sexp> = Vec::new();
    let mut add = |entry: &Entry, position: (i32, i32), angle: f64| {
        let pin = entry.pin;
        for (i, pad) in pin.pads.iter().enumerate() {
            nodes.push(pin_node(
                if i == 0 {
                    pin_type(pin)
                } else {
                    PinType::Passive
                },
                position,
                angle,
                &pin.datasheet,
                pad,
                i > 0,
            ));
        }
    };
    for (side, angle, entries) in [(-1, 0.0, &left), (1, 180.0, &right)] {
        for (row, entry) in entries.iter().enumerate() {
            // Rows a pitch apart, centred on the middle of the box.
            let y = rows - 1 - row as i32 * PIN_PITCH;
            add(entry, (side * (half_width + PIN_LENGTH), y), angle);
        }
    }
    for (edge, angle, entries) in [(1, 270.0, &top), (-1, 90.0, &bottom)] {
        let first = -((entries.len().saturating_sub(1) as i32 * across) / 2);
        for (i, entry) in entries.iter().enumerate() {
            add(
                entry,
                (first + i as i32 * across, edge * (half_height + PIN_LENGTH)),
                angle,
            );
        }
    }

    let rectangle = Sexp::node(
        "rectangle",
        vec![
            Sexp::node(
                "start",
                vec![Sexp::num(-mm(half_width)), Sexp::num(mm(half_height))],
            ),
            Sexp::node(
                "end",
                vec![Sexp::num(mm(half_width)), Sexp::num(-mm(half_height))],
            ),
            Sexp::node(
                "stroke",
                vec![
                    Sexp::node("width", vec![Sexp::num(0.254)]),
                    Sexp::node("type", vec![Sexp::atom("default")]),
                ],
            ),
            Sexp::node(
                "fill",
                vec![Sexp::node("type", vec![Sexp::atom("background")])],
            ),
        ],
    );
    let unit_part = |style_unit: usize, content: Vec<Sexp>| {
        let mut items = vec![Sexp::string(format!("{name}_{style_unit}_1"))];
        items.extend(content);
        Sexp::node("symbol", items)
    };
    let mut part_nodes = vec![rectangle];
    part_nodes.extend(nodes);
    (
        unit_part(unit, part_nodes),
        half_height,
        !top.is_empty(),
        !bottom.is_empty(),
    )
}

/// The definition `(symbol "<name>" ...)` of a part with `units`, each the pins of one box,
/// with the box of each unit. `reference` is the designator's letters and `footprint` the
/// KiCad footprint id, for the fields. The name is bare, as a library file holds it.
pub fn draw(name: &str, reference: &str, footprint: &str, units: &[Vec<&PartPin>]) -> Sexp {
    let mut items = vec![
        Sexp::string(name),
        Sexp::node(
            "pin_names",
            vec![Sexp::node("offset", vec![Sexp::num(1.016)])],
        ),
        Sexp::node("exclude_from_sim", vec![Sexp::atom("no")]),
        Sexp::node("in_bom", vec![Sexp::atom("yes")]),
        Sexp::node("on_board", vec![Sexp::atom("yes")]),
    ];
    let mut parts = Vec::new();
    let (mut height, mut above, mut below) = (0, false, false);
    let part_gives = part_gives_power(units.iter().flatten().copied());
    for (i, pins) in units.iter().enumerate() {
        let (part, half_height, top, bottom) = unit_node(name, i + 1, pins, part_gives);
        if i == 0 {
            (height, above, below) = (half_height, top, bottom);
        }
        parts.push(part);
    }
    // The fields sit above and below the first unit's box, clear of the pins that stick out.
    let reference_at = (0.0, mm(height + 1 + if above { PIN_LENGTH } else { 0 }));
    let value_at = (0.0, -mm(height + 1 + if below { PIN_LENGTH } else { 0 }));
    items.push(property("Reference", reference, reference_at, false));
    items.push(property("Value", name, value_at, false));
    items.push(property("Footprint", footprint, (0.0, 0.0), true));
    items.push(property("Datasheet", "~", (0.0, 0.0), true));
    items.push(property("Description", "", (0.0, 0.0), true));
    items.extend(parts);
    Sexp::node("symbol", items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(name: &str, datasheet: &str, kind: PinKind, pads: &[&str]) -> PartPin {
        PartPin {
            name: name.into(),
            datasheet: datasheet.into(),
            kind,
            pads: pads.iter().map(|p| p.to_string()).collect(),
            function: None,
            power_flow: None,
        }
    }

    fn part(kind: &str, package: Option<&str>, pins: Vec<PartPin>) -> Part {
        Part {
            refdes: "U3".into(),
            kind: kind.into(),
            provides: Vec::new(),
            value: String::new(),
            package: package.map(str::to_string),
            path: "u".into(),
            pins,
        }
    }

    /// An LDO as a pin table has it: a supply, a ground, an output with two pads, an enable,
    /// a power-good output, a bidirectional pin and a pad left open.
    fn ldo() -> Part {
        part(
            "Ldo8",
            Some("DFN-8(EP)"),
            vec![
                pin("vin.pos", "IN", PinKind::Power, &["1"]),
                pin("en", "EN", PinKind::Input, &["3"]),
                pin("vout.pos", "OUT", PinKind::Power, &["2", "7"]),
                pin("pg", "PG", PinKind::Output, &["4"]),
                pin("tri", "Y", PinKind::TriState, &["8"]),
                pin("io", "SDA", PinKind::Bidirectional, &["5"]),
                pin("nc", "NC", PinKind::NoConnect, &["6"]),
                pin("vin.neg", "GND", PinKind::Power, &["9"]),
                pin("cfg", "CFG", PinKind::Passive, &["10"]),
            ],
        )
    }

    /// The pins of a symbol as (pad, name, type), sorted by pad, for comparing with a table.
    fn as_table(symbol: &Symbol) -> Vec<(String, String, &'static str)> {
        let mut pins: Vec<_> = symbol
            .pins()
            .iter()
            .map(|p| (p.number.clone(), p.name.clone(), p.kind.name()))
            .collect();
        pins.sort();
        pins
    }

    #[test]
    fn the_symbol_has_the_pads_the_names_and_the_electrical_types_of_the_table() {
        let part = ldo();
        let symbol = symbol(
            &part,
            "Package_DFN_QFN:DFN-8-1EP_3x3mm_P0.65mm_EP1.55x2.4mm",
        )
        .unwrap();
        assert_eq!(symbol.lib_id(), "netsmith:Ldo8_DFN-8_EP");
        assert_eq!(symbol.units(), 1);
        let mut expected: Vec<(String, String, &'static str)> = Vec::new();
        for p in &part.pins {
            for (i, pad) in p.pads.iter().enumerate() {
                // The second pad of a pin lies on the first's point, hidden and passive.
                let kind = if i == 0 {
                    pin_type(p).name()
                } else {
                    "passive"
                };
                expected.push((pad.clone(), p.datasheet.clone(), kind));
            }
        }
        expected.sort();
        assert_eq!(as_table(&symbol), expected);
        // Each kind has the type KiCad's file names for it.
        for (kind, word) in [
            (PinKind::Power, "power_in"),
            (PinKind::Input, "input"),
            (PinKind::Output, "output"),
            (PinKind::Bidirectional, "bidirectional"),
            (PinKind::Passive, "passive"),
            (PinKind::NoConnect, "no_connect"),
            (PinKind::TriState, "tri_state"),
        ] {
            assert_eq!(pin_type(&pin("p", "P", kind, &["1"])).name(), word);
        }
    }

    /// The pin table of [`ldo`], solved: its output gives power, its input takes it.
    fn solved_ldo() -> Part {
        let mut part = ldo();
        for pin in &mut part.pins {
            match pin.name.as_str() {
                "vout.pos" => pin.power_flow = Some(PowerFlow::Gives),
                "vin.pos" => pin.power_flow = Some(PowerFlow::Takes),
                _ => {}
            }
        }
        part
    }

    #[test]
    fn a_power_pin_that_gives_power_is_a_power_output_on_the_right_and_one_that_takes_it_is_on_the_left()
     {
        let part = solved_ldo();
        let symbol = symbol(&part, "a:b").unwrap();
        let rect = symbol.definition().find_all("symbol").next().unwrap();
        let right_edge = rect.find("rectangle").unwrap().find("end").unwrap().args()[0]
            .text()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        // The output's first pad is a power output, on the right of the box, pointing left.
        let out = symbol.pin("2").unwrap();
        assert_eq!(out.kind.name(), "power_out");
        assert!(out.x > right_edge && out.angle == 180.0, "{out:?}");
        // Its second pad lies on the same point, hidden and passive, as for any pin.
        let second = symbol.pin("7").unwrap();
        assert_eq!(
            (second.kind.name(), (second.x, second.y)),
            ("passive", (out.x, out.y))
        );
        // A power pin that takes power, of a part that gives power too, is a power input on
        // the left of the box, where the layout reads the input of a converter; the ground is
        // below.
        let supply = symbol.pin("1").unwrap();
        let left_edge = rect
            .find("rectangle")
            .unwrap()
            .find("start")
            .unwrap()
            .args()[0]
            .text()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert_eq!((supply.kind.name(), supply.angle), ("power_in", 0.0));
        assert!(supply.x < left_edge, "{supply:?}");
        let ground = symbol.pin("9").unwrap();
        assert_eq!((ground.kind.name(), ground.angle), ("power_in", 90.0));
        // Nothing but the output changed against the table with no direction (the side is not
        // in the table).
        let plain = as_table(&super::symbol(&ldo(), "a:b").unwrap());
        let drawn = as_table(&symbol);
        let changed: Vec<_> = plain.iter().zip(&drawn).filter(|(a, b)| a != b).collect();
        assert_eq!(changed.len(), 1, "{changed:?}");
        assert_eq!(drawn.iter().filter(|p| p.2 == "power_out").count(), 1);
    }

    #[test]
    fn a_power_pin_with_no_known_direction_is_a_power_input_above_the_box() {
        for flow in [None, Some(PowerFlow::Takes)] {
            let mut part = ldo();
            part.pins[2].power_flow = flow;
            let symbol = symbol(&part, "a:b").unwrap();
            let out = symbol.pin("2").unwrap();
            assert_eq!(
                (out.kind.name(), out.angle),
                ("power_in", 270.0),
                "{flow:?}"
            );
            assert_eq!(symbol.lib_id(), "netsmith:Ldo8_DFN-8_EP", "{flow:?}");
        }
        // A direction on a pin that is not a power pin says nothing: a signal stays what its
        // kind says.
        let mut part = ldo();
        part.pins[3].power_flow = Some(PowerFlow::Gives);
        let symbol = symbol(&part, "a:b").unwrap();
        assert_eq!(symbol.pin("4").unwrap().kind.name(), "output");
        assert_eq!(symbol.lib_id(), "netsmith:Ldo8_DFN-8_EP");
    }

    #[test]
    fn the_symbols_of_parts_that_differ_in_direction_have_names_that_differ() {
        let giving = symbol(&solved_ldo(), "a:b").unwrap();
        let plain = symbol(&ldo(), "a:b").unwrap();
        assert_eq!(plain.lib_id(), "netsmith:Ldo8_DFN-8_EP");
        assert_eq!(giving.lib_id(), "netsmith:Ldo8_DFN-8_EP_out2");
        assert_ne!(giving.definition(), plain.definition());
        // The same table, solved the same, is the same bytes.
        let again = symbol(&solved_ldo(), "a:b").unwrap();
        assert_eq!(
            giving.definition().to_kicad_string(),
            again.definition().to_kicad_string()
        );
    }

    #[test]
    fn the_second_pad_of_a_pin_is_hidden_on_the_point_of_the_first() {
        let symbol = symbol(&ldo(), "a:b").unwrap();
        let first = symbol.pin("2").unwrap();
        let second = symbol.pin("7").unwrap();
        assert_eq!((first.x, first.y), (second.x, second.y));
        assert!(!first.hidden && second.hidden);
    }

    #[test]
    fn pins_stand_on_the_sides_the_editor_gives_them_all_on_the_grid_outside_the_box() {
        let symbol = symbol(&ldo(), "a:b").unwrap();
        let rect = symbol.definition().find_all("symbol").next().unwrap();
        let rectangle = rect.find("rectangle").unwrap();
        let corner = |tag: &str| -> (f64, f64) {
            let a = rectangle.find(tag).unwrap().args();
            (
                a[0].text().unwrap().parse().unwrap(),
                a[1].text().unwrap().parse().unwrap(),
            )
        };
        let (start, end) = (corner("start"), corner("end"));
        let on_grid = |v: f64| ((v / GRID).round() * GRID - v).abs() < 1e-3;
        for v in [start.0, start.1, end.0, end.1] {
            assert!(on_grid(v), "the box corner {v} is off the grid");
        }
        for p in symbol.pins() {
            assert!(
                on_grid(p.x) && on_grid(p.y),
                "pin {} is off the grid",
                p.number
            );
            assert!(on_grid(p.length));
        }
        let pin = |n: &str| symbol.pin(n).unwrap();
        // The supply above, the ground below, inputs left, outputs right.
        assert!(pin("1").y > end.1.max(start.1) - 1e-6 && pin("1").angle == 270.0);
        assert!(pin("9").y < start.1.min(end.1) + 1e-6 && pin("9").angle == 90.0);
        assert!(pin("3").x < start.0.min(end.0) && pin("3").angle == 0.0);
        assert!(pin("4").x > start.0.max(end.0) && pin("4").angle == 180.0);
        // Every pin line runs from outside the box to its edge.
        for p in symbol.pins() {
            let inner = (
                p.x + p.length * p.angle.to_radians().cos(),
                p.y + p.length * p.angle.to_radians().sin(),
            );
            let inside = |v: (f64, f64)| {
                v.0.abs() <= end.0.abs() + 1e-6 && v.1.abs() <= start.1.abs() + 1e-6
            };
            assert!(inside(inner) && !inside((p.x, p.y)), "pin {}", p.number);
        }
        // No two pins of different pads share a point, other than the stacked pads of one pin.
        for a in symbol.pins() {
            for b in symbol.pins() {
                if a.number != b.number && (a.x, a.y) == (b.x, b.y) {
                    assert!(a.hidden || b.hidden, "pins {} and {}", a.number, b.number);
                }
            }
        }
    }

    #[test]
    fn a_part_with_units_is_a_box_for_each_unit() {
        let supply = [
            pin("v", "VDD", PinKind::Power, &["1"]),
            pin("g", "GND", PinKind::Power, &["2"]),
        ];
        let signals = [
            pin("a", "A", PinKind::Input, &["3"]),
            pin("b", "B", PinKind::Output, &["4"]),
            pin("c", "C", PinKind::Bidirectional, &["5"]),
        ];
        let definition = draw(
            "Dual",
            "U",
            "a:b",
            &[supply.iter().collect(), signals.iter().collect()],
        );
        let symbol = Symbol::from_definition("netsmith:Dual", "Dual", definition).unwrap();
        assert_eq!(symbol.units(), 2);
        let boxes: Vec<_> = symbol
            .definition()
            .find_all("symbol")
            .map(|u| {
                (
                    u.args()[0].text().unwrap().to_string(),
                    u.find_all("rectangle").count(),
                )
            })
            .collect();
        assert_eq!(
            boxes,
            [("Dual_1_1".to_string(), 1), ("Dual_2_1".to_string(), 1)]
        );
        let unit_of = |pad: &str| symbol.pin(pad).unwrap().unit;
        assert_eq!(
            ["1", "2", "3", "4", "5"].map(unit_of),
            [1, 1, 2, 2, 2],
            "each pin is in the box of its unit"
        );
        assert_eq!(symbol.pins_of_unit(1).count(), 2);
        assert_eq!(symbol.pins_of_unit(2).count(), 3);
    }

    #[test]
    fn the_same_table_is_the_same_bytes_every_time() {
        let draw_once = || {
            symbol(&ldo(), "a:b")
                .unwrap()
                .definition()
                .to_kicad_string()
        };
        let first = draw_once();
        assert_eq!(first, draw_once());
        // The text is the same read back and written again.
        let again = Sexp::parse(&first).unwrap().to_kicad_string();
        assert_eq!(first, again);
        assert!(first.contains("(property \"Reference\" \"U\""), "{first}");
        assert!(first.contains("(property \"Footprint\" \"a:b\""), "{first}");
        assert!(
            first.contains("(property \"Value\" \"Ldo8_DFN-8_EP\""),
            "{first}"
        );
    }

    #[test]
    fn a_part_with_no_pin_table_has_no_symbol_and_the_name_is_safe_for_a_file() {
        assert!(symbol(&part("Resistor", Some("0402"), Vec::new()), "a:b").is_none());
        assert_eq!(symbol_name("Lt8641", Some("UDC20(18)")), "Lt8641_UDC20_18");
        assert_eq!(symbol_name("Mcu", None), "Mcu");
        assert_eq!(symbol_name("A B", Some("x/y")), "A_B_x_y");
    }

    #[test]
    fn a_box_wide_enough_for_long_names_and_many_pins_on_top_and_bottom_stays_on_the_grid() {
        let mut pins = Vec::new();
        for (i, name) in ["VERYLONGSUPPLY", "VDD", "VDDA", "VBAT", "VREF"]
            .iter()
            .enumerate()
        {
            pins.push(pin("p", name, PinKind::Power, &[&format!("{}", i + 1)]));
        }
        for (i, name) in ["GND", "AGNDX"].iter().enumerate() {
            pins.push(pin("n", name, PinKind::Power, &[&format!("{}", i + 20)]));
        }
        for i in 0..7 {
            pins.push(pin(
                "s",
                &format!("SIGNAL_NUMBER_{i}"),
                PinKind::Input,
                &[&format!("{}", i + 30)],
            ));
        }
        let symbol = symbol(&part("Big", None, pins), "a:b").unwrap();
        let on_grid = |v: f64| ((v / GRID).round() * GRID - v).abs() < 1e-3;
        assert_eq!(symbol.pins().len(), 14);
        for p in symbol.pins() {
            assert!(
                on_grid(p.x) && on_grid(p.y),
                "{} at {},{}",
                p.number,
                p.x,
                p.y
            );
        }
        // The five supply pins are a pitch of two steps or more apart.
        let mut xs: Vec<f64> = symbol
            .pins()
            .iter()
            .filter(|p| p.angle == 270.0)
            .map(|p| p.x)
            .collect();
        xs.sort_by(f64::total_cmp);
        assert!(xs.windows(2).all(|w| w[1] - w[0] >= 2.54 - 1e-6), "{xs:?}");
    }
}
