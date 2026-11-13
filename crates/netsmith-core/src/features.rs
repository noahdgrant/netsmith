//! The current of each net and the optional features of a design.

use crate::{
    CaseModel, Cutout, Keepout, Label, Offset, Outline, PartHeight, PinProtrusion, Placement, Pour,
    StepModel,
};

/// The current a net carries, and the least track that carries it (issue #431).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NetCurrent {
    pub net: String,
    /// The worst case, in amperes: every load on at once (ADR 0036). A lower bound where a
    /// current on the net is not known.
    pub current: f64,
    /// The narrowest track that carries it, in mm, before the process's least track.
    pub least_width: f64,
    /// How many of the process's vias, side by side, carry it where a track changes layer.
    pub vias: usize,
}

/// What a design says about the board beyond its parts and nets (ADR 0020). Everything is
/// optional; a design that says none of it is laid out as before.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Features {
    /// The narrowest track of the nets that have a rail with a `min_track`, in mm, by net
    /// name. A net takes the largest bottom of the ranges of the rails with a pin on it, so a
    /// ground that two rails return through takes the wider (ADR 0020; the reason for a field
    /// of the rail is in `std/sources.nsm`). The nets no rail gives a width are sized from
    /// their current (ADR 0036, see `netsmith_design::guidelines`) and put here too.
    pub net_widths: Vec<(String, f64)>,
    /// The current each net carries, where it is known, and the least width that carries it
    /// (IPC-2221 at the width tolerance, ADR 0036): what the router may not narrow a track of
    /// the net below, away from its pads, and what the design rules check holds the routed
    /// copper to (issue #431). Sorted by net name.
    pub net_currents: Vec<NetCurrent>,
    pub outline: Option<Outline>,
    pub cutouts: Vec<Cutout>,
    pub pours: Vec<Pour>,
    pub keepouts: Vec<Keepout>,
    /// The heights the design states for parts, by path (ADR 0045, "Height limits").
    pub heights: Vec<PartHeight>,
    /// How far the pins of through-hole parts protrude, as the design states it, by path
    /// (ADR 0045 section 15).
    pub protrusions: Vec<PinProtrusion>,
    /// The STEP models parts are drawn from, by path (ADR 0108).
    pub models: Vec<StepModel>,
    /// The STEP files of the board's case, by member name (ADR 0109).
    pub case_models: Vec<CaseModel>,
    /// The paths of the parts the board house does not assemble (`.assembly` is `hand` or
    /// `none`), so fitted by hand or not at all. Filled by the caller that knows who
    /// assembles each part; empty otherwise, which counts every part as assembled by the
    /// board house.
    pub hand_fitted: Vec<String>,
    pub labels: Vec<Label>,
    /// Requirements on where parts and modules go (ADR 0045), by path.
    pub constraints: Vec<Placement>,
    /// Relations between the places of two parts (ADR 0045, section 3).
    pub offsets: Vec<Offset>,
    /// The text of each line of the physical file that states a requirement, by the
    /// `file:line` the `source` fields name (`widget.physical.nsm:14`), so that a message can
    /// quote the line. Filled by the caller that knows the source files; empty otherwise.
    pub line_texts: std::collections::BTreeMap<String, String>,
}
