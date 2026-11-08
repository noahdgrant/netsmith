//! The netlist of the picked parts, cut into islands (ADR 0076, sections 2 and 3).
//!
//! What is read from the model:
//!
//! - **The nets** the checker made (the pins that `~`, `~>` and each `bridge` join). A net is a
//!   node.
//! - **Sources.** A `Rail` that the board itself declares (its supply from outside), with a
//!   voltage that the design states and that is finite, is an ideal source between its `pos`
//!   and `neg` nets. Its nets are *held*: the `neg` net at zero, the `pos` net over the range of
//!   the voltage. A rail of a part or of a module is no source: a part's rail is what the part
//!   draws, and a module's is a port whose range the design may have written as a seed or a
//!   promise. Treating such a range as a source would let the voltage of a node move
//!   independently of the current that makes it, which is not a circuit.
//! - **Elements.** A part with a `dc resistor` or a `dc diode` (stage 1 of the ADR) whose value
//!   has a range: a resistor between its two terminals, a diode between its anode and cathode.
//!   A part that has a `bridge` and no element the pass models (a boundary: an LED, an inductor
//!   with no `dc`, a MOSFET, whose element is a later stage) with a stated or worked-out
//!   `.current`: a current source over that range between the terminals of its `bridge`. A
//!   rail of a part with a `.current` that is never negative (a load): a current source from its
//!   `pos` to its `neg`. A rail of a part that can give current is an output, which holds its
//!   net's voltage and is not modelled in this stage.
//! - **What draws nothing.** A pin that the checker says draws nothing (ADR 0027) and is not
//!   on a connector, and a pin that connects to nothing.
//!
//! A net that no source holds is *complete* when every part pin on it is one of the above.
//! Anything else (a pin that drives, a pin with no kind, a part whose element has no value yet)
//! may set the net's voltage or draw from it, so the net is open and so is the island it is in:
//! it is left to the cheap bound. Nothing is guessed.
//!
//! An island is the free nets that elements join, cut at the held nets: a held net is a source's
//! node, so what is on its two sides is solved on its own, with the source's range as one of the
//! island's values (a cut at a node whose voltage a source holds loses nothing). An island that
//! touches nets measured from different grounds, that touches a net that two rails hold as a
//! positive and as a ground, or that touches no held net is left alone.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use netsmith_check::{DcKind, Domain, FieldId, InstId, InstKind, Member, Model, PinKind};
use netsmith_quantity::Dimension;

use super::net::{Element, Held, Island, Kind, MIN_RESISTANCE, Range};

/// A part of an island that the pass writes the results of.
#[derive(Debug, Clone)]
pub(crate) struct Modelled {
    /// The index of its element in the island.
    pub element: usize,
    pub part: InstId,
    /// Whether the part's `bridge` runs against the element's own direction (a resistor's
    /// first terminal to its second, a diode's anode to cathode): the current and the drop
    /// change sign.
    pub flipped: bool,
    pub current: Option<FieldId>,
    pub drop: Option<FieldId>,
    /// A diode: its current is never below zero in its own direction.
    pub diode: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Planned {
    pub island: Island,
    pub parts: Vec<Modelled>,
}

/// What the held nets are measured from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Hold {
    /// The `neg` of a source: zero.
    Ground,
    /// The `pos` of a source, measured from this ground net.
    Pos(usize),
    /// Held two ways: a net that is a ground and a positive, or a positive over two grounds.
    Conflict,
}

struct Held_ {
    hold: Hold,
    range: Range,
    name: String,
}

fn hull(a: Range, b: Range) -> Range {
    Range {
        lo: a.lo.min(b.lo),
        hi: a.hi.max(b.hi),
    }
}

/// An element before it is placed in an island, with the nets it touches.
struct Item {
    kind: Kind,
    name: String,
    part: Option<InstId>,
    flipped: bool,
    current: Option<FieldId>,
    drop: Option<FieldId>,
    /// The nets at its ends, in the order of the kind's fields.
    nets: Vec<usize>,
}

/// The islands of a model, in the order of their lowest net.
pub(crate) fn extract(model: &Model) -> Vec<Planned> {
    let nets = model.nets();
    let mut net_of: HashMap<InstId, usize> = HashMap::new();
    for (i, n) in nets.iter().enumerate() {
        for &p in &n.pins {
            net_of.insert(p, i);
        }
    }

    // The sources and the nets they hold.
    let mut held: BTreeMap<usize, Held_> = BTreeMap::new();
    let mut source_pins: HashSet<InstId> = HashSet::new();
    for (iface, inst) in model.instances() {
        if inst.kind != InstKind::Interface {
            continue;
        }
        let Some((voltage, pos, neg)) = model.rail_of(iface) else {
            continue;
        };
        let Some(parent) = inst.parent else { continue };
        if model.instance(parent).kind != InstKind::Board {
            continue;
        }
        let field = model.field(voltage);
        let Domain::Quantity(q) = &field.domain else {
            continue;
        };
        // A voltage the design states: not one the solver worked out from a relation, which
        // can be a node that a circuit sets.
        if field.narrowed_at.is_empty() || !q.lo().is_finite() || !q.hi().is_finite() {
            continue;
        }
        let (Some(&p), Some(&n)) = (net_of.get(&pos), net_of.get(&neg)) else {
            continue;
        };
        source_pins.insert(pos);
        source_pins.insert(neg);
        let name = model.path_of(iface);
        let range = Range::new(q.lo(), q.hi());
        hold_net(&mut held, n, Hold::Ground, Range::point(0.0), &name);
        if p == n {
            hold_net(&mut held, p, Hold::Conflict, range, &name);
        } else {
            hold_net(&mut held, p, Hold::Pos(n), range, &name);
        }
    }

    let mut items: Vec<Item> = Vec::new();
    // The pins the elements and sources account for.
    let mut accounted: HashSet<InstId> = HashSet::new();

    // Resistors and diodes.
    let mut modelled_parts: HashSet<InstId> = HashSet::new();
    for (part, dc) in model.dc_elements() {
        if model.instance(part).kind != InstKind::Part {
            continue;
        }
        let value = |f: FieldId, dim: Dimension| match &model.field(f).domain {
            Domain::Quantity(q) if q.dim() == dim && q.lo().is_finite() && q.hi().is_finite() => {
                Some(Range::new(q.lo(), q.hi()))
            }
            _ => None,
        };
        let net = |pin: InstId| net_of.get(&pin).copied();
        let (kind, nets, own) = match dc.kind {
            DcKind::Resistor => {
                let Some(r) = value(dc.values[0], Dimension::RESISTANCE) else {
                    continue;
                };
                // A resistance that cannot be above zero is no resistor; one that can be zero
                // is taken as the least conductance the matrix can hold.
                if r.hi <= 0.0 {
                    continue;
                }
                let r = Range::new(r.lo.max(MIN_RESISTANCE), r.hi.max(MIN_RESISTANCE));
                let (Some(a), Some(b)) = (net(dc.pins[0]), net(dc.pins[1])) else {
                    continue;
                };
                (
                    Kind::Resistor { a: 0, b: 0, r },
                    vec![a, b],
                    [dc.pins[0], dc.pins[1]],
                )
            }
            DcKind::Diode => {
                let Some(vf) = value(dc.values[0], Dimension::VOLTAGE) else {
                    continue;
                };
                let (Some(a), Some(k)) = (net(dc.pins[0]), net(dc.pins[1])) else {
                    continue;
                };
                (
                    Kind::Diode {
                        anode: 0,
                        cathode: 0,
                        vf,
                    },
                    vec![a, k],
                    [dc.pins[0], dc.pins[1]],
                )
            }
            // A MOSFET as a switch is a later stage: the part is a boundary here.
            DcKind::Nmos | DcKind::Pmos => continue,
        };
        // A diode between two held nets has no current the network can say (two ideal
        // sources and an ideal diode): it is left out, and so is nothing it touches that is
        // free.
        if matches!(kind, Kind::Diode { .. }) && nets.iter().all(|n| held.contains_key(n)) {
            accounted.extend(own);
            modelled_parts.insert(part);
            continue;
        }
        let bridge = model.instance(part).bridge;
        let flipped = bridge.is_some_and(|(first, _)| first == own[1] && first != own[0]);
        accounted.extend(own);
        modelled_parts.insert(part);
        items.push(Item {
            kind,
            name: model.path_of(part),
            part: Some(part),
            flipped,
            current: model.through_of(part),
            drop: match model.member(part, "drop") {
                Some(Member::Field(f))
                    if model.field(f).kind
                        == netsmith_check::FieldKind::Quantity(Dimension::VOLTAGE) =>
                {
                    Some(f)
                }
                _ => None,
            },
            nets,
        });
    }

    // A part with a `bridge` and no element: a current source over its `.current`.
    for (part, inst) in model.instances() {
        if inst.kind != InstKind::Part || modelled_parts.contains(&part) {
            continue;
        }
        let Some((a, b)) = inst.bridge else { continue };
        let Some(current) = model.through_of(part) else {
            continue;
        };
        let Domain::Quantity(q) = &model.field(current).domain else {
            continue;
        };
        if q.dim() != Dimension::CURRENT || !q.lo().is_finite() || !q.hi().is_finite() {
            continue;
        }
        let (Some(&na), Some(&nb)) = (net_of.get(&a), net_of.get(&b)) else {
            continue;
        };
        accounted.insert(a);
        accounted.insert(b);
        items.push(Item {
            kind: Kind::Current {
                from: 0,
                to: 0,
                i: Range::new(q.lo(), q.hi()),
            },
            name: model.path_of(part),
            part: None,
            flipped: false,
            current: None,
            drop: None,
            nets: vec![na, nb],
        });
    }

    // The pins of nets that a board's ports put on them and no source holds: what the outside
    // draws from a rail of the board (a load when it never gives current), and any other port,
    // which something outside may drive. A module's port adds nothing: what is inside it is
    // part pins, which are looked at themselves.
    let mut outside_open: BTreeSet<usize> = BTreeSet::new();
    for (iface, inst) in model.instances() {
        if inst.kind != InstKind::Interface {
            continue;
        }
        let Some(owner) = model.owner_of(iface) else {
            continue;
        };
        if model.instance(owner).kind != InstKind::Board {
            continue;
        }
        let is_rail = model.rail_of(iface);
        let current = model
            .draw_of(iface)
            .and_then(|(f, _)| match &model.field(f).domain {
                Domain::Quantity(q)
                    if q.dim() == Dimension::CURRENT
                        && q.lo().is_finite()
                        && q.hi().is_finite()
                        && q.lo() >= 0.0 =>
                {
                    Some(Range::new(q.lo(), q.hi()))
                }
                _ => None,
            });
        match (is_rail, current) {
            (Some((_, pos, neg)), _)
                if source_pins.contains(&pos) && source_pins.contains(&neg) => {}
            (Some((_, pos, neg)), Some(i)) => {
                if let (Some(&np), Some(&nn)) = (net_of.get(&pos), net_of.get(&neg)) {
                    items.push(Item {
                        kind: Kind::Current { from: 0, to: 0, i },
                        name: model.path_of(iface),
                        part: None,
                        flipped: false,
                        current: None,
                        drop: None,
                        nets: vec![np, nn],
                    });
                    accounted.insert(pos);
                    accounted.insert(neg);
                } else {
                    outside_open.extend(net_of.get(&pos).copied());
                }
            }
            _ => {}
        }
    }
    // Every pin of a board's own ports that is not one of those above.
    for (index, net) in nets.iter().enumerate() {
        for &p in &net.pins {
            if net.terminals.contains(&p) || accounted.contains(&p) || source_pins.contains(&p) {
                continue;
            }
            let Some(owner) = model.owner_of(p) else {
                continue;
            };
            if model.instance(owner).kind == InstKind::Board {
                outside_open.insert(index);
            }
        }
    }

    // The rails of the parts: what the part draws from `pos`, which comes back at `neg`.
    for (iface, inst) in model.instances() {
        if inst.kind != InstKind::Interface {
            continue;
        }
        let Some((_, pos, neg)) = model.rail_of(iface) else {
            continue;
        };
        let Some(owner) = model.owner_of(iface) else {
            continue;
        };
        if model.instance(owner).kind != InstKind::Part {
            continue;
        }
        let Some((current, _)) = model.draw_of(iface) else {
            continue;
        };
        let Domain::Quantity(q) = &model.field(current).domain else {
            continue;
        };
        // A part's rail is a load only when it draws: a range that has the part giving current
        // is an output (a regulator's), which holds its net's voltage and is no current
        // source, so the net stays open.
        if q.dim() != Dimension::CURRENT
            || !q.lo().is_finite()
            || !q.hi().is_finite()
            || q.lo() < 0.0
        {
            continue;
        }
        let (Some(&np), Some(&nn)) = (net_of.get(&pos), net_of.get(&neg)) else {
            continue;
        };
        accounted.insert(pos);
        accounted.insert(neg);
        items.push(Item {
            kind: Kind::Current {
                from: 0,
                to: 0,
                i: Range::new(q.lo(), q.hi()),
            },
            name: model.path_of(iface),
            part: None,
            flipped: false,
            current: None,
            drop: None,
            nets: vec![np, nn],
        });
    }

    // The nets some pin on them may set or draw from, that nothing here models.
    let mut open: BTreeSet<usize> = outside_open;
    for (index, net) in nets.iter().enumerate() {
        if held.contains_key(&index) {
            continue;
        }
        for &t in &net.terminals {
            if accounted.contains(&t) || source_pins.contains(&t) {
                continue;
            }
            let Some(part) = model.owner_of(t) else {
                open.insert(index);
                break;
            };
            let connector = model.member(part, "mating").is_some();
            let draws_nothing = !connector && model.pin_draws_nothing(t);
            let unused = model.pin_kind(t) == Some(PinKind::NoConnect);
            if !(draws_nothing || unused) {
                open.insert(index);
                break;
            }
        }
    }

    // The islands: free nets that elements join. A held net is not merged into anything.
    let mut parent: Vec<usize> = (0..nets.len()).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            let root = find(parent, parent[x]);
            parent[x] = root;
        }
        parent[x]
    }
    for item in &items {
        let free: Vec<usize> = item
            .nets
            .iter()
            .copied()
            .filter(|n| !held.contains_key(n))
            .collect();
        for w in free.windows(2) {
            let (ra, rb) = (find(&mut parent, w[0]), find(&mut parent, w[1]));
            if ra != rb {
                parent[ra.max(rb)] = ra.min(rb);
            }
        }
    }
    // Each item belongs to the island of its first free net, or stands alone when all its
    // nets are held.
    let mut groups: BTreeMap<(u8, usize), Vec<usize>> = BTreeMap::new();
    for (i, item) in items.iter().enumerate() {
        let key = match item.nets.iter().find(|n| !held.contains_key(n)) {
            Some(&n) => (0, find(&mut parent, n)),
            None => (1, i),
        };
        groups.entry(key).or_default().push(i);
    }

    let mut out = Vec::new();
    for members in groups.values() {
        netsmith_core::job::checkpoint();
        // Only an island with something to write is worth a solve.
        if members.iter().all(|&i| items[i].part.is_none()) {
            continue;
        }
        let touched: BTreeSet<usize> = members
            .iter()
            .flat_map(|&i| items[i].nets.iter().copied())
            .collect();
        let free_nets: Vec<usize> = touched
            .iter()
            .copied()
            .filter(|n| !held.contains_key(n))
            .collect();
        let held_nets: Vec<usize> = touched
            .iter()
            .copied()
            .filter(|n| held.contains_key(n))
            .collect();
        if held_nets.is_empty() || free_nets.iter().any(|n| open.contains(n)) {
            continue;
        }
        // One ground for everything the island is measured from.
        let mut ground: Option<usize> = None;
        let mut consistent = true;
        for n in &held_nets {
            let g = match held[n].hold {
                Hold::Ground => *n,
                Hold::Pos(g) => g,
                Hold::Conflict => {
                    consistent = false;
                    break;
                }
            };
            match ground {
                None => ground = Some(g),
                Some(prev) if prev != g => {
                    consistent = false;
                    break;
                }
                _ => {}
            }
        }
        if !consistent {
            continue;
        }
        let node_of = |net: usize| -> usize {
            match free_nets.iter().position(|&n| n == net) {
                Some(i) => i,
                None => {
                    free_nets.len() + held_nets.iter().position(|&n| n == net).unwrap_or_default()
                }
            }
        };
        let mut island = Island {
            free: free_nets.len(),
            held: held_nets
                .iter()
                .map(|n| Held {
                    name: held[n].name.clone(),
                    v: held[n].range,
                })
                .collect(),
            elements: Vec::new(),
        };
        let mut parts = Vec::new();
        for &i in members {
            let item = &items[i];
            let n: Vec<usize> = item.nets.iter().map(|&x| node_of(x)).collect();
            let kind = match &item.kind {
                Kind::Resistor { r, .. } => Kind::Resistor {
                    a: n[0],
                    b: n[1],
                    r: *r,
                },
                Kind::Diode { vf, .. } => Kind::Diode {
                    anode: n[0],
                    cathode: n[1],
                    vf: *vf,
                },
                Kind::Current { i, .. } => Kind::Current {
                    from: n[0],
                    to: n[1],
                    i: *i,
                },
            };
            if let Some(part) = item.part {
                parts.push(Modelled {
                    element: island.elements.len(),
                    part,
                    flipped: item.flipped,
                    current: item.current,
                    drop: item.drop,
                    diode: matches!(item.kind, Kind::Diode { .. }),
                });
            }
            island.elements.push(Element {
                name: item.name.clone(),
                kind,
            });
        }
        out.push(Planned { island, parts });
    }
    out
}

fn hold_net(held: &mut BTreeMap<usize, Held_>, net: usize, hold: Hold, range: Range, name: &str) {
    match held.get_mut(&net) {
        None => {
            held.insert(
                net,
                Held_ {
                    hold,
                    range,
                    name: name.to_string(),
                },
            );
        }
        Some(h) => {
            if h.hold == hold && hold != Hold::Conflict {
                h.range = hull(h.range, range);
            } else {
                h.hold = Hold::Conflict;
            }
        }
    }
}
