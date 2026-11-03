//! The requirements a design puts on where parts go (ADR 0045), measured on a board file:
//! the frame, the body box, the numbers in the messages, and what changes the severity.
//! None of these need KiCad.

mod common;

use common::*;
use netsmith_fab::artwork::Artwork;
use netsmith_fab::drc::constraints::{Constraint, Constraints, Measure, SideRule};
use netsmith_fab::drc::{Rule, Severity};

fn want(id: &str, parts: &[&str], measure: Measure, lo: f64, hi: f64) -> Constraint {
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
        severity: Severity::Error,
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

fn measure(c: &Constraints, pcb: &str) -> Vec<netsmith_fab::drc::constraints::Outcome> {
    c.measure(&Artwork::parse(pcb).unwrap())
}

#[test]
fn a_body_is_measured_from_the_top_left_corner_of_the_outline() {
    // The body is the courtyard here: x from 9 to 11, y from 9.3 to 10.7.
    let c = set(vec![
        want("j.place.x", &["J1"], Measure::X, 10.0, 10.0),
        want("j.place.y", &["J1"], Measure::Y, 10.0, 10.0),
        want("j.place.left", &["J1"], Measure::Left, 9.0, 9.0),
        want("j.place.right", &["J1"], Measure::Right, 11.0, 11.0),
        want("j.place.top", &["J1"], Measure::Top, 9.3, 9.3),
        want("j.place.bottom", &["J1"], Measure::Bottom, 10.7, 10.7),
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
    let c = set(vec![want("j.place.x", &["J1"], Measure::X, 10.0, 10.0)]);
    assert!(measure(&c, &b.pcb())[0].satisfied);
}

#[test]
fn a_violation_gives_the_measure_the_range_and_how_far_over() {
    let c = set(vec![want("j.place.left", &["J1"], Measure::Left, 2.0, 3.0)]);
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
    assert_eq!(findings[0].rule, Rule::Constraint);
    assert_eq!(findings[0].severity, Severity::Error);
    assert_eq!(findings[0].parts(), ["J1"]);
}

#[test]
fn a_satisfied_constraint_says_by_how_much_room_it_has() {
    let c = set(vec![want("j.place.x", &["J1"], Measure::X, 9.5, 11.0)]);
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
    let c = set(vec![want("a", &["J1"], Measure::X, 10.0004, 12.0)]);
    assert!(measure(&c, &board())[0].satisfied);
    let c = set(vec![want("a", &["J1"], Measure::X, 10.001, 12.0)]);
    assert!(!measure(&c, &board())[0].satisfied);
}

#[test]
fn the_fabrication_layer_is_the_body_when_the_footprint_draws_one() {
    // The drawing spans x from 11 to 17 and y from 11 to 15 for a footprint at (12, 12).
    let pcb = with_fab((12.0, 12.0), 0.0, false);
    let c = set(vec![
        want("u.place.left", &["U1"], Measure::Left, 11.0, 11.0),
        want("u.place.x", &["U1"], Measure::X, 14.0, 14.0),
        want("u.place.bottom", &["U1"], Measure::Bottom, 15.0, 15.0),
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
        Measure::Left,
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
        Measure::Rotation,
        270.0,
        270.0,
    )]);
    assert!(measure(&ok, &pcb)[0].satisfied);
    let wrapped = set(vec![want(
        "j.place.rotation",
        &["J1"],
        Measure::Rotation,
        -90.0,
        -90.0,
    )]);
    assert!(measure(&wrapped, &pcb)[0].satisfied);
    let bad = set(vec![want(
        "j.place.rotation",
        &["J1"],
        Measure::Rotation,
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

#[test]
fn a_module_is_measured_on_the_box_round_all_its_parts() {
    let mut b = Pcb::new(2);
    let pad = [smd("1", 0.0, 0.0, 0.5, 0.5, "A")];
    b.part("R1", (10.0, 10.0), 0.0, &pad, Draw::default());
    b.part("R2", (20.0, 18.0), 0.0, &pad, Draw::default());
    let c = set(vec![
        want("block.place.left", &["R1", "R2"], Measure::Left, 9.0, 9.0),
        want(
            "block.place.right",
            &["R1", "R2"],
            Measure::Right,
            21.0,
            21.0,
        ),
        want(
            "block.place.bottom",
            &["R1", "R2"],
            Measure::Bottom,
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
        severity: Severity::Error,
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
    let c = set(vec![want("x.place.x", &["U9"], Measure::X, 1.0, 2.0)]);
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
    let mut c = set(vec![want("j.place.left", &["J1"], Measure::Left, 2.0, 3.0)]);
    let art = Artwork::parse(&board()).unwrap();
    c.severity = Severity::Warning;
    assert_eq!(c.findings(&art)[0].severity, Severity::Warning);
    c.severity = Severity::Ignore;
    assert!(c.findings(&art).is_empty());
}

#[test]
fn an_open_range_has_one_end() {
    let c = set(vec![want(
        "j.place.left",
        &["J1"],
        Measure::Left,
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
    let c = set(vec![want("j.place.x", &["J1"], Measure::X, 10.0, 10.0)]);
    let o = &measure(&c, &pcb)[0];
    assert!(!o.satisfied);
    assert!(o.message.contains("no outline"), "{}", o.message);
}
