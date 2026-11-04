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

/// A top-level `board`, `system`, `panel`, `part`, `interface` or `module`. They share one body grammar;
/// what each kind may contain is a later stage's concern.
#[derive(Debug, Clone, PartialEq)]
pub struct Definition {
    /// `///` lines above the definition.
    pub docs: Vec<String>,
    /// `//!` lines at the start of the body.
    pub inner_docs: Vec<String>,
    pub kind: DefKind,
    pub name: Ident,
    pub items: Vec<Item>,
    pub span: Span,
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
    Bridge(Bridge),
    Catalog(Catalog),
    Pins(Pins),
    Tie(Tie),
    /// A `case { ... }` section; only a physical file may hold one.
    Case(Case),
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
}

impl PinKind {
    pub const ALL: [PinKind; 6] = [
        Self::Power,
        Self::Input,
        Self::Output,
        Self::Bidirectional,
        Self::Passive,
        Self::NoConnect,
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
}

/// One line of a pin table: `.sense input "SENSE" "3" fn GPIO UART1_TX optional`.
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
    pub span: Span,
}

/// `pins "DFN-8-EP" { ... }`: the datasheet's pin table of a part in one package, or
/// `pins { ... }` for a part whose pads do not depend on the package.
#[derive(Debug, Clone, PartialEq)]
pub struct Pins {
    pub package: Option<(String, Span)>,
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

/// `bridge .a ~> .b`: a two-terminal part that a `~>` chain passes through,
/// entering at `from` and leaving at `to`.
#[derive(Debug, Clone, PartialEq)]
pub struct Bridge {
    pub from: Path,
    pub to: Path,
    pub span: Span,
}

/// `catalog resistor { .resistance, .package }`: the columns of a parts catalog that
/// this part's fields are matched against. `catalog mcu by .mpn { .mpn, .package }` names
/// the column that picks one row, which the design must set.
#[derive(Debug, Clone, PartialEq)]
pub struct Catalog {
    pub name: Ident,
    pub key: Option<Path>,
    pub fields: Vec<Path>,
    pub span: Span,
}

/// `.name: Type` with an optional `{ ... }` body of further items.
#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    /// `///` lines above the declaration.
    pub docs: Vec<String>,
    pub name: Path,
    pub ty: TypePath,
    pub body: Option<Vec<Item>>,
    pub span: Span,
}

/// `std::parts::Resistor`
#[derive(Debug, Clone, PartialEq)]
pub struct TypePath {
    pub segments: Vec<Ident>,
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
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    /// `&=` narrow a field to the intersection with a set.
    Narrow,
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
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Self::Number(_, s)
            | Self::Str(_, s)
            | Self::Neg(_, s)
            | Self::Paren(_, s)
            | Self::Binary { span: s, .. }
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
