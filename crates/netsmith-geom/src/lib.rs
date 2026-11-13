//! A board's geometry as the board file holds it, the drawing of its layers, and its design
//! rules check: what the board model (`netsmith-pcb`), the router and fabrication
//! (`netsmith-fab`) share, in a crate below all three (issue #611).
//!
//! - [`artwork`]: the board read from its `.kicad_pcb` (pads, tracks, vias, zones, drawings,
//!   text), with [`geom`]'s points and arcs and [`edge`]'s outline.
//! - [`plot`], [`drill`], [`fill`], [`pads`], [`font`], [`gbr`], [`polygons`]: the drawing of
//!   each layer, as Gerber X2 and Excellon text or as polygons. `netsmith_pcb::copper` draws
//!   the copper as data with the same code the Gerbers are drawn with (ADR 0105), so they agree.
//! - [`drc`]: netsmith's own design rules check (ADR 0037), and the room and teardrops the
//!   router and the meanders measure with.
//! - [`pairs`]: the nets that make a diff pair.
//!
//! The package (the zip, the bill of materials, the placement file, the notes), the panel, the
//! cross-check with KiCad and the route scorecard are `netsmith-fab`'s.

pub mod artwork;
pub mod drc;
pub mod drill;
pub mod edge;
pub mod fill;
pub mod font;
pub mod gbr;
pub mod geom;
pub mod gerber;
pub mod pads;
pub mod pairs;
pub mod plot;
pub mod polygons;

/// A file: a Gerber or drill file netsmith plotted, or one KiCad wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// The rules the plot needs, from the text of the project file beside the board (`.kicad_pro`):
/// what `fab` plots with, and what `netsmith_pcb::copper` fills the pours with.
pub fn plot_rules_of(pro: Option<&str>) -> plot::Rules {
    let pro = pro.unwrap_or_default();
    let number = |key: &str| -> Option<f64> {
        let at = pro.find(&format!("\"{key}\""))?;
        let rest = pro[at..].split_once(':')?.1;
        let end = rest.find([',', '}', '\n'])?;
        rest[..end].trim().parse().ok()
    };
    plot::Rules {
        edge_clearance: number("min_copper_edge_clearance").unwrap_or(0.0),
        hole_clearance: number("min_hole_clearance"),
    }
}
