//! A piece of a fill that copper of another net cut off from the rest of its fill, with pads of
//! its net on it and no other way to them (issues #1509 and #1524): the design rules check's
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
//! - something that keeps the fill away lies within its keep-away distance (the clearance the
//!   fill keeps from it, and the fill's least width) of it and of another piece of the same
//!   fill: what cut it. It is a track, a pad or a via of another net (or of no net), the hole of
//!   a pad with no copper on the layer, a rule area that keeps the pour out, or the fill of a
//!   pour of another net and a higher priority ([`By`]); the one nearest the two is named.
//!
//! A piece with pads and no way off it that nothing parts from another piece of its fill (the
//! zone's own outline narrows to nothing between them, say) is not this rule's: it names a thing
//! to move.
//!
//! **A neighbourhood.** The check of an edit (`routes add-track`, `routes add-via`) cuts the board to the boxes
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
    /// What lies between the piece and the rest of the fill, and its point nearest the piece:
    /// where the cut is.
    pub by: By,
    pub near: Pt,
}

/// What cut a piece of a fill off from the rest of it (issue #1524): copper of another net, a
/// hole, a rule area or a pour of another net, each by its index in the board's list of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum By {
    Track(usize),
    /// A pad of another net (or of no net) with copper on the layer.
    Pad(usize),
    /// The hole of a pad with no copper on the layer (a hole with no plating, say).
    Hole(usize),
    Via(usize),
    /// A rule area that keeps the pour out, by its index in the board's zones.
    RuleArea(usize),
    /// The fill of a pour of another net on the layer, by its index in the board's zones.
    Pour(usize),
}

/// The shape of one thing that keeps a fill away, and how far it keeps it.
enum Shape {
    /// Points (a track's, sampled; a via's centre) and the radius round each.
    Points(Vec<Pt>, f64),
    /// A closed outline.
    Ring(Vec<Pt>),
}

impl Shape {
    fn bounds(&self) -> Option<Bounds> {
        match self {
            Shape::Points(p, r) => Bounds::of(p.iter().copied()).map(|b| b.expanded(*r)),
            Shape::Ring(p) => Bounds::of(p.iter().copied()),
        }
    }

    /// How far the shape is from `piece`, and its point nearest the piece.
    fn gap(&self, piece: &Area) -> (f64, Pt) {
        let rings = || std::iter::once(&piece.outer).chain(&piece.holes);
        let to = |q: Pt| {
            if piece.contains(q) {
                0.0
            } else {
                rings()
                    .map(|r| ring_distance(r, q))
                    .fold(f64::INFINITY, f64::min)
            }
        };
        match self {
            Shape::Points(points, r) => points
                .iter()
                .map(|&q| ((to(q) - r).max(0.0), q))
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .unwrap_or((f64::INFINITY, Pt::new(0.0, 0.0))),
            Shape::Ring(ring) => {
                let (from_ring, at) = ring
                    .iter()
                    .map(|&q| (to(q), q))
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .unwrap_or((f64::INFINITY, Pt::new(0.0, 0.0)));
                // The piece's corners against the ring's sides: a ring wider than the gap.
                let from_piece = rings()
                    .flat_map(|r| r.iter())
                    .map(|&q| {
                        if crate::geom::contains(ring, q) {
                            0.0
                        } else {
                            ring_distance(ring, q)
                        }
                    })
                    .fold(f64::INFINITY, f64::min);
                (from_ring.min(from_piece), at)
            }
        }
    }
}

/// What cuts the piece `p` of the fill `j` (of the zone `z`, on `layer`, its box `b`) off from
/// another piece of the same fill, and where (issue #1524): of each track, pad, hole, via, rule
/// area and pour of another net that comes within its keep-away distance (the clearance the
/// fill keeps from it, and the strip narrower than the least width that the trimming took out)
/// of both pieces, the one nearest the two. Its order breaks a tie: tracks first, then pads,
/// holes, vias, rule areas and pours.
#[allow(clippy::too_many_arguments)]
fn cutter(
    art: &Artwork,
    layer: &str,
    z: &crate::artwork::Zone,
    judged: &[Judged],
    j: &Judged,
    p: usize,
    rules: &Rules,
    b: &Bounds,
) -> Option<(By, Pt)> {
    let clearance = zone_clearance(z, rules);
    let from = |net: &Option<String>| clearance.max(rules.class_clearance(net.as_deref()));
    let slack = z.min_thickness + 0.05;
    // Each candidate: what it is, its shape, and how far it keeps the fill.
    let mut shapes: Vec<(By, Shape, f64)> = Vec::new();
    for (t, track) in art.tracks.iter().enumerate() {
        if track.layer == layer && !own_net(z, &track.net) {
            let points = track_points(&track.shape);
            shapes.push((
                By::Track(t),
                Shape::Points(points, track.width / 2.0),
                from(&track.net),
            ));
        }
    }
    for (k, pad) in art.pads.iter().enumerate() {
        if own_net(z, &pad.net) {
            continue;
        }
        if art.pad_copper_on(pad, layer) {
            let by = pad
                .clearance
                .map_or(from(&pad.net), |c| c.max(from(&pad.net)));
            if let Some(poly) = pads::outline(pad).polygon() {
                shapes.push((By::Pad(k), Shape::Ring(poly), by));
            }
        } else if let Some(d) = &pad.drill
            && let Some(poly) = super::hole_outline(pad, d).polygon()
        {
            shapes.push((
                By::Hole(k),
                Shape::Ring(poly),
                rules.hole_clearance.unwrap_or(clearance),
            ));
        }
    }
    for (v, via) in art.vias.iter().enumerate() {
        if art.via_on(via, layer) && !own_net(z, &via.net) {
            shapes.push((
                By::Via(v),
                Shape::Points(vec![via.at], via.diameter / 2.0),
                from(&via.net),
            ));
        }
    }
    for (k, area) in art.zones.iter().enumerate() {
        if area.keepout.as_ref().is_some_and(|k| k.copperpour)
            && area.layers.iter().any(|l| l == layer)
        {
            for o in area.outlines.iter().filter(|o| o.len() >= 3) {
                shapes.push((By::RuleArea(k), Shape::Ring(o.clone()), 0.0));
            }
        }
    }
    // The fill of a pour of another net and a higher priority on the layer, which this one
    // keeps away from.
    for o in judged {
        let other = &art.zones[o.zone];
        if o.zone == j.zone || other.net == z.net || other.priority <= z.priority {
            continue;
        }
        let by = clearance.max(zone_clearance(other, rules));
        for a in &o.fill {
            shapes.push((By::Pour(o.zone), Shape::Ring(a.outer.clone()), by));
        }
    }
    let piece = &j.fill[p];
    let mut best: Option<(f64, By, Pt)> = None;
    for (by, shape, keep) in &shapes {
        let reach = keep + slack;
        let Some(sb) = shape.bounds() else {
            continue;
        };
        if !sb.overlaps(&b.expanded(reach + 1.0)) {
            continue;
        }
        let (to_piece, at) = shape.gap(piece);
        if to_piece > reach {
            continue;
        }
        let to_other = j
            .fill
            .iter()
            .enumerate()
            .filter(|&(o, _)| o != p)
            .filter(|(_, o)| {
                Bounds::of(o.outer.iter().copied()).is_some_and(|q| q.overlaps(&sb.expanded(reach)))
            })
            .map(|(_, o)| shape.gap(o).0)
            .fold(f64::INFINITY, f64::min);
        if to_other > reach {
            continue;
        }
        let score = to_piece + to_other;
        if best.is_none_or(|(s, _, _)| score < s) {
            best = Some((score, *by, at));
        }
    }
    best.map(|(_, by, at)| (by, at))
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
            // The copper of another net (or the hole, rule area or pour) between it and another
            // piece of the fill.
            let Some((by, near)) = cutter(art, layer, z, judged, j, p, rules, &b) else {
                continue;
            };
            out.push(Cut {
                zone: j.zone,
                pads,
                at: piece.outer[0],
                area: piece.size(),
                by,
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
