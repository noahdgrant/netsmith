//! What KiCad calls the library's parts: which symbol draws them and which footprint
//! carries them. This is the only place KiCad's names appear. The library says
//! "Resistor" and "0402"; the mapping says `Device:R` and `Resistor_SMD:R_0402_1005Metric`.
//!
//! The mapping is TOML, keyed by (part kind, package). `std/kicad.toml` ships in the
//! binary and holds the standard library's parts; a project adds a `kicad.toml` at its
//! root for its own parts and for its own symbol and footprint libraries (ADR 0016,
//! ADR 0021).
//!
//! # The mapping and the project libraries (ADR 0021, accepted by the owner on 2026-11-04)
//!
//! `std/kicad.toml` is compiled into the binary and has a `[[part]]` row for each (kind,
//! package): `symbol`, `footprint`, optionally `default = true` (the row used when the part has no
//! package) and a `[part.pin_names]` table for pins whose symbol name differs from the datasheet's.
//! The Rust `ENTRIES` table is gone, and so is the `through_hole` flag: whether a footprint is
//! through hole is read from the footprint file's `attr`. A (kind, package) pair in both the
//! standard file and a project's, or twice in one, is an error that names both files. A footprint
//! pad of type `connect` (the Tag-Connect footprints) is read as a surface pad.
//!
//! **Project libraries are supported, and listed one way.** A real design keeps its ICs in a
//! library of its own. The project's `kicad.toml` lists them: `[[library]] name = "acme"`,
//! `symbols = "libs/acme.kicad_sym"`, `footprints = "libs/acme.pretty"`, paths relative to the
//! file, `${NAME}` expanded from the environment and nothing else. The mapping then writes
//! `symbol = "acme:Part"` and `footprint = "acme:Fp"`. KiCad's own `sym-lib-table` and
//! `fp-lib-table` are not read as a second way to configure: `netsmith kicad-libs import
//! <sym-lib-table> [<fp-lib-table>]` prints the `[[library]]` stanzas to paste, which makes porting
//! a KiCad project easy and is a generator and not a configuration. When it writes a board,
//! `build` (and `fab`, and the editor's build and fab) writes `sym-lib-table` and `fp-lib-table`
//! beside the `.kicad_pro`, with `${KIPRJMOD}`-relative paths, so that KiCad finds the project's
//! libraries when it opens the board.
//!
//! **The standard rows** use stock KiCad 9 names, read from the installed libraries and not
//! guessed. Validation confirmed or corrected the pad numbers ADR 0023 had written from memory:
//! the Type-C pads A1, A4, A5, A6, A7, A9, A12, B1, B4, B5, B6, B9, B12 and S1 are right for the
//! sixteen-contact receptacle, which also has the sideband pads A8 and B8 (the open pins `sbu1` and
//! `sbu2` of `UsbC2`); the six-contact power-only receptacle has no A1, A4, B1 or B4, so
//! `UsbCPower` has VBUS on A9 and B9 and GND on A12 and B12.
//!
//! **`check`, `solve`, `compare` and the editor's reading read no KiCad file; `build` and `fab`
//! read one only for an id netsmith cannot answer** (ADR 0097). `outcome_with` takes
//! `Option<&Libraries>`; with `None` no KiCad file is read, and the board's `schematic`, `project`
//! and `report` are empty. netsmith answers the ids of its own mapping itself, so `build`, `fab`
//! and the editor's build and fab stop, once, with the path and the variable, only when a KiCad id
//! of a project's `kicad.toml` is in no `[[library]]` and no generator makes it, and no installed
//! library has it. The editor draws a box where a symbol is missing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use toml::Spanned;

use crate::footprint_spec::{self, FootprintSpec};
use crate::sexp::Sexp;

/// The symbol that tells the electrical rules check a net is supplied from outside the
/// board. It maps no part: the emitter places it on nets that need it.
pub const POWER_FLAG_SYMBOL: &str = "power:PWR_FLAG";

/// A project's mapping file, at the project root.
pub const PROJECT_FILE: &str = "kicad.toml";

const STANDARD: &str = include_str!("../../../std/kicad.toml");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapError {
    /// No symbol or footprint is known for this kind of part.
    UnknownPart(String),
    UnknownPackage {
        kind: String,
        package: String,
    },
    /// The part was never given a package and this kind has no default.
    NeedsPackage(String),
    /// A mapping file is not valid; the text says which and why.
    Invalid(String),
}

impl std::fmt::Display for MapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPart(kind) => write!(f, "there is no KiCad symbol for a `{kind}` yet"),
            Self::UnknownPackage { kind, package } => {
                write!(
                    f,
                    "there is no KiCad footprint for a `{kind}` in package `{package}`"
                )
            }
            Self::NeedsPackage(kind) => {
                write!(
                    f,
                    "this `{kind}` has no package, and KiCad needs a footprint"
                )
            }
            Self::Invalid(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for MapError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapped {
    /// `Device:R`; `None` for a row with no `symbol`, whose part is drawn from its pin table
    /// ([`crate::generate`], ADR 0097).
    pub lib_id: Option<String>,
    /// `Resistor_SMD:R_0402_1005Metric`; for a row that asks for a generated footprint, its id
    /// (`gen:QFN_body7x7mm_...`, [`crate::footprint_spec`]).
    pub footprint: String,
    /// The file and line of the row's `footprint`, for messages: `kicad.toml line 12`.
    pub origin: String,
    /// Where KiCad's symbol names a pin differently from the datasheet: the datasheet's
    /// name and the symbol's.
    pub pin_names: BTreeMap<String, String>,
}

/// A project's own symbol and footprint libraries, listed in its `kicad.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectLibrary {
    /// What `Name:Symbol` and `Name:Footprint` ids call it.
    pub name: String,
    /// The `.kicad_sym` file, if the library has symbols.
    pub symbols: Option<PathBuf>,
    /// The `.pretty` folder, if the library has footprints.
    pub footprints: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: String,
    pub package: String,
    /// Used when the part was never given a package.
    pub default: bool,
    /// The KiCad symbol that draws the part; `None` when the row has none and the part is
    /// drawn from its pin table (ADR 0097).
    pub symbol: Option<String>,
    /// A `Library:Name` id, or, for a row that wrote `gen:QFN(...)`, the id of that
    /// generated footprint ([`FootprintSpec::id`]).
    pub footprint: String,
    pub pin_names: BTreeMap<String, String>,
    /// The file the row is from, for messages.
    pub source: String,
    /// The line of the row's `footprint` in `source`, 1-based.
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Mapping {
    rows: Vec<Row>,
    libraries: Vec<ProjectLibrary>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    part: Vec<RawPart>,
    #[serde(default)]
    library: Vec<RawLibrary>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPart {
    kind: String,
    package: String,
    #[serde(default)]
    default: bool,
    symbol: Option<String>,
    footprint: Spanned<String>,
    #[serde(default)]
    pin_names: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLibrary {
    name: String,
    symbols: Option<String>,
    footprints: Option<String>,
}

/// `Library:Name`
fn is_id(text: &str) -> bool {
    matches!(text.split_once(':'), Some((l, n)) if !l.is_empty() && !n.is_empty() && !n.contains(':'))
}

/// Expand `${NAME}` from the environment. Nothing else is expanded.
fn expand(text: &str, what: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after
            .find('}')
            .ok_or_else(|| format!("{what}: `${{` is never closed in {text:?}"))?;
        let name = &after[..end];
        let value = std::env::var(name)
            .map_err(|_| format!("{what}: ${{{name}}} in {text:?} is not set"))?;
        out.push_str(&value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

impl Mapping {
    /// The standard library's parts, from `std/kicad.toml`.
    pub fn standard() -> Mapping {
        Mapping::parse(STANDARD, "std/kicad.toml", Path::new("."))
            .expect("std/kicad.toml is valid (a test reads it)")
    }

    /// Read mapping text. `source` names the file in messages; library paths are relative
    /// to `base`, the folder the file is in.
    pub fn parse(text: &str, source: &str, base: &Path) -> Result<Mapping, MapError> {
        let invalid = |why: String| MapError::Invalid(format!("{source}: {why}"));
        let file: File = toml::from_str(text).map_err(|e| invalid(e.to_string()))?;
        let mut mapping = Mapping::default();
        for p in file.part {
            let at = format!("part `{}` in package `{}`", p.kind, p.package);
            if p.kind.is_empty() || p.package.is_empty() {
                return Err(invalid(format!("{at}: kind and package must not be empty")));
            }
            let line = text[..p.footprint.span().start].matches('\n').count() + 1;
            let footprint = p.footprint.into_inner();
            if let Some(symbol) = &p.symbol
                && !is_id(symbol)
            {
                return Err(invalid(format!(
                    "{at}: symbol {symbol:?} is not `Library:Name`"
                )));
            }
            // A footprint is an id, or a generator and its parameters (`gen:QFN(...)`, ADR 0104),
            // which the rest of netsmith knows by the id it spells.
            let footprint = if footprint_spec::is_spelling(&footprint) {
                FootprintSpec::parse(&footprint)
                    .map_err(|why| {
                        MapError::Invalid(format!(
                            "{source}:{line}: {at}: footprint {footprint:?}: {why}"
                        ))
                    })?
                    .id()
            } else if is_id(&footprint) {
                footprint
            } else {
                return Err(invalid(format!(
                    "{at}: footprint {footprint:?} is not `Library:Name` or `gen:Generator(key=value, ...)`"
                )));
            };
            if p.symbol.is_none() && !p.pin_names.is_empty() {
                return Err(invalid(format!(
                    "{at}: `pin_names` rename pins of a library symbol, and the row has no `symbol`: \
                     a symbol drawn from the pin table uses the datasheet's names"
                )));
            }
            if mapping
                .rows
                .iter()
                .any(|r| r.kind == p.kind && r.package == p.package)
            {
                return Err(invalid(format!("{at} is mapped twice")));
            }
            if p.default
                && let Some(other) = mapping.rows.iter().find(|r| r.kind == p.kind && r.default)
            {
                return Err(invalid(format!(
                    "{at} is marked `default`, but `{}` already is the default for `{}`",
                    other.package, p.kind
                )));
            }
            mapping.rows.push(Row {
                kind: p.kind,
                package: p.package,
                default: p.default,
                symbol: p.symbol,
                footprint,
                pin_names: p.pin_names,
                source: source.to_string(),
                line,
            });
        }
        for l in file.library {
            let at = format!("library `{}`", l.name);
            if l.name.is_empty() || l.name.contains(':') {
                return Err(invalid(format!(
                    "{at}: a library name must not be empty or contain `:`"
                )));
            }
            if l.name == footprint_spec::LIBRARY {
                return Err(invalid(format!(
                    "{at}: `{}` is the library of generated footprints (`gen:QFN(...)`, ADR 0104); give the library another name",
                    l.name
                )));
            }
            if l.name == crate::stdlib::NAME {
                return Err(invalid(format!(
                    "{at}: `{}` is the name of the standard library's own footprints and symbols (`std/libs/`, ADR 0087); give the library another name",
                    l.name
                )));
            }
            if mapping.libraries.iter().any(|o| o.name == l.name) {
                return Err(invalid(format!("{at} is listed twice")));
            }
            if l.symbols.is_none() && l.footprints.is_none() {
                return Err(invalid(format!(
                    "{at} lists no `symbols` file and no `footprints` folder"
                )));
            }
            let path = |value: Option<String>, key: &str| -> Result<Option<PathBuf>, MapError> {
                value
                    .map(|v| {
                        let v = expand(&v, &format!("{source}: {at}: {key}"))
                            .map_err(MapError::Invalid)?;
                        let path = PathBuf::from(v);
                        Ok(if path.is_absolute() {
                            path
                        } else {
                            base.join(path)
                        })
                    })
                    .transpose()
            };
            mapping.libraries.push(ProjectLibrary {
                name: l.name,
                symbols: path(l.symbols, "symbols")?,
                footprints: path(l.footprints, "footprints")?,
            });
        }
        Ok(mapping)
    }

    /// Add `other`'s parts and libraries. A (kind, package) pair or a library name that
    /// both have is an error, not an override.
    pub fn merged(mut self, other: Mapping) -> Result<Mapping, MapError> {
        for row in other.rows {
            if let Some(first) = self
                .rows
                .iter()
                .find(|r| r.kind == row.kind && r.package == row.package)
            {
                return Err(MapError::Invalid(format!(
                    "{}: part `{}` in package `{}` is already mapped by {}; a (part, package) pair has one answer",
                    row.source, row.kind, row.package, first.source
                )));
            }
            if row.default
                && let Some(first) = self.rows.iter().find(|r| r.kind == row.kind && r.default)
            {
                return Err(MapError::Invalid(format!(
                    "{}: part `{}` marks package `{}` as its default, but {} already marks `{}`",
                    row.source, row.kind, row.package, first.source, first.package
                )));
            }
            self.rows.push(row);
        }
        for lib in other.libraries {
            if self.libraries.iter().any(|l| l.name == lib.name) {
                return Err(MapError::Invalid(format!(
                    "library `{}` is listed twice",
                    lib.name
                )));
            }
            self.libraries.push(lib);
        }
        Ok(self)
    }

    /// The standard mapping and the project's `kicad.toml`, if the folder `project` has one.
    pub fn for_project(project: Option<&Path>) -> Result<Mapping, MapError> {
        let standard = Mapping::standard();
        let Some(dir) = project else {
            return Ok(standard);
        };
        let file = dir.join(PROJECT_FILE);
        if !file.is_file() {
            return Ok(standard);
        }
        let text = std::fs::read_to_string(&file)
            .map_err(|e| MapError::Invalid(format!("{}: {e}", file.display())))?;
        standard.merged(Mapping::parse(&text, PROJECT_FILE, dir)?)
    }

    /// The project's own libraries.
    pub fn libraries(&self) -> &[ProjectLibrary] {
        &self.libraries
    }

    /// Every row, in file order.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The symbol and footprint of a kind of part in a package. A part never given a
    /// package takes the kind's `default` row.
    pub fn map(&self, kind: &str, package: Option<&str>) -> Result<Mapped, MapError> {
        let mut of_kind = self.rows.iter().filter(|r| r.kind == kind).peekable();
        if of_kind.peek().is_none() {
            return Err(MapError::UnknownPart(kind.to_string()));
        }
        let row =
            match package {
                Some(package) => of_kind.find(|r| r.package == package).ok_or_else(|| {
                    MapError::UnknownPackage {
                        kind: kind.to_string(),
                        package: package.to_string(),
                    }
                })?,
                None => of_kind
                    .find(|r| r.default)
                    .ok_or_else(|| MapError::NeedsPackage(kind.to_string()))?,
            };
        Ok(Mapped {
            lib_id: row.symbol.clone(),
            footprint: row.footprint.clone(),
            origin: format!("{} line {}", row.source, row.line),
            pin_names: row.pin_names.clone(),
        })
    }
}

/// KiCad's `sym-lib-table` and `fp-lib-table` for a project's own libraries, to write
/// beside a `.kicad_pro` in `dir`, so that KiCad opens the project with the libraries its
/// symbols and footprints come from. The paths are relative to `dir` (`${KIPRJMOD}` is the
/// project folder), so the project moves with its libraries. Empty when there are none.
///
/// With `with_std` the standard library's own files (`crate::stdlib`, which the caller writes
/// into `dir` with [`crate::stdlib::files`]) are listed too, as `netsmith`, first: a board
/// that uses one of its footprints or symbols has it in the project's library table like any
/// other library.
pub fn library_tables(
    libraries: &[ProjectLibrary],
    dir: &Path,
    with_std: bool,
) -> Vec<(&'static str, String)> {
    let table = |head: &str, uris: Vec<(&str, String)>| {
        let mut out = format!("({head}\n\t(version 7)\n");
        for (name, uri) in uris {
            out.push_str(&format!(
                "\t(lib (name {name:?})(type \"KiCad\")(uri \"${{KIPRJMOD}}/{uri}\")(options \"\")(descr \"\"))\n"
            ));
        }
        out.push_str(")\n");
        out
    };
    let relative = |path: &Path| relative_to(path, dir).replace('\\', "/");
    let mut symbols: Vec<(&str, String)> = Vec::new();
    let mut footprints: Vec<(&str, String)> = Vec::new();
    if with_std {
        symbols.push((
            crate::stdlib::NAME,
            format!("{}.kicad_sym", crate::stdlib::NAME),
        ));
        footprints.push((
            crate::stdlib::NAME,
            crate::stdlib::FOOTPRINT_FOLDER.to_string(),
        ));
    }
    symbols.extend(
        libraries
            .iter()
            .filter_map(|l| Some((l.name.as_str(), relative(l.symbols.as_deref()?)))),
    );
    footprints.extend(
        libraries
            .iter()
            .filter_map(|l| Some((l.name.as_str(), relative(l.footprints.as_deref()?)))),
    );
    let mut tables = Vec::new();
    if !symbols.is_empty() {
        tables.push(("sym-lib-table", table("sym_lib_table", symbols)));
    }
    if !footprints.is_empty() {
        tables.push(("fp-lib-table", table("fp_lib_table", footprints)));
    }
    tables
}

/// `path` as seen from the folder `from`: `../../libs/a.kicad_sym`. Both are made absolute
/// first; with nothing in common the absolute path is kept.
fn relative_to(path: &Path, from: &Path) -> String {
    let absolute = |p: &Path| {
        std::fs::canonicalize(p)
            .unwrap_or_else(|_| std::path::absolute(p).unwrap_or(p.to_path_buf()))
    };
    let (path, from) = (absolute(path), absolute(from));
    let (a, b): (Vec<_>, Vec<_>) = (path.components().collect(), from.components().collect());
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    if common == 0 {
        return path.display().to_string();
    }
    let mut parts: Vec<String> = vec!["..".to_string(); b.len() - common];
    parts.extend(
        a[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    parts.join("/")
}

/// The `[[library]]` stanzas for the libraries in KiCad's `sym-lib-table` and
/// `fp-lib-table`, to paste into a project's `kicad.toml`. A generator for porting a
/// KiCad project, not another way to configure: the table is read once, here.
/// `${KIPRJMOD}` (the project folder) becomes a path relative to `kicad.toml`; libraries
/// at `${KICAD…}` are KiCad's own installed ones, which are read without being listed.
pub fn library_stanzas(sym_table: Option<&str>, fp_table: Option<&str>) -> Result<String, String> {
    let mut libraries: Vec<(String, Option<String>, Option<String>)> = Vec::new();
    for (table, is_symbols) in [(sym_table, true), (fp_table, false)] {
        let Some(text) = table else { continue };
        let root = Sexp::parse(text).map_err(|e| e.to_string())?;
        for lib in root.find_all("lib") {
            let field = |name: &str| {
                lib.find(name)
                    .and_then(|f| f.args().first())
                    .and_then(Sexp::text)
            };
            let (Some(name), Some(uri)) = (field("name"), field("uri")) else {
                return Err("a `lib` entry has no name or uri".to_string());
            };
            // KiCad's own libraries, and the standard library's own that a netsmith build
            // writes beside the board, are read without being listed.
            if uri.starts_with("${KICAD") || name == crate::stdlib::NAME {
                continue;
            }
            let uri = uri
                .strip_prefix("${KIPRJMOD}/")
                .or_else(|| uri.strip_prefix("${KIPRJMOD}\\"))
                .unwrap_or(uri)
                .replace('\\', "/");
            let slot = match libraries.iter().position(|l| l.0 == name) {
                Some(i) => i,
                None => {
                    libraries.push((name.to_string(), None, None));
                    libraries.len() - 1
                }
            };
            if is_symbols {
                libraries[slot].1 = Some(uri);
            } else {
                libraries[slot].2 = Some(uri);
            }
        }
    }
    let mut out = String::new();
    for (name, symbols, footprints) in libraries {
        out.push_str(&format!("[[library]]\nname = {name:?}\n"));
        if let Some(s) = symbols {
            out.push_str(&format!("symbols = {s:?}\n"));
        }
        if let Some(f) = footprints {
            out.push_str(&format!("footprints = {f:?}\n"));
        }
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standard() -> Mapping {
        Mapping::standard()
    }

    #[test]
    fn maps_each_kind_and_package() {
        let m = standard();
        let r = m.map("Resistor", Some("0402")).unwrap();
        assert_eq!(
            (r.lib_id.as_deref(), r.footprint.as_str()),
            (Some("Device:R"), "Resistor_SMD:R_0402_1005Metric")
        );
        let c = m.map("Capacitor", Some("0805")).unwrap();
        assert_eq!(c.footprint, "Capacitor_SMD:C_0805_2012Metric");
    }

    #[test]
    fn a_row_may_leave_out_the_symbol_and_then_the_part_is_drawn_from_its_pin_table() {
        let text = "[[part]]\nkind = \"Chip\"\npackage = \"P\"\nfootprint = \"a:Chip\"\n";
        let m = Mapping::parse(text, "kicad.toml", Path::new(".")).unwrap();
        let mapped = m.map("Chip", Some("P")).unwrap();
        assert_eq!(mapped.lib_id, None);
        assert_eq!(mapped.footprint, "a:Chip");
        assert_eq!(m.rows()[0].symbol, None);
        // A symbol that is named still has to be `Library:Name`.
        let bad = Mapping::parse(
            "[[part]]\nkind = \"Chip\"\npackage = \"P\"\nsymbol = \"Chip\"\nfootprint = \"a:Chip\"\n",
            "kicad.toml",
            Path::new("."),
        )
        .unwrap_err()
        .to_string();
        assert!(
            bad.contains("symbol \"Chip\" is not `Library:Name`"),
            "{bad}"
        );
    }

    #[test]
    fn pin_names_need_a_symbol_to_rename_pins_of() {
        let e = Mapping::parse(
            "[[part]]\nkind = \"Chip\"\npackage = \"P\"\nfootprint = \"a:Chip\"\n\n[part.pin_names]\nIN = \"VI\"\n",
            "kicad.toml",
            Path::new("."),
        )
        .unwrap_err()
        .to_string();
        assert!(
            e.contains("part `Chip` in package `P`")
                && e.contains("`pin_names` rename pins of a library symbol")
                && e.contains("no `symbol`"),
            "{e}"
        );
    }

    #[test]
    fn a_linear_regulator_maps_in_sot23_and_sot223() {
        let m = standard();
        let u = m.map("LinearRegulator", Some("SOT-23")).unwrap();
        // The regulator names no symbol: it is drawn from its pin table (ADR 0097).
        assert_eq!(
            (u.lib_id.as_deref(), u.footprint.as_str()),
            (None, "Package_TO_SOT_SMD:SOT-23")
        );
        // The ICs that name no symbol are drawn from their pin tables (ADR 0097).
        assert_eq!(m.map("Lt8641", Some("UDC20(18)")).unwrap().lib_id, None);
        assert_eq!(
            m.map("LinearRegulator", Some("SOT-223")).unwrap().footprint,
            "Package_TO_SOT_SMD:SOT-223-3_TabPin2"
        );
    }

    #[test]
    fn an_led_and_a_header_default_but_a_resistor_must_be_given_a_package() {
        let m = standard();
        assert_eq!(
            m.map("Led", None).unwrap().footprint,
            "LED_SMD:LED_0603_1608Metric"
        );
        assert_eq!(
            m.map("Header1x2", None).unwrap().footprint,
            "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical"
        );
        assert_eq!(
            m.map("Resistor", None),
            Err(MapError::NeedsPackage("Resistor".into()))
        );
    }

    #[test]
    fn says_what_is_missing() {
        let m = standard();
        assert_eq!(
            m.map("Nope", None),
            Err(MapError::UnknownPart("Nope".into()))
        );
        assert!(matches!(
            m.map("Resistor", Some("2512")),
            Err(MapError::UnknownPackage { .. })
        ));
    }

    #[test]
    fn the_standard_mapping_has_the_original_kinds_and_the_new_ones() {
        let m = standard();
        let mut kinds: Vec<&str> = m.rows().iter().map(|r| r.kind.as_str()).collect();
        kinds.dedup();
        assert_eq!(
            kinds[..4],
            ["Resistor", "Capacitor", "LinearRegulator", "Led"]
        );
        for kind in [
            "Header1x2",
            "Diode",
            "NMosfet",
            "UsbC2",
            "TestPoint",
            "Fiducial",
        ] {
            assert!(kinds.contains(&kind), "{kind}");
        }
        assert!(m.libraries().is_empty());
    }

    const PROJECT: &str = r#"
[[library]]
name = "acme"
symbols = "libs/acme.kicad_sym"
footprints = "libs/acme.pretty"

[[part]]
kind = "Ldo9"
package = "DFN-8-EP"
symbol = "acme:LDO9"
footprint = "acme:DFN-8-1EP"

[part.pin_names]
SENSE = "SNS"
"#;

    #[test]
    fn a_project_adds_parts_and_libraries_relative_to_its_file() {
        let project = Mapping::parse(PROJECT, "kicad.toml", Path::new("/work/proj")).unwrap();
        let m = standard().merged(project).unwrap();
        let u = m.map("Ldo9", Some("DFN-8-EP")).unwrap();
        assert_eq!(u.lib_id.as_deref(), Some("acme:LDO9"));
        assert_eq!(u.pin_names["SENSE"], "SNS");
        assert_eq!(
            m.libraries(),
            [ProjectLibrary {
                name: "acme".into(),
                symbols: Some("/work/proj/libs/acme.kicad_sym".into()),
                footprints: Some("/work/proj/libs/acme.pretty".into()),
            }]
        );
        // The standard parts are still there.
        assert!(m.map("Resistor", Some("0402")).is_ok());
    }

    #[test]
    fn a_pair_in_both_files_is_an_error_not_an_override() {
        let project = Mapping::parse(
            "[[part]]\nkind = \"Resistor\"\npackage = \"0402\"\nsymbol = \"a:R\"\nfootprint = \"a:R\"\n",
            "kicad.toml",
            Path::new("."),
        )
        .unwrap();
        let e = standard().merged(project).unwrap_err().to_string();
        assert!(
            e.contains(
                "kicad.toml: part `Resistor` in package `0402` is already mapped by std/kicad.toml"
            ),
            "{e}"
        );
    }

    #[test]
    fn a_pair_twice_in_one_file_names_it() {
        let row =
            "[[part]]\nkind = \"X\"\npackage = \"P\"\nsymbol = \"a:X\"\nfootprint = \"a:X\"\n";
        let e = Mapping::parse(&format!("{row}{row}"), "kicad.toml", Path::new("."))
            .unwrap_err()
            .to_string();
        assert_eq!(e, "kicad.toml: part `X` in package `P` is mapped twice");
    }

    #[test]
    fn two_defaults_for_a_kind_are_refused() {
        let row = |p| {
            format!(
                "[[part]]\nkind = \"X\"\npackage = \"{p}\"\ndefault = true\nsymbol = \"a:X\"\nfootprint = \"a:X\"\n"
            )
        };
        let e = Mapping::parse(
            &format!("{}{}", row("A"), row("B")),
            "kicad.toml",
            Path::new("."),
        )
        .unwrap_err()
        .to_string();
        assert!(
            e.contains("package `B`") && e.contains("`A` already is the default for `X`"),
            "{e}"
        );
    }

    #[test]
    fn a_symbol_or_footprint_must_name_its_library() {
        let e = Mapping::parse(
            "[[part]]\nkind = \"X\"\npackage = \"P\"\nsymbol = \"R\"\nfootprint = \"a:X\"\n",
            "kicad.toml",
            Path::new("."),
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("symbol \"R\" is not `Library:Name`"), "{e}");
    }

    #[test]
    fn an_unknown_key_and_a_bare_library_are_refused() {
        let e = Mapping::parse("[[part]]\nknid = \"X\"\n", "kicad.toml", Path::new("."))
            .unwrap_err()
            .to_string();
        assert!(e.starts_with("kicad.toml: "), "{e}");
        assert!(e.contains("knid"), "{e}");
        let e = Mapping::parse(
            "[[library]]\nname = \"acme\"\n",
            "kicad.toml",
            Path::new("."),
        )
        .unwrap_err()
        .to_string();
        assert!(
            e.contains("lists no `symbols` file and no `footprints` folder"),
            "{e}"
        );
    }

    #[test]
    fn only_environment_variables_are_expanded() {
        // PATH is set in every test environment.
        let path = std::env::var("PATH").unwrap();
        let m = Mapping::parse(
            "[[library]]\nname = \"a\"\nsymbols = \"${PATH}/a.kicad_sym\"\nfootprints = \"~/a.pretty\"\n",
            "kicad.toml",
            Path::new("/proj"),
        )
        .unwrap();
        let lib = &m.libraries()[0];
        assert_eq!(
            lib.symbols,
            Some(PathBuf::from(format!("{path}/a.kicad_sym")))
        );
        // `~` is not special.
        assert_eq!(lib.footprints, Some(PathBuf::from("/proj/~/a.pretty")));
        let e = Mapping::parse(
            "[[library]]\nname = \"a\"\nsymbols = \"${NETSMITH_NOT_SET_ANYWHERE}/a.kicad_sym\"\n",
            "kicad.toml",
            Path::new("."),
        )
        .unwrap_err()
        .to_string();
        assert!(
            e.contains("${NETSMITH_NOT_SET_ANYWHERE}") && e.contains("is not set"),
            "{e}"
        );
        assert!(e.contains("library `a`"), "{e}");
    }

    #[test]
    fn a_project_without_a_kicad_toml_has_the_standard_mapping() {
        let dir = crate::testlib::TempDir::new("nokicad");
        assert_eq!(Mapping::for_project(Some(dir.path())).unwrap(), standard());
        assert_eq!(Mapping::for_project(None).unwrap(), standard());
        std::fs::write(dir.path().join(PROJECT_FILE), PROJECT).unwrap();
        let m = Mapping::for_project(Some(dir.path())).unwrap();
        assert!(m.map("Ldo9", Some("DFN-8-EP")).is_ok());
        assert_eq!(
            m.libraries()[0].symbols,
            Some(dir.path().join("libs/acme.kicad_sym"))
        );
    }

    #[test]
    fn a_project_gets_library_tables_with_paths_relative_to_its_output() {
        let m = Mapping::parse(PROJECT, "kicad.toml", Path::new("/work/proj")).unwrap();
        let tables = library_tables(m.libraries(), Path::new("/work/proj/out/Board"), false);
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].0, "sym-lib-table");
        assert_eq!(
            tables[0].1,
            "(sym_lib_table\n\t(version 7)\n\t(lib (name \"acme\")(type \"KiCad\")(uri \"${KIPRJMOD}/../../libs/acme.kicad_sym\")(options \"\")(descr \"\"))\n)\n"
        );
        assert_eq!(tables[1].0, "fp-lib-table");
        assert!(
            tables[1].1.contains("${KIPRJMOD}/../../libs/acme.pretty"),
            "{}",
            tables[1].1
        );
        // A project with no libraries needs no tables, and what is written reads back.
        assert!(library_tables(&[], Path::new("/work"), false).is_empty());
        let back = library_stanzas(Some(&tables[0].1), Some(&tables[1].1)).unwrap();
        assert!(
            back.contains("symbols = \"../../libs/acme.kicad_sym\""),
            "{back}"
        );
    }

    #[test]
    fn the_standard_library_is_listed_first_in_the_tables_when_a_board_uses_it() {
        let m = Mapping::parse(PROJECT, "kicad.toml", Path::new("/work/proj")).unwrap();
        let dir = Path::new("/work/proj/out/Board");
        let tables = library_tables(m.libraries(), dir, true);
        assert_eq!(tables.len(), 2);
        assert!(
            tables[0].1.contains(
                "(lib (name \"netsmith\")(type \"KiCad\")(uri \"${KIPRJMOD}/netsmith.kicad_sym\")"
            ),
            "{}",
            tables[0].1
        );
        assert!(tables[0].1.find("netsmith").unwrap() < tables[0].1.find("acme").unwrap());
        assert!(
            tables[1]
                .1
                .contains("(uri \"${KIPRJMOD}/netsmith.pretty\")")
        );
        // Alone, it makes both tables; the files they name are those of `stdlib::files`.
        let alone = library_tables(&[], dir, true);
        assert_eq!(alone.len(), 2);
        let files: Vec<String> = crate::stdlib::files(&[]).into_iter().map(|f| f.0).collect();
        assert!(files.contains(&"netsmith.kicad_sym".to_string()));
        // What a netsmith build wrote is not a project library to port.
        let back = library_stanzas(Some(&tables[0].1), Some(&tables[1].1)).unwrap();
        assert!(!back.contains("netsmith"), "{back}");
        assert!(back.contains("acme"), "{back}");
    }

    #[test]
    fn a_project_cannot_list_a_library_named_like_the_standard_one() {
        let e = Mapping::parse(
            "[[library]]\nname = \"netsmith\"\nfootprints = \"libs/mine.pretty\"\n",
            "kicad.toml",
            Path::new("."),
        )
        .unwrap_err()
        .to_string();
        assert!(
            e.contains("kicad.toml: library `netsmith`")
                && e.contains("standard library's own footprints and symbols")
                && e.contains("another name"),
            "{e}"
        );
    }

    #[test]
    fn library_tables_become_stanzas() {
        let sym = r#"(sym_lib_table (version 7)
            (lib (name "acme")(type "KiCad")(uri "${KIPRJMOD}/libs/acme.kicad_sym")(options "")(descr ""))
            (lib (name "Device")(type "KiCad")(uri "${KICAD9_SYMBOL_DIR}/Device.kicad_sym")(options "")(descr "")))"#;
        let fp = r#"(fp_lib_table (version 7)
            (lib (name "acme")(type "KiCad")(uri "${KIPRJMOD}/libs/acme.pretty")(options "")(descr ""))
            (lib (name "other")(type "KiCad")(uri "${PATH}/fp/other.pretty")(options "")(descr "")))"#;
        let out = library_stanzas(Some(sym), Some(fp)).unwrap();
        assert_eq!(
            out,
            "[[library]]\nname = \"acme\"\nsymbols = \"libs/acme.kicad_sym\"\nfootprints = \"libs/acme.pretty\"\n\n\
             [[library]]\nname = \"other\"\nfootprints = \"${PATH}/fp/other.pretty\"\n\n"
        );
        // What it prints is what the mapping reads.
        let m =
            Mapping::parse(&out, "kicad.toml", Path::new("/p")).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(m.libraries().len(), 2);
        assert!(library_stanzas(Some("(lib"), None).is_err());
    }

    const GEN_ROW: &str = "# A part of my own.\n\n[[part]]\nkind = \"Chip\"\npackage = \"QFN-16\"\ndefault = true\nfootprint = \"gen:QFN(pins=16, pitch=0.5mm, body=4x4mm)\"\n";

    #[test]
    fn a_row_may_name_a_generator_and_its_parameters_and_the_mapping_holds_its_id() {
        let m =
            Mapping::parse(GEN_ROW, "kicad.toml", Path::new(".")).unwrap_or_else(|e| panic!("{e}"));
        let mapped = m.map("Chip", None).unwrap();
        assert_eq!(mapped.footprint, "gen:QFN_body4x4mm_pins16_pitch0.5mm");
        assert_eq!(mapped.lib_id, None);
        // The row says where it is, for the messages that name it.
        assert_eq!(mapped.origin, "kicad.toml line 7");
        assert_eq!(m.rows()[0].line, 7);
        // The id is the spec's, however the row spelled it.
        let again = Mapping::parse(
            &GEN_ROW.replace(
                "pins=16, pitch=0.5mm, body=4x4mm",
                "body=4.0x4mm,pitch=0.50mm,pins=16",
            ),
            "kicad.toml",
            Path::new("."),
        )
        .unwrap();
        assert_eq!(again.map("Chip", None).unwrap().footprint, mapped.footprint);
    }

    #[test]
    fn a_gen_footprint_that_cannot_be_read_names_the_row_and_the_line() {
        let e = Mapping::parse(
            &GEN_ROW.replace("pitch=0.5mm", "pitch"),
            "kicad.toml",
            Path::new("."),
        )
        .unwrap_err()
        .to_string();
        assert!(
            e.starts_with(
                "kicad.toml:7: part `Chip` in package `QFN-16`: footprint \"gen:QFN(pins=16, pitch, body=4x4mm)\": `pitch` is not `key=value`"
            ),
            "{e}"
        );
        let e = Mapping::parse(
            &GEN_ROW.replace("body=4x4mm)", "body=4x4mm"),
            "kicad.toml",
            Path::new("."),
        )
        .unwrap_err()
        .to_string();
        assert!(
            e.contains("kicad.toml:7:") && e.contains("missing the `)`"),
            "{e}"
        );
    }

    #[test]
    fn a_footprint_that_is_neither_an_id_nor_a_gen_spelling_is_refused() {
        let e = Mapping::parse(
            &GEN_ROW.replace("gen:QFN(pins=16, pitch=0.5mm, body=4x4mm)", "QFN16"),
            "kicad.toml",
            Path::new("."),
        )
        .unwrap_err()
        .to_string();
        assert!(
            e.contains(
                "footprint \"QFN16\" is not `Library:Name` or `gen:Generator(key=value, ...)`"
            ),
            "{e}"
        );
        // `gen:` always starts a spelling, so a generated id is not a way to write a row; and a
        // project library may not be called `gen`.
        let e = Mapping::parse(
            &GEN_ROW.replace(
                "gen:QFN(pins=16, pitch=0.5mm, body=4x4mm)",
                "gen:QFN_pins16",
            ),
            "kicad.toml",
            Path::new("."),
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("generator name"), "{e}");
        let e = Mapping::parse(
            "[[library]]\nname = \"gen\"\nfootprints = \"x.pretty\"\n",
            "kicad.toml",
            Path::new("."),
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("library of generated footprints"), "{e}");
    }
}
