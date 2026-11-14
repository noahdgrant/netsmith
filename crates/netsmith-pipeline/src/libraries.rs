//! The KiCad libraries a build reads: the mapping from parts to symbol and footprint names,
//! and the symbol and footprint libraries themselves (ADR 0016, ADR 0021; where the libraries come
//! from is ADR 0097's).
//!
//! netsmith answers the ids of its own mapping itself (generated symbols and footprints, `std/libs/`,
//! `gen:` footprints), so a run needs no KiCad install. An installed library is asked only for an id
//! that netsmith cannot answer, a KiCad id of a project's `kicad.toml` that no `[[library]]` and no
//! generator covers; only then does a missing install stop a run ([`needs_kicad`]).

use std::path::Path;
use std::sync::Arc;

use netsmith_core::Design;
use netsmith_kicad::{
    Mapped, Mapping, Search, Symbol, SymbolError, SymbolLib, is_library_id, stdlib_files,
};
use netsmith_pcb::{FootprintError, FootprintLibrary};

/// What `build`, `fab` and `view` read from KiCad. `check` and `solve` need none of it.
pub struct Libraries {
    pub mapping: Mapping,
    pub symbols: SymbolLib,
    pub footprints: FootprintLibrary,
}

/// Make every `gen:` footprint of `mapping` (ADR 0104) once, so that a generator that refuses
/// its parameters is named with the row and the line, before any board is read. The first
/// refusal is the error.
pub fn check_generated_footprints(mapping: &Mapping) -> Result<(), String> {
    for row in mapping.rows() {
        if let Some(spec) = netsmith_kicad::FootprintSpec::from_id(&row.footprint)
            && let Err(why) = netsmith_pcb::generate_footprint_spec(&spec)
        {
            return Err(format!(
                "{}:{}: part `{}` in package `{}`: footprint `{}`: {why}",
                row.source,
                row.line,
                row.kind,
                row.package,
                spec.spelling()
            ));
        }
    }
    for member in mapping.members() {
        if let Some(spec) = netsmith_kicad::FootprintSpec::from_id(&member.footprint)
            && let Err(why) = netsmith_pcb::generate_footprint_spec(&spec)
        {
            return Err(format!(
                "{}:{}: member `{}`: footprint `{}`: {why}",
                member.source,
                member.line,
                member.path,
                spec.spelling()
            ));
        }
    }
    Ok(())
}

/// What `needs_kicad` says about the id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wanted {
    Symbol,
    Footprint,
}

/// What the lookup of a KiCad id found out, for [`needs_kicad`].
#[derive(Debug, Clone, Copy)]
pub enum Looked<'a> {
    /// No install was found: `0` is the search's own account of where it looked (or that it is
    /// switched off).
    NoInstall(&'a str),
    /// An install was found, at `at`, and has no such id (a typo, or a name KiCad 9 lacks):
    /// `detail` is the error of the lookup, with the path it looked for.
    Installed { at: &'a Path, detail: &'a str },
}

/// The search's account of where it looked, when the footprint error is that it found no install.
fn lookup_text_of(error: &FootprintError) -> String {
    match error {
        FootprintError::NoInstall { lookup, .. } => lookup.to_string(),
        _ => String::new(),
    }
}

/// The message of a run that stops because a part names a KiCad id that netsmith cannot answer
/// (it is asked only after the generators and `std/libs/` have not) and no installed library has
/// it: the part, the id, the `kicad.toml` line, where it looked and the ways out. With no install
/// the ways out include installing KiCad 9; with an install that lacks the id they do not, and the
/// message says so. `kind` and `package` are the part's; `origin` is the row's `file line N`
/// ([`Mapped::origin`]).
pub fn needs_kicad(
    wanted: Wanted,
    refdes: &str,
    kind: &str,
    package: Option<&str>,
    id: &str,
    origin: &str,
    looked: Looked,
) -> String {
    let (noun, third) = match wanted {
        Wanted::Symbol => (
            "symbol",
            "omit `symbol` in that row, and netsmith draws the part from its pin table",
        ),
        Wanted::Footprint => (
            "footprint",
            "give that row a `gen:` footprint, for a package a generator makes (`footprint = \"gen:SOT23(pins=3)\"`)",
        ),
    };
    let part = match package {
        Some(package) => format!("part `{kind}` in package `{package}`"),
        None => format!("part `{kind}`"),
    };
    match looked {
        Looked::NoInstall(looked) => format!(
            "{refdes}: {part} needs KiCad's {noun} `{id}` ({origin}), and no installed KiCad \
             library has it.\n{looked}\n\
             Three ways out: install KiCad 9, so that netsmith reads its libraries; supply the file \
             through a `[[library]]` of kicad.toml; or {third}."
        ),
        Looked::Installed { at, detail } => format!(
            "{refdes}: {part} needs KiCad's {noun} `{id}` ({origin}).\n{detail}\n\
             KiCad is installed at {} but has no `{id}`: check the spelling, or use a project \
             library (a `[[library]]` of kicad.toml) or a netsmith generated {noun}: {third}.",
            at.display()
        ),
    }
}

impl Libraries {
    /// Whether `id` (`Library:Name`) is KiCad's: its library is neither one the project lists in
    /// `kicad.toml` nor netsmith's own (`std/libs/`), whose errors say what is wrong with them.
    fn is_kicads(&self, id: &str) -> bool {
        !is_library_id(id)
            && !id.split_once(':').is_some_and(|(library, _)| {
                self.mapping.libraries().iter().any(|l| l.name == library)
            })
    }

    /// The message for a footprint that could not be loaded, when the cause is that it is a
    /// KiCad id no installed library has; `None` for any other error, which says its own cause.
    pub fn footprint_needs_kicad(
        &self,
        refdes: &str,
        kind: &str,
        package: Option<&str>,
        mapped: &Mapped,
        error: &FootprintError,
    ) -> Option<String> {
        let (detail, lookup_text) = (error.to_string(), lookup_text_of(error));
        let looked = match error {
            FootprintError::NoInstall { .. } => Looked::NoInstall(&lookup_text),
            // An install was found and has no such file: the error says the path it looked for
            // and the variable that moves the search.
            FootprintError::NotFound { project: false, .. } => Looked::Installed {
                at: self.footprints.root(),
                detail: &detail,
            },
            _ => return None,
        };
        self.is_kicads(&mapped.footprint).then(|| {
            needs_kicad(
                Wanted::Footprint,
                refdes,
                kind,
                package,
                &mapped.footprint,
                &mapped.origin,
                looked,
            )
        })
    }

    /// The message for a symbol that could not be read, when the cause is that it is a KiCad id
    /// no installed library has; `None` for any other error.
    pub fn symbol_needs_kicad(
        &self,
        refdes: &str,
        kind: &str,
        package: Option<&str>,
        mapped: &Mapped,
        error: &SymbolError,
    ) -> Option<String> {
        let id = mapped.lib_id.as_deref()?;
        let detail = error.to_string();
        let lookup_text = match error {
            SymbolError::NoInstall { lookup, .. } => lookup.to_string(),
            _ => String::new(),
        };
        let looked = match error {
            SymbolError::NoInstall { .. } => Looked::NoInstall(&lookup_text),
            // An install was found and has no such library or symbol.
            SymbolError::NoLibrary { project: false, .. } | SymbolError::NoSymbol { .. } => {
                Looked::Installed {
                    at: self.symbols.root(),
                    detail: &detail,
                }
            }
            _ => return None,
        };
        self.is_kicads(id).then(|| {
            needs_kicad(
                Wanted::Symbol,
                refdes,
                kind,
                package,
                id,
                &mapped.origin,
                looked,
            )
        })
    }

    /// The installed KiCad libraries (found as `netsmith_kicad::Search` describes, ADR 0021)
    /// with the standard mapping, and nothing of a project.
    pub fn standard() -> Libraries {
        Libraries::with_mapping(
            Mapping::standard(),
            SymbolLib::installed(),
            FootprintLibrary::installed(),
        )
    }

    /// The installed libraries and the standard mapping, plus the `kicad.toml` of the
    /// project at `root`, if there is one: its parts and its own libraries.
    pub fn for_project(root: Option<&Path>) -> Result<Libraries, String> {
        Libraries::for_project_by(root, &Search::machine())
    }

    /// [`Libraries::for_project`] over another machine (`Search::under` an empty folder is
    /// one with no KiCad), for tests.
    pub fn for_project_by(root: Option<&Path>, search: &Search) -> Result<Libraries, String> {
        let mapping = Mapping::for_project(root).map_err(|e| e.to_string())?;
        check_generated_footprints(&mapping)?;
        Ok(Libraries::with_mapping(
            mapping,
            SymbolLib::installed_by(search),
            FootprintLibrary::installed_by(search),
        ))
    }

    /// Our own small libraries (`netsmith_testkit`) with the standard mapping, for tests that
    /// need no KiCad. Behind the `test-support` feature, which only dev-dependencies turn on.
    #[cfg(any(test, feature = "test-support"))]
    pub fn sample() -> Libraries {
        Libraries::with_mapping(
            Mapping::standard(),
            SymbolLib::at(netsmith_testkit::sample_symbols()),
            FootprintLibrary::at(netsmith_testkit::sample_footprints()),
        )
    }

    /// `mapping`, with the project libraries it lists added to the two libraries.
    pub fn with_mapping(
        mapping: Mapping,
        mut symbols: SymbolLib,
        mut footprints: FootprintLibrary,
    ) -> Libraries {
        for lib in mapping.libraries() {
            if let Some(file) = &lib.symbols {
                symbols = symbols.with_library(&lib.name, file);
            }
            if let Some(folder) = &lib.footprints {
                footprints = footprints.with_library(&lib.name, folder);
            }
        }
        Libraries {
            mapping,
            symbols,
            footprints,
        }
    }

    /// The symbols netsmith draws for `design`'s parts (ADR 0097): from the pin table, for a
    /// row of the mapping that has no `symbol`, and, for an id it makes (`Device:R`), the
    /// symbol of its own that answers it (`netsmith_kicad::passive_symbol_ids`). One for each distinct
    /// id, in the order of the parts, and the power flag after them when the schematic places
    /// one ([`netsmith_kicad::needs_power_flag`]).
    pub fn generated_symbols(&self, design: &Design) -> Vec<Arc<Symbol>> {
        let mut symbols: Vec<Arc<Symbol>> = Vec::new();
        // The same symbols the schematic embeds ([`netsmith_kicad::part_symbols`]), so that its ids and
        // the library file's agree.
        for (part, drawn) in design.parts.iter().zip(netsmith_kicad::part_symbols(
            design,
            &self.mapping,
            &self.symbols,
        )) {
            if part.kind == "Label" {
                continue;
            }
            if let Ok((_, symbol)) = drawn
                && is_library_id(symbol.lib_id())
                && !symbols.iter().any(|s| s.lib_id() == symbol.lib_id())
            {
                symbols.push(symbol);
            }
        }
        if netsmith_kicad::needs_power_flag(design, &self.mapping, &self.symbols)
            && let Ok(flag) = self.symbols.symbol(netsmith_kicad::POWER_FLAG_SYMBOL)
            && is_library_id(flag.lib_id())
        {
            symbols.push(flag);
        }
        symbols
    }

    /// Whether a part of `design` is drawn or carried by the standard library's own symbols or
    /// footprints (`std/libs/`, ADR 0087), or is drawn by a symbol netsmith made (ADR 0097), the
    /// power flag among them.
    pub fn uses_std_library(&self, design: &Design) -> bool {
        // A symbol netsmith draws (from the pin table, or for an id it makes) is in the library
        // `netsmith`.
        !self.generated_symbols(design).is_empty()
            || design.parts.iter().any(|part| {
                self.mapping
                    .map_part(&part.path, &part.kind, part.package.as_deref())
                    .is_ok_and(|m| is_library_id(&m.footprint))
            })
    }

    /// Write the library tables of KiCad beside a board's `.kicad_pro` in `dir`: the project's
    /// own libraries, and, when a part of `design` uses it, the standard library's own files
    /// with their entries, so that KiCad finds every symbol and footprint of the board like
    /// any other library's (ADR 0021, ADR 0087). Writes nothing for a board that needs no table.
    pub fn write_tables(&self, dir: &Path, design: &Design) -> std::io::Result<()> {
        let with_std = self.uses_std_library(design);
        if with_std {
            for (path, text) in stdlib_files(&self.generated_symbols(design)) {
                let file = dir.join(path);
                if let Some(parent) = file.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(file, text)?;
            }
        }
        for (name, text) in netsmith_kicad::library_tables(self.mapping.libraries(), dir, with_std)
        {
            std::fs::write(dir.join(name), text)?;
        }
        Ok(())
    }
}
