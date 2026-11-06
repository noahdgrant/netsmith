//! A schematic drawing for netsmith designs: a layout that reads left to right and an SVG
//! drawing of it, which the editor shows. Everything here is a pure,
//! deterministic function of a [`netsmith_core::Design`] and a symbol library.
//!
//! [`metrics`] measures how readable a drawing is and [`check`] tests the rules every drawing
//! must keep (its nets are the design's nets); neither changes what is drawn.

pub mod check;
pub mod geom;
pub mod glyph;
pub mod layout;
pub mod metrics;
pub mod sheets;
mod sheetview;
pub mod stability;
mod svg;

pub use layout::{
    Bus, BusEntry, BusFlag, BusMember, Layout, MarkKind, Marks, PinMark, Port, PowerKind,
    PowerSymbol, Rails, Tuning, layout, layout_marked, layout_rails, layout_tuned,
    layout_tuned_touched,
};
pub use sheets::{Group, Sheet, layout_sheets, layout_sheets_marked, layout_sheets_rails};
pub use svg::{
    ViewError, render_svg, render_svg_by_module, render_svg_by_module_marked,
    render_svg_by_module_with, render_svg_with,
};
