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
//!   with no `dc`, a MOSFET whose gate nothing sets) with a stated or worked-out
//!   `.current`: a current source over that range between the terminals of its `bridge`. A
//!   rail of a part with a `.current` that is never negative (a load): a current source from its
//!   `pos` to its `neg`. A rail of a part that can give current is an output, which holds its
//!   net's voltage and is not modelled in this stage.
//! - **Regulators** (`dc linear_regulator`, `dc switching_regulator`, ADR 0076 amendment 3). The
//!   output rail is an ideal source over the range of its voltage: its positive net is held over
//!   that range and its negative net at zero, so the output side is an island of its own (a board
//!   rail on that net is then a load, not a source). The input rail is an element, a current
//!   that the regulator's output sets: it joins the island of the input net, and the order in
//!   which islands are solved (`super::analyze`) is the order of the power tree, the output
//!   side first.
//! - **Logic pins that drive** (ADR 0076, amendment 4): a pin with `voh` and `vol` stated at
//!   their test currents, on a supply that a source holds, alone on its net, is an element with
//!   its states. **MOSFETs** (`dc nmos`, `dc pmos`) are switches when what sets their gate is
//!   known (see `extract`); otherwise they are boundaries, as before.
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

use netsmith_check::{DcKind, Domain, FieldId, InstId, InstKind, Member, Model, PinKind, RExpr};
use netsmith_lang::ast::BinOp;
use netsmith_quantity::Dimension;

use super::net::{Draw, Element, Held, Island, Kind, MIN_RESISTANCE, Range};

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

/// A regulator of the model: the part, the net its output holds, and what it says of itself.
#[derive(Debug, Clone)]
pub(crate) struct Reg {
    pub part: InstId,
    /// The path of the output rail, for the held node's name.
    pub name: String,
    /// The net the output holds, and the range of its voltage.
    pub out_net: usize,
    pub vout: Range,
    /// `Some` for a switching regulator.
    pub eff: Option<Range>,
    /// The current its output rail supplies (a negative `current`), to fall back to when the
    /// loads on the output net are not all modelled.
    pub out_current: Option<FieldId>,
    /// Whether every pin on the output net is one the pass accounts for.
    pub out_complete: bool,
}

/// What the island's regulator inputs are: the element, and which regulator it is.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Sink {
    pub element: usize,
    pub reg: usize,
}

/// A held node of the island that a regulator's output holds: its index in `Island::held`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputNode {
    pub held: usize,
    pub reg: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct Planned {
    pub island: Island,
    pub parts: Vec<Modelled>,
    pub sinks: Vec<Sink>,
    pub outputs: Vec<OutputNode>,
}

/// What `extract` found: the islands, the regulators, and the regulator outputs that an island
/// was left out of (so their load is not the sum of the islands).
pub(crate) struct Extracted {
    pub islands: Vec<Planned>,
    pub regs: Vec<Reg>,
    pub unsolved_outputs: BTreeSet<usize>,
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
    /// A regulator's input: which regulator.
    reg: Option<usize>,
}

/// The islands of a model, in the order of their lowest net.
pub(crate) fn extract(model: &Model) -> Extracted {
    let nets = model.nets();
    let mut net_of: HashMap<InstId, usize> = HashMap::new();
    for (i, n) in nets.iter().enumerate() {
        for &p in &n.pins {
            net_of.insert(p, i);
        }
    }

    // The regulators: what each holds on its output net, and what its input is.
    let mut regs: Vec<Reg> = Vec::new();
    // For each regulator: its part, the nets and fields of its input, and its pins.
    struct RegInput {
        net_pos: usize,
        net_neg: usize,
        current: Option<FieldId>,
        voltage: FieldId,
    }
    let mut reg_inputs: Vec<RegInput> = Vec::new();
    let mut reg_rails: HashSet<InstId> = HashSet::new();
    let mut reg_pins: Vec<[InstId; 4]> = Vec::new();
    let mut out_nets: Vec<(usize, usize)> = Vec::new();
    for (part, dc) in model.dc_elements() {
        let switching = match dc.kind {
            DcKind::LinearRegulator => false,
            DcKind::SwitchingRegulator => true,
            _ => continue,
        };
        if model.instance(part).kind != InstKind::Part {
            continue;
        }
        let (Some((v_in, in_pos, in_neg)), Some((v_out, out_pos, out_neg))) =
            (model.rail_of(dc.pins[0]), model.rail_of(dc.pins[1]))
        else {
            continue;
        };
        let nets_of = [in_pos, in_neg, out_pos, out_neg].map(|p| net_of.get(&p).copied());
        let [Some(ip), Some(ineg), Some(op), Some(oneg)] = nets_of else {
            continue;
        };
        // One ground for the input and the output, and an output net of its own.
        if ineg != oneg || ip == op || ip == ineg || op == oneg {
            continue;
        }
        let range = |f: FieldId, dim: Dimension| match &model.field(f).domain {
            Domain::Quantity(q) if q.dim() == dim && q.lo().is_finite() && q.hi().is_finite() => {
                Some(Range::new(q.lo(), q.hi()))
            }
            _ => None,
        };
        let Some(vout) = range(v_out, Dimension::VOLTAGE).filter(|r| r.lo > 0.0) else {
            continue;
        };
        let eff = if switching {
            match range(dc.values[0], Dimension::NONE).filter(|r| r.lo > 0.0) {
                Some(e) => Some(e),
                None => continue,
            }
        } else {
            None
        };
        // The input's `voltage` and `current`: the fields the pass writes.
        if model.field(v_in).kind != netsmith_check::FieldKind::Quantity(Dimension::VOLTAGE) {
            continue;
        }
        let current = model.draw_of(dc.pins[0]).map(|(f, _)| f);
        let out_current = model.draw_of(dc.pins[1]).map(|(f, _)| f);
        regs.push(Reg {
            part,
            name: model.path_of(dc.pins[1]),
            out_net: op,
            vout,
            eff,
            out_current,
            out_complete: false,
        });
        reg_inputs.push(RegInput {
            net_pos: ip,
            net_neg: ineg,
            current,
            voltage: v_in,
        });
        reg_rails.insert(dc.pins[0]);
        reg_rails.insert(dc.pins[1]);
        reg_pins.push([in_pos, in_neg, out_pos, out_neg]);
        out_nets.push((op, oneg));
    }
    // Two regulators on one output net share a load that no sum can tell apart: neither is
    // modelled.
    let claimed: Vec<usize> = regs.iter().map(|r| r.out_net).collect();
    let keep: Vec<bool> = claimed
        .iter()
        .map(|n| claimed.iter().filter(|m| *m == n).count() == 1)
        .collect();
    let mut k = 0;
    regs.retain(|_| {
        k += 1;
        keep[k - 1]
    });
    let mut k = 0;
    reg_inputs.retain(|_| {
        k += 1;
        keep[k - 1]
    });
    let mut k = 0;
    reg_pins.retain(|_| {
        k += 1;
        keep[k - 1]
    });
    let mut k = 0;
    out_nets.retain(|_| {
        k += 1;
        keep[k - 1]
    });
    let reg_out_nets: BTreeSet<usize> = regs.iter().map(|r| r.out_net).collect();
    let modelled_regs: HashSet<InstId> = regs.iter().map(|r| r.part).collect();
    reg_rails.retain(|i| {
        model
            .owner_of(*i)
            .is_some_and(|p| modelled_regs.contains(&p))
    });

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
        // A rail on a net that a regulator holds is what the regulator supplies, whatever
        // voltage the design asks of it: a load, not a second source.
        if reg_out_nets.contains(&p) {
            continue;
        }
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

    // The regulators: the output holds its net, the input is an element.
    for (idx, reg) in regs.iter().enumerate() {
        let (_, neg) = out_nets[idx];
        hold_net(&mut held, neg, Hold::Ground, Range::point(0.0), &reg.name);
        hold_net(&mut held, reg.out_net, Hold::Pos(neg), reg.vout, &reg.name);
        accounted.extend(reg_pins[idx]);
        let input = &reg_inputs[idx];
        items.push(Item {
            kind: Kind::Regulator {
                from: 0,
                to: 0,
                draw: match reg.eff {
                    Some(eff) => Draw::Switching {
                        p: Range::EMPTY,
                        eff,
                    },
                    None => Draw::Linear { i: Range::EMPTY },
                },
            },
            name: model.path_of(reg.part),
            part: Some(reg.part),
            flipped: false,
            current: input.current,
            drop: Some(input.voltage),
            nets: vec![input.net_pos, input.net_neg],
            reg: Some(idx),
        });
    }

    // Resistors and diodes.
    let mut modelled_parts: HashSet<InstId> = HashSet::new();
    // The MOSFETs are decided after everything else is known (below).
    let mut switches: Vec<(InstId, &netsmith_check::DcElement)> = Vec::new();
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
            // A MOSFET as a switch is decided below, when the nets its gate can be on are known.
            DcKind::Nmos | DcKind::Pmos => {
                switches.push((part, dc));
                continue;
            }
            // Regulators are taken above.
            DcKind::LinearRegulator | DcKind::SwitchingRegulator => continue,
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
            reg: None,
        });
    }

    // The logic pins that drive: each is one of its states (high, low, high impedance) through
    // its output resistance, which the level and the current it is measured at give (ADR 0076,
    // amendment 4).
    let reader = crate::levels::reader(model);
    let mut drivers: Vec<Driver> = Vec::new();
    for (index, net) in nets.iter().enumerate() {
        if held.contains_key(&index) {
            continue;
        }
        for &t in &net.terminals {
            if accounted.contains(&t) || source_pins.contains(&t) {
                continue;
            }
            if let Some(d) = driver_of(model, &reader, t, index, &net_of, &held) {
                drivers.push(d);
            }
        }
    }
    // Two pins that can drive one net may drive it apart: the states that fight are no circuit
    // anyone designs, and which pairs are never driven together is not stated, so none of them
    // is modelled and the net stays open.
    let mut per_net: HashMap<usize, usize> = HashMap::new();
    for d in &drivers {
        *per_net.entry(d.net).or_default() += 1;
    }
    for d in drivers.into_iter().filter(|d| per_net[&d.net] == 1) {
        accounted.insert(d.terminal);
        items.push(Item {
            kind: Kind::Pin {
                node: 0,
                high: d.high.map(|r| (0, r)),
                low: d.low.map(|r| (0, r)),
                z: d.z,
            },
            name: model.path_of(d.terminal),
            part: None,
            flipped: false,
            current: None,
            drop: None,
            nets: vec![d.net, d.pos, d.neg],
            reg: None,
        });
    }

    // A part with a `bridge` and no element: a current source over its `.current`.
    let switch_parts: HashSet<InstId> = switches.iter().map(|(p, _)| *p).collect();
    let boundary = |part: InstId| -> Option<(Item, InstId, InstId)> {
        let (a, b) = model.instance(part).bridge?;
        let current = model.through_of(part)?;
        let Domain::Quantity(q) = &model.field(current).domain else {
            return None;
        };
        if q.dim() != Dimension::CURRENT || !q.lo().is_finite() || !q.hi().is_finite() {
            return None;
        }
        let (Some(&na), Some(&nb)) = (net_of.get(&a), net_of.get(&b)) else {
            return None;
        };
        Some((
            Item {
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
                reg: None,
            },
            a,
            b,
        ))
    };
    for (part, inst) in model.instances() {
        if inst.kind != InstKind::Part
            || modelled_parts.contains(&part)
            || switch_parts.contains(&part)
        {
            continue;
        }
        if let Some((item, a, b)) = boundary(part) {
            accounted.insert(a);
            accounted.insert(b);
            items.push(item);
        }
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
                        reg: None,
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
        if reg_rails.contains(&iface) {
            continue;
        }
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
            reg: None,
        });
    }

    // The nets some pin on them may set or draw from, that nothing here models.
    let open_nets = |accounted: &HashSet<InstId>| -> BTreeSet<usize> {
        let mut open: BTreeSet<usize> = outside_open.clone();
        for (index, net) in nets.iter().enumerate() {
            if held.contains_key(&index) && !reg_out_nets.contains(&index) {
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
        open
    };

    // The MOSFETs as switches. One is modelled when what sets its gate is known: the gate's net
    // is held, or a resistor, a diode or a pin that drives is on it, and nothing else on that
    // net may set it. A switch whose gate is left to something unmodelled stays a boundary (a
    // current source over its `.current` when that has a value), so that the island of its drain
    // and source is solved as it was.
    let open: BTreeSet<usize> = {
        let mut tentative = accounted.clone();
        for (_, dc) in &switches {
            tentative.extend(dc.pins.iter().copied());
        }
        let open_if_all = open_nets(&tentative);
        let sets_gate = |net: usize, items: &[Item]| {
            held.contains_key(&net)
                || items.iter().any(|i| {
                    matches!(
                        i.kind,
                        Kind::Resistor { .. } | Kind::Diode { .. } | Kind::Pin { .. }
                    ) && i.nets.contains(&net)
                })
        };
        let mut chosen: Vec<Item> = Vec::new();
        let mut taken: Vec<[InstId; 3]> = Vec::new();
        let mut fallback: Vec<InstId> = Vec::new();
        for &(part, dc) in &switches {
            let value = |f: FieldId, dim: Dimension| match &model.field(f).domain {
                Domain::Quantity(q)
                    if q.dim() == dim && q.lo().is_finite() && q.hi().is_finite() =>
                {
                    Some(Range::new(q.lo(), q.hi()))
                }
                _ => None,
            };
            let net = |pin: InstId| net_of.get(&pin).copied();
            let modelled = (|| {
                let vth = value(dc.values[0], Dimension::VOLTAGE)?;
                let ron = value(dc.values[1], Dimension::RESISTANCE).filter(|r| r.hi > 0.0)?;
                let ron = Range::new(ron.lo.max(MIN_RESISTANCE), ron.hi.max(MIN_RESISTANCE));
                let (g, d, s) = (net(dc.pins[0])?, net(dc.pins[1])?, net(dc.pins[2])?);
                if d == s || open_if_all.contains(&g) || !sets_gate(g, &items) {
                    return None;
                }
                let bridge = model.instance(part).bridge;
                let flipped =
                    bridge.is_some_and(|(first, _)| first == dc.pins[2] && first != dc.pins[1]);
                Some(Item {
                    kind: Kind::Mosfet {
                        gate: 0,
                        drain: 0,
                        source: 0,
                        p_channel: dc.kind == DcKind::Pmos,
                        vth,
                        ron,
                    },
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
                    nets: vec![g, d, s],
                    reg: None,
                })
            })();
            match modelled {
                Some(item) => {
                    chosen.push(item);
                    taken.push([dc.pins[0], dc.pins[1], dc.pins[2]]);
                }
                None => fallback.push(part),
            }
        }
        for (item, pins) in chosen.into_iter().zip(taken) {
            accounted.extend(pins);
            if let Some(part) = item.part {
                modelled_parts.insert(part);
            }
            items.push(item);
        }
        for part in fallback {
            if let Some((item, a, b)) = boundary(part) {
                accounted.insert(a);
                accounted.insert(b);
                items.push(item);
            }
        }
        open_nets(&accounted)
    };

    for reg in regs.iter_mut() {
        reg.out_complete = !open.contains(&reg.out_net);
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
    let mut unsolved_outputs: BTreeSet<usize> = BTreeSet::new();
    for members in groups.values() {
        netsmith_core::job::checkpoint();
        let touched: BTreeSet<usize> = members
            .iter()
            .flat_map(|&i| items[i].nets.iter().copied())
            .collect();
        let touches_output = touched.iter().any(|n| reg_out_nets.contains(n));
        // Only an island with something to write, or that a regulator's output draws from, is
        // worth a solve.
        if members.iter().all(|&i| items[i].part.is_none()) && !touches_output {
            continue;
        }
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
        // An island left out is a load on a regulator's output that no sum has.
        let mut left_out =
            || unsolved_outputs.extend(touched.iter().filter(|n| reg_out_nets.contains(n)));
        if held_nets.is_empty() || free_nets.iter().any(|n| open.contains(n)) {
            left_out();
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
            left_out();
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
        let mut sinks = Vec::new();
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
                Kind::Regulator { draw, .. } => Kind::Regulator {
                    from: n[0],
                    to: n[1],
                    draw: *draw,
                },
                Kind::Mosfet {
                    p_channel,
                    vth,
                    ron,
                    ..
                } => Kind::Mosfet {
                    gate: n[0],
                    drain: n[1],
                    source: n[2],
                    p_channel: *p_channel,
                    vth: *vth,
                    ron: *ron,
                },
                Kind::Pin { high, low, z, .. } => Kind::Pin {
                    node: n[0],
                    high: high.map(|(_, r)| (n[1], r)),
                    low: low.map(|(_, r)| (n[2], r)),
                    z: *z,
                },
            };
            if let Some(reg) = item.reg {
                sinks.push(Sink {
                    element: island.elements.len(),
                    reg,
                });
            }
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
        let outputs = held_nets
            .iter()
            .enumerate()
            .filter_map(|(h, n)| {
                regs.iter()
                    .position(|r| r.out_net == *n)
                    .map(|reg| OutputNode { held: h, reg })
            })
            .collect();
        out.push(Planned {
            island,
            parts,
            sinks,
            outputs,
        });
    }
    Extracted {
        islands: out,
        regs,
        unsolved_outputs,
    }
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

/// A logic pin that can drive its net, with what it takes to model it.
struct Driver {
    terminal: InstId,
    net: usize,
    /// The nets of its supply and its ground, both held.
    pos: usize,
    neg: usize,
    /// The output resistance when it drives high and when it drives low.
    high: Option<Range>,
    low: Option<Range>,
    /// Whether it can let go of the line.
    z: bool,
}

/// The drive of a logic pin, from what its part states (ADR 0076, amendment 4). A pin is
/// modelled when it is an output, a tri-state output or a bidirectional pin of a `Logic` (or of
/// a bus's `Levels`) that:
///
/// - is not stated unused, and states no `swing` (a swing says the line goes to a supply that
///   has no pin, and the rail it is a reference to is not what it drives to),
/// - has a reference rail whose two nets a source holds,
/// - states a `voh` and a `vol` that have a value, each with the current it is measured at
///   (`.voh &= .vdd.voltage - 0.4V at 2mA`): the datasheet's own form (question 2 of the ADR).
///
/// A pin that states one of the two only might be an open-drain pin or a push-pull one that
/// leaves its high out; which is a marker the language does not have (ADR 0075), so it is a
/// boundary and not a guess.
fn driver_of(
    model: &Model,
    reader: &crate::levels::Run<'_>,
    pin: InstId,
    net: usize,
    net_of: &HashMap<InstId, usize>,
    held: &BTreeMap<usize, Held_>,
) -> Option<Driver> {
    let part = model.owner_of(pin)?;
    if model.instance(part).kind != InstKind::Part || model.unused_by(pin).is_some() {
        return None;
    }
    let z = match model.pin_kind(pin)? {
        PinKind::Output => false,
        PinKind::TriState | PinKind::Bidirectional => true,
        _ => return None,
    };
    let logic = reader.logic_of(pin)?;
    if reader.value(logic, "swing").is_some() {
        return None;
    }
    let Some(Member::Inst(reference)) = model.member(logic, "reference") else {
        return None;
    };
    let (_, pos_pin, neg_pin) = model.rail_of(reference)?;
    let (&pos, &neg) = (net_of.get(&pos_pin)?, net_of.get(&neg_pin)?);
    let (supply, ground) = (held.get(&pos)?, held.get(&neg)?);
    if supply.hold != Hold::Pos(neg) || ground.hold != Hold::Ground {
        return None;
    }
    let level = |name: &str| -> Option<(FieldId, Range, f64)> {
        let (f, q) = reader.value(logic, name)?;
        let test = model.level_test(f)?.current;
        let i = test.lo();
        if q.dim() != Dimension::VOLTAGE || !q.lo().is_finite() || !q.hi().is_finite() {
            return None;
        }
        (i.is_finite() && i > 0.0).then_some((f, Range::new(q.lo(), q.hi()), i))
    };
    let (voh, voh_range, ioh) = level("voh")?;
    let (_, vol_range, iol) = level("vol")?;
    // The datasheet gives `VOH = VDD - 0.4 V at 2 mA`: when the level is written that way the
    // drop is exact, and it is what the resistance is made of. Any other form is the most the
    // supply can be above the lowest high.
    let drop = stated_drop(model, voh, net_of, pos).unwrap_or(supply.range.hi - voh_range.lo);
    let roh = Range::new(MIN_RESISTANCE, (drop / ioh).max(MIN_RESISTANCE));
    let rol = Range::new(MIN_RESISTANCE, (vol_range.hi / iol).max(MIN_RESISTANCE));
    Some(Driver {
        terminal: pin,
        net,
        pos,
        neg,
        high: Some(roh),
        low: Some(rol),
        z,
    })
}

/// The constant `c` of a `voh` that is stated as `V - c` where `V` is the voltage of a rail whose
/// positive pin is on the net `pos`: the drop that the datasheet gives. `c` is taken at its
/// greatest.
fn stated_drop(
    model: &Model,
    level: FieldId,
    net_of: &HashMap<InstId, usize>,
    pos: usize,
) -> Option<f64> {
    model.relations().iter().find_map(|r| {
        let RExpr::Field(l) = &r.lhs else { return None };
        if *l != level {
            return None;
        }
        let RExpr::Bin(BinOp::Sub, a, b) = &r.rhs else {
            return None;
        };
        let (RExpr::Field(v), RExpr::Const(c)) = (&**a, &**b) else {
            return None;
        };
        let (_, rail_pos, _) = model.rail_of(model.field(*v).owner)?;
        if net_of.get(&rail_pos) != Some(&pos) || c.dim() != Dimension::VOLTAGE {
            return None;
        }
        (c.hi().is_finite() && c.hi() >= 0.0).then_some(c.hi())
    })
}
