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
//! ring of other nets' tracks, on an island of the pour with no via, keeps its via. So does a pad
//! the pour joins with fewer than two thermal spokes, counted as KiCad's refill leaves them
//! ([`netsmith_pcb::starved_pour_pads`], issue #1115): KiCad's check calls it a starved thermal
//! relief, and its own track and via join it to the plane as a second spoke would.
//!
//! All the vias that may go are taken out at once and the ratsnest made again; a via whose pad
//! the ratsnest then finds cut off, or that the pour then starves, is put back with its track,
//! and so on until no more come back. Copper of the net put back only joins more, so this ends,
//! and the board is left with no more lines than before. The pass runs after the stitching and
//! before the teardrops, so a pad that gave up its track gets no teardrop.
//!
//! The stitching counts the grid's vias that a pad's own via stands in the way of (issue 1484,
//! `crate::stitch::OnSpare`): such a via is on the board while the pass looks, so a pad on an
//! island of the pour whose only stitching via that is gives its own via up to it. Each trial
//! board takes out the stitching vias over the pads' vias it keeps, so a pad put back takes its
//! stitching via out with it, and "no more lines than before" is measured against the board with
//! every pad's via kept and those stitching vias out.

use netsmith_pcb::{Board, PadRef, Pt, Ratsnest, Track, Via};

use crate::stitch::OnSpare;

/// A pad's own way to its plane: the pad, its centre, and its track and via.
pub(crate) struct PadVia {
    pub pad: PadRef,
    pub at: Pt,
    pub tracks: Vec<Track>,
    pub via: Via,
}

/// The board with the copper of `drops` taken out where `out` says, and the stitching vias of
/// `on_spare` that stand on a via of a drop it keeps.
fn without(board: &Board, drops: &[PadVia], out: &[bool], on_spare: &[OnSpare]) -> Board {
    let mut b = board.clone();
    for o in on_spare {
        let kept = drops
            .iter()
            .zip(out)
            .any(|(d, &gone)| !gone && o.under.contains(&d.via));
        if kept && let Some(i) = b.vias.iter().position(|v| *v == o.via) {
            b.vias.remove(i);
        }
    }
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
/// none of: see the module header. The stitching vias of `on_spare` stay only where the vias
/// they stand on go (issue 1484). How many were taken out.
pub(crate) fn drop_vias(board: &mut Board, drops: &[PadVia], on_spare: &[OnSpare]) -> usize {
    if drops.is_empty() {
        return 0;
    }
    // The board with every drop kept, and so none of the stitching vias over them.
    let kept_all = without(board, drops, &vec![false; drops.len()], on_spare);
    let before = Ratsnest::of(&kept_all).len();
    let mut out = vec![true; drops.len()];
    loop {
        let trial = without(board, drops, &out, on_spare);
        let left = Ratsnest::of(&trial);
        // A fill that cannot be made starves nothing: the ratsnest joins nothing through it.
        let starved = netsmith_pcb::starved_pour_pads(&trial).unwrap_or_default();
        let mut back = false;
        for (d, o) in drops.iter().zip(out.iter_mut()).filter(|(_, o)| **o) {
            let cut = left.len() > before
                && left.lines.iter().any(|l| {
                    (l.from == d.pad && l.from_at.distance(d.at) < 1e-6)
                        || (l.to == d.pad && l.to_at.distance(d.at) < 1e-6)
                });
            let thin = starved.iter().any(|(refdes, number, at)| {
                *refdes == d.pad.refdes && *number == d.pad.number && at.distance(d.at) < 0.01
            });
            if cut || thin {
                *o = false;
                back = true;
            }
        }
        if !back {
            if left.len() > before {
                *board = kept_all;
                return 0;
            }
            *board = trial;
            return out.iter().filter(|&&o| o).count();
        }
        if !out.contains(&true) {
            *board = kept_all;
            return 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_pcb::{BoardNet, BoardPour, CopperLayer, Rect, ZonePads};

    /// What a via of the stitching keeps from a pad, as `crate::stitch` keeps it.
    fn from_pads(board: &Board) -> f64 {
        let rules = &board.rules;
        board.widest_clearance().max(rules.plane_clearance) + rules.via_diameter / 2.0 + 0.05 + 0.25
    }

    /// The LED board on four layers with only D1 on it, its top poured on GND and stitched every
    /// 4 mm, and D1's GND pad on an island of the pour that a ring of another net's track cuts
    /// (issue 1484): a box round D1 and a neck down from it to a point of the stitching grid,
    /// so narrow that the only place for a stitching via there is the pad's own via, which
    /// stands on that point, joined to the pad by a track. The board and that via.
    fn island() -> (Board, PadVia) {
        let mut board = crate::board_from_design(
            &crate::testlib::four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &crate::testlib::fixtures(),
        )
        .unwrap();
        board.parts.retain(|p| p.refdes == "D1");
        board.tracks.clear();
        board.vias.clear();
        board.pours = vec![BoardPour {
            layer: CopperLayer::Front,
            net: "GND".into(),
            stitch_pitch: Some(4.0),
            thermal_gap: None,
            spoke_width: None,
            clearance: None,
            min_width: None,
            pads: None,
        }];
        board.outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(20.0, 12.0));
        board.parts[0].rotation = 0.0;
        let g = netsmith_pcb::stitch_grid(&board, 4.0)
            .into_iter()
            .min_by(|a, b| {
                let c = Pt::new(10.0, 6.0);
                a.distance(c).total_cmp(&b.distance(c))
            })
            .unwrap();
        // D1's GND pad straight above the grid point, its box as far from it as a stitching via
        // keeps from a pad, and a tenth of a millimetre more.
        let part = &board.parts[0];
        let pad = part
            .footprint
            .pads
            .iter()
            .find(|p| p.number == "1")
            .unwrap();
        let r = part.pad_bounds(pad);
        let half = (r.max.y - r.min.y) / 2.0;
        let want = Pt::new(g.x, g.y - 0.1 - from_pads(&board) - half);
        let centre = Pt::new((r.min.x + r.max.x) / 2.0, (r.min.y + r.max.y) / 2.0);
        board.parts[0].at = board.parts[0].at + (want - centre);
        let part = &board.parts[0];
        let pads = part
            .footprint
            .pads
            .iter()
            .map(|p| part.pad_bounds(p))
            .reduce(|a, b| {
                Rect::new(
                    Pt::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y)),
                    Pt::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y)),
                )
            })
            .unwrap()
            .expanded(1.5);
        // The ring: its neck's sides and bottom a tenth of a millimetre further from the grid
        // point than a via there keeps from them, and wide enough that no place outside it is
        // within a third of the pitch of the point.
        let width = 0.8;
        let s =
            width / 2.0 + board.widest_clearance() + board.rules.via_diameter / 2.0 + 0.05 + 0.1;
        let neck = want.y + half + 0.3;
        let ring = [
            Pt::new(g.x - s, g.y + s),
            Pt::new(g.x + s, g.y + s),
            Pt::new(g.x + s, neck),
            Pt::new(pads.max.x, neck),
            Pt::new(pads.max.x, pads.min.y),
            Pt::new(pads.min.x, pads.min.y),
            Pt::new(pads.min.x, neck),
            Pt::new(g.x - s, neck),
        ];
        board.nets.push(BoardNet {
            name: "RING".into(),
            pads: Vec::new(),
        });
        for k in 0..ring.len() {
            board.tracks.push(Track::straight(
                "RING",
                CopperLayer::Front,
                ring[k],
                ring[(k + 1) % ring.len()],
                width,
            ));
        }
        let via = Via {
            net: "GND".into(),
            at: g,
            diameter: board.rules.via_diameter,
            drill: board.rules.via_drill,
            span: None,
        };
        let track = Track::straight("GND", CopperLayer::Front, want, g, 0.2);
        board.tracks.push(track.clone());
        board.vias.push(via.clone());
        let pad = PadRef {
            refdes: "D1".into(),
            number: "1".into(),
        };
        (
            board,
            PadVia {
                pad,
                at: want,
                tracks: vec![track],
                via,
            },
        )
    }

    /// Whether `board` has a line from D1's GND pad.
    fn d1_cut(board: &Board) -> bool {
        Ratsnest::of(board)
            .lines
            .iter()
            .any(|l| l.net == "GND" && (l.from.refdes == "D1" || l.to.refdes == "D1"))
    }

    #[test]
    fn a_pad_on_an_island_gives_its_via_up_to_the_stitching_via_it_stood_in_the_way_of() {
        let (mut board, drop) = island();
        assert!(!d1_cut(&board), "the pad's own via joins it to the plane");
        let on_spare = crate::stitch::stitch(&mut board, std::slice::from_ref(&drop.via));
        // The grid point's only free place is the pad's via's: the stitching via goes there
        // all the same, over it.
        let near = |b: &Board| -> Vec<Via> {
            b.vias
                .iter()
                .filter(|v| v.at.distance(drop.via.at) < 4.0 / 3.0)
                .cloned()
                .collect()
        };
        assert_eq!(on_spare.len(), 1, "{on_spare:?}");
        assert_eq!(on_spare[0].under, std::slice::from_ref(&drop.via));
        assert_eq!(near(&board).len(), 2, "{:?}", board.vias);
        // The pour joins the pad to the stitching via: the pad's own via and track go.
        assert_eq!(
            drop_vias(&mut board, std::slice::from_ref(&drop), &on_spare),
            1
        );
        assert_eq!(near(&board), [on_spare[0].via.clone()]);
        assert!(!board.tracks.contains(&drop.tracks[0]));
        assert!(!d1_cut(&board), "{:?}", Ratsnest::of(&board).lines);
        assert_eq!(netsmith_pcb::starved_pour_pads(&board).unwrap(), Vec::new());
    }

    #[test]
    fn a_pad_that_keeps_its_via_takes_the_stitching_via_over_it_out() {
        // A pour that joins no pad leaves D1's pad its own via, and the stitching via over it
        // goes instead: no two vias stand on one place.
        let (mut board, drop) = island();
        board.pours[0].pads = Some(ZonePads::None);
        let on_spare = crate::stitch::stitch(&mut board, std::slice::from_ref(&drop.via));
        assert_eq!(on_spare.len(), 1, "{on_spare:?}");
        assert_eq!(
            drop_vias(&mut board, std::slice::from_ref(&drop), &on_spare),
            0
        );
        assert!(board.vias.contains(&drop.via));
        assert!(!board.vias.contains(&on_spare[0].via), "{:?}", board.vias);
        assert!(!d1_cut(&board));
    }
}
