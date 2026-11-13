//! The shapes of the parts: their courtyards, text and silkscreen, as the annealer measures them.

use super::*;

/// The box the reference text takes, on the board, or `None` for a part that prints none.
/// The text is upright whatever the part's turn: its place turns with the part, and its
/// letters do not (KiCad takes the angle of a footprint's text as the board's).
pub(crate) fn text_box(part: &Placed, rules: &Rules) -> Option<Rect> {
    if part.kind == "Label" {
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
    part.at = at;
    part.rotation = rotation;
    part.reference_side = side;
    let mut reach = court;
    for r in text.iter().chain(silk.iter()).chain(copper.iter()) {
        reach = reach.union(r);
    }
    Shape {
        court,
        body,
        text,
        silk,
        pads,
        copper,
        pad_boxes,
        pad_inset,
        rooms,
        reach: reach.expanded(reach_pad),
    }
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
