//! The semantic difference between two readings of a project (ADR 0059): what the circuit,
//! its rails, its bill of materials and its problems did between a ref and the files on disk.
//! Pure: it takes what [`super::compare`] read out of two analyses and reads nothing else.
//!
//! Identity is by what the design calls a thing, never by what a tool numbered it: a part is
//! its path in the board (`stage.regulator`), a net its name, a module or a port its path.
//! A net that was renamed is found again by its pins, which are named by the paths of their
//! parts (ADR 0017's idea, with the paths to hand: no colouring of a graph is needed).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use netsmith_editor::api::{
    BoardStatus, Bom, BomDiff, BomLine, BudgetChange, CompareBoard, CompareBomChange,
    CompareBomLine, CompareField, CompareInstance, CompareInstanceChange, CompareMate,
    CompareMateChange, CompareNet, CompareNetChange, ComparePart, ComparePartChange,
    ComparePcbArea, ComparePcbPartChange, ComparePcbPlace, ComparePin, CompareRail,
    CompareRailChange, CompareSize, CompareSystem, CompareSystemPort, CompareSystemPortChange,
    EdgeKind, InstancesDiff, LayersChange, Location, MatesDiff, NamesDiff, NetsDiff, OutlineChange,
    PartsDiff, PcbBox, PcbDiff, PcbSide, PowerNode, Problem, ProblemsDiff, RailsDiff, Range,
    RangeChange, Severity, SystemPortsDiff, SystemView,
};

/// One analysis, reduced to what a comparison reads.
#[derive(Debug, Clone)]
pub struct Digest {
    pub boards: BTreeMap<String, BoardDigest>,
    /// The systems, by name.
    pub systems: BTreeMap<String, SystemDigest>,
    pub problems: Vec<Problem>,
}

/// A system as the System view and the power tree read it.
#[derive(Debug, Clone)]
pub struct SystemDigest {
    pub file: String,
    pub view: SystemView,
    /// The nodes of the system's power tree, each named `member:net`.
    pub rails: Vec<PowerNode>,
}

/// A part's place on the PCB.
#[derive(Debug, Clone)]
pub struct PcbPartDigest {
    pub path: String,
    pub designator: String,
    pub place: ComparePcbPlace,
}

/// A board as placed (`pcb::digest`): what the PCB view draws, in the frame of the outline's top
/// left corner so that a board that grew on its left moves none of its parts.
#[derive(Debug, Clone)]
pub struct PcbDigest {
    pub layout_file: String,
    pub outline: CompareSize,
    /// The design states the outline (the case's, or the board's own). A board that does not
    /// is as big as its parts need, so its size moves with every part added.
    pub outline_stated: bool,
    pub layers: Vec<String>,
    pub parts: Vec<PcbPartDigest>,
    pub keepouts: Vec<(String, Option<PcbBox>)>,
    pub cutouts: Vec<(String, Option<PcbBox>)>,
}

#[derive(Debug, Clone)]
pub struct BoardDigest {
    pub file: String,
    /// The reading is the last good one of a file that does not build.
    pub stale: bool,
    pub parts: Vec<PartDigest>,
    pub nets: Vec<NetDigest>,
    pub rails: Vec<PowerNode>,
    pub modules: Vec<InstanceDigest>,
    pub interfaces: Vec<InstanceDigest>,
    pub bom: Result<Bom, String>,
    /// The board placed, or why it could not be; absent when the board was not placed (it is
    /// not on this side).
    pub pcb: Option<Result<PcbDigest, String>>,
}

#[derive(Debug, Clone)]
pub struct PartDigest {
    pub path: String,
    pub designator: String,
    pub kind: String,
    pub value: String,
    pub package: Option<String>,
    pub lcsc: Option<String>,
    pub mpn: Option<String>,
    pub assembly: String,
    pub at: Option<Location>,
    /// Each pin of the part and the net it is on, in the part's own side's names.
    pub pins: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct NetDigest {
    pub name: String,
    pub named: bool,
    /// `(path of the part, pin)`.
    pub pins: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct InstanceDigest {
    pub path: String,
    pub type_name: String,
    pub at: Option<Location>,
}

impl BoardDigest {
    /// A board that is not there: the other side's board is all added (or all removed).
    pub fn nothing() -> BoardDigest {
        BoardDigest {
            file: String::new(),
            stale: false,
            parts: Vec::new(),
            nets: Vec::new(),
            rails: Vec::new(),
            modules: Vec::new(),
            interfaces: Vec::new(),
            bom: Ok(Bom {
                board: String::new(),
                lines: Vec::new(),
                hand: Vec::new(),
                total_usd: None,
                note: String::new(),
                stale_after_days: 0,
                catalogs: Vec::new(),
            }),
            pcb: None,
        }
    }
}

/// The result of comparing two digests.
pub struct Diff {
    pub boards: Vec<CompareBoard>,
    pub systems: Vec<CompareSystem>,
    /// The problems that name no board.
    pub problems: ProblemsDiff,
    pub summary: String,
    pub changed: bool,
}

/// Compare `base` (the ref) with `head` (the files on disk), for the board `only` or for all.
pub fn diff(base: &Digest, head: &Digest, only: Option<&str>) -> Diff {
    let names: BTreeSet<&String> = base.boards.keys().chain(head.boards.keys()).collect();
    let nothing = BoardDigest::nothing();
    let mut boards = Vec::new();
    for name in names {
        if only.is_some_and(|o| o != name.as_str()) {
            continue;
        }
        let (b, h) = (base.boards.get(name), head.boards.get(name));
        boards.push(board(
            name,
            b,
            h,
            b.unwrap_or(&nothing),
            h.unwrap_or(&nothing),
            problems_of(&base.problems, Some(name)),
            problems_of(&head.problems, Some(name)),
        ));
    }
    let problems = problems(
        problems_of(&base.problems, None),
        problems_of(&head.problems, None),
    );
    // A single board asked for leaves the systems and the project's problems out of the sentence.
    let systems = if only.is_none() {
        systems(base, head, &boards)
    } else {
        Vec::new()
    };
    let project = only.is_none().then_some(&problems);
    let summary = summarize(&boards, &systems, project);
    let changed = boards.iter().any(|b| b.status != BoardStatus::Unchanged)
        || systems.iter().any(|s| s.status != BoardStatus::Unchanged)
        || project.is_some_and(|p| !p.new.is_empty() || !p.resolved.is_empty());
    Diff {
        boards,
        systems,
        problems,
        summary,
        changed,
    }
}

/// The problems of one board, or of none. The layout file's (stage `layout`, ADR 0061) are about
/// how the schematic is drawn and not about the circuit, which is what a comparison is of.
fn problems_of(all: &[Problem], board: Option<&String>) -> Vec<Problem> {
    all.iter()
        .filter(|p| p.stage != Some(netsmith_editor::api::ProblemStage::Layout))
        .filter(|p| p.board.as_ref() == board)
        .cloned()
        .collect()
}

fn board(
    name: &str,
    base: Option<&BoardDigest>,
    head: Option<&BoardDigest>,
    b: &BoardDigest,
    h: &BoardDigest,
    base_problems: Vec<Problem>,
    head_problems: Vec<Problem>,
) -> CompareBoard {
    let (parts, canon) = parts(b, h);
    let (nets, renames) = nets(b, h, &canon);
    let rails = rails_of(&b.rails, &h.rails, &renames);
    let modules = instances(&b.modules, &h.modules);
    let interfaces = instances(&b.interfaces, &h.interfaces);
    let (bom, bom_error) = match (&b.bom, &h.bom) {
        (Ok(base), Ok(head)) => (Some(bom(base, head)), None),
        (Err(e), _) => (None, Some(format!("the ref's bill of materials: {e}"))),
        (_, Err(e)) => (None, Some(format!("the bill of materials: {e}"))),
    };
    let problems = problems(base_problems, head_problems);
    let (pcb, pcb_error) = pcb(b, h, &canon);
    let status = match (base, head) {
        (None, _) => BoardStatus::Added,
        (_, None) => BoardStatus::Removed,
        _ => {
            let differs = !parts.added.is_empty()
                || !parts.removed.is_empty()
                || !parts.changed.is_empty()
                || !nets.added.is_empty()
                || !nets.removed.is_empty()
                || !nets.renamed.is_empty()
                || !nets.changed.is_empty()
                || !rails.added.is_empty()
                || !rails.removed.is_empty()
                || !rails.changed.is_empty()
                || !modules.added.is_empty()
                || !modules.removed.is_empty()
                || !modules.changed.is_empty()
                || !interfaces.added.is_empty()
                || !interfaces.removed.is_empty()
                || !interfaces.changed.is_empty()
                || bom.as_ref().is_some_and(|b| {
                    !b.added.is_empty() || !b.removed.is_empty() || !b.changed.is_empty()
                })
                || !problems.new.is_empty()
                || !problems.resolved.is_empty()
                || pcb.as_ref().is_some_and(|p| {
                    !p.moved.is_empty()
                        || p.outline.is_some()
                        || p.layers.is_some()
                        || !p.keepouts.is_empty()
                        || !p.cutouts.is_empty()
                });
            if differs {
                BoardStatus::Changed
            } else {
                BoardStatus::Unchanged
            }
        }
    };
    let mut out = CompareBoard {
        name: name.to_string(),
        status,
        base_file: base.map(|b| b.file.clone()),
        head_file: head.map(|h| h.file.clone()),
        stale: head.is_some_and(|h| h.stale),
        summary: String::new(),
        parts,
        nets,
        rails,
        modules,
        interfaces,
        bom,
        bom_error,
        problems,
        pcb,
        pcb_error,
    };
    out.summary = summarize(std::slice::from_ref(&out), &[], None);
    out
}

/// The leaf of a path: `stage.regulator` is `regulator`.
fn leaf(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

/// What a part is for the purpose of finding it again after it moved: what it is, and which
/// named nets its pins are on (a net the design did not name stands for nothing).
fn signature(
    p: &PartDigest,
    named: &BTreeSet<&str>,
) -> (String, String, Option<String>, Vec<(String, String)>) {
    let mut pins: Vec<(String, String)> = p
        .pins
        .iter()
        .map(|(pin, net)| {
            let net = if named.contains(net.as_str()) {
                net.clone()
            } else {
                String::new()
            };
            (pin.clone(), net)
        })
        .collect();
    pins.sort();
    (p.kind.clone(), p.value.clone(), p.package.clone(), pins)
}

/// The parts: matched by path, and a part that is gone from one path and new at another with
/// the same kind, value, package and named nets is the same part, moved. The second result
/// maps the path on the head side to the path on the ref's of every part that is on both.
fn parts(b: &BoardDigest, h: &BoardDigest) -> (PartsDiff, HashMap<String, String>) {
    let base: BTreeMap<&str, &PartDigest> = b.parts.iter().map(|p| (p.path.as_str(), p)).collect();
    let head: BTreeMap<&str, &PartDigest> = h.parts.iter().map(|p| (p.path.as_str(), p)).collect();
    let mut pairs: Vec<(&PartDigest, &PartDigest)> = head
        .iter()
        .filter_map(|(path, h)| base.get(path).map(|b| (*b, *h)))
        .collect();
    let mut gone: Vec<&PartDigest> = base
        .iter()
        .filter(|(path, _)| !head.contains_key(*path))
        .map(|(_, p)| *p)
        .collect();
    let mut new: Vec<&PartDigest> = head
        .iter()
        .filter(|(path, _)| !base.contains_key(*path))
        .map(|(_, p)| *p)
        .collect();
    // Moved parts.
    fn named(d: &BoardDigest) -> BTreeSet<&str> {
        d.nets
            .iter()
            .filter(|n| n.named)
            .map(|n| n.name.as_str())
            .collect()
    }
    let (named_base, named_head) = (named(b), named(h));
    let mut still_new = Vec::new();
    // The signature of each part that is gone, made once and kept beside it (the two lists lose
    // the same entries).
    let mut gone_sigs: Vec<_> = gone.iter().map(|g| signature(g, &named_base)).collect();
    for p in new {
        let sig = signature(p, &named_head);
        let strong = sig.3.iter().any(|(_, net)| !net.is_empty());
        let candidates: Vec<usize> = gone_sigs
            .iter()
            .enumerate()
            .filter(|(_, g)| **g == sig)
            .map(|(i, _)| i)
            .collect();
        let pick = candidates
            .iter()
            .copied()
            .find(|&i| leaf(&gone[i].path) == leaf(&p.path))
            .or_else(|| {
                candidates
                    .iter()
                    .copied()
                    .find(|&i| gone[i].designator == p.designator)
            })
            .or_else(|| candidates.first().copied().filter(|_| strong));
        match pick {
            Some(i) => {
                gone_sigs.remove(i);
                pairs.push((gone.remove(i), p));
            }
            None => still_new.push(p),
        }
    }
    new = still_new;

    let mut changed = Vec::new();
    let mut renumbered = Vec::new();
    let mut unchanged = 0;
    let mut canon = HashMap::new();
    for (b, h) in &pairs {
        canon.insert(h.path.clone(), b.path.clone());
        let fields = part_fields(b, h);
        if fields.is_empty() {
            unchanged += 1;
            continue;
        }
        let change = ComparePartChange {
            path: h.path.clone(),
            base_path: (b.path != h.path).then(|| b.path.clone()),
            designator_base: b.designator.clone(),
            designator_head: h.designator.clone(),
            kind: h.kind.clone(),
            base_kind: (b.kind != h.kind).then(|| b.kind.clone()),
            at: h.at.clone(),
            base_at: b.at.clone(),
            fields,
        };
        if change.fields.iter().all(|f| f.field == "designator") {
            renumbered.push(change);
        } else {
            changed.push(change);
        }
    }
    changed.sort_by(|a, b| a.path.cmp(&b.path));
    renumbered.sort_by(|a, b| a.path.cmp(&b.path));
    let entry = |p: &PartDigest| ComparePart {
        path: p.path.clone(),
        designator: p.designator.clone(),
        kind: p.kind.clone(),
        value: p.value.clone(),
        package: p.package.clone(),
        lcsc: p.lcsc.clone(),
        mpn: p.mpn.clone(),
        assembly: p.assembly.clone(),
        at: p.at.clone(),
    };
    let mut added: Vec<ComparePart> = new.into_iter().map(entry).collect();
    let mut removed: Vec<ComparePart> = gone.into_iter().map(entry).collect();
    added.sort_by(|a, b| a.path.cmp(&b.path));
    removed.sort_by(|a, b| a.path.cmp(&b.path));
    (
        PartsDiff {
            added,
            removed,
            changed,
            renumbered,
            unchanged,
        },
        canon,
    )
}

fn part_fields(b: &PartDigest, h: &PartDigest) -> Vec<CompareField> {
    let mut fields = Vec::new();
    let mut field = |name: &str, old: Option<&String>, new: Option<&String>| {
        if old != new {
            fields.push(CompareField {
                field: name.to_string(),
                old: old.cloned(),
                new: new.cloned(),
            });
        }
    };
    field("path", Some(&b.path), Some(&h.path));
    field("kind", Some(&b.kind), Some(&h.kind));
    field("value", Some(&b.value), Some(&h.value));
    field("package", b.package.as_ref(), h.package.as_ref());
    field("lcsc", b.lcsc.as_ref(), h.lcsc.as_ref());
    field("mpn", b.mpn.as_ref(), h.mpn.as_ref());
    field("assembly", Some(&b.assembly), Some(&h.assembly));
    field("designator", Some(&b.designator), Some(&h.designator));
    fields
}

type Pins = BTreeSet<(String, String)>;

/// The nets, and which net of the ref is which on the head side by name, for the rails.
///
/// Pins are compared by the identity of their parts: the path at the ref, which a part that
/// moved keeps through `canon`; a part that only the head side has is its own.
fn nets(
    b: &BoardDigest,
    h: &BoardDigest,
    canon: &HashMap<String, String>,
) -> (NetsDiff, HashMap<String, String>) {
    let id_base = |path: &str| path.to_string();
    let id_head = |path: &str| {
        canon
            .get(path)
            .cloned()
            .unwrap_or_else(|| format!("\u{1}{path}"))
    };
    let pins_of = |n: &NetDigest, id: &dyn Fn(&str) -> String| -> Pins {
        n.pins
            .iter()
            .map(|(part, pin)| (id(part), pin.clone()))
            .collect()
    };
    let base_pins: Vec<Pins> = b.nets.iter().map(|n| pins_of(n, &id_base)).collect();
    let head_pins: Vec<Pins> = h.nets.iter().map(|n| pins_of(n, &id_head)).collect();
    let base_part: HashMap<&str, &PartDigest> =
        b.parts.iter().map(|p| (p.path.as_str(), p)).collect();
    let head_part: HashMap<&str, &PartDigest> =
        h.parts.iter().map(|p| (p.path.as_str(), p)).collect();

    let pin_of = |side: &HashMap<&str, &PartDigest>, path: &str, pin: &str| ComparePin {
        part: path.to_string(),
        designator: side
            .get(path)
            .map_or_else(String::new, |p| p.designator.clone()),
        pin: pin.to_string(),
        at: side.get(path).and_then(|p| p.at.clone()),
    };
    // The pins of net `i` on the head side / the ref's, in the order the design lists them.
    let head_pin_list = |n: &NetDigest| -> Vec<ComparePin> {
        n.pins
            .iter()
            .map(|(part, pin)| pin_of(&head_part, part, pin))
            .collect()
    };
    let base_pin_list = |n: &NetDigest| -> Vec<ComparePin> {
        n.pins
            .iter()
            .map(|(part, pin)| pin_of(&base_part, part, pin))
            .collect()
    };

    let mut pair: Vec<(usize, usize)> = Vec::new();
    let mut base_free: Vec<bool> = vec![true; b.nets.len()];
    let mut head_free: Vec<bool> = vec![true; h.nets.len()];
    // 1. By name.
    let by_name: HashMap<&str, usize> = b
        .nets
        .iter()
        .enumerate()
        .map(|(i, n)| (n.name.as_str(), i))
        .collect();
    for (j, n) in h.nets.iter().enumerate() {
        if let Some(&i) = by_name.get(n.name.as_str()) {
            pair.push((i, j));
            base_free[i] = false;
            head_free[j] = false;
        }
    }
    // 2. By the same pins.
    let mut by_pins: HashMap<&Pins, Vec<usize>> = HashMap::new();
    for (i, pins) in base_pins.iter().enumerate() {
        if base_free[i] && !pins.is_empty() {
            by_pins.entry(pins).or_default().push(i);
        }
    }
    for (j, pins) in head_pins.iter().enumerate() {
        if !head_free[j] {
            continue;
        }
        if let Some(list) = by_pins.get_mut(pins)
            && let Some(i) = list.first().copied()
        {
            list.remove(0);
            pair.push((i, j));
            base_free[i] = false;
            head_free[j] = false;
        }
    }
    // 3. By most of the pins: the pairs that share the most, each net once.
    let mut on_base: HashMap<(&str, &str), Vec<usize>> = HashMap::new();
    for (i, pins) in base_pins.iter().enumerate() {
        if base_free[i] {
            for (part, pin) in pins {
                on_base.entry((part, pin)).or_default().push(i);
            }
        }
    }
    let mut shared: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for (j, pins) in head_pins.iter().enumerate() {
        if head_free[j] {
            for (part, pin) in pins {
                for &i in on_base
                    .get(&(part.as_str(), pin.as_str()))
                    .into_iter()
                    .flatten()
                {
                    *shared.entry((i, j)).or_default() += 1;
                }
            }
        }
    }
    let mut ranked: Vec<((usize, usize), usize)> = shared.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for ((i, j), common) in ranked {
        if base_free[i] && head_free[j] && common * 2 >= base_pins[i].len().max(head_pins[j].len())
        {
            pair.push((i, j));
            base_free[i] = false;
            head_free[j] = false;
        }
    }

    let mut out = NetsDiff {
        added: Vec::new(),
        removed: Vec::new(),
        renamed: Vec::new(),
        changed: Vec::new(),
        unchanged: 0,
    };
    let mut renames = HashMap::new();
    pair.sort();
    for (i, j) in pair {
        let (bn, hn) = (&b.nets[i], &h.nets[j]);
        let same_name = bn.name == hn.name;
        if !same_name {
            renames.insert(bn.name.clone(), hn.name.clone());
        }
        let joined: Vec<ComparePin> = h.nets[j]
            .pins
            .iter()
            .filter(|(part, pin)| !base_pins[i].contains(&(id_head(part), pin.clone())))
            .map(|(part, pin)| pin_of(&head_part, part, pin))
            .collect();
        let left: Vec<ComparePin> = bn
            .pins
            .iter()
            .filter(|(part, pin)| !head_pins[j].contains(&(id_base(part), pin.clone())))
            .map(|(part, pin)| pin_of(&base_part, part, pin))
            .collect();
        let renamed = !same_name && (bn.named || hn.named);
        if same_name && joined.is_empty() && left.is_empty() {
            out.unchanged += 1;
            continue;
        }
        // Two names that netsmith made from the pins follow from the pins: nothing to say
        // when the pins are the same.
        if !same_name && !renamed && joined.is_empty() && left.is_empty() {
            out.unchanged += 1;
            continue;
        }
        let at = joined
            .first()
            .and_then(|p| p.at.clone())
            .or_else(|| left.first().and_then(|p| p.at.clone()));
        let change = CompareNetChange {
            name: hn.name.clone(),
            base_name: bn.name.clone(),
            renamed,
            joined,
            left,
            at,
        };
        if renamed {
            out.renamed.push(change);
        } else {
            out.changed.push(change);
        }
    }
    for (i, free) in base_free.iter().enumerate() {
        if *free {
            let n = &b.nets[i];
            let pins = base_pin_list(n);
            out.removed.push(CompareNet {
                name: n.name.clone(),
                named: n.named,
                at: pins.iter().find_map(|p| p.at.clone()),
                pins,
            });
        }
    }
    for (j, free) in head_free.iter().enumerate() {
        if *free {
            let n = &h.nets[j];
            let pins = head_pin_list(n);
            out.added.push(CompareNet {
                name: n.name.clone(),
                named: n.named,
                at: pins.iter().find_map(|p| p.at.clone()),
                pins,
            });
        }
    }
    out.added.sort_by(|a, b| a.name.cmp(&b.name));
    out.removed.sort_by(|a, b| a.name.cmp(&b.name));
    out.renamed.sort_by(|a, b| a.name.cmp(&b.name));
    out.changed.sort_by(|a, b| a.name.cmp(&b.name));
    (out, renames)
}

/// Two numbers that differ by no more than the arithmetic does.
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1e-12)
}

fn same_range(a: &Option<Range>, b: &Option<Range>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => close(a.lo, b.lo) && close(a.hi, b.hi),
        _ => false,
    }
}

/// What the supply can give above the largest the loads can draw.
fn headroom(node: &PowerNode) -> Option<f64> {
    match (&node.capacity, &node.demand) {
        // To the nanoampere: the subtraction's residue is not headroom.
        (Some(c), Some(d)) => Some(((c.hi - d.hi) * 1e9).round() / 1e9),
        _ => None,
    }
}

fn rail_entry(n: &PowerNode) -> CompareRail {
    CompareRail {
        net: n.net.clone(),
        voltage: n.voltage.clone(),
        demand: n.demand.clone(),
        capacity: n.capacity.clone(),
        budget: n.budget,
        at: n.at.clone(),
    }
}

fn rails_of(b: &[PowerNode], h: &[PowerNode], renames: &HashMap<String, String>) -> RailsDiff {
    let head: BTreeMap<&str, &PowerNode> = h.iter().map(|n| (n.net.as_str(), n)).collect();
    let mut seen = BTreeSet::new();
    let mut out = RailsDiff {
        added: Vec::new(),
        removed: Vec::new(),
        changed: Vec::new(),
        unchanged: 0,
    };
    for base in b {
        let name = renames.get(&base.net).unwrap_or(&base.net);
        let Some(&now) = head.get(name.as_str()) else {
            out.removed.push(rail_entry(base));
            continue;
        };
        seen.insert(now.net.as_str());
        let change = |a: &Option<Range>, b: &Option<Range>| {
            (!same_range(a, b)).then(|| RangeChange {
                base: a.clone(),
                head: b.clone(),
            })
        };
        let (voltage, demand, capacity) = (
            change(&base.voltage, &now.voltage),
            change(&base.demand, &now.demand),
            change(&base.capacity, &now.capacity),
        );
        let budget = (base.budget != now.budget).then_some(BudgetChange {
            base: base.budget,
            head: now.budget,
        });
        if voltage.is_none()
            && demand.is_none()
            && capacity.is_none()
            && budget.is_none()
            && base.net == now.net
        {
            out.unchanged += 1;
            continue;
        }
        if voltage.is_none() && demand.is_none() && capacity.is_none() && budget.is_none() {
            // Only the name differs; the net diff says so.
            out.unchanged += 1;
            continue;
        }
        out.changed.push(CompareRailChange {
            net: now.net.clone(),
            base_net: (base.net != now.net).then(|| base.net.clone()),
            voltage,
            demand,
            capacity,
            budget,
            headroom_base: headroom(base),
            headroom_head: headroom(now),
            at: now.at.clone(),
            base_at: base.at.clone(),
        });
    }
    for now in h {
        if !seen.contains(now.net.as_str()) {
            out.added.push(rail_entry(now));
        }
    }
    out.added.sort_by(|a, b| a.net.cmp(&b.net));
    out.removed.sort_by(|a, b| a.net.cmp(&b.net));
    out.changed.sort_by(|a, b| a.net.cmp(&b.net));
    out
}

fn instances(b: &[InstanceDigest], h: &[InstanceDigest]) -> InstancesDiff {
    let base: BTreeMap<&str, &InstanceDigest> = b.iter().map(|i| (i.path.as_str(), i)).collect();
    let head: BTreeMap<&str, &InstanceDigest> = h.iter().map(|i| (i.path.as_str(), i)).collect();
    let entry = |i: &InstanceDigest| CompareInstance {
        path: i.path.clone(),
        type_name: i.type_name.clone(),
        at: i.at.clone(),
    };
    let mut out = InstancesDiff {
        added: Vec::new(),
        removed: Vec::new(),
        changed: Vec::new(),
    };
    for (path, i) in &head {
        match base.get(path) {
            None => out.added.push(entry(i)),
            Some(b) if b.type_name != i.type_name => out.changed.push(CompareInstanceChange {
                path: i.path.clone(),
                base_type: b.type_name.clone(),
                head_type: i.type_name.clone(),
                at: i.at.clone(),
            }),
            Some(_) => {}
        }
    }
    for (path, i) in &base {
        if !head.contains_key(path) {
            out.removed.push(entry(i));
        }
    }
    out
}

type BomKey = (String, String, Option<String>, Option<String>, bool);

/// What makes a bill of materials line the same on both sides: what the bill itself groups parts
/// by (`sourcing.rs`: value, footprint, LCSC number, kind, hand soldering). The manufacturer's
/// number is not in it: two parts of one group may differ in it, and the line shows the first
/// one's, so a key with it would make a changed number a line removed and a line added. A line
/// whose number changed is a changed line.
fn bom_key(l: &BomLine) -> BomKey {
    (
        l.kind.clone(),
        l.value.clone(),
        l.footprint.clone().or_else(|| l.package.clone()),
        l.lcsc.clone(),
        l.hand_solder,
    )
}

fn bom_line(l: &BomLine) -> CompareBomLine {
    CompareBomLine {
        kind: l.kind.clone(),
        value: l.value.clone(),
        package: l.package.clone(),
        footprint: l.footprint.clone(),
        lcsc: l.lcsc.clone(),
        mpn: l.mpn.clone(),
        hand_solder: l.hand_solder,
        quantity: l.quantity,
        designators: l.designators.clone(),
        unit_usd: l.unit_usd,
    }
}

fn bom(b: &Bom, h: &Bom) -> BomDiff {
    let lines = |bom: &Bom| -> BTreeMap<BomKey, BomLine> {
        bom.lines
            .iter()
            .chain(&bom.hand)
            .map(|l| (bom_key(l), l.clone()))
            .collect()
    };
    let (base, head) = (lines(b), lines(h));
    let mut out = BomDiff {
        added: Vec::new(),
        removed: Vec::new(),
        changed: Vec::new(),
        unchanged: 0,
        total_usd_base: b.total_usd,
        total_usd_head: h.total_usd,
        delta_usd: match (b.total_usd, h.total_usd) {
            (Some(a), Some(b)) => Some(b - a),
            _ => None,
        },
    };
    for (key, now) in &head {
        match base.get(key) {
            None => out.added.push(bom_line(now)),
            Some(was) => {
                let price = match (was.unit_usd, now.unit_usd) {
                    (Some(a), Some(b)) => !close(a, b),
                    (a, b) => a.is_some() != b.is_some(),
                };
                if was.quantity == now.quantity && !price && was.mpn == now.mpn {
                    out.unchanged += 1;
                    continue;
                }
                out.changed.push(CompareBomChange {
                    kind: now.kind.clone(),
                    value: now.value.clone(),
                    package: now.package.clone(),
                    footprint: now.footprint.clone(),
                    lcsc: now.lcsc.clone(),
                    mpn: now.mpn.clone(),
                    hand_solder: now.hand_solder,
                    quantity_base: was.quantity,
                    quantity_head: now.quantity,
                    designators_base: was.designators.clone(),
                    designators_head: now.designators.clone(),
                    unit_usd_base: was.unit_usd,
                    unit_usd_head: now.unit_usd,
                });
            }
        }
    }
    for (key, was) in &base {
        if !head.contains_key(key) {
            out.removed.push(bom_line(was));
        }
    }
    out
}

// ---- the PCB ----

/// Millimetres the layout file does not tell apart: it keeps the hundredth.
const PCB_EPS: f64 = 0.006;

/// Millimetres as a person writes them: `12.5`, `3`, `0.25`.
fn mm(v: f64) -> String {
    let text = format!("{v:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Degrees between two angles, whichever way round is shorter.
fn turn_between(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

fn same_box(a: &Option<PcbBox>, b: &Option<PcbBox>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            (a.x - b.x).abs() < PCB_EPS
                && (a.y - b.y).abs() < PCB_EPS
                && (a.width - b.width).abs() < PCB_EPS
                && (a.height - b.height).abs() < PCB_EPS
        }
        _ => false,
    }
}

fn side_word(s: PcbSide) -> &'static str {
    match s {
        PcbSide::Top => "top",
        PcbSide::Bottom => "bottom",
    }
}

/// The PCBs of a board on the two sides. `canon` maps a part's path on the head side to its path
/// at the ref (a part that moved in the source keeps its place), as the part diff made it.
fn pcb(
    b: &BoardDigest,
    h: &BoardDigest,
    canon: &HashMap<String, String>,
) -> (Option<PcbDiff>, Option<String>) {
    match (&b.pcb, &h.pcb) {
        (Some(Ok(base)), Some(Ok(head))) => (Some(pcb_diff(base, head, canon)), None),
        (Some(Ok(_)), Some(Err(e))) => (
            None,
            Some(format!(
                "the PCB cannot be placed on the head side, so it is not compared: {e}"
            )),
        ),
        (Some(Err(e)), Some(Ok(_))) => (
            None,
            Some(format!(
                "the PCB cannot be placed at the ref, so it is not compared: {e}"
            )),
        ),
        // Not placed on a side (a board that is only on the other), or on neither (it has no
        // process): nothing to say.
        _ => (None, None),
    }
}

fn pcb_diff(b: &PcbDigest, h: &PcbDigest, canon: &HashMap<String, String>) -> PcbDiff {
    let base: HashMap<&str, &PcbPartDigest> =
        b.parts.iter().map(|p| (p.path.as_str(), p)).collect();
    let mut moved = Vec::new();
    let mut unchanged = 0;
    for hp in &h.parts {
        let was = canon.get(&hp.path).unwrap_or(&hp.path);
        let Some(bp) = base.get(was.as_str()) else {
            continue;
        };
        let (old, new) = (&bp.place, &hp.place);
        // The placer's own choice on both sides moves with every part added: not a statement.
        if old.placement == "derived" && new.placement == "derived" {
            unchanged += 1;
            continue;
        }
        let distance = (new.x - old.x).hypot(new.y - old.y);
        let mut fields = Vec::new();
        if distance >= PCB_EPS {
            fields.push(CompareField {
                field: "position".into(),
                old: Some(format!("{}, {} mm", mm(old.x), mm(old.y))),
                new: Some(format!("{}, {} mm", mm(new.x), mm(new.y))),
            });
        }
        if turn_between(old.rotation, new.rotation) >= PCB_EPS {
            fields.push(CompareField {
                field: "rotation".into(),
                old: Some(format!("{}°", mm(old.rotation))),
                new: Some(format!("{}°", mm(new.rotation))),
            });
        }
        if old.side != new.side {
            fields.push(CompareField {
                field: "side".into(),
                old: Some(side_word(old.side).into()),
                new: Some(side_word(new.side).into()),
            });
        }
        if old.locked != new.locked {
            fields.push(CompareField {
                field: "locked".into(),
                old: Some(old.locked.to_string()),
                new: Some(new.locked.to_string()),
            });
        }
        if fields.is_empty() {
            unchanged += 1;
            continue;
        }
        moved.push(ComparePcbPartChange {
            path: hp.path.clone(),
            base_path: (bp.path != hp.path).then(|| bp.path.clone()),
            designator_base: bp.designator.clone(),
            designator_head: hp.designator.clone(),
            base: old.clone(),
            head: new.clone(),
            fields,
            distance_mm: if distance >= PCB_EPS { distance } else { 0.0 },
        });
    }
    moved.sort_by(|a, b| a.path.cmp(&b.path));
    let outline = ((b.outline_stated || h.outline_stated)
        && ((b.outline.width - h.outline.width).abs() >= PCB_EPS
            || (b.outline.height - h.outline.height).abs() >= PCB_EPS))
        .then_some(OutlineChange {
            base: b.outline,
            head: h.outline,
        });
    let layers = (b.layers != h.layers).then(|| LayersChange {
        base: b.layers.clone(),
        head: h.layers.clone(),
    });
    PcbDiff {
        layout_file: h.layout_file.clone(),
        outline,
        layers,
        moved,
        keepouts: areas(&b.keepouts, &h.keepouts),
        cutouts: areas(&b.cutouts, &h.cutouts),
        unchanged,
    }
}

fn areas(b: &[(String, Option<PcbBox>)], h: &[(String, Option<PcbBox>)]) -> Vec<ComparePcbArea> {
    let base: BTreeMap<&str, &Option<PcbBox>> = b.iter().map(|(n, a)| (n.as_str(), a)).collect();
    let head: BTreeMap<&str, &Option<PcbBox>> = h.iter().map(|(n, a)| (n.as_str(), a)).collect();
    let mut out = Vec::new();
    for (name, now) in &head {
        match base.get(name) {
            None => out.push(ComparePcbArea {
                name: name.to_string(),
                status: BoardStatus::Added,
                base: None,
                head: **now,
            }),
            Some(was) if !same_box(was, now) => out.push(ComparePcbArea {
                name: name.to_string(),
                status: BoardStatus::Changed,
                base: **was,
                head: **now,
            }),
            Some(_) => {}
        }
    }
    for (name, was) in &base {
        if !head.contains_key(name) {
            out.push(ComparePcbArea {
                name: name.to_string(),
                status: BoardStatus::Removed,
                base: **was,
                head: None,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

// ---- the systems ----

fn names(b: &BTreeSet<String>, h: &BTreeSet<String>) -> NamesDiff {
    NamesDiff {
        added: h.difference(b).cloned().collect(),
        removed: b.difference(h).cloned().collect(),
    }
}

fn system_ports(v: &SystemView) -> BTreeMap<(String, String), CompareSystemPort> {
    v.nodes
        .iter()
        .filter(|n| n.kind == "board")
        .flat_map(|n| {
            n.ports.iter().map(|p| {
                (
                    (n.name.clone(), p.name.clone()),
                    CompareSystemPort {
                        board: n.name.clone(),
                        name: p.name.clone(),
                        voltage: p.voltage.clone(),
                        current: p.current.clone(),
                    },
                )
            })
        })
        .collect()
}

fn mates(b: &SystemView, h: &SystemView) -> MatesDiff {
    let key = |m: &netsmith_editor::api::MateView| (m.a.clone(), m.b.clone());
    let base: BTreeMap<_, _> = b.mates.iter().map(|m| (key(m), m)).collect();
    let head: BTreeMap<_, _> = h.mates.iter().map(|m| (key(m), m)).collect();
    let entry = |m: &netsmith_editor::api::MateView| CompareMate {
        a: m.a.clone(),
        b: m.b.clone(),
        ports: m.ports.clone(),
        at: m.at.clone(),
    };
    let mut out = MatesDiff {
        added: Vec::new(),
        removed: Vec::new(),
        changed: Vec::new(),
    };
    for (k, now) in &head {
        let Some(was) = base.get(k) else {
            out.added.push(entry(now));
            continue;
        };
        let mut fields = Vec::new();
        let contacts = |m: &netsmith_editor::api::MateView| -> BTreeMap<u32, String> {
            m.contacts
                .iter()
                .map(|c| {
                    (
                        c.number,
                        // The roles only: what a board calls the net behind a contact is the
                        // board's, and a rename is said in its nets.
                        format!("{} to {}", c.a_role, c.b_role),
                    )
                })
                .collect()
        };
        let (cb, ch) = (contacts(was), contacts(now));
        let numbers: BTreeSet<u32> = cb.keys().chain(ch.keys()).copied().collect();
        for n in numbers {
            if cb.get(&n) != ch.get(&n) {
                fields.push(CompareField {
                    field: format!("contact {n}"),
                    old: cb.get(&n).cloned(),
                    new: ch.get(&n).cloned(),
                });
            }
        }
        if was.ports != now.ports {
            fields.push(CompareField {
                field: "ports".into(),
                old: Some(was.ports.join(", ")),
                new: Some(now.ports.join(", ")),
            });
        }
        if !fields.is_empty() {
            out.changed.push(CompareMateChange {
                a: now.a.clone(),
                b: now.b.clone(),
                fields,
                at: now.at.clone(),
            });
        }
    }
    for (k, was) in &base {
        if !head.contains_key(k) {
            out.removed.push(entry(was));
        }
    }
    out
}

fn system_diff(
    name: &str,
    base: Option<&SystemDigest>,
    head: Option<&SystemDigest>,
    known_rails: &HashSet<(String, String)>,
) -> CompareSystem {
    let empty = SystemDigest {
        file: String::new(),
        view: SystemView {
            name: name.to_string(),
            kind: "system".into(),
            nodes: Vec::new(),
            edges: Vec::new(),
            mates: Vec::new(),
            ground_groups: Vec::new(),
        },
        rails: Vec::new(),
    };
    let (b, h) = (base.unwrap_or(&empty), head.unwrap_or(&empty));
    let boards = |d: &SystemDigest| -> Vec<InstanceDigest> {
        d.view
            .nodes
            .iter()
            .filter(|n| n.kind == "board")
            .map(|n| InstanceDigest {
                path: n.name.clone(),
                type_name: n.type_name.clone(),
                at: n.at.clone(),
            })
            .collect()
    };
    let boards = instances(&boards(b), &boards(h));
    let (pb, ph) = (system_ports(&b.view), system_ports(&h.view));
    let mut ports = SystemPortsDiff {
        added: Vec::new(),
        removed: Vec::new(),
        changed: Vec::new(),
    };
    for (k, now) in &ph {
        match pb.get(k) {
            None => ports.added.push(now.clone()),
            Some(was) => {
                let mut fields = Vec::new();
                if was.voltage != now.voltage {
                    fields.push(CompareField {
                        field: "voltage".into(),
                        old: was.voltage.clone(),
                        new: now.voltage.clone(),
                    });
                }
                if was.current != now.current {
                    fields.push(CompareField {
                        field: "current".into(),
                        old: was.current.clone(),
                        new: now.current.clone(),
                    });
                }
                if !fields.is_empty() {
                    ports.changed.push(CompareSystemPortChange {
                        board: now.board.clone(),
                        name: now.name.clone(),
                        fields,
                    });
                }
            }
        }
    }
    for (k, was) in &pb {
        if !ph.contains_key(k) {
            ports.removed.push(was.clone());
        }
    }
    let joins = |v: &SystemView| -> BTreeSet<String> {
        v.edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Join)
            .map(|e| e.label.clone())
            .collect()
    };
    let grounds = |v: &SystemView| -> BTreeSet<String> {
        v.ground_groups.iter().map(|g| g.nets.join(" + ")).collect()
    };
    let mut rails = rails_of(&b.rails, &h.rails, &HashMap::new());
    // What a board's own comparison says of its rail is not said again here: this lists what only
    // the system's joins explain.
    let listed = |net: &str| {
        net.split_once(':').is_some_and(|(member, n)| {
            known_rails.contains(&(format!("{name}/{member}"), n.to_string()))
        })
    };
    rails.added.retain(|r| !listed(&r.net));
    rails.removed.retain(|r| !listed(&r.net));
    rails
        .changed
        .retain(|r| !listed(&r.net) && !r.base_net.as_deref().is_some_and(listed));
    let mut out = CompareSystem {
        name: name.to_string(),
        status: BoardStatus::Unchanged,
        base_file: base.map(|b| b.file.clone()),
        head_file: head.map(|h| h.file.clone()),
        summary: String::new(),
        boards,
        ports,
        joins: names(&joins(&b.view), &joins(&h.view)),
        mates: mates(&b.view, &h.view),
        ground_groups: names(&grounds(&b.view), &grounds(&h.view)),
        rails,
    };
    out.status = match (base, head) {
        (None, _) => BoardStatus::Added,
        (_, None) => BoardStatus::Removed,
        _ if system_changes(&out) > 0 => BoardStatus::Changed,
        _ => BoardStatus::Unchanged,
    };
    out.summary = summarize(&[], std::slice::from_ref(&out), None);
    out
}

/// How many things differ in a system on both sides.
pub fn system_changes(s: &CompareSystem) -> usize {
    s.boards.added.len()
        + s.boards.removed.len()
        + s.boards.changed.len()
        + s.ports.added.len()
        + s.ports.removed.len()
        + s.ports.changed.len()
        + s.joins.added.len()
        + s.joins.removed.len()
        + s.mates.added.len()
        + s.mates.removed.len()
        + s.mates.changed.len()
        + s.ground_groups.added.len()
        + s.ground_groups.removed.len()
        + s.rails.added.len()
        + s.rails.removed.len()
        + s.rails.changed.len()
}

fn systems(base: &Digest, head: &Digest, boards: &[CompareBoard]) -> Vec<CompareSystem> {
    // The rails the boards' own comparisons list, by the board's name and the net's.
    let mut known: HashSet<(String, String)> = HashSet::new();
    for b in boards {
        for r in b.rails.added.iter().chain(&b.rails.removed) {
            known.insert((b.name.clone(), r.net.clone()));
        }
        for r in &b.rails.changed {
            known.insert((b.name.clone(), r.net.clone()));
            if let Some(n) = &r.base_net {
                known.insert((b.name.clone(), n.clone()));
            }
        }
        // A rail whose net was renamed and nothing else is not in the rails: it is in the nets.
        for n in &b.nets.renamed {
            known.insert((b.name.clone(), n.name.clone()));
            known.insert((b.name.clone(), n.base_name.clone()));
        }
    }
    let names: BTreeSet<&String> = base.systems.keys().chain(head.systems.keys()).collect();
    names
        .into_iter()
        .map(|n| system_diff(n, base.systems.get(n), head.systems.get(n), &known))
        .collect()
}

fn rank(severity: Severity) -> u8 {
    match severity {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Note => 2,
    }
}

/// What makes a problem the same on both sides.
fn problem_key(p: &Problem) -> (String, String, Option<String>, Option<String>, String) {
    (
        format!("{:?}", p.severity),
        format!("{:?}", p.stage),
        p.board.clone(),
        p.path.clone(),
        p.message.clone(),
    )
}

fn problems(base: Vec<Problem>, head: Vec<Problem>) -> ProblemsDiff {
    let mut was: HashMap<_, Vec<Problem>> = HashMap::new();
    for p in base {
        was.entry(problem_key(&p)).or_default().push(p);
    }
    let mut new = Vec::new();
    let mut unchanged = 0;
    for p in head {
        match was.get_mut(&problem_key(&p)) {
            Some(list) if !list.is_empty() => {
                list.pop();
                unchanged += 1;
            }
            _ => new.push(p),
        }
    }
    let mut resolved: Vec<Problem> = was.into_values().flatten().collect();
    let order = |p: &Problem| {
        (
            rank(p.severity),
            p.board.clone(),
            p.message.clone(),
            p.at.as_ref().map(|l| (l.file.clone(), l.line)),
        )
    };
    new.sort_by_key(order);
    resolved.sort_by_key(order);
    ProblemsDiff {
        new,
        resolved,
        unchanged,
    }
}

/// `error`, `warnings`: the word for a severity, in the plural for any number but one.
fn severity_word(severity: Severity, n: usize) -> &'static str {
    match (severity, n == 1) {
        (Severity::Error, true) => "error",
        (Severity::Error, false) => "errors",
        (Severity::Warning, true) => "warning",
        (Severity::Warning, false) => "warnings",
        (Severity::Note, true) => "note",
        (Severity::Note, false) => "notes",
    }
}

pub fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Amperes as a person writes them: `80 mA`, `1.5 A`.
pub fn amps(a: f64) -> String {
    let (value, unit) = if a.abs() >= 1.0 {
        (a, "A")
    } else if a.abs() >= 1e-3 {
        (a * 1e3, "mA")
    } else {
        (a * 1e6, "µA")
    };
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text} {unit}")
}

/// One line for the boards: `3 parts added, 1 removed, 2 values changed, 1 rail's headroom
/// shrank from 80 mA to 20 mA`. `project` is the problems that name no board.
fn summarize(
    boards: &[CompareBoard],
    systems: &[CompareSystem],
    project: Option<&ProblemsDiff>,
) -> String {
    let mut pieces: Vec<String> = Vec::new();
    let added_boards: Vec<&str> = boards
        .iter()
        .filter(|b| b.status == BoardStatus::Added)
        .map(|b| b.name.as_str())
        .collect();
    let removed_boards: Vec<&str> = boards
        .iter()
        .filter(|b| b.status == BoardStatus::Removed)
        .map(|b| b.name.as_str())
        .collect();
    let list = |names: &[&str], verb: &str| match names {
        [one] => format!("board {one} {verb}"),
        many => format!("{} {verb}", count(many.len(), "board", "boards")),
    };
    if !added_boards.is_empty() {
        pieces.push(list(&added_boards, "added"));
    }
    if !removed_boards.is_empty() {
        pieces.push(list(&removed_boards, "removed"));
    }
    // The boards on both sides: their parts and nets are what changed; those of a board that
    // is added or removed are all of it, and the sentence has said so.
    let both: Vec<&CompareBoard> = boards
        .iter()
        .filter(|b| !matches!(b.status, BoardStatus::Added | BoardStatus::Removed))
        .collect();
    let sum = |f: &dyn Fn(&CompareBoard) -> usize| both.iter().map(|b| f(b)).sum::<usize>();
    let parts_added = sum(&|b| b.parts.added.len());
    let parts_removed = sum(&|b| b.parts.removed.len());
    let mut values = 0;
    let mut moved = 0;
    let mut others = 0;
    // A part that is another library part (`Resistor` to `Capacitor`) is counted for that, and not
    // again for the value that came with it.
    let mut kinds: Vec<(&str, &str)> = Vec::new();
    for b in &both {
        for c in &b.parts.changed {
            let only = |name: &str| {
                c.fields
                    .iter()
                    .filter(|f| f.field != "designator")
                    .all(|f| f.field == name)
            };
            if let Some(was) = &c.base_kind {
                kinds.push((was, &c.kind));
            } else if c.fields.iter().any(|f| f.field == "value") {
                values += 1;
            } else if only("path") {
                moved += 1;
            } else {
                others += 1;
            }
        }
    }
    let renumbered = sum(&|b| b.parts.renumbered.len());
    if parts_added > 0 {
        pieces.push(format!("{} added", count(parts_added, "part", "parts")));
    }
    if parts_removed > 0 {
        // `3 parts added, 1 removed`: the noun is said once.
        pieces.push(if parts_added > 0 {
            format!("{parts_removed} removed")
        } else {
            format!("{} removed", count(parts_removed, "part", "parts"))
        });
    }
    match kinds.as_slice() {
        [] => {}
        [(was, now)] => pieces.push(format!("1 part's kind changed from {was} to {now}")),
        many => pieces.push(format!("{} parts' kinds changed", many.len())),
    }
    if values > 0 {
        pieces.push(format!("{} changed", count(values, "value", "values")));
    }
    if others > 0 {
        pieces.push(format!("{} changed", count(others, "part", "parts")));
    }
    if moved > 0 {
        pieces.push(format!("{} moved", count(moved, "part", "parts")));
    }
    let nets = |f: &dyn Fn(&NetsDiff) -> usize| both.iter().map(|b| f(&b.nets)).sum::<usize>();
    for (n, word) in [
        (nets(&|n| n.added.len()), "added"),
        (nets(&|n| n.removed.len()), "removed"),
        (nets(&|n| n.renamed.len()), "renamed"),
        (nets(&|n| n.changed.len()), "changed"),
    ] {
        if n > 0 {
            pieces.push(format!("{} {word}", count(n, "net", "nets")));
        }
    }
    let modules = |word: &str, n: usize, one: &str, many: &str| {
        (n > 0).then(|| format!("{} {word}", count(n, one, many)))
    };
    pieces.extend(modules(
        "added",
        sum(&|b| b.modules.added.len()),
        "module",
        "modules",
    ));
    pieces.extend(modules(
        "removed",
        sum(&|b| b.modules.removed.len()),
        "module",
        "modules",
    ));
    pieces.extend(modules(
        "added",
        sum(&|b| b.interfaces.added.len()),
        "port",
        "ports",
    ));
    pieces.extend(modules(
        "removed",
        sum(&|b| b.interfaces.removed.len()),
        "port",
        "ports",
    ));
    summarize_systems(systems, &mut pieces);
    summarize_pcb(&both, &mut pieces);
    // Rails: the headroom first, as it is what the owner looks for.
    let mut shrank = Vec::new();
    let mut grew = 0;
    let mut other_rails = 0;
    for b in &both {
        for r in &b.rails.changed {
            match (r.headroom_base, r.headroom_head) {
                (Some(a), Some(c)) if c < a && !close(a, c) => shrank.push((a, c)),
                (Some(a), Some(c)) if c > a && !close(a, c) => grew += 1,
                _ => other_rails += 1,
            }
        }
    }
    match shrank.as_slice() {
        [] => {}
        [(a, c)] => pieces.push(format!(
            "1 rail's headroom shrank from {} to {}",
            amps(*a),
            amps(*c)
        )),
        many => pieces.push(format!("{} rails' headroom shrank", many.len())),
    }
    if grew > 0 {
        pieces.push(if grew == 1 {
            "1 rail's headroom grew".to_string()
        } else {
            format!("{grew} rails' headroom grew")
        });
    }
    if other_rails > 0 {
        pieces.push(format!("{} changed", count(other_rails, "rail", "rails")));
    }
    for (n, word) in [
        (sum(&|b| b.rails.added.len()), "added"),
        (sum(&|b| b.rails.removed.len()), "removed"),
    ] {
        if n > 0 {
            pieces.push(format!("{} {word}", count(n, "rail", "rails")));
        }
    }
    let delta: f64 = both
        .iter()
        .filter_map(|b| b.bom.as_ref().and_then(|b| b.delta_usd))
        .sum();
    if delta.abs() > 1e-9 {
        pieces.push(format!(
            "BOM {}${:.4} per board",
            if delta < 0.0 { "-" } else { "+" },
            delta.abs()
        ));
    }
    // What the checker said that it did not say before, and what it no longer says, by how serious
    // it is: errors, warnings (a part not chosen, a guideline) and notes (what the solver left
    // alone). A note is last in the sentence, as it is the least to act on.
    let own = |severity: Severity, resolved: bool| -> usize {
        let list = |p: &ProblemsDiff| {
            let list = if resolved { &p.resolved } else { &p.new };
            list.iter().filter(|p| p.severity == severity).count()
        };
        let mut n = both.iter().map(|b| list(&b.problems)).sum::<usize>();
        if let Some(p) = project {
            n += list(p);
        }
        n
    };
    for severity in [Severity::Error, Severity::Warning, Severity::Note] {
        let n = own(severity, false);
        if n > 0 {
            pieces.push(format!("{n} new {}", severity_word(severity, n)));
        }
    }
    for severity in [Severity::Error, Severity::Warning, Severity::Note] {
        let n = own(severity, true);
        if n > 0 {
            pieces.push(format!("{n} {} resolved", severity_word(severity, n)));
        }
    }
    if pieces.is_empty() {
        return if renumbered > 0 {
            format!(
                "No circuit changes ({} renumbered)",
                count(renumbered, "part", "parts")
            )
        } else {
            "No circuit changes".to_string()
        };
    }
    pieces.join(", ")
}

/// `2 system ports changed`, `1 system added`: the systems' part of the sentence.
fn summarize_systems(systems: &[CompareSystem], pieces: &mut Vec<String>) {
    let status = |s: BoardStatus| -> Vec<&str> {
        systems
            .iter()
            .filter(|x| x.status == s)
            .map(|x| x.name.as_str())
            .collect()
    };
    for (names, verb) in [
        (status(BoardStatus::Added), "added"),
        (status(BoardStatus::Removed), "removed"),
    ] {
        match names.as_slice() {
            [] => {}
            [one] => pieces.push(format!("system {one} {verb}")),
            many => pieces.push(format!("{} {verb}", count(many.len(), "system", "systems"))),
        }
    }
    let both: Vec<&CompareSystem> = systems
        .iter()
        .filter(|s| !matches!(s.status, BoardStatus::Added | BoardStatus::Removed))
        .collect();
    let sum = |f: &dyn Fn(&CompareSystem) -> usize| both.iter().map(|s| f(s)).sum::<usize>();
    for (n, what) in [
        (sum(&|s| s.boards.added.len()), "system boards added"),
        (sum(&|s| s.boards.removed.len()), "system boards removed"),
        (sum(&|s| s.boards.changed.len()), "system boards changed"),
    ] {
        if n > 0 {
            pieces.push(format!("{n} {what}"));
        }
    }
    let one_or_many =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let ports = sum(&|s| s.ports.added.len() + s.ports.removed.len() + s.ports.changed.len());
    if ports > 0 {
        pieces.push(one_or_many(
            ports,
            "system port changed",
            "system ports changed",
        ));
    }
    let joins = sum(&|s| s.joins.added.len() + s.joins.removed.len());
    if joins > 0 {
        pieces.push(one_or_many(
            joins,
            "system join changed",
            "system joins changed",
        ));
    }
    let mates = sum(&|s| s.mates.added.len() + s.mates.removed.len() + s.mates.changed.len());
    if mates > 0 {
        pieces.push(one_or_many(mates, "mate changed", "mates changed"));
    }
    let grounds = sum(&|s| s.ground_groups.added.len() + s.ground_groups.removed.len());
    if grounds > 0 {
        pieces.push(one_or_many(
            grounds,
            "ground group changed",
            "ground groups changed",
        ));
    }
    let rails = sum(&|s| s.rails.added.len() + s.rails.removed.len() + s.rails.changed.len());
    if rails > 0 {
        pieces.push(one_or_many(
            rails,
            "system rail changed",
            "system rails changed",
        ));
    }
}

/// `2 parts moved on the PCB`, `outline 30 × 20 → 32 × 20 mm`, `layers 2 → 4`: the PCB's part.
fn summarize_pcb(boards: &[&CompareBoard], pieces: &mut Vec<String>) {
    let pcbs: Vec<&PcbDiff> = boards.iter().filter_map(|b| b.pcb.as_ref()).collect();
    let moved: usize = pcbs.iter().map(|p| p.moved.len()).sum();
    if moved > 0 {
        pieces.push(format!(
            "{} moved on the PCB",
            count(moved, "part", "parts")
        ));
    }
    let outlines: Vec<&OutlineChange> = pcbs.iter().filter_map(|p| p.outline.as_ref()).collect();
    match outlines.as_slice() {
        [] => {}
        [o] => pieces.push(format!(
            "outline {} × {} → {} × {} mm",
            mm(o.base.width),
            mm(o.base.height),
            mm(o.head.width),
            mm(o.head.height)
        )),
        many => pieces.push(format!("{} outlines changed", many.len())),
    }
    let layers: Vec<&LayersChange> = pcbs.iter().filter_map(|p| p.layers.as_ref()).collect();
    match layers.as_slice() {
        [] => {}
        [l] => pieces.push(format!("layers {} → {}", l.base.len(), l.head.len())),
        many => pieces.push(format!("{} layer stacks changed", many.len())),
    }
    let keepouts: usize = pcbs.iter().map(|p| p.keepouts.len()).sum();
    if keepouts > 0 {
        pieces.push(format!(
            "{} changed",
            count(keepouts, "keepout", "keepouts")
        ));
    }
    let cutouts: usize = pcbs.iter().map(|p| p.cutouts.len()).sum();
    if cutouts > 0 {
        pieces.push(format!("{} changed", count(cutouts, "cutout", "cutouts")));
    }
}

#[cfg(test)]
mod tests {
    use netsmith_editor::api::{Budget, ProblemStage};

    use super::*;

    fn part(path: &str, designator: &str, value: &str, pins: &[(&str, &str)]) -> PartDigest {
        PartDigest {
            path: path.into(),
            designator: designator.into(),
            kind: "Resistor".into(),
            value: value.into(),
            package: Some("0402".into()),
            lcsc: Some("C1".into()),
            mpn: None,
            assembly: "fab".into(),
            at: None,
            pins: pins
                .iter()
                .map(|(p, n)| (p.to_string(), n.to_string()))
                .collect(),
        }
    }

    fn net(name: &str, pins: &[(&str, &str)]) -> NetDigest {
        NetDigest {
            name: name.into(),
            named: !name.starts_with("net-"),
            pins: pins
                .iter()
                .map(|(p, n)| (p.to_string(), n.to_string()))
                .collect(),
        }
    }

    fn board(parts: Vec<PartDigest>, nets: Vec<NetDigest>) -> BoardDigest {
        BoardDigest {
            file: "src/main.nsm".into(),
            parts,
            nets,
            ..BoardDigest::nothing()
        }
    }

    fn digest(b: BoardDigest) -> Digest {
        Digest {
            boards: BTreeMap::from([("B".to_string(), b)]),
            systems: BTreeMap::new(),
            problems: Vec::new(),
        }
    }

    fn range(lo: f64, hi: f64) -> Range {
        Range {
            lo,
            hi,
            text: format!("{lo}..{hi}"),
        }
    }

    fn rail(net: &str, demand: f64, capacity: Option<f64>) -> PowerNode {
        PowerNode {
            id: net.into(),
            net: net.into(),
            board: None,
            voltage: Some(range(3.135, 3.465)),
            loads: Vec::new(),
            supplies: Vec::new(),
            demand: Some(range(0.0, demand)),
            capacity: capacity.map(|c| range(0.0, c)),
            capacity_from: None,
            budget: match capacity {
                Some(c) if demand > c => Budget::Over,
                Some(_) => Budget::Within,
                None => Budget::Unlimited,
            },
            at: None,
        }
    }

    fn problem(severity: Severity, board: Option<&str>, message: &str) -> Problem {
        Problem {
            severity,
            message: message.into(),
            rendered: message.into(),
            at: None,
            stage: Some(ProblemStage::Solve),
            board: board.map(str::to_string),
            path: None,
            cause: None,
        }
    }

    fn only(d: &Diff) -> &CompareBoard {
        assert_eq!(d.boards.len(), 1);
        &d.boards[0]
    }

    #[test]
    fn a_net_with_most_of_its_pins_under_another_name_is_renamed_and_changed() {
        let pins = |n: usize| -> Vec<(&'static str, &'static str)> {
            ["a", "b", "c", "d"][..n]
                .iter()
                .map(|p| (*p, "1"))
                .collect()
        };
        let parts = |names: &[&str]| -> Vec<PartDigest> {
            names
                .iter()
                .enumerate()
                .map(|(i, p)| part(p, &format!("R{}", i + 1), "1k", &[]))
                .collect()
        };
        let base = digest(board(
            parts(&["a", "b", "c", "d", "e"]),
            vec![net("SIG", &pins(4))],
        ));
        // Three of four pins and a new one: the same net, renamed and changed.
        let mut head_pins = pins(3);
        head_pins.push(("e", "1"));
        let head = digest(board(
            parts(&["a", "b", "c", "d", "e"]),
            vec![net("SIGNAL", &head_pins)],
        ));
        let d = diff(&base, &head, None);
        let b = only(&d);
        assert_eq!(b.nets.renamed.len(), 1, "{:#?}", b.nets);
        let n = &b.nets.renamed[0];
        assert_eq!((n.base_name.as_str(), n.name.as_str()), ("SIG", "SIGNAL"));
        assert_eq!(n.joined.len(), 1);
        assert_eq!(n.joined[0].part, "e");
        assert_eq!(n.left.len(), 1);
        assert_eq!(n.left[0].part, "d");
        assert!(b.nets.added.is_empty() && b.nets.removed.is_empty());
    }

    #[test]
    fn two_nets_that_share_less_than_half_their_pins_are_not_the_same_net() {
        let parts: Vec<PartDigest> = (0..6)
            .map(|i| part(&format!("p{i}"), &format!("R{i}"), "1k", &[]))
            .collect();
        let base = digest(board(
            parts.clone(),
            vec![net(
                "A",
                &[("p0", "1"), ("p1", "1"), ("p2", "1"), ("p3", "1")],
            )],
        ));
        let head = digest(board(
            parts,
            vec![net(
                "B",
                &[("p3", "1"), ("p4", "1"), ("p5", "1"), ("p0", "2")],
            )],
        ));
        let d = diff(&base, &head, None);
        let b = only(&d);
        assert_eq!(b.nets.added.len(), 1);
        assert_eq!(b.nets.removed.len(), 1);
        assert!(b.nets.renamed.is_empty());
    }

    #[test]
    fn nets_named_by_their_pins_follow_the_pins_and_are_not_renames() {
        let parts = |a: &str| vec![part(a, "R1", "1k", &[]), part("b", "R2", "1k", &[])];
        let base = digest(board(
            parts("old.a"),
            vec![net("net-b.1", &[("old.a", "1"), ("b", "1")])],
        ));
        let head = digest(board(
            parts("new.a"),
            vec![net("net-b.1", &[("new.a", "1"), ("b", "1")])],
        ));
        let d = diff(&base, &head, None);
        let b = only(&d);
        // `old.a` is gone and `new.a` is new, with the same kind, value and pins: it moved.
        assert!(b.parts.added.is_empty() && b.parts.removed.is_empty());
        assert_eq!(b.parts.changed.len(), 1);
        assert_eq!(b.parts.changed[0].base_path.as_deref(), Some("old.a"));
        assert_eq!(b.nets.unchanged, 1);
        assert!(b.nets.changed.is_empty() && b.nets.renamed.is_empty());
        assert_eq!(b.status, BoardStatus::Changed);
    }

    #[test]
    fn a_part_that_is_alike_but_on_other_nets_is_not_a_move() {
        let base = digest(board(
            vec![part("a.r", "R1", "1k", &[("1", "X")])],
            vec![net("X", &[("a.r", "1")])],
        ));
        let head = digest(board(
            vec![part("b.r", "R1", "1k", &[("1", "Y")])],
            vec![net("Y", &[("b.r", "1")])],
        ));
        let d = diff(&base, &head, None);
        let b = only(&d);
        assert_eq!((b.parts.added.len(), b.parts.removed.len()), (1, 1));
        assert!(b.parts.changed.is_empty());
    }

    #[test]
    fn a_designator_alone_is_renumbered_and_changes_nothing() {
        let base = digest(board(vec![part("r", "R1", "1k", &[])], Vec::new()));
        let head = digest(board(vec![part("r", "R7", "1k", &[])], Vec::new()));
        let d = diff(&base, &head, None);
        let b = only(&d);
        assert_eq!(b.status, BoardStatus::Unchanged);
        assert_eq!(b.parts.renumbered.len(), 1);
        assert!(b.parts.changed.is_empty());
        assert!(!d.changed);
        assert_eq!(d.summary, "No circuit changes (1 part renumbered)");
        // Fields come in a fixed order.
        let head = digest(board(
            vec![{
                let mut p = part("r", "R7", "2k", &[]);
                p.package = Some("0603".into());
                p
            }],
            Vec::new(),
        ));
        let d = diff(&base, &head, None);
        let fields: Vec<&str> = only(&d).parts.changed[0]
            .fields
            .iter()
            .map(|f| f.field.as_str())
            .collect();
        assert_eq!(fields, ["value", "package", "designator"]);
    }

    #[test]
    fn rails_say_what_changed_and_the_headroom_with_it() {
        let mut b = board(Vec::new(), Vec::new());
        b.rails = vec![
            rail("3V3", 0.02, Some(0.1)),
            rail("5V", 0.5, None),
            rail("OLD", 0.1, None),
        ];
        let mut h = board(Vec::new(), Vec::new());
        h.rails = vec![
            rail("3V3", 0.08, Some(0.1)),
            rail("5V", 0.5, None),
            rail("NEW", 0.1, None),
        ];
        let d = diff(&digest(b), &digest(h), None);
        let b = only(&d);
        assert_eq!(b.rails.unchanged, 1);
        assert_eq!(
            b.rails
                .added
                .iter()
                .map(|r| r.net.as_str())
                .collect::<Vec<_>>(),
            ["NEW"]
        );
        assert_eq!(
            b.rails
                .removed
                .iter()
                .map(|r| r.net.as_str())
                .collect::<Vec<_>>(),
            ["OLD"]
        );
        let r = &b.rails.changed[0];
        assert_eq!(r.net, "3V3");
        assert!((r.headroom_base.unwrap() - 0.08).abs() < 1e-12);
        assert!((r.headroom_head.unwrap() - 0.02).abs() < 1e-12);
        assert!(r.voltage.is_none() && r.capacity.is_none() && r.budget.is_none());
        assert_eq!(
            d.summary,
            "1 rail's headroom shrank from 80 mA to 20 mA, 1 rail added, 1 rail removed"
        );
    }

    #[test]
    fn a_demand_over_the_supply_flips_the_budget() {
        let mut b = board(Vec::new(), Vec::new());
        b.rails = vec![rail("3V3", 0.05, Some(0.1))];
        let mut h = board(Vec::new(), Vec::new());
        h.rails = vec![rail("3V3", 0.12, Some(0.1))];
        let d = diff(&digest(b), &digest(h), None);
        let r = &only(&d).rails.changed[0];
        let budget = r.budget.unwrap();
        assert_eq!((budget.base, budget.head), (Budget::Within, Budget::Over));
        assert!(r.headroom_head.unwrap() < 0.0);
    }

    #[test]
    fn a_renamed_net_carries_its_rail_with_it() {
        let mut b = board(Vec::new(), vec![net("3V3", &[("a", "1")])]);
        b.parts = vec![part("a", "R1", "1k", &[])];
        b.rails = vec![rail("3V3", 0.02, Some(0.1))];
        let mut h = board(Vec::new(), vec![net("VOUT", &[("a", "1")])]);
        h.parts = vec![part("a", "R1", "1k", &[])];
        h.rails = vec![rail("VOUT", 0.02, Some(0.1))];
        let d = diff(&digest(b), &digest(h), None);
        let b = only(&d);
        assert!(
            b.rails.added.is_empty() && b.rails.removed.is_empty() && b.rails.changed.is_empty()
        );
        assert_eq!(b.nets.renamed.len(), 1);
    }

    #[test]
    fn the_summary_reads_as_one_line() {
        let parts = |n: usize, value: &str| -> Vec<PartDigest> {
            (0..n)
                .map(|i| part(&format!("p{i}"), &format!("R{i}"), value, &[]))
                .collect()
        };
        // Four parts at the ref; one removed, two change value, three new.
        let mut base = board(parts(4, "1k"), Vec::new());
        base.rails = vec![rail("3V3", 0.02, Some(0.1))];
        let mut head_parts = parts(4, "1k");
        head_parts.remove(3);
        head_parts[0].value = "2k".into();
        head_parts[1].value = "2k".into();
        head_parts.extend((10..13).map(|i| part(&format!("p{i}"), &format!("R{i}"), "1k", &[])));
        let mut head = board(head_parts, Vec::new());
        head.rails = vec![rail("3V3", 0.08, Some(0.1))];
        let d = diff(&digest(base), &digest(head), None);
        assert_eq!(
            d.summary,
            "3 parts added, 1 removed, 2 values changed, 1 rail's headroom shrank from 80 mA to 20 mA"
        );
        let empty = diff(
            &digest(board(Vec::new(), Vec::new())),
            &digest(board(Vec::new(), Vec::new())),
            None,
        );
        assert_eq!(empty.summary, "No circuit changes");
        assert!(!empty.changed);
    }

    fn line(value: &str, lcsc: &str, quantity: u32, unit: Option<f64>) -> BomLine {
        BomLine {
            designators: (1..=quantity).map(|i| format!("R{i}")).collect(),
            quantity,
            value: value.into(),
            kind: "Resistor".into(),
            package: Some("0402".into()),
            footprint: Some("R_0402_1005Metric".into()),
            lcsc: Some(lcsc.into()),
            mpn: None,
            library: None,
            unit_usd: unit,
            line_usd: unit.map(|u| u * f64::from(quantity)),
            stock: None,
            date: None,
            age_days: None,
            stale: false,
            supply: None,
            unknown: None,
            unresolved: None,
            hand_solder: false,
            via: None,
        }
    }

    fn with_bom(lines: Vec<BomLine>) -> BoardDigest {
        let total: f64 = lines.iter().filter_map(|l| l.line_usd).sum();
        let mut b = board(Vec::new(), Vec::new());
        b.bom = Ok(Bom {
            lines,
            total_usd: Some(total),
            ..BoardDigest::nothing().bom.unwrap()
        });
        b
    }

    #[test]
    fn a_changed_manufacturers_number_is_a_changed_line_and_not_one_removed_and_one_added() {
        // The bill groups parts by value, footprint, LCSC number, kind and hand soldering: two
        // parts of a group may differ in their number, and the line shows the first one's.
        let with_mpn = |mpn: &str| {
            let mut l = line("1k", "C1", 2, Some(0.01));
            l.mpn = Some(mpn.into());
            l
        };
        let base = with_bom(vec![with_mpn("RC0402-A")]);
        let head = with_bom(vec![with_mpn("RC0402-B")]);
        let d = diff(&digest(base), &digest(head), None);
        let bom = only(&d).bom.as_ref().unwrap();
        assert!(bom.added.is_empty() && bom.removed.is_empty(), "{bom:?}");
        assert_eq!(bom.changed.len(), 1);
        assert_eq!(bom.changed[0].mpn.as_deref(), Some("RC0402-B"));
        assert_eq!(bom.changed[0].quantity_head, 2);
        assert_eq!(only(&d).status, BoardStatus::Changed);
        // The same number on both sides is no change.
        let same = diff(
            &digest(with_bom(vec![with_mpn("X")])),
            &digest(with_bom(vec![with_mpn("X")])),
            None,
        );
        assert_eq!(only(&same).status, BoardStatus::Unchanged);
    }

    #[test]
    fn the_bom_lines_are_added_removed_or_changed_in_quantity_or_price_with_the_cost() {
        let base = with_bom(vec![
            line("1k", "C1", 2, Some(0.01)),
            line("10k", "C2", 1, Some(0.02)),
            line("100k", "C3", 1, Some(0.03)),
        ]);
        let head = with_bom(vec![
            line("1k", "C1", 3, Some(0.01)),
            line("10k", "C2", 1, Some(0.04)),
            line("1M", "C4", 1, Some(0.05)),
        ]);
        let d = diff(&digest(base), &digest(head), None);
        let bom = only(&d).bom.as_ref().unwrap();
        assert_eq!(
            bom.added
                .iter()
                .map(|l| l.value.as_str())
                .collect::<Vec<_>>(),
            ["1M"]
        );
        assert_eq!(
            bom.removed
                .iter()
                .map(|l| l.value.as_str())
                .collect::<Vec<_>>(),
            ["100k"]
        );
        assert_eq!(bom.changed.len(), 2);
        let k = bom.changed.iter().find(|c| c.value == "1k").unwrap();
        assert_eq!((k.quantity_base, k.quantity_head), (2, 3));
        let ten = bom.changed.iter().find(|c| c.value == "10k").unwrap();
        assert_eq!(
            (ten.unit_usd_base, ten.unit_usd_head),
            (Some(0.02), Some(0.04))
        );
        assert!(
            (bom.delta_usd.unwrap() - (0.03 + 0.04 + 0.05 - (0.02 + 0.02 + 0.03))).abs() < 1e-12
        );
        assert!(
            d.summary.contains("BOM +$0.0500 per board"),
            "{}",
            d.summary
        );
        // A line that only has other designators is not a change.
        let base = with_bom(vec![line("1k", "C1", 1, None)]);
        let mut renumbered = line("1k", "C1", 1, None);
        renumbered.designators = vec!["R9".into()];
        let d = diff(&digest(base), &digest(with_bom(vec![renumbered])), None);
        assert!(only(&d).bom.as_ref().unwrap().changed.is_empty());
        assert_eq!(only(&d).bom.as_ref().unwrap().unchanged, 1);
    }

    #[test]
    fn problems_are_the_same_by_severity_stage_board_and_message_and_counted_once_each() {
        let mut base = digest(board(Vec::new(), Vec::new()));
        let mut head = digest(board(Vec::new(), Vec::new()));
        base.problems = vec![
            problem(Severity::Warning, Some("B"), "old"),
            problem(Severity::Warning, Some("B"), "same"),
            problem(Severity::Warning, Some("B"), "twice"),
            problem(Severity::Note, None, "a note of the project"),
        ];
        head.problems = vec![
            problem(Severity::Warning, Some("B"), "same"),
            problem(Severity::Warning, Some("B"), "twice"),
            problem(Severity::Warning, Some("B"), "twice"),
            problem(Severity::Error, Some("B"), "fresh"),
            problem(Severity::Error, None, "of the project"),
        ];
        let d = diff(&base, &head, None);
        let b = only(&d);
        assert_eq!(b.problems.unchanged, 2);
        let said = |l: &[Problem]| l.iter().map(|p| p.message.clone()).collect::<Vec<_>>();
        assert_eq!(said(&b.problems.new), ["fresh", "twice"], "errors first");
        assert_eq!(said(&b.problems.resolved), ["old"]);
        assert_eq!(said(&d.problems.new), ["of the project"]);
        assert_eq!(said(&d.problems.resolved), ["a note of the project"]);
        assert_eq!(b.status, BoardStatus::Changed);
        assert!(d.changed);
        // Errors, warnings and notes are each said, the notes last.
        assert_eq!(
            d.summary,
            "2 new errors, 1 new warning, 1 warning resolved, 1 note resolved"
        );
    }

    #[test]
    fn the_layout_files_problems_are_not_a_change_of_the_circuit() {
        let layout = |board: Option<&str>| Problem {
            stage: Some(ProblemStage::Layout),
            ..problem(Severity::Note, board, "the design has no part `gone`")
        };
        let mut base = digest(board(Vec::new(), Vec::new()));
        let mut head = digest(board(Vec::new(), Vec::new()));
        base.problems = vec![problem(Severity::Warning, Some("B"), "same")];
        head.problems = vec![
            problem(Severity::Warning, Some("B"), "same"),
            layout(Some("B")),
            layout(None),
        ];
        let d = diff(&base, &head, None);
        assert!(only(&d).problems.new.is_empty() && d.problems.new.is_empty());
        assert!(
            !d.changed,
            "a new note of the layout file changes nothing compared"
        );
    }

    #[test]
    fn a_module_and_a_port_are_kept_by_path() {
        let inst = |path: &str, type_name: &str| InstanceDigest {
            path: path.into(),
            type_name: type_name.into(),
            at: None,
        };
        let mut b = board(Vec::new(), Vec::new());
        b.modules = vec![inst("stage", "LdoStage"), inst("old", "Old")];
        b.interfaces = vec![inst("stage.vin", "Rail")];
        let mut h = board(Vec::new(), Vec::new());
        h.modules = vec![inst("stage", "BuckStage"), inst("new", "New")];
        h.interfaces = vec![inst("stage.vin", "Rail"), inst("stage.en", "Enable")];
        let d = diff(&digest(b), &digest(h), None);
        let b = only(&d);
        assert_eq!(b.modules.added[0].path, "new");
        assert_eq!(b.modules.removed[0].path, "old");
        assert_eq!(
            (
                b.modules.changed[0].base_type.as_str(),
                b.modules.changed[0].head_type.as_str()
            ),
            ("LdoStage", "BuckStage")
        );
        assert_eq!(b.interfaces.added[0].path, "stage.en");
        assert!(b.interfaces.removed.is_empty());
        assert_eq!(d.summary, "1 module added, 1 module removed, 1 port added");
    }

    #[test]
    fn amps_are_written_as_a_person_does() {
        assert_eq!(amps(0.08), "80 mA");
        assert_eq!(amps(0.0205), "20.5 mA");
        assert_eq!(amps(1.5), "1.5 A");
        assert_eq!(amps(0.000_25), "250 µA");
        assert_eq!(amps(0.0), "0 µA");
    }

    // ---- the PCB ----

    fn place(x: f64, y: f64, placement: &str) -> ComparePcbPlace {
        ComparePcbPlace {
            x,
            y,
            rotation: 0.0,
            side: PcbSide::Top,
            locked: false,
            placement: placement.into(),
        }
    }

    fn pcb_part(path: &str, place: ComparePcbPlace) -> PcbPartDigest {
        PcbPartDigest {
            path: path.into(),
            designator: "R1".into(),
            place,
        }
    }

    fn placed(parts: Vec<PcbPartDigest>) -> PcbDigest {
        PcbDigest {
            layout_file: "src/main.pcb.toml".into(),
            outline: CompareSize {
                width: 30.0,
                height: 20.0,
            },
            outline_stated: false,
            layers: vec!["top".into(), "bottom".into()],
            parts,
            keepouts: Vec::new(),
            cutouts: Vec::new(),
        }
    }

    fn with_pcb(mut b: BoardDigest, p: Result<PcbDigest, String>) -> BoardDigest {
        b.pcb = Some(p);
        b
    }

    fn pcb_of(base: PcbDigest, head: PcbDigest) -> (Diff, PcbDiff) {
        let d = diff(
            &digest(with_pcb(board(Vec::new(), Vec::new()), Ok(base))),
            &digest(with_pcb(board(Vec::new(), Vec::new()), Ok(head))),
            None,
        );
        let p = only(&d).pcb.clone().expect("a PCB diff");
        (d, p)
    }

    #[test]
    fn a_part_that_moved_turned_flipped_or_was_locked_is_listed_with_each_field() {
        let mut turned = place(5.0, 2.0, "hand");
        turned.rotation = 90.0;
        turned.side = PcbSide::Bottom;
        turned.locked = true;
        let (d, p) = pcb_of(
            placed(vec![
                pcb_part("a", place(5.0, 2.0, "hand")),
                pcb_part("b", place(1.0, 1.0, "hand")),
            ]),
            placed(vec![
                pcb_part("a", turned),
                pcb_part("b", place(1.003, 1.0, "hand")),
            ]),
        );
        assert_eq!(p.moved.len(), 1, "{:#?}", p.moved);
        let names: Vec<&str> = p.moved[0].fields.iter().map(|f| f.field.as_str()).collect();
        assert_eq!(names, ["rotation", "side", "locked"]);
        assert_eq!(p.moved[0].distance_mm, 0.0);
        // The hundredth of a millimetre is what the layout file keeps: 0.003 is no move.
        assert_eq!(p.unchanged, 1);
        assert_eq!(d.summary, "1 part moved on the PCB");
        assert_eq!(only(&d).status, BoardStatus::Changed);
        assert!(d.changed);
    }

    #[test]
    fn what_the_placer_chose_on_both_sides_is_not_a_move_but_one_side_stated_is() {
        let (d, p) = pcb_of(
            placed(vec![
                pcb_part("a", place(5.0, 2.0, "derived")),
                pcb_part("b", place(1.0, 1.0, "derived")),
            ]),
            placed(vec![
                pcb_part("a", place(9.0, 9.0, "derived")),
                pcb_part("b", place(4.0, 1.0, "hand")),
            ]),
        );
        assert_eq!(p.moved.len(), 1);
        assert_eq!(p.moved[0].path, "b");
        assert_eq!(p.moved[0].base.placement, "derived");
        assert!((p.moved[0].distance_mm - 3.0).abs() < 1e-9);
        assert_eq!(p.unchanged, 1);
        assert_eq!(d.summary, "1 part moved on the PCB");
    }

    #[test]
    fn a_part_that_moved_in_the_source_is_followed_to_its_new_path() {
        let digest_of = |path: &str, x: f64| {
            let mut b = board(vec![part(path, "R1", "1k", &[])], Vec::new());
            b.pcb = Some(Ok(placed(vec![pcb_part(path, place(x, 2.0, "hand"))])));
            digest(b)
        };
        let d = diff(&digest_of("old.r", 5.0), &digest_of("new.r", 6.0), None);
        let b = only(&d);
        assert_eq!(b.parts.changed[0].base_path.as_deref(), Some("old.r"));
        let m = &b.pcb.as_ref().unwrap().moved[0];
        assert_eq!(
            (m.path.as_str(), m.base_path.as_deref()),
            ("new.r", Some("old.r"))
        );
    }

    #[test]
    fn an_outline_is_compared_when_the_design_states_it_and_not_when_it_follows_the_parts() {
        let bigger = |stated: bool| {
            let mut p = placed(Vec::new());
            p.outline.width = 32.0;
            p.outline_stated = stated;
            p
        };
        let (d, p) = pcb_of(placed(Vec::new()), bigger(false));
        assert!(p.outline.is_none());
        assert_eq!(d.summary, "No circuit changes");
        assert_eq!(only(&d).status, BoardStatus::Unchanged);
        let (d, p) = pcb_of(placed(Vec::new()), bigger(true));
        assert_eq!(p.outline.unwrap().head.width, 32.0);
        assert_eq!(d.summary, "outline 30 × 20 → 32 × 20 mm");
        // Stated on the other side only is a statement too.
        let mut base = placed(Vec::new());
        base.outline_stated = true;
        let (_, p) = pcb_of(base, bigger(false));
        assert!(p.outline.is_some());
    }

    #[test]
    fn layers_keepouts_and_cutouts_are_compared_by_name() {
        let area = |x: f64| {
            Some(PcbBox {
                x,
                y: 1.0,
                width: 4.0,
                height: 2.0,
            })
        };
        let mut base = placed(Vec::new());
        base.keepouts = vec![("antenna".into(), area(1.0)), ("gone".into(), area(2.0))];
        base.cutouts = vec![("slot".into(), area(3.0))];
        let mut head = placed(Vec::new());
        head.layers = vec![
            "top".into(),
            "inner1".into(),
            "inner2".into(),
            "bottom".into(),
        ];
        head.keepouts = vec![("antenna".into(), area(6.0)), ("new".into(), area(2.0))];
        head.cutouts = vec![("slot".into(), area(3.001))];
        let (d, p) = pcb_of(base, head);
        assert_eq!(p.layers.as_ref().unwrap().head.len(), 4);
        let status = |v: &[ComparePcbArea]| -> Vec<(String, BoardStatus)> {
            v.iter().map(|a| (a.name.clone(), a.status)).collect()
        };
        assert_eq!(
            status(&p.keepouts),
            [
                ("antenna".to_string(), BoardStatus::Changed),
                ("gone".to_string(), BoardStatus::Removed),
                ("new".to_string(), BoardStatus::Added),
            ]
        );
        assert!(p.cutouts.is_empty(), "0.001 mm is the same area");
        assert_eq!(d.summary, "layers 2 → 4, 3 keepouts changed");
    }

    #[test]
    fn a_board_that_cannot_be_placed_on_one_side_says_so_and_on_both_says_nothing() {
        let side = |p: Option<Result<PcbDigest, String>>| BoardDigest {
            pcb: p,
            ..board(Vec::new(), Vec::new())
        };
        let ok = || Some(Ok(placed(Vec::new())));
        let broken = || Some(Err("no footprint".to_string()));
        let d = diff(&digest(side(ok())), &digest(side(broken())), None);
        assert!(only(&d).pcb.is_none());
        assert!(only(&d).pcb_error.as_deref().unwrap().contains("head side"));
        let d = diff(&digest(side(broken())), &digest(side(ok())), None);
        assert!(
            only(&d)
                .pcb_error
                .as_deref()
                .unwrap()
                .contains("at the ref")
        );
        let d = diff(&digest(side(broken())), &digest(side(broken())), None);
        assert!(only(&d).pcb.is_none() && only(&d).pcb_error.is_none());
        let d = diff(&digest(side(None)), &digest(side(None)), None);
        assert!(only(&d).pcb.is_none() && only(&d).pcb_error.is_none());
    }

    // ---- the systems ----

    use netsmith_editor::api::{Edge, GroundGroupView, MateContact, MateView, Port, SystemNode};

    fn node(name: &str, ty: &str, ports: &[(&str, Option<&str>, Option<&str>)]) -> SystemNode {
        SystemNode {
            id: name.into(),
            name: name.into(),
            type_name: ty.into(),
            kind: "board".into(),
            ports: ports
                .iter()
                .map(|(n, v, c)| Port {
                    name: (*n).into(),
                    voltage: v.map(str::to_string),
                    current: c.map(str::to_string),
                })
                .collect(),
            at: None,
        }
    }

    fn contact(n: u32, a: &str, b: &str) -> MateContact {
        MateContact {
            number: n,
            a_role: a.into(),
            b_role: b.into(),
            a_net: Some("NETA".into()),
            b_net: Some("NETB".into()),
        }
    }

    fn mate(a: &str, b: &str, contacts: Vec<MateContact>) -> MateView {
        MateView {
            a: a.into(),
            b: b.into(),
            a_board: "p".into(),
            b_board: "l".into(),
            contacts,
            ports: vec!["p.out ~ l.in".into()],
            reversible: None,
            at: None,
        }
    }

    fn sys(
        nodes: Vec<SystemNode>,
        joins: &[&str],
        mates: Vec<MateView>,
        grounds: &[&str],
        rails: Vec<PowerNode>,
    ) -> SystemDigest {
        SystemDigest {
            file: "src/main.nsm".into(),
            view: SystemView {
                name: "S".into(),
                kind: "system".into(),
                nodes,
                edges: joins
                    .iter()
                    .map(|j| Edge {
                        from: "p".into(),
                        to: "l".into(),
                        label: (*j).into(),
                        kind: EdgeKind::Join,
                        at: None,
                    })
                    .collect(),
                mates,
                ground_groups: grounds
                    .iter()
                    .map(|g| GroundGroupView {
                        nets: g.split(',').map(str::to_string).collect(),
                        contacts: Vec::new(),
                    })
                    .collect(),
            },
            rails,
        }
    }

    fn with_systems(d: Digest, s: Option<SystemDigest>) -> Digest {
        Digest {
            systems: s.into_iter().map(|s| ("S".to_string(), s)).collect(),
            ..d
        }
    }

    fn system_of(d: &Diff) -> &CompareSystem {
        assert_eq!(d.systems.len(), 1);
        &d.systems[0]
    }

    fn quiet() -> Digest {
        digest(board(Vec::new(), Vec::new()))
    }

    #[test]
    fn a_system_compares_its_boards_ports_joins_mates_and_grounds() {
        let base = sys(
            vec![
                node("p", "PowerBoard", &[("out", Some("3.3V"), Some("0..50mA"))]),
                node("l", "LedBoard", &[("in", None, None)]),
                node("gone", "Old", &[]),
            ],
            &["p.out ~ l.in", "p.a ~ l.a"],
            vec![
                mate(
                    "p.j1",
                    "l.j1",
                    vec![contact(1, "power", "power"), contact(2, "ground", "ground")],
                ),
                mate("p.j2", "l.j2", vec![contact(1, "signal", "signal")]),
            ],
            &["p.GND,l.GND"],
            Vec::new(),
        );
        let head = sys(
            vec![
                node(
                    "p",
                    "PowerBoard2",
                    &[
                        ("out", Some("3.3V"), Some("0..30mA")),
                        ("aux", Some("5V"), None),
                    ],
                ),
                node("l", "LedBoard", &[("in", None, None)]),
                node("new", "Fresh", &[]),
            ],
            &["p.out ~ l.in", "p.b ~ l.b"],
            vec![
                mate(
                    "p.j1",
                    "l.j1",
                    vec![
                        contact(1, "power", "power"),
                        contact(2, "signal", "ground"),
                        contact(3, "open", "open"),
                    ],
                ),
                mate("p.j3", "l.j3", vec![]),
            ],
            &["p.GND,l.GND,x.GND"],
            Vec::new(),
        );
        let d = diff(
            &with_systems(quiet(), Some(base)),
            &with_systems(quiet(), Some(head)),
            None,
        );
        let s = system_of(&d);
        assert_eq!(s.status, BoardStatus::Changed);
        assert_eq!(s.boards.added[0].path, "new");
        assert_eq!(s.boards.removed[0].path, "gone");
        assert_eq!(
            (
                s.boards.changed[0].base_type.as_str(),
                s.boards.changed[0].head_type.as_str()
            ),
            ("PowerBoard", "PowerBoard2")
        );
        assert_eq!(s.ports.added[0].name, "aux");
        let c = &s.ports.changed[0];
        assert_eq!((c.board.as_str(), c.name.as_str()), ("p", "out"));
        assert_eq!(c.fields.len(), 1);
        assert_eq!(c.fields[0].old.as_deref(), Some("0..50mA"));
        assert_eq!(c.fields[0].new.as_deref(), Some("0..30mA"));
        assert_eq!(
            (s.joins.added.as_slice(), s.joins.removed.as_slice()),
            (
                &["p.b ~ l.b".to_string()][..],
                &["p.a ~ l.a".to_string()][..]
            )
        );
        assert_eq!(
            (s.mates.added[0].a.as_str(), s.mates.removed[0].a.as_str()),
            ("p.j3", "p.j2")
        );
        let changed = &s.mates.changed[0];
        let fields: Vec<(&str, Option<&str>, Option<&str>)> = changed
            .fields
            .iter()
            .map(|f| (f.field.as_str(), f.old.as_deref(), f.new.as_deref()))
            .collect();
        assert_eq!(
            fields,
            [
                (
                    "contact 2",
                    Some("ground to ground"),
                    Some("signal to ground")
                ),
                ("contact 3", None, Some("open to open")),
            ]
        );
        assert_eq!(s.ground_groups.added, ["p.GND + l.GND + x.GND"]);
        assert_eq!(s.ground_groups.removed, ["p.GND + l.GND"]);
        assert_eq!(
            d.summary,
            "1 system boards added, 1 system boards removed, 1 system boards changed, 2 system ports changed, 2 system joins changed, 3 mates changed, 2 ground groups changed"
        );
    }

    #[test]
    fn what_the_nets_of_a_mate_are_called_is_not_a_change_of_the_mate() {
        let with = |net: &str| {
            let mut c = contact(1, "power", "power");
            c.a_net = Some(net.into());
            sys(
                Vec::new(),
                &[],
                vec![mate("p.j1", "l.j1", vec![c])],
                &[],
                Vec::new(),
            )
        };
        let d = diff(
            &with_systems(quiet(), Some(with("VOUT"))),
            &with_systems(quiet(), Some(with("3V3"))),
            None,
        );
        assert_eq!(system_of(&d).status, BoardStatus::Unchanged);
        assert_eq!(d.summary, "No circuit changes");
    }

    #[test]
    fn a_system_on_one_side_is_added_or_removed_and_the_sentence_names_it() {
        let one = || sys(vec![node("p", "P", &[])], &[], Vec::new(), &[], Vec::new());
        let d = diff(
            &with_systems(quiet(), None),
            &with_systems(quiet(), Some(one())),
            None,
        );
        assert_eq!(system_of(&d).status, BoardStatus::Added);
        assert_eq!(system_of(&d).base_file, None);
        assert_eq!(d.summary, "system S added");
        let d = diff(
            &with_systems(quiet(), Some(one())),
            &with_systems(quiet(), None),
            None,
        );
        assert_eq!(system_of(&d).status, BoardStatus::Removed);
        assert_eq!(d.summary, "system S removed");
        assert!(d.changed);
        // One board asked for leaves the systems out.
        let d = diff(
            &with_systems(digest(board(Vec::new(), Vec::new())), None),
            &with_systems(digest(board(Vec::new(), Vec::new())), Some(one())),
            Some("B"),
        );
        assert!(d.systems.is_empty());
    }

    #[test]
    fn a_system_rail_is_listed_only_when_no_board_already_says_it() {
        // `p:3V3` is a rail of the board `S/p`, whose own comparison lists it; `l:VIN` only the
        // system explains (what a join gives it).
        let node_rail = |id: &str, capacity: f64| {
            let mut r = rail(id, 0.01, Some(capacity));
            r.id = id.into();
            r
        };
        let rails = |c: f64| vec![node_rail("p:3V3", c), node_rail("l:VIN", c)];
        let base_board = board(Vec::new(), Vec::new());
        let mut head_board = board(Vec::new(), Vec::new());
        head_board.rails = vec![rail("3V3", 0.01, Some(0.02))];
        let mut base_board2 = base_board.clone();
        base_board2.rails = vec![rail("3V3", 0.01, Some(0.05))];
        let named = |b: BoardDigest| Digest {
            boards: BTreeMap::from([("S/p".to_string(), b)]),
            ..quiet()
        };
        let base = with_systems(
            named(base_board2),
            Some(sys(Vec::new(), &[], Vec::new(), &[], rails(0.05))),
        );
        let head = with_systems(
            named(head_board),
            Some(sys(Vec::new(), &[], Vec::new(), &[], rails(0.02))),
        );
        let d = diff(&base, &head, None);
        let s = system_of(&d);
        let nets: Vec<&str> = s.rails.changed.iter().map(|r| r.net.as_str()).collect();
        assert_eq!(nets, ["l:VIN"], "{:#?}", s.rails);
        assert_eq!(d.boards[0].rails.changed.len(), 1);
    }

    // ---- the kind of a part ----

    fn kind_part(path: &str, kind: &str, value: &str) -> PartDigest {
        PartDigest {
            kind: kind.into(),
            ..part(path, "R1", value, &[])
        }
    }

    #[test]
    fn a_part_that_is_another_library_part_says_what_it_was_and_what_it_is() {
        let base = digest(board(vec![kind_part("x", "Resistor", "1k")], Vec::new()));
        let head = digest(board(vec![kind_part("x", "Capacitor", "10u")], Vec::new()));
        let d = diff(&base, &head, None);
        let c = &only(&d).parts.changed[0];
        assert_eq!(
            (c.kind.as_str(), c.base_kind.as_deref()),
            ("Capacitor", Some("Resistor"))
        );
        let fields: Vec<&str> = c.fields.iter().map(|f| f.field.as_str()).collect();
        assert_eq!(fields, ["kind", "value"]);
        // Counted once, as a kind: the value that came with it is not another change.
        assert_eq!(
            d.summary,
            "1 part's kind changed from Resistor to Capacitor"
        );
        assert_eq!(only(&d).summary, d.summary);
    }

    #[test]
    fn a_value_alone_leaves_the_kind_to_the_row_and_says_no_kind_changed() {
        let base = digest(board(vec![kind_part("x", "Resistor", "1k")], Vec::new()));
        let head = digest(board(vec![kind_part("x", "Resistor", "2k")], Vec::new()));
        let d = diff(&base, &head, None);
        let c = &only(&d).parts.changed[0];
        assert_eq!((c.kind.as_str(), c.base_kind.clone()), ("Resistor", None));
        assert_eq!(d.summary, "1 value changed");
    }

    #[test]
    fn several_kind_changes_are_counted_and_beside_values_and_a_kind_alone_with_the_same_value() {
        let base = digest(board(
            vec![
                kind_part("a", "Header1x2", "Conn_01x02"),
                kind_part("b", "Resistor", "1k"),
                kind_part("c", "Led", "red"),
            ],
            Vec::new(),
        ));
        let head = digest(board(
            vec![
                kind_part("a", "Socket1x2", "Conn_01x02"),
                kind_part("b", "Inductor", "1k"),
                kind_part("c", "Led", "green"),
            ],
            Vec::new(),
        ));
        let d = diff(&base, &head, None);
        let a = only(&d)
            .parts
            .changed
            .iter()
            .find(|p| p.path == "a")
            .unwrap();
        // The value is the same: the kind is the whole of the change.
        assert_eq!(a.fields.len(), 1);
        assert_eq!(a.fields[0].field, "kind");
        assert_eq!(d.summary, "2 parts' kinds changed, 1 value changed");
        // A kind that is the same is not mentioned for a part that only moved.
        let moved = diff(
            &digest(board(vec![kind_part("old.x", "Led", "red")], Vec::new())),
            &digest(board(vec![kind_part("new.x", "Led", "red")], Vec::new())),
            None,
        );
        assert_eq!(only(&moved).parts.changed[0].base_kind, None);
        assert_eq!(moved.summary, "1 part moved");
    }
}
