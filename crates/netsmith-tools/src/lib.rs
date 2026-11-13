//! The support code of the `netsmith` command line, apart from the stage crates that do the work:
//! the scaffold of a new project (`init`), the findings the commands print (`findings`), the
//! comparison and parity checks of a ported board (`compare`, `parity`, `reference_table`, the
//! `port*` files), the DXF command (`dxf`), the symbol gallery and the part draft. The commands
//! themselves are `netsmith-cli`; the tests that start the binary are there, and the tests of
//! this code, with the board tests that use it, are here.

pub mod compare;
pub mod dxf;
pub mod findings;
pub mod init;
pub mod parity;
pub mod part_draft;
pub mod port;
pub mod port_skeleton;
pub mod port_tables;
pub mod reference_table;
pub mod symbol_gallery;
