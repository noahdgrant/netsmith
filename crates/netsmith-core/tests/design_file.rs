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
