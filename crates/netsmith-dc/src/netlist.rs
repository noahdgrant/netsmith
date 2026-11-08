//! The netlist the analysis works on: nodes, and elements whose values are ranges.
//!
//! An [`Element`] with ranges and choices (a logic pin that is high, low or high impedance, a
//! diode that conducts or blocks) has a list of *variants*: each is a [`Point`], the element
//! with every value fixed. The analysis solves one point netlist for each combination of the
//! variants of the elements (a corner of the box the ranges make, times the states).

use std::collections::HashMap;

use crate::range::{Range, si};

pub type NodeId = usize;

/// The reference node. Every voltage is relative to it.
pub const GROUND: NodeId = 0;

/// The smallest resistance a resistor is given, 1 mohm. A range such as a bead's `0 ohm..70 mohm`
/// has a lower end of zero, and a zero resistance is not a conductance a matrix can hold: with
/// 1 uohm the elimination lost three digits of a 70 uA current (conductances of 1e6 and 1e-5 in one
/// matrix). A millivolt at an ampere is below every tolerance the circuits here have.
pub const MIN_RESISTANCE: f64 = 1e-3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicState {
    High,
    Low,
    Z,
}

/// A logic pin: the output stage of a CMOS pin, or nothing.
///
/// A pin driving high joins its node to its own supply through `roh`, one driving low joins it
/// to its own ground through `rol`, and one that is not driving (`Z`) leaks through `rz`, to its
/// supply or to its ground (two cases, each the worst for some circuit). A datasheet's input
/// leakage `Ii` at the voltage `Vi` it is measured at is `rz = Vi / Ii` (3.3 V and 1 uA are
/// 3.3 Mohm); a leakage as a current source would be wrong on a node of high resistance, where
/// 1 uA through 1 Mohm is a volt that the pin's clamps would not allow. `rz = 0` is an input
/// with no leakage. This is what a datasheet's `voh` and `vol` are measured on: `voh` is the
/// supply less `Ioh * roh` at the test current `Ioh`, so `roh` is at most `(vdd - voh) / Ioh`
/// and at least zero. Because the high is the pin's own supply, a pin on a rail follows the
/// rail's range and cannot be above it (a range of its own for the high would let the pin
/// exceed the rail it comes from), and a pull-down current is the largest with the rail at its
/// top and `roh` at zero, which the levels check of ADR 0046 (the worst end, `voh`) does not
/// give.
#[derive(Clone, Debug)]
pub struct Logic {
    pub node: NodeId,
    pub vdd: NodeId,
    pub gnd: NodeId,
    pub states: Vec<LogicState>,
    pub roh: Range,
    pub rol: Range,
    pub rz: f64,
}

/// A regulator: an output that holds a voltage, and an input that draws what the output's
/// power asks for.
#[derive(Clone, Debug)]
pub struct Regulator {
    pub vin: NodeId,
    pub vout: NodeId,
    pub gnd: NodeId,
    /// The output voltage with its tolerance.
    pub v: Range,
    /// The efficiency as a range of fractions. Ignored for a linear regulator.
    pub eff: Range,
    /// The current the regulator draws with nothing at its output.
    pub iq: f64,
    /// A linear regulator draws what it supplies (plus `iq`); a switching one draws
    /// `Pout / (eff * Vin)`.
    pub linear: bool,
    /// The lowest input voltage the regulator works from, or `0` for none stated. A solution
    /// below it is reported as a warning.
    pub vin_min: f64,
}

#[derive(Clone, Debug)]
pub enum Kind {
    Resistor {
        a: NodeId,
        b: NodeId,
        r: Range,
    },
    /// An ideal source of a stated range (a rail).
    VSource {
        pos: NodeId,
        neg: NodeId,
        v: Range,
    },
    /// A constant current drawn from `from` and put into `to`.
    ISource {
        from: NodeId,
        to: NodeId,
        i: Range,
    },
    /// A diode with a fixed forward drop in the range `vf`: it blocks below it.
    Diode {
        anode: NodeId,
        cathode: NodeId,
        vf: Range,
    },
    Logic(Logic),
    /// A MOSFET as a switch: `Rds(on)` between drain and source when the gate-source voltage
    /// is above the threshold, nothing when it is below. A gate-source voltage inside the
    /// threshold's range is either, and both are tried. The gate draws nothing.
    Mosfet {
        gate: NodeId,
        drain: NodeId,
        source: NodeId,
        p_channel: bool,
        vth: Range,
        ron: Range,
    },
    Regulator(Regulator),
    /// A load that draws a constant power, whatever its voltage.
    PowerLoad {
        node: NodeId,
        gnd: NodeId,
        p: Range,
    },
}

#[derive(Clone, Debug)]
pub struct Element {
    pub name: String,
    pub kind: Kind,
}

/// An element with every value fixed and every choice made.
#[derive(Clone, Debug)]
pub enum Point {
    R {
        a: NodeId,
        b: NodeId,
        r: f64,
    },
    V {
        pos: NodeId,
        neg: NodeId,
        v: f64,
    },
    I {
        from: NodeId,
        to: NodeId,
        i: f64,
    },
    DiodeOn {
        a: NodeId,
        k: NodeId,
        vf: f64,
    },
    DiodeOff {
        a: NodeId,
        k: NodeId,
        vf: f64,
    },
    /// A logic pin driving: its node joined to `to` (its supply or its ground) through `r`.
    Drive {
        node: NodeId,
        to: NodeId,
        r: f64,
    },
    /// A logic pin not driving: a leakage current `i` from the node to its ground.
    Leak {
        node: NodeId,
        gnd: NodeId,
        i: f64,
    },
    MosOn {
        gate: NodeId,
        drain: NodeId,
        source: NodeId,
        p_channel: bool,
        vth: Range,
        ron: f64,
    },
    MosOff {
        gate: NodeId,
        source: NodeId,
        p_channel: bool,
        vth: Range,
    },
    Reg {
        vin: NodeId,
        vout: NodeId,
        gnd: NodeId,
        v: f64,
        eff: f64,
        iq: f64,
        linear: bool,
        vin_min: f64,
    },
    PLoad {
        node: NodeId,
        gnd: NodeId,
        p: f64,
    },
}

impl Element {
    /// The suffixes of the quantities this element reports, in the order `Point` values come
    /// in (see `op::solve`). `i` is a current, `p` a power; a regulator has the current
    /// into its input (`in`), the current out of its output (`out`) and `p`, its loss.
    pub fn quantity_names(&self) -> Vec<String> {
        let n = &self.name;
        match self.kind {
            Kind::Resistor { .. }
            | Kind::Diode { .. }
            | Kind::Mosfet { .. }
            | Kind::PowerLoad { .. } => vec![format!("i({n})"), format!("p({n})")],
            Kind::VSource { .. } | Kind::ISource { .. } | Kind::Logic(_) => {
                vec![format!("i({n})")]
            }
            Kind::Regulator(_) => vec![
                format!("i({n}.in)"),
                format!("i({n}.out)"),
                format!("p({n})"),
            ],
        }
    }

    /// Every way this element can be: the ends of each range, each state it may be in. A
    /// label says which, for the report of what produced an extreme.
    pub fn variants(&self) -> Vec<(String, Point)> {
        match &self.kind {
            Kind::Resistor { a, b, r } => r
                .corners()
                .into_iter()
                .map(|r| {
                    (
                        format!("{}ohm", si(r)),
                        Point::R {
                            a: *a,
                            b: *b,
                            r: r.max(MIN_RESISTANCE),
                        },
                    )
                })
                .collect(),
            Kind::VSource { pos, neg, v } => v
                .corners()
                .into_iter()
                .map(|v| {
                    (
                        format!("{}V", si(v)),
                        Point::V {
                            pos: *pos,
                            neg: *neg,
                            v,
                        },
                    )
                })
                .collect(),
            Kind::ISource { from, to, i } => i
                .corners()
                .into_iter()
                .map(|i| {
                    (
                        format!("{}A", si(i)),
                        Point::I {
                            from: *from,
                            to: *to,
                            i,
                        },
                    )
                })
                .collect(),
            Kind::Diode { anode, cathode, vf } => {
                let mut out = vec![(
                    "blocking".to_string(),
                    Point::DiodeOff {
                        a: *anode,
                        k: *cathode,
                        vf: vf.hi,
                    },
                )];
                for v in vf.corners() {
                    out.push((
                        format!("conducting at {}V", si(v)),
                        Point::DiodeOn {
                            a: *anode,
                            k: *cathode,
                            vf: v,
                        },
                    ));
                }
                out
            }
            Kind::Logic(l) => {
                let mut out = Vec::new();
                for state in &l.states {
                    match state {
                        LogicState::High => {
                            for r in l.roh.corners() {
                                out.push((
                                    format!("high through {}ohm", si(r)),
                                    Point::Drive {
                                        node: l.node,
                                        to: l.vdd,
                                        r: r.max(MIN_RESISTANCE),
                                    },
                                ));
                            }
                        }
                        LogicState::Low => {
                            for r in l.rol.corners() {
                                out.push((
                                    format!("low through {}ohm", si(r)),
                                    Point::Drive {
                                        node: l.node,
                                        to: l.gnd,
                                        r: r.max(MIN_RESISTANCE),
                                    },
                                ));
                            }
                        }
                        LogicState::Z => {
                            if l.rz > 0.0 {
                                for (what, to) in [("supply", l.vdd), ("ground", l.gnd)] {
                                    out.push((
                                        format!("high impedance, leaking to its {what}"),
                                        Point::Drive {
                                            node: l.node,
                                            to,
                                            r: l.rz,
                                        },
                                    ));
                                }
                            } else {
                                out.push((
                                    "high impedance".to_string(),
                                    Point::Leak {
                                        node: l.node,
                                        gnd: l.gnd,
                                        i: 0.0,
                                    },
                                ));
                            }
                        }
                    }
                }
                out
            }
            Kind::Mosfet {
                gate,
                drain,
                source,
                p_channel,
                vth,
                ron,
            } => {
                let mut out = vec![(
                    "off".to_string(),
                    Point::MosOff {
                        gate: *gate,
                        source: *source,
                        p_channel: *p_channel,
                        vth: *vth,
                    },
                )];
                for r in ron.corners() {
                    out.push((
                        format!("on at {}ohm", si(r)),
                        Point::MosOn {
                            gate: *gate,
                            drain: *drain,
                            source: *source,
                            p_channel: *p_channel,
                            vth: *vth,
                            ron: r.max(MIN_RESISTANCE),
                        },
                    ));
                }
                out
            }
            Kind::Regulator(r) => {
                let effs = if r.linear { vec![1.0] } else { r.eff.corners() };
                let mut out = Vec::new();
                for v in r.v.corners() {
                    for &eff in &effs {
                        let label = if r.linear {
                            format!("{}V out", si(v))
                        } else {
                            format!("{}V out at efficiency {eff:.3}", si(v))
                        };
                        out.push((
                            label,
                            Point::Reg {
                                vin: r.vin,
                                vout: r.vout,
                                gnd: r.gnd,
                                v,
                                eff,
                                iq: r.iq,
                                linear: r.linear,
                                vin_min: r.vin_min,
                            },
                        ));
                    }
                }
                out
            }
            Kind::PowerLoad { node, gnd, p } => p
                .corners()
                .into_iter()
                .map(|p| {
                    (
                        format!("{}W", si(p)),
                        Point::PLoad {
                            node: *node,
                            gnd: *gnd,
                            p,
                        },
                    )
                })
                .collect(),
        }
    }

    /// One point inside the ranges, at random (`u` gives numbers in `0..1`). It is used to
    /// check that what the corners say holds in the interior too.
    pub fn sample(&self, u: &mut dyn FnMut() -> f64) -> Point {
        fn pick(u: &mut dyn FnMut() -> f64, r: Range) -> f64 {
            r.lo + u() * r.width()
        }
        match &self.kind {
            Kind::Resistor { a, b, r } => Point::R {
                a: *a,
                b: *b,
                r: pick(u, *r).max(MIN_RESISTANCE),
            },
            Kind::VSource { pos, neg, v } => Point::V {
                pos: *pos,
                neg: *neg,
                v: pick(u, *v),
            },
            Kind::ISource { from, to, i } => Point::I {
                from: *from,
                to: *to,
                i: pick(u, *i),
            },
            Kind::Diode { anode, cathode, vf } => {
                let v = pick(u, *vf);
                if u() < 0.5 {
                    Point::DiodeOn {
                        a: *anode,
                        k: *cathode,
                        vf: v,
                    }
                } else {
                    Point::DiodeOff {
                        a: *anode,
                        k: *cathode,
                        vf: v,
                    }
                }
            }
            Kind::Logic(l) => {
                let state = l.states[(u() * l.states.len() as f64) as usize % l.states.len()];
                match state {
                    LogicState::High => Point::Drive {
                        node: l.node,
                        to: l.vdd,
                        r: pick(u, l.roh).max(MIN_RESISTANCE),
                    },
                    LogicState::Low => Point::Drive {
                        node: l.node,
                        to: l.gnd,
                        r: pick(u, l.rol).max(MIN_RESISTANCE),
                    },
                    LogicState::Z if l.rz > 0.0 => Point::Drive {
                        node: l.node,
                        to: if u() < 0.5 { l.vdd } else { l.gnd },
                        r: l.rz,
                    },
                    LogicState::Z => Point::Leak {
                        node: l.node,
                        gnd: l.gnd,
                        i: 0.0,
                    },
                }
            }
            Kind::Mosfet {
                gate,
                drain,
                source,
                p_channel,
                vth,
                ron,
            } => {
                if u() < 0.5 {
                    Point::MosOn {
                        gate: *gate,
                        drain: *drain,
                        source: *source,
                        p_channel: *p_channel,
                        vth: *vth,
                        ron: pick(u, *ron).max(MIN_RESISTANCE),
                    }
                } else {
                    Point::MosOff {
                        gate: *gate,
                        source: *source,
                        p_channel: *p_channel,
                        vth: *vth,
                    }
                }
            }
            Kind::Regulator(r) => Point::Reg {
                vin: r.vin,
                vout: r.vout,
                gnd: r.gnd,
                v: pick(u, r.v),
                eff: if r.linear { 1.0 } else { pick(u, r.eff) },
                iq: r.iq,
                linear: r.linear,
                vin_min: r.vin_min,
            },
            Kind::PowerLoad { node, gnd, p } => Point::PLoad {
                node: *node,
                gnd: *gnd,
                p: pick(u, *p),
            },
        }
    }
}

/// The nodes by name and the elements.
#[derive(Clone, Debug)]
pub struct Netlist {
    names: Vec<String>,
    index: HashMap<String, NodeId>,
    pub elements: Vec<Element>,
}

impl Default for Netlist {
    fn default() -> Self {
        Netlist::new()
    }
}

impl Netlist {
    pub fn new() -> Netlist {
        let mut index = HashMap::new();
        index.insert("GND".to_string(), GROUND);
        index.insert("0".to_string(), GROUND);
        Netlist {
            names: vec!["GND".to_string()],
            index,
            elements: Vec::new(),
        }
    }

    /// The node of this name, made on first use. `GND` and `0` are the reference.
    pub fn node(&mut self, name: &str) -> NodeId {
        if let Some(&id) = self.index.get(name) {
            return id;
        }
        let id = self.names.len();
        self.names.push(name.to_string());
        self.index.insert(name.to_string(), id);
        id
    }

    pub fn node_name(&self, id: NodeId) -> &str {
        &self.names[id]
    }

    pub fn node_count(&self) -> usize {
        self.names.len()
    }

    pub fn find(&self, name: &str) -> Option<NodeId> {
        self.index.get(name).copied()
    }

    pub fn add(&mut self, name: &str, kind: Kind) {
        self.elements.push(Element {
            name: name.to_string(),
            kind,
        });
    }

    pub fn resistor(&mut self, name: &str, a: &str, b: &str, r: Range) {
        let (a, b) = (self.node(a), self.node(b));
        self.add(name, Kind::Resistor { a, b, r });
    }

    pub fn vsource(&mut self, name: &str, pos: &str, neg: &str, v: Range) {
        let (pos, neg) = (self.node(pos), self.node(neg));
        self.add(name, Kind::VSource { pos, neg, v });
    }

    pub fn isource(&mut self, name: &str, from: &str, to: &str, i: Range) {
        let (from, to) = (self.node(from), self.node(to));
        self.add(name, Kind::ISource { from, to, i });
    }

    pub fn diode(&mut self, name: &str, anode: &str, cathode: &str, vf: Range) {
        let (anode, cathode) = (self.node(anode), self.node(cathode));
        self.add(name, Kind::Diode { anode, cathode, vf });
    }

    pub fn power_load(&mut self, name: &str, node: &str, gnd: &str, p: Range) {
        let (node, gnd) = (self.node(node), self.node(gnd));
        self.add(name, Kind::PowerLoad { node, gnd, p });
    }

    /// The names of every quantity the analysis reports, in order: each node's voltage, then
    /// the quantities of each element.
    pub fn quantity_labels(&self) -> Vec<String> {
        let mut labels: Vec<String> = (1..self.node_count())
            .map(|n| format!("v({})", self.node_name(n)))
            .collect();
        for e in &self.elements {
            labels.extend(e.quantity_names());
        }
        labels
    }
}
