//! The fabricator's rules a project file cannot say (`drc::fab_rules`, issue #431): each is
//! checked only when the caller passes it, at its limit, and only against what the page says it
//! is about.

use crate::common;

use common::*;
use netsmith_geom::{drc_check, DrcExtras, DrcReport, DrcRule};

const NONE: Draw = Draw {
    courtyard: None,
    silk: None,
    back: false,
};

/// JLCPCB's two-layer figures, as `fab` passes them.
fn jlcpcb() -> DrcExtras {
    DrcExtras {
        hole_to_track: Some(0.2),
        plated_hole_clearance: Some(0.28),
        pad_to_pad: Some(0.15),
        pad_hole_to_hole: Some(0.45),
        mask_to_track: Some(0.09),
        plated_slot: Some(0.5),
        npth_slot: Some(1.0),
        npth_drill: Some(0.5),
        pad_ring: Some(0.18),
        ..DrcExtras::default()
    }
}

fn check(b: &mut Pcb, extras: &DrcExtras) -> DrcReport {
    let (pcb, pro) = (b.pcb(), b.pro());
    drc_check(&pcb, Some(&pro), extras).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_track_keeps_more_from_a_plated_hole_than_from_a_hole_with_no_plating() {
    // A plated pad 1.0 mm across with a 0.6 mm hole (0.2 mm ring), and a hole of 1 mm with no
    // copper. A track whose edge is `gap` from each hole's edge.
    for (gap, plated, bare) in [(0.3, 0, 0), (0.25, 1, 0), (0.15, 1, 1)] {
        let mut b = Pcb::new(2);
        b.part(
            "J1",
            (10.0, 10.0),
            0.0,
            &[thru("1", 0.0, 0.0, 1.0, 0.6, "A")],
            NONE,
        );
        b.part("H1", (20.0, 10.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
        let y = 10.0 + 0.3 + gap + 0.05;
        b.track("B", "F.Cu", (8.0, y), (12.0, y), 0.1);
        let y = 10.0 + 0.5 + gap + 0.05;
        b.track("B", "F.Cu", (18.0, y), (22.0, y), 0.1);
        let r = check(&mut b, &jlcpcb());
        let found: Vec<_> = r
            .findings
            .iter()
            .filter(|f| f.rule == DrcRule::HoleToTrack)
            .collect();
        let on = |part: &str| {
            found
                .iter()
                .filter(|f| f.items[1].part.as_deref() == Some(part))
                .count()
        };
        assert_eq!((on("J1"), on("H1")), (plated, bare), "gap {gap}\n{r}");
    }
}

#[test]
fn a_pad_of_a_footprint_may_sit_nearer_its_own_hole_than_a_track_may() {
    // A GND pad 0.175 mm from the footprint's own peg, as the USB-C receptacle draws it.
    let mut b = Pcb::new(2);
    b.part(
        "J1",
        (10.0, 10.0),
        0.0,
        &[hole(0.0, 0.0, 0.65), smd("1", 0.0, 0.825, 0.6, 0.6, "GND")],
        NONE,
    );
    let r = check(&mut b, &jlcpcb());
    assert_eq!(r.count(DrcRule::HoleToTrack), 0, "{r}");
}

#[test]
fn surface_pads_of_different_nets_keep_the_pad_to_pad_space() {
    for (gap, count) in [(0.15, 0), (0.14, 1)] {
        let mut b = Pcb::new(2);
        b.part(
            "U1",
            (10.0, 10.0),
            0.0,
            &[
                smd("1", 0.0, 0.0, 0.3, 1.0, "A"),
                smd("2", 0.3 + gap, 0.0, 0.3, 1.0, "B"),
            ],
            NONE,
        );
        let r = check(&mut b, &jlcpcb());
        assert_eq!(r.count(DrcRule::PadToPad), count, "gap {gap}\n{r}");
        let none = check(&mut b, &DrcExtras::default());
        assert_eq!(
            none.count(DrcRule::PadToPad),
            0,
            "not checked without the process"
        );
    }
}

#[test]
fn a_track_keeps_its_space_from_another_nets_mask_opening() {
    // The board's mask margin is 0, so the opening is the pad: a 0.6 mm pad at (10, 10).
    for (gap, count) in [(0.1, 0), (0.08, 1)] {
        let mut b = Pcb::new(2);
        b.stub("A", 10.0, 10.0);
        let y = 10.3 + gap + 0.05;
        b.track("B", "F.Cu", (8.0, y), (12.0, y), 0.1);
        let r = check(&mut b, &jlcpcb());
        assert_eq!(r.count(DrcRule::MaskToTrack), count, "gap {gap}\n{r}");
    }
}

#[test]
fn slots_small_holes_thin_rings_and_near_pad_holes_are_named() {
    let mut b = Pcb::new(2);
    // A plated slot 0.4 wide (the least is 0.5), a non-plated hole of 0.4 (0.5), a plated pad
    // with a ring of 0.1 (0.18).
    b.raw("(footprint \"Test:J9\" (layer \"F.Cu\") (at 30 10 0) (property \"Reference\" \"J9\" (at 0 0 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr through_hole) (pad \"1\" thru_hole oval (at 0 0) (size 1 2) (drill oval 0.4 1.4) (layers \"*.Cu\" \"*.Mask\")))");
    b.part("H1", (10.0, 10.0), 0.0, &[hole(0.0, 0.0, 0.4)], NONE);
    b.part(
        "J1",
        (20.0, 10.0),
        0.0,
        &[thru("1", 0.0, 0.0, 0.8, 0.6, "A")],
        NONE,
    );
    // A plated pad's hole 0.35 mm from a hole with no plating: the project file's 0.2 mm (the
    // vias' figure, as `fab` writes it) passes, the pad figure of 0.45 mm does not.
    b.rules.hole_to_hole = 0.2;
    b.part(
        "J2",
        (20.0, 20.0),
        0.0,
        &[thru("1", 0.0, 0.0, 1.0, 0.6, "B")],
        NONE,
    );
    b.part("H2", (20.95, 20.0), 0.0, &[hole(0.0, 0.0, 0.6)], NONE);
    let r = check(&mut b, &jlcpcb());
    assert_eq!(r.count(DrcRule::SlotWidth), 1, "{r}");
    assert_eq!(r.count(DrcRule::NpthDrill), 1, "{r}");
    assert_eq!(r.count(DrcRule::PlatedRing), 1, "{r}");
    assert_eq!(r.count(DrcRule::PadHoleToHole), 1, "{r}");
    assert_eq!(r.count(DrcRule::HoleToHole), 0, "{r}");
    assert!(
        r.findings
            .iter()
            .all(|f| f.rule != DrcRule::PlatedRing || f.items[0].part.as_deref() == Some("J1"))
    );
    let none = check(&mut b, &DrcExtras::default());
    for rule in [
        DrcRule::SlotWidth,
        DrcRule::NpthDrill,
        DrcRule::PlatedRing,
        DrcRule::PadHoleToHole,
    ] {
        assert_eq!(none.count(rule), 0, "{rule:?} without the process");
    }
}

#[test]
fn the_fabricators_rules_are_netsmiths_and_not_compared_with_kicad() {
    for rule in [
        DrcRule::HoleToTrack,
        DrcRule::PadToPad,
        DrcRule::PadHoleToHole,
        DrcRule::MaskToTrack,
        DrcRule::SlotWidth,
        DrcRule::NpthDrill,
        DrcRule::PlatedRing,
        DrcRule::TrackCurrent,
        DrcRule::ViaCurrent,
        DrcRule::TrackAngle,
    ] {
        assert!(rule.is_netsmiths(), "{rule:?}");
        assert_eq!(DrcRule::from_id(rule.id()), Some(rule));
    }
}

#[test]
fn a_track_narrower_than_its_current_needs_is_named_unless_it_is_near_its_pads() {
    // A 1 A net that needs 0.4 mm: a 0.2 mm track 10 mm long between two pads, and a 0.2 mm
    // neck of 0.5 mm out of a pad.
    let extras = DrcExtras {
        least_widths: vec![("P".to_string(), 0.4)],
        neck_reach: 1.0,
        ..jlcpcb()
    };
    let mut b = Pcb::new(2);
    b.stub("P", 10.0, 10.0);
    b.stub("P", 20.0, 10.0);
    b.track("P", "F.Cu", (10.0, 10.0), (10.5, 10.0), 0.2);
    b.track("P", "F.Cu", (10.5, 10.0), (20.0, 10.0), 0.4);
    let r = check(&mut b, &extras);
    assert_eq!(
        r.count(DrcRule::TrackCurrent),
        0,
        "a neck out of a pad passes\n{r}"
    );
    let mut b = Pcb::new(2);
    b.stub("P", 10.0, 10.0);
    b.stub("P", 20.0, 10.0);
    b.track("P", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.2);
    let r = check(&mut b, &extras);
    let found: Vec<_> = r
        .findings
        .iter()
        .filter(|f| f.rule == DrcRule::TrackCurrent)
        .collect();
    assert_eq!(found.len(), 1, "{r}");
    assert!(
        found[0].message.contains("0.2000 mm wide") && found[0].message.contains("0.4000 mm"),
        "{}",
        found[0].message
    );
    let none = check(&mut b, &jlcpcb());
    assert_eq!(
        none.count(DrcRule::TrackCurrent),
        0,
        "not checked when no current is known"
    );
}

#[test]
fn a_track_on_an_inner_signal_layer_is_held_to_the_width_its_current_needs_there() {
    // P needs 0.4 mm on an outer layer and 0.9 mm on an inner one (issue #833): a 0.5 mm track
    // passes on the front and is named on In2.Cu; a net with no inner width is held to its
    // outer one on an inner layer too.
    let extras = DrcExtras {
        least_widths: vec![("P".to_string(), 0.4), ("Q".to_string(), 0.4)],
        inner_least_widths: vec![("P".to_string(), 0.9)],
        neck_reach: 1.0,
        ..jlcpcb()
    };
    let found = |layer: &str, net: &'static str, width: f64| {
        let mut b = Pcb::new(4);
        b.stub(net, 10.0, 10.0);
        b.stub(net, 20.0, 10.0);
        b.track(net, layer, (10.0, 10.0), (20.0, 10.0), width);
        check(&mut b, &extras).count(DrcRule::TrackCurrent)
    };
    assert_eq!(found("F.Cu", "P", 0.5), 0);
    assert_eq!(found("In2.Cu", "P", 0.5), 1);
    assert_eq!(found("In2.Cu", "P", 0.9), 0);
    assert_eq!(found("In2.Cu", "Q", 0.3), 1);
    assert_eq!(found("In2.Cu", "Q", 0.5), 0);
}

#[test]
fn a_bend_of_45_degrees_passes_and_a_right_or_acute_corner_does_not_outside_a_pad() {
    let angles = DrcExtras {
        track_angle: true,
        ..DrcExtras::default()
    };
    type Segment = ((f64, f64), (f64, f64));
    let count = |segments: &[Segment]| {
        let mut b = Pcb::new(2);
        b.stub("A", 10.0, 10.0);
        b.stub("A", 20.0, 16.0);
        for &(a, c) in segments {
            b.track("A", "F.Cu", a, c, 0.2);
        }
        let r = check(&mut b, &angles);
        let none = check(&mut b, &DrcExtras::default());
        assert_eq!(none.count(DrcRule::TrackAngle), 0, "not checked unless asked");
        r.count(DrcRule::TrackAngle)
    };
    // Right 4, diagonal 6 by 6 (two 45 degree bends): passes.
    assert_eq!(
        count(&[((10.0, 10.0), (14.0, 10.0)), ((14.0, 10.0), (20.0, 16.0))]),
        0
    );
    // Right 10, then down 6: one right angle.
    assert_eq!(
        count(&[((10.0, 10.0), (20.0, 10.0)), ((20.0, 10.0), (20.0, 16.0))]),
        1
    );
    // Out to (24, 10) and back to the pad at (20, 16): acute.
    assert_eq!(
        count(&[((10.0, 10.0), (24.0, 10.0)), ((24.0, 10.0), (20.0, 16.0))]),
        1
    );
    // A right angle whose corner is inside the second pad is the pad's copper, not a corner.
    assert_eq!(
        count(&[
            ((10.0, 10.0), (14.0, 10.0)),
            ((14.0, 10.0), (19.9, 15.9)),
            ((19.9, 15.9), (20.1, 15.9)),
            ((20.1, 15.9), (20.1, 16.1)),
        ]),
        0
    );
    // Two tracks of the net over one another are copper twice, not a corner.
    assert_eq!(
        count(&[
            ((10.0, 10.0), (14.0, 10.0)),
            ((14.0, 10.0), (20.0, 16.0)),
            ((12.0, 10.0), (14.0, 10.0)),
        ]),
        0
    );
}

#[test]
fn a_tangent_arc_at_a_bend_is_no_corner_and_a_kinked_arc_is_one() {
    let angles = DrcExtras {
        track_angle: true,
        ..DrcExtras::default()
    };
    // The 45 degree bend at (14, 10) of a track from (10, 10) to (20, 16), cut by an arc of
    // radius 2 tangent to both straight pieces.
    let (radius, half) = (2.0_f64, 22.5_f64.to_radians());
    let cut = radius * half.tan();
    let d = std::f64::consts::FRAC_1_SQRT_2;
    let p = (14.0 - cut, 10.0);
    let q = (14.0 + cut * d, 10.0 + cut * d);
    let centre = (p.0, p.1 + radius);
    let out = ((14.0 - centre.0), (10.0 - centre.1));
    let n = out.0.hypot(out.1);
    let mid = (centre.0 + out.0 / n * radius, centre.1 + out.1 / n * radius);
    let count = |mid: (f64, f64)| {
        let mut b = Pcb::new(2);
        b.stub("A", 10.0, 10.0);
        b.stub("A", 20.0, 16.0);
        b.track("A", "F.Cu", (10.0, 10.0), p, 0.2);
        b.track("A", "F.Cu", q, (20.0, 16.0), 0.2);
        b.arc("A", "F.Cu", [p, mid, q], 0.2);
        check(&mut b, &angles).count(DrcRule::TrackAngle)
    };
    assert_eq!(count(mid), 0, "a tangent arc");
    // The same ends through a point well outside the bend: the arc leaves both straight
    // pieces at about a right angle.
    assert_eq!(count((14.5, 8.5)), 2, "a kinked arc");
}

/// JLCPCB's two-layer copper, rise and tolerance, as `fab` sizes a branch with (issue #1113).
fn sizing() -> netsmith_geom::ipc2221::TrackSizing {
    netsmith_geom::ipc2221::TrackSizing {
        copper: 0.035,
        inner_copper: None,
        rise: 10.0,
        tolerance: 0.2,
        via_drill: 0.3,
        via_plating: 0.018,
    }
}

/// Net P, which needs 0.4 mm and three vias side by side: a trunk of 0.4 mm from S1 to S2, and a
/// branch of `width` from its middle to the through-hole pad of J1, through one via.
fn branch_board(width: f64, trunk_neck: bool) -> Pcb {
    let mut b = Pcb::new(2);
    b.stub("P", 10.0, 10.0);
    b.stub("P", 30.0, 10.0);
    if trunk_neck {
        b.track("P", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.4);
        b.track("P", "F.Cu", (20.0, 10.0), (30.0, 10.0), width);
    } else {
        b.track("P", "F.Cu", (10.0, 10.0), (30.0, 10.0), 0.4);
    }
    b.part(
        "J1",
        (20.0, 22.0),
        0.0,
        &[thru("1", 0.0, 0.0, 1.2, 0.7, "P")],
        NONE,
    );
    b.track("P", "F.Cu", (20.0, 10.0), (20.0, 16.0), width);
    b.via("P", (20.0, 16.0), 0.6, 0.3);
    b.track("P", "B.Cu", (20.0, 16.0), (20.0, 22.0), width);
    b
}

fn branch_extras(current: Option<f64>) -> DrcExtras {
    DrcExtras {
        least_widths: vec![("P".to_string(), 0.4)],
        via_counts: vec![("P".to_string(), 3)],
        neck_reach: 0.5,
        branches: current
            .map(|current| netsmith_geom::DrcBranch {
                net: "P".to_string(),
                part: "J1".to_string(),
                pads: vec!["1".to_string()],
                current,
            })
            .into_iter()
            .collect(),
        sizing: Some(sizing()),
        ..jlcpcb()
    }
}

#[test]
fn the_copper_of_a_branch_is_held_to_the_current_its_pin_states_and_not_to_its_nets() {
    // Issue #1113: a 0.2 mm branch with one via to a pin that states 1 mA, on a net that needs
    // 0.4 mm and three vias. Its two tracks and its via pass; with no statement they are the
    // net's, and are named.
    let counts = |b: &mut Pcb, extras: &DrcExtras| {
        let r = check(b, extras);
        (r.count(DrcRule::TrackCurrent), r.count(DrcRule::ViaCurrent))
    };
    let mut b = branch_board(0.2, false);
    assert_eq!(counts(&mut b, &branch_extras(Some(0.001))), (0, 0));
    assert_eq!(counts(&mut b, &branch_extras(None)), (2, 1));
    // A branch that states more than its copper carries: 2 A needs 0.98 mm and four vias.
    assert_eq!(counts(&mut b, &branch_extras(Some(2.0))), (2, 1));
    // Copper of the trunk is the net's whatever the branch states: a 0.2 mm piece of it between
    // two pads of no branch is named.
    let mut b = branch_board(0.2, true);
    assert_eq!(counts(&mut b, &branch_extras(Some(0.001))), (1, 0));
    // With no sizing the branches are not read.
    let mut b = branch_board(0.2, false);
    let blind = DrcExtras {
        sizing: None,
        ..branch_extras(Some(0.001))
    };
    assert_eq!(counts(&mut b, &blind), (2, 1));
}
