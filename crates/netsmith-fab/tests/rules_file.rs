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
        text_to_mask: Some(0.15),
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
        text.contains(
            "(rule \"netsmith pad_to_pad\"\n\t(constraint physical_clearance (min 0.15mm))"
        ),
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
fn kicads_check_with_the_rules_file_reports_a_gap_of_0_1448_mm_between_pads_of_one_footprint_as_netsmiths_does() {
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

/// Two surface-mount pads 0.3 mm wide of nets A and B, `gap` apart, in one footprint `U1` or in
/// two parts, `U1` and `U2`. `own_clearance` is written on pad 1 and `fields` among the
/// footprint's own (issue #1229).
fn pair(gap: f64, one_footprint: bool, own_clearance: Option<f64>, fields: &str) -> Pcb {
    let mut b = jlcpcb();
    let half = (0.3 + gap) / 2.0;
    let first = P {
        clearance: own_clearance,
        ..smd("1", if one_footprint { -half } else { 0.0 }, 0.0, 0.3, 0.6, "A")
    };
    if one_footprint {
        let second = smd("2", half, 0.0, 0.3, 0.6, "B");
        b.part_in("U1", (10.0, 10.0), 0.0, &[first, second], NONE, fields);
    } else {
        b.part_in("U1", (10.0 - half, 10.0), 0.0, &[first], NONE, fields);
        b.part("U2", (10.0 + half, 10.0), 0.0, &[smd("1", 0.0, 0.0, 0.3, 0.6, "B")], NONE);
    }
    b
}

/// KiCad's findings of the rule of the pads' space on the board.
fn kicad_pad_to_pad(name: &str, b: &mut Pcb, rules: &str) -> Option<Vec<netsmith_fab::KicadDrcReported>> {
    let (pcb, pro) = (b.pcb(), b.pro());
    kicad_findings_with(name, &pcb, &pro, rules)
        .map(|k| k.into_iter().filter(|k| k.kind == "pad_to_pad").collect())
}

#[test]
fn kicads_check_with_the_rules_file_reports_pads_0_14_mm_apart_of_one_footprint_and_of_two_parts_alike()
{
    // Measured with KiCad 9.0.0 (issue #1229): its check does not skip the pads of one footprint.
    for one_footprint in [true, false] {
        let mut b = pair(0.14, one_footprint, None, "");
        let text = rules_file(&b);
        let mine = ours(&mut b, &DrcExtras::default().with_rules_file(&text));
        assert_eq!(mine.count(DrcRule::PadToPad), 1, "{mine}");
        // The finding says where it is: within a footprint or between two parts.
        assert_eq!(
            mine.to_string().contains("within U1's own footprint"),
            one_footprint,
            "{mine}"
        );
        let Some(without) = kicad_pad_to_pad("gap14", &mut b, "") else {
            eprintln!("skipped: KiCad is not installed");
            return;
        };
        assert_eq!(without, vec![], "the project file's 0.1 mm passes the pads");
        let with = kicad_pad_to_pad("gap14", &mut b, &text).unwrap();
        assert_eq!(with.len(), 1, "one footprint: {one_footprint}: {with:?}");
        assert!(
            with[0].description.contains("actual 0.1400 mm"),
            "{}",
            with[0].description
        );
        let all = kicad_findings_with("gap14", &b.pcb(), &b.pro(), &text).unwrap();
        let c = compare_kicad_drc_with_rules_file(&mine, &all, 0.01);
        assert!(c.agrees() && c.agreed == 1, "{c}{all:?}");
    }
}

#[test]
fn a_clearance_written_on_a_pad_or_its_footprint_or_a_net_tie_group_does_not_silence_the_rule() {
    // KiCad's `clearance` constraint yields to a clearance written on a pad or on its footprint,
    // as it is in KiCad's own footprint libraries, and to a footprint's net tie groups: a rule of
    // that constraint is silent for those pads. The rule of the file is a `physical_clearance`,
    // which they do not override (measured with KiCad 9.0.0, issue #1229).
    let cases = [
        ("a pad's own clearance, one footprint", true, Some(0.05), ""),
        ("a pad's own clearance, two parts", false, Some(0.05), ""),
        ("a footprint's clearance", true, None, "(clearance 0.05)"),
        ("a net tie group", true, None, "(net_tie_pad_groups \"1, 2\")"),
    ];
    for (what, one_footprint, own, fields) in cases {
        let mut b = pair(0.14, one_footprint, own, fields);
        let text = rules_file(&b);
        let mine = ours(&mut b, &DrcExtras::default().with_rules_file(&text));
        assert_eq!(mine.count(DrcRule::PadToPad), 1, "{what}: {mine}");
        let Some(with) = kicad_pad_to_pad("own", &mut b, &text) else {
            eprintln!("skipped: KiCad is not installed");
            return;
        };
        assert_eq!(with.len(), 1, "{what}: {with:?}");
        // The same rule as a `clearance` is silent.
        let as_clearance = text.replace("physical_clearance", "clearance");
        let silent = kicad_pad_to_pad("own", &mut b, &as_clearance).unwrap();
        assert_eq!(silent, vec![], "{what}: a clearance constraint reports");
    }
}

#[test]
fn pads_that_overlap_are_the_short_and_pads_of_one_net_or_no_net_are_not_the_rules() {
    // Overlapping pads of two nets: netsmith reports the short, and KiCad too, with its rule
    // of the pads' space as a second finding of the pair, which the comparison counts as the
    // short's.
    let mut b = pair(-0.1, true, None, "");
    let text = rules_file(&b);
    let mine = ours(&mut b, &DrcExtras::default().with_rules_file(&text));
    assert_eq!(mine.count(DrcRule::PadToPad), 0, "{mine}");
    assert_eq!(mine.count(DrcRule::ShortingItems), 1, "{mine}");
    // Pads 0.14 mm apart of one net, or with no net: netsmith's rule is for pads of two nets.
    let net_of = |a, b2| {
        let mut b = jlcpcb();
        let pad = |n, x, net| P {
            net,
            ..smd(n, x, 0.0, 0.3, 0.6, "")
        };
        b.part("U1", (10.0, 10.0), 0.0, &[pad("1", -0.22, a), pad("2", 0.22, b2)], NONE);
        b
    };
    for (what, mut same) in [
        ("one net", net_of(Some("A"), Some("A"))),
        ("no net", net_of(None, None)),
    ] {
        let mine = ours(&mut same, &DrcExtras::default().with_rules_file(&text));
        assert_eq!(mine.count(DrcRule::PadToPad), 0, "{what}: {mine}");
        let Some(theirs) = kicad_pad_to_pad("same", &mut same, &text) else {
            eprintln!("skipped: KiCad is not installed");
            return;
        };
        assert_eq!(theirs, vec![], "{what}");
    }
    let Some(theirs) = kicad_findings_with("overlap", &b.pcb(), &b.pro(), &text) else {
        eprintln!("skipped: KiCad is not installed");
        return;
    };
    assert_eq!(
        theirs.iter().filter(|k| k.kind == "pad_to_pad").count(),
        1,
        "{theirs:?}"
    );
    let c = compare_kicad_drc_with_rules_file(&mine, &theirs, 0.01);
    assert!(c.agrees() && c.measured_otherwise == 1, "{c}{theirs:?}");
}

#[test]
fn kicads_check_judges_the_items_of_one_footprint_to_the_same_rules_as_netsmiths() {
    // The guide's table (chapter _Commands_, _The design rules check against KiCad's_) says,
    // for each class of finding, whether KiCad reports it where both items are of one
    // footprint (issue #1229). One footprint `U1` with pads and a hole that break the rules a
    // pair of its own items can break; the two checks name the same findings.
    let mut b = jlcpcb();
    let mut pads = vec![
        // Two nets 0.08 mm apart: `clearance` and `pad_to_pad`.
        smd("1", -0.19, 0.0, 0.3, 0.6, "A"),
        smd("2", 0.19, 0.0, 0.3, 0.6, "B"),
        // Two nets that overlap: a short.
        smd("3", -0.1, 3.0, 0.3, 0.6, "C"),
        smd("4", 0.1, 3.0, 0.3, 0.6, "D"),
        // (A hole with no copper 0.05 mm from a pad of its own footprint is left out: KiCad 9.0.0
        // reports its `hole_clearance`, 9.0.8 does not, and a check of the two must not depend on
        // which KiCad is installed.)
        // Two plated holes 0.1 mm apart (`hole_to_hole`, `pad_hole_to_hole`, thin rings).
        thru("6", -0.45, 9.0, 1.0, 0.8, "F"),
        thru("7", 0.45, 9.0, 1.0, 0.8, "F"),
        // A pad 0.1 mm from a line of its own footprint on `Edge.Cuts`, and silkscreen 0.1 mm
        // from it.
        smd("8", 8.25, 0.0, 0.3, 0.6, "G"),
    ];
    // Pad 9 is of pad 1's net and joined to it by nothing: `unconnected_items`.
    pads.push(smd("9", 12.0, 0.0, 0.3, 0.6, "A"));
    let own_edge = "(fp_rect (start 6 -2) (end 8 2) (stroke (width 0.05) (type solid)) (fill no) (layer \"Edge.Cuts\"))\n\t\t(fp_line (start 6 -2.1) (end 8 -2.1) (stroke (width 0.12) (type solid)) (layer \"F.SilkS\"))";
    b.part_in("U1", (10.0, 10.0), 0.0, &pads, NONE, own_edge);
    // A mask web of 0.04 mm: the short's pads break it.
    b.setup("(solder_mask_min_width 0.04)\n");
    let text = rules_file(&b);
    let mine = ours(&mut b, &DrcExtras::default().with_rules_file(&text));
    let Some(theirs) = kicad_findings_with("one_footprint", &b.pcb(), &b.pro(), &text) else {
        eprintln!("skipped: KiCad is not installed");
        return;
    };
    let c = compare_kicad_drc_with_rules_file(&mine, &theirs, 0.1);
    assert!(c.agrees(), "{c}netsmith's:\n{mine}KiCad's:\n{theirs:#?}");
    for rule in [
        "clearance",
        "pad_to_pad",
        "solder_mask_bridge",
        "shorting_items",
        "hole_to_hole",
        "pad_hole_to_hole",
        "copper_edge_clearance",
        "silk_edge_clearance",
        "unconnected_items",
    ] {
        assert!(
            // The process's `pad_hole_to_hole` replaces `hole_to_hole` for the pair in KiCad.
            theirs
                .iter()
                .any(|k| k.kind == rule || (rule == "hole_to_hole" && k.base == rule)),
            "KiCad reports no {rule}: {theirs:#?}"
        );
        assert!(
            DrcRule::from_id(rule).is_some_and(|r| mine.count(r) > 0),
            "netsmith reports no {rule}:\n{mine}"
        );
    }
}

#[test]
fn the_mask_web_between_pads_of_one_footprint_is_judged_by_both_checks() {
    // Pads of two nets 0.08 mm apart in one footprint, and in two parts, to a web of 0.13 mm.
    for one_footprint in [true, false] {
        let mut b = pair(0.08, one_footprint, None, "");
        b.setup("(solder_mask_min_width 0.13)\n");
        let mine = ours(&mut b, &DrcExtras::default());
        assert_eq!(mine.count(DrcRule::SolderMaskBridge), 1, "{mine}");
        let Some(theirs) = kicad_findings("web", &b.pcb(), &b.pro()) else {
            eprintln!("skipped: KiCad is not installed");
            return;
        };
        let c = compare_kicad_drc_with_rules_file(&mine, &theirs, 0.01);
        assert!(c.agrees(), "{c}{theirs:#?}");
        assert_eq!(
            theirs.iter().filter(|k| k.kind == "solder_mask_bridge").count(),
            1,
            "one footprint: {one_footprint}: {theirs:#?}"
        );
    }
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
            // The process's `pad_hole_to_hole` replaces `hole_to_hole` for the pair in KiCad.
            theirs
                .iter()
                .any(|k| k.kind == rule || (rule == "hole_to_hole" && k.base == rule)),
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

/// KiCad's findings of the rule of a hole's space to a track on the board.
fn kicad_hole_to_track(
    name: &str,
    b: &mut Pcb,
    rules: &str,
) -> Option<Vec<netsmith_fab::KicadDrcReported>> {
    let (pcb, pro) = (b.pcb(), b.pro());
    kicad_findings_with(name, &pcb, &pro, rules)
        .map(|k| k.into_iter().filter(|k| k.kind == "hole_to_track").collect())
}

/// A plated pad `J1` of net P with a hole of 0.5 mm, and a track of net `net` 0.27 mm from the
/// hole's edge (the process asks 0.28 mm), the pad stating `own_clearance` and the footprint
/// `fields` (issue #1231).
fn plated_hole_and_track(own_clearance: Option<f64>, fields: &str, net: &'static str) -> Pcb {
    let mut b = jlcpcb();
    let pad = P {
        clearance: own_clearance,
        ..thru("1", 0.0, 0.0, 0.8, 0.5, "P")
    };
    b.part_in("J1", (10.0, 10.0), 0.0, &[pad], NONE, fields);
    b.wire(net, "F.Cu", (8.0, 10.57), (12.0, 10.57), 0.1);
    b
}

#[test]
fn a_clearance_written_on_a_pad_does_not_silence_the_rule_of_a_holes_space_to_a_track() {
    // KiCad's `hole_clearance` constraint yields to a clearance written on a pad or its
    // footprint; the rule of the file is a `physical_hole_clearance`, which does not (measured
    // with KiCad 9.0.0, issue #1231).
    let cases = [
        ("no clearance of its own", None, ""),
        ("a pad's own clearance", Some(0.05), ""),
        ("a footprint's clearance", None, "(clearance 0.05)"),
    ];
    for (what, own, fields) in cases {
        let mut b = plated_hole_and_track(own, fields, "T");
        let text = rules_file(&b);
        assert!(
            text.contains("(rule \"netsmith hole_to_track plated\"\n\t(constraint physical_hole_clearance (min 0.28mm))"),
            "{text}"
        );
        let mine = ours(&mut b, &DrcExtras::default().with_rules_file(&text));
        assert_eq!(mine.count(DrcRule::HoleToTrack), 1, "{what}: {mine}");
        let Some(with) = kicad_hole_to_track("plated", &mut b, &text) else {
            eprintln!("skipped: KiCad is not installed");
            return;
        };
        assert_eq!(with.len(), 1, "{what}: {with:?}");
        let all = kicad_findings_with("plated", &b.pcb(), &b.pro(), &text).unwrap();
        let c = compare_kicad_drc_with_rules_file(&mine, &all, 0.01);
        assert!(c.agrees() && c.agreed >= 1, "{what}: {c}{all:?}");
        // The same rule as a `hole_clearance` is silent where the pad or its footprint states a
        // clearance of its own, and reports where none does.
        let as_hole_clearance = text.replace("physical_hole_clearance", "hole_clearance");
        let old = kicad_hole_to_track("plated", &mut b, &as_hole_clearance).unwrap();
        let expected = usize::from(own.is_none() && fields.is_empty());
        assert_eq!(old.len(), expected, "{what}: a hole_clearance constraint: {old:?}");
    }
}

#[test]
fn a_hole_and_a_track_of_one_net_are_not_the_rules_of_a_holes_space_to_a_track() {
    // `physical_hole_clearance` reports copper of the hole's own net, which `hole_clearance`
    // and netsmith's check do not: the condition of the rule leaves it out (issue #1231). A
    // plated pad's own track, a via's own track, a via of another net, and a track with no net
    // beside a hole with none.
    let mut plated = plated_hole_and_track(Some(0.05), "", "P");
    let mut via_own = jlcpcb();
    via_own.via("V", (12.0, 10.0), 0.45, 0.2);
    via_own.wire("V", "F.Cu", (10.0, 10.0), (12.0, 10.0), 0.1);
    via_own.wire("V", "F.Cu", (12.0, 10.0), (14.0, 10.3), 0.1);
    let mut via_other = jlcpcb();
    via_other.via("V", (12.0, 10.0), 0.45, 0.2);
    via_other.wire("T", "F.Cu", (10.0, 10.3), (14.0, 10.3), 0.1);
    let mut netless = jlcpcb();
    netless.part("H1", (4.0, 10.0), 0.0, &[hole(0.0, 0.0, 0.6)], NONE);
    netless.raw("(segment (start 2 10.45) (end 6 10.45) (width 0.1) (layer \"F.Cu\"))");
    let cases = [
        ("a plated pad's own track", &mut plated, 0),
        ("a via's own tracks", &mut via_own, 0),
        ("a track of another net beside a via", &mut via_other, 1),
        ("a track with no net beside a hole with none", &mut netless, 1),
    ];
    for (what, b, count) in cases {
        let text = rules_file(b);
        let mine = ours(b, &DrcExtras::default().with_rules_file(&text));
        assert_eq!(mine.count(DrcRule::HoleToTrack), count, "{what}: {mine}");
        let Some(with) = kicad_hole_to_track("own_net", b, &text) else {
            eprintln!("skipped: KiCad is not installed");
            return;
        };
        assert_eq!(with.len(), count, "{what}: {with:?}");
    }
}

#[test]
fn the_rule_of_two_holes_space_is_not_silenced_by_a_pads_own_clearance() {
    // `hole_to_hole` is not overridden by a clearance written on a pad (measured, issue #1231),
    // so `pad_hole_to_hole` stays a `hole_to_hole` rule.
    let mut b = jlcpcb();
    let pad = |n| P {
        clearance: Some(0.05),
        ..thru(n, 0.0, 0.0, 1.0, 0.5, "Q")
    };
    b.part("J2", (12.0, 16.0), 0.0, &[pad("1")], NONE);
    b.part("J3", (12.9, 16.0), 0.0, &[pad("1")], NONE);
    let text = rules_file(&b);
    assert!(
        text.contains("(rule \"netsmith pad_hole_to_hole\"\n\t(constraint hole_to_hole"),
        "{text}"
    );
    let mine = ours(&mut b, &DrcExtras::default().with_rules_file(&text));
    assert_eq!(mine.count(DrcRule::PadHoleToHole), 1, "{mine}");
    let Some(theirs) = kicad_findings_with("holes", &b.pcb(), &b.pro(), &text) else {
        eprintln!("skipped: KiCad is not installed");
        return;
    };
    assert_eq!(
        theirs.iter().filter(|k| k.kind == "pad_hole_to_hole").count(),
        1,
        "{theirs:?}"
    );
}
