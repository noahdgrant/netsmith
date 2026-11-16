//! The shapes of the parts: their courtyards, text and silkscreen, as the annealer measures them.

use super::*;

/// The box the text the silkscreen prints for the part takes, on the board (its designator,
/// or the text the design gives in its place), or `None` for a part that prints none.
/// The text is upright whatever the part's turn: its place turns with the part, and its
/// letters do not (KiCad takes the angle of a footprint's text as the board's).
pub(crate) fn text_box(part: &Placed, rules: &Rules) -> Option<Rect> {
    if part.kind == "Label" || part.silk_text().is_none() {
        return None;
    }
    let (height, width) = (part.text_height(rules), part.text_width(rules));
    let c = part.to_board(part.reference_at(rules));
    Some(
        Rect::new(
            Pt::new(c.x - width / 2.0, c.y - height / 2.0),
            Pt::new(c.x + width / 2.0, c.y + height / 2.0),
        )
        .expanded(TEXT_PAD),
    )
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
    // The strokes of the silkscreen, each as a box (a rectangle as its four sides).
    let silk: Vec<Rect> = part
        .footprint
        .graphics
        .iter()
        .filter(|g| g.layer == netsmith_pcb::Layer::Silk)
        .flat_map(|g| {
            let pieces: Vec<Vec<Pt>> = match &g.shape {
                netsmith_pcb::Shape::Line { a, b } => vec![vec![*a, *b]],
                netsmith_pcb::Shape::Rect { a, b } => {
                    let (c, d) = (Pt::new(b.x, a.y), Pt::new(a.x, b.y));
                    vec![vec![*a, c], vec![c, *b], vec![*b, d], vec![d, *a]]
                }
                netsmith_pcb::Shape::Poly { points } => {
                    points.windows(2).map(|w| vec![w[0], w[1]]).collect()
                }
                other => vec![other.extent_points()],
            };
            let stroke = g.stroke;
            pieces
                .into_iter()
                .filter_map(move |pts| Rect::around(pts).map(|r| r.expanded(stroke / 2.0)))
        })
        .map(|r| part.rect_to_board(r))
        .collect();
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
    for r in text.iter().chain(silk.iter()).chain(copper.iter()) {
        reach = reach.union(r);
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
