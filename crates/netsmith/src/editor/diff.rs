//! The semantic difference between two readings of a project (ADR 0059): what the circuit,
//! its rails, its bill of materials and its problems did between a ref and the files on disk.
//! Pure: it takes what [`super::compare`] read out of two analyses and reads nothing else.
//!
//! Identity is by what the design calls a thing, never by what a tool numbered it: a part is
//! its path in the board (`stage.regulator`), a net its name, a module or a port its path.
//! A net that was renamed is found again by its pins, which are named by the paths of their
//! parts (ADR 0017's idea, with the paths to hand: no colouring of a graph is needed).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use netsmith_editor::api::{
    BoardStatus, Bom, BomDiff, BomLine, BudgetChange, CompareBoard, CompareBomChange,
    CompareBomLine, CompareField, CompareInstance, CompareInstanceChange, CompareNet,
    CompareNetChange, ComparePart, ComparePartChange, ComparePin, CompareRail, CompareRailChange,
    InstancesDiff, Location, NetsDiff, PartsDiff, PowerNode, Problem, ProblemsDiff, RailsDiff,
    Range, RangeChange, Severity,
};

/// One analysis, reduced to what a comparison reads.
#[derive(Debug, Clone)]
pub struct Digest {
    pub boards: BTreeMap<String, BoardDigest>,
    pub problems: Vec<Problem>,
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
        }
    }
}

/// The result of comparing two digests.
pub struct Diff {
    pub boards: Vec<CompareBoard>,
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
    // A single board asked for leaves the project's problems out of the sentence.
    let project = only.is_none().then_some(&problems);
    let summary = summarize(&boards, project);
    let changed = boards.iter().any(|b| b.status != BoardStatus::Unchanged)
        || project.is_some_and(|p| !p.new.is_empty() || !p.resolved.is_empty());
    Diff {
        boards,
        problems,
        summary,
        changed,
    }
}

fn problems_of(all: &[Problem], board: Option<&String>) -> Vec<Problem> {
    all.iter()
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
    let rails = rails(b, h, &renames);
    let modules = instances(&b.modules, &h.modules);
    let interfaces = instances(&b.interfaces, &h.interfaces);
    let (bom, bom_error) = match (&b.bom, &h.bom) {
        (Ok(base), Ok(head)) => (Some(bom(base, head)), None),
        (Err(e), _) => (None, Some(format!("the ref's bill of materials: {e}"))),
        (_, Err(e)) => (None, Some(format!("the bill of materials: {e}"))),
    };
    let problems = problems(base_problems, head_problems);
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
                || !problems.resolved.is_empty();
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
    };
    out.summary = summarize(std::slice::from_ref(&out), None);
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

fn rails(b: &BoardDigest, h: &BoardDigest, renames: &HashMap<String, String>) -> RailsDiff {
    let head: BTreeMap<&str, &PowerNode> = h.rails.iter().map(|n| (n.net.as_str(), n)).collect();
    let mut seen = BTreeSet::new();
    let mut out = RailsDiff {
        added: Vec::new(),
        removed: Vec::new(),
        changed: Vec::new(),
        unchanged: 0,
    };
    for base in &b.rails {
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
    for now in &h.rails {
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
fn summarize(boards: &[CompareBoard], project: Option<&ProblemsDiff>) -> String {
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
    for b in &both {
        for c in &b.parts.changed {
            let only = |name: &str| {
                c.fields
                    .iter()
                    .filter(|f| f.field != "designator")
                    .all(|f| f.field == name)
            };
            if c.fields.iter().any(|f| f.field == "value") {
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
    let own = |f: &dyn Fn(&ProblemsDiff) -> Vec<&Problem>| -> usize {
        let mut n = both.iter().map(|b| f(&b.problems).len()).sum::<usize>();
        if let Some(p) = project {
            n += f(p).len();
        }
        n
    };
    let new = own(&|p| {
        p.new
            .iter()
            .filter(|p| p.severity != Severity::Note)
            .collect()
    });
    let resolved = own(&|p| {
        p.resolved
            .iter()
            .filter(|p| p.severity != Severity::Note)
            .collect()
    });
    if new > 0 {
        pieces.push(format!(
            "{new} new {}",
            if new == 1 { "problem" } else { "problems" }
        ));
    }
    if resolved > 0 {
        pieces.push(format!("{resolved} resolved"));
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
        // A note is in the lists and not in the sentence.
        assert_eq!(d.summary, "3 new problems, 1 resolved");
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
}
