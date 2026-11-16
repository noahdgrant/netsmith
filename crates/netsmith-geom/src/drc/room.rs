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

/// A pad's solder mask opening on a face: the pad's net, the opening's shapes and their box, and
/// the pad (an index into the artwork's pads).
type Opening = (Option<String>, Vec<Shape>, Bounds, usize);

/// The first thing [`Room::fits_with`] found new copper too near: an index into what it was
/// checked against, named only when someone asks ([`Room::in_the_way`]).
#[derive(Debug, Clone, Copy)]
enum Hit {
    Copper(usize),
    Kept(usize),
    Extra(usize),
    Hole(usize),
    Edge,
    Opening(usize, usize),
    Keepout(usize),
}

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
                .enumerate()
                .filter_map(|(i, p)| {
                    let s = m.mask_opening(p, front);
                    (!s.is_empty()).then(|| (p.net.clone(), s.clone(), Model::bounds_of(&s), i))
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
        self.fits_with(shape, net, layer, smd, &[])
    }

    /// The number of copper layers.
    pub fn layers(&self) -> usize {
        self.m.layers.len()
    }

    /// [`Room::fits`] with more copper counted besides what is kept: `extra` is the net, layer
    /// and shape of each piece, which is not remembered. A shove asks about a piece of copper
    /// in a place it may be moved to, with the other pieces where they are now.
    pub fn fits_with(
        &self,
        shape: &Shape,
        net: &str,
        layer: usize,
        smd: bool,
        extra: &[(&str, usize, &Shape)],
    ) -> bool {
        self.hit(shape, net, layer, smd, extra).is_none()
    }

    /// What `shape`, copper of `net` on `layer`, is too near, as a person would name it (`pad 2
    /// of R1 (net GND)`, `a track of net SDA`, `the board edge`, `keepout USB`), or `None` where
    /// it keeps every space ([`Room::fits_with`]). Where it is too near several things, the first
    /// found is named: copper, then holes, the edge, mask openings and keepouts.
    pub fn in_the_way(
        &self,
        shape: &Shape,
        net: &str,
        layer: usize,
        smd: bool,
        extra: &[(&str, usize, &Shape)],
    ) -> Option<String> {
        let hit = self.hit(shape, net, layer, smd, extra)?;
        let m = &self.m;
        let of_net = |n: &Option<String>| match n {
            Some(n) => format!(" of net {n}"),
            None => String::new(),
        };
        let pad = |i: usize| {
            let p = &m.art.pads[i];
            format!("pad {} of {}", p.number, p.part)
        };
        Some(match hit {
            Hit::Copper(i) => {
                let c = &m.items[i];
                match c.kind {
                    Kind::Pad(p) => match &c.net {
                        Some(n) => format!("{} (net {n})", pad(p)),
                        None => pad(p),
                    },
                    Kind::Via(_) => format!("a via{}", of_net(&c.net)),
                    Kind::Track(_) => format!("a track{}", of_net(&c.net)),
                    Kind::Teardrop(_) => format!("a teardrop{}", of_net(&c.net)),
                    Kind::Graphic(_) => "a drawing in copper".to_string(),
                }
            }
            Hit::Kept(i) => format!("copper of net {}", self.kept.borrow()[i].0),
            Hit::Extra(i) => format!("copper of net {}", extra[i].0),
            Hit::Hole(i) => {
                let h = &m.holes[i];
                match h.owner.map(|o| m.items[o].kind) {
                    Some(Kind::Pad(p)) => format!("the hole of {}", pad(p)),
                    Some(Kind::Via(_)) => format!("the hole of a via{}", of_net(&h.item.net)),
                    _ => "a hole".to_string(),
                }
            }
            Hit::Edge => "the board edge".to_string(),
            Hit::Opening(face, i) => {
                format!(
                    "the solder mask opening of {}",
                    pad(self.openings[face][i].3)
                )
            }
            Hit::Keepout(z) => match m.art.zones[z].name.as_str() {
                "" => "a keepout".to_string(),
                name => format!("keepout {name}"),
            },
        })
    }

    fn hit(
        &self,
        shape: &Shape,
        net: &str,
        layer: usize,
        smd: bool,
        extra: &[(&str, usize, &Shape)],
    ) -> Option<Hit> {
        let m = &self.m;
        let r = m.rules;
        let x = m.extras;
        let mine = Some(net.to_string());
        let near = |b: &Bounds, d: f64| shape.bounds.expanded(d + MARGIN).overlaps(b);
        let own_class = r.net_clearance(Some(net));
        let reach = m.max_gap() + x.pad_to_pad.unwrap_or(0.0) + x.mask_to_track.unwrap_or(0.0);
        for (i, c) in m.items.iter().enumerate() {
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
                return Some(Hit::Copper(i));
            }
        }
        for (i, (n, l, s)) in self.kept.borrow().iter().enumerate() {
            if n != net && *l == layer {
                let need = own_class.max(r.net_clearance(Some(n)));
                if near(&s.bounds, need) && gap(shape, s).d < need + MARGIN {
                    return Some(Hit::Kept(i));
                }
            }
        }
        for (i, (n, l, s)) in extra.iter().enumerate() {
            if *n != net && *l == layer {
                let need = own_class.max(r.net_clearance(Some(n)));
                if near(&s.bounds, need) && gap(shape, s).d < need + MARGIN {
                    return Some(Hit::Extra(i));
                }
            }
        }
        for (i, h) in m.holes.iter().enumerate() {
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
                return Some(Hit::Hole(i));
            }
        }
        for (s, _) in &m.edge {
            if near(&s.bounds, r.copper_edge) && gap(shape, s).d < r.copper_edge + MARGIN {
                return Some(Hit::Edge);
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
            for (i, (n, shapes, b, _)) in self.openings[face].iter().enumerate() {
                if n.as_deref() == Some(net) || !near(b, need) {
                    continue;
                }
                if shapes.iter().any(|s| gap(shape, s).d < need + MARGIN) {
                    return Some(Hit::Opening(face, i));
                }
            }
        }
        let layer_name = &m.layers[layer];
        for (zi, z) in m.art.zones.iter().enumerate().filter(|(_, z)| {
            z.owner.is_empty()
                && z.keepout.as_ref().is_some_and(|k| k.tracks)
                && z.layers.iter().any(|l| l == layer_name)
        }) {
            if z.outlines
                .iter()
                .any(|o| o.len() >= 3 && touches_area(shape, o))
            {
                return Some(Hit::Keepout(zi));
            }
        }
        None
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
