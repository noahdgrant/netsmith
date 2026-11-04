//! Filling zones: the copper pours of a board (the planes of an inner layer and the pours of
//! an outer one) written into a Gerber layer by layer polarity, with no polygon arithmetic.
//!
//! The file says it in order: the zone's outline as dark copper; then, in clear polarity,
//! everything the pour keeps away from, namely the outside of the board and a band along
//! its edge, rule areas that forbid pours, the clearance round the copper of other nets
//! (pads, vias, tracks, drawings) and round holes, and the thermal gaps round the pads of
//! its own net, drawn as the four pieces between the spokes so that the spokes stay copper;
//! then the caller draws every pad, via and track dark again. What the reader sees is the
//! pour with the clearances cut out, which is the fill.
//!
//! This is the same picture KiCad's fill makes, with these differences (also issue #61):
//! copper that no pad, via or track of the net reaches (islands) is not
//! removed, and slivers narrower than the zone's minimum thickness are not trimmed.

use crate::artwork::{Artwork, Connect, PadKind, Shape, TrackShape, Zone};
use crate::edge::Outline as BoardEdge;
use crate::gbr::{Contour, Gerber};
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
    g: &mut Gerber,
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
    g: &mut Gerber,
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
fn stroke_shape(g: &mut Gerber, shape: &Shape, width: f64) {
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

fn stroke(g: &mut Gerber, a: Pt, b: Pt, width: f64) {
    if a == b {
        g.flash(crate::gbr::Ap::Circle(width), None, a, &[]);
    } else {
        g.line(width, None, a, b, &[]);
    }
}

/// Clears what one zone keeps away from on `layer`.
fn around_copper(
    g: &mut Gerber,
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
    g: &mut Gerber,
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
    g: &mut Gerber,
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

    /// Whether the pour's picture is copper at `r` millimetres from the pad at (10, 10), in
    /// the direction `deg` degrees (counter-clockwise on the board).
    fn copper_at(a: &Artwork, deg: f64, r: f64) -> bool {
        let t = poured(a);
        let view = crate::raster::View {
            min: (0.0, -20.0),
            max: (20.0, 0.0),
        };
        let img = crate::raster::render_gerber(&t, &view, 40.0).unwrap();
        // The board's y is down and the Gerber's up: the pad is at (10, -10) there.
        let (x, y) = (
            10.0 + r * deg.to_radians().cos(),
            -10.0 + r * deg.to_radians().sin(),
        );
        let (col, row) = (((x - img.x0) * 40.0) as usize, ((0.0 - y) * 40.0) as usize);
        img.bits[row * img.width + col] != 0
    }

    fn pad_art(at_turn: f64, shape: &str, extra: &str) -> Artwork {
        art(&format!(
            r#"(footprint "L:P" (layer "F.Cu") (at 10 10 {at_turn})
                (property "Reference" "J1")
                (pad "1" thru_hole {shape} (at 0 0 {at_turn}) (size 1 1) (drill 0.4) (layers "*.Cu") (net 1 "GND") {extra}))"#
        ))
    }

    #[test]
    fn a_round_pads_spokes_run_at_45_degrees_as_kicad_has_them() {
        // The spokes are copper 0.75 mm from the centre, in the middle of the gap's ring.
        let a = pad_art(0.0, "circle", "");
        for deg in [45.0, 135.0, 225.0, 315.0] {
            assert!(copper_at(&a, deg, 0.75), "a spoke at {deg}");
        }
        for deg in [0.0, 90.0, 180.0, 270.0] {
            assert!(!copper_at(&a, deg, 0.75), "a gap at {deg}");
        }
    }

    #[test]
    fn any_other_pads_spokes_run_along_its_axes_and_turn_with_it() {
        let a = pad_art(0.0, "rect", "");
        for deg in [0.0, 90.0, 180.0, 270.0] {
            assert!(copper_at(&a, deg, 0.75), "a spoke at {deg}");
        }
        assert!(!copper_at(&a, 45.0, 0.9), "a gap at 45");
        // A part turned by 30 degrees (its pads are at 30 in the file) turns the spokes with it.
        let a = pad_art(30.0, "rect", "");
        assert!(copper_at(&a, 30.0, 0.75) && !copper_at(&a, 0.0, 0.75));
        // So does a round pad's: 45 degrees from the part's axes.
        let a = pad_art(30.0, "circle", "");
        assert!(copper_at(&a, 75.0, 0.75) && !copper_at(&a, 30.0, 0.75));
    }

    #[test]
    fn a_pads_own_thermal_bridge_angle_decides_its_spokes() {
        let a = pad_art(0.0, "circle", "(thermal_bridge_angle 90)");
        assert!(copper_at(&a, 0.0, 0.75) && !copper_at(&a, 45.0, 0.75));
        let a = pad_art(0.0, "rect", "(thermal_bridge_angle 45)");
        assert!(copper_at(&a, 45.0, 0.9) && !copper_at(&a, 0.0, 0.75));
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
