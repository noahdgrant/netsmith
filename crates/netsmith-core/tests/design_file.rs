//! The design file (`<stem>.design.json`, issue #609): the form between the design stage and the
//! placer, and the loader the tests of the later stages start from, on named example files.
//! (netsmith's `design_files` reads every example's file; scripts/check-example-designs.sh proves
//! that each file is what its board builds now.)

use std::fs;
use std::path::Path;

use netsmith_core::Design;
use netsmith_core::design_file::{read, to_json};
use netsmith_testkit::TempDir;

/// The design file `file` of the repository (`examples/led.design.json`), which this crate's tests
/// read by name, so that scripts/ci-changes knows which examples they read.
fn example(file: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(file)
}

#[test]
fn an_example_design_file_reads_back_to_the_same_bytes() {
    let file = example("examples/led.design.json");
    let text = fs::read_to_string(&file).unwrap();
    let designs = read(&file).unwrap();
    assert_eq!(designs.len(), 1);
    assert!(
        to_json(&designs) == text,
        "reading and writing the file changes it"
    );
}

#[test]
fn an_example_loads_as_its_board() {
    let led = example("examples/led.design.json");
    let design = Design::from_json_file(&led).unwrap();
    assert_eq!(design.name, "LedExample");
    assert!(design.parts.iter().any(|p| p.refdes == "D1"));
    assert!(design.process.is_some());
}

#[test]
fn from_json_file_refuses_a_file_of_several_boards() {
    let two = example("examples/two_boards.design.json");
    let e = Design::from_json_file(&two).unwrap_err();
    assert!(e.contains("holds 2 designs"), "{e}");
    assert_eq!(read(&two).unwrap().len(), 2);
}

#[test]
fn a_file_that_is_not_there_or_not_designs_is_named_in_the_error() {
    let dir = TempDir::new("design-file");
    let missing = dir.path().join("missing.design.json");
    let e = read(&missing).unwrap_err();
    assert!(e.starts_with(&missing.display().to_string()), "{e}");
    let bad = dir.path().join("bad.design.json");
    fs::write(&bad, "{}").unwrap();
    let e = Design::from_json_file(&bad).unwrap_err();
    assert!(e.starts_with(&bad.display().to_string()), "{e}");
}

#[test]
fn a_stated_mask_colour_is_in_the_file_and_an_unstated_one_leaves_no_trace() {
    // Issue #771: the colour the board states for its solder mask is written and read back; a
    // board that states none has no key, so the committed design files are as they were.
    let file = example("examples/led.design.json");
    let mut designs = read(&file).unwrap();
    assert!(!to_json(&designs).contains("mask_colour"));
    designs[0].process.as_mut().unwrap().stackup.mask_colour = Some("Black".into());
    let text = to_json(&designs);
    assert!(text.contains("\"mask_colour\": \"Black\""), "{text}");
    let dir = TempDir::new("design-file-mask");
    let path = dir.path().join("black.design.json");
    fs::write(&path, &text).unwrap();
    let back = read(&path).unwrap();
    assert_eq!(
        back[0].process.as_ref().unwrap().stackup.mask_colour.as_deref(),
        Some("Black")
    );
}

#[test]
fn a_stated_case_model_colour_is_in_the_file_and_an_unstated_one_leaves_no_trace() {
    // Issue #756: the colour the board states for a case model is written and read back; a
    // model that states none has no key, so the committed design files are as they were.
    let file = example("examples/led.design.json");
    let mut designs = read(&file).unwrap();
    let model = |name: &str, color: Option<&str>| netsmith_core::CaseModel {
        name: name.into(),
        file: format!("case/{name}.step"),
        offset: [0.0; 3],
        rotation: [0.0; 3],
        figures: netsmith_core::CaseFigures::default(),
        checked: true,
        color: color.map(str::to_string),
        source: String::new(),
        stated_in: Vec::new(),
        resolved: None,
    };
    designs[0].features.case_models = vec![model("base", None), model("lid", Some("Black"))];
    let text = to_json(&designs);
    assert_eq!(text.matches("\"color\"").count(), 1, "{text}");
    assert!(text.contains("\"color\": \"Black\""), "{text}");
    let dir = TempDir::new("design-file-case-colour");
    let path = dir.path().join("case.design.json");
    fs::write(&path, &text).unwrap();
    let back = read(&path).unwrap();
    let colours: Vec<_> = back[0]
        .features
        .case_models
        .iter()
        .map(|c| c.color.as_deref())
        .collect();
    assert_eq!(colours, [None, Some("Black")]);
}
