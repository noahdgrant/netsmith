//! The process's rules in the `.kicad_dru` beside the board (issue #1185): the text holds the
//! process's space between surface-mount pads and the other rules the project file cannot,
//! netsmith's check of a KiCad board reads them back, and KiCad's check, given the file, reports
//! what netsmith's does. The tests that run KiCad skip themselves where it is not installed.

use crate::common;

use common::*;
use netsmith_fab::{compare_kicad_drc_with_rules_file, in_kicad_dru};
use netsmith_geom::{DrcExtras, DrcReport, DrcRule, DrcRules, drc_check, drc_kicad_dru};

const NONE: Draw = Draw {
    courtyard: None,
    silk: None,
    back: false,
};

/// JLCPCB's numbers that the project file cannot hold (`std/mfg/jlcpcb.nsm`).
fn process() -> DrcExtras {
    DrcExtras {
        pad_to_pad: Some(0.15),
        hole_to_track: Some(0.2),
        plated_hole_clearance: Some(0.28),
        pad_hole_to_hole: Some(0.45),
        npth_drill: Some(0.5),
        npth_slot: Some(1.0),
        plated_slot: Some(0.5),
        pad_ring: Some(0.18),
        ..DrcExtras::default()
    }
}

/// A board with the project file netsmith writes for JLCPCB's two-layer process.
fn jlcpcb() -> Pcb {
    let mut b = Pcb::new(2);
    b.rules = Rules {
        clearance: 0.1,
        track: 0.1,
        via_diameter: 0.45,
        annular: 0.13,
        through_hole: 0.2,
        hole_to_hole: 0.2,
        hole_clearance: 0.1,
        edge: 0.2,
        ..Rules::default()
    };
    b
}

/// Two surface-mount pads 0.3 mm wide of nets A and B, `gap` apart, at (x, y).
fn pads(b: &mut Pcb, refdes: &str, (x, y): (f64, f64), gap: f64) {
    let half = (0.3 + gap) / 2.0;
    b.part(
        refdes,
        (x, y),
        0.0,
        &[
            smd("1", -half, 0.0, 0.3, 0.6, "A"),
            smd("2", half, 0.0, 0.3, 0.6, "B"),
        ],
        NONE,
    );
}

fn rules_file(b: &Pcb) -> String {
    drc_kicad_dru(&DrcRules::from_project(&b.pro()).unwrap(), &process())
}

fn ours(b: &mut Pcb, extras: &DrcExtras) -> DrcReport {
    let (pcb, pro) = (b.pcb(), b.pro());
    drc_check(&pcb, Some(&pro), extras).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn the_rules_file_holds_the_processs_pad_to_pad_and_netsmiths_check_reads_it_back() {
    let mut b = jlcpcb();
    pads(&mut b, "U1", (10.0, 10.0), 0.1448);
    let text = rules_file(&b);
    assert!(
        text.contains("(rule \"netsmith pad_to_pad\"\n\t(constraint clearance (min 0.15mm))"),
        "{text}"
    );
    // The project file's clearance is 0.1 mm: a gap of 0.1448 mm passes it.
    assert_eq!(ours(&mut b, &DrcExtras::default()).findings, vec![]);
    let read = DrcExtras::default().with_rules_file(&text);
    let report = ours(&mut b, &read);
    let found: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.rule == DrcRule::PadToPad)
        .collect();
    assert_eq!(found.len(), 1, "{report}");
    assert!(found[0].message.contains("0.1448 mm"), "{}", found[0]);
    // At 0.15 mm the pads meet the process.
    let mut at = jlcpcb();
    pads(&mut at, "U1", (10.0, 10.0), 0.15);
    assert_eq!(ours(&mut at, &read).count(DrcRule::PadToPad), 0);
}

#[test]
fn kicads_check_with_the_rules_file_reports_a_pad_gap_of_0_1448_mm_as_netsmiths_does() {
    let mut b = jlcpcb();
    pads(&mut b, "U1", (10.0, 10.0), 0.1448);
    let (pcb, pro, text) = (b.pcb(), b.pro(), rules_file(&b));
    let Some(without) = kicad_findings("pads", &pcb, &pro) else {
        eprintln!("skipped: KiCad is not installed");
        return;
    };
    // Without the file KiCad holds the pads to the project's 0.1 mm and is silent.
    assert!(
        without.iter().all(|k| k.base != "clearance"),
        "{without:?}"
    );
    let with = kicad_findings_with("pads", &pcb, &pro, &text).expect("KiCad ran once already");
    let found: Vec<_> = with.iter().filter(|k| k.kind == "pad_to_pad").collect();
    assert_eq!(found.len(), 1, "{with:?}");
    assert_eq!(found[0].base, "clearance");
    assert!(
        found[0].description.contains("actual 0.1448 mm"),
        "{}",
        found[0].description
    );
    let mine = ours(&mut b, &DrcExtras::default().with_rules_file(&text));
    let c = compare_kicad_drc_with_rules_file(&mine, &with, 0.01);
    assert!(c.agrees() && c.agreed == 1, "{c}");
}

#[test]
fn kicads_check_with_the_rules_file_agrees_with_netsmiths_on_every_rule_the_file_holds() {
    let mut b = jlcpcb();
    // Pads 0.08 mm apart break the project's clearance and the process's pad to pad: netsmith
    // names both, KiCad the stricter once.
    pads(&mut b, "U1", (4.0, 4.0), 0.08);
    pads(&mut b, "U2", (8.0, 4.0), 0.12);
    // A track 0.15 mm from a hole with no copper (`hole_to_track`, 0.2 mm), and from a via's.
    b.part("H1", (4.0, 10.0), 0.0, &[hole(0.0, 0.0, 0.6)], NONE);
    b.wire("T", "F.Cu", (2.0, 10.5), (6.0, 10.5), 0.1);
    b.via("V", (12.0, 10.0), 0.45, 0.2);
    b.wire("T2", "F.Cu", (10.0, 10.3), (14.0, 10.3), 0.1);
    // A plated pad with 0.15 mm of copper round its hole (`plated_ring`, 0.18 mm), and a track
    // 0.27 mm from its hole (`hole_to_track` of a plated hole, 0.28 mm).
    b.part("J1", (4.0, 16.0), 0.0, &[thru("1", 0.0, 0.0, 0.8, 0.5, "P")], NONE);
    b.wire("T3", "F.Cu", (2.0, 16.57), (6.0, 16.57), 0.1);
    // Two plated holes of one net 0.4 mm apart (`pad_hole_to_hole`, 0.45 mm).
    b.part("J2", (12.0, 16.0), 0.0, &[thru("1", 0.0, 0.0, 1.0, 0.5, "Q")], NONE);
    b.part("J3", (12.9, 16.0), 0.0, &[thru("1", 0.0, 0.0, 1.0, 0.5, "Q")], NONE);
    // A non-plated hole of 0.4 mm (`npth_drill`, 0.5 mm) and a plated slot 0.4 mm wide
    // (`slot_width`, 0.5 mm).
    b.part("H2", (20.0, 10.0), 0.0, &[hole(0.0, 0.0, 0.4)], NONE);
    b.raw(
        "(footprint \"Test:J4\" (layer \"F.Cu\") (at 20 16 0)\n\t\t(property \"Reference\" \"J4\" (at 0 -3 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15))))\n\t\t(attr through_hole)\n\t\t(pad \"1\" thru_hole oval (at 0 0) (size 0.8 1.4) (drill oval 0.4 1.0) (layers \"*.Cu\" \"*.Mask\"))\n\t)",
    );
    let (pcb, pro, text) = (b.pcb(), b.pro(), rules_file(&b));
    let mine = ours(&mut b, &DrcExtras::default().with_rules_file(&text));
    for rule in [
        DrcRule::PadToPad,
        DrcRule::HoleToTrack,
        DrcRule::PadHoleToHole,
        DrcRule::NpthDrill,
        DrcRule::SlotWidth,
        DrcRule::PlatedRing,
    ] {
        assert!(in_kicad_dru(rule), "{rule:?}");
        assert!(mine.count(rule) > 0, "no {rule:?} in\n{mine}");
    }
    assert_eq!(mine.count(DrcRule::PadToPad), 2, "{mine}");
    assert_eq!(mine.count(DrcRule::HoleToTrack), 3, "{mine}");
    let Some(theirs) = kicad_findings_with("every_rule", &pcb, &pro, &text) else {
        eprintln!("skipped: KiCad is not installed");
        return;
    };
    let c = compare_kicad_drc_with_rules_file(&mine, &theirs, 0.1);
    assert!(c.agrees(), "{c}netsmith's:\n{mine}KiCad's:\n{theirs:#?}");
    for rule in ["pad_to_pad", "hole_to_track", "pad_hole_to_hole", "npth_drill", "slot_width", "plated_ring"] {
        assert!(
            theirs.iter().any(|k| k.kind == rule),
            "KiCad reports no {rule}: {theirs:#?}"
        );
    }
}

#[test]
fn a_plated_pad_inside_a_pad_of_its_number_has_its_ring_measured_with_it_as_kicad_measures_it() {
    // An exposed pad with a via in it: the via's own ring is 0.175 mm, under the process's
    // 0.18 mm for a plated hole, but its hole is in the exposed pad's copper, and KiCad measures
    // the ring in the copper of both. A plated pad alone is held to the 0.18 mm.
    let mut b = jlcpcb();
    b.part(
        "U1",
        (10.0, 10.0),
        0.0,
        &[
            smd("9", 0.0, 0.0, 3.0, 3.0, "G"),
            thru("9", 0.0, 0.0, 0.6, 0.25, "G"),
        ],
        NONE,
    );
    b.part("J1", (20.0, 10.0), 0.0, &[thru("1", 0.0, 0.0, 0.6, 0.25, "H")], NONE);
    let (pcb, pro, text) = (b.pcb(), b.pro(), rules_file(&b));
    let mine = ours(&mut b, &DrcExtras::default().with_rules_file(&text));
    let rings: Vec<_> = mine
        .findings
        .iter()
        .filter(|f| f.rule == DrcRule::PlatedRing)
        .collect();
    assert_eq!(rings.len(), 1, "{mine}");
    assert_eq!(rings[0].parts(), ["J1"]);
    let Some(theirs) = kicad_findings_with("ring", &pcb, &pro, &text) else {
        eprintln!("skipped: KiCad is not installed");
        return;
    };
    let c = compare_kicad_drc_with_rules_file(&mine, &theirs, 0.01);
    assert!(c.agrees() && c.agreed == 1, "{c}KiCad's:\n{theirs:#?}");
}
