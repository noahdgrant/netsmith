//! A corpus of boards with a violation put in on purpose, written by `tests/common`: what
//! netsmith's design rules check finds in each, and that KiCad's check (when installed)
//! reports the same findings at the same places (ADR 0037).

mod common;

use common::*;
use netsmith_fab::drc::{self, Extras, Rule, kicad};

struct Case {
    name: &'static str,
    board: Pcb,
    /// How many findings of each rule netsmith must report; other rules are free.
    expect: Vec<(Rule, usize)>,
}

fn case(name: &'static str, board: Pcb, expect: &[(Rule, usize)]) -> Case {
    Case {
        name,
        board,
        expect: expect.to_vec(),
    }
}

const NONE: Draw = Draw {
    courtyard: None,
    silk: None,
    back: false,
};

fn two_layer() -> Pcb {
    Pcb::new(2)
}

fn corpus() -> Vec<Case> {
    let mut cases = Vec::new();

    let mut b = two_layer();
    b.wire("A", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.2);
    b.wire("B", "F.Cu", (10.0, 14.0), (20.0, 14.0), 0.2);
    cases.push(case("clean", b, &[]));

    // Clearance, one pair at a time.
    let mut b = two_layer();
    b.track("A", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.2);
    b.track("B", "F.Cu", (10.0, 10.3), (20.0, 10.3), 0.2);
    cases.push(case("track_track", b, &[(Rule::Clearance, 1)]));

    let mut b = two_layer();
    b.stub("A", 15.0, 10.0);
    b.track("B", "F.Cu", (10.0, 10.5), (20.0, 10.5), 0.2);
    cases.push(case("track_pad", b, &[(Rule::Clearance, 1)]));

    let mut b = two_layer();
    b.part(
        "R1",
        (10.0, 10.0),
        0.0,
        &[
            smd("1", -0.35, 0.0, 0.6, 0.6, "A"),
            smd("2", 0.35, 0.0, 0.6, 0.6, "B"),
        ],
        NONE,
    );
    b.part(
        "R2",
        (20.0, 10.0),
        90.0,
        &[
            smd("1", -0.35, 0.0, 0.6, 0.9, "A"),
            smd("2", 0.35, 0.0, 0.6, 0.9, "B"),
        ],
        NONE,
    );
    cases.push(case(
        "pad_pad",
        b,
        &[(Rule::Clearance, 1), (Rule::ShortingItems, 1)],
    ));

    let mut b = two_layer();
    b.via("A", (10.0, 10.0), 0.6, 0.3);
    b.track("B", "B.Cu", (5.0, 10.45), (15.0, 10.45), 0.2);
    b.via("B", (20.0, 10.0), 0.6, 0.3);
    b.via("C", (20.0, 10.65), 0.6, 0.3);
    b.stub("D", 30.0, 10.0);
    b.via("E", (30.0, 10.7), 0.6, 0.3);
    cases.push(case("via_to_others", b, &[(Rule::Clearance, 4)]));

    let mut b = two_layer();
    b.part(
        "J1",
        (10.0, 10.0),
        0.0,
        &[
            thru("1", 0.0, 0.0, 1.4, 0.8, "A"),
            thru("2", 1.5, 0.0, 1.4, 0.8, "B"),
        ],
        NONE,
    );
    cases.push(case("thru_pads_close", b, &[(Rule::Clearance, 1)]));

    // Shorts.
    let mut b = two_layer();
    b.stub("C", 30.0, 10.0);
    b.stub("D", 30.2, 10.0);
    cases.push(case("shorts", b, &[(Rule::ShortingItems, 1)]));

    let mut b = two_layer();
    b.wire("A", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.2);
    b.wire("B", "F.Cu", (15.0, 5.0), (15.0, 15.0), 0.2);
    cases.push(case("crossing", b, &[(Rule::TracksCrossing, 1)]));

    // Sizes.
    let mut b = two_layer();
    b.wire("A", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.1);
    b.wire("B", "F.Cu", (10.0, 14.0), (20.0, 14.0), 0.127);
    b.via("C", (30.0, 10.0), 0.4, 0.2);
    b.via("C", (30.0, 12.0), 0.5, 0.35);
    b.via("C", (30.0, 14.0), 0.7, 0.2);
    cases.push(case(
        "sizes",
        b,
        &[
            (Rule::TrackWidth, 1),
            (Rule::ViaDiameter, 1),
            (Rule::AnnularWidth, 2),
            (Rule::DrillOutOfRange, 2),
        ],
    ));

    let mut b = two_layer();
    b.part(
        "J1",
        (10.0, 10.0),
        0.0,
        &[thru("1", 0.0, 0.0, 1.0, 0.8, "A"), hole(5.0, 0.0, 0.2)],
        NONE,
    );
    b.part(
        "J2",
        (20.0, 10.0),
        0.0,
        &[thru("1", 0.0, 0.0, 1.6, 0.9, "A")],
        NONE,
    );
    cases.push(case(
        "pth_ring_and_small_holes",
        b,
        &[(Rule::AnnularWidth, 1), (Rule::DrillOutOfRange, 1)],
    ));

    // Holes.
    let mut b = two_layer();
    b.part("H1", (10.0, 10.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    b.part("H2", (10.7, 10.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    b.part("H3", (20.0, 10.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    b.part("H4", (21.3, 10.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    cases.push(case("hole_to_hole", b, &[(Rule::HoleToHole, 2)]));

    let mut b = two_layer();
    b.part("H1", (10.0, 10.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    b.track("A", "F.Cu", (7.0, 10.6), (13.0, 10.6), 0.2);
    b.part("H2", (20.0, 10.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    b.track("B", "B.Cu", (17.0, 10.65), (23.0, 10.65), 0.2);
    cases.push(case("hole_clearance", b, &[(Rule::HoleClearance, 2)]));

    // Edge.
    let mut b = two_layer();
    b.track("A", "F.Cu", (10.0, 0.25), (20.0, 0.25), 0.2);
    b.track("B", "F.Cu", (10.0, 29.8), (20.0, 29.8), 0.2);
    b.stub("C", 0.4, 15.0);
    b.via("D", (39.7, 10.0), 0.6, 0.3);
    b.part(
        "J1",
        (20.0, 25.0),
        0.0,
        &[thru("1", 0.0, 0.0, 1.6, 0.9, "E")],
        NONE,
    );
    cases.push(case("copper_edge", b, &[(Rule::CopperEdgeClearance, 4)]));

    let mut b = two_layer();
    b.part("H1", (10.0, 10.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    b.part("H2", (0.4, 15.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    b.part("H3", (20.0, 29.4), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    cases.push(case("hole_to_edge", b, &[(Rule::HoleToEdge, 2)]));

    // Courtyards.
    let c = |w: f64, h: f64| Draw {
        courtyard: Some((w, h)),
        silk: None,
        back: false,
    };
    let mut b = two_layer();
    b.part(
        "A1",
        (10.0, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "A")],
        c(1.0, 1.0),
    );
    b.part(
        "A2",
        (12.0, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "B")],
        c(1.0, 1.0),
    );
    b.part(
        "A3",
        (20.0, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "C")],
        c(2.0, 2.0),
    );
    b.part(
        "A4",
        (20.0, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "D")],
        c(0.5, 0.5),
    );
    b.part(
        "A5",
        (30.0, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "E")],
        c(1.0, 1.0),
    );
    b.part(
        "A6",
        (31.9, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "F")],
        c(1.0, 1.0),
    );
    b.part(
        "A7",
        (10.0, 20.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "G")],
        c(1.0, 1.0),
    );
    b.part(
        "A8",
        (10.0, 20.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "H")],
        Draw {
            back: true,
            ..c(1.0, 1.0)
        },
    );
    cases.push(case("courtyards", b, &[(Rule::CourtyardsOverlap, 2)]));

    let mut b = two_layer();
    b.part(
        "A1",
        (10.0, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "A")],
        c(1.0, 1.0),
    );
    b.raw("(footprint \"Test:OPEN\" (layer \"F.Cu\") (at 20 10 0) (property \"Reference\" \"OPEN\" (at 0 0 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr smd) (fp_line (start -1 -1) (end 1 -1) (stroke (width 0.05) (type solid)) (layer \"F.CrtYd\")) (fp_line (start 1 -1) (end 1 1) (stroke (width 0.05) (type solid)) (layer \"F.CrtYd\")) (pad \"1\" smd rect (at 0 0) (size 0.6 0.6) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")))");
    cases.push(case("open_courtyard", b, &[(Rule::MalformedCourtyard, 1)]));

    // Silkscreen.
    let mut b = two_layer();
    b.part(
        "R1",
        (10.0, 10.0),
        0.0,
        &[
            smd("1", -0.5, 0.0, 0.6, 0.6, "A"),
            smd("2", 0.5, 0.0, 0.6, 0.6, "A"),
        ],
        Draw {
            silk: Some(((-1.0, 0.0), (1.0, 0.0))),
            ..NONE
        },
    );
    b.part(
        "R2",
        (20.0, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 1.0, 1.0, "B")],
        Draw {
            silk: Some(((-2.0, 0.45), (2.0, 0.45))),
            ..NONE
        },
    );
    b.part(
        "R3",
        (30.0, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 1.0, 1.0, "C")],
        Draw {
            silk: Some(((-2.0, 0.6), (2.0, 0.6))),
            ..NONE
        },
    );
    cases.push(case("silk_over_pads", b, &[(Rule::SilkOverCopper, 2)]));

    let mut b = two_layer();
    b.silk_line((10.0, 10.0), (14.0, 10.0));
    b.silk_line((10.0, 10.1), (14.0, 10.1));
    b.part(
        "R1",
        (20.0, 10.0),
        0.0,
        &[smd("1", 0.0, 3.0, 0.6, 0.6, "A")],
        Draw {
            silk: Some(((-1.0, 0.0), (1.0, 0.0))),
            ..NONE
        },
    );
    b.part(
        "R2",
        (20.0, 10.1),
        0.0,
        &[smd("1", 0.0, 3.0, 0.6, 0.6, "B")],
        Draw {
            silk: Some(((-1.0, 0.0), (1.0, 0.0))),
            ..NONE
        },
    );
    b.part(
        "R3",
        (30.0, 10.0),
        0.0,
        &[smd("1", 0.0, 3.0, 0.6, 0.6, "C")],
        Draw {
            silk: Some(((-1.0, 0.0), (1.0, 0.0))),
            ..NONE
        },
    );
    b.part(
        "R4",
        (30.0, 10.5),
        0.0,
        &[smd("1", 0.0, 3.0, 0.6, 0.6, "D")],
        Draw {
            silk: Some(((-1.0, 0.0), (1.0, 0.0))),
            ..NONE
        },
    );
    cases.push(case("silk_close", b, &[]));

    // Which pairs of silkscreen KiCad compares.
    let mut b = two_layer();
    b.silk_line((10.0, 10.0), (14.0, 10.0)); // board line / board line
    b.silk_line((10.0, 10.1), (14.0, 10.1));
    b.silk_line((10.0, 15.0), (14.0, 15.0)); // board line / footprint line
    b.part(
        "R1",
        (12.0, 15.1),
        0.0,
        &[smd("1", 0.0, 3.0, 0.6, 0.6, "A")],
        Draw {
            silk: Some(((-1.0, 0.0), (1.0, 0.0))),
            ..NONE
        },
    );
    b.silk_text("TXT", (20.0, 10.0), 1.0, 0.15); // board text / board line
    b.silk_line((19.0, 10.2), (22.0, 10.2));
    b.silk_text("T1", (30.0, 10.0), 1.0, 0.15); // board text / board text
    b.silk_text("T2", (30.4, 10.0), 1.0, 0.15);
    b.raw("(footprint \"Test:TXT\" (layer \"F.Cu\") (at 20 20 0) (property \"Reference\" \"U9\" (at 0 0 0) (layer \"F.SilkS\") (effects (font (size 1 1) (thickness 0.15)))) (attr smd) (fp_line (start -2 0.3) (end 2 0.3) (stroke (width 0.12) (type solid)) (layer \"F.SilkS\")) (pad \"1\" smd rect (at 0 3) (size 0.6 0.6) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")))"); // footprint text / its own line
    b.part(
        "R2",
        (30.0, 20.0),
        0.0,
        &[smd("1", 0.0, 3.0, 0.6, 0.6, "B")],
        Draw {
            silk: Some(((-1.0, 0.0), (1.0, 0.0))),
            ..NONE
        },
    );
    b.raw("(footprint \"Test:TXT2\" (layer \"F.Cu\") (at 30.2 20 0) (property \"Reference\" \"U8\" (at 0 0 0) (layer \"F.SilkS\") (effects (font (size 1 1) (thickness 0.15)))) (attr smd) (pad \"1\" smd rect (at 0 3) (size 0.6 0.6) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")))"); // footprint text / other footprint's line
    cases.push(case("silk_pairs", b, &[]));

    let mut b = two_layer();
    b.silk_line((10.0, 0.05), (14.0, 0.05));
    b.silk_line((10.0, 0.5), (14.0, 0.5));
    b.silk_text("TEST", (30.0, 20.0), 0.5, 0.1);
    b.silk_text("OK", (30.0, 25.0), 1.0, 0.15);
    cases.push(case(
        "silk_edge_and_text",
        b,
        &[
            (Rule::SilkEdgeClearance, 1),
            (Rule::TextHeight, 1),
            (Rule::TextThickness, 1),
        ],
    ));

    // Solder mask.
    let mut b = two_layer();
    b.setup("(solder_mask_min_width 0.13)");
    b.part(
        "R1",
        (10.0, 10.0),
        0.0,
        &[
            smd("1", -0.35, 0.0, 0.6, 0.6, "A"),
            smd("2", 0.35, 0.0, 0.6, 0.6, "B"),
        ],
        NONE,
    );
    b.part(
        "R2",
        (20.0, 10.0),
        0.0,
        &[
            smd("1", -0.36, 0.0, 0.6, 0.6, "A"),
            smd("2", 0.36, 0.0, 0.6, 0.6, "B"),
        ],
        NONE,
    );
    b.part(
        "R3",
        (30.0, 10.0),
        0.0,
        &[
            smd("1", -0.45, 0.0, 0.6, 0.6, "A"),
            smd("2", 0.45, 0.0, 0.6, 0.6, "A"),
        ],
        NONE,
    );
    cases.push(case("mask_bridge", b, &[(Rule::SolderMaskBridge, 2)]));

    // Rule areas.
    let mut b = two_layer();
    b.keepout("\"F.Cu\"", (5.0, 5.0, 15.0, 15.0), true, true, true);
    b.track("A", "F.Cu", (6.0, 6.0), (8.0, 6.0), 0.2);
    b.track("A", "B.Cu", (6.0, 8.0), (8.0, 8.0), 0.2);
    b.via("A", (10.0, 10.0), 0.6, 0.3);
    b.part(
        "R1",
        (12.0, 12.0),
        0.0,
        &[
            smd("1", -0.5, 0.0, 0.6, 0.6, "A"),
            smd("2", 0.5, 0.0, 0.6, 0.6, "A"),
        ],
        NONE,
    );
    b.part(
        "R2",
        (4.0, 4.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "B")],
        NONE,
    );
    cases.push(case("keepout", b, &[(Rule::ItemsNotAllowed, 4)]));

    // A rule area a part carries.
    let mut b = two_layer();
    let ant = |name: &str, x: f64, angle: f64| {
        format!(
            "(footprint \"Test:{name}\" (layer \"F.Cu\") (at {x} 10 {angle}) (property \"Reference\" \"{name}\" (at 0 0 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr smd) (pad \"1\" smd rect (at 0 0) (size 1 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) (zone (net 0) (net_name \"\") (layer \"F.Cu\") (hatch edge 0.5) (connect_pads (clearance 0)) (min_thickness 0.25) (keepout (tracks not_allowed) (vias not_allowed) (pads not_allowed) (copperpour not_allowed) (footprints not_allowed)) (fill (thermal_gap 0.5) (thermal_bridge_width 0.5)) (polygon (pts (xy 2 -1) (xy 6 -1) (xy 6 1) (xy 2 1)))))"
        )
    };
    b.raw(&ant("ANT1", 10.0, 0.0));
    b.raw(&ant("ANT2", 25.0, 90.0));
    b.track("A", "F.Cu", (11.0, 9.0), (14.0, 9.0), 0.2);
    b.track("A", "F.Cu", (11.0, 11.0), (16.0, 11.0), 0.2);
    b.track("B", "F.Cu", (24.0, 9.0), (24.0, 14.0), 0.2);
    b.track("B", "F.Cu", (26.0, 9.0), (26.0, 13.0), 0.2);
    b.via("C", (13.0, 10.0), 0.6, 0.3);
    b.via("C", (25.0, 15.0), 0.6, 0.3);
    cases.push(case("part_rule_area", b, &[]));

    // Connectivity.
    let mut b = two_layer();
    b.stub("A", 10.0, 10.0);
    b.stub("A", 20.0, 10.0);
    b.stub("A", 30.0, 10.0);
    b.track("A", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.2);
    b.stub("B", 10.0, 20.0);
    b.stub("B", 20.0, 20.0);
    b.via("B", (15.0, 20.0), 0.6, 0.3);
    b.track("B", "F.Cu", (10.0, 20.0), (15.0, 20.0), 0.2);
    b.track("B", "B.Cu", (15.0, 20.0), (20.0, 21.0), 0.2);
    cases.push(case("unconnected", b, &[(Rule::UnconnectedItems, 2)]));

    let mut b = two_layer();
    b.stub("A", 10.0, 10.0);
    b.track("A", "F.Cu", (10.0, 10.0), (15.0, 10.0), 0.2);
    b.via("A", (15.0, 10.0), 0.6, 0.3);
    b.via("A", (25.0, 10.0), 0.6, 0.3);
    b.stub("B", 10.0, 20.0);
    b.track("B", "F.Cu", (10.0, 20.0), (12.0, 22.0), 0.2);
    cases.push(case(
        "dangling",
        b,
        &[(Rule::TrackDangling, 1), (Rule::ViaDangling, 2)],
    ));

    // Outline.
    let mut b = two_layer();
    b.raw("(gr_line (start 50 0) (end 60 0) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\"))");
    cases.push(case("open_outline", b, &[(Rule::InvalidOutline, 1)]));

    // How many times a pair on several layers is reported.
    let mut b = Pcb::new(4);
    b.via("A", (10.0, 10.0), 0.6, 0.3);
    b.via("B", (10.0, 10.65), 0.6, 0.3);
    b.part(
        "J1",
        (20.0, 10.0),
        0.0,
        &[thru("1", 0.0, 0.0, 1.4, 0.8, "C")],
        NONE,
    );
    b.part(
        "J2",
        (20.0, 11.5),
        0.0,
        &[thru("1", 0.0, 0.0, 1.4, 0.8, "D")],
        NONE,
    );
    b.via("E", (30.0, 10.0), 0.6, 0.3);
    b.part(
        "J3",
        (30.0, 11.1),
        0.0,
        &[thru("1", 0.0, 0.0, 1.4, 0.8, "F")],
        NONE,
    );
    b.part(
        "J4",
        (10.0, 20.0),
        0.0,
        &[thru("1", 0.0, 0.0, 1.4, 0.8, "G")],
        NONE,
    );
    b.part(
        "J5",
        (11.5, 20.0),
        0.0,
        &[smd("1", 0.0, 0.0, 1.6, 1.0, "H")],
        NONE,
    );
    cases.push(case("layers4", b, &[]));

    // A four-layer board with planes: a via joins a pad to its plane, a pad off the plane is
    // cut off.
    let mut b = Pcb::new(4);
    b.zone("GND", "In1.Cu", (2.0, 2.0, 38.0, 28.0));
    b.zone("VCC", "In2.Cu", (2.0, 2.0, 38.0, 28.0));
    b.stub("GND", 10.0, 10.0);
    b.via("GND", (10.0, 11.0), 0.6, 0.3);
    b.track("GND", "F.Cu", (10.0, 10.0), (10.0, 11.0), 0.2);
    b.stub("VCC", 20.0, 10.0);
    b.stub("GND", 30.0, 10.0);
    b.part(
        "J1",
        (10.0, 20.0),
        0.0,
        &[
            thru("1", 0.0, 0.0, 1.6, 0.9, "GND"),
            thru("2", 2.54, 0.0, 1.6, 0.9, "VCC"),
        ],
        NONE,
    );
    cases.push(case("planes", b, &[]));

    cases
}

#[test]
fn every_violation_of_the_corpus_is_found_and_kicad_reports_the_same() {
    let cases = corpus();
    let have_kicad = netsmith_fab::KicadCliExe::locate().is_some();
    let (mut agreed, mut failures) = (0usize, Vec::new());
    let results: Vec<_> = std::thread::scope(|s| {
        let handles: Vec<_> = cases
            .iter()
            .map(|c| {
                s.spawn(move || {
                    let mut board = Pcb::new(c.board.layers);
                    let _ = &mut board;
                    let mut b = c.board.clone_for_run();
                    let (pcb, pro) = (b.pcb(), b.pro());
                    let mine = drc::check(&pcb, Some(&pro), &Extras::default())
                        .unwrap_or_else(|e| panic!("{}: {e}", c.name));
                    let theirs = if have_kicad {
                        let k = kicad_findings(c.name, &pcb, &pro);
                        if std::env::var("DRC_DEBUG").is_ok() {
                            eprintln!("--- {} (KiCad)", c.name);
                            for f in k.iter().flatten() {
                                eprintln!(
                                    "  {} [{}] {} {:?}",
                                    f.kind,
                                    f.severity.word(),
                                    f.description,
                                    f.items
                                );
                            }
                        }
                        k
                    } else {
                        None
                    };
                    (mine, theirs)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for (c, (mine, theirs)) in cases.iter().zip(results) {
        for (rule, n) in &c.expect {
            if mine.count(*rule) != *n {
                failures.push(format!(
                    "{}: expected {n} of {}, netsmith found {}:\n{mine}",
                    c.name,
                    rule.id(),
                    mine.count(*rule)
                ));
            }
        }
        if let Some(theirs) = theirs {
            let cmp = kicad::compare(&mine, &theirs, 0.1);
            agreed += cmp.agreed;
            if !cmp.agrees() {
                failures.push(format!("{}: {cmp}\nnetsmith:\n{mine}", c.name));
            }
        }
    }
    eprintln!(
        "corpus: {} boards, {agreed} findings agree with KiCad",
        cases.len()
    );
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
