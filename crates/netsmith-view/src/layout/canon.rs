//! The design in a fixed order, so that no tie in the engine is decided by where a part or a
//! net happens to be in the design's lists.
//!
//! The engine breaks many ties by index (the lower part, the first net). Index is where the
//! design's author or the elaborator happened to put a thing, which an edit changes (a part is
//! inserted, a module is moved) and which means nothing on the sheet. This module draws the
//! design with its parts sorted by path (else reference), in natural order (`c2` before `c10`, as
//! the designators are numbered), its nets by their first pin (never by name: a rename moves
//! nothing) and the pins of a net by part and pad; the drawing is then a function of the *set* of parts and nets, and [`Canon::restore`]
//! puts the layout back into the design's own order (`layout.parts[i]` is part `i`).
//! A test presents the design in reverse order and requires the same drawing.

use std::cmp::Ordering;

use netsmith_core::{Design, Net, PinRef};

use super::{Layout, SpineMark};

/// Compares two names with a run of digits as a number: `c2` < `c10`.
pub(crate) fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) if p.is_ascii_digit() && q.is_ascii_digit() => {
                let run = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(&c) = it.peek() {
                        if !c.is_ascii_digit() {
                            break;
                        }
                        s.push(c);
                        it.next();
                    }
                    s
                };
                let (m, n) = (run(&mut x), run(&mut y));
                let (m, n) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                let by = m.len().cmp(&n.len()).then_with(|| m.cmp(n));
                if by != Ordering::Equal {
                    return by;
                }
            }
            (Some(p), Some(q)) => {
                if p != q {
                    return p.cmp(&q);
                }
                x.next();
                y.next();
            }
        }
    }
}

/// What a part is called across edits: its path, else its reference.
pub(crate) fn part_name(p: &netsmith_core::Part) -> &str {
    if p.path.is_empty() {
        &p.refdes
    } else {
        &p.path
    }
}

/// A design in canonical order, and how to get back.
pub(super) struct Canon {
    pub design: Design,
    /// For each canonical part, its index in the original design.
    parts: Vec<usize>,
    /// For each canonical net, its index in the original design.
    nets: Vec<usize>,
}

impl Canon {
    pub fn of(design: &Design) -> Canon {
        let mut parts: Vec<usize> = (0..design.parts.len()).collect();
        parts.sort_by(|&a, &b| {
            let (p, q) = (&design.parts[a], &design.parts[b]);
            natural_cmp(part_name(p), part_name(q))
                .then_with(|| natural_cmp(&p.refdes, &q.refdes))
                .then(a.cmp(&b))
        });
        // The rank of each part, for the order of the pins of a net and of the nets.
        let mut rank = std::collections::HashMap::new();
        for (r, &p) in parts.iter().enumerate() {
            rank.insert(design.parts[p].refdes.as_str(), r);
        }
        let by_part = |x: &PinRef| rank.get(x.refdes.as_str()).copied().unwrap_or(usize::MAX);
        // A net is known by its first pin, not by its name: renaming a net moves nothing, and a
        // pin added to a net moves it only when it is the new first.
        let first_pin = |n: usize| {
            design.nets[n]
                .pins
                .iter()
                .min_by(|p, q| {
                    by_part(p)
                        .cmp(&by_part(q))
                        .then_with(|| natural_cmp(&p.pin, &q.pin))
                })
                .map(|p| (by_part(p), p.pin.clone()))
        };
        let mut nets: Vec<usize> = (0..design.nets.len()).collect();
        nets.sort_by(|&a, &b| {
            match (first_pin(a), first_pin(b)) {
                (Some(x), Some(y)) => x.0.cmp(&y.0).then_with(|| natural_cmp(&x.1, &y.1)),
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (None, None) => Ordering::Equal,
            }
            .then_with(|| natural_cmp(&design.nets[a].name, &design.nets[b].name))
            .then(a.cmp(&b))
        });
        let canon_nets: Vec<Net> = nets
            .iter()
            .map(|&n| {
                let net = &design.nets[n];
                let mut pins: Vec<PinRef> = net.pins.clone();
                pins.sort_by(|p, q| {
                    by_part(p)
                        .cmp(&by_part(q))
                        .then_with(|| natural_cmp(&p.pin, &q.pin))
                });
                Net {
                    name: net.name.clone(),
                    pins,
                }
            })
            .collect();
        Canon {
            design: Design {
                modules: design.modules.clone(),
                name: design.name.clone(),
                parts: parts.iter().map(|&p| design.parts[p].clone()).collect(),
                nets: canon_nets,
                process: design.process.clone(),
                features: design.features.clone(),
                interfaces: design.interfaces.clone(),
                notes: design.notes.clone(),
            },
            parts,
            nets,
        }
    }

    /// For each part of the original design, its index in the canonical design.
    pub fn canonical_parts(&self) -> Vec<usize> {
        let mut at = vec![0; self.parts.len()];
        for (c, &o) in self.parts.iter().enumerate() {
            at[o] = c;
        }
        at
    }

    /// For each part of the canonical design, its index in the original design.
    pub fn original_parts(&self) -> &[usize] {
        &self.parts
    }

    /// `external[n]` of the original design's nets as the canonical design's.
    pub fn nets_of<T: Clone>(&self, original: &[T]) -> Vec<T> {
        self.nets.iter().map(|&n| original[n].clone()).collect()
    }

    /// The layout of the canonical design as the layout of the original: parts in the
    /// design's order (the boxes of further units after them), nets and parts by their
    /// original indices.
    pub fn restore(&self, layout: &mut Layout) {
        let n = self.parts.len();
        let part = |p: usize| self.parts.get(p).copied().unwrap_or(p);
        let net = |k: usize| self.nets.get(k).copied().unwrap_or(k);
        let mut parts: Vec<Option<super::PlacedPart>> = vec![None; layout.parts.len()];
        for (k, mut placed) in std::mem::take(&mut layout.parts).into_iter().enumerate() {
            placed.part = part(placed.part);
            for pin in &mut placed.pins {
                pin.net = pin.net.map(net);
            }
            let at = if k < n { self.parts[k] } else { k };
            parts[at] = Some(placed);
        }
        layout.parts = parts
            .into_iter()
            .map(|p| p.expect("every box has a place"))
            .collect();
        for s in &mut layout.symbols {
            s.net = net(s.net);
        }
        for p in &mut layout.ports {
            p.net = net(p.net);
        }
        for m in &mut layout.marks {
            m.part = part(m.part);
        }
        for w in &mut layout.wires {
            w.net = net(w.net);
        }
        for j in &mut layout.junctions {
            j.net = net(j.net);
        }
        for l in &mut layout.labels {
            l.net = net(l.net);
        }
        for b in &mut layout.buses {
            for m in &mut b.members {
                m.net = net(m.net);
            }
            for e in &mut b.entries {
                e.net = net(e.net);
            }
            for e in &mut b.ends {
                *e = part(*e);
            }
        }
        for note in &mut layout.notes {
            note.anchor = note.anchor.map(part);
        }
        let boxed = |p: usize| if p < n { self.parts[p] } else { p };
        layout.spines = std::mem::take(&mut layout.spines)
            .into_iter()
            .map(|s| SpineMark {
                ic: boxed(s.ic),
                chains: s
                    .chains
                    .into_iter()
                    .map(|c| c.into_iter().map(boxed).collect())
                    .collect(),
            })
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_sort_as_numbers() {
        let mut v = vec!["c10", "c2", "c1", "b9"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["b9", "c1", "c2", "c10"]);
        assert_eq!(natural_cmp("a.b2", "a.b10"), Ordering::Less);
        assert_eq!(natural_cmp("x", "x"), Ordering::Equal);
        assert_eq!(natural_cmp("x", "x1"), Ordering::Less);
    }
}
