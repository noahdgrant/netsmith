//! The shapes of the parts: their courtyards, text and silkscreen, as the annealer measures them.

use super::*;

/// The box the text the silkscreen prints for the part takes, on the board (its designator,
/// or the text the design gives in its place), or `None` for a part that prints none.
/// The text is upright whatever the part's turn: its place turns with the part, and its
/// letters do not (KiCad takes the angle of a footprint's text as the board's); a layout file
/// entry may place and turn it (issue #1506).
pub(crate) fn text_box(part: &Placed, rules: &Rules) -> Option<Rect> {
    part.text_rect(rules).map(|r| r.expanded(TEXT_PAD))
}

pub(super) fn shape_of(
    part: &mut Placed,
    angle: f64,
    text_side: ReferenceSide,
    rules: &Rules,
    reach_pad: f64,
) -> Shape {
    let (at, rotation, side) = (part.at, part.rotation, part.reference_side);
    part.at = Pt::default();
    part.rotation = angle;
    part.reference_side = text_side;
    let court = part.courtyard();
    let courts = part.courtyards();
    let body = part.body();
    let text = text_box(part, rules);
    // The silkscreen's lines and areas, and the text the footprint prints besides the designator.
    let silk = silk::pieces(part, rules);
    let pads: Vec<Pt> = part
        .footprint
        .pads
        .iter()
        .map(|p| part.pad_centre(p))
        .collect();
    let mut copper: Vec<Rect> = part
        .footprint
        .pads
        .iter()
        .filter(|p| part.pad_layers(p) != (false, false))
        .map(|p| part.pad_bounds(p))
        .collect();
    let pad_boxes = copper.clone();
    let pad_rects: Vec<Rect> = part
        .footprint
        .pads
        .iter()
        .map(|p| part.pad_bounds(p))
        .collect();
    let pad_inset = pad_boxes
        .iter()
        .map(|b| {
            (b.min.x - court.min.x)
                .min(court.max.x - b.max.x)
                .min(b.min.y - court.min.y)
                .min(court.max.y - b.max.y)
        })
        .fold(f64::MAX, f64::min);
    let rooms: Vec<Rect> = part
        .footprint
        .pads
        .iter()
        .map(|p| part.pad_bounds(p).expanded(rules.clearance))
        .collect();
    copper.extend(part.mask_openings());
    let ring = fine_pitch(part).then(|| {
        part.footprint
            .pads
            .iter()
            .filter(|p| p.drill.is_none())
            .map(|p| part.pad_bounds(p))
            .reduce(|a, b| a.union(&b))
            .map(|span| span.expanded(RING))
    });
    part.at = at;
    part.rotation = rotation;
    part.reference_side = side;
    let mut reach = court;
    for r in text
        .iter()
        .copied()
        .chain(silk.iter().map(silk::Piece::bounds))
        .chain(copper.iter().copied())
    {
        reach = reach.union(&r);
    }
    Shape {
        court,
        courts,
        body,
        text,
        silk,
        pads,
        copper,
        pad_boxes,
        pad_inset,
        pad_rects,
        rooms,
        ring: ring.flatten(),
        reach: reach.expanded(reach_pad),
    }
}

/// Whether a part is fine-pitch and gets an escape ring: the router's test for escape lanes
/// (`fine_pitch` in `netsmith_router::route`), on every surface-mount pad and not only the
/// pads on a net. At least 16 surface-mount pads, two of them 0.55 mm apart or nearer (pads at
/// one centre, the pieces of an exposed pad, are not counted apart), facing at least three
/// sides of the box round them: a QFN or a QFP, not a connector's single row.
pub(crate) fn fine_pitch(part: &Placed) -> bool {
    let pads: Vec<&netsmith_pcb::Pad> = part
        .footprint
        .pads
        .iter()
        .filter(|p| p.drill.is_none())
        .collect();
    if pads.len() < 16 {
        return false;
    }
    let near = pads.iter().enumerate().any(|(a, p)| {
        pads.iter().skip(a + 1).any(|q| {
            let d = part.pad_centre(p).distance(part.pad_centre(q));
            d > 1e-6 && d <= 0.55 + 1e-9
        })
    });
    if !near {
        return false;
    }
    let Some(span) = pads
        .iter()
        .map(|p| part.pad_bounds(p))
        .reduce(|a, b| a.union(&b))
    else {
        return false;
    };
    // The side of the box each pad is on, as the router finds it: of two, the one that points
    // away from the part's origin; a pad on none (an exposed pad) faces no side.
    let mut faces: Vec<(i8, i8)> = Vec::new();
    for pad in &pads {
        let b = part.pad_bounds(pad);
        let c = b.centre();
        let sides = [
            ((-1i8, 0i8), b.min.x - span.min.x),
            ((1, 0), span.max.x - b.max.x),
            ((0, -1), b.min.y - span.min.y),
            ((0, 1), span.max.y - b.max.y),
        ];
        let away = |(dx, dy): (i8, i8)| {
            f64::from(dx) * (c.x - part.at.x) + f64::from(dy) * (c.y - part.at.y)
        };
        if let Some(&(face, _)) = sides
            .iter()
            .filter(|(_, gap)| *gap <= 1e-6)
            .max_by(|a, b| away(a.0).total_cmp(&away(b.0)))
            && !faces.contains(&face)
        {
            faces.push(face);
        }
    }
    faces.len() >= 3
}

/// The box of a keepout in a part's own frame, relative to the part's origin, with the part
/// turned to `angle`: the box that holds it, as `Board::keepout_areas` finds it on the board.
pub(super) fn carried_box(part: &mut Placed, angle: f64, local: Rect) -> Rect {
    let (at, rotation) = (part.at, part.rotation);
    part.at = Pt::default();
    part.rotation = angle;
    let found = Rect::around(local.corners().map(|c| part.to_board(c))).unwrap_or(local);
    part.at = at;
    part.rotation = rotation;
    found
}

/// How far two boxes overlap, as the least of the overlaps along the axes after growing the
/// first by `gap`: positive when they are nearer than `gap` (and overlapping if more than
/// `gap`).
pub(super) fn penetration(a: &Rect, b: &Rect, gap: f64) -> f64 {
    let dx = a.max.x.min(b.max.x) - a.min.x.max(b.min.x);
    let dy = a.max.y.min(b.max.y) - a.min.y.max(b.min.y);
    if dx > -gap && dy > -gap {
        dx.min(dy) + gap
    } else {
        0.0
    }
}
