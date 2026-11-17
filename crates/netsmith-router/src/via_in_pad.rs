//! What it cost to refuse vias in pads (issue #1118).
//!
//! A board whose process fills and caps vias in pads lets the router stand a via in a
//! surface-mount pad as a last resort, for a pad it cannot reach any other way (the header of
//! `route.rs`). A board that refuses them (`.process.via_in_pad &= 0mm`) leaves the router one way
//! out of a pad: a track between the neighbouring pads, and a via beside the pad. A pad
//! walled in by its neighbours then stays open.
//!
//! [`closed_by_via_in_pad`] says how many of the connections the router left end at a pad where
//! a via could have stood, had the board allowed one: the pad is a surface-mount pad on one face,
//! a plain shape (not a custom pad), at least as wide as the router's via, and its net is one
//! via carries (a net that needs several vias for its current never gets a via in a pad). It is
//! the router's own test of a pad (`route::pads::in_pad`), with the board's refusal left out. It
//! counts and does not route again: a connection it names might still have been left with a via
//! in its pad, and one it does not name was left for another reason. [`cost_line`] is the line
//! `fab` writes with the connections it left.

use netsmith_pcb::{Board, PadRef, Pt, Ratline, Ratsnest};

/// The lines of `left` with an end at a pad where a via could stand, were vias in pads allowed;
/// empty when the board allows them (the router then tried them).
pub fn closed_by_via_in_pad(board: &Board, left: &Ratsnest) -> Vec<Ratline> {
    if board.rules.via_in_pad {
        return Vec::new();
    }
    left.lines
        .iter()
        .filter(|l| {
            let one_via = board
                .net_currents
                .iter()
                .find(|c| c.net == l.net)
                .is_none_or(|c| c.vias <= 1);
            one_via
                && (takes_via(board, &l.from, l.from_at)
                    || (!l.to_plane && takes_via(board, &l.to, l.to_at)))
        })
        .cloned()
        .collect()
}

/// Whether a via of the router's could stand in this pad, were vias in pads allowed.
fn takes_via(board: &Board, pad: &PadRef, at: Pt) -> bool {
    let Some(part) = board.part(&pad.refdes) else {
        return false;
    };
    let numbered: Vec<_> = part.footprint.pads_numbered(&pad.number).collect();
    let Some(found) = numbered
        .iter()
        .find(|p| part.pad_centre(p) == at)
        .or(numbered.first())
    else {
        return false;
    };
    let (front, back) = part.pad_layers(found);
    front != back
        && found.extent.is_none()
        && found.drill.is_none()
        && found.width.min(found.height) >= board.rules.via_diameter
}

/// The line `fab` writes for a board that refuses vias in pads and was left with connections:
/// how many of them end at a pad a via could stand in, and which. `None` when the board allows
/// vias in pads or nothing was left.
pub fn cost_line(board: &Board, left: &Ratsnest) -> Option<String> {
    if board.rules.via_in_pad || left.is_empty() {
        return None;
    }
    let closed = closed_by_via_in_pad(board, left);
    if closed.is_empty() {
        return Some(format!(
            "vias in pads forbidden: the router went out between the pads, and none of the {} connection(s) left open ends at a pad a via could stand in, so allowing vias in pads would not reach them",
            left.len()
        ));
    }
    let lines: Vec<String> = closed.iter().map(ToString::to_string).collect();
    Some(format!(
        "vias in pads forbidden: the router went out between the pads, and it cost {} open connection(s) of the {} left, each ending at a pad a via could stand in ({}); with vias in pads allowed (the process's `.via_in_pad`, not narrowed to `0mm`) the router may reach them through a filled and capped via, which costs extra",
        closed.len(),
        left.len(),
        lines.join("; ")
    ))
}
