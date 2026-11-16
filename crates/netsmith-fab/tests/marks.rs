//! The fabricator's rules for fiducials and test points (`drc::marks`, issue #761): each is
//! checked only when the caller passes `marks`, at its limit, and a limit is an error where a
//! guideline is a warning.

use crate::common;

use common::*;
use netsmith_geom::{DrcExtras, DrcMarks, DrcReport, DrcRule, DrcSeverity, drc_check};

/// JLCPCB's figures, as `fab` passes them, for the footprints `FID1`, `FID2`, `TP1` and `TP2`.
fn jlcpcb() -> DrcExtras {
    DrcExtras {
        marks: Some(DrcMarks {
            fiducials: vec!["FID1".into(), "FID2".into(), "FID3".into()],
            test_points: vec!["TP1".into(), "TP2".into()],
            assembled_front: false,
            assembled_back: false,
            min_fiducials: 2,
            fiducial_diameter: 1.0,
            fiducial_mask_ratio: 2.0,
            min_fiducial_edge: 3.35,
            min_test_point_pad: 0.8,
            min_test_point_pitch: 1.27,
            min_test_point_body: 0.5,
            min_test_point_edge: 3.2,
            power_nets: vec!["3V3".into(), "GND".into()],
            min_power_test_point_pad: 1.0,
            tall_parts: vec![("TALL1".into(), 8.0)],
            tall_part_height: 6.4,
            min_test_point_tall_body: 2.0,
            round_angle: 90.0,
            fine_pitch: 0.4,
            uneven_finish: None,
        }),
        ..DrcExtras::default()
    }
}

fn check(b: &mut Pcb, extras: &DrcExtras) -> DrcReport {
    let (pcb, pro) = (b.pcb(), b.pro());
    drc_check(&pcb, Some(&pro), extras).unwrap_or_else(|e| panic!("{e}"))
}

/// A fiducial of `copper` mm with a mask margin of `margin` mm all round, on the front or the
/// back, centred at (x, y).
fn fiducial(b: &mut Pcb, refdes: &str, at: (f64, f64), copper: f64, margin: f64, back: bool) {
    let (cu, mask) = if back {
        ("B.Cu", "B.Mask")
    } else {
        ("F.Cu", "F.Mask")
    };
    b.raw(&format!(
        "(footprint \"Fiducial:Fiducial_1mm_Mask2mm\" (layer \"{cu}\") (at {} {} 0) (property \"Reference\" \"{refdes}\" (at 0 0 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr smd exclude_from_pos_files exclude_from_bom) (pad \"\" smd circle (at 0 0) (size {copper} {copper}) (layers \"{cu}\" \"{mask}\") (solder_mask_margin {margin})))",
        at.0, at.1
    ));
}

/// A test point: one round pad of `d` mm.
fn test_point(b: &mut Pcb, refdes: &str, at: (f64, f64), d: f64, back: bool) {
    let (cu, mask) = if back {
        ("B.Cu", "B.Mask")
    } else {
        ("F.Cu", "F.Mask")
    };
    b.raw(&format!(
        "(footprint \"TestPoint:TestPoint_Pad\" (layer \"{cu}\") (at {} {} 0) (property \"Reference\" \"{refdes}\" (at 0 0 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr smd exclude_from_pos_files exclude_from_bom) (pad \"1\" smd circle (at 0 0) (size {d} {d}) (layers \"{cu}\" \"{mask}\")))",
        at.0, at.1
    ));
}

fn count(r: &DrcReport, rule: DrcRule) -> usize {
    r.count(rule)
}

#[test]
fn the_rules_are_netsmiths_and_a_limit_is_an_error_where_a_guideline_is_a_warning() {
    let limits = [
        DrcRule::FiducialEdge,
        DrcRule::FiducialMask,
        DrcRule::FiducialKeepout,
    ];
    let guidelines = [
        DrcRule::FiducialCount,
        DrcRule::FiducialSize,
        DrcRule::FiducialAngle,
        DrcRule::FinishFinePitch,
        DrcRule::TestPointPad,
        DrcRule::TestPointPitch,
        DrcRule::TestPointBody,
        DrcRule::TestPointEdge,
    ];
    for rule in limits.into_iter().chain(guidelines) {
        assert!(rule.is_netsmiths(), "{rule:?}");
        assert_eq!(DrcRule::from_id(rule.id()), Some(rule));
    }
    for rule in limits {
        assert_eq!(rule.default_severity(), Some(DrcSeverity::Error), "{rule:?}");
    }
    for rule in guidelines {
        assert_eq!(
            rule.default_severity(),
            Some(DrcSeverity::Warning),
            "{rule:?}"
        );
    }
}

#[test]
fn a_fiducial_keeps_its_copper_3_35_mm_from_the_board_edge() {
    // The board is 40 x 30 mm: a 1 mm fiducial centred `x` from the left edge has its copper
    // `x - 0.5` from it.
    for (x, found) in [(3.85, 0), (3.8, 1)] {
        let mut b = Pcb::new(2);
        fiducial(&mut b, "FID1", (x, 15.0), 1.0, 0.5, false);
        let r = check(&mut b, &jlcpcb());
        assert_eq!(count(&r, DrcRule::FiducialEdge), found, "x {x}\n{r}");
        if found == 1 {
            let f = &r.findings[0];
            assert_eq!(f.severity, DrcSeverity::Error, "{r}");
            assert!(f.message.contains("3.35"), "{}", f.message);
        }
        let none = check(&mut b, &DrcExtras::default());
        assert_eq!(count(&none, DrcRule::FiducialEdge), 0, "not checked without the process");
    }
}

#[test]
fn a_fiducial_has_no_solder_mask_over_its_copper() {
    // An opening of 1 mm round 1 mm of copper (margin 0) is the copper exactly; a negative
    // margin leaves mask over its edge; a pad with no mask layer is covered whole.
    let mut b = Pcb::new(2);
    fiducial(&mut b, "FID1", (10.0, 15.0), 1.0, 0.0, false);
    fiducial(&mut b, "FID2", (20.0, 15.0), 1.0, -0.1, false);
    b.raw("(footprint \"Fiducial:Covered\" (layer \"F.Cu\") (at 30 15 0) (property \"Reference\" \"FID3\" (at 0 0 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr smd exclude_from_bom) (pad \"\" smd circle (at 0 0) (size 1 1) (layers \"F.Cu\")))");
    let r = check(&mut b, &jlcpcb());
    let covered: Vec<_> = r
        .findings
        .iter()
        .filter(|f| f.rule == DrcRule::FiducialMask)
        .map(|f| f.items[0].part.clone().unwrap())
        .collect();
    assert_eq!(covered, ["FID2", "FID3"], "{r}");
    assert!(r.errors().any(|f| f.rule == DrcRule::FiducialMask));
}

#[test]
fn nothing_else_has_copper_in_a_fiducials_mask_opening() {
    // A 1 mm fiducial with a 2 mm opening (margin 0.5) at (10, 15): a track, a via and a pad of
    // another footprint 0.7 mm from its centre are inside the opening; one 1.2 mm away is not.
    let mut b = Pcb::new(2);
    fiducial(&mut b, "FID1", (10.0, 15.0), 1.0, 0.5, false);
    b.track("A", "F.Cu", (10.7, 12.0), (10.7, 18.0), 0.2);
    let r = check(&mut b, &jlcpcb());
    assert_eq!(count(&r, DrcRule::FiducialKeepout), 1, "{r}");
    assert!(r.errors().any(|f| f.rule == DrcRule::FiducialKeepout));

    let mut b = Pcb::new(2);
    fiducial(&mut b, "FID1", (10.0, 15.0), 1.0, 0.5, false);
    b.via("A", (10.0, 15.7), 0.45, 0.2);
    b.stub("B", 9.3, 15.0);
    let r = check(&mut b, &jlcpcb());
    assert_eq!(count(&r, DrcRule::FiducialKeepout), 2, "{r}");

    let mut b = Pcb::new(2);
    fiducial(&mut b, "FID1", (10.0, 15.0), 1.0, 0.5, false);
    b.track("A", "F.Cu", (11.3, 12.0), (11.3, 18.0), 0.2);
    // A track on the back does not touch the front's opening.
    b.track("B", "B.Cu", (10.3, 12.0), (10.3, 18.0), 0.2);
    let r = check(&mut b, &jlcpcb());
    assert_eq!(count(&r, DrcRule::FiducialKeepout), 0, "{r}");
}

#[test]
fn each_assembled_side_has_two_fiducials() {
    for (have, front, back, found) in [
        (2, true, false, 0),
        (1, true, false, 1),
        (0, true, false, 1),
        (0, false, false, 0),
        (2, true, true, 1),
    ] {
        let mut b = Pcb::new(2);
        for i in 0..have {
            fiducial(&mut b, &format!("FID{}", i + 1), (5.0 + 10.0 * i as f64, 5.0), 1.0, 0.5, false);
        }
        let mut extras = jlcpcb();
        let marks = extras.marks.as_mut().unwrap();
        marks.assembled_front = front;
        marks.assembled_back = back;
        let r = check(&mut b, &extras);
        assert_eq!(count(&r, DrcRule::FiducialCount), found, "{have} {front} {back}\n{r}");
        for f in r.findings.iter().filter(|f| f.rule == DrcRule::FiducialCount) {
            assert_eq!(f.severity, DrcSeverity::Warning);
        }
    }
}

#[test]
fn a_fiducial_is_the_recommended_size_with_a_mask_opening_twice_it() {
    // 1 mm of copper with a 2 mm opening is as recommended; 1.5 mm is not; a 1.5 mm opening
    // round 1 mm of copper is too narrow.
    let mut b = Pcb::new(2);
    fiducial(&mut b, "FID1", (5.0, 5.0), 1.0, 0.5, false);
    let r = check(&mut b, &jlcpcb());
    assert_eq!(count(&r, DrcRule::FiducialSize), 0, "{r}");

    let mut b = Pcb::new(2);
    fiducial(&mut b, "FID1", (5.0, 5.0), 1.5, 0.75, false);
    let r = check(&mut b, &jlcpcb());
    assert_eq!(count(&r, DrcRule::FiducialSize), 1, "{r}");

    let mut b = Pcb::new(2);
    fiducial(&mut b, "FID1", (5.0, 5.0), 1.0, 0.25, false);
    let r = check(&mut b, &jlcpcb());
    assert_eq!(count(&r, DrcRule::FiducialSize), 1, "{r}");
    assert!(r.findings.iter().all(|f| f.severity != DrcSeverity::Error), "{r}");
}

#[test]
fn a_test_point_is_at_least_0_8_mm_wide() {
    for (d, found) in [(0.8, 0), (0.7, 1)] {
        let mut b = Pcb::new(2);
        test_point(&mut b, "TP1", (10.0, 15.0), d, false);
        let r = check(&mut b, &jlcpcb());
        assert_eq!(count(&r, DrcRule::TestPointPad), found, "d {d}\n{r}");
        let none = check(&mut b, &DrcExtras::default());
        assert_eq!(count(&none, DrcRule::TestPointPad), 0);
    }
}

#[test]
fn test_points_on_a_side_are_1_27_mm_apart() {
    for (dx, back, found) in [(1.27, false, 0), (1.2, false, 1), (1.2, true, 0)] {
        let mut b = Pcb::new(2);
        test_point(&mut b, "TP1", (10.0, 15.0), 1.0, false);
        test_point(&mut b, "TP2", (10.0 + dx, 15.0), 1.0, back);
        let r = check(&mut b, &jlcpcb());
        assert_eq!(count(&r, DrcRule::TestPointPitch), found, "dx {dx} back {back}\n{r}");
    }
}

#[test]
fn a_test_point_is_half_a_millimetre_from_the_body_of_a_part() {
    // A part with a 2 x 1.4 mm courtyard at (20, 15): its body reaches x = 19 to 21. A test
    // point 1 mm wide whose copper is `gap` from it.
    for (gap, found) in [(0.5, 0), (0.4, 1)] {
        let mut b = Pcb::new(2);
        b.part(
            "U1",
            (20.0, 15.0),
            0.0,
            &[smd("1", 0.0, 0.0, 0.6, 0.6, "A")],
            Draw::default(),
        );
        test_point(&mut b, "TP1", (21.0 + gap + 0.5, 15.0), 1.0, false);
        let r = check(&mut b, &jlcpcb());
        assert_eq!(count(&r, DrcRule::TestPointBody), found, "gap {gap}\n{r}");
    }
    // A body on the other side of the board is not next to it.
    let mut b = Pcb::new(2);
    b.part(
        "U1",
        (20.0, 15.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "A")],
        Draw {
            back: true,
            ..Draw::default()
        },
    );
    test_point(&mut b, "TP1", (21.7, 15.0), 1.0, false);
    let r = check(&mut b, &jlcpcb());
    assert_eq!(count(&r, DrcRule::TestPointBody), 0, "{r}");
}

#[test]
fn a_test_point_is_3_2_mm_from_the_board_edge() {
    for (x, found) in [(3.7, 0), (3.6, 1)] {
        let mut b = Pcb::new(2);
        test_point(&mut b, "TP1", (x, 15.0), 1.0, false);
        let r = check(&mut b, &jlcpcb());
        assert_eq!(count(&r, DrcRule::TestPointEdge), found, "x {x}\n{r}");
        assert!(r.findings.iter().all(|f| f.severity == DrcSeverity::Warning), "{r}");
    }
}

#[test]
fn a_test_point_on_a_power_or_ground_net_is_1_mm_wide() {
    // The same pad: a signal net needs 0.8 mm, a supply or a ground 1.0 mm.
    for (net, d, found) in [
        ("3V3", 1.0, 0),
        ("3V3", 0.9, 1),
        ("GND", 0.9, 1),
        ("SIG", 0.9, 0),
        ("SIG", 0.7, 1),
    ] {
        let mut b = Pcb::new(2);
        b.part("TP1", (10.0, 15.0), 0.0, &[smd("1", 0.0, 0.0, d, d, net)], Draw::default());
        let r = check(&mut b, &jlcpcb());
        assert_eq!(count(&r, DrcRule::TestPointPad), found, "{net} {d}\n{r}");
        assert!(r.findings.iter().all(|f| f.severity == DrcSeverity::Warning), "{r}");
    }
    // A power test point's finding names the larger figure; a process that states none (0)
    // judges every net by the signal pad.
    let mut b = Pcb::new(2);
    b.part("TP1", (10.0, 15.0), 0.0, &[smd("1", 0.0, 0.0, 0.9, 0.9, "3V3")], Draw::default());
    let r = check(&mut b, &jlcpcb());
    let finding = r.findings.iter().find(|f| f.rule == DrcRule::TestPointPad).expect("a finding");
    assert!(
        finding.message.contains("0.9000 mm wide")
            && finding.message.contains("at least 1.00 mm on a power or ground net"),
        "{}",
        finding.message
    );
    let mut none = jlcpcb();
    none.marks.as_mut().unwrap().min_power_test_point_pad = 0.0;
    assert_eq!(count(&check(&mut b, &none), DrcRule::TestPointPad), 0);
}

#[test]
fn a_test_point_is_2_mm_from_the_body_of_a_part_taller_than_6_4_mm() {
    // A 2 x 1.4 mm body at (20, 15), reaching x = 19 to 21: a test point 1 mm wide whose copper
    // is `gap` from it, beside a part that is tall, one that is not and one of unknown height.
    for (part, gap, found) in [
        ("TALL1", 2.0, 0),
        ("TALL1", 1.9, 1),
        ("TALL1", 0.4, 1),
        // Not tall (or of unknown height, which is not in the list): the 0.5 mm of any part.
        ("U1", 1.9, 0),
        ("U1", 0.5, 0),
        ("U1", 0.4, 1),
    ] {
        let mut b = Pcb::new(2);
        b.part(part, (20.0, 15.0), 0.0, &[smd("1", 0.0, 0.0, 0.6, 0.6, "A")], Draw::default());
        test_point(&mut b, "TP1", (21.0 + gap + 0.5, 15.0), 1.0, false);
        let r = check(&mut b, &jlcpcb());
        assert_eq!(count(&r, DrcRule::TestPointBody), found, "{part} gap {gap}\n{r}");
        assert!(r.findings.iter().all(|f| f.severity == DrcSeverity::Warning), "{r}");
    }
    // The finding names the part's height and both figures.
    let mut b = Pcb::new(2);
    b.part("TALL1", (20.0, 15.0), 0.0, &[smd("1", 0.0, 0.0, 0.6, 0.6, "A")], Draw::default());
    test_point(&mut b, "TP1", (22.5, 15.0), 1.0, false);
    let r = check(&mut b, &jlcpcb());
    let finding = r.findings.iter().find(|f| f.rule == DrcRule::TestPointBody).expect("a finding");
    assert!(
        finding.message.contains("body of TALL1")
            && finding.message.contains("at least 2.00 mm")
            && finding.message.contains("8.00 mm tall, above 6.4 mm"),
        "{}",
        finding.message
    );
    // A process that states no larger space for a tall part judges it as any other.
    let mut same = jlcpcb();
    same.marks.as_mut().unwrap().min_test_point_tall_body = 0.5;
    assert_eq!(count(&check(&mut b, &same), DrcRule::TestPointBody), 0);
}

/// A round board of 40 mm across, written as a circle on `Edge.Cuts` in a square outline of its
/// own box, with a fiducial of the front at each of `degrees` about its centre, 16 mm from it
/// (angles counter-clockwise from the right).
fn round_board(degrees: &[f64], back: bool) -> Pcb {
    let mut b = Pcb::new(2);
    b.outline = (0.0, 0.0, 40.0, 40.0);
    b.raw("(gr_circle (center 20 20) (end 40 20) (stroke (width 0.05) (type default)) (fill none) (layer \"Edge.Cuts\"))");
    for (i, d) in degrees.iter().enumerate() {
        let a = d.to_radians();
        let at = (20.0 + 16.0 * a.cos(), 20.0 - 16.0 * a.sin());
        fiducial(&mut b, &format!("FID{}", i + 1), at, 1.0, 0.5, back);
    }
    b
}

#[test]
fn the_fiducials_of_a_round_board_are_three_in_an_l_about_90_degrees_apart() {
    for (degrees, found) in [
        // The page's own example: 0, 90 and 180 degrees.
        (vec![0.0, 90.0, 180.0], 0),
        // The same turned: any corner with a neighbour 90 degrees each way.
        (vec![200.0, 290.0, 20.0], 0),
        // Within the tolerance of 10 degrees, and just over it.
        (vec![0.0, 99.0, 180.0], 0),
        (vec![0.0, 101.0, 180.0], 1),
        // Evenly spread, a straight line, two, and a corner with one neighbour.
        (vec![0.0, 120.0, 240.0], 1),
        (vec![0.0, 180.0, 135.0], 1),
        (vec![0.0, 90.0], 1),
        (vec![45.0], 1),
        // More than three: one corner with its two neighbours is enough.
        (vec![0.0, 90.0, 180.0, 270.0], 0),
        // None is the count's finding, not this one's.
        (vec![], 0),
    ] {
        let mut b = round_board(&degrees, false);
        let r = check(&mut b, &jlcpcb());
        assert_eq!(count(&r, DrcRule::FiducialAngle), found, "{degrees:?}\n{r}");
        for f in r.findings.iter().filter(|f| f.rule == DrcRule::FiducialAngle) {
            assert_eq!(f.severity, DrcSeverity::Warning);
            assert!(f.message.contains("round") && f.message.contains("90"), "{}", f.message);
        }
    }
    // Each side by itself: a good front and a bad back.
    let mut b = round_board(&[0.0, 90.0, 180.0], false);
    fiducial(&mut b, "FID4", (30.0, 20.0), 1.0, 0.5, true);
    let mut with_four = jlcpcb();
    with_four.marks.as_mut().unwrap().fiducials.push("FID4".into());
    let r = check(&mut b, &with_four);
    assert_eq!(count(&r, DrcRule::FiducialAngle), 1, "{r}");
    assert!(r.findings.iter().any(|f| f.message.contains("the back")), "{r}");
    // Another angle of the process, and no figure: not checked.
    let mut b = round_board(&[0.0, 120.0, 240.0], false);
    let mut other = jlcpcb();
    other.marks.as_mut().unwrap().round_angle = 120.0;
    assert_eq!(count(&check(&mut b, &other), DrcRule::FiducialAngle), 0);
    other.marks.as_mut().unwrap().round_angle = 0.0;
    let mut b = round_board(&[0.0, 45.0], false);
    assert_eq!(count(&check(&mut b, &other), DrcRule::FiducialAngle), 0);
    assert_eq!(
        count(&check(&mut b, &DrcExtras::default()), DrcRule::FiducialAngle),
        0,
        "not checked without the process"
    );
}

#[test]
fn a_board_that_is_not_round_has_no_angle_to_keep() {
    // A rectangle, with two fiducials: the corner rule does not apply.
    let mut b = Pcb::new(2);
    fiducial(&mut b, "FID1", (5.0, 5.0), 1.0, 0.5, false);
    fiducial(&mut b, "FID2", (35.0, 25.0), 1.0, 0.5, false);
    assert_eq!(count(&check(&mut b, &jlcpcb()), DrcRule::FiducialAngle), 0);
    // A rectangle with a round hole cut out of it is not round either.
    b.raw("(gr_circle (center 20 15) (end 25 15) (stroke (width 0.05) (type default)) (fill none) (layer \"Edge.Cuts\"))");
    assert_eq!(count(&check(&mut b, &jlcpcb()), DrcRule::FiducialAngle), 0);
    // A circle drawn as two arcs is round.
    let mut b = Pcb::new(2);
    b.outline = (0.0, 0.0, 40.0, 40.0);
    b.raw("(gr_arc (start 40 20) (mid 20 0) (end 0 20) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\"))");
    b.raw("(gr_arc (start 0 20) (mid 20 40) (end 40 20) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\"))");
    fiducial(&mut b, "FID1", (20.0, 4.0), 1.0, 0.5, false);
    assert_eq!(count(&check(&mut b, &jlcpcb()), DrcRule::FiducialAngle), 1);
}

/// A part of two pads `pitch` mm apart (and a third far away), on the front.
fn fine_part(b: &mut Pcb, refdes: &str, pitch: f64) {
    b.part(
        refdes,
        (20.0, 15.0),
        0.0,
        &[
            smd("1", 0.0, 0.0, 0.2, 0.6, "A"),
            smd("2", pitch, 0.0, 0.2, 0.6, "B"),
            smd("3", 5.0, 0.0, 0.2, 0.6, "C"),
        ],
        Draw::default(),
    );
}

fn hasl() -> DrcExtras {
    let mut extras = jlcpcb();
    let marks = extras.marks.as_mut().unwrap();
    marks.assembled_front = true;
    marks.assembled_back = true;
    marks.uneven_finish = Some("HASL lead-free".into());
    extras
}

#[test]
fn hasl_is_advised_against_for_a_part_of_ultra_fine_pitch() {
    for (pitch, found) in [(0.4, 1), (0.35, 1), (0.45, 0), (0.5, 0), (1.27, 0)] {
        let mut b = Pcb::new(2);
        fine_part(&mut b, "U1", pitch);
        let r = check(&mut b, &hasl());
        assert_eq!(count(&r, DrcRule::FinishFinePitch), found, "pitch {pitch}\n{r}");
        for f in r.findings.iter().filter(|f| f.rule == DrcRule::FinishFinePitch) {
            assert_eq!(f.severity, DrcSeverity::Warning);
            assert!(
                f.message.contains("U1")
                    && f.message.contains("HASL lead-free")
                    && f.message.contains("ENIG or OSP"),
                "{}",
                f.message
            );
        }
    }
    // One finding for the board, whatever the number of parts, at the finest.
    let mut b = Pcb::new(2);
    fine_part(&mut b, "U1", 0.4);
    b.part("U2", (30.0, 15.0), 0.0, &[smd("1", 0.0, 0.0, 0.2, 0.6, "A"), smd("2", 0.3, 0.0, 0.2, 0.6, "B")], Draw::default());
    let r = check(&mut b, &hasl());
    assert_eq!(count(&r, DrcRule::FinishFinePitch), 1, "{r}");
    assert!(r.findings.iter().any(|f| f.message.starts_with("U2")), "{r}");
}

#[test]
fn a_flat_finish_or_a_side_nothing_is_assembled_on_is_not_advised_against() {
    let mut b = Pcb::new(2);
    fine_part(&mut b, "U1", 0.4);
    // ENIG and OSP are `None`; a process with no pitch is not checked; nor is a board read with
    // no process.
    let mut flat = hasl();
    flat.marks.as_mut().unwrap().uneven_finish = None;
    assert_eq!(count(&check(&mut b, &flat), DrcRule::FinishFinePitch), 0);
    let mut none = hasl();
    none.marks.as_mut().unwrap().fine_pitch = 0.0;
    assert_eq!(count(&check(&mut b, &none), DrcRule::FinishFinePitch), 0);
    assert_eq!(count(&check(&mut b, &DrcExtras::default()), DrcRule::FinishFinePitch), 0);
    // A side the board house assembles nothing on, and a hole's pads.
    let mut front_open = hasl();
    front_open.marks.as_mut().unwrap().assembled_front = false;
    assert_eq!(count(&check(&mut b, &front_open), DrcRule::FinishFinePitch), 0);
    let mut b = Pcb::new(2);
    b.part(
        "J1",
        (20.0, 15.0),
        0.0,
        &[thru("1", 0.0, 0.0, 0.6, 0.3, "A"), thru("2", 0.4, 0.0, 0.6, 0.3, "B")],
        Draw::default(),
    );
    assert_eq!(count(&check(&mut b, &hasl()), DrcRule::FinishFinePitch), 0);
}

#[test]
fn a_v_cut_line_is_not_the_board_edge_a_fiducial_keeps_away_from() {
    // A panel's rail keeps its fiducial 0.65 mm from the line that cuts it off, and the space the
    // page asks is to the panel's own edge.
    for (x, found) in [(3.85, 0), (3.8, 1)] {
        let mut b = Pcb::new(2);
        // The outline as a panel draws it, one rectangle, and the line across it.
        b.raw("(gr_rect (start 0 0) (end 40 30) (stroke (width 0.05) (type default)) (fill no) (layer \"Edge.Cuts\"))");
        b.edge_line((5.0, 0.0), (5.0, 30.0));
        fiducial(&mut b, "FID1", (x, 15.0), 1.0, 0.5, false);
        let r = check(&mut b, &jlcpcb());
        assert_eq!(count(&r, DrcRule::FiducialEdge), found, "x {x}\n{r}");
    }
    // A line that is not a cut (an end off the outline) is an edge.
    let mut b = Pcb::new(2);
    b.raw("(gr_rect (start 0 0) (end 40 30) (stroke (width 0.05) (type default)) (fill no) (layer \"Edge.Cuts\"))");
    b.edge_line((5.0, 10.0), (5.0, 20.0));
    fiducial(&mut b, "FID1", (3.85, 15.0), 1.0, 0.5, false);
    let r = check(&mut b, &jlcpcb());
    assert_eq!(count(&r, DrcRule::FiducialEdge), 1, "{r}");
}
