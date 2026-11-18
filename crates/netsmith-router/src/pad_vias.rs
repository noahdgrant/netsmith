//! The vias a datasheet asks for in a pin's pads (`vias NAME .pin N` in a part's `layout`
//! block, issue #994): an exposed pad or a tab that sinks its heat, or returns its current,
//! through vias to its net's other layers.
//!
//! After routing and before the pours are stitched, each such pin whose pads have fewer than
//! N vias of their net gets the rest, in its pads, when three things hold: the process fills
//! and caps a via in a pad (`.via_in_pad`; an open via wicks the solder away), the pin's net has
//! copper on another layer to reach (an inner plane, or the pour of the other outer layer), and
//! the pad has room. A via goes on a grid across the pad, kept a via's radius inside its copper
//! and the via to via space from the vias already there, rows from the top and points from the
//! left. A pad whose part or pad is turned by other than a right angle is left as it is: its box
//! is not its copper. A via keeps out of a keepout that forbids vias, a footprint's own among
//! them (issue #1291). What is not added is said by `fab`'s `layout` guideline, with what to do.

use netsmith_core::LayoutRule;
use netsmith_pcb::{Board, CopperLayer, Pt, Rect, Via};

/// Room kept beyond the rules' spaces, for rounding.
const MARGIN: f64 = 0.01;

/// Add the vias that the `vias` guidelines of the board's parts ask for. Returns how many
/// were added.
pub fn add(board: &mut Board) -> usize {
    let rules = board.rules.clone();
    if !rules.via_in_pad {
        return 0;
    }
    let radius = rules.via_diameter / 2.0;
    let apart = (rules.via_hole_to_hole + rules.via_drill).max(rules.via_diameter) + MARGIN;
    // A via keeps out of the keepouts that forbid vias, the design's and those of the parts'
    // footprints (a keepout a library draws over a pad, issue #1291).
    let keepouts: Vec<Rect> = board
        .copper_keepouts()
        .into_iter()
        .filter(|k| k.vias)
        .map(|k| k.rect)
        .collect();
    let in_keepout = |at: Pt| {
        keepouts.iter().any(|k| {
            let dx = (k.min.x - at.x).max(at.x - k.max.x).max(0.0);
            let dy = (k.min.y - at.y).max(at.y - k.max.y).max(0.0);
            dx.hypot(dy) < radius + MARGIN
        })
    };
    let mut added = 0;
    for i in 0..board.parts.len() {
        let wants: Vec<(Vec<String>, usize)> = board.parts[i]
            .layout
            .iter()
            .filter_map(|g| match g.rule.rule {
                LayoutRule::Vias { count, .. } => Some((g.pads[0].clone(), count)),
                _ => None,
            })
            .collect();
        for (numbers, count) in wants {
            let part = &board.parts[i];
            let Some(net) = board
                .nets
                .iter()
                .find(|n| {
                    n.pads
                        .iter()
                        .any(|p| p.refdes == part.refdes && numbers.contains(&p.number))
                })
                .map(|n| n.name.clone())
            else {
                continue;
            };
            if board.fixed.is_locked(&net) {
                continue;
            }
            let square = |a: f64| (a.rem_euclid(90.0)).abs() < 1e-9;
            let mut boxes: Vec<Rect> = Vec::new();
            let mut faces: Vec<CopperLayer> = Vec::new();
            for pad in numbers.iter().flat_map(|n| part.footprint.pads_numbered(n)) {
                let (front, back) = part.pad_layers(pad);
                if front == back || pad.drill.is_some() || pad.extent.is_some() {
                    continue;
                }
                if !square(part.rotation) || !square(pad.angle) {
                    continue;
                }
                boxes.push(part.pad_bounds(pad));
                faces.push(if front {
                    CopperLayer::Front
                } else {
                    CopperLayer::Back
                });
            }
            let reaches = |face: CopperLayer| {
                board.planes.iter().any(|p| p.net == net)
                    || board.pours.iter().any(|p| p.net == net && p.layer != face)
            };
            let on = |at: Pt| boxes.iter().any(|b| b.contains(&Rect::new(at, at)));
            let have = board
                .vias
                .iter()
                .filter(|v| v.net == net && on(v.at))
                .count();
            let mut need = count.saturating_sub(have);
            for (b, face) in boxes.iter().zip(&faces) {
                if need == 0 || !reaches(*face) {
                    continue;
                }
                let (w, h) = (b.width() - 2.0 * radius, b.height() - 2.0 * radius);
                if w < -1e-9 || h < -1e-9 {
                    continue;
                }
                // As many as fit across each side, spread evenly over it.
                let n_x = (w.max(0.0) / apart).floor() as usize + 1;
                let n_y = (h.max(0.0) / apart).floor() as usize + 1;
                let step = |span: f64, n: usize| if n > 1 { span / (n - 1) as f64 } else { 0.0 };
                let (sx, sy) = (step(w.max(0.0), n_x), step(h.max(0.0), n_y));
                let start = Pt::new(
                    b.min.x + radius + if n_x == 1 { w.max(0.0) / 2.0 } else { 0.0 },
                    b.min.y + radius + if n_y == 1 { h.max(0.0) / 2.0 } else { 0.0 },
                );
                for row in 0..n_y {
                    for col in 0..n_x {
                        if need == 0 {
                            break;
                        }
                        let at = Pt::new(start.x + sx * col as f64, start.y + sy * row as f64);
                        let clear = board.vias.iter().all(|v| {
                            let space = if v.net == net {
                                apart
                            } else {
                                rules.via_diameter + board.clearance_between(&v.net, &net) + MARGIN
                            };
                            v.at.distance(at) + 1e-9 >= space
                        });
                        if !clear || in_keepout(at) {
                            continue;
                        }
                        board.vias.push(Via {
                            net: net.clone(),
                            at,
                            diameter: rules.via_diameter,
                            drill: rules.via_drill,
                            span: None,
                        });
                        need -= 1;
                        added += 1;
                    }
                }
            }
        }
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testlib::{fixtures, test_process};
    use netsmith_core::{Design, LayoutGuideline, Net, Part, PartPin, PinRef};
    use netsmith_pcb::{BoardPour, PartGuideline};

    /// A regulator in SOT-223 whose tab (pad 2) is on GND, a bottom pour on GND when `poured`,
    /// and its datasheet's `vias tab .gnd 4`.
    fn board(poured: bool) -> Board {
        let pin = |name: &str, pad: &str| PartPin {
            name: name.into(),
            datasheet: name.to_uppercase(),
            kind: netsmith_core::PinKind::Power,
            pads: vec![pad.into()],
            function: None,
            power_flow: None,
        };
        let part = Part {
            provides: Vec::new(),
            declared_kind: Some(netsmith_core::PartKind::Ic),
            refdes: "U1".into(),
            kind: "LinearRegulator".into(),
            value: "3.3V".into(),
            package: Some("SOT-223".into()),
            path: "reg".into(),
            pins: vec![pin("gnd", "2"), pin("vin", "3")],
        };
        let rule = LayoutGuideline {
            part: "reg".into(),
            name: "tab".into(),
            source: "LD7, 10.2, p.15".into(),
            site: "ld7.nsm:30".into(),
            rule: LayoutRule::Vias {
                pin: "gnd".into(),
                count: 4,
            },
        };
        let design = Design {
            modules: Vec::new(),
            name: "T".into(),
            process: Some(test_process()),
            features: netsmith_core::Features {
                layout: vec![rule.clone()],
                ..Default::default()
            },
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts: vec![part.clone()],
            nets: vec![Net {
                name: "GND".into(),
                pins: vec![PinRef {
                    refdes: "U1".into(),
                    pin: "2".into(),
                }],
            }],
        };
        let mut board = netsmith_placer::board_from_design(
            &design,
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        board.parts[0].layout = PartGuideline::of_part(&part, &[rule]);
        if poured {
            board.pours.push(BoardPour {
                layer: CopperLayer::Back,
                net: "GND".into(),
                stitch_pitch: None,
                thermal_gap: None,
                spoke_width: None,
                clearance: None,
                min_width: None,
                pads: None,
            });
        }
        board
    }

    /// The vias of GND whose centre is on the tab.
    fn on_tab(board: &Board) -> Vec<Pt> {
        let part = &board.parts[0];
        let tab: Vec<Rect> = part
            .footprint
            .pads_numbered("2")
            .map(|p| part.pad_bounds(p))
            .collect();
        board
            .vias
            .iter()
            .filter(|v| v.net == "GND" && tab.iter().any(|b| b.contains(&Rect::new(v.at, v.at))))
            .map(|v| v.at)
            .collect()
    }

    #[test]
    fn a_pin_gets_the_vias_its_datasheet_asks_for_in_its_pads() {
        let mut b = board(true);
        assert!(b.rules.via_in_pad, "the process fills vias in pads");
        assert_eq!(add(&mut b), 4);
        let at = on_tab(&b);
        assert_eq!(at.len(), 4, "{at:?}");
        // Each two keep the via to via space.
        let r = &b.rules;
        let apart = (r.via_hole_to_hole + r.via_drill).max(r.via_diameter);
        for (k, p) in at.iter().enumerate() {
            for q in &at[k + 1..] {
                assert!(p.distance(*q) >= apart, "{p:?} and {q:?}");
            }
        }
        // A board that has them gets none more.
        assert_eq!(add(&mut b), 0);
        assert_eq!(on_tab(&b).len(), 4);
        // Its guideline is met.
        assert!(netsmith_pcb::layout_findings(&b).is_empty());
    }

    /// A keepout zone in U1's footprint, as a library draws one, over its pads numbered 2 that
    /// `which` picks (the pin, 1.5 mm tall, and the tab, 3.8 mm), in the footprint's frame,
    /// forbidding vias alone.
    fn keep_vias_off(b: &mut Board, which: fn(&netsmith_pcb::Pad) -> bool) {
        let mut pts = Vec::new();
        for pad in b.parts[0].footprint.pads_numbered("2").filter(|p| which(p)) {
            let (w, h) = (pad.width / 2.0 + 0.1, pad.height / 2.0 + 0.1);
            pts.extend([
                Pt::new(pad.at.x - w, pad.at.y - h),
                Pt::new(pad.at.x + w, pad.at.y + h),
            ]);
        }
        let r = Rect::around(pts).unwrap();
        let zone = netsmith_kicad::Sexp::parse(&format!(
            "(zone (net 0) (net_name \"\") (layers \"F&B.Cu\") (hatch full 0.508) (connect_pads (clearance 0)) (min_thickness 0.254) (keepout (tracks allowed) (vias not_allowed) (pads allowed) (copperpour allowed) (footprints allowed)) (fill (thermal_gap 0.5) (thermal_bridge_width 0.5)) (polygon (pts (xy {} {}) (xy {} {}) (xy {} {}) (xy {} {}))))",
            r.min.x, r.min.y, r.max.x, r.min.y, r.max.x, r.max.y, r.min.x, r.max.y
        ))
        .unwrap();
        match &mut b.parts[0].footprint.source {
            netsmith_kicad::Sexp::List(items) => items.push(zone),
            _ => unreachable!(),
        }
    }

    #[test]
    fn no_via_goes_into_a_keepout_the_footprint_carries_over_its_pads() {
        // A keepout over both pads of the pin: no via (issue #1291).
        let mut b = board(true);
        keep_vias_off(&mut b, |_| true);
        assert_eq!(b.footprint_keepouts().len(), 1);
        assert_eq!(add(&mut b), 0);
        // Over the small pad alone: the vias go in the tab, none in the keepout.
        let mut b = board(true);
        keep_vias_off(&mut b, |p| p.height < 2.0);
        assert_eq!(add(&mut b), 4);
        let keep = b.footprint_keepouts()[0].rect;
        let radius = b.rules.via_diameter / 2.0;
        for v in &b.vias {
            assert!(
                !Rect::new(v.at, v.at).expanded(radius).overlaps(&keep),
                "the via at {:?} is in the keepout {keep:?}",
                v.at
            );
        }
    }

    #[test]
    fn no_via_goes_where_it_reaches_nothing_or_the_process_leaves_it_open() {
        let mut b = board(false);
        assert_eq!(add(&mut b), 0, "no other copper of GND to reach");
        let found = netsmith_pcb::layout_findings(&b);
        assert!(
            found.is_empty(),
            "a board with no copper is not judged: {found:?}"
        );
        let mut b = board(true);
        b.rules.via_in_pad = false;
        assert_eq!(add(&mut b), 0, "an open via in a pad wicks the solder");
    }
}
