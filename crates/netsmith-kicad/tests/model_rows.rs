//! A footprint library's `(model ...)` lines as `[[model]]` rows (issue #1005).

use netsmith_kicad::{FootprintModelLine, ModelLine, ModelSkip, model_line, model_rows};
use netsmith_testkit::TempDir;

fn footprint(model: &str) -> String {
    format!("(footprint \"X\" (version 20240108) (layer \"F.Cu\") {model})\n")
}

#[test]
fn a_step_models_offset_and_rotation_are_copied() {
    let text = footprint(
        "(model \"${KICAD9_3DMODEL_DIR}/Resistor_SMD.3dshapes/R_0402_1005Metric.step\" (offset (xyz 0.5 -0.25 0)) (scale (xyz 1 1 1)) (rotate (xyz -90 0 180)))",
    );
    assert_eq!(
        model_line(&text),
        Ok(FootprintModelLine::Row(ModelLine {
            file: "${KICAD9_3DMODEL_DIR}/Resistor_SMD.3dshapes/R_0402_1005Metric.step".into(),
            offset: [0.5, -0.25, 0.0],
            rotation: [-90.0, 0.0, 180.0],
        }))
    );
}

#[test]
fn an_old_at_line_is_in_inches_and_kiprjmod_is_taken_off() {
    let text = footprint("(model \"${KIPRJMOD}/models/a.STP\" (at (xyz 0.1 0 0)))");
    assert_eq!(
        model_line(&text),
        Ok(FootprintModelLine::Row(ModelLine {
            file: "models/a.STP".into(),
            offset: [2.54, 0.0, 0.0],
            rotation: [0.0; 3],
        }))
    );
}

#[test]
fn a_footprint_with_no_step_model_is_no_row() {
    assert_eq!(model_line(&footprint("")), Ok(FootprintModelLine::None));
    assert_eq!(
        model_line(&footprint("(model \"a.wrl\")")),
        Ok(FootprintModelLine::Skipped(ModelSkip::NotStep))
    );
    assert_eq!(
        model_line(&footprint("(model \"a.step\" (scale (xyz 0.393701 0.393701 0.393701)))")),
        Ok(FootprintModelLine::Skipped(ModelSkip::Scaled))
    );
    assert!(model_line(&footprint("(model \"a.step\" (offset (xyz 1 2)))")).is_err());
}

#[test]
fn a_table_gives_the_rows_of_its_libraries_and_notes_what_it_left_out() {
    let root = TempDir::new("model-rows");
    let pretty = root.join("acme.pretty");
    std::fs::create_dir_all(&pretty).unwrap();
    std::fs::write(
        pretty.join("B.kicad_mod"),
        footprint("(model \"${KIPRJMOD}/m/b.step\" (rotate (xyz 0 0 90)))"),
    )
    .unwrap();
    std::fs::write(
        pretty.join("A.kicad_mod"),
        footprint("(model \"${KIPRJMOD}/m/a.step\" (offset (xyz 0 0 0.5)))"),
    )
    .unwrap();
    std::fs::write(pretty.join("C.kicad_mod"), footprint("")).unwrap();
    std::fs::write(pretty.join("D.kicad_mod"), footprint("(model \"d.wrl\")")).unwrap();
    std::fs::write(pretty.join("notes.txt"), "not a footprint").unwrap();
    let table = "(fp_lib_table (version 7)\n\
        (lib (name \"acme\")(type \"KiCad\")(uri \"${KIPRJMOD}/acme.pretty\")(options \"\")(descr \"\"))\n\
        (lib (name \"lost\")(type \"KiCad\")(uri \"${KIPRJMOD}/lost.pretty\")(options \"\")(descr \"\"))\n\
        (lib (name \"env\")(type \"KiCad\")(uri \"${NETSMITH_TEST_NO_SUCH_DIR}/x.pretty\")(options \"\")(descr \"\")))\n";
    let found = model_rows(table, &root, &[]).unwrap();
    assert_eq!(
        found.rows,
        "[[model]]\nfootprint = \"acme:A\"\nfile = \"m/a.step\"\noffset = [0, 0, 0.5]\n\n\
         [[model]]\nfootprint = \"acme:B\"\nfile = \"m/b.step\"\nrotation = [0, 0, 90]\n\n"
    );
    assert_eq!(found.notes.len(), 4, "{:?}", found.notes);
    assert!(found.notes.iter().any(|n| n.contains("`lost`") && n.contains("not a folder")));
    assert!(found.notes.iter().any(|n| n.contains("`env`") && n.contains("NETSMITH_TEST_NO_SUCH_DIR")));
    assert!(found.notes.contains(&"the library `acme`: 1 footprint has no model line: left out".to_string()));
    assert!(found.notes.contains(
        &"the library `acme`: 1 footprint names a model that is not a STEP file: left out".to_string()
    ));
    // The rows parse as the manifest's.
    let manifest = format!("[project]\nname = \"p\"\n\n{}", found.rows);
    let m = row_count(&manifest);
    assert_eq!(m, 2);
    // `only` leaves the other libraries out.
    let none = model_rows(table, &root, &["nope".to_string()]).unwrap();
    assert_eq!((none.rows.as_str(), none.notes.len()), ("", 0));
}

/// How many `[[model]]` rows `text` holds, read as TOML.
fn row_count(text: &str) -> usize {
    let value: toml::Table = text.parse().unwrap();
    value["model"].as_array().unwrap().len()
}
