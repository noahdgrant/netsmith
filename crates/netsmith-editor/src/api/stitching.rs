//! The stitching vias the export will have that the routes file does not (issue 1489): what
//! `netsmith view pcb --stitching` draws, as the page reads it. They are
//! `netsmith_router::stitching_vias` of the board, the stitching `fab` runs.
//!
//! **`GET /api/pcb/stitching?board=`** answers, of the board as the PCB view has it placed (and
//! routed), the stitching vias of its pours that the routes file does not hold: those the router
//! already laid, then those the stitching would add to the copper there. The places are in mm in
//! the frame of the board file, y down, as the scene's. `404` for a board that is not there; a
//! board that cannot be placed is a `500` with the reason.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A stitching via: its net, its centre and its diameter in mm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StitchingVia {
    pub net: String,
    pub x: f64,
    pub y: f64,
    pub diameter: f64,
}

/// The stitching vias of a board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbStitching {
    pub board: String,
    /// The stamps of the placement and of the routes file they were found on, and whether the
    /// board was routed.
    pub layout_stamp: String,
    pub routes_stamp: String,
    pub routed: bool,
    pub vias: Vec<StitchingVia>,
}
