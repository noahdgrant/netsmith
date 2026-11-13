//! KiCad 9 file output for netsmith, and the KiCad libraries it reads.
//!
//! The modules are private; this root lists the crate's API. A name that would be too general
//! at the root carries the module it comes from (`stdlib_files`, `passive_symbol`,
//! `generated_symbol`, `locate_install`).

#![warn(unreachable_pub)]

mod footprint_spec;
mod generate;
mod hierarchy;
mod install;
mod mapping;
mod passives;
mod pinout;
mod resolve;
mod schematic;
mod sexp;
mod stdlib;
mod symbols;

pub use footprint_spec::{FootprintSpec, LIBRARY as GENERATED_FOOTPRINT_LIBRARY};
pub use generate::{
    Side, overlapping_names, part_gives_power, side_of, symbol as generated_symbol, symbol_name,
};
pub use hierarchy::{Hierarchy, SymbolLink};
pub use install::{
    CLI_VARIABLE, FOOTPRINTS_VARIABLE, Found, Kind as InstallKind, NO_KICAD_VARIABLE, NotFound,
    SYMBOLS_VARIABLE, Search, locate as locate_install,
};
#[cfg(feature = "test-support")]
pub use install::{kicad_libraries_installed, skip_without_kicad_libraries};
pub use mapping::{
    MapError, Mapped, Mapping, POWER_FLAG_SYMBOL, PROJECT_FILE, ProjectLibrary, Row,
    library_stanzas, library_tables,
};
pub use netsmith_core::{PartPin, PinKind, PowerFlow};
pub use passives::{
    answer_id as passive_answer_id, ids as passive_symbol_ids, symbol as passive_symbol,
};
pub use pinout::Pinout;
pub use resolve::{PartSymbol, part_symbols};
pub use schematic::{
    EmitError, Schematic, emit_schematic, is_netsmith_sheet, needs_power_flag, project_file,
};
pub use sexp::{ParseError, Sexp, fmt_num};
pub use stdlib::{
    FOOTPRINT_FOLDER, NAME as STDLIB_NAME, files as stdlib_files, footprint as stdlib_footprint,
    footprint_names as stdlib_footprint_names, is_library_id,
};
pub use symbols::{
    PinGeometry, PinType, Symbol, SymbolError, SymbolLib, SymbolPin, symbol_in, symbol_names_in,
};
