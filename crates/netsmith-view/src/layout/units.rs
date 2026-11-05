//! A part whose symbol has several **units** is drawn as one box for each unit (ADR 0050).
//!
//! The KiCad symbol of a processor module often has two units, the power and the grounds
//! (`U1A`) and the signals (`U1B`); a dual op-amp has the two amplifiers and the supply. A hand
//! drawing, and KiCad's own schematic, put each unit on the sheet as a box of its own with the
//! part's reference and the unit's letter (`U1A`), and the decoupling capacitor stands beside the
//! power unit and not beside the signals. This module finds the units, from the library's
//! symbol (never from a name), and gives the engine what it needs to draw them: a derived
//! design in which each unit is a part of its own, so that everything that already stands beside
//! a main part's pins (a satellite, a rail, a bus, a flag) works for a unit unchanged.
//!
//! The derived design has the design's parts in their order, a part with units being its
//! **primary** unit (the one with most pins, the first on a tie), then the other units of each
//! part in the order of the parts and the units. So `layout.parts[i]` is the design's part `i`
//! for every `i` below the number of parts, and a box after that is another unit of a part
//! (`PlacedPart::part` says which). A unit's reference is the part's with its letter, `U5A`; the
//! nets are the design's, each pin on the unit that draws its pad. The netlist, the KiCad
//! schematic and the editor's other views are not changed: only this drawing has two boxes.

use std::collections::{HashMap, HashSet};

use netsmith_core::{Design, Net, Part, PartPin, PinRef};
use netsmith_kicad::{Mapping, Pinout, SymbolLib};

use super::Marks;
use crate::glyph::Glyph;

/// The layout's input when some part is drawn as several boxes.
pub(super) struct Split {
    /// The design with each unit a part of its own.
    pub design: Design,
    /// The glyph of each part of `design`.
    pub glyphs: Vec<Glyph>,
    /// For each part of `design` (each box), the index of its part in the original design.
    pub origin: Vec<usize>,
    /// The pins the checker says must be connected, by the derived references.
    pub marks: Marks,
}

/// The letter of a unit: 1 is `A`.
pub(crate) fn unit_letter(unit: u32) -> String {
    match unit {
        1..=26 => char::from(b'A' + (unit - 1) as u8).to_string(),
        n => n.to_string(),
    }
}

/// The derived design of `design`, or none when every part is one box (or a reference would
/// clash).
pub(super) fn split(
    design: &Design,
    used: &[Vec<String>],
    marks: &Marks,
    mapping: &Mapping,
    lib: &SymbolLib,
) -> Option<Split> {
    // The parts that have units, with each unit's box.
    let units: Vec<Option<Vec<(u32, Glyph)>>> = design
        .parts
        .iter()
        .zip(used)
        .map(|(p, u)| Glyph::unit_glyphs(p, u, mapping, lib))
        .collect();
    if units.iter().all(Option::is_none) {
        return None;
    }
    // The primary unit of each: the box with most pins.
    let primary: Vec<usize> = units
        .iter()
        .map(|u| {
            u.as_ref().map_or(0, |u| {
                let mut best = 0;
                for (i, (_, g)) in u.iter().enumerate() {
                    if g.pins.iter().filter(|p| !p.hidden).count()
                        > u[best].1.pins.iter().filter(|p| !p.hidden).count()
                    {
                        best = i;
                    }
                }
                best
            })
        })
        .collect();

    // The boxes: each part (its primary unit), then the other units.
    struct Boxed {
        part: usize,
        /// Which of the part's units, when it has them.
        unit: Option<usize>,
    }
    let mut boxes: Vec<Boxed> = (0..design.parts.len())
        .map(|p| Boxed {
            part: p,
            unit: units[p].as_ref().map(|_| primary[p]),
        })
        .collect();
    for (p, u) in units.iter().enumerate() {
        for i in 0..u.as_ref().map_or(0, Vec::len) {
            if i != primary[p] {
                boxes.push(Boxed {
                    part: p,
                    unit: Some(i),
                });
            }
        }
    }

    let refdes_of = |b: &Boxed| -> String {
        let part = &design.parts[b.part];
        match (b.unit, &units[b.part]) {
            (Some(i), Some(u)) => format!("{}{}", part.refdes, unit_letter(u[i].0)),
            _ => part.refdes.clone(),
        }
    };
    let names: Vec<String> = boxes.iter().map(refdes_of).collect();
    // A reference that is already a part's, or twice, would join two boxes in the nets.
    let originals: HashSet<&str> = design.parts.iter().map(|p| p.refdes.as_str()).collect();
    let mut seen: HashSet<&str> = HashSet::new();
    for (b, name) in boxes.iter().zip(&names) {
        let clash = b.unit.is_some() && originals.contains(name.as_str());
        if clash || !seen.insert(name) {
            return None;
        }
    }

    // The box that draws each pad of a part with units.
    let mut owner: HashMap<(usize, &str), usize> = HashMap::new();
    for (k, b) in boxes.iter().enumerate() {
        if let (Some(i), Some(u)) = (b.unit, &units[b.part]) {
            for pin in &u[i].1.pins {
                owner.entry((b.part, pin.id.as_str())).or_insert(k);
            }
        }
    }

    let parts: Vec<Part> = boxes
        .iter()
        .zip(&names)
        .map(|(b, name)| {
            let mut part = design.parts[b.part].clone();
            part.refdes = name.clone();
            if let (Some(i), Some(u)) = (b.unit, &units[b.part]) {
                let ids: HashSet<&str> = u[i].1.pins.iter().map(|p| p.id.as_str()).collect();
                part.pins = part
                    .pins
                    .iter()
                    .filter_map(|pin| {
                        let pads: Vec<String> = pin
                            .pads
                            .iter()
                            .filter(|pad| ids.contains(pad.as_str()))
                            .cloned()
                            .collect();
                        (!pads.is_empty()).then(|| PartPin {
                            pads,
                            ..pin.clone()
                        })
                    })
                    .collect();
            }
            part
        })
        .collect();
    let glyphs: Vec<Glyph> = boxes
        .iter()
        .map(|b| match (b.unit, &units[b.part]) {
            (Some(i), Some(u)) => u[i].1.clone(),
            _ => {
                let part = &design.parts[b.part];
                Glyph::for_part(part, &used[b.part], Pinout::of(part).as_ref(), mapping, lib)
            }
        })
        .collect();
    let origin: Vec<usize> = boxes.iter().map(|b| b.part).collect();
    Some(finish(design, marks, parts, glyphs, &owner, &names, origin))
}

fn finish(
    design: &Design,
    marks: &Marks,
    parts: Vec<Part>,
    glyphs: Vec<Glyph>,
    owner: &HashMap<(usize, &str), usize>,
    names: &[String],
    origin: Vec<usize>,
) -> Split {
    let index: HashMap<&str, usize> = design
        .parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.refdes.as_str(), i))
        .collect();
    // The reference of the box that draws pad `pad` of part `p`: the part's own for a part that
    // is one box.
    let reference = |p: usize, pad: &str| -> String {
        match owner.get(&(p, pad)) {
            Some(&k) => names[k].clone(),
            None => design.parts[p].refdes.clone(),
        }
    };
    let nets: Vec<Net> = design
        .nets
        .iter()
        .map(|net| Net {
            name: net.name.clone(),
            pins: net
                .pins
                .iter()
                .map(|r| PinRef {
                    refdes: reference(index[r.refdes.as_str()], &r.pin),
                    pin: r.pin.clone(),
                })
                .collect(),
        })
        .collect();
    let mut must = std::collections::BTreeMap::new();
    for ((refdes, pad), message) in &marks.must {
        let key = match index.get(refdes.as_str()) {
            Some(&p) => reference(p, pad),
            None => refdes.clone(),
        };
        must.insert((key, pad.clone()), message.clone());
    }
    Split {
        design: Design {
            parts,
            nets,
            ..design.clone()
        },
        glyphs,
        origin,
        marks: Marks { must },
    }
}
