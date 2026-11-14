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

/// The standard generic that says a part is a connector (`std::parts::connector::Connector`,
/// ADR 0068): the way in and out of a board.
pub const CONNECTOR_GENERIC: &str = "Connector";

impl Part {
    /// Whether the part is a connector: it provides the standard generic `Connector` (or is
    /// it). It is the generic that says so, never the part's name or its reference designator,
    /// which is a label that counts parts and may change (issue #166).
    pub fn is_connector(&self) -> bool {
        self.provides.iter().any(|g| g == CONNECTOR_GENERIC)
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
