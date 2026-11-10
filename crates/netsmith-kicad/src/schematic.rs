//! Emit a KiCad 9 `.kicad_sch` from a [`Design`].
//!
//! Parts go on a grid and every connected pin gets a net label on its connection
//! point, so there are no wires to route. The result is correct but not pretty.
//! A symbol with several units is placed whole: every unit is its own instance with the
//! part's reference, in a column. A pin that is on no net, or on a net with no other pin,
//! gets a no-connect flag unless its symbol already types it "no connection". A net
//! that has a power input pin and nothing on the board to drive it gets a power flag, in
//! a row below the parts, so the electrical rules check knows it is supplied.
//! Every UUID is derived from names, so regenerating a design yields identical
//! files and keeps symbols linked to their PCB footprints.

use std::collections::HashMap;
use std::sync::Arc;

use netsmith_core::{Design, DesignError};
use uuid::Uuid;

use crate::mapping::{MapError, Mapped, Mapping, POWER_FLAG_SYMBOL};
use crate::sexp::Sexp;
use crate::symbols::{PinGeometry, PinType, Symbol, SymbolError, SymbolLib, SymbolPin};

/// Grid pitch in mm (KiCad's default connection grid).
const GRID: f64 = 1.27;
const COLUMNS: usize = 8;
const COLUMN_PITCH: f64 = 20.0 * GRID;
const ROW_PITCH: f64 = 20.0 * GRID;
const ORIGIN: (f64, f64) = (20.0 * GRID, 60.0 * GRID);
/// Space kept around a unit's pins.
const PAD: f64 = 2.0 * GRID;
/// How far one character of a net label reaches, in mm.
const LABEL_CHAR: f64 = GRID;
/// The space between two units of one part, beyond their pins' extent: the bodies between
/// the pins are about that high.
const UNIT_GAP: f64 = 4.0 * GRID;
/// A symbol with more pins than this reserves room for its labels; a small part keeps the
/// grid and its labels may run over the next cell.
const SMALL_SYMBOL_PINS: usize = 4;

/// Fixed namespace so UUIDs are stable across runs and machines ("netsmith" in ASCII).
const NAMESPACE: Uuid = Uuid::from_u128(0x6e65_7473_6d69_7468_0000_0000_0000_0001);

#[derive(Debug)]
pub enum EmitError {
    Design(DesignError),
    /// KiCad has no symbol or footprint for this part.
    Map {
        refdes: String,
        error: MapError,
    },
    /// The mapping names a symbol that cannot be read.
    Symbol {
        refdes: String,
        error: SymbolError,
    },
    UnknownPin {
        refdes: String,
        lib_id: String,
        pin: String,
    },
}

impl std::fmt::Display for EmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Design(e) => write!(f, "invalid design: {e}"),
            Self::Map { refdes, error } => write!(f, "{refdes}: {error}"),
            Self::Symbol { refdes, error } => write!(f, "{refdes}: {error}"),
            Self::UnknownPin {
                refdes,
                lib_id,
                pin,
            } => {
                write!(f, "{refdes} ({lib_id}) has no pin {pin}")
            }
        }
    }
}

impl std::error::Error for EmitError {}

impl From<DesignError> for EmitError {
    fn from(e: DesignError) -> Self {
        Self::Design(e)
    }
}

fn uuid_for(name: &str) -> String {
    Uuid::new_v5(&NAMESPACE, name.as_bytes()).to_string()
}

fn snap_up(v: f64) -> f64 {
    ((v / GRID) - 1e-9).ceil() * GRID
}

fn flag_refdes(index: usize) -> String {
    format!("#FLG{:02}", index + 1)
}

/// A pin of a part that a net reaches.
struct Connection<'a> {
    net: &'a str,
    pin: &'a SymbolPin,
}

/// How much room a part takes on the sheet, around its first unit's origin.
struct Cell {
    left: f64,
    right: f64,
    up: f64,
    down: f64,
    /// How far below the first unit's origin each unit's origin is.
    unit_dy: Vec<f64>,
}

/// The units a pin is drawn in: its own, or all of them for a shared pin.
fn drawn_in(pin: &SymbolPin, unit: u32) -> bool {
    pin.unit == unit || pin.unit == 0
}

/// The extent of one unit in the symbol's own coordinates (y up): `(min x, min y, max x,
/// max y)`, around the origin, with its pins, the labels that run from them and some air.
fn unit_extent(symbol: &Symbol, unit: u32, connections: &[Connection]) -> (f64, f64, f64, f64) {
    let mut b = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut include = |x: f64, y: f64| {
        b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
    };
    let pins: Vec<&SymbolPin> = symbol.pins().iter().filter(|p| drawn_in(p, unit)).collect();
    for pin in &pins {
        include(pin.x, pin.y);
        if pins.len() > SMALL_SYMBOL_PINS {
            let reach = connections
                .iter()
                .filter(|c| c.pin.number == pin.number)
                .map(|c| c.net.chars().count())
                .max()
                .unwrap_or(0) as f64
                * LABEL_CHAR;
            let away = (pin.angle + 180.0).to_radians();
            include(pin.x + reach * away.cos(), pin.y + reach * away.sin());
        }
    }
    (b.0 - PAD, b.1 - PAD, b.2 + PAD, b.3 + PAD)
}

fn plan_cell(symbol: &Symbol, connections: &[Connection]) -> Cell {
    let (mut left, mut right) = (COLUMN_PITCH / 2.0, COLUMN_PITCH / 2.0);
    let mut unit_dy = Vec::new();
    let (mut up, mut down) = (ROW_PITCH / 2.0, ROW_PITCH / 2.0);
    let mut previous_down = 0.0;
    for unit in 1..=symbol.units() {
        let (min_x, min_y, max_x, max_y) = unit_extent(symbol, unit, connections);
        left = left.max(snap_up(-min_x));
        right = right.max(snap_up(max_x));
        let dy = match unit_dy.last() {
            None => {
                up = up.max(snap_up(max_y));
                0.0
            }
            Some(&last) => last + snap_up(previous_down + max_y) + UNIT_GAP,
        };
        unit_dy.push(dy);
        previous_down = -min_y;
        down = down.max(dy + snap_up(previous_down));
    }
    Cell {
        left,
        right,
        up,
        down,
        unit_dy,
    }
}

/// Whether [`emit_schematic`] places a power flag on a net of `design`: some net takes power in
/// on a power input pin and nothing on the board drives it. The flag is the symbol
/// [`POWER_FLAG_SYMBOL`], which netsmith draws itself ([`crate::passives`]), so a board that
/// needs one needs its `netsmith.kicad_sym` to hold it ([`crate::stdlib::symbol_library`]).
/// A part that has no symbol is left to `emit_schematic` to report, and says nothing here.
pub fn needs_power_flag(design: &Design, mapping: &Mapping, lib: &SymbolLib) -> bool {
    let symbols: Vec<Option<Arc<Symbol>>> = design
        .parts
        .iter()
        .map(|part| {
            let mapped = mapping.map(&part.kind, part.package.as_deref()).ok()?;
            lib.symbol_of(part, &mapped).ok()
        })
        .collect();
    design.nets.iter().any(|net| {
        let kinds: Vec<Option<PinType>> = net
            .pins
            .iter()
            .map(|pin| {
                let index = design.parts.iter().position(|p| p.refdes == pin.refdes)?;
                symbols[index].as_ref()?.pin_type(&pin.pin)
            })
            .collect();
        kinds.contains(&Some(PinType::PowerIn)) && !kinds.contains(&Some(PinType::PowerOut))
    })
}

pub fn emit_schematic(
    design: &Design,
    mapping: &Mapping,
    lib: &SymbolLib,
) -> Result<String, EmitError> {
    design.validate()?;

    let root = uuid_for(&design.name);
    let mut lib_symbols: Vec<Sexp> = Vec::new();
    let mut placed: Vec<Sexp> = Vec::new();
    let mut labels: Vec<Sexp> = Vec::new();
    let mut no_connects: Vec<Sexp> = Vec::new();

    let mut mapped: Vec<(Mapped, Arc<Symbol>)> = Vec::new();
    for part in &design.parts {
        let m = mapping
            .map(&part.kind, part.package.as_deref())
            .map_err(|error| EmitError::Map {
                refdes: part.refdes.clone(),
                error,
            })?;
        let symbol = lib.symbol_of(part, &m).map_err(|error| EmitError::Symbol {
            refdes: part.refdes.clone(),
            error,
        })?;
        if !lib_symbols.iter().any(|s| s == symbol.definition()) {
            lib_symbols.push(symbol.definition().clone());
        }
        mapped.push((m, symbol));
    }

    // The pins each part's nets reach, and nets that take power in and have nothing on the
    // board to supply it.
    let mut connections: Vec<Vec<Connection>> = design.parts.iter().map(|_| Vec::new()).collect();
    let mut unsupplied: Vec<&str> = Vec::new();
    for net in &design.nets {
        let (mut takes, mut supplies) = (false, false);
        for pin in &net.pins {
            let index = design
                .parts
                .iter()
                .position(|p| p.refdes == pin.refdes)
                .expect("validate() guarantees the part exists");
            let (_, symbol) = &mapped[index];
            let sym_pin = symbol.pin(&pin.pin).ok_or_else(|| EmitError::UnknownPin {
                refdes: pin.refdes.clone(),
                lib_id: symbol.lib_id().to_string(),
                pin: pin.pin.clone(),
            })?;
            match sym_pin.kind {
                PinType::PowerIn => takes = true,
                PinType::PowerOut => supplies = true,
                _ => {}
            }
            connections[index].push(Connection {
                net: &net.name,
                pin: sym_pin,
            });
        }
        if takes && !supplies {
            unsupplied.push(&net.name);
        }
    }

    // Parts go in rows of `COLUMNS`; a cell is as big as the part needs, and no smaller
    // than the grid.
    let cells: Vec<Cell> = mapped
        .iter()
        .zip(&connections)
        .map(|((_, symbol), c)| plan_cell(symbol, c))
        .collect();
    let mut origins: Vec<(f64, f64)> = Vec::new();
    let mut row_top = ORIGIN.1 - ROW_PITCH / 2.0;
    for row in cells.chunks(COLUMNS) {
        let mut x = ORIGIN.0 - COLUMN_PITCH / 2.0;
        for cell in row {
            origins.push((x + cell.left, row_top + cell.up));
            x += cell.left + cell.right;
        }
        row_top += row.iter().map(|c| c.up + c.down).fold(ROW_PITCH, f64::max);
    }

    // A label is a connection by name, so a net with a label at one point only is connected
    // to nothing and KiCad calls the label dangling. Such a net is left unlabelled and its
    // pin is flagged like any pin that no net reaches, unless a power flag will be labelled
    // on the net as well.
    let mut label_points: HashMap<&str, usize> = HashMap::new();
    for (index, (_, symbol)) in mapped.iter().enumerate() {
        for unit in 1..=symbol.units() {
            let mut seen: Vec<(f64, f64, &str)> = Vec::new();
            for c in connections[index].iter().filter(|c| drawn_in(c.pin, unit)) {
                if !seen.contains(&(c.pin.x, c.pin.y, c.net)) {
                    seen.push((c.pin.x, c.pin.y, c.net));
                    *label_points.entry(c.net).or_default() += 1;
                }
            }
        }
    }
    let lonely = |net: &str| label_points.get(net) == Some(&1) && !unsupplied.contains(&net);

    for (index, part) in design.parts.iter().enumerate() {
        let (m, symbol) = &mapped[index];
        for unit in 1..=symbol.units() {
            let origin = (
                origins[index].0,
                origins[index].1 + cells[index].unit_dy[unit as usize - 1],
            );
            let placement = Placement {
                refdes: &part.refdes,
                value: &part.value,
                symbol,
                footprint: &m.footprint,
                unit,
                hidden_reference: false,
                real: true,
            };
            placed.push(placed_symbol(design, &placement, origin, &root));

            // One label for each distinct point and net: stacked pins share a point.
            let mut labelled: Vec<(f64, f64, &str)> = Vec::new();
            for c in connections[index]
                .iter()
                .filter(|c| drawn_in(c.pin, unit) && !lonely(c.net))
            {
                if labelled.contains(&(c.pin.x, c.pin.y, c.net)) {
                    continue;
                }
                labelled.push((c.pin.x, c.pin.y, c.net));
                labels.push(pin_label(
                    c.net,
                    origin,
                    c.pin.geometry(),
                    &format!("{}/{}/{}", c.net, part.refdes, c.pin.number),
                ));
            }
            // A pin that no net reaches is left open on purpose, so the electrical rules
            // check is told so. A pin the symbol already types "no connection" is not
            // flagged: KiCad counts the flag as a connection to it and objects.
            let mut flagged: Vec<(f64, f64)> = Vec::new();
            for pin in symbol.pins_of_unit(unit) {
                let at = (pin.x, pin.y);
                if pin.kind == PinType::NoConnect
                    || labelled.iter().any(|l| (l.0, l.1) == at)
                    || flagged.contains(&at)
                {
                    continue;
                }
                flagged.push(at);
                no_connects.push(no_connect(
                    (origin.0 + pin.x, origin.1 - pin.y),
                    &format!("{}/{}/{}/{unit}", design.name, part.refdes, pin.number),
                ));
            }
        }
    }

    unsupplied.sort_unstable();
    if !unsupplied.is_empty() {
        let flag = lib
            .symbol(POWER_FLAG_SYMBOL)
            .map_err(|error| EmitError::Symbol {
                refdes: flag_refdes(0),
                error,
            })?;
        lib_symbols.push(flag.definition().clone());
        let flags_top = row_top;
        for (index, net) in unsupplied.iter().enumerate() {
            let refdes = flag_refdes(index);
            let origin = (
                ORIGIN.0 + (index % COLUMNS) as f64 * COLUMN_PITCH,
                flags_top + (index / COLUMNS) as f64 * ROW_PITCH + ROW_PITCH / 2.0,
            );
            let placement = Placement {
                refdes: &refdes,
                value: "PWR_FLAG",
                symbol: &flag,
                footprint: "",
                unit: 1,
                hidden_reference: true,
                real: false,
            };
            placed.push(placed_symbol(design, &placement, origin, &root));
            let geometry = flag
                .pin_geometry("1")
                .expect("the power flag symbol has a pin 1");
            labels.push(pin_label(
                net,
                origin,
                geometry,
                &format!("{net}/{refdes}/1"),
            ));
        }
    }

    let mut items = vec![
        Sexp::node("version", vec![Sexp::atom("20250114")]),
        Sexp::node("generator", vec![Sexp::string("netsmith")]),
        Sexp::node(
            "generator_version",
            vec![Sexp::string(env!("CARGO_PKG_VERSION"))],
        ),
        Sexp::node("uuid", vec![Sexp::string(root.clone())]),
        Sexp::node("paper", vec![Sexp::string("A4")]),
        Sexp::node("lib_symbols", lib_symbols),
    ];
    items.extend(no_connects);
    items.extend(labels);
    items.extend(placed);
    items.push(Sexp::node(
        "sheet_instances",
        vec![Sexp::node(
            "path",
            vec![
                Sexp::string("/"),
                Sexp::node("page", vec![Sexp::string("1")]),
            ],
        )],
    ));
    items.push(Sexp::node("embedded_fonts", vec![Sexp::atom("no")]));

    Ok(Sexp::node("kicad_sch", items).to_kicad_string())
}

/// A bare project file so KiCad opens the schematic as part of a project.
pub fn project_file(design_name: &str) -> String {
    format!(
        "{{\n  \"meta\": {{\n    \"filename\": \"{design_name}.kicad_pro\",\n    \"version\": 3\n  }}\n}}\n"
    )
}

fn effects(hide: bool) -> Sexp {
    let mut items = vec![Sexp::node(
        "font",
        vec![Sexp::node("size", vec![Sexp::num(1.27), Sexp::num(1.27)])],
    )];
    if hide {
        items.push(Sexp::node("hide", vec![Sexp::atom("yes")]));
    }
    Sexp::node("effects", items)
}

fn at(x: f64, y: f64, angle: f64) -> Sexp {
    Sexp::node("at", vec![Sexp::num(x), Sexp::num(y), Sexp::num(angle)])
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

fn visible_property(name: &str, value: &str, pos: (f64, f64), angle: f64) -> Sexp {
    Sexp::node(
        "property",
        vec![
            Sexp::string(name),
            Sexp::string(value),
            at(pos.0, pos.1, angle),
            effects(false),
        ],
    )
}

/// A net label on a pin's connection point, running away from the symbol body.
fn pin_label(net: &str, origin: (f64, f64), geometry: PinGeometry, uuid_name: &str) -> Sexp {
    // Symbol y points up, sheet y points down.
    let at = (origin.0 + geometry.x, origin.1 - geometry.y);
    // The pin runs toward the body, so the label goes the opposite way.
    let away = (geometry.angle + 180.0).rem_euclid(360.0);
    label(net, at, away, uuid_name)
}

fn label(net: &str, pos: (f64, f64), angle: f64, uuid_name: &str) -> Sexp {
    // KiCad writes labels that run left or down with right-justified text.
    let justify = if angle >= 180.0 { "right" } else { "left" };
    Sexp::node(
        "label",
        vec![
            Sexp::string(net),
            at(pos.0, pos.1, angle),
            Sexp::node(
                "effects",
                vec![
                    Sexp::node(
                        "font",
                        vec![Sexp::node("size", vec![Sexp::num(1.27), Sexp::num(1.27)])],
                    ),
                    Sexp::node("justify", vec![Sexp::atom(justify), Sexp::atom("bottom")]),
                ],
            ),
            Sexp::node(
                "uuid",
                vec![Sexp::string(uuid_for(&format!("label/{uuid_name}")))],
            ),
        ],
    )
}

/// A no-connect flag on a pin's connection point (sheet coordinates).
fn no_connect(at: (f64, f64), uuid_name: &str) -> Sexp {
    Sexp::node(
        "no_connect",
        vec![
            Sexp::node("at", vec![Sexp::num(at.0), Sexp::num(at.1)]),
            Sexp::node(
                "uuid",
                vec![Sexp::string(uuid_for(&format!("no_connect/{uuid_name}")))],
            ),
        ],
    )
}

/// One symbol instance on the sheet: one unit of a part from the design, or a power flag.
struct Placement<'a> {
    refdes: &'a str,
    value: &'a str,
    symbol: &'a Symbol,
    footprint: &'a str,
    /// Which unit of the symbol this instance is (1-based).
    unit: u32,
    /// The reference is only for the tool's bookkeeping, so it is not drawn.
    hidden_reference: bool,
    /// Whether it is a real component, which goes in the bill of materials and on the board.
    real: bool,
}

fn placed_symbol(design: &Design, placement: &Placement, origin: (f64, f64), root: &str) -> Sexp {
    // The first unit keeps the names it had when every symbol had one unit.
    let unit_key = if placement.unit == 1 {
        String::new()
    } else {
        format!("u{}/", placement.unit)
    };
    let key = |suffix: &str| uuid_for(&format!("{}/{}/{suffix}", design.name, placement.refdes));
    let unit_suffix = |suffix: &str| match placement.unit {
        1 => suffix.to_string(),
        n => format!("{suffix}/u{n}"),
    };
    let (x, y) = origin;
    // Fields sit where the symbol says they should (sheet y points down).
    let field = |name: &str, value: &str, hide: bool| {
        let (dx, dy, rot) = placement
            .symbol
            .field_offset(name)
            .unwrap_or((0.0, 0.0, 0.0));
        if hide {
            property(name, value, (x + dx, y - dy), true)
        } else {
            visible_property(name, value, (x + dx, y - dy), rot)
        }
    };
    let mut items = vec![
        Sexp::node("lib_id", vec![Sexp::string(placement.symbol.lib_id())]),
        at(x, y, 0.0),
        Sexp::node("unit", vec![Sexp::atom(placement.unit.to_string())]),
        Sexp::node("exclude_from_sim", vec![Sexp::atom("no")]),
        Sexp::node("in_bom", vec![Sexp::yes_no(placement.real)]),
        Sexp::node("on_board", vec![Sexp::yes_no(placement.real)]),
        Sexp::node("dnp", vec![Sexp::atom("no")]),
        Sexp::node("fields_autoplaced", vec![Sexp::atom("yes")]),
        Sexp::node("uuid", vec![Sexp::string(key(&unit_suffix("symbol")))]),
        field("Reference", placement.refdes, placement.hidden_reference),
        field("Value", placement.value, false),
        property("Footprint", placement.footprint, (x, y), true),
        property("Datasheet", "~", (x, y), true),
        property("Description", "", (x, y), true),
    ];
    for pin in placement.symbol.pins_of_unit(placement.unit) {
        items.push(Sexp::node(
            "pin",
            vec![
                Sexp::string(&pin.number),
                Sexp::node(
                    "uuid",
                    vec![Sexp::string(key(&format!("{unit_key}pin/{}", pin.number)))],
                ),
            ],
        ));
    }
    items.push(Sexp::node(
        "instances",
        vec![Sexp::node(
            "project",
            vec![
                Sexp::string(&design.name),
                Sexp::node(
                    "path",
                    vec![
                        Sexp::string(format!("/{root}")),
                        Sexp::node("reference", vec![Sexp::string(placement.refdes)]),
                        Sexp::node("unit", vec![Sexp::atom(placement.unit.to_string())]),
                    ],
                ),
            ],
        )],
    ));
    Sexp::node("symbol", items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testlib::{self, TempDir};
    use netsmith_core::{Net, Part, PartPin, PinKind, PinRef};

    /// The sample libraries of `testlib`, which hold what the standard mapping names.
    fn sample() -> SymbolLib {
        SymbolLib::at(testlib::sample_symbols())
    }

    /// The standard mapping and two regulators: `Ldo`, drawn with the sample library's AP1117
    /// symbol, which has a power output, and `IcLdo`, whose row names no symbol, so it is drawn
    /// from its pin table, whose power pins are all inputs.
    fn mapping() -> Mapping {
        let row = "[[part]]\nkind = \"Ldo\"\npackage = \"SOT-23\"\nsymbol = \"Regulator_Linear:AP1117-15\"\nfootprint = \"Package_TO_SOT_SMD:SOT-23\"\n\n\
                   [[part]]\nkind = \"IcLdo\"\npackage = \"SOT-23\"\nfootprint = \"Package_TO_SOT_SMD:SOT-23\"\n";
        Mapping::standard()
            .merged(Mapping::parse(row, "test", std::path::Path::new(".")).unwrap())
            .unwrap()
    }

    /// A 300 ohm resistor feeding an LED, with nets named the way the design stage names them.
    fn led_design() -> Design {
        let part = |refdes: &str, kind: &str, value: &str, package: Option<&str>| Part {
            provides: Vec::new(),
            refdes: refdes.into(),
            kind: kind.into(),
            value: value.into(),
            package: package.map(str::to_string),
            path: String::new(),
            pins: Vec::new(),
        };
        let pin = |refdes: &str, pin: &str| PinRef {
            refdes: refdes.into(),
            pin: pin.into(),
        };
        Design {
            name: "led".into(),
            process: None,
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts: vec![
                part("R1", "Resistor", "300", Some("0402")),
                part("D1", "Led", "LED", None),
                part("J1", "Header1x2", "Conn_01x02", None),
            ],
            nets: vec![
                Net {
                    name: "VCC".into(),
                    pins: vec![pin("R1", "1"), pin("J1", "1")],
                },
                Net {
                    name: "net-led.anode".into(),
                    pins: vec![pin("R1", "2"), pin("D1", "2")],
                },
                Net {
                    name: "GND".into(),
                    pins: vec![pin("D1", "1"), pin("J1", "2")],
                },
            ],
        }
    }

    fn parsed(design: &Design) -> Sexp {
        Sexp::parse(&emit_schematic(design, &mapping(), &sample()).unwrap()).unwrap()
    }

    fn label_at(root: &Sexp, net: &str, nth: usize) -> Sexp {
        root.find_all("label")
            .filter(|l| l.args()[0].text() == Some(net))
            .nth(nth)
            .unwrap()
            .clone()
    }

    #[test]
    fn output_is_well_formed_kicad_sch() {
        let root = parsed(&led_design());
        assert_eq!(root.head(), Some("kicad_sch"));
        assert_eq!(root.find_all("symbol").count(), 3);
        assert_eq!(root.find_all("label").count(), 6);
        assert_eq!(
            root.find("lib_symbols").unwrap().find_all("symbol").count(),
            3
        );
    }

    #[test]
    fn the_mapping_supplies_the_symbol_and_the_footprint() {
        let text = emit_schematic(&led_design(), &mapping(), &sample()).unwrap();
        assert!(text.contains("(lib_id \"Device:R\")"));
        assert!(text.contains("(lib_id \"Device:LED\")"));
        assert!(text.contains("\"Resistor_SMD:R_0402_1005Metric\""));
        // The LED was given no package, so it takes the default for its kind.
        assert!(text.contains("\"LED_SMD:LED_0603_1608Metric\""));
    }

    #[test]
    fn labels_run_away_from_the_symbol_body() {
        let root = parsed(&led_design());
        let angle = |net: &str, nth: usize| {
            label_at(&root, net, nth).find("at").unwrap().args()[2]
                .text()
                .unwrap()
                .to_string()
        };
        // R1 pin 1 is its top pin: the label points up. D1's cathode is on the left: left.
        assert_eq!(angle("VCC", 0), "90");
        assert_eq!(angle("GND", 0), "180");
        // D1's anode is on the right: right.
        assert_eq!(angle("net-led.anode", 1), "0");
    }

    #[test]
    fn output_is_byte_for_byte_stable() {
        let a = emit_schematic(&led_design(), &mapping(), &sample()).unwrap();
        let b = emit_schematic(&led_design(), &mapping(), &sample()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn printing_the_output_again_changes_nothing() {
        let text = emit_schematic(&led_design(), &mapping(), &sample()).unwrap();
        assert_eq!(Sexp::parse(&text).unwrap().to_kicad_string(), text);
    }

    #[test]
    fn labels_sit_on_pin_connection_points() {
        let root = parsed(&led_design());
        // R1 is the first part, so its origin is the sheet origin.
        let (rx, ry) = ORIGIN;
        let at = label_at(&root, "VCC", 0);
        let a = at.find("at").unwrap().args();
        let (x, y): (f64, f64) = (
            a[0].text().unwrap().parse().unwrap(),
            a[1].text().unwrap().parse().unwrap(),
        );
        // R1 pin 1 is the top pin (symbol y = +3.81 => sheet y smaller).
        assert!(
            (x - rx).abs() < 1e-3 && (y - (ry - 3.81)).abs() < 1e-3,
            "got {x},{y}"
        );
    }

    #[test]
    fn uuids_depend_only_on_names() {
        let mut d = led_design();
        let before = emit_schematic(&d, &mapping(), &sample()).unwrap();
        d.parts[0].value = "330".into();
        let after = emit_schematic(&d, &mapping(), &sample()).unwrap();
        let uuids = |t: &str| -> Vec<String> {
            t.lines()
                .filter(|l| l.trim_start().starts_with("(uuid"))
                .map(str::to_string)
                .collect()
        };
        assert_eq!(uuids(&before), uuids(&after));
        assert_ne!(before, after);
    }

    #[test]
    fn a_part_kind_kicad_does_not_know_is_an_error() {
        let mut d = led_design();
        d.parts[0].kind = "Nope".into();
        assert!(matches!(
            emit_schematic(&d, &mapping(), &sample()),
            Err(EmitError::Map {
                error: MapError::UnknownPart(_),
                ..
            })
        ));
    }

    #[test]
    fn a_resistor_without_a_package_is_an_error() {
        let mut d = led_design();
        d.parts[0].package = None;
        assert!(matches!(
            emit_schematic(&d, &mapping(), &sample()),
            Err(EmitError::Map {
                error: MapError::NeedsPackage(_),
                ..
            })
        ));
    }

    #[test]
    fn unknown_pin_is_an_error() {
        let mut d = led_design();
        d.nets[1].pins[1].pin = "9".into();
        assert!(matches!(
            emit_schematic(&d, &mapping(), &sample()),
            Err(EmitError::UnknownPin { .. })
        ));
    }

    /// A header feeds a regulator: VIN and GND are only taken from, 3V3 is driven by the
    /// regulator's output.
    fn regulator_design() -> Design {
        let part = |refdes: &str, kind: &str, value: &str, package: Option<&str>| Part {
            provides: Vec::new(),
            refdes: refdes.into(),
            kind: kind.into(),
            value: value.into(),
            package: package.map(str::to_string),
            path: String::new(),
            pins: Vec::new(),
        };
        let pin = |refdes: &str, pin: &str| PinRef {
            refdes: refdes.into(),
            pin: pin.into(),
        };
        Design {
            name: "regulator".into(),
            process: None,
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts: vec![
                part("J1", "Header1x2", "Conn_01x02", None),
                part("C1", "Capacitor", "1u", Some("0402")),
                part("U1", "Ldo", "XC6206", Some("SOT-23")),
            ],
            nets: vec![
                Net {
                    name: "VIN".into(),
                    pins: vec![pin("J1", "1"), pin("C1", "1"), pin("U1", "3")],
                },
                Net {
                    name: "GND".into(),
                    pins: vec![pin("J1", "2"), pin("C1", "2"), pin("U1", "1")],
                },
                Net {
                    name: "3V3".into(),
                    pins: vec![pin("U1", "2")],
                },
            ],
        }
    }

    /// The placed symbols that are power flags, as `(reference, net)` pairs, where the net
    /// is read from the label on the flag's connection point.
    fn flags(root: &Sexp) -> Vec<(String, String)> {
        let at_of = |node: &Sexp| -> (String, String) {
            let a = node.find("at").unwrap().args();
            (a[0].text().unwrap().into(), a[1].text().unwrap().into())
        };
        root.find_all("symbol")
            .filter(|s| {
                s.find("lib_id")
                    .is_some_and(|l| l.args()[0].text() == Some("power:PWR_FLAG"))
            })
            .map(|s| {
                let reference = s
                    .find_all("property")
                    .find(|p| p.args()[0].text() == Some("Reference"))
                    .unwrap()
                    .args()[1]
                    .text()
                    .unwrap()
                    .to_string();
                let origin = at_of(s);
                let label = root
                    .find_all("label")
                    .find(|l| at_of(l) == origin)
                    .expect("a label on the flag's pin");
                (reference, label.args()[0].text().unwrap().to_string())
            })
            .collect()
    }

    #[test]
    fn a_net_that_only_takes_power_gets_a_flag() {
        let root = parsed(&regulator_design());
        // Net-name order: GND before VIN. 3V3 has the regulator's output on it.
        assert_eq!(
            flags(&root),
            vec![
                ("#FLG01".to_string(), "GND".to_string()),
                ("#FLG02".to_string(), "VIN".to_string()),
            ]
        );
    }

    #[test]
    fn a_net_with_a_power_output_gets_no_flag() {
        let mut d = regulator_design();
        // Put the regulator's output pin on VIN as well: now VIN has a driver.
        d.nets[0].pins.push(PinRef {
            refdes: "U1".into(),
            pin: "2".into(),
        });
        d.nets.pop();
        let root = parsed(&d);
        assert_eq!(
            flags(&root),
            vec![("#FLG01".to_string(), "GND".to_string())]
        );
    }

    #[test]
    fn nets_of_passive_pins_get_no_flag() {
        let root = parsed(&led_design());
        assert!(flags(&root).is_empty());
        let text = emit_schematic(&led_design(), &mapping(), &sample()).unwrap();
        assert!(!text.contains("PWR_FLAG"));
    }

    #[test]
    fn flags_sit_below_the_parts_and_the_symbol_is_embedded_once() {
        let d = regulator_design();
        let text = emit_schematic(&d, &mapping(), &sample()).unwrap();
        let root = Sexp::parse(&text).unwrap();
        let y_of = |s: &Sexp| -> f64 {
            s.find("at").unwrap().args()[1]
                .text()
                .unwrap()
                .parse()
                .unwrap()
        };
        let parts_y = ORIGIN.1;
        let flag_ys: Vec<f64> = root
            .find_all("symbol")
            .filter(|s| {
                s.find("lib_id")
                    .is_some_and(|l| l.args()[0].text() == Some(POWER_FLAG_SYMBOL))
            })
            .map(y_of)
            .collect();
        assert_eq!(flag_ys.len(), 2);
        assert!(flag_ys.iter().all(|y| *y > parts_y));
        assert_eq!(
            root.find("lib_symbols").unwrap().find_all("symbol").count(),
            4
        );
    }

    #[test]
    fn flags_are_not_in_the_bill_of_materials_or_on_the_board() {
        let root = parsed(&regulator_design());
        let flag = root
            .find_all("symbol")
            .find(|s| {
                s.find("lib_id")
                    .is_some_and(|l| l.args()[0].text() == Some(POWER_FLAG_SYMBOL))
            })
            .unwrap();
        assert_eq!(flag.find("in_bom").unwrap().args()[0].text(), Some("no"));
        assert_eq!(flag.find("on_board").unwrap().args()[0].text(), Some("no"));
        // A real part is in both.
        let real = root
            .find_all("symbol")
            .find(|s| {
                s.find("lib_id")
                    .is_some_and(|l| l.args()[0].text() == Some("Device:C"))
            })
            .unwrap();
        assert_eq!(real.find("in_bom").unwrap().args()[0].text(), Some("yes"));
        assert_eq!(real.find("on_board").unwrap().args()[0].text(), Some("yes"));
    }

    /// `regulator_design()` with the `IcLdo` that names no symbol, and the pin table its part
    /// has in the package.
    fn drawn_regulator_design() -> Design {
        let mut d = regulator_design();
        let pin = |name: &str, datasheet: &str, kind: PinKind, pad: &str| PartPin {
            name: name.into(),
            datasheet: datasheet.into(),
            kind,
            pads: vec![pad.into()],
            function: None,
            power_flow: None,
        };
        d.parts[2].kind = "IcLdo".into();
        d.parts[2].pins = vec![
            pin("vin.pos", "IN", PinKind::Power, "3"),
            pin("vin.neg", "GND", PinKind::Power, "1"),
            pin("vout.pos", "OUT", PinKind::Power, "2"),
        ];
        d
    }

    #[test]
    fn an_ic_is_drawn_from_its_pin_table_and_its_symbol_is_embedded_once_in_the_library_netsmith() {
        let d = drawn_regulator_design();
        // The sample library has `Regulator_Linear`; the generated symbol must not read it.
        let text = emit_schematic(&d, &mapping(), &sample()).unwrap();
        let root = Sexp::parse(&text).unwrap();
        assert!(text.contains("(lib_id \"netsmith:IcLdo_SOT-23\")"));
        assert!(!text.contains("Regulator_Linear"));
        let embedded: Vec<&str> = root
            .find("lib_symbols")
            .unwrap()
            .find_all("symbol")
            .filter_map(|s| s.args()[0].text())
            .collect();
        assert_eq!(
            embedded
                .iter()
                .filter(|n| **n == "netsmith:IcLdo_SOT-23")
                .count(),
            1,
            "{embedded:?}"
        );
        // A label on each pin of the table, at the pin's point, named for its net.
        for net in ["VIN", "GND", "3V3"] {
            assert!(
                root.find_all("label")
                    .any(|l| l.args()[0].text() == Some(net)),
                "{net}"
            );
        }
        // Its power pins are inputs while no direction is known, so a net nothing drives is
        // flagged, the output's too.
        let mut flagged: Vec<String> = flags(&root).into_iter().map(|f| f.1).collect();
        flagged.sort();
        assert_eq!(flagged, ["3V3", "GND", "VIN"]);
        // The same bytes every time.
        assert_eq!(text, emit_schematic(&d, &mapping(), &sample()).unwrap());
    }

    #[test]
    fn the_output_of_an_ic_drawn_from_its_pin_table_is_a_power_output_and_its_net_gets_no_flag() {
        let mut d = drawn_regulator_design();
        for pin in &mut d.parts[2].pins {
            match pin.name.as_str() {
                "vin.pos" => pin.power_flow = Some(netsmith_core::PowerFlow::Takes),
                "vout.pos" => pin.power_flow = Some(netsmith_core::PowerFlow::Gives),
                _ => {}
            }
        }
        let text = emit_schematic(&d, &mapping(), &sample()).unwrap();
        let root = Sexp::parse(&text).unwrap();
        // One more symbol than for the same part with no direction: the output is another type.
        assert!(
            text.contains("(lib_id \"netsmith:IcLdo_SOT-23_out2\")"),
            "{text}"
        );
        let embedded = root
            .find("lib_symbols")
            .unwrap()
            .find_all("symbol")
            .find(|s| s.args()[0].text() == Some("netsmith:IcLdo_SOT-23_out2"))
            .expect("the symbol drawn for the solved part is embedded");
        let pin_types: Vec<(String, String)> = embedded
            .find_all("symbol")
            .flat_map(|unit| unit.find_all("pin"))
            .map(|p| {
                (
                    p.find("number").unwrap().args()[0].text().unwrap().into(),
                    p.args()[0].text().unwrap().into(),
                )
            })
            .collect();
        let type_of = |pad: &str| {
            pin_types
                .iter()
                .find(|(n, _)| n == pad)
                .map(|(_, t)| t.as_str())
        };
        assert_eq!(
            [type_of("1"), type_of("2"), type_of("3")],
            [Some("power_in"), Some("power_out"), Some("power_in")]
        );
        // The regulator's output net has a source, so only the two nets that nothing on the
        // board supplies are flagged.
        let mut flagged: Vec<String> = flags(&root).into_iter().map(|f| f.1).collect();
        flagged.sort();
        assert_eq!(flagged, ["GND", "VIN"]);
        // The same bytes every time.
        assert_eq!(text, emit_schematic(&d, &mapping(), &sample()).unwrap());
    }

    #[test]
    fn a_part_with_no_symbol_in_its_row_and_no_pin_table_is_refused_by_name() {
        let e = emit_schematic(&regulator_design_with("IcLdo"), &mapping(), &sample())
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("U1") && e.contains("no `symbol`") && e.contains("no pin table"),
            "{e}"
        );
    }

    fn regulator_design_with(kind: &str) -> Design {
        let mut d = regulator_design();
        d.parts[2].kind = kind.into();
        d
    }

    #[test]
    fn flag_output_is_byte_for_byte_stable() {
        let a = emit_schematic(&regulator_design(), &mapping(), &sample()).unwrap();
        let b = emit_schematic(&regulator_design(), &mapping(), &sample()).unwrap();
        assert_eq!(a, b);
        assert_eq!(Sexp::parse(&a).unwrap().to_kicad_string(), a);
    }

    #[test]
    fn a_flags_uuids_come_from_its_name_and_its_net() {
        let text = emit_schematic(&regulator_design(), &mapping(), &sample()).unwrap();
        assert!(text.contains(&uuid_for("regulator/#FLG01/symbol")));
        assert!(text.contains(&uuid_for("regulator/#FLG01/pin/1")));
        assert!(text.contains(&uuid_for("label/GND/#FLG01/1")));
        assert!(text.contains(&uuid_for("label/VIN/#FLG02/1")));
        // Changing a part's value moves no UUID, flags included.
        let mut d = regulator_design();
        d.parts[1].value = "2u2".into();
        let other = emit_schematic(&d, &mapping(), &sample()).unwrap();
        assert!(other.contains(&uuid_for("regulator/#FLG02/symbol")));
    }

    /// The standard mapping plus a kind `Gadget` per symbol of the test library `acme`,
    /// each in package `P`.
    fn acme(symbols: &[&str]) -> (TempDir, Mapping, SymbolLib) {
        let dir = TempDir::new("emit");
        let file = dir.path().join("acme.kicad_sym");
        std::fs::write(&file, testlib::acme()).unwrap();
        let rows: String = symbols
            .iter()
            .map(|s| {
                format!(
                    "[[part]]\nkind = \"{s}\"\npackage = \"P\"\nsymbol = \"acme:{s}\"\nfootprint = \"acme:F\"\n"
                )
            })
            .collect();
        let mapping = Mapping::standard()
            .merged(Mapping::parse(&rows, "kicad.toml", dir.path()).unwrap())
            .unwrap();
        let lib = SymbolLib::at(testlib::sample_symbols()).with_library("acme", file);
        (dir, mapping, lib)
    }

    fn gadget(refdes: &str, kind: &str) -> Part {
        Part {
            provides: Vec::new(),
            refdes: refdes.into(),
            kind: kind.into(),
            value: kind.into(),
            package: Some("P".into()),
            path: String::new(),
            pins: Vec::new(),
        }
    }

    fn design(parts: Vec<Part>, nets: &[(&str, &[(&str, &str)])]) -> Design {
        Design {
            name: "gadgets".into(),
            process: None,
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts,
            nets: nets
                .iter()
                .map(|(name, pins)| Net {
                    name: (*name).into(),
                    pins: pins
                        .iter()
                        .map(|(r, p)| PinRef {
                            refdes: (*r).into(),
                            pin: (*p).into(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn number(node: &Sexp, i: usize) -> f64 {
        node.find("at").unwrap().args()[i]
            .text()
            .unwrap()
            .parse()
            .unwrap()
    }

    fn placed_of<'a>(root: &'a Sexp, refdes: &str) -> Vec<&'a Sexp> {
        root.find_all("symbol")
            .filter(|s| {
                s.find_all("property").any(|p| {
                    p.args()[0].text() == Some("Reference") && p.args()[1].text() == Some(refdes)
                })
            })
            .collect()
    }

    #[test]
    fn pads_stacked_on_one_point_get_one_label() {
        let (_dir, mapping, lib) = acme(&["Stacked"]);
        let d = design(
            vec![gadget("U1", "Stacked"), gadget("U2", "Stacked")],
            &[
                ("OUT", &[("U1", "1"), ("U1", "2"), ("U1", "3"), ("U2", "1")]),
                ("IN", &[("U1", "4"), ("U2", "4")]),
            ],
        );
        let root = Sexp::parse(&emit_schematic(&d, &mapping, &lib).unwrap()).unwrap();
        let out: Vec<_> = root
            .find_all("label")
            .filter(|l| l.args()[0].text() == Some("OUT"))
            .collect();
        assert_eq!(
            out.len(),
            2,
            "one for U2, one for the three pads of U1 on a point"
        );
        // Every pad is still a pin of the instance.
        let u1 = placed_of(&root, "U1")[0];
        assert_eq!(u1.find_all("pin").count(), 5);
    }

    #[test]
    fn two_nets_on_one_point_each_keep_a_label() {
        // Pads 1 and 2 are stacked, but the design puts them on different nets: that is a
        // short in the symbol, which KiCad's ERC should be able to see, so both stay.
        let (_dir, mapping, lib) = acme(&["Stacked"]);
        let d = design(
            vec![gadget("U1", "Stacked"), gadget("U2", "Stacked")],
            &[
                ("A", &[("U1", "1"), ("U2", "1")]),
                ("B", &[("U1", "2"), ("U2", "2")]),
            ],
        );
        let root = Sexp::parse(&emit_schematic(&d, &mapping, &lib).unwrap()).unwrap();
        assert_eq!(root.find_all("label").count(), 4);
    }

    #[test]
    fn a_pin_on_no_net_gets_a_no_connect_flag_on_its_point() {
        let (_dir, mapping, lib) = acme(&["Reg"]);
        let d = design(
            vec![gadget("U1", "Reg"), gadget("U2", "Reg")],
            &[
                ("IN", &[("U1", "3"), ("U2", "3")]),
                ("OUT", &[("U1", "2"), ("U2", "2")]),
            ],
        );
        let root = Sexp::parse(&emit_schematic(&d, &mapping, &lib).unwrap()).unwrap();
        let flags: Vec<_> = root.find_all("no_connect").collect();
        assert_eq!(flags.len(), 2, "only pin 1 of each is open");
        // Pin 1 is at (0, -7.62) in the symbol, below the origin of the sheet's first cell.
        let origin = placed_of(&root, "U1")[0];
        assert_eq!(number(flags[0], 0), number(origin, 0));
        assert!((number(flags[0], 1) - (number(origin, 1) + 7.62)).abs() < 1e-6);
    }

    #[test]
    fn a_pin_the_symbol_types_no_connect_gets_no_flag() {
        // KiCad counts a no-connect flag on such a pin as a connection and objects to it.
        let (_dir, mapping, lib) = acme(&["Types"]);
        let d = design(vec![gadget("U1", "Types")], &[]);
        let root = Sexp::parse(&emit_schematic(&d, &mapping, &lib).unwrap()).unwrap();
        assert_eq!(root.find_all("no_connect").count(), 11, "all but pin 12");
        assert_eq!(root.find_all("label").count(), 0);
    }

    #[test]
    fn a_net_with_one_pin_has_no_label_and_its_pin_is_flagged() {
        // `A` reaches one pin of one part: a label there would be connected to nothing.
        let (_dir, mapping, lib) = acme(&["Reg"]);
        let d = design(
            vec![gadget("U1", "Reg"), gadget("U2", "Reg")],
            &[
                ("A", &[("U1", "2")]),
                ("B", &[("U1", "1"), ("U2", "1")]),
                ("C", &[("U1", "3"), ("U2", "3")]),
                ("D", &[("U2", "2")]),
            ],
        );
        let root = Sexp::parse(&emit_schematic(&d, &mapping, &lib).unwrap()).unwrap();
        let labelled = |net: &str| {
            root.find_all("label")
                .filter(|l| l.args()[0].text() == Some(net))
                .count()
        };
        // B is the two ground pins and the power flag that supplies them.
        assert_eq!((labelled("A"), labelled("B"), labelled("D")), (0, 3, 0));
        // U1.2 and U2.2 are the two output pins that nothing else shares.
        assert_eq!(root.find_all("no_connect").count(), 2);
    }

    #[test]
    fn a_power_input_alone_on_its_net_keeps_its_label_for_the_flag_to_join() {
        let (_dir, mapping, lib) = acme(&["Reg"]);
        let d = design(
            vec![gadget("U1", "Reg"), gadget("U2", "Reg")],
            &[
                ("IN", &[("U1", "3")]),
                ("OUT", &[("U1", "2"), ("U2", "2")]),
                ("GND", &[("U1", "1"), ("U2", "1")]),
            ],
        );
        let root = Sexp::parse(&emit_schematic(&d, &mapping, &lib).unwrap()).unwrap();
        // The pin and the power flag that supplies the net.
        assert_eq!(
            root.find_all("label")
                .filter(|l| l.args()[0].text() == Some("IN"))
                .count(),
            2
        );
        // Only U2's input pin is open.
        assert_eq!(root.find_all("no_connect").count(), 1);
    }

    #[test]
    fn a_pad_stacked_with_a_connected_one_needs_no_flag() {
        let (_dir, mapping, lib) = acme(&["Stacked"]);
        let d = design(
            vec![gadget("U1", "Stacked"), gadget("U2", "Stacked")],
            &[
                ("OUT", &[("U1", "1"), ("U2", "1")]),
                ("IN", &[("U1", "4"), ("U2", "4")]),
                ("GND", &[("U1", "5"), ("U2", "5")]),
            ],
        );
        let root = Sexp::parse(&emit_schematic(&d, &mapping, &lib).unwrap()).unwrap();
        assert_eq!(root.find_all("no_connect").count(), 0);
    }

    #[test]
    fn every_unit_of_a_symbol_is_placed_in_a_column_with_the_same_reference() {
        let (_dir, mapping, lib) = acme(&["Dual"]);
        let d = design(
            vec![gadget("U1", "Dual")],
            &[("N", &[("U1", "1"), ("U1", "6")]), ("V", &[("U1", "8")])],
        );
        let root = Sexp::parse(&emit_schematic(&d, &mapping, &lib).unwrap()).unwrap();
        let units = placed_of(&root, "U1");
        assert_eq!(units.len(), 3);
        let unit_no = |s: &Sexp| {
            s.find("unit").unwrap().args()[0]
                .text()
                .unwrap()
                .to_string()
        };
        assert_eq!(
            units.iter().map(|u| unit_no(u)).collect::<Vec<_>>(),
            ["1", "2", "3"]
        );
        // One column, each unit below the last, and far enough apart not to overlap:
        // the units are 7.62 high and the supply unit's pins reach 6.35 from its origin.
        assert!(units.iter().all(|u| number(u, 0) == number(units[0], 0)));
        let ys: Vec<f64> = units.iter().map(|u| number(u, 1)).collect();
        assert!(
            ys[1] - ys[0] >= 7.62 + 2.0 * 2.54 && ys[2] - ys[1] >= 6.35 + 2.54,
            "{ys:?}"
        );
        // Pins keep their pad numbers, split by unit; the instance path names the unit.
        let pins = |u: &Sexp| -> Vec<String> {
            u.find_all("pin")
                .map(|p| p.args()[0].text().unwrap().to_string())
                .collect()
        };
        assert_eq!(pins(units[0]), ["3", "2", "1"]);
        assert_eq!(pins(units[1]), ["5", "6", "7"]);
        assert_eq!(pins(units[2]), ["8", "4"]);
        for u in &units {
            let path = u
                .find("instances")
                .unwrap()
                .find("project")
                .unwrap()
                .find("path")
                .unwrap();
            assert_eq!(
                path.find("unit").unwrap().args()[0].text(),
                Some(unit_no(u).as_str())
            );
            assert_eq!(path.find("reference").unwrap().args()[0].text(), Some("U1"));
        }
        // Each net's label is on the pin of the unit that has it: pad 6 is in unit 2.
        let n: Vec<_> = root
            .find_all("label")
            .filter(|l| l.args()[0].text() == Some("N"))
            .collect();
        assert_eq!(n.len(), 2);
        assert!(
            n.iter()
                .any(|l| number(l, 1) > ys[1] - 7.0 && number(l, 1) < ys[2])
        );
        // Unit 3's supply pins are open or labelled; pad 4 is open, so it is flagged.
        assert_eq!(root.find_all("no_connect").count(), 5);
        // All uuids are different, units included.
        let mut uuids: Vec<String> = Vec::new();
        for u in &units {
            uuids.push(
                u.find("uuid").unwrap().args()[0]
                    .text()
                    .unwrap()
                    .to_string(),
            );
            uuids.extend(u.find_all("pin").map(|p| {
                p.find("uuid").unwrap().args()[0]
                    .text()
                    .unwrap()
                    .to_string()
            }));
        }
        let before = uuids.len();
        uuids.sort();
        uuids.dedup();
        assert_eq!(uuids.len(), before);
    }

    #[test]
    fn the_symbol_is_embedded_once_however_many_units_it_has() {
        let (_dir, mapping, lib) = acme(&["Dual"]);
        let d = design(vec![gadget("U1", "Dual"), gadget("U2", "Dual")], &[]);
        let root = Sexp::parse(&emit_schematic(&d, &mapping, &lib).unwrap()).unwrap();
        assert_eq!(
            root.find("lib_symbols").unwrap().find_all("symbol").count(),
            1
        );
        assert_eq!(
            placed_of(&root, "U1").len() + placed_of(&root, "U2").len(),
            6
        );
    }

    #[test]
    fn a_big_symbol_gets_room_and_the_next_part_starts_after_it() {
        let (_dir, mapping, lib) = acme(&["Types", "Reg"]);
        let d = design(
            vec![gadget("U1", "Types"), gadget("U2", "Reg")],
            &[("A_RATHER_LONG_NET_NAME", &[("U1", "1")])],
        );
        let root = Sexp::parse(&emit_schematic(&d, &mapping, &lib).unwrap()).unwrap();
        let (u1, u2) = (placed_of(&root, "U1")[0], placed_of(&root, "U2")[0]);
        // Types' pins reach 7.62 to the left of its origin; its label runs 22 characters
        // further left, and must stay inside its own cell, which starts half a pitch
        // before the first origin. U2's cell starts after Types' right edge.
        let label_end = number(u1, 0) - 7.62 - 22.0 * 1.27;
        assert!(label_end >= ORIGIN.0 - COLUMN_PITCH / 2.0, "{label_end}");
        assert!(
            number(u2, 0) - number(u1, 0) >= COLUMN_PITCH,
            "{}",
            number(u2, 0)
        );
        // The grid is kept: every origin is a multiple of 1.27.
        for u in [u1, u2] {
            for i in 0..2 {
                let v = number(u, i) / GRID;
                assert!((v - v.round()).abs() < 1e-6, "{}", number(u, i));
            }
        }
    }

    #[test]
    fn a_symbol_library_that_is_missing_stops_the_emit_naming_the_part() {
        let d = led_design();
        let lib = SymbolLib::at("/nonexistent/symbols");
        let e = emit_schematic(&d, &Mapping::standard(), &lib)
            .unwrap_err()
            .to_string();
        assert!(
            e.starts_with("R1: symbol library `Device` not found"),
            "{e}"
        );
        assert!(e.contains("/nonexistent/symbols/Device.kicad_sym"), "{e}");
    }
}
