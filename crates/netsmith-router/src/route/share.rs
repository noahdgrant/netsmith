//! The stage that lets two pads of a plane net share one via to the plane (issue #834).

use netsmith_pcb::PadRef;

use super::*;

/// The faces whose copper a pad has, front first, found by its pad and its centre: `None` for
/// a pad the board does not have.
fn faces(board: &Board, pad: &PadRef, at: Pt) -> Option<(bool, bool)> {
    let part = board.part(&pad.refdes)?;
    part.footprint
        .pads_numbered(&pad.number)
        .find(|p| part.pad_centre(p).distance(at) < 1e-6)
        .map(|p| part.pad_layers(p))
}

/// The pairs of the router's lines to a plane, as indices into `lines`, whose pads may share a
/// via, with the line that joins the two pads: of one net, on the same one outer face, at most
/// [`SHARE_NEAR`] apart, neither a decoupling capacitor's pad (its own via beside it is the
/// textbook way, `crate::decouple`); the nearest pairs first, then by line. Only the first
/// `routed` lines (the ratsnest's) are taken.
pub(super) fn share_lines(
    board: &Board,
    lines: &[Ratline],
    routed: usize,
) -> Vec<((usize, usize), Ratline)> {
    let capacitors = crate::decouple::capacitor_pads(board);
    let face = |l: usize| {
        let line = &lines[l];
        let single = |f: (bool, bool)| f.0 != f.1;
        (line.to_plane && !capacitors.contains(&line.from))
            .then(|| faces(board, &line.from, line.from_at))
            .flatten()
            .filter(|&f| single(f))
    };
    let faces: Vec<Option<(bool, bool)>> = (0..routed).map(face).collect();
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for a in 0..routed {
        let Some(fa) = faces[a] else { continue };
        for b in a + 1..routed {
            let d = lines[a].from_at.distance(lines[b].from_at);
            if faces[b] == Some(fa) && lines[a].net == lines[b].net && d > 1e-6 && d <= SHARE_NEAR {
                pairs.push((d, a, b));
            }
        }
    }
    pairs.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
    pairs
        .into_iter()
        .map(|(_, a, b)| {
            let (la, lb) = (&lines[a], &lines[b]);
            let line = Ratline {
                net: la.net.clone(),
                from: la.from.clone(),
                to: lb.from.clone(),
                from_at: la.from_at,
                to_at: lb.from_at,
                to_plane: false,
            };
            ((a, b), line)
        })
        .collect()
}

impl<'a> Router<'a> {
    /// Let pads of a plane net share a via on a try that routed every line (issue #834): for
    /// each pair of [`share_lines`], the nearest first, whose two pads each still have the one
    /// via of their own line to the plane, the second pad's copper is taken up and the line
    /// between the two pads searched for against all the other copper; it is kept only if it
    /// has no via and at most [`SHARE_TRACK`] of track, and then the first pad's via is the way
    /// to the plane of both. A via is shared by two pads at most, and a net whose current needs
    /// vias side by side shares none. No line is left and nothing else moves.
    pub(super) fn share_vias(&self, state: &mut State) {
        if !self.share || self.shares.is_empty() {
            return;
        }
        let stage = self.stage.get();
        let failed_in = self.failed_in.borrow().clone();
        self.fine.set(true);
        let mut taken = vec![false; self.routed_lines];
        for (k, &(a, b)) in self.shares.iter().enumerate() {
            if self.out_of_time() {
                break;
            }
            if taken[a] || taken[b] || self.vias_needed(a) > 1 {
                continue;
            }
            let own_via = |l: usize| {
                state
                    .routed
                    .iter()
                    .position(|r| r.line == l)
                    .filter(|&i| state.routed[i].vias.len() == 1)
            };
            let (Some(_), Some(at)) = (own_via(a), own_via(b)) else {
                continue;
            };
            let old = state.routed.remove(at);
            let kept = self
                .try_line(self.shared_from + k, state, &[], ROOM_LOOSE, false)
                .map(|(new, _)| new)
                .filter(|new| {
                    let length: f64 = new.tracks.iter().map(|t| t.from.distance(t.to)).sum();
                    new.vias.is_empty() && !new.tracks.is_empty() && length <= SHARE_TRACK
                });
            match kept {
                Some(new) => {
                    state.routed.insert(at, new);
                    taken[a] = true;
                    taken[b] = true;
                }
                None => state.routed.insert(at, old),
            }
        }
        self.fine.set(false);
        *self.failed_in.borrow_mut() = failed_in;
        self.stage.set(stage);
    }
}

impl<'a> Router<'a> {
    /// The ways to the plane of the pads that may give theirs up to an outer pour of their net
    /// once the board is stitched (`crate::pour_pads`): the copper of each line to a plane that
    /// has the one via, of a net whose current needs no vias side by side and that is poured on
    /// the pad's face, from a pad that is not a decoupling capacitor's.
    pub(super) fn pour_pad_vias(&self, state: &State) -> Vec<crate::pour_pads::PadVia> {
        let mut routed: Vec<&Routed> = state
            .routed
            .iter()
            .filter(|r| {
                r.line < self.routed_lines
                    && self.lines[r.line].to_plane
                    && r.vias.len() == 1
                    && self.vias_needed(r.line) <= 1
            })
            .collect();
        routed.sort_by_key(|r| r.line);
        // A decoupling capacitor's via beside its pad is the textbook way, pour or not.
        let capacitors = crate::decouple::capacitor_pads(self.board);
        routed
            .into_iter()
            .filter(|r| !capacitors.contains(&self.lines[r.line].from))
            .filter_map(|r| {
                let line = &self.lines[r.line];
                let (front, back) = faces(self.board, &line.from, line.from_at)?;
                let poured = self.board.pours.iter().any(|p| {
                    p.net == line.net
                        && match p.layer {
                            CopperLayer::Front => front,
                            CopperLayer::Back => back,
                            CopperLayer::Inner(_) => false,
                        }
                });
                poured.then(|| crate::pour_pads::PadVia {
                    pad: line.from.clone(),
                    at: line.from_at,
                    tracks: r.tracks.clone(),
                    via: r.vias[0].clone(),
                })
            })
            .collect()
    }
}
