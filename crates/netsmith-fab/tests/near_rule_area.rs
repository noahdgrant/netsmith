//! `near_rule_area` (issue #1591): the parts, tracks, vias and pours near a rule area that
//! forbids copper or parts, a module's antenna keep-out, are notes, which fail nothing.

use crate::common;

use common::*;
use netsmith_geom::{DrcExtras, DrcReport, DrcRule, DrcSeverity, drc_check};

/// A module U1 at (20, 15) whose footprint carries an antenna keep-out on the front, from
/// (15, 5) to (25, 10), that forbids tracks, vias, pours and parts.
fn module(b: &mut Pcb) {
    b.raw("(footprint \"Test:M\" (layer \"F.Cu\") (at 20 15 0) (property \"Reference\" \"U1\" (at 0 0 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr smd) (pad \"1\" smd rect (at -3 0) (size 1 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) (pad \"2\" smd rect (at 3 0) (size 1 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) (fp_rect (start -5 -10) (end 5 2) (stroke (width 0.05) (type solid)) (fill no) (layer \"F.CrtYd\")) (zone (net 0) (net_name \"\") (layer \"F.Cu\") (name \"antenna\") (hatch edge 0.5) (connect_pads (clearance 0)) (min_thickness 0.25) (keepout (tracks not_allowed) (vias not_allowed) (pads allowed) (copperpour not_allowed) (footprints not_allowed)) (fill (thermal_gap 0.5) (thermal_bridge_width 0.5)) (polygon (pts (xy 15 5) (xy 25 5) (xy 25 10) (xy 15 10)))))");
}

fn check(b: &mut Pcb, extras: &DrcExtras) -> DrcReport {
    let (pcb, pro) = (b.pcb(), b.pro());
    drc_check(&pcb, Some(&pro), extras).unwrap_or_else(|e| panic!("{e}"))
}

fn reach(strict: bool) -> DrcExtras {
    DrcExtras {
        rule_area_reach: Some(3.0),
        strict_guidelines: strict,
        ..DrcExtras::default()
    }
}

fn notes(r: &DrcReport) -> Vec<&str> {
    let mut found: Vec<&str> = r
        .findings
        .iter()
        .filter(|f| f.rule == DrcRule::NearRuleArea)
        .map(|f| f.message.as_str())
        .collect();
    found.sort_unstable();
    found
}

#[test]
fn a_part_under_a_modules_keep_out_on_the_back_and_a_via_1_mm_beside_it_are_notes() {
    let mut b = Pcb::new(2);
    module(&mut b);
    // A part on the back under the antenna, and a via whose copper is 1 mm right of the area.
    b.part(
        "C1",
        (20.0, 7.5),
        0.0,
        &[
            smd("1", -0.5, 0.0, 0.5, 0.5, "VCC"),
            smd("2", 0.5, 0.0, 0.5, 0.5, "GND"),
        ],
        Draw {
            courtyard: Some((1.0, 0.5)),
            back: true,
            ..Draw::default()
        },
    );
    b.via("GND", (26.3, 7.5), 0.6, 0.3);
    for strict in [false, true] {
        let r = check(&mut b, &reach(strict));
        assert_eq!(
            notes(&r),
            [
                "Part C1 on B.Cu is under the rule area 'antenna' of U1's footprint (no tracks, vias, copperpour, footprints on F.Cu), on B.Cu, a layer it is not on; within 3 mm of the area copper and parts can detune an antenna: move it or keep it",
                "Via [GND] on F.Cu - B.Cu is 1 mm from the rule area 'antenna' of U1's footprint (no tracks, vias, copperpour, footprints on F.Cu); within 3 mm of the area copper and parts can detune an antenna: move it or keep it",
            ],
            "{r}"
        );
        // Notes, with any strictness: none is an error or a warning, and the module's own pads
        // and footprint are not listed.
        for f in r.findings.iter().filter(|f| f.rule == DrcRule::NearRuleArea) {
            assert_eq!(f.severity, DrcSeverity::Note, "{f}");
            assert!(f.to_string().starts_with("note [near_rule_area] "), "{f}");
            assert!(!f.parts().contains(&"U1".to_string()), "{f}");
        }
        assert_eq!(r.notes().count(), 2, "{r}");
        assert!(r.warnings().all(|w| w.rule != DrcRule::NearRuleArea));
    }
    // With no reach (a KiCad board with no `.kicad_dru`), nothing is listed.
    let r = check(&mut b, &DrcExtras::default());
    assert!(notes(&r).is_empty(), "{r}");
}

#[test]
fn a_board_with_nothing_near_its_keep_out_lists_no_note() {
    let mut b = Pcb::new(2);
    module(&mut b);
    // The via 5 mm right of the area, beyond the reach; a part on the back far away.
    b.via("GND", (30.3, 7.5), 0.6, 0.3);
    b.part(
        "C1",
        (35.0, 25.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.5, 0.5, "VCC")],
        Draw {
            courtyard: Some((1.0, 0.5)),
            back: true,
            ..Draw::default()
        },
    );
    let r = check(&mut b, &reach(false));
    assert!(notes(&r).is_empty(), "{r}");
    assert_eq!(r.notes().count(), 0);
}

#[test]
fn what_items_not_allowed_flags_is_not_listed_again_and_a_pour_under_the_area_is() {
    let mut b = Pcb::new(2);
    module(&mut b);
    // A via inside the area is `items_not_allowed`, not a note; a ground pour on the back over
    // the whole board is under it.
    b.via("GND", (20.0, 7.5), 0.6, 0.3);
    b.zone("GND", "B.Cu", (1.0, 1.0, 39.0, 29.0));
    let r = check(&mut b, &reach(false));
    assert_eq!(r.count(DrcRule::ItemsNotAllowed), 1, "{r}");
    assert_eq!(
        notes(&r),
        ["A pour of GND on B.Cu is under the rule area 'antenna' of U1's footprint (no tracks, vias, copperpour, footprints on F.Cu), on B.Cu, a layer it is not on; within 3 mm of the area copper and parts can detune an antenna: move it or keep it"],
        "{r}"
    );
}

#[test]
fn the_reach_is_read_back_from_the_rules_file() {
    let rules = netsmith_geom::DrcRules::default();
    let text = netsmith_geom::drc_kicad_dru(&rules, &reach(false));
    assert!(text.contains("# netsmith near_rule_area 3mm"), "{text}");
    assert_eq!(
        DrcExtras::default().with_rules_file(&text).rule_area_reach,
        Some(3.0)
    );
}
