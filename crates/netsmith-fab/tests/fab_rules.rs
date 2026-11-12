//! The fabricator's rules a project file cannot say (`drc::fab_rules`, issue #431): each is
//! checked only when the caller passes it, at its limit, and only against what the page says it
//! is about.

use crate::common;

use common::*;
use netsmith_fab::drc::{self, Extras, Report, Rule};

const NONE: Draw = Draw {
    courtyard: None,
    silk: None,
    back: false,
};

/// JLCPCB's two-layer figures, as `fab` passes them.
fn jlcpcb() -> Extras {
    Extras {
        hole_to_track: Some(0.2),
        plated_hole_clearance: Some(0.28),
        pad_to_pad: Some(0.15),
        pad_hole_to_hole: Some(0.45),
        mask_to_track: Some(0.09),
        plated_slot: Some(0.5),
        npth_slot: Some(1.0),
        npth_drill: Some(0.5),
        pad_ring: Some(0.18),
        ..Extras::default()
    }
}

fn check(b: &mut Pcb, extras: &Extras) -> Report {
    let (pcb, pro) = (b.pcb(), b.pro());
    drc::check(&pcb, Some(&pro), extras).unwrap_or_else(|e| panic!("{e}"))
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
            .filter(|f| f.rule == Rule::HoleToTrack)
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
    assert_eq!(r.count(Rule::HoleToTrack), 0, "{r}");
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
        assert_eq!(r.count(Rule::PadToPad), count, "gap {gap}\n{r}");
        let none = check(&mut b, &Extras::default());
        assert_eq!(
            none.count(Rule::PadToPad),
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
        assert_eq!(r.count(Rule::MaskToTrack), count, "gap {gap}\n{r}");
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
    assert_eq!(r.count(Rule::SlotWidth), 1, "{r}");
    assert_eq!(r.count(Rule::NpthDrill), 1, "{r}");
    assert_eq!(r.count(Rule::PlatedRing), 1, "{r}");
    assert_eq!(r.count(Rule::PadHoleToHole), 1, "{r}");
    assert_eq!(r.count(Rule::HoleToHole), 0, "{r}");
    assert!(
        r.findings
            .iter()
            .all(|f| f.rule != Rule::PlatedRing || f.items[0].part.as_deref() == Some("J1"))
    );
    let none = check(&mut b, &Extras::default());
    for rule in [
        Rule::SlotWidth,
        Rule::NpthDrill,
        Rule::PlatedRing,
        Rule::PadHoleToHole,
    ] {
        assert_eq!(none.count(rule), 0, "{rule:?} without the process");
    }
}

#[test]
fn the_fabricators_rules_are_netsmiths_and_not_compared_with_kicad() {
    for rule in [
        Rule::HoleToTrack,
        Rule::PadToPad,
        Rule::PadHoleToHole,
        Rule::MaskToTrack,
        Rule::SlotWidth,
        Rule::NpthDrill,
        Rule::PlatedRing,
    ] {
        assert!(rule.is_netsmiths(), "{rule:?}");
        assert_eq!(Rule::from_id(rule.id()), Some(rule));
    }
}
