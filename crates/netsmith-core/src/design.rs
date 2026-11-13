//! The design: its parts, nets and everything else, and the checks that hold it together.

use crate::{Features, InterfaceInstance, InterfaceMember, Net, Note, Part, Process};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Design {
    pub name: String,
    pub parts: Vec<Part>,
    pub nets: Vec<Net>,
    /// How the board is to be made, if it says. A board with no process can still be drawn
    /// as a schematic, but not laid out.
    pub process: Option<Process>,
    pub features: Features,
    /// The interface instances whose members are on the design's nets, by path. Empty for a
    /// design that does not know them (a netlist read from a file, a test's small design).
    pub interfaces: Vec<InterfaceInstance>,
    /// The source's own writing the drawing may show: doc comments and `==` relations, in
    /// source order. Empty for a design that does not know them (a netlist read from a file, a
    /// test's small design).
    pub notes: Vec<Note>,
    /// The module instances of the design, by path, in the order the source declares them
    /// (a module inside a module after it). The KiCad schematic draws each module instance of
    /// the board as a sheet of its own (issue #563). Empty for a design that does not know them
    /// (a netlist read from a file, a test's small design), which is drawn as one sheet.
    pub modules: Vec<ModuleInstance>,
}

/// A module instance of the design and the nets its own members are on: the pins and the
/// pins of the interfaces it declares itself (`.vin: Rail` gives `vin.pos` and `vin.neg`), not
/// those of its parts.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModuleInstance {
    /// Where the instance is in the design, the path of its instance from the board:
    /// `stage`, or `power.ldo` for a module inside a module.
    pub path: String,
    /// The module definition it was made from: `LdoStage`.
    pub type_name: String,
    /// Its members that are on nets of the design, each named by its path inside the module
    /// (`vin.pos`), in the order the module declares them.
    pub ports: Vec<InterfaceMember>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DesignError {
    DuplicateRefdes(String),
    DuplicateNet(String),
    UnknownRefdes { net: String, refdes: String },
}

impl std::fmt::Display for DesignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateRefdes(r) => write!(f, "duplicate reference designator {r}"),
            Self::DuplicateNet(n) => write!(f, "duplicate net {n}"),
            Self::UnknownRefdes { net, refdes } => {
                write!(f, "net {net} refers to unknown part {refdes}")
            }
        }
    }
}

impl std::error::Error for DesignError {}

impl Design {
    pub fn validate(&self) -> Result<(), DesignError> {
        let mut refdes = HashSet::new();
        for p in &self.parts {
            if !refdes.insert(p.refdes.as_str()) {
                return Err(DesignError::DuplicateRefdes(p.refdes.clone()));
            }
        }
        let mut nets = HashSet::new();
        for n in &self.nets {
            if !nets.insert(n.name.as_str()) {
                return Err(DesignError::DuplicateNet(n.name.clone()));
            }
            for pin in &n.pins {
                if !refdes.contains(pin.refdes.as_str()) {
                    return Err(DesignError::UnknownRefdes {
                        net: n.name.clone(),
                        refdes: pin.refdes.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PinRef;

    fn part(refdes: &str) -> Part {
        Part {
            provides: Vec::new(),
            refdes: refdes.into(),
            kind: "Resistor".into(),
            value: "1k".into(),
            package: None,
            path: String::new(),
            pins: Vec::new(),
        }
    }

    #[test]
    fn a_connector_is_a_part_that_provides_the_connector_generic() {
        let mut p = part("J1");
        p.kind = "MolexPicoBlade".into();
        assert!(!p.is_connector());
        p.provides = vec!["Connector".into()];
        assert!(p.is_connector());
        // A name that starts like a connector's, or letters `J`, do not make one.
        let mut p = part("J2");
        p.kind = "HeaderBuffer".into();
        assert!(!p.is_connector());
    }

    #[test]
    fn rejects_duplicate_refdes() {
        let d = Design {
            modules: Vec::new(),
            name: "t".into(),
            parts: vec![part("R1"), part("R1")],
            nets: vec![],
            process: None,
            features: Features::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
        };
        assert_eq!(d.validate(), Err(DesignError::DuplicateRefdes("R1".into())));
    }

    #[test]
    fn rejects_net_with_unknown_part() {
        let d = Design {
            modules: Vec::new(),
            name: "t".into(),
            parts: vec![part("R1")],
            nets: vec![Net {
                name: "A".into(),
                pins: vec![PinRef {
                    refdes: "R9".into(),
                    pin: "1".into(),
                }],
            }],
            process: None,
            features: Features::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
        };
        assert!(matches!(
            d.validate(),
            Err(DesignError::UnknownRefdes { .. })
        ));
    }
}
