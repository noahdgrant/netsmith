//! The analysis of a board's power graph in a `scenario` (issue #1258): some supplies taken
//! away (`remove .usb`), a backup left (`source .cell`), and the current that then flows
//! backwards through the parts between rails.
//!
//! It runs after the values are solved, on the solved model, and says three things for each
//! scenario, in a note: which regulators and load switches each `source` reaches (forwards, from
//! their input, and backwards, from their output), which loads it powers and at what voltage, and
//! where each `sense` net settles. It is an error when a load or a sense net ends in a state its
//! own numbers do not allow.
//!
//! **The graph.** A node is a net, with the nets an inductor, a ferrite bead or a fuse joins to
//! it. Each node has a range of voltage in the scenario, or none (it is off). A supply that stays
//! holds its node at its rail's `.voltage`. Then, until nothing changes, each part between two
//! nodes gives the far node a range, and a node takes the highest of what it is given (the source
//! with the highest voltage wins, as at a diode-OR):
//!
//! - A linear regulator (`dc linear_regulator`) gives its output `min(set point, vin - dropout)`,
//!   with its `.dropout` when it has one; a switching regulator (`dc switching_regulator`)
//!   `min(set point, vin)`, a buck at its full duty. An undervoltage lockout is not modelled: a
//!   regulator whose input falls follows it down, the worst case for its loads.
//! - A load switch (`dc load_switch`) that its enable can turn on gives its output its input (the
//!   drop across it is left out).
//! - Backwards, from the output to the input, each of these reads its `.reverse`
//!   (`std::sources::Reverse`): a body diode gives the input the output less its
//!   `.reverse.forward_voltage`; a load switch that is on gives it the output through its on
//!   resistance; `Blocking` gives nothing. When the loads on the input draw more than the part's
//!   `.reverse.max_current`, the input cannot be held up from behind and its range runs from 0 V.
//! - A diode (`dc diode`) gives its cathode its anode less its forward voltage, and nothing
//!   backwards.
//!
//! A resistor joins no two nodes here, and a MOSFET switch (`dc nmos`, `dc pmos`) is not
//! followed. What a node's loads draw is the solved `.current` of the rails on it, in the board's
//! own state.
//!
//! **What a source reaches** is the graph worked out with that source alone held: the
//! regulators whose input or output it powers, and the loads (the rails of parts, other than the
//! rails the regulators give or are fed from) on the nodes it powers. Their voltages are those of
//! the graph with every supply that stays held.
//!
//! **A load's state.** A part that bounds the voltage of one of its rails by numbers alone
//! (`.vcc.voltage >= 2.4V`, `.vcc.voltage <= 5.5V`) is judged: a rail that is powered and not
//! inside the bounds is an error (the part is half on). A rail that is off is not judged. A
//! part that bounds none is listed with its voltage.
//!
//! **A sense net** that a part on the graph holds has its node's range. One that is off settles
//! through the resistors on it: the nodes that are off and that resistors join to it, solved
//! over every corner of the resistances and of the ranges of the nodes they reach (a node that is
//! on, or ground). One that no resistor joins to a node that is on or to ground floats, and is
//! not judged. Each pin on the net that has levels (`Model::levels_of`) is judged: the net must
//! be at or below its `vil` or at or above its `vih`, or it is an error that says the pin reads
//! neither; a pin whose part's supply (the levels' reference rail) is off while the net is not
//! is an error too (the pin powers the part); a pin whose supply sits at another voltage than
//! the one its levels were worked out at is not judged.

use std::collections::{HashMap, HashSet};

use netsmith_check::{
    DcKind, Diagnostic, Domain, FieldId, InstId, InstKind, Member, Model, RExpr, Scenario,
};
use netsmith_core::Cause;
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::{Dimension, Quantity};

use crate::eval::{Val, eval_with};
use crate::levels::NotEvaluated;

/// The most nodes a sense net's resistors may reach that are off, and the most ranges a corner
/// of them is made of: past these the net is not judged.
const MAX_FREE: usize = 8;
const MAX_CORNER_VALUES: usize = 12;

/// The faults of the board's scenarios, and a note for each that says what it found.
pub(crate) fn check(model: &Model) -> (Vec<Diagnostic>, Vec<NotEvaluated>) {
    let mut faults = Vec::new();
    let mut notes = Vec::new();
    if model.scenarios().is_empty() {
        return (faults, notes);
    }
    let graph = Graph::new(model);
    for scenario in model.scenarios() {
        graph.scenario(scenario, &mut faults, &mut notes);
    }
    (faults, notes)
}

/// A range of voltage, in volts.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Range {
    lo: f64,
    hi: f64,
}

impl Range {
    fn of(q: &Quantity) -> Option<Self> {
        (q.lo().is_finite() && q.hi().is_finite()).then(|| Self {
            lo: q.lo(),
            hi: q.hi(),
        })
    }

    /// The range less a drop, or none when nothing is left.
    fn less(self, drop: Range) -> Option<Self> {
        let hi = self.hi - drop.lo;
        (hi > 0.0).then(|| Self {
            lo: (self.lo - drop.hi).max(0.0),
            hi,
        })
    }

    /// At most `cap`, as a regulator's set point caps what it gives.
    fn capped(self, cap: Range) -> Self {
        Self {
            lo: self.lo.min(cap.lo),
            hi: self.hi.min(cap.hi),
        }
    }

    fn max(a: Option<Self>, b: Option<Self>) -> Option<Self> {
        match (a, b) {
            (Some(a), Some(b)) => Some(Self {
                lo: a.lo.max(b.lo),
                hi: a.hi.max(b.hi),
            }),
            (x, None) | (None, x) => x,
        }
    }

    fn within(self, lo: Option<f64>, hi: Option<f64>) -> bool {
        let eps = 1e-9;
        lo.is_none_or(|l| self.lo >= l - eps) && hi.is_none_or(|h| self.hi <= h + eps)
    }
}

impl std::fmt::Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match Quantity::interval(Dimension::VOLTAGE, self.lo, self.hi) {
            Ok(q) => write!(f, "{q}"),
            Err(_) => write!(f, "{}V..{}V", self.lo, self.hi),
        }
    }
}

fn volts(v: f64) -> String {
    match Quantity::point(Dimension::VOLTAGE, v) {
        Ok(q) => q.to_string(),
        Err(_) => format!("{v}V"),
    }
}

/// How current from a part's output reaches its input (`std::sources::Reverse`).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Back {
    /// Through a diode with this drop.
    Diode(Range),
    /// The part blocks it.
    Blocking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConverterKind {
    Linear,
    Switching,
    /// A load switch, and whether its enable can turn it on.
    Switch(bool),
}

/// A part between two rails.
struct Converter {
    part: InstId,
    kind: ConverterKind,
    vin: InstId,
    vout: InstId,
    vin_node: usize,
    vout_node: usize,
    /// The output's set point, the range of its rail's `.voltage`.
    set: Option<Range>,
    dropout: Range,
    back: Back,
    /// `.reverse.max_current`, when the part states one.
    back_limit: Option<f64>,
}

/// A diode, anode to cathode.
struct Diode {
    anode: usize,
    cathode: usize,
    drop: Range,
}

/// A resistor between two nodes, for the settling of a sense net.
struct Resistor {
    a: usize,
    b: usize,
    ohms: (f64, f64),
}

/// A supply of the board: a rail that nothing on it drives.
struct Supply {
    node: usize,
    voltage: Option<Range>,
}

/// A rail of a part that draws from its node.
struct Load {
    part: InstId,
    rail: InstId,
    node: usize,
    /// The bounds the part puts on the rail's voltage by numbers alone.
    least: Option<f64>,
    most: Option<f64>,
}

/// Why a node has the range it has: what gave it, for the note.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Given {
    Held,
    Forward(usize),
    Backward(usize),
    Diode,
}

struct Graph<'a> {
    model: &'a Model,
    net_of: HashMap<InstId, usize>,
    /// The node of each net.
    node: Vec<usize>,
    nodes: usize,
    ground: Vec<bool>,
    converters: Vec<Converter>,
    diodes: Vec<Diode>,
    resistors: Vec<Resistor>,
    supplies: Vec<Supply>,
    loads: Vec<Load>,
    /// The most current the loads of each node draw, in amperes.
    draw: Vec<f64>,
}

/// The state of the graph with some supplies held.
struct State {
    v: Vec<Option<Range>>,
    given: Vec<Option<Given>>,
    /// The converters that could not hold their input up from behind: more was drawn than
    /// their `.reverse.max_current`.
    limited: Vec<usize>,
}

fn quantity(model: &Model, f: FieldId) -> Option<Quantity> {
    match &model.field(f).domain {
        Domain::Quantity(q) => Some(*q),
        _ => None,
    }
}

fn member_field(model: &Model, inst: InstId, name: &str) -> Option<FieldId> {
    match model.member(inst, name)? {
        Member::Field(f) => Some(f),
        _ => None,
    }
}

fn member_inst(model: &Model, inst: InstId, name: &str) -> Option<InstId> {
    match model.member(inst, name)? {
        Member::Inst(i) => Some(i),
        _ => None,
    }
}

impl<'a> Graph<'a> {
    fn new(model: &'a Model) -> Self {
        let nets = model.nets();
        let mut net_of = HashMap::new();
        for (k, net) in nets.iter().enumerate() {
            for pin in &net.pins {
                net_of.insert(*pin, k);
            }
        }
        // Nets that an inductor, a bead or a fuse joins are one node.
        let mut parent: Vec<usize> = (0..nets.len()).collect();
        fn root(parent: &mut [usize], mut k: usize) -> usize {
            while parent[k] != k {
                parent[k] = parent[parent[k]];
                k = parent[k];
            }
            k
        }
        let joins = |part: InstId| {
            model
                .instance(part)
                .standard
                .iter()
                .any(|s| matches!(s.as_str(), "Inductor" | "FerriteBead" | "Fuse"))
        };
        for (part, element) in model.dc_elements() {
            if element.kind == DcKind::Resistor
                && joins(part)
                && let (Some(&a), Some(&b)) = (
                    element.pins.first().and_then(|p| net_of.get(p)),
                    element.pins.get(1).and_then(|p| net_of.get(p)),
                )
            {
                let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                parent[ra.max(rb)] = ra.min(rb);
            }
        }
        let roots: Vec<usize> = (0..nets.len()).map(|k| root(&mut parent, k)).collect();
        let mut index: HashMap<usize, usize> = HashMap::new();
        let node: Vec<usize> = roots
            .iter()
            .map(|r| {
                let n = index.len();
                *index.entry(*r).or_insert(n)
            })
            .collect();
        let nodes = index.len();
        let node_of = |pin: InstId| net_of.get(&pin).map(|&k| node[k]);
        let pos_node = |rail: InstId| model.rail_of(rail).and_then(|(_, pos, _)| node_of(pos));

        let mut has_pos = vec![false; nodes];
        let mut has_neg = vec![false; nodes];
        for (id, _) in model.instances() {
            if let Some((_, pos, neg)) = model.rail_of(id) {
                if let Some(n) = node_of(pos) {
                    has_pos[n] = true;
                }
                if let Some(n) = node_of(neg) {
                    has_neg[n] = true;
                }
            }
        }
        let ground: Vec<bool> = (0..nodes).map(|n| has_neg[n] && !has_pos[n]).collect();

        let range_of = |f: Option<FieldId>| {
            f.and_then(|f| quantity(model, f))
                .and_then(|q| Range::of(&q))
        };
        let mut converters = Vec::new();
        let mut diodes = Vec::new();
        let mut resistors = Vec::new();
        let mut source_rails: Vec<InstId> = Vec::new();
        for (part, element) in model.dc_elements() {
            match element.kind {
                DcKind::LinearRegulator | DcKind::SwitchingRegulator | DcKind::LoadSwitch => {
                    let (Some(&vin), Some(&vout)) = (element.pins.first(), element.pins.get(1))
                    else {
                        continue;
                    };
                    let (Some(vin_node), Some(vout_node)) = (pos_node(vin), pos_node(vout)) else {
                        continue;
                    };
                    let kind = match element.kind {
                        DcKind::LinearRegulator => ConverterKind::Linear,
                        DcKind::SwitchingRegulator => ConverterKind::Switching,
                        _ => ConverterKind::Switch(model.load_switch_can_turn_on(part)),
                    };
                    let set = model.rail_of(vout).and_then(|(v, _, _)| range_of(Some(v)));
                    let dropout = match kind {
                        ConverterKind::Linear => range_of(member_field(model, part, "dropout")),
                        _ => None,
                    }
                    .unwrap_or(Range { lo: 0.0, hi: 0.0 });
                    let reverse = member_inst(model, part, "reverse");
                    let blocking = reverse
                        .and_then(|r| member_field(model, r, "path"))
                        .is_some_and(|f| {
                            matches!(&model.field(f).domain, Domain::Text(t) if t.ends_with("Blocking"))
                        });
                    // A part that has no `.reverse` (a definition of a project's own that does
                    // not provide a standard generic) is taken to have a body diode with the
                    // standard default drop.
                    let drop = reverse
                        .and_then(|r| range_of(member_field(model, r, "forward_voltage")))
                        .unwrap_or(Range { lo: 0.4, hi: 1.2 });
                    let back_limit = reverse
                        .and_then(|r| member_field(model, r, "max_current"))
                        .and_then(|f| quantity(model, f))
                        .map(|q| q.hi())
                        .filter(|x| x.is_finite());
                    converters.push(Converter {
                        part,
                        kind,
                        vin,
                        vout,
                        vin_node,
                        vout_node,
                        set,
                        dropout,
                        back: if blocking {
                            Back::Blocking
                        } else {
                            Back::Diode(drop)
                        },
                        back_limit,
                    });
                }
                DcKind::Diode => {
                    if let (Some(a), Some(c), Some(drop)) = (
                        element.pins.first().and_then(|p| node_of(*p)),
                        element.pins.get(1).and_then(|p| node_of(*p)),
                        range_of(element.values.first().copied()),
                    ) && a != c
                    {
                        diodes.push(Diode {
                            anode: a,
                            cathode: c,
                            drop,
                        });
                    }
                }
                DcKind::Resistor if !joins(part) => {
                    if let (Some(a), Some(b), Some(r)) = (
                        element.pins.first().and_then(|p| node_of(*p)),
                        element.pins.get(1).and_then(|p| node_of(*p)),
                        element.values.first().and_then(|f| quantity(model, *f)),
                    ) && a != b
                        && r.lo() > 0.0
                        && r.hi().is_finite()
                    {
                        resistors.push(Resistor {
                            a,
                            b,
                            ohms: (r.lo(), r.hi()),
                        });
                    }
                }
                DcKind::Source => source_rails.extend(element.pins.first().copied()),
                _ => {}
            }
        }

        // The supplies: a board's own rails and the rails parts say they give from outside,
        // that no part between rails drives.
        let driven: HashSet<usize> = converters.iter().map(|c| c.vout_node).collect();
        let mut supplies = Vec::new();
        let mut seen = HashSet::new();
        for (id, inst) in model.instances() {
            let own = inst.kind == InstKind::Interface
                && inst
                    .parent
                    .is_some_and(|p| model.instance(p).kind == InstKind::Board);
            if !(own || source_rails.contains(&id)) || !seen.insert(id) {
                continue;
            }
            let Some((voltage, _, _)) = model.rail_of(id) else {
                continue;
            };
            let Some(n) = pos_node(id) else {
                continue;
            };
            if driven.contains(&n) {
                continue;
            }
            supplies.push(Supply {
                node: n,
                voltage: range_of(Some(voltage)),
            });
        }

        // The loads: the rails of parts (and a board's own rails behind a regulator), but those
        // a part between rails gives or is fed from and those a part gives from outside.
        let own_rails: HashSet<InstId> = converters
            .iter()
            .flat_map(|c| [c.vin, c.vout])
            .chain(source_rails.iter().copied())
            .collect();
        let mut loads = Vec::new();
        let mut draw = vec![0.0; nodes];
        for (id, inst) in model.instances() {
            if inst.kind != InstKind::Interface {
                continue;
            }
            let Some((voltage, _, _)) = model.rail_of(id) else {
                continue;
            };
            // A rail of the part or the board itself: not the reference rail of a `Logic` or of
            // `Levels`, which carries nothing.
            let Some(owner) = inst.parent else {
                continue;
            };
            let Some(n) = pos_node(id) else {
                continue;
            };
            // A part's rail, or a board's own rail that a part between rails drives (a load the
            // board states on a regulator's output).
            let part = match model.instance(owner).kind {
                InstKind::Part => owner,
                InstKind::Board if driven.contains(&n) => id,
                _ => continue,
            };
            if let Some(c) = member_field(model, id, "current").and_then(|f| quantity(model, f))
                && c.hi().is_finite()
            {
                draw[n] += c.hi().max(0.0);
            }
            if own_rails.contains(&id) {
                continue;
            }
            let (least, most) = bounds_of(model, voltage);
            loads.push(Load {
                part,
                rail: id,
                node: n,
                least,
                most,
            });
        }
        Self {
            model,
            net_of,
            node,
            nodes,
            ground,
            converters,
            diodes,
            resistors,
            supplies,
            loads,
            draw,
        }
    }

    fn node_of(&self, pin: InstId) -> Option<usize> {
        self.net_of.get(&pin).map(|&k| self.node[k])
    }

    fn path(&self, inst: InstId) -> String {
        format!(".{}", self.model.path_of(inst))
    }

    /// The graph with the supplies `held` holding their nodes.
    fn propagate(&self, held: &[&Supply]) -> State {
        let mut v: Vec<Option<Range>> = vec![None; self.nodes];
        let mut given: Vec<Option<Given>> = vec![None; self.nodes];
        for s in held {
            if let Some(r) = s.voltage {
                v[s.node] = Range::max(v[s.node], Some(r));
                given[s.node] = Some(Given::Held);
            }
        }
        let mut limited = Vec::new();
        // Every range only grows and is bounded by the highest supply, so this ends; the bound
        // on the rounds is a guard.
        for _ in 0..(4 * self.nodes + 8) {
            let mut changed = false;
            let mut offer = |v: &mut Vec<Option<Range>>, n: usize, r: Option<Range>, why: Given| {
                let Some(r) = r else { return };
                let next = Range::max(v[n], Some(r));
                if let (Some(a), Some(b)) = (v[n], next)
                    && b.lo <= a.lo + 1e-12
                    && b.hi <= a.hi + 1e-12
                {
                    return;
                }
                if v[n].is_none_or(|a| r.hi > a.hi + 1e-12) {
                    given[n] = Some(why);
                }
                v[n] = next;
                changed = true;
            };
            for (k, c) in self.converters.iter().enumerate() {
                let forward = v[c.vin_node].and_then(|vin| match c.kind {
                    ConverterKind::Linear => vin
                        .less(c.dropout)
                        .map(|r| c.set.map_or(r, |s| r.capped(s))),
                    ConverterKind::Switching => Some(c.set.map_or(vin, |s| vin.capped(s))),
                    ConverterKind::Switch(on) => on.then_some(vin),
                });
                offer(&mut v, c.vout_node, forward, Given::Forward(k));
                let backward = v[c.vout_node].and_then(|out| self.backward(c, out));
                let backward = match (backward, c.back_limit) {
                    (Some(r), Some(limit)) if self.draw[c.vin_node] > limit => {
                        if !limited.contains(&k) {
                            limited.push(k);
                        }
                        Some(Range { lo: 0.0, hi: r.hi })
                    }
                    (r, _) => r,
                };
                offer(&mut v, c.vin_node, backward, Given::Backward(k));
            }
            for d in &self.diodes {
                let r = v[d.anode].and_then(|a| a.less(d.drop));
                offer(&mut v, d.cathode, r, Given::Diode);
            }
            if !changed {
                break;
            }
        }
        State { v, given, limited }
    }

    /// What a part's output gives its input backwards.
    fn backward(&self, c: &Converter, out: Range) -> Option<Range> {
        match (c.back, c.kind) {
            (Back::Blocking, _) => None,
            (Back::Diode(_), ConverterKind::Switch(true)) => Some(out),
            (Back::Diode(drop), _) => out.less(drop),
        }
    }

    /// How a part passes current backwards, for a message.
    fn backward_how(&self, c: &Converter) -> &'static str {
        match (c.back, c.kind) {
            (Back::Blocking, _) => "blocks it",
            (Back::Diode(_), ConverterKind::Switch(true)) => "through its on resistance",
            _ => "through its body diode",
        }
    }

    fn scenario(&self, q: &Scenario, faults: &mut Vec<Diagnostic>, notes: &mut Vec<NotEvaluated>) {
        let model = self.model;
        let fault = |faults: &mut Vec<Diagnostic>, span, message: String| {
            faults.push(Diagnostic {
                source: q.source,
                span,
                message: format!("in the scenario `{}`, {message}", q.name),
            });
        };
        // Each rail it names is a supply.
        let mut removed_nodes = HashSet::new();
        let mut sources: Vec<(&netsmith_check::ScenarioPath, &Supply)> = Vec::new();
        let mut ok = true;
        for (path, removed) in q
            .removed
            .iter()
            .map(|p| (p, true))
            .chain(q.sources.iter().map(|p| (p, false)))
        {
            let node = model
                .rail_of(path.inst)
                .and_then(|(_, pos, _)| self.node_of(pos));
            let supply = node.and_then(|n| self.supplies.iter().find(|s| s.node == n));
            let Some(supply) = supply else {
                let why = match node.and_then(|n| self.converters.iter().find(|c| c.vout_node == n))
                {
                    Some(c) => format!("`{}` drives it", self.path(c.part)),
                    None => "it is not a rail of the board, and no part gives it from outside (`dc source`)".to_string(),
                };
                fault(
                    faults,
                    path.span,
                    format!(
                        "`{}` is not a supply, so it cannot be {}: {why}",
                        path.written,
                        if removed { "removed" } else { "a source" }
                    ),
                );
                ok = false;
                continue;
            };
            if removed {
                removed_nodes.insert(supply.node);
            } else {
                sources.push((path, supply));
            }
        }
        for (path, s) in &sources {
            if removed_nodes.contains(&s.node) {
                fault(
                    faults,
                    path.span,
                    format!(
                        "the source `{}` is on the net of a supply the scenario removes: name the rail of the backup itself",
                        path.written
                    ),
                );
                ok = false;
            }
        }
        if !ok {
            return;
        }
        let held: Vec<&Supply> = self
            .supplies
            .iter()
            .filter(|s| !removed_nodes.contains(&s.node))
            .collect();
        let all = self.propagate(&held);
        let mut parts: Vec<String> = Vec::new();
        let removed: Vec<&str> = q.removed.iter().map(|p| p.written.as_str()).collect();
        parts.push(format!(
            "scenario `{}`, with {} removed",
            q.name,
            list_of(&removed.iter().map(|w| format!("`{w}`")).collect::<Vec<_>>())
        ));
        let mut judged_loads: HashSet<usize> = HashSet::new();
        for (path, supply) in &sources {
            let alone = self.propagate(&[supply]);
            let at = supply
                .voltage
                .map(|r| format!(" ({r})"))
                .unwrap_or_default();
            let mut said: Vec<String> = Vec::new();
            for (k, c) in self.converters.iter().enumerate() {
                let p = self.path(c.part);
                let back = alone.given[c.vin_node] == Some(Given::Backward(k));
                if back {
                    let vin = all.v[c.vin_node].map(|r| r.to_string()).unwrap_or_default();
                    let limit = if all.limited.contains(&k) {
                        format!(
                            ", but its input's loads draw more than its reverse limit, so `{}` may be at any voltage up to the highest",
                            self.path(c.vin)
                        )
                    } else {
                        String::new()
                    };
                    said.push(format!(
                        "reaches `{p}` backwards {} (`{}` at {vin}{limit})",
                        self.backward_how(c),
                        self.path(c.vin)
                    ));
                } else if alone.v[c.vout_node].is_some()
                    && alone.given[c.vout_node] != Some(Given::Forward(k))
                    && c.back == Back::Blocking
                    && alone.v[c.vin_node].is_none()
                {
                    said.push(format!("is blocked by `{p}`"));
                }
                if alone.given[c.vout_node] == Some(Given::Forward(k)) {
                    let vout = all.v[c.vout_node]
                        .map(|r| r.to_string())
                        .unwrap_or_default();
                    said.push(format!("drives `{p}` (`{}` at {vout})", self.path(c.vout)));
                }
            }
            let mut powered: Vec<String> = Vec::new();
            for (k, load) in self.loads.iter().enumerate() {
                if alone.v[load.node].is_none() {
                    continue;
                }
                let Some(v) = all.v[load.node] else {
                    continue;
                };
                let unbounded = if load.least.is_none() && load.most.is_none() {
                    " (which states no supply range)"
                } else {
                    ""
                };
                powered.push(format!("`{}` at {v}{unbounded}", self.path(load.rail)));
                judged_loads.insert(k);
            }
            let reach = if said.is_empty() {
                "reaches no regulator".to_string()
            } else {
                list_of(&said)
            };
            let powers = if powered.is_empty() {
                "it powers no load".to_string()
            } else {
                format!("it powers {}", list_of(&powered))
            };
            parts.push(format!("`{}`{at} {reach}; {powers}", path.written));
        }
        // A load that the graph powers outside its bounds, whoever powers it.
        for (k, load) in self.loads.iter().enumerate() {
            let Some(v) = all.v[load.node] else {
                continue;
            };
            if v.within(load.least, load.most) || (load.least.is_none() && load.most.is_none()) {
                continue;
            }
            let bounds = match (load.least, load.most) {
                (Some(l), Some(h)) => format!("{}..{}", volts(l), volts(h)),
                (Some(l), None) => format!("at least {}", volts(l)),
                (None, Some(h)) => format!("at most {}", volts(h)),
                (None, None) => unreachable!(),
            };
            let from = if judged_loads.contains(&k) {
                self.feed(&all, load.node)
            } else {
                String::new()
            };
            fault(
                faults,
                q.span,
                format!(
                    "`{}` is at {v}{from}, and `{}` needs {bounds} on it: the part is powered but not inside its supply range",
                    self.path(load.rail),
                    self.path(load.part)
                ),
            );
        }
        // The sense nets.
        for sense in &q.senses {
            let Some(n) = self.node_of(sense.inst) else {
                notes.push(NotEvaluated {
                    at: sense.inst,
                    involves: vec![sense.inst],
                    message: format!(
                        "in the scenario `{}`, `{}` is not judged: it is on no net",
                        q.name, sense.written
                    ),
                    cause: Cause::NotStated,
                });
                continue;
            };
            let v = match self.settle(&all, n) {
                Ok(v) => v,
                Err(why) => {
                    notes.push(NotEvaluated {
                        at: sense.inst,
                        involves: vec![sense.inst],
                        message: format!(
                            "in the scenario `{}`, where `{}` settles is not judged: {why}",
                            q.name, sense.written
                        ),
                        cause: Cause::Unjudged,
                    });
                    parts.push(format!("`{}` is not judged ({why})", sense.written));
                    continue;
                }
            };
            let mut reads: Vec<String> = Vec::new();
            for &pin in &model.nets()[self.node_net(sense.inst)].terminals {
                let Some(levels) = model.levels_of(pin) else {
                    continue;
                };
                match self.judge_pin(&all, levels, v) {
                    Judged::Low => reads.push(format!("low by {}", self.reader(pin, sense.inst))),
                    Judged::High => reads.push(format!("high by {}", self.reader(pin, sense.inst))),
                    Judged::Neither { vil, vih } => fault(
                        faults,
                        sense.span,
                        format!(
                            "`{}` settles at {v}, which {} reads as neither low (at most {}) nor high (at least {})",
                            sense.written,
                            self.reader(pin, sense.inst),
                            volts(vil),
                            volts(vih)
                        ),
                    ),
                    Judged::Unpowered(reference) => fault(
                        faults,
                        sense.span,
                        format!(
                            "`{}` settles at {v} while `{}`, the supply of `{}`, is off: the pin is driven above a part that has no supply",
                            sense.written,
                            self.path(reference),
                            self.path(pin)
                        ),
                    ),
                    Judged::Not(why) => notes.push(NotEvaluated {
                        at: pin,
                        involves: vec![pin],
                        message: format!(
                            "in the scenario `{}`, how `{}` reads `{}` at {v} is not judged: {why}",
                            q.name,
                            self.path(pin),
                            sense.written
                        ),
                        cause: Cause::Unjudged,
                    }),
                }
            }
            let read = if reads.is_empty() {
                String::new()
            } else {
                format!(", read as {}", list_of(&reads))
            };
            parts.push(format!("`{}` settles at {v}{read}", sense.written));
        }
        let Some(at) = q.sources.iter().chain(&q.removed).map(|p| p.inst).next() else {
            return;
        };
        notes.push(NotEvaluated {
            at,
            involves: q.sources.iter().map(|p| p.inst).collect(),
            message: format!("{}.", parts.join("; ")),
            cause: Cause::Other,
        });
    }

    /// `` `.mcu.det` ``, or `the pin` when it is the pin the scenario senses.
    fn reader(&self, pin: InstId, sensed: InstId) -> String {
        if pin == sensed {
            "the pin".to_string()
        } else {
            format!("`{}`", self.path(pin))
        }
    }

    /// The net of a pin, by its place in `Model::nets`.
    fn node_net(&self, pin: InstId) -> usize {
        self.net_of[&pin]
    }

    /// ` (from `.ldo` backwards)`: what gives a node its range, for a message.
    fn feed(&self, state: &State, node: usize) -> String {
        match state.given[node] {
            Some(Given::Backward(k)) => {
                let c = &self.converters[k];
                format!(
                    ", fed backwards {} of `{}`",
                    self.backward_how(c),
                    self.path(c.part)
                )
            }
            Some(Given::Forward(k)) => {
                let c = &self.converters[k];
                format!(", from `{}`", self.path(c.part))
            }
            _ => String::new(),
        }
    }

    /// Where a sense net on `node` settles.
    fn settle(&self, state: &State, node: usize) -> Result<Range, String> {
        if let Some(v) = state.v[node] {
            return Ok(v);
        }
        if self.ground[node] {
            return Ok(Range { lo: 0.0, hi: 0.0 });
        }
        // The nodes that are off that resistors join to this one, and the resistors.
        let mut free = vec![node];
        let mut used: Vec<usize> = Vec::new();
        let mut k = 0;
        while k < free.len() {
            let n = free[k];
            for (i, r) in self.resistors.iter().enumerate() {
                let far = if r.a == n {
                    r.b
                } else if r.b == n {
                    r.a
                } else {
                    continue;
                };
                if !used.contains(&i) {
                    used.push(i);
                }
                if state.v[far].is_none() && !self.ground[far] && !free.contains(&far) {
                    free.push(far);
                }
            }
            k += 1;
            if free.len() > MAX_FREE {
                return Err(format!(
                    "its resistors reach more than {MAX_FREE} nets that are off"
                ));
            }
        }
        // The boundary nodes: on, or ground.
        let boundary: Vec<usize> = {
            let mut b: Vec<usize> = used
                .iter()
                .flat_map(|&i| [self.resistors[i].a, self.resistors[i].b])
                .filter(|n| !free.contains(n))
                .collect();
            b.sort_unstable();
            b.dedup();
            b
        };
        if boundary.is_empty() {
            return Err(
                "no resistor joins it to a net that is on or to ground, so it floats".into(),
            );
        }
        // Each free node must reach the boundary, or the system is singular.
        let mut values: Vec<(f64, f64)> = used.iter().map(|&i| self.resistors[i].ohms).collect();
        let at = |n: usize| -> (f64, f64) {
            match state.v[n] {
                Some(r) => (r.lo, r.hi),
                None => (0.0, 0.0),
            }
        };
        values.extend(boundary.iter().map(|&n| at(n)));
        let varying: Vec<usize> = (0..values.len())
            .filter(|&i| values[i].0 != values[i].1)
            .collect();
        if varying.len() > MAX_CORNER_VALUES {
            return Err(format!(
                "its network has more than {MAX_CORNER_VALUES} values with a range"
            ));
        }
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for corner in 0..(1usize << varying.len()) {
            let pick = |i: usize| match varying.iter().position(|&j| j == i) {
                Some(bit) if corner >> bit & 1 == 1 => values[i].1,
                _ => values[i].0,
            };
            let nf = free.len();
            let mut g = vec![vec![0.0; nf + 1]; nf];
            for (u, &i) in used.iter().enumerate() {
                let r = &self.resistors[i];
                let cond = 1.0 / pick(u);
                for (end, other) in [(r.a, r.b), (r.b, r.a)] {
                    let Some(row) = free.iter().position(|&n| n == end) else {
                        continue;
                    };
                    g[row][row] += cond;
                    match free.iter().position(|&n| n == other) {
                        Some(col) => g[row][col] -= cond,
                        None => {
                            let b = boundary.iter().position(|&n| n == other).unwrap_or(0);
                            g[row][nf] += cond * pick(used.len() + b);
                        }
                    }
                }
            }
            let Some(x) = gauss(g) else {
                return Err(
                    "a net its resistors reach is joined to nothing that is on or ground, so it floats"
                        .into(),
                );
            };
            lo = lo.min(x[0]);
            hi = hi.max(x[0]);
        }
        Ok(Range {
            lo: lo.max(0.0),
            hi: hi.max(0.0),
        })
    }

    /// How a pin with the levels `levels` reads a net at `v`. A level worked out from the
    /// voltage of its reference rail (`.levels.vil &= 0.3 * .vcc.voltage`) is worked out again at
    /// the rail's voltage in the scenario.
    fn judge_pin(&self, state: &State, levels: InstId, v: Range) -> Judged {
        let model = self.model;
        let mut windows: Vec<(FieldId, Quantity)> = Vec::new();
        if let Some(reference) = member_inst(model, levels, "reference")
            && let Some((voltage, pos, _)) = model.rail_of(reference)
            && let Some(n) = self.node_of(pos)
        {
            let stated = quantity(model, voltage).and_then(|q| Range::of(&q));
            match state.v[n] {
                None if v.hi > 0.0 => return Judged::Unpowered(reference),
                None => return Judged::Low,
                Some(now) if stated != Some(now) => {
                    let Ok(q) = Quantity::interval(Dimension::VOLTAGE, now.lo, now.hi) else {
                        return Judged::Not(format!("its supply is at {now}"));
                    };
                    windows = self.rail_voltages_on(n).map(|f| (f, q)).collect();
                }
                Some(_) => {}
            }
        }
        let level = |name: &str| -> Result<Option<Range>, String> {
            let Some(f) = member_field(model, levels, name) else {
                return Ok(None);
            };
            let worked = model.relations().iter().find(|r| {
                matches!(r.op, CmpOp::Narrow | CmpOp::EqEq)
                    && matches!(r.lhs, RExpr::Field(g) if g == f)
            });
            match worked {
                Some(r) if !windows.is_empty() => match eval_with(model, &r.rhs, &windows) {
                    Ok(Val::Q(q)) => Ok(Range::of(&q)),
                    _ => Err(format!(
                        "its `{name}` could not be worked out at its supply's voltage in the scenario"
                    )),
                },
                _ => Ok(quantity(model, f).and_then(|q| Range::of(&q))),
            }
        };
        let (vil, vih) = match (level("vil"), level("vih")) {
            (Ok(Some(vil)), Ok(Some(vih))) => (vil, vih),
            (Err(why), _) | (_, Err(why)) => return Judged::Not(why),
            _ => return Judged::Not("its levels state no `vil` or no `vih`".into()),
        };
        if v.hi <= vil.lo + 1e-12 {
            Judged::Low
        } else if v.lo >= vih.hi - 1e-12 {
            Judged::High
        } else {
            Judged::Neither {
                vil: vil.lo,
                vih: vih.hi,
            }
        }
    }

    /// The `.voltage` of every rail whose `pos` is on `node`.
    fn rail_voltages_on(&self, node: usize) -> impl Iterator<Item = FieldId> + '_ {
        self.model.instances().filter_map(move |(id, _)| {
            let (voltage, pos, _) = self.model.rail_of(id)?;
            (self.node_of(pos) == Some(node)).then_some(voltage)
        })
    }
}

enum Judged {
    Low,
    High,
    Neither {
        vil: f64,
        vih: f64,
    },
    /// The pin's supply, this rail, is off.
    Unpowered(InstId),
    Not(String),
}

/// The bounds a part puts on a rail's voltage by numbers alone: `.vcc.voltage >= 2.4V` and
/// `.vcc.voltage <= 5.5V`, the greatest least and the least greatest.
fn bounds_of(model: &Model, voltage: FieldId) -> (Option<f64>, Option<f64>) {
    let (mut least, mut most): (Option<f64>, Option<f64>) = (None, None);
    for r in model.relations() {
        let (field, op, value) = match (&r.lhs, r.op, &r.rhs) {
            (RExpr::Field(f), op, RExpr::Const(c)) => (*f, op, c),
            (RExpr::Const(c), CmpOp::Ge, RExpr::Field(f)) => (*f, CmpOp::Le, c),
            (RExpr::Const(c), CmpOp::Le, RExpr::Field(f)) => (*f, CmpOp::Ge, c),
            _ => continue,
        };
        if field != voltage || value.dim() != Dimension::VOLTAGE {
            continue;
        }
        match op {
            CmpOp::Ge => least = Some(least.map_or(value.hi(), |l| l.max(value.hi()))),
            CmpOp::Le => most = Some(most.map_or(value.lo(), |m| m.min(value.lo()))),
            _ => {}
        }
    }
    (least, most)
}

/// `a`, `a and b`, `a, b and c`.
fn list_of(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Solves `g` (rows of `n` coefficients and the right-hand side) by Gaussian elimination with
/// partial pivoting; none when it is singular.
fn gauss(mut g: Vec<Vec<f64>>) -> Option<Vec<f64>> {
    let n = g.len();
    for col in 0..n {
        let pivot = (col..n).max_by(|&a, &b| g[a][col].abs().total_cmp(&g[b][col].abs()))?;
        if g[pivot][col].abs() < 1e-18 {
            return None;
        }
        g.swap(col, pivot);
        for row in 0..n {
            if row == col {
                continue;
            }
            let f = g[row][col] / g[col][col];
            if f == 0.0 {
                continue;
            }
            for k in col..=n {
                g[row][k] -= f * g[col][k];
            }
        }
    }
    Some((0..n).map(|i| g[i][n] / g[i][i]).collect())
}
