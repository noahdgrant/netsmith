//! A corpus of boards with a violation put in on purpose, written by `tests/common`: what
//! netsmith's design rules check finds in each, and that KiCad's check (when installed)
//! reports the same findings at the same places (ADR 0037).

use crate::common;

use common::*;
use netsmith_fab::{compare_kicad_drc};
use netsmith_geom::{drc_check, DrcExtras, DrcRule};

struct Case {
    name: &'static str,
    board: Pcb,
    /// How many findings of each rule netsmith must report; other rules are free.
    expect: Vec<(DrcRule, usize)>,
}

fn case(name: &'static str, board: Pcb, expect: &[(DrcRule, usize)]) -> Case {
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
    cases.push(case("track_track", b, &[(DrcRule::Clearance, 1)]));

    let mut b = two_layer();
    b.stub("A", 15.0, 10.0);
    b.track("B", "F.Cu", (10.0, 10.5), (20.0, 10.5), 0.2);
    cases.push(case("track_pad", b, &[(DrcRule::Clearance, 1)]));

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
        &[(DrcRule::Clearance, 1), (DrcRule::ShortingItems, 1)],
    ));

    let mut b = two_layer();
    b.via("A", (10.0, 10.0), 0.6, 0.3);
    b.track("B", "B.Cu", (5.0, 10.45), (15.0, 10.45), 0.2);
    b.via("B", (20.0, 10.0), 0.6, 0.3);
    b.via("C", (20.0, 10.65), 0.6, 0.3);
    b.stub("D", 30.0, 10.0);
    b.via("E", (30.0, 10.7), 0.6, 0.3);
    cases.push(case("via_to_others", b, &[(DrcRule::Clearance, 4)]));

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
    cases.push(case("thru_pads_close", b, &[(DrcRule::Clearance, 1)]));

    // Shorts.
    let mut b = two_layer();
    b.stub("C", 30.0, 10.0);
    b.stub("D", 30.2, 10.0);
    cases.push(case("shorts", b, &[(DrcRule::ShortingItems, 1)]));

    let mut b = two_layer();
    b.wire("A", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.2);
    b.wire("B", "F.Cu", (15.0, 5.0), (15.0, 15.0), 0.2);
    cases.push(case("crossing", b, &[(DrcRule::TracksCrossing, 1)]));

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
            (DrcRule::TrackWidth, 1),
            (DrcRule::ViaDiameter, 1),
            (DrcRule::AnnularWidth, 2),
            (DrcRule::DrillOutOfRange, 2),
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
        &[(DrcRule::AnnularWidth, 1), (DrcRule::DrillOutOfRange, 1)],
    ));

    // Holes.
    let mut b = two_layer();
    b.part("H1", (10.0, 10.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    b.part("H2", (10.7, 10.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    b.part("H3", (20.0, 10.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    b.part("H4", (21.3, 10.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    cases.push(case("hole_to_hole", b, &[(DrcRule::HoleToHole, 2)]));

    let mut b = two_layer();
    b.part("H1", (10.0, 10.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    b.track("A", "F.Cu", (7.0, 10.6), (13.0, 10.6), 0.2);
    b.part("H2", (20.0, 10.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    b.track("B", "B.Cu", (17.0, 10.65), (23.0, 10.65), 0.2);
    cases.push(case("hole_clearance", b, &[(DrcRule::HoleClearance, 2)]));

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
    cases.push(case("copper_edge", b, &[(DrcRule::CopperEdgeClearance, 4)]));

    let mut b = two_layer();
    b.part("H1", (10.0, 10.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    b.part("H2", (0.4, 15.0), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    b.part("H3", (20.0, 29.4), 0.0, &[hole(0.0, 0.0, 1.0)], NONE);
    cases.push(case("hole_to_edge", b, &[(DrcRule::HoleToEdge, 2)]));

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
    cases.push(case("courtyards", b, &[(DrcRule::CourtyardsOverlap, 2)]));

    let mut b = two_layer();
    b.part(
        "A1",
        (10.0, 10.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "A")],
        c(1.0, 1.0),
    );
    b.raw("(footprint \"Test:OPEN\" (layer \"F.Cu\") (at 20 10 0) (property \"Reference\" \"OPEN\" (at 0 0 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr smd) (fp_line (start -1 -1) (end 1 -1) (stroke (width 0.05) (type solid)) (layer \"F.CrtYd\")) (fp_line (start 1 -1) (end 1 1) (stroke (width 0.05) (type solid)) (layer \"F.CrtYd\")) (pad \"1\" smd rect (at 0 0) (size 0.6 0.6) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")))");
    cases.push(case("open_courtyard", b, &[(DrcRule::MalformedCourtyard, 1)]));

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
    cases.push(case("silk_over_pads", b, &[(DrcRule::SilkOverCopper, 2)]));

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
            (DrcRule::SilkEdgeClearance, 1),
            (DrcRule::TextHeight, 1),
            (DrcRule::TextThickness, 1),
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
    cases.push(case("mask_bridge", b, &[(DrcRule::SolderMaskBridge, 2)]));

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
    cases.push(case("keepout", b, &[(DrcRule::ItemsNotAllowed, 4)]));

    // A rule area a part carries (issue #1291): a board file has its points in the board's
    // frame, where the footprint is (issue #1237). ANT1's, beside its pad, forbids tracks, vias
    // and pads on F.Cu: a track and a via of another net and a pad of another part in it are
    // flagged, a track on B.Cu under it is not. ANT2's covers its own pad and forbids pads,
    // tracks and vias: its own pad is flagged, as KiCad 9 flags it, and so is a via beside the
    // pad. ANT3's covers its own courtyard and R2's and forbids footprints alone: R2 is flagged,
    // ANT3 itself is not.
    let mut b = two_layer();
    let ant = |name: &str, x: f64, area: (f64, f64, f64, f64), keep: &str| {
        let (x0, y0, x1, y1) = area;
        format!(
            "(footprint \"Test:{name}\" (layer \"F.Cu\") (at {x} 10 0) (property \"Reference\" \"{name}\" (at 0 0 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr smd) (pad \"1\" smd rect (at 0 0) (size 1 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) (zone (net 0) (net_name \"\") (layer \"F.Cu\") (hatch edge 0.5) (connect_pads (clearance 0)) (min_thickness 0.25) (keepout {keep}) (fill (thermal_gap 0.5) (thermal_bridge_width 0.5)) (polygon (pts (xy {x0} {y0}) (xy {x1} {y0}) (xy {x1} {y1}) (xy {x0} {y1})))))"
        )
    };
    let forbids = "(tracks not_allowed) (vias not_allowed) (pads not_allowed) (copperpour not_allowed) (footprints allowed)";
    b.raw(&ant("ANT1", 10.0, (12.0, 8.0, 18.0, 12.0), forbids));
    b.raw(&ant("ANT2", 30.0, (28.0, 8.0, 32.0, 12.0), forbids));
    b.track("A", "F.Cu", (14.0, 10.0), (16.0, 10.0), 0.2);
    b.track("A", "B.Cu", (14.0, 9.0), (16.0, 9.0), 0.2);
    b.via("C", (16.5, 11.2), 0.6, 0.3);
    b.via("C", (31.2, 11.2), 0.6, 0.3);
    b.part(
        "R1",
        (13.0, 8.8),
        0.0,
        &[smd("1", 0.0, 0.0, 0.4, 0.4, "D")],
        NONE,
    );
    b.raw("(footprint \"Test:ANT3\" (layer \"F.Cu\") (at 10 22 0) (property \"Reference\" \"ANT3\" (at 0 0 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr smd) (pad \"1\" smd rect (at 0 0) (size 1 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) (fp_rect (start -1 -1) (end 1 1) (stroke (width 0.05) (type solid)) (fill no) (layer \"F.CrtYd\")) (zone (net 0) (net_name \"\") (layer \"F.Cu\") (hatch edge 0.5) (connect_pads (clearance 0)) (min_thickness 0.25) (keepout (tracks allowed) (vias allowed) (pads allowed) (copperpour allowed) (footprints not_allowed)) (fill (thermal_gap 0.5) (thermal_bridge_width 0.5)) (polygon (pts (xy 7 19) (xy 17 19) (xy 17 25) (xy 7 25)))))");
    b.part(
        "R2",
        (14.0, 22.0),
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 0.6, "E")],
        Draw {
            courtyard: Some((0.8, 0.8)),
            ..NONE
        },
    );
    cases.push(case(
        "part_rule_area",
        b,
        &[(DrcRule::ItemsNotAllowed, 6)],
    ));

    // What only touches a rule area's edge is not inside it (issue #1372): KiCad shrinks the area
    // by its DRC epsilon (half a micrometre) before it asks what reaches into it, so a library's
    // keepout drawn up to its own pads flags none of them. Measured with KiCad 9.0.0; 9.0.8's
    // `collidesWithArea` is the same. A pad of the footprint's own (x 11 to 12) and another part's
    // (y 12 to 12.4) touching the edge, a track's round end and a via touching it: nothing. The
    // footprint's own pad, a track and a via a micrometre or more into it: flagged.
    let mut b = two_layer();
    b.raw("(footprint \"Test:ANT\" (layer \"F.Cu\") (at 15 10 0) (property \"Reference\" \"ANT1\" (at 0 0 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr smd) (pad \"1\" smd rect (at -3.5 0) (size 1 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) (pad \"2\" smd rect (at 3.499 0) (size 1 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) (zone (net 0) (net_name \"\") (layer \"F.Cu\") (hatch edge 0.5) (connect_pads (clearance 0)) (min_thickness 0.25) (keepout (tracks not_allowed) (vias not_allowed) (pads not_allowed) (copperpour allowed) (footprints allowed)) (fill (thermal_gap 0.5) (thermal_bridge_width 0.5)) (polygon (pts (xy 12 8) (xy 18 8) (xy 18 12) (xy 12 12)))))");
    b.track("A", "F.Cu", (14.0, 6.0), (14.0, 7.9), 0.2);
    b.track("A", "F.Cu", (16.5, 6.0), (16.5, 7.95), 0.2);
    b.via("C", (13.0, 12.3), 0.6, 0.3);
    b.via("C", (17.0, 12.29), 0.6, 0.3);
    b.part(
        "R1",
        (15.0, 12.2),
        0.0,
        &[smd("1", 0.0, 0.0, 0.4, 0.4, "D")],
        NONE,
    );
    cases.push(case(
        "part_rule_area_edge",
        b,
        &[(DrcRule::ItemsNotAllowed, 3)],
    ));

    // A hole of a rule area (a further `polygon` of the zone, a cutout) is not in it (issue
    // #1392): a module's keepout cut out round its own pads flags none of the pads in the hole.
    // KiCad tests an item against the outline less its holes (`collidesWithArea`, measured with
    // KiCad 9.0.0). The footprint's own pad and another part's pad in the hole: nothing. The
    // footprint's own pad across the hole's edge and its own pad in the keepout: flagged (KiCad
    // flags a footprint's own pads inside its own rule area).
    let mut b = two_layer();
    b.raw("(footprint \"Test:MOD\" (layer \"F.Cu\") (at 15 10 0) (property \"Reference\" \"U1\" (at 0 0 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr smd) (pad \"1\" smd rect (at -2.5 -0.5) (size 0.6 0.6) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) (pad \"2\" smd rect (at 2 0) (size 0.6 0.6) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) (pad \"3\" smd rect (at -1 0.5) (size 0.6 0.6) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) (zone (net 0) (net_name \"\") (layer \"F.Cu\") (hatch edge 0.5) (connect_pads (clearance 0)) (min_thickness 0.25) (keepout (tracks not_allowed) (vias not_allowed) (pads not_allowed) (copperpour not_allowed) (footprints allowed)) (fill (thermal_gap 0.5) (thermal_bridge_width 0.5)) (polygon (pts (xy 10 8) (xy 20 8) (xy 20 12) (xy 10 12))) (polygon (pts (xy 11 8.5) (xy 14 8.5) (xy 14 11.5) (xy 11 11.5)))))");
    b.part(
        "R1",
        (12.5, 10.7),
        0.0,
        &[smd("1", 0.0, 0.0, 0.4, 0.4, "D")],
        NONE,
    );
    cases.push(case(
        "part_rule_area_hole",
        b,
        &[(DrcRule::ItemsNotAllowed, 2)],
    ));

    // A fill the board file holds that reaches into a footprint's keepout that forbids pours
    // (issue #1291): KiCad 9's check does not judge a zone's fill against a rule area (nor
    // against a board's, measured), and neither does netsmith's. The fill keeps out of the
    // keepout itself: netsmith's and KiCad's refill (`netsmith-router`'s `footprint_keepout`).
    let mut b = two_layer();
    b.raw("(footprint \"Test:ANT\" (layer \"F.Cu\") (at 20 10 0) (property \"Reference\" \"U1\" (at 0 0 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr smd) (zone (net 0) (net_name \"\") (layer \"F.Cu\") (hatch edge 0.5) (connect_pads (clearance 0)) (min_thickness 0.25) (keepout (tracks allowed) (vias allowed) (pads allowed) (copperpour not_allowed) (footprints allowed)) (fill (thermal_gap 0.5) (thermal_bridge_width 0.5)) (polygon (pts (xy 16 7) (xy 24 7) (xy 24 13) (xy 16 13)))))");
    b.stub("GND", 5.0, 10.0);
    b.raw("(zone (net 1) (net_name \"GND\") (layer \"F.Cu\") (hatch edge 0.5) (connect_pads (clearance 0.2)) (min_thickness 0.2) (filled_areas_thickness no) (fill yes (thermal_gap 0.3) (thermal_bridge_width 0.3)) (polygon (pts (xy 1 1) (xy 39 1) (xy 39 29) (xy 1 29))) (filled_polygon (layer \"F.Cu\") (pts (xy 1 1) (xy 20 1) (xy 20 29) (xy 1 29))))");
    cases.push(case(
        "part_rule_area_fill",
        b,
        &[(DrcRule::ItemsNotAllowed, 0)],
    ));

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
    cases.push(case("unconnected", b, &[(DrcRule::UnconnectedItems, 2)]));

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
        &[(DrcRule::TrackDangling, 1), (DrcRule::ViaDangling, 2)],
    ));

    // Outline.
    let mut b = two_layer();
    b.raw("(gr_line (start 50 0) (end 60 0) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\"))");
    cases.push(case("open_outline", b, &[(DrcRule::InvalidOutline, 1)]));

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
    let have_kicad = netsmith_fab::KicadCliExe::locate_runnable().is_some();
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
                    let mine = drc_check(&pcb, Some(&pro), &DrcExtras::default())
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
            let cmp = compare_kicad_drc(&mine, &theirs, 0.1);
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
