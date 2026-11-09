//! The most a net's voltage can be, from the supplies that reach it (ADR 0076, amendment 7).
//!
//! A supply the design states (a board's rail with a stated voltage, or the rail a part says it
//! gives with `dc source`) reaches a net through series elements: resistors, diodes and the
//! channel of a switch. None of them raises a voltage, so the net is at most the highest supply
//! that reaches it. It is the upper end of the window that a pick judged before the network is
//! solved can lean on (a regulator's input, between what its part needs and what the supply
//! gives), and it needs no island: the network does not have to be closed to say it.

use std::collections::{HashMap, HashSet};

use netsmith_check::{DcKind, Domain, InstId, InstKind, Model};

/// The nets the supplies reach, and how high each supply is.
pub(crate) struct Ceilings {
    net_of: HashMap<InstId, usize>,
    /// For each net, the nets that a series element joins it to.
    joined: HashMap<usize, Vec<usize>>,
    /// The upper end of each supply, by the net its positive terminal is on.
    supplies: HashMap<usize, f64>,
}

impl Ceilings {
    pub(crate) fn new(model: &Model) -> Self {
        let mut net_of = HashMap::new();
        for (i, n) in model.nets().iter().enumerate() {
            for &p in &n.pins {
                net_of.insert(p, i);
            }
        }
        let mut joined: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut join = |a: InstId, b: InstId| {
            if let (Some(&x), Some(&y)) = (net_of.get(&a), net_of.get(&b))
                && x != y
            {
                joined.entry(x).or_default().push(y);
                joined.entry(y).or_default().push(x);
            }
        };
        let mut given: HashSet<InstId> = HashSet::new();
        for (part, dc) in model.dc_elements() {
            if model.instance(part).kind != InstKind::Part {
                continue;
            }
            match dc.kind {
                DcKind::Resistor | DcKind::Diode => join(dc.pins[0], dc.pins[1]),
                DcKind::Nmos | DcKind::Pmos => join(dc.pins[1], dc.pins[2]),
                DcKind::Source => {
                    given.insert(dc.pins[0]);
                }
                DcKind::LinearRegulator | DcKind::SwitchingRegulator => {}
            }
        }
        let mut supplies: HashMap<usize, f64> = HashMap::new();
        for (iface, inst) in model.instances() {
            if inst.kind != InstKind::Interface {
                continue;
            }
            let Some((voltage, pos, _)) = model.rail_of(iface) else {
                continue;
            };
            let Some(parent) = inst.parent else { continue };
            if model.instance(parent).kind != InstKind::Board && !given.contains(&iface) {
                continue;
            }
            let field = model.field(voltage);
            // A voltage the design states, not one the solver worked out.
            let Domain::Quantity(q) = &field.domain else {
                continue;
            };
            if field.narrowed_at.is_empty() || !q.hi().is_finite() {
                continue;
            }
            if let Some(&net) = net_of.get(&pos) {
                let top = supplies.entry(net).or_insert(f64::NEG_INFINITY);
                *top = top.max(q.hi());
            }
        }
        Ceilings {
            net_of,
            joined,
            supplies,
        }
    }

    /// The highest supply that reaches the net of `pin`, if one does.
    pub(crate) fn of(&self, pin: InstId) -> Option<f64> {
        let start = *self.net_of.get(&pin)?;
        let mut seen: HashSet<usize> = HashSet::from([start]);
        let mut todo = vec![start];
        let mut top: Option<f64> = None;
        while let Some(net) = todo.pop() {
            if let Some(&v) = self.supplies.get(&net) {
                top = Some(top.map_or(v, |t: f64| t.max(v)));
            }
            for &next in self.joined.get(&net).into_iter().flatten() {
                if seen.insert(next) {
                    todo.push(next);
                }
            }
        }
        top
    }
}
