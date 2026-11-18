//! The silkscreen rules (issue #1186): the text sizes, silkscreen over an opening in the solder
//! mask (`silk_over_copper`), silkscreen too near other silkscreen (`silk_overlap`) and too near
//! the board edge (`silk_edge_clearance`), each as KiCad 9's check makes it, measured on boards
//! made for the purpose with `kicad-cli pcb drc`:
//!
//! - **`silk_over_copper`**: a drawing or a text on `F.SilkS` (`B.SilkS`) against everything that
//!   opens the mask on its face: a pad's opening (the pad grown by its mask margin), a via that is
//!   not tented on that face (its copper grown by the board's margin), and what is drawn on
//!   `F.Mask` (`B.Mask`): lines, arcs, circles, polygons, texts and the fill a zone on that layer
//!   holds. KiCad's least is 0 (the ink touches the opening) unless a custom rule gives one. A
//!   drawing is held to that: the board house clips the line where it lies over the opening and
//!   prints the rest. A text is held to the process's space from a pad to silkscreen
//!   (`.min_silk_to_pad`, JLCPCB's 0.15 mm), because the board house removes it: "characters
//!   overlapping with solder pads or copper areas within 0.15mm will be removed" (its page on
//!   character design, read 2026-11-18). The space reaches both checks as the rule `netsmith
//!   silk_over_copper` of the `.kicad_dru` ([`super::Extras::text_to_mask`]); holding a library
//!   footprint's lines to it too would warn of every small resistor of KiCad's library, whose
//!   lines stand 0.11 mm off its pads. One finding for each drawing or text, with the nearest
//!   opening, at the ink's place.
//! - **`silk_overlap`**: a text against any silkscreen on its layer, and two drawings unless both
//!   are one part's or both the board's (a part's outline is many lines that touch), nearer than
//!   the project's `min_silk_clearance` (the process's `.min_silk_to_silk`).
//! - **`silk_edge_clearance`**: a drawing or a text nearer the board edge than
//!   `min_silk_clearance` (KiCad's check takes the silkscreen's clearance for the edge too; JLCPCB
//!   publishes no space from silkscreen to the edge), to the edge's path (`model::edge_of`: the
//!   centre of a line on `Edge.Cuts`, the stroke of one on `Margin`), one finding for each drawing
//!   or text, with the nearest edge.
//!
//! A hidden text is not read from the file ([`crate::artwork`]), so a part whose designator the
//! design hides (`.silk &= "hide"`) has none to check.
//!
//! **The font.** A text is drawn in netsmith's own font ([`crate::font`]), which is what the
//! package's Gerbers print; KiCad's check draws KiCad's. The two differ at the sides of a text by
//! up to [`FONT_MARGIN`] of its size (measured from KiCad 9's Gerbers of every character the font
//! draws, issue #1186), so a text that only just clears or only just breaks a limit can be judged
//! otherwise by KiCad. [`font_dependent`] lists those pairs, so that the cross-check does not count
//! a finding one check alone makes of them as a difference.

use super::model::{Model, drawn_at, drawn_kind, drawn_shape};
use super::shape::{Gap, Shape, gap, near_pairs};
use super::{FontDependent, Item, Rule, checks::EPS, checks::Out};
use crate::artwork::{Graphic, Text};
use crate::font;
use crate::geom::{Bounds, Pt};

/// How far the ink of a text in netsmith's font may lie from where KiCad's font puts it, as a
/// share of the text's size: the most measured, 0.61 mm at the far end of a left-justified `R1`
/// 1 mm high (KiCad's glyphs keep a side bearing; netsmith's start at the anchor), with a margin.
pub const FONT_MARGIN: f64 = 0.65;

/// A drawing or text on a silkscreen layer, as the ink it lays down.
struct Ink {
    /// The size of a text (the larger of its width and height); `None` for a drawing.
    text: Option<f64>,
    layer: &'static str,
    owner: String,
    shapes: Vec<Shape>,
    bounds: Bounds,
    item: Item,
}

/// What opens the solder mask on one face.
struct Opening {
    front: bool,
    shapes: Vec<Shape>,
    bounds: Bounds,
    /// What it is, for the message: `pad 1 of R1`, `Via [GND]`, `Segment on F.Mask`.
    what: String,
    item: Item,
}

fn silk_layer(name: &str) -> Option<&'static str> {
    match name {
        "F.SilkS" => Some("F.SilkS"),
        "B.SilkS" => Some("B.SilkS"),
        _ => None,
    }
}

fn mask_layer(name: &str) -> Option<bool> {
    match name {
        "F.Mask" => Some(true),
        "B.Mask" => Some(false),
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
    Ink {
        text: None,
        layer,
        owner: g.owner.clone(),
        bounds: shape.bounds,
        shapes: vec![shape],
        item: Item {
            what: format!("{} on {layer}", drawn_kind(&g.shape)),
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
        text: Some(t.size.0.max(t.size.1)),
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

/// Everything that opens the mask: pads, vias that are not tented, and what is drawn on the
/// mask layers.
fn openings(m: &Model<'_>) -> Vec<Opening> {
    let art = m.art;
    let mut out = Vec::new();
    let mut push = |front: bool, shapes: Vec<Shape>, what: String, item: Item| {
        if !shapes.is_empty() {
            let bounds = Model::bounds_of(&shapes);
            out.push(Opening {
                front,
                shapes,
                bounds,
                what,
                item,
            });
        }
    };
    for p in &art.pads {
        for front in [true, false] {
            push(
                front,
                m.mask_opening(p, front),
                format!("pad {} of {}", p.number, p.part),
                Item {
                    what: format!("Pad {} of {}", p.number, p.part),
                    at: p.at,
                    part: Some(p.part.clone()),
                    pad: Some(p.number.clone()),
                    net: p.net.clone(),
                },
            );
        }
    }
    for v in &art.vias {
        for (front, tented) in [(true, v.tented.0), (false, v.tented.1)] {
            // The outer copper of the face: a buried via has none to open the mask over.
            let face = if front {
                art.copper.first()
            } else {
                art.copper.last()
            };
            if tented || face.is_some_and(|f| !art.via_on(v, f)) {
                continue;
            }
            let net = v
                .net
                .as_ref()
                .map(|n| format!(" [{n}]"))
                .unwrap_or_default();
            push(
                front,
                vec![Shape::disc(v.at, v.diameter / 2.0 + art.mask_margin)],
                format!("Via{net}"),
                Item {
                    what: format!("Via{net}"),
                    at: v.at,
                    part: None,
                    pad: None,
                    net: v.net.clone(),
                },
            );
        }
    }
    for g in &art.graphics {
        let Some(front) = mask_layer(&g.layer) else {
            continue;
        };
        let what = format!("{} on {}", drawn_kind(&g.shape), g.layer);
        push(
            front,
            vec![drawn_shape(&g.shape, g.width, g.fill)],
            what.clone(),
            Item {
                what,
                at: drawn_at(&g.shape),
                part: (!g.owner.is_empty()).then(|| g.owner.clone()),
                pad: None,
                net: None,
            },
        );
    }
    for t in &art.texts {
        let Some(front) = mask_layer(&t.layer) else {
            continue;
        };
        let layer = if front { "F.Mask" } else { "B.Mask" };
        let ink = text_ink(t, layer);
        push(front, ink.shapes, ink.item.what.clone(), ink.item);
    }
    for z in &art.zones {
        for (layer, points) in &z.filled {
            let Some(front) = mask_layer(layer) else {
                continue;
            };
            let what = format!("Zone on {layer}");
            push(
                front,
                vec![Shape::polygon(points.clone(), 0.0)],
                what.clone(),
                Item {
                    what,
                    at: points.first().copied().unwrap_or_default(),
                    part: None,
                    pad: None,
                    net: None,
                },
            );
        }
    }
    out
}

fn nearest(a: &[Shape], b: &[Shape]) -> Option<Gap> {
    let mut best: Option<Gap> = None;
    for sa in a {
        for sb in b {
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

/// Whether KiCad compares two pieces of silkscreen: a text with anything, and two drawings
/// unless they are both the board's or both one part's.
fn compared(a: &Ink, b: &Ink) -> bool {
    a.layer == b.layer && (a.text.is_some() || b.text.is_some() || a.owner != b.owner)
}

/// The least space from a text to an opening in the mask: KiCad's 0, or the process's.
fn text_least(m: &Model<'_>) -> f64 {
    m.extras.text_to_mask.unwrap_or(0.0).max(0.0)
}

/// The least space from a piece of silkscreen to an opening: a text's, or 0 for a drawing.
fn mask_least(m: &Model<'_>, ink: &Ink) -> f64 {
    if ink.text.is_some() {
        text_least(m)
    } else {
        0.0
    }
}

/// Whether a gap breaks a least: touching where the least is 0, nearer than it otherwise.
fn breaks(d: f64, least: f64) -> bool {
    d <= 0.0 || d < least - EPS
}

pub(crate) fn run(out: &mut Out<'_>) {
    let m = out.m;
    text_sizes(out);
    let inks = inks(m);
    if inks.is_empty() {
        return;
    }
    over_mask(out, &inks);
    let least = m.rules.silk_clearance;
    if least > 0.0 {
        overlap(out, &inks, least);
        to_edge(out, &inks, least);
    }
}

fn text_sizes(out: &mut Out<'_>) {
    let r = out.m.rules;
    for t in &out.m.art.texts {
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
}

/// Silkscreen over (or nearer than the least to) an opening in the mask, where the fabricator
/// clips it: once for each drawing or text, with the nearest opening.
fn over_mask(out: &mut Out<'_>, inks: &[Ink]) {
    let m = out.m;
    let openings = openings(m);
    let mut boxes: Vec<Bounds> = inks.iter().map(|i| i.bounds).collect();
    boxes.extend(openings.iter().map(|o| o.bounds));
    let n = inks.len();
    let mut nearest_of: Vec<Option<(Gap, usize)>> = (0..n).map(|_| None).collect();
    for (i, j) in near_pairs(&boxes, text_least(m)) {
        if i >= n || j < n {
            continue;
        }
        let (ink, o) = (&inks[i], &openings[j - n]);
        if (ink.layer == "F.SilkS") != o.front {
            continue;
        }
        if let Some(g) = nearest(&ink.shapes, &o.shapes)
            && breaks(g.d, mask_least(m, ink))
            && nearest_of[i].is_none_or(|(b, _)| g.d < b.d)
        {
            nearest_of[i] = Some((g, j - n));
        }
    }
    for (ink, near) in inks.iter().zip(&nearest_of) {
        let Some((g, o)) = near else { continue };
        let o = &openings[*o];
        let least = mask_least(m, ink);
        let message = if g.d <= 0.0 {
            format!(
                "{} lies over the opening in the solder mask of {}, where the fabricator clips it",
                ink.item.what, o.what
            )
        } else {
            format!(
                "{} is {:.4} mm from the opening in the solder mask of {}, where the fabricator clips it; the least is {:.4} mm",
                ink.item.what, g.d, o.what, least
            )
        };
        out.push(
            Rule::SilkOverCopper,
            message,
            ink.item.at,
            Some(ink.layer),
            vec![ink.item.clone(), o.item.clone()],
        );
    }
}

/// Silkscreen too near other silkscreen.
fn overlap(out: &mut Out<'_>, inks: &[Ink], least: f64) {
    let boxes: Vec<Bounds> = inks.iter().map(|i| i.bounds).collect();
    for (i, j) in near_pairs(&boxes, least) {
        let (a, b) = (&inks[i], &inks[j]);
        if !compared(a, b) {
            continue;
        }
        if let Some(g) = nearest(&a.shapes, &b.shapes)
            && g.d < least - EPS
        {
            out.push(
                Rule::SilkOverlap,
                format!(
                    "silkscreen is {:.4} mm between {} and {}; the least is {:.4} mm",
                    g.d, a.item.what, b.item.what, least
                ),
                g.middle(),
                Some(a.layer),
                vec![a.item.clone(), b.item.clone()],
            );
        }
    }
}

/// Silkscreen too near the board edge: once for each drawing or text, with the nearest edge.
fn to_edge(out: &mut Out<'_>, inks: &[Ink], least: f64) {
    let m = out.m;
    let n = inks.len();
    let mut boxes: Vec<Bounds> = inks.iter().map(|i| i.bounds).collect();
    boxes.extend(m.edge.iter().map(|(s, _)| s.bounds));
    let mut nearest_of: Vec<Option<(Gap, usize)>> = (0..n).map(|_| None).collect();
    for (i, j) in near_pairs(&boxes, least) {
        if i >= n || j < n {
            continue;
        }
        let edge = &m.edge[j - n].0;
        if let Some(g) = nearest(&inks[i].shapes, std::slice::from_ref(edge))
            && g.d < least - EPS
            && nearest_of[i].is_none_or(|(b, _)| g.d < b.d)
        {
            nearest_of[i] = Some((g, j - n));
        }
    }
    for (ink, near) in inks.iter().zip(&nearest_of) {
        let Some((g, e)) = near else { continue };
        out.push(
            Rule::SilkEdgeClearance,
            format!(
                "{} is {:.4} mm from the board edge; the least is {:.4} mm",
                ink.item.what, g.d, least
            ),
            g.middle(),
            Some(ink.layer),
            vec![m.edge[*e].1.clone(), ink.item.clone()],
        );
    }
}

/// The box round a text's ink, as a filled shape.
fn text_box(ink: &Ink) -> Shape {
    let b = ink.bounds;
    Shape::polygon(
        vec![
            b.min,
            Pt::new(b.max.x, b.min.y),
            b.max,
            Pt::new(b.min.x, b.max.y),
        ],
        0.0,
    )
}

/// The pairs of a silkscreen rule with a text in them that the font decides: the other thing
/// lies within the least and [`FONT_MARGIN`] of the text's size of the box round the text's ink,
/// so that KiCad's glyphs, which keep within that margin of netsmith's, may break the rule where
/// netsmith's do not, or the other way round. A pair further off is judged alike by both.
pub(crate) fn font_dependent(m: &Model<'_>) -> Vec<FontDependent> {
    let inks = inks(m);
    let texts: Vec<usize> = (0..inks.len())
        .filter(|i| inks[*i].text.is_some())
        .collect();
    if texts.is_empty() {
        return Vec::new();
    }
    let margin = |i: &Ink| i.text.unwrap_or(0.0) * FONT_MARGIN;
    let widest = texts.iter().map(|i| margin(&inks[*i])).fold(0.0, f64::max);
    let mut out = Vec::new();
    let mut record = |rule: Rule, places: Vec<Pt>| out.push(FontDependent { rule, places });

    // A text against the openings of the mask.
    let least = text_least(m);
    let openings = openings(m);
    let mut boxes: Vec<Bounds> = texts.iter().map(|i| inks[*i].bounds).collect();
    boxes.extend(openings.iter().map(|o| o.bounds));
    let n = texts.len();
    for (i, j) in near_pairs(&boxes, least + widest) {
        if i >= n || j < n {
            continue;
        }
        let (ink, o) = (&inks[texts[i]], &openings[j - n]);
        if (ink.layer == "F.SilkS") != o.front {
            continue;
        }
        if nearest(&[text_box(ink)], &o.shapes).is_some_and(|g| g.d < least + margin(ink)) {
            record(Rule::SilkOverCopper, vec![ink.item.at, o.item.at]);
        }
    }

    // A text against other silkscreen, and against the edge.
    let least = m.rules.silk_clearance;
    if least <= 0.0 {
        return out;
    }
    let boxes: Vec<Bounds> = inks.iter().map(|i| i.bounds).collect();
    for (i, j) in near_pairs(&boxes, least + 2.0 * widest) {
        let (a, b) = (&inks[i], &inks[j]);
        if (a.text.is_none() && b.text.is_none()) || !compared(a, b) {
            continue;
        }
        let shape = |ink: &Ink| match ink.text {
            Some(_) => vec![text_box(ink)],
            None => ink.shapes.clone(),
        };
        if nearest(&shape(a), &shape(b)).is_some_and(|g| g.d < least + margin(a) + margin(b)) {
            record(Rule::SilkOverlap, vec![a.item.at, b.item.at]);
        }
    }
    let mut boxes: Vec<Bounds> = texts.iter().map(|i| inks[*i].bounds).collect();
    boxes.extend(m.edge.iter().map(|(s, _)| s.bounds));
    for (i, j) in near_pairs(&boxes, least + widest) {
        if i >= n || j < n {
            continue;
        }
        let (ink, (edge, edge_item)) = (&inks[texts[i]], &m.edge[j - n]);
        if gap(&text_box(ink), edge).d < least + margin(ink) {
            record(Rule::SilkEdgeClearance, vec![edge_item.at, ink.item.at]);
        }
    }
    out
}
