//! The per-net table of the board (issue #1223): what `netsmith board nets` prints, as the page
//! reads it. The values are `netsmith_fab::NetTable`'s, so the page and the command line cannot
//! differ; their field names are camelCase here and nothing else differs.
//!
//! **`GET /api/pcb/nets?board=`** measures the board's copper as the PCB view has it placed
//! (and routed) and answers for each net that no zone pours its routed and straight length, its
//! vias, its layer changes and its layers, and for each diff pair both lengths, the skew against
//! the router's target, the coupled length, the gaps over it and the vias side by side. `404`
//! for a board that is not there; a board that cannot be measured is a `500` with the reason.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One net's numbers, in mm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NetsRow {
    pub net: String,
    /// The length of all its tracks.
    pub length: f64,
    /// The shortest tree of straight lines between its pads' centres.
    pub straight: f64,
    /// `length` over `straight`; 0 for pads on one point.
    pub ratio: f64,
    pub vias: u32,
    /// The vias where its tracks meet on two layers or more.
    pub layer_changes: u32,
    /// The copper layers its tracks are on, front to back, as the board file names them.
    pub layers: Vec<String>,
}

/// One diff pair's numbers, in mm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NetsPair {
    /// The two nets, the positive first.
    pub p: String,
    pub n: String,
    pub length_p: f64,
    pub length_n: f64,
    pub skew: f64,
    /// The skew the pair may have: the router's target.
    pub limit: f64,
    pub over: bool,
    /// The length of `p` that runs coupled, and its share of `p`, 0 to 1.
    pub coupled: f64,
    pub share: f64,
    /// The least and the largest gap, edge to edge, over the coupled length.
    pub gaps: Option<(f64, f64)>,
    pub vias_p: u32,
    pub vias_n: u32,
    /// The vias of `p` with a via of `n` beside them.
    pub via_pairs: u32,
}

/// The table of the board's nets and diff pairs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbNets {
    pub board: String,
    /// The stamps of the placement and of the routes file it was measured on, and whether it
    /// was routed.
    pub layout_stamp: String,
    pub routes_stamp: String,
    pub routed: bool,
    /// The nets, by name.
    pub nets: Vec<NetsRow>,
    pub pairs: Vec<NetsPair>,
}
