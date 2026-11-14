//! The requirements a design puts on where parts go (ADR 0045), measured on a board file:
//! the frame, the body box, the numbers in the messages, and what changes the severity.
//! None of these need KiCad.

use crate::common;

use common::*;
use netsmith_geom::{Artwork, Constraint, Constraints, ConstraintMeasure, ConstraintRelation, ConstraintRelationEnd, SideRule, DrcRule, DrcSeverity};

fn want(id: &str, parts: &[&str], measure: ConstraintMeasure, lo: f64, hi: f64) -> Constraint {
    Constraint {
        id: id.into(),
        source: "widget.nsm:14".into(),
        parts: parts.iter().map(|p| p.to_string()).collect(),
        measure,
        lo,
        hi,
    }
}

fn set(list: Vec<Constraint>) -> Constraints {
    Constraints {
        list,
        sides: Vec::new(),
        relations: Vec::new(),
        severity: DrcSeverity::Error,
    }
}

/// A board 40 by 30 mm with `J1` (a courtyard 2 by 1.4 mm, no fabrication drawing) at (10, 10).
fn board() -> String {
    let mut b = Pcb::new(2);
    b.part(
        "J1",
        (10.0, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.5, 0.5, "A")],
        Draw::default(),
    );
    b.pcb()
}

/// A footprint that draws a fabrication layer body of 6 by 4 mm, offset from its origin, and
/// a courtyard much bigger.
fn with_fab(at: (f64, f64), angle: f64, back: bool) -> String {
    let mut b = Pcb::new(2);
    let (fab, crt, copper) = if back {
        ("B.Fab", "B.CrtYd", "B.Cu")
    } else {
        ("F.Fab", "F.CrtYd", "F.Cu")
    };
    b.raw(&format!(
        "(footprint \"Test:U1\" (layer \"{copper}\") (at {} {} {angle})
            (property \"Reference\" \"U1\" (at 0 -3 0) (layer \"{fab}\") (hide yes) (effects (font (size 1 1) (thickness 0.15))))
            (fp_rect (start -1 -1) (end 5 3) (stroke (width 0.1) (type solid)) (fill no) (layer \"{fab}\"))
            (fp_rect (start -3 -3) (end 7 5) (stroke (width 0.05) (type solid)) (fill no) (layer \"{crt}\")))",
        at.0, at.1
    ));
    b.pcb()
}

fn measure(c: &Constraints, pcb: &str) -> Vec<netsmith_geom::ConstraintOutcome> {
    c.measure(&Artwork::parse(pcb).unwrap())
}

#[test]
fn a_body_is_measured_from_the_top_left_corner_of_the_outline() {
    // The body is the courtyard here: x from 9 to 11, y from 9.3 to 10.7.
    let c = set(vec![
        want("j.place.x", &["J1"], ConstraintMeasure::X, 10.0, 10.0),
        want("j.place.y", &["J1"], ConstraintMeasure::Y, 10.0, 10.0),
        want("j.place.left", &["J1"], ConstraintMeasure::Left, 9.0, 9.0),
        want("j.place.right", &["J1"], ConstraintMeasure::Right, 11.0, 11.0),
        want("j.place.top", &["J1"], ConstraintMeasure::Top, 9.3, 9.3),
        want("j.place.bottom", &["J1"], ConstraintMeasure::Bottom, 10.7, 10.7),
    ]);
    let all = measure(&c, &board());
    assert!(all.iter().all(|o| o.satisfied), "{all:#?}");
    let right = &all[3];
    assert_eq!(right.measured, Some(11.0));
    assert_eq!(right.body, "F.CrtYd");
}

#[test]
fn the_frame_moves_with_the_outline() {
    let mut b = Pcb::new(2);
    b.outline = (20.0, 20.0, 60.0, 50.0);
    b.part(
        "J1",
        (30.0, 30.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.5, 0.5, "A")],
        Draw::default(),
    );
    let c = set(vec![want("j.place.x", &["J1"], ConstraintMeasure::X, 10.0, 10.0)]);
    assert!(measure(&c, &b.pcb())[0].satisfied);
}

#[test]
fn a_violation_gives_the_measure_the_range_and_how_far_over() {
    let c = set(vec![want("j.place.left", &["J1"], ConstraintMeasure::Left, 2.0, 3.0)]);
    let pcb = board();
    let all = measure(&c, &pcb);
    assert!(!all[0].satisfied);
    assert!((all[0].over - 6.0).abs() < 1e-9);
    assert_eq!(
        all[0].message,
        "constraint j.place.left (widget.nsm:14) is violated: the left edge of the body of J1 (F.CrtYd) is 9 mm from the left edge of the outline, and it allows 2 to 3 mm (6 mm over)"
    );
    let findings = c.findings(&Artwork::parse(&pcb).unwrap());
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, DrcRule::Constraint);
    assert_eq!(findings[0].severity, DrcSeverity::Error);
    assert_eq!(findings[0].parts(), ["J1"]);
}

#[test]
fn a_satisfied_constraint_says_by_how_much_room_it_has() {
    let c = set(vec![want("j.place.x", &["J1"], ConstraintMeasure::X, 9.5, 11.0)]);
    let all = measure(&c, &board());
    assert!(all[0].satisfied);
    assert!((all[0].margin - 0.5).abs() < 1e-9);
    assert_eq!(
        all[0].message,
        "constraint j.place.x (widget.nsm:14) is satisfied: the centre of the body of J1 (F.CrtYd) is 10 mm from the left edge of the outline, and it allows 9.5 to 11 mm"
    );
}

#[test]
fn half_a_micrometre_short_passes_as_in_kicads_check() {
    let c = set(vec![want("a", &["J1"], ConstraintMeasure::X, 10.0004, 12.0)]);
    assert!(measure(&c, &board())[0].satisfied);
    let c = set(vec![want("a", &["J1"], ConstraintMeasure::X, 10.001, 12.0)]);
    assert!(!measure(&c, &board())[0].satisfied);
}

#[test]
fn the_fabrication_layer_is_the_body_when_the_footprint_draws_one() {
    // The drawing spans x from 11 to 17 and y from 11 to 15 for a footprint at (12, 12).
    let pcb = with_fab((12.0, 12.0), 0.0, false);
    let c = set(vec![
        want("u.place.left", &["U1"], ConstraintMeasure::Left, 11.0, 11.0),
        want("u.place.x", &["U1"], ConstraintMeasure::X, 14.0, 14.0),
        want("u.place.bottom", &["U1"], ConstraintMeasure::Bottom, 15.0, 15.0),
    ]);
    let all = measure(&c, &pcb);
    assert!(all.iter().all(|o| o.satisfied), "{all:#?}");
    assert_eq!(all[0].body, "F.Fab");
}

#[test]
fn a_part_on_the_back_is_measured_on_its_back_fabrication_layer() {
    let pcb = with_fab((12.0, 12.0), 0.0, true);
    let c = set(vec![want(
        "u.place.left",
        &["U1"],
        ConstraintMeasure::Left,
        11.0,
        11.0,
    )]);
    let all = measure(&c, &pcb);
    assert!(all[0].satisfied, "{all:#?}");
    assert_eq!(all[0].body, "B.Fab");
}

#[test]
fn the_rotation_is_the_footprints_turn_and_a_full_turn_is_none() {
    let mut b = Pcb::new(2);
    b.part(
        "J1",
        (10.0, 10.0),
        270.0,
        &[smd("1", 0.0, 0.0, 0.5, 0.5, "A")],
        Draw::default(),
    );
    let pcb = b.pcb();
    let ok = set(vec![want(
        "j.place.rotation",
        &["J1"],
        ConstraintMeasure::Rotation,
        270.0,
        270.0,
    )]);
    assert!(measure(&ok, &pcb)[0].satisfied);
    let wrapped = set(vec![want(
        "j.place.rotation",
        &["J1"],
        ConstraintMeasure::Rotation,
        -90.0,
        -90.0,
    )]);
    assert!(measure(&wrapped, &pcb)[0].satisfied);
    let bad = set(vec![want(
        "j.place.rotation",
        &["J1"],
        ConstraintMeasure::Rotation,
        0.0,
        0.0,
    )]);
    let o = &measure(&bad, &pcb)[0];
    assert!(!o.satisfied);
    assert_eq!(
        o.message,
        "constraint j.place.rotation (widget.nsm:14) is violated: J1 is turned 270 degrees, and it allows 0 degrees (90 degrees over)"
    );
}

/// A back footprint's file orientation is the turn plus a half (KiCad's flip mirrors in y,
/// netsmith's in x, #553): a footprint the file turns 300 on the back is turned 120.
#[test]
fn the_rotation_of_a_back_footprint_is_the_files_less_a_half_turn() {
    let pcb = with_fab((20.0, 15.0), 300.0, true);
    let c = set(vec![want(
        "u.place.rotation",
        &["U1"],
        ConstraintMeasure::Rotation,
        120.0,
        120.0,
    )]);
    assert!(measure(&c, &pcb)[0].satisfied, "{:#?}", measure(&c, &pcb));
    let front = with_fab((20.0, 15.0), 300.0, false);
    assert!(!measure(&c, &front)[0].satisfied);
}

#[test]
fn a_module_is_measured_on_the_box_round_all_its_parts() {
    let mut b = Pcb::new(2);
    let pad = [smd("1", 0.0, 0.0, 0.5, 0.5, "A")];
    b.part("R1", (10.0, 10.0), 0.0, &pad, Draw::default());
    b.part("R2", (20.0, 18.0), 0.0, &pad, Draw::default());
    let c = set(vec![
        want("block.place.left", &["R1", "R2"], ConstraintMeasure::Left, 9.0, 9.0),
        want(
            "block.place.right",
            &["R1", "R2"],
            ConstraintMeasure::Right,
            21.0,
            21.0,
        ),
        want(
            "block.place.bottom",
            &["R1", "R2"],
            ConstraintMeasure::Bottom,
            18.0,
            18.0,
        ),
    ]);
    let all = measure(&c, &b.pcb());
    assert!(all[0].satisfied && all[1].satisfied, "{all:#?}");
    assert!(!all[2].satisfied);
    assert!(
        all[2]
            .message
            .contains("the bottom edge of the box round the bodies of R1, R2 is 18.7 mm"),
        "{}",
        all[2].message
    );
}

#[test]
fn a_face_is_checked_too() {
    let c = Constraints {
        list: vec![],
        sides: vec![SideRule {
            id: "j.place.side".into(),
            source: "widget.nsm:15".into(),
            part: "J1".into(),
            side: "bottom".into(),
        }],
        relations: Vec::new(),
        severity: DrcSeverity::Error,
    };
    let o = &measure(&c, &board())[0];
    assert!(!o.satisfied);
    assert_eq!(
        o.message,
        "constraint j.place.side (widget.nsm:15) is violated: J1 is on the top, and it must be on the bottom"
    );
}

#[test]
fn a_part_that_is_not_on_the_board_is_a_violation_that_says_so() {
    let c = set(vec![want("x.place.x", &["U9"], ConstraintMeasure::X, 1.0, 2.0)]);
    let o = &measure(&c, &board())[0];
    assert!(!o.satisfied);
    assert!(
        o.message.contains("U9 is not on the board"),
        "{}",
        o.message
    );
}

#[test]
fn a_warning_setting_makes_warnings_and_ignore_makes_none() {
    let mut c = set(vec![want("j.place.left", &["J1"], ConstraintMeasure::Left, 2.0, 3.0)]);
    let art = Artwork::parse(&board()).unwrap();
    c.severity = DrcSeverity::Warning;
    assert_eq!(c.findings(&art)[0].severity, DrcSeverity::Warning);
    c.severity = DrcSeverity::Ignore;
    assert!(c.findings(&art).is_empty());
}

#[test]
fn an_open_range_has_one_end() {
    let c = set(vec![want(
        "j.place.left",
        &["J1"],
        ConstraintMeasure::Left,
        9.5,
        f64::INFINITY,
    )]);
    let o = &measure(&c, &board())[0];
    assert!(!o.satisfied);
    assert!(
        o.message.contains("allows at least 9.5 mm"),
        "{}",
        o.message
    );
}

#[test]
fn a_board_with_no_outline_cannot_be_measured() {
    let mut b = Pcb::new(2);
    b.outline = (0.0, 0.0, 0.0, 0.0);
    let pcb = board().replace("Edge.Cuts", "Dwgs.User");
    let c = set(vec![want("j.place.x", &["J1"], ConstraintMeasure::X, 10.0, 10.0)]);
    let o = &measure(&c, &pcb)[0];
    assert!(!o.satisfied);
    assert!(o.message.contains("no outline"), "{}", o.message);
}

/// Two 6 by 4 mm bodies (`U1` and `U2`, as `with_fab` draws them, body from x -1 to 5 and y
/// -1 to 3 of its origin) at the two places.
fn two_bodies(a: (f64, f64), b: (f64, f64)) -> String {
    let mut p = Pcb::new(2);
    for (name, at) in [("U1", a), ("U2", b)] {
        p.raw(&format!(
            "(footprint \"Test:{name}\" (layer \"F.Cu\") (at {} {} 0)
                (property \"Reference\" \"{name}\" (at 0 -3 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15))))
                (fp_rect (start -1 -1) (end 5 3) (stroke (width 0.1) (type solid)) (fill no) (layer \"F.Fab\"))
                (fp_rect (start -3 -3) (end 7 5) (stroke (width 0.05) (type solid)) (fill no) (layer \"F.CrtYd\")))",
            at.0, at.1
        ));
    }
    p.pcb()
}

fn between(measure: ConstraintMeasure, lo: f64, hi: f64) -> Constraints {
    Constraints {
        list: Vec::new(),
        sides: Vec::new(),
        relations: vec![ConstraintRelation {
            id: "radio.row_b.place.x - radio.row_a.place.x".into(),
            source: "radio.physical.nsm:12".into(),
            a: ConstraintRelationEnd {
                part: "U1".into(),
                path: "radio.row_a".into(),
                measure,
            },
            b: ConstraintRelationEnd {
                part: "U2".into(),
                path: "radio.row_b".into(),
                measure,
            },
            lo,
            hi,
        }],
        severity: DrcSeverity::Error,
    }
}

#[test]
fn a_relation_between_two_parts_is_measured_on_their_bodies() {
    let c = between(ConstraintMeasure::X, 17.45, 17.55);
    let all = measure(&c, &two_bodies((10.0, 10.0), (27.5, 10.0)));
    assert_eq!(all.len(), 1);
    assert!(all[0].satisfied, "{}", all[0].message);
    assert!((all[0].margin - 0.05).abs() < 1e-9);
    assert_eq!(
        all[0].message,
        "constraint radio.row_b.place.x - radio.row_a.place.x (radio.physical.nsm:12) is satisfied: the centre of the body of U2 (`radio.row_b`, F.Fab) minus the centre of the body of U1 (`radio.row_a`, F.Fab) is 17.5 mm along x, and it allows 17.45 to 17.55 mm"
    );
}

#[test]
fn an_unmet_relation_names_both_parts_the_offset_the_window_and_the_line() {
    let c = between(ConstraintMeasure::X, 17.45, 17.55);
    let pcb = two_bodies((10.0, 10.0), (27.0, 10.0));
    let all = measure(&c, &pcb);
    assert!(!all[0].satisfied);
    assert!((all[0].over - 0.45).abs() < 1e-9, "{}", all[0].over);
    assert_eq!(all[0].measured, Some(17.0));
    assert_eq!(
        all[0].message,
        "constraint radio.row_b.place.x - radio.row_a.place.x (radio.physical.nsm:12) is violated: the centre of the body of U2 (`radio.row_b`, F.Fab) minus the centre of the body of U1 (`radio.row_a`, F.Fab) is 17 mm along x, and it allows 17.45 to 17.55 mm (0.45 mm over)"
    );
    let findings = c.findings(&Artwork::parse(&pcb).unwrap());
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, DrcRule::Constraint);
    assert_eq!(findings[0].severity, DrcSeverity::Error);
    assert_eq!(findings[0].parts(), ["U2", "U1"]);
}

#[test]
fn a_relation_along_y_and_between_edges_is_measured_too() {
    // The bodies are 4 mm high: bottom of U1 (y 3 of its origin) against top of U2 (-1).
    let mut c = between(ConstraintMeasure::Y, 0.0, 0.0);
    c.relations[0].a.measure = ConstraintMeasure::Bottom;
    c.relations[0].b.measure = ConstraintMeasure::Top;
    let ok = measure(&c, &two_bodies((10.0, 10.0), (30.0, 14.0)));
    assert!(ok[0].satisfied, "{}", ok[0].message);
    let bad = measure(&c, &two_bodies((10.0, 10.0), (30.0, 16.0)));
    assert!(!bad[0].satisfied);
    assert!(
        bad[0]
            .message
            .contains("the top edge of the body of U2 (`radio.row_b`, F.Fab) minus the bottom edge of the body of U1")
            && bad[0].message.contains("is 2 mm along y, and it allows 0 mm (2 mm over)"),
        "{}",
        bad[0].message
    );
}

#[test]
fn a_relation_is_a_warning_when_the_project_says_warn() {
    let mut c = between(ConstraintMeasure::X, 17.45, 17.55);
    c.severity = DrcSeverity::Warning;
    let findings = c.findings(&Artwork::parse(&two_bodies((10.0, 10.0), (27.0, 10.0))).unwrap());
    assert_eq!(findings[0].severity, DrcSeverity::Warning);
}

#[test]
fn a_relation_whose_part_is_not_on_the_board_is_violated() {
    let c = between(ConstraintMeasure::X, 17.45, 17.55);
    let all = measure(&c, &board());
    assert!(!all[0].satisfied);
    assert!(
        all[0]
            .message
            .ends_with("is violated: U1 and U2 is not on the board")
            || all[0].message.contains("not on the board"),
        "{}",
        all[0].message
    );
}

/// A footprint on the back, turned 90 degrees, that draws no courtyard and no fabrication
/// layer, with pads not centred on its origin (a miniature of an edge connector's fingers),
/// as netsmith writes it: each pad's place in the footprint's frame (x already negated for the
/// back) and its turn on the board.
fn bare_fingers() -> String {
    let mut b = Pcb::new(2);
    b.raw(
        "(footprint \"Test:J1\" (layer \"B.Cu\") (at 10 10 90)
            (property \"Reference\" \"J1\" (at 0 -3 0) (layer \"B.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15))))
            (pad \"1\" connect rect (at 0 0.55 90) (size 0.7 3.2) (layers \"B.Cu\" \"B.Mask\"))
            (pad \"2\" connect rect (at 1 0 90) (size 0.7 4.3) (layers \"B.Cu\" \"B.Mask\")))",
    );
    b.pcb()
}

#[test]
fn a_footprint_with_no_courtyard_is_measured_by_its_turned_pads_and_the_placers_margin() {
    // #457: the body of a footprint that draws neither layer is its pads and 0.25 mm, as the
    // placer measures it (`BARE_COURTYARD_MARGIN`), taken in the footprint's frame and turned
    // with it. In that frame the pads span x -0.35 to 1.35 and y -2.15 to 2.15; with the margin
    // and turned 90 degrees about (10, 10) the body is x 7.6 to 12.4 and y 8.4 to 10.6. The
    // pads' sizes not turned put the centre at x 10.275 (0.275 mm off).
    let c = set(vec![
        want("j.place.x", &["J1"], ConstraintMeasure::X, 10.0, 10.0),
        want("j.place.y", &["J1"], ConstraintMeasure::Y, 9.5, 9.5),
        want("j.place.left", &["J1"], ConstraintMeasure::Left, 7.6, 7.6),
        want("j.place.right", &["J1"], ConstraintMeasure::Right, 12.4, 12.4),
        want("j.place.top", &["J1"], ConstraintMeasure::Top, 8.4, 8.4),
        want("j.place.bottom", &["J1"], ConstraintMeasure::Bottom, 10.6, 10.6),
    ]);
    let all = measure(&c, &bare_fingers());
    assert!(all.iter().all(|o| o.satisfied), "{all:#?}");
    assert_eq!(
        all[0].message,
        "constraint j.place.x (widget.nsm:14) is satisfied: the centre of the body of J1 (pads and 0.25 mm) is 10 mm from the left edge of the outline, and it allows 10 mm"
    );
}

#[test]
fn a_hole_that_reaches_past_the_courtyard_is_in_the_body() {
    // The placer's courtyard takes in every hole (a mounting hole, a pogo-pin pad), and every
    // pad that reaches past it with 0.15 mm round it, and so does the body the check measures:
    // a 3 mm hole at (4, 0) of a part whose courtyard is 2 by 1.4 mm reaches x 5.5, and its
    // 3.2 mm pad x 5.6, which with the 0.15 mm is x 5.75.
    let mut b = Pcb::new(2);
    b.part(
        "J1",
        (10.0, 10.0),
        0.0,
        &[thru("1", 4.0, 0.0, 3.2, 3.0, "A")],
        Draw::default(),
    );
    let c = set(vec![want(
        "j.place.right",
        &["J1"],
        ConstraintMeasure::Right,
        15.75,
        15.75,
    )]);
    let all = measure(&c, &b.pcb());
    assert!(all[0].satisfied, "{all:#?}");
}
