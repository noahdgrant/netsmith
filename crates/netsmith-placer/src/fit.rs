//! Placing the parts inside an outline the design states (ADR 0020, item 3).
//!
//! The parts that are not mechanical are arranged as for a board that is sized to fit, and
//! the arrangement is centred in the outline. If the outline is too small for it, the
//! error says by how much. Mounting holes and fiducials go in the corners of the outline.

use netsmith_core::{Outline, PartClass};

use crate::place::{self, Context};
use netsmith_pcb::{
    BoardEdge, BoardNet, DoesNotFit, Fixed, ORIGIN, PlaceOptions, Placed, Polygon, Pt, Rect,
    RoundRect, Rules, is_mechanical,
};

/// The space between mechanical parts side by side in a corner.
const GAP: f64 = 1.0;
/// Space kept between silkscreen and the board edge, on top of the copper to edge space.
const SILK_EDGE: f64 = 0.3;

/// Round up to a tenth of a millimetre.
fn up(v: f64) -> f64 {
    (v * 10.0 - 1e-9).ceil() / 10.0
}

fn tidy(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The size along one axis: the least the design allows that holds `need`, or the
/// arrangement's own size if the design puts no limit on it.
fn axis(range: (f64, f64), need: f64, arranged: f64) -> Result<f64, f64> {
    let (lo, hi) = range;
    if need > hi + 1e-9 {
        return Err(need - hi);
    }
    let natural = if hi.is_finite() { up(need) } else { arranged };
    Ok(lo.max(natural.min(hi)))
}

/// Lay `parts` out inside the outline the design states, and return its rectangle.
///
/// The arrangement is the placer's ([`place::arrange_best`]) given the size to fit
/// (`PlaceOptions::fit`), with its keepouts and the faces the parts were told to be on. A
/// size that is a range is the least that holds the parts, within the range; if the parts
/// need more than its top, the placer says by how much.
///
/// A polygon outline (issue #57, on the sheet) is exactly the size of its box: the parts are
/// laid out in the box, clear of the part of it outside the polygon (which `placement` holds
/// among its cutouts), and the corners and the pads are measured against the polygon.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fit(
    parts: &mut [Placed],
    nets: &[BoardNet],
    outline: &Outline,
    polygon: Option<&Polygon>,
    rules: &Rules,
    placement: &PlaceOptions,
    planes: &[String],
    pours: bool,
) -> Result<Rect, DoesNotFit> {
    // A polygon is its box's size, whatever the ranges say.
    let outline = &match polygon {
        Some(p) => {
            let b = p.bounds();
            Outline {
                width: (b.width(), b.width()),
                height: (b.height(), b.height()),
                corner_radius: 0.0,
                polygon: None,
            }
        }
        None => outline.clone(),
    };
    let context = Context {
        rules,
        planes,
        corner_radius: outline.corner_radius,
        pours,
    };
    let (mechanical, movable): (Vec<usize>, Vec<usize>) =
        (0..parts.len()).partition(|&i| is_mechanical(parts[i].class));
    let originals: Vec<Placed> = movable.iter().map(|&i| parts[i].clone()).collect();
    let group = || originals.clone();

    // What the parts need, if the board is as big as they like.
    let mut natural = group();
    let sized = PlaceOptions {
        fit: None,
        ..placement.clone()
    };
    let arranged = place::arrange_best(&mut natural, nets, &sized, &context)?;
    let need = place::boxes(&natural, rules).map_or((0.0, 0.0), |(court, all)| {
        place::needs_of(court, all, placement.wall()).0
    });

    let (w, h) = (
        axis(outline.width, need.0, arranged.width()),
        axis(outline.height, need.1, arranged.height()),
    );
    // The size to lay out in: the one the range gives, or its top where the parts need more
    // (so that the placer, which may share the faces, says by how much if it cannot).
    let top = |range: (f64, f64), natural: f64| {
        if range.1.is_finite() {
            range.1
        } else {
            natural
        }
    };
    let size = (
        w.unwrap_or_else(|_| top(outline.width, arranged.width())),
        h.unwrap_or_else(|_| top(outline.height, arranged.height())),
    );
    // The mechanical parts are in the corners of the outline, whose size is known now; the
    // rows of the other parts keep clear of them.
    let outline_rect = Rect::new(ORIGIN, Pt::new(ORIGIN.x + size.0, ORIGIN.y + size.1));
    let edge = match polygon {
        Some(p) => BoardEdge::Polygon(p.clone()),
        None => BoardEdge::Round(RoundRect::new(outline_rect, outline.corner_radius)),
    };
    corners(parts, &mechanical, outline_rect, &edge, rules);
    // A mechanical part the layout file places is where it says.
    for &i in &mechanical {
        if let Some(f) = placement.fixed.get(&parts[i].refdes) {
            parts[i].at = ORIGIN + f.at;
            parts[i].rotation = f.rotation;
            parts[i].side = f.side;
            parts[i].reference_side = f.text;
        }
    }
    // A mechanical part the design's requirements govern is put in the corner and then moved
    // to where they allow.
    for &i in &mechanical {
        if let Some(want) = placement.wants.get(&parts[i].refdes) {
            parts[i].at = want.clamp(parts[i].rotation, parts[i].at);
        }
    }
    let mut keepouts = placement.keepouts.clone();
    keepouts.extend(mechanical.iter().map(|&i| parts[i].courtyard()));
    let mut placed = natural;
    let mut rect = outline_rect;
    let mut done = false;
    // (Not with requirements on positions: shifting the arrangement would move the parts
    // out of their windows, so the parts are laid out and annealed in the size instead. A
    // relation between two parts is one too: the arrangement that stands is the rows', which
    // do not know it.)
    if w.is_ok() && h.is_ok() && placement.wants.is_empty() && placement.offsets.is_empty() {
        // The arrangement the parts make fits the size: it stands, in the middle of the
        // outline, and moves only where a hole or a keepout is in its way.
        let spare = Pt::new(size.0 - arranged.width(), size.1 - arranged.height());
        for p in placed
            .iter_mut()
            .filter(|p| !placement.fixed.contains_key(&p.refdes))
        {
            p.at = Pt::new(
                tidy(p.at.x + (spare.x / 2.0 / 0.05).round() * 0.05),
                tidy(p.at.y + (spare.y / 2.0 / 0.05).round() * 0.05),
            );
        }
        // The rows keep the courtyards off the edge; a pad that sticks out of its courtyard
        // may still be nearer than the process allows, which only the annealer sees to.
        let pads_clear = placed.iter().all(|p| {
            p.footprint
                .pads
                .iter()
                .filter(|pad| p.pad_layers(pad) != (false, false))
                .all(|pad| edge.box_depth(&p.pad_bounds(pad)) >= rules.copper_to_edge - 1e-9)
        });
        // Nor may a courtyard stand outside a polygon: the arrangement was made for its box.
        let inside = polygon.is_none()
            || placed
                .iter()
                .all(|p| edge.box_depth(&p.courtyard()) >= -1e-9);
        done = pads_clear
            && inside
            && ((keepouts.is_empty() && placement.height_zones.is_empty()) || {
                // The mechanical parts are in the corners, and stay: with them in the run, their
                // reference text and silkscreen are kept clear of too.
                let count = placed.len();
                let mut options = placement.clone();
                for &i in &mechanical {
                    placed.push(parts[i].clone());
                    options.fixed.insert(
                        parts[i].refdes.clone(),
                        Fixed {
                            at: parts[i].at - ORIGIN,
                            rotation: parts[i].rotation,
                            side: parts[i].side,
                            text: parts[i].reference_side,
                        },
                    );
                }
                let ok = place::anneal_within(
                    &mut placed,
                    nets,
                    &options,
                    &context,
                    outline_rect,
                    &placement.keepouts,
                );
                placed.truncate(count);
                ok
            });
    }
    if !done {
        // The parts need more than the range gives, or could not be kept clear of what is in
        // the way: lay them out in the size from the start, sharing the faces if that helps.
        placed = group();
        let fixed = PlaceOptions {
            fit: Some(size),
            keepouts,
            ..placement.clone()
        };
        rect = place::arrange_best(&mut placed, nets, &fixed, &context)?;
    }
    for (k, &i) in movable.iter().enumerate() {
        parts[i].at = placed[k].at;
        parts[i].rotation = placed[k].rotation;
        parts[i].reference_side = placed[k].reference_side;
        parts[i].side = placed[k].side;
    }
    Ok(rect)
}

/// Put the mechanical parts in the corners: holes first, then fiducials, one to a corner
/// clockwise from the top left, and a second round beside the first. The corners are those
/// of the outline's box `r`; a part is moved in along the corner's diagonal until it is far
/// enough inside the edge, which for a polygon whose box corner is cut away (an L) is the
/// inner corner nearest it.
fn corners(parts: &mut [Placed], mechanical: &[usize], r: Rect, shape: &BoardEdge, rules: &Rules) {
    let mut order = mechanical.to_vec();
    let rank = |class: Option<PartClass>| match class {
        Some(PartClass::MountingHole) => 0,
        Some(PartClass::PlatedHole) => 1,
        _ => 2,
    };
    order.sort_by(|&a, &b| {
        (rank(parts[a].class), &parts[a].refdes).cmp(&(rank(parts[b].class), &parts[b].refdes))
    });
    // How far from the side edge each corner has been used, counting the gap after it.
    let mut used = [0.0f64; 4];
    for (n, &i) in order.iter().enumerate() {
        let k = n % 4;
        let c = parts[i].footprint.courtyard;
        let (w, h) = (c.width(), c.height());
        let (left, top) = (k == 0 || k == 3, k < 2);
        // The reference text sits above the courtyard: it has to be inside the board too.
        let text_top =
            parts[i].reference_at(rules).y - rules.min_text_height / 2.0 - rules.min_text_thickness;
        let above = (c.min.y - text_top).max(0.0);
        // Inset the part from the edge until all four corners of its courtyard and text are
        // the least distance inside the shape, which a rounded corner makes more than a
        // straight one.
        let mut inset = rules.copper_to_edge + SILK_EDGE;
        let origin = loop {
            let along = inset.max(used[k]);
            let x = if left {
                r.min.x + along
            } else {
                r.max.x - along - w
            };
            let y = if top {
                r.min.y + inset + above
            } else {
                r.max.y - inset - h
            };
            let keep_in = Rect::new(Pt::new(x, y - above), Pt::new(x + w, y + h));
            let depth = shape.box_depth(&keep_in);
            if depth >= rules.copper_to_edge + SILK_EDGE - 1e-9 || inset > r.width().max(r.height())
            {
                break Pt::new(x, y);
            }
            inset += 0.1;
        };
        parts[i].at = Pt::new(tidy(origin.x - c.min.x), tidy(origin.y - c.min.y));
        let along = if left {
            origin.x - r.min.x
        } else {
            r.max.x - origin.x - w
        };
        used[k] = along + w + GAP;
    }
}
