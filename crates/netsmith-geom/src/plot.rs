//! The Gerber layers of a board, made from its [`Artwork`]: every copper layer (pads, vias,
//! tracks, drawings, and the fill of its zones), the solder mask openings, the paste
//! stencil, the silkscreen and the board outline, named and attributed as KiCad names
//! them (ADR 0034).
//!
//! The files are RS-274X (Gerber X2), format 4.6 in millimetres, origin the drill/place origin,
//! y up, apertures numbered by first use, the `RoundRect`, `RotRect` and `HorizOval` macros
//! KiCad writes and `TF`, `TA.AperFunction` and `TO` attributes (pad, net, component). There is no
//! creation date, so equal input gives equal bytes. A pad is a flash where the shape allows it
//! (circle, rectangle, obround, rounded rectangle, and the turned ones through macros) and a
//! region where it does not (trapezoid, chamfered corners, custom pads: the anchor and each
//! primitive). `F.Mask` and `B.Mask` are negatives: the openings.
//!
//! Mask and paste use the pad's own margins first, then the board's (`pad_to_mask_clearance`,
//! `pad_to_paste_clearance` and its ratio). KiCad's rules, found by the cross-check: the mask
//! grows a shape by the margin all round (a rectangle gets round corners of that radius); the
//! paste ratio is of each side of the pad, not of the shorter one (a 1.6 by 1.0 pad with a ratio
//! of -0.1 loses 0.16 along and 0.1 across, each side) and a rounded rectangle keeps its corner
//! ratio; a via is open in the mask only on a face where it is not tented (`(tenting ...)` of the
//! board or of the via); a hole with no copper round it (a pad no larger than its drill) has no
//! copper on any layer.

use crate::Output;
use crate::artwork::{Artwork, Graphic, Pad, PadKind, PadShape, Shape, TrackShape};
use crate::edge::{self, Outline as BoardEdge};
use crate::fill;
use crate::font;
use crate::gbr::{Ap, Contour, Gerber, Pen, escape};
use crate::geom::{Arc, Pt};
use crate::gerber::{self, GerberError};
use crate::pads;

/// The width of a line drawn with none, in mm.
const DEFAULT_LINE: f64 = 0.1;
/// The width of the board outline when its lines give none.
const DEFAULT_EDGE: f64 = 0.05;

/// The board's rules that the plot needs, from its project file.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rules {
    /// The copper-to-edge clearance zones keep from the outline (0 if the board says none).
    pub edge_clearance: f64,
    /// The clearance a pour keeps from a hole with no copper round it, if the board says.
    pub hole_clearance: Option<f64>,
}

/// What plotting made.
#[derive(Debug)]
pub struct Plot {
    /// The Gerber files, named `<board>-F_Cu.gbr`... and sorted by name.
    pub files: Vec<Output>,
    /// Things left out or approximated, for the notes.
    pub notes: Vec<String>,
}

/// Every Gerber layer a board has. `edge_clearance` is the copper-to-edge clearance zones
/// keep from the outline.
pub fn plot(art: &Artwork, name: &str, rules: &Rules) -> Result<Plot, GerberError> {
    let edge = edge::outline(art).map_err(GerberError::Outline)?;
    let mut notes: Vec<String> = art.skipped.clone();
    let mut files = Vec::new();
    for layer in gerber::plot_layers(&art.copper)? {
        let mut g = Gerber::new(art.origin);
        let (function, negative) = if let Some(i) = art.copper.iter().position(|l| *l == layer) {
            copper(&mut g, art, &layer, &edge, rules, &mut notes)?;
            let n = art.copper.len();
            let side = if i == 0 {
                "Top"
            } else if i + 1 == n {
                "Bot"
            } else {
                "Inr"
            };
            (format!("Copper,L{},{side}", i + 1), false)
        } else {
            match layer.as_str() {
                "F.Mask" | "B.Mask" => {
                    mask(&mut g, art, &layer, &mut notes)?;
                    (format!("Soldermask,{}", side(&layer)), true)
                }
                "F.Paste" | "B.Paste" => {
                    paste(&mut g, art, &layer, &mut notes)?;
                    (format!("Paste,{}", side(&layer)), false)
                }
                "F.SilkS" | "B.SilkS" => {
                    ink(&mut g, art, &layer, &mut notes);
                    (format!("Legend,{}", side(&layer)), false)
                }
                _ => {
                    outline(&mut g, art, &mut notes);
                    ("Profile,NP".to_string(), false)
                }
            }
        };
        if gerber::required(&layer) || !g.is_empty() {
            files.push(Output {
                name: format!("{name}-{}.gbr", gerber::file_suffix(&layer)),
                bytes: g.text(&function, negative, name).into_bytes(),
            });
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    notes.sort();
    notes.dedup();
    Ok(Plot { files, notes })
}

fn side(layer: &str) -> &'static str {
    if layer.starts_with('F') { "Top" } else { "Bot" }
}

/// The attributes of an object: the pad it is, its net, its part.
fn pad_attrs(pad: &Pad) -> Vec<String> {
    let mut v = Vec::new();
    if !pad.part.is_empty() {
        let mut p = format!("P,{},{}", escape(&pad.part), escape(&pad.number));
        if let Some(f) = pad.function.as_deref().filter(|f| !f.is_empty()) {
            p.push(',');
            p.push_str(&escape(f));
        }
        v.push(p);
    }
    v.push(format!("N,{}", escape(pad.net.as_deref().unwrap_or("N/C"))));
    if !pad.part.is_empty() {
        v.push(format!("C,{}", escape(&pad.part)));
    }
    v
}

fn net_attrs(net: Option<&str>) -> Vec<String> {
    vec![format!("N,{}", escape(net.unwrap_or("N/C")))]
}

fn component_attrs(pad: &Pad) -> Vec<String> {
    if pad.part.is_empty() {
        Vec::new()
    } else {
        vec![format!("C,{}", escape(&pad.part))]
    }
}

fn pad_function(pad: &Pad) -> &'static str {
    match pad.kind {
        PadKind::Smd => "SMDPad,CuDef",
        PadKind::Thru | PadKind::Npth => "ComponentPad",
        PadKind::Connect => "ConnectorPad",
    }
}

fn copper(
    g: &mut dyn Pen,
    art: &Artwork,
    layer: &str,
    edge: &BoardEdge,
    rules: &Rules,
    notes: &mut Vec<String>,
) -> Result<(), GerberError> {
    fill::pour(g, art, layer, edge, rules).map_err(GerberError::Fill)?;
    // A fill the board file holds already.
    for z in art.zones.iter().filter(|z| z.keepout.is_none()) {
        for (l, poly) in &z.filled {
            if l == layer && poly.len() >= 3 {
                g.region(
                    Contour::polygon(poly),
                    Some("Conductor"),
                    &net_attrs(z.net.as_deref()),
                );
            }
        }
    }
    for pad in art.pads.iter().filter(|p| art.pad_copper_on(p, layer)) {
        pads::outline(pad).draw(g, Some(pad_function(pad)), &pad_attrs(pad));
    }
    for via in &art.vias {
        g.flash(
            Ap::Circle(via.diameter),
            Some("ViaPad"),
            via.at,
            &net_attrs(via.net.as_deref()),
        );
    }
    for t in art.tracks.iter().filter(|t| t.layer == layer) {
        let attrs = net_attrs(t.net.as_deref());
        match &t.shape {
            TrackShape::Line(a, b) if a == b => {
                g.flash(Ap::Circle(t.width), Some("Conductor"), *a, &attrs)
            }
            TrackShape::Line(a, b) => g.line(t.width, Some("Conductor"), *a, *b, &attrs),
            TrackShape::Arc(arc) => g.arc(t.width, Some("Conductor"), arc, &attrs),
        }
    }
    drawings(g, art, layer, Some("Conductor"), &mut *notes);
    Ok(())
}

/// The openings of the mask on one face: pads (grown by their margin), vias that are not
/// tented, and what is drawn on the mask layer. The file is a negative: what is drawn is
/// where the mask is open.
pub fn mask(
    g: &mut dyn Pen,
    art: &Artwork,
    layer: &str,
    notes: &mut Vec<String>,
) -> Result<(), GerberError> {
    for pad in art
        .pads
        .iter()
        .filter(|p| p.layers.iter().any(|l| l == layer))
    {
        let margin = pad.mask_margin.unwrap_or(art.mask_margin);
        let shape = pads::outline(pad);
        if pad.size.0 <= 0.0 || pad.size.1 <= 0.0 {
            continue;
        }
        match shape.grow(margin) {
            Ok(Some(s)) => s.draw(g, None, &component_attrs(pad)),
            Ok(None) => {}
            Err(e) => {
                return Err(GerberError::Pad {
                    pad: format!("{}.{}", pad.part, pad.number),
                    layer: layer.to_string(),
                    what: e,
                });
            }
        }
    }
    let front = layer.starts_with('F');
    for via in &art.vias {
        let tented = if front { via.tented.0 } else { via.tented.1 };
        if !tented {
            g.flash(
                Ap::Circle(via.diameter + 2.0 * art.mask_margin),
                None,
                via.at,
                &net_attrs(via.net.as_deref()),
            );
        }
    }
    drawings(g, art, layer, None, notes);
    Ok(())
}

/// The stencil openings: pads on the paste layer, shrunk or grown by their margin and
/// ratio. As KiCad does, the ratio is of each side of the pad: a pad 1.6 by 1.0 with a ratio
/// of -0.1 is shrunk by 0.16 along its length and by 0.1 across (both sides), and a rounded
/// rectangle keeps its corner ratio. A trapezoid or custom pad is shrunk evenly by the
/// ratio of its shorter side.
fn paste(
    g: &mut dyn Pen,
    art: &Artwork,
    layer: &str,
    notes: &mut Vec<String>,
) -> Result<(), GerberError> {
    for pad in art
        .pads
        .iter()
        .filter(|p| p.layers.iter().any(|l| l == layer))
    {
        let ratio = pad.paste_ratio.unwrap_or(art.paste_ratio);
        let base = pad.paste_margin.unwrap_or(art.paste_margin);
        let (w, h) = pad.size;
        let opening = match pad.shape {
            PadShape::Circle | PadShape::Rect | PadShape::RoundRect | PadShape::Oval => {
                let mut resized = pad.clone();
                resized.size = (w + 2.0 * (base + ratio * w), h + 2.0 * (base + ratio * h));
                if resized.size.0 <= 0.0 || resized.size.1 <= 0.0 {
                    continue;
                }
                Ok(Some(pads::outline(&resized)))
            }
            PadShape::Trapezoid | PadShape::Custom => {
                pads::outline(pad).grow(base + ratio * w.min(h))
            }
        };
        match opening {
            Ok(Some(s)) => s.draw(g, None, &component_attrs(pad)),
            Ok(None) => {}
            Err(e) => {
                return Err(GerberError::Pad {
                    pad: format!("{}.{}", pad.part, pad.number),
                    layer: layer.to_string(),
                    what: e,
                });
            }
        }
    }
    drawings(g, art, layer, None, notes);
    Ok(())
}

/// The silkscreen: lines, arcs, circles, polygons and text.
fn ink(g: &mut dyn Pen, art: &Artwork, layer: &str, notes: &mut Vec<String>) {
    drawings(g, art, layer, None, notes);
}

/// The board outline: each line and arc stroked with the outline's pen.
fn outline(g: &mut dyn Pen, art: &Artwork, notes: &mut Vec<String>) {
    for gr in art.graphics.iter().filter(|gr| gr.layer == "Edge.Cuts") {
        let width = if gr.width > 0.0 {
            gr.width
        } else {
            DEFAULT_EDGE
        };
        let f = Some("Profile");
        match &gr.shape {
            Shape::Line(a, b) => g.line(width, f, *a, *b, &[]),
            Shape::Arc(arc) => g.arc(width, f, arc, &[]),
            Shape::Circle { centre, radius } => {
                let start = Pt::new(centre.x + radius, centre.y);
                g.arc(
                    width,
                    f,
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
                    g.line(width, f, p[i], p[(i + 1) % p.len()], &[]);
                }
            }
        }
    }
    // Text on the outline layer is part of it: the "V-CUT" mark of a panel (ADR 0040).
    texts(g, art, "Edge.Cuts", Some("Profile"), notes);
}

/// Everything drawn and written on `layer` (graphics and text), as ink.
pub fn drawings(
    g: &mut dyn Pen,
    art: &Artwork,
    layer: &str,
    function: Option<&'static str>,
    notes: &mut Vec<String>,
) {
    for gr in art.graphics.iter().filter(|gr| gr.layer == layer) {
        draw_graphic(g, gr, function);
    }
    texts(g, art, layer, function, notes);
}

/// The text written on `layer`, as ink.
fn texts(
    g: &mut dyn Pen,
    art: &Artwork,
    layer: &str,
    function: Option<&'static str>,
    notes: &mut Vec<String>,
) {
    for t in art.texts.iter().filter(|t| t.layer == layer) {
        let (paths, missing) = font::strokes(t);
        if !missing.is_empty() {
            notes.push(format!(
                "text `{}` on {layer} ({}) has {} the built-in stroke font does not have; drawn as a box",
                t.text.replace('\n', " "),
                t.owner,
                missing
                    .iter()
                    .map(|c| format!("`{c}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let width = t.thickness.max(0.0);
        for path in paths {
            let dot = path.iter().all(|p| *p == path[0]);
            if dot {
                g.flash(Ap::Circle(width), function, path[0], &[]);
            } else {
                for pair in path.windows(2) {
                    if pair[0] != pair[1] {
                        g.line(width, function, pair[0], pair[1], &[]);
                    }
                }
            }
        }
    }
}

fn draw_graphic(g: &mut dyn Pen, gr: &Graphic, function: Option<&'static str>) {
    let width = if gr.width > 0.0 {
        gr.width
    } else {
        DEFAULT_LINE
    };
    match &gr.shape {
        Shape::Line(a, b) => {
            if a == b {
                g.flash(Ap::Circle(width), function, *a, &[]);
            } else {
                g.line(width, function, *a, *b, &[]);
            }
        }
        Shape::Arc(arc) => g.arc(width, function, arc, &[]),
        Shape::Circle { centre, radius } => {
            if gr.fill {
                let ink = if gr.width > 0.0 { gr.width } else { 0.0 };
                g.flash(Ap::Circle(2.0 * radius + ink), function, *centre, &[]);
            } else {
                let start = Pt::new(centre.x + radius, centre.y);
                g.arc(
                    width,
                    function,
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
        }
        Shape::Poly(p) => {
            if gr.fill && p.len() >= 3 {
                g.region(Contour::polygon(p), function, &[]);
            }
            if !gr.fill || gr.width > 0.0 {
                for i in 0..p.len() {
                    let (a, b) = (p[i], p[(i + 1) % p.len()]);
                    if a != b {
                        g.line(width, function, a, b, &[]);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOARD: &str = r#"(kicad_pcb
        (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
        (setup (pad_to_mask_clearance 0.05) (tenting front back))
        (net 0 "") (net 1 "GND") (net 2 "VCC")
        (footprint "L:R" (layer "F.Cu") (at 10 10)
          (property "Reference" "R1" (at 0 -2 0) (layer "F.SilkS") (effects (font (size 1 1) (thickness 0.15))))
          (fp_line (start -1 -1) (end 1 -1) (stroke (width 0.12) (type solid)) (layer "F.SilkS"))
          (pad "1" smd roundrect (at -1 0) (size 0.8 0.9) (layers "F.Cu" "F.Mask" "F.Paste") (roundrect_rratio 0.25) (net 1 "GND"))
          (pad "2" thru_hole circle (at 1 0) (size 1.6 1.6) (drill 0.8) (layers "*.Cu" "*.Mask") (net 2 "VCC")))
        (segment (start 9 10) (end 5 10) (width 0.2) (layer "F.Cu") (net 1))
        (via (at 5 10) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))
        (gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
    )"#;

    fn files() -> Plot {
        plot(
            &Artwork::parse(BOARD).unwrap(),
            "B",
            &Rules {
                edge_clearance: 0.2,
                hole_clearance: None,
            },
        )
        .unwrap()
    }

    fn get<'a>(p: &'a Plot, suffix: &str) -> &'a str {
        std::str::from_utf8(
            &p.files
                .iter()
                .find(|f| f.name == format!("B-{suffix}.gbr"))
                .unwrap_or_else(|| panic!("no {suffix}"))
                .bytes,
        )
        .unwrap()
    }

    #[test]
    fn a_two_layer_board_gets_the_layers_the_accepted_package_has() {
        let p = files();
        let names: Vec<&str> = p.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "B-B_Cu.gbr",
                "B-B_Mask.gbr",
                "B-Edge_Cuts.gbr",
                "B-F_Cu.gbr",
                "B-F_Mask.gbr",
                "B-F_Paste.gbr",
                "B-F_Silkscreen.gbr",
            ]
        );
    }

    #[test]
    fn copper_has_pads_vias_and_tracks_with_their_attributes() {
        let p = files();
        let f = get(&p, "F_Cu");
        assert!(
            f.contains("%TF.FileFunction,Copper,L1,Top*%")
                && f.contains("%TF.FilePolarity,Positive*%")
        );
        assert!(
            f.contains("%TA.AperFunction,SMDPad,CuDef*%\n%ADD10RoundRect,0.200000X"),
            "{f}"
        );
        assert!(
            f.contains("%TO.P,R1,1*%\n%TO.N,GND*%\n%TO.C,R1*%\nD10*\nX9000000Y-10000000D03*"),
            "{f}"
        );
        assert!(
            f.contains("%TA.AperFunction,ComponentPad*%\n%ADD11C,1.600000*%"),
            "{f}"
        );
        assert!(f.contains("%TA.AperFunction,ViaPad*%") && f.contains("C,0.600000"));
        assert!(f.contains("%TA.AperFunction,Conductor*%") && f.contains("C,0.200000"));
        let b = get(&p, "B_Cu");
        assert!(b.contains("%TF.FileFunction,Copper,L2,Bot*%"));
        assert!(
            b.contains("ADD10C,1.600000") && b.contains("ViaPad"),
            "the through pad and the via reach the back: {b}"
        );
        assert!(
            !b.contains("RoundRect"),
            "the surface pad stays on the front"
        );
    }

    #[test]
    fn the_mask_is_a_negative_with_the_pads_grown_by_the_margin() {
        let p = files();
        let m = get(&p, "F_Mask");
        assert!(
            m.contains("%TF.FileFunction,Soldermask,Top*%\n%TF.FilePolarity,Negative*%"),
            "{m}"
        );
        // 0.8 x 0.9 with a radius of 0.2 grown by 0.05 is 0.9 x 1.0 with a radius of 0.25.
        assert!(m.contains("RoundRect,0.250000"), "{m}");
        assert!(m.contains("C,1.700000"), "the through pad: 1.6 + 0.1: {m}");
        assert!(
            !m.contains("C,0.700000"),
            "a via is tented, so it is not open: {m}"
        );
        let b = get(&p, "B_Mask");
        assert!(b.contains("C,1.700000") && !b.contains("RoundRect"));
    }

    #[test]
    fn paste_has_only_the_surface_pads_and_the_outline_is_a_profile() {
        let p = files();
        let t = get(&p, "F_Paste");
        assert!(t.contains("%TF.FileFunction,Paste,Top*%") && t.contains("RoundRect"));
        assert!(!t.contains("C,1.6"));
        let e = get(&p, "Edge_Cuts");
        assert!(e.contains("%TF.FileFunction,Profile,NP*%") && !e.contains("FilePolarity"));
        assert!(e.contains("%TA.AperFunction,Profile*%\n%ADD10C,0.050000*%"));
        assert_eq!(e.matches("D01*").count(), 4);
    }

    #[test]
    fn the_silkscreen_has_lines_and_text() {
        let p = files();
        let s = get(&p, "F_Silkscreen");
        assert!(s.contains("%TF.FileFunction,Legend,Top*%"));
        assert!(s.contains("C,0.120000") && s.contains("C,0.150000"), "{s}");
        assert!(s.matches("D01*").count() > 5, "the text R1 is strokes");
    }

    #[test]
    fn a_back_layer_with_nothing_on_it_is_left_out_and_with_something_it_is_kept() {
        let p = files();
        assert!(
            !p.files
                .iter()
                .any(|f| f.name.contains("B_Paste") || f.name.contains("B_Silkscreen"))
        );
        let with_back = BOARD.replace(
            "(fp_line (start -1 -1)",
            r#"(fp_line (start -1 1) (end 1 1) (stroke (width 0.12) (type solid)) (layer "B.SilkS"))
          (fp_line (start -1 -1)"#,
        );
        let p = plot(
            &Artwork::parse(&with_back).unwrap(),
            "B",
            &Rules {
                edge_clearance: 0.2,
                hole_clearance: None,
            },
        )
        .unwrap();
        assert!(p.files.iter().any(|f| f.name == "B-B_Silkscreen.gbr"));
    }

    #[test]
    fn a_four_layer_board_has_inner_copper_with_its_function() {
        let src = BOARD.replace(
            "(layers (0 \"F.Cu\" signal) (2 \"B.Cu\" signal))",
            "(layers (0 \"F.Cu\" signal) (4 \"In1.Cu\" signal) (6 \"In2.Cu\" signal) (2 \"B.Cu\" signal))",
        );
        let p = plot(
            &Artwork::parse(&src).unwrap(),
            "B",
            &Rules {
                edge_clearance: 0.2,
                hole_clearance: None,
            },
        )
        .unwrap();
        assert!(get(&p, "In1_Cu").contains("%TF.FileFunction,Copper,L2,Inr*%"));
        assert!(get(&p, "In2_Cu").contains("%TF.FileFunction,Copper,L3,Inr*%"));
        assert!(get(&p, "B_Cu").contains("%TF.FileFunction,Copper,L4,Bot*%"));
    }

    #[test]
    fn a_board_without_an_outline_is_refused_with_the_reason() {
        let src = BOARD.replace(
            r#"(gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))"#,
            "",
        );
        let err = plot(&Artwork::parse(&src).unwrap(), "B", &Rules::default())
            .unwrap_err()
            .to_string();
        assert!(err.contains("no outline"), "{err}");
    }

    #[test]
    fn text_with_a_character_the_font_lacks_is_noted() {
        let src = BOARD.replace(
            "(property \"Reference\" \"R1\"",
            "(property \"Reference\" \"R\u{e9}\"",
        );
        let p = plot(
            &Artwork::parse(&src).unwrap(),
            "B",
            &Rules {
                edge_clearance: 0.2,
                hole_clearance: None,
            },
        )
        .unwrap();
        assert!(
            p.notes
                .iter()
                .any(|n| n.contains("`\u{e9}`") && !n.contains("R1") && n.contains("F.SilkS")),
            "{:?}",
            p.notes
        );
    }

    #[test]
    fn a_custom_pad_with_a_margin_that_shrinks_it_is_refused_with_its_name() {
        let src = BOARD.replace(
            "(pad \"1\" smd roundrect (at -1 0) (size 0.8 0.9) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\") (roundrect_rratio 0.25) (net 1 \"GND\"))",
            "(pad \"1\" smd custom (at -1 0) (size 0.8 0.9) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\") (solder_mask_margin -0.1) (net 1 \"GND\") (options (anchor circle)) (primitives (gr_line (start 0 0) (end 1 0) (width 0.2))))",
        );
        let err = plot(&Artwork::parse(&src).unwrap(), "B", &Rules::default())
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("pad R1.1 on F.Mask: a custom pad cannot be shrunk"),
            "{err}"
        );
    }

    #[test]
    fn paste_is_shrunk_by_the_ratio_of_each_side_as_kicad_does() {
        let src = BOARD.replace(
            "(roundrect_rratio 0.25) (net 1 \"GND\"))",
            "(roundrect_rratio 0.25) (net 1 \"GND\") (solder_paste_margin_ratio -0.1))",
        );
        let p = plot(&Artwork::parse(&src).unwrap(), "B", &Rules::default()).unwrap();
        let t = get(&p, "F_Paste");
        // 0.8 x 0.9 becomes 0.64 x 0.72, the corner ratio kept: radius 0.25 * 0.64 = 0.16.
        assert!(t.contains("RoundRect,0.160000X-0.160000X-0.200000"), "{t}");
    }
}
