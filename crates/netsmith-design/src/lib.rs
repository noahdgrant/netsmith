//! The design stage of a build (check, solve, design, export): turns a solved board into the
//! tool-independent [`Design`] that exporters read. The stage and the type share a word: the
//! stage is this crate, the [`Design`] is what it makes.
//!
//! Every part gets a reference designator, a value, and the pads of its terminals. Nothing
//! here names a CAD tool: a resistor is still a `Resistor`, and an exporter decides what
//! that looks like.
//!
//! Alongside the design it returns where each part comes from: the supplier's part number
//! the solver picked, and whether the part is assembled by the manufacturer or soldered by
//! hand. Those are sourcing data, not CAD data, so they are not part of the [`Design`].

pub mod environment;
mod features;
mod polygon;
pub use features::Sites;
pub mod guidelines;
mod mates;
mod soldering;
pub use mates::{SystemMate, system_mates};

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use netsmith_check::{
    Board, Domain, FieldId, InstId, InstKind, Member, Model, Panel, PinKind, Pinout, System,
    SystemBoard,
};
use netsmith_core::{
    Design, DielectricLayer, DielectricPly, InnerLayers, InnerUse, InterfaceInstance,
    InterfaceMember, ModuleInstance, Net, Note, NoteAt, NoteKind, PanelRules, PanelSpec, Part,
    PartKind, PartPin, PinRef, PowerFlow, Process, SolderMask, Stackup,
};
use netsmith_solve::{Library, Solution};

/// The letters of a reference designator for a part with none stated: an integrated circuit,
/// the one kind that has no letters of its own (IEEE 315, ASME Y14.44).
const DEFAULT_LETTERS: &str = "U";

/// The order in which parts are numbered: paths compared segment by segment, and a number at
/// the end of a segment compared as a number, so `pads.tp2` comes before `pads.tp10` and
/// `.tp02` and `.tp2` stay apart (the shorter digit string first). The index of a member of a
/// group is such a number too: `decoupling[2]` before `decoupling[10]`. The same inputs always
/// give the same order, and a path without such a number sorts as plain text does.
pub fn natural_path_order(a: &str, b: &str) -> std::cmp::Ordering {
    let mut left = a.split('.');
    let mut right = b.split('.');
    loop {
        match (left.next(), right.next()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) => {
                let order = natural_segment_order(x, y);
                if order.is_ne() {
                    return order;
                }
            }
        }
    }
}

/// `tp` and `10` of `tp10`, and `decoupling[` and `10` of `decoupling[10]` (a member of a
/// group, ADR 0119); a segment that does not end in digits, or in an index, has none.
fn split_trailing_number(segment: &str) -> (&str, Option<&str>) {
    if let Some(inner) = segment.strip_suffix(']') {
        let stem = inner.trim_end_matches(|c: char| c.is_ascii_digit());
        if stem.len() < inner.len() && stem.ends_with('[') {
            return (stem, Some(&inner[stem.len()..]));
        }
    }
    let stem = segment.trim_end_matches(|c: char| c.is_ascii_digit());
    if stem.len() == segment.len() {
        (segment, None)
    } else {
        (stem, Some(&segment[stem.len()..]))
    }
}

fn natural_segment_order(a: &str, b: &str) -> std::cmp::Ordering {
    let ((stem_a, num_a), (stem_b, num_b)) = (split_trailing_number(a), split_trailing_number(b));
    stem_a.cmp(stem_b).then_with(|| match (num_a, num_b) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(x), Some(y)) => {
            // Without leading zeros the longer string is the larger number; the zeros are
            // only a tie-break, so that two spellings never compare equal.
            let (tx, ty) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
            tx.len()
                .cmp(&ty.len())
                .then_with(|| tx.cmp(ty))
                .then_with(|| x.len().cmp(&y.len()))
        }
    })
}

/// The letters of a part's reference designator (ADR 0046): the field `.designator` that its
/// kind or the design states, else `U`. Letters that are not one or more capital letters
/// (`A` to `Z`) are reported, because the number follows them and a designator with a digit,
/// a space or nothing in the letters is not one.
///
/// Why the letters are a field of the part (ADR 0046). Every `part` has a built-in text field
/// `.designator` and nothing is declared for it, exactly as for `.assembly` (ADR 0024, ADR 0028,
/// ADR 0042): declaring it again is the error "`designator` is already declared in this block". A
/// standard kind states its own letters (`.designator &= "R"` in `Resistor`), because they are
/// always true of it. A part you define states its own the same way, or states none and is `U`,
/// as every kind that was not in the old `prefix` table was. A board may narrow the letters of a
/// part of its own that states none (`.codec.designator &= "IC"`). The letters are those the old
/// table had, which are the usage of IEEE 315 and ASME Y14.44: `R`, `C`, `L`, `D`, `Q`, `Y`, `F`,
/// `J`, `TP`, `H`, `FID`, `U`, and `FB`, `SW` and `LBL` as KiCad writes them. The same usage gives
/// `K` for a relay, `BZ` for a buzzer, `BT` for a battery and `T` for a transformer; the library
/// has no such kind, so a project that defines one writes the letters on it. A switch stays `SW`,
/// as it was and as KiCad writes it, where the standards' class letter is `S`: that is
/// continuity, and it changes with one line in `std/parts/passive.nsm`. A test reads `std/` and
/// fails for any `part` that does not state `.designator &=` with capital letters, so a new
/// standard kind gets its letters on purpose.
///
/// A text field narrows to one value, so the letters a standard kind states cannot be changed by
/// a part's user or a board: "contradiction on `r.designator`: it is already "R", which does not
/// overlap "Z"". It is the limit ADR 0024 records for `.assembly` and is accepted for the same
/// reason: a resistor is `R`; the owner accepted it as built, 2026-11-11. A default that a board
/// may replace, for the letters of a standard kind and for `.assembly`, is not built (one language
/// choice for both; not planned until a board needs it).
///
/// The layout file is keyed by the path of a part (ADR 0032), so a saved placement does not move
/// when letters change. It was checked two ways: a test
/// (`a_layout_entry_still_places_its_part_when_the_designator_letters_change` in
/// `crates/netsmith-fabricate/tests/kicad_drc.rs`) places a part by its path, renames every designator of
/// the design, and finds the part at the same place and turn; and a build of
/// `examples/outline_and_pours.nsm` with a layout file that locks `stage.regulator` and
/// `stage.output_cap`, once from before ADR 0044 and once after, had every part, the two locked
/// ones included, at the same place and turn. A fab package that was already ordered keeps the
/// designators it was made with, in its BOM, placement file and board file; re-running `netsmith
/// fab` makes a new package with the new designators, and the board house must be told if it is
/// the same order.
fn designator_of(model: &Model, part: InstId, errors: &mut Vec<String>) -> String {
    let Some(word) = text_of(model, part, "designator") else {
        return DEFAULT_LETTERS.to_string();
    };
    if word.is_empty() || !word.chars().all(|c| c.is_ascii_uppercase()) {
        errors.push(format!(
            "`{}` has `.designator` \"{word}\", which is not one or more capital letters such as \"R\" or \"IC\"",
            model.path_of(part)
        ));
        return DEFAULT_LETTERS.to_string();
    }
    word
}

/// Who puts a part on the board (ADR 0024).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assembly {
    /// The board house places and solders it: a line in the BOM and the placement file.
    Fab,
    /// Soldered by hand after the board comes back: in the fab notes, not in the BOM or the
    /// placement file.
    Hand,
    /// Not on the BOM or the placement file at all: a mounting hole, a fiducial, a pad that
    /// a cable presses on. The fab notes list it as not fitted.
    None,
    /// The board house places and solders it, and the owner ships the part to the board house
    /// (JLCPCB's consigned parts): a line in the BOM and the placement file, with no supplier
    /// part number needed. The fab notes list it as consigned (ADR 0084).
    Consigned,
}

impl Assembly {
    /// The word for it in the language, in the catalog files and in messages.
    pub fn word(self) -> &'static str {
        match self {
            Self::Fab => "fab",
            Self::Hand => "hand",
            Self::None => "none",
            Self::Consigned => "consigned",
        }
    }

    /// The word for it in the catalog files and the project manifest (`"fab"`).
    pub fn parse(word: &str) -> Option<Self> {
        [Self::Fab, Self::Hand, Self::None, Self::Consigned]
            .into_iter()
            .find(|a| a.word() == word)
    }

    /// The variant of the language's `AssemblyMode` for it (`Fab`, ADR 0054).
    pub fn variant(self) -> &'static str {
        match self {
            Self::Fab => "Fab",
            Self::Hand => "Hand",
            Self::None => "None",
            Self::Consigned => "Consigned",
        }
    }

    /// The part a variant of `AssemblyMode` names.
    pub fn from_variant(variant: &str) -> Option<Self> {
        [Self::Fab, Self::Hand, Self::None, Self::Consigned]
            .into_iter()
            .find(|a| a.variant() == variant)
    }
}

/// What a part shows as its value when the solver did not choose one for it: the
/// `#[value(...)]` of its definition (ADR 0125), else the name of its kind.
fn default_value(inst: &netsmith_check::Instance, kind: &str) -> String {
    inst.marks.value.clone().unwrap_or_else(|| kind.to_string())
}

fn text_of(model: &Model, inst: InstId, field: &str) -> Option<String> {
    match model.member(inst, field)? {
        Member::Field(f) => match &model.field(f).domain {
            Domain::Text(t) => Some(t.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// What the design or the catalog row says about who assembles a part, if anything. A word
/// that is not one of the three is reported.
fn assembly_of(
    model: &Model,
    part: InstId,
    pick: Option<&netsmith_solve::Pick>,
    errors: &mut Vec<String>,
) -> Option<Assembly> {
    // The design says it with a variant of `AssemblyMode`; the checker has refused any
    // other value, so a variant that is not one of the three is a mistake of the tool.
    if let Some(variant) = text_of(model, part, "assembly") {
        let found = Assembly::from_variant(&variant);
        if found.is_none() {
            errors.push(format!(
                "`{}` has `.assembly` {variant}, which is not `AssemblyMode::Fab`, `AssemblyMode::Hand`, `AssemblyMode::None` or `AssemblyMode::Consigned`",
                model.path_of(part)
            ));
        }
        return found;
    }
    // The catalog row says it with a word.
    let word = pick.and_then(|p| p.assembly.clone())?;
    let found = Assembly::parse(&word);
    if found.is_none() {
        errors.push(format!(
            "`{}` has `.assembly` \"{word}\" in its catalog row, which is not \"fab\", \"hand\", \"none\" or \"consigned\"",
            model.path_of(part)
        ));
    }
    found
}

/// The path of the `k`th (from 0) of the `n` parts a bank stands for (ADR 0052): `bulk[1]`, the
/// index syntax of `N x Kind` (ADR 0119). A bank of one is the part itself, `bulk`.
pub(crate) fn bank_member_path(path: &str, k: usize, n: usize) -> String {
    if n > 1 {
        format!("{path}[{k}]")
    } else {
        path.to_string()
    }
}

/// The most parts a bank may stand for. The solver offers far fewer; this stops a stated
/// number that is a mistake.
const MAX_BANK: f64 = 64.0;

/// How many identical parts stand in this place (a bank, ADR 0052): the whole number in the
/// part's `.count`, and one for a part with none or with none stated. A count that is not a
/// whole number from 1 up, or that is still a range, is reported.
fn bank_of(model: &Model, part: InstId, errors: &mut Vec<String>) -> usize {
    let Some(field) = model.bank_count(part) else {
        return 1;
    };
    match &model.field(field).domain {
        Domain::Unset => 1,
        Domain::Quantity(q)
            if q.lo() == q.hi() && q.lo().fract() == 0.0 && (1.0..=MAX_BANK).contains(&q.lo()) =>
        {
            q.lo() as usize
        }
        other => {
            errors.push(format!(
                "`{}` is {other}, which is not a whole number of parts from 1 to {MAX_BANK}: state `.count` as one number, or leave it to the solver to choose",
                model.path_of_field(field)
            ));
            1
        }
    }
}

/// The part a terminal belongs to, reaching up through any interfaces it sits in.
fn owning_part(model: &Model, pin: InstId) -> InstId {
    let mut cur = model.instance(pin).parent.expect("a pin has a parent");
    while model.instance(cur).kind != InstKind::Part {
        cur = model
            .instance(cur)
            .parent
            .expect("a terminal is inside a part");
    }
    cur
}

/// The pins of a part in the package it is in, as its pin table lists them.
fn pins_of(model: &Model, part: InstId) -> Vec<PartPin> {
    let Pinout::Table(table) = model.pinout(part) else {
        return Vec::new();
    };
    table
        .rows
        .iter()
        .map(|row| PartPin {
            name: model.path_below(row.pin, part),
            datasheet: row.name.clone(),
            kind: match row.kind {
                PinKind::Power => netsmith_core::PinKind::Power,
                PinKind::Input => netsmith_core::PinKind::Input,
                PinKind::Output => netsmith_core::PinKind::Output,
                PinKind::Bidirectional => netsmith_core::PinKind::Bidirectional,
                PinKind::Passive => netsmith_core::PinKind::Passive,
                PinKind::NoConnect => netsmith_core::PinKind::NoConnect,
                PinKind::TriState => netsmith_core::PinKind::TriState,
                PinKind::OpenDrain => netsmith_core::PinKind::OpenDrain,
            },
            pads: row.pads.clone(),
            function: model.function_of(row.pin),
            power_flow: if row.kind == PinKind::Power {
                power_flow_of(model, row.pin)
            } else {
                None
            },
        })
        .collect()
}

/// Whether a power pin takes or gives power: the sign of the `current` of the rail whose `pos`
/// it is, read from the solved model. A pin that the part joins to a rail's `pos` inside
/// (`.vin2 ~ .vin.pos`) has that rail's direction. The current of a rail is what its loads
/// draw, so a negative one is a supply (a regulator's output, a source) and a positive one a
/// load (its input). The sign of a rail is settled only when the whole range is on one side:
/// from below 0 A to at most 0 A gives, from at least 0 A to above 0 A takes. A rail whose
/// current has no value, is exactly 0 A (the reference of a limit that a pin states) or crosses
/// 0 A settles nothing, and the pins joined to the pin settle it instead. A pin that no settled
/// rail's `pos` is joined to (a ground, a plain pin), and one joined to rails that disagree,
/// stays `None`.
fn power_flow_of(model: &Model, pin: InstId) -> Option<PowerFlow> {
    let mut flow: Option<PowerFlow> = None;
    for candidate in std::iter::once(pin).chain(model.joined_inside(pin)) {
        let Some(rail) = model.instance(candidate).parent else {
            continue;
        };
        let Some((current, pos)) = model.draw_of(rail) else {
            continue;
        };
        if pos != candidate {
            continue;
        }
        let Domain::Quantity(q) = &model.field(current).domain else {
            continue;
        };
        let here = if q.hi() <= 0.0 && q.lo() < 0.0 {
            PowerFlow::Gives
        } else if q.lo() >= 0.0 && q.hi() > 0.0 {
            PowerFlow::Takes
        } else {
            continue;
        };
        match flow {
            Some(other) if other != here => return None,
            _ => flow = Some(here),
        }
    }
    flow
}

/// The interface instances whose members are on the nets of the design, by path: for each
/// instance of kind interface, the pins directly in it that sit on one of `nets`, in the order
/// the interface declares them. The checker's model keeps, on each net, the ports of modules
/// and the pins of interfaces that only pass the connection along, so this is what the
/// source joined, not a reading of net names.
fn interfaces_of(model: &Model, nets: &[Net]) -> Vec<InterfaceInstance> {
    let kept: std::collections::HashSet<&str> = nets.iter().map(|n| n.name.as_str()).collect();
    let mut net_of: HashMap<InstId, &str> = HashMap::new();
    for net in model.nets() {
        if kept.contains(net.name.as_str()) {
            for &pin in &net.pins {
                net_of.insert(pin, net.name.as_str());
            }
        }
    }
    let mut out = Vec::new();
    for (id, inst) in model.instances() {
        if inst.kind != InstKind::Interface {
            continue;
        }
        let members: Vec<InterfaceMember> = inst
            .children
            .iter()
            .filter_map(|(name, member)| match member {
                Member::Inst(pin) if model.instance(*pin).kind == InstKind::Pin => {
                    net_of.get(pin).map(|net| InterfaceMember {
                        name: name.clone(),
                        net: (*net).to_string(),
                    })
                }
                _ => None,
            })
            .collect();
        if members.is_empty() {
            continue;
        }
        out.push(InterfaceInstance {
            path: model.path_of(id),
            type_name: inst.type_name.clone(),
            members,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The module instances of the design, by path in natural order, each with its own members
/// that are on one of `nets`: its pins and the pins of the interfaces it declares, through
/// interfaces inside interfaces, named by their path in the module (`vin.pos`), in natural
/// order too. A part's pins are not the module's. Neither order is the source's, so moving a
/// statement moves no sheet pin (issue #634).
fn modules_of(model: &Model, nets: &[Net]) -> Vec<ModuleInstance> {
    let kept: std::collections::HashSet<&str> = nets.iter().map(|n| n.name.as_str()).collect();
    let mut net_of: HashMap<InstId, &str> = HashMap::new();
    for net in model.nets() {
        if kept.contains(net.name.as_str()) {
            for &pin in &net.pins {
                net_of.insert(pin, net.name.as_str());
            }
        }
    }
    fn walk(
        model: &Model,
        id: InstId,
        prefix: &str,
        net_of: &HashMap<InstId, &str>,
        out: &mut Vec<InterfaceMember>,
    ) {
        for (name, member) in &model.instance(id).children {
            let Member::Inst(child) = member else {
                continue;
            };
            let name = format!("{prefix}{name}");
            match model.instance(*child).kind {
                InstKind::Pin => {
                    if let Some(net) = net_of.get(child) {
                        out.push(InterfaceMember {
                            name,
                            net: (*net).to_string(),
                        });
                    }
                }
                InstKind::Interface => walk(model, *child, &format!("{name}."), net_of, out),
                _ => {}
            }
        }
    }
    let mut modules: Vec<ModuleInstance> = model
        .instances()
        .filter(|(_, inst)| inst.kind == InstKind::Module)
        .map(|(id, inst)| {
            let mut ports = Vec::new();
            walk(model, id, "", &net_of, &mut ports);
            ports.sort_by(|a, b| natural_path_order(&a.name, &b.name));
            ModuleInstance {
                path: model.path_of(id),
                type_name: inst.type_name.clone(),
                ports,
            }
        })
        .collect();
    modules.sort_by(|a, b| natural_path_order(&a.path, &b.path));
    modules
}

/// The most parts an interface's doc comment may be anchored on: a rail that most of a sheet
/// hangs on is a note of the sheet, not of a few parts.
const MAX_DOC_ANCHORS: usize = 3;

/// The doc comment lines as one text: the lines of a paragraph joined with a space, a blank
/// line between paragraphs as a line break.
fn doc_text(lines: &[String]) -> String {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut open = false;
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            open = false;
        } else if open {
            let p = paragraphs.last_mut().expect("an open paragraph");
            p.push(' ');
            p.push_str(line);
        } else {
            paragraphs.push(line.to_string());
            open = true;
        }
    }
    paragraphs.join("\n")
}

/// The part (of `drawn`) that `inst` is, or that it is a terminal or an interface of.
fn drawn_part_of(
    model: &Model,
    inst: InstId,
    drawn: &HashMap<InstId, Vec<String>>,
) -> Option<InstId> {
    let mut cur = Some(inst);
    while let Some(c) = cur {
        if model.instance(c).kind == InstKind::Part {
            return drawn.contains_key(&c).then_some(c);
        }
        cur = model.instance(c).parent;
    }
    None
}

/// The notes a drawing may show: the doc comments of the board, of each module, of each part
/// declaration and of each interface declaration, and every `==` relation the source states:
/// the doc comments before the relations, each by path in natural order and then by text, not
/// in the source's order, so moving a statement moves no note (issue #634). Only what the source already says: ordinary
/// `//` comments are gone before the model, the standard library's own docs are not kept
/// (`Instance::doc_at`), and a relation of a physical file states where parts go, not what the
/// circuit is. `drawn` has the parts of the design, with their references.
fn notes_of(model: &Model, drawn: &HashMap<InstId, Vec<String>>) -> Vec<Note> {
    let paths: Vec<String> = {
        let mut v: Vec<String> = drawn.keys().map(|&p| model.path_of(p)).collect();
        v.sort();
        v
    };
    let net_parts = |pin: InstId| -> Vec<InstId> {
        model
            .net_of(pin)
            .map(|net| {
                net.terminals
                    .iter()
                    .map(|&t| owning_part(model, t))
                    .filter(|p| drawn.contains_key(p))
                    .collect()
            })
            .unwrap_or_default()
    };
    // The parts on the nets of the pins of an interface, by path in natural order, leaving out
    // the nets of `skip`.
    let interface_parts = |id: InstId, skip: &BTreeSet<String>| -> Vec<String> {
        let mut parts: Vec<InstId> = Vec::new();
        for (_, member) in &model.instance(id).children {
            if let Member::Inst(pin) = member
                && model.instance(*pin).kind == InstKind::Pin
                && !model.net_of(*pin).is_some_and(|n| skip.contains(&n.name))
            {
                parts.extend(net_parts(*pin));
            }
        }
        let mut named: Vec<String> = parts.iter().map(|&p| model.path_of(p)).collect();
        named.sort_by(|a, b| natural_path_order(a, b));
        named.dedup();
        named
    };
    // The nets a return runs on and that no rail draws from: ground, which every part is on.
    let ground: BTreeSet<String> = {
        let supply: BTreeSet<&str> = model
            .balances()
            .iter()
            .filter(|b| !b.returns)
            .map(|b| b.net.as_str())
            .collect();
        model
            .balances()
            .iter()
            .filter(|b| b.returns && !supply.contains(b.net.as_str()))
            .map(|b| b.net.clone())
            .collect()
    };
    let mut notes = Vec::new();
    for (id, inst) in model.instances() {
        let Some((file, line)) = &inst.doc_at else {
            continue;
        };
        let path = model.path_of(id);
        let mut anchors: Vec<String> = Vec::new();
        let text = match inst.kind {
            InstKind::Board if id == model.root => doc_text(&inst.def_docs),
            InstKind::Module => {
                // A module that has no part in this design has no sheet.
                let below = format!("{path}.");
                if !paths.iter().any(|p| p.starts_with(&below)) {
                    continue;
                }
                let mut lines = inst.def_docs.clone();
                if !inst.docs.is_empty() {
                    lines.push(String::new());
                    lines.extend(inst.docs.iter().cloned());
                }
                doc_text(&lines)
            }
            InstKind::Part if drawn.contains_key(&id) => {
                anchors.push(path.clone());
                doc_text(&inst.docs)
            }
            InstKind::Interface => {
                // Inside a part it is the part's own definition, not the design.
                let mut cur = inst.parent;
                let mut in_part = false;
                while let Some(c) = cur {
                    if model.instance(c).kind == InstKind::Part {
                        in_part = true;
                        break;
                    }
                    cur = model.instance(c).parent;
                }
                if in_part {
                    continue;
                }
                let named = interface_parts(id, &BTreeSet::new());
                // An interface nothing drawn is on (a process, a plain port) is not in the
                // schematic: its comment stays in the source.
                if named.is_empty() {
                    continue;
                }
                if named.len() <= MAX_DOC_ANCHORS {
                    anchors = named;
                }
                doc_text(&inst.docs)
            }
            _ => continue,
        };
        if text.is_empty() {
            continue;
        }
        notes.push(Note {
            path,
            kind: NoteKind::Doc,
            text,
            at: Some(NoteAt {
                file: file.clone(),
                line: *line,
            }),
            anchors,
            result: None,
            derived: false,
        });
    }
    let fields_of = |relation: &netsmith_check::Relation| {
        let mut fields = Vec::new();
        relation.lhs.fields(&mut fields);
        relation.rhs.fields(&mut fields);
        fields
    };
    // The `==` relations the source writes, by the fields each names: a board that writes the
    // relation of a `~>` chain by hand as well (ADR 0113) gets the one note it wrote.
    let by_hand: HashSet<(InstId, BTreeSet<FieldId>)> = model
        .relations()
        .iter()
        .filter(|r| r.op == netsmith_lang::ast::CmpOp::EqEq)
        .filter_map(|r| {
            let written = r.written.as_ref().filter(|w| w.chain.is_empty())?;
            Some((written.scope, fields_of(r).into_iter().collect()))
        })
        .collect();
    for relation in model.relations() {
        let Some(written) = &relation.written else {
            continue;
        };
        if relation.op != netsmith_lang::ast::CmpOp::EqEq {
            continue;
        }
        let fields = fields_of(relation);
        if fields.iter().any(|&f| model.is_place_field(f)) {
            continue;
        }
        let derived = !written.chain.is_empty();
        if derived && by_hand.contains(&(written.scope, fields.iter().copied().collect())) {
            continue;
        }
        let mut anchors: Vec<String> = Vec::new();
        // A relation is about the parts whose fields it names, in the order it names them (the
        // part whose current a chain's relation says comes first, as a hand-written one puts
        // it). A chain's relation is about the chain's parts only: the rail it names is the
        // chain's supply, whichever part gives it.
        let owners = fields
            .iter()
            .map(|&f| model.field(f).owner)
            .filter(|owner| !derived || written.chain.contains(owner));
        for owner in owners {
            if let Some(part) = drawn_part_of(model, owner, drawn) {
                let path = model.path_of(part);
                if !anchors.contains(&path) {
                    anchors.push(path);
                }
            }
        }
        // A relation of rails only (`.cards.voltage == .supply.voltage * 0.9`) names no part: it
        // is about the parts on the nets of the rails it names, as an interface's doc comment is,
        // without the return net (ground has every part) and, as there, only when there are few.
        if anchors.is_empty() {
            let mut interfaces: Vec<InstId> = Vec::new();
            for f in &fields {
                let mut cur = Some(model.field(*f).owner);
                while let Some(c) = cur {
                    match model.instance(c).kind {
                        InstKind::Interface => {
                            if !interfaces.contains(&c) {
                                interfaces.push(c);
                            }
                            break;
                        }
                        // The interface of a part that is not drawn is that part's own.
                        InstKind::Part => break,
                        _ => cur = model.instance(c).parent,
                    }
                }
            }
            let mut named: Vec<String> = Vec::new();
            for i in interfaces {
                for path in interface_parts(i, &ground) {
                    if !named.contains(&path) {
                        named.push(path);
                    }
                }
            }
            if named.len() <= MAX_DOC_ANCHORS {
                anchors = named;
            }
        }
        let scope = model.path_of(written.scope);
        // What the solver settled on, in the worst case: the value or the range the field on
        // the left holds after solving, when it holds one.
        let result = match &relation.lhs {
            netsmith_check::RExpr::Field(f) => match &model.field(*f).domain {
                Domain::Quantity(q) => {
                    let path = model.path_of_field(*f);
                    let shown = if scope.is_empty() {
                        path.as_str()
                    } else {
                        path.strip_prefix(&format!("{scope}.")).unwrap_or(&path)
                    };
                    Some(format!("{shown} = {q}"))
                }
                _ => None,
            },
            _ => None,
        };
        notes.push(Note {
            path: scope,
            kind: NoteKind::Relation,
            text: written.text.clone(),
            at: Some(NoteAt {
                file: written.file.clone(),
                line: written.line,
            }),
            anchors,
            result,
            derived,
        });
    }
    notes.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| natural_path_order(&a.path, &b.path))
            .then_with(|| a.text.cmp(&b.text))
    });
    notes
}

/// The board's manufacturing process: its `process` member, with each rule read as the least
/// the board is now made to (the bottom of what the board left of the library's range).
/// A board with no `process` member has none. Lengths become millimetres. `nets` are the
/// names of the board's nets, which a plane on an inner layer must be one of.
fn process_of(model: &Model, nets: &[String], errors: &mut Vec<String>) -> Option<Process> {
    let member = model.member(model.root, "process")?;
    let Member::Inst(process) = member else {
        errors.push("`process` must be a manufacturing process such as `TwoLayer`".to_string());
        return None;
    };
    // The type the board wrote: a generic process (ADR 0113) is named as the board names it,
    // `TwoLayer`, whichever weight's provider was picked; the weight is in the stackup.
    let instance = model.instance(process);
    let kind = instance
        .placed_as
        .clone()
        .unwrap_or_else(|| instance.type_name.clone());
    let problems = RefCell::new(Vec::<String>::new());
    let find = |path: &str| -> Option<Domain> {
        let full = format!("process.{path}");
        match model.field_at(&full).map(|f| &model.field(f).domain) {
            Some(Domain::Unset) | None => {
                problems.borrow_mut().push(format!(
                    "`process` is a `{kind}` and has no value for `{path}`, which every manufacturing process states"
                ));
                None
            }
            Some(domain) => Some(domain.clone()),
        }
    };
    // A rule is the bottom of its range, and one that is not a number at its end is refused:
    // `.min_track &= ..5mm` has no bottom. A length is rounded to a nanometre of the
    // millimetre so that `0.1mm` is `0.1`, not `0.10000000000000002`.
    let end = |path: &str, q: &netsmith_quantity::Quantity, top: bool| {
        let v = if top { q.hi() } else { q.lo() };
        if v.is_finite() {
            return v;
        }
        let (word, open) = if top {
            ("top", "has no upper limit")
        } else {
            ("bottom", "has no lower limit")
        };
        problems.borrow_mut().push(format!(
            "`process.{path}` is {q}, which {open}: a process states a number at the {word} of the range that the design reads"
        ));
        0.0
    };
    let mm = |path: &str| match find(path) {
        Some(Domain::Quantity(q)) => (end(path, &q, false) * 1e12).round() / 1e9,
        _ => 0.0,
    };
    // A length a process may leave unstated (it has no value, or the field is not declared).
    let optional_mm = |full: &str| -> Option<f64> {
        let f = model.field_at(full)?;
        match &model.field(f).domain {
            Domain::Quantity(q) => {
                let path = full.trim_start_matches("process.");
                Some((end(path, q, false) * 1e12).round() / 1e9)
            }
            _ => None,
        }
    };
    let text = |d: Option<Domain>| match d {
        Some(Domain::Text(t)) => t,
        _ => String::new(),
    };
    let number = |path: &str| match find(path) {
        Some(Domain::Quantity(q)) => end(path, &q, false),
        _ => 0.0,
    };
    let layers = number("stackup.layers").round() as u32;
    let mut built = Process {
        name: kind.clone(),
        fabricator: text(find("fabricator")),
        source: text(find("source")),
        min_track: mm("min_track"),
        min_clearance: mm("min_clearance"),
        min_drill: mm("min_drill"),
        min_via_diameter: mm("min_via_diameter"),
        min_annular_ring: mm("min_annular_ring"),
        min_via_ring: mm("min_via_ring"),
        min_hole_clearance: mm("min_hole_clearance"),
        min_plated_hole_clearance: mm("min_plated_hole_clearance"),
        min_pad_to_pad: mm("min_pad_to_pad"),
        min_mask_to_track: mm("min_mask_to_track"),
        min_npth_drill: mm("min_npth_drill"),
        min_plated_slot: mm("min_plated_slot"),
        min_npth_slot: mm("min_npth_slot"),
        via_drill: mm("via_drill"),
        via_diameter: mm("via_diameter"),
        min_hole_to_hole: mm("min_hole_to_hole"),
        min_via_hole_to_hole: mm("min_via_hole_to_hole"),
        min_edge_clearance: mm("min_edge_clearance"),
        min_silk_width: mm("min_silk_width"),
        min_text_height: mm("min_text_height"),
        min_silk_to_pad: mm("min_silk_to_pad"),
        // A process that does not state it (one written before issue #1186) keeps its space
        // from a pad to silkscreen between silkscreen, as netsmith did before.
        min_silk_to_silk: if mm("min_silk_to_silk") > 0.0 {
            mm("min_silk_to_silk")
        } else {
            mm("min_silk_to_pad")
        },
        min_mask_bridge: mm("min_mask_bridge"),
        min_fiducials: number("min_fiducials"),
        fiducial_diameter: mm("fiducial_diameter"),
        fiducial_mask_ratio: number("fiducial_mask_ratio"),
        min_fiducial_edge: mm("min_fiducial_edge"),
        min_test_point_pad: mm("min_test_point_pad"),
        min_test_point_pitch: mm("min_test_point_pitch"),
        min_test_point_body: mm("min_test_point_body"),
        min_test_point_edge: mm("min_test_point_edge"),
        min_power_test_point_pad: mm("min_power_test_point_pad"),
        tall_part_height: mm("tall_part_height"),
        min_test_point_tall_body: mm("min_test_point_tall_body"),
        fiducial_round_angle: number("fiducial_round_angle"),
        fine_pitch: mm("fine_pitch"),
        // An upper bound, so the top of what the board left.
        via_in_pad: match find("via_in_pad") {
            Some(Domain::Quantity(q)) => (end("via_in_pad", &q, true) * 1e12).round() / 1e9,
            _ => 0.0,
        },
        // Unstated in the library; a board may state it. An upper bound, so the top.
        max_temperature_rise: match model.field_at("process.max_temperature_rise") {
            Some(f) => match &model.field(f).domain {
                Domain::Quantity(q) => Some(end("max_temperature_rise", q, true)),
                _ => None,
            },
            None => None,
        },
        // Unstated in the library; a board may state it. A lower bound, so the bottom.
        min_round_track_angle: match model.field_at("process.min_round_track_angle") {
            Some(f) => match &model.field(f).domain {
                Domain::Quantity(q) => Some(end("min_round_track_angle", q, false)),
                _ => None,
            },
            None => None,
        },
        // Unstated where the board house publishes no figure. A lower bound, so the bottom.
        min_single_sided_assembly_size: optional_mm("process.min_single_sided_assembly_size"),
        min_double_sided_assembly_size: optional_mm("process.min_double_sided_assembly_size"),
        // Unstated where the board house publishes no figure; the page, service and day of
        // the two figures above (issue 1393).
        assembly_size_source: match model.field_at("process.assembly_size_source") {
            Some(f) => match &model.field(f).domain {
                Domain::Text(t) if !t.is_empty() => Some(t.clone()),
                _ => None,
            },
            None => None,
        },
        track_width_tolerance: number("track_width_tolerance"),
        hole_plating: mm("hole_plating"),
        finish: text(find("finish")),
        hdi: None,
        stackup: Stackup {
            layers,
            thickness: match find("stackup.thickness") {
                Some(Domain::Quantity(q)) => (
                    (end("stackup.thickness", &q, false) * 1e12).round() / 1e9,
                    (end("stackup.thickness", &q, true) * 1e12).round() / 1e9,
                ),
                _ => (0.0, 0.0),
            },
            copper_weight: number("stackup.copper_weight"),
            inner: None,
            // The board's choice (issue #771): a variant's name, or nothing where it says none.
            mask_colour: match model.field_at("process.stackup.mask_colour") {
                Some(f) => match &model.field(f).domain {
                    Domain::Text(t) if !t.is_empty() => Some(t.clone()),
                    _ => None,
                },
                None => None,
            },
        },
    };
    // The finish is one of the process's offer (issue #842): a word in the list `.finishes`,
    // which the process states, not a rule of this code.
    let offered = text(find("finishes"));
    let offered: Vec<&str> = offered.split(',').map(str::trim).collect();
    if !built.finish.is_empty() && !offered.contains(&built.finish.as_str()) {
        problems.borrow_mut().push(format!(
            "`process.finish` is `{}`, but a `{kind}` board is made with one of: {}",
            built.finish,
            offered
                .iter()
                .map(|f| format!("`{f}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    // A board of more than two layers has inner layers, each a plane on a net the board
    // names or a signal layer the board says it is (ADR 0111), and the build-up round them.
    if layers > 2 {
        let mut uses = Vec::new();
        for n in 1..=layers - 2 {
            let name = format!("inner{n}");
            if model.field_at(&format!("process.{name}.net")).is_none() {
                problems.borrow_mut().push(format!(
                    "`process` is a `{kind}` with {layers} copper layers and has no `{name}` layer, which each inner layer needs"
                ));
                continue;
            }
            let field = |f: &str| {
                model
                    .field_at(&format!("process.{name}.{f}"))
                    .and_then(|f| match &model.field(f).domain {
                        Domain::Text(t) => Some(t.clone()),
                        _ => None,
                    })
            };
            let net = field("net");
            match (field("use").as_deref().unwrap_or("plane"), net) {
                ("plane", Some(net)) => {
                    if !nets.contains(&net) {
                        problems.borrow_mut().push(format!(
                            "`process.{name}` is a plane on the net `{net}`, but the board has no net of that name (its nets: {})",
                            nets.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")
                        ));
                    }
                    uses.push(InnerUse::Plane(net));
                }
                ("plane", None) => problems.borrow_mut().push(format!(
                    "`process.{name}` is an inner layer of a {layers}-layer board and has no net: the board says which, for example `.process.{name}.net &= \"GND\"`, or that it is a signal layer, `.process.{name}.use &= \"signal\"`"
                )),
                ("signal", None) => uses.push(InnerUse::Signal),
                ("signal", Some(net)) => problems.borrow_mut().push(format!(
                    "`process.{name}` is a signal layer and names the net `{net}`: a signal layer carries the tracks of any net, so it names none; for a plane on `{net}`, leave out `.use &= \"signal\"`"
                )),
                (other, _) => problems.borrow_mut().push(format!(
                    "`process.{name}.use` is `{other}`, but an inner layer is `\"plane\"` or `\"signal\"`"
                )),
            }
        }
        // Whether the process states a field, without the problem `find` reports for one it
        // does not: a dielectric layer states its own kind or its plies, not both.
        let stated = |path: &str| {
            model
                .field_at(&format!("process.{path}"))
                .is_some_and(|f| model.field(f).domain != Domain::Unset)
        };
        // One prepreg or core, the layer's own fields or one of its plies: `None` when its kind
        // is not a word a dielectric is (reported, unless it is unstated, which `find` reports).
        let ply = |at: &str| -> Option<DielectricPly> {
            let kind = text(find(&format!("{at}.kind")));
            if kind != "prepreg" && kind != "core" {
                if !kind.is_empty() {
                    problems.borrow_mut().push(format!(
                        "`process.{at}.kind` is `{kind}`, but a dielectric layer is `\"prepreg\"` or `\"core\"`"
                    ));
                }
                return None;
            }
            let core = kind == "core";
            Some(DielectricPly {
                core,
                glass: if core {
                    String::new()
                } else {
                    text(find(&format!("{at}.glass")))
                },
                thickness: mm(&format!("{at}.thickness")),
                epsilon: number(&format!("{at}.epsilon")),
            })
        };
        let mut dielectrics = Vec::new();
        for n in 1..layers {
            let at = format!("stackup.dielectric{n}");
            // A layer pressed from several plies states them from `.ply1`, with no gap.
            let plies: Vec<String> = (1..)
                .map(|i| format!("{at}.ply{i}"))
                .take_while(|p| stated(&format!("{p}.kind")))
                .collect();
            if plies.is_empty() {
                if let Some(one) = ply(&at) {
                    dielectrics.push(DielectricLayer::of_plies(vec![one]));
                }
                continue;
            }
            if stated(&format!("{at}.kind")) {
                problems.borrow_mut().push(format!(
                    "`process.{at}` states its own `.kind` and its plies (`.ply1`...): a dielectric layer is one prepreg or core, stated in its own fields, or several plies, stated each in a `.plyN`"
                ));
                continue;
            }
            let plies: Vec<DielectricPly> = plies.iter().filter_map(|p| ply(p)).collect();
            if !plies.is_empty() {
                dielectrics.push(DielectricLayer::of_plies(plies));
            }
        }
        // What the fabricator's impedance calculator uses, where the process states it (issue
        // #1007): unstated, the member is not in the model with a value.
        let stated = |path: &str| match model
            .field_at(&format!("process.{path}"))
            .map(|f| &model.field(f).domain)
        {
            Some(Domain::Quantity(q)) => Some(end(path, q, false)),
            _ => None,
        };
        let round = |v: f64| (v * 1e12).round() / 1e9;
        let mask = match (
            stated("stackup.impedance_mask_thickness"),
            stated("stackup.impedance_mask_epsilon"),
        ) {
            (Some(thickness), Some(epsilon)) => Some(SolderMask {
                thickness: round(thickness),
                epsilon,
            }),
            _ => None,
        };
        built.stackup.inner = Some(InnerLayers {
            layers: uses,
            copper: mm("stackup.inner_copper"),
            dielectrics,
            build: text(find("stackup.build")),
            hole_clearance: mm("min_plane_hole_clearance"),
            mask,
            outer_copper: stated("stackup.impedance_outer_copper").map(round),
        });
    }
    // An HDI process (issue #1015) states its order; no other process has the field.
    if model.field_at("process.hdi_order").is_some() {
        let top = |path: &str| match find(path) {
            Some(Domain::Quantity(q)) => end(path, &q, true),
            _ => 0.0,
        };
        let buried = text(find("buried_vias"));
        if buried != "none" && buried != "core" {
            problems.borrow_mut().push(format!(
                "`process.buried_vias` is `{buried}`, but a `{kind}` board has buried vias `\"core\"` or `\"none\"`"
            ));
        }
        built.hdi = Some(netsmith_core::Hdi {
            order: number("hdi_order").round() as u32,
            buried: buried == "core",
            laser_drill: mm("laser_drill"),
            laser_via_diameter: mm("laser_via_diameter"),
            min_laser_drill: mm("min_laser_drill"),
            max_laser_drill: (top("max_laser_drill") * 1e12).round() / 1e9,
            min_ring: mm("min_hdi_via_ring"),
            max_laser_aspect: top("max_laser_aspect"),
            max_drill_aspect: top("max_drill_aspect"),
            min_laser_dielectric: mm("min_laser_dielectric"),
            max_laser_dielectric: (top("max_laser_dielectric") * 1e12).round() / 1e9,
            min_buried_core: mm("min_buried_core"),
            max_buried_drill: (top("max_buried_drill") * 1e12).round() / 1e9,
        });
    }
    let problems = problems.into_inner();
    if problems.is_empty() {
        Some(built)
    } else {
        errors.extend(problems);
        None
    }
}

/// Where one part comes from. A part the solver picked from a real catalog has its supplier
/// number; a part with a pick but no `lcsc` is left for the fabrication step to refuse.
#[derive(Debug, Clone, PartialEq)]
pub struct Sourced {
    pub refdes: String,
    pub kind: String,
    pub value: String,
    pub package: Option<String>,
    /// The LCSC number, such as `C25104`.
    pub lcsc: Option<String>,
    /// The manufacturer's part number.
    pub mpn: Option<String>,
    pub library: Option<Library>,
    /// Who assembles it. The `.assembly` the kind or the design states, else the catalog
    /// row's, else the project's default (the manifest's `assembly`, ADR 0028), else `Fab`
    /// (`Hand` for a kind that is always through hole).
    pub assembly: Assembly,
    /// Whether the design or the catalog row said so. If not, the footprint decides when
    /// it is known: a through-hole footprint is soldered by hand.
    pub assembly_stated: bool,
    /// The `height` cell of the catalog row the part was picked as, in mm (ADR 0094), if it has
    /// one. A generic part has none until it is resolved to a row. It is the part's height only
    /// when the design states none.
    pub catalog_height: Option<f64>,
    /// Set for a generic part (ADR 0047): the solver chose its value and nothing else, and
    /// `lcsc`, `mpn` and `library` are empty until it is resolved to a row of a catalog.
    pub generic: Option<netsmith_solve::Generic>,
    /// The catalog row a generic part was resolved to, once `fab` or the bill of materials has
    /// resolved it (`None` before, and for a part that was not generic): the row's name and its
    /// temperature range in kelvin, for the judgement against the board's environment (ADR 0099).
    pub resolved_row: Option<ResolvedRow>,
    /// Set for a part that is not fitted (`.fitted &= false`, issue #1273): a `DNP` line of
    /// `bom.csv` and on no placement file. Its `assembly` is then `None`.
    pub not_fitted: Option<netsmith_core::NotFitted>,
}

/// The catalog row that stands for a generic part, as far as the judgement against the board's
/// environment needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRow {
    /// What a message calls the row.
    pub name: String,
    pub temp_min: Option<f64>,
    pub temp_max: Option<f64>,
    /// The row's `max_voltage` in steps over temperature, if it writes it so (issue #1257).
    pub max_voltage_steps: Option<netsmith_check::environment::RatingSteps>,
    /// The cited range of the row's dielectric class, for an end the row leaves empty.
    pub temp_class: Option<&'static netsmith_check::environment::ClassRange>,
    /// The row's `qualification` cell, `Aec` or `None`, if it has one.
    pub qualification: Option<String>,
}

impl Sourced {
    /// Whether the board house places it: a part it assembles from stock (`Fab`) and a part
    /// the owner ships to it (`Consigned`). Both are in the placement file and the BOM.
    pub fn assembled(&self) -> bool {
        matches!(self.assembly, Assembly::Fab | Assembly::Consigned)
    }

    /// Whether the board house is to buy the part, so that it needs a supplier part number:
    /// a part it assembles from stock. A consigned part is the owner's to supply.
    pub fn needs_part_number(&self) -> bool {
        self.assembly == Assembly::Fab
    }
}

/// What the design stage makes of one board: the tool-independent [`Design`] and the sourcing
/// of each of its parts, in the same order.
#[derive(Debug, Clone)]
pub struct Designed {
    pub design: Design,
    pub sourcing: Vec<Sourced>,
    /// The instance each part was made from, in the same order.
    pub origins: Vec<InstId>,
    /// What the design does against a guideline (ADR 0036): findings that a person decides
    /// about, which the project may make blocking (`guidelines = "error"`).
    pub guidelines: Vec<guidelines::Guideline>,
    /// What the judgement against the board's environment did (ADR 0099): for the notes of `fab`.
    /// `None` for a board with no environment.
    pub environment: Option<environment::Tally>,
    /// Things worth knowing that did not stop the design stage: the nets with a current that is
    /// not known, whose tracks are therefore a lower bound (ADR 0036).
    pub notes: Vec<String>,
    /// Why each of `notes` is there, in the same order (see [`netsmith_core::Cause`]).
    pub note_causes: Vec<netsmith_core::Cause>,
    /// Where the design states each requirement on a position (ADR 0045), for a caller that
    /// has the source files to put in `design.features.constraints[..].source`.
    pub constraint_sites: Sites,
    /// Where the design states each of `design.features.offsets`, in the same order.
    pub offset_sites: Vec<(netsmith_check::SourceId, netsmith_lang::Span)>,
    /// Where the design states each of `design.features.pin_distances`, in the same order.
    pub pin_distance_sites: Vec<(netsmith_check::SourceId, netsmith_lang::Span)>,
    /// Where the design states each of `design.features.net_routes`, in the same order.
    pub net_route_sites: Vec<(netsmith_check::SourceId, netsmith_lang::Span)>,
    /// Where the design states each of `design.features.soft_margins`, in the same order.
    pub soft_margin_sites: Vec<(netsmith_check::SourceId, netsmith_lang::Span)>,
    /// Where the design states each of `design.features.keep_apart`, in the same order.
    pub keep_apart_sites: Vec<(netsmith_check::SourceId, netsmith_lang::Span)>,
    /// Where the design states each of `design.features.protrusions`, in the same order.
    pub protrusion_sites: Vec<Vec<(netsmith_check::SourceId, netsmith_lang::Span)>>,
    /// Where the design states the file of each of `design.features.models`, in the same order.
    pub model_sites: Vec<Vec<(netsmith_check::SourceId, netsmith_lang::Span)>>,
    /// Where the design states the file of each of `design.features.case_models`, in the same
    /// order.
    pub case_model_sites: Vec<Vec<(netsmith_check::SourceId, netsmith_lang::Span)>>,
    /// Where the design states the file of each of `design.features.logos`, in the same order.
    pub logo_sites: Vec<Vec<(netsmith_check::SourceId, netsmith_lang::Span)>>,
}

/// A panel as the specification `fab` builds it from: the board it repeats, the grid and the
/// board house's rules (its `fab` member, read as the least the panel now allows for a lower
/// bound and the most for an upper bound). Every mistake is one line.
pub fn design_panel(panel: &Panel) -> Result<PanelSpec, Vec<String>> {
    let mut errors = Vec::new();
    let model = &panel.model;
    let name = &panel.name;
    let Some(cell) = panel.cell.clone() else {
        return Err(vec![
            "it has no board to repeat: add `.cell: SomeBoard`".to_string(),
        ]);
    };
    let kind = match model.member(model.root, "fab") {
        Some(Member::Inst(fab)) => model.instance(fab).type_name.clone(),
        _ => {
            return Err(vec![
                "it does not say who makes it, and a panel needs the board house's rules: add `import std::mfg::jlcpcb::VCutPanel` and `.fab: VCutPanel { .columns &= 3; .rows &= 2 }`".to_string(),
            ]);
        }
    };
    let mut field = |path: &str| -> Option<Domain> {
        match model.domain_at(&format!("fab.{path}")) {
            Some(Domain::Unset) | None => {
                errors.push(format!(
                    "`fab` is a `{kind}` and has no value for `{path}`{}",
                    match path {
                        "columns" | "rows" => format!(
                            ": say how many, for example `.fab: {kind} {{ .{path} &= 3 }}`"
                        ),
                        _ => ", which every panel process states".to_string(),
                    }
                ));
                None
            }
            Some(d) => Some(d.clone()),
        }
    };
    let mm = |d: Option<Domain>, top: bool| match d {
        Some(Domain::Quantity(q)) => {
            let v = if top { q.hi() } else { q.lo() };
            (v * 1e12).round() / 1e9
        }
        _ => 0.0,
    };
    let count = |d: Option<Domain>, top: bool| match d {
        Some(Domain::Quantity(q)) => {
            let v = if top { q.hi() } else { q.lo() };
            v.round().max(0.0) as u32
        }
        _ => 0,
    };
    let text = |d: Option<Domain>| match d {
        Some(Domain::Text(t)) => t,
        _ => String::new(),
    };
    let columns = count(field("columns"), false);
    let rows = count(field("rows"), false);
    let rail = mm(field("rail"), false);
    let rules = PanelRules {
        name: kind.clone(),
        fabricator: text(field("fabricator")),
        source: text(field("source")),
        min_rail: mm(field("min_rail"), false),
        min_size: mm(field("min_size"), false),
        max_size: mm(field("max_size"), true),
        max_assembled_size: mm(field("max_assembled_size"), true),
        max_cuts: count(field("max_cuts"), true),
        min_cut_spacing: mm(field("min_cut_spacing"), false),
        advised_cut_spacing: mm(field("advised_cut_spacing"), false),
        min_edge_clearance: mm(field("min_edge_clearance"), false),
        advised_edge_clearance: mm(field("advised_edge_clearance"), false),
        min_thickness: mm(field("min_thickness"), false),
        tooling_hole: mm(field("tooling_hole"), false),
        fiducial: mm(field("fiducial"), false),
        fiducial_offset: mm(field("fiducial_offset"), false),
    };
    if errors.is_empty() && (columns == 0 || rows == 0) {
        errors.push(format!(
            "a panel has at least one column and one row, but `.fab.columns` is {columns} and `.fab.rows` is {rows}"
        ));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(PanelSpec {
        name: name.clone(),
        cell,
        columns,
        rows,
        rail,
        rules,
    })
}

/// The design stage for one solved board: its [`Design`] with the sourcing of each part, or
/// every mistake the stage found.
pub fn design_board(board: &Board, solution: &Solution) -> Result<Designed, Vec<String>> {
    design_model(&board.name, &solution.model, solution)
}

/// One board of a solved system. The design is named after the board's definition, so the
/// files read `PowerBoard.kicad_sch` wherever the system put them.
pub fn design_system_board(
    system: &System,
    member: &SystemBoard,
    solution: &Solution,
) -> Result<Designed, Vec<String>> {
    debug_assert!(system.boards.iter().any(|b| b.inst == member.inst));
    let model = solution.model.scoped_to(member.inst);
    let mut designed = design_model(&member.type_name, &model, solution)?;
    // A mate that works turned end for end is a guideline (ADR 0040), said once, by the board
    // of the first connector the system names.
    for mate in solution.model.mates() {
        if let Some(why) = &mate.reversible
            && solution.model.board_of(mate.a) == Some(member.inst)
        {
            designed.guidelines.push(guidelines::Guideline {
                rule: "mating",
                message: why.clone(),
                at: Some(mate.a),
            });
        }
    }
    Ok(designed)
}

fn design_model(name: &str, model: &Model, solution: &Solution) -> Result<Designed, Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

    // The project's default for who assembles a part nothing else speaks for (ADR 0028).
    let project_default = solution.default_assembly.as_deref().and_then(|word| {
        let found = Assembly::parse(word);
        if found.is_none() {
            errors.push(format!(
                "the project's default assembly \"{word}\" is not \"fab\", \"hand\", \"none\" or \"consigned\""
            ));
        }
        found
    });

    // Parts are numbered by path (owner decision 2026-11-13, issue #634): within each set of
    // letters, the parts sorted by their full path in the board in natural order (a number at
    // the end of a segment counts as a number, `tp2` before `tp10`, and an index too,
    // `decoupling[2]` before `decoupling[10]`), from 1. The path alone decides it, so the order
    // of statements, the layout engine, KiCad and how a part is drawn change nothing. The
    // parts of the design are in the same order, which the placer and the drawing read.
    let declared: Vec<InstId> = model
        .instances()
        .filter(|(_, i)| i.kind == InstKind::Part)
        .map(|(id, _)| id)
        .collect();
    let mut ids: Vec<(String, InstId)> =
        declared.iter().map(|&id| (model.path_of(id), id)).collect();
    ids.sort_by(|a, b| natural_path_order(&a.0, &b.0));

    // The letters and the first number of each part, and how many parts it is (a bank is one
    // part in the design for each of its capacitors, numbered one after the other, ADR 0052).
    let mut counters: BTreeMap<String, usize> = BTreeMap::new();
    let mut lettered: HashMap<InstId, (String, usize, usize)> = HashMap::new();
    for (_, id) in &ids {
        let id = *id;
        netsmith_core::job::checkpoint();
        let letters = designator_of(model, id, &mut errors);
        let bank = bank_of(model, id, &mut errors);
        let n = counters.entry(letters.clone()).or_insert(0);
        let first = *n + 1;
        *n += bank;
        lettered.insert(id, (letters, first, bank));
    }

    let kinds: HashMap<InstId, String> = declared
        .iter()
        .map(|&id| {
            let inst = model.instance(id);
            let kind = match model.bank(id) {
                Some(b) if inst.type_name == b.generic => b.member,
                _ => inst.type_name.clone(),
            };
            (id, kind)
        })
        .collect();
    let mut refdes_of: HashMap<InstId, Vec<String>> = HashMap::new();
    let mut parts = Vec::new();
    let mut fitted: Vec<environment::Fitted<'_>> = Vec::new();
    let mut sourcing = Vec::new();
    let mut origins = Vec::new();
    // The path of each part that is a bank of more than one, and how many (ADR 0052).
    let mut banks: BTreeMap<String, usize> = BTreeMap::new();
    for (_, id) in &ids {
        netsmith_core::job::checkpoint();
        let inst = model.instance(*id);
        let (letters, first, bank) = lettered[id].clone();
        // What each part the design makes is: the part's own kind, or for a bank (ADR 0123) the
        // generic its parts are, so a member of a `CapacitorBank` is drawn, mapped to a footprint
        // and listed as the `Capacitor` it is.
        let kind = &kinds[id];
        let pick = solution.picks.iter().find(|p| p.part == *id);
        // The order code of a part that states `.variants` (issue 1311), which the checker
        // narrowed its `.mpn` to: the number and the value of a part that picks from no catalog,
        // so that the bill of materials names the code.
        let order_code = model
            .variants
            .contains_key(id)
            .then(|| text_of(model, *id, "mpn"))
            .flatten();
        // A label's value is the text it prints.
        let value = match (inst.marks.declared_kind, text_of(model, *id, "text")) {
            (Some(PartKind::Label), Some(text)) => text,
            // A logo's value is its file, so a list of parts says which drawing it is.
            (Some(PartKind::Logo), _) => {
                text_of(model, *id, "file").unwrap_or_else(|| default_value(inst, kind))
            }
            _ => pick
                .map(|p| p.label.clone())
                .or_else(|| order_code.clone())
                .unwrap_or_else(|| default_value(inst, kind)),
        };
        let package =
            features::hole_package(model, *id, text_of(model, *id, "package"), &mut errors);
        let (assembly, assembly_stated) = match assembly_of(model, *id, pick, &mut errors) {
            Some(a) => (a, true),
            None => match project_default {
                Some(a) => (a, true),
                // The kind's own default, its `#[assembly(...)]` (ADR 0125): a header is
                // through-hole and soldered by hand.
                None => (
                    inst.marks
                        .assembly
                        .as_deref()
                        .and_then(Assembly::parse)
                        .unwrap_or(Assembly::Fab),
                    false,
                ),
            },
        };
        // A part that is not fitted (issue #1273) is assembled by nobody, whatever else says
        // who assembles it, and keeps the range it is tuned in.
        let tune = (text_of(model, *id, "fitted").as_deref() == Some("false")).then(|| match model
            .member(*id, "tune")
        {
            Some(Member::Field(f)) if model.field(f).domain != Domain::Unset => {
                Some(model.field(f).domain.to_string())
            }
            _ => None,
        });
        let (assembly, assembly_stated) = match tune {
            Some(_) => (Assembly::None, true),
            None => (assembly, assembly_stated),
        };
        // A part that is not fitted and was given no row shows the range it is tuned in.
        let value = match (&tune, pick) {
            (Some(Some(range)), None) => range.clone(),
            _ => value,
        };
        fitted.push(environment::Fitted {
            part: *id,
            kind,
            assembly,
            pick,
        });
        // The parts of a bank of two or more are `path[0]`, `path[1]`, ..., so that each is a
        // part a layout file keys by its own path, as the members of `N x Kind` are.
        if bank > 1 {
            banks.insert(model.path_of(*id), bank);
        }
        // What a part of the design provides, of the standard generics: a provider of a bank
        // makes parts that provide what the bank's parts are (ADR 0123).
        let provides: Vec<String> = match model.bank(*id) {
            Some(b) if inst.type_name != b.generic => vec![b.member],
            _ => inst
                .provides
                .iter()
                .filter(|g| inst.standard.contains(g))
                .cloned()
                .collect(),
        };
        for k in 0..bank {
            let refdes = format!("{letters}{}", first + k);
            let path = bank_member_path(&model.path_of(*id), k, bank);
            sourcing.push(Sourced {
                refdes: refdes.clone(),
                kind: kind.clone(),
                value: value.clone(),
                package: package.clone(),
                lcsc: pick.and_then(|p| p.lcsc.clone()),
                mpn: pick
                    .and_then(|p| p.mpn.clone())
                    .or_else(|| order_code.clone()),
                library: pick.and_then(|p| p.library),
                assembly,
                assembly_stated,
                catalog_height: pick.and_then(|p| p.height),
                generic: pick.and_then(|p| p.generic.clone()),
                resolved_row: None,
                not_fitted: tune.as_ref().map(|tune| netsmith_core::NotFitted {
                    refdes: refdes.clone(),
                    tune: tune.clone(),
                }),
            });
            parts.push(Part {
                refdes: refdes.clone(),
                kind: kind.clone(),
                // Only the generics of the standard library: a tool reads them by name, and a
                // project's own generic of the same name is not that one.
                provides: provides.clone(),
                declared_kind: inst.marks.declared_kind,
                // The schematic marks a part that is not fitted where its value is written
                // (issue #1273); KiCad's draws its symbol crossed out as well.
                value: match tune {
                    Some(_) => format!("DNP {value}"),
                    None => value.clone(),
                },
                package: package.clone(),
                path,
                pins: pins_of(model, *id),
            });
            refdes_of.entry(*id).or_default().push(refdes);
            origins.push(*id);
        }
    }

    // Where each part is in the order of the design's parts, which is the order of their paths
    // (above): a net's pins are sorted by it, not by designator, so renumbering or relettering
    // the parts reorders no net, and the placer's nets and the router's lines stay as they were
    // (owner decision, issue #1048).
    let position: HashMap<&str, usize> = parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.refdes.as_str(), i))
        .collect();
    let mut nets = Vec::new();
    // Pins with no pad, gathered per part so each part is reported once.
    let mut padless: BTreeMap<InstId, Vec<String>> = BTreeMap::new();
    // The pads of each part's pin, for the pins that state the current of their branch or
    // how a pour joins them.
    let mut pads_of: HashMap<InstId, Vec<PinRef>> = HashMap::new();
    for net in model.nets() {
        netsmith_core::job::checkpoint();
        let mut pins: Vec<PinRef> = Vec::new();
        for &terminal in &net.terminals {
            let part = owning_part(model, terminal);
            // One reference for each pad of the pin. A pin with no row of its own that the
            // part joins to one that has needs none: it is that pin.
            let pads: Vec<String> = match model.pin_row(terminal) {
                Some(row) => row.pads.clone(),
                None if model.is_alias(terminal) => continue,
                None => {
                    let inside = model.path_of(terminal);
                    let prefix = format!("{}.", model.path_of(part));
                    padless
                        .entry(part)
                        .or_default()
                        .push(inside.strip_prefix(&prefix).unwrap_or(&inside).to_string());
                    continue;
                }
            };
            for pad in pads {
                // Each part of a bank has the pad on the net.
                for refdes in &refdes_of[&part] {
                    let pin = PinRef {
                        refdes: refdes.clone(),
                        pin: pad.clone(),
                    };
                    pads_of.entry(terminal).or_default().push(pin.clone());
                    // Two pins of one part on the same pad are one physical pin.
                    if !pins.contains(&pin) {
                        pins.push(pin);
                    }
                }
            }
        }
        pins.sort_by(|a, b| {
            (position[a.refdes.as_str()], &a.pin).cmp(&(position[b.refdes.as_str()], &b.pin))
        });
        if !pins.is_empty() {
            nets.push(Net {
                name: net.name.clone(),
                pins,
            });
        }
    }
    for (part, mut pins) in padless {
        pins.sort();
        pins.dedup();
        let ty = &model.instance(part).type_name;
        let path = model.path_of(part);
        match model.pinout(part) {
            Pinout::Unknown { packages } => errors.push(format!(
                "`{path}` is a `{ty}` and no package is chosen, so its pads are not known; its pin tables are for {}: choose one, for example with `.{path}.package &= \"{}\"`",
                packages
                    .iter()
                    .map(|p| format!("`{p}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
                packages[0]
            )),
            _ => errors.push(format!(
                "`{path}` is a `{ty}` and its pins ({}) have no pads, so it cannot be placed on a symbol; add them to a `pins` table of `{ty}`",
                pins.iter().map(|p| format!("`{p}`")).collect::<Vec<_>>().join(", "),
            )),
        }
    }

    let net_names: Vec<String> = nets.iter().map(|n| n.name.clone()).collect();
    let interfaces = interfaces_of(model, &nets);
    let modules = modules_of(model, &nets);
    let notes = notes_of(model, &refdes_of);
    let process = process_of(model, &net_names, &mut errors);
    let layers = process.as_ref().map_or(2, |p| p.stackup.layers);
    let least_text = process.as_ref().map_or(0.0, |p| p.min_text_height);
    let least = features::Least {
        clearance: process.as_ref().map_or(0.0, |p| p.min_clearance),
        track: process.as_ref().map_or(0.0, |p| p.min_track),
    };
    let mut features =
        features::features_of(model, &net_names, layers, least_text, least, &mut errors);
    let (constraints, constraint_sites) = features::placements(model, &mut errors);
    features.constraints = constraints;
    features.not_fitted = sourcing
        .iter()
        .filter_map(|s: &Sourced| s.not_fitted.clone())
        .collect();
    let (offsets, offset_sites) = features::offsets(model, &mut errors);
    features.offsets = offsets;
    let ((pin_distances, pin_distance_sites), (net_routes, net_route_sites)) =
        features::rf_statements(model, &mut errors);
    features.pin_distances = pin_distances;
    features.net_routes = net_routes;
    let (soft_margins, mut soft_margin_sites) = features::soft_margins(model);
    features.soft_margins = soft_margins;
    let (keep_apart, keep_apart_sites) = features::keep_apart(model);
    features.keep_apart = keep_apart;
    let (protrusions, mut protrusion_sites) = features::protrusions(model, &mut errors);
    features.protrusions = protrusions;
    let (models, mut model_sites) = features::models(model, &mut errors);
    features.models = models;
    let (case_models, case_model_sites) = features::case_models(model, &mut errors);
    features.case_models = case_models;
    let least_track = process.as_ref().map_or(0.0, |p| p.min_track);
    let (logos, logo_sites) = features::logos(model, least_text, least_track, &mut errors);
    features.logos = logos;
    let part_layout = features::layout(model, &solution.picks, layers);
    features.layout = part_layout.guidelines;
    features.keepouts.extend(part_layout.keepouts);
    let (mut constraint_sites, mut offset_sites) = (constraint_sites, offset_sites);
    let mut bank_notes: Vec<String> = Vec::new();
    features::expand_banks(
        &mut features,
        &mut constraint_sites,
        &mut offset_sites,
        &mut soft_margin_sites,
        &mut protrusion_sites,
        &mut model_sites,
        &banks,
        &mut bank_notes,
    );
    // Tracks sized from the current of nets no rail gave a width (ADR 0036).
    let plan = match &process {
        Some(process) => guidelines::plan_tracks(
            model,
            process,
            &features,
            &solution.current_bounds,
            &solution.net_currents,
        ),
        None => guidelines::TrackPlan::default(),
    };
    features.net_widths.extend(plan.widths);
    // A `route` statement of a part's `layout` block widens its net (issue #994).
    let part_layout_widths = part_layout.widths.clone();
    for (net, width) in part_layout.widths {
        match features.net_widths.iter_mut().find(|(n, _)| *n == net) {
            Some((_, least)) => *least = least.max(width),
            None => features.net_widths.push((net, width)),
        }
    }
    features.net_widths.sort_by(|a, b| a.0.cmp(&b.0));
    features.net_currents = plan.currents;
    features.net_currents.sort_by(|a, b| a.net.cmp(&b.net));
    // The pins that state how a pour joins their pads (issue #1276).
    features.pad_pours = features::pad_pours(model, &pads_of, least_track, &mut errors);
    // The branches whose pins state their current (issue #1113), never narrower than a `route`
    // statement of a part's `layout` block asks of their net.
    for b in plan.branches {
        let Some(pins) = pads_of.remove(&b.pin) else {
            continue;
        };
        let floor = part_layout_widths
            .iter()
            .filter(|(n, _)| *n == b.net)
            .map(|(_, w)| *w)
            .fold(0.0, f64::max);
        features.branch_currents.push(netsmith_core::BranchCurrent {
            net: b.net,
            path: model.path_of(b.pin),
            pins,
            current: b.current,
            width: b.width.max(floor),
            least_width: b.least_width,
            inner_least_width: b.inner_least_width,
            vias: b.vias,
        });
    }
    features.branch_currents.sort_by(|a, b| a.path.cmp(&b.path));
    // The pins whose current the model knows, below their net's (issue #1333): the design
    // rules check sizes the copper that only feeds them for what they draw.
    for l in plan.pin_currents {
        let Some(pins) = pads_of.get(&l.pin) else {
            continue;
        };
        features.pin_currents.push(netsmith_core::PinCurrent {
            net: l.net,
            path: model.path_of(l.pin),
            pins: pins.clone(),
            current: l.current,
        });
    }
    features.pin_currents.sort_by(|a, b| a.path.cmp(&b.path));
    errors.extend(plan.errors);
    let mut guideline_findings = plan.findings;
    // Every fitted part against the board's environment, if it states one (ADR 0099).
    let judged = environment::judge(model, &fitted, &mut errors);
    guideline_findings.extend(judged.findings);
    // How each part may be soldered against who solders it (issue #1261).
    soldering::judge(model, &fitted, &mut errors);

    if errors.is_empty() {
        Ok(Designed {
            design: Design {
                name: name.to_string(),
                parts,
                nets,
                process,
                features,
                interfaces,
                notes,
                modules,
            },
            sourcing,
            origins,
            guidelines: guideline_findings,
            environment: judged.tally,
            note_causes: std::iter::repeat_n(netsmith_core::Cause::Track, plan.notes.len())
                .chain(std::iter::repeat_n(
                    netsmith_core::Cause::Other,
                    bank_notes.len(),
                ))
                .collect(),
            notes: {
                let mut notes = plan.notes;
                notes.extend(bank_notes);
                notes
            },
            constraint_sites,
            offset_sites,
            pin_distance_sites,
            net_route_sites,
            soft_margin_sites,
            keep_apart_sites,
            protrusion_sites,
            model_sites,
            case_model_sites,
            logo_sites,
        })
    } else {
        errors.sort();
        errors.dedup();
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    /// The `part` and `generic` names of the standard library (a standard generic is a part
    /// that is its own first provider, ADR 0068) and the letters their body states (`None` for
    /// none), and the generics each provides. A provider has the letters of its generic. A
    /// module-shaped generic (`BuckConverter`, which modules provide) is left out: it is never
    /// a placed part (`netsmith_check::generic_is_part_shaped`).
    fn std_definitions() -> Vec<(String, Option<String>, Vec<String>)> {
        let std = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std");
        let mut parts = Vec::new();
        // The generics interfaces provide: processes (ADR 0113), not parts.
        let mut of_interfaces: Vec<String> = Vec::new();
        let mut dirs = vec![std];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "nsm") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    // A module-shaped generic (no `pins` table and no catalog, ADR 0068) is
                    // made as a module and never placed, so it has no designator: the parts
                    // inside its providers carry theirs. The checker's own rule says which.
                    let file = netsmith_lang::parse(&text).unwrap();
                    let module_shaped: Vec<String> = file
                        .definitions
                        .iter()
                        .filter(|d| {
                            d.kind == netsmith_lang::ast::DefKind::Generic
                                && !netsmith_check::generic_is_part_shaped(d)
                        })
                        .map(|d| d.name.name.clone())
                        .collect();
                    let mut current: Option<usize> = None;
                    for line in text.lines() {
                        if let Some((_, g)) = line
                            .strip_prefix("interface ")
                            .and_then(|r| r.split_once(" provides "))
                        {
                            of_interfaces.push(g.trim_end_matches(" {").trim().to_string());
                        }
                        if let Some(rest) = line
                            .strip_prefix("part ")
                            .or_else(|| line.strip_prefix("generic "))
                        {
                            let name = rest.split([' ', '{']).next().unwrap();
                            if module_shaped.iter().any(|m| m == name) {
                                current = None;
                                continue;
                            }
                            let provides: Vec<String> = rest
                                .split_once(" provides ")
                                .map(|(_, g)| {
                                    g.trim_end_matches(" {")
                                        .split(',')
                                        .map(|n| n.trim().to_string())
                                        .collect()
                                })
                                .unwrap_or_default();
                            parts.push((name.to_string(), None, provides));
                            current = Some(parts.len() - 1);
                        } else if line.starts_with('}') {
                            current = None;
                        } else if let (Some(i), Some(rest)) =
                            (current, line.trim().strip_prefix(".designator &= \""))
                        {
                            parts[i].1 = Some(rest.trim_end_matches('"').to_string());
                        }
                    }
                }
            }
        }
        parts.retain(|(name, _, _)| !of_interfaces.contains(name));
        let own: Vec<(String, Option<String>)> = parts
            .iter()
            .map(|(n, l, _)| (n.clone(), l.clone()))
            .collect();
        for (_, letters, provides) in &mut parts {
            if letters.is_none() {
                *letters = provides.iter().find_map(|g| {
                    own.iter()
                        .find(|(n, _)| n == g)
                        .and_then(|(_, l)| l.clone())
                });
            }
        }
        parts
    }

    /// The names and letters of the standard library's definitions.
    /// The names of the standard library's `generic` definitions.
    fn std_generics() -> Vec<String> {
        let std = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std");
        let mut names = Vec::new();
        let mut dirs = vec![std];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "nsm") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    names.extend(text.lines().filter_map(|line| {
                        let rest = line.strip_prefix("generic ")?;
                        Some(rest.split([' ', '{']).next().unwrap().to_string())
                    }));
                }
            }
        }
        names
    }

    fn std_parts() -> Vec<(String, Option<String>)> {
        std_definitions()
            .into_iter()
            .map(|(name, letters, _)| (name, letters))
            .collect()
    }

    #[test]
    fn each_standard_kind_has_its_usual_designator_letters() {
        let expected = [
            ("Resistor", "R"),
            ("Capacitor", "C"),
            ("Inductor", "L"),
            ("FerriteBead", "FB"),
            ("Led", "D"),
            ("Diode", "D"),
            ("Schottky", "D"),
            ("Zener", "D"),
            ("Tvs", "D"),
            ("TvsBidirectional", "D"),
            ("NMosfet", "Q"),
            ("PMosfet", "Q"),
            ("Crystal", "Y"),
            ("Fuse", "F"),
            ("Switch", "SW"),
            ("Header1x2", "J"),
            ("Socket1x4", "J"),
            ("Header2x10", "J"),
            ("Coax", "J"),
            ("UsbCPower", "J"),
            ("UsbC2", "J"),
            ("Obd2M", "J"),
            ("ProgrammingPads6", "J"),
            ("TestPoint", "TP"),
            ("MountingHole", "H"),
            ("PlatedHole", "H"),
            ("Fiducial", "FID"),
            ("Label", "LBL"),
            ("Logo", "LOGO"),
            ("LinearRegulator", "U"),
            ("LoadSwitch", "U"),
        ];
        let parts = std_parts();
        for (kind, letters) in expected {
            let found = parts.iter().find(|(name, _)| name == kind);
            assert_eq!(
                found.and_then(|(_, l)| l.as_deref()),
                Some(letters),
                "{kind}"
            );
        }
    }

    /// A kind added to the standard library gets its letters on purpose: every `part` in
    /// `std/` states `.designator &= "..."`, made of capital letters.
    #[test]
    fn every_standard_part_states_its_designator_letters() {
        let parts = std_parts();
        assert!(parts.len() > 30, "found only {parts:?}");
        let unlettered: Vec<&String> = parts
            .iter()
            .filter(|(_, l)| l.is_none())
            .map(|(name, _)| name)
            .collect();
        assert!(
            unlettered.is_empty(),
            "no `.designator &=` in these standard parts: {unlettered:?}"
        );
        for (name, letters) in &parts {
            let letters = letters.as_deref().unwrap();
            assert!(
                !letters.is_empty() && letters.chars().all(|c| c.is_ascii_uppercase()),
                "{name} has letters {letters:?}"
            );
        }
    }

    /// Only definitions that become a placed part need letters: a module-shaped generic is
    /// left out, a part-shaped one (a pin table or a catalog) is checked like a part.
    #[test]
    fn a_module_shaped_generic_needs_no_letters_and_a_part_shaped_one_does() {
        let names: Vec<String> = std_parts().into_iter().map(|(n, _)| n).collect();
        assert!(!names.iter().any(|n| n == "BuckConverter"), "{names:?}");
        for part_shaped in ["Connector", "BuckIc", "Resistor"] {
            assert!(
                names.iter().any(|n| n == part_shaped),
                "{part_shaped}: {names:?}"
            );
        }
    }

    /// The layout and the placer find connectors by the kind `connector`, which the standard
    /// `Connector` states (ADR 0125), not by a name or by the letters of a designator: every
    /// standard kind that has the letters `J` provides it, and no other does.
    #[test]
    fn every_standard_connector_kind_provides_the_connector_generic() {
        // A generic provides nothing itself: a generic with the letters `J` (the USB-C sink
        // receptacle) passes them to its providers, which are checked here like any part.
        let generics = std_generics();
        for (name, letters, provides) in std_definitions() {
            if generics.contains(&name) {
                continue;
            }
            assert_eq!(
                provides.iter().any(|g| g == "Connector"),
                letters.as_deref() == Some("J") && name != "Connector",
                "{name} has letters {letters:?} and provides {provides:?}"
            );
        }
    }
}
