//! Symbols read from KiCad's `.kicad_sym` libraries: the installed ones and a project's
//! own (ADR 0016, ADR 0021).
//!
//! A library file holds every symbol of the library, so [`SymbolLib`] parses a file once
//! per run and extracts the symbols asked for. A symbol that `extends` another is
//! flattened, since the schematic's `lib_symbols` holds whole symbols.
//!
//! # What the loader does (ADR 0021, accepted by the owner on 2026-11-04)
//!
//! A symbol that `extends` another is flattened: the parent's drawing, pins and settings, with
//! the child's properties on top, and its units renamed for the child because KiCad wants a unit's
//! name to start with its symbol's. Every KiCad electrical type is read, with the pin's name,
//! number, position, direction, length, whether the symbol hides it, and the unit it is in. A
//! hidden pin that repeats another pin's point (how stock symbols stack the pads of one supply) is
//! an ordinary pin. Only the first body style lists pins. A symbol's `(alternate ...)` pin names
//! are not compared with a pin's functions, and the version in a library file's header is not
//! compared with the KiCad that `kicad-cli` is. A missing library says which file it looked for
//! and the variable that moves it; a missing symbol names its library. A library named like an
//! installed one is an error and not an override.
//!
//! **Multi-unit symbols are supported** (ADR 0016 had refused them and placed unit 1 only). The
//! loader keeps the pin-to-unit assignment; the schematic writer places every unit of a part as
//! its own instance with the same reference and the right `unit` field, in a column, each unit
//! with its own pins and labels, and a pin drawn in the shared unit 0 is labelled in every unit.
//! Units are spaced by their pins' extent (a unit's body is not measured, only its pins, with a
//! fixed gap). A part's cell on the sheet is as large as its units and, for a symbol of more than
//! four pins, the labels that run from them; a small part keeps the grid it had, so the existing
//! exports did not move. KiCad's ERC passes on a dual op-amp-like symbol with two amplifier units
//! and a supply unit, and on one with a pin in the shared unit (`crates/netsmith/tests/erc.rs`).
//! The viewer reads from this loader too (`netsmith-view`'s `Glyph::for_part` takes the mapping
//! and the [`SymbolLib`]; the units of a symbol are drawn one below the other; the box drawn where
//! there is no symbol lists every pin of the package from the part's pin facts, supplies on top,
//! grounds below, inputs left, outputs right, no-connects last, the chosen function beside the
//! name; a missing library gives the box and the reason as its tooltip).
//!
//! The bundled copies of KiCad's libraries (`crates/netsmith-kicad/symbols/` and
//! `crates/netsmith-pcb/fixtures/`) are gone. Tests write small libraries of their own
//! (`netsmith_kicad::testlib`), and the tests that want the stock libraries skip without KiCad.
//! The licensing reading is in `docs/research/kicad-library-licensing.md`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::install::{self, NotFound};
use crate::sexp::Sexp;

/// The environment variable that names the folder holding the `.kicad_sym` libraries.
pub const ROOT_VARIABLE: &str = install::SYMBOLS_VARIABLE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolError {
    /// The name is not `Library:Name`.
    BadName(String),
    /// There is no library file by that name.
    NoLibrary {
        library: String,
        path: PathBuf,
        /// Whether the library is a project's own, listed in `kicad.toml`.
        project: bool,
    },
    /// The library is not a project's own and no KiCad install was found to look in.
    NoInstall {
        library: String,
        lookup: NotFound,
    },
    /// The library is a project's own and an installed library has the same name.
    Ambiguous {
        library: String,
        installed: PathBuf,
        project: PathBuf,
    },
    Unreadable {
        path: PathBuf,
        reason: String,
    },
    /// The file is not a symbol library netsmith can read.
    Invalid {
        name: String,
        reason: String,
    },
    /// The mapping row has no `symbol`, so the part is drawn from its pin table, and it has none.
    NoPinTable {
        kind: String,
    },
    /// The library exists but does not hold the symbol.
    NoSymbol {
        name: String,
        library: String,
        path: PathBuf,
    },
}

impl std::fmt::Display for SymbolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadName(name) => write!(f, "`{name}` is not a symbol name like `Library:Name`"),
            Self::NoLibrary {
                library,
                path,
                project: true,
            } => write!(
                f,
                "symbol library `{library}` not found: kicad.toml lists {}, which is not a file",
                path.display()
            ),
            Self::NoLibrary {
                library,
                path,
                project: false,
            } => write!(
                f,
                "symbol library `{library}` not found: looked for {}; set {ROOT_VARIABLE} to the \
                 folder that holds the .kicad_sym libraries, or list the library in kicad.toml",
                path.display()
            ),
            Self::NoInstall { library, lookup } => write!(
                f,
                "symbol library `{library}` not found: {lookup}\n(a library of the project itself is listed in kicad.toml and needs no KiCad)"
            ),
            Self::Ambiguous {
                library,
                installed,
                project,
            } => write!(
                f,
                "symbol library `{library}` is both installed ({}) and listed in kicad.toml ({}); \
                 give the project's library another name",
                installed.display(),
                project.display()
            ),
            Self::Unreadable { path, reason } => write!(f, "{}: {reason}", path.display()),
            Self::Invalid { name, reason } => write!(f, "symbol `{name}`: {reason}"),
            Self::NoPinTable { kind } => write!(
                f,
                "the mapping row of `{kind}` has no `symbol`, so the part is drawn from its pin \
                 table (ADR 0097), and a `{kind}` has no pin table; name a `symbol` in the row"
            ),
            Self::NoSymbol {
                name,
                library,
                path,
            } => write!(
                f,
                "symbol `{name}` not found in the library `{library}` ({})",
                path.display()
            ),
        }
    }
}

impl std::error::Error for SymbolError {}

/// What a pin does electrically, as KiCad's symbol file names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinType {
    Input,
    Output,
    Bidirectional,
    TriState,
    Passive,
    Free,
    Unspecified,
    /// Takes power from the net.
    PowerIn,
    /// Supplies power to the net.
    PowerOut,
    OpenCollector,
    OpenEmitter,
    NoConnect,
}

impl PinType {
    pub fn parse(text: &str) -> Option<PinType> {
        Some(match text {
            "input" => Self::Input,
            "output" => Self::Output,
            "bidirectional" => Self::Bidirectional,
            "tri_state" => Self::TriState,
            "passive" => Self::Passive,
            "free" => Self::Free,
            "unspecified" => Self::Unspecified,
            "power_in" => Self::PowerIn,
            "power_out" => Self::PowerOut,
            "open_collector" => Self::OpenCollector,
            "open_emitter" => Self::OpenEmitter,
            "no_connect" => Self::NoConnect,
            _ => return None,
        })
    }

    /// The word the symbol file uses.
    pub fn name(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
            Self::Bidirectional => "bidirectional",
            Self::TriState => "tri_state",
            Self::Passive => "passive",
            Self::Free => "free",
            Self::Unspecified => "unspecified",
            Self::PowerIn => "power_in",
            Self::PowerOut => "power_out",
            Self::OpenCollector => "open_collector",
            Self::OpenEmitter => "open_emitter",
            Self::NoConnect => "no_connect",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PinGeometry {
    pub x: f64,
    pub y: f64,
    pub angle: f64,
}

/// One pin of a symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolPin {
    /// The pad number the pin is for.
    pub number: String,
    /// The name the symbol draws; `~` when it leaves the pin unnamed.
    pub name: String,
    pub kind: PinType,
    pub x: f64,
    pub y: f64,
    /// The direction the pin line runs from its connection point toward the body.
    pub angle: f64,
    pub length: f64,
    /// Stock symbols stack several pads on one point and hide all but one.
    pub hidden: bool,
    /// The unit the pin belongs to; 0 is shared by every unit.
    pub unit: u32,
}

impl SymbolPin {
    pub fn geometry(&self) -> PinGeometry {
        PinGeometry {
            x: self.x,
            y: self.y,
            angle: self.angle,
        }
    }

    /// Whether the symbol leaves the pin unnamed.
    pub fn is_unnamed(&self) -> bool {
        self.name == "~" || self.name.is_empty()
    }
}

/// One symbol of a library, with `extends` flattened.
#[derive(Debug, Clone)]
pub struct Symbol {
    lib_id: String,
    definition: Sexp,
    pins: Vec<SymbolPin>,
    units: u32,
}

impl Symbol {
    /// The symbol `lib_id` made from `definition`, a `(symbol "<name>" ...)` whose units are
    /// named `<name>_<unit>_<style>`: how [`crate::generate`] makes the symbol of a pin table.
    pub(crate) fn from_definition(
        lib_id: &str,
        name: &str,
        definition: Sexp,
    ) -> Result<Symbol, String> {
        read_symbol(lib_id, name, definition)
    }

    /// The definition as a library file holds it: named `<name>`, with no library in front.
    pub fn library_definition(&self) -> Sexp {
        let bare = self
            .lib_id
            .split_once(':')
            .map_or(self.lib_id.as_str(), |(_, n)| n);
        let mut all = vec![Sexp::atom("symbol"), Sexp::string(bare)];
        all.extend(self.definition.args().iter().skip(1).cloned());
        Sexp::List(all)
    }

    /// `Device:R`
    pub fn lib_id(&self) -> &str {
        &self.lib_id
    }

    /// The full `(symbol "lib:Name" ...)` definition, as embedded in `lib_symbols`.
    pub fn definition(&self) -> &Sexp {
        &self.definition
    }

    /// Whether the symbol says a part drawn with it is on a bill of materials: its
    /// `(in_bom ...)`, `yes` when it says nothing.
    pub fn in_bom(&self) -> bool {
        self.says_yes("in_bom")
    }

    /// Whether the symbol says a part drawn with it is on the board: its `(on_board ...)`,
    /// `yes` when it says nothing.
    pub fn on_board(&self) -> bool {
        self.says_yes("on_board")
    }

    fn says_yes(&self, flag: &str) -> bool {
        self.definition
            .find(flag)
            .and_then(|f| f.args().first())
            .and_then(Sexp::text)
            != Some("no")
    }

    /// How many units the symbol has: 1 for an ordinary symbol.
    pub fn units(&self) -> u32 {
        self.units
    }

    /// Every pin, in definition order, in the first body style. A pin in unit 0 is shared
    /// by all units.
    pub fn pins(&self) -> &[SymbolPin] {
        &self.pins
    }

    /// The pins drawn in `unit` (1-based): its own and the shared ones.
    pub fn pins_of_unit(&self, unit: u32) -> impl Iterator<Item = &SymbolPin> {
        self.pins
            .iter()
            .filter(move |p| p.unit == unit || p.unit == 0)
    }

    pub fn pin(&self, number: &str) -> Option<&SymbolPin> {
        self.pins.iter().find(|p| p.number == number)
    }

    /// Pin numbers in definition order.
    pub fn pin_numbers(&self) -> Vec<String> {
        self.pins.iter().map(|p| p.number.clone()).collect()
    }

    /// Position and direction of a pin, in the symbol's own coordinates (y grows upward,
    /// unlike the schematic sheet). `angle` is the direction the pin line runs from its
    /// connection point toward the body (0 = right, 90 = up).
    pub fn pin_geometry(&self, number: &str) -> Option<PinGeometry> {
        self.pin(number).map(SymbolPin::geometry)
    }

    /// The electrical type of a pin, as the symbol declares it.
    pub fn pin_type(&self, number: &str) -> Option<PinType> {
        self.pin(number).map(|p| p.kind)
    }

    /// Where the symbol wants a field (`Reference`, `Value`) relative to its origin, as
    /// `(x, y, rotation)` with y up.
    pub fn field_offset(&self, field: &str) -> Option<(f64, f64, f64)> {
        let prop = self
            .definition
            .find_all("property")
            .find(|p| p.args().first().and_then(Sexp::text) == Some(field))?;
        let at = prop.find("at")?.args();
        let coord = |i: usize| at.get(i)?.text()?.parse().ok();
        Some((coord(0)?, coord(1)?, coord(2)?))
    }
}

/// A parsed `.kicad_sym` file: its symbols, by name.
#[derive(Debug)]
struct Library {
    path: PathBuf,
    symbols: HashMap<String, Sexp>,
}

/// The installed KiCad symbol libraries and a project's own.
#[derive(Debug)]
pub struct SymbolLib {
    root: PathBuf,
    /// Set when `root` is a guess because no install was found (see [`SymbolLib::installed`]).
    no_install: Option<NotFound>,
    /// A project's libraries: name and `.kicad_sym` file.
    project: Vec<(String, PathBuf)>,
    /// Whether the symbols netsmith draws ([`crate::passives`]) answer before any library.
    generators: bool,
    libraries: Mutex<HashMap<String, Arc<Library>>>,
    symbols: Mutex<HashMap<String, Arc<Symbol>>>,
}

impl SymbolLib {
    /// The libraries in `root`, a folder of `<Library>.kicad_sym` files, and nothing else: the
    /// symbols netsmith draws itself ([`crate::passives`]) are not consulted (see
    /// [`SymbolLib::with_generators`]), which is what a test of the file reader, or of the
    /// generators against a copy of KiCad's libraries, wants.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            no_install: None,
            project: Vec::new(),
            generators: false,
            libraries: Mutex::default(),
            symbols: Mutex::default(),
        }
    }

    /// The symbols netsmith draws, then the installed KiCad libraries, found as [`install`]
    /// describes: `NETSMITH_KICAD_SYMBOLS`, then beside `kicad-cli`, then the usual install
    /// places. An id the generators make (`Device:R`; ADR 0097, step 2) is never read from a
    /// file, so a build gives the same symbol on every machine whatever KiCad has installed;
    /// every other id is read from a library.
    ///
    /// Finding none is not an error here: a design that uses only a project's own
    /// libraries needs no KiCad. [`SymbolLib::symbol`] fails, naming every place looked at,
    /// when a symbol of an installed library is asked for. [`SymbolLib::root`] is then the
    /// first place looked at.
    pub fn installed() -> Self {
        Self::installed_by(&install::Search::machine())
    }

    /// [`SymbolLib::installed`] over another machine, for tests.
    pub fn installed_by(search: &install::Search) -> Self {
        match search.find(install::Kind::Symbols) {
            Ok(found) => Self::at(found.dir),
            Err(lookup) => {
                let mut lib = Self::at(lookup.first_dir().unwrap_or(Path::new("")));
                lib.no_install = Some(lookup);
                lib
            }
        }
        .with_generators()
    }

    /// Consult the generators of [`crate::passives`] before the libraries: the id of a
    /// resistor, a capacitor, a diode, a FET and the other passive and discrete symbols of
    /// the standard mapping is answered with the symbol netsmith draws, unless a project
    /// library of the same name is listed (that library is read, as before).
    pub fn with_generators(mut self) -> Self {
        self.generators = true;
        self
    }

    /// Add a project's library: `name` is what `Name:Symbol` ids use.
    pub fn with_library(mut self, name: &str, file: impl Into<PathBuf>) -> Self {
        self.project.push((name.to_string(), file.into()));
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn library_path(&self, library: &str) -> Result<(PathBuf, bool), SymbolError> {
        let installed = self.root.join(format!("{library}.kicad_sym"));
        // With no install found `root` is a guess (possibly `""`, which makes `installed` a
        // path relative to the working directory): no library is installed then, so none is
        // shadowed.
        let is_installed = self.no_install.is_none() && installed.is_file();
        match self.project.iter().find(|(n, _)| n == library) {
            Some((_, file)) if is_installed => Err(SymbolError::Ambiguous {
                library: library.to_string(),
                installed,
                project: file.clone(),
            }),
            Some((_, file)) => Ok((file.clone(), true)),
            None => Ok((installed, false)),
        }
    }

    fn library(&self, library: &str) -> Result<Arc<Library>, SymbolError> {
        let mut loaded = self.libraries.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(lib) = loaded.get(library) {
            return Ok(lib.clone());
        }
        // netsmith's own library is in the binary, whatever is installed (ADR 0087).
        let own = library == crate::stdlib::NAME;
        let (path, project) = if own {
            (PathBuf::from(crate::stdlib::SYMBOL_FILE), false)
        } else {
            self.library_path(library)?
        };
        // With no install found there is no installed library: `root` is a guess, possibly `""`,
        // and a path under it may be relative to the working directory. It is never looked at.
        if let (false, false, Some(lookup)) = (own, project, &self.no_install) {
            return Err(SymbolError::NoInstall {
                library: library.to_string(),
                lookup: lookup.clone(),
            });
        }
        if !own && !path.is_file() {
            return Err(SymbolError::NoLibrary {
                library: library.to_string(),
                path,
                project,
            });
        }
        let text = if own {
            crate::stdlib::SYMBOLS.to_string()
        } else {
            std::fs::read_to_string(&path).map_err(|e| SymbolError::Unreadable {
                path: path.clone(),
                reason: e.to_string(),
            })?
        };
        let invalid = |reason: String| SymbolError::Invalid {
            name: format!("{library} ({})", path.display()),
            reason,
        };
        let root = Sexp::parse(&text).map_err(|e| invalid(e.to_string()))?;
        if root.head() != Some("kicad_symbol_lib") {
            return Err(invalid("this is not a `kicad_symbol_lib` file".to_string()));
        }
        let symbols = match root {
            Sexp::List(items) => items
                .into_iter()
                .filter(|i| i.head() == Some("symbol"))
                .filter_map(|i| Some((i.args().first()?.text()?.to_string(), i)))
                .collect(),
            _ => HashMap::new(),
        };
        let lib = Arc::new(Library { path, symbols });
        loaded.insert(library.to_string(), lib.clone());
        Ok(lib)
    }

    /// The symbol that draws `part`, mapped as `mapped`: the library's, or, for a row with no
    /// `symbol` (ADR 0097), a plain box drawn from the part's pin table, which needs no
    /// library at all ([`crate::generate`]).
    pub fn symbol_of(
        &self,
        part: &netsmith_core::Part,
        mapped: &crate::mapping::Mapped,
    ) -> Result<Arc<Symbol>, SymbolError> {
        match &mapped.lib_id {
            Some(lib_id) => self.symbol(lib_id),
            None => crate::generate::symbol(part, &mapped.footprint)
                .map(Arc::new)
                .ok_or_else(|| SymbolError::NoPinTable {
                    kind: part.kind.clone(),
                }),
        }
    }

    /// Read `Library:Name`. A library is parsed the first time any of its symbols is asked
    /// for, and a symbol is flattened the first time it is.
    pub fn symbol(&self, lib_id: &str) -> Result<Arc<Symbol>, SymbolError> {
        if let Some(symbol) = self
            .symbols
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(lib_id)
        {
            return Ok(symbol.clone());
        }
        let (library, name) = match lib_id.split_once(':') {
            Some((l, n)) if !l.is_empty() && !n.is_empty() && !n.contains(':') => (l, n),
            _ => return Err(SymbolError::BadName(lib_id.to_string())),
        };
        if self.generators
            && !self.project.iter().any(|(n, _)| n == library)
            && let Some(symbol) = crate::passives::symbol(lib_id)
        {
            let symbol = Arc::new(symbol);
            self.symbols
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(lib_id.to_string(), symbol.clone());
            return Ok(symbol);
        }
        let lib = self.library(library)?;
        if !lib.symbols.contains_key(name) {
            return Err(SymbolError::NoSymbol {
                name: lib_id.to_string(),
                library: library.to_string(),
                path: lib.path.clone(),
            });
        }
        let flat = flatten(&lib, name, 0).map_err(|reason| SymbolError::Invalid {
            name: lib_id.to_string(),
            reason,
        })?;
        let symbol =
            Arc::new(
                read_symbol(lib_id, name, flat).map_err(|reason| SymbolError::Invalid {
                    name: lib_id.to_string(),
                    reason,
                })?,
            );
        self.symbols
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(lib_id.to_string(), symbol.clone());
        Ok(symbol)
    }
}

/// The symbol `name` with its parents' contents merged in: the parent's drawing, pins and
/// settings, with the child's properties on top. The units are renamed for the child,
/// because KiCad wants a unit's name to start with its symbol's.
fn flatten(lib: &Library, name: &str, depth: u32) -> Result<Sexp, String> {
    let def = &lib.symbols[name];
    let Some(parent_name) = def
        .find("extends")
        .and_then(|e| e.args().first())
        .and_then(Sexp::text)
    else {
        return Ok(def.clone());
    };
    if depth >= 16 || !lib.symbols.contains_key(parent_name) {
        return Err(if depth >= 16 {
            "its `extends` chain is too deep".to_string()
        } else {
            format!("it extends `{parent_name}`, which the library does not hold")
        });
    }
    let parent = flatten(lib, parent_name, depth + 1)?;
    let own_units = def.find("symbol").is_some();
    let mut items: Vec<Sexp> = parent
        .args()
        .iter()
        .skip(1)
        .filter(|i| !(own_units && i.head() == Some("symbol")))
        .map(|i| rename_unit(i, parent_name, name))
        .collect();
    for item in def.args().iter().skip(1) {
        let head = item.head();
        if head == Some("extends") {
            continue;
        }
        let same = |other: &Sexp| {
            other.head() == head
                && (head != Some("property") || other.args().first() == item.args().first())
        };
        match items.iter().position(same) {
            Some(i) if head != Some("symbol") => items[i] = item.clone(),
            _ => items.push(item.clone()),
        }
    }
    let mut all = vec![Sexp::atom("symbol"), Sexp::string(name)];
    all.extend(items);
    Ok(Sexp::List(all))
}

/// `Parent_1_1` becomes `Child_1_1`.
fn rename_unit(item: &Sexp, parent: &str, child: &str) -> Sexp {
    if item.head() == Some("symbol")
        && let Some(unit) = item.args().first().and_then(Sexp::text)
        && let Some(rest) = unit.strip_prefix(parent)
        && rest.starts_with('_')
    {
        let mut all = vec![Sexp::atom("symbol"), Sexp::string(format!("{child}{rest}"))];
        all.extend(item.args().iter().skip(1).cloned());
        return Sexp::List(all);
    }
    item.clone()
}

fn num(s: Option<&Sexp>) -> Option<f64> {
    s?.text()?.parse().ok()
}

/// Turn a flattened library entry into a [`Symbol`] named `lib_id`.
fn read_symbol(lib_id: &str, name: &str, flat: Sexp) -> Result<Symbol, String> {
    let mut pins = Vec::new();
    let mut units = 1;
    for unit in flat.find_all("symbol") {
        let unit_name = unit.args().first().and_then(Sexp::text).unwrap_or("");
        let Some(rest) = unit_name.strip_prefix(name) else {
            return Err(format!(
                "its part `{unit_name}` does not start with the symbol's name"
            ));
        };
        let mut numbers = rest.rsplit('_');
        let style = numbers.next().and_then(|s| s.parse::<u32>().ok());
        let unit_no = numbers.next().and_then(|s| s.parse::<u32>().ok());
        let (Some(unit_no), Some(style)) = (unit_no, style) else {
            return Err(format!(
                "its part `{unit_name}` is not `{name}_<unit>_<style>`"
            ));
        };
        units = units.max(unit_no);
        // Style 2 redraws the same pins in the alternate (De Morgan) form.
        if style > 1 {
            continue;
        }
        for pin in unit.find_all("pin") {
            pins.push(read_pin(pin, unit_no)?);
        }
    }
    let mut all = vec![Sexp::atom("symbol"), Sexp::string(lib_id)];
    all.extend(flat.args().iter().skip(1).cloned());
    Ok(Symbol {
        lib_id: lib_id.to_string(),
        definition: Sexp::List(all),
        pins,
        units,
    })
}

fn read_pin(pin: &Sexp, unit: u32) -> Result<SymbolPin, String> {
    let kind_text = pin.args().first().and_then(Sexp::text).unwrap_or("");
    let text_of = |field: &str| {
        pin.find(field)
            .and_then(|f| f.args().first())
            .and_then(Sexp::text)
    };
    let number = text_of("number").ok_or("a pin has no number")?;
    let kind = PinType::parse(kind_text).ok_or_else(|| {
        format!("pin {number} has the electrical type `{kind_text}`, which is not one of KiCad's")
    })?;
    let at = pin
        .find("at")
        .map(Sexp::args)
        .ok_or_else(|| format!("pin {number} has no position"))?;
    let (Some(x), Some(y)) = (num(at.first()), num(at.get(1))) else {
        return Err(format!("pin {number} has no position"));
    };
    let hidden = pin
        .args()
        .iter()
        .any(|a| matches!(a, Sexp::Atom(s) if s == "hide"))
        || pin
            .find("hide")
            .is_some_and(|h| h.args().first().and_then(Sexp::text) == Some("yes"));
    Ok(SymbolPin {
        number: number.to_string(),
        name: text_of("name").unwrap_or("~").to_string(),
        kind,
        x,
        y,
        angle: num(at.get(2)).unwrap_or(0.0),
        length: num(pin.find("length").and_then(|l| l.args().first())).unwrap_or(0.0),
        hidden,
        unit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testlib::{self, TempDir};

    fn lib() -> (TempDir, SymbolLib) {
        let dir = TempDir::new("symbols");
        std::fs::write(dir.path().join("Device.kicad_sym"), testlib::device()).unwrap();
        std::fs::write(dir.path().join("acme.kicad_sym"), testlib::acme()).unwrap();
        let lib = SymbolLib::at(dir.path());
        (dir, lib)
    }

    #[test]
    fn resistor_pins_sit_above_and_below_the_origin() {
        let (_dir, lib) = lib();
        let r = lib.symbol("Device:R").unwrap();
        assert_eq!(r.pin_geometry("1").map(|p| (p.x, p.y)), Some((0.0, 3.81)));
        assert_eq!(r.pin_geometry("2").map(|p| (p.x, p.y)), Some((0.0, -3.81)));
        // The top pin's line runs down into the body.
        assert_eq!(r.pin_geometry("1").map(|p| p.angle), Some(270.0));
        let led = lib.symbol("Device:LED").unwrap();
        assert_eq!(
            led.pin_geometry("1").map(|p| (p.x, p.y)),
            Some((-3.81, 0.0))
        );
        assert_eq!(led.pin_geometry("1").map(|p| p.angle), Some(0.0));
    }

    #[test]
    fn the_definition_is_named_for_the_library_and_keeps_its_units_bare() {
        let (_dir, lib) = lib();
        let r = lib.symbol("Device:R").unwrap();
        assert_eq!(r.definition().args()[0].text(), Some("Device:R"));
        let unit = r.definition().find_all("symbol").next().unwrap();
        assert_eq!(unit.args()[0].text(), Some("R_0_1"));
    }

    #[test]
    fn reads_field_offsets_from_the_symbol() {
        let (_dir, lib) = lib();
        let r = lib.symbol("Device:R").unwrap();
        assert_eq!(r.field_offset("Reference"), Some((2.032, 0.0, 90.0)));
        assert_eq!(r.field_offset("Nope"), None);
    }

    #[test]
    fn reads_every_electrical_type() {
        let (_dir, lib) = lib();
        let s = lib.symbol("acme:Types").unwrap();
        let kinds: Vec<_> = s.pins().iter().map(|p| p.kind.name()).collect();
        assert_eq!(
            kinds,
            [
                "input",
                "output",
                "bidirectional",
                "tri_state",
                "passive",
                "free",
                "unspecified",
                "power_in",
                "power_out",
                "open_collector",
                "open_emitter",
                "no_connect"
            ]
        );
        for kind in kinds {
            assert_eq!(PinType::parse(kind).unwrap().name(), kind);
        }
        assert_eq!(PinType::parse("sideways"), None);
    }

    #[test]
    fn an_unknown_electrical_type_names_the_pin() {
        let dir = TempDir::new("badtype");
        std::fs::write(
            dir.path().join("L.kicad_sym"),
            "(kicad_symbol_lib (symbol \"S\" (symbol \"S_1_1\" \
             (pin sideways line (at 0 0 0) (length 1) (name \"A\") (number \"7\")))))",
        )
        .unwrap();
        let e = SymbolLib::at(dir.path()).symbol("L:S").unwrap_err();
        let text = e.to_string();
        assert!(text.contains("L:S") && text.contains("pin 7"), "{text}");
        assert!(text.contains("`sideways`"), "{text}");
    }

    #[test]
    fn a_symbol_that_extends_another_is_flattened() {
        let (_dir, lib) = lib();
        let parent = lib.symbol("acme:Reg").unwrap();
        let child = lib.symbol("acme:Reg33").unwrap();
        // The child has the parent's pins and drawing, with its own value.
        assert_eq!(child.pin_numbers(), parent.pin_numbers());
        assert_eq!(child.pin_numbers(), ["1", "2", "3"]);
        let value = |s: &Symbol| {
            s.definition()
                .find_all("property")
                .find(|p| p.args()[0].text() == Some("Value"))
                .unwrap()
                .args()[1]
                .text()
                .unwrap()
                .to_string()
        };
        assert_eq!(
            (value(&parent).as_str(), value(&child).as_str()),
            ("Reg", "Reg33")
        );
        // KiCad wants a unit's name to start with its symbol's.
        let units: Vec<_> = child
            .definition()
            .find_all("symbol")
            .map(|u| u.args()[0].text().unwrap())
            .collect();
        assert_eq!(units, ["Reg33_0_1", "Reg33_1_1"]);
        assert!(child.definition().find("extends").is_none());
        assert_eq!(child.definition().args()[0].text(), Some("acme:Reg33"));
    }

    #[test]
    fn units_keep_their_pins_and_shared_pins_belong_to_every_unit() {
        let (_dir, lib) = lib();
        let dual = lib.symbol("acme:Dual").unwrap();
        assert_eq!(dual.units(), 3);
        let of = |s: &Symbol, unit| -> Vec<String> {
            s.pins_of_unit(unit).map(|p| p.number.clone()).collect()
        };
        assert_eq!(of(&dual, 1), ["3", "2", "1"]);
        assert_eq!(of(&dual, 2), ["5", "6", "7"]);
        assert_eq!(of(&dual, 3), ["8", "4"]);
        assert_eq!(dual.pin("5").unwrap().unit, 2);
        // A pin drawn in unit 0 is in every unit.
        let shared = lib.symbol("acme:Shared").unwrap();
        assert_eq!(shared.units(), 2);
        assert_eq!(shared.pin("9").unwrap().unit, 0);
        assert_eq!(of(&shared, 1), ["9", "1", "2"]);
        assert_eq!(of(&shared, 2), ["9", "3", "4"]);
    }

    #[test]
    fn hidden_stacked_pins_are_ordinary_pins() {
        let (_dir, lib) = lib();
        let s = lib.symbol("acme:Reg").unwrap();
        let hidden: Vec<_> = s.pins().iter().filter(|p| p.hidden).collect();
        assert_eq!(hidden.len(), 0);
        let s = lib.symbol("acme:Stacked").unwrap();
        let out: Vec<_> = s.pins().iter().filter(|p| p.name == "OUT").collect();
        assert_eq!(out.len(), 3);
        assert_eq!(out.iter().filter(|p| p.hidden).count(), 2);
        assert!(out.iter().all(|p| (p.x, p.y) == (out[0].x, out[0].y)));
        assert_eq!(s.pin_type("2"), Some(PinType::Passive));
    }

    #[test]
    fn only_the_first_body_style_lists_pins() {
        let (_dir, lib) = lib();
        let s = lib.symbol("acme:Gate").unwrap();
        assert_eq!(s.pin_numbers(), ["1", "2", "3"]);
    }

    #[test]
    fn lists_pin_numbers_and_types() {
        let (_dir, lib) = lib();
        let c = lib.symbol("Device:C").unwrap();
        assert_eq!(c.pin_numbers(), ["1", "2"]);
        assert_eq!(c.pin_type("1"), Some(PinType::Passive));
        assert_eq!(c.pin_type("9"), None);
    }

    #[test]
    fn a_missing_library_names_the_path_and_the_variable() {
        let (dir, lib) = lib();
        let text = lib.symbol("Nowhere:X").unwrap_err().to_string();
        assert!(
            text.contains("symbol library `Nowhere` not found"),
            "{text}"
        );
        assert!(text.contains("Nowhere.kicad_sym"), "{text}");
        assert!(text.contains(&dir.path().display().to_string()), "{text}");
        assert!(text.contains(ROOT_VARIABLE), "{text}");
    }

    #[test]
    fn a_missing_symbol_names_the_library_it_is_missing_from() {
        let (_dir, lib) = lib();
        let text = lib.symbol("Device:Nope").unwrap_err().to_string();
        assert!(text.contains("symbol `Device:Nope` not found"), "{text}");
        assert!(text.contains("library `Device`"), "{text}");
        assert!(text.contains("Device.kicad_sym"), "{text}");
    }

    #[test]
    fn names_need_a_library() {
        let (_dir, lib) = lib();
        assert_eq!(
            lib.symbol("R").unwrap_err(),
            SymbolError::BadName("R".into())
        );
    }

    #[test]
    fn a_project_library_is_found_by_its_name() {
        let dir = TempDir::new("project");
        let file = dir.path().join("mine.kicad_sym");
        std::fs::write(&file, testlib::acme()).unwrap();
        let lib = SymbolLib::at("/nonexistent/symbols").with_library("proj", &file);
        assert_eq!(lib.symbol("proj:Reg").unwrap().pin_numbers().len(), 3);
        // A project library that is not there says where kicad.toml pointed.
        let lib =
            SymbolLib::at("/nonexistent/symbols").with_library("gone", "/nonexistent/g.kicad_sym");
        let text = lib.symbol("gone:X").unwrap_err().to_string();
        assert!(
            text.contains("kicad.toml lists /nonexistent/g.kicad_sym"),
            "{text}"
        );
    }

    #[test]
    fn no_install_is_an_error_only_when_an_installed_symbol_is_asked_for() {
        let empty = TempDir::new("no-kicad");
        let lib = SymbolLib::installed_by(&install::Search::under(empty.path()));
        // A project's own library needs no install.
        let dir = TempDir::new("project-only");
        let file = dir.path().join("mine.kicad_sym");
        std::fs::write(&file, testlib::acme()).unwrap();
        let lib = lib.with_library("proj", &file);
        assert_eq!(lib.symbol("proj:Reg").unwrap().pin_numbers().len(), 3);
        // A stock library says what was looked for, every place, and the variables.
        let text = lib.symbol("Device:C_Polarized").unwrap_err().to_string();
        assert!(text.contains("symbol library `Device` not found"), "{text}");
        assert!(text.contains("no KiCad symbol libraries found"), "{text}");
        assert!(text.contains("usr/share/kicad/symbols"), "{text}");
        assert!(text.contains("mnt/c/Program Files/KiCad/8.0"), "{text}");
        assert!(text.contains(install::SYMBOLS_VARIABLE), "{text}");
        assert!(text.contains(install::FOOTPRINTS_VARIABLE), "{text}");
        assert!(text.contains("kicad.toml"), "{text}");
    }

    #[test]
    fn an_install_found_by_the_search_is_read() {
        let root = TempDir::new("fake-linux");
        let symbols = root.path().join("usr/share/kicad/symbols");
        std::fs::create_dir_all(&symbols).unwrap();
        std::fs::write(symbols.join("Device.kicad_sym"), testlib::device()).unwrap();
        let lib = SymbolLib::installed_by(&install::Search::under(root.path()));
        assert_eq!(lib.root(), symbols);
        // A symbol the generators do not make is read from the install.
        assert!(lib.symbol("Device:LED").is_ok());
    }

    #[test]
    fn a_generated_id_is_answered_with_no_install_and_before_an_installed_library() {
        let empty = TempDir::new("no-kicad-generated");
        let lib = SymbolLib::installed_by(&install::Search::under(empty.path()));
        assert_eq!(lib.symbol("Device:R").unwrap().pin_numbers(), ["1", "2"]);
        // Even where a library of the same id is installed, the generated one is answered, so
        // two machines build the same board.
        let root = TempDir::new("fake-linux-generated");
        let symbols = root.path().join("usr/share/kicad/symbols");
        std::fs::create_dir_all(&symbols).unwrap();
        std::fs::write(symbols.join("Device.kicad_sym"), testlib::device()).unwrap();
        let lib = SymbolLib::installed_by(&install::Search::under(root.path()));
        let drawn = lib.symbol("Device:C").unwrap();
        assert_eq!(drawn.lib_id(), "netsmith:C");
        assert_eq!(drawn.pin("1").unwrap().length, 2.794);
        // `at` is the folder alone.
        let read = SymbolLib::at(&symbols).symbol("Device:C").unwrap();
        assert_eq!(read.pin("1").unwrap().length, 1.27);
        // A project's library named like a generated one is read, not shadowed.
        let file = root.path().join("device.kicad_sym");
        std::fs::write(&file, testlib::acme()).unwrap();
        let lib = SymbolLib::at("/nonexistent/symbols")
            .with_generators()
            .with_library("Device", &file);
        let text = lib.symbol("Device:R").unwrap_err().to_string();
        assert!(text.contains("not found in the library `Device`"), "{text}");
        assert!(lib.symbol("Device:Reg").is_ok());
    }

    #[test]
    fn a_project_library_cannot_shadow_an_installed_one() {
        let (dir, _) = lib();
        let file = dir.path().join("elsewhere.kicad_sym");
        std::fs::write(&file, testlib::acme()).unwrap();
        let lib = SymbolLib::at(dir.path()).with_library("Device", &file);
        let text = lib.symbol("Device:R").unwrap_err().to_string();
        assert!(text.contains("both installed"), "{text}");
        assert!(text.contains("elsewhere.kicad_sym"), "{text}");
    }

    #[test]
    fn a_project_library_is_not_shadowing_a_file_of_the_guessed_root_when_no_kicad_is_found() {
        // With no install the root is a guess: here a folder that has a `Device.kicad_sym`, as
        // the working directory does when the root is `""`. It is no installed library.
        let (dir, _) = lib();
        let file = dir.path().join("elsewhere.kicad_sym");
        std::fs::write(&file, testlib::acme()).unwrap();
        let mut lib = SymbolLib::at(dir.path()).with_library("Device", &file);
        lib.no_install = Some(
            install::Search::under(dir.path())
                .with_no_kicad()
                .find(install::Kind::Symbols)
                .unwrap_err(),
        );
        let (path, project) = lib.library_path("Device").unwrap();
        assert_eq!((path, project), (file, true));
    }

    #[test]
    fn a_file_that_is_not_a_symbol_library_is_refused() {
        let dir = TempDir::new("junk");
        std::fs::write(dir.path().join("J.kicad_sym"), "(footprint \"X\")").unwrap();
        let text = SymbolLib::at(dir.path())
            .symbol("J:X")
            .unwrap_err()
            .to_string();
        assert!(text.contains("not a `kicad_symbol_lib` file"), "{text}");
    }

    #[test]
    fn the_stock_libraries_flatten_and_load_when_kicad_is_installed() {
        let lib = SymbolLib::installed();
        if !lib.root().is_dir() {
            eprintln!("skipped: no KiCad symbols at {}", lib.root().display());
            return;
        }
        // `AP1117-18` extends `AP1117-15`; both have the same three pins.
        let a = lib.symbol("Regulator_Linear:AP1117-18").unwrap();
        let b = lib.symbol("Regulator_Linear:AP1117-15").unwrap();
        assert_eq!(a.pin_numbers(), b.pin_numbers());
        assert_eq!(a.pin_type("2"), Some(PinType::PowerOut));
        assert!(lib.symbol("power:PWR_FLAG").is_ok());
    }
}
