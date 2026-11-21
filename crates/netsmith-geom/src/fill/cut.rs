//! A piece of a fill that a track of another net cut off from the rest of its fill, with pads
//! of its net on it and no other way to them (issue #1509): the design rules check's
//! `pad_island_cut`.
//!
//! The check's connectivity joins every pad in a pour's outline to the pour (`drc/connect.rs`),
//! so a pad whose piece of the fill lost its via, when a track laid across the pour cut the
//! piece in two, still counts as joined; only the router of `fab`, which reads the fill, sees it
//! and draws a join. This reads the fill itself, the one `board fill` lists and the Gerbers
//! hold. A piece is **cut** when:
//!
//! - it joins one or more pads of its net on the layer (solid copper, or the spokes of a
//!   relief, touch the pad), and its net has a pad that it does not join;
//! - nothing else of its net leads off it: no via of its net, no plated hole among its pads, no
//!   track of its net with an end off the piece and off its pads, and no piece of another fill
//!   of its net touches it;
//! - a track of another net runs within its zone's clearance (and the fill's least width) of it
//!   and of another piece of the same fill: the track that cut it. The one nearest the two is
//!   named.
//!
//! A piece with pads and no way off it that no track parts from the rest of its fill (the pads
//! of other nets round it cut it off, say) is not this rule's: it names a track to move.
//!
//! **A neighbourhood.** The check of an edit (`routes add-track`) cuts the board to the boxes
//! round its change and the zones to them ([`crate::DrcExtras::open`]): a piece that reaches the
//! edge of a box may go on outside it, so it is not judged, and the pieces wholly inside are
//! judged as on the whole board.

use super::{Judged, TOUCH, connection, own_net, ring_distance, track_points, zone_clearance};
use crate::artwork::{Artwork, Connect, PadKind, TrackShape};
use crate::geom::{Bounds, Pt};
use crate::pads;
use crate::plot::Rules;
use crate::polygons::{Area, grown, overlap};

/// A piece of a fill cut off from the rest of it by a track of another net ([module](self)).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cut {
    /// The zone, by its index in the board's zones.
    pub zone: usize,
    /// The pads of the zone's net the piece joins, by their index in the board's pads.
    pub pads: Vec<usize>,
    /// A corner of the piece, and its area in mm².
    pub at: Pt,
    pub area: f64,
    /// The track of another net between the piece and the rest of the fill, by its index in
    /// the board's tracks, and its point nearest the piece: where the cut is.
    pub track: usize,
    pub near: Pt,
}

/// The cut pieces of the fills `judged` of `layer` ([module](self)). A piece whose box is not
/// inside one of `open` is not judged, when `open` is not empty.
pub(crate) fn cuts(
    art: &Artwork,
    layer: &str,
    judged: &[Judged],
    rules: &Rules,
    open: &[Bounds],
) -> Vec<Cut> {
    let mut out = Vec::new();
    for j in judged {
        if j.fill.len() < 2 {
            continue;
        }
        let z = &art.zones[j.zone];
        if z.net.is_none() {
            continue;
        }
        let others: Vec<&Area> = judged
            .iter()
            .filter(|o| o.zone != j.zone && art.zones[o.zone].net == z.net)
            .flat_map(|o| o.fill.iter())
            .collect();
        // How near a track of another net comes to the fill it cuts: its clearance, and the
        // strip narrower than the least width that the trimming took out.
        let reach = zone_clearance(z, rules) + z.min_thickness + 0.05;
        for (p, piece) in j.fill.iter().enumerate() {
            let Some(b) = Bounds::of(piece.outer.iter().copied()) else {
                continue;
            };
            if !open.is_empty() && !open.iter().any(|o| inside(&b, o)) {
                continue;
            }
            let near = |p: Pt, by: f64| {
                piece.contains(p)
                    || std::iter::once(&piece.outer)
                        .chain(&piece.holes)
                        .any(|r| ring_distance(r, p) <= by)
            };
            // A way off the piece to other layers: a via of its net on it.
            if art
                .vias
                .iter()
                .filter(|v| own_net(z, &v.net) && art.via_on(v, layer))
                .any(|v| near(v.at, v.diameter / 2.0))
            {
                continue;
            }
            let one = std::slice::from_ref(piece);
            let pads: Vec<usize> = art
                .pads
                .iter()
                .enumerate()
                .filter(|(_, pad)| {
                    own_net(z, &pad.net)
                        && art.pad_copper_on(pad, layer)
                        && connection(pad, z) != Connect::None
                })
                .filter_map(|(k, pad)| {
                    let poly = pads::outline(pad).polygon()?;
                    let q = Bounds::of(poly.iter().copied())?;
                    (q.expanded(TOUCH).overlaps(&b)
                        && overlap(one, &grown(&[Area::of(&poly)], TOUCH)))
                    .then_some(k)
                })
                .collect();
            if pads.is_empty() {
                continue;
            }
            // A plated hole among its pads joins the other layers.
            if pads.iter().any(|&k| art.pads[k].kind == PadKind::Thru) {
                continue;
            }
            // Its net has more than these pads, or there is nothing to lose.
            if !art
                .pads
                .iter()
                .enumerate()
                .any(|(k, pad)| own_net(z, &pad.net) && !pads.contains(&k))
            {
                continue;
            }
            let in_pad = |e: Pt| {
                pads.iter().any(|&k| {
                    pads::outline(&art.pads[k])
                        .polygon()
                        .is_some_and(|poly| crate::geom::contains(&poly, e))
                })
            };
            // A track of its net that leads off the piece: an end off it and off its pads.
            let leads_off = art
                .tracks
                .iter()
                .filter(|t| t.layer == layer && own_net(z, &t.net))
                .any(|t| {
                    let (start, end) = match &t.shape {
                        TrackShape::Line(a, b) => (*a, *b),
                        TrackShape::Arc(arc) => (arc.start, arc.end),
                    };
                    let on = |e: Pt| piece.contains(e) || in_pad(e);
                    let touches = track_points(&t.shape)
                        .into_iter()
                        .any(|q| near(q, t.width / 2.0));
                    touches && !(on(start) && on(end))
                });
            if leads_off {
                continue;
            }
            // Another fill of its net that touches it.
            if others.iter().any(|o| {
                Bounds::of(o.outer.iter().copied()).is_some_and(|q| q.expanded(TOUCH).overlaps(&b))
                    && overlap(one, &grown(std::slice::from_ref(*o), TOUCH))
            }) {
                continue;
            }
            // The track of another net between it and another piece of the fill.
            let gap = |piece: &Area, q: Pt| -> f64 {
                std::iter::once(&piece.outer)
                    .chain(&piece.holes)
                    .map(|r| ring_distance(r, q))
                    .fold(f64::INFINITY, f64::min)
            };
            let grown_b = b.expanded(reach + 1.0);
            let mut best: Option<(f64, usize, Pt)> = None;
            for (t, track) in art.tracks.iter().enumerate() {
                if track.layer != layer || own_net(z, &track.net) {
                    continue;
                }
                let points = track_points(&track.shape);
                let Some(tb) = Bounds::of(points.iter().copied()) else {
                    continue;
                };
                if !tb.overlaps(&grown_b) {
                    continue;
                }
                let by = reach + track.width / 2.0;
                let Some((to_piece, at)) = points
                    .iter()
                    .map(|&q| (gap(piece, q), q))
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                else {
                    continue;
                };
                if to_piece > by {
                    continue;
                }
                let to_other = j
                    .fill
                    .iter()
                    .enumerate()
                    .filter(|&(o, _)| o != p)
                    .filter(|(_, o)| {
                        Bounds::of(o.outer.iter().copied())
                            .is_some_and(|q| q.overlaps(&tb.expanded(by)))
                    })
                    .flat_map(|(_, o)| points.iter().map(move |&q| gap(o, q)))
                    .fold(f64::INFINITY, f64::min);
                if to_other > by {
                    continue;
                }
                let score = to_piece + to_other;
                if best.is_none_or(|(s, _, _)| score < s) {
                    best = Some((score, t, at));
                }
            }
            let Some((_, track, near)) = best else {
                continue;
            };
            out.push(Cut {
                zone: j.zone,
                pads,
                at: piece.outer[0],
                area: piece.size(),
                track,
                near,
            });
        }
    }
    out
}

/// Whether the box `b` is inside `o`, off its edges.
fn inside(b: &Bounds, o: &Bounds) -> bool {
    b.min.x > o.min.x && b.min.y > o.min.y && b.max.x < o.max.x && b.max.y < o.max.y
}
