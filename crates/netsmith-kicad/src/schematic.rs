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
//!
//! A design with modules is a hierarchy (issue #563, [`crate::hierarchy`]): the board's own parts
//! and a sheet symbol for each module instance on the root sheet, and a file for each module
//! definition, which each instance of the same circuit places with its own references. A net that
//! leaves a sheet is a hierarchical label inside and a sheet pin with a label of the board's net
//! outside. A design with no module is one sheet, byte for byte as before.
//!
//! The emitter reads from the symbol loader (ADR 0021): one net label for each distinct point
//! and net (stacked pads share one), a `no_connect` flag on every pin that no net reaches, and the
//! power-flag rule from the symbol's electrical types. The schematic is a grid of cells with
//! labels and no wires. On the five examples that existed when the loader was written, the export
//! was byte for byte what it was before, and passed KiCad's electrical rules check with no
//! violation (`crates/netsmith/tests/erc.rs` runs it).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use netsmith_core::{Design, DesignError};

use crate::hierarchy::{Hierarchy, SheetInstance, uuid_for};
use crate::mapping::{MapError, Mapped, Mapping, POWER_FLAG_SYMBOL};
use crate::resolve::part_symbols;
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
    let symbols: Vec<Option<Arc<Symbol>>> = part_symbols(design, mapping, lib)
        .into_iter()
        .map(|drawn| drawn.ok().map(|(_, symbol)| symbol))
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

/// The files of a design's schematic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schematic {
    /// The root sheet, `<design>.kicad_sch`: the whole schematic of a design with no module.
    pub root: String,
    /// The sheet of each module file, as `(file name, text)`, in the order of
    /// [`Hierarchy::files`]. Empty for a schematic of one sheet.
    pub sheets: Vec<(String, String)>,
}

/// One sheet to draw: its parts and what its nets and symbols are called.
struct SheetPlan<'a> {
    /// Indices into the design's parts, in the order they are drawn.
    parts: Vec<usize>,
    /// For each instance of the sheet: the path of the instance as KiCad's `(instances ...)`
    /// write it (`/<root>` or `/<root>/<sheet>`), and the reference of each of `parts` in it.
    instances: Vec<(String, Vec<&'a str>)>,
    /// What each part is called in the UUIDs of its labels and flags: its reference on the
    /// root, `<file>:<path inside>` on a module's sheet.
    part_ids: Vec<String>,
    /// The net names inside the sheet, for a module's sheet.
    names: Option<&'a std::collections::BTreeMap<String, String>>,
    /// The nets that leave a module's sheet, by board name.
    leaving: HashSet<&'a str>,
}

impl SheetPlan<'_> {
    fn name<'n>(&'n self, net: &'n str) -> &'n str {
        self.names
            .and_then(|names| names.get(net))
            .map_or(net, String::as_str)
    }
}

/// What one sheet's drawing is made of.
#[derive(Default)]
struct Drawn {
    lib_symbols: Vec<Sexp>,
    no_connects: Vec<Sexp>,
    labels: Vec<Sexp>,
    hierarchical: Vec<Sexp>,
    placed: Vec<Sexp>,
    /// The top of the next row below the parts.
    bottom: f64,
}

impl Drawn {
    fn uses(&mut self, symbol: &Symbol) {
        if !self.lib_symbols.iter().any(|s| s == symbol.definition()) {
            self.lib_symbols.push(symbol.definition().clone());
        }
    }
}

/// A power flag to place on a sheet: its net's name on the sheet, the text its UUIDs
/// are made from, and its reference in each instance of the sheet.
struct Flag {
    name: String,
    key: String,
    label_id: String,
    references: Vec<String>,
}

/// The schematic of `design`: one sheet, or a root sheet and a sheet for each module of the
/// board ([`crate::hierarchy`]). A design with no module is drawn as it always was.
pub fn emit_schematic(
    design: &Design,
    mapping: &Mapping,
    lib: &SymbolLib,
) -> Result<Schematic, EmitError> {
    design.validate()?;
    let hierarchy = Hierarchy::of(design);
    let root = hierarchy.root_uuid.clone();

    let mut mapped: Vec<(Mapped, Arc<Symbol>)> = Vec::new();
    for drawn in part_symbols(design, mapping, lib) {
        mapped.push(drawn?);
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

    // A label is a connection by name, so a net with a label at one point only is connected
    // to nothing and KiCad calls the label dangling. Such a net is left unlabelled and its
    // pin is flagged like any pin that no net reaches, unless a power flag will be labelled
    // on the net as well. A net that leaves a sheet has a point on each sheet it is on, so it
    // is never alone.
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
    unsupplied.sort_unstable();
    let lonely = |net: &str| label_points.get(net) == Some(&1) && !unsupplied.contains(&net);

    // A net's power flag goes on the one sheet the net is on, or on the root when it is on the
    // root's parts or leaves a sheet.
    let index_of: HashMap<&str, usize> = design
        .parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.refdes.as_str(), i))
        .collect();
    let home = |net: &str| -> Option<usize> {
        let net = design.nets.iter().find(|n| n.name == net)?;
        let mut sheets = net
            .pins
            .iter()
            .map(|p| hierarchy.part_sheet[index_of[p.refdes.as_str()]]);
        let first = sheets.next()?;
        sheets.all(|s| s == first).then_some(first).flatten()
    };
    let flag_symbol = if unsupplied.is_empty() {
        None
    } else {
        Some(
            lib.symbol(POWER_FLAG_SYMBOL)
                .map_err(|error| EmitError::Symbol {
                    refdes: flag_refdes(0),
                    error,
                })?,
        )
    };

    // The root sheet.
    let root_parts: Vec<usize> = (0..design.parts.len())
        .filter(|&p| hierarchy.part_sheet[p].is_none())
        .collect();
    let plan = SheetPlan {
        instances: vec![(
            format!("/{root}"),
            root_parts
                .iter()
                .map(|&p| design.parts[p].refdes.as_str())
                .collect(),
        )],
        part_ids: root_parts
            .iter()
            .map(|&p| design.parts[p].refdes.clone())
            .collect(),
        parts: root_parts,
        names: None,
        leaving: HashSet::new(),
    };
    let ctx = Context {
        design,
        hierarchy: &hierarchy,
        mapped: &mapped,
        connections: &connections,
        lonely: &lonely,
    };
    let mut drawn = ctx.draw(&plan);
    let mut sheet_symbols = Vec::new();
    let mut flags_top = drawn.bottom;
    if !hierarchy.is_flat() {
        flags_top = sheet_boxes(
            design,
            &hierarchy,
            drawn.bottom,
            &mut drawn,
            &mut sheet_symbols,
        );
    }
    let root_flags: Vec<Flag> = unsupplied
        .iter()
        .enumerate()
        .filter(|(_, net)| home(net).is_none())
        .map(|(index, net)| {
            let refdes = flag_refdes(index);
            Flag {
                name: net.to_string(),
                key: format!("{}/{refdes}", design.name),
                label_id: format!("{net}/{refdes}/1"),
                references: vec![refdes],
            }
        })
        .collect();
    if let Some(flag) = &flag_symbol
        && !root_flags.is_empty()
    {
        ctx.flags(&plan, flag, &root_flags, flags_top, &mut drawn);
    }

    let mut items = header(&root, drawn.lib_symbols);
    items.extend(drawn.no_connects);
    items.extend(drawn.labels);
    items.extend(drawn.placed);
    items.extend(sheet_symbols);
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
    let root_text = Sexp::node("kicad_sch", items).to_kicad_string();

    // The sheet of each module file, drawn from its first instance.
    let mut sheets = Vec::new();
    for file in &hierarchy.files {
        let first = &hierarchy.instances[file.instances[0]];
        let refs = |instance: &SheetInstance| -> Vec<&str> {
            instance
                .parts
                .iter()
                .map(|&p| design.parts[p].refdes.as_str())
                .collect()
        };
        let plan = SheetPlan {
            parts: first.parts.clone(),
            instances: file
                .instances
                .iter()
                .map(|&i| {
                    let instance = &hierarchy.instances[i];
                    (format!("/{root}/{}", instance.uuid), refs(instance))
                })
                .collect(),
            part_ids: first
                .parts
                .iter()
                .map(|&p| format!("{}:{}", file.name, hierarchy.local_path(design, p)))
                .collect(),
            names: Some(&first.names),
            leaving: first.pins.iter().map(|(net, _)| net.as_str()).collect(),
        };
        let mut drawn = ctx.draw(&plan);
        // The flags of the nets inside the sheet, by their name inside, in that order; each
        // instance has its own reference for its own net.
        let mut flags: Vec<Flag> = Vec::new();
        for (net, name) in &first.names {
            if !unsupplied.contains(&net.as_str()) || home(net) != Some(file.instances[0]) {
                continue;
            }
            let references = file
                .instances
                .iter()
                .map(|&i| {
                    let theirs = hierarchy.instances[i]
                        .names
                        .iter()
                        .find(|(_, n)| *n == name)
                        .map(|(net, _)| net.as_str())
                        .expect("an instance of the file has the same nets");
                    let index = unsupplied
                        .iter()
                        .position(|n| *n == theirs)
                        .expect("the same circuit is unsupplied in every instance");
                    flag_refdes(index)
                })
                .collect();
            flags.push(Flag {
                name: name.clone(),
                key: format!("{}/{}/flag/{name}", design.name, file.name),
                label_id: format!("{name}/{}:flag/1", file.name),
                references,
            });
        }
        flags.sort_by(|a, b| a.name.cmp(&b.name));
        if let Some(flag) = &flag_symbol
            && !flags.is_empty()
        {
            let top = drawn.bottom;
            ctx.flags(&plan, flag, &flags, top, &mut drawn);
        }
        let uuid = uuid_for(&format!("{}/file/{}", design.name, file.name));
        let mut items = header(&uuid, drawn.lib_symbols);
        items.extend(drawn.no_connects);
        items.extend(drawn.labels);
        items.extend(drawn.hierarchical);
        items.extend(drawn.placed);
        items.push(Sexp::node("embedded_fonts", vec![Sexp::atom("no")]));
        sheets.push((
            file.name.clone(),
            Sexp::node("kicad_sch", items).to_kicad_string(),
        ));
    }

    Ok(Schematic {
        root: root_text,
        sheets,
    })
}

/// The first items of a `.kicad_sch`.
fn header(uuid: &str, lib_symbols: Vec<Sexp>) -> Vec<Sexp> {
    vec![
        Sexp::node("version", vec![Sexp::atom("20250114")]),
        Sexp::node("generator", vec![Sexp::string("netsmith")]),
        Sexp::node(
            "generator_version",
            vec![Sexp::string(env!("CARGO_PKG_VERSION"))],
        ),
        Sexp::node("uuid", vec![Sexp::string(uuid)]),
        Sexp::node("paper", vec![Sexp::string("A4")]),
        Sexp::node("lib_symbols", lib_symbols),
    ]
}

/// What drawing a sheet reads, the same for every sheet.
struct Context<'a> {
    design: &'a Design,
    hierarchy: &'a Hierarchy,
    mapped: &'a [(Mapped, Arc<Symbol>)],
    connections: &'a [Vec<Connection<'a>>],
    lonely: &'a dyn Fn(&str) -> bool,
}

impl Context<'_> {
    /// The parts of a sheet in rows of `COLUMNS`, each with its labels and no-connect flags; a
    /// cell is as big as the part needs, and no smaller than the grid.
    fn draw(&self, plan: &SheetPlan) -> Drawn {
        let design = self.design;
        let mut drawn = Drawn::default();
        for &p in &plan.parts {
            drawn.uses(&self.mapped[p].1);
        }
        let cells: Vec<Cell> = plan
            .parts
            .iter()
            .map(|&p| plan_cell(&self.mapped[p].1, &self.connections[p]))
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
        drawn.bottom = row_top;

        // A net that leaves the sheet has a hierarchical label at its first point, and plain
        // labels of the same name at the others, which KiCad joins to it by name.
        let mut hierarchical: HashSet<&str> = HashSet::new();
        for (slot, &index) in plan.parts.iter().enumerate() {
            let (m, symbol) = &self.mapped[index];
            let part = &design.parts[index];
            let part_id = &plan.part_ids[slot];
            for unit in 1..=symbol.units() {
                let origin = (
                    origins[slot].0,
                    origins[slot].1 + cells[slot].unit_dy[unit as usize - 1],
                );
                let placement = Placement {
                    reference: plan.instances[0].1[slot],
                    key: self.hierarchy.symbol_key(design, index),
                    instances: plan
                        .instances
                        .iter()
                        .map(|(path, refs)| (path.clone(), refs[slot].to_string()))
                        .collect(),
                    value: &part.value,
                    symbol,
                    footprint: &m.footprint,
                    unit,
                    hidden_reference: false,
                    real: true,
                };
                drawn.placed.push(placed_symbol(design, &placement, origin));

                // One label for each distinct point and net: stacked pins share a point.
                let mut labelled: Vec<(f64, f64, &str)> = Vec::new();
                for c in self.connections[index]
                    .iter()
                    .filter(|c| drawn_in(c.pin, unit) && !(self.lonely)(c.net))
                {
                    if labelled.contains(&(c.pin.x, c.pin.y, c.net)) {
                        continue;
                    }
                    labelled.push((c.pin.x, c.pin.y, c.net));
                    let name = plan.name(c.net);
                    let id = format!("{name}/{part_id}/{}", c.pin.number);
                    if plan.leaving.contains(c.net) && hierarchical.insert(c.net) {
                        drawn.hierarchical.push(pin_hierarchical_label(
                            name,
                            origin,
                            c.pin.geometry(),
                            &id,
                        ));
                    } else {
                        drawn
                            .labels
                            .push(pin_label(name, origin, c.pin.geometry(), &id));
                    }
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
                    drawn.no_connects.push(no_connect(
                        (origin.0 + pin.x, origin.1 - pin.y),
                        &format!("{}/{part_id}/{}/{unit}", design.name, pin.number),
                    ));
                }
            }
        }
        drawn
    }

    /// The power flags of a sheet, in a row from `top`, each labelled with its net.
    fn flags(&self, plan: &SheetPlan, flag: &Symbol, flags: &[Flag], top: f64, drawn: &mut Drawn) {
        drawn.lib_symbols.push(flag.definition().clone());
        for (index, f) in flags.iter().enumerate() {
            let origin = (
                ORIGIN.0 + (index % COLUMNS) as f64 * COLUMN_PITCH,
                top + (index / COLUMNS) as f64 * ROW_PITCH + ROW_PITCH / 2.0,
            );
            let placement = Placement {
                reference: &f.references[0],
                key: f.key.clone(),
                instances: plan
                    .instances
                    .iter()
                    .zip(&f.references)
                    .map(|((path, _), r)| (path.clone(), r.clone()))
                    .collect(),
                value: "PWR_FLAG",
                symbol: flag,
                footprint: "",
                unit: 1,
                hidden_reference: true,
                real: false,
            };
            drawn
                .placed
                .push(placed_symbol(self.design, &placement, origin));
            let geometry = flag
                .pin_geometry("1")
                .expect("the power flag symbol has a pin 1");
            drawn
                .labels
                .push(pin_label(&f.name, origin, geometry, &f.label_id));
        }
    }
}

/// The height of a sheet symbol's pin pitch, and of the room for its name above and its file
/// below, in mm.
const SHEET_PIN_PITCH: f64 = 2.0 * GRID;

/// The sheet symbols of the root, in rows below its parts from `top`, each with its pins on its
/// left side and a label of the pin's net on each; returns the top of the next row below them.
fn sheet_boxes(
    design: &Design,
    hierarchy: &Hierarchy,
    top: f64,
    drawn: &mut Drawn,
    symbols: &mut Vec<Sexp>,
) -> f64 {
    let chars = |s: &str| s.chars().count() as f64;
    let right_edge = ORIGIN.0 - COLUMN_PITCH / 2.0 + COLUMNS as f64 * COLUMN_PITCH;
    let mut x = ORIGIN.0 - COLUMN_PITCH / 2.0;
    let mut row_top = top;
    let mut row_height: f64 = 0.0;
    for instance in &hierarchy.instances {
        let file = &hierarchy.files[instance.file].name;
        let room = snap_up(
            instance
                .pins
                .iter()
                .map(|(net, _)| chars(net))
                .fold(0.0, f64::max)
                * LABEL_CHAR
                + 2.0 * PAD,
        );
        let inside = instance
            .pins
            .iter()
            .map(|(_, name)| chars(name))
            .fold(0.0, f64::max)
            * LABEL_CHAR
            + PAD;
        let width = snap_up(
            inside
                .max(chars(&instance.path).max(chars(file)) * LABEL_CHAR)
                .max(8.0 * GRID),
        );
        let height = snap_up(
            ((instance.pins.len() + 1) as f64 * SHEET_PIN_PITCH).max(4.0 * SHEET_PIN_PITCH),
        );
        if x + room + width > right_edge && x > ORIGIN.0 - COLUMN_PITCH / 2.0 {
            x = ORIGIN.0 - COLUMN_PITCH / 2.0;
            row_top += row_height;
            row_height = 0.0;
        }
        let corner = (x + room, row_top + 2.0 * SHEET_PIN_PITCH);
        let mut items = vec![
            Sexp::node("at", vec![Sexp::num(corner.0), Sexp::num(corner.1)]),
            Sexp::node("size", vec![Sexp::num(width), Sexp::num(height)]),
            Sexp::node("exclude_from_sim", vec![Sexp::atom("no")]),
            Sexp::node("in_bom", vec![Sexp::atom("yes")]),
            Sexp::node("on_board", vec![Sexp::atom("yes")]),
            Sexp::node("dnp", vec![Sexp::atom("no")]),
            Sexp::node("fields_autoplaced", vec![Sexp::atom("yes")]),
            Sexp::node(
                "stroke",
                vec![
                    Sexp::node("width", vec![Sexp::num(0.1524)]),
                    Sexp::node("type", vec![Sexp::atom("solid")]),
                ],
            ),
            Sexp::node(
                "fill",
                vec![Sexp::node(
                    "color",
                    vec![
                        Sexp::num(0.0),
                        Sexp::num(0.0),
                        Sexp::num(0.0),
                        Sexp::atom("0.0000"),
                    ],
                )],
            ),
            Sexp::node("uuid", vec![Sexp::string(&instance.uuid)]),
            justified_property(
                "Sheetname",
                &instance.path,
                (corner.0, corner.1 - 0.7116),
                "bottom",
            ),
            justified_property(
                "Sheetfile",
                file,
                (corner.0, corner.1 + height + 0.5846),
                "top",
            ),
        ];
        for (k, (net, name)) in instance.pins.iter().enumerate() {
            let pin_at = (corner.0, corner.1 + (k + 1) as f64 * SHEET_PIN_PITCH);
            items.push(Sexp::node(
                "pin",
                vec![
                    Sexp::string(name),
                    Sexp::atom("passive"),
                    at(pin_at.0, pin_at.1, 180.0),
                    Sexp::node(
                        "uuid",
                        vec![Sexp::string(uuid_for(&format!(
                            "{}/sheet/{}/pin/{name}",
                            design.name, instance.path
                        )))],
                    ),
                    Sexp::node(
                        "effects",
                        vec![
                            Sexp::node(
                                "font",
                                vec![Sexp::node("size", vec![Sexp::num(1.27), Sexp::num(1.27)])],
                            ),
                            Sexp::node("justify", vec![Sexp::atom("left")]),
                        ],
                    ),
                ],
            ));
            drawn.labels.push(label(
                net,
                pin_at,
                180.0,
                &format!("sheet/{}/{name}", instance.path),
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
                            Sexp::string(format!("/{}", hierarchy.root_uuid)),
                            Sexp::node("page", vec![Sexp::string(instance.page.to_string())]),
                        ],
                    ),
                ],
            )],
        ));
        symbols.push(Sexp::node("sheet", items));
        x += room + width + 2.0 * PAD;
        row_height = row_height.max(height + 4.0 * SHEET_PIN_PITCH);
    }
    row_top + row_height
}

/// A shown property with a justification, as the name and the file of a sheet are.
fn justified_property(name: &str, value: &str, pos: (f64, f64), vertical: &str) -> Sexp {
    Sexp::node(
        "property",
        vec![
            Sexp::string(name),
            Sexp::string(value),
            at(pos.0, pos.1, 0.0),
            Sexp::node(
                "effects",
                vec![
                    Sexp::node(
                        "font",
                        vec![Sexp::node("size", vec![Sexp::num(1.27), Sexp::num(1.27)])],
                    ),
                    Sexp::node("justify", vec![Sexp::atom("left"), Sexp::atom(vertical)]),
                ],
            ),
        ],
    )
}

/// The electrical types in the order of the rows and the columns of KiCad's ERC pin map.
const PIN_MAP_TYPES: [PinType; 12] = [
    PinType::Input,
    PinType::Output,
    PinType::Bidirectional,
    PinType::TriState,
    PinType::Passive,
    PinType::Free,
    PinType::Unspecified,
    PinType::PowerIn,
    PinType::PowerOut,
    PinType::OpenCollector,
    PinType::OpenEmitter,
    PinType::NoConnect,
];

/// KiCad 9's default ERC pin map: 0 is no finding, 1 a warning, 2 an error, in the order of
/// [`PIN_MAP_TYPES`] (the `erc.pin_map` of the projects KiCad 9 writes, such as its template
/// `API_Series-500.kicad_pro`).
const KICAD_PIN_MAP: [[u8; 12]; 12] = [
    [0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 2],
    [0, 2, 0, 1, 0, 0, 1, 0, 2, 2, 2, 2],
    [0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 1, 2],
    [0, 1, 0, 0, 0, 0, 1, 1, 2, 1, 1, 2],
    [0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 2],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2],
    [1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 2],
    [0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 2],
    [0, 2, 1, 2, 0, 0, 1, 0, 2, 2, 2, 2],
    [0, 2, 0, 1, 0, 0, 1, 0, 2, 0, 0, 2],
    [0, 2, 1, 1, 0, 0, 1, 0, 2, 0, 0, 2],
    [2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2],
];

/// The pairs of electrical types that netsmith's own pin rules accept and KiCad's default map
/// reports (#461): a bidirectional or tri-state pin on a net with a power output (the power flag
/// that [`emit_schematic`] puts on a net nothing on the board drives, or a regulator's output),
/// and a tri-state pin on a net with a power input. netsmith's checker owns the pin rules (ADR
/// 0016), so the exported project says the same: a GPIO tied to ground is an ordinary circuit.
/// The map is never made stricter than KiCad's: what netsmith refuses, it reports itself.
const ACCEPTED: [(PinType, PinType); 3] = [
    (PinType::Bidirectional, PinType::PowerOut),
    (PinType::TriState, PinType::PowerOut),
    (PinType::TriState, PinType::PowerIn),
];

/// The ERC pin map of the exported project: KiCad's default with the pairs of [`ACCEPTED`]
/// made 0, both ways round.
pub fn erc_pin_map() -> [[u8; 12]; 12] {
    let mut map = KICAD_PIN_MAP;
    let index = |t: PinType| {
        PIN_MAP_TYPES
            .iter()
            .position(|&p| p == t)
            .expect("every type has a row")
    };
    for (a, b) in ACCEPTED {
        map[index(a)][index(b)] = 0;
        map[index(b)][index(a)] = 0;
    }
    map
}

/// The project file that goes beside the schematic, so KiCad opens the schematic as part of a
/// project, with the ERC pin map of netsmith's pin rules ([`erc_pin_map`]).
pub fn project_file(design_name: &str) -> String {
    let rows: Vec<String> = erc_pin_map()
        .iter()
        .map(|row| {
            let cells: Vec<String> = row.iter().map(u8::to_string).collect();
            format!("      [{}]", cells.join(", "))
        })
        .collect();
    format!(
        "{{\n  \"erc\": {{\n    \"meta\": {{\n      \"version\": 0\n    }},\n    \"pin_map\": [\n{}\n    ]\n  }},\n  \"meta\": {{\n    \"filename\": \"{design_name}.kicad_pro\",\n    \"version\": 3\n  }}\n}}\n",
        rows.join(",\n")
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

fn visible_property(
    name: &str,
    value: &str,
    pos: (f64, f64),
    angle: f64,
    justify: &[String],
) -> Sexp {
    let mut shown = effects(false);
    // A field the symbol justifies (a FET's text starts at its position, clear of the body) is
    // placed with the same justification, as KiCad copies it from the library symbol.
    if !justify.is_empty()
        && let Sexp::List(items) = &mut shown
    {
        items.push(Sexp::node(
            "justify",
            justify.iter().map(|w| Sexp::atom(w.as_str())).collect(),
        ));
    }
    Sexp::node(
        "property",
        vec![
            Sexp::string(name),
            Sexp::string(value),
            at(pos.0, pos.1, angle),
            shown,
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

/// A hierarchical label on a pin's connection point, running away from the symbol body: the
/// pin's net leaves the sheet through the sheet pin of the same name.
fn pin_hierarchical_label(
    net: &str,
    origin: (f64, f64),
    geometry: PinGeometry,
    uuid_name: &str,
) -> Sexp {
    let at_pin = (origin.0 + geometry.x, origin.1 - geometry.y);
    let away = (geometry.angle + 180.0).rem_euclid(360.0);
    let justify = if away >= 180.0 { "right" } else { "left" };
    Sexp::node(
        "hierarchical_label",
        vec![
            Sexp::string(net),
            Sexp::node("shape", vec![Sexp::atom("passive")]),
            at(at_pin.0, at_pin.1, away),
            Sexp::node(
                "effects",
                vec![
                    Sexp::node(
                        "font",
                        vec![Sexp::node("size", vec![Sexp::num(1.27), Sexp::num(1.27)])],
                    ),
                    Sexp::node("justify", vec![Sexp::atom(justify)]),
                ],
            ),
            Sexp::node(
                "uuid",
                vec![Sexp::string(uuid_for(&format!("hierarchical/{uuid_name}")))],
            ),
        ],
    )
}

/// One symbol instance on the sheet: one unit of a part from the design, or a power flag.
struct Placement<'a> {
    /// The reference the symbol shows: that of its first instance.
    reference: &'a str,
    /// The text its UUIDs are made from ([`Hierarchy::symbol_key`]).
    key: String,
    /// Each instance of the sheet as `(instance path, reference)`.
    instances: Vec<(String, String)>,
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

fn placed_symbol(design: &Design, placement: &Placement, origin: (f64, f64)) -> Sexp {
    // The first unit keeps the names it had when every symbol had one unit.
    let unit_key = if placement.unit == 1 {
        String::new()
    } else {
        format!("u{}/", placement.unit)
    };
    let key = |suffix: &str| uuid_for(&format!("{}/{suffix}", placement.key));
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
            visible_property(
                name,
                value,
                (x + dx, y - dy),
                rot,
                &placement.symbol.field_justify(name),
            )
        }
    };
    let mut items = vec![
        Sexp::node("lib_id", vec![Sexp::string(placement.symbol.lib_id())]),
        at(x, y, 0.0),
        Sexp::node("unit", vec![Sexp::atom(placement.unit.to_string())]),
        Sexp::node("exclude_from_sim", vec![Sexp::atom("no")]),
        // A part is on the bill of materials and the board when it is a part and its symbol
        // says so, as KiCad's own placement copies the library symbol's flags.
        Sexp::node(
            "in_bom",
            vec![Sexp::yes_no(placement.real && placement.symbol.in_bom())],
        ),
        Sexp::node(
            "on_board",
            vec![Sexp::yes_no(placement.real && placement.symbol.on_board())],
        ),
        Sexp::node("dnp", vec![Sexp::atom("no")]),
        Sexp::node("fields_autoplaced", vec![Sexp::atom("yes")]),
        Sexp::node("uuid", vec![Sexp::string(key(&unit_suffix("symbol")))]),
        field("Reference", placement.reference, placement.hidden_reference),
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
    let mut project = vec![Sexp::string(&design.name)];
    for (path, reference) in &placement.instances {
        project.push(Sexp::node(
            "path",
            vec![
                Sexp::string(path),
                Sexp::node("reference", vec![Sexp::string(reference)]),
                Sexp::node("unit", vec![Sexp::atom(placement.unit.to_string())]),
            ],
        ));
    }
    items.push(Sexp::node(
        "instances",
        vec![Sexp::node("project", project)],
    ));
    Sexp::node("symbol", items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testlib::{self, TempDir};
    use netsmith_core::{Net, Part, PartPin, PinKind, PinRef};

    #[test]
    fn the_erc_pin_map_is_kicads_but_for_the_pairs_netsmith_accepts() {
        let map = erc_pin_map();
        let index = |t: PinType| PIN_MAP_TYPES.iter().position(|&p| p == t).unwrap();
        for (i, row) in map.iter().enumerate() {
            for (j, &cell) in row.iter().enumerate() {
                let accepted = ACCEPTED.iter().any(|&(a, b)| {
                    (index(a), index(b)) == (i, j) || (index(b), index(a)) == (i, j)
                });
                let want = if accepted { 0 } else { KICAD_PIN_MAP[i][j] };
                assert_eq!(
                    cell, want,
                    "{:?} with {:?}",
                    PIN_MAP_TYPES[i], PIN_MAP_TYPES[j]
                );
                // The map is symmetric, as KiCad's is.
                assert_eq!(cell, map[j][i]);
            }
        }
        // The charger's case: a bidirectional pin with a power flag.
        assert_eq!(
            map[index(PinType::Bidirectional)][index(PinType::PowerOut)],
            0
        );
        // An output against a power output stays an error.
        assert_eq!(map[index(PinType::Output)][index(PinType::PowerOut)], 2);
    }

    #[test]
    fn the_project_file_holds_the_pin_map_as_kicad_reads_it() {
        let text = project_file("Demo");
        assert!(text.contains("\"filename\": \"Demo.kicad_pro\""), "{text}");
        let rows: Vec<&str> = text
            .lines()
            .filter(|l| l.trim_start().starts_with('['))
            .collect();
        assert_eq!(rows.len(), 12, "{text}");
        assert_eq!(
            rows[2].trim().trim_end_matches(','),
            "[0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 2]"
        );
        assert!(text.contains("\"pin_map\": ["), "{text}");
    }

    /// The schematic of a design with no module: one sheet.
    fn emit(design: &Design, mapping: &Mapping, lib: &SymbolLib) -> Result<String, EmitError> {
        emit_schematic(design, mapping, lib).map(|s| {
            assert!(s.sheets.is_empty(), "a design with no module is one sheet");
            s.root
        })
    }

    #[test]
    fn a_design_with_no_module_is_drawn_byte_for_byte_as_before_sheets() {
        // The files are what the writer made before it drew modules as sheets (issue #563),
        // with the version of the generator taken out.
        let version = env!("CARGO_PKG_VERSION");
        for (name, design, want) in [
            (
                "led",
                led_design(),
                include_str!("../testdata/flat/led.kicad_sch"),
            ),
            (
                "regulator",
                regulator_design(),
                include_str!("../testdata/flat/regulator.kicad_sch"),
            ),
            (
                "two_drawn_regulators",
                two_drawn_regulators("OUT", "5V"),
                include_str!("../testdata/flat/two_drawn_regulators.kicad_sch"),
            ),
        ] {
            let text = emit(&design, &mapping(), &sample()).unwrap();
            assert!(
                text.replace(version, "VERSION") == want,
                "{name}: the schematic of a design with no module changed"
            );
        }
    }

    /// Two instances `s1` and `s2` of a module `Stage` (a capacitor and a regulator whose input
    /// is a net inside the module, which nothing on the board drives), each with its output on a
    /// header of the board, and ground through the module's member `ground`.
    fn staged_design() -> Design {
        let part =
            |refdes: &str, kind: &str, value: &str, package: Option<&str>, path: &str| Part {
                provides: Vec::new(),
                refdes: refdes.into(),
                kind: kind.into(),
                value: value.into(),
                package: package.map(str::to_string),
                path: path.into(),
                pins: Vec::new(),
            };
        let member = |name: &str, net: &str| netsmith_core::InterfaceMember {
            name: name.into(),
            net: net.into(),
        };
        let mut d = design(
            vec![
                part("J1", "Header1x2", "Conn_01x02", None, "out1"),
                part("J2", "Header1x2", "Conn_01x02", None, "out2"),
                part("C1", "Capacitor", "1u", Some("0402"), "s1.c"),
                part("U1", "Ldo", "XC6206", Some("SOT-23"), "s1.u"),
                part("C2", "Capacitor", "1u", Some("0402"), "s2.c"),
                part("U2", "Ldo", "XC6206", Some("SOT-23"), "s2.u"),
            ],
            &[
                ("net-s1.c.p1", &[("C1", "1"), ("U1", "3")]),
                ("net-s2.c.p1", &[("C2", "1"), ("U2", "3")]),
                (
                    "GND",
                    &[
                        ("J1", "2"),
                        ("C1", "2"),
                        ("U1", "1"),
                        ("C2", "2"),
                        ("U2", "1"),
                        ("J2", "2"),
                    ],
                ),
                ("OUT1", &[("U1", "2"), ("J1", "1")]),
                ("OUT2", &[("U2", "2"), ("J2", "1")]),
            ],
        );
        d.modules = ["1", "2"]
            .iter()
            .map(|n| netsmith_core::ModuleInstance {
                path: format!("s{n}"),
                type_name: "Stage".into(),
                ports: vec![
                    member("vout.pos", &format!("OUT{n}")),
                    member("ground", "GND"),
                ],
            })
            .collect();
        d
    }

    /// The references of each instance path of the placed symbols of a sheet, by the value of
    /// the symbol's `Reference` field.
    fn instance_references(sheet: &Sexp) -> Vec<(String, Vec<(String, String)>)> {
        sheet
            .find_all("symbol")
            .map(|s| {
                let reference = s
                    .find_all("property")
                    .find(|p| p.args()[0].text() == Some("Reference"))
                    .and_then(|p| p.args()[1].text())
                    .unwrap()
                    .to_string();
                let paths = s
                    .find("instances")
                    .unwrap()
                    .find("project")
                    .unwrap()
                    .find_all("path")
                    .map(|p| {
                        (
                            p.args()[0].text().unwrap().to_string(),
                            p.find("reference").unwrap().args()[0]
                                .text()
                                .unwrap()
                                .to_string(),
                        )
                    })
                    .collect();
                (reference, paths)
            })
            .collect()
    }

    fn texts(sheet: &Sexp, head: &str) -> Vec<String> {
        let mut v: Vec<String> = sheet
            .find_all(head)
            .map(|n| n.args()[0].text().unwrap().to_string())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn two_instances_of_one_circuit_are_one_sheet_file_placed_twice() {
        let d = staged_design();
        let schematic = emit_schematic(&d, &mapping(), &sample()).unwrap();
        let names: Vec<&str> = schematic.sheets.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Stage.kicad_sch"]);
        let root = Sexp::parse(&schematic.root).unwrap();
        let stage = Sexp::parse(&schematic.sheets[0].1).unwrap();
        let root_uuid = uuid_for("gadgets");
        let sheet_uuid = |path: &str| uuid_for(&format!("gadgets/sheet/{path}"));

        // The root has the headers, the ground's flag and a sheet symbol for each instance,
        // each with a pin for each net that leaves it, in the order of the module's members,
        // and a label of the board's net on each pin.
        let sheets: Vec<&Sexp> = root.find_all("sheet").collect();
        assert_eq!(sheets.len(), 2);
        for (sheet, path, page) in [(sheets[0], "s1", "2"), (sheets[1], "s2", "3")] {
            let property = |name: &str| {
                sheet
                    .find_all("property")
                    .find(|p| p.args()[0].text() == Some(name))
                    .and_then(|p| p.args()[1].text())
                    .unwrap()
                    .to_string()
            };
            assert_eq!(property("Sheetname"), path);
            assert_eq!(property("Sheetfile"), "Stage.kicad_sch");
            assert_eq!(
                sheet.find("uuid").unwrap().args()[0].text(),
                Some(sheet_uuid(path).as_str())
            );
            let pins: Vec<&str> = sheet
                .find_all("pin")
                .map(|p| p.args()[0].text().unwrap())
                .collect();
            assert_eq!(pins, ["vout.pos", "ground"]);
            let instance = sheet.find("instances").unwrap().find("project").unwrap();
            let at = instance.find("path").unwrap();
            assert_eq!(at.args()[0].text(), Some(format!("/{root_uuid}").as_str()));
            assert_eq!(at.find("page").unwrap().args()[0].text(), Some(page));
        }
        assert_eq!(
            texts(&root, "label"),
            [
                "GND", "GND", "GND", "GND", "GND", "OUT1", "OUT1", "OUT2", "OUT2"
            ]
        );
        assert_eq!(flags(&root), [("#FLG01".to_string(), "GND".to_string())]);
        let on_root: Vec<String> = instance_references(&root)
            .into_iter()
            .map(|(r, _)| r)
            .collect();
        assert_eq!(on_root, ["J1", "J2", "#FLG01"]);

        // The file draws the module once, with each symbol's reference in each instance, its
        // nets by their names inside, and the flag of its own input net, a reference for each.
        let s1 = format!("/{root_uuid}/{}", sheet_uuid("s1"));
        let s2 = format!("/{root_uuid}/{}", sheet_uuid("s2"));
        let placed = instance_references(&stage);
        let pair =
            |a: &str, b: &str| vec![(s1.clone(), a.to_string()), (s2.clone(), b.to_string())];
        assert_eq!(
            placed,
            [
                ("C1".to_string(), pair("C1", "C2")),
                ("U1".to_string(), pair("U1", "U2")),
                ("#FLG02".to_string(), pair("#FLG02", "#FLG03")),
            ]
        );
        assert_eq!(texts(&stage, "hierarchical_label"), ["ground", "vout.pos"]);
        assert_eq!(
            texts(&stage, "label"),
            ["ground", "net-c.p1", "net-c.p1", "net-c.p1"]
        );
        assert!(stage.find("sheet_instances").is_none());
        assert!(stage.find("sheet").is_none());

        // The same bytes on a second run.
        assert_eq!(
            schematic,
            emit_schematic(&d, &mapping(), &sample()).unwrap()
        );
    }

    #[test]
    fn an_instance_of_another_value_has_a_file_of_its_own() {
        let mut d = staged_design();
        d.parts[4].value = "2u2".into();
        let schematic = emit_schematic(&d, &mapping(), &sample()).unwrap();
        let names: Vec<&str> = schematic.sheets.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Stage.kicad_sch", "Stage_2.kicad_sch"]);
        for (_, text) in &schematic.sheets {
            let sheet = Sexp::parse(text).unwrap();
            for (_, paths) in instance_references(&sheet) {
                assert_eq!(paths.len(), 1, "{paths:?}");
            }
        }
    }

    #[test]
    fn the_footprint_links_name_the_symbols_and_sheets_the_schematic_draws() {
        let d = staged_design();
        let schematic = emit_schematic(&d, &mapping(), &sample()).unwrap();
        let root = Sexp::parse(&schematic.root).unwrap();
        let stage = Sexp::parse(&schematic.sheets[0].1).unwrap();
        let links = Hierarchy::of(&d).links(&d);
        let uuid_of = |sheet: &Sexp, reference: &str, path: &str| -> String {
            sheet
                .find_all("symbol")
                .find(|s| {
                    s.find("instances")
                        .unwrap()
                        .find("project")
                        .unwrap()
                        .find_all("path")
                        .any(|p| {
                            p.args()[0].text() == Some(path)
                                && p.find("reference").unwrap().args()[0].text() == Some(reference)
                        })
                })
                .and_then(|s| s.find("uuid"))
                .and_then(|u| u.args()[0].text())
                .unwrap_or_else(|| panic!("no symbol {reference} at {path}"))
                .to_string()
        };
        let root_uuid = uuid_for("gadgets");
        for (part, link) in d.parts.iter().zip(&links) {
            let link = link.as_ref().unwrap();
            match link.path.split('/').collect::<Vec<_>>()[..] {
                ["", symbol] => {
                    assert_eq!(link.sheet_name, "/");
                    assert_eq!(link.sheet_file, "gadgets.kicad_sch");
                    assert_eq!(
                        uuid_of(&root, &part.refdes, &format!("/{root_uuid}")),
                        symbol
                    );
                }
                ["", sheet, symbol] => {
                    let module = part.path.split('.').next().unwrap();
                    assert_eq!(link.sheet_name, format!("/{module}/"));
                    assert_eq!(link.sheet_file, "Stage.kicad_sch");
                    assert_eq!(sheet, uuid_for(&format!("gadgets/sheet/{module}")));
                    let at = format!("/{root_uuid}/{sheet}");
                    assert_eq!(uuid_of(&stage, &part.refdes, &at), symbol);
                }
                _ => panic!("{link:?}"),
            }
        }
    }

    #[test]
    fn a_sheet_whose_names_inside_would_meet_keeps_the_boards_names() {
        // The board names the input net of `s1` `ground`, the name of the module's member.
        let mut d = staged_design();
        d.nets[0].name = "ground".into();
        let hierarchy = Hierarchy::of(&d);
        let s1 = &hierarchy.instances[0];
        assert_eq!(s1.names["ground"], "ground");
        assert_eq!(s1.names["GND"], "GND");
        assert_eq!(s1.names["OUT1"], "OUT1");
        assert_eq!(
            s1.pins,
            [
                ("OUT1".to_string(), "OUT1".to_string()),
                ("GND".to_string(), "GND".to_string())
            ]
        );
        // `s2` keeps the module's names, so the two are two files.
        assert_eq!(hierarchy.instances[1].names["GND"], "ground");
        assert_eq!(hierarchy.files.len(), 2);
    }

    #[test]
    fn a_name_inside_a_sheet_that_differs_from_another_only_in_case_is_the_boards() {
        // The module calls its ground `gnd` and the board calls the net `GND`: KiCad's rules
        // check would report the two labels, so the sheets say `GND`.
        let mut d = staged_design();
        for module in &mut d.modules {
            module.ports[1].name = "gnd".into();
        }
        let hierarchy = Hierarchy::of(&d);
        for instance in &hierarchy.instances {
            assert_eq!(instance.names["GND"], "GND");
            assert_eq!(instance.pins[1], ("GND".to_string(), "GND".to_string()));
            assert_eq!(instance.pins[0].1, "vout.pos");
        }
        assert_eq!(hierarchy.files.len(), 1);
    }

    #[test]
    fn a_module_with_no_part_and_a_design_with_no_module_are_one_sheet() {
        let mut d = staged_design();
        for part in &mut d.parts {
            part.path = part.path.replace('.', "_");
        }
        assert!(Hierarchy::of(&d).is_flat());
        let links = Hierarchy::of(&d).links(&d);
        assert_eq!(
            links[0].as_ref().unwrap().path,
            format!("/{}", uuid_for("gadgets/J1/symbol"))
        );
        emit(&d, &mapping(), &sample()).unwrap();
    }

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
            modules: Vec::new(),
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
        Sexp::parse(&emit(design, &mapping(), &sample()).unwrap()).unwrap()
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
        let text = emit(&led_design(), &mapping(), &sample()).unwrap();
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
        let a = emit(&led_design(), &mapping(), &sample()).unwrap();
        let b = emit(&led_design(), &mapping(), &sample()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn printing_the_output_again_changes_nothing() {
        let text = emit(&led_design(), &mapping(), &sample()).unwrap();
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
        let before = emit(&d, &mapping(), &sample()).unwrap();
        d.parts[0].value = "330".into();
        let after = emit(&d, &mapping(), &sample()).unwrap();
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
            emit(&d, &mapping(), &sample()),
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
            emit(&d, &mapping(), &sample()),
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
            emit(&d, &mapping(), &sample()),
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
            modules: Vec::new(),
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
        let text = emit(&led_design(), &mapping(), &sample()).unwrap();
        assert!(!text.contains("PWR_FLAG"));
    }

    #[test]
    fn flags_sit_below_the_parts_and_the_symbol_is_embedded_once() {
        let d = regulator_design();
        let text = emit(&d, &mapping(), &sample()).unwrap();
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
        let text = emit(&d, &mapping(), &sample()).unwrap();
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
        assert_eq!(text, emit(&d, &mapping(), &sample()).unwrap());
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
        let text = emit(&d, &mapping(), &sample()).unwrap();
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
        assert_eq!(text, emit(&d, &mapping(), &sample()).unwrap());
    }

    /// The electrical type the embedded symbol of the placed part `refdes` gives its pin `pad`.
    fn type_written(root: &Sexp, refdes: &str, pad: &str) -> String {
        let placed = root
            .find_all("symbol")
            .find(|s| {
                s.find("lib_id").is_some()
                    && s.find_all("property").any(|p| {
                        p.args()[0].text() == Some("Reference")
                            && p.args()[1].text() == Some(refdes)
                    })
            })
            .unwrap_or_else(|| panic!("{refdes} is not placed"));
        let lib_id = placed.find("lib_id").unwrap().args()[0].text().unwrap();
        let embedded = root
            .find("lib_symbols")
            .unwrap()
            .find_all("symbol")
            .find(|s| s.args()[0].text() == Some(lib_id))
            .unwrap_or_else(|| panic!("{lib_id} is not embedded"));
        embedded
            .find_all("symbol")
            .flat_map(|unit| unit.find_all("pin"))
            .find(|p| p.find("number").unwrap().args()[0].text() == Some(pad))
            .unwrap_or_else(|| panic!("{lib_id} has no pin {pad}"))
            .args()[0]
            .text()
            .unwrap()
            .to_string()
    }

    /// The names of the symbols of `lib_symbols`, in order.
    fn embedded_names(root: &Sexp) -> Vec<String> {
        root.find("lib_symbols")
            .unwrap()
            .find_all("symbol")
            .filter_map(|s| s.args()[0].text().map(str::to_string))
            .collect()
    }

    /// The `drawn_regulator_design()` with a second `IcLdo` in the same package, `U2`, on the
    /// same supply and ground and its own output net, whose output pin is named `rename`.
    fn two_drawn_regulators(rename: &str, u2_output_net: &str) -> Design {
        let mut d = drawn_regulator_design();
        let mut second = d.parts[2].clone();
        second.refdes = "U2".into();
        for pin in &mut second.pins {
            if pin.name == "vout.pos" {
                pin.datasheet = rename.into();
            }
        }
        d.parts.push(second);
        let pin = |pin: &str| PinRef {
            refdes: "U2".into(),
            pin: pin.into(),
        };
        for net in &mut d.nets {
            match net.name.as_str() {
                "VIN" => net.pins.push(pin("3")),
                "GND" => net.pins.push(pin("1")),
                _ => {}
            }
        }
        match d.nets.iter_mut().find(|n| n.name == u2_output_net) {
            Some(net) => net.pins.push(pin("2")),
            None => d.nets.push(Net {
                name: u2_output_net.into(),
                pins: vec![pin("2")],
            }),
        }
        d
    }

    #[test]
    fn two_parts_of_one_kind_and_package_with_different_pin_tables_are_two_symbols_with_two_ids() {
        let d = two_drawn_regulators("VOUT", "5V");
        let text = emit(&d, &mapping(), &sample()).unwrap();
        let root = Sexp::parse(&text).unwrap();
        let names = embedded_names(&root);
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(
            names.len(),
            unique.len(),
            "a name is embedded twice: {names:?}"
        );
        // Each placed part names a symbol that is embedded, and the two are not the same one.
        let id_of = |refdes: &str| -> String {
            root.find_all("symbol")
                .find(|s| {
                    s.find_all("property").any(|p| {
                        p.args()[0].text() == Some("Reference")
                            && p.args()[1].text() == Some(refdes)
                    })
                })
                .and_then(|s| {
                    s.find("lib_id")
                        .map(|l| l.args()[0].text().unwrap().to_string())
                })
                .unwrap()
        };
        let (a, b) = (id_of("U1"), id_of("U2"));
        assert_ne!(a, b);
        assert!(names.contains(&a) && names.contains(&b), "{names:?}");
        // The ids do not depend on the order the parts are in.
        let mut swapped = d.clone();
        swapped.parts.swap(2, 3);
        let other = Sexp::parse(&emit(&swapped, &mapping(), &sample()).unwrap()).unwrap();
        let mut left = embedded_names(&root);
        let mut right = embedded_names(&other);
        left.sort();
        right.sort();
        assert_eq!(left, right);
        // The same two parts with one table keep the name they always had.
        let same = emit(&two_drawn_regulators("OUT", "5V"), &mapping(), &sample()).unwrap();
        assert_eq!(
            embedded_names(&Sexp::parse(&same).unwrap())
                .iter()
                .filter(|n| n.starts_with("netsmith:"))
                .collect::<Vec<_>>(),
            ["netsmith:IcLdo_SOT-23"]
        );
        // Every id that is generated has the same bytes every time.
        assert_eq!(text, emit(&d, &mapping(), &sample()).unwrap());
    }

    #[test]
    fn two_pins_that_give_power_to_one_net_are_one_power_output_and_a_power_input() {
        let mut d = two_drawn_regulators("OUT", "3V3");
        for part in &mut d.parts[2..] {
            for pin in &mut part.pins {
                match pin.name.as_str() {
                    "vin.pos" => pin.power_flow = Some(netsmith_core::PowerFlow::Takes),
                    "vout.pos" => pin.power_flow = Some(netsmith_core::PowerFlow::Gives),
                    _ => {}
                }
            }
        }
        let text = emit(&d, &mapping(), &sample()).unwrap();
        let root = Sexp::parse(&text).unwrap();
        // KiCad's pin matrix reports two outputs on one net: the first (by designator) stays
        // the output and the other is typed as an input, which an output may drive.
        assert_eq!(type_written(&root, "U1", "2"), "power_out");
        assert_eq!(type_written(&root, "U2", "2"), "power_in");
        // Only pins that were not outputs changed: the supply pins are inputs on both.
        assert_eq!(type_written(&root, "U2", "3"), "power_in");
        // The net has its output, so it gets no flag.
        let mut flagged: Vec<String> = flags(&root).into_iter().map(|f| f.1).collect();
        flagged.sort();
        assert_eq!(flagged, ["GND", "VIN"]);
        // The order of the parts does not choose the output.
        let mut swapped = d.clone();
        swapped.parts.swap(2, 3);
        let other = Sexp::parse(&emit(&swapped, &mapping(), &sample()).unwrap()).unwrap();
        assert_eq!(type_written(&other, "U1", "2"), "power_out");
        assert_eq!(type_written(&other, "U2", "2"), "power_in");
        // The design is not changed by the choice.
        assert!(d.parts[3].pins.iter().any(PartPin::gives_power));
    }

    #[test]
    fn two_pins_of_one_part_that_give_power_to_one_net_are_one_power_output() {
        // `.vdd2 ~ .vdd.pos` inside a part: two pins of one part on one net.
        let mut d = drawn_regulator_design();
        for pin in &mut d.parts[2].pins {
            match pin.name.as_str() {
                "vin.pos" => pin.power_flow = Some(netsmith_core::PowerFlow::Takes),
                "vout.pos" => pin.power_flow = Some(netsmith_core::PowerFlow::Gives),
                _ => {}
            }
        }
        d.parts[2].pins.push(PartPin {
            name: "vout2".into(),
            datasheet: "OUT2".into(),
            kind: PinKind::Power,
            pads: vec!["4".into()],
            function: None,
            power_flow: Some(netsmith_core::PowerFlow::Gives),
        });
        d.nets
            .iter_mut()
            .find(|n| n.name == "3V3")
            .unwrap()
            .pins
            .push(PinRef {
                refdes: "U1".into(),
                pin: "4".into(),
            });
        let root = Sexp::parse(&emit(&d, &mapping(), &sample()).unwrap()).unwrap();
        assert_eq!(type_written(&root, "U1", "2"), "power_out");
        assert_eq!(type_written(&root, "U1", "4"), "power_in");
    }

    #[test]
    fn a_giving_pin_that_meets_a_library_symbol_s_output_is_the_one_that_gives_way() {
        // `Ldo` is KiCad's AP1117 with a power output on pad 2 that netsmith cannot retype: a
        // generated pin that gives power to the same net becomes an input.
        let mut d = drawn_regulator_design();
        for pin in &mut d.parts[2].pins {
            if pin.name == "vout.pos" {
                pin.power_flow = Some(netsmith_core::PowerFlow::Gives);
            }
        }
        // U0 sorts before U1, so a rule that only looked at the order would keep U0's pin.
        d.parts.push(Part {
            provides: Vec::new(),
            refdes: "U0".into(),
            kind: "Ldo".into(),
            value: "XC6206".into(),
            package: Some("SOT-23".into()),
            path: String::new(),
            pins: Vec::new(),
        });
        d.nets
            .iter_mut()
            .find(|n| n.name == "3V3")
            .unwrap()
            .pins
            .push(PinRef {
                refdes: "U0".into(),
                pin: "2".into(),
            });
        let root = Sexp::parse(&emit(&d, &mapping(), &sample()).unwrap()).unwrap();
        assert_eq!(type_written(&root, "U0", "2"), "power_out");
        assert_eq!(type_written(&root, "U1", "2"), "power_in");
    }

    #[test]
    fn a_placed_part_is_on_the_bill_of_materials_as_its_symbol_says_and_a_flag_is_on_neither() {
        let part = |refdes: &str, kind: &str, package: Option<&str>| Part {
            provides: Vec::new(),
            refdes: refdes.into(),
            kind: kind.into(),
            value: String::new(),
            package: package.map(str::to_string),
            path: String::new(),
            pins: Vec::new(),
        };
        let d = Design {
            modules: Vec::new(),
            name: "mechanical".into(),
            process: None,
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts: vec![
                part("FID1", "Fiducial", Some("1mm")),
                part("H1", "MountingHole", Some("M3")),
                part("H2", "PlatedHole", Some("M3")),
                part("TP1", "TestPoint", Some("D1.0mm")),
                part("R1", "Resistor", Some("0402")),
                part("U1", "Ldo", Some("SOT-23")),
            ],
            nets: vec![Net {
                name: "N".into(),
                pins: vec![
                    PinRef {
                        refdes: "U1".into(),
                        pin: "3".into(),
                    },
                    PinRef {
                        refdes: "TP1".into(),
                        pin: "1".into(),
                    },
                    PinRef {
                        refdes: "R1".into(),
                        pin: "1".into(),
                    },
                    PinRef {
                        refdes: "H2".into(),
                        pin: "1".into(),
                    },
                ],
            }],
        };
        let lib = SymbolLib::at(testlib::sample_symbols()).with_generators();
        let root = Sexp::parse(&emit(&d, &mapping(), &lib).unwrap()).unwrap();
        let flags_of = |refdes: &str| -> (String, String) {
            let placed = root
                .find_all("symbol")
                .find(|s| {
                    s.find("lib_id").is_some()
                        && s.find_all("property").any(|p| {
                            p.args()[0].text() == Some("Reference")
                                && p.args()[1].text() == Some(refdes)
                        })
                })
                .unwrap_or_else(|| panic!("{refdes} is not placed"));
            let word = |name: &str| placed.find(name).unwrap().args()[0].text().unwrap().into();
            (word("in_bom"), word("on_board"))
        };
        let yes = || ("yes".to_string(), "yes".to_string());
        // KiCad's symbols for the hole and the fiducial are `in_bom no`, and netsmith's own bill
        // of materials leaves them out too (their standard definitions are not assembled).
        for refdes in ["FID1", "H1", "H2"] {
            assert_eq!(
                flags_of(refdes),
                ("no".to_string(), "yes".to_string()),
                "{refdes}"
            );
        }
        assert_eq!(flags_of("TP1"), yes());
        assert_eq!(flags_of("R1"), yes());
        // The flag the exporter places on the net nothing drives is no part at all.
        assert_eq!(flags_of("#FLG01"), ("no".to_string(), "no".to_string()));
    }

    #[test]
    fn a_part_with_no_symbol_in_its_row_and_no_pin_table_is_refused_by_name() {
        let e = emit(&regulator_design_with("IcLdo"), &mapping(), &sample())
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
        let a = emit(&regulator_design(), &mapping(), &sample()).unwrap();
        let b = emit(&regulator_design(), &mapping(), &sample()).unwrap();
        assert_eq!(a, b);
        assert_eq!(Sexp::parse(&a).unwrap().to_kicad_string(), a);
    }

    #[test]
    fn a_flags_uuids_come_from_its_name_and_its_net() {
        let text = emit(&regulator_design(), &mapping(), &sample()).unwrap();
        assert!(text.contains(&uuid_for("regulator/#FLG01/symbol")));
        assert!(text.contains(&uuid_for("regulator/#FLG01/pin/1")));
        assert!(text.contains(&uuid_for("label/GND/#FLG01/1")));
        assert!(text.contains(&uuid_for("label/VIN/#FLG02/1")));
        // Changing a part's value moves no UUID, flags included.
        let mut d = regulator_design();
        d.parts[1].value = "2u2".into();
        let other = emit(&d, &mapping(), &sample()).unwrap();
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
            modules: Vec::new(),
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
        let root = Sexp::parse(&emit(&d, &mapping, &lib).unwrap()).unwrap();
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
        let root = Sexp::parse(&emit(&d, &mapping, &lib).unwrap()).unwrap();
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
        let root = Sexp::parse(&emit(&d, &mapping, &lib).unwrap()).unwrap();
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
        let root = Sexp::parse(&emit(&d, &mapping, &lib).unwrap()).unwrap();
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
        let root = Sexp::parse(&emit(&d, &mapping, &lib).unwrap()).unwrap();
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
        let root = Sexp::parse(&emit(&d, &mapping, &lib).unwrap()).unwrap();
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

    /// The points of the schematic's no-connect markers, as written.
    fn marker_points(root: &Sexp) -> Vec<(String, String)> {
        root.find_all("no_connect")
            .map(|m| {
                let at = m.find("at").unwrap().args();
                (at[0].text().unwrap().into(), at[1].text().unwrap().into())
            })
            .collect()
    }

    #[test]
    fn a_no_connect_pin_of_several_pads_is_one_marker_on_a_stack_of_passive_pins() {
        // KiCad's ERC calls a `no_connect` pad with another pad on its point connected, flag
        // or no flag, so every pad of the stack is `passive` and the point gets one marker.
        let mut d = drawn_regulator_design();
        let extra = |name: &str, kind: PinKind, pads: &[&str]| PartPin {
            name: name.into(),
            datasheet: name.to_uppercase(),
            kind,
            pads: pads.iter().map(|p| p.to_string()).collect(),
            function: None,
            power_flow: None,
        };
        d.parts[2].pins.extend([
            extra("nc", PinKind::NoConnect, &["4", "5", "6"]),
            extra("spare", PinKind::NoConnect, &["7"]),
            extra("unused", PinKind::Output, &["8", "9"]),
        ]);
        let root = parsed(&d);
        for pad in ["4", "5", "6", "8", "9"] {
            assert_eq!(
                type_written(&root, "U1", pad),
                if pad == "8" { "output" } else { "passive" },
                "pad {pad}"
            );
        }
        // A lone no-connect pad is typed so and has no marker; the stack of three has one, and
        // so does the output nothing reaches (the three nets are labelled).
        assert_eq!(type_written(&root, "U1", "7"), "no_connect");
        let points = marker_points(&root);
        assert_eq!(points.len(), 2, "{points:?}");
        let mut unique = points.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique, {
            let mut sorted = points.clone();
            sorted.sort();
            sorted
        });
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
        let root = Sexp::parse(&emit(&d, &mapping, &lib).unwrap()).unwrap();
        assert_eq!(root.find_all("no_connect").count(), 0);
    }

    #[test]
    fn every_unit_of_a_symbol_is_placed_in_a_column_with_the_same_reference() {
        let (_dir, mapping, lib) = acme(&["Dual"]);
        let d = design(
            vec![gadget("U1", "Dual")],
            &[("N", &[("U1", "1"), ("U1", "6")]), ("V", &[("U1", "8")])],
        );
        let root = Sexp::parse(&emit(&d, &mapping, &lib).unwrap()).unwrap();
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
        let root = Sexp::parse(&emit(&d, &mapping, &lib).unwrap()).unwrap();
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
        let root = Sexp::parse(&emit(&d, &mapping, &lib).unwrap()).unwrap();
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
        let e = emit(&d, &Mapping::standard(), &lib)
            .unwrap_err()
            .to_string();
        assert!(
            e.starts_with("R1: symbol library `Device` not found"),
            "{e}"
        );
        assert!(e.contains("/nonexistent/symbols/Device.kicad_sym"), "{e}");
    }
}
