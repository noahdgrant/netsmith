//! Whether new copper of a net keeps every space the check ([`super::check_artwork`]) asks of a
//! track, against a board as the check reads it: the room the teardrops (`teardrop.rs`) and the
//! meanders of diff pairs (`netsmith_pcb::meander`) are drawn in.
//!
//! The spaces are the clearance to copper of other nets (the larger of the two nets' classes, a
//! pad's own), the hole clearance and the process's hole-to-track spaces to holes not of the
//! net, the copper-to-edge space, the space from a track to a solder mask opening of another
//! net, and no rule area that forbids tracks. Copper on a surface-mount pad also keeps the
//! process's space between surface-mount pads from the pads of other nets. Each rule is met with
//! [`MARGIN`] to spare, and the copper kept so far ([`Room::keep`]) counts too.

use super::model::{Kind, Model};
use super::shape::{Shape, gap, touches_area};
use super::{Extras, Rules};
use crate::artwork::{Artwork, PadKind, TrackShape};
use crate::geom::{Bounds, Pt};

/// How much more than each rule's space new copper keeps, in mm.
pub(super) const MARGIN: f64 = 0.001;

/// A pad's solder mask opening on a face: the pad's net, the opening's shapes and their box.
type Opening = (Option<String>, Vec<Shape>, Bounds);

/// The board's copper, holes, edge and mask openings, and the copper kept so far, that new
/// copper is checked against.
pub struct Room<'a> {
    pub(crate) m: Model<'a>,
    /// The solder mask openings of the pads of each face, with the pad's net.
    openings: [Vec<Opening>; 2],
    /// The copper kept so far, with its net, layer and shape.
    kept: std::cell::RefCell<Vec<(String, usize, Shape)>>,
}

impl<'a> Room<'a> {
    pub fn new(art: &'a Artwork, rules: &'a Rules, extras: &'a Extras) -> Room<'a> {
        let m = Model::new(art, rules, extras, false);
        let opening = |front: bool| {
            art.pads
                .iter()
                .filter_map(|p| {
                    let s = m.mask_opening(p, front);
                    (!s.is_empty()).then(|| (p.net.clone(), s.clone(), Model::bounds_of(&s)))
                })
                .collect::<Vec<_>>()
        };
        let openings = [opening(true), opening(false)];
        Room {
            m,
            openings,
            kept: Default::default(),
        }
    }

    /// The index of a copper layer by KiCad's name (`F.Cu`).
    pub fn layer(&self, name: &str) -> Option<usize> {
        self.m.layers.iter().position(|l| l == name)
    }

    /// The clearance the rules ask round copper of `net` (its class's).
    pub fn clearance(&self, net: &str) -> f64 {
        self.m.rules.net_clearance(Some(net))
    }

    /// Count `shape` of `net` on `layer` as copper from now on.
    pub fn keep(&self, net: &str, layer: usize, shape: Shape) {
        self.kept.borrow_mut().push((net.to_string(), layer, shape));
    }

    /// Whether `shape`, copper of `net` on `layer`, keeps every space; `smd` when it lies on a
    /// surface-mount pad, so that it keeps the space between such pads too.
    pub fn fits(&self, shape: &Shape, net: &str, layer: usize, smd: bool) -> bool {
        let m = &self.m;
        let r = m.rules;
        let x = m.extras;
        let mine = Some(net.to_string());
        let near = |b: &Bounds, d: f64| shape.bounds.expanded(d + MARGIN).overlaps(b);
        let own_class = r.net_clearance(Some(net));
        let reach = m.max_gap() + x.pad_to_pad.unwrap_or(0.0) + x.mask_to_track.unwrap_or(0.0);
        for c in &m.items {
            if c.net == mine || !c.layers.contains(&layer) || !near(&c.bounds, reach) {
                continue;
            }
            let mut need = own_class
                .max(r.net_clearance(c.net.as_deref()))
                .max(c.own_clearance);
            if smd
                && let Kind::Pad(p) = c.kind
                && matches!(m.art.pads[p].kind, PadKind::Smd | PadKind::Connect)
            {
                need = need.max(x.pad_to_pad.unwrap_or(0.0));
            }
            if c.shapes.iter().any(|s| gap(shape, s).d < need + MARGIN) {
                return false;
            }
        }
        for (n, l, s) in self.kept.borrow().iter() {
            if n != net && *l == layer {
                let need = own_class.max(r.net_clearance(Some(n)));
                if near(&s.bounds, need) && gap(shape, s).d < need + MARGIN {
                    return false;
                }
            }
        }
        for h in &m.holes {
            if h.item.net == mine || !near(&h.shape.bounds, reach) {
                continue;
            }
            let plated_pad = h.plated
                && h.owner
                    .is_some_and(|o| matches!(m.items[o].kind, Kind::Pad(_)));
            let to_track = if plated_pad {
                x.plated_hole_clearance
            } else {
                x.hole_to_track
            };
            let need = r.hole_clearance.max(to_track.unwrap_or(0.0));
            if gap(shape, &h.shape).d < need + MARGIN {
                return false;
            }
        }
        for (s, _) in &m.edge {
            if near(&s.bounds, r.copper_edge) && gap(shape, s).d < r.copper_edge + MARGIN {
                return false;
            }
        }
        let last = m.layers.len().saturating_sub(1);
        let face = if layer == 0 {
            Some(0)
        } else if layer == last {
            Some(1)
        } else {
            None
        };
        if let Some(face) = face {
            let need = x.mask_to_track.unwrap_or(0.0);
            for (n, shapes, b) in &self.openings[face] {
                if n.as_deref() == Some(net) || !near(b, need) {
                    continue;
                }
                if shapes.iter().any(|s| gap(shape, s).d < need + MARGIN) {
                    return false;
                }
            }
        }
        let layer_name = &m.layers[layer];
        for z in m.art.zones.iter().filter(|z| {
            z.owner.is_empty()
                && z.keepout.as_ref().is_some_and(|k| k.tracks)
                && z.layers.iter().any(|l| l == layer_name)
        }) {
            if z.outlines
                .iter()
                .any(|o| o.len() >= 3 && touches_area(shape, o))
            {
                return false;
            }
        }
        true
    }

    /// The least distance from `shape` on `layer` to the copper of its own `net` (its pads,
    /// vias and tracks), leaving out the straight tracks `skip` says are the copper `shape`
    /// replaces; `f64::INFINITY` where the net has none on the layer.
    pub fn own_gap(
        &self,
        shape: &Shape,
        net: &str,
        layer: usize,
        skip: impl Fn(Pt, Pt) -> bool,
    ) -> f64 {
        let mine = Some(net.to_string());
        let mut least = f64::INFINITY;
        for c in &self.m.items {
            if c.net != mine || !c.layers.contains(&layer) {
                continue;
            }
            if let Kind::Track(i) = c.kind
                && let TrackShape::Line(a, b) = self.m.art.tracks[i].shape
                && skip(a, b)
            {
                continue;
            }
            for s in &c.shapes {
                least = least.min(gap(shape, s).d);
            }
        }
        least
    }
}
