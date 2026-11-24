//! `via_hole_near_pad` and `via_ring_near_pad` (issue #1579): a via whose hole, or whose ring,
//! comes near a pad's copper of any net, on any layer the via is on, is a note, which fails
//! nothing.

use crate::common;

use common::*;
use netsmith_geom::{DrcExtras, DrcReport, DrcRule, DrcSeverity, drc_check};

fn check(b: &mut Pcb, extras: &DrcExtras) -> DrcReport {
    let (pcb, pro) = (b.pcb(), b.pro());
    drc_check(&pcb, Some(&pro), extras).unwrap_or_else(|e| panic!("{e}"))
}

fn reach(strict: bool) -> DrcExtras {
    DrcExtras {
        via_hole_pad_reach: Some(0.1),
        via_ring_pad_reach: Some(0.0),
        strict_guidelines: strict,
        ..DrcExtras::default()
    }
}

/// The notes of `rule`, each as the via's place and the pads it names (`U1.1`).
fn noted(r: &DrcReport, rule: DrcRule) -> Vec<((f64, f64), Vec<String>)> {
    let mut found: Vec<((f64, f64), Vec<String>)> = r
        .findings
        .iter()
        .filter(|f| f.rule == rule)
        .map(|f| {
            let pads = f
                .items
                .iter()
                .filter_map(|i| Some(format!("{}.{}", i.part.as_ref()?, i.pad.as_ref()?)))
                .collect();
            (
                ((f.at.x * 1000.0).round() / 1000.0, f.at.y),
                pads,
            )
        })
        .collect();
    found.sort_by(|a, b| a.0.0.total_cmp(&b.0.0).then(a.0.1.total_cmp(&b.0.1)));
    found
}

/// Pads of 1 mm squares and a round plated pad, and vias of 0.45 mm with 0.2 mm holes (a hole's
/// radius 0.1 mm, a ring's 0.225 mm) at known distances from them.
fn board() -> Pcb {
    let mut b = Pcb::new(2);
    // U1 on the front at (10, 10): pad 1 spans x 9.5 to 10.5; pad 2 at (10, 13) and pad 3 at
    // (11.15, 13), 0.15 mm apart.
    b.part(
        "U1",
        (10.0, 10.0),
        0.0,
        &[
            smd("1", 0.0, 0.0, 1.0, 1.0, "VCC"),
            smd("2", 0.0, 3.0, 1.0, 1.0, "VCC"),
            smd("3", 1.15, 3.0, 1.0, 1.0, "SIG"),
        ],
        Draw::default(),
    );
    // C1 on the back at (20, 10): its pad spans x 19.5 to 20.5 on B.Cu.
    b.part(
        "C1",
        (20.0, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 1.0, 1.0, "SIG")],
        Draw {
            back: true,
            ..Draw::default()
        },
    );
    // J1 at (30, 10): a round plated pad of 1.5 mm, its copper to x 30.75 on every layer.
    b.part(
        "J1",
        (30.0, 10.0),
        0.0,
        &[thru("1", 0.0, 0.0, 1.5, 0.8, "SIG")],
        Draw::default(),
    );
    // R1 at (10, 20): pad 1 spans x 9.5 to 10.5.
    b.part(
        "R1",
        (10.0, 20.0),
        0.0,
        &[smd("1", 0.0, 0.0, 1.0, 1.0, "VCC")],
        Draw::default(),
    );
    // A via of another net whose hole is 0.05 mm from U1's pad 1: noted.
    b.via("GND", (10.65, 10.0), 0.45, 0.2);
    // A via of the pad's net whose hole is 0.3 mm away (its ring 0.175 mm): not noted.
    b.via("VCC", (9.1, 10.0), 0.45, 0.2);
    // A through via 0.05 mm from a pad on the back: noted, on B.Cu.
    b.via("SIG", (20.65, 10.0), 0.45, 0.2);
    // A via 0.05 mm from a plated pad's copper: noted.
    b.via("SIG", (30.9, 10.0), 0.45, 0.2);
    // A via whose hole is 0.11 mm from R1's pad and whose ring reaches 0.015 mm into it: the
    // ring is noted, the hole is not.
    b.via("VCC", (10.71, 20.0), 0.45, 0.2);
    // A via in U1's pad 2 (its centre on the pad's edge), its hole 0.05 mm from pad 3: a via in
    // a pad, not noted again.
    b.via("VCC", (10.5, 13.0), 0.45, 0.2);
    b
}

#[test]
fn a_via_whose_hole_comes_near_a_pad_of_any_net_and_layer_is_a_note() {
    let mut b = board();
    for strict in [false, true] {
        let r = check(&mut b, &reach(strict));
        assert_eq!(
            noted(&r, DrcRule::ViaHoleNearPad),
            [
                ((10.65, 10.0), vec!["U1.1".to_string()]),
                ((20.65, 10.0), vec!["C1.1".to_string()]),
                ((30.9, 10.0), vec!["J1.1".to_string()]),
            ],
            "{r}"
        );
        assert_eq!(
            noted(&r, DrcRule::ViaRingNearPad),
            [((10.71, 20.0), vec!["R1.1".to_string()])],
            "{r}"
        );
        // Notes, with any strictness: none is an error or a warning.
        for f in r.findings.iter().filter(|f| {
            matches!(f.rule, DrcRule::ViaHoleNearPad | DrcRule::ViaRingNearPad)
        }) {
            assert_eq!(f.severity, DrcSeverity::Note, "{f}");
        }
        assert!(
            r.errors()
                .chain(r.warnings())
                .all(|f| !matches!(f.rule, DrcRule::ViaHoleNearPad | DrcRule::ViaRingNearPad))
        );
    }
    let r = check(&mut b, &reach(false));
    let first = r
        .findings
        .iter()
        .find(|f| f.rule == DrcRule::ViaHoleNearPad)
        .unwrap();
    assert!(
        first.to_string().starts_with(
            "note [via_hole_near_pad] the 0.2 mm hole of Via [GND] on F.Cu - B.Cu is 0.05 mm from SMD pad 1 [VCC] of U1; within 0.1 mm of a pad's copper (the process's `.via_hole_pad_reach`, 0.1 mm, a figure of netsmith's, not the board house's)"
        ),
        "{first}"
    );
    // `board drc` says the count and the pads after the findings.
    assert_eq!(
        r.via_near_pad_lines(),
        [
            "3 vias whose hole comes near a pad (via_hole_near_pad): pads U1.1, C1.1, J1.1",
            "1 via whose ring comes near a pad (via_ring_near_pad): pads R1.1",
        ],
    );
    let back = r
        .findings
        .iter()
        .find(|f| f.rule == DrcRule::ViaHoleNearPad && f.at.x > 20.0 && f.at.x < 21.0)
        .unwrap();
    assert_eq!(back.layer.as_deref(), Some("B.Cu"), "{back}");
    let ring = r
        .findings
        .iter()
        .find(|f| f.rule == DrcRule::ViaRingNearPad)
        .unwrap();
    assert!(
        ring.message
            .contains("the 0.45 mm ring of Via [VCC] on F.Cu - B.Cu reaches 0.015 mm into SMD pad 1 [VCC] of R1"),
        "{ring}"
    );
}

#[test]
fn the_reaches_say_how_near_and_none_notes_nothing() {
    let mut b = board();
    // A reach of 0.35 mm takes the via 0.3 mm away too.
    let wide = DrcExtras {
        via_hole_pad_reach: Some(0.35),
        ..reach(false)
    };
    let r = check(&mut b, &wide);
    assert!(
        noted(&r, DrcRule::ViaHoleNearPad)
            .iter()
            .any(|(at, pads)| *at == (9.1, 10.0) && pads == &["U1.1".to_string()]),
        "{r}"
    );
    // 0 takes only a hole that crosses a pad's edge: none here.
    let zero = DrcExtras {
        via_hole_pad_reach: Some(0.0),
        via_ring_pad_reach: None,
        ..DrcExtras::default()
    };
    let r = check(&mut b, &zero);
    assert!(noted(&r, DrcRule::ViaHoleNearPad).is_empty(), "{r}");
    // With no reach (a KiCad board with no `.kicad_dru`), nothing is noted.
    let r = check(&mut b, &DrcExtras::default());
    assert_eq!(r.notes().count(), 0, "{r}");
    // `rule_severities` can turn it off.
    let pro = b.pro().replace(
        "\"rule_severities\": {",
        "\"rule_severities\": {\"via_hole_near_pad\": \"ignore\", ",
    );
    assert!(pro.contains("via_hole_near_pad"), "{pro}");
    let r = drc_check(&b.pcb(), Some(&pro), &reach(false)).unwrap();
    assert!(noted(&r, DrcRule::ViaHoleNearPad).is_empty(), "{r}");
    // The rings of the vias whose hole is no longer noted are, then.
    assert_eq!(noted(&r, DrcRule::ViaRingNearPad).len(), 4, "{r}");
}
