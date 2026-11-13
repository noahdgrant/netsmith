//! The board as the check sees it: copper items with their shapes, nets and layers, the
//! holes, the zones, the outline.

use super::shape::Shape;
use super::{Extras, Item, Rules};
use crate::artwork::{Artwork, Pad, PadKind, PadShape, Shape as Drawn, TrackShape};
use crate::drill;
use crate::geom::{self, Bounds, Pt, circle_polygon};
use crate::pads::{self, Outline, Prim};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Pad(usize),
    Via(usize),
    Track(usize),
    /// A teardrop: an index into the artwork's zones.
    Teardrop(usize),
}

impl Kind {
    /// A track or a teardrop, the copper the router draws between pads and vias: the rules of
    /// the process that are "to a track" hold for both.
    pub(crate) fn is_track_like(self) -> bool {
        matches!(self, Kind::Track(_) | Kind::Teardrop(_))
    }
}

/// A pad, via or track: copper that exists on some layers.
#[derive(Debug, Clone)]
pub(crate) struct Copper {
    pub kind: Kind,
    pub net: Option<String>,
    /// Indices into [`Model::layers`].
    pub layers: Vec<usize>,
    pub shapes: Vec<Shape>,
    pub bounds: Bounds,
    pub item: Item,
    /// The pad's own clearance, which counts if it is larger than the class's.
    pub own_clearance: f64,
    /// Points where a track joins other copper, or the centre of a pad or via.
    pub anchors: Vec<Pt>,
    /// A track's length, else zero.
    pub length: f64,
}

/// A pour: the copper of a zone on one layer, as its outline (no fill is computed).
#[derive(Debug, Clone)]
pub(crate) struct Pour {
    pub layer: usize,
    pub net: Option<String>,
    pub polygons: Vec<Vec<Pt>>,
    /// Whether the zone joins the pads of its net (`connect_pads` is not `no`).
    pub joins_pads: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Hole {
    pub shape: Shape,
    pub diameter: f64,
    pub plated: bool,
    /// The pad or via that has the hole: an index into [`Model::items`].
    pub owner: Option<usize>,
    pub item: Item,
    pub at: Pt,
}

pub(crate) struct Model<'a> {
    pub art: &'a Artwork,
    pub rules: &'a Rules,
    pub extras: &'a Extras,
    pub bridges_in_footprints: bool,
    pub layers: Vec<String>,
    pub items: Vec<Copper>,
    pub pours: Vec<Pour>,
    pub holes: Vec<Hole>,
    /// The drawings of `Edge.Cuts`, each with the stroke it is drawn with.
    pub edge: Vec<(Shape, Item)>,
    pub via_item: Vec<usize>,
}

fn net_text(net: &Option<String>) -> String {
    net.as_ref().map(|n| format!(" [{n}]")).unwrap_or_default()
}

/// The shapes that make a pad's outline.
pub fn outline_shapes(outline: &Outline) -> Vec<Shape> {
    match outline {
        Outline::Circle { c, r } => vec![Shape::disc(*c, *r)],
        Outline::Oval { c, w, h, angle } => {
            let (long, short) = (w.max(*h), w.min(*h));
            let half = (long - short) / 2.0;
            let axis = if w >= h {
                Pt::new(half, 0.0)
            } else {
                Pt::new(0.0, half)
            }
            .rotate(*angle);
            vec![Shape::capsule(*c - axis, *c + axis, short / 2.0)]
        }
        Outline::Custom { anchor, prims } => {
            let mut shapes = outline_shapes(anchor);
            for p in prims {
                match p {
                    Prim::Line { a, b, width } => shapes.push(Shape::capsule(*a, *b, width / 2.0)),
                    Prim::Arc { arc, width } => {
                        shapes.push(Shape::stroke(arc.points(), width / 2.0))
                    }
                    Prim::Circle { c, r, width, fill } => {
                        if *fill {
                            shapes.push(Shape::disc(*c, r + width / 2.0));
                        } else {
                            shapes.push(Shape::ring(circle_polygon(*c, *r), width / 2.0));
                        }
                    }
                    Prim::Poly {
                        points,
                        width,
                        fill,
                    } => {
                        if *fill {
                            shapes.push(Shape::polygon(points.clone(), width / 2.0));
                        } else {
                            shapes.push(Shape::ring(points.clone(), width / 2.0));
                        }
                    }
                }
            }
            shapes
        }
        other => match other.polygon() {
            Some(p) => vec![Shape::polygon(p, 0.0)],
            None => Vec::new(),
        },
    }
}

fn bounds_of(shapes: &[Shape]) -> Bounds {
    let mut it = shapes.iter();
    let mut b = it.next().map(|s| s.bounds).unwrap_or(Bounds {
        min: Pt::default(),
        max: Pt::default(),
    });
    for s in it {
        b.add(s.bounds.min);
        b.add(s.bounds.max);
    }
    b
}

/// A drawn line, arc, circle or polygon as the shape it inks, `r` being half its width.
pub(crate) fn drawn_shape(shape: &Drawn, width: f64, filled: bool) -> Shape {
    let r = width / 2.0;
    match shape {
        Drawn::Line(a, b) => Shape::capsule(*a, *b, r),
        Drawn::Arc(arc) => Shape::stroke(arc.points(), r),
        Drawn::Circle { centre, radius } => {
            if filled {
                Shape::disc(*centre, radius + r)
            } else {
                Shape::ring(circle_polygon(*centre, *radius), r)
            }
        }
        Drawn::Poly(points) => {
            if filled {
                Shape::polygon(points.clone(), r)
            } else {
                Shape::ring(points.clone(), r)
            }
        }
    }
}

/// Where KiCad says a drawing is: the start of a line or arc, the centre of a circle, the
/// first point of a polygon.
pub(crate) fn drawn_at(shape: &Drawn) -> Pt {
    match shape {
        Drawn::Line(a, _) => *a,
        Drawn::Arc(a) => a.start,
        Drawn::Circle { centre, .. } => *centre,
        Drawn::Poly(p) => p.first().copied().unwrap_or_default(),
    }
}

impl<'a> Model<'a> {
    pub(crate) fn new(
        art: &'a Artwork,
        rules: &'a Rules,
        extras: &'a Extras,
        bridges_in_footprints: bool,
    ) -> Model<'a> {
        let layers = art.copper.clone();
        let layer_index = |name: &str| layers.iter().position(|l| l == name);
        let mut items = Vec::new();
        let mut pad_item = Vec::new();
        let mut via_item = Vec::new();

        for (i, pad) in art.pads.iter().enumerate() {
            let on: Vec<usize> = layers
                .iter()
                .enumerate()
                .filter(|(_, l)| art.pad_copper_on(pad, l))
                .map(|(k, _)| k)
                .collect();
            if on.is_empty() {
                pad_item.push(None);
                continue;
            }
            let shapes = outline_shapes(&pads::outline(pad));
            let kind = match pad.kind {
                PadKind::Smd | PadKind::Connect => "SMD pad",
                PadKind::Thru => "PTH pad",
                PadKind::Npth => "NPTH pad",
            };
            let item = Item {
                what: format!(
                    "{kind} {}{} of {}",
                    pad.number,
                    net_text(&pad.net),
                    pad.part
                ),
                at: pad.at,
                part: Some(pad.part.clone()),
                pad: Some(pad.number.clone()),
                net: pad.net.clone(),
            };
            pad_item.push(Some(items.len()));
            items.push(Copper {
                kind: Kind::Pad(i),
                net: pad.net.clone(),
                layers: on,
                bounds: bounds_of(&shapes),
                shapes,
                item,
                own_clearance: pad.clearance.unwrap_or(0.0),
                anchors: vec![pad.at],
                length: 0.0,
            });
        }
        for (i, via) in art.vias.iter().enumerate() {
            let shapes = vec![Shape::disc(via.at, via.diameter / 2.0)];
            via_item.push(items.len());
            items.push(Copper {
                kind: Kind::Via(i),
                net: via.net.clone(),
                layers: (0..layers.len()).collect(),
                bounds: bounds_of(&shapes),
                shapes,
                item: Item {
                    what: format!(
                        "Via{} on {} - {}",
                        net_text(&via.net),
                        layers.first().map(String::as_str).unwrap_or("F.Cu"),
                        layers.last().map(String::as_str).unwrap_or("B.Cu")
                    ),
                    at: via.at,
                    part: None,
                    pad: None,
                    net: via.net.clone(),
                },
                own_clearance: 0.0,
                anchors: vec![via.at],
                length: 0.0,
            });
        }
        for (i, t) in art.tracks.iter().enumerate() {
            let Some(layer) = layer_index(&t.layer) else {
                continue;
            };
            // Where KiCad says a track is: a line's start, an arc's centre.
            let (shape, start, end, length, at, kind) = match &t.shape {
                TrackShape::Line(a, b) => (
                    Shape::capsule(*a, *b, t.width / 2.0),
                    *a,
                    *b,
                    a.dist(*b),
                    *a,
                    "Track",
                ),
                TrackShape::Arc(arc) => (
                    Shape::stroke(arc.points(), t.width / 2.0),
                    arc.start,
                    arc.end,
                    arc.radius * arc.sweep().abs(),
                    arc.centre,
                    "Track (arc)",
                ),
            };
            items.push(Copper {
                kind: Kind::Track(i),
                net: t.net.clone(),
                layers: vec![layer],
                bounds: shape.bounds,
                shapes: vec![shape],
                item: Item {
                    what: format!(
                        "{kind}{} on {}, length {:.4} mm",
                        net_text(&t.net),
                        t.layer,
                        length
                    ),
                    at,
                    part: None,
                    pad: None,
                    net: t.net.clone(),
                },
                own_clearance: 0.0,
                anchors: vec![start, end],
                length,
            });
        }

        // A teardrop's fill is copper like a track's, checked against everything else.
        for (i, z) in art.zones.iter().enumerate() {
            if !z.teardrop || z.keepout.is_some() {
                continue;
            }
            for l in &z.layers {
                let Some(layer) = layer_index(l) else {
                    continue;
                };
                let shapes: Vec<Shape> = z
                    .filled
                    .iter()
                    .filter(|(fl, p)| fl == l && p.len() >= 3)
                    .map(|(_, p)| Shape::polygon(p.clone(), 0.0))
                    .collect();
                let Some(at) = shapes.first().map(|s| s.pts[0]) else {
                    continue;
                };
                items.push(Copper {
                    kind: Kind::Teardrop(i),
                    net: z.net.clone(),
                    layers: vec![layer],
                    bounds: bounds_of(&shapes),
                    shapes,
                    item: Item {
                        what: format!("Teardrop{} on {l}", net_text(&z.net)),
                        at,
                        part: None,
                        pad: None,
                        net: z.net.clone(),
                    },
                    own_clearance: 0.0,
                    anchors: vec![at],
                    length: 0.0,
                });
            }
        }

        let mut pours = Vec::new();
        for z in &art.zones {
            if z.keepout.is_some() || z.net.is_none() || z.teardrop {
                continue;
            }
            for l in &z.layers {
                let Some(layer) = layer_index(l) else {
                    continue;
                };
                // The fill the file holds, else the outline the fill would stay inside.
                let filled: Vec<Vec<Pt>> = z
                    .filled
                    .iter()
                    .filter(|(fl, _)| fl == l)
                    .map(|(_, p)| p.clone())
                    .collect();
                let polygons = if filled.is_empty() {
                    z.outlines.clone()
                } else {
                    filled
                };
                pours.push(Pour {
                    layer,
                    net: z.net.clone(),
                    polygons,
                    joins_pads: z.connect != crate::artwork::Connect::None,
                });
            }
        }

        let mut holes = Vec::new();
        for h in drill::holes(art) {
            let (owner, item, label) = if h.via {
                let it = via_item[h.index];
                (Some(it), items[it].item.clone(), "Via")
            } else {
                let pad = &art.pads[h.index];
                let label = if pad.kind == PadKind::Npth {
                    "NPTH pad"
                } else {
                    "PTH pad"
                };
                let item = Item {
                    what: format!("{label} {} of {}", pad.number, pad.part),
                    at: pad.at,
                    part: Some(pad.part.clone()),
                    pad: Some(pad.number.clone()),
                    net: pad.net.clone(),
                };
                (pad_item[h.index], item, label)
            };
            let _ = label;
            let shape = match h.slot_to {
                Some(to) => Shape::capsule(h.at, to, h.diameter / 2.0),
                None => Shape::disc(h.at, h.diameter / 2.0),
            };
            holes.push(Hole {
                shape,
                diameter: h.diameter,
                plated: h.plated,
                owner,
                item,
                at: h.at,
            });
        }

        let edge = art
            .graphics
            .iter()
            .filter(|g| g.layer == "Edge.Cuts")
            .map(|g| {
                (
                    drawn_shape(&g.shape, g.width, g.fill),
                    Item {
                        what: "Segment on Edge.Cuts".to_string(),
                        at: drawn_at(&g.shape),
                        part: None,
                        pad: None,
                        net: None,
                    },
                )
            })
            .collect();

        Model {
            art,
            rules,
            extras,
            bridges_in_footprints,
            layers,
            items,
            pours,
            holes,
            edge,
            via_item,
        }
    }

    pub(crate) fn layer_name(&self, i: usize) -> &str {
        &self.layers[i]
    }

    /// The shape of the opening in the solder mask over a pad on the front (`front`) or the
    /// back, or `None` if the mask covers it.
    pub(crate) fn mask_opening(&self, pad: &Pad, front: bool) -> Vec<Shape> {
        let layer = if front { "F.Mask" } else { "B.Mask" };
        if !pad.layers.iter().any(|l| l == layer) {
            return Vec::new();
        }
        let margin = pad.mask_margin.unwrap_or(self.art.mask_margin);
        let mut outline = pads::outline(pad);
        if pad.shape == PadShape::Custom && margin < 0.0 {
            return Vec::new();
        }
        if margin != 0.0 {
            match outline.grow(margin) {
                Ok(Some(o)) => outline = o,
                _ => return Vec::new(),
            }
        }
        outline_shapes(&outline)
    }

    /// The box round every shape of a copper item.
    pub(crate) fn bounds_of(shapes: &[Shape]) -> Bounds {
        bounds_of(shapes)
    }
}

/// A closed path's points with the repeated end dropped.
pub(crate) fn open_ring(mut pts: Vec<Pt>) -> Vec<Pt> {
    if pts.len() > 1 && pts[0].dist(pts[pts.len() - 1]) < 1e-9 {
        pts.pop();
    }
    pts
}

/// Whether the polygons `a` and `b` share area (touching along an edge does not count).
pub(crate) fn overlap_area(a: &[Pt], b: &[Pt]) -> bool {
    const EPS: f64 = 1e-6;
    let strictly_inside = |poly: &[Pt], p: Pt| {
        geom::contains(poly, p) && {
            let s = Shape::ring(poly.to_vec(), 0.0);
            super::shape::gap(&s, &Shape::disc(p, 0.0)).d > EPS
        }
    };
    let samples = |poly: &[Pt]| -> Vec<Pt> {
        let n = poly.len();
        let mut out: Vec<Pt> = poly.to_vec();
        for i in 0..n {
            let (p, q) = (poly[i], poly[(i + 1) % n]);
            out.push(Pt::new((p.x + q.x) / 2.0, (p.y + q.y) / 2.0));
        }
        let (sx, sy) = poly.iter().fold((0.0, 0.0), |s, p| (s.0 + p.x, s.1 + p.y));
        out.push(Pt::new(sx / n as f64, sy / n as f64));
        out
    };
    if samples(a).into_iter().any(|p| strictly_inside(b, p))
        || samples(b).into_iter().any(|p| strictly_inside(a, p))
    {
        return true;
    }
    // Edges that cross each other properly.
    let (na, nb) = (a.len(), b.len());
    for i in 0..na {
        let (p0, p1) = (a[i], a[(i + 1) % na]);
        for j in 0..nb {
            let (q0, q1) = (b[j], b[(j + 1) % nb]);
            let (r, s) = (p1 - p0, q1 - q0);
            let denom = r.x * s.y - r.y * s.x;
            if denom.abs() < 1e-12 {
                continue;
            }
            let qp = q0 - p0;
            let t = (qp.x * s.y - qp.y * s.x) / denom;
            let u = (qp.x * r.y - qp.y * r.x) / denom;
            if t > EPS && t < 1.0 - EPS && u > EPS && u < 1.0 - EPS {
                return true;
            }
        }
    }
    false
}
