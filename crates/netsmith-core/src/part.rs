//! Parts, their pins and the nets that join them.

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Part {
    pub refdes: String,
    /// The library definition it was made from: `Resistor`, `Led`. An exporter decides
    /// what each kind looks like in its own tool.
    pub kind: String,
    /// The standard generics the part's definition provides (`part X provides Resistor`, ADR
    /// 0068), by name. A tool that treats a kind of part in its own way (an inductor lies along the
    /// wire it is in, a capacitor decouples) reads it as well as `kind`, so that a provider
    /// with a name of its own is treated as the kind it provides. A generic of a project is
    /// not listed. Empty for a part that provides no standard generic.
    pub provides: Vec<String>,
    /// What the tools treat the part as, from the `#[class(...)]` of its definition or of a
    /// generic it provides (ADR 0123). `None` for a part whose definitions state none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<PartClass>,
    /// What a schematic shows as the part's value: `300`.
    pub value: String,
    /// The package the part was chosen in, such as `0402`, if it was chosen.
    pub package: Option<String>,
    /// Where the part is in the design: `stage.output_cap`, the path of its instance from
    /// the board. It does not change when reference designators are renumbered, so it is
    /// what a layout file keys a part by (ADR 0030). The capacitors of a bank of two or more
    /// are `stage.output_cap[0]`, `stage.output_cap[1]` (ADR 0052), as the members of `N x Kind`.
    pub path: String,
    /// The part's pins in the package it is in, in the order its pin table lists them. Empty
    /// for a part that has no pin table.
    pub pins: Vec<PartPin>,
}

impl Part {
    /// Whether the part is a connector, the way in and out of a board: its class is
    /// `connector` (ADR 0123). It is the class that says so, never the part's name or its
    /// reference designator, which is a label that counts parts and may change (issue #166).
    pub fn is_connector(&self) -> bool {
        self.class == Some(PartClass::Connector)
    }
}

/// What the tools treat a part as: the word of a definition's `#[class(...)]` (ADR 0123). A
/// tool that draws, places or routes a kind of part in its own way reads this, never the
/// name of the part's definition, so a project's own part is treated the same way when its
/// definition, or a generic it provides, states the class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartClass {
    Resistor,
    Capacitor,
    Inductor,
    FerriteBead,
    Fuse,
    Led,
    Diode,
    Transistor,
    Crystal,
    /// An integrated circuit: a regulator, a logic gate.
    Ic,
    Connector,
    MountingHole,
    PlatedHole,
    Fiducial,
    /// Text printed on the board (`std::parts::mechanical::Label`).
    Label,
}

impl PartClass {
    pub const ALL: [PartClass; 15] = [
        Self::Resistor,
        Self::Capacitor,
        Self::Inductor,
        Self::FerriteBead,
        Self::Fuse,
        Self::Led,
        Self::Diode,
        Self::Transistor,
        Self::Crystal,
        Self::Ic,
        Self::Connector,
        Self::MountingHole,
        Self::PlatedHole,
        Self::Fiducial,
        Self::Label,
    ];

    /// The word in `#[class(...)]` and in the design file: `ferrite_bead`.
    pub fn word(self) -> &'static str {
        match self {
            Self::Resistor => "resistor",
            Self::Capacitor => "capacitor",
            Self::Inductor => "inductor",
            Self::FerriteBead => "ferrite_bead",
            Self::Fuse => "fuse",
            Self::Led => "led",
            Self::Diode => "diode",
            Self::Transistor => "transistor",
            Self::Crystal => "crystal",
            Self::Ic => "ic",
            Self::Connector => "connector",
            Self::MountingHole => "mounting_hole",
            Self::PlatedHole => "plated_hole",
            Self::Fiducial => "fiducial",
            Self::Label => "label",
        }
    }

    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.word() == word)
    }

    /// Whether a part of the class hangs in series on the wire it is in rather than across it,
    /// as a schematic draws it: an inductor, a ferrite bead or a fuse.
    pub fn hangs_in_series(self) -> bool {
        matches!(self, Self::Inductor | Self::FerriteBead | Self::Fuse)
    }

    /// Whether a part of the class is a mechanical item that belongs in the corners of a fixed
    /// outline: a hole or a fiducial. A test point is on a net and is placed with the circuit.
    pub fn is_mechanical(self) -> bool {
        matches!(self, Self::MountingHole | Self::PlatedHole | Self::Fiducial)
    }

    /// Whether a part of the class is a hole that a case's screw may pass (a mounting hole or a
    /// plated hole).
    pub fn is_hole(self) -> bool {
        matches!(self, Self::MountingHole | Self::PlatedHole)
    }
}

/// What a pin is electrically. It is the datasheet's word for the pin, not a CAD tool's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinKind {
    Power,
    Input,
    Output,
    Bidirectional,
    Passive,
    NoConnect,
    TriState,
    /// An output that only pulls its line low or lets it go; a pull-up sets its high (#460).
    OpenDrain,
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
            Self::TriState => "tri_state",
            Self::OpenDrain => "open_drain",
        }
    }
}

/// Which way a power pin passes power, as the solved design says (ADR 0016: the sign of the
/// current of the rail on the pin, which a pin table cannot state because it is the board's
/// loads that decide it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerFlow {
    /// The pin takes power from the net: the current of its rail is drawn, at least 0 A and
    /// above 0 A at most.
    Takes,
    /// The pin gives power to the net: the current of its rail is negative, from below 0 A
    /// to at most 0 A.
    Gives,
}

impl PowerFlow {
    /// The word for it in messages and in the files of tools.
    pub fn word(self) -> &'static str {
        match self {
            Self::Takes => "takes",
            Self::Gives => "gives",
        }
    }
}

/// One pin of a part, as its pin table gives it. Which nets the pads are on is in
/// [`Design::nets`]; a pad that is on no net is a pad of a pin the board leaves open.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// Whether a `power` pin takes or gives power, when the solved design settles it: the sign
    /// of the current of the rail whose `pos` the pin is (or is joined to inside the part). It
    /// is `None` for a pin that is not a power pin, for a power pin that no rail's `pos` is
    /// (a ground), and for a rail whose current is not known or may take either sign. An
    /// exporter types an unknown power pin as one that takes power, which asks the net for a
    /// source and never claims one that may not be there ([`PartPin::gives_power`]).
    pub power_flow: Option<PowerFlow>,
}

impl PartPin {
    /// Whether the pin is known to give power to its net. Only a `power` pin does; a pin
    /// whose direction is not known does not.
    pub fn gives_power(&self) -> bool {
        self.kind == PinKind::Power && self.power_flow == Some(PowerFlow::Gives)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PinRef {
    pub refdes: String,
    /// Pin number as the symbol names it (`"1"`, `"A5"`).
    pub pin: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Net {
    pub name: String,
    pub pins: Vec<PinRef>,
}
