//! A board's geometry as the board file holds it, the drawing of its layers, and its design
//! rules check: what the board model (`netsmith-pcb`), the router and fabrication
//! (`netsmith-fab`) share, in a crate below all three (issue #611).
//!
//! - [`Artwork`]: the board read from its `.kicad_pcb` (pads, tracks, vias, zones, drawings,
//!   text), with [`Pt`] and [`Arc`] and [`edge_outline`]'s outline.
//! - [`plot_layer`], [`drill_files`], [`fill_pour`], [`pad_outline`], [`font_strokes`],
//!   [`Gerber`], [`Polygons`]: the drawing of each layer, as Gerber X2 and Excellon text or as
//!   polygons. `netsmith_pcb::copper_of` draws the
//!   copper as data with the same code the Gerbers are drawn with (ADR 0105), so they agree.
//! - [`drc_check`]: netsmith's own design rules check (ADR 0037), and the room and teardrops the
//!   router and the meanders measure with.
//! - [`COUPLED`] and [`PAIR_SUFFIXES`]: the nets that make a diff pair.
//!
//! The package (the zip, the bill of materials, the placement file, the notes), the panel, the
//! cross-check with KiCad and the route scorecard are `netsmith-fab`'s.
//!
//! The modules are private; this root lists the crate's API. A name that would be too general
//! at the root carries what it belongs to (`DrcReport`, `PlotRules`, `drill_files`).

#![cfg_attr(not(test), warn(unreachable_pub))]

mod artwork;
mod drc;
mod drill;
mod edge;
mod fill;
mod font;
mod gbr;
mod geom;
mod gerber;
mod pads;
mod pairs;
mod plot;
mod polygons;

pub use artwork::{
    Artwork, ArtworkError, Connect as ArtworkConnect, Drill as ArtworkDrill,
    Footprint as ArtworkFootprint, Graphic as ArtworkGraphic, HAlign, Keepout as ArtworkKeepout,
    Pad as ArtworkPad, PadKind as ArtworkPadKind, PadShape as ArtworkPadShape,
    Primitive as ArtworkPrimitive, Shape as ArtworkShape, Text as ArtworkText,
    Track as ArtworkTrack, TrackShape, VAlign, Via as ArtworkVia, Zone as ArtworkZone,
};
pub use drc::constraints::{
    BARE_COURTYARD_MARGIN, Constraint, Constraints, Measure as ConstraintMeasure,
    Outcome as ConstraintOutcome, PAD_COURTYARD_MARGIN, Relation as ConstraintRelation,
    RelationEnd as ConstraintRelationEnd, SideRule, body_of as constraint_body_of,
    outcome_findings as constraint_outcome_findings,
};
pub use drc::shape::{Gap as ShapeGap, Shape as DrcShape, gap as shape_gap};
pub use drc::{
    Class as DrcClass, Counts as DrcCounts, Error as DrcError, Extras as DrcExtras,
    Finding as DrcFinding, Item as DrcItem, Report as DrcReport, Room, Rule as DrcRule,
    Rules as DrcRules, Severity as DrcSeverity, Teardrop as DrcTeardrop, check as drc_check,
    check_artwork as drc_check_artwork, matches_pattern as drc_matches_pattern,
    outline_shapes as drc_outline_shapes, project_of as drc_project_of, teardrops,
};
pub use drill::{Hole as DrillHole, files as drill_files, holes as drill_holes};
pub use edge::{Outline as EdgeOutline, cut_lines as edge_cut_lines, outline as edge_outline};
pub use fill::pour as fill_pour;
pub use font::strokes as font_strokes;
pub use gbr::{Ap, Contour, Gerber, Pen, Seg as GerberSeg, escape as gerber_escape};
pub use geom::{
    ARC_ERROR, Arc, Bounds, Pt, contains as polygon_contains, steps_for as arc_steps_for,
};
pub use gerber::{
    GerberError, file_suffix as gerber_file_suffix, plot_layers as gerber_plot_layers,
};
pub use pads::{Outline as PadOutline, Prim as PadPrim, outline as pad_outline};
pub use pairs::{COUPLED, PAIR_SUFFIXES};
pub use plot::{
    Plot, Rules as PlotRules, drawings as plot_drawings, mask as plot_mask, plot as plot_layer,
};
pub use polygons::{Area as PolygonArea, Polygons};

/// A file: a Gerber or drill file netsmith plotted, or one KiCad wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// The rules the plot needs, from the text of the project file beside the board (`.kicad_pro`):
/// what `fab` plots with, and what `netsmith_pcb::copper_of` fills the pours with.
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
