//! A part within a distance of a pin (issue #1274): a `within` statement of the physical file,
//! `.ant within 8mm of .radio.ant`, found on the board.
//!
//! It is measured as a datasheet's `near ... within` is ([`crate::part_layout`]): pad centre to
//! pad centre, from the nearest pad of the part on the net of the pin (any of its pads when none
//! is on it, and only the pads of the pin when the statement names a pin of the part) to the
//! nearest pad of the pin. Unlike a datasheet's guideline it is a requirement: the placer holds
//! it as a hard rule (at the weight of the walls for each mm past it, as a relation between two
//! parts is held) on top of the pull of a decoupling capacitor, and `fab` measures it on the
//! board as the rule `constraint`, an error unless `constraints = "warn"`.

use netsmith_core::Design;

use crate::board::Placed;
use crate::part_layout::{centres, nearest};

/// A `within` statement on the board: the parts by designator, with the pads it measures.
#[derive(Debug, Clone, PartialEq)]
pub struct PinNear {
    /// The statement, `ant within 8 mm of radio.ant`.
    pub id: String,
    /// Where it is written: `widget.physical.nsm:14`; empty if not known.
    pub source: String,
    /// The part that must be near, by designator, and its path.
    pub part: String,
    pub part_path: String,
    /// The pads of `part` measured.
    pub pads: Vec<String>,
    /// The part whose pin it must be near, by designator, and its path.
    pub to: String,
    pub to_path: String,
    /// The pads of the pin.
    pub to_pads: Vec<String>,
    /// The most the distance may be, in mm.
    pub within: f64,
}

impl PinNear {
    /// Every `within` statement of the design whose two parts it has, with their pads. A
    /// statement about a part the design does not have (a bank's path) is left out.
    pub fn of_design(design: &Design) -> Vec<PinNear> {
        let pads_of = |part: &netsmith_core::Part, pin: &str| -> Vec<String> {
            part.pins
                .iter()
                .filter(|p| p.name == pin)
                .flat_map(|p| p.pads.iter().cloned())
                .collect()
        };
        let mut found = Vec::new();
        for d in &design.features.pin_distances {
            let by_path = |path: &str| design.parts.iter().find(|p| p.path == path);
            let (Some(part), Some(to)) = (by_path(&d.part), by_path(&d.to_part)) else {
                continue;
            };
            let to_pads = pads_of(to, &d.to_pin);
            let pads = match &d.pin {
                Some(pin) => pads_of(part, pin),
                None => {
                    // The part's pads on the net of the pin, else all of them.
                    let on_net: Vec<String> = design
                        .nets
                        .iter()
                        .filter(|n| {
                            n.pins
                                .iter()
                                .any(|r| r.refdes == to.refdes && to_pads.contains(&r.pin))
                        })
                        .flat_map(|n| n.pins.iter())
                        .filter(|r| r.refdes == part.refdes)
                        .map(|r| r.pin.clone())
                        .collect();
                    if on_net.is_empty() {
                        part.pins
                            .iter()
                            .flat_map(|p| p.pads.iter().cloned())
                            .collect()
                    } else {
                        on_net
                    }
                }
            };
            found.push(PinNear {
                id: d.id(),
                source: d.source.clone(),
                part: part.refdes.clone(),
                part_path: part.path.clone(),
                pads,
                to: to.refdes.clone(),
                to_path: to.path.clone(),
                to_pads,
                within: d.within,
            });
        }
        found
    }

    /// The distance on the placed board, in mm: `None` when a part is not on it or a pad is not
    /// on its footprint.
    pub fn distance(&self, parts: &[Placed]) -> Option<f64> {
        let part = parts.iter().find(|p| p.refdes == self.part)?;
        let to = parts.iter().find(|p| p.refdes == self.to)?;
        nearest(&centres(part, &self.pads), &centres(to, &self.to_pads))
    }

    /// The statement and where it is written: `ant within 8 mm of radio.ant (t.physical.nsm:3)`.
    pub fn label(&self) -> String {
        if self.source.is_empty() {
            self.id.clone()
        } else {
            format!("{} ({})", self.id, self.source)
        }
    }
}
