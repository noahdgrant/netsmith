//! Filling zones: the copper pours of a board (the planes of an inner layer and the pours of
//! an outer one) written into a Gerber layer by layer polarity, with no polygon arithmetic.
//!
//! The file says it in order: the zone's outline as dark copper; then, in clear polarity,
//! everything the pour keeps away from, namely the outside of the board and a band along
//! its edge, rule areas that forbid pours, the clearance round the copper of other nets
//! (pads, vias, tracks,
//! teardrops, drawings) and round holes, and the thermal gaps round the pads of
//! its own net, drawn as the four pieces between the spokes so that the spokes stay copper;
//! then the caller draws every pad, via and track dark again. What the reader sees is the
//! pour with the clearances cut out, which is the fill.
//!
//! It draws with a [`Pen`]: the Gerber file, or [`crate::polygons::Polygons`], which works the
//! same objects out as polygons with holes. `netsmith_pcb::copper` fills a board's pours that
//! way for the 3D view (ADR 0105), so its pours are this fill and there is one filler.
//!
//! This is the same picture KiCad's fill makes, with these differences (also issue #61):
//! copper that no pad, via or track of the net reaches (islands) is not
//! removed, and slivers narrower than the zone's minimum thickness are not trimmed.
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
//! (`filled_polygon`) are used as they are, and two zones of different nets that overlap on one
//! layer are refused. The pour's difference from KiCad's is islands and spokes: KiCad removes a
//! spoke or a patch of copper that no pad of the net reaches, and netsmith leaves it. It is copper
//! outside every clearance, so it cannot make a short, but it is dead copper.

use crate::artwork::{Artwork, Connect, PadKind, Shape, TrackShape, Zone};
use crate::edge::Outline as BoardEdge;
use crate::gbr::{Contour, Pen};
use crate::geom::{self, Arc, Bounds, Pt};
use crate::pads::{self, Outline};
use crate::plot::Rules;

/// KiCad's clearance when a zone gives none.
const DEFAULT_CLEARANCE: f64 = 0.2;

/// Whether a via joins a pour with thermal spokes (as a pad does) rather than solid copper:
/// KiCad joins it solid (found by the cross-check, ADR 0034).
const VIA_THERMAL: bool = false;

/// Writes the fill of every zone of `layer` that holds no fill of its own in the file.
/// `edge_clearance` is the copper-to-edge clearance of the board.
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
    for (i, a) in zones.iter().enumerate() {
        for b in zones.iter().skip(i + 1) {
            let (ba, bb) = (zone_bounds(a), zone_bounds(b));
            if a.net != b.net && ba.zip(bb).is_some_and(|(x, y)| x.overlaps(&y)) {
                return Err(format!(
                    "two zones on {layer} overlap and are on different nets ({} and {}); netsmith fills one net's pour per area of a layer",
                    a.net.as_deref().unwrap_or("no net"),
                    b.net.as_deref().unwrap_or("no net")
                ));
            }
        }
    }

    g.polarity(true);
    for z in &zones {
        for o in &z.outlines {
            if o.len() >= 3 {
                g.region(
                    Contour::polygon(o),
                    Some("Conductor"),
                    &net_attr(z.net.as_deref()),
                );
            }
        }
    }

    g.polarity(false);
    keep_out_of_the_edge(g, art, edge, rules.edge_clearance, &zones);
    for k in art.zones.iter().filter(|z| {
        z.keepout.as_ref().is_some_and(|k| k.copperpour) && z.layers.iter().any(|l| l == layer)
    }) {
        for o in &k.outlines {
            if o.len() >= 3 {
                g.region(Contour::polygon(o), None, &[]);
            }
        }
    }
    for z in &zones {
        around_copper(g, art, layer, z, rules)?;
    }
    g.polarity(true);
    Ok(())
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
    let clearance = if z.clearance > 0.0 {
        z.clearance
    } else {
        DEFAULT_CLEARANCE
    };
    let reach = clearance + z.thermal_gap.max(0.0);
    let Some(area) = zone_bounds(z).map(|b| b.expanded(reach)) else {
        return Ok(());
    };
    let own = |net: &Option<String>| z.net.is_some() && *net == z.net;

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
        let mode = match pad.zone_connect.unwrap_or(z.connect) {
            Connect::ThruHoleOnly if pad.kind == PadKind::Smd => Connect::Solid,
            Connect::ThruHoleOnly => Connect::Thermal,
            other => other,
        };
        if own(&pad.net) && mode != Connect::None {
            if mode == Connect::Thermal {
                thermal(g, &shape, pad.at, pad.angle + pad.spoke_angle(), z)?;
            }
        } else {
            let by = pad.clearance.map_or(clearance, |c| c.max(clearance));
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
            draw_grown(g, &shape, clearance, "via", "")?;
        }
    }
    for t in art
        .tracks
        .iter()
        .filter(|t| t.layer == layer && !own(&t.net))
    {
        let width = t.width + 2.0 * clearance;
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
    let c = pad.at + d.offset.rotate(pad.angle);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gbr::Gerber;

    fn art(items: &str) -> Artwork {
        Artwork::parse(&format!(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
            (net 0 "") (net 1 "GND") (net 2 "SIG")
            (gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
            (zone (net 1) (net_name "GND") (layer "F.Cu") (connect_pads (clearance 0.3)) (min_thickness 0.25)
              (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5))
              (polygon (pts (xy 0.2 0.2) (xy 19.8 0.2) (xy 19.8 19.8) (xy 0.2 19.8))))
            {items})"#
        ))
        .unwrap()
    }

    fn poured(a: &Artwork) -> String {
        let edge = crate::edge::outline(a).unwrap();
        let mut g = Gerber::new(Pt::default());
        pour(
            &mut g,
            a,
            "F.Cu",
            &edge,
            &Rules {
                edge_clearance: 0.2,
                hole_clearance: None,
            },
        )
        .unwrap();
        g.text("Copper,L1,Top", false, "B")
    }

    #[test]
    fn a_pour_is_its_outline_dark_then_what_it_avoids_in_clear_polarity() {
        let a = art(r#"(segment (start 5 10) (end 15 10) (width 0.2) (layer "F.Cu") (net 2))"#);
        let t = poured(&a);
        let dark = t.find("G36*").unwrap();
        let clear = t.find("%LPC*%").unwrap();
        let track = t.find("X5000000Y-10000000D02*").unwrap();
        assert!(dark < clear && clear < track, "{t}");
        // The track is cleared with its width and twice the clearance: 0.2 + 0.6 = 0.8.
        assert!(t.contains("C,0.800000"), "{t}");
        assert!(t.trim_end().ends_with("M02*"));
        assert!(t.contains("%TO.N,GND*%"), "the pour is on its net");
    }

    #[test]
    fn a_pour_keeps_its_clearance_from_a_teardrop_of_another_net() {
        let drop = r#"(zone (net 2) (net_name "SIG") (layer "F.Cu") (attr (teardrop (type padvia)))
              (connect_pads yes (clearance 0)) (polygon (pts (xy 5 9) (xy 6 9.9) (xy 6 10.1) (xy 5 11)))
              (filled_polygon (layer "F.Cu") (pts (xy 5 9) (xy 6 9.9) (xy 6 10.1) (xy 5 11))))"#;
        let t = poured(&art(drop));
        let clear = t.find("%LPC*%").unwrap();
        // The teardrop as a clear region, and its edges stroked at twice the clearance.
        let region = t[clear..].find("X5000000Y-9000000D02*").map(|i| i + clear);
        assert!(region.is_some(), "{t}");
        assert!(t.contains("C,0.600000"), "{t}");
        // Of the pour's own net, nothing is cleared round it.
        let own = drop.replace("(net 2) (net_name \"SIG\")", "(net 1) (net_name \"GND\")");
        assert!(!poured(&art(&own)).contains("X5000000Y-9000000"));
    }

    #[test]
    fn a_track_of_the_pours_own_net_is_not_cleared() {
        let a = art(r#"(segment (start 5 10) (end 15 10) (width 0.2) (layer "F.Cu") (net 1))"#);
        assert!(!poured(&a).contains("C,0.800000"));
    }

    #[test]
    fn a_pad_of_the_pours_net_gets_four_gap_pieces_and_another_nets_pad_a_full_ring() {
        let own = art(r#"(footprint "L:R" (layer "F.Cu") (at 10 10)
                (property "Reference" "R1")
                (pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu") (net 1 "GND")))"#);
        let t = poured(&own);
        // One region for the pour, one for the outside, and four pieces; the pad has no ring.
        assert_eq!(t.matches("G36*").count(), 1 + 1 + 4, "{t}");
        let other = art(r#"(footprint "L:R" (layer "F.Cu") (at 10 10)
                (property "Reference" "R1")
                (pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu") (net 2 "SIG")))"#);
        let t = poured(&other);
        // The pad is cleared by a rounded rectangle of 1 + 2 * 0.3.
        assert!(t.contains("RoundRect,0.300000"), "{t}");
        assert_eq!(t.matches("G36*").count(), 2);
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
}
