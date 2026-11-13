//! A through-hole part whose footprint draws its courtyard on one face's layer only is judged
//! on the other face by its pads, so a large part may sit on the other face over the rest of
//! its body (ADR 0031). A footprint that draws both courtyard layers, or none,
//! keeps counting on both faces. The parts are invented: a right-angle header whose two pads
//! are at one end of a 5 x 13.5 mm body, and an 8 x 8 mm part with two pads.

use crate::common;

use netsmith_testkit::TempDir;
use std::collections::BTreeMap;

use netsmith_core::{Design, Features, Net, Outline, Part, PinRef};
use netsmith_pcb::board::PlacementProblem;
use netsmith_pcb::{Board, FootprintLibrary, PlaceOptions, Pt, Side};

use common::process;

/// The header: pads at the top of the body, courtyard on the layers `layers` names.
fn angle(layers: &[&str]) -> String {
    let yards: String = layers
        .iter()
        .map(|l| {
            format!("(fp_rect (start -2.5 -1.5) (end 2.5 12) (stroke (width 0.05) (type solid)) (fill none) (layer \"{l}\")) ")
        })
        .collect();
    format!(
        "(footprint \"ANGLE\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
         (property \"Reference\" \"REF**\" (at 0 -2.5 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (property \"Value\" \"ANGLE\" (at 0 13 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (attr through_hole) \
         (pad \"1\" thru_hole circle (at -1.27 0) (size 1.7 1.7) (drill 1) (layers \"*.Cu\" \"*.Mask\")) \
         (pad \"2\" thru_hole circle (at 1.27 0) (size 1.7 1.7) (drill 1) (layers \"*.Cu\" \"*.Mask\")) \
         {yards})\n"
    )
}

const BIG: &str = "(footprint \"BIG\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
    (property \"Reference\" \"REF**\" (at 0 -4.8 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
    (property \"Value\" \"BIG\" (at 0 4.8 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
    (attr smd) \
    (pad \"1\" smd rect (at -3 0) (size 1 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (pad \"2\" smd rect (at 3 0) (size 1 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (fp_rect (start -4 -4) (end 4 4) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n";

/// The header `J1` and the big part `U1`, on an outline of `size` mm, placed as `options` say.
fn board(
    header: &str,
    size: (f64, f64),
    options: &PlaceOptions,
) -> Result<Board, netsmith_pcb::BoardError> {
    let folder = TempDir::new("hole-faces");
    std::fs::write(folder.join("ANGLE.kicad_mod"), header).unwrap();
    std::fs::write(folder.join("BIG.kicad_mod"), BIG).unwrap();
    let part = |refdes: &str, package: &str| Part {
        provides: Vec::new(),
        refdes: refdes.into(),
        kind: "Resistor".into(),
        value: package.into(),
        package: Some(package.into()),
        path: String::new(),
        pins: Vec::new(),
    };
    let pin = |refdes: &str, pin: &str| PinRef {
        refdes: refdes.into(),
        pin: pin.into(),
    };
    let design = Design {
        modules: Vec::new(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        name: "Faces".into(),
        process: Some(process()),
        features: Features {
            outline: Some(Outline {
                width: (size.0, size.0),
                height: (size.1, size.1),
                corner_radius: 0.0,
                polygon: None,
            }),
            ..Features::default()
        },
        parts: vec![part("J1", "ANGLE"), part("U1", "BIG")],
        nets: vec![Net {
            name: "N".into(),
            pins: vec![pin("J1", "1"), pin("U1", "1")],
        }],
    };
    let mapping = netsmith_kicad::Mapping::parse(
        "[[part]]\nkind = \"Resistor\"\npackage = \"ANGLE\"\nsymbol = \"t:R\"\nfootprint = \"t:ANGLE\"\n\
         [[part]]\nkind = \"Resistor\"\npackage = \"BIG\"\nsymbol = \"t:R\"\nfootprint = \"t:BIG\"\n",
        "faces",
        std::path::Path::new("."),
    )
    .unwrap();
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let library = FootprintLibrary::at(netsmith_testkit::sample_footprints())
        .with_library("t", folder.path());
    netsmith_place_route::board_from_design_placed(&design, &mapping, &library, options)
}

/// The overlaps the placement check finds.
fn overlaps(board: &Board) -> Vec<String> {
    board
        .check_placement()
        .into_iter()
        .filter(|p| matches!(p, PlacementProblem::Overlap { .. }))
        .map(|p| p.to_string())
        .collect()
}

/// The board with the header at the top left of a 30 mm square, and the big part on the back
/// with its centre `down` mm below the header's pads.
fn stacked(header: &str, down: f64) -> Board {
    let mut b = board(header, (30.0, 30.0), &PlaceOptions::default()).unwrap();
    let origin = b.outline.min;
    for p in &mut b.parts {
        p.rotation = 0.0;
        match p.refdes.as_str() {
            "J1" => p.at = origin + Pt::new(10.0, 8.0),
            _ => {
                p.side = Side::Back;
                p.at = origin + Pt::new(10.0, 8.0 + down);
            }
        }
    }
    b
}

#[test]
fn a_big_part_on_the_other_face_may_stand_over_the_body_of_a_header_that_draws_one_courtyard() {
    // The big part's courtyard (11 to 19 mm down) is over the header's body (to 20 mm) but
    // well clear of its pads.
    let b = stacked(&angle(&["F.CrtYd"]), 15.0);
    assert_eq!(overlaps(&b), Vec::<String>::new());
    // Netsmith's design rules check agrees: the courtyards are on different layers.
    let report = netsmith_fab::drc::check(
        &netsmith_pcb::kicad_pcb(&b),
        Some(&netsmith_pcb::kicad_pro(&b)),
        &Default::default(),
    )
    .unwrap();
    // (The parts are not routed; only the missing connection is found.)
    let errors: Vec<String> = report
        .errors()
        .map(|f| f.to_string())
        .filter(|e| !e.contains("[unconnected_items]"))
        .collect();
    assert_eq!(errors, Vec::<String>::new());
}

#[test]
fn the_same_two_parts_overlap_when_the_footprint_draws_both_courtyards() {
    let b = stacked(&angle(&["F.CrtYd", "B.CrtYd"]), 15.0);
    assert_eq!(overlaps(&b).len(), 1, "{:?}", overlaps(&b));
}

#[test]
fn a_footprint_with_no_courtyard_counts_the_box_made_from_its_pads_on_both_faces_as_before() {
    // No courtyard is drawn: the box is the pads and a small margin, on both faces.
    let b = stacked(&angle(&[]), 2.0);
    assert_eq!(overlaps(&b).len(), 1, "{:?}", overlaps(&b));
    let b = stacked(&angle(&[]), 15.0);
    assert_eq!(overlaps(&b), Vec::<String>::new());
}

#[test]
fn a_part_over_the_pads_of_the_header_on_the_other_face_is_still_an_error() {
    // The big part's courtyard covers the two pads.
    let b = stacked(&angle(&["F.CrtYd"]), 2.0);
    assert_eq!(overlaps(&b).len(), 1, "{:?}", overlaps(&b));
    // A hair clear of the pads and their clearance is not.
    let origin = b.outline.min;
    let mut b = b;
    let pad_edge = 0.85 + b.rules.clearance;
    b.parts[1].at = origin + Pt::new(10.0, 8.0 + pad_edge + 4.0 + 0.01);
    assert_eq!(overlaps(&b), Vec::<String>::new());
    b.parts[1].at = origin + Pt::new(10.0, 8.0 + pad_edge + 4.0 - 0.05);
    assert_eq!(overlaps(&b).len(), 1);
}

#[test]
fn the_placer_puts_the_big_part_under_the_body_where_the_old_rule_had_no_room() {
    // Inside the process's wall (0.75 mm) the board has 12.5 x 20.5 mm. The header's body takes 5 x 13.5 mm
    // and the big part 8 x 8 mm: side by side they need 13 mm of width, one above the other
    // 21.5 mm of height (each more than there is), so they only fit if they share the area.
    let options = PlaceOptions {
        sides: BTreeMap::from([("U1".to_string(), Side::Back)]),
        ..PlaceOptions::default()
    };
    let b = board(&angle(&["F.CrtYd"]), (14.0, 22.0), &options).unwrap();
    assert_eq!(b.check_placement(), vec![]);
    let (j, u) = (b.part("J1").unwrap(), b.part("U1").unwrap());
    assert!(
        j.courtyard().overlaps(&u.courtyard()),
        "they share area: {:?} {:?}",
        j.courtyard(),
        u.courtyard()
    );
    let report = netsmith_fab::drc::check(
        &netsmith_pcb::kicad_pcb(&b),
        Some(&netsmith_pcb::kicad_pro(&b)),
        &Default::default(),
    )
    .unwrap();
    let errors: Vec<String> = report.errors().map(|f| f.to_string()).collect();
    assert!(
        errors.iter().all(|e| e.contains("[unconnected_items]")),
        "{errors:?}"
    );
}

#[test]
fn a_footprint_that_draws_both_courtyards_does_not_fit_there() {
    let options = PlaceOptions {
        sides: BTreeMap::from([("U1".to_string(), Side::Back)]),
        ..PlaceOptions::default()
    };
    let error = board(&angle(&["F.CrtYd", "B.CrtYd"]), (14.0, 22.0), &options)
        .unwrap_err()
        .to_string();
    // Two parts, neither bigger than the board, whose courtyards overlap: the message names
    // the pair (not the rows' size, which is not a certain shortfall).
    assert!(
        error.contains("2 part(s) do not fit the outline of 14.0 x 22.0 mm")
            && error.contains("J1 and U1 "),
        "{error}"
    );
}

// ---- KiCad's own courtyard checks on the same boards (ADR 0031, the two-face rule) ----

/// What KiCad's DRC says about the courtyards of a board: whether it reports
/// `courtyards_overlap` (two courtyards on one layer) and `pth_inside_courtyard` (a plated
/// hole inside another footprint's courtyard, on either face). KiCad ignores the second by
/// default, so the project file is given an error severity for it. `None` where KiCad is not
/// installed (the test then skips).
fn kicad_courtyard_findings(b: &Board) -> Option<(bool, bool)> {
    use netsmith_fab::{KicadCli, KicadCliExe, drc::kicad};
    let cli = KicadCliExe::locate_runnable()?;
    let dir = TempDir::new("hole-faces-kicad");
    let path = dir.join("faces.kicad_pcb");
    std::fs::write(&path, netsmith_pcb::kicad_pcb(b)).unwrap();
    let pro = netsmith_pcb::kicad_pro(b);
    let on = pro.replacen(
        "\"rule_severities\": {",
        "\"rule_severities\": {\"pth_inside_courtyard\": \"error\",",
        1,
    );
    assert_ne!(
        on, pro,
        "the project file has no rule_severities to change:\n{pro}"
    );
    std::fs::write(path.with_extension("kicad_pro"), on).unwrap();
    let found = kicad::parse(&cli.drc_json(&path).unwrap()).unwrap();
    let has = |kind: &str| found.iter().any(|f| f.kind == kind);
    Some((has("courtyards_overlap"), has("pth_inside_courtyard")))
}

/// A board with the header `J1` (courtyard on `layers`, pads at the top) and the big part
/// `U1` on `side`, its centre `down` mm below the header's pads, so its courtyard edge is
/// `down - 4` mm below the pads' centres.
fn two_parts(layers: &[&str], side: Side, down: f64) -> Board {
    let mut b = stacked(&angle(layers), down);
    b.parts[1].side = side;
    b
}

#[test]
fn kicads_own_checks_never_flag_what_the_two_face_rule_passes() {
    // Each row: the header's courtyard layers, the face of the big part, how far down it is
    // (15: its courtyard over the header's body and clear of the pads; 2: over the pads),
    // then what netsmith's placement check says (an overlap) and what KiCad's DRC says
    // (`courtyards_overlap`, `pth_inside_courtyard`).
    type Row = (&'static [&'static str], Side, f64, bool, bool, bool);
    const TABLE: &[Row] = &[
        // One courtyard drawn, on the front: on the back the big part may stand over the
        // body (both agree), but not over the pads (KiCad's hole check agrees; its courtyard
        // check, layer by layer, does not see it).
        (&["F.CrtYd"], Side::Back, 15.0, false, false, false),
        (&["F.CrtYd"], Side::Back, 2.0, true, false, true),
        (&["F.CrtYd"], Side::Front, 15.0, true, true, false),
        (&["F.CrtYd"], Side::Front, 2.0, true, true, true),
        // One courtyard drawn, on the back: the same with the faces swapped.
        (&["B.CrtYd"], Side::Back, 15.0, true, true, false),
        (&["B.CrtYd"], Side::Back, 2.0, true, true, true),
        (&["B.CrtYd"], Side::Front, 15.0, false, false, false),
        (&["B.CrtYd"], Side::Front, 2.0, true, false, true),
        // Both drawn: the whole courtyard counts on both faces.
        (&["F.CrtYd", "B.CrtYd"], Side::Back, 15.0, true, true, false),
        (&["F.CrtYd", "B.CrtYd"], Side::Back, 2.0, true, true, true),
        (
            &["F.CrtYd", "B.CrtYd"],
            Side::Front,
            15.0,
            true,
            true,
            false,
        ),
        (&["F.CrtYd", "B.CrtYd"], Side::Front, 2.0, true, true, true),
        // None drawn: netsmith uses the box made from the pads; KiCad has no courtyard to
        // overlap, and finds only the hole inside the other part's courtyard.
        (&[], Side::Back, 15.0, false, false, false),
        (&[], Side::Back, 2.0, true, false, true),
        (&[], Side::Front, 15.0, false, false, false),
        (&[], Side::Front, 2.0, true, false, true),
    ];
    for &(layers, side, down, ours, overlap, hole) in TABLE {
        let b = two_parts(layers, side, down);
        let Some(kicad) = kicad_courtyard_findings(&b) else {
            eprintln!("skipped: kicad-cli is not installed");
            return;
        };
        let what = format!("header courtyard {layers:?}, big part on {side:?}, {down} mm down");
        assert_eq!(overlaps(&b).len() == 1, ours, "netsmith, {what}");
        assert_eq!(kicad, (overlap, hole), "KiCad, {what}");
        // The direction that matters: a board netsmith passes is clean for both of KiCad's
        // courtyard checks.
        if overlaps(&b).is_empty() {
            assert_eq!(
                kicad,
                (false, false),
                "netsmith passes, KiCad flags: {what}"
            );
        }
        // A courtyard overlap KiCad reports is on one layer, and netsmith finds it too.
        if overlap {
            assert!(
                ours,
                "KiCad flags a courtyard overlap netsmith passes: {what}"
            );
        }
    }
}

#[test]
fn netsmith_keeps_a_wider_band_round_the_pads_than_kicads_hole_check() {
    // The big part on the back, the header drawing its courtyard on the front only. The big
    // part's courtyard edge is `edge` mm below the pads' centres. The holes are 1 mm wide,
    // the pads 1.7 mm and the process's clearance 0.2 mm: netsmith keeps the courtyard out
    // of 0.85 + 0.2 mm, KiCad's `pth_inside_courtyard` only out of the hole's 0.5 mm. So
    // between 0.5 and 1.05 mm netsmith flags what KiCad does not; the other way round there
    // is nothing (ADR 0031).
    for (edge, ours, kicad_hole) in [
        (0.3, true, true),
        (0.45, true, true),
        (0.55, true, false),
        (0.9, true, false),
        (1.0, true, false),
        (1.1, false, false),
        (1.5, false, false),
    ] {
        let b = two_parts(&["F.CrtYd"], Side::Back, 4.0 + edge);
        let Some(kicad) = kicad_courtyard_findings(&b) else {
            eprintln!("skipped: kicad-cli is not installed");
            return;
        };
        assert_eq!(overlaps(&b).len() == 1, ours, "netsmith, edge {edge}");
        assert_eq!(kicad, (false, kicad_hole), "KiCad, edge {edge}");
    }
}
