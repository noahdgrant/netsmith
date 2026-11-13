//! What is on a board: outline, cutouts, pours, keepouts, heights, models, labels and
//! placement requests.

use crate::design_file;

/// The board edge a design asks for: a rounded rectangle whose origin is its top left
/// corner. A size is the range the board allows, in mm: `(60.0, 60.0)` is exactly 60 mm,
/// `(0.0, 60.0)` at most 60 mm, and no member at all is `(0.0, f64::INFINITY)`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Outline {
    #[serde(with = "design_file::range")]
    pub width: (f64, f64),
    #[serde(with = "design_file::range")]
    pub height: (f64, f64),
    /// The radius of the four corners, 0 for square ones.
    pub corner_radius: f64,
    /// The edge when it is not a rectangle with four equal corners (issue #57): its corners
    /// in order, in mm in the board's frame, the top left of their bounding box at the
    /// origin. The width and the height are then that box's, exactly, and `corner_radius`
    /// is 0. `None` for the rounded rectangle.
    pub polygon: Option<Vec<OutlineVertex>>,
}

/// A corner of a polygon outline and the edge from it to the next corner (the last's edge
/// runs to the first).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OutlineVertex {
    pub at: (f64, f64),
    /// A point the edge to the next corner passes through when that edge is an arc (the
    /// three points name one circle, as KiCad states an arc); `None` for a straight edge.
    pub through: Option<(f64, f64)>,
}

/// A rectangular hole cut through the board.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Cutout {
    /// The member's path in the board.
    pub name: String,
    /// The corner nearest the board's origin, if the design states it. Otherwise the layout
    /// file does.
    pub at: Option<(f64, f64)>,
    pub size: (f64, f64),
    pub corner_radius: f64,
}

/// An outer copper layer poured on a net.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pour {
    /// `top` or `bottom`.
    pub layer: String,
    pub net: String,
    /// The most space between stitching vias, if the design states one.
    pub stitch_pitch: Option<f64>,
}

/// An area where tracks, vias, copper or parts are not allowed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Keepout {
    /// The member's path: `antenna_clear`, or `radio.keepout` for one carried by a part.
    pub name: String,
    /// The path of the part that carries it. Then `at` is in the part's frame.
    pub part: Option<String>,
    /// The corner nearest the origin of the board, or of the part. A board's keepout may
    /// leave it to the layout file.
    pub at: Option<(f64, f64)>,
    pub size: (f64, f64),
    /// The copper layers it applies to: `top`, `bottom`, `inner1`, `inner2`.
    pub layers: Vec<String>,
    pub tracks: bool,
    pub vias: bool,
    pub copper: bool,
    pub parts: bool,
    /// The tallest a part may stand where the keepout is, in mm, on the faces in `layers`
    /// (ADR 0045, "Height limits"). `None`: no height limit. A keepout that
    /// states a limit and none of `tracks`, `vias`, `copper` and `parts` forbids nothing else.
    pub max_height: Option<f64>,
}

/// How tall a part stands above the board, as the design states it (ADR 0045, "Height
/// limits"). A part the design does not state a height for is not here: its
/// height is unknown, and it is reported as such where a limit applies to it, never taken
/// as zero.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PartHeight {
    /// The part's path in the design.
    pub path: String,
    /// In mm, from the face of the board it is on to the top of its body.
    pub height: f64,
}

/// How far the pins of a through-hole part protrude out of the board on the face opposite the
/// part, whether they were cut after soldering or bought short, as the board's physical file
/// states it (`.pin_protrusion`, ADR 0045 section 15). A part the design states no
/// protrusion for is not here: its protrusion is unknown, which is not zero, and its pads
/// stay an obstacle for a part on that face.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PinProtrusion {
    /// The part's path in the design.
    pub path: String,
    /// In mm, from the face the pins come out of to the end of the cut pins.
    pub length: f64,
    /// The lines that state it (`widget.physical.nsm:41`), separated by commas. Filled by
    /// the caller that knows the source files; empty otherwise.
    pub source: String,
}

/// The STEP model a part is drawn from in place of its generated shape (`.model`, ADR 0108).
/// It is drawn, never measured: the part's height does not come from it. A part the design
/// names no model for is not here.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StepModel {
    /// The part's path in the design.
    pub path: String,
    /// The file as the design states it (`.model.file`), relative to the file that states it.
    pub file: String,
    /// The offset, in mm along x, y and z of the footprint's frame (`.model.x`, `.y`, `.z`).
    pub offset: [f64; 3],
    /// The rotation in degrees about x, then y, then z (`.model.rx`, `.ry`, `.rz`), applied
    /// before the offset.
    pub rotation: [f64; 3],
    /// The lines that state the file (`widget.nsm:12`), separated by commas. Filled by the
    /// caller that knows the source files; empty otherwise.
    pub source: String,
    /// The file of each statement of the file, as it was loaded (`src/usb.nsm`), for a caller
    /// that finds the model relative to it. Filled by the same caller; empty otherwise.
    pub stated_in: Vec<String>,
    /// The file on disk, inside the project. Filled by the caller that knows where the files
    /// that state it are; `None` otherwise, and when the file is not there, so that the part
    /// is drawn with its generated shape. A design file leaves it out: it is a path on this
    /// machine, and a reader finds the file again (`netsmith::models::locate`).
    #[serde(skip)]
    pub resolved: Option<std::path::PathBuf>,
}

/// A STEP file of the board's case that the board is drawn in and checked against (a
/// `CaseModel` of the physical file's `case { }` section, ADR 0109). It is a drawing and a
/// check, never a height, an outline or a hole.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CaseModel {
    /// The member's name in the board (`shell`).
    pub name: String,
    /// The file as the design states it (`.shell.file`), relative to the file that states it.
    pub file: String,
    /// The offset, in mm along x, y and z of the 3D scene's frame (`docs/BOARD3D.md`: z up,
    /// the origin at the outline's bottom-left corner on the top face).
    pub offset: [f64; 3],
    /// The rotation in degrees about x, then y, then z, applied before the offset.
    pub rotation: [f64; 3],
    /// The fit check's figures, in mm (`.clearance`, `.reach`, `.boss_gap`; std's defaults
    /// unless the board states them): the least gap from a body to a case surface, how far
    /// inside a wall's outer face a connector at the edge may stand, and how far from a
    /// mounting hole's ring a case solid may be and still hold it.
    pub figures: CaseFigures,
    /// The lines that state the file, separated by commas. Filled by the caller that knows the
    /// source files; empty otherwise.
    pub source: String,
    /// The file of each statement of the file, as it was loaded. Filled by the same caller;
    /// empty otherwise.
    pub stated_in: Vec<String>,
    /// The file on disk, inside the project. Filled by the caller that knows where the files
    /// are; `None` otherwise, and when the file is not there, so that the fit is not checked. A
    /// design file leaves it out, as it does [`StepModel::resolved`].
    #[serde(skip)]
    pub resolved: Option<std::path::PathBuf>,
}

/// The figures of a case model's fit check (ADR 0109), in mm.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CaseFigures {
    pub clearance: f64,
    pub reach: f64,
    pub boss_gap: f64,
}

impl Default for CaseFigures {
    /// std's defaults (`std::pcb::CaseModel`).
    fn default() -> CaseFigures {
        CaseFigures {
            clearance: 0.5,
            reach: 1.5,
            boss_gap: 0.5,
        }
    }
}

/// A line of silkscreen text: a `Label` part. The part's value is the text.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Label {
    /// The part's path in the design.
    pub path: String,
    pub text: String,
    /// The height of the letters the design asks for (`.text_height`), in mm, if it does.
    /// The process's least text height otherwise.
    pub text_height: Option<f64>,
}

/// A field of a part's or module's `.place` that holds a range (ADR 0045). All are measured
/// in the board's frame: the origin is the outline's top left corner, x grows to the right
/// and y downward, in millimetres; `x` and `y` are the centre of the part's body box and
/// `left`, `right`, `top` and `bottom` its edges, after the part's turn. `Rotation` is in
/// degrees, counterclockwise on screen, as the layout file has it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PlaceField {
    X,
    Y,
    Left,
    Right,
    Top,
    Bottom,
    Rotation,
}

impl PlaceField {
    pub const ALL: [PlaceField; 7] = [
        Self::X,
        Self::Y,
        Self::Left,
        Self::Right,
        Self::Top,
        Self::Bottom,
        Self::Rotation,
    ];

    /// The name of the member in the language.
    pub fn word(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Rotation => "rotation",
        }
    }

    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.word() == word)
    }

    /// The unit its numbers are in.
    pub fn unit(self) -> &'static str {
        if self == Self::Rotation {
            "degrees"
        } else {
            "mm"
        }
    }
}

/// One requirement on a part's or module's place: the range its field may take. A bound
/// with one side open has an infinite `lo` or `hi`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlaceBound {
    pub field: PlaceField,
    #[serde(with = "design_file::end")]
    pub lo: f64,
    #[serde(with = "design_file::end")]
    pub hi: f64,
    /// Where the design states it: `widget.nsm:14`, or several places, separated by commas.
    /// Empty until the caller that knows the source files fills it.
    pub source: String,
}

/// What the design requires of where a part or a module goes on the board (ADR 0045): the
/// narrowed fields of its `.place`. A field the design leaves alone is not here.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Placement {
    /// The path of the part or module in the design: `usb`, `sensor.header`.
    pub path: String,
    /// Whether it is a module, whose box is that of all the parts inside it. A module has no
    /// rotation and no side.
    pub module: bool,
    /// In the order of [`PlaceField::ALL`].
    pub bounds: Vec<PlaceBound>,
    /// `top` or `bottom`, if the design says, and where.
    pub side: Option<String>,
    pub side_source: String,
}

impl Placement {
    pub fn bound(&self, field: PlaceField) -> Option<&PlaceBound> {
        self.bounds.iter().find(|b| b.field == field)
    }
}

/// One end of an [`Offset`]: a field of the `.place` of a part, by the part's path.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OffsetEnd {
    /// The path of the part in the design: `radio.row_a`.
    pub path: String,
    /// The field of its place that is measured (never `rotation`).
    pub field: PlaceField,
}

/// A relation between the places of two parts (ADR 0045, section 3): the measure of `b`
/// minus the measure of `a` is in the window. `.radio.row_b.place.x - .radio.row_a.place.x ==
/// 17.5mm +/- 0.05mm` is `b` = `radio.row_b`, `a` = `radio.row_a`, a window of 17.45 to 17.55
/// mm. Both ends are measured along the same axis, in millimetres, in the frame of the
/// layout file.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Offset {
    pub a: OffsetEnd,
    pub b: OffsetEnd,
    #[serde(with = "design_file::end")]
    pub lo: f64,
    #[serde(with = "design_file::end")]
    pub hi: f64,
    /// Where the design states it: `widget.physical.nsm:14`. Empty until the caller that
    /// knows the source files fills it.
    pub source: String,
}

impl Offset {
    /// Whether the relation is along x (else along y).
    pub fn along_x(&self) -> bool {
        matches!(
            self.a.field,
            PlaceField::X | PlaceField::Left | PlaceField::Right
        )
    }
}
