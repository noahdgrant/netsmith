//! The 3D view of an open page, as an agent drives it (P8a of issue 430; docs/BOARD3D.md, "Driving
//! the editor's 3D view"). The names are `netsmith view 3d`'s: a view, a section, highlights, x-ray,
//! a lens; with the editor's own tools beside them (what the bodies are coloured by, the height
//! preview, the stackup pulled apart, the selection).
//!
//! Points are in the scene's frame of docs/BOARD3D.md: millimetres, z up, the origin at the
//! outline's bottom-left corner on the board's top face.
//!
//! **Nothing here is the design.** A change moves the camera, cuts the picture, lights parts and
//! picks them in the page's shared selection; no file is written and no part moves. The server
//! reads only the shape of a change (`deny_unknown_fields`, so a misspelt name is an error); the
//! page decides whether the ids and the section are the board's, as `view 3d` does, since only the
//! page has the scene it shows.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A view of `netsmith view 3d --view`: the camera looks at the board's middle from that side,
/// framed to fit it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum RenderView {
    Top,
    Bottom,
    Front,
    Back,
    Left,
    Right,
    Iso,
    IsoBottom,
}

/// What the bodies are coloured by: where each height came from (the default), the part's kind,
/// or its margin to the height limit over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ColourBy {
    Provenance,
    Kind,
    Margin,
}

/// How the case model (ADR 0109) is drawn, as `netsmith view 3d --case` names it: not at all, see-through
/// (the default), or solid, which hides the board inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum CaseMode {
    Hidden,
    SeeThrough,
    Solid,
}

/// The camera as a place: where the eye is and the point it looks at, in the scene's frame. `up`
/// is the screen's up, in the same frame; a change does not take it (the view keeps z up).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CameraPlace {
    pub position: [f64; 3],
    pub target: [f64; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub up: Option<[f64; 3]>,
}

/// The height preview of a change: `true` or `false` turns it on or off at the height it has, a
/// number turns it on at that many mm (0.1 to 50).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum PreviewSetting {
    On(bool),
    Mm(f64),
}

/// `POST /api/view`: what to change, every field optional. A change opens the PCB section in 3D
/// on `board` (or on the board the page shows), applies the rest, and is answered with the
/// [`ViewState`] once the page has drawn it. One that the page cannot make changes nothing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewChange {
    /// The board, as `BoardSummary::name` has it (`Demo/main` for a board of a system), or a
    /// system's name (`Demo`), which shows its System view (ADR 0110): every member placed by
    /// its mates. The page's board is then a member of it: the one it is on, else the first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub board: Option<String>,
    /// A view of `view 3d`; with `camera` too, an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub view: Option<RenderView>,
    /// The camera put at a place (its `up` is not taken).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub camera: Option<CameraPlace>,
    /// `x=13`, `y=8` or `z=-0.8` as `view 3d --section` reads it, or `none` for no cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub section: Option<String>,
    /// What is lit, as `view 3d --highlight` names it (a designator, a path, `net:<name>`,
    /// `limit:<name>`, `case:<name>`, `module:<path>`; an item may be comma-separated): the parts
    /// and the net through the page's selection, the limits and the case models in the view, the
    /// modules in the 2D and 3D views, each in its own colour in the order given (issue 1129).
    /// It replaces what was lit; `[]` lights nothing. In the System view a part, a net and a
    /// module are a member's, `led/J1`, `net:led/GND` (`led/net:GND` too) and `module:led/reg`;
    /// one of another member makes it the page's board; a case model is one board's, so `case:`
    /// there is refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub highlight: Option<Vec<String>>,
    /// The page's selection, as a click makes it: parts by designator or path, one `net:<name>`,
    /// or one `module:<path>`, which is lit too, as a module picked in the schematic is (in the
    /// System view, `led/J1` and `net:led/GND`, of one member). The limits and the case models
    /// lit stay as they are; `[]` clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub select: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub xray: Option<bool>,
    /// `true` for the parallel projection; `perspective: true` is the same as `ortho: false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ortho: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub perspective: Option<bool>,
    /// How the case model is drawn. A board with no case model refuses it (409).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub case: Option<CaseMode>,
    /// The case models hidden one by one, by name (`view 3d --case hidden:<name>[,<name>]`),
    /// whatever `case` says; it replaces the list, and `[]` shows every model. A model lit by
    /// `highlight` is shown. A board with no case model refuses it (409), and a name it has no
    /// case model of is refused (400) with the list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub case_hidden: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub colour_by: Option<ColourBy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub height_preview: Option<PreviewSetting>,
    /// The stackup pulled apart (`true`) or together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exploded: Option<bool>,
    /// Refused by the server with the reason: the theme is the person's setting for the whole
    /// editor, not a view of the board. Read here so that the answer says so.
    #[serde(default, skip_serializing)]
    #[ts(skip)]
    pub theme: Option<serde_json::Value>,
}

/// A module lit in the PCB views (issue 1129): its path in the design, the colour it is drawn
/// in under the page's theme, and its parts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LitModule {
    /// As `--highlight module:<path>` names it (`reg`, `radio.row_a`).
    pub path: String,
    /// `#rrggbb`.
    pub colour: String,
    /// The designators of its parts.
    pub parts: Vec<String>,
}

/// `GET /api/view` and the answer to a change: what the page shows now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ViewState {
    /// The board the PCB section is on, or the system whose System view the 3D view shows; none
    /// when the project has no board.
    pub board: Option<String>,
    /// In the System view, the member the page's board is (`led`): its parts and nets are named
    /// `led/J1` and `net:led/GND`, as every member's are. Absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub member: Option<String>,
    /// The 3D view of `board` is on screen and drawn. When false, `view`, `camera`, `section`,
    /// `exploded` and `ortho` are absent.
    pub shown: bool,
    /// The view of `view 3d` the camera is at (within a few degrees), else none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub view: Option<RenderView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub camera: Option<CameraPlace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ortho: Option<bool>,
    /// The cut as `view 3d --section` writes it (`x=13`), or none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub section: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exploded: Option<bool>,
    /// What is lit, as `view 3d --highlight` would name it: the parts picked by designator, the
    /// net picked as `net:<name>`, the limits lit as `limit:<name>`, the case models lit as
    /// `case:<name>`, the modules lit as `module:<path>` (in the System view, the member first:
    /// `led/J1`, `net:led/GND`, `module:led/reg`).
    pub highlight: Vec<String>,
    /// The modules lit in the 2D and 3D views (issue 1129), in the order of their colours.
    #[serde(default)]
    pub modules: Vec<LitModule>,
    /// The page's selection: the parts by designator, one `net:<name>`, or one `module:<path>`.
    pub selection: Vec<String>,
    /// What the pointer (or a row of another view) lights now: a designator or `net:<name>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hover: Option<String>,
    pub xray: bool,
    /// How the case model is drawn; absent when the board has none (or the view is not shown).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub case: Option<CaseMode>,
    /// The case models hidden one by one, by name; absent when none is (or the board has no case
    /// model, or the view is not shown).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub case_hidden: Option<Vec<String>>,
    pub colour_by: ColourBy,
    /// The height preview's height in mm while it is on, else none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub height_preview: Option<f64>,
    /// The page's palette, as `view 3d --theme` names one: `light` or `dark`. Read-only.
    pub theme: String,
    /// The `netsmith view 3d` command that draws what the view shows, as "Copy as command" has
    /// it; absent when the view is not shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub command: Option<String>,
}
