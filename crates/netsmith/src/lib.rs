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

/// `netsmith render`'s picture: what the command line asks of the 3D crate, under the names it
/// uses.
pub mod render {
    pub use netsmith_3d::{
        RenderCaseMode as CaseMode, RenderRequest as Request, default_render_out as default_out,
        parse_render_section as parse_section, parse_render_size as parse_size, render_png as draw,
        render_system_png as draw_system,
    };
}
pub use netsmith_fabricate::{
    board3d, case_fit, fab, layout, models, no_supplier, order, physical, picture, pin_export,
    pin_swap, pins, placed, project, refresh, system3d,
};
pub use netsmith_pipeline::*;
pub use netsmith_workspace as editor;
