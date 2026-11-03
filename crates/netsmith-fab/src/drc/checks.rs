//! The rules that look at one board's geometry: clearances, holes, edge, courtyards,
//! silkscreen, solder mask, rule areas and the outline. Connectivity is in `connect.rs`.

use super::model::{Copper, Kind, Model, drawn_at, drawn_shape, open_ring, overlap_area};
use super::shape::{Shape, gap, near_pairs, touches_area};
use super::{Finding, Item, Rule};
use crate::artwork::{Graphic, PadKind, Text};
use crate::edge;
use crate::font;
use crate::geom::{self, Bounds, Pt};

/// What a measurement may fall short of its limit by and still pass: KiCad's own epsilon
/// (half a micrometre), so that a track routed exactly at the limit passes in both checks.
pub(crate) const EPS: f64 = 0.0005;

pub(crate) struct Out<'a> {
    pub m: &'a Model<'a>,
    pub findings: &'a mut Vec<Finding>,
}

impl Out<'_> {
    pub fn push(
        &mut self,
        rule: Rule,
        message: String,
        at: Pt,
        layer: Option<&str>,
        items: Vec<Item>,
    ) {
        if let Some(severity) = self.m.rules.severity(rule) {
            self.findings.push(Finding {
                rule,
                severity,
                message,
                at,
                layer: layer.map(str::to_string),
                items,
            });
        }
    }
}

pub(crate) fn run(m: &Model<'_>, findings: &mut Vec<Finding>) {
    let mut out = Out { m, findings };
    outline(&mut out);
    sizes(&mut out);
    copper_clearance(&mut out);
    holes(&mut out);
    edges(&mut out);
    courtyards(&mut out);
    silk(&mut out);
    mask_bridges(&mut out);
    rule_areas(&mut out);
    via_in_pad(&mut out);
}

fn same_net(a: &Option<String>, b: &Option<String>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => x == y,
        (None, None) => true,
        _ => false,
    }
}

/// The least gap between two copper items on any layer they share, with the layer.
fn copper_gap(a: &Copper, b: &Copper, m: &Model<'_>) -> Option<(super::shape::Gap, usize)> {
    let layer = a.layers.iter().copied().find(|l| b.layers.contains(l))?;
    let mut best: Option<super::shape::Gap> = None;
    for sa in &a.shapes {
        for sb in &b.shapes {
            if !boxes_within(&sa.bounds, &sb.bounds, m.max_gap()) {
                continue;
            }
            let g = gap(sa, sb);
            if best.is_none_or(|b| g.d < b.d) {
                best = Some(g);
            }
        }
    }
    best.map(|g| (g, layer))
}

fn boxes_within(a: &Bounds, b: &Bounds, d: f64) -> bool {
    a.min.x <= b.max.x + d
        && b.min.x <= a.max.x + d
        && a.min.y <= b.max.y + d
        && b.min.y <= a.max.y + d
}

impl Model<'_> {
    /// The largest space any rule asks for between two things.
    pub fn max_gap(&self) -> f64 {
        let r = self.rules;
        let classes = r
            .classes
            .iter()
            .filter_map(|c| c.clearance)
            .fold(r.default_clearance, f64::max);
        classes
            .max(r.clearance)
            .max(r.hole_clearance)
            .max(r.hole_to_hole)
            .max(r.copper_edge)
            .max(r.silk_clearance)
            .max(r.mask_web)
            .max(0.5)
    }
}

fn outline(out: &mut Out<'_>) {
    let m = out.m;
    match edge::outline(m.art) {
        Err(e) => {
            let at = m.edge.first().map(|(_, i)| i.at).unwrap_or_default();
            let items = m
                .edge
                .first()
                .map(|(_, i)| vec![i.clone()])
                .unwrap_or_default();
            out.push(Rule::InvalidOutline, e, at, Some("Edge.Cuts"), items);
        }
        Ok(o) => {
            for cut in &o.cutouts {
                if let Some(p) = cut.iter().find(|p| !geom::contains(&o.outer, **p)) {
                    out.push(
                        Rule::InvalidOutline,
                        format!(
                            "a cutout of the board outline has a corner at ({:.3}, {:.3}) outside the board",
                            p.x, p.y
                        ),
                        *p,
                        Some("Edge.Cuts"),
                        Vec::new(),
                    );
                }
            }
        }
    }
}

/// Track width, via size, annular ring and drill size.
fn sizes(out: &mut Out<'_>) {
    let m = out.m;
    let r = m.rules;
    for c in &m.items {
        match c.kind {
            Kind::Track(i) => {
                let t = &m.art.tracks[i];
                if t.width < r.track_width - EPS / 100.0 {
                    out.push(
                        Rule::TrackWidth,
                        format!(
                            "track is {:.4} mm wide, the least is {:.4} mm",
                            t.width, r.track_width
                        ),
                        c.item.at,
                        Some(&t.layer),
                        vec![c.item.clone()],
                    );
                }
            }
            Kind::Via(i) => {
                let v = &m.art.vias[i];
                if v.diameter < r.via_diameter - EPS / 100.0 {
                    out.push(
                        Rule::ViaDiameter,
                        format!(
                            "via is {:.4} mm across, the least is {:.4} mm",
                            v.diameter, r.via_diameter
                        ),
                        v.at,
                        None,
                        vec![c.item.clone()],
                    );
                }
                let ring = (v.diameter - v.drill) / 2.0;
                if v.drill > 0.0 && ring < r.annular_width - EPS / 100.0 {
                    out.push(
                        Rule::AnnularWidth,
                        format!(
                            "via has {ring:.4} mm of copper round its {:.4} mm drill, the least is {:.4} mm",
                            v.drill, r.annular_width
                        ),
                        v.at,
                        None,
                        vec![c.item.clone()],
                    );
                }
            }
            Kind::Pad(i) => {
                let pad = &m.art.pads[i];
                let Some(d) = &pad.drill else { continue };
                if pad.kind != PadKind::Thru {
                    continue;
                }
                // The copper left between the hole and the pad's edge, the least all round.
                let hole = m
                    .holes
                    .iter()
                    .find(|h| h.owner.is_some_and(|o| m.items[o].kind == c.kind));
                let Some(hole) = hole else { continue };
                let _ = d;
                let ring = ring_width(&c.shapes, &hole.shape);
                if ring < r.annular_width - EPS / 100.0 {
                    out.push(
                        Rule::AnnularWidth,
                        format!(
                            "plated hole has {ring:.4} mm of copper round its {:.4} mm drill, the least is {:.4} mm",
                            hole.diameter, r.annular_width
                        ),
                        pad.at,
                        None,
                        vec![c.item.clone()],
                    );
                }
            }
        }
    }
    for h in &m.holes {
        if h.diameter < r.through_hole - EPS / 100.0 {
            out.push(
                Rule::DrillOutOfRange,
                format!(
                    "hole is {:.4} mm, the least the process drills is {:.4} mm",
                    h.diameter, r.through_hole
                ),
                h.at,
                None,
                vec![h.item.clone()],
            );
        }
    }
}

/// The copper left between a hole and the edge of the pad round it, the least all round.
fn ring_width(copper: &[Shape], hole: &Shape) -> f64 {
    // How far inside the pad's copper a point is: the distance to its edge, or negative
    // outside.
    let depth = |p: Pt| -> f64 {
        copper
            .iter()
            .map(|s| {
                if s.filled && s.pts.len() >= 3 {
                    let edge = gap(&Shape::ring(s.pts.clone(), 0.0), &Shape::disc(p, 0.0)).d;
                    if geom::contains(&s.pts, p) {
                        edge + s.r
                    } else {
                        s.r - edge
                    }
                } else {
                    s.r - gap(&Shape::stroke(s.pts.clone(), 0.0), &Shape::disc(p, 0.0)).d
                }
            })
            .fold(f64::MIN, f64::max)
    };
    let mut at: Vec<Pt> = hole.pts.clone();
    if let [a, b] = hole.pts[..] {
        at.push(Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0));
    }
    at.into_iter().map(depth).fold(f64::MAX, f64::min) - hole.r
}

/// Copper of two nets that is too near, or touches.
fn copper_clearance(out: &mut Out<'_>) {
    let m = out.m;
    let boxes: Vec<Bounds> = m.items.iter().map(|c| c.bounds).collect();
    for (i, j) in near_pairs(&boxes, m.max_gap()) {
        let (a, b) = (&m.items[i], &m.items[j]);
        if same_net(&a.net, &b.net) {
            continue;
        }
        // Two items of one pad cannot be told apart from a pad; a pad's own copper is one item.
        let Some((g, layer)) = copper_gap(a, b, m) else {
            continue;
        };
        let need = m
            .rules
            .net_clearance(a.net.as_deref())
            .max(m.rules.net_clearance(b.net.as_deref()))
            .max(a.own_clearance)
            .max(b.own_clearance);
        let layer_name = m.layer_name(layer).to_string();
        let tracks = matches!(a.kind, Kind::Track(_)) && matches!(b.kind, Kind::Track(_));
        let both_nets = a.net.is_some() && b.net.is_some();
        // KiCad reports a short or a crossing once, and a clearance once on every layer the
        // two share, except for two pads, which it reports once.
        let layers: Vec<usize> = if matches!(a.kind, Kind::Pad(_)) && matches!(b.kind, Kind::Pad(_))
        {
            vec![layer]
        } else {
            a.layers
                .iter()
                .copied()
                .filter(|l| b.layers.contains(l))
                .collect()
        };
        if g.d == 0.0 && both_nets && tracks {
            out.push(
                Rule::TracksCrossing,
                format!(
                    "{} and {} cross; they are on nets {} and {}",
                    a.item.what,
                    b.item.what,
                    a.net.as_deref().unwrap_or(""),
                    b.net.as_deref().unwrap_or("")
                ),
                g.middle(),
                Some(&layer_name),
                vec![a.item.clone(), b.item.clone()],
            );
        } else if g.d == 0.0 && both_nets {
            out.push(
                Rule::ShortingItems,
                format!(
                    "{} and {} touch; they are on nets {} and {}",
                    a.item.what,
                    b.item.what,
                    a.net.as_deref().unwrap_or(""),
                    b.net.as_deref().unwrap_or("")
                ),
                g.middle(),
                Some(&layer_name),
                vec![a.item.clone(), b.item.clone()],
            );
        } else if g.d < need - EPS {
            for l in layers {
                out.push(
                    Rule::Clearance,
                    format!(
                        "clearance is {:.4} mm between {} and {}; the least is {:.4} mm",
                        g.d, a.item.what, b.item.what, need
                    ),
                    g.middle(),
                    Some(m.layer_name(l)),
                    vec![a.item.clone(), b.item.clone()],
                );
            }
        }
    }
}

/// Holes against copper of other nets and against each other.
fn holes(out: &mut Out<'_>) {
    let m = out.m;
    let r = m.rules;
    let hole_boxes: Vec<Bounds> = m.holes.iter().map(|h| h.shape.bounds).collect();
    // Hole to hole.
    for (i, j) in near_pairs(&hole_boxes, m.max_gap()) {
        let (a, b) = (&m.holes[i], &m.holes[j]);
        if a.owner.is_some() && a.owner == b.owner {
            continue;
        }
        let g = gap(&a.shape, &b.shape);
        if g.d < r.hole_to_hole - EPS {
            out.push(
                Rule::HoleToHole,
                format!(
                    "holes are {:.4} mm apart ({} and {}); the least is {:.4} mm",
                    g.d, a.item.what, b.item.what, r.hole_to_hole
                ),
                g.middle(),
                None,
                vec![a.item.clone(), b.item.clone()],
            );
        }
    }
    // Hole to copper of another net, on any layer: the hole goes through them all.
    let item_boxes: Vec<Bounds> = m.items.iter().map(|c| c.bounds).collect();
    let mut all = item_boxes.clone();
    all.extend(hole_boxes);
    let n = m.items.len();
    for (i, j) in near_pairs(&all, m.max_gap()) {
        if i >= n || j < n {
            continue;
        }
        let (c, h) = (&m.items[i], &m.holes[j - n]);
        if h.owner == Some(i) {
            continue;
        }
        let hole_net = &h.item.net;
        if hole_net.is_some() && hole_net == &c.net {
            continue;
        }
        let g =
            c.shapes
                .iter()
                .map(|s| gap(s, &h.shape))
                .fold(None::<super::shape::Gap>, |best, g| match best {
                    Some(b) if b.d <= g.d => Some(b),
                    _ => Some(g),
                });
        let Some(g) = g else { continue };
        if g.d < r.hole_clearance - EPS {
            out.push(
                Rule::HoleClearance,
                format!(
                    "hole clearance is {:.4} mm between {} and {}; the least is {:.4} mm",
                    g.d, c.item.what, h.item.what, r.hole_clearance
                ),
                g.middle(),
                None,
                vec![c.item.clone(), h.item.clone()],
            );
        }
    }
}

/// Copper and holes against the board edge.
fn edges(out: &mut Out<'_>) {
    let m = out.m;
    if m.edge.is_empty() {
        return;
    }
    let least = m.rules.copper_edge;
    let edge_boxes: Vec<Bounds> = m.edge.iter().map(|(s, _)| s.bounds).collect();
    let n = m.items.len();
    let mut all: Vec<Bounds> = m.items.iter().map(|c| c.bounds).collect();
    all.extend(m.holes.iter().map(|h| h.shape.bounds));
    all.extend(edge_boxes);
    let nh = m.holes.len();
    // The nearest edge for each copper item and each hole.
    let mut nearest_item: Vec<Option<(super::shape::Gap, usize)>> = vec![None; n];
    let mut nearest_hole: Vec<Option<(super::shape::Gap, usize)>> = vec![None; nh];
    for (i, j) in near_pairs(&all, least) {
        if j < n + nh || i >= n + nh {
            continue;
        }
        let e = j - n - nh;
        let edge_shape = &m.edge[e].0;
        if i < n {
            let best = m.items[i].shapes.iter().map(|s| gap(s, edge_shape)).fold(
                None::<super::shape::Gap>,
                |b, g| match b {
                    Some(b) if b.d <= g.d => Some(b),
                    _ => Some(g),
                },
            );
            if let Some(g) = best
                && nearest_item[i].is_none_or(|(b, _)| g.d < b.d)
            {
                nearest_item[i] = Some((g, e));
            }
        } else {
            let g = gap(&m.holes[i - n].shape, edge_shape);
            if nearest_hole[i - n].is_none_or(|(b, _)| g.d < b.d) {
                nearest_hole[i - n] = Some((g, e));
            }
        }
    }
    for (i, near) in nearest_item.iter().enumerate() {
        let Some((g, e)) = near else { continue };
        if g.d < least - EPS {
            let c = &m.items[i];
            let layer = c.layers.first().map(|l| m.layer_name(*l).to_string());
            out.push(
                Rule::CopperEdgeClearance,
                format!(
                    "{} is {:.4} mm from the board edge; the least is {:.4} mm",
                    c.item.what, g.d, least
                ),
                g.middle(),
                layer.as_deref(),
                vec![m.edge[*e].1.clone(), c.item.clone()],
            );
        }
    }
    for (i, near) in nearest_hole.iter().enumerate() {
        let Some((g, e)) = near else { continue };
        // A plated hole is inside its copper, which the rule above has measured already.
        let h = &m.holes[i];
        if !h.plated && g.d < least - EPS {
            out.push(
                Rule::HoleToEdge,
                format!(
                    "{} has its hole {:.4} mm from the board edge; the least is {:.4} mm",
                    h.item.what, g.d, least
                ),
                g.middle(),
                None,
                vec![m.edge[*e].1.clone(), h.item.clone()],
            );
        }
    }
}

/// The courtyards of the footprints: closed shapes on `F.CrtYd` or `B.CrtYd`.
struct Courtyard {
    owner: String,
    layer: String,
    polygon: Vec<Pt>,
    at: Pt,
}

fn courtyards(out: &mut Out<'_>) {
    let m = out.m;
    let mut yards: Vec<Courtyard> = Vec::new();
    for fp in &m.art.footprints {
        for layer in ["F.CrtYd", "B.CrtYd"] {
            let mut pieces: Vec<Vec<Pt>> = Vec::new();
            let mut closed: Vec<Vec<Pt>> = Vec::new();
            let mut any = false;
            for g in m
                .art
                .graphics
                .iter()
                .filter(|g| g.layer == layer && g.owner == fp.part)
            {
                any = true;
                match &g.shape {
                    crate::artwork::Shape::Line(a, b) => pieces.push(vec![*a, *b]),
                    crate::artwork::Shape::Arc(arc) => pieces.push(arc.points()),
                    crate::artwork::Shape::Circle { centre, radius } => {
                        closed.push(geom::circle_polygon(*centre, *radius))
                    }
                    crate::artwork::Shape::Poly(p) => closed.push(open_ring(p.clone())),
                }
            }
            if !any {
                continue;
            }
            let item = Item {
                what: format!("Footprint {}", fp.part),
                at: fp.at,
                part: Some(fp.part.clone()),
                pad: None,
                net: None,
            };
            match edge::join_paths(pieces, closed) {
                Ok(polys) => {
                    for polygon in polys.into_iter().filter(|p| p.len() >= 3) {
                        yards.push(Courtyard {
                            owner: fp.part.clone(),
                            layer: layer.to_string(),
                            polygon,
                            at: fp.at,
                        });
                    }
                }
                Err((start, end)) => out.push(
                    Rule::MalformedCourtyard,
                    format!(
                        "the courtyard of {} on {layer} is not closed: a path from ({:.3}, {:.3}) ends at ({:.3}, {:.3})",
                        fp.part, start.x, start.y, end.x, end.y
                    ),
                    start,
                    Some(layer),
                    vec![item],
                ),
            }
        }
    }
    let boxes: Vec<Bounds> = yards
        .iter()
        .map(|y| {
            Bounds::of(y.polygon.iter().copied()).unwrap_or(Bounds {
                min: Pt::default(),
                max: Pt::default(),
            })
        })
        .collect();
    let mut seen: Vec<(String, String)> = Vec::new();
    for (i, j) in near_pairs(&boxes, 0.0) {
        let (a, b) = (&yards[i], &yards[j]);
        if a.layer != b.layer || a.owner == b.owner {
            continue;
        }
        if !overlap_area(&a.polygon, &b.polygon) {
            continue;
        }
        // Two courtyards of one pair of parts are one finding.
        let key = (a.owner.clone(), b.owner.clone());
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let shared = Bounds {
            min: Pt::new(
                boxes[i].min.x.max(boxes[j].min.x),
                boxes[i].min.y.max(boxes[j].min.y),
            ),
            max: Pt::new(
                boxes[i].max.x.min(boxes[j].max.x),
                boxes[i].max.y.min(boxes[j].max.y),
            ),
        };
        let at = Pt::new(
            (shared.min.x + shared.max.x) / 2.0,
            (shared.min.y + shared.max.y) / 2.0,
        );
        let item = |y: &Courtyard| Item {
            what: format!("Footprint {}", y.owner),
            at: y.at,
            part: Some(y.owner.clone()),
            pad: None,
            net: None,
        };
        out.push(
            Rule::CourtyardsOverlap,
            format!(
                "the courtyards of {} and {} overlap on {}",
                a.owner, b.owner, a.layer
            ),
            at,
            Some(&a.layer),
            vec![item(a), item(b)],
        );
    }
}

/// A drawing or text on a silkscreen layer, as the ink it lays down.
struct Ink {
    text: bool,
    layer: &'static str,
    owner: String,
    shapes: Vec<Shape>,
    bounds: Bounds,
    item: Item,
}

fn silk_layer(name: &str) -> Option<&'static str> {
    match name {
        "F.SilkS" => Some("F.SilkS"),
        "B.SilkS" => Some("B.SilkS"),
        _ => None,
    }
}

fn inks(m: &Model<'_>) -> Vec<Ink> {
    let mut inks = Vec::new();
    for g in &m.art.graphics {
        let Some(layer) = silk_layer(&g.layer) else {
            continue;
        };
        inks.push(graphic_ink(g, layer));
    }
    for t in &m.art.texts {
        let Some(layer) = silk_layer(&t.layer) else {
            continue;
        };
        inks.push(text_ink(t, layer));
    }
    inks
}

fn graphic_ink(g: &Graphic, layer: &'static str) -> Ink {
    let shape = drawn_shape(&g.shape, g.width, g.fill);
    let what = match g.shape {
        crate::artwork::Shape::Line(..) => "Segment",
        crate::artwork::Shape::Arc(_) => "Arc",
        crate::artwork::Shape::Circle { .. } => "Circle",
        crate::artwork::Shape::Poly(_) => "Polygon",
    };
    Ink {
        text: false,
        layer,
        owner: g.owner.clone(),
        bounds: shape.bounds,
        shapes: vec![shape],
        item: Item {
            what: format!("{what} on {layer}"),
            at: drawn_at(&g.shape),
            part: (!g.owner.is_empty()).then(|| g.owner.clone()),
            pad: None,
            net: None,
        },
    }
}

fn text_ink(t: &Text, layer: &'static str) -> Ink {
    let (strokes, _) = font::strokes(t);
    let shapes: Vec<Shape> = strokes
        .into_iter()
        .map(|pts| {
            if pts.iter().all(|p| p.dist(pts[0]) < 1e-9) {
                Shape::disc(pts[0], t.thickness / 2.0)
            } else {
                Shape::stroke(pts, t.thickness / 2.0)
            }
        })
        .collect();
    let bounds = Model::bounds_of(&shapes);
    Ink {
        text: true,
        layer,
        owner: if t.owner == "the board" {
            String::new()
        } else {
            t.owner.clone()
        },
        bounds,
        shapes,
        item: Item {
            what: format!("Text '{}' on {layer}", t.text.replace('\n', " ")),
            at: t.at,
            part: (t.owner != "the board").then(|| t.owner.clone()),
            pad: None,
            net: None,
        },
    }
}

fn ink_gap(a: &Ink, b: &Ink) -> Option<super::shape::Gap> {
    let mut best: Option<super::shape::Gap> = None;
    for sa in &a.shapes {
        for sb in &b.shapes {
            let g = gap(sa, sb);
            if best.is_none_or(|x| g.d < x.d) {
                best = Some(g);
                if g.d == 0.0 {
                    return best;
                }
            }
        }
    }
    best
}

fn silk(out: &mut Out<'_>) {
    let m = out.m;
    let r = m.rules;
    // Text sizes.
    for t in &m.art.texts {
        let Some(layer) = silk_layer(&t.layer) else {
            continue;
        };
        let item = text_ink(t, layer).item;
        if t.size.1 < r.text_height - EPS / 100.0 {
            out.push(
                Rule::TextHeight,
                format!(
                    "text '{}' is {:.4} mm high; the least is {:.4} mm",
                    t.text.replace('\n', " "),
                    t.size.1,
                    r.text_height
                ),
                t.at,
                Some(layer),
                vec![item.clone()],
            );
        }
        if t.thickness < r.text_thickness - EPS / 100.0 {
            out.push(
                Rule::TextThickness,
                format!(
                    "text '{}' is drawn {:.4} mm thick; the least is {:.4} mm",
                    t.text.replace('\n', " "),
                    t.thickness,
                    r.text_thickness
                ),
                t.at,
                Some(layer),
                vec![item],
            );
        }
    }

    let inks = inks(m);
    if inks.is_empty() {
        return;
    }
    // Silkscreen over a pad's opening in the mask, where the fabricator clips it.
    let openings: Vec<(bool, Vec<Shape>, Bounds, usize)> = m
        .art
        .pads
        .iter()
        .enumerate()
        .flat_map(|(i, pad)| {
            [true, false].into_iter().filter_map(move |front| {
                let shapes = m.mask_opening(pad, front);
                (!shapes.is_empty()).then(|| {
                    let b = Model::bounds_of(&shapes);
                    (front, shapes, b, i)
                })
            })
        })
        .collect();
    let ink_boxes: Vec<Bounds> = inks.iter().map(|i| i.bounds).collect();
    let mut all = ink_boxes.clone();
    all.extend(openings.iter().map(|o| o.2));
    let n = inks.len();
    let mut clipped = vec![false; n];
    for (i, j) in near_pairs(&all, 0.0) {
        if i >= n || j < n {
            continue;
        }
        let (ink, (front, shapes, _, pad)) = (&inks[i], &openings[j - n]);
        if (ink.layer == "F.SilkS") != *front || clipped[i] {
            continue;
        }
        let touch = ink
            .shapes
            .iter()
            .any(|s| shapes.iter().any(|o| gap(s, o).d == 0.0));
        if touch {
            clipped[i] = true;
            let p = &m.art.pads[*pad];
            let pad_item = Item {
                what: format!("Pad {} of {}", p.number, p.part),
                at: p.at,
                part: Some(p.part.clone()),
                pad: Some(p.number.clone()),
                net: p.net.clone(),
            };
            out.push(
                Rule::SilkOverCopper,
                format!(
                    "{} lies over the opening in the solder mask of pad {} of {}, where the fabricator clips it",
                    ink.item.what, p.number, p.part
                ),
                ink.item.at,
                Some(ink.layer),
                vec![ink.item.clone(), pad_item],
            );
        }
    }

    // Silkscreen too near other silkscreen, and near the board edge.
    if r.silk_clearance > 0.0 {
        for (i, j) in near_pairs(&ink_boxes, r.silk_clearance) {
            let (a, b) = (&inks[i], &inks[j]);
            // KiCad compares text with anything, and two drawings unless they are both the
            // board's or both one part's.
            let drawings = !a.text && !b.text;
            if a.layer != b.layer || (drawings && (a.owner == b.owner)) {
                continue;
            }
            if let Some(g) = ink_gap(a, b)
                && g.d < r.silk_clearance - EPS
            {
                out.push(
                    Rule::SilkOverlap,
                    format!(
                        "silkscreen is {:.4} mm between {} and {}; the least is {:.4} mm",
                        g.d, a.item.what, b.item.what, r.silk_clearance
                    ),
                    g.middle(),
                    Some(a.layer),
                    vec![a.item.clone(), b.item.clone()],
                );
            }
        }
        let n = inks.len();
        let mut all = ink_boxes.clone();
        all.extend(m.edge.iter().map(|(s, _)| s.bounds));
        let mut seen = vec![false; n];
        for (i, j) in near_pairs(&all, r.silk_clearance) {
            if i >= n || j < n || seen[i] {
                continue;
            }
            let (ink, (edge_shape, edge_item)) = (&inks[i], &m.edge[j - n]);
            let best = ink.shapes.iter().map(|s| gap(s, edge_shape)).fold(
                None::<super::shape::Gap>,
                |b, g| match b {
                    Some(b) if b.d <= g.d => Some(b),
                    _ => Some(g),
                },
            );
            if let Some(g) = best
                && g.d < r.silk_clearance - EPS
            {
                seen[i] = true;
                out.push(
                    Rule::SilkEdgeClearance,
                    format!(
                        "{} is {:.4} mm from the board edge; the least is {:.4} mm",
                        ink.item.what, g.d, r.silk_clearance
                    ),
                    g.middle(),
                    Some(ink.layer),
                    vec![edge_item.clone(), ink.item.clone()],
                );
            }
        }
    }
}

/// Pads of different nets whose mask openings run together or leave less web than the
/// process can make.
fn mask_bridges(out: &mut Out<'_>) {
    let m = out.m;
    let web = m.rules.mask_web;
    for front in [true, false] {
        let layer = if front { "F.Mask" } else { "B.Mask" };
        let openings: Vec<(usize, Vec<Shape>, Bounds)> = m
            .art
            .pads
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let shapes = m.mask_opening(p, front);
                (!shapes.is_empty()).then(|| {
                    let b = Model::bounds_of(&shapes);
                    (i, shapes, b)
                })
            })
            .collect();
        let boxes: Vec<Bounds> = openings.iter().map(|o| o.2).collect();
        for (i, j) in near_pairs(&boxes, web) {
            let (a, b) = (&m.art.pads[openings[i].0], &m.art.pads[openings[j].0]);
            if a.net == b.net || (m.bridges_in_footprints && a.part == b.part) {
                continue;
            }
            let mut best: Option<super::shape::Gap> = None;
            for sa in &openings[i].1 {
                for sb in &openings[j].1 {
                    let g = gap(sa, sb);
                    if best.is_none_or(|x| g.d < x.d) {
                        best = Some(g);
                    }
                }
            }
            let Some(g) = best else { continue };
            // Without a web to keep, openings are a problem only where they run together.
            let bridged = if web > 0.0 {
                g.d < web - EPS / 100.0
            } else {
                g.depth > 1e-6
            };
            if bridged {
                let item = |p: &crate::artwork::Pad| Item {
                    what: format!(
                        "Pad {}{} of {}",
                        p.number,
                        p.net
                            .as_ref()
                            .map(|n| format!(" [{n}]"))
                            .unwrap_or_default(),
                        p.part
                    ),
                    at: p.at,
                    part: Some(p.part.clone()),
                    pad: Some(p.number.clone()),
                    net: p.net.clone(),
                };
                out.push(
                    Rule::SolderMaskBridge,
                    format!(
                        "the solder mask openings of pad {} of {} and pad {} of {} (nets {} and {}) are {:.4} mm apart; the process leaves a web of at least {:.4} mm",
                        a.number,
                        a.part,
                        b.number,
                        b.part,
                        a.net.as_deref().unwrap_or(""),
                        b.net.as_deref().unwrap_or(""),
                        g.d,
                        web
                    ),
                    g.middle(),
                    Some(layer),
                    vec![item(a), item(b)],
                );
            }
        }
    }
}

/// Tracks, vias, pads and parts inside a rule area that forbids them.
fn rule_areas(out: &mut Out<'_>) {
    let m = out.m;
    // A rule area a library footprint carries (an antenna module's) is not enforced, as KiCad
    // does not: only the board's own are.
    for z in m
        .art
        .zones
        .iter()
        .filter(|z| z.keepout.is_some() && z.owner.is_empty())
    {
        let Some(k) = &z.keepout else { continue };
        let on: Vec<usize> = z
            .layers
            .iter()
            .filter_map(|l| m.layers.iter().position(|x| x == l))
            .collect();
        let name = if z.name.is_empty() {
            "(unnamed)".to_string()
        } else {
            z.name.clone()
        };
        let area_bounds = Bounds::of(z.outlines.iter().flatten().copied());
        let Some(area_bounds) = area_bounds else {
            continue;
        };
        for c in &m.items {
            let forbidden = match c.kind {
                Kind::Track(_) => k.tracks,
                Kind::Via(_) => k.vias,
                Kind::Pad(_) => k.pads,
            };
            if !forbidden || !c.layers.iter().any(|l| on.contains(l)) {
                continue;
            }
            if !(c.bounds.min.x <= area_bounds.max.x
                && area_bounds.min.x <= c.bounds.max.x
                && c.bounds.min.y <= area_bounds.max.y
                && area_bounds.min.y <= c.bounds.max.y)
            {
                continue;
            }
            let hit = z
                .outlines
                .iter()
                .find(|poly| poly.len() >= 3 && c.shapes.iter().any(|s| touches_area(s, poly)));
            if hit.is_some() {
                let layer = c
                    .layers
                    .iter()
                    .find(|l| on.contains(l))
                    .map(|l| m.layer_name(*l).to_string());
                out.push(
                    Rule::ItemsNotAllowed,
                    format!(
                        "{} is inside the rule area '{name}', which does not allow it",
                        c.item.what
                    ),
                    c.item.at,
                    layer.as_deref(),
                    vec![c.item.clone()],
                );
            }
        }
        if k.footprints {
            for fp in &m.art.footprints {
                let side = if fp.back { "B.CrtYd" } else { "F.CrtYd" };
                let side_cu = if fp.back { "B.Cu" } else { "F.Cu" };
                if !z.layers.iter().any(|l| l == side_cu) {
                    continue;
                }
                let pts: Vec<Pt> = m
                    .art
                    .graphics
                    .iter()
                    .filter(|g| g.owner == fp.part && g.layer == side)
                    .flat_map(|g| match &g.shape {
                        crate::artwork::Shape::Line(a, b) => vec![*a, *b],
                        crate::artwork::Shape::Poly(p) => p.clone(),
                        crate::artwork::Shape::Arc(a) => a.points(),
                        crate::artwork::Shape::Circle { centre, radius } => {
                            geom::circle_polygon(*centre, *radius)
                        }
                    })
                    .collect();
                let Some(yard) = Bounds::of(pts) else {
                    continue;
                };
                let corners = vec![
                    yard.min,
                    Pt::new(yard.max.x, yard.min.y),
                    yard.max,
                    Pt::new(yard.min.x, yard.max.y),
                ];
                let hit = z
                    .outlines
                    .iter()
                    .any(|poly| poly.len() >= 3 && overlap_area(&corners, poly));
                if hit {
                    out.push(
                        Rule::ItemsNotAllowed,
                        format!(
                            "{} is inside the rule area '{name}', which does not allow parts",
                            fp.part
                        ),
                        fp.at,
                        Some(side_cu),
                        vec![Item {
                            what: format!("Footprint {}", fp.part),
                            at: fp.at,
                            part: Some(fp.part.clone()),
                            pad: None,
                            net: None,
                        }],
                    );
                }
            }
        }
    }
}

/// A via sunk into a pad, which the process fills only up to a diameter.
fn via_in_pad(out: &mut Out<'_>) {
    let m = out.m;
    let Some(widest) = m.extras.via_in_pad else {
        return;
    };
    for (vi, v) in m.art.vias.iter().enumerate() {
        if v.diameter <= widest + EPS / 100.0 {
            continue;
        }
        let via = &m.items[m.via_item[vi]];
        for c in m.items.iter().filter(|c| matches!(c.kind, Kind::Pad(_))) {
            let centre = Shape::disc(v.at, 0.0);
            if c.shapes.iter().any(|s| gap(s, &centre).d == 0.0) {
                out.push(
                    Rule::ViaInPad,
                    format!(
                        "the {:.4} mm via is inside {}; the process fills vias up to {:.4} mm",
                        v.diameter, c.item.what, widest
                    ),
                    v.at,
                    None,
                    vec![via.item.clone(), c.item.clone()],
                );
            }
        }
    }
}
