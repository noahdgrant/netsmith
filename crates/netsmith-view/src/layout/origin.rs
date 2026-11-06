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

/// The part of `layout` that is the sheet's reference: the box with most pins, then the
/// largest symbol, then the first by path (else reference). A drawing with no part has none.
pub(super) fn reference(design: &Design, layout: &Layout) -> Option<usize> {
    let key = |p: &PlacedPart| -> (usize, i64) {
        let r = p.glyph.bounds();
        (
            p.pins.len(),
            ((r.max.x - r.min.x) * (r.max.y - r.min.y) * 100.0).round() as i64,
        )
    };
    let name = |p: &PlacedPart| {
        let part = &design.parts[p.part];
        if part.path.is_empty() {
            part.refdes.clone()
        } else {
            part.path.clone()
        }
    };
    (0..layout.parts.len()).max_by(|&a, &b| {
        let (pa, pb) = (&layout.parts[a], &layout.parts[b]);
        key(pa)
            .cmp(&key(pb))
            // The smaller name is the greater: `max_by` keeps the first of equals.
            .then_with(|| {
                super::canon::natural_cmp(&name(pb), &name(pa))
                    .then_with(|| pb.glyph.unit.cmp(&pa.glyph.unit))
            })
    })
}

/// Moves the drawing so that its reference part's origin is at (0, 0) on the grid.
pub(super) fn anchor(design: &Design, layout: &mut Layout) {
    let Some(r) = reference(design, layout) else {
        return;
    };
    let at = layout.parts[r].at;
    translate(
        layout,
        Pt::new(0.0 - snap(at.x, GRID), 0.0 - snap(at.y, GRID)),
    );
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
