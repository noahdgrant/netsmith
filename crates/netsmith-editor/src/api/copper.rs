//! The board's copper areas as data (issue #1085): what `netsmith board copper` prints, as the
//! page reads it. The values are `netsmith_pcb::CopperAreas`', so the page and the command line
//! cannot differ; their field names are camelCase here and nothing else differs.
//!
//! **`GET /api/pcb/copper?board=`** answers from the source and the routes file, with no placing
//! and no routing: every copper layer from the top with its use (`plane`, `pour` or `signal`), its
//! net and a pour's fields as the design states them; the zones of the routes file, counted apart
//! from the process's pours; and the teardrop fills, counted (`counted`) only when the board has
//! been routed in this session. `404` for a board that is not there; it waits for the first
//! analysis like the other views and answers `503` to a request that sends
//! `X-Netsmith-Pending: 1`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The fields of a pour of the process, as the design states them, in mm; null where it does
/// not state one (the process's default).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CopperPour {
    pub stitch_pitch: Option<f64>,
    pub thermal_gap: Option<f64>,
    pub spoke_width: Option<f64>,
    pub clearance: Option<f64>,
    pub min_width: Option<f64>,
    /// `solid`, `thermal`, `none` or `thru_hole_only`; null for the default, a thermal relief.
    pub pads: Option<String>,
}

/// One copper layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CopperLayer {
    /// `top`, `inner1`, `bottom`: the routes file's word for it.
    pub layer: String,
    /// `plane`, `pour` or `signal`.
    pub kind: String,
    /// The net of a plane or a pour.
    pub net: Option<String>,
    pub pour: Option<CopperPour>,
    /// How many zones of the routes file are on the layer.
    pub zones: u32,
    /// Whether a zone of the plane's or the pour's own net replaces it: the design's copper on
    /// the layer is then not drawn. Zones of other nets cut it and leave it drawn round them
    /// (issue 1314), and a patch of its own net (issue 1608) keeps it.
    pub replaced_by_zones: bool,
    /// Whether a patch of the plane's or the pour's own net is on the layer (issue 1608): the
    /// copper is drawn, and the patch holds its settings inside its outline.
    pub patched_by_zones: bool,
}

/// A zone of the routes file, counted apart from the process's pours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CopperZone {
    pub net: String,
    pub layers: Vec<String>,
    pub priority: u32,
    pub clearance: f64,
    pub min_width: f64,
    pub pads: String,
    pub thermal_gap: f64,
    pub spoke_width: f64,
    /// `remove`, `keep` or `remove_below`.
    pub islands: String,
    /// The outline's area less its holes, in mm2, on one layer.
    pub area: f64,
    pub locked: bool,
    /// A patch of the plane or pour of its net (issue 1608): the pour is kept, and the zone's
    /// settings hold inside its outline alone.
    pub patch: bool,
}

/// The teardrop fills of a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CopperTeardrops {
    pub layer: String,
    pub count: u32,
}

/// The teardrop fills of the routed board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CopperFills {
    /// False when the board has not been routed: the source holds none.
    pub counted: bool,
    pub total: u32,
    pub layers: Vec<CopperTeardrops>,
}

/// The copper areas of a board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PcbCopper {
    pub board: String,
    /// Every copper layer, from the top.
    pub layers: Vec<CopperLayer>,
    /// The zones of the routes file.
    pub zones: Vec<CopperZone>,
    pub teardrops: CopperFills,
}
