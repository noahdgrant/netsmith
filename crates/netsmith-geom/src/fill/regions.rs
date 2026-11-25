//! The regions of a fill (issue #1655): each separate piece of the fill of each zone on each
//! layer, the pads and vias of its net it joins, whether it reaches the net's main region, and,
//! of a piece that does not, the copper of other nets and the rule areas that bound it, so that
//! the track that closed a pocket can be found.
//!
//! **Joined.** The copper of a net is followed across every layer: two pads, vias or tracks of
//! the net are joined where they share a layer and their copper runs into each other
//! ([`crate::copper_joins`], the design rules check's rule), a pad with a hole and a via are one
//! piece on every layer they have copper on, and a piece of a fill is joined to the copper of its
//! net it touches on its layer and to the pieces of other fills of the net it touches, as
//! [`isolated`](super) judges a copper island. A piece joins a pad or a via when it touches its
//! copper: a thermal relief's spoke is copper of the fill, so a pad one spoke joins is joined.
//!
//! **The main region.** Of the separate groups of a net's copper so joined, the main one is the
//! one with the most pads, then the most fill: a piece in it reaches the main region; any other
//! piece is cut off, with the pads of the net it holds.
//!
//! **What bounds a cut-off piece.** Each track, via and pad of another net on the layer, and each
//! rule area that forbids pours there, whose copper (or outline) comes within the fill's
//! clearance and its minimum width (with a hundredth of a millimetre to spare) of the piece's
//! edge: the fill stops there because of it. Each says how far it is from the piece and whether
//! it also comes so near a piece of the net's main region on the layer: copper between the two is
//! what parts them, the track that closed the pocket among it.

use super::{Area, Bounds, Pt, TOUCH, own_net, zone_clearance};
use crate::artwork::{Artwork, TrackShape, Zone};
use crate::drc::shape::Shape;
use crate::gbr::Pen;
use crate::plot::Rules;
use crate::polygons::{Polygons, grown, overlap};

use super::ZoneJoins;

/// How much more than the clearance and the minimum width copper may be from a piece and still
/// bound it, in mm.
const SPARE: f64 = 0.01;

/// What bounds a cut-off piece of a fill ([module](self)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bounding {
    /// A track, by its index in the board's tracks.
    Track(usize),
    /// A via, by its index in the board's vias.
    Via(usize),
    /// A pad, by its index in the board's pads.
    Pad(usize),
    /// A rule area that forbids pours, by its index in the board's zones.
    RuleArea(usize),
}

/// One thing that bounds a cut-off piece, how far its copper is from the piece's edge, and
/// whether it comes as near a piece of the net's main region on the layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bound {
    pub what: Bounding,
    /// The gap between its copper (a rule area's outline) and the piece, in mm.
    pub gap: f64,
    /// Whether it also bounds a piece of the main region of the piece's net on its layer: it is
    /// between the two, and so among what parts them.
    pub parts_from_main: bool,
}

/// One separate piece of the fill of a zone on a layer ([module](self)).
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    /// The piece, as the Gerbers hold it.
    pub piece: Area,
    /// The pads of the zone's net it touches, by their index in the board's pads.
    pub pads: Vec<usize>,
    /// The vias of the zone's net it touches, by their index in the board's vias.
    pub vias: Vec<usize>,
    /// Whether it is in the main region of its net.
    pub main: bool,
    /// Of a piece cut off from the main region: what bounds it, the nearest first.
    pub bounds: Vec<Bound>,
}

/// The regions of the fill of one zone on one layer.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoneRegions {
    /// The zone, by its index in the board's zones.
    pub zone: usize,
    pub regions: Vec<Region>,
}

/// A pad, via or track of a net: copper on some layers.
struct Item {
    /// What it is: [`Bounding::Track`], [`Bounding::Via`] or [`Bounding::Pad`].
    what: Bounding,
    net: Option<String>,
    layers: Vec<String>,
    shapes: Vec<Shape>,
    bounds: Bounds,
}

fn shapes_bounds(shapes: &[Shape]) -> Bounds {
    let mut b = shapes[0].bounds;
    for s in &shapes[1..] {
        b.min = Pt::new(b.min.x.min(s.bounds.min.x), b.min.y.min(s.bounds.min.y));
        b.max = Pt::new(b.max.x.max(s.bounds.max.x), b.max.y.max(s.bounds.max.y));
    }
    b
}

/// Every pad, via and track of the board with copper, with its shapes.
fn items(art: &Artwork) -> Vec<Item> {
    let mut out = Vec::new();
    for (i, pad) in art.pads.iter().enumerate() {
        let layers: Vec<String> = art
            .copper
            .iter()
            .filter(|l| art.pad_copper_on(pad, l))
            .cloned()
            .collect();
        let shapes = crate::drc::pad_copper(pad);
        if layers.is_empty() || shapes.is_empty() {
            continue;
        }
        out.push(Item {
            what: Bounding::Pad(i),
            net: pad.net.clone(),
            layers,
            bounds: shapes_bounds(&shapes),
            shapes,
        });
    }
    for (i, via) in art.vias.iter().enumerate() {
        let shapes = vec![Shape::disc(via.at, via.diameter / 2.0)];
        out.push(Item {
            what: Bounding::Via(i),
            net: via.net.clone(),
            layers: art
                .copper
                .iter()
                .filter(|l| art.via_on(via, l))
                .cloned()
                .collect(),
            bounds: shapes_bounds(&shapes),
            shapes,
        });
    }
    for (i, t) in art.tracks.iter().enumerate() {
        let shapes = vec![match &t.shape {
            TrackShape::Line(a, b) => Shape::capsule(*a, *b, t.width / 2.0),
            TrackShape::Arc(arc) => Shape::stroke(arc.points(), t.width / 2.0),
        }];
        out.push(Item {
            what: Bounding::Track(i),
            net: t.net.clone(),
            layers: vec![t.layer.clone()],
            bounds: shapes_bounds(&shapes),
            shapes,
        });
    }
    out
}

/// The copper of an item on a layer, as areas: what a piece of a fill touches.
fn item_area(art: &Artwork, what: Bounding) -> Vec<Area> {
    let mut p = Polygons::new();
    match what {
        Bounding::Pad(i) => crate::pads::outline(&art.pads[i]).draw(&mut p, None, &[]),
        Bounding::Via(i) => {
            let via = &art.vias[i];
            crate::pads::Outline::Circle {
                c: via.at,
                r: via.diameter / 2.0,
            }
            .draw(&mut p, None, &[]);
        }
        Bounding::Track(i) => {
            let t = &art.tracks[i];
            match &t.shape {
                TrackShape::Line(a, b) => super::stroke(&mut p, *a, *b, t.width),
                TrackShape::Arc(arc) => p.arc(t.width, None, arc, &[]),
            }
        }
        Bounding::RuleArea(_) => {}
    }
    p.areas()
}

struct Sets(Vec<usize>);

impl Sets {
    fn find(&mut self, mut a: usize) -> usize {
        while self.0[a] != a {
            self.0[a] = self.0[self.0[a]];
            a = self.0[a];
        }
        a
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[b] = a;
        }
    }
}

/// A piece of a fill, by its layer, its zone and its place in the zone's [`ZoneRegions`].
struct Piece<'a> {
    layer: &'a str,
    zone: usize,
    /// Its place in the output: the fill's and the piece's.
    fill: usize,
    index: usize,
    area: &'a Area,
    bounds: Option<Bounds>,
}

/// The edges of a piece as shapes with nothing inside: what the gap to its edge is measured to.
fn edges(piece: &Area) -> Vec<Shape> {
    std::iter::once(&piece.outer)
        .chain(&piece.holes)
        .filter(|r| r.len() >= 3)
        .map(|r| Shape::ring(r.clone(), 0.0))
        .collect()
}

/// The least gap between two sets of shapes.
fn least_gap(a: &[Shape], b: &[Shape]) -> f64 {
    a.iter()
        .flat_map(|x| b.iter().map(move |y| crate::drc::shape::gap(x, y).d))
        .fold(f64::INFINITY, f64::min)
}

/// The regions of every fill of `joins` ([`crate::pad_joins`]'s), in its order: each piece of
/// each fill, the pads and vias of its net it touches, whether it is in its net's main region,
/// and what bounds it when it is not ([module](self)).
pub fn fill_regions(
    art: &Artwork,
    rules: &Rules,
    joins: &[(String, Vec<ZoneJoins>)],
) -> Vec<(String, Vec<ZoneRegions>)> {
    let items = items(art);
    let pieces: Vec<Piece> = joins
        .iter()
        .flat_map(|(layer, zones)| {
            zones.iter().enumerate().flat_map(move |(f, z)| {
                z.fill.iter().enumerate().map(move |(k, a)| Piece {
                    layer,
                    zone: z.zone,
                    fill: f,
                    index: k,
                    area: a,
                    bounds: Bounds::of(a.outer.iter().copied()),
                })
            })
        })
        .collect();
    let net_of = |p: &Piece| art.zones[p.zone].net.as_deref();
    let n = items.len();
    let mut sets = Sets((0..n + pieces.len()).collect());
    // Copper of one net that runs into each other on a layer they share.
    for i in 0..n {
        for j in i + 1..n {
            let (a, b) = (&items[i], &items[j]);
            if a.net.is_none()
                || a.net != b.net
                || !a.bounds.expanded(TOUCH).overlaps(&b.bounds)
                || !a.layers.iter().any(|l| b.layers.contains(l))
            {
                continue;
            }
            if crate::copper_joins(&a.shapes, &b.shapes) {
                sets.join(i, j);
            }
        }
    }
    // The pieces of the fills and the copper of their net they touch.
    let mut touched: Vec<Vec<Bounding>> = vec![Vec::new(); pieces.len()];
    for (k, p) in pieces.iter().enumerate() {
        let Some(pb) = p.bounds else { continue };
        let z = &art.zones[p.zone];
        for (i, item) in items.iter().enumerate() {
            if net_of(p).is_none()
                || !own_net(z, &item.net)
                || !item.layers.iter().any(|l| l == p.layer)
                || !item.bounds.expanded(TOUCH).overlaps(&pb)
            {
                continue;
            }
            let copper = grown(&item_area(art, item.what), TOUCH);
            if overlap(std::slice::from_ref(p.area), &copper) {
                sets.join(n + k, i);
                touched[k].push(item.what);
            }
        }
        for (q, other) in pieces.iter().enumerate().skip(k + 1) {
            let Some(ob) = other.bounds else { continue };
            if other.layer == p.layer
                && net_of(other) == net_of(p)
                && net_of(p).is_some()
                && ob.expanded(TOUCH).overlaps(&pb)
                && overlap(
                    std::slice::from_ref(p.area),
                    &grown(std::slice::from_ref(other.area), TOUCH),
                )
            {
                sets.join(n + k, n + q);
            }
        }
    }
    // Each net's main group: the most pads, then the most fill.
    let mut groups: std::collections::BTreeMap<(String, usize), (usize, f64)> = Default::default();
    for (i, item) in items.iter().enumerate() {
        if let (Bounding::Pad(_), Some(net)) = (item.what, &item.net) {
            groups.entry((net.clone(), sets.find(i))).or_default().0 += 1;
        }
    }
    for (k, p) in pieces.iter().enumerate() {
        if let Some(net) = net_of(p) {
            groups
                .entry((net.to_string(), sets.find(n + k)))
                .or_default()
                .1 += p.area.size();
        }
    }
    let mut main: std::collections::BTreeMap<&str, (usize, (usize, f64))> = Default::default();
    for ((net, group), (pads, area)) in &groups {
        let better = |(bp, ba): (usize, f64)| *pads > bp || (*pads == bp && *area > ba);
        match main.get(net.as_str()) {
            Some((_, best)) if !better(*best) => {}
            _ => {
                main.insert(net, (*group, (*pads, *area)));
            }
        }
    }
    let in_main: Vec<bool> = (0..pieces.len())
        .map(|k| {
            let group = sets.find(n + k);
            net_of(&pieces[k]).is_some_and(|net| main.get(net).is_some_and(|(g, _)| *g == group))
        })
        .collect();
    let edges_of: Vec<Vec<Shape>> = pieces.iter().map(|p| edges(p.area)).collect();
    // The rule areas that forbid pours, by layer, as their outlines.
    let rule_areas: Vec<(usize, &Zone, Vec<Shape>, Bounds)> = art
        .zones
        .iter()
        .enumerate()
        .filter(|(_, z)| z.keepout.as_ref().is_some_and(|k| k.copperpour))
        .filter_map(|(i, z)| {
            let shapes: Vec<Shape> = z
                .outlines
                .iter()
                .filter(|o| o.len() >= 3)
                .map(|o| Shape::ring(o.clone(), 0.0))
                .collect();
            (!shapes.is_empty()).then(|| {
                let b = shapes_bounds(&shapes);
                (i, z, shapes, b)
            })
        })
        .collect();
    // What bounds each cut-off piece, and whether it bounds a main piece on the layer too.
    let mut bounds: Vec<Vec<Bound>> = vec![Vec::new(); pieces.len()];
    for (k, p) in pieces.iter().enumerate() {
        let Some(pb) = p.bounds else { continue };
        if in_main[k] || net_of(p).is_none() {
            continue;
        }
        let z = &art.zones[p.zone];
        let reach = |net: Option<&str>, own: f64| {
            zone_clearance(z, rules)
                .max(rules.class_clearance(net))
                .max(own)
                + z.min_thickness
                + SPARE
        };
        let mains: Vec<usize> = (0..pieces.len())
            .filter(|&m| {
                in_main[m] && pieces[m].layer == p.layer && net_of(&pieces[m]) == net_of(p)
            })
            .collect();
        let near_main = |shapes: &[Shape], b: &Bounds, within: f64| {
            mains.iter().any(|&m| {
                pieces[m]
                    .bounds
                    .is_some_and(|mb| mb.expanded(within).overlaps(b))
                    && least_gap(shapes, &edges_of[m]) <= within
            })
        };
        for item in &items {
            if own_net(z, &item.net) || !item.layers.iter().any(|l| l == p.layer) {
                continue;
            }
            let own = match item.what {
                Bounding::Pad(i) => art.pads[i].clearance.unwrap_or(0.0),
                _ => 0.0,
            };
            let within = reach(item.net.as_deref(), own);
            if !pb.expanded(within).overlaps(&item.bounds) {
                continue;
            }
            let gap = least_gap(&item.shapes, &edges_of[k]);
            if gap <= within {
                bounds[k].push(Bound {
                    what: item.what,
                    gap,
                    parts_from_main: near_main(&item.shapes, &item.bounds, within),
                });
            }
        }
        for (i, area, shapes, b) in &rule_areas {
            if !area.layers.iter().any(|l| l == p.layer) {
                continue;
            }
            let within = super::MARGIN + z.min_thickness + SPARE;
            if !pb.expanded(within).overlaps(b) {
                continue;
            }
            let gap = least_gap(shapes, &edges_of[k]);
            if gap <= within {
                bounds[k].push(Bound {
                    what: Bounding::RuleArea(*i),
                    gap,
                    parts_from_main: near_main(shapes, b, within),
                });
            }
        }
        bounds[k].sort_by(|a, b| {
            b.parts_from_main
                .cmp(&a.parts_from_main)
                .then(a.gap.total_cmp(&b.gap))
        });
    }
    let mut out: Vec<(String, Vec<ZoneRegions>)> = joins
        .iter()
        .map(|(layer, zones)| {
            (
                layer.clone(),
                zones
                    .iter()
                    .map(|z| ZoneRegions {
                        zone: z.zone,
                        regions: Vec::new(),
                    })
                    .collect(),
            )
        })
        .collect();
    let layer_at = |name: &str| joins.iter().position(|(l, _)| l == name);
    for (k, p) in pieces.iter().enumerate() {
        let Some(l) = layer_at(p.layer) else { continue };
        let (pads, vias) =
            touched[k]
                .iter()
                .fold((Vec::new(), Vec::new()), |(mut pads, mut vias), t| {
                    match *t {
                        Bounding::Pad(i) => pads.push(i),
                        Bounding::Via(i) => vias.push(i),
                        _ => {}
                    }
                    (pads, vias)
                });
        debug_assert_eq!(out[l].1[p.fill].regions.len(), p.index);
        out[l].1[p.fill].regions.push(Region {
            piece: p.area.clone(),
            pads,
            vias,
            main: in_main[k],
            bounds: std::mem::take(&mut bounds[k]),
        });
    }
    out
}
