//! The placed board summed up as data (issue #1003): what `netsmith layout summary` prints, as
//! the page reads it. The numbers are `netsmith_pcb::Summary`'s, so the page, the command line
//! and the placer agree on what "close" means; their field names are camelCase here and nothing
//! else differs.
//!
//! **`GET /api/pcb/summary?board=`** answers the board as it is placed now (the placement the
//! scene draws, routed or not), in mm in the layout file's frame: from the outline's top left
//! corner, x to the right and y down. `near` (a part by path or designator, may be repeated)
//! with `within` (mm, default 2) lists the parts within that distance courtyard to courtyard,
//! and `between` (`PART.PAD,PART.PAD`, may be repeated) gives the distance between two pads;
//! both are left empty without the parameter. `404` for a board that is not there, `400` with
//! the reason for a part or pad the board has none of; it waits for the first analysis like the
//! other views and answers `503` to a request that sends `X-Netsmith-Pending: 1`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A box in the layout file's frame, in mm.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SummaryBox {
    pub min: [f64; 2],
    pub max: [f64; 2],
}

/// The board's measures of the placement as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SummaryTotals {
    /// The sum over the nets of the half perimeter of the box round the net's pads, in mm.
    pub hpwl: f64,
    /// The length of the ratsnest, in mm.
    pub ratsnest: f64,
    /// The pairs of ratsnest lines of different nets that cross.
    pub crossings: u32,
    /// The mean and the greatest distance of a decoupling capacitor from its part, in mm.
    pub decoupling_mean: f64,
    pub decoupling_max: f64,
}

/// One face of the board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SummaryFace {
    /// `top` or `bottom`.
    pub face: String,
    pub parts: u32,
    /// The area a courtyard may take, in mm2.
    pub usable: f64,
    /// The area the parts' courtyards take, in mm2.
    pub courtyards: f64,
    /// `courtyards` over `usable`.
    pub fill: f64,
    /// The usable area no part takes, in mm2.
    pub free: f64,
    /// The side of a cell of the grid the areas are measured on, in mm.
    pub cell: f64,
    /// The largest empty rectangles, largest first.
    pub largest_free: Vec<SummaryBox>,
}

/// One module instance with two parts or more.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SummaryModule {
    pub path: String,
    /// The parts' designators, in board order.
    pub parts: Vec<String>,
    /// The box round the parts' courtyards.
    #[serde(rename = "box")]
    pub bounds: SummaryBox,
    pub box_area: f64,
    pub courtyards: f64,
    /// The half perimeter of the box round the parts' courtyard centres, in mm.
    pub spread: f64,
    /// The parts of other modules whose courtyard centre is inside that box, by designator.
    pub intruders: Vec<String>,
}

/// A decoupling capacitor and the part it decouples.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SummaryDecoupling {
    pub cap: String,
    pub cap_path: String,
    pub part: String,
    /// The mean distance of the capacitor's pads from the part's nearest pin pad on their net, in mm.
    pub distance: f64,
}

/// What one guideline of a part's `layout` block measures on the board (issue #994).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SummaryGuideline {
    /// The path of the part that states it, and the guideline's name.
    pub part: String,
    pub name: String,
    /// `near`, `away`, `loop`, `edge`, `vias` or `route`.
    pub statement: String,
    /// The designator of the part found, for `near`, `away` and `loop`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub other: Option<String>,
    /// In mm (a count of vias for `vias`); none when a pad to measure is not on the footprint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub measured: Option<f64>,
    /// The most the datasheet allows, or the least it asks for; neither for "close to".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at_most: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at_least: Option<f64>,
    /// Whether the measure meets the number; none when there is no number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub met: Option<bool>,
    /// The document and section the part's block cites.
    pub source: String,
}

/// One ratsnest line: the pads, `R1.2`, and its length in mm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SummaryLine {
    pub from: String,
    pub to: String,
    pub length: f64,
}

/// One net with two pads or more.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SummaryNet {
    pub name: String,
    pub pads: u32,
    pub hpwl: f64,
    pub crossings: u32,
    /// Its longest ratsnest line; left out for a net all of whose lines go to its plane.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longest: Option<SummaryLine>,
}

/// A part near another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SummaryNeighbour {
    pub part: String,
    pub path: String,
    pub face: String,
    /// The space between the two courtyards, in mm: 0 where they touch or overlap.
    pub gap: f64,
}

/// The parts near one part, nearest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SummaryNeighbours {
    pub part: String,
    pub path: String,
    pub face: String,
    pub within: f64,
    pub near: Vec<SummaryNeighbour>,
}

/// The distance between two pads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SummaryPads {
    /// The pads, `R1.2`.
    pub from: String,
    pub to: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_net: Option<String>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_net: Option<String>,
    /// Between the pads' centres, in mm.
    pub distance: f64,
    /// From the first pad to the second, in mm.
    pub dx: f64,
    pub dy: f64,
    /// Between the pads' copper, in mm: 0 where they touch.
    pub gap: f64,
}

/// The placed board summed up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbSummary {
    pub board: String,
    /// The layout stamp of the placement this is of, as `PcbScene.layoutStamp` has it.
    pub layout_stamp: String,
    pub width: f64,
    pub height: f64,
    /// The space the placer keeps between a courtyard and the edge, in mm.
    pub edge_wall: f64,
    pub parts: u32,
    pub totals: SummaryTotals,
    /// The top face, then the bottom.
    pub faces: Vec<SummaryFace>,
    pub modules: Vec<SummaryModule>,
    pub decoupling: Vec<SummaryDecoupling>,
    pub nets: Vec<SummaryNet>,
    /// What each guideline of the parts' `layout` blocks measures, by part and guideline.
    #[serde(default)]
    pub guidelines: Vec<SummaryGuideline>,
    /// The parts near each `near` part of the request.
    #[serde(default)]
    pub neighbours: Vec<SummaryNeighbours>,
    /// The distance of each `between` pair of the request.
    #[serde(default)]
    pub pads: Vec<SummaryPads>,
    /// What the placement check finds, as `fab` says it.
    pub findings: Vec<String>,
}
