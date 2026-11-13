//! The netsmith library: the pipeline, the fab driver and the editor's backend (re-exported from
//! the stage crates that hold them), and what the command line builds on them.

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

pub use netsmith_3d::render;
pub use netsmith_fabricate::{
    board3d, case_fit, fab, layout, models, no_supplier, order, physical, picture, pin_export,
    pin_swap, pins, placed, project, refresh, system3d,
};
pub use netsmith_pipeline::*;
pub use netsmith_workspace as editor;
