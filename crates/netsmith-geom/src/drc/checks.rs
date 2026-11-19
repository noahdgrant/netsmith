//! The rules that look at one board's geometry: clearances, holes, edge, courtyards,
//! silkscreen, solder mask, rule areas and the outline. Connectivity is in `connect.rs`.

use super::model::{Copper, Kind, Model, open_ring, overlap_area};
use super::shape::{Shape, gap, near_pairs, touches_area};
use super::{Finding, Item, Rule};
use crate::artwork::PadKind;
use crate::edge;
use crate::geom::{self, Bounds, Pt};
use crate::polygons::{self, Area};

/// What a measurement may fall short of its limit by and still pass: KiCad's own epsilon
/// (half a micrometre), so that a track routed exactly at the limit passes in both checks.
pub(crate) const EPS: f64 = 0.0005;

pub(crate) struct Out<'a> {
    pub m: &'a Model<'a>,
    pub findings: &'a mut Vec<Finding>,
}

impl Out<'_> {
    pub(super) fn push(
        &mut self,
        rule: Rule,
        message: String,
        at: Pt,
        layer: Option<&str>,
        items: Vec<Item>,
    ) {
        self.push_carrying(rule, message, at, layer, items, None);
    }

    /// A finding that has the numbers of copper held to a current (issue 1373).
    pub(super) fn push_carrying(
        &mut self,
        rule: Rule,
        message: String,
        at: Pt,
        layer: Option<&str>,
        items: Vec<Item>,
        carrying: Option<super::Carrying>,
    ) {
        if let Some(severity) = self.m.rules.severity(rule) {
            self.findings.push(Finding {
                rule,
                severity,
                message,
                at,
                layer: layer.map(str::to_string),
                items,
                carrying,
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
    super::silk::run(&mut out);
    mask_bridges(&mut out);
    rule_areas(&mut out);
    via_in_pad(&mut out);
    super::fab_rules::run(&mut out);
    super::marks::run(&mut out);
    super::pour::run(&mut out);
    super::acid::run(&mut out);
    super::stackup::run(&mut out);
    super::vias::run(&mut out);
}

/// Whether two holes share a layer: a hole through the board meets every hole, a blind or a
/// buried via's (its first and last layer) those whose layers overlap its own (issue #1015).
pub(crate) fn holes_meet(a: Option<(usize, usize)>, b: Option<(usize, usize)>) -> bool {
    match (a, b) {
        (Some((a0, a1)), Some((b0, b1))) => a0 <= b1 && b0 <= a1,
        _ => true,
    }
}

pub(crate) fn same_net(a: &Option<String>, b: &Option<String>) -> bool {
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
    pub(crate) fn max_gap(&self) -> f64 {
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
            let first = m.edge.iter().find(|(_, i)| i.what.ends_with("Edge.Cuts"));
            let at = first.map(|(_, i)| i.at).unwrap_or_default();
            let items = first.map(|(_, i)| vec![i.clone()]).unwrap_or_default();
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
    for (index, c) in m.items.iter().enumerate() {
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
                let least = match r.microvia_diameter {
                    Some(micro) if v.micro => micro,
                    _ => r.via_diameter,
                };
                if v.diameter < least - EPS / 100.0 {
                    out.push(
                        Rule::ViaDiameter,
                        format!(
                            "via is {:.4} mm across, the least is {:.4} mm",
                            v.diameter, least
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
            Kind::Teardrop(_) | Kind::Graphic(_) => {}
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
                let ring = ring_width(&pad_copper(m, index), &hole.shape);
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
        // A micro via's hole is the laser's, held to the project's least micro via drill.
        let least = match (h.owner.map(|o| m.items[o].kind), r.microvia_drill) {
            (Some(Kind::Via(v)), Some(micro)) if m.art.vias[v].micro => micro,
            _ => r.through_hole,
        };
        if h.diameter < least - EPS / 100.0 {
            out.push(
                Rule::DrillOutOfRange,
                format!(
                    "hole is {:.4} mm, the least the process drills is {:.4} mm",
                    h.diameter, least
                ),
                h.at,
                None,
                vec![h.item.clone()],
            );
        }
    }
}

/// The copper a plated pad's ring is measured in: its own, and that of the other pads of its
/// part with its number whose copper touches it, on any layer (a pad made of several, such as
/// an exposed pad with vias in it). So KiCad measures it (KiCad 9.0.0: a plated pad inside an
/// SMD pad of its number has no `annular_width` finding, issue #1185).
pub(crate) fn pad_copper(m: &Model<'_>, item: usize) -> Vec<Shape> {
    let c = &m.items[item];
    let Kind::Pad(i) = c.kind else {
        return c.shapes.clone();
    };
    let pad = &m.art.pads[i];
    let mut shapes = c.shapes.clone();
    for o in &m.items {
        let Kind::Pad(j) = o.kind else { continue };
        let other = &m.art.pads[j];
        if j == i || other.part != pad.part || other.number != pad.number {
            continue;
        }
        let touches = o
            .shapes
            .iter()
            .any(|so| c.shapes.iter().any(|sc| gap(sc, so).d == 0.0));
        if touches {
            shapes.extend(o.shapes.iter().cloned());
        }
    }
    shapes
}

/// The copper left between a hole and the edge of the pad round it, the least all round.
pub(crate) fn ring_width(copper: &[Shape], hole: &Shape) -> f64 {
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
        // Two holes of blind or buried vias that share no layer do not meet (issue #1015).
        if !holes_meet(a.span, b.span) {
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
    // Hole to copper of another net, on any layer the hole goes through: every layer, or a blind
    // or buried via's own (issue #1015).
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
        if let Some((a, b)) = h.span
            && !c.layers.iter().any(|&l| a <= l && l <= b)
        {
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
        // A part that stands over the other's protruding pins is excused (the caller judged).
        if m.extras
            .excused_courtyards
            .iter()
            .any(|(x, y)| (x == &a.owner && y == &b.owner) || (x == &b.owner && y == &a.owner))
        {
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
            // Two copper pads with no net are not of one net: solder that bridges them joins two
            // pins the design keeps apart, and KiCad reports them (issue #1185). The opening of a
            // hole with no plating bares no copper, and two of them are no bridge, as in KiCad.
            let copper = |p: &crate::artwork::Pad| p.kind != crate::artwork::PadKind::Npth;
            let one_net = match (&a.net, &b.net) {
                (Some(x), Some(y)) => x == y,
                (None, None) => !(copper(a) && copper(b)),
                _ => false,
            };
            if one_net || (m.bridges_in_footprints && a.part == b.part) {
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

/// Tracks, vias, pads and parts inside a rule area that forbids them: the board's own and those a
/// footprint carries (a library's keepout over or round its pads, issue #1291), as KiCad's check
/// flags both. A footprint's rule area forbids its own pads too, but not the footprint itself
/// (measured with KiCad 9.0.0, and so in 9.0.8's source). An item counts when it reaches into the
/// area by more than [`EPS`]: KiCad shrinks the area by its DRC epsilon first, so a pad, a track
/// or a via that only touches the area's edge is not flagged (measured, issue #1372; a library's
/// keepout drawn up to its own pads is the usual case). A pour's fill is not judged against a
/// rule area, as KiCad 9's check does not judge it (measured, `drc_corpus`): the fill keeps out of
/// one that forbids pours ([`crate::fill`]), as KiCad's refill does. Each finding names the area
/// and what of it flags the item (`(no tracks)`, `(no vias)`, `(no pads)`, `(no footprints)`).
fn rule_areas(out: &mut Out<'_>) {
    let m = out.m;
    for z in m.art.zones.iter().filter(|z| z.keepout.is_some()) {
        let Some(k) = &z.keepout else { continue };
        let on: Vec<usize> = z
            .layers
            .iter()
            .filter_map(|l| m.layers.iter().position(|x| x == l))
            .collect();
        let owner = (!z.owner.is_empty()).then_some(z.owner.as_str());
        let name = match (owner, z.name.is_empty()) {
            (Some(o), true) => format!("of {o}'s footprint"),
            (Some(o), false) => format!("'{}' of {o}'s footprint", z.name),
            (None, true) => "'(unnamed)'".to_string(),
            (None, false) => format!("'{}'", z.name),
        };
        let area_bounds = Bounds::of(z.outlines.iter().flatten().copied());
        let Some(area_bounds) = area_bounds else {
            continue;
        };
        let outlines: Vec<Area> = z
            .outlines
            .iter()
            .filter(|poly| poly.len() >= 3)
            .map(|poly| Area::of(poly))
            .collect();
        let inner = polygons::shrunk(&outlines, EPS);
        for c in &m.items {
            let (forbidden, kept) = match c.kind {
                Kind::Track(_) | Kind::Teardrop(_) => (k.tracks, "tracks"),
                Kind::Via(_) => (k.vias, "vias"),
                Kind::Pad(_) => (k.pads, "pads"),
                Kind::Graphic(_) => (false, ""),
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
            let hit = inner
                .iter()
                .any(|a| a.outer.len() >= 3 && c.shapes.iter().any(|s| touches_area(s, &a.outer)));
            if hit {
                let layer = c
                    .layers
                    .iter()
                    .find(|l| on.contains(l))
                    .map(|l| m.layer_name(*l).to_string());
                out.push(
                    Rule::ItemsNotAllowed,
                    format!("{} is inside the rule area {name} (no {kept})", c.item.what),
                    c.item.at,
                    layer.as_deref(),
                    vec![c.item.clone()],
                );
            }
        }
        if k.footprints {
            for fp in m
                .art
                .footprints
                .iter()
                .filter(|fp| Some(fp.part.as_str()) != owner)
            {
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
                let hit = inner
                    .iter()
                    .any(|a| a.outer.len() >= 3 && overlap_area(&corners, &a.outer));
                if hit {
                    out.push(
                        Rule::ItemsNotAllowed,
                        format!("{} is inside the rule area {name} (no footprints)", fp.part),
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
        // An HDI process fills and planarizes its laser-drilled blind vias by plating, so one
        // stands in a pad whatever the process fills of its drilled vias (issue #1071).
        if v.micro && m.extras.vias.is_some() {
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
