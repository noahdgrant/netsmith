//! The checked model of one board: what exists, what is known about it, and what is
//! left for the solver. Everything lives in arenas addressed by `InstId` and `FieldId`.

use netsmith_lang::Span;
use netsmith_lang::ast::{BinOp, CmpOp};
pub use netsmith_lang::ast::{PinKind, Requirement};
use netsmith_quantity::{Dimension, Quantity};

use std::sync::Arc;

use crate::diag::{SourceId, Sources};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldId(pub usize);

/// An `enum` that a field of the model has the type of (ADR 0054).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnumId(pub usize);

/// An `enum`: its name and its variants, in the order written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumInfo {
    pub name: String,
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstKind {
    Board,
    System,
    Panel,
    Module,
    Part,
    Interface,
    Pin,
}

impl InstKind {
    /// The word for it in a message.
    pub fn word(self) -> &'static str {
        match self {
            Self::Board => "board",
            Self::System => "system",
            Self::Panel => "panel",
            Self::Module => "module",
            Self::Part => "part",
            Self::Interface => "interface",
            Self::Pin => "pin",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Member {
    Inst(InstId),
    Field(FieldId),
    /// A name whose declaration failed. Using it reports nothing further.
    Poison,
}

/// The standard generics whose parts are banks (ADR 0052): a number of identical parts that
/// stand in parallel in one place, chosen by the design or by the solver through the part's
/// `.count`. The bank is the generic's, so a provider of it is one too and nothing a design
/// defines is (ADR 0068 open question 9: no syntax for it yet, and a list in one place).
const BANKS: [&str; 1] = ["Capacitor"];

/// The standard generics that are a series passive for the rule of `unused` (ADR 0083): a
/// two-ended part that carries what the board feeds through it, so an interface joined through
/// one to the rest of the board is used. The narrowest reading; a capacitor, an inductor, a
/// diode and a switch are not in it (a decision of the owner's).
const SERIES: [&str; 3] = ["Resistor", "Fuse", "FerriteBead"];

#[derive(Debug, Clone)]
pub struct Instance {
    pub name: String,
    pub kind: InstKind,
    /// The definition it was made from (`Resistor`), or `Pin`.
    pub type_name: String,
    pub parent: Option<InstId>,
    pub children: Vec<(String, Member)>,
    /// Terminals a `~>` chain enters and leaves a part by.
    pub bridge: Option<(InstId, InstId)>,
    /// For a part: the element it is in a DC analysis, from its `dc` statement or its
    /// generic's (ADR 0076, amendment 1). None for a part that states none.
    pub dc: Option<DcElement>,
    pub catalog: Option<Catalog>,
    /// For a part: its `pins` tables, in source order (ADR 0016).
    pub tables: Vec<PinTable>,
    /// For a part: the pairs of pins its board must put on one net.
    pub ties: Vec<Tie>,
    /// For a part: its peripherals and shared resources (ADR 0071).
    pub rules: PartRules,
    pub source: SourceId,
    pub span: Span,
    /// The member declaration that made it (`.r1: Resistor`), where the user wrote it.
    /// `span` is the definition it was made from, which may be in the standard library.
    pub declared: Option<(SourceId, Span)>,
    /// The `///` lines above the member declaration that made it (`.r1: Resistor`); for the
    /// board itself, those above its definition.
    pub docs: Vec<String>,
    /// The `///` lines above the definition it was made from, then the `//!` lines at the
    /// start of its body (a module's own description), in the order written.
    pub def_docs: Vec<String>,
    /// Where the docs are written: the file as it was loaded and the 1-based line of the
    /// declaration (or of the definition when only that has docs).
    pub doc_at: Option<(String, usize)>,
    pub(crate) def: Option<(usize, usize)>,
    /// The generics its definition provides (`part X provides A, B`, ADR 0068), by name.
    pub provides: Vec<String>,
    /// The generics of the standard library it stands for, by name: its own definition when
    /// that is one (`Capacitor` placed as itself, since a standard generic is its own first
    /// provider), and those of `provides` that are in the standard library (ADR 0068). What a
    /// standard generic gives a part (a bank, ADR 0052) a provider of it has too.
    pub standard: Vec<String>,
    /// For a use of a generic after a provider was put in its place: the generic's name
    /// (`CoaxReceptacle`). `type_name` is then the provider's. None for a part or module that
    /// was written by its own type, `.rf: AcmeMicroCoax`.
    pub placed_as: Option<String>,
    /// The definition of the generic a use was placed as.
    pub(crate) placed_def: Option<(usize, usize)>,
}

/// The kind of DC element a part states with `dc` (ADR 0076, amendment 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DcKind {
    /// `dc resistor .a .b .resistance`
    Resistor,
    /// `dc diode .anode .cathode .forward_voltage`
    Diode,
    /// `dc nmos .gate .drain .source .gate_threshold .on_resistance`: an N-channel MOSFET as a
    /// switch.
    Nmos,
    /// `dc pmos .gate .drain .source .gate_threshold .on_resistance`: a P-channel MOSFET as a
    /// switch. Voltages and currents of a part are magnitudes (see `std::parts::PMosfet`).
    Pmos,
    /// `dc linear_regulator .vin .vout [.quiescent_current]`: an ideal output source over the
    /// range of the output rail's voltage, and an input that draws what the output supplies and
    /// the quiescent current, when the statement names one (ADR 0076, amendments 3 and 9). The
    /// first two operands are the rails, not pins.
    LinearRegulator,
    /// `dc switching_regulator .vin .sw .efficiency [.quiescent_current]`: an ideal output
    /// source, and an input that draws the output's power divided by the efficiency and the input
    /// voltage, and the quiescent current when the statement names one (ADR 0076, amendment 3;
    /// ADR 0096). The first two operands are rails.
    SwitchingRegulator,
    /// `dc source .vbatt`: the part gives this rail from outside the board (a connector's
    /// supply): its positive net is held over the range of the rail's voltage and its negative
    /// net at zero, as a board's own supply rail is (ADR 0076, amendment 5). The operand is a
    /// rail, not a pin.
    Source,
}

impl DcKind {
    pub const ALL: [DcKind; 7] = [
        Self::Resistor,
        Self::Diode,
        Self::Nmos,
        Self::Pmos,
        Self::LinearRegulator,
        Self::SwitchingRegulator,
        Self::Source,
    ];

    /// The word after `dc`.
    pub fn word(self) -> &'static str {
        match self {
            Self::Resistor => "resistor",
            Self::Diode => "diode",
            Self::Nmos => "nmos",
            Self::Pmos => "pmos",
            Self::LinearRegulator => "linear_regulator",
            Self::SwitchingRegulator => "switching_regulator",
            Self::Source => "source",
        }
    }

    /// Whether the first operands are rails (interfaces with a `pos`, a `neg` and a `voltage`)
    /// and not pins.
    pub fn takes_rails(self) -> bool {
        matches!(
            self,
            Self::LinearRegulator | Self::SwitchingRegulator | Self::Source
        )
    }

    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.word() == word)
    }

    /// The pins the statement names, first, and what each is.
    pub fn pin_roles(self) -> &'static [&'static str] {
        match self {
            Self::Resistor => &["terminal", "terminal"],
            Self::Diode => &["anode", "cathode"],
            Self::Nmos | Self::Pmos => &["gate", "drain", "source"],
            Self::LinearRegulator | Self::SwitchingRegulator => &["input rail", "output rail"],
            Self::Source => &["supply rail"],
        }
    }

    /// The values the statement may name after the required ones, with what each is: an
    /// operand that can be left out (ADR 0076, amendment 9). A part that leaves it out is the
    /// element without it, as before.
    pub fn optional_value_roles(self) -> &'static [(&'static str, Dimension)] {
        match self {
            Self::LinearRegulator | Self::SwitchingRegulator => {
                &[("quiescent current", Dimension::CURRENT)]
            }
            _ => &[],
        }
    }

    /// The values the statement names after the pins, with what each is.
    pub fn value_roles(self) -> &'static [(&'static str, Dimension)] {
        match self {
            Self::Resistor => &[("resistance", Dimension::RESISTANCE)],
            Self::Diode => &[("forward voltage", Dimension::VOLTAGE)],
            Self::Nmos | Self::Pmos => &[
                ("gate threshold", Dimension::VOLTAGE),
                ("on resistance", Dimension::RESISTANCE),
            ],
            Self::LinearRegulator | Self::Source => &[],
            Self::SwitchingRegulator => &[("efficiency", Dimension::NONE)],
        }
    }

    /// The statement written with the names the standard library uses, for a message.
    pub fn example(self) -> &'static str {
        match self {
            Self::Resistor => "dc resistor .a .b .resistance",
            Self::Diode => "dc diode .anode .cathode .forward_voltage",
            Self::Nmos => "dc nmos .gate .drain .source .gate_threshold .on_resistance",
            Self::Pmos => "dc pmos .gate .drain .source .gate_threshold .on_resistance",
            Self::LinearRegulator => "dc linear_regulator .vin .vout",
            Self::SwitchingRegulator => "dc switching_regulator .vin .sw .efficiency",
            Self::Source => "dc source .vbatt",
        }
    }
}

/// The DC element of a part: its `dc` statement with every name resolved. A later stage builds
/// the network of an analysis from these. `pins` are in the order of [`DcKind::pin_roles`] and
/// `values` in the order of [`DcKind::value_roles`] and then [`DcKind::optional_value_roles`]
/// (the optional ones the statement names), so that a resistor's two terminals are
/// `pins[0]` and `pins[1]` and its resistance is `values[0]`, a diode's anode is `pins[0]`, and
/// a MOSFET's gate, drain and source are `pins[0]`, `pins[1]` and `pins[2]`. A regulator's
/// `pins` are its input rail and its output rail (interfaces, see [`DcKind::takes_rails`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DcElement {
    pub kind: DcKind,
    pub pins: Vec<InstId>,
    pub values: Vec<FieldId>,
    pub source: SourceId,
    pub span: Span,
}

/// An output level that a part states with the current it is measured at, as its datasheet
/// does (`.voh &= .vdd.voltage - 0.4V at 2mA`, ADR 0076, amendment 1). The statement also
/// narrows or constrains the level as it would without `at`; this records the load only.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelTest {
    /// The `voh` or `vol` field of a `Logic` or of a `Levels`.
    pub level: FieldId,
    /// The operator of the statement (`&=`, `?=`, `==`, `>=` or `<=`).
    pub op: CmpOp,
    /// The test current, a single positive number. `voh` is measured with this current drawn
    /// from the pin, `vol` with it sunk.
    pub current: Quantity,
    pub source: SourceId,
    /// The whole statement.
    pub span: Span,
}

/// One part or module that can stand where a generic is used (ADR 0068).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    /// `src::rf::AcmeMicroCoax`: the module the definition is in and its name. What an
    /// assignment names.
    pub id: String,
    pub name: String,
    /// `src::rf`.
    pub module: String,
    /// Defined in a file of the standard library.
    pub std: bool,
    /// The generic itself, which is its own provider when it has a pin table with pads or a
    /// catalog.
    pub own: bool,
    /// What it is made as: a part, a module, or an interface (a process, ADR 0113).
    pub kind: InstKind,
}

/// A member declared with the type of a generic (`.rf: CoaxReceptacle { ... }`): what
/// provides it, in the order a pick tries them, and which one stands in its place.
#[derive(Debug, Clone, PartialEq)]
pub struct GenericUse {
    /// `rf`, or `antenna.rf` for a use inside a module.
    pub path: String,
    pub inst: InstId,
    /// The generic's name.
    pub generic: String,
    /// Every provider in scope of the file, in the order of ADR 0068 section 4.
    pub candidates: Vec<Provider>,
    /// The index into `candidates` of the provider the instance was made from. With no
    /// candidates, zero and the instance is the generic itself.
    pub chosen: usize,
}

impl Instance {
    /// Where to point a message about this instance: the line that declared it, or its
    /// definition when it was not declared by a member (the board itself).
    pub fn at(&self) -> (SourceId, Span) {
        self.declared.unwrap_or((self.source, self.span))
    }
}

#[derive(Debug, Clone)]
pub struct Catalog {
    pub name: String,
    pub fields: Vec<FieldId>,
    /// The column that names one row (`catalog mcu by .mpn`). The design must set it when
    /// several rows fit: the solver does not pick among rows of a keyed catalog. One row that
    /// fits is taken without it.
    pub key: Option<FieldId>,
    /// The columns marked `optional` in the statement (ADR 0078): a file may leave them
    /// out, and a row may leave the cell empty. A subset of `fields`.
    pub optional: Vec<FieldId>,
}

/// One line of a pin table: what one pin of a part is and which pads it has in the package.
#[derive(Debug, Clone, PartialEq)]
pub struct PinRow {
    pub pin: InstId,
    pub kind: PinKind,
    /// The name the datasheet prints, as written.
    pub name: String,
    pub pads: Vec<String>,
    pub functions: Vec<PinFunction>,
    pub requirement: Option<Requirement>,
    pub span: Span,
}

/// One function a pin can be chosen as. It has its own kind when the table gives one
/// (`UART1_TX:output`, ADR 0046); otherwise the pin's kind is its kind.
#[derive(Debug, Clone, PartialEq)]
pub struct PinFunction {
    pub name: String,
    pub kind: Option<PinKind>,
    /// The row marks the function `class` (`fn GPIO class`): the board may choose it on any
    /// number of pins.
    pub class: bool,
}

impl PinRow {
    /// The names of the functions, in the order the table lists them.
    pub fn function_names(&self) -> Vec<String> {
        self.functions.iter().map(|f| f.name.clone()).collect()
    }

    /// The kind of the pin for the rules of a net, given the function the board chose for
    /// it: the function's own kind if it has one, else the pin's.
    pub fn kind_with(&self, chosen: Option<&str>) -> PinKind {
        self.function_kind(chosen).unwrap_or(self.kind)
    }

    /// The function's own kind, when the board chose a function that has one.
    pub fn function_kind(&self, chosen: Option<&str>) -> Option<PinKind> {
        let name = chosen?;
        self.functions
            .iter()
            .find(|f| f.name == name)
            .and_then(|f| f.kind)
    }
}

/// A `pins` table: the pinout of a part in one package, or in every package when it names
/// none.
#[derive(Debug, Clone, PartialEq)]
pub struct PinTable {
    pub package: Option<String>,
    pub rows: Vec<PinRow>,
    pub source: SourceId,
    pub span: Span,
}

impl PinTable {
    pub fn row(&self, pin: InstId) -> Option<&PinRow> {
        self.rows.iter().find(|r| r.pin == pin)
    }

    /// How the table is written in a message: `pins "DFN-8-EP"`, or `pins`.
    pub fn describe(&self) -> String {
        match &self.package {
            Some(p) => format!("pins \"{p}\""),
            None => "pins".to_string(),
        }
    }
}

/// `tie .a ~ .b` in a part: its board must connect the two.
#[derive(Debug, Clone, PartialEq)]
pub struct Tie {
    pub a: InstId,
    pub b: InstId,
    pub source: SourceId,
    pub span: Span,
}

/// What a part says about how its functions go together: the `peripheral` and `shared`
/// statements of its definition (ADR 0071), in source order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PartRules {
    pub peripherals: Vec<Peripheral>,
    pub shares: Vec<SharedRes>,
}

/// `peripheral UARTE0 on APP_GPIO { UARTE0_TX:output UARTE0_RX:input }`: functions that belong
/// together. It is in use when the board chooses one of them on a pin. With `on`, every pin
/// whose row lists the class offers them (ADR 0071).
#[derive(Debug, Clone, PartialEq)]
pub struct Peripheral {
    pub name: String,
    /// The function after `on`.
    pub class: Option<String>,
    pub class_span: Span,
    /// The bus interface the peripheral is an end of, if it says (`as Spi SpiRole::Master`,
    /// ADR 0064, stage 4).
    pub bus: Option<PeripheralBus>,
    pub functions: Vec<PeripheralFunction>,
    pub source: SourceId,
    /// The whole statement.
    pub span: Span,
    /// The name after the keyword.
    pub name_span: Span,
}

/// One function of a peripheral. It has its own kind when the peripheral gives one
/// (`UARTE0_TX:output`, ADR 0046).
#[derive(Debug, Clone, PartialEq)]
pub struct PeripheralFunction {
    pub name: String,
    pub kind: Option<PinKind>,
    /// The line of the peripheral's bus that the function is (`as sclk`).
    pub line: Option<String>,
    pub span: Span,
}

/// The bus a peripheral is an end of: the definition of the interface and the role it plays.
#[derive(Debug, Clone, PartialEq)]
pub struct PeripheralBus {
    /// The module and index of the interface's definition.
    pub(crate) def: (usize, usize),
    /// The interface's name: `Spi`.
    pub interface: String,
    /// The variant it plays (`Master`), if the bus has roles.
    pub role: Option<String>,
}

/// `shared UARTS limit 2 { UARTE0 UARTE1 .p10 }`: hardware that at most `limit` of its members
/// use at a time (one when the statement has no `limit`).
#[derive(Debug, Clone, PartialEq)]
pub struct SharedRes {
    pub name: String,
    pub limit: u32,
    /// The number after `limit`, when the statement wrote one.
    pub limit_span: Option<Span>,
    pub members: Vec<SharedMember>,
    pub source: SourceId,
    pub span: Span,
    pub name_span: Span,
}

/// A member of a shared resource, as written: the name of a peripheral or of a function, or a
/// pin (`pin` is set, and `name` is how it was written, `.p10`).
#[derive(Debug, Clone, PartialEq)]
pub struct SharedMember {
    pub name: String,
    pub pin: Option<InstId>,
    pub span: Span,
}

/// The `lines` of a bus interface (ADR 0064): for each role of the bus, the kind of each of its
/// lines and a few properties of a line. It belongs to the interface's definition, so that a
/// peripheral that names the interface (`as Spi SpiRole::Master`) reads the same rows.
#[derive(Debug, Clone, PartialEq)]
pub struct BusDef {
    /// The interface's name: `Spi`.
    pub name: String,
    /// The enum of the interface's `.role`: `SpiRole`. None for a bus with one role.
    pub role_enum: Option<String>,
    /// The member that holds the clock of the bus: `frequency`, or `baud` for a `Uart`.
    pub clock: Option<String>,
    /// One table for each role, or the one table of a bus with a single role.
    pub tables: Vec<RoleLines>,
}

/// One `lines` table: the lines a role has.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleLines {
    /// The variant the table is for (`Master`), or None for a bus with one role.
    pub role: Option<String>,
    /// `limit N`: at most N interfaces of this role on one bus.
    pub limit: Option<u32>,
    pub rows: Vec<LineRow>,
    pub source: SourceId,
    pub span: Span,
}

/// One line of a role: its name is the member of the interface (`sclk`).
#[derive(Debug, Clone, PartialEq)]
pub struct LineRow {
    pub name: String,
    pub kind: PinKind,
    pub requirement: Option<Requirement>,
    /// A net carries this line of at most one interface of the role.
    pub single: bool,
    /// The line of the other end that this one is joined to, instead of its own name.
    pub crosses: Option<String>,
    pub span: Span,
}

impl BusDef {
    /// The table for a role: the variant's table, or the one table of a bus with no roles.
    pub fn table(&self, role: Option<&str>) -> Option<&RoleLines> {
        self.tables.iter().find(|t| t.role.as_deref() == role)
    }

    /// Whether a variant of the role enum passes the bus through: the interface has `lines` for
    /// other variants and none for this one (`UsbRole::Passthrough`, a receptacle). Such an end
    /// is not an end of the bus: it has no table to judge and no limit to count, and what it
    /// states is not a role that another end could disagree with (ADR 0064, section 10).
    pub fn passes_through(&self, role: &str) -> bool {
        self.role_enum.is_some() && self.table(Some(role)).is_none()
    }

    /// The line that `line` crosses to, or that crosses to it: `tx` and `rx`.
    pub fn partner(&self, line: &str) -> Option<&str> {
        self.tables
            .iter()
            .flat_map(|t| t.rows.iter())
            .find_map(|r| match (&r.crosses, r.name == line) {
                (Some(other), true) => Some(other.as_str()),
                (Some(other), false) if other == line => Some(r.name.as_str()),
                _ => None,
            })
    }

    /// Every pair of lines that cross, each once, in the order the tables write them.
    pub fn crossings(&self) -> Vec<(&str, &str)> {
        let mut pairs: Vec<(&str, &str)> = Vec::new();
        for r in self.tables.iter().flat_map(|t| t.rows.iter()) {
            if let Some(other) = &r.crosses
                && !pairs.iter().any(|(a, b)| {
                    (*a, *b) == (r.name.as_str(), other.as_str())
                        || (*b, *a) == (r.name.as_str(), other.as_str())
                })
            {
                pairs.push((r.name.as_str(), other.as_str()));
            }
        }
        pairs
    }
}

/// Which pin table applies to a part, and why not if none does.
#[derive(Debug, Clone, PartialEq)]
pub enum Pinout<'a> {
    /// The part has no pin tables.
    NoTables,
    Table(&'a PinTable),
    /// The tables are for packages and the part's package is not known (yet).
    Unknown {
        packages: Vec<&'a str>,
    },
    /// The part is in a package that none of the tables is for.
    NoSuchPackage {
        package: String,
        packages: Vec<&'a str>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Quantity(Dimension),
    Text,
    /// One variant of an `enum` (ADR 0054). Its domain is `Domain::Text` holding the name of
    /// the variant.
    Enum(EnumId),
}

/// What is currently known about a field's value.
#[derive(Debug, Clone, PartialEq)]
pub enum Domain {
    /// No constraint yet: anything of the right kind.
    Unset,
    Quantity(Quantity),
    Text(String),
}

#[derive(Debug, Clone)]
pub struct Field {
    pub owner: InstId,
    pub name: String,
    pub kind: FieldKind,
    pub domain: Domain,
    /// The statements that narrowed it, in source order.
    pub narrowed_at: Vec<(SourceId, Span)>,
    /// Where `optional` marks the declaration (ADR 0074): the relations that read the member
    /// are skipped when nothing states it.
    pub optional: Option<OptionalMark>,
}

/// The place of an `optional` marker, and the definition whose text declared the member (for
/// a generic's member that is the generic, not the provider whose instance holds it).
#[derive(Clone, Copy, Debug)]
pub struct OptionalMark {
    pub source: SourceId,
    pub span: Span,
    pub declared_in: Option<(usize, usize)>,
}

/// A resolved expression over fields.
#[derive(Debug, Clone, PartialEq)]
pub enum RExpr {
    Const(Quantity),
    Text(String),
    Field(FieldId),
    Neg(Box<RExpr>),
    Bin(BinOp, Box<RExpr>, Box<RExpr>),
    /// `lo..hi` where an end is not a fixed number (ADR 0066): the smallest range that holds
    /// the low end's lowest value and the high end's highest, once both are known.
    Range(Box<RExpr>, Box<RExpr>),
    /// `min(.f)`, `max(.f)` (ADR 0098) or `nominal(.f)`: the least value, the greatest value or
    /// the centre of a field's range, as one value. It is read, never solved for: the solver
    /// does not narrow `.f` through a relation that holds it, and a statement that is not `<=`
    /// or `>=` is refused it.
    End(RangeEnd, FieldId),
}

/// Which point of a field's range `min(.f)`, `max(.f)` or `nominal(.f)` reads: the least end,
/// the greatest end, or the centre, which is the value the field is stated at when its range is
/// a value with a tolerance (a range is stored as its two ends and nothing else).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeEnd {
    Min,
    Max,
    Nominal,
}

impl RangeEnd {
    /// The word that is written: `min`, `max` or `nominal`.
    pub fn word(self) -> &'static str {
        match self {
            Self::Min => "min",
            Self::Max => "max",
            Self::Nominal => "nominal",
        }
    }
}

impl RExpr {
    /// Every field the expression mentions.
    pub fn fields(&self, out: &mut Vec<FieldId>) {
        match self {
            Self::Const(_) | Self::Text(_) => {}
            Self::Field(f) | Self::End(_, f) => out.push(*f),
            Self::Neg(inner) => inner.fields(out),
            Self::Bin(_, a, b) | Self::Range(a, b) => {
                a.fields(out);
                b.fields(out);
            }
        }
    }
}

/// A constraint that mentions other fields, so it waits for the solver. Its two sides
/// have already been checked to have the same dimension.
#[derive(Debug, Clone)]
pub struct Relation {
    pub lhs: RExpr,
    pub op: CmpOp,
    pub rhs: RExpr,
    pub source: SourceId,
    pub span: Span,
    /// For a relation the author wrote (a `==`, `>=` or `<=` statement): its text as written
    /// and where. The relation Kirchhoff's voltage law gives a `~>` chain across a rail
    /// (ADR 0113) has one too, with the text the checker writes for it and the chain's line,
    /// because it says what a hand-written one would. Other relations the checker makes from
    /// statements have none.
    pub written: Option<Written>,
    /// Whether a part's own definition states it (a relation of a generic or a part, such as
    /// `.gate_source_voltage >= .on_resistance_vgs`), as the library or a project's part
    /// writes it, and not the design that uses the part. A row with no value in an optional
    /// catalog column (ADR 0078) is still offered when only such a relation names the
    /// column, because the design has said nothing about it.
    pub own: bool,
}

/// A relation statement as the source has it, or as a `~>` chain implies it.
#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    /// The module or board instance whose body states it: what its `.x` paths are relative to.
    pub scope: InstId,
    /// The statement's text with its white space collapsed to single spaces; for a chain's
    /// relation, the relation written out as a statement of `scope` would write it.
    pub text: String,
    /// The file as it was loaded, and the 1-based line the statement (or the chain) starts on.
    pub file: String,
    pub line: usize,
    /// For the relation the checker derives from a `~>` chain across a rail (ADR 0113): the
    /// chain's parts, in the order the chain runs. Empty for a statement the author wrote.
    pub chain: Vec<InstId>,
}

/// A run of `~>` through bridge parts, from one pin to another.
#[derive(Debug, Clone)]
pub struct Chain {
    pub start: InstId,
    pub parts: Vec<InstId>,
    pub end: InstId,
    /// The `~>` statement.
    pub source: SourceId,
    pub span: Span,
    /// For a chain from a rail's `pos` to the same rail's `neg` with no tap, whose current
    /// Kirchhoff's voltage law would set (ADR 0113): the first part in it that is not a
    /// `dc resistor` or a `dc diode` the way the chain runs, which leaves the current free.
    pub stopped_by: Option<InstId>,
}

/// One current as it appears in a [`Balance`]: a rail's, or the through-current of the first
/// or last part of a chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Draw {
    pub field: FieldId,
    /// Whether the current counts with its sign flipped. A module's own port, seen from
    /// inside, is the supply of what is inside, so what the module draws is what the
    /// inside is given. A rail's current comes back at its `neg` pin, which is a draw of
    /// the opposite sign, and a chain gives back at its end what it drew at its start.
    pub flipped: bool,
}

/// Kirchhoff's current law at one net, in one scope: what is drawn there adds up to zero.
/// The currents of rails are not equated by `~` (a rail can feed several loads); they are
/// summed here instead, together with the currents of the chains that start or end there
/// (ADR 0014).
#[derive(Debug, Clone)]
pub struct Balance {
    /// The net's name, or the path of the pin when nothing else is on the net.
    pub net: String,
    /// The board or module whose rails these are: its own, and those of its children.
    pub scope: InstId,
    pub draws: Vec<Draw>,
    /// Whether this is a return net: no rail draws from it through `pos`, so it is where
    /// the current comes back (ground). It is checked and never solved from, because its
    /// terms repeat those of the supply nets and the same ranges added a second time are
    /// wider than the ranges they came from.
    pub returns: bool,
    /// The part pins on the net, in the scope, that are none of the terms and that the
    /// checker cannot say draw nothing (ADR 0027): an IC's signal pin, an inductor's pin
    /// that is not in a counted chain, a connector. Empty when the balance is complete.
    pub unmodelled: Vec<InstId>,
}

impl Balance {
    /// Whether every pin on the net is accounted for: a term, or a pin that draws no DC
    /// current by its part's model. Only a complete balance may set or verify the
    /// through-current of a part (ADR 0027).
    pub fn is_complete(&self) -> bool {
        self.unmodelled.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct Net {
    /// Every pin on the net: part terminals and the interface or module ports that
    /// merely pass the connection along.
    pub pins: Vec<InstId>,
    /// The pins that belong to parts. These are the real connection points, and the
    /// only ones an exporter writes out.
    pub terminals: Vec<InstId>,
    /// What the net is called: the name its pins asked for, or `net-` and its smallest
    /// terminal path when none did.
    pub name: String,
    /// Whether a pin asked for `name`. If not, it was generated.
    pub explicit: bool,
}

/// What a contact of a mated connector is, read from the board behind it (ADR 0040).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The net is the `neg` of a rail.
    Ground,
    /// The net is the `pos` of a rail.
    Power,
    /// A part's `input`, `output` or `bidirectional` pin is on the net.
    Signal,
    /// Nothing the checker can say anything about is on the net.
    Open,
}

impl Role {
    pub fn word(self) -> &'static str {
        match self {
            Self::Ground => "ground",
            Self::Power => "power",
            Self::Signal => "a signal",
            Self::Open => "open",
        }
    }
}

/// One side of one contact of a mate: the pin of the connector, the net it is on in its
/// board and what that net is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactSide {
    pub pin: InstId,
    pub role: Role,
    /// The name of the net in the board, if the pin is on one.
    pub net: Option<String>,
}

/// Contact `number` of two mated connectors: the pins of the two sides, which meet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    /// From 1: pin `pN` of each connector.
    pub number: usize,
    pub a: ContactSide,
    pub b: ContactSide,
}

/// Two connectors that a system mates with `~` between their `.mating` (ADR 0040): contact N
/// of one meets contact N of the other.
#[derive(Debug, Clone)]
pub struct Mate {
    /// The two connector parts, in the order the statement names them.
    pub a: InstId,
    pub b: InstId,
    pub source: SourceId,
    pub span: Span,
    /// The contacts, from 1. Empty when the two connectors do not mate (an error was
    /// reported) or are not read yet.
    pub contacts: Vec<Contact>,
    /// The ports of the two boards that the mate joins, derived from the contacts: a rail
    /// whose `pos` and `neg` sit on mated contacts is joined to the rail on the same
    /// contacts of the other board, as `~` between the two would.
    pub ports: Vec<(InstId, InstId)>,
    /// Set when the mate would also work with one connector turned end for end and neither
    /// is keyed: what would swap, for the guideline that says so.
    pub reversible: Option<String>,
}

/// One net of one board that is joined to others by mated ground contacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundNet {
    /// The board member of the system.
    pub board: InstId,
    pub net: String,
}

/// The grounds that mated ground contacts join (ADR 0040): each board keeps its own ground net,
/// and the system reports which of them share a ground.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundGroup {
    pub nets: Vec<GroundNet>,
    /// How many ground contacts each mate (an index into `Model::mates`) puts in the group.
    pub contacts: Vec<(usize, usize)>,
}

/// A `~` between two connectors' `.mating`, before the nets are known.
#[derive(Debug, Clone)]
pub(crate) struct MateDecl {
    pub a: InstId,
    pub b: InstId,
    pub source: SourceId,
    pub span: Span,
}

/// A `~` between two ports as the author wrote it, kept to refuse it where a mate says the
/// same.
#[derive(Debug, Clone)]
pub(crate) struct ExplicitJoin {
    pub a: InstId,
    pub b: InstId,
    pub source: SourceId,
    pub span: Span,
}

/// The text of the files that a model's statements are in, kept so that a message can say where a
/// statement is and show it as it was written (the solver names the lines of the two statements
/// that make a bound reach a part, ADR 0065).
#[derive(Clone, Default)]
pub(crate) struct SourceTexts(pub(crate) Option<Arc<Sources>>);

impl std::fmt::Debug for SourceTexts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SourceTexts(..)")
    }
}

/// The domains and the added relations of a model at one moment ([`Model::snapshot`]).
#[derive(Debug, Clone)]
pub struct Snapshot {
    domains: Vec<Domain>,
    relations: Arc<Vec<Relation>>,
}

impl Snapshot {
    /// The domain a field had.
    pub fn domain(&self, field: FieldId) -> &Domain {
        &self.domains[field.0]
    }
}

/// A moment of a model to go back to ([`Model::bookmark`]) from any later moment, also after
/// going back to an earlier one. It holds a place in the model's journal, not the domains: a
/// solver that keeps one for each group of parts it searched keeps what the groups changed, not
/// a copy of every field for each (#569).
#[derive(Debug, Clone)]
pub struct Bookmark {
    at: usize,
    relations: Arc<Vec<Relation>>,
    /// While a bookmark is held, the model keeps its journal.
    _held: Arc<()>,
}

/// What the [`Bookmark`]s of a model hold: more than one holder means one is held. A clone of
/// the model starts with none, so that a clone kept for later does not keep the journal of the
/// model it was cloned from.
#[derive(Debug, Default)]
struct Holders(Arc<()>);

impl Clone for Holders {
    fn clone(&self) -> Self {
        Holders::default()
    }
}

/// A moment of a model to go back to ([`Model::checkpoint`]).
#[derive(Debug)]
pub struct Checkpoint {
    mark: usize,
    relations: Arc<Vec<Relation>>,
}

/// An interface the board states is not used (`unused .processor.usb`, ADR 0083), and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unused {
    pub inst: InstId,
    pub source: SourceId,
    pub span: Span,
}

/// A request for a pin by capability (`.status: .u.GPIO`, ADR 0101): the pin of the board it
/// declares, the part it asks and what it needs the pin to offer. Which pin it gets is decided
/// after the elaboration, in `assign.rs`.
#[derive(Debug, Clone)]
pub struct PinRequest {
    /// The pin the request declares: what the board connects.
    pub pin: InstId,
    /// The board or module that declared it, where `target` is looked up.
    pub block: InstId,
    /// The path of the part that is asked, from `block` (`u`), as written.
    pub target: Vec<String>,
    /// What the pin must offer (`GPIO`, `UARTE1_TX`): a function a row lists or a peripheral
    /// offers.
    pub capability: String,
    pub source: SourceId,
    /// The whole declaration, and the part's and the capability's names in it.
    pub span: Span,
    pub target_span: Span,
    pub capability_span: Span,
}

/// The pin a request got: from the pin file (a kept entry) or picked by a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinAssignment {
    pub request: InstId,
    pub part: InstId,
    pub pin: InstId,
    pub function: String,
    /// The request's path, the key of its entry in the pin file.
    pub key: String,
    pub lock: bool,
    pub source: crate::pinfile::EntrySource,
}

#[derive(Debug, Clone)]
pub struct Model {
    pub root: InstId,
    /// The requests for a pin by capability, in the order they were declared.
    pub requests: Vec<PinRequest>,
    /// The pin each request that could be served got, in the byte order of the requests' paths.
    pub pin_assignments: Vec<PinAssignment>,
    pub(crate) instances: Vec<Instance>,
    pub(crate) fields: Vec<Field>,
    pub(crate) relations: Arc<Vec<Relation>>,
    /// Pairs of fields joined by connecting two interfaces: they hold one value.
    pub(crate) equalities: Vec<(FieldId, FieldId)>,
    pub(crate) chains: Vec<Chain>,
    pub(crate) nets: Vec<Net>,
    pub(crate) balances: Arc<Vec<Balance>>,
    /// In a system, the pairs of ports that were joined with `~`. Their pins are not on a
    /// net together (only a board names nets); their values are equated and their currents
    /// are balanced.
    pub(crate) joins: Vec<(InstId, InstId)>,
    pub(crate) mate_decls: Vec<MateDecl>,
    pub(crate) explicit_joins: Vec<ExplicitJoin>,
    /// In a system, the mated connectors with what is behind their contacts.
    pub(crate) mates: Vec<Mate>,
    pub(crate) ground_groups: Vec<GroundGroup>,
    /// Pairs of pins that a part's own definition joins (`.vin.neg ~ .vout.neg`): one
    /// physical pin, or pins that are one node inside the part.
    pub(crate) part_joins: Vec<(InstId, InstId)>,
    /// The `~` statements of a module that join one of its own interfaces to an interface of
    /// a part inside it, as (the module's, the part's). Whether each makes the module's an
    /// alias of the part's is settled when the balances are made (ADR 0077).
    pub(crate) rail_aliases: Vec<(InstId, InstId)>,
    /// The output levels stated with a test current, in the order they were read (ADR 0076).
    pub(crate) level_tests: Vec<LevelTest>,
    /// The interfaces the board states are not used (ADR 0083), in path order.
    pub(crate) unused: Vec<Unused>,
    /// The enums that fields of this model have the type of.
    pub(crate) enums: Vec<EnumInfo>,
    /// The members declared with the type of a generic, in the order they were made.
    pub uses: Vec<GenericUse>,
    /// The contradictions on a field inside a use that has more than one provider (also among
    /// the check's diagnostics): another provider may not have them, so the build tries the
    /// others before it stops on them (ADR 0113). A package the provider has no pin table for
    /// is one too: a narrowing from outside the module (ADR 0116) may ask for another
    /// provider's package.
    pub provider_conflicts: Vec<crate::diag::Diagnostic>,
    /// The `lines` of the bus interfaces in use, by the module and index of their definition
    /// (ADR 0064).
    pub(crate) bus_defs: std::collections::BTreeMap<(usize, usize), BusDef>,
    pub(crate) sources: SourceTexts,
    /// The `polygon` statements of `Outline`s (issue #57), in the order they were read.
    pub(crate) polygons: Vec<OutlinePolygon>,
    /// The domains that [`Model::set_domain`] replaced while a checkpoint was open or a
    /// bookmark was held, oldest first, with the field. Empty when neither is.
    journal: Vec<(FieldId, Domain)>,
    /// Held by every [`Bookmark`] of this model.
    bookmarks: Holders,
    /// How many checkpoints are open.
    open_checkpoints: usize,
    /// The equations that pass a pin's function on (`pinout.rs`), found once for a list of
    /// relations.
    pub(crate) forwards: ForwardCache,
}

/// The equations `.a.function == .b.function` between two pins' functions, by the field on
/// each side, for the list of relations they were read from (ADR 0071, stage 0). The solver asks
/// for a pin's kind, and so for its function, for every pin of every candidate it tries; a scan
/// of all the relations each time made the picks of a board of 1,200 parts take half a minute
/// (#561). The cache holds a [`Weak`](std::sync::Weak) of the list: it does not stop
/// [`Arc::make_mut`] from changing the list in place, and any change leaves it stale. A clone of
/// the model starts with none.
#[derive(Default)]
pub(crate) struct ForwardCache(std::sync::Mutex<Option<std::sync::Arc<Forwards>>>);

/// What [`ForwardCache`] holds.
pub(crate) struct Forwards {
    relations: std::sync::Weak<Vec<Relation>>,
    /// For a pin's `function` field, the pin of the first equation with it alone on the left
    /// and another pin's `function` alone on the right.
    pub(crate) from: std::collections::HashMap<FieldId, InstId>,
    /// For a pin's `function` field, the pins of the equations with it alone on the right, in
    /// the order of the relations.
    pub(crate) read_by: std::collections::HashMap<FieldId, Vec<InstId>>,
}

impl Clone for ForwardCache {
    fn clone(&self) -> Self {
        ForwardCache::default()
    }
}

impl std::fmt::Debug for ForwardCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ForwardCache(..)")
    }
}

impl Model {
    /// Whether `inst` is, or is inside, a use of a generic with more than one provider in
    /// scope (ADR 0113).
    pub fn in_varied_use(&self, inst: InstId) -> bool {
        let mut cur = Some(inst);
        while let Some(at) = cur {
            if self
                .uses
                .iter()
                .any(|u| u.inst == at && u.candidates.len() > 1)
            {
                return true;
            }
            cur = self.instance(at).parent;
        }
        false
    }

    /// The forwarding equations of the relations as they are now.
    pub(crate) fn forwards(&self) -> std::sync::Arc<Forwards> {
        let mut held = self.forwards.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(f) = held.as_ref()
            && std::sync::Weak::ptr_eq(&f.relations, &Arc::downgrade(&self.relations))
            && f.relations.strong_count() > 0
        {
            return Arc::clone(f);
        }
        let mut from = std::collections::HashMap::new();
        let mut read_by: std::collections::HashMap<FieldId, Vec<InstId>> =
            std::collections::HashMap::new();
        let pin_function = |f: FieldId| self.is_pin_function(f).then_some(self.field(f).owner);
        for r in self.relations.iter() {
            if let (RExpr::Field(l), CmpOp::EqEq, RExpr::Field(g)) = (&r.lhs, r.op, &r.rhs) {
                if let Some(owner) = pin_function(*g) {
                    from.entry(*l).or_insert(owner);
                }
                if let Some(owner) = pin_function(*l) {
                    read_by.entry(*g).or_default().push(owner);
                }
            }
        }
        let made = Arc::new(Forwards {
            relations: Arc::downgrade(&self.relations),
            from,
            read_by,
        });
        *held = Some(Arc::clone(&made));
        made
    }
}

/// A `polygon` statement of an `Outline` (issue #57): its corners in mm, in the order written.
#[derive(Debug, Clone, PartialEq)]
pub struct OutlinePolygon {
    /// The `Outline` it is in.
    pub outline: InstId,
    pub corners: Vec<PolygonCorner>,
    pub source: SourceId,
    pub span: Span,
}

/// A corner of an [`OutlinePolygon`], in mm in the board's frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolygonCorner {
    pub at: (f64, f64),
    /// `round R`: the radius of a tangent fillet at the corner.
    pub round: Option<f64>,
    /// `arc through (x, y)`: the edge to the next corner is an arc through this point.
    pub through: Option<(f64, f64)>,
}

impl Model {
    /// The `polygon` statement of an `Outline`, if it has one.
    pub fn polygon_of(&self, outline: InstId) -> Option<&OutlinePolygon> {
        self.polygons.iter().find(|p| p.outline == outline)
    }

    pub(crate) fn new() -> Self {
        Self {
            root: InstId(0),
            requests: Vec::new(),
            pin_assignments: Vec::new(),
            instances: Vec::new(),
            fields: Vec::new(),
            relations: Arc::new(Vec::new()),
            equalities: Vec::new(),
            chains: Vec::new(),
            nets: Vec::new(),
            balances: Arc::new(Vec::new()),
            joins: Vec::new(),
            mate_decls: Vec::new(),
            explicit_joins: Vec::new(),
            mates: Vec::new(),
            ground_groups: Vec::new(),
            part_joins: Vec::new(),
            rail_aliases: Vec::new(),
            level_tests: Vec::new(),
            unused: Vec::new(),
            enums: Vec::new(),
            uses: Vec::new(),
            provider_conflicts: Vec::new(),
            bus_defs: std::collections::BTreeMap::new(),
            sources: SourceTexts::default(),
            polygons: Vec::new(),
            journal: Vec::new(),
            bookmarks: Holders::default(),
            open_checkpoints: 0,
            forwards: ForwardCache::default(),
        }
    }

    /// A value of a field as a message shows it: a variant of an enum as it is written in
    /// source (`AssemblyMode::Hand`), anything else as its domain prints.
    pub fn show_value(&self, field: FieldId, domain: &Domain) -> String {
        match (self.field(field).kind, domain) {
            (FieldKind::Enum(e), Domain::Text(variant)) => {
                format!("{}::{variant}", self.enum_info(e).name)
            }
            _ => domain.to_string(),
        }
    }

    /// Whether a field is the `function` of a pin: a name from the pin table of its part
    /// (`.u.pa9.function &= USART1_TX`), not text.
    pub fn is_pin_function(&self, f: FieldId) -> bool {
        let field = self.field(f);
        field.name == "function" && self.instance(field.owner).kind == InstKind::Pin
    }

    /// The enum a field of kind `FieldKind::Enum` has the type of.
    pub fn enum_info(&self, id: EnumId) -> &EnumInfo {
        &self.enums[id.0]
    }

    /// A field's path from the part, module or board whose definition declares it, with the
    /// interfaces it is in: `.out.voh`. The same whether the definition is read alone or inside
    /// a board, so a message that names it is one message.
    pub fn field_in_scope(&self, field: FieldId) -> String {
        let f = self.field(field);
        let scope = if self.instance(f.owner).kind == InstKind::Interface {
            self.owner_of(f.owner)
        } else {
            Some(f.owner)
        };
        match scope {
            Some(scope) if scope != f.owner => {
                format!(".{}.{}", self.path_below(f.owner, scope), f.name)
            }
            _ => format!(".{}", f.name),
        }
    }

    /// The output levels stated with a test current (`.voh &= ... at 2mA`), in source order.
    /// The interfaces the board states are not used (`unused .processor.usb`, ADR 0083).
    pub fn unused(&self) -> &[Unused] {
        &self.unused
    }

    /// The statement that makes `inst` unused, when it is, or is inside, an interface the board
    /// states is not used: the pins and values of that interface need no source, its limits are
    /// not checked and its levels are not compared (ADR 0083).
    pub fn unused_by(&self, inst: InstId) -> Option<&Unused> {
        self.unused.iter().find(|u| self.is_inside(inst, u.inst))
    }

    pub fn level_tests(&self) -> &[LevelTest] {
        &self.level_tests
    }

    /// The test current a level was stated at, if it was (ADR 0076).
    pub fn level_test(&self, level: FieldId) -> Option<&LevelTest> {
        self.level_tests.iter().find(|t| t.level == level)
    }

    /// The parts that state a DC element, with it, in the order the parts were made.
    pub fn dc_elements(&self) -> impl Iterator<Item = (InstId, &DcElement)> {
        self.instances
            .iter()
            .enumerate()
            .filter_map(|(i, inst)| inst.dc.as_ref().map(|d| (InstId(i), d)))
    }

    pub fn instance(&self, id: InstId) -> &Instance {
        &self.instances[id.0]
    }

    pub fn field(&self, id: FieldId) -> &Field {
        &self.fields[id.0]
    }

    /// Every instance under the root, the root included.
    pub fn instances(&self) -> impl Iterator<Item = (InstId, &Instance)> {
        self.instances
            .iter()
            .enumerate()
            .map(|(i, x)| (InstId(i), x))
            .filter(|(i, _)| self.is_inside(*i, self.root))
    }

    /// Every field of the instances under the root.
    pub fn fields(&self) -> impl Iterator<Item = (FieldId, &Field)> {
        self.fields
            .iter()
            .enumerate()
            .map(|(i, x)| (FieldId(i), x))
            .filter(|(_, f)| self.is_inside(f.owner, self.root))
    }

    /// The model of one board of a system, as if the board had been checked alone: the
    /// board is the root, and everything outside it, and every net, chain, relation and
    /// balance that is not wholly inside it, is left out. Ids stay the same, so a solution
    /// of the system can be read through it.
    pub fn scoped_to(&self, board: InstId) -> Model {
        let inside = |i: &InstId| self.is_inside(*i, board);
        let owned = |f: &FieldId| self.is_inside(self.field(*f).owner, board);
        let mut m = self.clone();
        m.root = board;
        m.nets.retain(|n| n.pins.iter().all(inside));
        m.chains.retain(|c| inside(&c.start));
        Arc::make_mut(&mut m.balances).retain(|b| inside(&b.scope));
        m.joins.clear();
        m.mate_decls.clear();
        m.explicit_joins.clear();
        m.mates.clear();
        m.ground_groups.clear();
        m.uses.retain(|u| inside(&u.inst));
        m.equalities.retain(|(a, b)| owned(a) && owned(b));
        Arc::make_mut(&mut m.relations).retain(|r| {
            let mut fields = Vec::new();
            r.lhs.fields(&mut fields);
            r.rhs.fields(&mut fields);
            fields.iter().all(owned)
        });
        m
    }

    /// Whether a pin is a real connection point of a part. A pin of an interface counts
    /// when the interface belongs to a part (`supply.out.pos`), and does not when it
    /// belongs to a module or board, where it only passes a connection along.
    pub fn is_terminal(&self, pin: InstId) -> bool {
        let mut cur = self.instance(pin).parent;
        while let Some(p) = cur {
            match self.instance(p).kind {
                InstKind::Interface => cur = self.instance(p).parent,
                InstKind::Part => return true,
                _ => return false,
            }
        }
        false
    }

    pub fn set_domain(&mut self, field: FieldId, domain: Domain) {
        let old = std::mem::replace(&mut self.fields[field.0].domain, domain);
        if self.keeps_journal() {
            self.journal.push((field, old));
        }
    }

    fn keeps_journal(&self) -> bool {
        self.open_checkpoints > 0 || Arc::strong_count(&self.bookmarks.0) > 1
    }

    /// Start keeping what changes, to be put back by [`Model::go_back_to`] from any later
    /// moment, as often as asked. What it costs is what changes while it is held, not a copy of
    /// every domain as [`Model::snapshot`] takes. A checkpoint that is rolled back must not have
    /// been opened before the bookmark was taken and rolled back while the bookmark is still used.
    pub fn bookmark(&mut self) -> Bookmark {
        if !self.keeps_journal() {
            self.journal.clear();
        }
        Bookmark {
            at: self.journal.len(),
            relations: Arc::clone(&self.relations),
            _held: Arc::clone(&self.bookmarks.0),
        }
    }

    /// Put the model back as it was when `bookmark` was taken of it. What this changes goes in
    /// the journal as any change does, so a later bookmark can still be gone back to.
    pub fn go_back_to(&mut self, bookmark: &Bookmark) {
        assert!(
            bookmark.at <= self.journal.len(),
            "a checkpoint was rolled back past a bookmark"
        );
        // The oldest entry of a field after the bookmark is the domain it had then.
        let mut then: std::collections::BTreeMap<FieldId, &Domain> =
            std::collections::BTreeMap::new();
        for (field, old) in &self.journal[bookmark.at..] {
            then.entry(*field).or_insert(old);
        }
        let back: Vec<(FieldId, Domain)> = then
            .into_iter()
            .filter(|(field, old)| self.fields[field.0].domain != **old)
            .map(|(field, old)| (field, old.clone()))
            .collect();
        for (field, domain) in back {
            self.set_domain(field, domain);
        }
        self.relations = Arc::clone(&bookmark.relations);
    }

    /// Start keeping what changes, to be put back by [`Model::rollback`] or kept by
    /// [`Model::commit`]. A search that tries a choice and takes it back does this instead of
    /// copying the domains of every field: what it costs is what the choice changed. Checkpoints
    /// nest, and each is closed, in the reverse order they were opened, by one of the two.
    pub fn checkpoint(&mut self) -> Checkpoint {
        self.open_checkpoints += 1;
        Checkpoint {
            mark: self.journal.len(),
            relations: Arc::clone(&self.relations),
        }
    }

    /// Put the model back as it was at the checkpoint, and close it.
    pub fn rollback(&mut self, checkpoint: Checkpoint) {
        while self.journal.len() > checkpoint.mark {
            let (field, old) = self
                .journal
                .pop()
                .expect("the journal is longer than the mark");
            self.fields[field.0].domain = old;
        }
        self.relations = checkpoint.relations;
        self.close_checkpoint();
    }

    /// Keep what changed since the checkpoint, and close it. An open checkpoint outside it
    /// still puts it back.
    pub fn commit(&mut self, checkpoint: Checkpoint) {
        drop(checkpoint);
        self.close_checkpoint();
    }

    fn close_checkpoint(&mut self) {
        self.open_checkpoints -= 1;
        if !self.keeps_journal() {
            self.journal.clear();
        }
    }

    /// The fields whose domain is not what it was at the checkpoint.
    pub fn changed_since(&self, checkpoint: &Checkpoint) -> std::collections::HashSet<FieldId> {
        // The oldest entry of a field is the domain it had then.
        let mut then: std::collections::HashMap<FieldId, &Domain> =
            std::collections::HashMap::new();
        for (field, old) in &self.journal[checkpoint.mark..] {
            then.entry(*field).or_insert(old);
        }
        then.into_iter()
            .filter(|(field, old)| self.fields[field.0].domain != **old)
            .map(|(field, _)| field)
            .collect()
    }

    /// What the solver changes of a model: the domain of each field and the relations it adds
    /// ([`Model::set_domain`], [`Model::push_relation`], [`Model::truncate_relations`], the
    /// only changes the model takes once it is checked). A search that tries a choice and puts
    /// the model back takes this instead of a copy of the whole model, which holds every
    /// instance and every name too.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            domains: self.fields.iter().map(|f| f.domain.clone()).collect(),
            relations: Arc::clone(&self.relations),
        }
    }

    /// Put the model back as it was when `snapshot` was taken of it. The model must be the one
    /// the snapshot was taken of, changed only by the solver's three changes.
    pub fn restore(&mut self, snapshot: &Snapshot) {
        debug_assert_eq!(self.fields.len(), snapshot.domains.len());
        for (index, domain) in snapshot.domains.iter().enumerate() {
            if self.fields[index].domain != *domain {
                self.set_domain(FieldId(index), domain.clone());
            }
        }
        self.relations = Arc::clone(&snapshot.relations);
    }

    /// Whether `inst` is `ancestor` or something inside it.
    pub fn is_inside(&self, inst: InstId, ancestor: InstId) -> bool {
        let mut cur = Some(inst);
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            cur = self.instance(c).parent;
        }
        false
    }

    /// An expression as text, with fields shown by their path.
    pub fn show_expr(&self, e: &RExpr) -> String {
        match e {
            RExpr::Const(q) => q.to_string(),
            RExpr::Text(t) => format!("\"{t}\""),
            RExpr::Field(f) => self.path_of_field(*f),
            RExpr::End(end, f) => format!("{}({})", end.word(), self.path_of_field(*f)),
            RExpr::Neg(inner) => format!("-{}", self.show_expr(inner)),
            RExpr::Bin(op, a, b) => {
                let sym = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                };
                format!("({} {sym} {})", self.show_expr(a), self.show_expr(b))
            }
            RExpr::Range(lo, hi) => {
                format!("({}..{})", self.show_expr(lo), self.show_expr(hi))
            }
        }
    }

    pub fn relations(&self) -> &[Relation] {
        &self.relations
    }

    /// The list of relations itself, which is another list (not the same one) after any change
    /// to it: what a cache of answers over the relations keeps to know that it is stale.
    pub fn shared_relations(&self) -> &Arc<Vec<Relation>> {
        &self.relations
    }

    /// Add a relation after the check: the solver adds the ones that a bound entails and takes
    /// them out again with [`Model::truncate_relations`] (ADR 0065). They go at the end, so no
    /// index of a relation of the design changes.
    pub fn push_relation(&mut self, relation: Relation) {
        Arc::make_mut(&mut self.relations).push(relation);
    }

    /// Keep the first `len` relations and drop the rest.
    pub fn truncate_relations(&mut self, len: usize) {
        Arc::make_mut(&mut self.relations).truncate(len);
    }

    /// `file:line` of a statement of this model, if its file is known.
    pub fn place_of_statement(&self, source: SourceId, span: Span) -> Option<String> {
        self.sources.0.as_ref()?.place(source, span)
    }

    /// A statement of this model as it was written, on one line, if its file is known.
    pub fn statement_text(&self, source: SourceId, span: Span) -> Option<String> {
        self.sources.0.as_ref()?.statement(source, span)
    }

    pub fn equalities(&self) -> &[(FieldId, FieldId)] {
        &self.equalities
    }

    pub fn chains(&self) -> &[Chain] {
        &self.chains
    }

    pub fn nets(&self) -> &[Net] {
        &self.nets
    }

    /// Whether a current of a balance is exactly zero now (a stated `0A`, or worked out to
    /// it): it adds nothing to the net, so it does not count as a term when a part's current is
    /// worked out from the others (ADR 0026).
    pub fn draw_is_zero(&self, d: &Draw) -> bool {
        matches!(&self.field(d.field).domain, Domain::Quantity(q) if q.lo() == 0.0 && q.hi() == 0.0)
    }

    pub fn balances(&self) -> &[Balance] {
        &self.balances
    }

    /// The list of balances itself, to hold while the model changes (the structure does not).
    pub fn shared_balances(&self) -> &Arc<Vec<Balance>> {
        &self.balances
    }

    /// The pairs of ports joined with `~` in a system.
    pub fn joins(&self) -> &[(InstId, InstId)] {
        &self.joins
    }

    /// The `.mating` of a part, if it has one: an interface of type `Mating`.
    pub fn mating_of(&self, part: InstId) -> Option<InstId> {
        if self.instance(part).kind != InstKind::Part {
            return None;
        }
        match self.member(part, "mating")? {
            Member::Inst(i)
                if self.instance(i).kind == InstKind::Interface
                    && self.instance(i).type_name == "Mating" =>
            {
                Some(i)
            }
            _ => None,
        }
    }

    /// The two ends of a series passive for `unused` (the pins of its `bridge`): a part that
    /// stands for a standard generic in [`SERIES`], whatever its provider names its pins. None
    /// for any other part.
    pub fn series_ends(&self, part: InstId) -> Option<(InstId, InstId)> {
        let inst = self.instance(part);
        let series = inst.standard.iter().any(|g| SERIES.contains(&g.as_str()));
        if inst.kind != InstKind::Part || !series {
            return None;
        }
        inst.bridge
    }

    /// The `.count` of a part that is a bank (ADR 0052): the number of identical parts that
    /// stand in this place. Only a part that stands for a standard generic in [`BANKS`] is one,
    /// whatever a design's own part names its members.
    pub fn bank_count(&self, part: InstId) -> Option<FieldId> {
        let inst = self.instance(part);
        let bank = inst.standard.iter().any(|g| BANKS.contains(&g.as_str()));
        if inst.kind != InstKind::Part || !bank {
            return None;
        }
        match self.member(part, "count")? {
            Member::Field(f) if matches!(self.field(f).kind, FieldKind::Quantity(_)) => Some(f),
            _ => None,
        }
    }

    /// The connectors a system mates (ADR 0040).
    pub fn mates(&self) -> &[Mate] {
        &self.mates
    }

    /// The grounds the mates join.
    pub fn ground_groups(&self) -> &[GroundGroup] {
        &self.ground_groups
    }

    /// The board an instance is on: the nearest board above it, or itself.
    pub fn board_of(&self, inst: InstId) -> Option<InstId> {
        let mut cur = Some(inst);
        while let Some(c) = cur {
            if self.instance(c).kind == InstKind::Board {
                return Some(c);
            }
            cur = self.instance(c).parent;
        }
        None
    }

    /// Whether `field` is the current of an interface. Such a field is a flow, not a
    /// potential: connecting two interfaces equates their voltages, but their currents
    /// add up at the net (see [`Balance`]).
    pub fn is_flow(&self, field: FieldId) -> bool {
        let f = self.field(field);
        f.name == "current"
            && f.kind == FieldKind::Quantity(Dimension::CURRENT)
            && self.instance(f.owner).kind == InstKind::Interface
    }

    /// Whether `field` is a logic level of a `Logic` line or of the `Levels` of a bus interface
    /// (`vih`, `vil`, `voh`, `vol`). It is what one part's pin guarantees or needs, so
    /// connecting two lines does not equate them the way it equates the voltage of two rails
    /// (ADR 0046, ADR 0064).
    pub fn is_level(&self, field: FieldId) -> bool {
        let f = self.field(field);
        matches!(f.name.as_str(), "vih" | "vil" | "voh" | "vol")
            && f.kind == FieldKind::Quantity(Dimension::VOLTAGE)
            && self.instance(f.owner).kind == InstKind::Interface
            && (self.instance(f.owner).type_name == "Logic" || self.is_levels(f.owner))
    }

    /// Whether `field` is a figure of one pin that a `Logic` or a `Levels` may state: its leakage
    /// `ii` and its output resistances `roh` and `rol` (ADR 0076, amendment 8). Like the levels,
    /// they belong to the part's own pin, so connecting two lines does not equate them. So is the
    /// `capacitance` of an `I2c` interface, the capacitance of one part's pin, which the board adds
    /// up with the others' (`std::bus::I2cPullups`), and its `sink_current`, the current one part's
    /// driver sinks at its `vol`, of which the board states the least.
    pub fn is_pin_figure(&self, field: FieldId) -> bool {
        let f = self.field(field);
        let kind = match f.name.as_str() {
            "ii" => FieldKind::Quantity(Dimension::CURRENT),
            "roh" | "rol" => FieldKind::Quantity(Dimension::RESISTANCE),
            // The capacitance of one pin of an I2C bus interface and the current its driver sinks
            // at its low level (`std::bus::I2c`).
            "capacitance" | "sink_current"
                if self.instance(f.owner).kind == InstKind::Interface
                    && self.instance(f.owner).type_name == "I2c" =>
            {
                return f.kind
                    == FieldKind::Quantity(if f.name == "capacitance" {
                        Dimension::CAPACITANCE
                    } else {
                        Dimension::CURRENT
                    });
            }
            _ => return false,
        };
        f.kind == kind
            && self.instance(f.owner).kind == InstKind::Interface
            && (self.instance(f.owner).type_name == "Logic" || self.is_levels(f.owner))
    }

    /// Whether an instance is the `Levels` of the standard library (`std::logic::Levels`): the
    /// logic levels that a bus interface carries for all its lines (ADR 0064).
    pub fn is_levels(&self, inst: InstId) -> bool {
        let i = self.instance(inst);
        i.kind == InstKind::Interface
            && i.type_name == "Levels"
            && self
                .sources
                .0
                .as_ref()
                .is_none_or(|s| s.get(i.source).name.starts_with("std/"))
    }

    /// The `current` field and the `pos` pin of an interface that draws current from a
    /// net, such as a `Rail`.
    pub fn draw_of(&self, iface: InstId) -> Option<(FieldId, InstId)> {
        if self.instance(iface).kind != InstKind::Interface {
            return None;
        }
        match (self.member(iface, "current")?, self.member(iface, "pos")?) {
            (Member::Field(f), Member::Inst(pin)) if self.is_flow(f) => Some((f, pin)),
            _ => None,
        }
    }

    /// The through-current of a part: its `current` field, taken from the first terminal of
    /// its `bridge` to the second, the way a `~>` chain runs through it. A part without
    /// one, such as a capacitor, passes no current at DC.
    pub fn through_of(&self, part: InstId) -> Option<FieldId> {
        if self.instance(part).kind != InstKind::Part {
            return None;
        }
        self.instance(part).bridge?;
        match self.member(part, "current")? {
            Member::Field(f) if self.field(f).kind == FieldKind::Quantity(Dimension::CURRENT) => {
                Some(f)
            }
            _ => None,
        }
    }

    /// Whether a part draws no DC current by its model (ADR 0027): it states no `current`,
    /// has no rail or other interface that draws, and every pin of its tables is `passive`
    /// (a capacitor, a TVS diode, a crystal, a header, a test point). A part with no `pins`
    /// table has no pin kinds to go by and is not taken for one; a part that does conduct
    /// must state a `current` or a rail to be counted.
    pub fn draws_nothing(&self, part: InstId) -> bool {
        let inst = self.instance(part);
        if inst.kind != InstKind::Part || inst.tables.is_empty() {
            return false;
        }
        let states_current = inst.children.iter().any(|(name, m)| match m {
            Member::Field(_) => name == "current",
            Member::Inst(i) => self.draw_of(*i).is_some(),
            Member::Poison => false,
        });
        !states_current
            && inst
                .tables
                .iter()
                .all(|t| t.rows.iter().all(|r| r.kind == PinKind::Passive))
    }

    /// Whether a pin draws nothing by its part's model (ADR 0027): the part draws nothing
    /// ([`Model::draws_nothing`]), or the part declares its supply through a rail (an
    /// interface that draws, so the draw is the rail's) and the pin is an `input` or a
    /// `no_connect`, in every table that has it. An `input` only leaks, which is ignored
    /// like a quiescent current; a part with no rail has no supply the draw could belong
    /// to, so its inputs stay unmodelled.
    pub fn pin_draws_nothing(&self, pin: InstId) -> bool {
        let Some(part) = self.owner_of(pin) else {
            return false;
        };
        if self.draws_nothing(part) {
            return true;
        }
        let inst = self.instance(part);
        let has_rail = inst
            .children
            .iter()
            .any(|(_, m)| matches!(m, Member::Inst(i) if self.draw_of(*i).is_some()));
        let mut kinds = inst
            .tables
            .iter()
            .filter_map(|t| t.row(pin))
            .map(|r| r.kind);
        has_rail
            && inst.kind == InstKind::Part
            && kinds.clone().next().is_some()
            && kinds.all(|k| matches!(k, PinKind::Input | PinKind::NoConnect))
    }

    /// The `voltage` field and the `pos` and `neg` pins of an interface that is a DC supply,
    /// such as a `Rail`.
    pub fn rail_of(&self, iface: InstId) -> Option<(FieldId, InstId, InstId)> {
        if self.instance(iface).kind != InstKind::Interface {
            return None;
        }
        match (
            self.member(iface, "voltage")?,
            self.member(iface, "pos")?,
            self.member(iface, "neg")?,
        ) {
            (Member::Field(v), Member::Inst(pos), Member::Inst(neg))
                if self.field(v).kind == FieldKind::Quantity(Dimension::VOLTAGE) =>
            {
                Some((v, pos, neg))
            }
            _ => None,
        }
    }

    /// The member `.environment` of the board a part is on, if the board has one: an
    /// interface (ADR 0099). Each board of a system has its own.
    pub fn environment_of(&self, part: InstId) -> Option<InstId> {
        let board = self.board_of(part)?;
        match self.member(board, "environment")? {
            Member::Inst(env) if self.instance(env).kind == InstKind::Interface => Some(env),
            _ => None,
        }
    }

    /// The temperature field of the environment of the board a part is on, `.environment.
    /// temperature`. A board with no `.environment`, or one whose environment has no
    /// temperature of that kind, has none.
    pub fn environment_temperature_of(&self, part: InstId) -> Option<FieldId> {
        let env = self.environment_of(part)?;
        match self.member(env, "temperature")? {
            Member::Field(f)
                if self.field(f).kind == FieldKind::Quantity(Dimension::TEMPERATURE) =>
            {
                Some(f)
            }
            _ => None,
        }
    }

    /// The `qualification` field of the environment of the board a part is on, the
    /// qualification the board requires of every fitted part (ADR 0099). A board with no
    /// `.environment`, or one whose environment has no `qualification` of that kind, has none.
    pub fn environment_qualification_of(&self, part: InstId) -> Option<FieldId> {
        let env = self.environment_of(part)?;
        match self.member(env, "qualification")? {
            Member::Field(f) if self.is_qualification(f) => Some(f),
            _ => None,
        }
    }

    /// Whether a field is of the enum `Qualification`.
    fn is_qualification(&self, field: FieldId) -> bool {
        match self.field(field).kind {
            FieldKind::Enum(e) => self.enums[e.0].name == "Qualification",
            _ => false,
        }
    }

    /// The `temperature_shift` and `voltage_over_temperature` fields of a part, how far the
    /// temperature goes from the datasheet's 25 C and the voltage over that shift (the `Zener`,
    /// ADR 0080). A part with only one of them, or with them of another kind, has none; the
    /// checker derives the shift from the board's environment (`Checker::derive_temperature_shift`).
    pub fn temperature_shift_of(&self, part: InstId) -> Option<(FieldId, FieldId)> {
        if self.instance(part).kind != InstKind::Part {
            return None;
        }
        let field = |name: &str, dim: Dimension| match self.member(part, name)? {
            Member::Field(f) if self.field(f).kind == FieldKind::Quantity(dim) => Some(f),
            _ => None,
        };
        Some((
            field("temperature_shift", Dimension::TEMPERATURE)?,
            field("voltage_over_temperature", Dimension::VOLTAGE)?,
        ))
    }

    /// The `temp_min` and `temp_max` fields of a part, the least and the greatest temperature
    /// it works at (ADR 0099). Every part has both; anything else has none.
    pub fn temperature_range_of(&self, part: InstId) -> Option<(FieldId, FieldId)> {
        if self.instance(part).kind != InstKind::Part {
            return None;
        }
        let field = |name: &str| match self.member(part, name)? {
            Member::Field(f)
                if self.field(f).kind == FieldKind::Quantity(Dimension::TEMPERATURE) =>
            {
                Some(f)
            }
            _ => None,
        };
        Some((field("temp_min")?, field("temp_max")?))
    }

    /// The `bias` field of a part, the DC voltage across it, and the two terminals its
    /// `bridge` runs between. A part with both has its bias derived from the rail it sits
    /// across (see `Checker::derive_bias`).
    pub fn bias_of(&self, part: InstId) -> Option<(FieldId, InstId, InstId)> {
        if self.instance(part).kind != InstKind::Part {
            return None;
        }
        let (a, b) = self.instance(part).bridge?;
        match self.member(part, "bias")? {
            Member::Field(f) if self.field(f).kind == FieldKind::Quantity(Dimension::VOLTAGE) => {
                Some((f, a, b))
            }
            _ => None,
        }
    }

    /// The `bridge_voltage` field of a part, the voltage from the first terminal of its
    /// `bridge` to the second, and the two terminals. Unlike `bias` it has a sign, so a part
    /// that has it is given the rail's voltage the way round it is wired (see
    /// `Checker::derive_bias`). A part without a `bridge`, or without a voltage field of
    /// that name, has none.
    pub fn bridge_voltage_of(&self, part: InstId) -> Option<(FieldId, InstId, InstId)> {
        if self.instance(part).kind != InstKind::Part {
            return None;
        }
        let (a, b) = self.instance(part).bridge?;
        match self.member(part, "bridge_voltage")? {
            Member::Field(f) if self.field(f).kind == FieldKind::Quantity(Dimension::VOLTAGE) => {
                Some((f, a, b))
            }
            _ => None,
        }
    }

    /// The `supply_peak` field of a part, the highest voltage of the rail a `~>` chain it is
    /// in starts on (see `Checker::derive_chain_values`). A part without a `bridge`, or
    /// without a voltage field of that name, is not given one.
    pub fn supply_peak_of(&self, part: InstId) -> Option<FieldId> {
        self.chain_field_of(part, "supply_peak", Dimension::VOLTAGE)
    }

    /// The `supply_reverse_peak` field of a part, the highest voltage the supply of the `~>`
    /// chain it is in reaches the wrong way round, as a positive number (ADR 0091).
    pub fn supply_reverse_peak_of(&self, part: InstId) -> Option<FieldId> {
        self.chain_field_of(part, "supply_reverse_peak", Dimension::VOLTAGE)
    }

    /// The `supply_fault_current` field of a part, the largest current that the part in front
    /// of it in a `~>` chain, one that has a `limit_current`, lets through (ADR 0091).
    pub fn supply_fault_current_of(&self, part: InstId) -> Option<FieldId> {
        self.chain_field_of(part, "supply_fault_current", Dimension::CURRENT)
    }

    /// The `limit_current` field of a part, the most current it lets through whatever is
    /// behind it: a fuse's rating (ADR 0091).
    pub fn limit_current_of(&self, part: InstId) -> Option<FieldId> {
        self.chain_field_of(part, "limit_current", Dimension::CURRENT)
    }

    /// A field of a part with a `bridge` that the checker reads by its name, of a quantity.
    fn chain_field_of(&self, part: InstId, name: &str, dim: Dimension) -> Option<FieldId> {
        if self.instance(part).kind != InstKind::Part {
            return None;
        }
        self.instance(part).bridge?;
        match self.member(part, name)? {
            Member::Field(f) if self.field(f).kind == FieldKind::Quantity(dim) => Some(f),
            _ => None,
        }
    }

    /// The `peak` field of a rail: the highest voltage it reaches, transients included.
    pub fn peak_of(&self, rail: InstId) -> Option<FieldId> {
        self.rail_of(rail)?;
        match self.member(rail, "peak")? {
            Member::Field(f) if self.field(f).kind == FieldKind::Quantity(Dimension::VOLTAGE) => {
                Some(f)
            }
            _ => None,
        }
    }

    /// The `reverse_peak` field of a rail: the highest voltage it reaches the wrong way round,
    /// as a positive number (ADR 0091). A rail from outside the standard library may not
    /// have one.
    pub fn reverse_peak_of(&self, rail: InstId) -> Option<FieldId> {
        self.rail_of(rail)?;
        match self.member(rail, "reverse_peak")? {
            Member::Field(f) if self.field(f).kind == FieldKind::Quantity(Dimension::VOLTAGE) => {
                Some(f)
            }
            _ => None,
        }
    }

    /// Whether `field` is a field of a part's or module's built-in `.place` (ADR 0045). The
    /// placer and the check read such a field as a requirement on a position, so a bound on
    /// it (`>=`, `<=`) narrows it, where a bound on any other field is a claim to verify.
    pub fn is_place_field(&self, field: FieldId) -> bool {
        let owner = self.instance(self.field(field).owner);
        owner.kind == InstKind::Interface && owner.type_name == "Place" && owner.def.is_none()
    }

    /// Whether an instance is the built-in `.model` of a part (ADR 0108): its `StepModel`.
    pub fn is_step_model(&self, inst: InstId) -> bool {
        let i = self.instance(inst);
        i.name == "model"
            && i.type_name == "StepModel"
            && i.parent
                .is_some_and(|p| self.instance(p).kind == InstKind::Part)
    }

    /// The part, module or board that an interface belongs to, looking through any
    /// interfaces it is nested in.
    pub fn owner_of(&self, inst: InstId) -> Option<InstId> {
        let mut cur = self.instance(inst).parent;
        while let Some(p) = cur {
            if self.instance(p).kind != InstKind::Interface {
                return Some(p);
            }
            cur = self.instance(p).parent;
        }
        None
    }

    pub fn member(&self, inst: InstId, name: &str) -> Option<Member> {
        self.instance(inst)
            .children
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, m)| *m)
    }

    /// Find something by a dotted path from the root: `led.forward_voltage`.
    pub fn lookup(&self, path: &str) -> Option<Member> {
        let mut current = Member::Inst(self.root);
        for part in path.split('.').filter(|p| !p.is_empty()) {
            let Member::Inst(inst) = current else {
                return None;
            };
            current = self.member(inst, part)?;
        }
        Some(current)
    }

    pub fn inst_at(&self, path: &str) -> Option<InstId> {
        match self.lookup(path)? {
            Member::Inst(i) => Some(i),
            _ => None,
        }
    }

    pub fn field_at(&self, path: &str) -> Option<FieldId> {
        match self.lookup(path)? {
            Member::Field(f) => Some(f),
            _ => None,
        }
    }

    pub fn domain_at(&self, path: &str) -> Option<&Domain> {
        self.field_at(path).map(|f| &self.field(f).domain)
    }

    /// `led.anode`, relative to the root (the root's own name is left out).
    pub fn path_of(&self, inst: InstId) -> String {
        self.path_below(inst, self.root)
    }

    /// The path of `inst` from `ancestor`, which is left out. The whole path to the top
    /// if `inst` is not inside `ancestor`.
    pub fn path_below(&self, inst: InstId, ancestor: InstId) -> String {
        let mut names = Vec::new();
        let mut cur = inst;
        while cur != ancestor {
            let Some(parent) = self.instance(cur).parent else {
                break;
            };
            names.push(self.instance(cur).name.clone());
            cur = parent;
        }
        names.reverse();
        names.join(".")
    }

    pub fn path_of_field(&self, field: FieldId) -> String {
        let f = self.field(field);
        let owner = self.path_of(f.owner);
        if owner.is_empty() {
            f.name.clone()
        } else {
            format!("{owner}.{}", f.name)
        }
    }

    pub(crate) fn add_instance(&mut self, inst: Instance) -> InstId {
        self.instances.push(inst);
        InstId(self.instances.len() - 1)
    }

    pub(crate) fn add_field(&mut self, owner: InstId, name: &str, kind: FieldKind) -> FieldId {
        self.fields.push(Field {
            owner,
            name: name.to_string(),
            kind,
            domain: Domain::Unset,
            narrowed_at: Vec::new(),
            optional: None,
        });
        FieldId(self.fields.len() - 1)
    }

    pub(crate) fn add_member(&mut self, owner: InstId, name: &str, member: Member) {
        self.instances[owner.0]
            .children
            .push((name.to_string(), member));
    }
}

impl std::fmt::Display for Domain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unset => write!(f, "unconstrained"),
            Self::Quantity(q) => write!(f, "{q}"),
            Self::Text(t) => write!(f, "\"{t}\""),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &str) -> Domain {
        Domain::Text(t.to_string())
    }

    fn domains(m: &Model) -> Vec<Domain> {
        m.fields().map(|(_, f)| f.domain.clone()).collect()
    }

    /// A bookmark is gone back to after changes, inside and outside checkpoints, and a later
    /// bookmark is gone back to after going back to an earlier one, as the solver's retry does.
    #[test]
    fn a_bookmark_puts_the_domains_back_from_before_or_after_it() {
        let mut m = Model::new();
        let a = m.add_field(InstId(0), "a", FieldKind::Text);
        let b = m.add_field(InstId(0), "b", FieldKind::Text);
        let _c = m.add_field(InstId(0), "c", FieldKind::Text);
        m.set_domain(a, text("a0"));
        let first = m.bookmark();
        let at_first = domains(&m);
        m.set_domain(a, text("a1"));
        m.set_domain(b, text("b1"));
        let kept = m.checkpoint();
        m.set_domain(b, text("b2"));
        m.commit(kept);
        let tried = m.checkpoint();
        m.set_domain(a, text("a3"));
        m.rollback(tried);
        let second = m.bookmark();
        let at_second = domains(&m);
        m.set_domain(a, text("a4"));
        m.go_back_to(&first);
        assert_eq!(domains(&m), at_first);
        m.go_back_to(&second);
        assert_eq!(domains(&m), at_second);
        m.go_back_to(&first);
        assert_eq!(domains(&m), at_first);
        drop((first, second));
        // With no bookmark held and no checkpoint open, nothing is kept.
        let closed = m.checkpoint();
        m.commit(closed);
        assert!(m.journal.is_empty());
    }
}
