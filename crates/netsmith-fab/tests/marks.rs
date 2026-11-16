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
