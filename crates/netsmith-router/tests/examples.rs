//! The example boards routed, from their design files: the copper clears every pad, track and the
//! edge, the rules reach the router and the project file, and the planes of the layered examples
//! are written. (The placer's `examples` hold the boards before they are routed.)

use netsmith_pcb::{Board, CopperLayer, Pt, Ratsnest, Rect, Track, kicad_pcb, svg};
use netsmith_router::route;

use crate::common;

use common::{edge_gap, example_board as board, fixtures};

const EXAMPLES: [&str; 3] = ["led", "status_indicator", "linear_regulator"];

/// Points along a track, close enough together to find its nearest approach to a shape.
fn samples(t: &Track) -> Vec<Pt> {
    let steps = (t.from.distance(t.to) / 0.02).ceil().max(1.0) as usize;
    (0..=steps)
        .map(|k| {
            let f = k as f64 / steps as f64;
            Pt::new(
                t.from.x + (t.to.x - t.from.x) * f,
                t.from.y + (t.to.y - t.from.y) * f,
            )
        })
        .collect()
}

fn distance_to_rect(p: Pt, r: &Rect) -> f64 {
    let dx = (r.min.x - p.x).max(p.x - r.max.x).max(0.0);
    let dy = (r.min.y - p.y).max(p.y - r.max.y).max(0.0);
    dx.hypot(dy)
}

fn distance_to_track(p: Pt, t: &Track) -> f64 {
    samples(t)
        .iter()
        .map(|q| p.distance(*q))
        .fold(f64::MAX, f64::min)
}

#[test]
fn placement_leaves_the_copper_to_edge_distance_round_every_pad_and_track() {
    for example in EXAMPLES {
        let mut board = board(example, &fixtures());
        let wanted = board.rules.copper_to_edge;
        let placed = edge_gap(&board);
        assert!(
            placed >= wanted,
            "{example}: pads {placed:.2} mm from the edge"
        );
        route(&mut board);
        let routed = edge_gap(&board);
        assert!(
            routed >= wanted,
            "{example}: copper {routed:.2} mm from the edge"
        );
    }
}

#[test]
fn each_example_routes_completely_and_cleanly() {
    for example in EXAMPLES {
        let mut board = board(example, &fixtures());
        let left = route(&mut board);
        assert!(left.is_empty(), "{example}: {} lines unrouted", left.len());
        assert!(!board.tracks.is_empty(), "{example}");
        assert_eq!(Ratsnest::of(&board).len(), 0, "{example}");

        for t in &board.tracks {
            let layer_has_copper = |pad_front: bool, pad_back: bool| match t.layer {
                CopperLayer::Front => pad_front,
                CopperLayer::Back => pad_back,
            };
            for p in samples(t) {
                assert!(
                    board.outline.contains(&Rect::new(p, p)),
                    "{example}: track leaves the board"
                );
            }
            // Clear of every pad of another net on the same layer.
            for part in &board.parts {
                for pad in &part.footprint.pads {
                    let (front, back) = part.pad_layers(pad);
                    let own = board
                        .net_of(&part.refdes, &pad.number)
                        .is_some_and(|n| n.name == t.net);
                    if own || !layer_has_copper(front, back) {
                        continue;
                    }
                    let bounds = part.pad_bounds(pad);
                    for p in samples(t) {
                        let d = distance_to_rect(p, &bounds) - t.width / 2.0;
                        assert!(
                            d >= board.rules.clearance - 1e-9,
                            "{example}: track of {} is {d:.3} mm from pad {}.{}",
                            t.net,
                            part.refdes,
                            pad.number
                        );
                    }
                }
            }
        }
        // Clear of every track of another net on the same layer.
        for (i, a) in board.tracks.iter().enumerate() {
            for b in &board.tracks[i + 1..] {
                if a.net == b.net || a.layer != b.layer {
                    continue;
                }
                let d = samples(a)
                    .iter()
                    .map(|&p| distance_to_track(p, b))
                    .fold(f64::MAX, f64::min)
                    - (a.width + b.width) / 2.0;
                assert!(
                    d >= board.rules.clearance - 1e-9,
                    "{example}: tracks {d:.3} mm apart"
                );
            }
        }
    }
}

#[test]
fn routing_is_the_same_every_time_and_reaches_the_file() {
    let routed = |example| {
        let mut b = board(example, &fixtures());
        route(&mut b);
        b
    };
    for example in EXAMPLES {
        let (a, b) = (routed(example), routed(example));
        assert_eq!(a, b, "{example}");
        let text = kicad_pcb(&a);
        assert_eq!(text, kicad_pcb(&b), "{example}");
        // A straight track is a segment, a rounded bend an arc.
        let arcs = a.tracks.iter().filter(|t| t.mid.is_some()).count();
        assert_eq!(
            text.matches("(segment").count(),
            a.tracks.len() - arcs,
            "{example}"
        );
        assert_eq!(text.matches("(arc").count(), arcs, "{example}");
        assert_eq!(text.matches("(via").count(), a.vias.len(), "{example}");
        assert_eq!(svg::render(&a), svg::render(&b), "{example}");
    }
}

/// The LED example with `lines` added to its `.process` block: its design, with the process the
/// design stage resolves for `TwoLayer` at 1.6 mm and those narrowings (`common::process_with`).
fn led_board_narrowed(lines: &[&str]) -> Board {
    let mut design = common::example_design("led");
    design.process = Some(common::process_with(lines));
    netsmith_placer::board_from_design(
        &design,
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
    )
    .unwrap()
}

#[test]
fn the_rules_come_from_the_source_and_reach_the_router_and_the_project_file() {
    let mut board = led_board_narrowed(&[]);
    assert_eq!(board.rules.min_track_width, 0.1);
    assert!(route(&mut board).is_empty());
    assert!(!board.tracks.is_empty());
    assert!(board.tracks.iter().all(|t| t.width == 0.1));
    assert!(
        board
            .vias
            .iter()
            .all(|v| v.drill == 0.2 && v.diameter == 0.45)
    );
    let pro = netsmith_pcb::kicad_pro(&board);
    assert!(pro.contains(r#""min_track_width": 0.1"#), "{pro}");
    assert!(pro.contains(r#""min_clearance": 0.1"#), "{pro}");
}

#[test]
fn narrowing_a_rule_in_the_source_changes_the_tracks_and_the_project_file() {
    let mut board = led_board_narrowed(&[
        ".min_track &= 0.3mm..1m",
        ".min_clearance &= 0.25mm..1m",
        ".min_drill &= 0.4mm..1m",
        // A hole that big needs a via of its own: the process's offer is 0.2 in 0.45 mm.
        ".via_drill &= 0.4mm",
        ".via_diameter &= 0.8mm",
    ]);
    assert!(route(&mut board).is_empty());
    assert!(board.tracks.iter().all(|t| t.width == 0.3));
    assert!(
        board
            .vias
            .iter()
            .all(|v| v.drill == 0.4 && v.diameter == 0.8)
    );
    let pro = netsmith_pcb::kicad_pro(&board);
    for line in [
        r#""min_track_width": 0.3"#,
        r#""min_clearance": 0.25"#,
        r#""min_through_hole_diameter": 0.4"#,
    ] {
        assert!(pro.contains(line), "missing {line}\n{pro}");
    }
}

#[test]
fn the_board_file_takes_its_thickness_from_the_process() {
    let board = led_board_narrowed(&[]);
    let pcb = kicad_pcb(&board);
    assert!(pcb.contains("(thickness 1.6)"), "{pcb}");
    assert!(
        pcb.contains("(thickness 1.51)"),
        "the core is what is left of 1.6 mm"
    );
}

#[test]
fn a_board_with_no_process_cannot_become_a_pcb() {
    let mut design = common::example_design("led");
    design.process = None;
    let err = netsmith_placer::board_from_design(
        &design,
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
    )
    .unwrap_err();
    assert!(err.to_string().contains(".process: TwoLayer"), "{err}");
}

#[test]
fn the_four_layer_example_has_planes_and_routes_completely_on_the_outer_layers() {
    let mut board = board("four_layer", &fixtures());
    assert_eq!(board.planes.len(), 2);
    assert_eq!(board.check_placement(), vec![]);
    let before = Ratsnest::of(&board);
    // The plane nets' surface-mount pads each want a via; the through-hole pads do not.
    assert!(before.lines.iter().any(|l| l.to_plane));
    let left = route(&mut board);
    assert!(left.is_empty(), "{} lines unrouted", left.len());
    assert_eq!(Ratsnest::of(&board).len(), 0);
    assert!(edge_gap(&board) >= board.rules.copper_to_edge);
    let text = kicad_pcb(&board);
    // The two planes; the teardrops (zones KiCad 9 names `$teardrop_padvia$`) are not counted.
    assert_eq!(
        text.matches("(zone").count() - text.matches("$teardrop_padvia$").count(),
        2
    );
    assert!(text.contains("\"In1.Cu\"") && text.contains("\"In2.Cu\""));
    // The planes are drawn in the picture and listed in its key.
    let picture = svg::render(&board);
    assert!(picture.contains("In1.Cu: plane GND") && picture.contains("In2.Cu: plane 3V3"));
}

#[test]
fn the_ten_layer_example_has_four_planes_four_empty_signal_layers_and_routes_on_the_outer_layers() {
    let mut board = board("ten_layer", &fixtures());
    assert_eq!(board.planes.len(), 4);
    assert_eq!(board.inner_layers().len(), 8);
    assert_eq!(board.check_placement(), vec![]);
    let left = route(&mut board);
    assert!(left.is_empty(), "{} lines unrouted", left.len());
    assert_eq!(Ratsnest::of(&board).len(), 0);
    let text = kicad_pcb(&board);
    assert_eq!(
        text.matches("(zone").count() - text.matches("$teardrop_padvia$").count(),
        4
    );
    assert!(
        !text
            .lines()
            .any(|l| l.contains("(segment") && l.contains("\"In")),
        "a track on an inner layer"
    );
    assert_eq!(own_findings(&board), Vec::<String>::new());
}

#[test]
fn the_six_layer_example_has_two_planes_two_empty_signal_layers_and_routes_on_the_outer_layers() {
    let mut board = board("six_layer", &fixtures());
    assert_eq!(board.planes.len(), 2);
    assert_eq!(board.inner_layers().len(), 4);
    assert_eq!(board.check_placement(), vec![]);
    let left = route(&mut board);
    assert!(left.is_empty(), "{} lines unrouted", left.len());
    assert_eq!(Ratsnest::of(&board).len(), 0);
    let text = kicad_pcb(&board);
    // The two planes; the teardrops (zones KiCad 9 names `$teardrop_padvia$`) are not counted.
    assert_eq!(
        text.matches("(zone").count() - text.matches("$teardrop_padvia$").count(),
        2
    );
    // Every track is on an outer layer.
    assert!(
        !text
            .lines()
            .any(|l| l.contains("(segment") && l.contains("\"In")),
        "a track on an inner layer"
    );
    assert_eq!(own_findings(&board), Vec::<String>::new());
}

/// What netsmith's own design rules check (ADR 0037) finds on a routed board the placer made:
/// nothing, with no KiCad needed. `kicad_drc.rs` compares the two checks where KiCad is here.
fn own_findings(board: &Board) -> Vec<String> {
    let report = netsmith_fab::drc::check(
        &kicad_pcb(board),
        Some(&netsmith_pcb::kicad_pro(board)),
        &Default::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    report.errors().map(|f| f.to_string()).collect()
}

#[test]
fn each_routed_example_the_placer_makes_passes_netsmiths_own_design_rules_check() {
    for example in EXAMPLES {
        let mut b = board(example, &fixtures());
        assert!(route(&mut b).is_empty(), "{example}");
        assert_eq!(own_findings(&b), Vec::<String>::new(), "{example}");
    }
}
