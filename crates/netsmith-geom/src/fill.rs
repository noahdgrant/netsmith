//! Filling zones: the copper pours of a board (the planes of an inner layer and the pours of
//! an outer one), as KiCad's zone fill makes them, written into a Gerber layer.
//!
//! Each zone is filled in four steps, as KiCad fills it:
//!
//! 1. **The clearances.** The zone's outline less its holes, less everything the pour keeps away
//!    from: the outside of the board and a band along its edge, rule areas that forbid pours,
//!    the clearance round the copper of other nets (pads, vias, tracks, teardrops, drawings) and
//!    round holes, and the thermal gaps round the pads of its own net, drawn as the four pieces
//!    between the spokes so that the spokes stay copper. Where zones of one layer overlap, a
//!    zone also gives up what a zone of higher priority has ([`fill_zones`]).
//! 2. **The minimum thickness.** The copper narrower than the zone's `min_thickness` is taken
//!    out ([`crate::polygons::trimmed`]: shrunk by half the width and grown back), so a sliver
//!    goes and a neck narrower than it parts the copper in two.
//! 3. **Spokes that reach nothing.** A spoke whose outer end is not on the fill (it runs into
//!    another net's clearance, a keepout or the edge band) is taken out, as KiCad adds a spoke
//!    only where its end meets the fill. A spoke the fill reaches stays, even if a clearance
//!    cuts across it further in.
//! 4. **Copper islands.** A piece of the fill that touches no copper of the zone's net on the layer (a
//!    pad, a via, a track, a teardrop, or a piece of another zone of the net) is removed, as
//!    the zone's `island_removal_mode` says ([`crate::artwork::Islands`]: always, the default,
//!    never, or below `island_area_min`). A zone of no net keeps every piece, as KiCad's does.
//!
//! Each piece is then written as one dark region with its holes cleared; the caller draws every
//! pad, via and track dark again after it.
//!
//! The polygons are worked out with [`crate::polygons::Polygons`], and the fill is written to
//! any [`Pen`]: the Gerber file, or `Polygons` again. `netsmith_pcb::copper_of` fills a board's
//! pours that way for the 3D view (ADR 0105), so its pours are this fill and there is one filler.
//!
//! The gap and spoke width come from the zone; the spokes' turn is the pad's own
//! `thermal_bridge_angle`, else KiCad's default: 45 degrees from the pad's axes for a round pad
//! or a custom pad with a round anchor, along the axes for every other shape (KiCad's board
//! reader, `parsePAD`, the same in 9.0.0 and 9.0.5; a file older than KiCad 6, version
//! 20211014 or earlier, gave a custom pad with a round anchor 90 degrees, and netsmith does not
//! read the file's version and gives it 45). netsmith once drew every spoke along the axes, so
//! its planes differed from the ones KiCad fills round every round pad (0.64% of the pixels of a
//! small plane, 1.2% with more pads). Neither angle is better for the factory (JLCPCB's page
//! lists no rule about spokes), and matching KiCad costs nothing, so the file is the same as the
//! one the designer sees (`crates/netsmith-fab/tests/known_differences.rs`). A via of the
//! pour's net is joined solid, as KiCad joins it. Fills the file already holds
//! (`filled_polygon`) are used as they are.
//!
//! **Where it differs from KiCad.** Copper islands are removed once every zone of the layer is filled,
//! and a zone of lower priority keeps away from a higher one's fill as it was before its islands
//! went, so it does not grow into the room an island leaves. The spokes' ends are tried against
//! the fill after the minimum thickness is taken out, so a spoke into a sliver goes with the
//! sliver and leaves no stub.

use crate::artwork::{Artwork, Connect, Islands, Pad, PadKind, Shape, TrackShape, Zone};
use crate::edge::Outline as BoardEdge;
use crate::gbr::{Contour, Pen};
use crate::geom::{self, Arc, Bounds, Pt};
use crate::pads::{self, Outline};
use crate::plot::Rules;
use crate::polygons::{Area, Polygons, difference, grown, overlap, trimmed};

/// KiCad's clearance when a zone gives none.
const DEFAULT_CLEARANCE: f64 = 0.2;

/// Whether a via joins a pour with thermal spokes (as a pad does) rather than solid copper:
/// KiCad joins it solid (found by the cross-check, ADR 0034).
const VIA_THERMAL: bool = false;

/// How far beyond a thermal gap a spoke's end is tried against the fill, in mm.
const SPOKE_PROBE: f64 = 0.01;

/// How far a piece of another zone of the net is grown to count as touching an island, in mm:
/// two zones of one net that meet share an edge and no area.
const TOUCH: f64 = 0.001;

/// Writes the fill of every zone of `layer` that holds no fill of its own in the file.
pub fn pour(
    g: &mut dyn Pen,
    art: &Artwork,
    layer: &str,
    edge: &BoardEdge,
    rules: &Rules,
) -> Result<(), String> {
    let zones: Vec<&Zone> = art
        .zones
        .iter()
        .filter(|z| {
            z.keepout.is_none()
                && z.layers.iter().any(|l| l == layer)
                && !z.filled.iter().any(|(l, _)| l == layer)
        })
        .collect();
    if zones.is_empty() {
        return Ok(());
    }
    let filled = fill_zones(art, layer, edge, rules, &zones)?;
    // Each piece of copper as one dark region with its holes cleared, the pieces with the
    // largest box first, so that clearing a hole never takes copper drawn before it (a piece
    // inside a hole is smaller than the piece round it).
    let mut pieces: Vec<(f64, &Zone, &Area)> = Vec::new();
    for (i, areas) in &filled {
        for a in areas {
            let size = Bounds::of(a.outer.iter().copied())
                .map_or(0.0, |b| (b.max.x - b.min.x) * (b.max.y - b.min.y));
            pieces.push((size, zones[*i], a));
        }
    }
    pieces.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, z, a) in pieces {
        g.polarity(true);
        g.region(
            Contour::polygon(&a.outer),
            Some("Conductor"),
            &net_attr(z.net.as_deref()),
        );
        if !a.holes.is_empty() {
            g.polarity(false);
            for h in &a.holes {
                g.region(Contour::polygon(h), None, &[]);
            }
        }
    }
    g.polarity(true);
    Ok(())
}

/// Clears the rule areas of `layer` that forbid pours.
fn keep_out_of_rule_areas(g: &mut dyn Pen, art: &Artwork, layer: &str) {
    for k in art.zones.iter().filter(|z| {
        z.keepout.as_ref().is_some_and(|k| k.copperpour) && z.layers.iter().any(|l| l == layer)
    }) {
        for o in &k.outlines {
            if o.len() >= 3 {
                g.region(Contour::polygon(o), None, &[]);
            }
        }
    }
}

/// A zone's clearance, KiCad's default when it gives none.
fn clearance_of(z: &Zone) -> f64 {
    if z.clearance > 0.0 {
        z.clearance
    } else {
        DEFAULT_CLEARANCE
    }
}

/// The clearance a zone keeps from copper of other nets: its own, or its net's class's if
/// that is larger (issue #758). Copper of a net whose class keeps more keeps that.
fn zone_clearance(z: &Zone, rules: &Rules) -> f64 {
    clearance_of(z).max(rules.class_clearance(z.net.as_deref()))
}

/// A zone's outline less its holes.
fn outline_area(z: &Zone) -> Vec<Area> {
    let mut p = Polygons::new();
    for o in z.outlines.iter().filter(|o| o.len() >= 3) {
        p.region(Contour::polygon(o), None, &[]);
    }
    p.polarity(false);
    for h in z.holes.iter().filter(|h| h.len() >= 3) {
        p.region(Contour::polygon(h), None, &[]);
    }
    p.areas()
}

/// The fill of each zone of a layer (by its index in `zones`), in the four steps of the module
/// docs. Where zones overlap, KiCad's rule holds: from the highest priority down, a zone gives
/// up the outline of every zone of its net with a higher priority (that zone's settings hold
/// there), and the fill of every zone of another net with a higher priority, grown by the larger
/// of the two zones' clearances. Of two zones of one net with the same priority, the one first
/// in the file is taken as the higher; two zones of different nets with the same priority whose
/// outlines overlap are refused, since which one has the area would be KiCad's choice by an
/// identifier (the zone's uuid), not the design's.
fn fill_zones(
    art: &Artwork,
    layer: &str,
    edge: &BoardEdge,
    rules: &Rules,
    zones: &[&Zone],
) -> Result<Vec<(usize, Vec<Area>)>, String> {
    let outlines: Vec<Vec<Area>> = zones.iter().map(|z| outline_area(z)).collect();
    for (i, a) in zones.iter().enumerate() {
        for (j, b) in zones.iter().enumerate().skip(i + 1) {
            if a.net != b.net && a.priority == b.priority && overlap(&outlines[i], &outlines[j]) {
                return Err(format!(
                    "two zones on {layer} overlap, are on different nets ({} and {}) and have the same priority ({}); give the one that is to have the area the higher priority",
                    a.net.as_deref().unwrap_or("no net"),
                    b.net.as_deref().unwrap_or("no net"),
                    a.priority
                ));
            }
        }
    }
    // The highest priority first; of equal ones, the one first in the file.
    let mut order: Vec<usize> = (0..zones.len()).collect();
    order.sort_by(|&a, &b| zones[b].priority.cmp(&zones[a].priority).then(a.cmp(&b)));
    let mut filled: Vec<(usize, Vec<Area>)> = Vec::new();
    for &i in &order {
        let z = zones[i];
        let mut p = Polygons::new();
        for o in z.outlines.iter().filter(|o| o.len() >= 3) {
            p.region(Contour::polygon(o), None, &[]);
        }
        p.polarity(false);
        for h in z.holes.iter().filter(|h| h.len() >= 3) {
            p.region(Contour::polygon(h), None, &[]);
        }
        keep_out_of_the_edge(&mut p, art, edge, rules.edge_clearance, &[z]);
        keep_out_of_rule_areas(&mut p, art, layer);
        around_copper(&mut p, art, layer, z, rules)?;
        let mut fill = p.areas();
        let mut taken: Vec<Area> = Vec::new();
        for (j, higher) in &filled {
            let h = zones[*j];
            if h.net == z.net {
                taken.extend(outlines[*j].iter().cloned());
            } else {
                taken.extend(grown(
                    higher,
                    zone_clearance(z, rules).max(zone_clearance(h, rules)),
                ));
            }
        }
        if !taken.is_empty() {
            fill = difference(&fill, &taken);
        }
        let fill = trimmed(&fill, z.min_thickness);
        let fill = without_dead_spokes(fill, art, layer, z);
        filled.push((i, fill));
    }
    let kept: Vec<Vec<Area>> = filled
        .iter()
        .map(|(i, fill)| {
            let z = zones[*i];
            let others: Vec<Area> = filled
                .iter()
                .filter(|(j, _)| j != i && zones[*j].net == z.net)
                .flat_map(|(_, f)| f.iter().cloned())
                .collect();
            without_islands(fill, art, layer, z, &others)
        })
        .collect();
    Ok(filled
        .into_iter()
        .zip(kept)
        .map(|((i, _), fill)| (i, fill))
        .collect())
}

/// How a pad of the zone's net joins the zone: the pad's own setting, else the zone's, with
/// `thru_hole_only` made thermal or solid by the pad's kind.
fn connection(pad: &Pad, z: &Zone) -> Connect {
    match pad.zone_connect.unwrap_or(z.connect) {
        Connect::ThruHoleOnly if pad.kind == PadKind::Smd => Connect::Solid,
        Connect::ThruHoleOnly => Connect::Thermal,
        other => other,
    }
}

/// Whether a pad is of the zone's net (a zone of no net has no pads of its own).
fn own_net(z: &Zone, net: &Option<String>) -> bool {
    z.net.is_some() && *net == z.net
}

/// The fill less each thermal spoke of the zone whose outer end is not on the fill (step 3).
fn without_dead_spokes(fill: Vec<Area>, art: &Artwork, layer: &str, z: &Zone) -> Vec<Area> {
    if fill.is_empty() || z.net.is_none() {
        return fill;
    }
    let Some(area) = zone_bounds(z).map(|b| b.expanded(z.thermal_gap.max(0.0))) else {
        return fill;
    };
    let on_fill = |p: Pt| fill.iter().any(|a| a.contains(p));
    let mut dead: Vec<Area> = Vec::new();
    let mut pads_of: Vec<Area> = Vec::new();
    for pad in art.pads.iter().filter(|p| {
        own_net(z, &p.net) && art.pad_copper_on(p, layer) && connection(p, z) == Connect::Thermal
    }) {
        let shape = pads::outline(pad);
        if !shape.bounds().overlaps(&area) {
            continue;
        }
        let mut any = false;
        for (spoke, end) in spokes(&shape, pad.at, pad.angle + pad.spoke_angle(), z) {
            if !on_fill(end) {
                dead.push(Area::of(&spoke));
                any = true;
            }
        }
        if any && let Some(p) = shape.polygon() {
            pads_of.push(Area::of(&p));
        }
    }
    if dead.is_empty() {
        return fill;
    }
    // The spokes less the pads they leave, which stay copper.
    let cut = difference(&dead, &pads_of);
    difference(&fill, &cut)
}

/// The fill less its copper islands (step 4): the pieces that touch no copper of the zone's net on
/// `layer` and none of `others` (the pieces of the other zones of the net on the layer), as the
/// zone's island setting says.
fn without_islands(
    fill: &[Area],
    art: &Artwork,
    layer: &str,
    z: &Zone,
    others: &[Area],
) -> Vec<Area> {
    if z.islands == Islands::Keep {
        return fill.to_vec();
    }
    let isolated = isolated(fill, art, layer, z, others);
    fill.iter()
        .zip(isolated)
        .filter(|(piece, isolated)| {
            !isolated || matches!(z.islands, Islands::RemoveBelow(min) if piece.size() >= min)
        })
        .map(|(piece, _)| piece.clone())
        .collect()
}

/// For each piece of `fill`, whether it is a copper island: it touches no copper of the zone's net
/// on `layer` and none of `others`. A zone of no net has no islands, as KiCad's has none.
fn isolated(fill: &[Area], art: &Artwork, layer: &str, z: &Zone, others: &[Area]) -> Vec<bool> {
    let Some(net) = z.net.as_deref() else {
        return vec![false; fill.len()];
    };
    if fill.is_empty() {
        return Vec::new();
    }
    let mut anchors = net_copper(art, layer, net);
    if !others.is_empty() {
        anchors.extend(grown(others, TOUCH));
    }
    let boxes: Vec<Option<Bounds>> = anchors
        .iter()
        .map(|a| Bounds::of(a.outer.iter().copied()))
        .collect();
    fill.iter()
        .map(|piece| {
            let Some(b) = Bounds::of(piece.outer.iter().copied()) else {
                return true;
            };
            let near: Vec<Area> = anchors
                .iter()
                .zip(&boxes)
                .filter(|(_, ab)| ab.is_some_and(|ab| ab.overlaps(&b)))
                .map(|(a, _)| a.clone())
                .collect();
            near.is_empty() || !overlap(std::slice::from_ref(piece), &near)
        })
        .collect()
}

/// What the design rules check reads of the fill of one zone on one layer (issue #838): the
/// thermal reliefs of the pads in it and the copper islands its setting keeps.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Judged {
    /// The zone, by its index in the board's zones.
    pub zone: usize,
    /// Each pad of the zone's net that the zone joins with thermal spokes and whose centre is
    /// in the zone's outline.
    pub reliefs: Vec<Relief>,
    /// Each piece of the fill that touches no copper of the zone's net and that the zone's
    /// `island_removal_mode` keeps: a corner of it, and its area in mm².
    pub islands: Vec<(Pt, f64)>,
}

/// A thermal relief of a pad in a zone, as the fill leaves it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Relief {
    /// The pad, by its index in the board's pads.
    pub pad: usize,
    /// How many spokes the relief has, and how many of them reach the fill.
    pub spokes: usize,
    pub reach: usize,
    /// How many tracks of the zone's net on the layer run from the pad onto the fill, which
    /// join the pad to the pour as a spoke does.
    pub tracks: usize,
}

/// Points along a track, at most 0.05 mm apart, its two ends among them.
fn track_points(shape: &TrackShape) -> Vec<Pt> {
    match shape {
        TrackShape::Line(a, b) => {
            let n = ((a.dist(*b) / 0.05).ceil() as usize).max(1);
            (0..=n)
                .map(|i| *a + (*b - *a) * (i as f64 / n as f64))
                .collect()
        }
        TrackShape::Arc(arc) => arc.points(),
    }
}

/// The fill of every zone of `layer` with a net that holds no fill of its own in the file,
/// worked out as [`pour`] draws it, and what the check reads of it ([`Judged`]).
pub(crate) fn judge(
    art: &Artwork,
    layer: &str,
    edge: &BoardEdge,
    rules: &Rules,
) -> Result<Vec<Judged>, String> {
    let index: Vec<usize> = art
        .zones
        .iter()
        .enumerate()
        .filter(|(_, z)| {
            z.keepout.is_none()
                && z.layers.iter().any(|l| l == layer)
                && !z.filled.iter().any(|(l, _)| l == layer)
        })
        .map(|(i, _)| i)
        .collect();
    if !index.iter().any(|&i| art.zones[i].net.is_some()) {
        return Ok(Vec::new());
    }
    let zones: Vec<&Zone> = index.iter().map(|&i| &art.zones[i]).collect();
    let filled = fill_zones(art, layer, edge, rules, &zones)?;
    let mut out = Vec::new();
    for (i, fill) in &filled {
        let z = zones[*i];
        if z.net.is_none() {
            continue;
        }
        let outline = outline_area(z);
        let mut reliefs = Vec::new();
        for (k, pad) in art.pads.iter().enumerate().filter(|(_, p)| {
            own_net(z, &p.net)
                && art.pad_copper_on(p, layer)
                && connection(p, z) == Connect::Thermal
                && outline.iter().any(|a| a.contains(p.at))
        }) {
            let shape = pads::outline(pad);
            let spokes = spokes(&shape, pad.at, pad.angle + pad.spoke_angle(), z);
            if spokes.is_empty() {
                continue;
            }
            let on_fill = |p: &Pt| fill.iter().any(|a| a.contains(*p));
            let reach = spokes.iter().filter(|(_, end)| on_fill(end)).count();
            let tracks = shape.polygon().map_or(0, |poly| {
                art.tracks
                    .iter()
                    .filter(|t| t.layer == layer && t.net == z.net)
                    .filter(|t| {
                        let points = track_points(&t.shape);
                        let ends = [points.first(), points.last()];
                        ends.iter().flatten().any(|p| geom::contains(&poly, **p))
                            && points.iter().any(on_fill)
                    })
                    .count()
            });
            reliefs.push(Relief {
                pad: k,
                spokes: spokes.len(),
                reach,
                tracks,
            });
        }
        let mut islands = Vec::new();
        if z.islands != Islands::Remove {
            let others: Vec<Area> = filled
                .iter()
                .filter(|(j, _)| j != i && zones[*j].net == z.net)
                .flat_map(|(_, f)| f.iter().cloned())
                .collect();
            for (piece, isolated) in fill.iter().zip(isolated(fill, art, layer, z, &others)) {
                if isolated && let Some(&corner) = piece.outer.first() {
                    islands.push((corner, piece.size()));
                }
            }
        }
        out.push(Judged {
            zone: index[*i],
            reliefs,
            islands,
        });
    }
    Ok(out)
}

/// The copper of `net` on `layer` that joins a piece of its zone: its pads with copper on the
/// layer, its vias, its tracks on the layer and its teardrops there.
fn net_copper(art: &Artwork, layer: &str, net: &str) -> Vec<Area> {
    let is = |n: &Option<String>| n.as_deref() == Some(net);
    let mut p = Polygons::new();
    for pad in art
        .pads
        .iter()
        .filter(|pad| is(&pad.net) && art.pad_copper_on(pad, layer))
    {
        pads::outline(pad).draw(&mut p, None, &[]);
    }
    for via in art.vias.iter().filter(|v| is(&v.net)) {
        Outline::Circle {
            c: via.at,
            r: via.diameter / 2.0,
        }
        .draw(&mut p, None, &[]);
    }
    for t in art.tracks.iter().filter(|t| t.layer == layer && is(&t.net)) {
        match &t.shape {
            TrackShape::Line(a, b) => stroke(&mut p, *a, *b, t.width),
            TrackShape::Arc(arc) => p.arc(t.width, None, arc, &[]),
        }
    }
    for t in art
        .zones
        .iter()
        .filter(|t| t.teardrop && t.keepout.is_none() && is(&t.net))
    {
        for (l, poly) in &t.filled {
            if l == layer && poly.len() >= 3 {
                p.region(Contour::polygon(poly), None, &[]);
            }
        }
    }
    p.areas()
}

fn net_attr(net: Option<&str>) -> Vec<String> {
    vec![format!("N,{}", crate::gbr::escape(net.unwrap_or("N/C")))]
}

fn zone_bounds(z: &Zone) -> Option<Bounds> {
    Bounds::of(z.outlines.iter().flatten().copied())
}

/// Clears the outside of the board, the cutouts, and a band of `2 * clearance` along every
/// edge line.
fn keep_out_of_the_edge(
    g: &mut dyn Pen,
    art: &Artwork,
    edge: &BoardEdge,
    clearance: f64,
    zones: &[&Zone],
) {
    let Some(mut b) = Bounds::of(
        zones
            .iter()
            .filter_map(|z| zone_bounds(z))
            .flat_map(|b| [b.min, b.max]),
    ) else {
        return;
    };
    b = b.expanded(1.0);
    // The outside: a frame with the board's outline cut out of it, joined by a slit.
    let mut outer = edge.outer.clone();
    let frame = [
        Pt::new(b.min.x, b.min.y),
        Pt::new(b.max.x, b.min.y),
        Pt::new(b.max.x, b.max.y),
        Pt::new(b.min.x, b.max.y),
    ];
    // The frame and the outline must run in opposite directions.
    if (geom::area2(&frame) > 0.0) == (geom::area2(&outer) > 0.0) {
        outer.reverse();
    }
    let mut ring: Vec<Pt> = frame.to_vec();
    ring.push(frame[0]);
    ring.push(outer[0]);
    ring.extend(outer.iter().skip(1));
    ring.push(outer[0]);
    ring.push(frame[0]);
    g.region(Contour::polygon(&ring), None, &[]);
    for c in &edge.cutouts {
        g.region(Contour::polygon(c), None, &[]);
    }
    let width = 2.0 * clearance;
    for gr in art.graphics.iter().filter(|gr| gr.layer == "Edge.Cuts") {
        stroke_shape(g, &gr.shape, width);
    }
}

/// A shape's outline stroked with a round pen (in whatever polarity the file is in).
fn stroke_shape(g: &mut dyn Pen, shape: &Shape, width: f64) {
    match shape {
        Shape::Line(a, b) => stroke(g, *a, *b, width),
        Shape::Arc(arc) => g.arc(width, None, arc, &[]),
        Shape::Circle { centre, radius } => {
            let start = Pt::new(centre.x + radius, centre.y);
            g.arc(
                width,
                None,
                &Arc {
                    start,
                    end: start,
                    centre: *centre,
                    radius: *radius,
                    ccw: true,
                },
                &[],
            );
        }
        Shape::Poly(p) => {
            for i in 0..p.len() {
                stroke(g, p[i], p[(i + 1) % p.len()], width);
            }
        }
    }
}

fn stroke(g: &mut dyn Pen, a: Pt, b: Pt, width: f64) {
    if a == b {
        g.flash(crate::gbr::Ap::Circle(width), None, a, &[]);
    } else {
        g.line(width, None, a, b, &[]);
    }
}

/// Clears what one zone keeps away from on `layer`.
fn around_copper(
    g: &mut dyn Pen,
    art: &Artwork,
    layer: &str,
    z: &Zone,
    rules: &Rules,
) -> Result<(), String> {
    let clearance = zone_clearance(z, rules);
    // The clearance from copper of `net`: the zone's, or the class's of `net` if larger.
    let from = |net: &Option<String>| clearance.max(rules.class_clearance(net.as_deref()));
    let reach = clearance.max(rules.widest_class_clearance()) + z.thermal_gap.max(0.0);
    let Some(area) = zone_bounds(z).map(|b| b.expanded(reach)) else {
        return Ok(());
    };
    let own = |net: &Option<String>| own_net(z, net);

    for pad in &art.pads {
        let shape = pads::outline(pad);
        let copper = art.pad_copper_on(pad, layer);
        if !copper {
            // A hole with no copper round it still keeps the clearance.
            if let Some(d) = &pad.drill {
                let by = rules.hole_clearance.unwrap_or(clearance);
                if hole_outline(pad, d).bounds().expanded(by).overlaps(&area) {
                    draw_grown(g, &hole_outline(pad, d), by, &pad.part, &pad.number)?;
                }
            }
            continue;
        }
        if !shape.bounds().expanded(reach).overlaps(&area) {
            continue;
        }
        let mode = connection(pad, z);
        if own(&pad.net) && mode != Connect::None {
            if mode == Connect::Thermal {
                thermal(g, &shape, pad.at, pad.angle + pad.spoke_angle(), z)?;
            }
        } else {
            let by = pad
                .clearance
                .map_or(from(&pad.net), |c| c.max(from(&pad.net)));
            draw_grown(g, &shape, by, &pad.part, &pad.number)?;
        }
    }
    for via in &art.vias {
        let shape = Outline::Circle {
            c: via.at,
            r: via.diameter / 2.0,
        };
        if !shape.bounds().expanded(reach).overlaps(&area) {
            continue;
        }
        if own(&via.net) {
            if VIA_THERMAL && z.connect == Connect::Thermal {
                thermal(g, &shape, via.at, 0.0, z)?;
            }
        } else {
            draw_grown(g, &shape, from(&via.net), "via", "")?;
        }
    }
    for t in art
        .tracks
        .iter()
        .filter(|t| t.layer == layer && !own(&t.net))
    {
        let width = t.width + 2.0 * from(&t.net);
        match &t.shape {
            TrackShape::Line(a, b) => {
                if Bounds::of([*a, *b]).is_some_and(|bb| bb.expanded(width).overlaps(&area)) {
                    stroke(g, *a, *b, width);
                }
            }
            TrackShape::Arc(arc) => {
                if Bounds::of(arc.points()).is_some_and(|bb| bb.expanded(width).overlaps(&area)) {
                    g.arc(width, None, arc, &[]);
                }
            }
        }
    }
    // A teardrop of another net: its fill grown by the clearance.
    for t in art
        .zones
        .iter()
        .filter(|t| t.teardrop && t.keepout.is_none() && !own(&t.net))
    {
        let clearance = from(&t.net);
        for (l, poly) in &t.filled {
            if l != layer
                || poly.len() < 3
                || !Bounds::of(poly.iter().copied())
                    .is_some_and(|b| b.expanded(clearance).overlaps(&area))
            {
                continue;
            }
            g.region(Contour::polygon(poly), None, &[]);
            let mut ring = poly.clone();
            ring.push(poly[0]);
            for w in ring.windows(2) {
                stroke(g, w[0], w[1], 2.0 * clearance);
            }
        }
    }
    for gr in art.graphics.iter().filter(|gr| gr.layer == layer) {
        let width = gr.width.max(0.0) + 2.0 * clearance;
        if gr.fill
            && let Shape::Poly(p) = &gr.shape
            && p.len() >= 3
        {
            g.region(Contour::polygon(p), None, &[]);
        }
        if let Shape::Circle { centre, radius } = &gr.shape
            && gr.fill
        {
            g.flash(
                crate::gbr::Ap::Circle(2.0 * radius + width),
                None,
                *centre,
                &[],
            );
            continue;
        }
        stroke_shape(g, &gr.shape, width);
    }
    Ok(())
}

/// The shape of a pad's hole.
fn hole_outline(pad: &crate::artwork::Pad, d: &crate::artwork::Drill) -> Outline {
    let c = pad.at - d.offset.rotate(pad.angle);
    if d.is_slot() {
        Outline::Oval {
            c,
            w: d.width,
            h: d.length,
            angle: pad.angle,
        }
    } else {
        Outline::Circle {
            c,
            r: d.width / 2.0,
        }
    }
}

fn draw_grown(
    g: &mut dyn Pen,
    shape: &Outline,
    by: f64,
    part: &str,
    number: &str,
) -> Result<(), String> {
    match shape.grow(by) {
        Ok(Some(s)) => {
            s.draw(g, None, &[]);
            Ok(())
        }
        Ok(None) => Ok(()),
        Err(e) => Err(format!("pad {part}.{number}: {e}")),
    }
}

/// The thermal relief of a pad or via: the gap round it, in the four pieces between the
/// spokes, which run along the axes turned by `angle` (the pad's turn and its spoke angle,
/// [`crate::artwork::Pad::spoke_angle`]). A custom pad is joined solid.
fn thermal(
    g: &mut dyn Pen,
    shape: &Outline,
    centre: Pt,
    angle: f64,
    z: &Zone,
) -> Result<(), String> {
    let Ok(Some(grown)) = shape.grow(z.thermal_gap) else {
        return Ok(());
    };
    let Some(poly) = grown.polygon() else {
        return Ok(());
    };
    let local: Vec<Pt> = poly.iter().map(|p| (*p - centre).rotate(-angle)).collect();
    let half = z.thermal_bridge / 2.0;
    // Each piece: the part of the gap in one quadrant beyond the spokes' half width.
    let quadrants = [
        [
            (Pt::new(half, 0.0), Pt::new(0.0, -1.0)),
            (Pt::new(0.0, half), Pt::new(1.0, 0.0)),
        ],
        [
            (Pt::new(-half, 0.0), Pt::new(0.0, 1.0)),
            (Pt::new(0.0, half), Pt::new(1.0, 0.0)),
        ],
        [
            (Pt::new(-half, 0.0), Pt::new(0.0, 1.0)),
            (Pt::new(0.0, -half), Pt::new(-1.0, 0.0)),
        ],
        [
            (Pt::new(half, 0.0), Pt::new(0.0, -1.0)),
            (Pt::new(0.0, -half), Pt::new(-1.0, 0.0)),
        ],
    ];
    for q in quadrants {
        let mut piece = local.clone();
        for (at, dir) in q {
            piece = geom::clip_half_plane(&piece, at, dir);
        }
        if piece.len() >= 3 {
            let world: Vec<Pt> = piece.iter().map(|p| centre + p.rotate(angle)).collect();
            g.region(Contour::polygon(&world), None, &[]);
        }
    }
    Ok(())
}

/// The four spokes of a thermal relief, each as the polygon of copper it is across the gap
/// (the pad's own copper with it) and the point just beyond the gap at its outer end, which the
/// fill has to reach (KiCad adds a spoke only where its end is on the fill). They run as
/// [`thermal`] leaves them, along the axes turned by `angle`.
fn spokes(shape: &Outline, centre: Pt, angle: f64, z: &Zone) -> Vec<(Vec<Pt>, Pt)> {
    let Ok(Some(grown)) = shape.grow(z.thermal_gap) else {
        return Vec::new();
    };
    let Some(poly) = grown.polygon() else {
        return Vec::new();
    };
    let half = z.thermal_bridge / 2.0;
    let mut out = Vec::new();
    for k in 0..4 {
        // In a frame where the spoke runs along +x.
        let turn = angle + 90.0 * f64::from(k);
        let local: Vec<Pt> = poly.iter().map(|p| (*p - centre).rotate(-turn)).collect();
        let mut spoke = geom::clip_half_plane(&local, Pt::new(0.0, half), Pt::new(-1.0, 0.0));
        spoke = geom::clip_half_plane(&spoke, Pt::new(0.0, -half), Pt::new(1.0, 0.0));
        spoke = geom::clip_half_plane(&spoke, Pt::new(0.0, 0.0), Pt::new(0.0, -1.0));
        if spoke.len() < 3 {
            continue;
        }
        let reach = spoke.iter().map(|p| p.x).fold(0.0, f64::max);
        let end = centre + Pt::new(reach + SPOKE_PROBE, 0.0).rotate(turn);
        let world = spoke.iter().map(|p| centre + p.rotate(turn)).collect();
        out.push((world, end));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gbr::Gerber;

    /// A via of GND in a corner, so that the GND pour touches its net and is no island.
    const GND_VIA: &str = r#"(via (at 2 2) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))"#;

    /// A board with a GND zone on F.Cu of minimum thickness `min` and the fill settings `fill`.
    fn art_with(min: f64, fill: &str, items: &str) -> Artwork {
        Artwork::parse(&format!(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
            (net 0 "") (net 1 "GND") (net 2 "SIG")
            (gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
            (zone (net 1) (net_name "GND") (layer "F.Cu") (connect_pads (clearance 0.3)) (min_thickness {min})
              (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5) {fill})
              (polygon (pts (xy 0.2 0.2) (xy 19.8 0.2) (xy 19.8 19.8) (xy 0.2 19.8))))
            {items})"#
        ))
        .unwrap()
    }

    fn art(items: &str) -> Artwork {
        art_with(0.25, "", items)
    }

    fn rules() -> Rules {
        Rules {
            edge_clearance: 0.2,
            hole_clearance: None,
            classes: Vec::new(),
        }
    }

    fn poured(a: &Artwork) -> String {
        let edge = crate::edge::outline(a).unwrap();
        let mut g = Gerber::new(Pt::default());
        pour(&mut g, a, "F.Cu", &edge, &rules()).unwrap();
        g.text("Copper,L1,Top", false, "B")
    }

    /// The fill of F.Cu of `a` as polygons, by net, with `rules`.
    fn fill_with(a: &Artwork, rules: &Rules) -> Vec<(Option<String>, Vec<Area>)> {
        let edge = crate::edge::outline(a).unwrap();
        let mut p = Polygons::new();
        pour(&mut p, a, "F.Cu", &edge, rules).unwrap();
        p.areas_by_net()
    }

    /// The fill of F.Cu of `a` as polygons, by net.
    fn fill_by_net(a: &Artwork) -> Vec<(Option<String>, Vec<Area>)> {
        fill_with(a, &Rules::default())
    }

    fn size(areas: &[(Option<String>, Vec<Area>)], net: &str) -> f64 {
        areas
            .iter()
            .filter(|(n, _)| n.as_deref() == Some(&format!("N,{net}")))
            .flat_map(|(_, a)| a.iter().map(Area::size))
            .sum()
    }

    /// Whether the fill of `net` is copper at (x, y).
    fn copper(areas: &[(Option<String>, Vec<Area>)], net: &str, x: f64, y: f64) -> bool {
        areas
            .iter()
            .filter(|(n, _)| n.as_deref() == Some(&format!("N,{net}")))
            .any(|(_, a)| a.iter().any(|a| a.contains(Pt::new(x, y))))
    }

    #[test]
    fn a_pour_is_dark_regions_of_its_net_less_the_clearance_round_other_copper() {
        let a = art(&format!(
            r#"{GND_VIA} (segment (start 5 10) (end 15 10) (width 0.2) (layer "F.Cu") (net 2))"#
        ));
        let t = poured(&a);
        assert!(t.contains("G36*") && t.contains("%TO.N,GND*%"), "{t}");
        assert!(t.trim_end().ends_with("M02*"));
        // The track is cleared to its half width and the clearance: 0.1 + 0.3.
        let f = fill_by_net(&a);
        assert!(!copper(&f, "GND", 10.0, 10.35) && copper(&f, "GND", 10.0, 10.45));
    }

    #[test]
    fn a_pour_keeps_the_clearance_of_the_class_of_another_net_when_it_is_larger() {
        let a = art(&format!(
            r#"{GND_VIA} (segment (start 5 10) (end 15 10) (width 0.2) (layer "F.Cu") (net 2))"#
        ));
        let class = |clearance: f64, pattern: &str| crate::drc::Class {
            name: format!("Clearance_{clearance}mm"),
            clearance: Some(clearance),
            priority: 0,
            patterns: vec![pattern.to_string()],
        };
        let rules = Rules {
            edge_clearance: 0.2,
            hole_clearance: None,
            classes: vec![class(0.5, "SIG")],
        };
        // The track is cleared to its half width and its class's clearance: 0.1 + 0.5.
        let f = fill_with(&a, &rules);
        assert!(!copper(&f, "GND", 10.0, 10.55) && copper(&f, "GND", 10.0, 10.65));
        assert_eq!(rules.class_clearance(Some("SIG")), 0.5);
        assert_eq!(rules.class_clearance(Some("GND")), 0.0);
    }

    #[test]
    fn a_pour_keeps_its_clearance_from_a_teardrop_of_another_net() {
        let drop = r#"(zone (net 2) (net_name "SIG") (layer "F.Cu") (attr (teardrop (type padvia)))
              (connect_pads yes (clearance 0)) (polygon (pts (xy 5 9) (xy 6 9.9) (xy 6 10.1) (xy 5 11)))
              (filled_polygon (layer "F.Cu") (pts (xy 5 9) (xy 6 9.9) (xy 6 10.1) (xy 5 11))))"#;
        let f = fill_by_net(&art(&format!("{GND_VIA} {drop}")));
        // Inside it, and 0.2 mm from its edge, less than the clearance.
        assert!(!copper(&f, "GND", 5.5, 10.0) && !copper(&f, "GND", 4.8, 10.0));
        assert!(copper(&f, "GND", 4.6, 10.0));
        // Of the pour's own net, nothing is cleared round it.
        let own = drop.replace("(net 2) (net_name \"SIG\")", "(net 1) (net_name \"GND\")");
        let f = fill_by_net(&art(&format!("{GND_VIA} {own}")));
        assert!(copper(&f, "GND", 5.5, 10.0) && copper(&f, "GND", 4.8, 10.0));
    }

    #[test]
    fn a_track_of_the_pours_own_net_is_not_cleared() {
        let a = art(r#"(segment (start 5 10) (end 15 10) (width 0.2) (layer "F.Cu") (net 1))"#);
        assert!(copper(&fill_by_net(&a), "GND", 10.0, 10.2));
    }

    fn pad(net: &str) -> String {
        format!(
            r#"(footprint "L:R" (layer "F.Cu") (at 10 10)
                (property "Reference" "R1")
                (pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu") (net {net})))"#
        )
    }

    #[test]
    fn a_pad_of_the_pours_net_gets_a_gap_crossed_by_spokes_and_another_nets_pad_a_full_ring() {
        let f = fill_by_net(&art(&pad("1 \"GND\"")));
        // The spokes across the gap (0.5 to 1.0 mm from the centre), and the gap between them.
        for (x, y) in [(10.75, 10.0), (9.25, 10.0), (10.0, 10.75), (10.0, 9.25)] {
            assert!(copper(&f, "GND", x, y), "a spoke at ({x}, {y})");
        }
        assert!(!copper(&f, "GND", 10.7, 10.7), "the gap");
        let f = fill_by_net(&art(&format!("{GND_VIA} {}", pad("2 \"SIG\""))));
        // The pad is cleared by a rounded rectangle of 0.5 + 0.3 from its centre.
        assert!(!copper(&f, "GND", 10.75, 10.0) && copper(&f, "GND", 10.85, 10.0));
    }

    #[test]
    fn copper_narrower_than_the_minimum_thickness_is_removed() {
        // Two tracks of SIG 1 mm apart leave a strip of GND 0.2 mm wide between them.
        let tracks = format!(
            r#"{GND_VIA} (segment (start 5 10) (end 15 10) (width 0.2) (layer "F.Cu") (net 2))
            (segment (start 5 11) (end 15 11) (width 0.2) (layer "F.Cu") (net 2))"#
        );
        assert!(!copper(&fill_by_net(&art(&tracks)), "GND", 10.0, 10.5));
        let thin = art_with(0.1, "", &tracks);
        assert!(copper(&fill_by_net(&thin), "GND", 10.0, 10.5));
        // A zone with no minimum keeps it.
        let none = art_with(0.0, "", &tracks);
        assert!(copper(&fill_by_net(&none), "GND", 10.0, 10.5));
    }

    /// A square ring of SIG round the middle of the board, which leaves GND an island inside.
    fn ring() -> String {
        let side = |(x0, y0): (f64, f64), (x1, y1): (f64, f64)| {
            format!(
                r#"(segment (start {x0} {y0}) (end {x1} {y1}) (width 0.2) (layer "F.Cu") (net 2))"#
            )
        };
        let c = [(5.0, 5.0), (15.0, 5.0), (15.0, 15.0), (5.0, 15.0)];
        (0..4).map(|i| side(c[i], c[(i + 1) % 4])).collect()
    }

    #[test]
    fn an_island_is_removed_as_the_zone_says() {
        let items = format!("{GND_VIA} {}", ring());
        let f = fill_by_net(&art(&items));
        assert!(!copper(&f, "GND", 10.0, 10.0), "the island goes");
        assert!(copper(&f, "GND", 3.0, 3.0), "the pour round the via stays");
        // Joined by a via of its net, it stays.
        let joined = format!(
            r#"{items} (via (at 10 10) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))"#
        );
        assert!(copper(&fill_by_net(&art(&joined)), "GND", 10.0, 10.0));
        // Kept by the zone, or kept because it is not smaller than the zone's least island
        // (the inside of the ring is 9.2 mm square, 84.6 mm²).
        let keep = art_with(0.25, "(island_removal_mode 1)", &items);
        assert!(copper(&fill_by_net(&keep), "GND", 10.0, 10.0));
        let big = art_with(0.25, "(island_removal_mode 2) (island_area_min 50)", &items);
        assert!(copper(&fill_by_net(&big), "GND", 10.0, 10.0));
        let small = art_with(
            0.25,
            "(island_removal_mode 2) (island_area_min 100)",
            &items,
        );
        assert!(!copper(&fill_by_net(&small), "GND", 10.0, 10.0));
        // A zone that touches nothing of its net is all island.
        assert_eq!(size(&fill_by_net(&art(&ring())), "GND"), 0.0);
    }

    #[test]
    fn a_zone_of_no_net_keeps_its_islands() {
        let a = art(&ring());
        let a = Artwork {
            zones: a
                .zones
                .into_iter()
                .map(|mut z| {
                    z.net = None;
                    z
                })
                .collect(),
            ..a
        };
        let f = fill_by_net(&a);
        assert!(
            f.iter()
                .any(|(_, areas)| { areas.iter().any(|a| a.contains(Pt::new(10.0, 10.0))) })
        );
    }

    #[test]
    fn a_spoke_whose_end_is_in_another_nets_clearance_is_removed() {
        // A SIG track at x = 11.3 clears GND from 10.8 to 11.8: the spoke to the right (10.5 to
        // 11.0) ends in it, and would be a stub from 10.5 to 10.8.
        let items = format!(
            r#"{} (segment (start 11.3 5) (end 11.3 15) (width 0.2) (layer "F.Cu") (net 2))"#,
            pad("1 \"GND\"")
        );
        let f = fill_by_net(&art(&items));
        assert!(!copper(&f, "GND", 10.65, 10.0), "the stub is gone");
        for (x, y) in [(9.25, 10.0), (10.0, 10.75), (10.0, 9.25)] {
            assert!(copper(&f, "GND", x, y), "the spoke at ({x}, {y}) stays");
        }
    }

    #[test]
    fn two_zones_of_different_nets_that_overlap_are_refused() {
        let a = art(
            r#"(zone (net 2) (net_name "SIG") (layer "F.Cu") (connect_pads (clearance 0.3))
              (polygon (pts (xy 5 5) (xy 15 5) (xy 15 15) (xy 5 15))))"#,
        );
        let edge = crate::edge::outline(&a).unwrap();
        let mut g = Gerber::new(Pt::default());
        let err = pour(&mut g, &a, "F.Cu", &edge, &Rules::default()).unwrap_err();
        assert!(
            err.contains("GND") && err.contains("SIG") && err.contains("F.Cu"),
            "{err}"
        );
    }

    #[test]
    fn of_two_zones_of_different_nets_that_overlap_the_higher_priority_has_the_area() {
        // SIG, priority 1, over the middle of the GND zone (priority 0): GND keeps 0.3 mm
        // (the larger clearance) away from SIG's fill. A via of each net joins its zone.
        // No minimum thickness, so that no corner is rounded and the areas are exact.
        let sig = r#"(via (at 10 10) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 2))"#;
        let a = art_with(
            0.0,
            "",
            &format!(
                r#"{GND_VIA} {sig} (zone (net 2) (net_name "SIG") (layer "F.Cu") (priority 1) (connect_pads (clearance 0.2))
              (polygon (pts (xy 5 5) (xy 15 5) (xy 15 15) (xy 5 15))))"#,
            ),
        );
        let by_net = fill_by_net(&a);
        let sig_size = size(&by_net, "SIG");
        let gnd = size(&by_net, "GND");
        assert!((sig_size - 100.0).abs() < 1e-6, "{sig_size}");
        // GND: the board less its edge band (19.6 - 2 * 0.2 on each side, 0.2 mm band from
        // the edge at 0) less SIG's square grown by 0.3 with round corners.
        let grown = 10.6 * 10.6 - (4.0 - std::f64::consts::PI) * 0.3 * 0.3;
        let board = 19.6 * 19.6;
        assert!((gnd - (board - grown)).abs() < 0.01, "{gnd}");
        // The other way round, GND has the middle and SIG nothing.
        let b = art_with(
            0.0,
            "",
            &format!(
                r#"{GND_VIA} {sig} (zone (net 2) (net_name "SIG") (layer "F.Cu") (priority 0) (connect_pads (clearance 0.2))
              (polygon (pts (xy 5 5) (xy 15 5) (xy 15 15) (xy 5 15))))"#,
            ),
        );
        let b = Artwork {
            zones: b
                .zones
                .into_iter()
                .map(|mut z| {
                    if z.net.as_deref() == Some("GND") {
                        z.priority = 2;
                    }
                    z
                })
                .collect(),
            ..b
        };
        // SIG keeps only the copper under its via (radius 0.3), where GND keeps away from it.
        let under = size(&fill_by_net(&b), "SIG");
        assert!(under <= std::f64::consts::PI * 0.09 + 1e-9, "{under}");
    }

    #[test]
    fn a_zones_further_polygons_are_holes() {
        let a = Artwork::parse(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal)) (net 1 "GND")
            (gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
            (via (at 3 3) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))
            (zone (net 1) (net_name "GND") (layer "F.Cu") (connect_pads (clearance 0.3))
              (polygon (pts (xy 2 2) (xy 18 2) (xy 18 18) (xy 2 18)))
              (polygon (pts (xy 5 5) (xy 10 5) (xy 10 10) (xy 5 10)))))"#,
        )
        .unwrap();
        assert_eq!((a.zones[0].outlines.len(), a.zones[0].holes.len()), (1, 1));
        let gnd = size(&fill_by_net(&a), "GND");
        assert!((gnd - (256.0 - 25.0)).abs() < 1e-6, "{gnd}");
    }

    #[test]
    fn a_fill_the_file_already_holds_is_not_filled_again() {
        let a = Artwork::parse(
            r#"(kicad_pcb (layers (0 "F.Cu" signal)) (net 1 "GND")
            (zone (net 1) (net_name "GND") (layer "F.Cu") (connect_pads (clearance 0.3))
              (polygon (pts (xy 0 0) (xy 5 0) (xy 5 5) (xy 0 5)))
              (filled_polygon (layer "F.Cu") (pts (xy 0 0) (xy 4 0) (xy 4 4) (xy 0 4)))))"#,
        )
        .unwrap();
        let edge = BoardEdge {
            outer: vec![Pt::new(0.0, 0.0), Pt::new(5.0, 0.0), Pt::new(5.0, 5.0)],
            cutouts: Vec::new(),
        };
        let mut g = Gerber::new(Pt::default());
        pour(&mut g, &a, "F.Cu", &edge, &Rules::default()).unwrap();
        assert!(g.is_empty());
    }

    #[test]
    fn a_zones_island_setting_is_read_from_its_fill() {
        assert_eq!(art("").zones[0].islands, Islands::Remove);
        let keep = art_with(0.25, "(island_removal_mode 1)", "");
        assert_eq!(keep.zones[0].islands, Islands::Keep);
        let below = art_with(0.25, "(island_removal_mode 2) (island_area_min 3.5)", "");
        assert_eq!(below.zones[0].islands, Islands::RemoveBelow(3.5));
    }
}
