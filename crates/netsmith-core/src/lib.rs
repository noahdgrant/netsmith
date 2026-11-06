//! Tool-independent design model: parts and the nets that join their pins.
//!
//! This is the seam between a future language front-end and the exporters.
//! It deliberately knows nothing about KiCad.

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub refdes: String,
    /// The library definition it was made from: `Resistor`, `Led`. An exporter decides
    /// what each kind looks like in its own tool.
    pub kind: String,
    /// What a schematic shows as the part's value: `300`.
    pub value: String,
    /// The package the part was chosen in, such as `0402`, if it was chosen.
    pub package: Option<String>,
    /// Where the part is in the design: `stage.output_cap`, the path of its instance from
    /// the board. It does not change when reference designators are renumbered, so it is
    /// what a layout file keys a part by (ADR 0030).
    pub path: String,
    /// The part's pins in the package it is in, in the order its pin table lists them. Empty
    /// for a part that has no pin table.
    pub pins: Vec<PartPin>,
}

/// What a pin is electrically. It is the datasheet's word for the pin, not a CAD tool's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinKind {
    Power,
    Input,
    Output,
    Bidirectional,
    Passive,
    NoConnect,
}

impl PinKind {
    /// The word for it in the language and in messages.
    pub fn word(self) -> &'static str {
        match self {
            Self::Power => "power",
            Self::Input => "input",
            Self::Output => "output",
            Self::Bidirectional => "bidirectional",
            Self::Passive => "passive",
            Self::NoConnect => "no_connect",
        }
    }
}

/// One pin of a part, as its pin table gives it. Which nets the pads are on is in
/// [`Design::nets`]; a pad that is on no net is a pad of a pin the board leaves open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartPin {
    /// Where the part declares it: `sense`, or `vout.pos` for a pin of an interface.
    pub name: String,
    /// The name the datasheet prints: `SENSE`.
    pub datasheet: String,
    pub kind: PinKind,
    /// The pads the pin is on, in the package. Several for a pin the package brings out
    /// more than once.
    pub pads: Vec<String>,
    /// The function the board chose for the pin, if it did.
    pub function: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinRef {
    pub refdes: String,
    /// Pin number as the symbol names it (`"1"`, `"A5"`).
    pub pin: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Net {
    pub name: String,
    pub pins: Vec<PinRef>,
}

/// The manufacturing process a board is made with, as the board's `process` member
/// resolved it: the least the process can make, after the board has narrowed it, and the
/// stackup. Lengths are in millimetres. Nothing here is specific to one manufacturer; a
/// different house is a different definition with the same fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Process {
    /// The definition it came from: `TwoLayer`.
    pub name: String,
    /// Who makes it: `JLCPCB`.
    pub fabricator: String,
    /// Where the numbers are published.
    pub source: String,
    /// The narrowest track.
    pub min_track: f64,
    /// The least space between copper of different nets.
    pub min_clearance: f64,
    /// The smallest drilled hole, which is a via's drill.
    pub min_drill: f64,
    /// The smallest copper disc of a via.
    pub min_via_diameter: f64,
    /// The least copper left round a drilled hole.
    pub min_annular_ring: f64,
    /// The least space between the edges of two holes.
    pub min_hole_to_hole: f64,
    /// The least space between copper and the board edge.
    pub min_edge_clearance: f64,
    /// The thinnest silkscreen stroke.
    pub min_silk_width: f64,
    /// The smallest silkscreen text.
    pub min_text_height: f64,
    /// The least space between silkscreen and a pad.
    pub min_silk_to_pad: f64,
    /// The least solder mask left between two pads.
    pub min_mask_bridge: f64,
    /// The largest via the fabricator fills and caps, so that it may sit in a pad. This is
    /// an upper bound: 0 where the board does not want vias in pads or the process cannot
    /// make them.
    pub via_in_pad: f64,
    /// The most a loaded track may heat above the board, in kelvin, if the board states it
    /// (`.process.max_temperature_rise`). `None` is the IPC-2221 check's default
    /// (ADR 0036).
    pub max_temperature_rise: Option<f64>,
    pub stackup: Stackup,
}

/// A panel (ADR 0040): one board repeated in a grid, cut apart by V-cuts, as a `panel`
/// definition states it. Lengths are in millimetres.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelSpec {
    /// The panel's name.
    pub name: String,
    /// The board it repeats, by the name of its definition.
    pub cell: String,
    /// Boards across the panel, and down it.
    pub columns: u32,
    pub rows: u32,
    /// The handling rail on all four sides, 0 for none.
    pub rail: f64,
    pub rules: PanelRules,
}

/// What the board house needs of a V-cut panel, as its `fab` member resolved it. Nothing here
/// is specific to one house; a different house is a different definition with the same
/// fields.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelRules {
    /// The definition it came from: `VCutPanel`.
    pub name: String,
    pub fabricator: String,
    pub source: String,
    pub min_rail: f64,
    pub min_size: f64,
    pub max_size: f64,
    pub max_assembled_size: f64,
    pub max_cuts: u32,
    pub min_cut_spacing: f64,
    pub advised_cut_spacing: f64,
    pub min_edge_clearance: f64,
    pub advised_edge_clearance: f64,
    pub min_thickness: f64,
    pub tooling_hole: f64,
    pub fiducial: f64,
    pub fiducial_offset: f64,
}

/// The thickness of one ounce of copper per square foot, in mm. It is a unit conversion,
/// not a figure from any manufacturer.
pub const OUNCE_OF_COPPER: f64 = 0.035;

/// What a board is built from.
#[derive(Debug, Clone, PartialEq)]
pub struct Stackup {
    /// Copper layers.
    pub layers: u32,
    /// The finished board's thickness in mm: the range the board still allows. It is one
    /// value once the board has chosen.
    pub thickness: (f64, f64),
    /// Finished outer copper, in ounces per square foot.
    pub copper_weight: f64,
    /// What the inner layers are, if the board has any (more than two copper layers).
    pub inner: Option<InnerLayers>,
}

/// The inner copper layers of a board of more than two layers: planes, and how the board is
/// built up round them. Lengths are in mm. From the top the board is outer copper, a
/// prepreg, the first inner layer, the core, the last inner layer, a prepreg and outer
/// copper; only that build of four layers is described so far.
#[derive(Debug, Clone, PartialEq)]
pub struct InnerLayers {
    /// The net each inner layer is a plane of, from the top: `["GND", "3V3"]` makes `In1.Cu`
    /// a ground plane and `In2.Cu` a 3.3 V plane.
    pub planes: Vec<String>,
    /// The copper of an inner layer.
    pub copper: f64,
    /// The prepreg between an outer layer and the inner layer beside it.
    pub prepreg: f64,
    /// The core between the inner layers.
    pub core: f64,
    /// Relative permittivity of the prepreg and of the core.
    pub prepreg_epsilon: f64,
    pub core_epsilon: f64,
    /// The least space between a plane and the hole of a via or pad that is not on its net.
    pub hole_clearance: f64,
}

impl Stackup {
    /// The thickness, if the board has chosen one.
    pub fn chosen_thickness(&self) -> Option<f64> {
        (self.thickness.0 == self.thickness.1).then_some(self.thickness.0)
    }
}

/// The board edge a design asks for: a rounded rectangle whose origin is its top left
/// corner. A size is the range the board allows, in mm: `(60.0, 60.0)` is exactly 60 mm,
/// `(0.0, 60.0)` at most 60 mm, and no member at all is `(0.0, f64::INFINITY)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    pub width: (f64, f64),
    pub height: (f64, f64),
    /// The radius of the four corners, 0 for square ones.
    pub corner_radius: f64,
}

/// A rectangular hole cut through the board.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct Pour {
    /// `top` or `bottom`.
    pub layer: String,
    pub net: String,
    /// The most space between stitching vias, if the design states one.
    pub stitch_pitch: Option<f64>,
}

/// An area where tracks, vias, copper or parts are not allowed.
#[derive(Debug, Clone, PartialEq)]
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
    /// (ADR 0045, "Height limits", PROVISIONAL). `None`: no height limit. A keepout that
    /// states a limit and none of `tracks`, `vias`, `copper` and `parts` forbids nothing else.
    pub max_height: Option<f64>,
}

/// How tall a part stands above the board, as the design states it (ADR 0045, "Height
/// limits", PROVISIONAL). A part the design does not state a height for is not here: its
/// height is unknown, and it is reported as such where a limit applies to it, never taken
/// as zero.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct PinProtrusion {
    /// The part's path in the design.
    pub path: String,
    /// In mm, from the face the pins come out of to the end of the cut pins.
    pub length: f64,
    /// The lines that state it (`widget.physical.nsm:41`), separated by commas. Filled by
    /// the caller that knows the source files; empty otherwise.
    pub source: String,
}

/// A line of silkscreen text: a `Label` part. The part's value is the text.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceBound {
    pub field: PlaceField,
    pub lo: f64,
    pub hi: f64,
    /// Where the design states it: `widget.nsm:14`, or several places, separated by commas.
    /// Empty until the caller that knows the source files fills it.
    pub source: String,
}

/// What the design requires of where a part or a module goes on the board (ADR 0045): the
/// narrowed fields of its `.place`. A field the design leaves alone is not here.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct Offset {
    pub a: OffsetEnd,
    pub b: OffsetEnd,
    pub lo: f64,
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

/// What a design says about the board beyond its parts and nets (ADR 0020). Everything is
/// optional; a design that says none of it is laid out as before.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Features {
    /// The narrowest track of the nets that have a rail with a `min_track`, in mm, by net
    /// name.
    pub net_widths: Vec<(String, f64)>,
    pub outline: Option<Outline>,
    pub cutouts: Vec<Cutout>,
    pub pours: Vec<Pour>,
    pub keepouts: Vec<Keepout>,
    /// The heights the design states for parts, by path (ADR 0045, "Height limits").
    pub heights: Vec<PartHeight>,
    /// How far the pins of through-hole parts protrude, as the design states it, by path
    /// (ADR 0045 section 15).
    pub protrusions: Vec<PinProtrusion>,
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

/// One member of an interface instance and the net it is on: `tx` on `UART0_TX`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceMember {
    /// The member's name in its interface: `tx`.
    pub name: String,
    /// The name of the net the member is on, one of [`Design::nets`].
    pub net: String,
}

/// An interface instance of the design and the nets its members are on, as the checker's
/// model has them: the instance a module or a part declares (`.uart0: Uart`), not a guess
/// from net names. A schematic reads it to draw the members of one interface as one bus.
/// Only the pins that are directly in the interface are members (a rail inside a `Usb2` is
/// an interface instance of its own), only members that are on a net of the design are
/// listed, and an instance with none is left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceInstance {
    /// Where the instance is in the design, the path of its instance from the board:
    /// `mcu.uart0`. It does not change when reference designators are renumbered.
    pub path: String,
    /// The interface definition it was made from: `Uart`.
    pub type_name: String,
    /// The members that are on nets, in the order the interface declares them.
    pub members: Vec<InterfaceMember>,
}

/// What a [`Note`] is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NoteKind {
    /// The text of the `///` (or `//!`) doc comments of a declaration or a definition.
    Doc,
    /// A `==` relation line of the source, as it is written.
    Relation,
}

/// Where in the source a [`Note`] is: a file as it was loaded and a 1-based line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteAt {
    pub file: String,
    pub line: usize,
}

/// A piece of the source's own writing that a drawing may carry onto the page: the doc comments
/// of a board, a module, a part or an interface, and the `==` relations of the design. Existing
/// source information only: nothing here is a new construct of the language, and an ordinary
/// `//` comment is never a note. A schematic draws a note as a framed text box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// The path of the instance the note belongs to, from the board like the paths of
    /// [`InterfaceInstance`]: the part, module or interface a doc comment is on, or the board
    /// or module whose body states a relation (`""` for the board itself).
    pub path: String,
    pub kind: NoteKind,
    /// The words. A doc comment's lines are joined with a space and a blank `///` line makes a
    /// line break (`\n`); a relation is the text of its line, as written.
    pub text: String,
    /// Where it is written, when the checker's model says.
    pub at: Option<NoteAt>,
    /// The paths of the parts the note is about, in the order the source mentions them: for a
    /// doc comment the declaration it is on (a part itself; an interface's parts, or none for a
    /// board or a module, whose note is the sheet's); for a relation the parts whose fields it
    /// names. A note with no anchors is the sheet's.
    pub anchors: Vec<String>,
    /// For a relation that gave a field its value, what the solver settled on
    /// (`led.current = 8.6mA`), when the model has it.
    pub result: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Design {
    pub name: String,
    pub parts: Vec<Part>,
    pub nets: Vec<Net>,
    /// How the board is to be made, if it says. A board with no process can still be drawn
    /// as a schematic, but not laid out.
    pub process: Option<Process>,
    pub features: Features,
    /// The interface instances whose members are on the design's nets, by path. Empty for a
    /// design that does not know them (a netlist read from a file, a test's small design).
    pub interfaces: Vec<InterfaceInstance>,
    /// The source's own writing the drawing may show: doc comments and `==` relations, in
    /// source order. Empty for a design that does not know them (a netlist read from a file, a
    /// test's small design).
    pub notes: Vec<Note>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DesignError {
    DuplicateRefdes(String),
    DuplicateNet(String),
    UnknownRefdes { net: String, refdes: String },
}

impl std::fmt::Display for DesignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateRefdes(r) => write!(f, "duplicate reference designator {r}"),
            Self::DuplicateNet(n) => write!(f, "duplicate net {n}"),
            Self::UnknownRefdes { net, refdes } => {
                write!(f, "net {net} refers to unknown part {refdes}")
            }
        }
    }
}

impl std::error::Error for DesignError {}

impl Design {
    pub fn validate(&self) -> Result<(), DesignError> {
        let mut refdes = HashSet::new();
        for p in &self.parts {
            if !refdes.insert(p.refdes.as_str()) {
                return Err(DesignError::DuplicateRefdes(p.refdes.clone()));
            }
        }
        let mut nets = HashSet::new();
        for n in &self.nets {
            if !nets.insert(n.name.as_str()) {
                return Err(DesignError::DuplicateNet(n.name.clone()));
            }
            for pin in &n.pins {
                if !refdes.contains(pin.refdes.as_str()) {
                    return Err(DesignError::UnknownRefdes {
                        net: n.name.clone(),
                        refdes: pin.refdes.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(refdes: &str) -> Part {
        Part {
            refdes: refdes.into(),
            kind: "Resistor".into(),
            value: "1k".into(),
            package: None,
            path: String::new(),
            pins: Vec::new(),
        }
    }

    #[test]
    fn rejects_duplicate_refdes() {
        let d = Design {
            name: "t".into(),
            parts: vec![part("R1"), part("R1")],
            nets: vec![],
            process: None,
            features: Features::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
        };
        assert_eq!(d.validate(), Err(DesignError::DuplicateRefdes("R1".into())));
    }

    #[test]
    fn rejects_net_with_unknown_part() {
        let d = Design {
            name: "t".into(),
            parts: vec![part("R1")],
            nets: vec![Net {
                name: "A".into(),
                pins: vec![PinRef {
                    refdes: "R9".into(),
                    pin: "1".into(),
                }],
            }],
            process: None,
            features: Features::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
        };
        assert!(matches!(
            d.validate(),
            Err(DesignError::UnknownRefdes { .. })
        ));
    }
}
