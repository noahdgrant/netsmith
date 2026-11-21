//! The grid of a pour's stitching vias (ADR 0020, item 2), shared by the router that lays the
//! vias (`netsmith_router::stitch`) and by `netsmith routes import`, which knows them again in a
//! board `fab` wrote (issue #1483).
//!
//! A pour is stitched when it states a pitch and its net has another layer: an inner plane, or
//! the pour of the other outer layer. The grid is at the pitch, from the outline's corner kept
//! the room a via needs from the edge, every other row shifted by half a pitch. A grid point
//! takes the via nearest it within a third of the pitch, so a via of the pour's net already that
//! near (a stitching via of an earlier export, or a via the router laid) stands for the point and
//! no other is put beside it.

use crate::{Board, BoardPour, CopperLayer, Pt, Via};

/// Space kept beyond the rules' clearance from the board edge, for rounding (the router's).
const MARGIN: f64 = 0.05;

/// Whether `net` has another layer than `layer` to be stitched to.
fn other_layers(board: &Board, net: &str, layer: CopperLayer) -> bool {
    board.planes.iter().any(|p| p.net == net)
        || board.pours.iter().any(|p| p.net == net && p.layer != layer)
}

/// The pours of `board` that are stitched, each with its pitch (the module's header).
pub fn stitched_pours(board: &Board) -> Vec<(&BoardPour, f64)> {
    board
        .pours
        .iter()
        .filter_map(|p| Some((p, p.stitch_pitch?)))
        .filter(|(p, _)| other_layers(board, &p.net, p.layer))
        .collect()
}

/// The points of the stitching grid at `pitch` on `board`, row by row.
pub fn stitch_grid(board: &Board, pitch: f64) -> Vec<Pt> {
    let mut points = Vec::new();
    if !(pitch.is_finite() && pitch > 0.0) {
        return points;
    }
    let rules = &board.rules;
    let o = board.outline;
    let reach = rules.copper_to_edge + rules.via_diameter / 2.0 + MARGIN;
    let (mut y, mut row) = (o.min.y + reach, 0);
    while y <= o.max.y - reach + 1e-9 {
        let mut x = o.min.x + reach + if row % 2 == 1 { pitch / 2.0 } else { 0.0 };
        while x <= o.max.x - reach + 1e-9 {
            points.push(Pt::new(x, y));
            x += pitch;
        }
        y += pitch;
        row += 1;
    }
    points
}

/// Whether `via` is one the stitching of a pour of its net would lay: a through via of the
/// rules' size, within a third of the pitch of a point of the grid of a stitched pour of its
/// net, and inside no pad. Whether a track ends on it is the caller's to say.
pub fn is_stitching_via(board: &Board, via: &Via) -> bool {
    let rules = &board.rules;
    let sized = via.span.is_none()
        && (via.diameter - rules.via_diameter).abs() < 1e-6
        && (via.drill - rules.via_drill).abs() < 1e-6;
    if !sized {
        return false;
    }
    let in_pad = board.parts.iter().any(|part| {
        part.footprint.pads.iter().any(|pad| {
            part.pad_bounds(pad)
                .contains(&crate::Rect::new(via.at, via.at))
        })
    });
    if in_pad {
        return false;
    }
    stitched_pours(board)
        .into_iter()
        .filter(|(p, _)| p.net == via.net)
        .any(|(_, pitch)| {
            stitch_grid(board, pitch)
                .iter()
                .any(|g| g.distance(via.at) <= pitch / 3.0 + 1e-6)
        })
}
