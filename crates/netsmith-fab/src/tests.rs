//! Packaging end to end: KiCad is not needed.

use super::*;
use crate::board::tests::SAMPLE;
use netsmith_testkit::TempFile;

fn entry(designators: &[&str], value: &str, lcsc: Option<&str>) -> BomEntry {
    BomEntry {
        designators: designators.iter().map(|s| s.to_string()).collect(),
        value: value.into(),
        footprint: "R_0603_1608Metric".into(),
        lcsc: lcsc.map(str::to_string),
    }
}

/// Entries for SAMPLE's parts: R1, R2 and the header J1 (and the do-not-populate D1).
fn sample_bom() -> Vec<BomEntry> {
    vec![
        entry(&["R1"], "10k", Some("C25804")),
        entry(&["R2"], "10k", Some("C25804")),
        BomEntry {
            designators: vec!["J1".into()],
            value: "Conn_01x02".into(),
            footprint: "PinHeader_1x02_P2.54mm_Vertical".into(),
            lcsc: None,
        },
        BomEntry {
            designators: vec!["D1".into()],
            value: "red".into(),
            footprint: "LED_0603_1608Metric".into(),
            lcsc: Some("C2286".into()),
        },
    ]
}

/// The text of `bom.csv` for a plan: the rows from stock and the consigned ones.
fn render_rows(plan: &Plan) -> String {
    bom::render(&plan.bom_rows())
}

/// SAMPLE with its back-side resistor moved to the front, so the whole board is on top.
fn all_top_board() -> String {
    SAMPLE.replace("(layer \"B.Cu\")", "(layer \"F.Cu\")")
}

fn write_pcb(test: &str, text: &str) -> TempFile {
    let pcb = TempFile::new(&format!("fab-test-{test}"), "sample.kicad_pcb");
    std::fs::write(&pcb, text).unwrap();
    pcb
}

#[test]
fn through_hole_and_unpopulated_parts_leave_the_bom() {
    let board = Board::parse(SAMPLE).unwrap();
    let p = plan(&board, &sample_bom(), &Options::default()).unwrap();
    assert_eq!(p.hand_solder, ["J1"]);
    assert_eq!(p.not_fitted, ["D1"]);
    assert_eq!(p.bom.len(), 1);
    assert_eq!(p.bom[0].designators, ["R1", "R2"]);
    let placed: Vec<_> = p.assembled.iter().map(|p| p.designator.as_str()).collect();
    assert_eq!(placed, ["R1", "R2"]);
}

#[test]
fn what_the_design_says_about_a_part_overrides_the_footprints_attributes() {
    let board = Board::parse(SAMPLE).unwrap();
    // R2 is surface mount but hand-soldered by the design; the hole H1 has no footprint in
    // the board's bill of materials at all and is not fitted; the header is placed by the fab.
    let mut bom = sample_bom();
    bom.retain(|e| e.designators != ["R2"]);
    bom[1].lcsc = Some("C492403".into());
    let options = Options {
        fit: [
            ("R1", Fit::Assembled),
            ("R2", Fit::HandSolder),
            ("J1", Fit::Assembled),
            ("H1", Fit::NotFitted),
        ]
        .into_iter()
        .map(|(d, f)| (d.to_string(), f))
        .collect(),
        ..Options::default()
    };
    let p = plan(&board, &bom, &options).unwrap();
    assert_eq!(p.hand_solder, ["R2"]);
    assert_eq!(p.not_fitted, ["D1", "H1"]);
    let placed: Vec<_> = p.assembled.iter().map(|p| p.designator.as_str()).collect();
    assert_eq!(placed, ["R1", "J1"]);
}

#[test]
fn a_consigned_part_is_in_the_bom_and_the_placement_with_no_part_number() {
    let board = Board::parse(SAMPLE).unwrap();
    // R2 is the owner's to ship: its row has the manufacturer's number as the comment and no
    // LCSC number, and `fab` does not refuse it.
    let mut bom = sample_bom();
    bom[1] = entry(&["R2"], "ZZR-1234", None);
    let options = Options {
        fit: [("R2".to_string(), Fit::Consigned)].into(),
        ..Options::default()
    };
    let p = plan(&board, &bom, &options).unwrap();
    assert_eq!(p.bom.len(), 1);
    assert_eq!(p.bom[0].designators, ["R1"]);
    assert_eq!(p.consigned.len(), 1);
    assert_eq!(p.consigned[0].designators, ["R2"]);
    assert!(p.unassembled.is_empty(), "{:?}", p.unassembled);
    assert!(p.hand_solder == ["J1"] && p.not_fitted == ["D1"]);
    let placed: Vec<_> = p.assembled.iter().map(|p| p.designator.as_str()).collect();
    assert_eq!(placed, ["R1", "R2"]);
    assert_eq!(
        render_rows(&p),
        "Comment,Designator,Footprint,LCSC Part #\n10k,R1,R_0603_1608Metric,C25804\nZZR-1234,R2,R_0603_1608Metric,\n"
    );
}

#[test]
fn a_consigned_part_does_not_share_a_row_with_a_part_from_stock() {
    let board = Board::parse(SAMPLE).unwrap();
    // R1 and R2 are one row but for who supplies them.
    let options = Options {
        fit: [("R2".to_string(), Fit::Consigned)].into(),
        ..Options::default()
    };
    let p = plan(&board, &sample_bom(), &options).unwrap();
    assert_eq!(p.bom[0].designators, ["R1"]);
    assert_eq!(p.consigned[0].designators, ["R2"]);
    assert_eq!(p.consigned[0].lcsc.as_deref(), Some("C25804"));
    assert_eq!(p.bom_rows().len(), 2);
}

#[test]
fn a_consigned_part_the_bom_does_not_list_is_an_error_like_any_part_to_place() {
    let board = Board::parse(SAMPLE).unwrap();
    let mut bom = sample_bom();
    bom.retain(|e| e.designators != ["R2"]);
    let options = Options {
        fit: [("R2".to_string(), Fit::Consigned)].into(),
        ..Options::default()
    };
    let err = plan(&board, &bom, &options).unwrap_err();
    assert!(
        matches!(err, Error::NotInBom(ref d) if d == &["R2"]),
        "{err}"
    );
}

#[test]
fn the_notes_list_consigned_parts_with_their_number_and_quantity_and_say_who_ships_them() {
    let pcb = write_pcb("consigned", &all_top_board());
    let mut bom = sample_bom();
    bom[1] = entry(&["R2"], "ZZR-1234", None);
    let options = Options {
        fit: [("R2".to_string(), Fit::Consigned)].into(),
        ..Options::default()
    };
    let pkg = package(&pcb, &bom, &options).unwrap();
    assert_eq!(
        pkg.bom_csv,
        "Comment,Designator,Footprint,LCSC Part #\n10k,R1,R_0603_1608Metric,C25804\nZZR-1234,R2,R_0603_1608Metric,\n"
    );
    assert_eq!(
        pkg.cpl_csv,
        "Designator,Mid X,Mid Y,Layer,Rotation\nR1,10,10,top,270\nR2,20,10,top,90\n"
    );
    let line = pkg
        .notes
        .lines()
        .find(|l| l.starts_with("consigned:"))
        .unwrap_or_else(|| panic!("{}", pkg.notes));
    assert!(
        line.starts_with("consigned: R2 (ZZR-1234, 1 part)")
            && line.contains("you ship these parts to JLCPCB"),
        "{line}"
    );
    assert!(!pkg.notes.contains("unassembled"), "{}", pkg.notes);
}

/// The number the summary line gives before "BOM line(s)", and the data rows of `bom.csv`.
fn summary_lines_and_rows(summary: &str, bom_csv: &str) -> (usize, usize) {
    let first = summary.lines().next().unwrap();
    let before = first.split(" BOM line(s)").next().unwrap();
    let lines = before
        .rsplit(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap_or_else(|_| panic!("{first}"));
    (lines, bom_csv.lines().count() - 1)
}

#[test]
fn the_summary_counts_the_lines_of_bom_csv_the_consigned_ones_too() {
    // R1 is from stock and R2 is consigned, so `bom.csv` has two lines and `plan.bom` one.
    let pcb = write_pcb("summary-lines", &all_top_board());
    let mut bom = sample_bom();
    bom[1] = entry(&["R2"], "ZZR-1234", None);
    let options = Options {
        fit: [("R2".to_string(), Fit::Consigned)].into(),
        ..Options::default()
    };
    let pkg = package(&pcb, &bom, &options).unwrap();
    let (lines, rows) = summary_lines_and_rows(&pkg.summary(), &pkg.bom_csv);
    assert_eq!(rows, 2, "{}", pkg.bom_csv);
    assert_eq!(lines, rows, "{}", pkg.summary());
    assert!(
        pkg.summary()
            .starts_with("sample: 2 part(s) on 2 BOM line(s), 2 placement(s)"),
        "{}",
        pkg.summary()
    );
}

#[test]
fn a_part_the_design_leaves_to_hand_but_the_bom_lists_is_not_placed() {
    let board = Board::parse(SAMPLE).unwrap();
    let options = Options {
        fit: [("R2".to_string(), Fit::HandSolder)].into(),
        ..Options::default()
    };
    let p = plan(&board, &sample_bom(), &options).unwrap();
    assert_eq!(p.bom.len(), 1);
    assert_eq!(p.bom[0].designators, ["R1"]);
    assert_eq!(p.hand_solder, ["J1", "R2"]);
}

#[test]
fn a_part_without_an_lcsc_number_is_an_error_that_names_it() {
    let board = Board::parse(SAMPLE).unwrap();
    let mut bom = sample_bom();
    bom[1].lcsc = None;
    bom[1].value = "4k7".into();
    let err = plan(&board, &bom, &Options::default()).unwrap_err();
    let text = err.to_string();
    assert!(matches!(err, Error::MissingLcsc(_)));
    assert!(text.contains("R2 (4k7, R_0603_1608Metric)"), "{text}");
    assert!(text.contains("allow_missing_lcsc"), "{text}");
}

#[test]
fn the_option_lists_such_parts_as_unassembled_and_drops_them() {
    let board = Board::parse(SAMPLE).unwrap();
    let mut bom = sample_bom();
    bom[1].lcsc = None;
    bom[1].value = "4k7".into();
    let options = Options {
        allow_missing_lcsc: true,
        ..Options::default()
    };
    let p = plan(&board, &bom, &options).unwrap();
    assert_eq!(p.unassembled, ["R2"]);
    assert_eq!(p.bom.len(), 1);
    assert_eq!(p.bom[0].designators, ["R1"]);
    assert_eq!(p.assembled.len(), 1);
}

#[test]
fn bom_and_board_must_agree() {
    let board = Board::parse(SAMPLE).unwrap();
    let mut bom = sample_bom();
    bom.push(entry(&["R9"], "1k", Some("C21190")));
    assert!(matches!(
        plan(&board, &bom, &Options::default()),
        Err(Error::NotOnBoard(d)) if d == ["R9"]
    ));
    let missing = &sample_bom()[1..];
    assert!(matches!(
        plan(&board, missing, &Options::default()),
        Err(Error::NotInBom(d)) if d == ["R1"]
    ));
}

#[test]
fn packaging_writes_a_zip_bom_cpl_and_notes() {
    let pcb = write_pcb("package", &all_top_board());
    let pkg = package(&pcb, &sample_bom(), &Options::default()).unwrap();
    assert_eq!(pkg.name, "sample");
    assert_eq!(pkg.drc, DrcOutcome::Passed);
    assert_eq!(
        pkg.bom_csv,
        "Comment,Designator,Footprint,LCSC Part #\n10k,\"R1,R2\",R_0603_1608Metric,C25804\n"
    );
    assert_eq!(
        pkg.cpl_csv,
        "Designator,Mid X,Mid Y,Layer,Rotation\nR1,10,10,top,270\nR2,20,10,top,90\n"
    );
    assert!(pkg.notes.contains("hand-solder: J1\n"), "{}", pkg.notes);
    assert!(
        pkg.notes
            .contains("not fitted (no BOM line, no placement): D1\n")
    );
    assert!(pkg.notes.contains("design rules check: passed"));
    assert!(!pkg.notes.contains("rotation unverified"));
    // Two copper layers, nothing on the back: no inner layers, no B_Paste or B_Silkscreen.
    assert!(pkg.gerber_files.contains(&"sample-F_Cu.gbr".to_string()));
    assert!(pkg.gerber_files.contains(&"sample-PTH.drl".to_string()));
    assert!(!pkg.gerber_files.iter().any(|f| f.contains("In1")));
    assert_eq!(pkg.zip_name(), "sample-gerbers.zip");

    let out = pcb.parent().unwrap().join("out");
    let written = pkg.write(&out).unwrap();
    let names: Vec<_> = written
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        ["sample-gerbers.zip", "bom.csv", "cpl.csv", "fab-notes.txt"]
    );
}

/// The all-top sample with two inner layers, as a board of four layers has.
fn four_layer_board() -> String {
    all_top_board().replace(
        "(0 \"F.Cu\" signal)\n\t\t(2 \"B.Cu\" signal)",
        "(0 \"F.Cu\" signal)\n\t\t(4 \"In1.Cu\" signal)\n\t\t(6 \"In2.Cu\" signal)\n\t\t(2 \"B.Cu\" signal)",
    )
}

#[test]
fn a_four_layer_board_gets_its_inner_layers_in_the_zip_and_the_notes_say_so() {
    let pcb = write_pcb("four", &four_layer_board());
    let pkg = package(&pcb, &sample_bom(), &Options::default()).unwrap();
    for file in [
        "sample-F_Cu.gbr",
        "sample-In1_Cu.gbr",
        "sample-In2_Cu.gbr",
        "sample-B_Cu.gbr",
    ] {
        assert!(
            pkg.gerber_files.contains(&file.to_string()),
            "{file}: {:?}",
            pkg.gerber_files
        );
    }
    assert!(
        pkg.notes
            .contains("copper layers: 4 (F.Cu, In1.Cu, In2.Cu, B.Cu)"),
        "{}",
        pkg.notes
    );
    // A board of two layers has no such line, and no inner files.
    let two = package(
        &write_pcb("two", &all_top_board()),
        &sample_bom(),
        &Options::default(),
    )
    .unwrap();
    assert!(!two.notes.contains("copper layers:"), "{}", two.notes);
}

#[test]
fn the_same_input_gives_the_same_bytes() {
    let pcb = write_pcb("determinism", &all_top_board());
    let a = package(&pcb, &sample_bom(), &Options::default()).unwrap();
    let b = package(&pcb, &sample_bom(), &Options::default()).unwrap();
    assert_eq!(a.gerber_zip, b.gerber_zip);
    assert_eq!(a.bom_csv, b.bom_csv);
    assert_eq!(a.cpl_csv, b.cpl_csv);
    assert_eq!(a.notes, b.notes);
}

#[test]
fn notes_list_unverified_rotations_and_bottom_parts() {
    // SAMPLE as written has R2 on the back; R1 is a measured footprint.
    let pcb = write_pcb("unverified", SAMPLE);
    let pkg = package(&pcb, &sample_bom(), &Options::default()).unwrap();
    assert!(
        pkg.notes.contains(
            "bottom side unverified (rotation and mirroring): check JLCPCB's order preview: R2"
        ),
        "{}",
        pkg.notes
    );
    assert!(pkg.cpl_csv.contains("R2,20,10,bottom,90"));
}

#[test]
fn notes_carry_what_the_board_needs_of_the_fabricator_beyond_its_standard_process() {
    let pcb = write_pcb("extras", &all_top_board());
    let options = Options {
        process: Some("JLCPCB FourLayer".into()),
        extras: vec!["vias in pads: 2 via(s) stand in pads (U1.5, U1.6)".into()],
        ..Options::default()
    };
    let pkg = package(&pcb, &sample_bom(), &options).unwrap();
    let lines: Vec<&str> = pkg.notes.lines().collect();
    let process = lines
        .iter()
        .position(|l| l.starts_with("process:"))
        .unwrap();
    assert_eq!(
        lines[process + 1],
        "vias in pads: 2 via(s) stand in pads (U1.5, U1.6)"
    );
}

/// The sample board with a second pad of another net laid 0.05 mm from the header's pad: a
/// clearance violation, a short of the mask openings, and the like.
fn crowded_board() -> String {
    all_top_board().replace(
        "(pad \"1\" thru_hole rect (at 0 0) (size 1.7 1.7) (drill 1) (layers \"*.Cu\" \"*.Mask\"))",
        "(pad \"1\" thru_hole rect (at 0 0) (size 1.7 1.7) (drill 1) (layers \"*.Cu\" \"*.Mask\") (net 1 \"A\"))\n\t\t(pad \"2\" thru_hole rect (at 1.75 0) (size 1.7 1.7) (drill 1) (layers \"*.Cu\" \"*.Mask\") (net 2 \"B\"))",
    )
    .replace("(setup", "(net 0 \"\")\n\t(net 1 \"A\")\n\t(net 2 \"B\")\n\t(setup")
}

#[test]
fn a_failing_design_rules_check_stops_packaging_and_names_what_it_found() {
    let pcb = write_pcb("drc", &crowded_board());
    let err = package(&pcb, &sample_bom(), &Options::default()).unwrap_err();
    let text = err.to_string();
    assert!(matches!(err, Error::Drc { .. }), "{text}");
    assert!(text.contains("design rules check found"), "{text}");
    assert!(text.contains("--allow-drc-violations"), "{text}");
    assert!(text.contains("[clearance]"), "{text}");
    assert!(text.contains("PTH pad 1 [A] of J1"), "{text}");
    assert!(
        text.contains("0.05"),
        "the numbers are in the message: {text}"
    );
}

#[test]
fn the_option_waives_the_gate_and_the_notes_say_so() {
    let pcb = write_pcb("waive", &crowded_board());
    let options = Options {
        allow_drc_violations: true,
        ..Options::default()
    };
    let pkg = package(&pcb, &sample_bom(), &options).unwrap();
    assert!(matches!(pkg.drc, DrcOutcome::Waived(c) if c.violations >= 1));
    assert!(
        pkg.notes
            .contains("design rules check: NOT clean, packaged anyway:"),
        "{}",
        pkg.notes
    );
    assert!(!pkg.drc_report.is_clean());
}

#[test]
fn a_board_with_the_project_rules_beside_it_is_checked_to_them() {
    // The same crowded board passes when the project says the least clearance is 0.01 mm.
    let pcb = write_pcb("project", &crowded_board());
    let pro = pcb.with_extension("kicad_pro");
    std::fs::write(
        &pro,
        r#"{"board": {"design_settings": {"rules": {"min_clearance": 0.01, "min_hole_clearance": 0.01, "min_hole_to_hole": 0.01, "min_copper_edge_clearance": 0.0, "min_via_annular_width": 0.0, "min_through_hole_diameter": 0.1}, "rule_severities": {"solder_mask_bridge": "ignore", "shorting_items": "ignore"}}}, "net_settings": {"classes": [{"name": "Default", "clearance": 0.01}]}}"#,
    )
    .unwrap();
    let pkg = package(&pcb, &sample_bom(), &Options::default()).unwrap();
    assert_eq!(pkg.drc, DrcOutcome::Passed);
}

#[test]
fn warnings_do_not_stop_a_package_and_are_listed_in_the_notes() {
    // A silkscreen line over the header's pad is a warning (so is its reference, which the
    // sample puts on the pad).
    let board = all_top_board().replace(
        "(attr through_hole)",
        "(attr through_hole)\n\t\t(fp_line (start -1 0) (end 1 0) (stroke (width 0.12) (type solid)) (layer \"F.SilkS\"))",
    );
    let pcb = write_pcb("warn", &board);
    let pkg = package(&pcb, &sample_bom(), &Options::default()).unwrap();
    assert_eq!(pkg.drc, DrcOutcome::Passed);
    assert_eq!(
        pkg.drc_report.count(drc::Rule::SilkOverCopper),
        2,
        "{}",
        pkg.drc_report
    );
    assert!(
        pkg.notes
            .contains("design rules warnings: 2\n  warning [silk_over_copper]"),
        "{}",
        pkg.notes
    );
}

#[test]
fn the_notes_cite_the_process_the_board_was_made_to() {
    let pcb = write_pcb("process", &all_top_board());
    let options = Options {
        process: Some("JLCPCB TwoLayer (numbers from https://example.test/rules)".into()),
        ..Options::default()
    };
    let pkg = package(&pcb, &sample_bom(), &options).unwrap();
    assert!(
        pkg.notes
            .contains("process: JLCPCB TwoLayer (numbers from https://example.test/rules)"),
        "{}",
        pkg.notes
    );
    let plain = package(&pcb, &sample_bom(), &Options::default()).unwrap();
    assert!(!plain.notes.contains("process:"), "no process, no line");
}

#[test]
fn part_number_errors_come_before_the_check_runs() {
    let pcb = write_pcb("early", &crowded_board());
    let mut bom = sample_bom();
    bom[0].lcsc = None;
    assert!(matches!(
        package(&pcb, &bom, &Options::default()),
        Err(Error::MissingLcsc(_))
    ));
}
