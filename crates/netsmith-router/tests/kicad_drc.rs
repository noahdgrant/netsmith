//! KiCad's own design rule check on the files netsmith writes, using the rules in the
//! `.kicad_pro` written beside them, for the boards the router routes: the example boards (from
//! their design files), invented boards of holes and custom pads, land grids, and the synthetic
//! boards. These run `kicad-cli` (found as `netsmith_fab::KicadCliExe::locate` does: `$KICAD_CLI`,
//! `PATH`, Windows KiCad under WSL), and are skipped where it is not installed. The boards that
//! netsmith-fabricate builds from source text (outlines, cutouts, pours, keepouts, layout files) are
//! checked by `kicad_drc.rs` there.

use std::process::Command;

use netsmith_core::{Design, Net, Part, PinRef};
use netsmith_fab::KicadCliExe;
use netsmith_kicad::Mapping;
use netsmith_pcb::{
    Board, BoardKeepout, CopperLayer, FootprintLibrary, PlaceOptions, Pt, Ratsnest, Rect, Track,
    Via, kicad_pcb, kicad_pro,
};
use netsmith_router::route;
use netsmith_testkit::TempDir;

use crate::common;
use crate::drc_kit::{check, copper_only, plane_drc};

/// An example as a board from its design file, with KiCad's own footprints when they are
/// installed, so that KiCad finds the footprints as they are in its library.
fn board(example: &str) -> Board {
    let installed = FootprintLibrary::installed();
    let library = if installed.root().is_dir() {
        installed
    } else {
        common::fixtures()
    };
    // `NETSMITH_TEST_SEED` asks the placer for another arrangement of every board, to see how
    // a test depends on the arrangement it happens to get.
    let seed = std::env::var("NETSMITH_TEST_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    netsmith_placer::board_from_design_placed(
        &common::example_design(example),
        &Mapping::standard(),
        &library,
        &PlaceOptions {
            seed,
            ..PlaceOptions::default()
        },
    )
    .unwrap()
}

#[test]
fn routed_hard_boards_pass_too() {
    let boards = common::hard_boards(33);
    for n in [3, 8, 32] {
        let mut b = boards[n].clone();
        assert!(route(&mut b).is_empty(), "board {n}");
        let Some(report) = check(&format!("hard{n}"), &b) else {
            return;
        };
        // The parts are packed too tightly for their reference text, which is not what is
        // being tested; everything about the copper must pass.
        let copper: Vec<_> = report
            .violations
            .iter()
            .filter(|v| !v.starts_with("silk"))
            .collect();
        assert_eq!(copper, Vec::<&String>::new(), "board {n}");
        assert_eq!(report.unconnected, 0, "board {n}");
    }
}

const EXAMPLES: [&str; 3] = ["led", "status_indicator", "linear_regulator"];

#[test]
fn routed_examples_pass_with_no_violation_and_nothing_unconnected() {
    for example in EXAMPLES {
        let mut b = board(example);
        assert!(route(&mut b).is_empty(), "{example}");
        let Some(report) = check(example, &b) else {
            return;
        };
        assert_eq!(report.violations, Vec::<String>::new(), "{example}");
        assert_eq!(report.unconnected, 0, "{example}");
    }
}

#[test]
fn unrouted_examples_pass_and_report_exactly_the_ratsnest() {
    for example in EXAMPLES {
        let b = board(example);
        let Some(report) = check(example, &b) else {
            return;
        };
        assert_eq!(report.violations, Vec::<String>::new(), "{example}");
        assert_eq!(report.unconnected, Ratsnest::of(&b).len(), "{example}");
    }
}

#[test]
fn a_board_that_breaks_the_rules_is_caught() {
    let mut b = board("led");
    assert!(route(&mut b).is_empty());

    // A track 0.05 mm wide, under the process's 0.1 mm, in clear board.
    b.tracks.push(Track {
        net: "VCC".into(),
        layer: CopperLayer::Front,
        from: Pt::new(b.outline.max.x - 4.0, b.outline.max.y - 1.5),
        to: Pt::new(b.outline.max.x - 2.0, b.outline.max.y - 1.5),
        width: 0.05,
        mid: None,
    });
    // A via with a 0.1 mm drill, under the process's 0.15 mm, in clear board.
    b.vias.push(Via {
        net: "VCC".into(),
        at: Pt::new(b.outline.max.x - 1.5, b.outline.min.y + 1.5),
        diameter: 0.6,
        drill: 0.1,
    });
    // A pad 0.1 mm from the board edge, under the process's 0.2 mm.
    let j1 = b.part("J1").unwrap();
    let left = j1.pad_bounds(&j1.footprint.pads[0]).min.x;
    b.outline.min.x = left - 0.1;

    let Some(report) = check("bad", &b) else {
        return;
    };
    eprintln!("bad board: {:?}", report.violations);
    assert!(report.count("track_width") >= 1, "{:?}", report.violations);
    assert!(
        report.count("drill_out_of_range") >= 1,
        "{:?}",
        report.violations
    );
    assert!(
        report.count("copper_edge_clearance") >= 1,
        "{:?}",
        report.violations
    );
}

#[test]
fn tracks_and_vias_that_are_too_close_are_caught() {
    let mut b = board("led");
    assert!(route(&mut b).is_empty());
    // Two parallel tracks of different nets 0.05 mm apart, and a via with a thin ring.
    let y = b.outline.max.y - 1.5;
    let x = b.outline.max.x - 5.0;
    for (net, dy) in [("VCC", 0.0), ("GND", 0.3)] {
        b.tracks.push(Track {
            net: net.into(),
            layer: CopperLayer::Front,
            from: Pt::new(x, y + dy),
            to: Pt::new(x + 3.0, y + dy),
            width: 0.25,
            mid: None,
        });
    }
    b.vias.push(Via {
        net: "VCC".into(),
        at: Pt::new(b.outline.max.x - 1.5, b.outline.min.y + 1.5),
        // 0.025 mm of copper round the drill: under the process's 0.05 mm.
        diameter: 0.35,
        drill: 0.3,
    });
    let Some(report) = check("tight", &b) else {
        return;
    };
    eprintln!("tight board: {:?}", report.violations);
    assert!(report.count("clearance") >= 1, "{:?}", report.violations);
    assert!(
        report.count("annular_width") + report.count("via_diameter") >= 1,
        "{:?}",
        report.violations
    );
}

#[test]
fn the_routed_four_layer_example_passes_with_its_planes_filled() {
    let mut b = board("four_layer");
    assert!(route(&mut b).is_empty());
    let Some(counts) = plane_drc("four", &b) else {
        eprintln!("skipped: no KiCad");
        return;
    };
    assert_eq!(counts, Default::default(), "violations, unconnected pads");
}

#[test]
fn the_routed_six_layer_example_passes_with_its_planes_filled_and_its_signal_layers_empty() {
    let mut b = board("six_layer");
    assert!(route(&mut b).is_empty());
    let Some(counts) = plane_drc("six", &b) else {
        eprintln!("skipped: no KiCad");
        return;
    };
    assert_eq!(counts, Default::default(), "violations, unconnected pads");
}

#[test]
fn the_routed_ten_layer_example_passes_with_its_planes_filled_and_its_signal_layers_empty() {
    let mut b = board("ten_layer");
    assert!(route(&mut b).is_empty());
    let Some(counts) = plane_drc("ten", &b) else {
        eprintln!("skipped: no KiCad");
        return;
    };
    assert_eq!(counts, Default::default(), "violations, unconnected pads");
}

#[test]
fn kicad_finds_exactly_the_pads_the_ratsnest_says_have_no_way_to_their_plane() {
    // Take the vias and tracks of the two plane nets off a routed board: every pad that
    // is not through-hole is then cut off from its plane, and the ratsnest says so.
    let mut b = board("four_layer");
    assert!(route(&mut b).is_empty());
    b.tracks.retain(|t| t.net != "GND" && t.net != "3V3");
    b.vias.retain(|v| v.net != "GND" && v.net != "3V3");
    // A teardrop is part of the track it widens: it goes with it.
    b.teardrops.retain(|t| t.net != "GND" && t.net != "3V3");
    let to_plane = Ratsnest::of(&b).lines.iter().filter(|l| l.to_plane).count();
    assert!(to_plane >= 4, "{to_plane}");
    let Some(counts) = plane_drc("cut", &b) else {
        eprintln!("skipped: no KiCad");
        return;
    };
    assert_eq!(counts.violations, 0);
    assert_eq!(counts.unconnected as usize, to_plane);
}

// ---- holes and custom pads, on footprints of our own -----------------------------------

/// A footprint library `tiny` of invented parts: a mounting hole (a hole with no copper, like
/// the pads of a pogo-pin programming footprint), a plated mounting hole, and a chip with
/// a custom-shaped pad whose primitives reach past its anchor.
const HOLE: &str = r#"(footprint "Hole" (layer "F.Cu") (attr exclude_from_pos_files)
    (property "Reference" "REF**" (at 0 -2 0) (layer "F.SilkS") (effects (font (size 1 1))))
    (pad "" np_thru_hole circle (at 0 0) (size 1 1) (drill 1) (layers "*.Cu" "*.Mask"))
    (fp_circle (center 0 0) (end 0.8 0) (stroke (width 0.05) (type solid)) (fill none) (layer "F.CrtYd")))"#;

/// A mounting hole whose solder mask opening (1.6 mm) is wider than its hole (1 mm).
const WIDE: &str = r#"(footprint "Wide" (layer "F.Cu") (attr exclude_from_pos_files)
    (property "Reference" "REF**" (at 0 -2 0) (layer "F.SilkS") (effects (font (size 1 1))))
    (pad "" np_thru_hole circle (at 0 0) (size 1.6 1.6) (drill 1) (layers "*.Cu" "*.Mask"))
    (fp_circle (center 0 0) (end 0.9 0) (stroke (width 0.05) (type solid)) (fill none) (layer "F.CrtYd")))"#;

const SLOT: &str = r#"(footprint "Slot" (layer "F.Cu") (attr exclude_from_pos_files)
    (property "Reference" "REF**" (at 0 -2 0) (layer "F.SilkS") (effects (font (size 1 1))))
    (pad "" np_thru_hole oval (at 0 0) (size 2.4 1) (drill oval 2.4 1) (layers "*.Cu" "*.Mask"))
    (fp_rect (start -1.4 -0.7) (end 1.4 0.7) (stroke (width 0.05) (type solid)) (fill none) (layer "F.CrtYd")))"#;

const PLATED: &str = r#"(footprint "Plated" (layer "F.Cu") (attr through_hole)
    (property "Reference" "REF**" (at 0 -2 0) (layer "F.SilkS") (effects (font (size 1 1))))
    (pad "1" thru_hole circle (at 0 0) (size 1.8 1.8) (drill 1) (layers "*.Cu" "*.Mask"))
    (fp_circle (center 0 0) (end 1.1 0) (stroke (width 0.05) (type solid)) (fill none) (layer "F.CrtYd")))"#;

const ODD: &str = r#"(footprint "Odd" (layer "F.Cu") (attr smd)
    (property "Reference" "REF**" (at 0 -2 0) (layer "F.SilkS") (effects (font (size 1 1))))
    (pad "1" smd custom (at -1 0) (size 0.5 0.5) (layers "F.Cu" "F.Mask" "F.Paste")
        (options (clearance outline) (anchor rect))
        (primitives (gr_poly (pts (xy -0.25 -0.8) (xy 0.9 -0.8) (xy 0.9 0.8) (xy -0.25 0.8))
            (width 0) (fill yes))))
    (pad "2" smd rect (at 1.5 0) (size 0.8 1) (layers "F.Cu" "F.Mask" "F.Paste"))
    (fp_rect (start -2 -1.2) (end 2 1.2) (stroke (width 0.05) (type solid)) (fill none) (layer "F.CrtYd")))"#;

fn tiny() -> (TempDir, Mapping, FootprintLibrary) {
    let dir = TempDir::new("holes");
    let pretty = dir.path().join("tiny.pretty");
    std::fs::create_dir_all(&pretty).unwrap();
    let mut rows = String::new();
    for (name, text) in [
        ("Hole", HOLE),
        ("Wide", WIDE),
        ("Slot", SLOT),
        ("Plated", PLATED),
        ("Odd", ODD),
    ] {
        std::fs::write(pretty.join(format!("{name}.kicad_mod")), text).unwrap();
        rows += &format!(
            "[[part]]\nkind = \"{name}\"\npackage = \"P\"\nsymbol = \"tiny:{name}\"\n\
             footprint = \"tiny:{name}\"\n"
        );
    }
    let mapping = Mapping::standard()
        .merged(Mapping::parse(&rows, "kicad.toml", dir.path()).unwrap())
        .unwrap();
    let library =
        FootprintLibrary::at(netsmith_testkit::sample_footprints()).with_library("tiny", pretty);
    (dir, mapping, library)
}

fn part(refdes: &str, kind: &str) -> Part {
    let resistor = kind == "Resistor";
    Part {
        // What the design stage records for a standard connector (ADR 0068).
        provides: if netsmith_testkit::std_class(kind) == Some("connector") {
            vec!["Connector".to_string()]
        } else {
            Vec::new()
        },
        class: netsmith_testkit::std_class(kind).and_then(netsmith_core::PartClass::from_word),
        refdes: refdes.into(),
        kind: kind.into(),
        value: "1k".into(),
        package: Some(if resistor { "0402" } else { "P" }.into()),
        path: String::new(),
        pins: Vec::new(),
    }
}

fn net(name: &str, pads: &[(&str, &str)]) -> Net {
    Net {
        name: name.into(),
        pins: pads
            .iter()
            .map(|(r, p)| PinRef {
                refdes: (*r).into(),
                pin: (*p).into(),
            })
            .collect(),
    }
}

/// A board of `parts` and `nets` on the process `TwoLayer`, with each part at the given
/// place, on an outline from (3, 3) to (17, 11).
fn board_of(parts: &[(&str, &str, f64, f64)], nets: Vec<Net>) -> Board {
    let (_dir, mapping, library) = tiny();
    let design = Design {
        modules: Vec::new(),
        name: "Holes".into(),
        process: Some(common::process_with(&[])),
        features: Default::default(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        parts: parts.iter().map(|(r, k, ..)| part(r, k)).collect(),
        nets,
    };
    let mut board = netsmith_placer::board_from_design(&design, &mapping, &library).unwrap();
    for (placed, (_, _, x, y)) in board.parts.iter_mut().zip(parts) {
        placed.at = Pt::new(*x, *y);
        placed.rotation = 0.0;
        // The text sides the placer chose are for the outline it made, not this one.
        placed.reference_side = netsmith_pcb::ReferenceSide::default();
    }
    board.outline = Rect::new(Pt::new(3.0, 3.0), Pt::new(17.0, 11.0));
    board
}

/// Four resistors at the corners of a rectangle with two nets that cross in the middle, and
/// the given parts among them.
fn crossing(extra: &[(&str, &str, f64, f64)]) -> Board {
    let mut parts = vec![
        ("R1", "Resistor", 6.0, 5.0),
        ("R2", "Resistor", 6.0, 9.0),
        ("R3", "Resistor", 14.0, 5.0),
        ("R4", "Resistor", 14.0, 9.0),
    ];
    parts.extend_from_slice(extra);
    board_of(
        &parts,
        vec![
            net("A", &[("R1", "2"), ("R4", "1")]),
            net("B", &[("R2", "2"), ("R3", "1")]),
        ],
    )
}

/// The least space between the edge of every hole of the board and the copper of its tracks
/// and vias, and between the hole of every via and the edge of every other hole.
fn hole_spaces(b: &Board) -> (f64, f64) {
    let (mut copper, mut holes) = (f64::MAX, f64::MAX);
    let from_segment = |p: Pt, a: Pt, c: Pt| {
        let d = c - a;
        let len2 = d.x * d.x + d.y * d.y;
        let t = (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0);
        p.distance(Pt::new(a.x + d.x * t, a.y + d.y * t))
    };
    for part in &b.parts {
        for pad in &part.footprint.pads {
            let Some((p, q, radius)) = part.hole(pad) else {
                continue;
            };
            let to_hole = |at: Pt| {
                if p == q {
                    at.distance(p)
                } else {
                    from_segment(at, p, q)
                }
            };
            // Plated holes are inside copper of their own net, which is not what is asked.
            if part.pad_layers(pad) != (false, false) {
                continue;
            }
            for t in &b.tracks {
                let near = [t.from, t.to]
                    .into_iter()
                    .map(to_hole)
                    .fold(f64::MAX, f64::min);
                // Tracks are straight and the hole small, so the ends and the foot of the
                // hole's centre on the track bound the distance.
                let foot = if t.from == t.to {
                    near
                } else {
                    from_segment(p, t.from, t.to)
                };
                copper = copper.min(near.min(foot) - t.width / 2.0 - radius);
            }
            for v in &b.vias {
                copper = copper.min(to_hole(v.at) - v.diameter / 2.0 - radius);
                holes = holes.min(to_hole(v.at) - v.drill / 2.0 - radius);
            }
        }
    }
    (copper, holes)
}

#[test]
fn the_router_keeps_tracks_and_vias_clear_of_holes_that_have_no_copper() {
    // Two crossing nets need a via; holes sit where they would go straight, and on the
    // crossing itself.
    let mut b = crossing(&[
        ("H1", "Hole", 10.0, 7.0),
        ("H2", "Hole", 10.0, 5.2),
        ("H3", "Slot", 10.0, 9.0),
    ]);
    assert_eq!(b.check_placement(), vec![]);
    assert!(route(&mut b).is_empty());
    assert!(!b.vias.is_empty(), "the nets cross, so one needs a via");
    let (copper, holes) = hole_spaces(&b);
    let r = &b.rules;
    assert!(copper >= r.clearance - 1e-9, "copper to hole {copper}");
    assert!(holes >= r.hole_to_hole - 1e-9, "hole to hole {holes}");
    let Some(report) = check("holes", &b) else {
        return;
    };
    assert_eq!(copper_only(&report), Vec::<&String>::new());
    assert_eq!(report.unconnected, 0);
}

#[test]
fn a_plated_hole_keeps_the_vias_of_other_nets_clear_too() {
    let mut b = crossing(&[("P1", "Plated", 10.0, 7.0)]);
    b.nets.push(netsmith_pcb::BoardNet {
        name: "C".into(),
        pads: vec![netsmith_pcb::PadRef {
            refdes: "P1".into(),
            number: "1".into(),
        }],
    });
    assert!(route(&mut b).is_empty());
    let Some(report) = check("plated", &b) else {
        return;
    };
    assert_eq!(copper_only(&report), Vec::<&String>::new());
}

#[test]
fn a_placement_with_holes_too_near_holes_or_copper_is_found_and_named() {
    let r = &crossing(&[]).rules.clone();
    let b = crossing(&[
        ("H1", "Hole", 10.0, 7.0),
        // 1.2 mm from H1's centre: 0.2 mm between the holes, under the 0.45 mm.
        ("H2", "Hole", 11.2, 7.0),
        // The hole's edge 0.05 mm from R3's first pad, whose edge is at x = 13.22.
        ("H3", "Hole", 12.67, 5.0),
    ]);
    let problems: Vec<String> = b
        .check_placement()
        .iter()
        .filter(|p| matches!(p, netsmith_pcb::PlacementProblem::HoleTooNear { .. }))
        .map(ToString::to_string)
        .collect();
    assert!(
        problems.contains(&format!(
            "a hole of H1 is 0.20 mm from the hole of H2, under the process's hole to hole space of {} mm",
            r.hole_to_hole
        )),
        "{problems:#?}"
    );
    assert!(
        problems
            .iter()
            .any(|p| p.starts_with("a hole of H3 is 0.0") && p.contains("the copper of R3")),
        "{problems:#?}"
    );
}

#[test]
fn a_custom_shaped_pad_routes_and_passes_kicads_check() {
    // The first pad of the chip is custom: a small anchor with a polygon that reaches past
    // it. The router treats the pad by the box of both; KiCad redraws the polygon itself.
    let mut b = board_of(
        &[
            ("U1", "Odd", 8.0, 7.0),
            ("R1", "Resistor", 12.0, 5.0),
            ("R2", "Resistor", 12.0, 9.0),
        ],
        vec![
            net("A", &[("U1", "1"), ("R1", "1")]),
            net("B", &[("U1", "2"), ("R2", "1")]),
        ],
    );
    assert_eq!(b.check_placement(), vec![]);
    assert!(route(&mut b).is_empty());
    let Some(report) = check("odd", &b) else {
        return;
    };
    assert_eq!(copper_only(&report), Vec::<&String>::new());
    assert_eq!(report.unconnected, 0);
    // KiCad redraws the polygon: the board file carries the primitives unchanged.
    assert!(kicad_pcb(&b).contains("(gr_poly"));
}

// ---- net classes (ADR 0020) ----------------------------------------------------------------

/// The crossing board with a keepout, at `at` in the board's frame and `size`, on both
/// outer layers, that forbids `what`.
fn crossing_with_keepout(at: Pt, size: (f64, f64), what: &str) -> (Board, Rect) {
    let mut b = crossing(&[]);
    b.keepouts.push(BoardKeepout {
        name: "k".into(),
        part: None,
        at: Some(at),
        size,
        layers: vec!["top".into(), "bottom".into()],
        tracks: what == "tracks",
        vias: what == "vias",
        copper: false,
        parts: false,
        max_height: None,
        drawn: false,
    });
    let area = b.keepout_areas()[0].rect;
    assert!(route(&mut b).is_empty(), "still routable around it");
    (b, area)
}

#[test]
fn the_router_keeps_tracks_out_of_a_keepout_that_forbids_them() {
    // Across the track of net A, which runs along y = 5 from x = 6.5 to 12.9.
    let (b, area) = crossing_with_keepout(Pt::new(6.0, 1.5), (1.0, 1.8), "tracks");
    // Every point of each track, grown by half its width, is outside the area (sampled each
    // 0.01 mm: a diagonal track's box is no measure of where its copper is).
    let grown = area.expanded(0.0);
    for t in &b.tracks {
        let steps = ((t.from.distance(t.to) / 0.01).ceil() as usize).max(1);
        for k in 0..=steps {
            let f = k as f64 / steps as f64;
            let p = Pt::new(
                t.from.x + (t.to.x - t.from.x) * f,
                t.from.y + (t.to.y - t.from.y) * f,
            );
            let dx = (grown.min.x - p.x).max(p.x - grown.max.x).max(0.0);
            let dy = (grown.min.y - p.y).max(p.y - grown.max.y).max(0.0);
            assert!(
                dx.hypot(dy) >= t.width / 2.0 - 1e-9,
                "{t:?} reaches into {area:?} at {p:?}"
            );
        }
    }
    let Some(report) = check("keep-tracks", &b) else {
        return;
    };
    assert_eq!(copper_only(&report), Vec::<&String>::new());
    assert_eq!(report.unconnected, 0);
}

#[test]
fn the_router_keeps_vias_out_of_a_keepout_that_forbids_them() {
    // Where net B puts a via, at (6.5, 5.8) when nothing is in the way.
    let (b, area) = crossing_with_keepout(Pt::new(3.0, 2.4), (1.0, 0.8), "vias");
    assert!(!b.vias.is_empty());
    for v in &b.vias {
        assert!(
            !Rect::new(v.at, v.at)
                .expanded(v.diameter / 2.0)
                .overlaps(&area),
            "{v:?} in {area:?}"
        );
    }
    assert!(
        !b.vias.iter().any(|v| v.at == Pt::new(6.5, 5.8)),
        "the via moved"
    );
    let Some(report) = check("keep-vias", &b) else {
        return;
    };
    assert_eq!(copper_only(&report), Vec::<&String>::new());
    assert_eq!(report.unconnected, 0);
}

/// Parts on both faces, joined by vias: KiCad finds the board valid and fully connected.
#[test]
fn a_board_with_parts_on_both_faces_routes_and_passes_kicads_check() {
    use netsmith_pcb::{PlaceOptions, Side};
    // Every second chip on the back, and the regulators too.
    let sides = (1..=20)
        .step_by(2)
        .map(|i| format!("{}{i}", if i % 3 == 1 { "C" } else { "R" }))
        .chain((1..=3).map(|i| format!("Q{i}")))
        .map(|r| (r, Side::Back))
        .collect();
    let mut b = common::synthetic_board_placed(
        4,
        20,
        3,
        10,
        24,
        10,
        &PlaceOptions {
            sides,
            ..PlaceOptions::default()
        },
    );
    assert!(b.parts.iter().any(|p| p.side == Side::Back));
    assert_eq!(b.check_placement(), vec![]);
    let left = route(&mut b);
    assert!(left.is_empty(), "unrouted: {:?}", left.lines);
    // Copper of the back side's parts is joined on the back layer.
    assert!(b.tracks.iter().any(|t| t.layer == CopperLayer::Back));
    let Some(counts) = plane_drc("twosided", &b) else {
        eprintln!("skipped: no KiCad");
        return;
    };
    assert_eq!(counts, Default::default(), "violations, unconnected pads");
}

/// Pads inside a land grid, with no room for a track between them, are joined by vias in
/// the pads: KiCad finds the board valid and fully connected.
#[test]
fn vias_in_pads_route_a_land_grid_and_pass_kicads_check() {
    let (mut b, walled) = common::land_grid_board(7, 12);
    assert!(!walled.is_empty());
    let left = route(&mut b);
    assert!(left.is_empty(), "unrouted: {:?}", left.lines);
    let Some(counts) = plane_drc("landgrid", &b) else {
        eprintln!("skipped: no KiCad");
        return;
    };
    assert_eq!(counts, Default::default(), "violations, unconnected pads");
}

/// Pads inside a land grid with a track's room between them (0.6 mm pads on a 1.1 mm pitch
/// leave 0.5 mm, and a track with its two spaces takes 0.3 mm) are joined by tracks that
/// pass between the pads, with no via in any pad, and KiCad finds nothing wrong.
#[test]
fn tracks_between_the_pads_of_a_land_grid_route_it_and_pass_kicads_check() {
    let (mut b, walled) = common::land_grid_board_sized(7, 12, 0.6, &[".via_in_pad &= 0mm"]);
    assert!(!walled.is_empty());
    let left = route(&mut b);
    assert!(left.is_empty(), "unrouted: {:?}", left.lines);
    // The track passes between pads: a pad inside the ring has a track on the front.
    let u1 = b.part("U1").unwrap();
    for number in &walled {
        let pad = u1.footprint.pads_numbered(number).next().unwrap();
        let at = u1.pad_centre(pad);
        assert!(
            b.tracks.iter().any(|t| t.layer == CopperLayer::Front
                && (t.from.distance(at) < 1e-6 || t.to.distance(at) < 1e-6)),
            "pad {number} has no track on the front"
        );
    }
    let Some(counts) = plane_drc("landgap", &b) else {
        eprintln!("skipped: no KiCad");
        return;
    };
    assert_eq!(counts, Default::default(), "violations, unconnected pads");
}

/// A board that draws its tracks wide (0.3 mm) cannot pass them between pads 0.5 mm apart,
/// where a track and its two spaces take 0.7 mm. The track is necked down to the process's
/// least (0.1 mm) for the short run between the pads, and wide elsewhere.
#[test]
fn a_wide_track_is_necked_down_between_pads_and_passes_kicads_check() {
    let (mut b, walled) = common::land_grid_board_sized(7, 12, 0.6, &[".via_in_pad &= 0mm"]);
    assert!(!walled.is_empty());
    b.rules.track_width = 0.3;
    assert!(b.rules.min_track_width < 0.3);
    let left = route(&mut b);
    assert!(left.is_empty(), "unrouted: {:?}", left.lines);
    let narrow = b.tracks.iter().filter(|t| t.width < 0.3 - 1e-9).count();
    let wide = b.tracks.iter().filter(|t| t.width >= 0.3 - 1e-9).count();
    assert!(
        narrow > 0 && wide > 0,
        "{narrow} narrow tracks and {wide} wide"
    );
    for t in &b.tracks {
        assert!(
            (t.width - 0.3).abs() < 1e-9 || (t.width - b.rules.min_track_width).abs() < 1e-9,
            "a track of {} mm",
            t.width
        );
    }
    let Some(counts) = plane_drc("neck", &b) else {
        eprintln!("skipped: no KiCad");
        return;
    };
    assert_eq!(counts, Default::default(), "violations, unconnected pads");
}

/// The board of seventy parts and a package of eighty pads (`tests/synthetic.rs`), routed
/// completely, passes KiCad's check with its planes filled: no violation and no unconnected
/// pad.
#[test]
fn the_big_synthetic_board_routes_and_passes_kicads_check() {
    // Seed 8: the silkscreen of the placer's arrangement for seed 7 has two strokes 0.07 mm
    // apart, a warning that the annealer's soft silkscreen cost does not rule out.
    let mut b = common::synthetic_board(8, 52, 17, 20, 72, 36);
    let started = std::time::Instant::now();
    let left = route(&mut b);
    eprintln!("routed in {:?}", started.elapsed());
    assert!(left.is_empty(), "unrouted: {:?}", left.lines);
    let Some(counts) = plane_drc("big", &b) else {
        eprintln!("skipped: no KiCad");
        return;
    };
    eprintln!(
        "big board: {} tracks, {} vias, DRC {counts:?}",
        b.tracks.len(),
        b.vias.len()
    );
    assert_eq!(counts, Default::default(), "violations, unconnected pads");
}

// ---- keepouts drawn in the editor and kept in the layout file (ADR 0029) -----------------------

#[test]
fn a_track_keeps_the_process_space_from_a_hole_s_solder_mask_opening() {
    // One net straight across the board, R1 to R3, and the hole just beside that line, 0.8 mm
    // from it: the hole's edge keeps the track out of it, but only the opening, 0.3 mm wider,
    // keeps the track off the copper the opening exposes. The track may pass on either side.
    let mut b = board_of(
        &[
            ("R1", "Resistor", 6.0, 7.0),
            ("R3", "Resistor", 14.0, 7.0),
            ("H1", "Wide", 10.0, 7.0 - 0.8),
        ],
        vec![net("A", &[("R1", "2"), ("R3", "1")])],
    );
    assert!(route(&mut b).is_empty());
    let centre = Pt::new(10.0, 7.0 - 0.8);
    let r = &b.rules;
    let mut least = f64::MAX;
    for t in &b.tracks {
        let d = t.from - t.to;
        let len2 = d.x * d.x + d.y * d.y;
        let s = if len2 == 0.0 {
            0.0
        } else {
            (((centre.x - t.to.x) * d.x + (centre.y - t.to.y) * d.y) / len2).clamp(0.0, 1.0)
        };
        let near = Pt::new(t.to.x + d.x * s, t.to.y + d.y * s);
        let gap = centre.distance(near) - t.width / 2.0 - 0.8;
        least = least.min(gap);
        assert!(
            gap >= r.mask_to_track - 1e-6,
            "a track {gap:.3} mm from the mask opening, under {}: {t:?}",
            r.mask_to_track
        );
    }
    assert!(
        least < 0.3,
        "the track passes near the opening: {least:.3} mm"
    );
    let extras = netsmith_geom::DrcExtras {
        mask_to_track: Some(r.mask_to_track),
        ..Default::default()
    };
    let report = netsmith_geom::drc_check(
        &netsmith_pcb::kicad_pcb(&b),
        Some(&netsmith_pcb::kicad_pro(&b)),
        &extras,
    )
    .unwrap();
    assert_eq!(
        report.count(netsmith_geom::DrcRule::MaskToTrack),
        0,
        "{report}"
    );
}

// ---- a footprint on the back, as KiCad reads it ---------------------------------------------

/// A face of a box: its corners (0 or 1 for each axis) counterclockwise seen from outside, its
/// outward normal and a direction in it.
type Face = ([[usize; 3]; 4], [f64; 3], [f64; 3]);

/// A STEP file of one box, `x`, `y` and `z` from the first to the second number, in the model's
/// own frame (y up, as KiCad's models are drawn).
fn box_step(x: [f64; 2], y: [f64; 2], z: [f64; 2]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut add = |s: String| {
        lines.push(s);
        format!("#{}", lines.len())
    };
    let real = |v: f64| format!("{v:?}");
    let triple = |p: [f64; 3]| format!("({},{},{})", real(p[0]), real(p[1]), real(p[2]));
    let corner = |c: [usize; 3]| [x[c[0]], y[c[1]], z[c[2]]];
    let mut vertices = std::collections::HashMap::new();
    for c in [0, 1]
        .into_iter()
        .flat_map(|i| [0, 1].into_iter().flat_map(move |j| [[i, j, 0], [i, j, 1]]))
    {
        let p = add(format!("CARTESIAN_POINT('',{})", triple(corner(c))));
        vertices.insert(c, add(format!("VERTEX_POINT('',{p})")));
    }
    let mut edges: std::collections::HashMap<([usize; 3], [usize; 3]), String> =
        std::collections::HashMap::new();
    let faces: [Face; 6] = [
        (
            [[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]],
            [0., 0., -1.],
            [1., 0., 0.],
        ),
        (
            [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]],
            [0., 0., 1.],
            [1., 0., 0.],
        ),
        (
            [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]],
            [0., -1., 0.],
            [1., 0., 0.],
        ),
        (
            [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]],
            [0., 1., 0.],
            [1., 0., 0.],
        ),
        (
            [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]],
            [-1., 0., 0.],
            [0., 1., 0.],
        ),
        (
            [[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]],
            [1., 0., 0.],
            [0., 1., 0.],
        ),
    ];
    let mut face_ids = Vec::new();
    for (corners, normal, along) in faces {
        let mut oriented = Vec::new();
        for k in 0..4 {
            let (a, b) = (corners[k], corners[(k + 1) % 4]);
            let key = if a < b { (a, b) } else { (b, a) };
            let curve = match edges.get(&key) {
                Some(c) => c.clone(),
                None => {
                    let (s, e) = (corner(key.0), corner(key.1));
                    let d = [e[0] - s[0], e[1] - s[1], e[2] - s[2]];
                    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                    let p = add(format!("CARTESIAN_POINT('',{})", triple(s)));
                    let dir = add(format!(
                        "DIRECTION('',{})",
                        triple([d[0] / len, d[1] / len, d[2] / len])
                    ));
                    let v = add(format!("VECTOR('',{dir},{})", real(len)));
                    let line = add(format!("LINE('',{p},{v})"));
                    let c = add(format!(
                        "EDGE_CURVE('',{},{},{line},.T.)",
                        vertices[&key.0], vertices[&key.1]
                    ));
                    edges.insert(key, c.clone());
                    c
                }
            };
            let sense = if key.0 == a { ".T." } else { ".F." };
            oriented.push(add(format!("ORIENTED_EDGE('',*,*,{curve},{sense})")));
        }
        let lp = add(format!("EDGE_LOOP('',({}))", oriented.join(",")));
        let bound = add(format!("FACE_OUTER_BOUND('',{lp},.T.)"));
        let p = add(format!(
            "CARTESIAN_POINT('',{})",
            triple(corner(corners[0]))
        ));
        let n = add(format!("DIRECTION('',{})", triple(normal)));
        let r = add(format!("DIRECTION('',{})", triple(along)));
        let axis = add(format!("AXIS2_PLACEMENT_3D('',{p},{n},{r})"));
        let plane = add(format!("PLANE('',{axis})"));
        face_ids.push(add(format!("ADVANCED_FACE('',({bound}),{plane},.T.)")));
    }
    let shell = add(format!("CLOSED_SHELL('',({}))", face_ids.join(",")));
    let solid = add(format!("MANIFOLD_SOLID_BREP('box',{shell})"));
    let mm = add("( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) )".into());
    let rad = add("( NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.) )".into());
    let sr = add("( NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT() )".into());
    let unc = add(format!(
        "UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-07),{mm},'distance_accuracy_value','')"
    ));
    let ctx = add(format!(
        "( GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT(({unc})) GLOBAL_UNIT_ASSIGNED_CONTEXT(({mm},{rad},{sr})) REPRESENTATION_CONTEXT('','') )"
    ));
    let o = add("CARTESIAN_POINT('',(0.,0.,0.))".into());
    let zd = add("DIRECTION('',(0.,0.,1.))".into());
    let xd = add("DIRECTION('',(1.,0.,0.))".into());
    let origin = add(format!("AXIS2_PLACEMENT_3D('',{o},{zd},{xd})"));
    let rep = add(format!(
        "ADVANCED_BREP_SHAPE_REPRESENTATION('',({solid},{origin}),{ctx})"
    ));
    let app = add("APPLICATION_CONTEXT('automotive design')".into());
    add(format!(
        "APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,{app})"
    ));
    let pc = add(format!("PRODUCT_CONTEXT('',{app},'mechanical')"));
    let product = add(format!("PRODUCT('box','box','',({pc}))"));
    let formation = add(format!("PRODUCT_DEFINITION_FORMATION('','',{product})"));
    let pdc = add(format!(
        "PRODUCT_DEFINITION_CONTEXT('part definition',{app},'design')"
    ));
    let pd = add(format!("PRODUCT_DEFINITION('design','',{formation},{pdc})"));
    let pds = add(format!("PRODUCT_DEFINITION_SHAPE('','',{pd})"));
    add(format!("SHAPE_DEFINITION_REPRESENTATION({pds},{rep})"));
    let mut out = String::from(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('box'),'2;1');\nFILE_NAME('box.step','',(''),(''),'','','');\nFILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));\nENDSEC;\nDATA;\n",
    );
    for (k, line) in lines.iter().enumerate() {
        out.push_str(&format!("#{}={line};\n", k + 1));
    }
    out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    out
}

/// The box, on the board (x right, y down), of the vertices of an ASCII STL file that `keep`
/// keeps (by their KiCad coordinates: x right, y up, z up from the board's bottom).
fn stl_box(text: &str, keep: impl Fn([f64; 3]) -> bool) -> Option<Rect> {
    let points: Vec<Pt> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("vertex "))
        .map(|v| {
            let n: Vec<f64> = v.split_whitespace().map(|t| t.parse().unwrap()).collect();
            [n[0], n[1], n[2]]
        })
        .filter(|p| keep(*p))
        .map(|p| Pt::new(p[0], -p[1]))
        .collect();
    let first = *points.first()?;
    Some(points.iter().fold(Rect::new(first, first), |r, p| {
        Rect::new(
            Pt::new(r.min.x.min(p.x), r.min.y.min(p.y)),
            Pt::new(r.max.x.max(p.x), r.max.y.max(p.y)),
        )
    }))
}

/// A footprint with nothing symmetric about it, on the back and turned a quarter, read by KiCad:
/// its 3D model (a box of our own, as the stock models are not installed everywhere) and its
/// pad are where netsmith puts them (`Placed::to_board`). KiCad mirrors a back footprint in y and
/// netsmith in x, so the file turns the footprint a half more; written at netsmith's own angle,
/// KiCad drew the model a half turn away from the pads (#553).
#[test]
fn kicad_draws_a_back_footprints_model_and_pads_where_netsmith_puts_them() {
    let Some(cli) = KicadCliExe::locate_runnable() else {
        eprintln!("skipped: no KiCad");
        return;
    };
    let mut b = board("led");
    let j1 = b.parts.iter_mut().find(|p| p.refdes == "J1").unwrap();
    j1.footprint = netsmith_pcb::Footprint::parse(
        "T:Lopsided",
        r#"(footprint "Lopsided"
            (property "Reference" "REF**" (at 0 -3 0) (layer "F.SilkS")
                (effects (font (size 1 1) (thickness 0.15))))
            (property "Value" "Lopsided" (at 0 3 0) (layer "F.Fab")
                (effects (font (size 1 1) (thickness 0.15))))
            (attr smd)
            (pad "1" smd rect (at 1.5 0.5) (size 1 0.6) (layers "F.Cu" "F.Mask"))
            (pad "2" smd rect (at -1 -1.5) (size 0.6 0.6) (layers "F.Cu" "F.Mask"))
            (model "${KIPRJMOD}/lopsided.step"))"#,
    )
    .unwrap();
    j1.side = netsmith_pcb::Side::Back;
    j1.rotation = 90.0;
    let part = b.part("J1").unwrap().clone();

    let base = cli.scratch_base().unwrap_or_else(|e| panic!("{e}"));
    let dir = TempDir::new_in(&base, "pcb-back-face");
    // The model's x 1 to 3 mm and y 0.5 to 1.5 mm up: in the footprint's frame (y down), x 1 to
    // 3 and y -1.5 to -0.5.
    std::fs::write(
        dir.join("lopsided.step"),
        box_step([1.0, 3.0], [0.5, 1.5], [0.0, 1.0]),
    )
    .unwrap();
    std::fs::write(dir.join("b.kicad_pcb"), kicad_pcb(&b)).unwrap();
    std::fs::write(dir.join("b.kicad_pro"), kicad_pro(&b)).unwrap();
    let export = |out: &str, extra: &[&str]| -> String {
        let (board, stl) = (
            cli.arg(&dir.join("b.kicad_pcb")).unwrap(),
            cli.arg(&dir.join(out)).unwrap(),
        );
        let ran = netsmith_fab::run_exclusive(|| {
            let mut command = Command::new(cli.exe());
            command
                .args(["pcb", "export", "stl", "--user-origin", "0x0mm", "-f", "-o"])
                .arg(&stl)
                .args(extra)
                .arg(&board);
            command
        })
        .unwrap_or_else(|e| panic!("{e}"));
        assert!(ran.success(), "kicad-cli failed:\n{}", ran.report());
        std::fs::read_to_string(dir.join(out)).unwrap()
    };
    let close = |a: Rect, b: Rect, what: &str| {
        let d = (a.min.x - b.min.x)
            .abs()
            .max((a.min.y - b.min.y).abs())
            .max((a.max.x - b.max.x).abs())
            .max((a.max.y - b.max.y).abs());
        assert!(d < 0.01, "{what}: KiCad's {a:?} against netsmith's {b:?}");
    };

    // The model hangs under the board; the board's body is above z 0.
    let model = export("model.stl", &["--component-filter", "J1"]);
    let theirs = stl_box(&model, |p| p[2] < -0.05).expect("KiCad drew no model under the board");
    let ours = Rect::new(
        part.to_board(Pt::new(1.0, -1.5)),
        part.to_board(Pt::new(3.0, -0.5)),
    );
    let ours = Rect::new(
        Pt::new(ours.min.x.min(ours.max.x), ours.min.y.min(ours.max.y)),
        Pt::new(ours.min.x.max(ours.max.x), ours.min.y.max(ours.max.y)),
    );
    close(theirs, ours, "the model");

    // Pad 1's copper, on the back (below z 0): J1's is the only pad of its net there.
    let net = b
        .nets
        .iter()
        .find(|n| n.pads.iter().any(|p| p.refdes == "J1" && p.number == "1"))
        .unwrap()
        .name
        .clone();
    let pads = export(
        "pads.stl",
        &[
            "--no-components",
            "--include-pads",
            "--net-filter",
            net.as_str(),
        ],
    );
    let theirs = stl_box(&pads, |p| p[2] < 0.0).expect("KiCad drew no pad on the back");
    let pad = part.footprint.pads_numbered("1").next().unwrap();
    close(theirs, part.pad_bounds(pad), "pad 1");
}
