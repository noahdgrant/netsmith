//! A pad in a pour of its net gives up its via (issue #834).
//!
//! The router gives each pad of a plane net that is not through-hole a via of its own beside it,
//! joined by a short track, and the vias that stitch a pour to the other layers of its net
//! (`crate::stitch`) come after. Where the pad stands in an outer pour of the same net, the pour
//! joins the pad and the stitching vias join the pour to the plane, so the pad's own via and its
//! track may go. Whether the pour reaches the pad, and a via it covers, is the ratsnest's
//! ([`Ratsnest::of`]): the pour filled as the Gerbers fill it, less the copper narrower than its
//! minimum width that KiCad's fill trims, so that KiCad's own check finds the pad joined as well
//! (ADR 0020 made KiCad's unconnected check the gate; the router's tests run it). A pad inside a
//! ring of other nets' tracks, on an island of the pour with no via, keeps its via.
//!
//! All the vias that may go are taken out at once and the ratsnest made again; a via whose pad
//! the ratsnest then finds cut off is put back with its track, and so on until no more come
//! back. Copper of the net put back only joins more, so this ends, and the board is left with
//! no more lines than before. The pass runs after the stitching and before the teardrops, so a
//! pad that gave up its track gets no teardrop.

use netsmith_pcb::{Board, PadRef, Pt, Ratsnest, Track, Via};

/// A pad's own way to its plane: the pad, its centre, and its track and via.
pub(crate) struct PadVia {
    pub pad: PadRef,
    pub at: Pt,
    pub tracks: Vec<Track>,
    pub via: Via,
}

/// The board with the copper of `drops` taken out where `out` says.
fn without(board: &Board, drops: &[PadVia], out: &[bool]) -> Board {
    let mut b = board.clone();
    for (d, _) in drops.iter().zip(out).filter(|&(_, &o)| o) {
        if let Some(i) = b.vias.iter().position(|v| *v == d.via) {
            b.vias.remove(i);
        }
        for t in &d.tracks {
            if let Some(i) = b.tracks.iter().position(|x| x == t) {
                b.tracks.remove(i);
            }
        }
    }
    b
}

/// Take out the vias of `drops` (and their tracks) that the ratsnest finds the board needs
/// none of: see the module header. How many were taken out.
pub(crate) fn drop_vias(board: &mut Board, drops: &[PadVia]) -> usize {
    if drops.is_empty() {
        return 0;
    }
    let before = Ratsnest::of(board).len();
    let mut out = vec![true; drops.len()];
    loop {
        let trial = without(board, drops, &out);
        let left = Ratsnest::of(&trial);
        if left.len() <= before {
            *board = trial;
            return out.iter().filter(|&&o| o).count();
        }
        let mut back = false;
        for (d, o) in drops.iter().zip(out.iter_mut()).filter(|(_, o)| **o) {
            let cut = left.lines.iter().any(|l| {
                (l.from == d.pad && l.from_at.distance(d.at) < 1e-6)
                    || (l.to == d.pad && l.to_at.distance(d.at) < 1e-6)
            });
            if cut {
                *o = false;
                back = true;
            }
        }
        if !back || !out.contains(&true) {
            return 0;
        }
    }
}
