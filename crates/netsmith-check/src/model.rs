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
    /// Those of `provides` that are generics of the standard library: a part that provides
    /// the standard `Capacitor` is a bank like it (ADR 0052, ADR 0068 stage 2).
    pub provides_std: Vec<String>,
    /// For a use of a generic after a provider was put in its place: the generic's name
    /// (`CoaxReceptacle`). `type_name` is then the provider's. None for a part or module that
    /// was written by its own type, `.rf: AcmeMicroCoax`.
    pub placed_as: Option<String>,
    /// The definition of the generic a use was placed as.
    pub(crate) placed_def: Option<(usize, usize)>,
    /// Whether the definition is in a file of the standard library. A bank (ADR 0052) is a
    /// property of the standard `Capacitor` and of nothing a design defines.
    pub std: bool,
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
    /// Whether it is a module, not a part.
    pub module_shaped: bool,
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
    /// The column that names one row (`catalog mcu by .mpn`). The design must set it: the
    /// solver does not pick among rows of a keyed catalog.
    pub key: Option<FieldId>,
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
    pub span: Span,
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
}

impl RExpr {
    /// Every field the expression mentions.
    pub fn fields(&self, out: &mut Vec<FieldId>) {
        match self {
            Self::Const(_) | Self::Text(_) => {}
            Self::Field(f) => out.push(*f),
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
    /// and where. Relations the checker makes from other statements have none.
    pub written: Option<Written>,
}

/// A relation statement as the source has it.
#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    /// The module or board instance whose body states it: what its `.x` paths are relative to.
    pub scope: InstId,
    /// The statement's text with its white space collapsed to single spaces.
    pub text: String,
    /// The file as it was loaded, and the 1-based line the statement starts on.
    pub file: String,
    pub line: usize,
}

/// A run of `~>` through bridge parts, from one pin to another.
#[derive(Debug, Clone)]
pub struct Chain {
    pub start: InstId,
    pub parts: Vec<InstId>,
    pub end: InstId,
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

#[derive(Debug, Clone)]
pub struct Model {
    pub root: InstId,
    pub(crate) instances: Vec<Instance>,
    pub(crate) fields: Vec<Field>,
    pub(crate) relations: Vec<Relation>,
    /// Pairs of fields joined by connecting two interfaces: they hold one value.
    pub(crate) equalities: Vec<(FieldId, FieldId)>,
    pub(crate) chains: Vec<Chain>,
    pub(crate) nets: Vec<Net>,
    pub(crate) balances: Vec<Balance>,
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
    /// The enums that fields of this model have the type of.
    pub(crate) enums: Vec<EnumInfo>,
    /// The members declared with the type of a generic, in the order they were made.
    pub uses: Vec<GenericUse>,
    pub(crate) sources: SourceTexts,
}

impl Model {
    pub(crate) fn new() -> Self {
        Self {
            root: InstId(0),
            instances: Vec::new(),
            fields: Vec::new(),
            relations: Vec::new(),
            equalities: Vec::new(),
            chains: Vec::new(),
            nets: Vec::new(),
            balances: Vec::new(),
            joins: Vec::new(),
            mate_decls: Vec::new(),
            explicit_joins: Vec::new(),
            mates: Vec::new(),
            ground_groups: Vec::new(),
            part_joins: Vec::new(),
            enums: Vec::new(),
            uses: Vec::new(),
            sources: SourceTexts::default(),
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

    /// The enum a field of kind `FieldKind::Enum` has the type of.
    pub fn enum_info(&self, id: EnumId) -> &EnumInfo {
        &self.enums[id.0]
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
        m.balances.retain(|b| inside(&b.scope));
        m.joins.clear();
        m.mate_decls.clear();
        m.explicit_joins.clear();
        m.mates.clear();
        m.ground_groups.clear();
        m.uses.retain(|u| inside(&u.inst));
        m.equalities.retain(|(a, b)| owned(a) && owned(b));
        m.relations.retain(|r| {
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
        self.fields[field.0].domain = domain;
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

    /// Add a relation after the check: the solver adds the ones that a bound entails and takes
    /// them out again with [`Model::truncate_relations`] (ADR 0065). They go at the end, so no
    /// index of a relation of the design changes.
    pub fn push_relation(&mut self, relation: Relation) {
        self.relations.push(relation);
    }

    /// Keep the first `len` relations and drop the rest.
    pub fn truncate_relations(&mut self, len: usize) {
        self.relations.truncate(len);
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

    /// The `.count` of a part that is a bank (ADR 0052): the number of identical parts that
    /// stand in this place. Only the standard library's `Capacitor`, and a part that provides
    /// it, is one, whatever a design's own part names its members.
    pub fn bank_count(&self, part: InstId) -> Option<FieldId> {
        let inst = self.instance(part);
        let capacitor = (inst.std && inst.type_name == "Capacitor")
            || inst.provides_std.iter().any(|g| g == "Capacitor");
        if inst.kind != InstKind::Part || !capacitor {
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

    /// Whether `field` is a logic level of a `Logic` line (`vih`, `vil`, `voh`, `vol`). It is
    /// what one part's pin guarantees or needs, so connecting two lines does not equate them
    /// the way it equates the voltage of two rails (ADR 0046).
    pub fn is_level(&self, field: FieldId) -> bool {
        let f = self.field(field);
        matches!(f.name.as_str(), "vih" | "vil" | "voh" | "vol")
            && f.kind == FieldKind::Quantity(Dimension::VOLTAGE)
            && self.instance(f.owner).kind == InstKind::Interface
            && self.instance(f.owner).type_name == "Logic"
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

    /// Whether `field` is a field of a part's or module's built-in `.place` (ADR 0045). The
    /// placer and the check read such a field as a requirement on a position, so a bound on
    /// it (`>=`, `<=`) narrows it, where a bound on any other field is a claim to verify.
    pub fn is_place_field(&self, field: FieldId) -> bool {
        let owner = self.instance(self.field(field).owner);
        owner.kind == InstKind::Interface && owner.type_name == "Place" && owner.def.is_none()
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
