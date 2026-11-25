//! The fabricator's rules a project file cannot say (`drc::fab_rules`, issue #431): each is
//! checked only when the caller passes it, at its limit, and only against what the page says it
//! is about.

use crate::common;

use common::*;
use netsmith_geom::{drc_check, DrcExtras, DrcNetCurrent, DrcReport, DrcRule};

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
fn a_pads_offset_moves_the_copper_the_pad_to_pad_space_is_kept_from() {
    // Issue 1322: the pads' files places are 0.5 mm apart edge to edge (0.3 mm wide pads at
    // 0 and 0.8); the second pad's `(drill (offset -0.36 0))` puts its copper 0.14 mm from
    // the first, nearer than the process asks.
    for (offset, count) in [("", 0), (" (drill (offset -0.36 0))", 1)] {
        let mut b = Pcb::new(2);
        let (a, c) = (b.net_ref(Some("A")), b.net_ref(Some("B")));
        b.raw(&format!(
            "(footprint \"Test:U1\" (layer \"F.Cu\") (at 10 10 0)
            (property \"Reference\" \"U1\" (at 0 -3 0) (layer \"F.Fab\") (hide yes))
            (attr smd)
            (pad \"1\" smd rect (at 0 0) (size 0.3 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\") {a})
            (pad \"2\" smd rect (at 0.8 0) (size 0.3 1){offset} (layers \"F.Cu\" \"F.Mask\" \"F.Paste\") {c}))"
        ));
        let r = check(&mut b, &jlcpcb());
        assert_eq!(r.count(DrcRule::PadToPad), count, "{offset}\n{r}");
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
fn the_fabricators_rules_are_netsmiths_and_kicad_checks_those_of_the_rules_file() {
    // KiCad's project file has none of them; the `.kicad_dru` beside it holds the first six for
    // KiCad's check (issue #1185), which the cross-check then compares.
    for (rule, in_rules_file) in [
        (DrcRule::HoleToTrack, true),
        (DrcRule::PadToPad, true),
        (DrcRule::PadHoleToHole, true),
        (DrcRule::SlotWidth, true),
        (DrcRule::NpthDrill, true),
        (DrcRule::PlatedRing, true),
        (DrcRule::MaskToTrack, false),
        (DrcRule::TrackCurrent, false),
        (DrcRule::ViaCurrent, false),
        (DrcRule::TrackAngle, false),
    ] {
        assert!(rule.is_netsmiths(), "{rule:?}");
        assert_eq!(netsmith_fab::in_kicad_dru(rule), in_rules_file, "{rule:?}");
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
fn a_track_finding_names_the_width_the_width_needed_and_the_current_assumed() {
    // Issue 1373: net P carries 1 A (two rails) and needs 0.4 mm; a track 0.2 mm wide says what
    // it is, what the current needs and on what basis, so that a person can choose between a
    // wider track and a lower current.
    let from = "the sum of the currents on it with every load on at once: rail `a` 600 mA + rail `b` 400 mA";
    let extras = DrcExtras {
        least_widths: vec![("P".to_string(), 0.4)],
        neck_reach: 1.0,
        sizing: Some(sizing()),
        net_currents: vec![DrcNetCurrent {
            net: "P".to_string(),
            current: 1.0,
            source: from.to_string(),
            ..Default::default()
        }],
        ..jlcpcb()
    };
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
    let said = &found[0].message;
    for part in [
        "is 0.2000 mm wide",
        "its net `P` carries 1 A (the sum of the currents on it with every load on at once: rail `a` 600 mA + rail `b` 400 mA)",
        "which needs 0.4000 mm",
        "IPC-2221, 35 \u{3bc}m outer copper, a 10 K temperature rise, the fabricator's 20% width tolerance",
        // Issue 1514: the reach says what it is measured from.
        "only within 1.00 mm of the box round the pad centres of a part on the net (measured from the centres of its pads, not from their edges) may a track be narrower",
        "widen it to 0.40 mm or more, or lower the current",
    ] {
        assert!(said.contains(part), "{part}: {said}");
    }
    let c = found[0].carrying.as_ref().expect("the numbers");
    assert_eq!(c.width, Some(0.2));
    assert_eq!(c.needed_width, Some(0.4));
    assert_eq!(c.current, Some(1.0));
    assert_eq!(c.current_from, from);
    assert_eq!((c.copper, c.rise, c.tolerance), (Some(0.035), Some(10.0), Some(0.2)));
    // With no current known the finding still names the widths.
    let blind = DrcExtras {
        net_currents: Vec::new(),
        sizing: None,
        ..extras
    };
    let r = check(&mut b, &blind);
    let c = r.findings.iter().find(|f| f.rule == DrcRule::TrackCurrent).unwrap();
    assert!(c.message.contains("its net's current needs 0.4000 mm"), "{}", c.message);
    assert_eq!(c.carrying.as_ref().unwrap().current, None);
}

#[test]
fn the_findings_of_copper_that_feeds_branches_name_the_pins_and_what_they_state() {
    // Issue 1373: a branch that states 2 A needs 0.98 mm and three vias, and the findings of the
    // copper say so, with the pin that states it, and not the net's current, which is a lower
    // bound here (a current on it is not known).
    let extras = DrcExtras {
        net_currents: vec![DrcNetCurrent {
            net: "P".to_string(),
            current: 1.0,
            source: "rail `a` 1 A".to_string(),
            lower_bound: true,
        }],
        ..branch_extras(Some(2.0))
    };
    let mut b = branch_board(0.2, false);
    let r = check(&mut b, &extras);
    let track = r.findings.iter().find(|f| f.rule == DrcRule::TrackCurrent).unwrap();
    for part in [
        "it only feeds branches of net `P` and carries at most 2 A (the most the pins `j1.bias` carry, by their `.current` or what the model finds through them)",
        "which needs 0.9",
    ] {
        assert!(track.message.contains(part), "{part}: {}", track.message);
    }
    let c = track.carrying.as_ref().unwrap();
    assert_eq!((c.current, c.width), (Some(2.0), Some(0.2)));
    assert!(c.needed_width.unwrap() > 0.9, "{c:?}");
    let via = r.findings.iter().find(|f| f.rule == DrcRule::ViaCurrent).unwrap();
    assert!(
        via.message.contains("carries at most 2 A") && via.message.contains("so 3 side by side are needed"),
        "{}",
        via.message
    );
    let c = via.carrying.as_ref().unwrap();
    assert_eq!((c.vias, c.needed_vias, c.current), (Some(1), Some(3), Some(2.0)));
}

#[test]
fn a_via_finding_names_the_vias_the_current_needs_and_what_one_carries() {
    // Issue 1373: net P needs three vias side by side (1.2 A, at 0.56 A a via of 0.3 mm...).
    let extras = DrcExtras {
        net_currents: vec![DrcNetCurrent {
            net: "P".to_string(),
            current: 1.2,
            source: "rail `a` 1.2 A".to_string(),
            ..Default::default()
        }],
        ..branch_extras(None)
    };
    let mut b = branch_board(0.4, false);
    let r = check(&mut b, &extras);
    let via = r.findings.iter().find(|f| f.rule == DrcRule::ViaCurrent).unwrap();
    for part in [
        "stands with 1 via(s) of its net side by side",
        "its net `P` carries 1.2 A (rail `a` 1.2 A)",
        "a via of the process (a 0.30 mm hole plated 18 \u{3bc}m) carries ",
        "at a 10 K rise (IPC-2221, the inner-layer constant), so 3 side by side are needed",
    ] {
        assert!(via.message.contains(part), "{part}: {}", via.message);
    }
    let c = via.carrying.as_ref().unwrap();
    assert_eq!((c.vias, c.needed_vias, c.current), (Some(1), Some(3), Some(1.2)));
    assert_eq!((c.rise, c.tolerance), (Some(10.0), Some(0.2)));
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
                path: "j1.bias".to_string(),
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

fn pin_current(part: &str, current: f64) -> netsmith_geom::DrcBranch {
    netsmith_geom::DrcBranch {
        net: "P".to_string(),
        path: format!("{}.p1", part.to_lowercase()),
        part: part.to_string(),
        pads: vec!["1".to_string()],
        current,
    }
}

#[test]
fn the_copper_to_a_pin_whose_current_the_model_knows_is_held_to_what_flows_through_it() {
    // Issue #1333: the same 0.2 mm branch with one via, to a pin that states nothing but whose
    // current the model knows (a feedback divider's top takes microamps from a rail of amps).
    let counts = |b: &mut Pcb, extras: &DrcExtras| {
        let r = check(b, extras);
        (r.count(DrcRule::TrackCurrent), r.count(DrcRule::ViaCurrent))
    };
    let with = |pin_currents: Vec<netsmith_geom::DrcBranch>| DrcExtras {
        pin_currents,
        ..branch_extras(None)
    };
    let mut b = branch_board(0.2, false);
    assert_eq!(counts(&mut b, &with(vec![pin_current("J1", 1e-5)])), (0, 0));
    // A pin current that needs more than the branch's copper carries is named as the net's would be.
    assert_eq!(counts(&mut b, &with(vec![pin_current("J1", 2.0)])), (2, 1));
    // A pin current of another net is not this one's.
    let other = netsmith_geom::DrcBranch {
        net: "Q".to_string(),
        ..pin_current("J1", 1e-5)
    };
    assert_eq!(counts(&mut b, &with(vec![other])), (2, 1));
    // A stated current comes before the model's: a pin stated at 2 A is held to 2 A.
    let both = DrcExtras {
        pin_currents: vec![pin_current("J1", 1e-5)],
        ..branch_extras(Some(2.0))
    };
    assert_eq!(counts(&mut b, &both), (2, 1));
    // The trunk is the net's whatever the pins past it carry.
    let mut b = branch_board(0.2, true);
    assert_eq!(counts(&mut b, &with(vec![pin_current("J1", 1e-5)])), (1, 0));
}

#[test]
fn a_branch_carries_the_sum_of_the_currents_of_the_pins_past_it() {
    // Issue #1333: the branch of `branch_board` goes on past J1 to J2. Its first track carries
    // both pins' currents, the last only J2's.
    let mut b = branch_board(0.2, false);
    b.part(
        "J2",
        (20.0, 32.0),
        0.0,
        &[thru("1", 0.0, 0.0, 1.2, 0.7, "P")],
        NONE,
    );
    b.track("P", "B.Cu", (20.0, 22.0), (20.0, 32.0), 0.2);
    let mut tracks = |j1: f64, j2: f64| {
        let extras = DrcExtras {
            pin_currents: vec![pin_current("J1", j1), pin_current("J2", j2)],
            ..branch_extras(None)
        };
        check(&mut b, &extras).count(DrcRule::TrackCurrent)
    };
    // 0.2 mm carries about 0.6 A at JLCPCB's copper: each pin alone fits, both do not.
    assert_eq!(tracks(1e-5, 1e-5), 0);
    assert_eq!(tracks(0.4, 0.4), 2, "the two tracks before J1 carry 0.8 A");
    assert_eq!(tracks(0.0, 2.0), 3, "all three carry J2's 2 A");
}

#[test]
fn a_via_and_a_track_to_an_input_that_takes_no_current_stand_alone() {
    // Issue #1504: the branch of `branch_board` feeds J1's pin, an input the design stage says
    // takes no current (an enable pin tied to a converter's input), on a net that needs 0.4 mm
    // and three vias. Its 0.2 mm tracks and its one via are held to what J1 takes, and the
    // finding of a branch that needs more names the current it assumed.
    let mut b = branch_board(0.2, false);
    let extras = DrcExtras {
        pin_currents: vec![pin_current("J1", 0.0)],
        ..branch_extras(None)
    };
    let r = check(&mut b, &extras);
    assert_eq!(
        (r.count(DrcRule::TrackCurrent), r.count(DrcRule::ViaCurrent)),
        (0, 0)
    );
    let extras = DrcExtras {
        pin_currents: vec![pin_current("J1", 2.0)],
        ..branch_extras(None)
    };
    let r = check(&mut b, &extras);
    let via = r
        .findings
        .iter()
        .find(|v| v.rule == DrcRule::ViaCurrent)
        .unwrap();
    assert!(
        via.message.contains("only feeds branches of net `P` and carries at most 2 A")
            && via.message.contains("`j1.p1`"),
        "{}",
        via.message
    );
}

#[test]
fn vias_whose_copper_is_within_the_group_gap_count_as_one_group() {
    // Issue #1505: two 0.45 mm vias of net P, which needs two side by side, at a 0.6 mm pitch
    // (copper 0.15 mm apart). They are one group when the gap is at least 0.15 mm, two lone
    // vias when it is less, or when the copper has to touch (a gap of 0).
    let mut b = Pcb::new(2);
    b.via("P", (10.0, 10.0), 0.45, 0.2);
    b.via("P", (10.6, 10.0), 0.45, 0.2);
    let count = |b: &mut Pcb, gap: f64| {
        let extras = DrcExtras {
            via_counts: vec![("P".to_string(), 2)],
            via_group_gap: gap,
            ..jlcpcb()
        };
        check(b, &extras).count(DrcRule::ViaCurrent)
    };
    assert_eq!(count(&mut b, 0.0), 2, "touching copper only");
    assert_eq!(count(&mut b, 0.1), 2, "a gap under the copper's space");
    assert_eq!(count(&mut b, 0.15), 0, "a gap of the copper's space");
    // A third via 0.6 mm on joins the group through the second.
    b.via("P", (11.2, 10.0), 0.45, 0.2);
    let extras = DrcExtras {
        via_counts: vec![("P".to_string(), 3)],
        via_group_gap: 0.15,
        ..jlcpcb()
    };
    assert_eq!(check(&mut b, &extras).count(DrcRule::ViaCurrent), 0);
}

#[test]
fn copper_that_feeds_branches_is_never_held_to_more_than_its_nets_whole_current() {
    // Issue #1584: a pin that states 2 A (a rating) on a net known in full to carry 1 A. The
    // copper to it carries no more than the net, so its track and its via are held to the net's
    // one current, as its other copper is: the findings say the net's 1 A, not the pin's 2 A.
    let whole = |lower_bound: bool| DrcExtras {
        net_currents: vec![DrcNetCurrent {
            net: "P".to_string(),
            current: 1.0,
            source: "rail `a` 1 A".to_string(),
            lower_bound,
        }],
        ..branch_extras(Some(2.0))
    };
    let mut b = branch_board(0.2, false);
    let r = check(&mut b, &whole(false));
    let track = r.findings.iter().find(|f| f.rule == DrcRule::TrackCurrent).unwrap();
    assert!(
        track.message.contains("its net `P` carries 1 A (rail `a` 1 A), which needs 0.4000 mm"),
        "{}",
        track.message
    );
    assert_eq!(track.carrying.as_ref().unwrap().current, Some(1.0));
    let via = r.findings.iter().find(|f| f.rule == DrcRule::ViaCurrent).unwrap();
    let c = via.carrying.as_ref().unwrap();
    assert_eq!((c.current, c.needed_vias), (Some(1.0), Some(3)), "{}", via.message);
    // Where the net's current is a lower bound the pin's statement still holds its copper.
    let r = check(&mut b, &whole(true));
    let via = r.findings.iter().find(|f| f.rule == DrcRule::ViaCurrent).unwrap();
    assert_eq!(via.carrying.as_ref().unwrap().current, Some(2.0));
}

/// Net P needs 0.4 mm; a 0.2 mm track of P from S1 to S2 along y = 10, and a 0.2 mm track of
/// net Q beside it, `apart` mm from centre line to centre line.
fn widening_board(apart: f64) -> (Pcb, DrcExtras) {
    let extras = DrcExtras {
        least_widths: vec![("P".to_string(), 0.4)],
        neck_reach: 1.0,
        sizing: Some(sizing()),
        ..jlcpcb()
    };
    let mut b = Pcb::new(2);
    b.stub("P", 10.0, 10.0);
    b.stub("P", 20.0, 10.0);
    b.track("P", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.2);
    b.track("Q", "F.Cu", (13.0, 10.0 + apart), (17.0, 10.0 + apart), 0.2);
    (b, extras)
}

#[test]
fn a_track_held_by_a_track_of_another_net_names_it_its_net_and_the_width_it_allows() {
    // Issue #1691: Q's track runs 0.4 mm from P's centre line to its centre line, so its edge
    // is 0.3 mm from P's centre line; with the 0.127 mm clearance P may be no wider than
    // 2 (0.3 - 0.127) = 0.346 mm there, less than the 0.4 mm its current needs.
    let (mut b, extras) = widening_board(0.4);
    let r = check(&mut b, &extras);
    let f = r.findings.iter().find(|f| f.rule == DrcRule::TrackCurrent).expect("a finding");
    let c = f.carrying.as_ref().unwrap();
    let widest = c.widest.expect("the widest width");
    assert!((widest - 0.346).abs() < 1e-9, "{widest}");
    assert_eq!(c.held_by.len(), 1, "{:?}", c.held_by);
    let held = &c.held_by[0];
    assert_eq!((held.kind, held.rule), ("track", DrcRule::Clearance));
    assert_eq!(held.item.net.as_deref(), Some("Q"));
    assert!(held.item.what.starts_with("Track [Q] on F.Cu"), "{}", held.item.what);
    assert!((held.distance.required - 0.127).abs() < 1e-9, "{:?}", held.distance);
    assert!((held.distance.measured - 0.2).abs() < 1e-9, "{:?}", held.distance);
    assert!((held.at.y - 10.3).abs() < 1e-9, "{:?}", held.at);
    for part in [
        "where it lies it may be no wider than 0.3460 mm: Track [Q] on F.Cu",
        "of net `Q` at (",
        "allows 0.3460 mm (clearance 0.1270 mm, 0.2000 mm now)",
    ] {
        assert!(f.message.contains(part), "{part}: {}", f.message);
    }
}

#[test]
fn a_track_with_room_says_the_width_it_needs_fits() {
    // Issue #1691: 0.6 mm apart, Q's edge is 0.5 mm from P's centre line, which allows
    // 2 (0.5 - 0.127) = 0.746 mm: the 0.4 mm fits, and nothing holds the track.
    let (mut b, extras) = widening_board(0.6);
    let r = check(&mut b, &extras);
    let f = r.findings.iter().find(|f| f.rule == DrcRule::TrackCurrent).expect("a finding");
    let c = f.carrying.as_ref().unwrap();
    assert_eq!(c.widest, Some(0.4));
    assert!(c.held_by.is_empty(), "{:?}", c.held_by);
    assert!(f.message.contains("where it lies the 0.4000 mm fits"), "{}", f.message);
}
