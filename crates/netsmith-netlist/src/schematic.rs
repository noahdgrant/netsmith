//! Reading a KiCad schematic (`.kicad_sch`) and the sheets under it into a netlist, from the
//! files alone, with no KiCad installed (#855).
//!
//! The reader follows KiCad's connection rules for what a schematic of wires, labels and
//! symbols can hold:
//!
//! - Items join where their connection points meet: the two ends of a wire, the end of a pin of
//!   a placed symbol (its library position turned and mirrored as the symbol is), a label, a
//!   sheet pin. A label also joins a wire anywhere along it; a pin, like a wire's end, joins
//!   only at the points of other items (KiCad splits a wire where a pin or another wire meets
//!   it, so a saved file has its ends there).
//! - A local label or a hierarchical label joins every other one of its name in the same
//!   instance of a sheet; a global label and a power symbol join every other one of their name
//!   in the design. A power symbol (the library symbol says `(power)`) names its net with its
//!   value, or, in a file older than KiCad 8 (format 20230221), its pin's name, as KiCad does
//!   when it opens one. A hidden power-input pin of any other symbol joins the net of its pin
//!   name. A local power symbol (`(power local)`) joins only within its sheet instance.
//! - A sheet symbol's pin joins the hierarchical labels of its name in the sheet it places. A
//!   sheet's file is found relative to the file of the sheet that places it, and each placed
//!   sheet is an instance of its own: a symbol takes its designator and unit from the
//!   `instances` entry of the instance's path (or the root's `symbol_instances`, before KiCad 7).
//!
//! A symbol excluded from the board (`(on_board no)`) is not in the netlist, as the board does
//! not get it. A pin joined to nothing else is on a net named `unconnected-(...)`, which
//! [`Netlist::from_parts`] leaves out. Net names follow KiCad's choice of driver (a global name
//! before a local one, a local label before a hierarchical one, a named net before a pin's
//! `Net-(R1-Pad1)`) but are not part of a comparison. A bus (a bus wire or a bus entry) is
//! refused with its file named: the reader does not split one into its members.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use netsmith_sexp::Sexp;

use crate::model::Netlist;

/// A point in KiCad's schematic units (100 nm), so points compare exactly.
type Point = (i64, i64);

fn iu(mm: f64) -> i64 {
    (mm * 10_000.0).round() as i64
}

/// The first file format whose power symbols are named by their value (KiCad 8). KiCad opens an
/// older file with each power symbol's value set to its pin's name.
const POWER_BY_VALUE: i64 = 20230221;

impl Netlist {
    /// Read a schematic and its sheets from their files.
    pub fn from_kicad_schematic(root: &Path) -> Result<Netlist, String> {
        read_schematic(root, &|p: &Path| {
            std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))
        })
    }
}

/// Read a schematic whose files `read` gives, by their paths.
pub(crate) fn read_schematic(
    root: &Path,
    read: &dyn Fn(&Path) -> Result<String, String>,
) -> Result<Netlist, String> {
    let mut reader = Reader {
        read,
        files: Vec::new(),
        by_path: HashMap::new(),
        instances: Vec::new(),
    };
    let file = reader.file(root)?;
    let root_uuid = reader.files[file].uuid.clone();
    reader.walk(file, vec![root_uuid], "/".to_string(), &mut Vec::new())?;
    reader.netlist()
}

/// One file of the schematic.
struct File {
    path: PathBuf,
    version: i64,
    uuid: String,
    root: Sexp,
}

/// One placed sheet: a file, at a path of sheet symbols from the root.
struct Instance {
    file: usize,
    /// The UUIDs of the root file and of each sheet symbol down to this sheet.
    uuids: Vec<String>,
    /// The sheet names, as KiCad prefixes a local net's name: `/` for the root, `/Power/` below.
    names: String,
    /// The pins of the sheet symbol that places this sheet, with their parent instance.
    parent: Option<(usize, Vec<(String, Point)>)>,
}

struct Reader<'a> {
    read: &'a dyn Fn(&Path) -> Result<String, String>,
    files: Vec<File>,
    by_path: HashMap<PathBuf, usize>,
    instances: Vec<Instance>,
}

fn text(s: &Sexp, i: usize) -> Option<&str> {
    s.items().get(i).and_then(Sexp::text)
}

fn number(s: &Sexp, i: usize) -> Option<f64> {
    text(s, i).and_then(|t| t.parse().ok())
}

/// The value of a `(property "Name" "value" ...)` of an item.
fn property<'s>(s: &'s Sexp, name: &str) -> Option<&'s str> {
    s.find_all("property")
        .find(|p| text(p, 1) == Some(name))
        .and_then(|p| text(p, 2))
}

/// The point of an item's `(at x y ...)`.
fn at(s: &Sexp) -> Option<Point> {
    let a = s.find("at")?;
    Some((iu(number(a, 1)?), iu(number(a, 2)?)))
}

/// Whether a list holds a bare `yes`/`no` flag, as `(on_board no)` or a bare `hide`.
fn flag(s: &Sexp, name: &str) -> Option<bool> {
    if s.items()
        .iter()
        .any(|i| matches!(i, Sexp::Atom(a) if a == name))
    {
        return Some(true);
    }
    s.find(name).map(|f| text(f, 1) != Some("no"))
}

impl Reader<'_> {
    /// Read and parse a file once.
    fn file(&mut self, path: &Path) -> Result<usize, String> {
        let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if let Some(&i) = self.by_path.get(&key) {
            return Ok(i);
        }
        let source = (self.read)(path)?;
        let root = Sexp::parse(&source)
            .map_err(|e| format!("{}: cannot read the schematic: {e}", path.display()))?;
        if root.head() != Some("kicad_sch") {
            return Err(format!(
                "{}: the file is not a KiCad schematic: it has no `(kicad_sch` list",
                path.display()
            ));
        }
        for bus in ["bus", "bus_entry"] {
            if root.find(bus).is_some() {
                return Err(format!(
                    "{}: the schematic has a bus, which compare does not read: export its netlist with \
                     `kicad-cli sch export netlist --format kicadxml` and compare that",
                    path.display()
                ));
            }
        }
        let version = root
            .find("version")
            .and_then(|v| number(v, 1))
            .map_or(0, |v| v as i64);
        let uuid = root
            .find("uuid")
            .and_then(|u| text(u, 1))
            .unwrap_or_default()
            .to_string();
        self.files.push(File {
            path: path.to_path_buf(),
            version,
            uuid,
            root,
        });
        self.by_path.insert(key, self.files.len() - 1);
        Ok(self.files.len() - 1)
    }

    /// Record the instance of `file` at `uuids` and the sheets it places, depth first.
    fn walk(
        &mut self,
        file: usize,
        uuids: Vec<String>,
        names: String,
        open: &mut Vec<usize>,
    ) -> Result<usize, String> {
        if open.contains(&file) {
            return Err(format!(
                "{}: the sheet places itself, through its own sheets",
                self.files[file].path.display()
            ));
        }
        open.push(file);
        let me = self.instances.len();
        self.instances.push(Instance {
            file,
            uuids: uuids.clone(),
            names: names.clone(),
            parent: None,
        });
        let sheets: Vec<Sexp> = self.files[file].root.find_all("sheet").cloned().collect();
        let dir = self.files[file]
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        for sheet in &sheets {
            let here = self.files[file].path.display().to_string();
            let name = property(sheet, "Sheetname")
                .or_else(|| property(sheet, "Sheet name"))
                .unwrap_or_default();
            let Some(sheet_file) =
                property(sheet, "Sheetfile").or_else(|| property(sheet, "Sheet file"))
            else {
                return Err(format!("{here}: sheet `{name}` names no file"));
            };
            // A file saved on Windows may separate folders with `\`.
            let relative = sheet_file.replace('\\', "/");
            let path = dir.join(&relative);
            if !path.is_file() {
                return Err(format!(
                    "{here}: sheet `{name}` places {sheet_file}, which is not at {}",
                    path.display()
                ));
            }
            let uuid = sheet
                .find("uuid")
                .and_then(|u| text(u, 1))
                .unwrap_or_default();
            let pins = sheet
                .find_all("pin")
                .filter_map(|p| Some((text(p, 1)?.to_string(), at(p)?)))
                .collect();
            let child = self.file(&path)?;
            let mut below = uuids.clone();
            below.push(uuid.to_string());
            let at = self.walk(child, below, format!("{names}{name}/"), open)?;
            self.instances[at].parent = Some((me, pins));
        }
        open.pop();
        Ok(me)
    }

    /// The designator and unit of a placed symbol in an instance: the entry of its path in the
    /// symbol's `instances` (in any project), or in the root's `symbol_instances` (KiCad 6), or
    /// else its `Reference` property and `unit`.
    fn reference(&self, instance: &Instance, symbol: &Sexp) -> (String, u32, Option<Sexp>) {
        let own_unit = symbol
            .find("unit")
            .and_then(|u| number(u, 1))
            .map_or(1, |u| u as u32);
        let path = format!("/{}", instance.uuids.join("/"));
        // The same path below the root, for a sheet read on its own or a root of another UUID.
        let below = |p: &str| p.splitn(3, '/').nth(2).map(str::to_string);
        let ours_below = below(&path).unwrap_or_default();
        let entries: Vec<&Sexp> = symbol
            .find("instances")
            .into_iter()
            .flat_map(|i| i.find_all("project"))
            .flat_map(|p| p.find_all("path"))
            .collect();
        let pick = entries
            .iter()
            .find(|e| text(e, 1) == Some(path.as_str()))
            .or_else(|| {
                entries
                    .iter()
                    .find(|e| text(e, 1).and_then(below).unwrap_or_default() == ours_below)
            });
        let from = |e: &Sexp| {
            let r = e.find("reference").and_then(|r| text(r, 1));
            let u = e.find("unit").and_then(|u| number(u, 1)).map(|u| u as u32);
            (r.map(str::to_string), u)
        };
        if let Some(e) = pick {
            let (r, u) = from(e);
            if let Some(r) = r {
                return (r, u.unwrap_or(own_unit), None);
            }
        }
        // KiCad 6: `(symbol_instances (path "/<sheet>/<symbol>" (reference ..) (unit ..)
        // (value ..) (footprint ..)))` in the root file, by the path below the root.
        let uuid = symbol
            .find("uuid")
            .and_then(|u| text(u, 1))
            .unwrap_or_default();
        let old = format!(
            "{}/{uuid}",
            instance.uuids[1..]
                .iter()
                .map(|u| format!("/{u}"))
                .collect::<String>()
        );
        if let Some(e) = self.files[self.instances[0].file]
            .root
            .find("symbol_instances")
            .into_iter()
            .flat_map(|s| s.find_all("path"))
            .find(|e| text(e, 1) == Some(old.as_str()))
        {
            let (r, u) = from(e);
            if let Some(r) = r {
                return (r, u.unwrap_or(own_unit), Some(e.clone()));
            }
        }
        let r = property(symbol, "Reference").unwrap_or_default();
        (r.to_string(), own_unit, None)
    }

    fn netlist(&self) -> Result<Netlist, String> {
        let mut graph = Graph::default();
        let mut components: Vec<(String, String, String)> = Vec::new();
        let mut seen_refs: HashMap<String, ()> = HashMap::new();
        // Each instance's sheet-pin nodes are made with its parent; the label keys join them.
        for (index, instance) in self.instances.iter().enumerate() {
            let file = &self.files[instance.file];
            let here = file.path.display().to_string();
            let local = |name: &str| format!("{index}\u{0}{name}");
            let global = |name: &str| format!("\u{1}{name}");
            let mut points: HashMap<Point, Vec<usize>> = HashMap::new();
            let mut wires: Vec<(Point, Point, usize)> = Vec::new();
            let mut labels: Vec<(Point, usize)> = Vec::new();
            for wire in file.root.find_all("wire") {
                let ends: Vec<Point> = wire
                    .find("pts")
                    .into_iter()
                    .flat_map(|p| p.find_all("xy"))
                    .filter_map(|xy| Some((iu(number(xy, 1)?), iu(number(xy, 2)?))))
                    .collect();
                let [a, b] = ends[..] else {
                    return Err(format!("{here}: a wire has {} points, not 2", ends.len()));
                };
                let node = graph.node();
                points.entry(a).or_default().push(node);
                points.entry(b).or_default().push(node);
                wires.push((a, b, node));
            }
            for (kind, priority) in [
                ("label", Priority::LocalLabel),
                ("hierarchical_label", Priority::HierLabel),
                ("global_label", Priority::GlobalLabel),
            ] {
                for label in file.root.find_all(kind) {
                    let (Some(name), Some(p)) = (text(label, 1), at(label)) else {
                        return Err(format!("{here}: a {kind} has no text or position"));
                    };
                    let node = graph.node();
                    points.entry(p).or_default().push(node);
                    labels.push((p, node));
                    let (key, shown) = if priority == Priority::GlobalLabel {
                        (global(name), name.to_string())
                    } else {
                        (local(name), format!("{}{name}", instance.names))
                    };
                    graph.key(node, key);
                    graph.name(node, priority, shown);
                }
            }
            // This instance's own sheet symbols' pins: each joins the hierarchical labels of its
            // name in the instance it places.
            for (child, placed) in self.instances.iter().enumerate() {
                let Some((parent, pins)) = &placed.parent else {
                    continue;
                };
                if *parent != index {
                    continue;
                }
                for (name, p) in pins {
                    let node = graph.node();
                    points.entry(*p).or_default().push(node);
                    graph.key(node, format!("{child}\u{0}{name}"));
                }
            }
            let library = Library::of(&file.root);
            for symbol in file.root.find_all("symbol") {
                if flag(symbol, "on_board") == Some(false) {
                    continue;
                }
                let lib_name = symbol
                    .find("lib_name")
                    .or_else(|| symbol.find("lib_id"))
                    .and_then(|l| text(l, 1))
                    .unwrap_or_default();
                let lib = library
                    .symbol(lib_name)
                    .map_err(|e| format!("{here}: {e}"))?;
                let (refdes, unit, old) = self.reference(instance, symbol);
                let value = old
                    .as_ref()
                    .and_then(|e| e.find("value"))
                    .and_then(|v| text(v, 1))
                    .or_else(|| property(symbol, "Value"))
                    .unwrap_or_default()
                    .to_string();
                let footprint = old
                    .as_ref()
                    .and_then(|e| e.find("footprint"))
                    .and_then(|v| text(v, 1))
                    .or_else(|| property(symbol, "Footprint"))
                    .unwrap_or_default()
                    .to_string();
                if seen_refs.insert(refdes.clone(), ()).is_none() {
                    components.push((refdes.clone(), value.clone(), footprint));
                }
                let style = symbol
                    .find("body_style")
                    .or_else(|| symbol.find("convert"))
                    .and_then(|s| number(s, 1))
                    .map_or(1, |s| s as u32);
                let origin =
                    at(symbol).ok_or_else(|| format!("{here}: {refdes} has no position"))?;
                let angle = symbol.find("at").and_then(|a| number(a, 3)).unwrap_or(0.0);
                let mirror = symbol.find("mirror").and_then(|m| text(m, 1));
                let transform = Transform::of(angle, mirror).ok_or_else(|| {
                    format!("{here}: {refdes} is turned by {angle}°, not a multiple of 90°")
                })?;
                for pin in &lib.pins {
                    if (pin.unit != 0 && pin.unit != unit) || (pin.style != 0 && pin.style != style)
                    {
                        continue;
                    }
                    let (dx, dy) = transform.apply(pin.at);
                    let p = (origin.0 + dx, origin.1 + dy);
                    let node = graph.node();
                    points.entry(p).or_default().push(node);
                    graph.pins.push(PinNode {
                        refdes: refdes.clone(),
                        number: pin.number.clone(),
                        name: pin.name.clone(),
                        node,
                    });
                    let global_power = pin.power_in && (pin.hidden || lib.power != Power::No);
                    if global_power {
                        let power_name = if lib.power != Power::No && file.version >= POWER_BY_VALUE
                        {
                            value.clone()
                        } else {
                            pin.name.clone()
                        };
                        if lib.power == Power::Local {
                            graph.key(node, local(&power_name));
                            graph.name(
                                node,
                                Priority::LocalPower,
                                format!("{}{power_name}", instance.names),
                            );
                        } else {
                            graph.key(node, global(&power_name));
                            graph.name(node, Priority::GlobalPower, power_name);
                        }
                    }
                }
            }
            for nodes in points.values() {
                for pair in nodes.windows(2) {
                    graph.union(pair[0], pair[1]);
                }
            }
            for (p, label) in &labels {
                for (a, b, wire) in &wires {
                    if on_segment(*p, *a, *b) {
                        graph.union(*label, *wire);
                    }
                }
            }
        }
        graph.join_keys();
        let nets = graph.nets();
        Netlist::from_parts(components, nets).map_err(|e| e.to_string())
    }
}

/// Whether `p` is on the segment from `a` to `b`, its ends included.
fn on_segment(p: Point, a: Point, b: Point) -> bool {
    let cross =
        (b.0 - a.0) as i128 * (p.1 - a.1) as i128 - (b.1 - a.1) as i128 * (p.0 - a.0) as i128;
    cross == 0
        && p.0 >= a.0.min(b.0)
        && p.0 <= a.0.max(b.0)
        && p.1 >= a.1.min(b.1)
        && p.1 <= a.1.max(b.1)
}

/// How a placed symbol maps a library point (y up) onto the sheet (y down): KiCad's
/// `TRANSFORM`, `x' = x1·x + y1·y`, `y' = x2·x + y2·y`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Transform {
    x1: i64,
    y1: i64,
    x2: i64,
    y2: i64,
}

impl Transform {
    /// KiCad's orientation at `angle` degrees counterclockwise, then mirrored about the x axis
    /// (`x`, top to bottom) or the y axis (`y`, left to right), composed as `SCH_SYMBOL::
    /// SetOrientation` composes them.
    fn of(angle: f64, mirror: Option<&str>) -> Option<Transform> {
        let turns = (angle / 90.0).round();
        if (turns * 90.0 - angle).abs() > 1e-6 {
            return None;
        }
        let mut t = Transform {
            x1: 1,
            y1: 0,
            x2: 0,
            y2: -1,
        };
        let counterclockwise = Transform {
            x1: 0,
            y1: 1,
            x2: -1,
            y2: 0,
        };
        for _ in 0..(turns as i64).rem_euclid(4) {
            t = t.then(counterclockwise);
        }
        match mirror {
            Some("x") => {
                t = t.then(Transform {
                    x1: 1,
                    y1: 0,
                    x2: 0,
                    y2: -1,
                })
            }
            Some("y") => {
                t = t.then(Transform {
                    x1: -1,
                    y1: 0,
                    x2: 0,
                    y2: 1,
                })
            }
            _ => {}
        }
        Some(t)
    }

    /// `self` modified by `temp`, as KiCad composes them.
    fn then(self, temp: Transform) -> Transform {
        Transform {
            x1: self.x1 * temp.x1 + self.x2 * temp.y1,
            y1: self.y1 * temp.x1 + self.y2 * temp.y1,
            x2: self.x1 * temp.x2 + self.x2 * temp.y2,
            y2: self.y1 * temp.x2 + self.y2 * temp.y2,
        }
    }

    fn apply(self, (x, y): Point) -> Point {
        (self.x1 * x + self.y1 * y, self.x2 * x + self.y2 * y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Power {
    No,
    Global,
    Local,
}

/// A pin of a library symbol, at its connection point in library units.
struct LibPin {
    number: String,
    name: String,
    at: Point,
    /// 0 for a pin of every unit (or body style).
    unit: u32,
    style: u32,
    power_in: bool,
    hidden: bool,
}

struct LibSymbol {
    pins: Vec<LibPin>,
    power: Power,
}

/// The library symbols a schematic file embeds (`lib_symbols`).
struct Library<'a> {
    symbols: HashMap<&'a str, &'a Sexp>,
}

impl<'a> Library<'a> {
    fn of(root: &'a Sexp) -> Self {
        let symbols = root
            .find("lib_symbols")
            .into_iter()
            .flat_map(|l| l.find_all("symbol"))
            .filter_map(|s| Some((text(s, 1)?, s)))
            .collect();
        Library { symbols }
    }

    fn symbol(&self, name: &str) -> Result<LibSymbol, String> {
        let mut s = *self.symbols.get(name).ok_or_else(|| {
            format!("the schematic has no library symbol `{name}` for a symbol it places")
        })?;
        let power = |s: &Sexp| {
            s.find("power").map(|p| match text(p, 1) {
                Some("local") => Power::Local,
                _ => Power::Global,
            })
        };
        let mut found = power(s);
        // A derived symbol (`extends`) draws its parent's pins.
        let mut depth = 0;
        while let Some(parent) = s.find("extends").and_then(|e| text(e, 1)) {
            depth += 1;
            let lib_prefix = name.split_once(':').map(|(l, _)| format!("{l}:"));
            s = self
                .symbols
                .get(parent)
                .or_else(|| {
                    lib_prefix
                        .as_ref()
                        .and_then(|l| self.symbols.get(format!("{l}{parent}").as_str()))
                })
                .copied()
                .filter(|_| depth < 16)
                .ok_or_else(|| format!("library symbol `{name}` extends `{parent}`, which the schematic does not have"))?;
            found = found.or(power(s));
        }
        let mut pins = Vec::new();
        let mut add = |list: &Sexp, unit: u32, style: u32| -> Result<(), String> {
            for pin in list.find_all("pin") {
                let place = pin.find("at");
                let (Some(x), Some(y)) = (
                    place.and_then(|a| number(a, 1)),
                    place.and_then(|a| number(a, 2)),
                ) else {
                    return Err(format!("a pin of library symbol `{name}` has no position"));
                };
                pins.push(LibPin {
                    number: pin
                        .find("number")
                        .and_then(|n| text(n, 1))
                        .unwrap_or_default()
                        .to_string(),
                    name: pin
                        .find("name")
                        .and_then(|n| text(n, 1))
                        .unwrap_or_default()
                        .to_string(),
                    at: (iu(x), iu(y)),
                    unit,
                    style,
                    power_in: text(pin, 1) == Some("power_in"),
                    hidden: flag(pin, "hide") == Some(true),
                });
            }
            Ok(())
        };
        add(s, 0, 0)?;
        for unit in s.find_all("symbol") {
            // `R_1_1`: the symbol's name, its unit and its body style.
            let id = text(unit, 1).unwrap_or_default();
            let mut parts = id.rsplitn(3, '_');
            let style = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let unit_number = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            add(unit, unit_number, style)?;
        }
        Ok(LibSymbol {
            pins,
            power: found.unwrap_or(Power::No),
        })
    }
}

/// Which name of a net KiCad prefers, the higher first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Priority {
    HierLabel,
    LocalLabel,
    LocalPower,
    GlobalPower,
    GlobalLabel,
}

struct PinNode {
    refdes: String,
    number: String,
    name: String,
    node: usize,
}

/// The items of all instances as nodes, joined by a union-find.
#[derive(Default)]
struct Graph {
    parent: Vec<usize>,
    keys: Vec<(String, usize)>,
    names: Vec<(usize, Priority, String)>,
    pins: Vec<PinNode>,
}

impl Graph {
    fn node(&mut self) -> usize {
        self.parent.push(self.parent.len());
        self.parent.len() - 1
    }

    fn find(&mut self, mut n: usize) -> usize {
        while self.parent[n] != n {
            self.parent[n] = self.parent[self.parent[n]];
            n = self.parent[n];
        }
        n
    }

    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.parent[a.max(b)] = a.min(b);
        }
    }

    fn key(&mut self, node: usize, key: String) {
        self.keys.push((key, node));
    }

    fn name(&mut self, node: usize, priority: Priority, name: String) {
        self.names.push((node, priority, name));
    }

    /// Join the nodes that share a label's or a power symbol's name.
    fn join_keys(&mut self) {
        let mut first: HashMap<String, usize> = HashMap::new();
        for (key, node) in std::mem::take(&mut self.keys) {
            match first.get(&key) {
                Some(&other) => self.union(node, other),
                None => {
                    first.insert(key, node);
                }
            }
        }
    }

    /// Each set of joined nodes with a pin, as a named net.
    fn nets(&mut self) -> Vec<(String, Vec<(String, String)>)> {
        let mut pins_of: HashMap<usize, Vec<usize>> = HashMap::new();
        for i in 0..self.pins.len() {
            let root = self.find(self.pins[i].node);
            pins_of.entry(root).or_default().push(i);
        }
        let mut named: HashMap<usize, (Priority, usize, String)> = HashMap::new();
        for i in 0..self.names.len() {
            let (node, priority, name) = self.names[i].clone();
            let root = self.find(node);
            // The higher priority, then the name nearer the root, then the first in order.
            let rank = (priority, usize::MAX - name.matches('/').count());
            let better = named.get(&root).is_none_or(|(p, depth, n)| {
                rank > (*p, *depth) || (rank == (*p, *depth) && name < *n)
            });
            if better {
                named.insert(root, (rank.0, rank.1, name));
            }
        }
        let mut nets: Vec<(String, Vec<(String, String)>)> = pins_of
            .into_iter()
            .map(|(root, pins)| {
                let name = match named.get(&root) {
                    Some((_, _, n)) => n.clone(),
                    None => {
                        let unconnected = pins.len() == 1;
                        pins.iter()
                            .map(|&i| default_name(&self.pins[i], unconnected))
                            .min()
                            .unwrap_or_default()
                    }
                };
                let pins = pins
                    .iter()
                    .map(|&i| (self.pins[i].refdes.clone(), self.pins[i].number.clone()))
                    .collect();
                (name, pins)
            })
            .collect();
        nets.sort();
        nets
    }
}

/// KiCad's name of a net that only pins name: `Net-(D1-A)`, `Net-(R1-Pad1)`, or
/// `unconnected-(U1-PA0-Pad12)` for a pin joined to nothing.
fn default_name(pin: &PinNode, unconnected: bool) -> String {
    let named = !pin.name.is_empty() && pin.name != "~";
    match (unconnected, named) {
        (true, true) => format!(
            "unconnected-({}-{}-Pad{})",
            pin.refdes, pin.name, pin.number
        ),
        (true, false) => format!("unconnected-({}-Pad{})", pin.refdes, pin.number),
        (false, true) => format!("Net-({}-{})", pin.refdes, pin.name),
        (false, false) => format!("Net-({}-Pad{})", pin.refdes, pin.number),
    }
}
