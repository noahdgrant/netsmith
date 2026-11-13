//! KiCad 9 file output for netsmith, and the KiCad libraries it reads.

pub mod footprint_spec;
pub mod generate;
pub mod hierarchy;
pub mod install;
pub mod mapping;
pub mod passives;
pub mod pinout;
pub mod resolve;
pub mod schematic;
pub mod sexp;
pub mod stdlib;
pub mod symbols;

pub use hierarchy::{Hierarchy, SymbolLink};
pub use mapping::{
    MapError, Mapped, Mapping, POWER_FLAG_SYMBOL, PROJECT_FILE, ProjectLibrary, Row,
    library_stanzas, library_tables,
};
pub use netsmith_core::{PartPin, PinKind, PowerFlow};
pub use pinout::Pinout;
pub use schematic::{
    EmitError, Schematic, emit_schematic, is_netsmith_sheet, needs_power_flag, project_file,
};
pub use symbols::{
    PinGeometry, PinType, Symbol, SymbolError, SymbolLib, SymbolPin, symbol_in, symbol_names_in,
};
