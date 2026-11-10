//! KiCad 9 file output for netsmith, and the KiCad libraries it reads.

pub mod generate;
pub mod install;
pub mod mapping;
pub mod pinout;
pub mod schematic;
pub mod sexp;
pub mod stdlib;
pub mod symbols;
pub mod testlib;

pub use mapping::{
    MapError, Mapped, Mapping, POWER_FLAG_SYMBOL, PROJECT_FILE, ProjectLibrary, Row,
    library_stanzas, library_tables,
};
pub use netsmith_core::{PartPin, PinKind, PowerFlow};
pub use pinout::Pinout;
pub use schematic::{EmitError, emit_schematic, project_file};
pub use symbols::{PinGeometry, PinType, Symbol, SymbolError, SymbolLib, SymbolPin};
