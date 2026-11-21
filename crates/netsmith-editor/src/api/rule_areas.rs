//! The rule areas of a board (issue 1543): what `netsmith view pcb` hatches, as the page reads
//! it. They are the `rule_areas` of `netsmith board rules` (`netsmith_geom::DrcBoardRules`),
//! less the keepouts of the physical file, which the scene draws as keepouts already
//! (`netsmith_fabricate::drc::rule_areas_to_draw`).
//!
//! **`GET /api/pcb/rule-areas?board=`** answers, of the board as the PCB view has it placed (and
//! routed), the footprints' rule areas (a library's keep-out, an RF module's antenna area) and
//! the board's that are not keepouts of the physical file, each with its copper layers, its
//! outlines and holes, and the kinds it forbids. The places are in mm in the frame of the board
//! file, y down, as the scene's. `404` for a board that is not there; a board that cannot be
//! placed is a `500` with the reason.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::PcbPoint;

/// A rule area: where it is, on which copper layers, and what it forbids there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuleArea {
    pub name: String,
    /// The part whose footprint carries it; `None` for the board's. A footprint's area does not
    /// forbid the footprint itself.
    pub part: Option<String>,
    /// KiCad's names of its copper layers (`F.Cu`, `In1.Cu`, `B.Cu`).
    pub layers: Vec<String>,
    pub outlines: Vec<Vec<PcbPoint>>,
    pub holes: Vec<Vec<PcbPoint>>,
    /// The kinds it forbids: `tracks`, `vias`, `pads`, `copperpour`, `footprints`.
    pub forbids: Vec<String>,
}

/// The rule areas of a board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbRuleAreas {
    pub board: String,
    /// The stamps of the placement and of the routes file they were found on, and whether the
    /// board was routed.
    pub layout_stamp: String,
    pub routes_stamp: String,
    pub routed: bool,
    pub areas: Vec<RuleArea>,
}
