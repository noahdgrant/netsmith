//! Where one part fits, as data (issue #1002): what `netsmith layout fit` says, as the page
//! reads it. The places and distances are `netsmith_pcb::PartFit`'s, so the page and the command
//! line agree on what fits.
//!
//! **`GET /api/pcb/fit?board=&part=`** asks where the part `part` (a path in the design, or a
//! designator) fits on the board as it is placed now (the placement the scene draws, routed or
//! not: nothing is placed for the question when the scene has been drawn), with every other part
//! where it is. `near` is a part (path or designator: the centre of its courtyard) or a point
//! `X,Y` in mm from the outline's top left corner, y down; with none, where the part is now.
//! `step` (mm, above 0; 0.5 by default) is the grid of origins tried; `side` (`top` or `bottom`)
//! keeps to one face. Nothing is written: a place found is written with `POST /api/pcb/edit`
//! (a `part` change to the `PartPlacement` with this `at`, `turn` as `rotation` and `side`), as a
//! drag writes one.
//!
//! The answer has a pose for each turn (the four right angles from the part's own) and face the
//! part may take: the nearest place to `near` where nothing is found (`fits`), or, when there is
//! none, the closest try with what is in the way there. The poses that fit come first, nearest
//! first. Places are footprint origins in the layout file's frame, as the layout file's `at`.
//! `404` for a board that is not there, `400` with the reason for a part the board has none of, a
//! `near` that is neither a part nor `X,Y`, a `step` that is not above 0 and a `side` that is
//! neither `top` nor `bottom`; it waits for the first analysis like the other views and answers
//! `503` to a request that sends `X-Netsmith-Pending: 1`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A part: its designator and its path in the design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbFitPart {
    pub refdes: String,
    pub path: String,
}

/// What the distances are measured from, in mm in the layout file's frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbFitNear {
    pub x: f64,
    pub y: f64,
    /// The part, when `near` named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub part: Option<PcbFitPart>,
}

/// One fault the placement check finds with the part in a place: the check's name for it
/// (`overlap`, `outside_outline`, `in_keepout`, `hole_too_near`, ...; the editor's rule names)
/// and the designators of the parts it is about.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbFitFault {
    pub kind: String,
    pub parts: Vec<String>,
}

/// One requirement of the physical file the part does not meet in a place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbFitMiss {
    /// What the design calls it: `power_in.place.left`.
    pub id: String,
    /// The line of the physical file that states it, when the design knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
    /// The designators of the parts it is about.
    pub parts: Vec<String>,
}

/// The part in one place: where, how far from `near`, and what is found there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbFitPlace {
    /// Degrees, counterclockwise on screen.
    pub turn: f64,
    /// `top` or `bottom`.
    pub side: String,
    /// The footprint's origin, as the layout file's `at`.
    pub at: [f64; 2],
    /// From `near` to the centre of the part's courtyard, in mm.
    pub distance: f64,
    /// Whether nothing is found there.
    pub fits: bool,
    /// The placement check's faults about the part there.
    pub placement: Vec<PcbFitFault>,
    /// The requirements the part does not meet there.
    pub requirements: Vec<PcbFitMiss>,
    /// The sentence for each of them, as the command line prints them.
    pub messages: Vec<String>,
}

/// `GET /api/pcb/fit`: where one part fits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbFit {
    /// The board asked for, as `BoardSummary.name` has it.
    pub board: String,
    /// The stamp of the layout file the placement was made with, as `PcbScene` has it.
    pub layout_stamp: String,
    pub part: PcbFitPart,
    pub near: PcbFitNear,
    /// The grid the origins were tried on, in mm.
    pub step: f64,
    /// The part where it is now.
    pub now: PcbFitPlace,
    /// A pose for each turn and face; those that fit first, nearest first.
    pub poses: Vec<PcbFitPlace>,
}
