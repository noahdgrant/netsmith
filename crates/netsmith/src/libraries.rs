//! The KiCad libraries a build reads: the mapping from parts to symbol and footprint names,
//! and the symbol and footprint libraries themselves (ADR 0016, ADR 0021).

use std::path::Path;

use netsmith_kicad::{Mapping, SymbolLib, testlib};
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
}
