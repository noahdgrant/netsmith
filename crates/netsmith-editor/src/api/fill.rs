//! How the fill of the board's pours joins each pad (issue #1187): what `netsmith board fill`
//! prints, as the page reads it. The values are `netsmith_pcb::FillReport`'s, so the page and
//! the command line cannot differ; their field names are camelCase here and nothing else differs,
//! but a fill's copper (`FillArea::outline`) is left out: the 2D view draws it from
//! `GET /api/pcb/pours`.
//!
//! **`GET /api/pcb/fill?board=`** fills the pours, planes and zones of the board as the PCB view
//! has it placed (and routed), as the Gerbers fill them, and answers for each fill the pads of
//! its net and how each is joined (`solid`, `thermal` with the spokes that reach the fill, or
//! `none`), the counts, the pads of other nets it keeps clear of, its copper islands kept and
//! removed (a removed island with its outline, which the 2D view draws, issue #1188), the
//! starved thermal reliefs of the board, and the ground pads of its switching regulators that a
//! fill does not join solid (issue #1586). The places are in mm in the frame of the board file,
//! y down; `outlineAt` is the outline's top left corner there. `404` for a board that is not
//! there; a board that cannot be filled is a `500` with the reason.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A place in mm in the frame of the board file, y down.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct FillAt {
    pub x: f64,
    pub y: f64,
}

/// A pad of the fill's net, and how the fill joins it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FillPad {
    pub part: String,
    pub pad: String,
    pub at: FillAt,
    /// `solid`, `thermal` or `none`.
    pub join: String,
    /// Of a thermal relief: its spokes, and how many of them reach the fill.
    pub spokes: Option<u32>,
    pub joined_spokes: Option<u32>,
    /// The tracks of its net that run from it onto the fill.
    pub tracks: u32,
    /// Whether the fill reaches it at all.
    pub joined: bool,
    /// Whether it is a starved thermal relief (`starved_thermal`).
    pub starved: bool,
    /// Of a thermal relief that no spoke and no track joins (`unjoinable_thermal`, issue 1222):
    /// why the fill makes none of its spokes.
    #[serde(default)]
    pub unjoinable: Option<String>,
    /// Of a thermal relief that no spoke and no track joins to this fill: whether another fill
    /// of its net joins the pad, which makes it not unjoinable (issue 1292).
    #[serde(default)]
    pub joined_elsewhere: bool,
    /// The path of the pin whose own `.pour_connection` or `.spoke_width` joins the pad, over
    /// what the pour says (issue 1276), when one does (`radio.gnd`).
    #[serde(default)]
    pub stated_by: Option<String>,
    /// The width of the spokes that pin states, in mm, if it states one.
    #[serde(default)]
    pub spoke_width: Option<f64>,
}

/// A pad of another net in the fill's outline, which the fill keeps clear of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FillCleared {
    pub part: String,
    pub pad: String,
    pub net: Option<String>,
    pub at: FillAt,
}

/// A copper island: a corner of it and its area in mm2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FillIsland {
    pub at: FillAt,
    pub area: f64,
    /// Of an island the fill took out: its rings, the outline first and then its holes, which
    /// the 2D view outlines (issue 1188). Empty for an island kept, which is drawn as the fill.
    #[serde(default)]
    pub outline: Vec<Vec<FillAt>>,
}

/// How many pads are joined each way.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FillCounts {
    pub no_spokes: u32,
    pub one_spoke: u32,
    pub two_or_more_spokes: u32,
    pub solid: u32,
    pub none: u32,
    pub starved: u32,
    #[serde(default)]
    pub unjoinable: u32,
}

/// The fill of one pour, plane or zone on one layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FillArea {
    /// `top`, `inner1`, `bottom`, and KiCad's name of it.
    pub layer: String,
    pub layer_name: String,
    pub net: String,
    /// `plane`, `pour` or `zone`.
    pub source: String,
    /// Whether it is the fill the board file holds.
    pub held: bool,
    /// Its area in mm2.
    pub area: f64,
    pub pads: Vec<FillPad>,
    pub counts: FillCounts,
    pub cleared: Vec<FillCleared>,
    pub islands: Vec<FillIsland>,
    pub removed_islands: Vec<FillIsland>,
}

/// A starved thermal relief.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FillStarved {
    pub layer: String,
    pub net: String,
    pub part: String,
    pub pad: String,
    pub at: FillAt,
    pub spokes: u32,
    pub joined_spokes: u32,
    pub tracks: u32,
}

/// A thermal relief that the fill joins by nothing (`unjoinable_thermal`, issue 1222), and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FillUnjoinable {
    pub layer: String,
    pub net: String,
    pub part: String,
    pub pad: String,
    pub at: FillAt,
    pub spokes: u32,
    pub why: String,
}

/// A switching regulator's ground pad that no fill of its net on its layer joins with solid
/// copper (issues #1586 and #1666), a warning of `board fill`: `netsmith_pcb::RegulatorGround`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FillRegulatorGround {
    /// The regulator IC's path in the design (`reg.ic`).
    pub path: String,
    pub layer: String,
    pub net: String,
    pub part: String,
    pub pad: String,
    pub at: FillAt,
    /// `thermal` or `none` (or `solid` where the fill does not reach it).
    pub join: String,
    /// Of a thermal relief: its spokes, and how many of them reach the fill.
    #[serde(default)]
    pub spokes: Option<u32>,
    #[serde(default)]
    pub joined_spokes: Option<u32>,
    /// Whether it is also a starved thermal relief.
    pub starved: bool,
    /// The path of the pin that states the pad's connection, when one does (`reg.ic.vin.neg`).
    #[serde(default)]
    pub stated_by: Option<String>,
    /// The path of the IC's pin that has the pad (`reg.ic.gnd2`), whose `.pour_connection` the
    /// fill takes for it (issue #1667), and the pads of the part that its setting reaches.
    #[serde(default)]
    pub pin: String,
    #[serde(default)]
    pub pin_pads: Vec<String>,
    /// Each fill of the net on the layer that holds the pad; none joins it solid.
    #[serde(default)]
    pub fills: Vec<FillRegulatorFill>,
    /// The warning in words after the pad and its place, as `board fill` prints it.
    #[serde(default)]
    pub warning: String,
}

/// How one fill holds a regulator's ground pad: `netsmith_pcb::RegulatorFill`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FillRegulatorFill {
    /// `pour`, `plane` or `zone`.
    pub source: String,
    /// `thermal`, `none`, or `solid` where the fill does not reach it.
    pub join: String,
    pub joined: bool,
    #[serde(default)]
    pub spokes: Option<u32>,
    #[serde(default)]
    pub joined_spokes: Option<u32>,
    pub starved: bool,
}

/// How the fill of the board's pours joins its pads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbFill {
    pub board: String,
    /// The stamps of the placement and of the routes file it was filled on, and whether it was
    /// routed.
    pub layout_stamp: String,
    pub routes_stamp: String,
    pub routed: bool,
    pub outline_at: FillAt,
    /// Each fill, by layer from the top.
    pub fills: Vec<FillArea>,
    pub counts: FillCounts,
    pub starved: Vec<FillStarved>,
    /// The thermal reliefs the fill joins by nothing, which the 2D view rings as it rings the
    /// starved ones.
    #[serde(default)]
    pub unjoinable: Vec<FillUnjoinable>,
    /// The switching regulators' ground pads a fill holds and does not join solid.
    #[serde(default)]
    pub regulator_grounds: Vec<FillRegulatorGround>,
}
