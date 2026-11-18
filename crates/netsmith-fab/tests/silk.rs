//! The silkscreen rules of the design rules check (issue #1186): `silk_over_copper`,
//! `silk_overlap` and `silk_edge_clearance`, each with a board that breaks it and one that does
//! not, judged to JLCPCB's numbers (a text 0.15 mm from an opening in the mask, through the
//! `.kicad_dru`, and 0.15 mm between silkscreen and to the edge, the project's
//! `min_silk_clearance`). Where KiCad is
//! installed its check runs on the same files and the two agree, the places of the items within
//! 0.01 mm; the test skips that part where it is not.

use crate::common;

use common::*;
use netsmith_fab::{KicadDrcComparison, compare_kicad_drc_with_rules_file};
use netsmith_geom::{DrcExtras, DrcReport, DrcRule, DrcRules, drc_check, drc_kicad_dru};

const NONE: Draw = Draw {
    courtyard: None,
    silk: None,
    back: false,
};

/// A board with JLCPCB's silkscreen numbers in its project file.
fn board() -> Pcb {
    let mut b = Pcb::new(2);
    b.rules = Rules {
        silk: 0.15,
        ..Rules::default()
    };
    b
}

/// JLCPCB's space from a text to a pad (`.min_silk_to_pad`), which the `.kicad_dru` holds.
fn process() -> DrcExtras {
    DrcExtras {
        text_to_mask: Some(0.15),
        ..DrcExtras::default()
    }
}

/// netsmith's check of the board, with the rules file `fab` writes read back, and KiCad's
/// comparison where KiCad is installed.
fn check(name: &str, b: &mut Pcb) -> (DrcReport, Option<KicadDrcComparison>) {
    let (pcb, pro) = (b.pcb(), b.pro());
    let rules = drc_kicad_dru(&DrcRules::from_project(&pro).unwrap(), &process());
    assert!(
        rules.contains("(rule \"netsmith silk_over_copper\"\n\t(constraint silk_clearance (min 0.15mm))"),
        "{rules}"
    );
    let ours = drc_check(&pcb, Some(&pro), &DrcExtras::default().with_rules_file(&rules))
        .unwrap_or_else(|e| panic!("{e}"));
    let compared = kicad_findings_with(name, &pcb, &pro, &rules)
        .map(|theirs| compare_kicad_drc_with_rules_file(&ours, &theirs, 0.01));
    if let Some(c) = &compared {
        eprintln!("{name}: {c}");
    } else {
        eprintln!("{name}: skipped the comparison: KiCad is not installed");
    }
    (ours, compared)
}

fn agrees(name: &str, c: &Option<KicadDrcComparison>, ours: &DrcReport) {
    if let Some(c) = c {
        assert!(c.agrees(), "{name}: the two checks differ:\n{c}netsmith's:\n{ours}");
    }
}

/// A part of one surface pad 1 mm square on `net` at `at`, with a silkscreen line of its own.
fn pad_with_line(b: &mut Pcb, refdes: &str, at: (f64, f64), net: &'static str, line: ((f64, f64), (f64, f64))) {
    b.part(
        refdes,
        at,
        0.0,
        &[smd("1", 0.0, 0.0, 1.0, 1.0, net)],
        Draw {
            silk: Some(line),
            ..NONE
        },
    );
}

#[test]
fn silk_over_a_pad_a_drawing_on_the_mask_or_an_open_via_and_text_near_a_pad_are_found_and_silk_clear_of_them_is_not()
 {
    let mut b = board();
    // A line over the pad, and a text 0.1 mm from a pad's opening (the capitals' tops 0.575 mm
    // above the anchor in netsmith's font): JLCPCB removes a character within 0.15 mm of a pad.
    pad_with_line(&mut b, "R1", (5.0, 5.0), "A", ((-1.0, 0.0), (1.0, 0.0)));
    b.part(
        "R2",
        (10.0, 5.0),
        0.0,
        &[smd("1", 0.0, 0.0, 1.0, 1.0, "B")],
        NONE,
    );
    b.silk_text("R2", (10.0, 6.175), 1.0, 0.15);
    // A rectangle drawn on F.Mask, which opens the mask, under a line of the board's silkscreen.
    b.raw("(gr_rect (start 14 4) (end 16 6) (stroke (width 0) (type default)) (fill yes) (layer \"F.Mask\"))");
    b.silk_line((13.0, 5.0), (17.0, 5.0));
    // A via the mask leaves open, under a line; one it covers, under another.
    b.via("V", (20.0, 20.0), 0.6, 0.3);
    b.raw("(via (at 20 5) (size 0.6) (drill 0.3) (layers \"F.Cu\" \"B.Cu\") (tenting none) (net 3))");
    b.silk_line((19.0, 5.0), (21.0, 5.0));
    b.silk_line((19.0, 20.0), (21.0, 20.0));
    let (ours, kicad) = check("silk_over_copper", &mut b);
    let found: Vec<_> = ours
        .findings
        .iter()
        .filter(|f| f.rule == DrcRule::SilkOverCopper)
        .collect();
    assert_eq!(found.len(), 4, "{ours}");
    assert!(
        found.iter().any(|f| f.message.contains("is 0.1000 mm from the opening")),
        "{ours}"
    );
    assert!(found.iter().any(|f| f.message.contains("Polygon on F.Mask")), "{ours}");
    assert!(found.iter().any(|f| f.message.contains("of Via [V]")), "{ours}");
    agrees("silk_over_copper", &kicad, &ours);

    let mut clear = board();
    // A line 0.1 mm from the pad's opening (the line 0.12 mm wide, its centre 0.66 mm from the
    // pad's), which the board house prints whole; over a track, which the mask covers; over a
    // tented via.
    pad_with_line(&mut clear, "R1", (5.0, 5.0), "A", ((-1.0, 0.66), (1.0, 0.66)));
    clear.wire("T", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.2);
    clear.silk_line((15.0, 9.0), (15.0, 11.0));
    clear.via("V", (20.0, 20.0), 0.6, 0.3);
    clear.silk_line((19.0, 20.0), (21.0, 20.0));
    let (ours, kicad) = check("silk_clear_of_copper", &mut clear);
    assert_eq!(ours.count(DrcRule::SilkOverCopper), 0, "{ours}");
    agrees("silk_clear_of_copper", &kicad, &ours);
}

#[test]
fn silkscreen_of_two_parts_too_near_is_found_and_a_parts_own_lines_and_a_hidden_designator_are_not() {
    let mut b = board();
    // Two parts' lines 0.1 mm apart (0.22 mm between their centres, each 0.12 mm wide).
    pad_with_line(&mut b, "R1", (5.0, 5.0), "A", ((-1.0, 2.0), (1.0, 2.0)));
    pad_with_line(&mut b, "R2", (5.0, 9.22), "B", ((-1.0, -2.0), (1.0, -2.0)));
    // A text of the board 0.1 mm under a part's line.
    pad_with_line(&mut b, "R3", (15.0, 5.0), "C", ((-2.0, 2.0), (2.0, 2.0)));
    b.silk_text("TOP", (15.0, 7.735), 1.0, 0.15);
    let (ours, kicad) = check("silk_overlap", &mut b);
    assert_eq!(ours.count(DrcRule::SilkOverlap), 2, "{ours}");
    agrees("silk_overlap", &kicad, &ours);

    let mut clear = board();
    // A part's own lines that meet (its outline), and two parts' lines 0.2 mm apart.
    clear.part(
        "R1",
        (5.0, 5.0),
        0.0,
        &[smd("1", 0.0, 0.0, 1.0, 1.0, "A")],
        Draw {
            courtyard: None,
            silk: Some(((-1.0, 2.0), (1.0, 2.0))),
            back: false,
        },
    );
    clear.raw("(footprint \"Test:OUT\" (layer \"F.Cu\") (at 15 5 0) (property \"Reference\" \"U1\" (at 0 -3 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr smd) (fp_line (start -1 1) (end 1 1) (stroke (width 0.12) (type solid)) (layer \"F.SilkS\")) (fp_line (start 1 1) (end 1 -1) (stroke (width 0.12) (type solid)) (layer \"F.SilkS\")) (pad \"1\" smd rect (at 0 0) (size 0.6 0.6) (layers \"F.Cu\" \"F.Mask\")))");
    pad_with_line(&mut clear, "R2", (5.0, 9.32), "B", ((-1.0, -2.0), (1.0, -2.0)));
    // A designator the design hides (`.silk &= "hide"`: hidden in the file) over another
    // part's line: it is not printed, so not checked.
    clear.raw("(footprint \"Test:HID\" (layer \"F.Cu\") (at 25 5 0) (property \"Reference\" \"U2\" (at 0 2 0) (layer \"F.SilkS\") (hide yes) (effects (font (size 1 1) (thickness 0.15)))) (attr smd) (pad \"1\" smd rect (at 0 -2 0) (size 0.6 0.6) (layers \"F.Cu\" \"F.Mask\")))");
    pad_with_line(&mut clear, "R3", (25.0, 5.0), "C", ((-2.0, 2.0), (2.0, 2.0)));
    let (ours, kicad) = check("silk_apart", &mut clear);
    assert_eq!(ours.count(DrcRule::SilkOverlap), 0, "{ours}");
    agrees("silk_apart", &kicad, &ours);
}

#[test]
fn silkscreen_too_near_the_edge_a_cutout_or_the_margin_is_found_and_silk_clear_of_them_is_not() {
    let mut b = board();
    // 0.1 mm from the outline's line at x = 40 (to the line's centre: KiCad gives the edge no
    // width), 0.1 mm from a rectangular cutout, and 0.08 mm from a line on Margin.
    b.silk_line((39.84, 10.0), (39.84, 12.0));
    b.raw("(gr_rect (start 20 10) (end 24 14) (stroke (width 0.05) (type default)) (fill no) (layer \"Edge.Cuts\"))");
    b.silk_line((21.0, 14.16), (23.0, 14.16));
    b.raw("(gr_line (start 10 20) (end 10 24) (stroke (width 0.12) (type default)) (layer \"Margin\"))");
    b.silk_line((10.2, 20.0), (10.2, 24.0));
    let (ours, kicad) = check("silk_edge", &mut b);
    let found: Vec<_> = ours
        .findings
        .iter()
        .filter(|f| f.rule == DrcRule::SilkEdgeClearance)
        .collect();
    assert_eq!(found.len(), 3, "{ours}");
    assert!(found.iter().any(|f| f.message.contains("0.1000 mm")), "{ours}");
    assert!(found.iter().any(|f| f.message.contains("0.0800 mm")), "{ours}");
    agrees("silk_edge", &kicad, &ours);

    let mut clear = board();
    // 0.16 mm from the line's centre: the 0.05 mm line's own width does not count.
    clear.silk_line((39.78, 10.0), (39.78, 12.0));
    clear.raw("(gr_rect (start 20 10) (end 24 14) (stroke (width 0.05) (type default)) (fill no) (layer \"Edge.Cuts\"))");
    clear.silk_line((21.0, 14.3), (23.0, 14.3));
    let (ours, kicad) = check("silk_inside", &mut clear);
    assert_eq!(ours.count(DrcRule::SilkEdgeClearance), 0, "{ours}");
    agrees("silk_inside", &kicad, &ours);
}

#[test]
fn a_text_the_font_decides_is_listed_and_kicads_finding_of_it_is_not_a_difference() {
    let mut b = board();
    // `R1` in netsmith's font ends 0.55 mm right of its anchor; KiCad's `1` is wider and ends
    // 0.86 mm right. A line 0.2 mm clear of netsmith's ink crosses KiCad's.
    b.silk_text("R1", (10.0, 10.0), 1.0, 0.15);
    b.silk_line((10.81, 9.0), (10.81, 11.0));
    // Far from anything: no pair the font decides.
    b.silk_text("C2", (25.0, 20.0), 1.0, 0.15);
    let (ours, kicad) = check("silk_font", &mut b);
    assert_eq!(ours.count(DrcRule::SilkOverlap), 0, "{ours}");
    assert_eq!(ours.font_dependent.len(), 1, "{:?}", ours.font_dependent);
    assert_eq!(ours.font_dependent[0].rule, DrcRule::SilkOverlap);
    if let Some(c) = &kicad {
        assert!(c.agrees(), "{c}");
        assert_eq!(c.measured_otherwise, 1, "KiCad's font crosses the line: {c}");
    }
}
