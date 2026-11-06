//! Turns a solved board into the tool-independent [`Design`] that exporters read.
//!
//! Every part gets a reference designator, a value, and the pads of its terminals. Nothing
//! here names a CAD tool: a resistor is still a `Resistor`, and an exporter decides what
//! that looks like.
//!
//! Alongside the design it returns where each part comes from: the supplier's part number
//! the solver picked, and whether the part is assembled by the manufacturer or soldered by
//! hand. Those are sourcing data, not CAD data, so they are not part of the [`Design`].

mod features;
pub use features::Sites;
pub mod guidelines;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use netsmith_check::{
    Board, Domain, InstId, InstKind, Member, Model, Panel, PinKind, Pinout, System, SystemBoard,
};
use netsmith_core::{
    Design, InnerLayers, Net, PanelRules, PanelSpec, Part, PartPin, PinRef, Process, Stackup,
};
use netsmith_solve::{Library, Solution};

/// The letters of a reference designator for a part with none stated: an integrated circuit,
/// the one kind that has no letters of its own (IEEE 315, ASME Y14.44).
const DEFAULT_LETTERS: &str = "U";

/// The order in which parts are numbered: paths compared segment by segment, and a number at
/// the end of a segment compared as a number, so `pads.tp2` comes before `pads.tp10` and
/// `.tp02` and `.tp2` stay apart (the shorter digit string first). The same inputs always
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

/// `tp` and `10` of `tp10`; a segment that does not end in digits has none.
fn split_trailing_number(segment: &str) -> (&str, Option<&str>) {
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

/// Whether a kind of part is through-hole and soldered by hand, so the manufacturer does not
/// assemble it. A kind a CAD mapping marks through-hole is also hand-soldered; see
/// `netsmith::build`.
fn hand_soldered(kind: &str) -> bool {
    kind.starts_with("Header") || kind.starts_with("Socket")
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
}

impl Assembly {
    /// The word for it in the language, in the catalog files and in messages.
    pub fn word(self) -> &'static str {
        match self {
            Self::Fab => "fab",
            Self::Hand => "hand",
            Self::None => "none",
        }
    }

    /// The word for it in the catalog files and the project manifest (`"fab"`).
    pub fn parse(word: &str) -> Option<Self> {
        [Self::Fab, Self::Hand, Self::None]
            .into_iter()
            .find(|a| a.word() == word)
    }

    /// The variant of the language's `AssemblyMode` for it (`Fab`, ADR 0054).
    pub fn variant(self) -> &'static str {
        match self {
            Self::Fab => "Fab",
            Self::Hand => "Hand",
            Self::None => "None",
        }
    }

    /// The part a variant of `AssemblyMode` names.
    pub fn from_variant(variant: &str) -> Option<Self> {
        [Self::Fab, Self::Hand, Self::None]
            .into_iter()
            .find(|a| a.variant() == variant)
    }
}

/// What a part shows as its value when the solver did not choose one for it.
fn default_value(kind: &str) -> String {
    match kind {
        "Led" => "LED".to_string(),
        "Header1x2" => "Conn_01x02".to_string(),
        other => other.to_string(),
    }
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
                "`{}` has `.assembly` {variant}, which is not `AssemblyMode::Fab`, `AssemblyMode::Hand` or `AssemblyMode::None`",
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
            "`{}` has `.assembly` \"{word}\" in its catalog row, which is not \"fab\", \"hand\" or \"none\"",
            model.path_of(part)
        ));
    }
    found
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
            },
            pads: row.pads.clone(),
            function: model.function_of(row.pin),
        })
        .collect()
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
    let kind = model.instance(process).type_name.clone();
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
    // A rule is the bottom of its range. A length is rounded to a nanometre of the
    // millimetre so that `0.1mm` is `0.1`, not `0.10000000000000002`.
    let mm = |d: Option<Domain>| match d {
        Some(Domain::Quantity(q)) => (q.lo() * 1e12).round() / 1e9,
        _ => 0.0,
    };
    let text = |d: Option<Domain>| match d {
        Some(Domain::Text(t)) => t,
        _ => String::new(),
    };
    let number = |d: Option<Domain>| match d {
        Some(Domain::Quantity(q)) => q.lo(),
        _ => 0.0,
    };
    let layers = number(find("stackup.layers")).round() as u32;
    let mut built = Process {
        name: kind.clone(),
        fabricator: text(find("fabricator")),
        source: text(find("source")),
        min_track: mm(find("min_track")),
        min_clearance: mm(find("min_clearance")),
        min_drill: mm(find("min_drill")),
        min_via_diameter: mm(find("min_via_diameter")),
        min_annular_ring: mm(find("min_annular_ring")),
        min_hole_to_hole: mm(find("min_hole_to_hole")),
        min_edge_clearance: mm(find("min_edge_clearance")),
        min_silk_width: mm(find("min_silk_width")),
        min_text_height: mm(find("min_text_height")),
        min_silk_to_pad: mm(find("min_silk_to_pad")),
        min_mask_bridge: mm(find("min_mask_bridge")),
        // An upper bound, so the top of what the board left.
        via_in_pad: match find("via_in_pad") {
            Some(Domain::Quantity(q)) => (q.hi() * 1e12).round() / 1e9,
            _ => 0.0,
        },
        // Unstated in the library; a board may state it. An upper bound, so the top.
        max_temperature_rise: match model.field_at("process.max_temperature_rise") {
            Some(f) => match &model.field(f).domain {
                Domain::Quantity(q) => Some(q.hi()),
                _ => None,
            },
            None => None,
        },
        stackup: Stackup {
            layers,
            thickness: match find("stackup.thickness") {
                Some(Domain::Quantity(q)) => {
                    ((q.lo() * 1e12).round() / 1e9, (q.hi() * 1e12).round() / 1e9)
                }
                _ => (0.0, 0.0),
            },
            copper_weight: number(find("stackup.copper_weight")),
            inner: None,
        },
    };
    // A board of more than two layers has inner layers, each a plane on a net the board
    // names, and the build-up round them.
    if layers > 2 {
        let mut planes = Vec::new();
        for n in 1..=layers - 2 {
            let name = format!("inner{n}");
            let net = match model.field_at(&format!("process.{name}.net")) {
                None => {
                    problems.borrow_mut().push(format!(
                        "`process` is a `{kind}` with {layers} copper layers and has no `{name}` plane, which each inner layer needs"
                    ));
                    continue;
                }
                Some(f) => match &model.field(f).domain {
                    Domain::Text(net) => net.clone(),
                    _ => {
                        problems.borrow_mut().push(format!(
                            "`process.{name}` is an inner layer of a {layers}-layer board and has no net: the board says which, for example `.process.{name}.net &= \"GND\"`"
                        ));
                        continue;
                    }
                },
            };
            if !nets.contains(&net) {
                problems.borrow_mut().push(format!(
                    "`process.{name}` is a plane on the net `{net}`, but the board has no net of that name (its nets: {})",
                    nets.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")
                ));
            }
            planes.push(net);
        }
        built.stackup.inner = Some(InnerLayers {
            planes,
            copper: mm(find("stackup.inner_copper")),
            prepreg: mm(find("stackup.prepreg")),
            core: mm(find("stackup.core")),
            prepreg_epsilon: number(find("stackup.prepreg_epsilon")),
            core_epsilon: number(find("stackup.core_epsilon")),
            hole_clearance: mm(find("min_plane_hole_clearance")),
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
    /// Set for a generic part (ADR 0047): the solver chose its value and nothing else, and
    /// `lcsc`, `mpn` and `library` are empty until it is resolved to a row of a catalog.
    pub generic: Option<netsmith_solve::Generic>,
}

impl Sourced {
    /// Whether the manufacturer places it.
    pub fn assembled(&self) -> bool {
        self.assembly == Assembly::Fab
    }
}

/// A board lowered: the tool-independent design and the sourcing of each of its parts, in
/// the same order.
#[derive(Debug, Clone)]
pub struct Lowered {
    pub design: Design,
    pub sourcing: Vec<Sourced>,
    /// The instance each part was made from, in the same order.
    pub origins: Vec<InstId>,
    /// What the design does against a guideline (ADR 0036): findings that a person decides
    /// about, which the project may make blocking (`guidelines = "error"`).
    pub guidelines: Vec<guidelines::Guideline>,
    /// Things worth knowing that did not stop the lowering: the nets whose tracks were not
    /// sized from their current because it is not known (ADR 0036).
    pub notes: Vec<String>,
    /// Where the design states each requirement on a position (ADR 0045), for a caller that
    /// has the source files to put in `design.features.constraints[..].source`.
    pub constraint_sites: Sites,
    /// Where the design states each of `design.features.offsets`, in the same order.
    pub offset_sites: Vec<(netsmith_check::SourceId, netsmith_lang::Span)>,
    /// Where the design states each of `design.features.protrusions`, in the same order.
    pub protrusion_sites: Vec<Vec<(netsmith_check::SourceId, netsmith_lang::Span)>>,
}

/// A panel as the specification `fab` builds it from: the board it repeats, the grid and the
/// board house's rules (its `fab` member, read as the least the panel now allows for a lower
/// bound and the most for an upper bound). Every mistake is one line.
pub fn lower_panel(panel: &Panel) -> Result<PanelSpec, Vec<String>> {
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

pub fn lower(board: &Board, solution: &Solution) -> Result<Lowered, Vec<String>> {
    lower_model(&board.name, &solution.model, solution)
}

/// One board of a solved system. The design is named after the board's definition, so the
/// files read `PowerBoard.kicad_sch` wherever the system put them.
pub fn lower_system_board(
    system: &System,
    member: &SystemBoard,
    solution: &Solution,
) -> Result<Lowered, Vec<String>> {
    debug_assert!(system.boards.iter().any(|b| b.inst == member.inst));
    let model = solution.model.scoped_to(member.inst);
    let mut lowered = lower_model(&member.type_name, &model, solution)?;
    // A mate that works turned end for end is a guideline (ADR 0040), said once, by the board
    // of the first connector the system names.
    for mate in solution.model.mates() {
        if let Some(why) = &mate.reversible
            && solution.model.board_of(mate.a) == Some(member.inst)
        {
            lowered.guidelines.push(guidelines::Guideline {
                rule: "mating",
                message: why.clone(),
                at: Some(mate.a),
            });
        }
    }
    Ok(lowered)
}

fn lower_model(name: &str, model: &Model, solution: &Solution) -> Result<Lowered, Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

    // The project's default for who assembles a part nothing else speaks for (ADR 0028).
    let project_default = solution.default_assembly.as_deref().and_then(|word| {
        let found = Assembly::parse(word);
        if found.is_none() {
            errors.push(format!(
                "the project's default assembly \"{word}\" is not \"fab\", \"hand\" or \"none\""
            ));
        }
        found
    });

    // Parts are numbered in path order, so the numbering does not depend on the order the
    // source declares them in. A number at the end of a segment counts as a number
    // (`tp2` before `tp10`, ADR 0048).
    let mut ids: Vec<(String, InstId)> = model
        .instances()
        .filter(|(_, i)| i.kind == InstKind::Part)
        .map(|(id, _)| (model.path_of(id), id))
        .collect();
    ids.sort_by(|a, b| natural_path_order(&a.0, &b.0));

    let mut counters: BTreeMap<String, usize> = BTreeMap::new();
    let mut refdes_of: HashMap<InstId, Vec<String>> = HashMap::new();
    let mut parts = Vec::new();
    let mut sourcing = Vec::new();
    let mut origins = Vec::new();
    // The path of each part that is a bank of more than one, and how many (ADR 0052).
    let mut banks: BTreeMap<String, usize> = BTreeMap::new();
    for (_, id) in &ids {
        let inst = model.instance(*id);
        let letters = designator_of(model, *id, &mut errors);
        let pick = solution.picks.iter().find(|p| p.part == *id);
        // A label's value is the text it prints.
        let value = match (inst.type_name.as_str(), text_of(model, *id, "text")) {
            ("Label", Some(text)) => text,
            _ => pick
                .map(|p| p.label.clone())
                .unwrap_or_else(|| default_value(&inst.type_name)),
        };
        let package = text_of(model, *id, "package");
        let (assembly, assembly_stated) = match assembly_of(model, *id, pick, &mut errors) {
            Some(a) => (a, true),
            None => match project_default {
                Some(a) => (a, true),
                None if hand_soldered(&inst.type_name) => (Assembly::Hand, false),
                None => (Assembly::Fab, false),
            },
        };
        // A bank is one part in the design for each of its capacitors, numbered one after
        // the other (ADR 0052). The first has the path of the instance and the others add
        // `#2`, `#3`, so that each is a part a layout file keys by its own path.
        let bank = bank_of(model, *id, &mut errors);
        if bank > 1 {
            banks.insert(model.path_of(*id), bank);
        }
        for k in 1..=bank {
            let n = counters.entry(letters.clone()).or_insert(0);
            *n += 1;
            let refdes = format!("{letters}{n}");
            let path = match k {
                1 => model.path_of(*id),
                k => format!("{}#{k}", model.path_of(*id)),
            };
            sourcing.push(Sourced {
                refdes: refdes.clone(),
                kind: inst.type_name.clone(),
                value: value.clone(),
                package: package.clone(),
                lcsc: pick.and_then(|p| p.lcsc.clone()),
                mpn: pick.and_then(|p| p.mpn.clone()),
                library: pick.and_then(|p| p.library),
                assembly,
                assembly_stated,
                generic: pick.and_then(|p| p.generic.clone()),
            });
            parts.push(Part {
                refdes: refdes.clone(),
                kind: inst.type_name.clone(),
                provides: inst.provides.clone(),
                value: value.clone(),
                package: package.clone(),
                path,
                pins: pins_of(model, *id),
            });
            refdes_of.entry(*id).or_default().push(refdes);
            origins.push(*id);
        }
    }

    let mut nets = Vec::new();
    // Pins with no pad, gathered per part so each part is reported once.
    let mut padless: BTreeMap<InstId, Vec<String>> = BTreeMap::new();
    for net in model.nets() {
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
                    // Two pins of one part on the same pad are one physical pin.
                    if !pins.contains(&pin) {
                        pins.push(pin);
                    }
                }
            }
        }
        pins.sort_by(|a, b| (&a.refdes, &a.pin).cmp(&(&b.refdes, &b.pin)));
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
    let process = process_of(model, &net_names, &mut errors);
    let layers = process.as_ref().map_or(2, |p| p.stackup.layers);
    let least_text = process.as_ref().map_or(0.0, |p| p.min_text_height);
    let mut features = features::features_of(model, &net_names, layers, least_text, &mut errors);
    let (constraints, constraint_sites) = features::placements(model, &mut errors);
    features.constraints = constraints;
    let (offsets, offset_sites) = features::offsets(model, &mut errors);
    features.offsets = offsets;
    let (protrusions, mut protrusion_sites) = features::protrusions(model, &mut errors);
    features.protrusions = protrusions;
    let (mut constraint_sites, mut offset_sites) = (constraint_sites, offset_sites);
    let mut bank_notes: Vec<String> = Vec::new();
    features::expand_banks(
        &mut features,
        &mut constraint_sites,
        &mut offset_sites,
        &mut protrusion_sites,
        &banks,
        &mut bank_notes,
    );
    // Tracks sized from the current of nets no rail gave a width (ADR 0036).
    let plan = match &process {
        Some(process) => guidelines::plan_tracks(model, process, &features),
        None => guidelines::TrackPlan::default(),
    };
    features.net_widths.extend(plan.widths);
    features.net_widths.sort_by(|a, b| a.0.cmp(&b.0));
    let guideline_findings = plan.findings;

    if errors.is_empty() {
        Ok(Lowered {
            design: Design {
                name: name.to_string(),
                parts,
                nets,
                process,
                features,
            },
            sourcing,
            origins,
            guidelines: guideline_findings,
            notes: {
                let mut notes = plan.notes;
                notes.extend(bank_notes);
                notes
            },
            constraint_sites,
            offset_sites,
            protrusion_sites,
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
    /// that is its own first provider, ADR 0059) and the letters their body states (`None` for
    /// none).
    fn std_parts() -> Vec<(String, Option<String>)> {
        let std = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std");
        let mut parts = Vec::new();
        let mut dirs = vec![std];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "nsm") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    let mut current: Option<usize> = None;
                    for line in text.lines() {
                        if let Some(rest) = line
                            .strip_prefix("part ")
                            .or_else(|| line.strip_prefix("generic "))
                        {
                            let name = rest.split([' ', '{']).next().unwrap();
                            parts.push((name.to_string(), None));
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
        parts
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
            ("LinearRegulator", "U"),
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
}
