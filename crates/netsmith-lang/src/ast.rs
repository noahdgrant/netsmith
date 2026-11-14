//! Syntax tree. Every node keeps its source span so later stages can point at the
//! text that caused a problem. Comments are discarded by the lexer.

use crate::error::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct File {
    /// `//!` lines at the top of the file.
    pub inner_docs: Vec<String>,
    pub imports: Vec<Import>,
    pub definitions: Vec<Definition>,
    /// `physical for Board { ... }` units (ADR 0048). Only a physical file
    /// (`<stem>.physical.nsm`) may hold them; the checker says so for any other file.
    pub physicals: Vec<Physical>,
}

/// `physical for Trellis { ... }`: the physical intent of one board (ADR 0048, "Syntax
/// decisions" 1). Its statements are the language of the board's body; the checker puts
/// them with the board it names and lets them reach the parts inside its modules, to state
/// physical intent only.
#[derive(Debug, Clone, PartialEq)]
pub struct Physical {
    /// `///` lines above the unit.
    pub docs: Vec<String>,
    /// `//!` lines at the start of the body.
    pub inner_docs: Vec<String>,
    /// The board it belongs to (`Trellis`).
    pub board: Ident,
    pub items: Vec<Item>,
    pub span: Span,
}

/// `case { ... }`: the section of a physical file that holds the outline, openings,
/// mounting holes, fiducials and labels of the board and what the case fixes (ADR 0048,
/// "Syntax decisions" 7). It can be lifted out unchanged into a shared definition later.
#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    pub items: Vec<Item>,
    pub span: Span,
}

/// `support { .boost: Capacitor { ... }  .bst ~> .boost ~> .sw.pos }`: the circuit that a
/// part's datasheet asks for around it, which is placed wherever the part is (ADR 0115).
#[derive(Debug, Clone, PartialEq)]
pub struct Support {
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ident {
    pub name: String,
    /// Written as `'5'`, for names that are not plain identifiers.
    pub quoted: bool,
    pub span: Span,
}

/// `import a::b::X` (`names` is `None`) or `import a::b::{X, Y}`.
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub path: Vec<Ident>,
    pub names: Option<Vec<Ident>>,
    pub span: Span,
}

/// A top-level `board`, `system`, `panel`, `part`, `interface`, `module`, `enum` or `generic`. They share one body grammar;
/// what each kind may contain is a later stage's concern.
#[derive(Debug, Clone, PartialEq)]
pub struct Definition {
    /// `///` lines above the definition.
    pub docs: Vec<String>,
    /// `//!` lines at the start of the body.
    pub inner_docs: Vec<String>,
    /// `#[bank]`, `#[series(E24, E96)]`, ... above the definition, in the order written
    /// (ADR 0125). The checker says which a definition may carry.
    pub attrs: Vec<Attribute>,
    pub kind: DefKind,
    pub name: Ident,
    /// The generics a `part` or a `module` provides (`part X provides A, B`, ADR 0068), in the
    /// order written. Empty for every other kind.
    pub provides: Vec<Ident>,
    /// The type parameters of a `module` (`module Buck<Ic: BuckIc>`, ADR 0115), in the order
    /// written. Empty for every other kind, and for a module that takes none.
    pub params: Vec<TypeParam>,
    pub items: Vec<Item>,
    /// The variants of an `enum` (ADR 0054), in the order written. Empty for every other kind.
    pub variants: Vec<Variant>,
    pub span: Span,
}

/// `#[series(E24, E96)]`: an attribute, which says what a definition or a member is to the
/// tools, in a way a project's own definition can say too (ADR 0125). The parser takes any
/// name and arguments; the checker knows the attributes and refuses the rest.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    /// `series`.
    pub name: Ident,
    /// `E24` and `E96`, in the order written. Empty for `#[bank]`, and for `#[bank()]`.
    pub args: Vec<AttrArg>,
    /// From `#[` to `]`.
    pub span: Span,
}

/// One argument of an [`Attribute`]: a name (`E24`, `bias`) or a text (`"LED"`).
#[derive(Debug, Clone, PartialEq)]
pub enum AttrArg {
    Name(Ident),
    Text { text: String, span: Span },
}

impl AttrArg {
    pub fn span(&self) -> Span {
        match self {
            Self::Name(i) => i.span,
            Self::Text { span, .. } => *span,
        }
    }
}

/// `Ic: BuckIc` in `module Buck<Ic: BuckIc>`: a type parameter of a module, a part that
/// provides the generic after the colon, its bound (ADR 0115).
#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    pub name: Ident,
    pub bound: Ident,
}

/// One variant of an `enum`: `Hand`, with the `///` lines above it.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub docs: Vec<String>,
    pub name: Ident,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefKind {
    /// One physical PCB: stackup, parts, and the circuits placed on it.
    Board,
    /// Several boards and the ports of theirs that are joined.
    System,
    /// One board repeated in a grid on one fabrication panel (ADR 0040).
    Panel,
    /// A physical component: pins, quantities, footprint.
    Part,
    /// A typed bundle of pins and quantities that is connected as one (a power rail).
    Interface,
    /// A reusable sub-circuit.
    Module,
    /// A type with a fixed set of named values (ADR 0054).
    Enum,
    /// What the users of a part or module may rely on, and what a part or module that
    /// provides it must have (ADR 0068).
    Generic,
}

impl DefKind {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Board => "board",
            Self::System => "system",
            Self::Panel => "panel",
            Self::Part => "part",
            Self::Interface => "interface",
            Self::Module => "module",
            Self::Enum => "enum",
            Self::Generic => "generic",
        }
    }

    pub fn from_keyword(word: &str) -> Option<Self> {
        [
            Self::Board,
            Self::System,
            Self::Panel,
            Self::Part,
            Self::Interface,
            Self::Module,
            Self::Enum,
            Self::Generic,
        ]
        .into_iter()
        .find(|k| k.keyword() == word)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Decl(Decl),
    Constraint(Constraint),
    Connect(Connect),
    /// `.headers ~ .module { gpio4 = io4 } except { gnd }`: the members of two ports joined
    /// name by name, with renames or members left out (ADR 0117). Without a map or `except` the
    /// statement is a [`Connect`], and the checker joins two ports by name the same way.
    JoinMap(JoinMap),
    Bridge(Bridge),
    /// `dc resistor .a .b .resistance`: what a part is at DC (ADR 0076, amendment 1).
    Dc(Dc),
    Catalog(Catalog),
    Pins(Pins),
    Tie(Tie),
    /// `unused .processor.usb`: the board says that nothing on it uses an interface (ADR 0083).
    Unused(Unused),
    /// `peripheral UARTE0 { UARTE0_TX:output UARTE0_RX:input }`; only a part may hold one
    /// (ADR 0071).
    Peripheral(Peripheral),
    /// `shared SERIAL0 { UARTE0 SPIM0 }`; only a part may hold one (ADR 0071).
    Shared(Shared),
    /// `lines SpiRole::Slave { .sclk input ... }`; only an interface may hold one (ADR 0064).
    Lines(Lines),
    /// A `case { ... }` section; only a physical file may hold one.
    Case(Case),
    /// A `support { ... }` section: the parts a part's datasheet application circuit puts
    /// around it and their connections to its pins; only a part may hold one (ADR 0115).
    Support(Support),
    /// A `row .a1 .. .a12 along x pitch 2.54mm` statement; only a physical file may hold one.
    Row(Row),
    /// A `polygon (0mm, 0mm) (40mm, 0mm) ...` statement; only an `Outline` in a physical
    /// file may hold one (issue #57).
    Polygon(Polygon),
}

/// `polygon (0mm, 0mm) (40mm, 0mm) round 2mm (40mm, 15mm) arc through (30mm, 20mm) (0mm, 15mm)`:
/// the board edge as an ordered run of corners (issue #57, owner decision 2026-11-12). The edge
/// runs from each corner to the next and from the last back to the first.
#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
    pub corners: Vec<PolygonCorner>,
    pub span: Span,
}

/// One corner of a [`Polygon`], with what follows it: `round R`, a tangent fillet of radius R
/// at the corner, and `arc through (x, y)`, which makes the edge to the next corner an arc
/// through that point.
#[derive(Debug, Clone, PartialEq)]
pub struct PolygonCorner {
    pub at: Point,
    pub round: Option<Expr>,
    pub through: Option<Point>,
    pub span: Span,
}

/// `(x, y)`: a point of a [`Polygon`].
#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    pub x: Expr,
    pub y: Expr,
    pub span: Span,
}

/// `row .pads.tp1 .. .pads.tp12 along x pitch 2.54mm +/- 0.05mm`: a numbered run of parts on
/// one line, each neighbour at the pitch along the axis (ADR 0048, "Syntax decisions" 10).
/// The checker lowers it to the same relations the equation form states.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The first part of the run (`.pads.tp1`).
    pub first: Path,
    /// The last part of the run (`.pads.tp12`).
    pub last: Path,
    /// The axis the pitch is along.
    pub axis: Axis,
    pub axis_span: Span,
    /// The distance between a part and the next: a length, with or without a window.
    pub pitch: Expr,
    pub span: Span,
}

/// `x` or `y` after `along`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

impl Axis {
    pub fn word(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
        }
    }
}

/// What a pin is electrically. The kinds are a closed set (ADR 0016); a checker reads them,
/// no CAD tool is named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinKind {
    /// A pin of a supply: the sign of the rail's current says whether it takes or gives.
    Power,
    Input,
    Output,
    Bidirectional,
    Passive,
    /// A pin that must stay unconnected.
    NoConnect,
    /// An output that drives only while its part is selected and is high impedance
    /// otherwise: it may share a net with other drivers (ADR 0064).
    TriState,
    /// An output that only pulls its line low, or lets it go (open drain, or open collector):
    /// the line's high comes from a pull-up, and several may share a net (#460).
    OpenDrain,
}

impl PinRow {
    /// Whether this is a row of a generic's contract: it has no name and no pads.
    pub fn is_contract(&self) -> bool {
        self.pads.is_empty()
    }
}

impl PinKind {
    pub const ALL: [PinKind; 8] = [
        Self::Power,
        Self::Input,
        Self::Output,
        Self::Bidirectional,
        Self::Passive,
        Self::NoConnect,
        Self::TriState,
        Self::OpenDrain,
    ];

    /// The word for it in source and in messages.
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

    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.word() == word)
    }
}

/// Whether a pin must be connected whatever its kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    /// `optional`: an input or power pin that may float.
    Optional,
    /// `required`: an output, bidirectional or passive pin that may not.
    Required,
}

impl Requirement {
    pub fn word(self) -> &'static str {
        match self {
            Self::Optional => "optional",
            Self::Required => "required",
        }
    }
}

/// A pad number, which is text: `"1"`, `"A5"`, `"EP"`.
#[derive(Debug, Clone, PartialEq)]
pub struct Pad {
    pub text: String,
    pub span: Span,
}

/// One function of a pin: `UART1_TX`, or `UART1_TX:output` when the function has its own
/// kind (ADR 0046), as KiCad's alternate pin definitions each have their own electrical
/// type. A function written without a kind has the pin's.
#[derive(Debug, Clone, PartialEq)]
pub struct PinFunction {
    pub name: Ident,
    pub kind: Option<(PinKind, Span)>,
    /// `class` after the function in a pin row (`fn GPIO class`): the function is a class, which
    /// the board may choose on any number of pins of the part. Always None in a peripheral.
    pub class: Option<Span>,
    /// `as sclk`: in a peripheral that has a bus (`as Spi SpiRole::Master`), the line of the bus
    /// that the function is (ADR 0064). Always None in a pin row.
    pub line: Option<Ident>,
}

/// One line of a pin table: `.sense input "SENSE" "3" fn GPIO UART1_TX optional`.
///
/// In a `generic` (ADR 0068) a row may have no name and no pads, `.signal passive required`:
/// it states the kind a provider's row for the pin must have. Such a row has an empty `name`
/// and no `pads`; every row of a part has at least one pad.
#[derive(Debug, Clone, PartialEq)]
pub struct PinRow {
    /// A plain pin of the part (`.sense`), which the row declares, or a pin of one of
    /// its interfaces (`.vout.pos`).
    pub pin: Path,
    pub kind: PinKind,
    /// The name the datasheet prints.
    pub name: String,
    pub name_span: Span,
    pub pads: Vec<Pad>,
    /// What the pin can be, in the order written. Empty for a pin with one purpose.
    pub functions: Vec<PinFunction>,
    pub requirement: Option<(Requirement, Span)>,
    /// `abs_max 65V` or `abs_max 4V from .gnd` at the end of the row.
    pub abs_max: Option<AbsMax>,
    pub span: Span,
}

/// `abs_max 4V from .gnd` at the end of a pin row: the pin's absolute maximum, the voltage its
/// net must not exceed above the reference pin (ADR 0124). With no `from` the reference is the
/// ground of the pin's `Logic`, else the one rail ground of the table.
#[derive(Debug, Clone, PartialEq)]
pub struct AbsMax {
    pub value: Expr,
    pub from: Option<Path>,
    /// The word `abs_max`.
    pub span: Span,
}

/// `pins "DFN-8-EP" { ... }`: the datasheet's pin table of a part in one or more packages
/// (`pins "SOT-23" "SOT-223" { ... }`, which share the table), or `pins { ... }` for a part
/// whose pads do not depend on the package.
#[derive(Debug, Clone, PartialEq)]
pub struct Pins {
    /// The packages of the header, in the order written; empty for an unnamed table.
    pub packages: Vec<(String, Span)>,
    pub rows: Vec<PinRow>,
    pub span: Span,
}

/// `tie .sense ~ .vout.pos`: the board must put the two pins on one net.
#[derive(Debug, Clone, PartialEq)]
pub struct Tie {
    pub a: Path,
    pub b: Path,
    pub span: Span,
}

/// `unused .processor.usb`: a board states that the interface named is not used by anything on
/// it, so the limits the part states for that function, the sources its pins would need and the
/// level checks of its pins do not apply (ADR 0083).
#[derive(Debug, Clone, PartialEq)]
pub struct Unused {
    pub path: Path,
    pub span: Span,
}

/// `peripheral UARTE0 on APP_GPIO { UARTE0_TX:output UARTE0_RX:input }`: the functions of a part
/// that belong together and are in use together (ADR 0071). A function takes its kind from the
/// peripheral, as it would from a pin row (ADR 0046). With `on CLASS`, every pin whose row lists
/// the function `CLASS` offers the peripheral's functions as well.
#[derive(Debug, Clone, PartialEq)]
pub struct Peripheral {
    pub name: Ident,
    /// The class after `on`: a function that rows list.
    pub on: Option<Ident>,
    /// `as Spi SpiRole::Master`: the bus interface that the peripheral is an end of, and its role
    /// (ADR 0064, stage 4). A function that names a line (`SPIM0_SCK:output as sclk`) is a pin of
    /// that bus when the board chooses it on a pin.
    pub bus: Option<PeripheralBus>,
    pub functions: Vec<PinFunction>,
    pub span: Span,
}

/// The `as Spi SpiRole::Master` of a peripheral.
#[derive(Debug, Clone, PartialEq)]
pub struct PeripheralBus {
    pub interface: TypePath,
    pub role: Option<Expr>,
    pub span: Span,
}

/// `shared UARTS limit 2 { UARTE0 UARTE1 .p10 }`: the hardware that its members use at most
/// `limit` of at a time, one when it states none (ADR 0071). A member is a peripheral or a
/// function, by name, or a pin.
#[derive(Debug, Clone, PartialEq)]
pub struct Shared {
    pub name: Ident,
    /// `limit N`: how many members may be in use at once.
    pub limit: Option<(u32, Span)>,
    pub members: Vec<SharedMember>,
    pub span: Span,
}

/// `lines SpiRole::Master limit 1 { .sclk output  .miso input optional }`: for each role of a bus
/// interface, the kind of each line and a few properties of a line (ADR 0064). The role is a
/// variant of an enum; an interface with one role states none (`lines limit 2 { ... }`).
#[derive(Debug, Clone, PartialEq)]
pub struct Lines {
    /// The variant after `lines`: `SpiRole::Master`.
    pub role: Option<Expr>,
    /// `limit N`: how many interfaces of this role may be on one bus.
    pub limit: Option<(u32, Span)>,
    pub rows: Vec<LineRow>,
    pub span: Span,
}

/// One row of a `lines` table: `.tx output crosses .rx`, `.miso output required`,
/// `.cs input single`.
#[derive(Debug, Clone, PartialEq)]
pub struct LineRow {
    /// The pin of the interface that the row is about (`.sclk`).
    pub pin: Path,
    pub kind: PinKind,
    /// `optional` or `required`, as on a pin row.
    pub requirement: Option<(Requirement, Span)>,
    /// `single`: a net carries this line of at most one interface of the role.
    pub single: Option<Span>,
    /// `crosses .rx`: the line is joined to this one of the other end, not to its own name.
    pub crosses: Option<Path>,
    pub span: Span,
}

/// A member of a `shared` resource: a name (a peripheral or a function of the part) or a pin
/// (`.p10`).
#[derive(Debug, Clone, PartialEq)]
pub enum SharedMember {
    Name(Ident),
    Pin(Path),
}

impl SharedMember {
    pub fn span(&self) -> Span {
        match self {
            Self::Name(n) => n.span,
            Self::Pin(p) => p.span,
        }
    }
}

/// `bridge .a ~> .b`: a two-terminal part that a `~>` chain passes through,
/// entering at `from` and leaving at `to`.
#[derive(Debug, Clone, PartialEq)]
pub struct Bridge {
    pub from: Path,
    pub to: Path,
    pub span: Span,
}

/// `dc resistor .a .b .resistance`: the element a part is in a DC analysis, then the operands
/// of that element in the order its kind fixes (pins first, then values). The parser keeps the
/// operands as written; the checker knows each kind's operands (ADR 0076, amendment 1).
#[derive(Debug, Clone, PartialEq)]
pub struct Dc {
    /// The word after `dc`: `resistor`, `diode`, `nmos` or `pmos`.
    pub kind: Ident,
    pub operands: Vec<Path>,
    pub span: Span,
}

/// `catalog resistor { .resistance, .package }`: the columns of a parts catalog that
/// this part's fields are matched against. `catalog mcu by .mpn { .mpn, .package }` names
/// the column that picks one row, which the design must set. A column may end with
/// `optional` (`catalog tvs { .clamping_voltage, .breakdown_voltage optional }`, ADR 0078):
/// a file may leave it out and a row may leave its cell empty.
#[derive(Debug, Clone, PartialEq)]
pub struct Catalog {
    pub name: Ident,
    pub key: Option<Path>,
    pub fields: Vec<Path>,
    /// One entry per column of `fields`: where its `optional` is, if it has one.
    pub optional: Vec<Option<Span>>,
    pub span: Span,
}

/// `.name: Type` with an optional `{ ... }` body of further items.
#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    /// `///` lines above the declaration.
    pub docs: Vec<String>,
    /// `#[role(bias)]`, ... above the declaration, in the order written (ADR 0125).
    pub attrs: Vec<Attribute>,
    pub name: Path,
    pub ty: TypePath,
    /// `4 x` before the type: a group of that many identical members, `.decoupling[0]` to
    /// `.decoupling[3]` (ADR 0119). `None` for a declaration of one member.
    pub count: Option<Count>,
    /// `optional` after the type (ADR 0074): a value that a design may leave unstated, and
    /// the checks that read it are then skipped. The word's place.
    pub optional: Option<Span>,
    pub body: Option<Vec<Item>>,
    pub span: Span,
}

/// `4 x` in `.decoupling: 4 x Capacitor` or `~> 2 x Capacitor { ... } ~>`: how many identical
/// members a group has (ADR 0119). Always at least two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Count {
    pub n: u32,
    /// The number and the `x`.
    pub span: Span,
}

/// `2 x Capacitor { .capacitance &= 100nF }` written in a chain, between its two ends
/// (`.v.pos ~> 2 x Capacitor { ... } ~> .v.neg`, ADR 0119): a group with no name of its own,
/// each member in parallel between the ends.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineGroup {
    /// Where the group is in the chain's `rest`. The path there is the name the group is
    /// given (`.capacitor_v_pos_v_neg`), with the span of the group.
    pub at: usize,
    pub count: Count,
    pub ty: TypePath,
    pub body: Option<Vec<Item>>,
    pub span: Span,
}

/// `std::parts::Resistor`, or `.u.GPIO` (a pin request, `of_member`): a member path whose last
/// segment is a capability of the part the segments before it name.
#[derive(Debug, Clone, PartialEq)]
pub struct TypePath {
    pub segments: Vec<Ident>,
    /// `<Lt8641>` after a module's name: the parts that stand for its type parameters (ADR
    /// 0115), in the order of the parameters. Empty for a type written without them.
    pub args: Vec<TypePath>,
    /// Written with a leading dot and joined by dots: a request for a pin of the part that
    /// the path names, not the name of a type.
    pub of_member: bool,
}

/// `.supply.pos` (local) or `supply.pos` (looked up outward).
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub leading_dot: bool,
    pub segments: Vec<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    pub lhs: Expr,
    pub op: CmpOp,
    pub rhs: Expr,
    /// `at 2mA` after the right side: the load a datasheet measures a level at (ADR 0076,
    /// amendment 1). The statement means what it does without it.
    pub at: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    /// `&=` narrow a field to the intersection with a set.
    Narrow,
    /// `?=` give a field a default: its value if no `&=` statement states the field.
    /// The checker resolves it, so the solver never sees this operator.
    Default,
    /// `==` a relation that must hold in the worst case; a field with no limit yet
    /// takes the computed value.
    EqEq,
    /// `>=`
    Ge,
    /// `<=`
    Le,
}

/// `a ~> b ~> c`: operands joined by `~` (same net) or `~>` (series).
#[derive(Debug, Clone, PartialEq)]
pub struct Connect {
    pub first: Path,
    pub rest: Vec<(ConnectOp, Path)>,
    /// The groups written in the chain itself (`~> 2 x Capacitor { ... } ~>`, ADR 0119). Each
    /// stands in `rest` as the path of the name it is given; empty for most chains.
    pub groups: Vec<InlineGroup>,
    pub span: Span,
}

/// `.left ~ .right { a = b } except { c }`: joins each pin or interface of `left` to
/// the member of `right` with the same name, or with the name `renames` gives it, and leaves out
/// the names `except` lists (ADR 0117).
#[derive(Debug, Clone, PartialEq)]
pub struct JoinMap {
    pub left: Path,
    pub right: Path,
    /// `gpio4 = io4`: a member of `left` (the first name) and the member of `right` it is joined
    /// to, in source order.
    pub renames: Vec<(Ident, Ident)>,
    /// Members of either side that are not joined, in source order.
    pub except: Vec<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectOp {
    Same,
    Series,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(Number, Span),
    Str(String, Span),
    Path(Path),
    /// `AssemblyMode::Hand`: a variant of an enum (ADR 0054). `ty` is the enum's name as
    /// written (`AssemblyMode`), `variant` the name after the last `::`.
    Variant {
        ty: TypePath,
        variant: Ident,
        span: Span,
    },
    Neg(Box<Expr>, Span),
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    /// `lo..hi`
    Range {
        lo: Box<Expr>,
        hi: Box<Expr>,
        span: Span,
    },
    /// `center +/- tol`
    Tolerance {
        center: Box<Expr>,
        tol: Box<Expr>,
        span: Span,
    },
    Paren(Box<Expr>, Span),
    /// `name(arg, ...)`: a call of a function the checker knows (`min(.bias)`, `max(.bias)`,
    /// ADR 0098). The parser reads any name; whether it is a function, and the count and kind
    /// of its arguments, are the checker's to say.
    Call {
        name: Ident,
        args: Vec<Expr>,
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Self::Number(_, s)
            | Self::Str(_, s)
            | Self::Neg(_, s)
            | Self::Paren(_, s)
            | Self::Binary { span: s, .. }
            | Self::Variant { span: s, .. }
            | Self::Call { span: s, .. }
            | Self::Range { span: s, .. }
            | Self::Tolerance { span: s, .. } => *s,
            Self::Path(p) => p.span,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// A numeric literal with an optional unit suffix, as written (`1.8V`, `5%`, `0x1A`).
/// Resolving SI prefixes and dimensions is a later stage's job.
#[derive(Debug, Clone, PartialEq)]
pub struct Number {
    pub text: String,
    pub value: f64,
    pub unit: Option<String>,
}
