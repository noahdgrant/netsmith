//! The KiCad libraries a build reads: the mapping from parts to symbol and footprint names,
//! and the symbol and footprint libraries themselves (ADR 0016, ADR 0021).

use std::path::Path;
use std::sync::Arc;

use netsmith_core::Design;
use netsmith_kicad::{Mapping, Symbol, SymbolLib, stdlib, testlib};
use netsmith_pcb::FootprintLibrary;

/// What `build`, `fab` and `view` read from KiCad. `check` and `solve` need none of it.
pub struct Libraries {
    pub mapping: Mapping,
    pub symbols: SymbolLib,
    pub footprints: FootprintLibrary,
}

impl Libraries {
    /// The installed KiCad libraries (found as `netsmith_kicad::install` describes, ADR 0021)
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
        let mapping = Mapping::for_project(root).map_err(|e| e.to_string())?;
        Ok(Libraries::with_mapping(
            mapping,
            SymbolLib::installed(),
            FootprintLibrary::installed(),
        ))
    }

    /// Our own small libraries (`netsmith_kicad::testlib`) with the standard mapping, for
    /// tests that need no KiCad.
    pub fn sample() -> Libraries {
        Libraries::with_mapping(
            Mapping::standard(),
            SymbolLib::at(testlib::sample_symbols()),
            FootprintLibrary::at(testlib::sample_footprints()),
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

    /// The symbols drawn from the pin tables of `design`'s parts, for the rows of the mapping
    /// that have no `symbol` (ADR 0097), one for each distinct id, in the order of the parts.
    pub fn generated_symbols(&self, design: &Design) -> Vec<Arc<Symbol>> {
        let mut symbols: Vec<Arc<Symbol>> = Vec::new();
        for part in design.parts.iter().filter(|p| p.kind != "Label") {
            let Ok(mapped) = self.mapping.map(&part.kind, part.package.as_deref()) else {
                continue;
            };
            if mapped.lib_id.is_some() {
                continue;
            }
            if let Ok(symbol) = self.symbols.symbol_of(part, &mapped)
                && !symbols.iter().any(|s| s.lib_id() == symbol.lib_id())
            {
                symbols.push(symbol);
            }
        }
        symbols
    }

    /// Whether a part of `design` is drawn or carried by the standard library's own symbols or
    /// footprints (`std/libs/`, ADR 0087), or is drawn by a symbol netsmith made (ADR 0097).
    pub fn uses_std_library(&self, design: &Design) -> bool {
        design.parts.iter().any(|part| {
            self.mapping
                .map(&part.kind, part.package.as_deref())
                .is_ok_and(|m| {
                    // A row with no symbol is drawn from the pin table, in the library `netsmith`.
                    let own_symbol = m
                        .lib_id
                        .as_deref()
                        .map_or(!part.pins.is_empty(), stdlib::is_library_id);
                    own_symbol || stdlib::is_library_id(&m.footprint)
                })
        })
    }

    /// Write the library tables of KiCad beside a board's `.kicad_pro` in `dir`: the project's
    /// own libraries, and, when a part of `design` uses it, the standard library's own files
    /// with their entries, so that KiCad finds every symbol and footprint of the board like
    /// any other library's (ADR 0021, ADR 0087). Writes nothing for a board that needs no table.
    pub fn write_tables(&self, dir: &Path, design: &Design) -> std::io::Result<()> {
        let with_std = self.uses_std_library(design);
        if with_std {
            for (path, text) in stdlib::files(&self.generated_symbols(design)) {
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
