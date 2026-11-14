//! Tests of the board.

use netsmith_core::{Net, Part, PinRef};

use super::*;

pub fn fixtures() -> FootprintLibrary {
    FootprintLibrary::at(netsmith_testkit::sample_footprints())
}

fn part(refdes: &str, kind: &str, value: &str, package: Option<&str>) -> Part {
    Part {
        // What the design stage records for a standard connector (ADR 0068).
        provides: if netsmith_testkit::std_kind(kind) == Some("connector") {
            vec!["Connector".to_string()]
        } else {
            Vec::new()
        },
        declared_kind: netsmith_testkit::std_kind(kind)
            .and_then(netsmith_core::PartKind::from_word),
        refdes: refdes.into(),
        kind: kind.into(),
        value: value.into(),
        package: package.map(str::to_string),
        path: String::new(),
        pins: Vec::new(),
    }
}

fn net(name: &str, pins: &[(&str, &str)]) -> Net {
    Net {
        name: name.into(),
        pins: pins
            .iter()
            .map(|(r, p)| PinRef {
                refdes: (*r).into(),
                pin: (*p).into(),
            })
            .collect(),
    }
}

/// The board `json` holds, a placed `Board` from `tests/fixtures`, which the placer
/// wrote (`netsmith-placer`'s `pcb_fixtures` test keeps them in step with it, and says how
/// to write them again). The tests of this crate start from a placed board without
/// depending on the placer (issue #608).
fn fixture(json: &str) -> Board {
    serde_json::from_str(json).expect("a fixture is the JSON of a Board")
}

/// `led_design` placed: the board most tests of this crate check.
pub fn led_board() -> Board {
    fixture(include_str!("../../tests/fixtures/led.board.json"))
}

/// `four_layer_design` placed.
pub fn four_layer_board() -> Board {
    fixture(include_str!("../../tests/fixtures/four_layer.board.json"))
}

/// The LED board placed on six layers.
pub fn six_layer_board() -> Board {
    fixture(include_str!("../../tests/fixtures/six_layer.board.json"))
}

/// The LED board placed on ACME's ten layers.
pub fn ten_layer_board() -> Board {
    fixture(include_str!("../../tests/fixtures/ten_layer.board.json"))
}

/// The board of `design` with its parts where [`Board::unplaced`] leaves them, in an outline
/// of 60 by 40 mm: for the tests that put the parts where they want them.
pub fn unplaced_board(design: &Design) -> Board {
    Board::unplaced(
        design,
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
        &PlaceOptions::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .into_board(design, Rect::new(ORIGIN, ORIGIN + Pt::new(60.0, 40.0)), 0.0)
}

/// The same circuit as `examples/led.nsm`.
pub fn led_design() -> Design {
    Design {
        modules: Vec::new(),
        name: "LedExample".into(),
        process: Some(crate::rules::test_process()),
        features: Default::default(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        parts: vec![
            part("D1", "Led", "LED", None),
            part("J1", "Header1x2", "Conn_01x02", None),
            part("R1", "Resistor", "300", Some("0402")),
        ],
        nets: vec![
            net("GND", &[("D1", "1"), ("J1", "2")]),
            net("VCC", &[("J1", "1"), ("R1", "1")]),
            net("net-led.anode", &[("D1", "2"), ("R1", "2")]),
        ],
    }
}

/// The LED board on four layers: ground and 3.3 V planes, and the supply net called `3V3`.
pub fn four_layer_design() -> Design {
    let mut design = led_design();
    design.process = Some(crate::rules::four_layer_process(&[]));
    design.nets[1].name = "3V3".into();
    design
}

#[test]
fn a_board_of_a_fictional_fabricator_says_it_is_not_for_ordering() {
    let board = ten_layer_board();
    assert_eq!(board.inner_layers().len(), 8);
    let note = board.process_note();
    assert!(
        note.starts_with("ACME TenLayer (numbers from std/mfg/acme.nsm: chosen, not sourced; ACME is a fictional fabricator)")
            && note.contains("10 layers (In1.Cu is a plane on GND, In2.Cu is a signal layer")
            && note.contains("In8.Cu is a plane on 3V3), 1.6 mm thick, 1 oz copper, build-up ACME-10L-1.6")
            && note.ends_with("; a fictional process: this package is not for ordering")
            && !note.contains("order it with these"),
        "{note}"
    );
    let extras = board.process_extras();
    assert!(
        extras.iter().any(|e| e.starts_with(
            "fictional fabricator: ACME does not exist; its numbers were chosen, not sourced, and stand for no real fabricator's capability, so this package cannot be ordered"
        )),
        "{extras:?}"
    );
    // A real fabricator's board says neither.
    let six = six_layer_board();
    assert!(six.process_note().ends_with("order it with these"));
    assert!(!six.process_extras().iter().any(|e| e.contains("fictional")));
}

#[test]
fn a_six_layer_board_has_planes_on_the_named_layers_and_says_what_each_inner_layer_is() {
    let board = six_layer_board();
    let planes: Vec<(&str, &str)> = board
        .planes
        .iter()
        .map(|p| (p.layer.as_str(), p.net.as_str()))
        .collect();
    assert_eq!(planes, [("In1.Cu", "GND"), ("In4.Cu", "3V3")]);
    assert_eq!(
        board.inner_layers(),
        ["In1.Cu", "In2.Cu", "In3.Cu", "In4.Cu"]
    );
    let note = board.process_note();
    assert!(
        note.contains(
            "6 layers (In1.Cu is a plane on GND, In2.Cu is a signal layer, with no tracks yet, In3.Cu is a signal layer, with no tracks yet, In4.Cu is a plane on 3V3), 1.6 mm thick, 1 oz copper, build-up JLC06161H-3313"
        ),
        "{note}"
    );
}

#[test]
fn the_inner_layers_of_a_four_layer_board_are_planes_on_the_nets_the_board_named() {
    let board = four_layer_board();
    let planes: Vec<(&str, &str)> = board
        .planes
        .iter()
        .map(|p| (p.layer.as_str(), p.net.as_str()))
        .collect();
    assert_eq!(planes, [("In1.Cu", "GND"), ("In2.Cu", "3V3")]);
    assert!(board.rules.plane_clearance >= board.rules.clearance);
    let two = led_board();
    assert_eq!(two.planes, vec![]);
}

#[test]
fn a_plane_on_a_net_the_design_does_not_have_names_the_layer_and_the_net() {
    // The design's supply net is called VCC, not 3V3.
    let mut design = four_layer_design();
    design.nets[1].name = "VCC".into();
    let text = Board::unplaced(
        &design,
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
        &PlaceOptions::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(
        text.contains("In2.Cu is a plane on the net `3V3`")
            && text.contains("`LedExample` has no net of that name"),
        "{text}"
    );
}

#[test]
fn the_process_note_says_which_net_each_plane_is() {
    let board = four_layer_board();
    let note = board.process_note();
    assert!(
        note.contains(
            "4 layers (In1.Cu is a plane on GND, In2.Cu is a plane on 3V3), 1.6 mm thick"
        ),
        "{note}"
    );
    assert!(note.contains("track 0.09 mm"), "{note}");
    let two = led_board();
    assert!(two.process_note().contains("; 2 layers, 1.6 mm thick"));
}

#[test]
fn a_design_with_no_process_is_refused_with_what_to_add() {
    let mut design = led_design();
    design.process = None;
    let text = Board::unplaced(
        &design,
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
        &PlaceOptions::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(
        text.contains("`LedExample` does not say how it is made"),
        "{text}"
    );
    assert!(text.contains(".process: TwoLayer"), "{text}");
    assert!(text.contains("import std::mfg::jlcpcb::TwoLayer"), "{text}");
}

#[test]
fn a_board_that_has_not_chosen_a_thickness_says_the_range() {
    let mut design = led_design();
    design.process.as_mut().unwrap().stackup.thickness = (0.4, 2.0);
    let text = Board::unplaced(
        &design,
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
        &PlaceOptions::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(text.contains("offers 0.4 mm to 2 mm"), "{text}");
    assert!(
        text.contains(".process.stackup.thickness &= 1.6mm"),
        "{text}"
    );
}

#[test]
fn builds_a_board_from_a_design() {
    let board = led_board();
    assert_eq!(board.name, "LedExample");
    let refs: Vec<&str> = board.parts.iter().map(|p| p.refdes.as_str()).collect();
    assert_eq!(refs, ["D1", "J1", "R1"]);
    assert_eq!(
        board.part("D1").unwrap().library_id,
        "LED_SMD:LED_0603_1608Metric"
    );
    assert_eq!(board.part("R1").unwrap().value, "300");
    assert_eq!(board.nets.len(), 3);
    assert_eq!(board.net_of("R1", "2").unwrap().name, "net-led.anode");
    assert!(board.net_of("R1", "9").is_none());
}

#[test]
fn the_placed_led_board_is_a_valid_placement() {
    assert_eq!(led_board().check_placement(), vec![]);
}

#[test]
fn a_part_on_the_back_is_the_footprint_in_a_mirror_with_its_pads_on_the_back_layer() {
    let mut board = led_board();
    let mut front = board.part("R1").unwrap().clone();
    front.rotation = 0.0;
    let pad2 = front.footprint.pads_numbered("2").next().unwrap().clone();
    let r1 = board.parts.iter_mut().find(|p| p.refdes == "R1").unwrap();
    r1.at = Pt::new(10.0, 10.0);
    r1.rotation = 0.0;
    let before = r1.pad_centre(&pad2);
    r1.side = Side::Back;
    let after = r1.pad_centre(&pad2);
    // Pad 2 is on the right of the footprint; on the back it is on the left.
    assert!((before.x - 10.0) > 0.0 && (after.x - 10.0) < 0.0);
    assert!((before.x - 10.0 + (after.x - 10.0)).abs() < 1e-9 && before.y == after.y);
    assert_eq!(r1.pad_layers(&pad2), (false, true));
    assert_eq!(front.pad_layers(&pad2), (true, false));
    assert_eq!(r1.pad_bounds(&pad2).centre().x, after.x);
    assert_eq!(r1.courtyard().width(), front.courtyard().width());
    // Turned by 90 degrees, the mirrored pad is where the mirror and the turn put it.
    r1.rotation = 90.0;
    let turned = r1.pad_centre(&pad2);
    assert!(close(
        turned,
        Pt::new(10.0 + (after.y - 10.0), 10.0 - (after.x - 10.0))
    ));
    assert_eq!(r1.pad_angle(&pad2), 90.0 - pad2.angle);
}

#[test]
fn courtyards_of_opposite_faces_may_overlap_unless_one_part_has_a_hole() {
    let mut board = led_board();
    let d1 = board.part("D1").unwrap().at;
    {
        let r1 = board.parts.iter_mut().find(|p| p.refdes == "R1").unwrap();
        r1.at = d1;
        r1.side = Side::Back;
    }
    assert_eq!(board.check_placement(), vec![]);
    // The same place on the same face is an overlap.
    board
        .parts
        .iter_mut()
        .find(|p| p.refdes == "R1")
        .unwrap()
        .side = Side::Front;
    assert!(board.check_placement().iter().any(|p| matches!(
        p,
        PlacementProblem::Overlap { a, b, pins: None, .. } if a == "D1" && b == "R1"
    )));
    // A header has holes through the board, so a part on the other face under it clashes.
    let j1 = board.part("J1").unwrap().at;
    let r1 = board.parts.iter_mut().find(|p| p.refdes == "R1").unwrap();
    r1.at = j1;
    r1.side = Side::Back;
    assert!(board.check_placement().iter().any(|p| matches!(
        p, PlacementProblem::Overlap { a, b, .. } if (a == "J1" && b == "R1") || (a == "R1" && b == "J1")
    )));
}

#[test]
fn two_parts_placed_by_an_import_may_share_courtyard_area_and_are_said_apart() {
    let mut board = led_board();
    let d1 = board.part("D1").unwrap().at;
    let shifted = Pt::new(d1.x + 0.3, d1.y);
    board
        .parts
        .iter_mut()
        .find(|p| p.refdes == "R1")
        .unwrap()
        .at = shifted;
    let overlap = |p: &PlacementProblem| matches!(p, PlacementProblem::Overlap { a, b, .. } if a == "D1" && b == "R1");
    assert!(board.check_placement().iter().any(overlap));
    // One of them where an imported board has it is not enough.
    board.imported.insert("R1".into());
    assert!(board.check_placement().iter().any(overlap));
    assert!(board.imported_overlaps().is_empty());
    // Both: the check leaves it out, and it is said apart.
    board.imported.insert("D1".into());
    assert!(!board.check_placement().iter().any(overlap));
    assert!(board.imported_overlaps().iter().any(overlap));
}

#[test]
fn an_overlap_names_both_members_by_path_with_the_designator_and_by_how_much() {
    let mut board = led_board();
    for p in &mut board.parts {
        p.path = match p.refdes.as_str() {
            "D1" => "status.led".into(),
            "R1" => "status.limit".into(),
            _ => String::new(),
        };
    }
    let r1 = board.part("R1").unwrap().at;
    let d1 = board.parts.iter_mut().find(|p| p.refdes == "D1").unwrap();
    d1.at = r1;
    let (a, b) = (board.part("D1").unwrap(), board.part("R1").unwrap());
    let (ca, cb) = (a.courtyard(), b.courtyard());
    let shared = (ca.max.x.min(cb.max.x) - ca.min.x.max(cb.min.x))
        * (ca.max.y.min(cb.max.y) - ca.min.y.max(cb.min.y));
    let text: Vec<String> = board
        .check_placement()
        .iter()
        .filter(|p| matches!(p, PlacementProblem::Overlap { .. }))
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        text,
        vec![format!(
            "the courtyards of `status.led` (D1) and `status.limit` (R1) overlap by {shared:.2} mm2"
        )]
    );
    // A part with no path is named by its designator alone.
    assert_eq!(member("", "J1"), "J1");
    assert_eq!(member("conn", "J1"), "`conn` (J1)");
}

#[test]
fn every_placement_check_message_names_the_member_by_path_with_the_designator() {
    let text = |p: PlacementProblem| p.to_string();
    let (r, path) = (|| "R3".to_string(), || "stage.r3".to_string());
    assert_eq!(
        text(PlacementProblem::InKeepout {
            refdes: r(),
            path: path(),
            keepout: "lid".into()
        }),
        "`stage.r3` (R3) overlaps the keepout `lid`, which forbids parts"
    );
    assert_eq!(
        text(PlacementProblem::PadInKeepout {
            refdes: r(),
            path: path(),
            keepout: "lid".into()
        }),
        "a pad of `stage.r3` (R3) is inside the keepout `lid`, which forbids tracks, so none can reach it"
    );
    assert_eq!(
        text(PlacementProblem::InCutout {
            refdes: r(),
            path: path(),
            cutout: "slot".into()
        }),
        "`stage.r3` (R3) overlaps the cutout `slot`"
    );
    assert_eq!(
        text(PlacementProblem::PadNearCutout {
            refdes: r(),
            path: path(),
            cutout: "slot".into(),
            distance: 0.12
        }),
        "a pad of `stage.r3` (R3) is 0.12 mm from the edge of the cutout `slot`"
    );
    assert_eq!(
        text(PlacementProblem::PadNearEdge {
            refdes: r(),
            path: path(),
            distance: 0.12
        }),
        "a pad of `stage.r3` (R3) is 0.12 mm from the board edge"
    );
    assert_eq!(
        text(PlacementProblem::PadNearEdge {
            refdes: r(),
            path: path(),
            distance: -0.5
        }),
        "a pad of `stage.r3` (R3) is 0.50 mm outside the board edge"
    );
    assert_eq!(
        text(PlacementProblem::HoleTooNear {
            refdes: "J1".into(),
            path: "conn".into(),
            other_path: path(),
            pad: "1".into(),
            other: r(),
            other_pad: "2".into(),
            to: HoleNeighbour::Copper,
            distance: 0.1,
            least: 0.2
        }),
        "a hole of `conn` (J1) (pad 1) is 0.10 mm from the copper of `stage.r3` (R3) (pad 2), under the process's clearance of 0.2 mm"
    );
    assert_eq!(
        text(PlacementProblem::TooTall {
            refdes: r(),
            path: path(),
            keepout: "lid".into(),
            height: 3.0,
            limit: 2.0,
            side: Side::Front,
            at: (1.0, 2.0)
        }),
        "`stage.r3` (R3) stands 3.00 mm tall on the top face, and the keepout `lid` allows 2.00 mm there (1.00 mm too tall), where its body is, around (1.0, 2.0) mm from the board's top left corner"
    );
    assert!(
        text(PlacementProblem::OutsideOutline {
            refdes: r(),
            path: path(),
            past: vec![(Edge::Left, 0.5)],
            stated: Vec::new(),
            by: String::new(),
            cut: None,
        })
        .starts_with("`stage.r3` (R3) is not wholly inside the board outline: its courtyard is 0.50 mm past the left edge")
    );
    assert_eq!(
        HeightUnchecked {
            refdes: r(),
            path: path(),
            keepout: "lid".into(),
            limit: 2.0,
            side: Side::Front,
            standard: None,
        }
        .to_string(),
        "height of `stage.r3` (R3) is unchecked: the keepout `lid` limits parts on the top face to 2.00 mm and the part states no height (say `.stage.r3.height &= 8mm`, with the real number)"
    );
    assert!(
        Hang {
            refdes: r(),
            path: path(),
            edge: Edge::Left,
            by: 0.5,
            stated: "the requirement".into()
        }
        .to_string()
        .starts_with("`stage.r3` (R3) hangs over the left edge: its courtyard is 0.50 mm past it")
    );
}

#[test]
fn the_check_finds_overlaps_and_parts_off_the_board() {
    let mut board = led_board();
    let r1 = board.part("R1").unwrap().at;
    let d1 = board.parts.iter_mut().find(|p| p.refdes == "D1").unwrap();
    d1.at = r1 + Pt::new(0.5, 0.0);
    let j1 = board.parts.iter_mut().find(|p| p.refdes == "J1").unwrap();
    j1.at = Pt::new(1000.0, 0.0);
    let problems = board.check_placement();
    assert!(problems.iter().any(|p| matches!(
        p,
        PlacementProblem::Overlap { a, b, pins: None, .. } if a == "D1" && b == "R1"
    )));
    assert!(problems.iter().any(|p| matches!(
        p,
        PlacementProblem::OutsideOutline { refdes, .. } if refdes == "J1"
    )));
    assert!(matches!(
        problems.last(),
        Some(PlacementProblem::PadNearEdge { refdes, .. }) if refdes == "J1"
    ));
    assert_eq!(problems.len(), 3);
}

/// The led board with `refdes` moved so that its courtyard's right side is `past` mm past
/// the right edge (negative: inside it), and what `check_placement` says of that part.
fn moved_past_the_right_edge(refdes: &str, past: f64) -> Board {
    let mut board = led_board();
    let edge = board.outline.max.x;
    let part = board.parts.iter_mut().find(|p| p.refdes == refdes).unwrap();
    let right = part.courtyard().max.x;
    part.at.x += edge + past - right;
    board
}

fn about(board: &Board, refdes: &str) -> Vec<PlacementProblem> {
    board
        .check_placement()
        .into_iter()
        .filter(|p| match p {
            PlacementProblem::OutsideOutline { refdes: r, .. }
            | PlacementProblem::PadNearEdge { refdes: r, .. } => r == refdes,
            _ => false,
        })
        .collect()
}

fn stated(edge: Edge) -> Overhang {
    let mut edges = crate::overhang::Edges::default();
    edges.set(edge);
    Overhang::Stated {
        edges,
        says: "`j1.place.right (a.nsm:3)` allows at least 36 mm".into(),
    }
}

#[test]
fn a_courtyard_past_an_edge_nobody_stated_is_outside_and_says_how_far() {
    let board = moved_past_the_right_edge("J1", 0.5);
    let found = about(&board, "J1");
    assert_eq!(found.len(), 1, "{found:?}");
    let text = found[0].to_string();
    let path = &board.part("J1").unwrap().path;
    assert_eq!(
        text,
        format!(
            "J1 is not wholly inside the board outline: its courtyard is 0.50 mm past the right edge, and the design does not say it may hang over an edge; to allow it past the right edge, require the body to reach it, for example `.{path}.place.right >= outline.width`"
        )
    );
}

#[test]
fn a_courtyard_past_the_edge_the_design_states_is_not_reported_and_one_past_another_is() {
    let mut board = moved_past_the_right_edge("J1", 0.5);
    board.overhang.insert("J1".into(), stated(Edge::Right));
    assert_eq!(
        about(&board, "J1"),
        vec![],
        "a stated overhang is not a finding"
    );
    assert_eq!(board.hangs().len(), 1);
    assert!((board.hangs()[0].by - 0.5).abs() < 1e-6);
    // Stating another edge does not help.
    board.overhang.insert("J1".into(), stated(Edge::Bottom));
    let found = about(&board, "J1");
    assert_eq!(found.len(), 1, "{found:?}");
    let text = found[0].to_string();
    assert!(
        text.contains("its courtyard is 0.50 mm past the right edge")
            && text.contains("the design says it may hang over only the bottom edge (`j1.place.right (a.nsm:3)` allows at least 36 mm)"),
        "{text}"
    );
}

#[test]
fn a_pad_outside_is_a_finding_even_where_the_overhang_is_stated() {
    // The header's pads are 0.95 mm inside its courtyard's right side.
    let mut board = moved_past_the_right_edge("J1", 2.0);
    board.overhang.insert("J1".into(), stated(Edge::Right));
    let found = about(&board, "J1");
    assert_eq!(found.len(), 1, "{found:?}");
    let text = found[0].to_string();
    assert!(
        text.starts_with("a pad of J1 is ") && text.ends_with(" mm outside the board edge"),
        "{text}"
    );
    // Just far enough in that the copper keeps the process's space from the edge.
    let mut board = moved_past_the_right_edge("J1", 0.95 - board.rules.copper_to_edge);
    board.overhang.insert("J1".into(), stated(Edge::Right));
    assert_eq!(about(&board, "J1"), vec![]);
}

#[test]
fn a_stated_overhang_overlaps_other_parts_only_inside_the_outline() {
    let mut board = moved_past_the_right_edge("R1", 1.0);
    board.overhang.insert("R1".into(), stated(Edge::Right));
    board.overhang.insert("D1".into(), stated(Edge::Right));
    let overlaps = |board: &Board| {
        board
            .check_placement()
            .into_iter()
            .filter(|p| matches!(p, PlacementProblem::Overlap { .. }))
            .count()
    };
    // D1 is put on R1, so that their courtyards share the area beyond the edge only.
    let edge = board.outline.max.x;
    let r1 = board.part("R1").unwrap().courtyard();
    let d1 = board.parts.iter_mut().find(|p| p.refdes == "D1").unwrap();
    let c = d1.courtyard();
    d1.at = Pt::new(d1.at.x + edge - c.min.x, d1.at.y + r1.min.y - c.min.y);
    assert_eq!(overlaps(&board), 0, "the shared area is beyond the edge");
    // One that reaches into the board overlaps there.
    let d1 = board.parts.iter_mut().find(|p| p.refdes == "D1").unwrap();
    d1.at.x -= 0.4;
    assert_eq!(overlaps(&board), 1);
}

#[test]
fn a_pad_too_near_the_edge_is_found() {
    let near_edge = |board: &Board| -> Vec<PlacementProblem> {
        board
            .check_placement()
            .into_iter()
            .filter(|p| matches!(p, PlacementProblem::PadNearEdge { .. }))
            .collect()
    };
    let mut board = led_board();
    assert_eq!(board.check_placement(), vec![]);
    // The leftmost pad of the board, wherever the placer put it.
    let (leftmost, left) = board
        .parts
        .iter()
        .flat_map(|p| {
            p.footprint
                .pads
                .iter()
                .map(move |pad| (p.refdes.clone(), p.pad_bounds(pad).min.x))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    board.outline.min.x = left - 0.1;
    // Every part whose leftmost pad is as far left as that one is too near the edge.
    let problems = near_edge(&board);
    assert!(!problems.is_empty());
    let mut found = false;
    for problem in &problems {
        match problem {
            PlacementProblem::PadNearEdge {
                refdes, distance, ..
            } => {
                assert!(
                    (distance - 0.1).abs() < 1e-9,
                    "{refdes} is {distance} mm off"
                );
                found |= *refdes == leftmost;
            }
            other => panic!("{other}"),
        }
    }
    assert!(found, "{leftmost} is not named: {problems:?}");
    board.outline.min.x = left - board.rules.copper_to_edge;
    assert_eq!(near_edge(&board), vec![], "exactly the rule is allowed");
}

/// A round pad of a mounting hole in a rounded corner is measured as the circle it is: its
/// box corner is nearly at the arc (0.05 mm), the copper is 1.09 mm from it.
#[test]
fn a_round_pad_near_a_rounded_corner_is_measured_as_a_circle() {
    let mut board = led_board();
    board.outline = Rect {
        min: Pt::new(0.0, 0.0),
        max: Pt::new(50.0, 50.0),
    };
    board.corner_radius = 5.0;
    let mut part = board.parts[0].clone();
    let mut pad = part.footprint.pads[0].clone();
    pad.shape = crate::footprint::PadShape::Circle;
    pad.width = 5.0;
    pad.height = 5.0;
    pad.at = Pt::new(0.0, 0.0);
    pad.extent = None;
    part.at = Pt::new(4.0, 4.0);
    part.rotation = 0.0;
    let circle = board.pad_edge_distance(&part, &pad);
    let by_box = board.edge_distance(&part.pad_bounds(&pad));
    let expected = 5.0 - 2f64.sqrt() - 2.5;
    assert!((circle - expected).abs() < 1e-9, "{circle}");
    assert!(by_box < 0.06, "{by_box}");
    // A pad that is not round is still measured by its box.
    pad.shape = crate::footprint::PadShape::Rect;
    assert!((board.pad_edge_distance(&part, &pad) - by_box).abs() < 1e-9);
}

fn close(a: Pt, b: Pt) -> bool {
    a.distance(b) < 1e-9
}

#[test]
fn a_rotated_part_moves_its_pads_and_courtyard() {
    let board = led_board();
    let mut r1 = board.part("R1").unwrap().clone();
    r1.at = Pt::new(10.0, 10.0);
    r1.rotation = 0.0;
    let pad2 = r1.footprint.pads[1].clone();
    assert!(close(r1.pad_centre(&pad2), Pt::new(10.51, 10.0)));
    assert!((r1.courtyard().width() - 1.86).abs() < 1e-9);
    r1.rotation = 90.0;
    assert!(close(r1.pad_centre(&pad2), Pt::new(10.0, 9.49)));
    assert_eq!(r1.pad_angle(&pad2), 90.0);
    assert!((r1.courtyard().height() - 1.86).abs() < 1e-9);
}

#[test]
fn the_placer_and_the_fab_check_measure_one_body_for_a_footprint_with_no_courtyard() {
    // #457: a footprint that draws neither a courtyard nor a fabrication layer, on the
    // back and turned, with pads off its origin. The placer's body (`Placed::body`) and
    // the one the fab check measures on the written board are one box, at a right angle
    // and at any other.
    use netsmith_geom::{Constraint, ConstraintMeasure, Constraints};
    let mut board = led_board();
    let bare = crate::footprint::Footprint::parse(
        "T:Fingers",
        r#"(footprint "Fingers"
            (property "Reference" "REF**" (at 0 -3 0) (layer "F.SilkS"))
            (pad "1" connect rect (at 0 0.55) (size 0.7 3.2) (layers "F.Cu" "F.Mask"))
            (pad "2" connect rect (at -1 0) (size 0.7 4.3) (layers "F.Cu" "F.Mask")))"#,
    )
    .unwrap();
    assert!(bare.body_is_courtyard);
    for rotation in [90.0, 30.0] {
        let r1 = board.parts.iter_mut().find(|p| p.refdes == "R1").unwrap();
        r1.footprint = bare.clone();
        r1.side = Side::Back;
        r1.at = Pt::new(15.0, 12.0);
        r1.rotation = rotation;
        let placer = r1.body();
        let o = board.outline.min;
        let want = |measure| Constraint {
            id: "r1".into(),
            source: "t.nsm:1".into(),
            parts: vec!["R1".into()],
            measure,
            lo: f64::NEG_INFINITY,
            hi: f64::INFINITY,
        };
        let constraints = Constraints {
            list: vec![
                want(ConstraintMeasure::Left),
                want(ConstraintMeasure::Right),
                want(ConstraintMeasure::Top),
                want(ConstraintMeasure::Bottom),
            ],
            sides: Vec::new(),
            relations: Vec::new(),
            severity: netsmith_geom::DrcSeverity::Error,
        };
        let art = netsmith_geom::Artwork::parse(&crate::export::kicad_pcb(&board)).unwrap();
        let checked: Vec<f64> = constraints
            .measure(&art)
            .iter()
            .map(|m| m.measured.unwrap_or_else(|| panic!("{}", m.message)))
            .collect();
        let expected = [
            placer.min.x - o.x,
            placer.max.x - o.x,
            placer.min.y - o.y,
            placer.max.y - o.y,
        ];
        for (c, e) in checked.iter().zip(expected) {
            assert!(
                (c - e).abs() < 1e-6,
                "turned {rotation}: the check measures {checked:?} and the placer {expected:?}"
            );
        }
    }
}

#[test]
fn a_net_that_names_a_missing_pad_says_which() {
    let mut design = led_design();
    design.nets[0].pins[0].pin = "7".into();
    let err = Board::unplaced(
        &design,
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
        &PlaceOptions::default(),
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "net GND joins pad 7 of D1, but footprint LED_SMD:LED_0603_1608Metric has only pads 1, 2"
    );
}

#[test]
fn a_part_with_no_footprint_is_named() {
    let mut design = led_design();
    design.parts[2].kind = "Gizmo".into();
    let err = Board::unplaced(
        &design,
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
        &PlaceOptions::default(),
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "R1: there is no KiCad symbol for a `Gizmo` yet"
    );
}

#[test]
fn a_missing_library_file_names_the_footprint_and_path() {
    let empty = FootprintLibrary::at("/nonexistent/footprints");
    let text = Board::unplaced(
        &led_design(),
        &netsmith_kicad::Mapping::standard(),
        &empty,
        &PlaceOptions::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(text.starts_with("D1: footprint `LED_SMD:LED_0603_1608Metric` not found"));
    assert!(text.contains("/nonexistent/footprints/LED_SMD.pretty/LED_0603_1608Metric.kicad_mod"));
}
