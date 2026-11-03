//! The netlist a comparison works on: components with pins, and nets joining them.

use std::collections::{BTreeMap, HashMap};

/// A part with a designator. Its pins are the ones that are on a net, by pin number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    pub refdes: String,
    pub value: String,
    /// The footprint as the file names it; the library (`Resistor_SMD:`) is ignored when
    /// comparing.
    pub footprint: String,
    /// `(pin number, index into Netlist::nets)`, ordered by pin number.
    pub pins: Vec<(String, usize)>,
}

/// A set of pins that are joined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Net {
    pub name: String,
    /// `(index into Netlist::components, pin number)`.
    pub pins: Vec<(usize, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Netlist {
    pub components: Vec<Component>,
    pub nets: Vec<Net>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum NetlistError {
    /// The text is not a netlist or board of the expected kind.
    Format(String),
    DuplicateRefdes(String),
    /// A net names a component the netlist does not have.
    UnknownRefdes {
        net: String,
        refdes: String,
    },
    /// One pin is on two nets, which would join them.
    PinOnTwoNets {
        refdes: String,
        pin: String,
        nets: [String; 2],
    },
}

impl std::fmt::Display for NetlistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(m) => write!(f, "{m}"),
            Self::DuplicateRefdes(r) => write!(f, "two components have the designator {r}"),
            Self::UnknownRefdes { net, refdes } => {
                write!(
                    f,
                    "net {net} has a pin of {refdes}, which is not a component"
                )
            }
            Self::PinOnTwoNets { refdes, pin, nets } => write!(
                f,
                "pin {refdes}.{pin} is on two nets, {} and {}",
                nets[0], nets[1]
            ),
        }
    }
}

impl std::error::Error for NetlistError {}

/// A designator that is not a part: power ports (`#PWR01`) and flags (`#FLG01`).
fn is_graphic(refdes: &str) -> bool {
    refdes.starts_with('#')
}

/// What KiCad calls the net of a pin that is joined to nothing: `unconnected-(R1-Pad2)`.
fn is_unconnected_name(name: &str) -> bool {
    name.starts_with("unconnected-")
}

impl Netlist {
    /// Build a netlist from `(designator, value, footprint)` components and
    /// `(net name, [(designator, pin)])` nets.
    ///
    /// Components whose designator starts with `#` (power ports, flags) are not parts and
    /// are left out with their pins. A pin joined to nothing is not on a net: a net
    /// that KiCad names `unconnected-...` and a net left with no pin are dropped, and a pin
    /// listed twice on one net counts once.
    pub fn from_parts(
        components: Vec<(String, String, String)>,
        nets: Vec<(String, Vec<(String, String)>)>,
    ) -> Result<Netlist, NetlistError> {
        let mut index: HashMap<String, usize> = HashMap::new();
        let mut out = Netlist::default();
        for (refdes, value, footprint) in components {
            if is_graphic(&refdes) {
                continue;
            }
            if index.insert(refdes.clone(), out.components.len()).is_some() {
                return Err(NetlistError::DuplicateRefdes(refdes));
            }
            out.components.push(Component {
                refdes,
                value,
                footprint,
                pins: Vec::new(),
            });
        }
        // Which net each pin is on, so that a pin on two nets is caught.
        let mut seen: BTreeMap<(usize, String), usize> = BTreeMap::new();
        for (name, pins) in nets {
            let mut kept = Vec::new();
            for (refdes, pin) in pins {
                if is_graphic(&refdes) {
                    continue;
                }
                let Some(&c) = index.get(&refdes) else {
                    return Err(NetlistError::UnknownRefdes { net: name, refdes });
                };
                if !kept.contains(&(c, pin.clone())) {
                    kept.push((c, pin));
                }
            }
            if kept.is_empty() || (is_unconnected_name(&name) && kept.len() == 1) {
                continue;
            }
            let n = out.nets.len();
            for (c, pin) in &kept {
                if let Some(&other) = seen.get(&(*c, pin.clone())) {
                    return Err(NetlistError::PinOnTwoNets {
                        refdes: out.components[*c].refdes.clone(),
                        pin: pin.clone(),
                        nets: [out.nets[other].name.clone(), name],
                    });
                }
                seen.insert((*c, pin.clone()), n);
                out.components[*c].pins.push((pin.clone(), n));
            }
            out.nets.push(Net { name, pins: kept });
        }
        for c in &mut out.components {
            c.pins.sort();
        }
        Ok(out)
    }

    /// The same netlist without the nets of a single pin, as `--ignore-unconnected` asks.
    pub fn without_single_pin_nets(&self) -> Netlist {
        let keep: Vec<bool> = self.nets.iter().map(|n| n.pins.len() > 1).collect();
        let mut renumber = vec![usize::MAX; self.nets.len()];
        let mut nets = Vec::new();
        for (i, n) in self.nets.iter().enumerate() {
            if keep[i] {
                renumber[i] = nets.len();
                nets.push(n.clone());
            }
        }
        let components = self
            .components
            .iter()
            .map(|c| Component {
                pins: c
                    .pins
                    .iter()
                    .filter(|(_, n)| keep[*n])
                    .map(|(p, n)| (p.clone(), renumber[*n]))
                    .collect(),
                ..c.clone()
            })
            .collect();
        Netlist { components, nets }
    }

    /// The index of the component with this designator.
    pub fn find(&self, refdes: &str) -> Option<usize> {
        self.components.iter().position(|c| c.refdes == refdes)
    }

    /// The net a pin is on, if any.
    pub fn net_of(&self, component: usize, pin: &str) -> Option<usize> {
        self.components[component]
            .pins
            .iter()
            .find(|(p, _)| p == pin)
            .map(|(_, n)| *n)
    }
}
