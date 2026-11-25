//! How wide a track may be drawn where it lies (issue #1691): for each thing near it that the
//! design rules check holds it apart from, the widest width that keeps the rule's space, measured
//! as the check measures it. A `track_current` finding names what stops the track widening, and
//! a widening pass (issue #1692) reads the same numbers ([`super::Room::widest`]).
//!
//! A track of width `w` is its centre line grown by `w / 2`, round ends and all, so it keeps a
//! space `s` from a neighbour exactly when the centre line is at least `s + w / 2` from the
//! neighbour's edge: the widest width a neighbour allows is `2 (d - s)`, `d` the distance from
//! the centre line to its edge. The spaces are the check's: the clearance between the two nets
//! (the larger of their classes', the pair's, a pad's own), the hole clearance and the process's
//! hole-to-track space to a hole not of the net, the copper-to-edge space, the space from a
//! track to the solder mask opening of a pad of another net, and no rule area that forbids
//! tracks (a space of 0). A rule the board ignores holds nothing.

use super::model::{Kind, Model};
use super::shape::{Shape, gap, within_hole};
use super::{Distance, Item, Rule};
use crate::artwork::TrackShape;
use crate::geom::{Bounds, Pt};

/// A pad's solder mask opening on a face: the pad's net, the opening's shapes and their box, and
/// the pad (an index into the artwork's pads).
pub(super) type Opening = (Option<String>, Vec<Shape>, Bounds, usize);

/// The solder mask openings of the pads of each face, front and back.
pub(super) fn openings(m: &Model<'_>) -> [Vec<Opening>; 2] {
    let of = |front: bool| {
        m.art
            .pads
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let s = m.mask_opening(p, front);
                (!s.is_empty()).then(|| (p.net.clone(), s.clone(), Model::bounds_of(&s), i))
            })
            .collect::<Vec<_>>()
    };
    [of(true), of(false)]
}

/// One thing that keeps a track from being drawn wider.
#[derive(Debug, Clone, PartialEq)]
pub struct Limit {
    /// What it is, as the check names it (`Track [GND] on F.Cu, length 4.0000 mm`, `PTH pad 1
    /// [VIN] of J1`, `Segment on Edge.Cuts`, `rule area 'ANT'`), with its net and its part.
    pub item: Item,
    /// What kind of thing it is: `track`, `via`, `pad`, `teardrop`, `drawing` (a drawing in
    /// copper), `copper` (copper kept so far), `hole`, `edge`, `mask_opening` or `rule_area`.
    pub kind: &'static str,
    /// The rule that keeps the space: `clearance`, `hole_clearance`, `hole_to_track`,
    /// `copper_edge_clearance`, `mask_to_track` or `items_not_allowed`.
    pub rule: Rule,
    /// The point of it nearest the track's centre line, in the board's frame.
    pub at: Pt,
    /// The space the rule asks and the space there is now, edge to edge from the track as it is
    /// drawn (below 0 where the track already runs into it), mm.
    pub distance: Distance,
    /// The widest the track may be drawn and keep that space, mm (0 where no width does).
    pub width: f64,
}

/// How wide a track may be drawn where it lies, up to a width asked about.
#[derive(Debug, Clone, PartialEq)]
pub struct Widest {
    /// The widest width every neighbour allows, no more than the width asked about, mm.
    pub width: f64,
    /// Each neighbour that allows less than the width asked about, the one that allows least
    /// first: empty when that width fits.
    pub limits: Vec<Limit>,
}

impl Widest {
    /// Whether the width asked about fits.
    pub fn fits(&self) -> bool {
        self.limits.is_empty()
    }
}

/// The centre line of track `t` (an index into the artwork's tracks) as a shape of no width.
fn centre_line(m: &Model<'_>, t: usize) -> Shape {
    match &m.art.tracks[t].shape {
        TrackShape::Line(a, b) => Shape::capsule(*a, *b, 0.0),
        TrackShape::Arc(arc) => Shape::stroke(arc.points(), 0.0),
    }
}

/// How wide track `t` (an index into the artwork's tracks) may be drawn where it lies, up to
/// `upto` mm, against the board in `m` and the copper in `kept` (net, layer and shape of each
/// piece, held apart by the clearance between the nets): `None` for a track on no copper layer
/// or of no net.
pub(super) fn widest(
    m: &Model<'_>,
    openings: &[Vec<Opening>; 2],
    kept: &[(String, usize, Shape)],
    t: usize,
    upto: f64,
) -> Option<Widest> {
    let track = &m.art.tracks[t];
    let net = track.net.as_deref()?;
    let layer = m.layers.iter().position(|l| *l == track.layer)?;
    let r = m.rules;
    let x = m.extras;
    let mine = Some(net.to_string());
    let centre = centre_line(m, t);
    let half = track.width / 2.0;
    let on = |rule: Rule| r.severity(rule).is_some();
    let mut limits = Vec::new();
    // What a neighbour at `space` from its edge allows, kept when it is less than `upto`.
    let mut consider = |item: &Item, kind: &'static str, rule: Rule, need: f64, s: &Shape| {
        if !centre
            .bounds
            .expanded(upto / 2.0 + need)
            .overlaps(&s.bounds)
        {
            return;
        }
        let g = gap(&centre, s);
        let width = (2.0 * (g.d - need)).max(0.0);
        if width < upto {
            limits.push(Limit {
                item: item.clone(),
                kind,
                rule,
                at: g.b,
                distance: Distance {
                    required: need,
                    measured: g.d - half,
                },
                width,
            });
        }
    };
    // Copper of other nets on the layer.
    if on(Rule::Clearance) {
        for c in &m.items {
            if c.net == mine || !c.layers.contains(&layer) {
                continue;
            }
            let need = r
                .pair_clearance(Some(net), c.net.as_deref())
                .max(c.own_clearance);
            let kind = match c.kind {
                Kind::Pad(_) => "pad",
                Kind::Via(_) => "via",
                Kind::Track(_) => "track",
                Kind::Teardrop(_) => "teardrop",
                Kind::Graphic(_) => "drawing",
            };
            if let Some(s) = nearest(&centre, &c.shapes) {
                consider(&c.item, kind, Rule::Clearance, need, s);
            }
        }
        for (n, l, s) in kept {
            if n != net && *l == layer {
                let item = Item {
                    what: format!("copper of net {n}"),
                    at: s.pts[0],
                    part: None,
                    pad: None,
                    net: Some(n.clone()),
                };
                consider(
                    &item,
                    "copper",
                    Rule::Clearance,
                    r.pair_clearance(Some(net), Some(n)),
                    s,
                );
            }
        }
    }
    // Holes not of the net that reach the layer: the hole clearance, or the process's space
    // from such a hole to a track where it is more.
    for h in &m.holes {
        if h.item.net.is_some() && h.item.net == mine {
            continue;
        }
        if let Some((a, b)) = h.span
            && !(a <= layer && layer <= b)
        {
            continue;
        }
        let plated_pad = h.plated
            && h.owner
                .is_some_and(|o| matches!(m.items[o].kind, Kind::Pad(_)));
        let clearance = on(Rule::HoleClearance).then_some(r.hole_clearance);
        let to_track = x
            .hole_to_track_of(plated_pad)
            .filter(|_| on(Rule::HoleToTrack));
        let (rule, need) = match (clearance, to_track) {
            (Some(c), Some(t)) if t > c => (Rule::HoleToTrack, t),
            (Some(c), _) => (Rule::HoleClearance, c),
            (None, Some(t)) => (Rule::HoleToTrack, t),
            (None, None) => continue,
        };
        consider(&h.item, "hole", rule, need, &h.shape);
    }
    // The board edge.
    if on(Rule::CopperEdgeClearance) {
        for (s, item) in &m.edge {
            consider(item, "edge", Rule::CopperEdgeClearance, r.copper_edge, s);
        }
    }
    // The solder mask openings of pads of other nets, on an outer layer.
    let last = m.layers.len().saturating_sub(1);
    let face = match layer {
        0 => Some(0),
        l if l == last => Some(1),
        _ => None,
    };
    if let (Some(face), Some(need)) = (face, x.mask_to_track.filter(|_| on(Rule::MaskToTrack))) {
        for (n, shapes, _, p) in &openings[face] {
            if n.as_deref() == Some(net) {
                continue;
            }
            let pad = &m.art.pads[*p];
            let item = Item {
                what: format!(
                    "the solder mask opening of pad {} of {}",
                    pad.number, pad.part
                ),
                at: pad.at,
                part: Some(pad.part.clone()),
                pad: Some(pad.number.clone()),
                net: pad.net.clone(),
            };
            if let Some(s) = nearest(&centre, shapes) {
                consider(&item, "mask_opening", Rule::MaskToTrack, need, s);
            }
        }
    }
    // Rule areas that forbid tracks on the layer: the track may come up to their edge.
    if on(Rule::ItemsNotAllowed) {
        for z in m.art.zones.iter().filter(|z| {
            z.keepout.as_ref().is_some_and(|k| k.tracks) && z.layers.contains(&track.layer)
        }) {
            let name = super::checks::area_name(z);
            for area in crate::fill::outline_area(z) {
                if area.outer.len() < 3 {
                    continue;
                }
                // In a hole of the area (a cutout), the hole's edge holds it; else the area's.
                let s = match area.holes.iter().find(|h| within_hole(&centre, h)) {
                    Some(h) => Shape::ring(h.clone(), 0.0),
                    None => Shape::polygon(area.outer.clone(), 0.0),
                };
                let item = Item {
                    what: format!("rule area {name}"),
                    at: area.outer[0],
                    part: (!z.owner.is_empty()).then(|| z.owner.clone()),
                    pad: None,
                    net: None,
                };
                consider(&item, "rule_area", Rule::ItemsNotAllowed, 0.0, &s);
            }
        }
    }
    limits.sort_by(|a, b| a.width.total_cmp(&b.width));
    let width = limits.first().map_or(upto, |l| l.width.min(upto));
    Some(Widest { width, limits })
}

/// Of `shapes`, the one nearest `to`.
fn nearest<'s>(to: &Shape, shapes: &'s [Shape]) -> Option<&'s Shape> {
    shapes
        .iter()
        .map(|s| (gap(to, s).d, s))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, s)| s)
}
