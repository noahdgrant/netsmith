//! The design rules check, rule by rule, on small boards from `tests/common`: the numbers in
//! the messages, the limits (a track exactly at the clearance passes), what the project file
//! changes, and what the check leaves alone. None of these need KiCad.

use crate::common;

use common::*;
use netsmith_geom::{DrcFinding, drc_check, DrcExtras, DrcReport, DrcRule, DrcSeverity};

const NONE: Draw = Draw {
    courtyard: None,
    silk: None,
    back: false,
};

fn check(b: &mut Pcb) -> DrcReport {
    let (pcb, pro) = (b.pcb(), b.pro());
    drc_check(&pcb, Some(&pro), &DrcExtras::default()).unwrap_or_else(|e| panic!("{e}"))
}

fn only(report: &DrcReport, rule: DrcRule) -> &DrcFinding {
    let found: Vec<_> = report.findings.iter().filter(|f| f.rule == rule).collect();
    assert_eq!(found.len(), 1, "{rule:?} in\n{report}");
    found[0]
}

#[test]
fn a_track_exactly_at_the_clearance_passes_and_one_a_hair_under_does_not() {
    // Tracks 0.2 wide whose edges are 0.127 apart (the rule), then 0.1255 apart.
    for (pitch, count) in [(0.327, 0), (0.3255, 1), (0.3, 1)] {
        let mut b = Pcb::new(2);
        b.wire("A", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.2);
        b.track("B", "F.Cu", (10.0, 10.0 + pitch), (20.0, 10.0 + pitch), 0.2);
        let r = check(&mut b);
        assert_eq!(r.count(DrcRule::Clearance), count, "pitch {pitch}\n{r}");
    }
}

#[test]
fn a_clearance_finding_has_the_numbers_the_place_the_layer_and_both_items() {
    let mut b = Pcb::new(2);
    b.track("A", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.2);
    b.track("B", "F.Cu", (10.0, 10.3), (20.0, 10.3), 0.2);
    let r = check(&mut b);
    let f = only(&r, DrcRule::Clearance);
    assert_eq!(f.severity, DrcSeverity::Error);
    assert!(
        f.message.contains("0.1000 mm") && f.message.contains("0.1270 mm"),
        "{}",
        f.message
    );
    assert_eq!(f.layer.as_deref(), Some("F.Cu"));
    // Between the two edges: y = 10.15, and x anywhere along the overlap.
    assert!((f.at.y - 10.15).abs() < 1e-6, "{:?}", f.at);
    let nets: Vec<_> = f.items.iter().map(|i| i.net.as_deref().unwrap()).collect();
    assert_eq!(nets, ["A", "B"]);
    assert_eq!(f.items[0].at.x, 10.0, "a track is where it starts");
}

#[test]
fn a_net_class_asks_for_more_clearance_than_the_default() {
    let mut b = Pcb::new(2);
    b.track("PWR", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.2);
    b.track("SIG", "F.Cu", (10.0, 10.6), (20.0, 10.6), 0.2); // gap 0.4
    let pcb = b.pcb();
    let pro = b.pro().replace(
        r#""classes": [ "#,
        r#""netclass_patterns": [{"netclass": "Power", "pattern": "PWR"}], "classes": [ {"name": "Power", "clearance": 0.5, "priority": 0, "track_width": 0.2}, "#,
    );
    let r = drc_check(&pcb, Some(&pro), &DrcExtras::default()).unwrap();
    let f = only(&r, DrcRule::Clearance);
    assert!(
        f.message.contains("0.4000 mm") && f.message.contains("0.5000 mm"),
        "{}",
        f.message
    );
}

#[test]
fn a_rule_the_project_ignores_is_not_reported_and_a_warning_is_not_an_error() {
    let mut b = Pcb::new(2);
    b.track("A", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.05);
    let pcb = b.pcb();
    let pro = b.pro();
    let r = drc_check(&pcb, Some(&pro), &DrcExtras::default()).unwrap();
    assert_eq!(only(&r, DrcRule::TrackWidth).severity, DrcSeverity::Error);
    let ignored = pro.replace(r#""track_width": "error""#, r#""track_width": "ignore""#);
    assert_eq!(
        drc_check(&pcb, Some(&ignored), &DrcExtras::default())
            .unwrap()
            .count(DrcRule::TrackWidth),
        0
    );
    let warned = pro.replace(r#""track_width": "error""#, r#""track_width": "warning""#);
    let r = drc_check(&pcb, Some(&warned), &DrcExtras::default()).unwrap();
    assert_eq!(only(&r, DrcRule::TrackWidth).severity, DrcSeverity::Warning);
    assert!(r.errors().all(|f| f.rule != DrcRule::TrackWidth));
}

#[test]
fn the_counts_name_violations_missing_connections_and_footprint_errors_apart() {
    let mut b = Pcb::new(2);
    b.stub("A", 10.0, 10.0);
    b.stub("A", 20.0, 10.0);
    b.track("B", "F.Cu", (10.0, 15.0), (20.0, 15.0), 0.05);
    let r = check(&mut b);
    let c = r.counts();
    assert_eq!(c.unconnected, 1, "{r}");
    assert!(c.violations >= 1, "{r}");
    assert_eq!(c.footprint_errors, 0);
    assert!(!r.is_clean());
    assert_eq!(c.total(), c.violations + c.unconnected);
}

#[test]
fn a_hole_that_is_a_slot_is_measured_by_its_width_and_its_ends() {
    let mut b = Pcb::new(2);
    // Two slots 0.8 wide, 3 long, side by side 1.1 apart centre to centre: 0.3 between them.
    for (r, x) in [("J1", 10.0), ("J2", 11.1)] {
        b.raw(&format!(
            "(footprint \"Test:{r}\" (layer \"F.Cu\") (at {x} 10 0) (property \"Reference\" \"{r}\" (at 0 0 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr through_hole) (pad \"1\" thru_hole oval (at 0 0) (size 1.6 4) (drill oval 0.8 3) (layers \"*.Cu\" \"*.Mask\") (net {} \"N{r}\")))",
            if r == "J1" { 1 } else { 2 }
        ));
    }
    b.nets = vec!["NJ1", "NJ2"];
    let r = check(&mut b);
    let f = only(&r, DrcRule::HoleToHole);
    assert!(f.message.contains("0.3000 mm"), "{}", f.message);
}

#[test]
fn a_via_in_a_pad_is_a_warning_only_where_the_process_cannot_fill_it() {
    let mut b = Pcb::new(2);
    b.stub("A", 10.0, 10.0);
    b.via("A", (10.0, 10.0), 0.6, 0.3);
    let (pcb, pro) = (b.pcb(), b.pro());
    let none = drc_check(&pcb, Some(&pro), &DrcExtras::default()).unwrap();
    assert_eq!(
        none.count(DrcRule::ViaInPad),
        0,
        "unchecked without the process"
    );
    let fills = DrcExtras {
        via_in_pad: Some(0.65),
        ..DrcExtras::default()
    };
    assert_eq!(
        drc_check(&pcb, Some(&pro), &fills)
            .unwrap()
            .count(DrcRule::ViaInPad),
        0
    );
    let narrow = DrcExtras {
        via_in_pad: Some(0.55),
        ..DrcExtras::default()
    };
    let r = drc_check(&pcb, Some(&pro), &narrow).unwrap();
    let f = only(&r, DrcRule::ViaInPad);
    assert_eq!(f.severity, DrcSeverity::Warning);
    assert!(
        f.message.contains("0.6000 mm") && f.message.contains("0.5500 mm"),
        "{}",
        f.message
    );
    assert_eq!(f.items[1].part.as_deref(), Some("S1"));
}

#[test]
fn the_mask_web_comes_from_the_board_setup_or_else_from_the_caller() {
    let mut b = Pcb::new(2);
    b.part(
        "R1",
        (10.0, 10.0),
        0.0,
        &[
            smd("1", -0.4, 0.0, 0.6, 0.6, "A"),
            smd("2", 0.4, 0.0, 0.6, 0.6, "B"),
        ],
        NONE,
    );
    let (pcb, pro) = (b.pcb(), b.pro());
    // 0.2 mm between the pads: fine without a web, a bridge for a process that needs 0.3 mm.
    assert_eq!(
        drc_check(&pcb, Some(&pro), &DrcExtras::default())
            .unwrap()
            .count(DrcRule::SolderMaskBridge),
        0
    );
    let web = DrcExtras {
        mask_web: Some(0.3),
        ..DrcExtras::default()
    };
    let r = drc_check(&pcb, Some(&pro), &web).unwrap();
    let f = only(&r, DrcRule::SolderMaskBridge);
    assert!(
        f.message.contains("0.2000 mm") && f.message.contains("0.3000 mm"),
        "{}",
        f.message
    );
    assert_eq!(f.layer.as_deref(), Some("F.Mask"));
    // The board's own setup wins over the caller.
    b.setup("(solder_mask_min_width 0.1)");
    let (pcb, pro) = (b.pcb(), b.pro());
    assert_eq!(
        drc_check(&pcb, Some(&pro), &web)
            .unwrap()
            .count(DrcRule::SolderMaskBridge),
        0
    );
}

#[test]
fn a_pour_joins_the_pads_and_vias_of_its_net_under_it_and_a_pad_set_to_no_connection_is_left_out() {
    let mut b = Pcb::new(4);
    b.zone("GND", "In1.Cu", (2.0, 2.0, 38.0, 28.0));
    b.stub("GND", 10.0, 10.0);
    b.via("GND", (10.0, 11.0), 0.6, 0.3);
    b.track("GND", "F.Cu", (10.0, 10.0), (10.0, 11.0), 0.2);
    b.stub("GND", 20.0, 10.0);
    b.via("GND", (20.0, 11.0), 0.6, 0.3);
    b.track("GND", "F.Cu", (20.0, 10.0), (20.0, 11.0), 0.2);
    let r = check(&mut b);
    assert_eq!(r.count(DrcRule::UnconnectedItems), 0, "{r}");
    assert!(r.is_clean(), "{r}");

    // The same pads with no via: nothing joins them to each other or to the plane.
    let mut b = Pcb::new(4);
    b.zone("GND", "In1.Cu", (2.0, 2.0, 38.0, 28.0));
    b.stub("GND", 10.0, 10.0);
    b.stub("GND", 20.0, 10.0);
    let r = check(&mut b);
    assert_eq!(r.count(DrcRule::UnconnectedItems), 1, "{r}");

    // A zone that connects no pads does not join a through-hole pad of its net either.
    let mut b = Pcb::new(4);
    b.raw("(zone (net 1) (net_name \"GND\") (layer \"In1.Cu\") (hatch edge 0.5) (connect_pads no (clearance 0.2)) (min_thickness 0.2) (fill yes (thermal_gap 0.3) (thermal_bridge_width 0.3)) (polygon (pts (xy 2 2) (xy 38 2) (xy 38 28) (xy 2 28))))");
    b.part(
        "J1",
        (10.0, 10.0),
        0.0,
        &[
            thru("1", 0.0, 0.0, 1.6, 0.9, "GND"),
            thru("2", 3.0, 0.0, 1.6, 0.9, "GND"),
        ],
        NONE,
    );
    let r = check(&mut b);
    assert_eq!(r.count(DrcRule::UnconnectedItems), 1, "{r}");
}

#[test]
fn a_rule_area_the_board_draws_forbids_tracks_vias_pads_and_parts_but_one_a_footprint_carries_is_not_enforced()
 {
    // The same antenna keepout, drawn on the board and carried by a footprint (KiCad does not
    // enforce the second).
    let rule_area = |inside: &str| {
        format!(
            "(zone (net 0) (net_name \"\") (layer \"F.Cu\") (name \"antenna\") (hatch edge 0.5) (connect_pads (clearance 0)) (min_thickness 0.25) (keepout (tracks not_allowed) (vias not_allowed) (pads not_allowed) (copperpour not_allowed) (footprints not_allowed)) (fill (thermal_gap 0.5) (thermal_bridge_width 0.5)) (polygon (pts {inside})))"
        )
    };
    let mut b = Pcb::new(2);
    b.raw(&rule_area("(xy 16 7) (xy 24 7) (xy 24 13) (xy 16 13)"));
    b.part(
        "R1",
        (21.0, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "A")],
        Draw {
            courtyard: Some((0.8, 0.8)),
            ..NONE
        },
    );
    let r = check(&mut b);
    let found: Vec<_> = r
        .findings
        .iter()
        .filter(|f| f.rule == DrcRule::ItemsNotAllowed)
        .collect();
    // R1's pad and R1 itself.
    assert_eq!(found.len(), 2, "{r}");
    assert!(
        found
            .iter()
            .all(|f| f.items[0].part.as_deref() == Some("R1")),
        "{r}"
    );
    assert!(
        found[0].message.contains("'antenna'"),
        "{}",
        found[0].message
    );

    let mut b = Pcb::new(2);
    b.raw(&format!("(footprint \"Test:ANT\" (layer \"F.Cu\") (at 20 10 0) (property \"Reference\" \"U1\" (at 0 0 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr smd) (pad \"1\" smd rect (at 0 0) (size 1 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) {})", rule_area("(xy -4 -3) (xy 4 -3) (xy 4 3) (xy -4 3)")));
    b.part(
        "R1",
        (21.0, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "A")],
        Draw {
            courtyard: Some((0.8, 0.8)),
            ..NONE
        },
    );
    let r = check(&mut b);
    assert_eq!(r.count(DrcRule::ItemsNotAllowed), 0, "{r}");
}

#[test]
fn the_outline_must_be_one_closed_path_and_a_cutout_must_lie_inside_it() {
    let mut b = Pcb::new(2);
    b.raw("(gr_rect (start 30 10) (end 45 20) (stroke (width 0.05) (type default)) (fill no) (layer \"Edge.Cuts\"))");
    let r = check(&mut b);
    let f = only(&r, DrcRule::InvalidOutline);
    assert!(
        f.message.contains("cutout") && f.message.contains("outside the board"),
        "{}",
        f.message
    );
    assert!(f.at.x >= 40.0, "at the corner that is outside: {:?}", f.at);

    // No outline at all.
    let pcb = "(kicad_pcb (layers (0 \"F.Cu\" signal) (2 \"B.Cu\" signal)))";
    let r = drc_check(pcb, None, &DrcExtras::default()).unwrap();
    assert!(
        only(&r, DrcRule::InvalidOutline)
            .message
            .contains("no outline")
    );
}

#[test]
fn a_part_on_the_back_has_its_courtyard_on_the_back() {
    let mut b = Pcb::new(2);
    let c = |back| Draw {
        courtyard: Some((1.0, 1.0)),
        silk: None,
        back,
    };
    b.part(
        "A1",
        (10.0, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "A")],
        c(false),
    );
    b.part(
        "A2",
        (10.0, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "B")],
        c(true),
    );
    b.part(
        "A3",
        (10.5, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "C")],
        c(true),
    );
    let r = check(&mut b);
    let f = only(&r, DrcRule::CourtyardsOverlap);
    assert_eq!(f.layer.as_deref(), Some("B.CrtYd"));
    assert_eq!(f.parts(), ["A2", "A3"]);
}

#[test]
fn a_board_with_no_project_file_is_checked_to_kicads_defaults() {
    let mut b = Pcb::new(2);
    b.track("A", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.1);
    b.track("B", "F.Cu", (10.0, 10.25), (20.0, 10.25), 0.1);
    let r = drc_check(&b.pcb(), None, &DrcExtras::default()).unwrap();
    // KiCad's default clearance is 0.2 mm; these tracks are 0.15 mm apart.
    assert!(only(&r, DrcRule::Clearance).message.contains("0.2000 mm"));
}

#[test]
fn a_project_file_that_is_not_json_and_a_file_that_is_not_a_board_are_refused() {
    let mut b = Pcb::new(2);
    let pcb = b.pcb();
    let e = drc_check(&pcb, Some("not json"), &DrcExtras::default()).unwrap_err();
    assert!(e.to_string().contains("not JSON"), "{e}");
    let e = drc_check("(not_a_board)", None, &DrcExtras::default()).unwrap_err();
    assert!(e.to_string().contains("not a KiCad board"), "{e}");
}
