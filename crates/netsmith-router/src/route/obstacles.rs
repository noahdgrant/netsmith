//! The fixed copper of a board as obstacles.

use super::*;

pub(crate) fn initial_obstacles(board: &Board) -> Obstacles {
    let rules = &board.rules;
    let net_of: HashMap<(&str, &str), usize> = board
        .nets
        .iter()
        .enumerate()
        .flat_map(|(n, net)| {
            net.pads
                .iter()
                .map(move |p| ((p.refdes.as_str(), p.number.as_str()), n))
        })
        .collect();
    let mut list = Vec::new();
    let mut via_keepouts = Vec::new();
    for part in &board.parts {
        for pad in &part.footprint.pads {
            let (front, back) = part.pad_layers(pad);
            if !front && !back {
                // A hole with no copper (a mounting hole, a pogo-pin pad) still keeps
                // copper and other holes away from its edge.
                if let Some((a, b, radius)) = part.hole(pad) {
                    // The page's "NPTH to track" (issue #431).
                    list.push(Obstacle {
                        net: None,
                        layers: [true, true],
                        form: Form::Segment(a, b, radius),
                        clearance: rules.clearance.max(rules.hole_clearance) + MARGIN,
                    });
                    // A via's hole keeps `hole_to_hole` from this hole's edge.
                    via_keepouts.push((
                        Form::Segment(a, b, radius),
                        rules.hole_to_hole + rules.via_drill / 2.0 + MARGIN,
                    ));
                }
                // Its opening in the solder mask, which may be wider than the hole, exposes
                // copper: a track keeps the process's space from it (issue #431).
                let mask = mask_sides(part, pad);
                if mask != [false, false] && pad.width > 0.0 && pad.height > 0.0 {
                    let form = match pad.shape {
                        netsmith_pcb::PadShape::Circle => {
                            Form::Circle(part.pad_centre(pad), pad.width.min(pad.height) / 2.0)
                        }
                        _ => Form::Rect(part.pad_bounds(pad)),
                    };
                    list.push(Obstacle {
                        net: None,
                        layers: mask,
                        form,
                        clearance: rules.mask_to_track + MARGIN,
                    });
                }
                continue;
            }
            let bounds = part.pad_bounds(pad);
            let pad_net = net_of
                .get(&(part.refdes.as_str(), pad.number.as_str()))
                .copied();
            // A track keeps the page's "PTH to track" from a plated pad's hole, and "NPTH to
            // track" from a hole with copper round it that is not plated (issue #431); the pad's
            // copper and its clearance usually keep more.
            if let Some((a, b, radius)) = part.hole(pad) {
                let keep = if pad.kind == netsmith_pcb::PadKind::NpThruHole {
                    rules.hole_clearance
                } else {
                    rules.plated_hole_clearance
                };
                list.push(Obstacle {
                    net: pad_net,
                    layers: [true, true],
                    form: Form::Segment(a, b, radius),
                    clearance: keep + MARGIN,
                });
            }
            list.push(Obstacle {
                net: pad_net,
                layers: [front, back],
                form: Form::Rect(bounds),
                // The larger of the board's clearance and the pad's own, as the design
                // rules check applies it.
                clearance: rules.clearance.max(pad.clearance.unwrap_or(0.0)) + MARGIN,
            });
            // A via keeps off every pad, and keeps its hole clear of a pad's hole.
            let reach = match pad.drill {
                Some(drill) => {
                    // The via's hole keeps `hole_to_hole` from the pad's hole, measured from
                    // the pad's centre, which is at least half the shorter side from its edge.
                    let apart = drill.width.max(drill.height) / 2.0
                        + rules.hole_to_hole
                        + rules.via_drill / 2.0;
                    let inset = bounds.width().min(bounds.height()) / 2.0;
                    (apart - inset).max(rules.via_diameter / 2.0) + MARGIN
                }
                None => rules.via_diameter / 2.0 + PITCH / 2.0,
            };
            via_keepouts.push((Form::Rect(bounds), reach));
        }
    }
    // An opening the footprint draws in the solder mask exposes any copper in it, so a via
    // (whose own hole the mask may not cover) keeps the mask's least web from it.
    for part in &board.parts {
        for opening in part.mask_openings() {
            via_keepouts.push((
                Form::Rect(opening),
                rules.via_diameter / 2.0 + rules.mask_bridge + MARGIN,
            ));
        }
    }
    // Keepouts: tracks keep out of the areas that forbid them on a layer (and, because a
    // track's cells also bound vias here, vias keep out of them too), and vias of the
    // areas that forbid them.
    for k in board.keepout_areas() {
        let layers = [k.on(CopperLayer::Front), k.on(CopperLayer::Back)];
        if k.tracks && layers != [false, false] {
            list.push(Obstacle {
                net: None,
                layers,
                form: Form::Rect(k.rect),
                clearance: MARGIN,
            });
        }
        if k.vias {
            via_keepouts.push((Form::Rect(k.rect), rules.via_diameter / 2.0 + PITCH / 2.0));
        }
    }
    // The board edge: the outline (its rounded rectangle, or its polygon's lines and arcs as
    // short straight pieces) and the edge of each cutout.
    let edges = board.edge().segments().into_iter().chain(
        board
            .cutout_shapes()
            .into_iter()
            .flat_map(|(_, shape)| shape.segments()),
    );
    for (from, to) in edges {
        list.push(Obstacle {
            net: None,
            layers: [true, true],
            form: Form::Segment(from, to, 0.0),
            clearance: rules.copper_to_edge + MARGIN,
        });
    }
    Obstacles { list, via_keepouts }
}
