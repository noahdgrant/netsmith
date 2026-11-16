//! The fabricator's rules for fiducials and test points (issue #761), which the caller passes in
//! [`Extras::marks`](super::Extras::marks) from the board's process. Each is netsmith's own: the
//! cross-check counts its findings and does not compare them. A board read with no process is
//! checked as KiCad would check it, so these rules are not checked without `marks`.
//!
//! **A limit or a guideline is the wording of the fabricator's page** (the owner's decision of
//! 2026-11-15). What the page says it cannot make or place is a limit, a rule of severity error
//! that stops `fab`; its advice, and wording that is not clear, is a guideline, a warning.
//!
//! - **Limits** (JLCPCB, "Fiducial Marks in PCB and SMT Assembly" and the help article "How to Add
//!   Edge Rails and Fiducials for PCB Assembly Orders", both read 2026-11-16): "fiducial markers
//!   must maintain a minimum clearance of 3.35 mm from the board edge" (`fiducial_edge`); "the
//!   fiducial must be exposed copper and free of any solder mask" (`fiducial_mask`); "a clear
//!   zone free of other copper features (traces, planes, vias, text) is necessary"
//!   (`fiducial_keepout`).
//! - **Guidelines**: "a minimum of two, but three fiducial marks are strongly recommended", on
//!   each side that has assembled parts (`fiducial_count`: the help article says JLCPCB adds edge
//!   rails with fiducials to an assembled board, so a board with fewer is not one it cannot
//!   assemble); "a copper pad with a diameter of approximately 1.0 mm" and "the solder mask
//!   opening should be at least twice the diameter of the fiducial pad" (`fiducial_size`); and
//!   every number of the blog "Maximizing Circuit Reliability with PCB Test Points and Smart DFT"
//!   (read 2026-11-16): the table's "absolute minimum" pad of 0.8 mm for a signal net
//!   (`test_point_pad`), probe pitch 1.27 mm (`test_point_pitch`), clearance to the component
//!   body 0.5 mm (`test_point_body`) and to the board edge 3.2 mm (`test_point_edge`), with the two rows that depend on the board: the pad of a
//!   test point on a power or ground net, 1.0 mm (the same `test_point_pad` rule, [`Marks::power_nets`] says which
//!   nets), and the clearance to the body of a part above 6.4 mm tall, 2.0 mm (the same
//!   `test_point_body` rule, [`Marks::tall_parts`] says which parts: a part of unknown height is
//!   not in it and is judged as any other). The blog
//!   is about the fixture an in-circuit test needs and not a statement of what JLCPCB makes, so
//!   it is advice, and its "recommended production value" of each row is named in the finding and
//!   not enforced.
//!
//! **The measures.** The fiducial's distance to the edge is from its copper (`Fiducial_1mm_Mask2mm`
//! centred 3.85 mm from the edge is 3.35 mm of copper, as the panel's rails put theirs, which is
//! the reading that agrees with JLCPCB's own panel); the keep-out is the fiducial's mask opening,
//! which the page makes equal to the mask clearance; planes are not checked, because a pour's fill
//! is not computed here (see the module [`super`]). A test point's clearances are from its copper;
//! its body is the box of a part on its side (`body_of`) and fiducials and other test points are
//! not bodies. A test point's pitch is between pad centres on one side.

use super::checks::{EPS, Out};
use super::constraints::body_of;
use super::model::{Kind, Model};
use super::shape::{Gap, Shape, gap};
use super::{Item, Marks, Rule};
use crate::artwork::Pad;
use crate::geom::Pt;

pub(crate) fn run(out: &mut Out<'_>) {
    let Some(marks) = out.m.extras.marks.clone() else {
        return;
    };
    fiducials(out, &marks);
    fiducial_count(out, &marks);
    test_points(out, &marks);
}

fn back_of(m: &Model<'_>, pad: &Pad) -> bool {
    m.art
        .footprints
        .iter()
        .find(|f| f.part == pad.part)
        .map_or(!pad.layers.iter().any(|l| l == "F.Cu"), |f| f.back)
}

/// The shapes of a pad's copper, with the item that has them.
fn copper_of<'a>(m: &'a Model<'_>, pad: usize) -> Option<&'a super::model::Copper> {
    m.items
        .iter()
        .find(|c| matches!(c.kind, Kind::Pad(i) if i == pad))
}

fn pad_item(p: &Pad) -> Item {
    Item {
        what: format!("Pad {} of {}", p.number, p.part),
        at: p.at,
        part: Some(p.part.clone()),
        pad: Some(p.number.clone()),
        net: p.net.clone(),
    }
}

/// The nearest board edge to some shapes, with the edge's item: the distance is to the path
/// the edge is drawn along, as the pages measure "from the board edge".
fn edge_gap(m: &Model<'_>, shapes: &[Shape]) -> Option<(Gap, Item)> {
    let mut best: Option<(Gap, Item)> = None;
    for (edge, item) in &m.edge {
        for s in shapes {
            // To the edge's path, not to the ink of the line that draws it.
            let mut g = gap(s, edge);
            g.d += edge.r;
            if best.as_ref().is_none_or(|(b, _)| g.d < b.d) {
                best = Some((g, item.clone()));
            }
        }
    }
    best
}

fn width_of(shapes: &[Shape]) -> f64 {
    let b = Model::bounds_of(shapes);
    (b.max.x - b.min.x).max(b.max.y - b.min.y)
}

fn layer_name(back: bool) -> &'static str {
    if back { "B.Cu" } else { "F.Cu" }
}

fn fiducials(out: &mut Out<'_>, marks: &Marks) {
    let m = out.m;
    for (i, pad) in m.art.pads.iter().enumerate() {
        if !marks.fiducials.contains(&pad.part) {
            continue;
        }
        let Some(copper) = copper_of(m, i) else {
            continue;
        };
        let back = back_of(m, pad);
        let side = if back { "back" } else { "front" };
        let layer = layer_name(back);
        let diameter = width_of(&copper.shapes);
        let opening = m.mask_opening(pad, !back);
        let opening_width = width_of(&opening);

        // A limit: the copper is exposed, all of it.
        if opening.is_empty() || opening_width < diameter - EPS {
            out.push(
                Rule::FiducialMask,
                format!(
                    "the fiducial {} on the {side} has solder mask over its copper (its mask opening is {:.4} mm for {:.4} mm of copper); the board house asks that it \"must be exposed copper and free of any solder mask\"",
                    pad.part, opening_width, diameter
                ),
                pad.at,
                Some(layer),
                vec![pad_item(pad)],
            );
        }

        // A limit: the copper keeps `min_fiducial_edge` from the board edge.
        if marks.min_fiducial_edge > 0.0
            && let Some((g, edge)) = edge_gap(m, &copper.shapes)
            && g.d < marks.min_fiducial_edge - EPS
        {
            out.push(
                Rule::FiducialEdge,
                format!(
                    "the fiducial {} is {:.4} mm from the board edge; the board house asks \"a minimum clearance of {:.2} mm from the board edge\"",
                    pad.part, g.d, marks.min_fiducial_edge
                ),
                g.middle(),
                Some(layer),
                vec![edge, pad_item(pad)],
            );
        }

        // A limit: no other copper in the mask opening, the keep-out.
        let on_layer = if back { m.layers.len() - 1 } else { 0 };
        for other in &m.items {
            if matches!(other.kind, Kind::Pad(j) if j == i)
                || !other.layers.contains(&on_layer)
                || matches!(other.kind, Kind::Pad(j) if m.art.pads[j].part == pad.part)
            {
                continue;
            }
            let near = opening
                .iter()
                .flat_map(|o| other.shapes.iter().map(move |s| gap(s, o)))
                .fold(None::<Gap>, |b, g| match b {
                    Some(b) if b.depth >= g.depth => Some(b),
                    _ => Some(g),
                });
            if let Some(g) = near
                && g.depth > EPS
            {
                out.push(
                    Rule::FiducialKeepout,
                    format!(
                        "{} is inside the mask opening of the fiducial {}, which is its keep-out; the board house asks for \"a clear zone free of other copper features (traces, planes, vias, text)\"",
                        other.item.what, pad.part
                    ),
                    g.middle(),
                    Some(layer),
                    vec![pad_item(pad), other.item.clone()],
                );
            }
        }

        // Guidelines: the recommended diameter, and a mask opening twice it.
        if marks.fiducial_diameter > 0.0 && (diameter - marks.fiducial_diameter).abs() > EPS {
            out.push(
                Rule::FiducialSize,
                format!(
                    "the fiducial {} has {:.4} mm of copper; the board house recommends {:.2} mm",
                    pad.part, diameter, marks.fiducial_diameter
                ),
                pad.at,
                Some(layer),
                vec![pad_item(pad)],
            );
        }
        if marks.fiducial_mask_ratio > 0.0
            && !opening.is_empty()
            && opening_width < diameter * marks.fiducial_mask_ratio - EPS
        {
            out.push(
                Rule::FiducialSize,
                format!(
                    "the mask opening of the fiducial {} is {:.4} mm for {:.4} mm of copper; the board house recommends an opening at least {} times the copper ({:.4} mm)",
                    pad.part,
                    opening_width,
                    diameter,
                    marks.fiducial_mask_ratio,
                    diameter * marks.fiducial_mask_ratio
                ),
                pad.at,
                Some(layer),
                vec![pad_item(pad)],
            );
        }
    }
}

/// A guideline: each side that has assembled parts has at least `min_fiducials` of them.
fn fiducial_count(out: &mut Out<'_>, marks: &Marks) {
    let m = out.m;
    if marks.min_fiducials == 0 {
        return;
    }
    let at = Model::bounds_of(&m.edge.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>());
    let place = Pt::new(at.min.x, at.min.y);
    for (back, assembled) in [(false, marks.assembled_front), (true, marks.assembled_back)] {
        if !assembled {
            continue;
        }
        let have = m
            .art
            .footprints
            .iter()
            .filter(|f| f.back == back && marks.fiducials.contains(&f.part))
            .count();
        if have < marks.min_fiducials {
            let side = if back { "back" } else { "front" };
            out.push(
                Rule::FiducialCount,
                format!(
                    "the {side} of the board has parts the board house assembles and {have} fiducial(s); it asks for at least {} on each assembled side (\"a minimum of two, but three fiducial marks are strongly recommended\")",
                    marks.min_fiducials
                ),
                place,
                Some(layer_name(back)),
                Vec::new(),
            );
        }
    }
}

/// Guidelines for the test points: the pad, the pitch, the body and the edge.
fn test_points(out: &mut Out<'_>, marks: &Marks) {
    let m = out.m;
    // One entry per pad of a test point: the pad, its index, the side.
    let pads: Vec<(usize, &Pad, bool)> = m
        .art
        .pads
        .iter()
        .enumerate()
        .filter(|(_, p)| marks.test_points.contains(&p.part))
        .map(|(i, p)| (i, p, back_of(m, p)))
        .collect();
    for (i, pad, back) in &pads {
        let Some(copper) = copper_of(m, *i) else {
            continue;
        };
        let layer = layer_name(*back);
        let side = pad_size(pad);
        // A supply or a ground has a larger pad than a signal, where the process states one.
        let power = pad
            .net
            .as_ref()
            .is_some_and(|n| marks.power_nets.contains(n))
            && marks.min_power_test_point_pad > marks.min_test_point_pad;
        let (least, what) = if power {
            (
                marks.min_power_test_point_pad,
                "on a power or ground net (an \"absolute minimum\"; 1.5 to 2.0 mm is recommended)",
            )
        } else {
            (
                marks.min_test_point_pad,
                "(an \"absolute minimum\"; 1.0 to 1.27 mm is recommended)",
            )
        };
        if least > 0.0 && side < least - EPS {
            out.push(
                Rule::TestPointPad,
                format!(
                    "the test point {} is {:.4} mm wide; its pad is advised to be at least {:.2} mm {what}",
                    pad.part, side, least
                ),
                pad.at,
                Some(layer),
                vec![pad_item(pad)],
            );
        }
        if marks.min_test_point_edge > 0.0
            && let Some((g, edge)) = edge_gap(m, &copper.shapes)
            && g.d < marks.min_test_point_edge - EPS
        {
            out.push(
                Rule::TestPointEdge,
                format!(
                    "the test point {} is {:.4} mm from the board edge; the advice is at least {:.2} mm (an \"absolute minimum\"; 3.8 mm is recommended)",
                    pad.part, g.d, marks.min_test_point_edge
                ),
                g.middle(),
                Some(layer),
                vec![edge, pad_item(pad)],
            );
        }
        if marks.min_test_point_body > 0.0 {
            let body_shapes = |part: &str, back: bool| -> Option<Vec<Shape>> {
                let (b, _) = body_of(m.art, part, back)?;
                Some(vec![Shape::polygon(
                    vec![
                        b.min,
                        Pt::new(b.max.x, b.min.y),
                        b.max,
                        Pt::new(b.min.x, b.max.y),
                    ],
                    0.0,
                )])
            };
            let mut parts: Vec<&str> = m
                .art
                .footprints
                .iter()
                .filter(|f| {
                    f.back == *back
                        && f.part != pad.part
                        && !marks.test_points.contains(&f.part)
                        && !marks.fiducials.contains(&f.part)
                })
                .map(|f| f.part.as_str())
                .collect();
            parts.dedup();
            for part in parts {
                let Some(body) = body_shapes(part, *back) else {
                    continue;
                };
                let Some(g) = copper
                    .shapes
                    .iter()
                    .flat_map(|s| body.iter().map(move |b| gap(s, b)))
                    .min_by(|a, b| a.d.total_cmp(&b.d))
                else {
                    continue;
                };
                // A part of known height above the process's tall one needs more room.
                let tall = marks
                    .tall_parts
                    .iter()
                    .find(|(p, _)| p == part)
                    .filter(|_| marks.min_test_point_tall_body > marks.min_test_point_body);
                let (least, what) = match tall {
                    Some((_, height)) => (
                        marks.min_test_point_tall_body,
                        format!(
                            "(it stands {height:.2} mm tall, above {:.1} mm; an \"absolute minimum\", and 5.0 mm is recommended)",
                            marks.tall_part_height
                        ),
                    ),
                    None => (
                        marks.min_test_point_body,
                        "(an \"absolute minimum\"; 1.0 mm is recommended)".to_string(),
                    ),
                };
                if g.d < least - EPS {
                    out.push(
                        Rule::TestPointBody,
                        format!(
                            "the test point {} is {:.4} mm from the body of {part}; the advice is at least {least:.2} mm {what}",
                            pad.part, g.d
                        ),
                        g.middle(),
                        Some(layer),
                        vec![
                            pad_item(pad),
                            Item {
                                what: format!("Footprint {part}"),
                                at: g.b,
                                part: Some(part.to_string()),
                                pad: None,
                                net: None,
                            },
                        ],
                    );
                }
            }
        }
    }
    if marks.min_test_point_pitch > 0.0 {
        for (k, (_, a, side_a)) in pads.iter().enumerate() {
            for (_, b, side_b) in &pads[k + 1..] {
                if side_a != side_b || a.part == b.part {
                    continue;
                }
                let d = a.at.dist(b.at);
                if d < marks.min_test_point_pitch - EPS {
                    out.push(
                        Rule::TestPointPitch,
                        format!(
                            "the test points {} and {} are {:.4} mm apart, centre to centre; the advice is at least {:.2} mm (an \"absolute minimum\"; 2.54 mm is recommended, and a fixture for less costs more)",
                            a.part, b.part, d, marks.min_test_point_pitch
                        ),
                        Pt::new((a.at.x + b.at.x) / 2.0, (a.at.y + b.at.y) / 2.0),
                        Some(layer_name(*side_a)),
                        vec![pad_item(a), pad_item(b)],
                    );
                }
            }
        }
    }
}

/// The smaller side of a pad.
fn pad_size(pad: &Pad) -> f64 {
    pad.size.0.min(pad.size.1)
}
