//! A schematic drawing for netsmith designs: a layout that reads left to right and an SVG
//! drawing of it, which the editor shows. Everything here is a pure,
//! deterministic function of a [`netsmith_core::Design`] and a symbol library.

pub mod geom;
pub mod glyph;
pub mod layout;
pub mod sheets;
mod svg;

pub use layout::{Layout, layout};
pub use sheets::{Group, Sheet, layout_sheets};
pub use svg::{ViewError, render_svg, render_svg_by_module};
