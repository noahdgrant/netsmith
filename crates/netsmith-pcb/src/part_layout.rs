//! The layout guidelines of the parts' datasheets on a placed board (`layout` blocks, ADR 0030,
//! issue #829): which parts each `near` statement is about, found from the nets, and how far
//! they are from the pin they belong next to.
//!
//! A part's `near` names its own pins. `bridging .a .b` finds every other part with two pads,
//! one on the net of each pin; `on .pin kind K` every other part with a pad on the net of the
//! pin, of the kind when one is given. A part found is measured from its pads on the net of the
//! pin after `to` (all its pads, when none is on it) to the pads of that pin, the nearest pair,
//! centre to centre. The placer pulls each part found as it pulls a decoupling capacitor
//! (`netsmith_placer`'s annealer), and `fab` warns when a number the datasheet gives is not met.

use std::collections::HashMap;

use crate::board::{BoardNet, Placed};

/// One `near` statement of a part, with the pads of the pins it names, as the board's part
/// carries it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PartNear {
    pub rule: netsmith_core::LayoutNear,
    /// The pads of the pin `to`.
    pub to_pads: Vec<String>,
    /// The pads of the pins of the selector: both pins of `bridging`, the one of `on`.
    pub select_pads: Vec<Vec<String>>,
}

impl PartNear {
    /// The guidelines of the part at `part` among `rules`, with the pads of their pins from the
    /// part's pin table. A pin with no pads in the package has none, and its guideline then
    /// finds no part.
    pub fn of_part(part: &netsmith_core::Part, rules: &[netsmith_core::LayoutNear]) -> Vec<Self> {
        let pads = |name: &str| -> Vec<String> {
            part.pins
                .iter()
                .filter(|p| p.name == name)
                .flat_map(|p| p.pads.iter().cloned())
                .collect()
        };
        rules
            .iter()
            .filter(|r| r.part == part.path)
            .map(|r| PartNear {
                rule: r.clone(),
                to_pads: pads(&r.to),
                select_pads: match &r.select {
                    netsmith_core::NearSelect::Bridging { a, b } => vec![pads(a), pads(b)],
                    netsmith_core::NearSelect::On { pin, .. } => vec![pads(pin)],
                },
            })
            .collect()
    }
}

/// A part a `near` statement found on the board.
#[derive(Debug, Clone, PartialEq)]
pub struct NearPair {
    /// The index of the part that states the guideline, and of the guideline in its
    /// [`Placed::layout`].
    pub part: usize,
    pub rule: usize,
    /// The index of the part found.
    pub other: usize,
    /// The numbers of the pads of the part found that are measured.
    pub other_pads: Vec<String>,
    /// The numbers of the pads of the pin `to`.
    pub to_pads: Vec<String>,
}

impl NearPair {
    /// The guideline, as the part states it.
    pub fn rule<'a>(&self, parts: &'a [Placed]) -> &'a netsmith_core::LayoutNear {
        &parts[self.part].layout[self.rule].rule
    }

    /// The distance from the nearest pad of the part found to the nearest pad of the pin, centre
    /// to centre, in mm; `None` when either has no pad on the footprint.
    pub fn distance(&self, parts: &[Placed]) -> Option<f64> {
        let (a, b) = (&parts[self.other], &parts[self.part]);
        let mut best: Option<f64> = None;
        for number in &self.other_pads {
            for p in a.footprint.pads_numbered(number) {
                let at = a.pad_centre(p);
                for to in &self.to_pads {
                    for q in b.footprint.pads_numbered(to) {
                        let d = at.distance(b.pad_centre(q));
                        best = Some(best.map_or(d, |x: f64| x.min(d)));
                    }
                }
            }
        }
        best
    }
}

/// Every part that a `near` statement of a part of the board finds, in the order of the parts
/// that state them, their guidelines, and the parts found.
pub fn near_pairs(parts: &[Placed], nets: &[BoardNet]) -> Vec<NearPair> {
    if parts.iter().all(|p| p.layout.is_empty()) {
        return Vec::new();
    }
    let index: HashMap<&str, usize> = parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.refdes.as_str(), i))
        .collect();
    let mut net_of: HashMap<(usize, &str), usize> = HashMap::new();
    for (n, net) in nets.iter().enumerate() {
        for pad in &net.pads {
            if let Some(&i) = index.get(pad.refdes.as_str()) {
                net_of.insert((i, pad.number.as_str()), n);
            }
        }
    }
    let nets_of = |i: usize, pads: &[String]| -> Vec<usize> {
        let mut out: Vec<usize> = pads
            .iter()
            .filter_map(|p| net_of.get(&(i, p.as_str())).copied())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    };
    let mut pairs = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        for (r, near) in part.layout.iter().enumerate() {
            let sides: Vec<Vec<usize>> = near.select_pads.iter().map(|p| nets_of(i, p)).collect();
            let to_nets = nets_of(i, &near.to_pads);
            for (j, other) in parts.iter().enumerate() {
                if j == i {
                    continue;
                }
                let numbers = other.footprint.pad_numbers();
                let on = |number: &str| net_of.get(&(j, number)).copied();
                let found = match &near.rule.select {
                    netsmith_core::NearSelect::Bridging { .. } => match numbers.as_slice() {
                        [x, y] => match (on(x), on(y)) {
                            (Some(nx), Some(ny)) => {
                                let (a, b) = (&sides[0], &sides[1]);
                                (a.contains(&nx) && b.contains(&ny))
                                    || (a.contains(&ny) && b.contains(&nx))
                            }
                            _ => false,
                        },
                        _ => false,
                    },
                    netsmith_core::NearSelect::On { kind, .. } => {
                        kind.is_none_or(|k| other.declared_kind == Some(k))
                            && numbers
                                .iter()
                                .any(|x| on(x).is_some_and(|n| sides[0].contains(&n)))
                    }
                };
                if !found {
                    continue;
                }
                let mut other_pads: Vec<String> = numbers
                    .iter()
                    .filter(|x| on(x).is_some_and(|n| to_nets.contains(&n)))
                    .map(|x| x.to_string())
                    .collect();
                if other_pads.is_empty() {
                    other_pads = numbers.iter().map(|x| x.to_string()).collect();
                }
                pairs.push(NearPair {
                    part: i,
                    rule: r,
                    other: j,
                    other_pads,
                    to_pads: near.to_pads.clone(),
                });
            }
        }
    }
    pairs
}

/// A sentence for each part that a `near` statement with a number finds further from the pin
/// than the number: the guideline, the distance, the source and what to do. A statement with no
/// number, and a part with no pad to measure, has none.
pub fn near_findings(parts: &[Placed], nets: &[BoardNet]) -> Vec<String> {
    near_pairs(parts, nets)
        .iter()
        .filter_map(|pair| {
            let rule = pair.rule(parts);
            let within = rule.within?;
            let distance = pair.distance(parts)?;
            if distance <= within + 1e-6 {
                return None;
            }
            let (part, other) = (&parts[pair.part], &parts[pair.other]);
            Some(format!(
                "`{}` ({}) asks for `{}` within {} mm of its pin `.{}` (pads {}), and {} (`{}`) is {} mm from it ({}; {}): move {} nearer {}, or turn it so that its pad faces the pin",
                part.path,
                part.refdes,
                rule.name,
                number(within),
                rule.to,
                pair.to_pads.join(", "),
                other.refdes,
                other.path,
                number(distance),
                rule.source,
                rule.site,
                other.refdes,
                part.refdes,
            ))
        })
        .collect()
}

/// A number with at most two decimals and no trailing zeros: `3`, `2.45`.
fn number(x: f64) -> String {
    let text = format!("{x:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}
