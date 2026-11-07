//! Where a sheet's coordinates start.
//!
//! The engine places the first piece of the sheet at the left of the first row, so the origin
//! is the corner of whatever is drawn: an edit that makes the sheet grow upward or leftward
//! (a capacitor above a pin, a longer name at the left) moves every coordinate of the sheet,
//! and with them every part, wire and name, though none of them changed. The drawing is
//! instead translated so that its **reference part**, the largest part of the sheet, is at the
//! origin (on the grid): what grows on any side leaves the reference part, and with it
//! everything that did not change, where it was. The page reads the box of each sheet
//! (`data-bounds`, the `viewBox`), which may start below zero.

use netsmith_core::Design;

use super::{GRID, Layout, PlacedPart};
use crate::geom::{Pt, Rect, snap};

/// What the choice of a sheet's reference part reads of a box: its pins, the size of its
/// symbol, its name (path, else reference) and its unit.
pub(super) struct RefKey<'a> {
    pub pins: usize,
    pub area: i64,
    pub name: &'a str,
    pub unit: Option<u32>,
}

impl<'a> RefKey<'a> {
    pub fn of(glyph: &crate::glyph::Glyph, pins: usize, name: &'a str) -> Self {
        let r = glyph.bounds();
        RefKey {
            pins,
            area: ((r.max.x - r.min.x) * (r.max.y - r.min.y) * 100.0).round() as i64,
            name,
            unit: glyph.unit,
        }
    }
}

/// The reference among `boxes`: the one with most pins, then the largest symbol, then the
/// first by name (the smaller name is the greater: `max_by` keeps the first of equals), then
/// the first unit. The engine and [`reference`] choose by this one rule, so that a `pin` of the
/// layout file is from the part the frame starts at.
pub(super) fn reference_among<'a>(
    boxes: impl Iterator<Item = usize>,
    key: impl Fn(usize) -> RefKey<'a>,
) -> Option<usize> {
    boxes.max_by(|&a, &b| {
        let (ka, kb) = (key(a), key(b));
        (ka.pins, ka.area).cmp(&(kb.pins, kb.area)).then_with(|| {
            super::canon::natural_cmp(kb.name, ka.name).then_with(|| kb.unit.cmp(&ka.unit))
        })
    })
}

/// The part of `layout` that is the sheet's reference: the box with most pins, then the
/// largest symbol, then the first by path (else reference). A drawing with no part has none.
pub(super) fn reference(design: &Design, layout: &Layout) -> Option<usize> {
    let names: Vec<&str> = layout
        .parts
        .iter()
        .map(|p| {
            let part = &design.parts[p.part];
            if part.path.is_empty() {
                part.refdes.as_str()
            } else {
                part.path.as_str()
            }
        })
        .collect();
    reference_among(0..layout.parts.len(), |i| {
        let p: &PlacedPart = &layout.parts[i];
        RefKey::of(&p.glyph, p.pins.len(), names[i])
    })
}

/// Moves the drawing so that its reference part's origin is at (0, 0) on the grid. `hinted`
/// is how far the layout file moved each box from where the drawing put it (by index of
/// `layout.parts`; empty for none): the frame starts where the reference part would have
/// been, so that a part moved by the file does not move the frame, and an offset or a pin
/// means the same however the rest of the sheet is drawn.
pub(super) fn anchor(design: &Design, layout: &mut Layout, hinted: &[Pt]) {
    let Some(r) = reference(design, layout) else {
        return;
    };
    let mut at = layout.parts[r].at;
    if let Some(by) = hinted.get(r) {
        at = Pt::new(at.x - by.x, at.y - by.y);
    }
    translate(
        layout,
        Pt::new(0.0 - snap(at.x, GRID), 0.0 - snap(at.y, GRID)),
    );
}

/// Marks the reference part of each sheet of `layout` (`PlacedPart::is_reference`), by the rule the
/// engine and [`anchor`] use. `sheet_of` is the sheet of each part when one drawing holds several
/// (a small board drawn as one picture; a `pin` is from the reference part of its own sheet),
/// and none when the drawing is one sheet. The box of a further unit is a box of its part's sheet.
pub(super) fn mark_references(design: &Design, layout: &mut Layout, sheet_of: Option<&[usize]>) {
    let mut sheets: std::collections::BTreeMap<usize, Vec<usize>> =
        std::collections::BTreeMap::new();
    for p in &mut layout.parts {
        p.is_reference = false;
    }
    for (k, p) in layout.parts.iter().enumerate() {
        let sheet = sheet_of.and_then(|s| s.get(p.part)).copied().unwrap_or(0);
        sheets.entry(sheet).or_default().push(k);
    }
    let marked: Vec<usize> = {
        let parts = &layout.parts;
        sheets
            .values()
            .filter_map(|boxes| {
                reference_among(boxes.iter().copied(), |k| {
                    let p: &PlacedPart = &parts[k];
                    RefKey::of(
                        &p.glyph,
                        p.pins.len(),
                        super::canon::part_name(&design.parts[p.part]),
                    )
                })
            })
            .collect()
    };
    for k in marked {
        layout.parts[k].is_reference = true;
    }
}

/// A coordinate to a millionth of a millimetre: a sum of grid steps carries no stray digits.
fn round(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

fn shifted(p: Pt, by: Pt) -> Pt {
    Pt::new(round(p.x + by.x), round(p.y + by.y))
}

fn moved(r: Rect, by: Pt) -> Rect {
    Rect {
        min: shifted(r.min, by),
        max: shifted(r.max, by),
    }
}

fn text(t: &mut super::Text, by: Pt) {
    t.at = shifted(t.at, by);
}

/// Moves everything of `layout` by `by`.
pub(super) fn translate(layout: &mut Layout, by: Pt) {
    if by.x == 0.0 && by.y == 0.0 {
        return;
    }
    for part in &mut layout.parts {
        part.at = shifted(part.at, by);
        part.extent = moved(part.extent, by);
        for pin in &mut part.pins {
            pin.at = shifted(pin.at, by);
        }
        text(&mut part.reference, by);
        if let Some(v) = &mut part.value {
            text(v, by);
        }
    }
    for s in &mut layout.symbols {
        s.at = shifted(s.at, by);
        if let Some(l) = &mut s.label {
            text(l, by);
        }
    }
    for p in &mut layout.ports {
        p.at = shifted(p.at, by);
        if let Some(l) = &mut p.label {
            text(l, by);
        }
    }
    for m in &mut layout.marks {
        m.at = shifted(m.at, by);
    }
    for w in &mut layout.wires {
        for p in &mut w.points {
            *p = shifted(*p, by);
        }
    }
    for j in &mut layout.junctions {
        j.at = shifted(j.at, by);
    }
    for l in &mut layout.labels {
        l.at = shifted(l.at, by);
        text(&mut l.text, by);
    }
    for b in &mut layout.buses {
        for line in &mut b.lines {
            for p in line.iter_mut() {
                *p = shifted(*p, by);
            }
        }
        for e in &mut b.entries {
            e.at = shifted(e.at, by);
        }
        if let Some(f) = &mut b.flag {
            f.at = shifted(f.at, by);
            text(&mut f.label, by);
        }
        text(&mut b.label, by);
        text(&mut b.list, by);
        if let Some(o) = &mut b.other {
            text(o, by);
        }
        b.tick = (shifted(b.tick.0, by), shifted(b.tick.1, by));
        text(&mut b.count, by);
    }
    for n in &mut layout.notes {
        n.frame = moved(n.frame, by);
        if let Some(h) = &mut n.heading {
            text(h, by);
        }
        for l in &mut n.lines {
            text(l, by);
        }
    }
    layout.bounds = moved(layout.bounds, by);
}
